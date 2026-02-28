//! RL-UX Agent — Reinforcement Learning-based UX assistant (Level 1)
//!
//! Observes user actions, builds a behavioral model, and learns to predict
//! the user's next likely action to offer proactive suggestions.
//! Uses a simple Q-table approach (no external ML dependencies) suitable
//! for on-device incremental learning.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

// ── UX Action ─────────────────────────────────────────────────────────────────

/// Discrete UI actions the user can take in Logos.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UxAction {
    CreateLayer,
    SelectLayer,
    ResizeLayer,
    MoveLayer,
    SetFill,
    SetOpacity,
    SetStroke,
    GroupLayers,
    UngroupLayers,
    DeleteLayer,
    UndoAction,
    RedoAction,
    OpenColorPicker,
    OpenTextEditor,
    RunAiSuggest,
    CheckAccessibility,
    ExportDesign,
    ZoomIn,
    ZoomOut,
    PanCanvas,
    EnterSpreadsheet,
    EditFormula,
    RunPipeline,
    AddPage,
    SwitchPage,
    InstallPlugin,
    RunPlugin,
    Custom(String),
}

impl UxAction {
    pub fn display_name(&self) -> String {
        format!("{:?}", self)
    }

    pub fn is_destructive(&self) -> bool {
        matches!(self, UxAction::DeleteLayer)
    }
}

// ── UX State ──────────────────────────────────────────────────────────────────

/// Current editor context that influences action predictions.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UxState {
    /// How many layers are selected.
    pub selection_count: u32,
    /// Whether a text layer is selected.
    pub text_selected: bool,
    /// Whether the spreadsheet panel is open.
    pub spreadsheet_open: bool,
    /// Whether any layer is locked.
    pub has_locked: bool,
    /// Current zoom level bucket (0=<50%, 1=50-100%, 2=100-200%, 3=>200%).
    pub zoom_bucket: u8,
}

impl UxState {
    pub fn empty() -> Self {
        UxState {
            selection_count: 0,
            text_selected: false,
            spreadsheet_open: false,
            has_locked: false,
            zoom_bucket: 1,
        }
    }

    pub fn with_selection(count: u32) -> Self {
        UxState { selection_count: count, ..Self::empty() }
    }
}

// ── UX Reward ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UxReward {
    /// Reward signal (-1.0 to +1.0). Positive = user approved suggestion.
    pub value: f32,
    pub action: UxAction,
    pub source: RewardSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RewardSource {
    /// User accepted the AI suggestion.
    SuggestionAccepted,
    /// User rejected the suggestion.
    SuggestionRejected,
    /// User performed the predicted action without being prompted.
    ObservedMatch,
    /// User performed a different action from prediction.
    ObservedMismatch,
}

// ── Behavior record ───────────────────────────────────────────────────────────

/// A single observed user interaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorRecord {
    pub state: UxState,
    pub action: UxAction,
    pub next_state: UxState,
    pub timestamp_secs: u64,
}

// ── Pattern matcher ───────────────────────────────────────────────────────────

/// Counts how often each (state, action) pair has been observed.
pub struct PatternMatcher {
    counts: HashMap<(UxState, UxAction), u32>,
    totals: HashMap<UxState, u32>,
}

impl PatternMatcher {
    pub fn new() -> Self {
        PatternMatcher {
            counts: HashMap::new(),
            totals: HashMap::new(),
        }
    }

    pub fn record(&mut self, state: UxState, action: UxAction) {
        *self.counts.entry((state.clone(), action)).or_insert(0) += 1;
        *self.totals.entry(state).or_insert(0) += 1;
    }

    /// Probability of `action` given `state`.
    pub fn prob(&self, state: &UxState, action: &UxAction) -> f32 {
        let total = *self.totals.get(state).unwrap_or(&0);
        if total == 0 { return 0.0; }
        let count = *self.counts.get(&(state.clone(), action.clone())).unwrap_or(&0);
        count as f32 / total as f32
    }

    /// Top-N most likely actions for a given state.
    pub fn top_actions(&self, state: &UxState, n: usize) -> Vec<(UxAction, f32)> {
        let total = *self.totals.get(state).unwrap_or(&0);
        if total == 0 { return vec![]; }
        let mut entries: Vec<(UxAction, f32)> = self.counts.iter()
            .filter_map(|((s, a), &count)| {
                if s == state {
                    Some((a.clone(), count as f32 / total as f32))
                } else {
                    None
                }
            })
            .collect();
        entries.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        entries.truncate(n);
        entries
    }

