//! Training pipeline — score-driven rounds of prompt improvement with rubric
//! evaluation and automatic certification.
//!
//! Use [`TrainingSession`] to drive an iterative training loop:
//!
//! 1. `start()` provides the initial agent response.
//! 2. `train_round()` adds subsequent improved responses.
//! 3. `is_certified()` returns `true` once the rubric score meets the threshold.
//! 4. `finalize()` records the final response.

use serde::{Deserialize, Serialize};

// ── Rubric criterion ──────────────────────────────────────────────────────────

/// A single weighted evaluation criterion.
///
/// A response *passes* a criterion when the criterion's `keyword` is found
/// (case-insensitive) anywhere in the response text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RubricCriterion {
    /// Keyword to look for in the response (case-insensitive substring match).
    pub keyword: String,
    /// Relative weight of this criterion (must be > 0).
    pub weight: f32,
    /// Human-readable description of what this criterion tests.
    pub description: String,
}

impl RubricCriterion {
    pub fn new(
        keyword: impl Into<String>,
        weight: f32,
        description: impl Into<String>,
    ) -> Self {
        Self {
            keyword: keyword.into(),
            weight: weight.max(0.0),
            description: description.into(),
        }
    }
}

// ── Rubric evaluator ──────────────────────────────────────────────────────────

/// Evaluates a response against a set of weighted [`RubricCriterion`]s.
///
/// Score is the sum of weights of matched criteria divided by the total weight,
/// giving a value in `[0.0, 1.0]`. Returns `0.0` for an empty evaluator.
#[derive(Debug, Default)]
pub struct RubricEvaluator {
    criteria: Vec<RubricCriterion>,
}

impl RubricEvaluator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a criterion to the evaluator.
    pub fn add_criterion(&mut self, criterion: RubricCriterion) {
        self.criteria.push(criterion);
    }

    /// Score a response: `matched_weight / total_weight` in `[0.0, 1.0]`.
    pub fn score(&self, response: &str) -> f32 {
        if self.criteria.is_empty() {
            return 0.0;
        }
        let total_weight: f32 = self.criteria.iter().map(|c| c.weight).sum();
        if total_weight <= 0.0 {
            return 0.0;
        }
        let response_lower = response.to_lowercase();
        let matched: f32 = self
            .criteria
            .iter()
            .filter(|c| response_lower.contains(&c.keyword.to_lowercase()))
            .map(|c| c.weight)
            .sum();
        matched / total_weight
    }

    /// Number of criteria registered.
    pub fn criterion_count(&self) -> usize {
        self.criteria.len()
    }
}

// ── Training config ───────────────────────────────────────────────────────────

/// Configuration for a [`TrainingSession`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingConfig {
    /// Maximum number of training rounds (not counting the initial `start` round).
    pub max_rounds: u32,
    /// Minimum rubric score required for certification (`0.0`–`1.0`).
    pub threshold: f32,
    /// Optional label attached to the session when it reaches certification.
    pub certification_tag: Option<String>,
}

impl Default for TrainingConfig {
    fn default() -> Self {
        Self {
            max_rounds: 10,
            threshold: 0.75,
            certification_tag: None,
        }
    }
}

impl TrainingConfig {
    /// Set the minimum score required for certification.
    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.threshold = threshold.clamp(0.0, 1.0);
        self
    }

    /// Attach a certification tag (e.g. `"phase-15.5"`).
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.certification_tag = Some(tag.into());
        self
    }
}

// ── Training session ──────────────────────────────────────────────────────────

/// A scored, iterative training loop that drives an agent toward certification.
///
/// # Lifecycle
///
/// ```text
/// TrainingSession::new(…)
///   → start(initial_response, …)              ← round 0
///   → train_round(better_response, …)         ← round 1..N
///   → is_certified() / is_done()
///   → finalize()
/// ```
pub struct TrainingSession {
    /// Unique session identifier.
    pub id: String,
    /// The task description used as the training objective.
    pub task: String,
    /// Session configuration.
    pub config: TrainingConfig,
    /// The rubric evaluator used to score each response.
    pub evaluator: RubricEvaluator,
    /// Rubric score for each round (index 0 = initial `start` round).
    ///
    /// Can be manually overwritten for testing purposes.
    pub scores: Vec<f32>,
    /// Response set as the canonical "best" output after `finalize()`.
    pub final_response: Option<String>,
    /// Internal response history (one entry per round).
    responses: Vec<String>,
}

