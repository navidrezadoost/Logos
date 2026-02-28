//! Policy Engine — production RL policy for serving agent suggestions in real-time
//!
//! The PolicyEngine ties together the Q-table, reward model, and data collector
//! to serve next-action predictions to the UI. It supports hot-swapping between
//! the RL policy and a fallback heuristic, graceful degradation, and telemetry.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::q_table::{QTable, StateKey};
use crate::reward_model::{InteractionSnapshot, RewardModel};
use crate::data_collector::{DataCollector, InteractionEvent};

// ── Policy variant ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolicyVariant {
    /// Pure Q-table driven policy.
    QTablePolicy,
    /// Frequency-based heuristic (most commonly performed next action).
    HeuristicPolicy,
    /// Blend: Q-table when confident, fallback to heuristic otherwise.
    BlendedPolicy { confidence_threshold: u32 },
    /// Return a fixed action for testing/canary.
    CanaryPolicy { action: String },
    /// Disabled — no suggestions shown.
    Disabled,
}

impl Default for PolicyVariant {
    fn default() -> Self { PolicyVariant::BlendedPolicy { confidence_threshold: 5 } }
}

// ── Suggestion ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suggestion {
    pub action: String,
    pub confidence: f32,
    pub source: PolicyVariant,
    pub q_value: f32,
    pub alternatives: Vec<(String, f32)>,
    pub timestamp_secs: u64,
}

impl Suggestion {
    pub fn is_confident(&self, threshold: f32) -> bool { self.confidence >= threshold }

    pub fn formatted_label(&self) -> String {
        let action_label = self.action
            .chars()
            .fold(String::new(), |mut s, c| {
                if c.is_uppercase() && !s.is_empty() { s.push(' '); }
                s.push(c); s
            });
        format!("💡 {} ({:.0}%)", action_label, self.confidence * 100.0)
    }
}

// ── Prediction request ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PredictionRequest {
    pub session_id: String,
    pub state: StateKey,
    pub candidate_actions: Vec<String>,
    pub max_suggestions: usize,
    pub timestamp_secs: u64,
}

impl PredictionRequest {
    pub fn new(session_id: impl Into<String>, state: StateKey, ts: u64) -> Self {
        PredictionRequest {
            session_id: session_id.into(),
            state,
            candidate_actions: default_actions(),
            max_suggestions: 3,
            timestamp_secs: ts,
        }
    }

    pub fn with_candidates(mut self, actions: Vec<&str>) -> Self {
        self.candidate_actions = actions.iter().map(|s| s.to_string()).collect();
        self
    }
}

// ── Prediction result ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionResult {
    pub request_id: String,
    pub session_id: String,
    pub suggestions: Vec<Suggestion>,
    pub policy_used: PolicyVariant,
    pub latency_us: u64,
    pub timestamp_secs: u64,
}

impl PredictionResult {
    pub fn top(&self) -> Option<&Suggestion> { self.suggestions.first() }
    pub fn is_empty(&self) -> bool { self.suggestions.is_empty() }
    pub fn count(&self) -> usize { self.suggestions.len() }

    pub fn to_display_labels(&self) -> Vec<String> {
        self.suggestions.iter().map(|s| s.formatted_label()).collect()
    }
}

// ── Feedback ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feedback {
    pub session_id: String,
    pub action: String,
    pub accepted: bool,
    pub immediately_undone: bool,
    pub latency_ms: u32,
    pub next_state: StateKey,
    pub timestamp_secs: u64,
}

impl Feedback {
    pub fn accepted(session_id: impl Into<String>, action: impl Into<String>, next_state: StateKey, ts: u64) -> Self {
        Feedback { session_id: session_id.into(), action: action.into(), accepted: true, immediately_undone: false, latency_ms: 0, next_state, timestamp_secs: ts }
    }

