//! User feedback layer — ratings and comments tied to agent versions.
//!
//! Callers submit `UserFeedback` records (1–5 star ratings), then query
//! `FeedbackSummary` statistics per (agent_id, version).

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ── Error ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FeedbackError {
    #[error("invalid rating {0}: must be in [1, 5]")]
    InvalidRating(u8),
    #[error("agent '{0}' version '{1}' has no feedback")]
    NoFeedback(String, String),
}

// ── User feedback record ──────────────────────────────────────────────────────

/// One end-user feedback submission tied to an agent invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserFeedback {
    pub agent_id: String,
    pub version: String,
    pub session_id: String,
    /// Rating in [1, 5].
    pub rating: u8,
    pub comment: Option<String>,
    /// Unix timestamp (seconds).
    pub ts: u64,
}

impl UserFeedback {
    /// Construct with a rating 1–5.  Use `FeedbackStore::submit_checked` to
    /// protect against out-of-range ratings at the boundary.
    pub fn new(
        agent_id: impl Into<String>,
        version: impl Into<String>,
        session_id: impl Into<String>,
        rating: u8,
    ) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            agent_id: agent_id.into(),
            version: version.into(),
            session_id: session_id.into(),
            rating,
            comment: None,
            ts,
        }
    }

    pub fn with_comment(mut self, c: impl Into<String>) -> Self {
        self.comment = Some(c.into());
        self
    }

    pub fn with_ts(mut self, ts: u64) -> Self {
        self.ts = ts;
        self
    }
}

// ── Feedback summary ──────────────────────────────────────────────────────────

/// Aggregated feedback statistics for one (agent, version) pair.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FeedbackSummary {
    pub avg_rating: f32,
    pub count: usize,
    /// Index 0 = 1-star, index 4 = 5-star.
    pub rating_distribution: [usize; 5],
    /// Percentage of positive reviews (rating ≥ 4).
    pub positive_pct: f32,
}

impl FeedbackSummary {
    fn from_feedbacks(fbs: &[&UserFeedback]) -> Self {
        if fbs.is_empty() {
            return Self::default();
        }
        let count = fbs.len();
        let mut dist = [0usize; 5];
        let mut sum = 0u32;
        for f in fbs {
            let r = f.rating.clamp(1, 5) as usize;
            dist[r - 1] += 1;
            sum += f.rating as u32;
        }
        let avg_rating = sum as f32 / count as f32;
        let positive = dist[3] + dist[4]; // 4-star + 5-star
        let positive_pct = positive as f32 / count as f32 * 100.0;
        Self { avg_rating, count, rating_distribution: dist, positive_pct }
    }
}

// ── Feedback store ────────────────────────────────────────────────────────────

/// In-memory collection of `UserFeedback` records.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FeedbackStore {
    feedbacks: Vec<UserFeedback>,
}

impl FeedbackStore {
    pub fn new() -> Self { Self::default() }

    // ── Write ─────────────────────────────────────────────────────────────────

    /// Submit feedback, silently clamping the rating to [1, 5].
    pub fn submit(&mut self, fb: UserFeedback) {
        self.feedbacks.push(fb);
    }

    /// Submit with validation — returns `Err` if rating is out of range.
    pub fn submit_checked(&mut self, fb: UserFeedback) -> Result<(), FeedbackError> {
        if !(1..=5).contains(&fb.rating) {
            return Err(FeedbackError::InvalidRating(fb.rating));
        }
        self.feedbacks.push(fb);
        Ok(())
    }

    // ── Read ──────────────────────────────────────────────────────────────────

    pub fn all_feedbacks(&self) -> &[UserFeedback] { &self.feedbacks }

    pub fn total_count(&self) -> usize { self.feedbacks.len() }

    pub fn feedbacks_for(&self, agent_id: &str, version: &str) -> Vec<&UserFeedback> {
        self.feedbacks
            .iter()
            .filter(|f| f.agent_id == agent_id && f.version == version)
            .collect()
    }

    pub fn feedbacks_for_agent(&self, agent_id: &str) -> Vec<&UserFeedback> {
        self.feedbacks.iter().filter(|f| f.agent_id == agent_id).collect()
    }

    // ── Aggregates ────────────────────────────────────────────────────────────