    pub fn observation_count(&self) -> u32 {
        self.totals.values().sum()
    }
}

impl Default for PatternMatcher {
    fn default() -> Self {
        Self::new()
    }
}

// ── Action predictor ──────────────────────────────────────────────────────────

/// Predicts the user's likely next action using a simple Q-table.
pub struct ActionPredictor {
    /// Q-table: (state_key, action) → expected reward.
    q_table: HashMap<(String, String), f32>,
    /// Learning rate.
    alpha: f32,
    /// Discount factor.
    gamma: f32,
}

impl ActionPredictor {
    pub fn new(alpha: f32, gamma: f32) -> Self {
        ActionPredictor {
            q_table: HashMap::new(),
            alpha,
            gamma,
        }
    }

    fn state_key(state: &UxState) -> String {
        format!("sel{}txt{}ss{}lk{}zm{}",
            state.selection_count,
            state.text_selected as u8,
            state.spreadsheet_open as u8,
            state.has_locked as u8,
            state.zoom_bucket)
    }

    fn action_key(action: &UxAction) -> String {
        format!("{:?}", action)
    }

    /// Update Q-value from observed transition.
    pub fn update(&mut self, state: &UxState, action: &UxAction, reward: f32, next_state: &UxState) {
        let sk = Self::state_key(state);
        let ak = Self::action_key(action);

        let current_q = *self.q_table.get(&(sk.clone(), ak.clone())).unwrap_or(&0.0);

        // Max Q of next state
        let next_sk = Self::state_key(next_state);
        let max_next_q = self.q_table.iter()
            .filter(|((s, _), _)| s == &next_sk)
            .map(|(_, &v)| v)
            .fold(0.0_f32, f32::max);

        let new_q = current_q + self.alpha * (reward + self.gamma * max_next_q - current_q);
        self.q_table.insert((sk, ak), new_q);
    }

    /// Predict the best action for the current state.
    pub fn predict(&self, state: &UxState) -> Option<UxAction> {
        let sk = Self::state_key(state);
        let best = self.q_table.iter()
            .filter(|((s, _), _)| s == &sk)
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal));

        best.map(|((_, ak), _)| Self::action_from_key(ak))
    }

    /// Get Q-value for (state, action) pair.
    pub fn q_value(&self, state: &UxState, action: &UxAction) -> f32 {
        let sk = Self::state_key(state);
        let ak = Self::action_key(action);
        *self.q_table.get(&(sk, ak)).unwrap_or(&0.0)
    }

    fn action_from_key(key: &str) -> UxAction {
        match key {
            "CreateLayer" => UxAction::CreateLayer,
            "SelectLayer" => UxAction::SelectLayer,
            "ResizeLayer" => UxAction::ResizeLayer,
            "MoveLayer" => UxAction::MoveLayer,
            "SetFill" => UxAction::SetFill,
            "SetOpacity" => UxAction::SetOpacity,
            "GroupLayers" => UxAction::GroupLayers,
            "DeleteLayer" => UxAction::DeleteLayer,
            "UndoAction" => UxAction::UndoAction,
            "RunAiSuggest" => UxAction::RunAiSuggest,
            "CheckAccessibility" => UxAction::CheckAccessibility,
            "ExportDesign" => UxAction::ExportDesign,
            _ => UxAction::Custom(key.to_string()),
        }
    }

    pub fn q_table_size(&self) -> usize {
        self.q_table.len()
    }
}

impl Default for ActionPredictor {
    fn default() -> Self {
        Self::new(0.1, 0.9) // standard RL defaults
    }
}

// ── UX agent config ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct UxAgentConfig {
    /// Minimum observations before making predictions.
    pub min_observations: u32,
    /// Minimum probability threshold for suggesting an action.
    pub suggestion_threshold: f32,
    /// Max suggestions to show simultaneously.
    pub max_suggestions: usize,
    /// Whether to apply RL or use frequency stats only.
    pub use_rl: bool,
}

impl Default for UxAgentConfig {
    fn default() -> Self {
        UxAgentConfig {
            min_observations: 10,
            suggestion_threshold: 0.25,
            max_suggestions: 3,
            use_rl: true,
        }
    }
}

// ── UX Agent ──────────────────────────────────────────────────────────────────

