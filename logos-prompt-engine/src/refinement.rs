//! Iterative refinement — loop-based self-improvement for agent responses.
//!
//! `RefinementSession` tracks multiple rounds of (response → critique → revision),
//! `CritiqueTemplate` builds the self-critique prompt, and `FeedbackAnnotation`
//! captures human or automated feedback for analytics.

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

// ── Critique template ─────────────────────────────────────────────────────────

pub struct CritiqueTemplate;

impl CritiqueTemplate {
    /// Build a self-critique prompt asking the model to review its prior response.
    pub fn build_critique_prompt(original_task: &str, prior_response: &str) -> String {
        format!(
            "You previously responded to the following task:\n\
             ---\n\
             TASK: {original_task}\n\
             ---\n\
             YOUR RESPONSE:\n\
             {prior_response}\n\
             ---\n\n\
             Now critically review your response. Identify:\n\
             1. Any factual errors or invalid assumptions.\n\
             2. Missing information that would make the answer more complete.\n\
             3. Opportunities to improve clarity or structure.\n\n\
             After your critique, provide a REVISED ANSWER that addresses the gaps.\n\n\
             Critique:\n\
             Revised Answer:"
        )
    }

    /// Build a focused quality-improvement prompt for a specific issue.
    pub fn build_focused_prompt(original_task: &str, prior_response: &str, issue: &str) -> String {
        format!(
            "Task: {original_task}\n\n\
             Previous response: {prior_response}\n\n\
             Identified issue: {issue}\n\n\
             Provide an improved response that specifically addresses this issue:\n\
             Improved Response:"
        )
    }
}

// ── Refinement config ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefinementConfig {
    /// Maximum number of refinement rounds (not counting round 0).
    pub max_rounds: u32,
    /// A round is considered an improvement when the response changes non-trivially.
    /// (A future implementation would use embedding similarity; here we use length diff.)
    pub require_improvement: bool,
    /// Stop early if `require_improvement` is true and this many consecutive rounds
    /// showed no improvement.
    pub early_stop_patience: u32,
}

impl Default for RefinementConfig {
    fn default() -> Self {
        Self {
            max_rounds: 3,
            require_improvement: false,
            early_stop_patience: 2,
        }
    }
}

// ── Refinement round ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefinementRound {
    /// 0 = initial response, 1+ = refined rounds
    pub round: u32,
    pub response: String,
    pub critique: Option<String>,
    /// Simplified improvement signal: true if response is materially different from prior
    pub improved: bool,
    pub timestamp_secs: u64,
}

impl RefinementRound {
    pub fn initial(response: impl Into<String>, ts: u64) -> Self {
        Self { round: 0, response: response.into(), critique: None, improved: true, timestamp_secs: ts }
    }

    pub fn refined(
        round: u32,
        response: impl Into<String>,
        critique: impl Into<String>,
        improved: bool,
        ts: u64,
    ) -> Self {
        Self {
            round,
            response: response.into(),
            critique: Some(critique.into()),
            improved,
            timestamp_secs: ts,
        }
    }
}

// ── Refinement session ────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct RefinementSession {
    pub session_id: String,
    pub task: String,
    pub config: RefinementConfig,
    pub rounds: Vec<RefinementRound>,
    pub final_response: Option<String>,
    pub created_ts: u64,
}

impl RefinementSession {
    pub fn new(session_id: impl Into<String>, task: impl Into<String>, config: RefinementConfig) -> Self {
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        Self {
            session_id: session_id.into(),
            task: task.into(),
            config,
            rounds: Vec::new(),
            final_response: None,
            created_ts: ts,
        }
    }

    /// Push the initial agent response (round 0).
    pub fn start(&mut self, initial_response: impl Into<String>, ts: u64) {
        self.rounds.push(RefinementRound::initial(initial_response, ts));
    }

    /// Push a refined round. `improved` is true when the new response is
    /// meaningfully different from (better than) the previous round.
    pub fn add_round(
        &mut self,
        response: impl Into<String>,
        critique: impl Into<String>,
        improved: bool,
        ts: u64,
    ) {
        let round_n = self.rounds.len() as u32;
        self.rounds.push(RefinementRound::refined(round_n, response, critique, improved, ts));
    }

    /// Auto-detect improvement by simple length and content change heuristic.
    pub fn add_round_auto_detect(
        &mut self,
        response: impl Into<String>,
        critique: impl Into<String>,
        ts: u64,
    ) {
        let new_response: String = response.into();
        let improved = self.rounds.last()
            .map(|prev| {
                // Heuristic: content changed AND new is longer/same length
                prev.response != new_response
                    && new_response.len() >= prev.response.len().saturating_sub(20)
            })
            .unwrap_or(true);
        self.add_round(new_response, critique, improved, ts);
    }

    /// Has reached max rounds without finding a stopping condition.
    pub fn is_done(&self) -> bool {
        let refinement_rounds = self.rounds.len().saturating_sub(1) as u32;
        if refinement_rounds >= self.config.max_rounds { return true; }
        if self.config.require_improvement {
            let patience = self.config.early_stop_patience as usize;
            if self.rounds.len() > patience {
                let recent: Vec<bool> = self.rounds.iter()
                    .rev().take(patience).map(|r| r.improved).collect();
                if recent.iter().all(|&i| !i) { return true; }
            }
        }
        false
    }

    pub fn round_count(&self) -> usize { self.rounds.len() }

    /// Returns last response that was marked as improved, or simply the last response.
    pub fn best_response(&self) -> Option<&str> {
        // Try to find the last improved round
        self.rounds.iter().rev()
            .find(|r| r.improved && r.round > 0)
            .or_else(|| self.rounds.last())
            .map(|r| r.response.as_str())
    }