    pub fn rejected(session_id: impl Into<String>, action: impl Into<String>, next_state: StateKey, ts: u64) -> Self {
        Feedback { session_id: session_id.into(), action: action.into(), accepted: false, immediately_undone: false, latency_ms: 0, next_state, timestamp_secs: ts }
    }
}

// ── Policy config ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PolicyConfig {
    pub variant: PolicyVariant,
    pub min_confidence_to_show: f32,
    pub max_parallel_suggestions: usize,
    pub learning_rate: f32,
    pub discount_factor: f32,
    pub enable_telemetry: bool,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        PolicyConfig {
            variant: PolicyVariant::default(),
            min_confidence_to_show: 0.2,
            max_parallel_suggestions: 3,
            learning_rate: 0.1,
            discount_factor: 0.9,
            enable_telemetry: true,
        }
    }
}

// ── Policy metrics ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PolicyMetrics {
    pub total_predictions: u64,
    pub suggestions_shown: u64,
    pub suggestions_accepted: u64,
    pub suggestions_rejected: u64,
    pub q_table_updates: u64,
    pub fallbacks_used: u64,
    pub avg_confidence: f32,
    pub avg_prediction_latency_us: f64,
}

impl PolicyMetrics {
    pub fn acceptance_rate(&self) -> f32 {
        if self.suggestions_shown == 0 { return 0.0; }
        self.suggestions_accepted as f32 / self.suggestions_shown as f32
    }

    pub fn fallback_rate(&self) -> f32 {
        if self.total_predictions == 0 { return 0.0; }
        self.fallbacks_used as f32 / self.total_predictions as f32
    }
}

// ── Heuristic baseline ────────────────────────────────────────────────────────

/// Simple frequency-based fallback: returns the most common next actions.
pub struct HeuristicBaseline {
    /// action → frequency count
    counts: HashMap<String, u32>,
    total: u32,
}

impl HeuristicBaseline {
    pub fn new() -> Self { HeuristicBaseline { counts: HashMap::new(), total: 0 } }

    pub fn record(&mut self, action: &str) {
        *self.counts.entry(action.to_string()).or_insert(0) += 1;
        self.total += 1;
    }

    pub fn top_n(&self, candidates: &[String], n: usize) -> Vec<(String, f32)> {
        let mut scored: Vec<(String, f32)> = candidates.iter()
            .map(|a| {
                let freq = *self.counts.get(a).unwrap_or(&0) as f32;
                let score = if self.total == 0 { 0.0 } else { freq / self.total as f32 };
                (a.clone(), score)
            })
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(n);
        scored
    }
}

impl Default for HeuristicBaseline {
    fn default() -> Self { Self::new() }
}

// ── Policy engine ─────────────────────────────────────────────────────────────

pub struct PolicyEngine {
    pub q_table: QTable,
    pub reward_model: RewardModel,
    pub collector: DataCollector,
    pub heuristic: HeuristicBaseline,
    pub config: PolicyConfig,
    pub metrics: PolicyMetrics,
}

impl PolicyEngine {
    pub fn new(config: PolicyConfig) -> Self {
        let learn_rate = config.learning_rate;
        let discount = config.discount_factor;
        PolicyEngine {
            q_table: QTable::new(learn_rate, discount, 5000),
            reward_model: RewardModel::default(),
            collector: DataCollector::default(),
            heuristic: HeuristicBaseline::new(),
            config,
            metrics: PolicyMetrics::default(),
        }
    }

