//! Comment filtering — spatial, temporal, page-based, and author-based.
//!
//! `CommentFilter` is a builder that chains criteria. All criteria
//! are AND-ed: a thread must pass all specified conditions.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::model::{CommentThread, Priority, ResolutionState};

/// A composable filter for comment threads.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommentFilter {
    /// Only threads on this page (Canvas/Region anchors).
    pub page_id: Option<Uuid>,
    /// Only threads targeting this layer/component.
    pub target_id: Option<Uuid>,
    /// Only threads by a specific author.
    pub author_id: Option<Uuid>,
    /// Only threads with this resolution state.
    pub resolution: Option<ResolutionState>,
    /// Only threads with at least this priority.
    pub min_priority: Option<Priority>,
    /// Only threads assigned to this user.
    pub assignee_id: Option<Uuid>,
    /// Only threads with any of these tags.
    pub tags: Option<Vec<String>>,
    /// Only threads created after this timestamp.
    pub created_after: Option<u64>,
    /// Only threads updated after this timestamp.
    pub updated_after: Option<u64>,
    /// Only threads containing this search query (case-insensitive).
    pub search_query: Option<String>,
    /// Spatial filter: (x, y, width, height) — only threads whose anchor
    /// falls within this viewport rectangle.
    pub viewport: Option<(f64, f64, f64, f64)>,
    /// Proximity filter: (x, y, radius) — only threads whose anchor
    /// is within radius of this point.
    pub near_point: Option<(f64, f64, f64)>,
}