/// Level 1 RL-based UX assistant. Learns from user behavior over time.
pub struct UxAgent {
    config: UxAgentConfig,
    pub matcher: PatternMatcher,
    pub predictor: ActionPredictor,
    pub history: Vec<BehaviorRecord>,
}

impl UxAgent {
    pub fn new(config: UxAgentConfig) -> Self {
        UxAgent {
            config,
            matcher: PatternMatcher::new(),
            predictor: ActionPredictor::default(),
            history: Vec::new(),
        }
    }

    /// Record an observed user action.
    pub fn observe(&mut self, state: UxState, action: UxAction, next_state: UxState, ts: u64) {
        // Update pattern matcher
        self.matcher.record(state.clone(), action.clone());

        // RL update: reward = +0.1 for any observed action (neutral exploration)
        if self.config.use_rl {
            self.predictor.update(&state, &action, 0.1, &next_state);
        }

        self.history.push(BehaviorRecord {
            state,
            action,
            next_state,
            timestamp_secs: ts,
        });
    }

    /// Apply a reward signal based on user feedback.
    pub fn apply_reward(&mut self, reward: UxReward) {
        if let Some(last) = self.history.last() {
            self.predictor.update(
                &last.state,
                &reward.action,
                reward.value,
                &last.next_state,
            );
        }
    }

    /// Suggest the top actions for the current state.
    pub fn suggest(&self, state: &UxState) -> Vec<(UxAction, f32)> {
        if self.matcher.observation_count() < self.config.min_observations {
            return vec![];
        }

        let candidates = if self.config.use_rl {
            // Use RL Q-values for ranking
            let mut all: Vec<(UxAction, f32)> = [
                UxAction::CreateLayer, UxAction::ResizeLayer, UxAction::MoveLayer,
                UxAction::SetFill, UxAction::GroupLayers, UxAction::RunAiSuggest,
                UxAction::CheckAccessibility, UxAction::UndoAction,
            ].iter().map(|a| {
                let q = self.predictor.q_value(state, a);
                (a.clone(), q)
            }).collect();
            all.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            all
        } else {
            self.matcher.top_actions(state, self.config.max_suggestions * 2)
        };

        // Filter by threshold and limit count
        candidates.into_iter()
            .filter(|(_, score)| *score >= self.config.suggestion_threshold)
            .take(self.config.max_suggestions)
            .collect()
    }

    pub fn observation_count(&self) -> u32 {
        self.matcher.observation_count()
    }

    pub fn history_len(&self) -> usize {
        self.history.len()
    }
}