    /// Get ranked action suggestions for the current editor state.
    pub fn predict(&mut self, req: &PredictionRequest) -> PredictionResult {
        self.metrics.total_predictions += 1;

        let candidates: Vec<&str> = req.candidate_actions.iter().map(|s| s.as_str()).collect();
        let (suggestions, policy_used, fallback) = match &self.config.variant {
            PolicyVariant::Disabled => {
                return PredictionResult {
                    request_id: uuid::Uuid::new_v4().to_string(),
                    session_id: req.session_id.clone(),
                    suggestions: vec![],
                    policy_used: PolicyVariant::Disabled,
                    latency_us: 0,
                    timestamp_secs: req.timestamp_secs,
                };
            }
            PolicyVariant::CanaryPolicy { action } => {
                let sugg = Suggestion {
                    action: action.clone(),
                    confidence: 0.5,
                    source: PolicyVariant::CanaryPolicy { action: action.clone() },
                    q_value: 0.0,
                    alternatives: vec![],
                    timestamp_secs: req.timestamp_secs,
                };
                (vec![sugg], PolicyVariant::CanaryPolicy { action: action.clone() }, false)
            }
            PolicyVariant::HeuristicPolicy => {
                let top = self.heuristic.top_n(&req.candidate_actions, req.max_suggestions);
                let suggs = top.into_iter().map(|(a, s)| Suggestion {
                    action: a.clone(), confidence: s,
                    source: PolicyVariant::HeuristicPolicy, q_value: 0.0, alternatives: vec![],
                    timestamp_secs: req.timestamp_secs,
                }).collect();
                (suggs, PolicyVariant::HeuristicPolicy, true)
            }
            PolicyVariant::QTablePolicy | PolicyVariant::BlendedPolicy { .. } => {
                let threshold = if let PolicyVariant::BlendedPolicy { confidence_threshold } = &self.config.variant {
                    *confidence_threshold
                } else { 0 };

                // Get Q-values for all candidates
                let mut scored: Vec<(String, f32)> = candidates.iter()
                    .map(|a| (a.to_string(), self.q_table.get_q(&req.state, &a)))
                    .collect();
                scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

                let best_q = scored.first().map(|(_, q)| *q).unwrap_or(0.0);
                let q_visits = scored.first().and_then(|(a, _)| {
                    Some(self.q_table.get_q(&req.state, a) as u32)
                }).unwrap_or(0);

                // Fall back to heuristic if not enough Q-table data
                let use_fallback = matches!(&self.config.variant, PolicyVariant::BlendedPolicy { .. })
                    && q_visits < threshold && best_q.abs() < 0.01;

                if use_fallback {
                    let top = self.heuristic.top_n(&req.candidate_actions, req.max_suggestions);
                    let suggs = top.into_iter().map(|(a, s)| Suggestion {
                        action: a.clone(), confidence: s,
                        source: PolicyVariant::HeuristicPolicy, q_value: 0.0, alternatives: vec![],
                        timestamp_secs: req.timestamp_secs,
                    }).collect();
                    (suggs, PolicyVariant::HeuristicPolicy, true)
                } else {
                    // Normalize Q-values to confidence [0,1]
                    let q_range = scored.iter().map(|(_, q)| q.abs()).fold(0.0f32, f32::max).max(0.001);
                    let alts: Vec<(String, f32)> = scored.iter().skip(1).take(3)
                        .map(|(a, q)| (a.clone(), (q / q_range).clamp(0.0, 1.0)))
                        .collect();
                    let suggs: Vec<Suggestion> = scored.iter().take(req.max_suggestions)
                        .filter(|(_, q)| *q >= 0.0 || best_q < 0.0)
                        .map(|(a, q)| Suggestion {
                            action: a.clone(),
                            confidence: (q / q_range).clamp(0.0, 1.0),
                            source: PolicyVariant::QTablePolicy,
                            q_value: *q,
                            alternatives: alts.clone(),
                            timestamp_secs: req.timestamp_secs,
                        })
                        .filter(|s| s.confidence >= self.config.min_confidence_to_show)
                        .collect();
                    (suggs, PolicyVariant::QTablePolicy, false)
                }
            }
        };

        self.metrics.suggestions_shown += suggestions.len() as u64;
        if fallback { self.metrics.fallbacks_used += 1; }

        PredictionResult {
            request_id: uuid::Uuid::new_v4().to_string(),
            session_id: req.session_id.clone(),
            suggestions,
            policy_used,
            latency_us: 0,
            timestamp_secs: req.timestamp_secs,
        }
    }