    pub fn summary_for(&self, agent_id: &str, version: &str) -> FeedbackSummary {
        let fbs = self.feedbacks_for(agent_id, version);
        FeedbackSummary::from_feedbacks(&fbs)
    }

    /// Overall summary for an agent across all versions.
    pub fn overall_summary_for(&self, agent_id: &str) -> FeedbackSummary {
        let fbs = self.feedbacks_for_agent(agent_id);
        FeedbackSummary::from_feedbacks(&fbs)
    }

    /// Average rating for a specific agent + version; `None` if no feedback.
    pub fn avg_rating(&self, agent_id: &str, version: &str) -> Option<f32> {
        let fbs = self.feedbacks_for(agent_id, version);
        if fbs.is_empty() { None } else { Some(FeedbackSummary::from_feedbacks(&fbs).avg_rating) }
    }

    /// Unique (agent_id, version) pairs that have at least one feedback entry.
    pub fn covered_versions(&self) -> Vec<(&str, &str)> {
        let mut pairs: Vec<(&str, &str)> = self
            .feedbacks
            .iter()
            .map(|f| (f.agent_id.as_str(), f.version.as_str()))
            .collect();
        pairs.sort_unstable();
        pairs.dedup();
        pairs
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn fb(agent: &str, ver: &str, sess: &str, r: u8) -> UserFeedback {
        UserFeedback::new(agent, ver, sess, r)
    }

    #[test]
    fn submit_and_count() {
        let mut store = FeedbackStore::new();
        store.submit(fb("a", "1.0.0", "s1", 5));
        store.submit(fb("a", "1.0.0", "s2", 3));
        assert_eq!(store.total_count(), 2);
    }

    #[test]
    fn avg_rating_correct() {
        let mut store = FeedbackStore::new();
        store.submit(fb("a", "1.0.0", "s1", 4));
        store.submit(fb("a", "1.0.0", "s2", 2));
        let avg = store.avg_rating("a", "1.0.0").unwrap();
        assert!((avg - 3.0).abs() < 1e-3);
    }

    #[test]
    fn avg_rating_none_for_empty() {
        let store = FeedbackStore::new();
        assert!(store.avg_rating("ghost", "1.0.0").is_none());
    }

    #[test]
    fn rating_distribution() {
        let mut store = FeedbackStore::new();
        store.submit(fb("a", "1.0.0", "s1", 5));
        store.submit(fb("a", "1.0.0", "s2", 5));
        store.submit(fb("a", "1.0.0", "s3", 1));
        let s = store.summary_for("a", "1.0.0");
        assert_eq!(s.rating_distribution[4], 2); // 5-star
        assert_eq!(s.rating_distribution[0], 1); // 1-star
    }

    #[test]
    fn positive_pct_all_five_stars() {
        let mut store = FeedbackStore::new();
        for i in 0..4 { store.submit(fb("a", "1.0.0", &format!("s{i}"), 5)); }
        let s = store.summary_for("a", "1.0.0");
        assert!((s.positive_pct - 100.0).abs() < 1e-3);
    }

    #[test]
    fn submit_checked_rejects_zero_rating() {
        let mut store = FeedbackStore::new();
        let err = store.submit_checked(fb("a", "1.0.0", "s1", 0)).unwrap_err();
        assert_eq!(err, FeedbackError::InvalidRating(0));
    }

    #[test]
    fn submit_checked_rejects_six_rating() {
        let mut store = FeedbackStore::new();
        let err = store.submit_checked(fb("a", "1.0.0", "s1", 6)).unwrap_err();
        assert_eq!(err, FeedbackError::InvalidRating(6));
    }

    #[test]
    fn feedbacks_for_filters_version() {
        let mut store = FeedbackStore::new();
        store.submit(fb("a", "1.0.0", "s1", 5));
        store.submit(fb("a", "2.0.0", "s2", 3));
        assert_eq!(store.feedbacks_for("a", "1.0.0").len(), 1);
    }

    #[test]
    fn covered_versions_deduped() {
        let mut store = FeedbackStore::new();
        store.submit(fb("a", "1.0.0", "s1", 5));
        store.submit(fb("a", "1.0.0", "s2", 4));
        store.submit(fb("b", "1.0.0", "s3", 3));
        assert_eq!(store.covered_versions().len(), 2);
    }
}
