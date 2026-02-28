//! Ratings and reviews — 5-star ratings, written reviews, and moderation.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Moderation status ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModerationStatus {
    Pending,
    Approved,
    Rejected { reason: String },
    Flagged { flag_count: u32 },
}

impl ModerationStatus {
    pub fn is_visible(&self) -> bool {
        matches!(self, Self::Approved)
    }
}

// ── Rating (numeric score only) ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rating {
    pub rating_id: String,
    pub agent_id: String,
    pub user_id: String,
    /// 1–5 stars
    pub stars: u8,
    pub timestamp_secs: u64,
}

impl Rating {
    pub fn new(agent_id: impl Into<String>, user_id: impl Into<String>, stars: u8, ts: u64) -> Self {
        Self {
            rating_id: uuid_str(),
            agent_id: agent_id.into(),
            user_id: user_id.into(),
            stars: stars.clamp(1, 5),
            timestamp_secs: ts,
        }
    }
}

// ── Review (rating + written text) ───────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Review {
    pub review_id: String,
    pub agent_id: String,
    pub user_id: String,
    pub display_name: String,
    /// 1–5 stars
    pub stars: u8,
    /// Optional short title (max 80 chars)
    pub title: Option<String>,
    /// Review body (max 2000 chars)
    pub body: String,
    pub helpful_votes: u32,
    pub unhelpful_votes: u32,
    pub status: ModerationStatus,
    pub timestamp_secs: u64,
    pub edited_ts: Option<u64>,
}

impl Review {
    pub fn new(
        agent_id: impl Into<String>,
        user_id: impl Into<String>,
        display_name: impl Into<String>,
        stars: u8,
        body: impl Into<String>,
        ts: u64,
    ) -> Self {
        Self {
            review_id: uuid_str(),
            agent_id: agent_id.into(),
            user_id: user_id.into(),
            display_name: display_name.into(),
            stars: stars.clamp(1, 5),
            title: None,
            body: body.into(),
            helpful_votes: 0,
            unhelpful_votes: 0,
            status: ModerationStatus::Pending,
            timestamp_secs: ts,
            edited_ts: None,
        }
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into()); self
    }

    pub fn approve(mut self) -> Self {
        self.status = ModerationStatus::Approved; self
    }

    pub fn reject(mut self, reason: impl Into<String>) -> Self {
        self.status = ModerationStatus::Rejected { reason: reason.into() }; self
    }

    pub fn is_visible(&self) -> bool { self.status.is_visible() }

    pub fn helpfulness_score(&self) -> f32 {
        let total = self.helpful_votes + self.unhelpful_votes;
        if total == 0 { return 0.5; }
        self.helpful_votes as f32 / total as f32
    }
}

// ── Rating summary ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RatingSummary {
    pub agent_id: String,
    pub total_ratings: u32,
    pub avg_rating: f32,
    /// Count per star: index 0 = 1-star, index 4 = 5-star
    pub star_counts: [u32; 5],
    pub review_count: u32,
}

impl RatingSummary {
    pub fn new(agent_id: impl Into<String>) -> Self {
        Self { agent_id: agent_id.into(), ..Default::default() }
    }

    pub fn add_rating(&mut self, stars: u8) {
        let idx = (stars.clamp(1, 5) - 1) as usize;
        self.star_counts[idx] += 1;
        self.total_ratings += 1;
        // Recompute EMA average
        let total: u32 = self.star_counts.iter().sum();
        let weighted: u32 = self.star_counts.iter().enumerate()
            .map(|(i, &c)| c * (i as u32 + 1))
            .sum();
        self.avg_rating = if total > 0 { weighted as f32 / total as f32 } else { 0.0 };
    }

    pub fn star_pct(&self, stars: u8) -> f32 {
        if self.total_ratings == 0 { return 0.0; }
        let idx = (stars.clamp(1, 5) - 1) as usize;
        self.star_counts[idx] as f32 / self.total_ratings as f32 * 100.0
    }

    pub fn increment_review_count(&mut self) { self.review_count += 1; }
}

// ── Review store ──────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct ReviewStore {
    /// agent_id → list of reviews
    reviews: HashMap<String, Vec<Review>>,
    /// agent_id → rating summary
    summaries: HashMap<String, RatingSummary>,
    /// user_id → set of agent_ids they've rated (one rating per user per agent)
    user_ratings: HashMap<String, HashMap<String, u8>>,
}

impl ReviewStore {
    pub fn new() -> Self { Self::default() }

    // ── Submit rating only ────────────────────────────────────────────────