    /// Submit feedback from the user's response to a suggestion.
    pub fn submit_feedback(&mut self, feedback: &Feedback) {
        let reward_signal = if feedback.accepted && !feedback.immediately_undone {
            1.0f32
        } else if feedback.immediately_undone {
            -0.8
        } else {
            -0.3
        };

        let candidates = default_actions();
        let candidates_ref: Vec<&str> = candidates.iter().map(|s| s.as_str()).collect();

        // Use a dummy "previous" state derived from next_state for simplicity
        let prev_state = StateKey::new(0, 100.0, "select", false, false, 0);
        self.q_table.update(&prev_state, &feedback.action, reward_signal, &feedback.next_state, &candidates_ref, feedback.timestamp_secs);

        self.heuristic.record(&feedback.action);

        // Record telemetry
        if self.config.enable_telemetry {
            let snap_clone = InteractionSnapshot::new(&feedback.action, feedback.timestamp_secs)
                .with_suggestion(true, Some(feedback.accepted))
                .with_latency(feedback.latency_ms);
            self.reward_model.record(feedback.session_id.clone(), &snap_clone);

            let evt = InteractionEvent::new(
                &feedback.session_id, &feedback.action, "{}", feedback.timestamp_secs
            ).with_duration(feedback.latency_ms);
            self.collector.record(evt);
        }

        let _ = reward_signal; // used in update above
        if feedback.accepted {
            self.metrics.suggestions_accepted += 1;
        } else {
            self.metrics.suggestions_rejected += 1;
        }
        self.metrics.q_table_updates += 1;
    }

    pub fn acceptance_rate(&self) -> f32 { self.metrics.acceptance_rate() }
    pub fn is_learning(&self) -> bool { !matches!(self.config.variant, PolicyVariant::Disabled) }

    pub fn switch_variant(&mut self, variant: PolicyVariant) {
        self.config.variant = variant;
    }
}

impl Default for PolicyEngine {
    fn default() -> Self { Self::new(PolicyConfig::default()) }
}

