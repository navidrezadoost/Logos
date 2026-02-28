//! Reward Model — multi-factor reward signals for RL-UX training
//!
//! Computes reward signals from user interaction data. Combines explicit
//! signals (suggestion accept/reject) with implicit signals (action latency,
//! undo rate, session length) and a WCAG accessibility bonus channel.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Reward signal ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardSignal {
    pub value: f32,
    pub source: RewardSource,
    pub confidence: f32,
    pub timestamp_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RewardSource {
    /// User explicitly accepted the agent suggestion.
    SuggestionAccepted,
    /// User explicitly rejected the suggestion.
    SuggestionRejected,
    /// User performed predicted action independently (implicit positive).
    ImplicitMatch,
    /// User undid an action immediately after accepting suggestion.
    ImmediateUndo,
    /// Fast task completion relative to baseline.
    FastCompletion,
    /// Slow task completion (user struggled).
    SlowCompletion,
    /// WCAG accessibility improvement applied.
    AccessibilityImprovement,
    /// Layer count increased (creative progress).
    CanvasProgress,
    /// Session ended cleanly (export or save).
    SessionComplete,
    /// Manual reward override (A/B test calibration).
    ManualOverride,
}

impl RewardSignal {
    pub fn accepted(ts: u64) -> Self {
        RewardSignal { value: 1.0, source: RewardSource::SuggestionAccepted, confidence: 1.0, timestamp_secs: ts }
    }

    pub fn rejected(ts: u64) -> Self {
        RewardSignal { value: -0.5, source: RewardSource::SuggestionRejected, confidence: 1.0, timestamp_secs: ts }
    }

    pub fn implicit_match(confidence: f32, ts: u64) -> Self {
        RewardSignal { value: 0.6 * confidence, source: RewardSource::ImplicitMatch, confidence, timestamp_secs: ts }
    }

    pub fn undo(ts: u64) -> Self {
        RewardSignal { value: -0.8, source: RewardSource::ImmediateUndo, confidence: 0.9, timestamp_secs: ts }
    }

    pub fn accessibility_bonus(improvement_score: f32, ts: u64) -> Self {
        RewardSignal {
            value: (improvement_score * 0.5).clamp(0.0, 1.0),
            source: RewardSource::AccessibilityImprovement,
            confidence: 0.8,
            timestamp_secs: ts,
        }
    }

    pub fn canvas_progress(ts: u64) -> Self {
        RewardSignal { value: 0.2, source: RewardSource::CanvasProgress, confidence: 0.7, timestamp_secs: ts }
    }

    pub fn session_complete(ts: u64) -> Self {
        RewardSignal { value: 0.3, source: RewardSource::SessionComplete, confidence: 1.0, timestamp_secs: ts }
    }

    pub fn manual(value: f32, ts: u64) -> Self {
        RewardSignal { value: value.clamp(-1.0, 1.0), source: RewardSource::ManualOverride, confidence: 1.0, timestamp_secs: ts }
    }

    /// Weighted reward (signal × confidence).
    pub fn weighted(&self) -> f32 {
        self.value * self.confidence
    }
}

// ── Reward config ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RewardConfig {
    pub acceptance_weight: f32,
    pub latency_weight: f32,
    pub accessibility_weight: f32,
    pub undo_penalty_weight: f32,
    /// Baseline latency in ms. Actions faster than this get a bonus.
    pub baseline_latency_ms: u32,
    /// Clamp final reward to [-max, +max].
    pub max_reward: f32,
}

impl Default for RewardConfig {
    fn default() -> Self {
        RewardConfig {
            acceptance_weight: 1.0,
            latency_weight: 0.3,
            accessibility_weight: 0.5,
            undo_penalty_weight: 0.8,
            baseline_latency_ms: 2000,
            max_reward: 1.0,
        }
    }
}

// ── Interaction snapshot ──────────────────────────────────────────────────────

/// Input data for computing reward from one interaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionSnapshot {
    pub action: String,
    pub suggestion_shown: bool,
    pub suggestion_accepted: Option<bool>,
    pub immediately_undone: bool,
    pub action_latency_ms: u32,
    pub wcag_issues_fixed: u32,
    pub new_layers_created: u32,
    pub session_exported: bool,
    pub timestamp_secs: u64,
}

impl InteractionSnapshot {
    pub fn new(action: impl Into<String>, ts: u64) -> Self {
        InteractionSnapshot {
            action: action.into(),
            suggestion_shown: false,
            suggestion_accepted: None,
            immediately_undone: false,
            action_latency_ms: 0,
            wcag_issues_fixed: 0,
            new_layers_created: 0,
            session_exported: false,
            timestamp_secs: ts,
        }
    }