impl Default for UxAgent {
    fn default() -> Self {
        Self::new(UxAgentConfig::default())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_matcher_records_observations() {
        let mut pm = PatternMatcher::new();
        let state = UxState::with_selection(1);
        pm.record(state.clone(), UxAction::SetFill);
        pm.record(state.clone(), UxAction::SetFill);
        pm.record(state.clone(), UxAction::ResizeLayer);
        assert_eq!(pm.observation_count(), 3);
    }

    #[test]
    fn pattern_matcher_probability() {
        let mut pm = PatternMatcher::new();
        let state = UxState::with_selection(1);
        pm.record(state.clone(), UxAction::SetFill);
        pm.record(state.clone(), UxAction::SetFill);
        pm.record(state.clone(), UxAction::ResizeLayer);
        let p = pm.prob(&state, &UxAction::SetFill);
        assert!((p - 2.0 / 3.0).abs() < 0.01);
    }

    #[test]
    fn pattern_matcher_top_actions() {
        let mut pm = PatternMatcher::new();
        let state = UxState::with_selection(2);
        for _ in 0..5 { pm.record(state.clone(), UxAction::GroupLayers); }
        for _ in 0..3 { pm.record(state.clone(), UxAction::SetFill); }
        let top = pm.top_actions(&state, 2);
        assert_eq!(top[0].0, UxAction::GroupLayers);
        assert!(top[0].1 > top[1].1);
    }

    #[test]
    fn pattern_matcher_empty_state_returns_zero() {
        let pm = PatternMatcher::new();
        let state = UxState::empty();
        assert_eq!(pm.prob(&state, &UxAction::CreateLayer), 0.0);
    }

    #[test]
    fn predictor_q_value_zero_initially() {
        let pred = ActionPredictor::default();
        let state = UxState::empty();
        assert_eq!(pred.q_value(&state, &UxAction::CreateLayer), 0.0);
    }

    #[test]
    fn predictor_update_increases_q_value() {
        let mut pred = ActionPredictor::default();
        let state = UxState::empty();
        let next = UxState::with_selection(1);
        pred.update(&state, &UxAction::CreateLayer, 1.0, &next);
        assert!(pred.q_value(&state, &UxAction::CreateLayer) > 0.0);
    }

    #[test]
    fn predictor_predict_returns_best_action() {
        let mut pred = ActionPredictor::default();
        let state = UxState::with_selection(1);
        let next = UxState::with_selection(1);
        pred.update(&state, &UxAction::SetFill, 1.0, &next);
        pred.update(&state, &UxAction::ResizeLayer, 0.5, &next);
        let best = pred.predict(&state).unwrap();
        assert_eq!(best, UxAction::SetFill);
    }

    #[test]
    fn ux_agent_observes_and_records() {
        let mut agent = UxAgent::default();
        let s = UxState::empty();
        let ns = UxState::with_selection(1);
        agent.observe(s, UxAction::CreateLayer, ns, 0);
        assert_eq!(agent.history_len(), 1);
        assert_eq!(agent.observation_count(), 1);
    }

    #[test]
    fn ux_agent_no_suggestions_below_min_observations() {
        let config = UxAgentConfig { min_observations: 20, ..Default::default() };
        let mut agent = UxAgent::new(config);
        let s = UxState::with_selection(1);
        let ns = s.clone();
        // Only 5 observations
        for _ in 0..5 {
            agent.observe(s.clone(), UxAction::SetFill, ns.clone(), 0);
        }
        assert!(agent.suggest(&s).is_empty());
    }

    #[test]
    fn ux_agent_suggests_after_enough_observations() {
        let config = UxAgentConfig {
            min_observations: 5,
            suggestion_threshold: 0.0, // accept any score
            use_rl: false,
            ..Default::default()
        };
        let mut agent = UxAgent::new(config);
        let s = UxState::with_selection(1);
        let ns = s.clone();
        for _ in 0..10 {
            agent.observe(s.clone(), UxAction::SetFill, ns.clone(), 0);
        }
        let suggestions = agent.suggest(&s);
        assert!(!suggestions.is_empty());
    }

    #[test]
    fn ux_agent_apply_positive_reward() {
        let mut agent = UxAgent::default();
        let s = UxState::empty();
        let ns = UxState::with_selection(1);
        agent.observe(s.clone(), UxAction::RunAiSuggest, ns.clone(), 0);
        agent.apply_reward(UxReward {
            value: 1.0,
            action: UxAction::RunAiSuggest,
            source: RewardSource::SuggestionAccepted,
        });
        // Q-value should be positive now
        assert!(agent.predictor.q_value(&s, &UxAction::RunAiSuggest) > 0.0);
    }

    #[test]
    fn ux_action_destructive() {
        assert!(UxAction::DeleteLayer.is_destructive());
        assert!(!UxAction::SetFill.is_destructive());
        assert!(!UxAction::CreateLayer.is_destructive());
    }

    #[test]
    fn ux_state_empty_creates_default() {
        let s = UxState::empty();
        assert_eq!(s.selection_count, 0);
        assert!(!s.text_selected);
        assert!(!s.spreadsheet_open);
    }

    #[test]
    fn predictor_q_table_grows_with_updates() {
        let mut pred = ActionPredictor::default();
        let s = UxState::empty();
        let ns = s.clone();
        pred.update(&s, &UxAction::CreateLayer, 1.0, &ns);
        pred.update(&s, &UxAction::SetFill, 0.5, &ns);
        assert_eq!(pred.q_table_size(), 2);
    }

    #[test]
    fn ux_agent_history_grows() {
        let mut agent = UxAgent::default();
        for i in 0..10u64 {
            let s = UxState::with_selection(i as u32 % 3);
            let ns = UxState::with_selection((i + 1) as u32 % 3);
            agent.observe(s, UxAction::MoveLayer, ns, i);
        }
        assert_eq!(agent.history_len(), 10);
    }

    #[test]
    fn pattern_matcher_top_actions_limit() {
        let mut pm = PatternMatcher::new();
        let state = UxState::empty();
        pm.record(state.clone(), UxAction::CreateLayer);
        pm.record(state.clone(), UxAction::SetFill);
        pm.record(state.clone(), UxAction::MoveLayer);
        pm.record(state.clone(), UxAction::ResizeLayer);
        let top = pm.top_actions(&state, 2);
        assert!(top.len() <= 2);
    }
}