impl TrainingSession {
    /// Create a new training session.
    pub fn new(
        id: impl Into<String>,
        task: impl Into<String>,
        config: TrainingConfig,
        evaluator: RubricEvaluator,
    ) -> Self {
        Self {
            id: id.into(),
            task: task.into(),
            config,
            evaluator,
            scores: Vec::new(),
            final_response: None,
            responses: Vec::new(),
        }
    }

    /// Submit the initial response (round 0) and record its rubric score.
    ///
    /// `_expected_keywords` is metadata for callers; scoring is driven entirely
    /// by the registered [`RubricCriterion`]s.
    pub fn start(&mut self, response: impl Into<String>, _expected_keywords: &[&str], _ts: u64) {
        let response = response.into();
        let score = self.evaluator.score(&response);
        self.scores.push(score);
        self.responses.push(response);
    }

    /// Submit a refined response for the next training round.
    ///
    /// `_critique` and `_expected_keywords` are logged for introspection;
    /// the numeric score is computed solely from the evaluator criteria.
    pub fn train_round(
        &mut self,
        response: impl Into<String>,
        _critique: impl Into<String>,
        _expected_keywords: &[&str],
        _ts: u64,
    ) {
        let response = response.into();
        let score = self.evaluator.score(&response);
        self.scores.push(score);
        self.responses.push(response);
    }

    /// `true` when any recorded score meets or exceeds the certification threshold.
    pub fn is_certified(&self) -> bool {
        self.scores.iter().any(|&s| s >= self.config.threshold)
    }

    /// `true` when the session should stop — either certified or max rounds reached.
    pub fn is_done(&self) -> bool {
        if self.is_certified() {
            return true;
        }
        // `scores` has len = rounds completed (0-indexed); training rounds = len - 1
        let training_rounds = self.scores.len().saturating_sub(1) as u32;
        training_rounds >= self.config.max_rounds
    }

    /// Ordered rubric scores for every round (index 0 = initial `start` call).
    pub fn score_trajectory(&self) -> Vec<f32> {
        self.scores.clone()
    }

    /// Highest score seen across all rounds.
    ///
    /// Returns `f32::NEG_INFINITY` if no rounds have been recorded yet, or the
    /// manually highest value in `self.scores` otherwise.
    pub fn best_score(&self) -> f32 {
        self.scores
            .iter()
            .cloned()
            .fold(f32::NEG_INFINITY, f32::max)
    }

    /// The certification tag from the config, if any.
    pub fn certification_tag(&self) -> Option<&str> {
        self.config.certification_tag.as_deref()
    }

    /// Lock in the last recorded response as the final output.
    pub fn finalize(&mut self) {
        self.final_response = self.responses.last().cloned();
    }

    /// Number of rounds recorded so far (including the initial `start` call).
    pub fn round_count(&self) -> usize {
        self.scores.len()
    }