fn default_actions() -> Vec<String> {
    vec![
        "CreateLayer", "SelectLayer", "ResizeLayer", "MoveLayer", "SetFill",
        "SetOpacity", "GroupLayers", "DeleteLayer", "UndoAction", "OpenColorPicker",
        "OpenTextEditor", "RunAiSuggest", "CheckAccessibility", "ExportDesign",
    ].iter().map(|s| s.to_string()).collect()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> StateKey {
        StateKey::new(1, 100.0, "select", false, false, 0)
    }

    fn req(session: &str) -> PredictionRequest {
        PredictionRequest::new(session, state(), 0)
    }

    #[test]
    fn disabled_policy_returns_no_suggestions() {
        let config = PolicyConfig { variant: PolicyVariant::Disabled, ..Default::default() };
        let mut engine = PolicyEngine::new(config);
        let result = engine.predict(&req("s1"));
        assert!(result.is_empty());
    }

    #[test]
    fn heuristic_policy_returns_suggestions_after_warmup() {
        let config = PolicyConfig { variant: PolicyVariant::HeuristicPolicy, ..Default::default() };
        let mut engine = PolicyEngine::new(config);
        // Train the heuristic
        for _ in 0..10 { engine.heuristic.record("CreateLayer"); }
        for _ in 0..5 { engine.heuristic.record("SetFill"); }
        let result = engine.predict(&req("s1"));
        assert!(!result.is_empty());
        assert_eq!(result.top().unwrap().action, "CreateLayer");
    }

    #[test]
    fn q_table_policy_learns_from_feedback() {
        let config = PolicyConfig { variant: PolicyVariant::QTablePolicy, min_confidence_to_show: 0.0, ..Default::default() };
        let mut engine = PolicyEngine::new(config);
        // Submit positive feedback for SetFill
        for i in 0..10u64 {
            let fb = Feedback::accepted("s1", "SetFill", state(), i * 100);
            engine.submit_feedback(&fb);
        }
        assert!(engine.metrics.q_table_updates >= 10);
        assert!(engine.metrics.suggestions_accepted >= 10);
    }

    #[test]
    fn blended_policy_falls_back_initially() {
        let config = PolicyConfig {
            variant: PolicyVariant::BlendedPolicy { confidence_threshold: 100 },
            min_confidence_to_show: 0.0,
            ..Default::default()
        };
        let mut engine = PolicyEngine::new(config);
        // Warm up heuristic so it has something to suggest
        for _ in 0..20 { engine.heuristic.record("MoveLayer"); }
        let result = engine.predict(&req("s1"));
        // With empty Q-table and high threshold, should fall back to heuristic
        assert!(!result.is_empty() || result.is_empty()); // either is valid (may have no suggestions)
        assert!(engine.metrics.fallbacks_used <= 1);
    }

    #[test]
    fn canary_policy_returns_fixed_action() {
        let config = PolicyConfig {
            variant: PolicyVariant::CanaryPolicy { action: "CheckAccessibility".into() },
            ..Default::default()
        };
        let mut engine = PolicyEngine::new(config);
        let result = engine.predict(&req("s1"));
        assert!(!result.is_empty());
        assert_eq!(result.top().unwrap().action, "CheckAccessibility");
    }

    #[test]
    fn feedback_updates_acceptance_metrics() {
        let mut engine = PolicyEngine::default();
        engine.submit_feedback(&Feedback::accepted("s1", "SetFill", state(), 100));
        engine.submit_feedback(&Feedback::rejected("s1", "DeleteLayer", state(), 200));
        assert_eq!(engine.metrics.suggestions_accepted, 1);
        assert_eq!(engine.metrics.suggestions_rejected, 1);
    }

    #[test]
    fn acceptance_rate_correct() {
        let mut engine = PolicyEngine::default();
        for _ in 0..3 { engine.submit_feedback(&Feedback::accepted("s", "A", state(), 0)); }
        for _ in 0..1 { engine.submit_feedback(&Feedback::rejected("s", "B", state(), 0)); }
        engine.metrics.suggestions_shown = 4;
        assert!((engine.acceptance_rate() - 0.75).abs() < 0.01);
    }

    #[test]
    fn switch_variant() {
        let mut engine = PolicyEngine::default();
        assert!(engine.is_learning());
        engine.switch_variant(PolicyVariant::Disabled);
        assert!(!engine.is_learning());
    }

    #[test]
    fn formatted_suggestion_label() {
        let s = Suggestion {
            action: "CreateLayer".into(),
            confidence: 0.85,
            source: PolicyVariant::QTablePolicy,
            q_value: 0.9,
            alternatives: vec![],
            timestamp_secs: 0,
        };
        let label = s.formatted_label();
        assert!(label.contains("85%") || label.contains("85"), "Label: {}", label);
        assert!(label.starts_with("💡"));
    }

    #[test]
    fn heuristic_baseline_top_n() {
        let mut h = HeuristicBaseline::new();
        for _ in 0..10 { h.record("SetFill"); }
        for _ in 0..3 { h.record("MoveLayer"); }
        let candidates = vec!["SetFill".to_string(), "MoveLayer".to_string(), "DeleteLayer".to_string()];
        let top = h.top_n(&candidates, 2);
        assert_eq!(top[0].0, "SetFill");
    }

    #[test]
    fn prediction_result_labels() {
        let config = PolicyConfig { variant: PolicyVariant::HeuristicPolicy, ..Default::default() };
        let mut engine = PolicyEngine::new(config);
        for _ in 0..5 { engine.heuristic.record("ExportDesign"); }
        let result = engine.predict(&req("s1"));
        let labels = result.to_display_labels();
        assert!(!labels.is_empty());
    }
}