    /// Returns false if the user already rated this agent (update not allowed
    /// directly; must `update_rating` instead).
    pub fn submit_rating(&mut self, rating: Rating) -> bool {
        if self.user_ratings
            .entry(rating.user_id.clone())
            .or_default()
            .contains_key(&rating.agent_id) { return false; }
        let stars = rating.stars;
        let agent_id = rating.agent_id.clone();
        let user_id = rating.user_id.clone();
        self.user_ratings.entry(user_id).or_default().insert(agent_id.clone(), stars);
        self.summaries.entry(agent_id).or_insert_with(|| RatingSummary::new(&rating.agent_id)).add_rating(stars);
        true
    }

    pub fn update_rating(&mut self, agent_id: &str, user_id: &str, new_stars: u8) -> bool {
        if let Some(user_map) = self.user_ratings.get_mut(user_id) {
            if let Some(old_stars) = user_map.get_mut(agent_id) {
                let summary = self.summaries.entry(agent_id.to_string())
                    .or_insert_with(|| RatingSummary::new(agent_id));
                // Remove old star
                let old_idx = (*old_stars).clamp(1, 5) as usize - 1;
                if summary.star_counts[old_idx] > 0 { summary.star_counts[old_idx] -= 1; }
                if summary.total_ratings > 0 { summary.total_ratings -= 1; }
                // Add new star
                summary.add_rating(new_stars);
                *old_stars = new_stars;
                return true;
            }
        }
        false
    }

    // ── Submit review ─────────────────────────────────────────────────────

    pub fn submit_review(&mut self, review: Review) {
        let agent_id = review.agent_id.clone();
        let stars = review.stars;
        let was_approved = review.is_visible();
        self.reviews.entry(agent_id.clone()).or_default().push(review);
        // Auto-count the rating if approved
        if was_approved {
            self.summaries.entry(agent_id.clone())
                .or_insert_with(|| RatingSummary::new(&agent_id))
                .add_rating(stars);
            self.summaries.get_mut(&agent_id).unwrap().increment_review_count();
        }
    }

    pub fn approve_review(&mut self, agent_id: &str, review_id: &str) -> bool {
        if let Some(list) = self.reviews.get_mut(agent_id) {
            if let Some(r) = list.iter_mut().find(|r| r.review_id == review_id) {
                if !r.is_visible() {
                    let stars = r.stars;
                    r.status = ModerationStatus::Approved;
                    self.summaries.entry(agent_id.to_string())
                        .or_insert_with(|| RatingSummary::new(agent_id))
                        .add_rating(stars);
                    self.summaries.get_mut(agent_id).unwrap().increment_review_count();
                    return true;
                }
            }
        }
        false
    }

    pub fn reject_review(&mut self, agent_id: &str, review_id: &str, reason: &str) -> bool {
        if let Some(list) = self.reviews.get_mut(agent_id) {
            if let Some(r) = list.iter_mut().find(|r| r.review_id == review_id) {
                r.status = ModerationStatus::Rejected { reason: reason.to_string() };
                return true;
            }
        }
        false
    }

    pub fn vote_helpful(&mut self, agent_id: &str, review_id: &str, helpful: bool) {
        if let Some(list) = self.reviews.get_mut(agent_id) {
            if let Some(r) = list.iter_mut().find(|r| r.review_id == review_id) {
                if helpful { r.helpful_votes += 1; } else { r.unhelpful_votes += 1; }
            }
        }
    }

    // ── Queries ───────────────────────────────────────────────────────────

    pub fn summary(&self, agent_id: &str) -> Option<&RatingSummary> {
        self.summaries.get(agent_id)
    }

    /// Returns approved reviews sorted most helpful first
    pub fn visible_reviews(&self, agent_id: &str) -> Vec<&Review> {
        let mut list: Vec<&Review> = self.reviews.get(agent_id)
            .map(|v| v.iter().filter(|r| r.is_visible()).collect())
            .unwrap_or_default();
        list.sort_by(|a, b| b.helpfulness_score().partial_cmp(&a.helpfulness_score()).unwrap_or(std::cmp::Ordering::Equal));
        list
    }

    pub fn total_reviews(&self, agent_id: &str) -> usize {
        self.reviews.get(agent_id).map(|v| v.len()).unwrap_or(0)
    }