    pub fn with_suggestion(mut self, shown: bool, accepted: Option<bool>) -> Self {
        self.suggestion_shown = shown; self.suggestion_accepted = accepted; self
    }

    pub fn with_latency(mut self, ms: u32) -> Self { self.action_latency_ms = ms; self }
    pub fn with_undo(mut self) -> Self { self.immediately_undone = true; self }
    pub fn with_wcag_fix(mut self, count: u32) -> Self { self.wcag_issues_fixed = count; self }
    pub fn with_new_layers(mut self, count: u32) -> Self { self.new_layers_created = count; self }
    pub fn with_export(mut self) -> Self { self.session_exported = true; self }
}

// ── Reward history ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RewardHistory {
    pub signals: Vec<RewardSignal>,
    pub session_id: String,
    pub total_reward: f32,
    pub step_count: u32,
}

impl RewardHistory {
    pub fn new(session_id: impl Into<String>) -> Self {
        RewardHistory { session_id: session_id.into(), ..Default::default() }
    }

    pub fn push(&mut self, signal: RewardSignal) {
        self.total_reward += signal.weighted();
        self.step_count += 1;
        self.signals.push(signal);
    }

    pub fn avg_reward(&self) -> f32 {
        if self.step_count == 0 { return 0.0; }
        self.total_reward / self.step_count as f32
    }

    pub fn positive_count(&self) -> usize {
        self.signals.iter().filter(|s| s.value > 0.0).count()
    }

    pub fn negative_count(&self) -> usize {
        self.signals.iter().filter(|s| s.value < 0.0).count()
    }

    pub fn last_n_avg(&self, n: usize) -> f32 {
        if self.signals.is_empty() { return 0.0; }
        let n = n.min(self.signals.len());
        let sum: f32 = self.signals.iter().rev().take(n).map(|s| s.weighted()).sum();
        sum / n as f32
    }

    pub fn signals_by_source(&self, source: &RewardSource) -> Vec<&RewardSignal> {
        self.signals.iter().filter(|s| &s.source == source).collect()
    }
}

// ── Reward model ──────────────────────────────────────────────────────────────

pub struct RewardModel {
    config: RewardConfig,
    histories: HashMap<String, RewardHistory>,
}

impl RewardModel {
    pub fn new(config: RewardConfig) -> Self {
        RewardModel { config, histories: HashMap::new() }
    }

    /// Compute composite reward from a single interaction snapshot.
    pub fn compute(&self, snap: &InteractionSnapshot) -> Vec<RewardSignal> {
        let mut signals = Vec::new();

        // Explicit suggestion signal
        if snap.suggestion_shown {
            match snap.suggestion_accepted {
                Some(true) => signals.push(RewardSignal::accepted(snap.timestamp_secs)),
                Some(false) => signals.push(RewardSignal::rejected(snap.timestamp_secs)),
                None => {}
            }
        }

        // Undo penalty overrides acceptance
        if snap.immediately_undone {
            signals.push(RewardSignal::undo(snap.timestamp_secs));
        }

        // Latency heuristic
        if snap.action_latency_ms > 0 {
            let latency_ratio = snap.action_latency_ms as f32 / self.config.baseline_latency_ms as f32;
            if latency_ratio < 0.5 {
                // Very fast — user knew exactly what to do
                let val = (0.5 - latency_ratio) * self.config.latency_weight;
                signals.push(RewardSignal {
                    value: val.min(self.config.max_reward),
                    source: RewardSource::FastCompletion,
                    confidence: 0.6,
                    timestamp_secs: snap.timestamp_secs,
                });
            } else if latency_ratio > 3.0 {
                // Very slow — user was confused
                let val = -(latency_ratio - 3.0).min(1.0) * self.config.latency_weight;
                signals.push(RewardSignal {
                    value: val,
                    source: RewardSource::SlowCompletion,
                    confidence: 0.4,
                    timestamp_secs: snap.timestamp_secs,
                });
            }
        }

        // WCAG bonus
        if snap.wcag_issues_fixed > 0 {
            let score = (snap.wcag_issues_fixed as f32 * 0.25).min(1.0);
            signals.push(RewardSignal::accessibility_bonus(score * self.config.accessibility_weight, snap.timestamp_secs));
        }

        // Canvas progress
        if snap.new_layers_created > 0 {
            signals.push(RewardSignal::canvas_progress(snap.timestamp_secs));
        }

        // Session completion
        if snap.session_exported {
            signals.push(RewardSignal::session_complete(snap.timestamp_secs));
        }

        signals
    }