    /// The response from the last round, if any.
    pub fn latest_response(&self) -> Option<&str> {
        self.responses.last().map(|s| s.as_str())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_evaluator() -> RubricEvaluator {
        let mut ev = RubricEvaluator::new();
        ev.add_criterion(RubricCriterion::new("contrast", 1.0, "WCAG contrast check"));
        ev.add_criterion(RubricCriterion::new("aria", 0.8, "ARIA labels present"));
        ev
    }

    fn make_session() -> TrainingSession {
        TrainingSession::new(
            "sess-01",
            "Design accessible button",
            TrainingConfig::default(),
            make_evaluator(),
        )
    }

    // ── RubricEvaluator ───────────────────────────────────────────────────────

    #[test]
    fn evaluator_empty_returns_zero() {
        let ev = RubricEvaluator::new();
        assert!((ev.score("anything")).abs() < 0.001);
    }

    #[test]
    fn evaluator_all_matched_returns_one() {
        let ev = make_evaluator(); // contrast + aria
        let s = ev.score("contrast ratio 4.5:1 with aria-label");
        assert!((s - 1.0).abs() < 0.001);
    }

    #[test]
    fn evaluator_partial_match() {
        let ev = make_evaluator(); // total weight = 1.8
        let s = ev.score("contrast ratio looks fine"); // "contrast" ✓, "aria" ✗
        // matched_weight = 1.0, total = 1.8
        let expected = 1.0_f32 / 1.8;
        assert!((s - expected).abs() < 0.001);
    }

    #[test]
    fn evaluator_case_insensitive() {
        let mut ev = RubricEvaluator::new();
        ev.add_criterion(RubricCriterion::new("ARIA", 1.0, ""));
        assert!((ev.score("aria-label is set")).abs() > 0.5);
    }

    #[test]
    fn evaluator_criterion_count() {
        let ev = make_evaluator();
        assert_eq!(ev.criterion_count(), 2);
    }

    // ── TrainingConfig ────────────────────────────────────────────────────────

    #[test]
    fn config_default_threshold() {
        let c = TrainingConfig::default();
        assert!((c.threshold - 0.75).abs() < 0.001);
    }

    #[test]
    fn config_with_threshold_clamped() {
        let c = TrainingConfig::default().with_threshold(1.5);
        assert!((c.threshold - 1.0).abs() < 0.001);
        let c2 = TrainingConfig::default().with_threshold(-0.1);
        assert!((c2.threshold - 0.0).abs() < 0.001);
    }

    #[test]
    fn config_with_tag() {
        let c = TrainingConfig::default().with_tag("v15.5");
        assert_eq!(c.certification_tag.as_deref(), Some("v15.5"));
    }

    // ── TrainingSession ───────────────────────────────────────────────────────

    #[test]
    fn session_initial_round_recorded() {
        let mut s = make_session();
        s.start("blue button", &[], 0);
        assert_eq!(s.round_count(), 1);
    }

    #[test]
    fn session_not_certified_on_no_keywords() {
        let mut s = make_session();
        s.start("plain button with no special keywords", &[], 0);
        assert!(!s.is_certified());
    }

    #[test]
    fn session_certified_when_threshold_met() {
        let mut s = make_session();
        s.start("button", &[], 0);
        s.train_round(
            "contrast ratio 5:1 with aria-label='Close'",
            "matched",
            &[],
            1,
        );
        assert!(s.is_certified());
    }

    #[test]
    fn session_is_done_when_certified() {
        let mut s = make_session();
        s.start("contrast and aria labels everywhere", &[], 0);
        assert!(s.is_certified());
        assert!(s.is_done());
    }

    #[test]
    fn session_is_done_after_max_rounds() {
        let config = TrainingConfig::default().with_threshold(1.0);
        let mut s = TrainingSession::new("x", "t", config, RubricEvaluator::new());
        s.start("no match", &[], 0); // score 0
        // max_rounds = 10; add 10 training rounds (all score 0)
        for i in 1..=10_u64 {
            s.train_round("no match", "c", &[], i);
        }
        assert!(s.is_done());
    }

    #[test]
    fn session_score_trajectory_length() {
        let mut s = make_session();
        s.start("r0", &[], 0);
        s.train_round("r1", "c", &[], 1);
        s.train_round("r2", "c", &[], 2);
        assert_eq!(s.score_trajectory().len(), 3);
    }

    #[test]
    fn session_best_score_is_max() {
        let mut s = make_session();
        s.scores = vec![0.1, 0.9, 0.4];
        assert!((s.best_score() - 0.9).abs() < 0.001);
    }

    #[test]
    fn session_certification_tag_propagates() {
        let config = TrainingConfig::default().with_tag("phase-15.5");
        let s = TrainingSession::new("t", "task", config, RubricEvaluator::new());
        assert_eq!(s.certification_tag(), Some("phase-15.5"));
    }

    #[test]
    fn session_certification_tag_none_by_default() {
        let s = make_session();
        assert!(s.certification_tag().is_none());
    }

    #[test]
    fn session_finalize_records_last_response() {
        let mut s = make_session();
        s.start("v0", &[], 0);
        s.train_round("v1 final", "c", &[], 1);
        s.finalize();
        assert_eq!(s.final_response.as_deref(), Some("v1 final"));
    }

    #[test]
    fn session_latest_response() {
        let mut s = make_session();
        s.start("first", &[], 0);
        s.train_round("second", "c", &[], 1);
        assert_eq!(s.latest_response(), Some("second"));
    }

    #[test]
    fn session_score_increases_after_improvement() {
        let mut s = make_session();
        s.start("no keywords here", &[], 0);
        s.train_round("contrast ratio and aria-label", "improved", &[], 1);
        let traj = s.score_trajectory();
        assert!(traj[1] > traj[0]);
    }
}