    pub fn user_has_rated(&self, user_id: &str, agent_id: &str) -> bool {
        self.user_ratings.get(user_id)
            .map(|m| m.contains_key(agent_id))
            .unwrap_or(false)
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn uuid_str() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ns = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().subsec_nanos();
    format!("r-{:x}", ns)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rating_clamps_to_1_5() {
        let r = Rating::new("agent-1", "user-1", 0, 100);
        assert_eq!(r.stars, 1);
        let r = Rating::new("agent-1", "user-2", 9, 100);
        assert_eq!(r.stars, 5);
    }

    #[test]
    fn summary_avg_computation() {
        let mut s = RatingSummary::new("agent-1");
        s.add_rating(5);
        s.add_rating(5);
        s.add_rating(3);
        // avg = (5+5+3)/3 = 4.333
        assert!((s.avg_rating - 13.0 / 3.0).abs() < 0.01);
        assert_eq!(s.total_ratings, 3);
        assert!((s.star_pct(5) - 66.66).abs() < 0.5);
    }

    #[test]
    fn review_store_submit_rating_deduplication() {
        let mut store = ReviewStore::new();
        let r1 = Rating::new("agent-1", "user-1", 5, 100);
        let r2 = Rating::new("agent-1", "user-1", 3, 200); // duplicate
        assert!(store.submit_rating(r1));
        assert!(!store.submit_rating(r2), "Duplicate rating should be rejected");
        assert_eq!(store.summary("agent-1").unwrap().total_ratings, 1);
    }

    #[test]
    fn update_rating_changes_average() {
        let mut store = ReviewStore::new();
        store.submit_rating(Rating::new("agent-1", "user-1", 2, 100));
        store.submit_rating(Rating::new("agent-1", "user-2", 4, 200));
        // avg = 3.0
        let avg_before = store.summary("agent-1").unwrap().avg_rating;
        assert!((avg_before - 3.0).abs() < 0.01);

        store.update_rating("agent-1", "user-1", 5);
        // avg = (5+4)/2 = 4.5
        let avg_after = store.summary("agent-1").unwrap().avg_rating;
        assert!((avg_after - 4.5).abs() < 0.01);
    }

    #[test]
    fn review_approve_reject() {
        let mut store = ReviewStore::new();
        let rev = Review::new("agent-1", "user-1", "Alice", 4, "Great agent!", 100);
        let review_id = rev.review_id.clone();
        store.submit_review(rev);

        // Pending: not visible
        assert_eq!(store.visible_reviews("agent-1").len(), 0);

        assert!(store.approve_review("agent-1", &review_id));

        // Approved: visible now
        assert_eq!(store.visible_reviews("agent-1").len(), 1);
        assert_eq!(store.summary("agent-1").unwrap().review_count, 1);
    }

    #[test]
    fn review_reject_hides_review() {
        let mut store = ReviewStore::new();
        let rev = Review::new("agent-1", "user-1", "Bob", 1, "spam content", 100);
        let rid = rev.review_id.clone();
        store.submit_review(rev);
        assert!(store.reject_review("agent-1", &rid, "spam"));
        assert_eq!(store.visible_reviews("agent-1").len(), 0);
    }

    #[test]
    fn helpful_voting() {
        let mut store = ReviewStore::new();
        let rev = Review::new("agent-1", "user-1", "Carol", 5, "Excellent!", 100).approve();
        let rid = rev.review_id.clone();
        store.submit_review(rev);
        store.vote_helpful("agent-1", &rid, true);
        store.vote_helpful("agent-1", &rid, true);
        store.vote_helpful("agent-1", &rid, false);
        let reviews = store.visible_reviews("agent-1");
        assert_eq!(reviews[0].helpful_votes, 2);
        assert_eq!(reviews[0].unhelpful_votes, 1);
    }

    #[test]
    fn helpfulness_score() {
        let mut r = Review::new("a", "u", "X", 4, "ok", 0).approve();
        r.helpful_votes = 8; r.unhelpful_votes = 2;
        assert!((r.helpfulness_score() - 0.8).abs() < 0.01);
    }

    #[test]
    fn user_has_rated_check() {
        let mut store = ReviewStore::new();
        store.submit_rating(Rating::new("agent-1", "user-A", 4, 0));
        assert!(store.user_has_rated("user-A", "agent-1"));
        assert!(!store.user_has_rated("user-B", "agent-1"));
    }

    #[test]
    fn moderation_status_visibility() {
        assert!(!ModerationStatus::Pending.is_visible());
        assert!(ModerationStatus::Approved.is_visible());
        assert!(!ModerationStatus::Rejected { reason: "spam".into() }.is_visible());
        assert!(!ModerationStatus::Flagged { flag_count: 3 }.is_visible());
    }

    #[test]
    fn star_pct_correctness() {
        let mut s = RatingSummary::new("x");
        for _ in 0..3 { s.add_rating(5); }
        for _ in 0..1 { s.add_rating(1); }
        assert!((s.star_pct(5) - 75.0).abs() < 0.1);
        assert!((s.star_pct(1) - 25.0).abs() < 0.1);
    }
}