    /// Compute and record reward for a session.
    pub fn record(&mut self, session_id: impl Into<String>, snap: &InteractionSnapshot) -> f32 {
        let session_id = session_id.into();
        let signals = self.compute(snap);
        let total: f32 = signals.iter().map(|s| s.weighted()).sum();
        let history = self.histories.entry(session_id).or_insert_with(|| RewardHistory::new(""));
        for s in signals { history.push(s); }
        total.clamp(-self.config.max_reward, self.config.max_reward)
    }

    pub fn session_history(&self, session_id: &str) -> Option<&RewardHistory> {
        self.histories.get(session_id)
    }

    pub fn global_avg_reward(&self) -> f32 {
        if self.histories.is_empty() { return 0.0; }
        let sum: f32 = self.histories.values().map(|h| h.avg_reward()).sum();
        sum / self.histories.len() as f32
    }
}

impl Default for RewardModel {
    fn default() -> Self { Self::new(RewardConfig::default()) }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(ts: u64) -> InteractionSnapshot {
        InteractionSnapshot::new("SetFill", ts)
    }

    #[test]
    fn reward_accepted_is_positive() {
        let model = RewardModel::default();
        let s = snap(0).with_suggestion(true, Some(true));
        let signals = model.compute(&s);
        assert!(signals.iter().any(|r| r.value > 0.0));
    }

    #[test]
    fn reward_rejected_is_negative() {
        let model = RewardModel::default();
        let s = snap(0).with_suggestion(true, Some(false));
        let signals = model.compute(&s);
        assert!(signals.iter().any(|r| r.value < 0.0));
    }

    #[test]
    fn reward_undo_is_negative() {
        let model = RewardModel::default();
        let s = snap(0).with_undo();
        let signals = model.compute(&s);
        assert!(signals.iter().any(|s| s.source == RewardSource::ImmediateUndo));
        assert!(signals.iter().all(|r| r.source != RewardSource::SuggestionAccepted || r.value < 0.0));
    }

    #[test]
    fn reward_fast_latency_bonus() {
        let model = RewardModel::default();
        let s = snap(0).with_latency(200); // 200ms vs 2000ms baseline = very fast
        let signals = model.compute(&s);
        assert!(signals.iter().any(|s| s.source == RewardSource::FastCompletion));
    }

    #[test]
    fn reward_slow_latency_penalty() {
        let model = RewardModel::default();
        let s = snap(0).with_latency(10_000); // 10s = very slow
        let signals = model.compute(&s);
        assert!(signals.iter().any(|s| s.source == RewardSource::SlowCompletion));
    }

    #[test]
    fn reward_accessibility_bonus() {
        let model = RewardModel::default();
        let s = snap(0).with_wcag_fix(3);
        let signals = model.compute(&s);
        assert!(signals.iter().any(|s| s.source == RewardSource::AccessibilityImprovement));
    }

    #[test]
    fn reward_session_complete_bonus() {
        let model = RewardModel::default();
        let s = snap(0).with_export();
        let signals = model.compute(&s);
        assert!(signals.iter().any(|s| s.source == RewardSource::SessionComplete));
    }

    #[test]
    fn reward_history_avg() {
        let mut history = RewardHistory::new("s");
        history.push(RewardSignal::accepted(0));
        history.push(RewardSignal::rejected(1));
        let avg = history.avg_reward();
        assert!(avg > -1.0 && avg < 1.0);
    }

    #[test]
    fn reward_history_last_n() {
        let mut h = RewardHistory::new("s");
        for _ in 0..5 { h.push(RewardSignal::accepted(0)); }
        for _ in 0..5 { h.push(RewardSignal::rejected(1)); }
        let last3 = h.last_n_avg(3);
        assert!(last3 < 0.0, "Last 3 are all rejections: {}", last3);
    }

    #[test]
    fn reward_model_record_tracks_history() {
        let mut model = RewardModel::default();
        let s = snap(0).with_suggestion(true, Some(true)).with_new_layers(2);
        model.record("sess-1", &s);
        assert!(model.session_history("sess-1").is_some());
        assert!(model.session_history("sess-1").unwrap().total_reward > 0.0);
    }

    #[test]
    fn global_avg_reward_positive_session() {
        let mut model = RewardModel::default();
        model.record("s1", &snap(0).with_suggestion(true, Some(true)));
        model.record("s2", &snap(1).with_suggestion(true, Some(true)));
        assert!(model.global_avg_reward() > 0.0);
    }
}