impl CommentFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn on_page(mut self, page_id: Uuid) -> Self {
        self.page_id = Some(page_id);
        self
    }

    pub fn on_target(mut self, target_id: Uuid) -> Self {
        self.target_id = Some(target_id);
        self
    }

    pub fn by_author(mut self, author_id: Uuid) -> Self {
        self.author_id = Some(author_id);
        self
    }

    pub fn with_resolution(mut self, state: ResolutionState) -> Self {
        self.resolution = Some(state);
        self
    }

    pub fn open_only(self) -> Self {
        self.with_resolution(ResolutionState::Open)
    }

    pub fn resolved_only(self) -> Self {
        self.with_resolution(ResolutionState::Resolved)
    }

    pub fn min_priority(mut self, priority: Priority) -> Self {
        self.min_priority = Some(priority);
        self
    }

    pub fn assigned_to(mut self, user_id: Uuid) -> Self {
        self.assignee_id = Some(user_id);
        self
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = Some(tags);
        self
    }

    pub fn created_after(mut self, ts: u64) -> Self {
        self.created_after = Some(ts);
        self
    }

    pub fn updated_after(mut self, ts: u64) -> Self {
        self.updated_after = Some(ts);
        self
    }

    pub fn search(mut self, query: impl Into<String>) -> Self {
        self.search_query = Some(query.into());
        self
    }

    pub fn in_viewport(mut self, x: f64, y: f64, w: f64, h: f64) -> Self {
        self.viewport = Some((x, y, w, h));
        self
    }

    pub fn near(mut self, x: f64, y: f64, radius: f64) -> Self {
        self.near_point = Some((x, y, radius));
        self
    }

    /// Test whether a thread matches all filter criteria.
    pub fn matches(&self, thread: &CommentThread) -> bool {
        // Page filter
        if let Some(pid) = self.page_id {
            if thread.anchor.page_id() != Some(pid) {
                // Also check if target_id matches (layer on that page)
                // For non-spatial anchors, page_id is None, so they won't match
                return false;
            }
        }

        // Target filter
        if let Some(tid) = self.target_id {
            if thread.anchor.target_id() != tid {
                return false;
            }
        }

        // Author filter (checks if author is a participant)
        if let Some(aid) = self.author_id {
            if !thread.participants.contains(&aid) {
                return false;
            }
        }

        // Resolution filter
        if let Some(res) = self.resolution {
            if thread.resolution != res {
                return false;
            }
        }

        // Priority filter
        if let Some(min_p) = self.min_priority {
            if thread.priority < min_p {
                return false;
            }
        }

        // Assignment filter
        if let Some(uid) = self.assignee_id {
            if thread.assignee != Some(uid) {
                return false;
            }
        }

        // Tag filter (any of the specified tags)
        if let Some(ref tags) = self.tags {
            if !tags.iter().any(|t| thread.tags.contains(t)) {
                return false;
            }
        }

        // Time filters
        if let Some(ts) = self.created_after {
            if thread.created_at <= ts {
                return false;
            }
        }
        if let Some(ts) = self.updated_after {
            if thread.updated_at <= ts {
                return false;
            }
        }

        // Search query
        if let Some(ref query) = self.search_query {
            let q = query.to_lowercase();
            let found = thread.comments.iter().any(|c| {
                !c.deleted && c.content.to_lowercase().contains(&q)
            });
            if !found {
                return false;
            }
        }

        // Viewport filter
        if let Some((vx, vy, vw, vh)) = self.viewport {
            match &thread.anchor {
                crate::model::CommentAnchor::Canvas { x, y, .. } => {
                    if *x < vx || *x > vx + vw || *y < vy || *y > vy + vh {
                        return false;
                    }
                }
                crate::model::CommentAnchor::Region {
                    x, y, width, height, ..
                } => {
                    // AABB intersection
                    if x + width < vx || *x > vx + vw || y + height < vy || *y > vy + vh {
                        return false;
                    }
                }
                _ => {
                    // Non-spatial anchors don't match viewport filters
                    return false;
                }
            }
        }

        // Proximity filter
        if let Some((px, py, radius)) = self.near_point {
            if !thread.anchor.contains_point(px, py, radius) {
                return false;
            }
        }

        true
    }

    /// Apply this filter to a collection of threads.
    pub fn apply<'a>(&self, threads: impl Iterator<Item = &'a CommentThread>) -> Vec<&'a CommentThread> {
        threads.filter(|t| self.matches(t)).collect()
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{CommentAnchor, CommentThread, Priority};

    fn alice() -> Uuid {
        Uuid::from_bytes([1; 16])
    }
    fn bob() -> Uuid {
        Uuid::from_bytes([2; 16])
    }
    fn page_id() -> Uuid {
        Uuid::from_bytes([20; 16])
    }
    fn layer_id() -> Uuid {
        Uuid::from_bytes([10; 16])
    }

    fn canvas_thread(x: f64, y: f64, ts: u64) -> CommentThread {
        CommentThread::start(
            CommentAnchor::canvas(x, y, page_id()),
            alice(),
            "Alice",
            "Canvas comment",
            ts,
        )
    }

    #[test]
    fn filter_by_page() {
        let t1 = canvas_thread(10.0, 20.0, 1000);
        let t2 = CommentThread::start(
            CommentAnchor::layer(layer_id()),
            alice(),
            "Alice",
            "Layer comment",
            1000,
        );

        let filter = CommentFilter::new().on_page(page_id());
        assert!(filter.matches(&t1));
        assert!(!filter.matches(&t2));
    }

    #[test]
    fn filter_by_resolution() {
        let mut t = canvas_thread(10.0, 20.0, 1000);
        let filter = CommentFilter::new()
            .on_page(page_id())
            .open_only();
        assert!(filter.matches(&t));

        t.resolve(bob(), 1001);
        assert!(!filter.matches(&t));

        let resolved_filter = CommentFilter::new()
            .on_page(page_id())
            .resolved_only();
        assert!(resolved_filter.matches(&t));
    }

    #[test]
    fn filter_by_priority() {
        let mut t = canvas_thread(10.0, 20.0, 1000);
        t.set_priority(Priority::High, 1001);

        let filter = CommentFilter::new()
            .on_page(page_id())
            .min_priority(Priority::High);
        assert!(filter.matches(&t));

        let urgent_filter = CommentFilter::new()
            .on_page(page_id())
            .min_priority(Priority::Urgent);
        assert!(!urgent_filter.matches(&t));
    }

    #[test]
    fn filter_by_viewport() {
        let t1 = canvas_thread(50.0, 50.0, 1000);
        let t2 = canvas_thread(500.0, 500.0, 1001);

        let filter = CommentFilter::new().in_viewport(0.0, 0.0, 200.0, 200.0);
        assert!(filter.matches(&t1));
        assert!(!filter.matches(&t2));
    }

    #[test]
    fn filter_by_proximity() {
        let t = canvas_thread(100.0, 100.0, 1000);

        let near_filter = CommentFilter::new().near(105.0, 105.0, 20.0);
        assert!(near_filter.matches(&t));

        let far_filter = CommentFilter::new().near(500.0, 500.0, 10.0);
        assert!(!far_filter.matches(&t));
    }

    #[test]
    fn filter_by_search() {
        let t = CommentThread::start(
            CommentAnchor::canvas(10.0, 10.0, page_id()),
            alice(),
            "Alice",
            "Fix the padding issue",
            1000,
        );
        let filter = CommentFilter::new()
            .on_page(page_id())
            .search("padding");
        assert!(filter.matches(&t));

        let no_match = CommentFilter::new()
            .on_page(page_id())
            .search("color");
        assert!(!no_match.matches(&t));
    }

    #[test]
    fn filter_by_assignment() {
        let mut t = canvas_thread(10.0, 20.0, 1000);
        t.assign(bob(), 1001);

        let filter = CommentFilter::new()
            .on_page(page_id())
            .assigned_to(bob());
        assert!(filter.matches(&t));

        let alice_filter = CommentFilter::new()
            .on_page(page_id())
            .assigned_to(alice());
        assert!(!alice_filter.matches(&t));
    }

    #[test]
    fn filter_created_after() {
        let t1 = canvas_thread(10.0, 20.0, 1000);
        let t2 = canvas_thread(30.0, 40.0, 2000);

        let filter = CommentFilter::new()
            .on_page(page_id())
            .created_after(1500);
        assert!(!filter.matches(&t1));
        assert!(filter.matches(&t2));
    }

    #[test]
    fn filter_apply_to_collection() {
        let threads = vec![
            canvas_thread(50.0, 50.0, 1000),
            canvas_thread(150.0, 150.0, 2000),
            canvas_thread(500.0, 500.0, 3000),
        ];

        let filter = CommentFilter::new().in_viewport(0.0, 0.0, 200.0, 200.0);
        let result = filter.apply(threads.iter());
        assert_eq!(result.len(), 2);
    }
}