    pub fn improvement_trajectory(&self) -> Vec<bool> {
        self.rounds.iter().map(|r| r.improved).collect()
    }

    pub fn finalize(&mut self) {
        self.final_response = self.best_response().map(|s| s.to_string());
    }

    pub fn latest_response(&self) -> Option<&str> {
        self.rounds.last().map(|r| r.response.as_str())
    }

    pub fn consecutive_no_improvement(&self) -> u32 {
        self.rounds.iter().rev()
            .take_while(|r| !r.improved)
            .count() as u32
    }

    /// Build the next critique prompt for this session.
    pub fn next_critique_prompt(&self) -> Option<String> {
        let latest = self.rounds.last()?;
        Some(CritiqueTemplate::build_critique_prompt(&self.task, &latest.response))
    }
}

// ── Feedback annotation ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackAnnotation {
    pub annotation_id: String,
    pub session_id: String,
    pub round: u32,
    pub annotator: String,
    pub score: f32,
    pub comment: String,
    pub timestamp_secs: u64,
}

impl FeedbackAnnotation {
    pub fn new(
        session_id: impl Into<String>,
        round: u32,
        annotator: impl Into<String>,
        score: f32,
        comment: impl Into<String>,
        ts: u64,
    ) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        Self {
            annotation_id: format!("ann-{n}"),
            session_id: session_id.into(),
            round,
            annotator: annotator.into(),
            score: score.clamp(0.0, 1.0),
            comment: comment.into(),
            timestamp_secs: ts,
        }
    }

    pub fn is_positive(&self) -> bool { self.score >= 0.5 }
}

// ── Feedback store ────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct FeedbackStore {
    annotations: Vec<FeedbackAnnotation>,
}

impl FeedbackStore {
    pub fn new() -> Self { Self::default() }

    pub fn add(&mut self, annotation: FeedbackAnnotation) { self.annotations.push(annotation); }

    pub fn for_session(&self, session_id: &str) -> Vec<&FeedbackAnnotation> {
        self.annotations.iter().filter(|a| a.session_id == session_id).collect()
    }

    pub fn average_score_for(&self, session_id: &str) -> Option<f32> {
        let items = self.for_session(session_id);
        if items.is_empty() { return None; }
        Some(items.iter().map(|a| a.score).sum::<f32>() / items.len() as f32)
    }

    pub fn total(&self) -> usize { self.annotations.len() }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_session() -> RefinementSession {
        RefinementSession::new("sess-1", "Design a login screen", RefinementConfig::default())
    }

    #[test]
    fn session_starts_with_round_zero() {
        let mut s = make_session();
        s.start("Initial layout design.", 100);
        assert_eq!(s.round_count(), 1);
        assert_eq!(s.latest_response().unwrap(), "Initial layout design.");
    }

    #[test]
    fn session_add_rounds_and_is_done() {
        let mut s = make_session(); // max_rounds = 3
        s.start("v0", 0);
        s.add_round("v1", "Minor spacing fix", true, 1);
        s.add_round("v2", "Improved contrast", true, 2);
        assert!(!s.is_done());
        s.add_round("v3", "Nothing left to improve", false, 3);
        assert!(s.is_done()); // 3 refinement rounds reached
    }

    #[test]
    fn best_response_returns_last_improved() {
        let mut s = make_session();
        s.start("v0", 0);
        s.add_round("v1 (better)", "critique", true, 1);
        s.add_round("v2 (no change)", "critique", false, 2);
        assert_eq!(s.best_response().unwrap(), "v1 (better)");
    }

    #[test]
    fn improvement_trajectory() {
        let mut s = make_session();
        s.start("v0", 0);
        s.add_round("v1", "c", true, 1);
        s.add_round("v2", "c", false, 2);
        assert_eq!(s.improvement_trajectory(), vec![true, true, false]);
    }

    #[test]
    fn auto_detect_improvement_detects_identical() {
        let mut s = make_session();
        s.start("same content here", 0);
        s.add_round_auto_detect("same content here", "no change", 1);
        assert!(!s.rounds.last().unwrap().improved);
    }

    #[test]
    fn critique_template_contains_task_and_response() {
        let prompt = CritiqueTemplate::build_critique_prompt("Design a button", "It is blue.");
        assert!(prompt.contains("Design a button"));
        assert!(prompt.contains("It is blue."));
        assert!(prompt.contains("Critique:"));
    }

    #[test]
    fn next_critique_prompt_uses_latest_response() {
        let mut s = make_session();
        s.start("initial response", 0);
        let prompt = s.next_critique_prompt().unwrap();
        assert!(prompt.contains("initial response"));
    }

    #[test]
    fn finalize_captures_best_response() {
        let mut s = make_session();
        s.start("v0", 0);
        s.add_round("v1 better", "c", true, 1);
        s.finalize();
        assert_eq!(s.final_response.as_deref(), Some("v1 better"));
    }

    #[test]
    fn early_stop_with_patience() {
        let config = RefinementConfig { require_improvement: true, early_stop_patience: 2, max_rounds: 10 };
        let mut s = RefinementSession::new("s", "task", config);
        s.start("v0", 0);
        s.add_round("v1", "c", false, 1);
        s.add_round("v2", "c", false, 2); // 2 consecutive no-improvement
        assert!(s.is_done());
    }

    #[test]
    fn feedback_store_average_score() {
        let mut store = FeedbackStore::new();
        store.add(FeedbackAnnotation::new("s1", 0, "human", 0.8, "good", 0));
        store.add(FeedbackAnnotation::new("s1", 1, "human", 0.6, "ok",   1));
        let avg = store.average_score_for("s1").unwrap();
        assert!((avg - 0.7).abs() < 0.01);
    }
}
