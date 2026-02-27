//! Top-level Comment Engine — the orchestrator for the entire comment system.
//!
//! `CommentEngine` wraps comments, annotations, mentions, notifications,
//! sync, filtering, and permissions into a single, high-level API that
//! UI layers can call without dealing with internal details.

use uuid::Uuid;

use crate::annotation::{Annotation, AnnotationId, AnnotationKind, AnnotationStyle};
use crate::filter::CommentFilter;
use crate::model::{
    CommentAnchor, CommentId, CommentThread, Priority, ResolutionState, ThreadId,
};
use crate::notification::NotificationStore;
use crate::ops::{CommentOp, LamportClock, OpEnvelope};
use crate::permission::{CommentPermission, PermissionChecker, UserRole};
use crate::state::{ApplyResult, CommentSyncState};

// ── Engine Stats ─────────────────────────────────────────────────────

/// Summary statistics for the comment system.
#[derive(Debug, Clone, Default)]
pub struct CommentStats {
    pub thread_count: usize,
    pub open_threads: usize,
    pub resolved_threads: usize,
    pub total_comments: usize,
    pub annotation_count: usize,
    pub op_count: usize,
}

// ── Comment Engine ───────────────────────────────────────────────────

/// The top-level orchestrator for comments, annotations, and notifications.
///
/// Combines sync state + permissions + filtering into one API.
pub struct CommentEngine {
    state: CommentSyncState,
    permissions: PermissionChecker,
    /// User directory for resolving @mentions.
    user_directory: std::collections::HashMap<String, Uuid>,
}

impl CommentEngine {
    pub fn new(user_id: Uuid, user_name: impl Into<String>) -> Self {
        Self {
            state: CommentSyncState::new(user_id, user_name),
            permissions: PermissionChecker::new(),
            user_directory: std::collections::HashMap::new(),
        }
    }

    // ── User management ──────────────────────────────────────────────

    /// Register a user in the directory (for @mention resolution).
    pub fn register_user(&mut self, username: impl Into<String>, user_id: Uuid, role: UserRole) {
        let username = username.into();
        self.user_directory.insert(username, user_id);
        self.permissions.set_role(user_id, role);
    }

    /// Set a user's role.
    pub fn set_role(&mut self, user_id: Uuid, role: UserRole) {
        self.permissions.set_role(user_id, role);
    }

    // ── Thread operations ────────────────────────────────────────────

    /// Start a new comment thread. Returns (thread_id, op_envelope) for broadcast.
    pub fn start_thread(
        &mut self,
        anchor: CommentAnchor,
        content: impl Into<String>,
        timestamp: u64,
    ) -> Option<(ThreadId, OpEnvelope)> {
        let tid = ThreadId::new();
        let cid = CommentId::new();
        let op = CommentOp::StartThread {
            thread_id: tid,
            anchor,
            comment_id: cid,
            content: content.into(),
        };

        let perm = self.permissions.check(self.state.local_user_id, &op);
        if perm == CommentPermission::Denied {
            return None;
        }

        let env = self.state.apply_local(op, timestamp);
        Some((tid, env))
    }

    /// Reply to a thread.
    pub fn reply(
        &mut self,
        thread_id: ThreadId,
        content: impl Into<String>,
        timestamp: u64,
    ) -> Option<(CommentId, OpEnvelope)> {
        let cid = CommentId::new();
        let op = CommentOp::Reply {
            thread_id,
            comment_id: cid,
            content: content.into(),
        };

        let perm = self.permissions.check(self.state.local_user_id, &op);
        if perm == CommentPermission::Denied {
            return None;
        }

        let env = self.state.apply_local(op, timestamp);
        Some((cid, env))
    }

    /// Edit a comment.
    pub fn edit_comment(
        &mut self,
        thread_id: ThreadId,
        comment_id: CommentId,
        new_content: impl Into<String>,
        timestamp: u64,
    ) -> Option<OpEnvelope> {
        let op = CommentOp::EditComment {
            thread_id,
            comment_id,
            new_content: new_content.into(),
        };

        // Check OwnOnly permission
        if let Some(thread) = self.state.comments.get_thread(thread_id) {
            if let Some(comment) = thread.get_comment(comment_id) {
                if !self.permissions.can_edit_comment(
                    self.state.local_user_id,
                    comment,
                    &op,
                ) {
                    return None;
                }
            }
        }

        Some(self.state.apply_local(op, timestamp))
    }

    /// Delete a comment.
    pub fn delete_comment(
        &mut self,
        thread_id: ThreadId,
        comment_id: CommentId,
        timestamp: u64,
    ) -> Option<OpEnvelope> {
        let op = CommentOp::DeleteComment {
            thread_id,
            comment_id,
        };
        let perm = self.permissions.check(self.state.local_user_id, &op);
        if perm == CommentPermission::Denied {
            return None;
        }
        Some(self.state.apply_local(op, timestamp))
    }

    /// Resolve a thread.
    pub fn resolve_thread(
        &mut self,
        thread_id: ThreadId,
        timestamp: u64,
    ) -> Option<OpEnvelope> {
        let op = CommentOp::SetResolution {
            thread_id,
            resolution: ResolutionState::Resolved,
        };
        Some(self.state.apply_local(op, timestamp))
    }

    /// Reopen a thread.
    pub fn reopen_thread(
        &mut self,
        thread_id: ThreadId,
        timestamp: u64,
    ) -> Option<OpEnvelope> {
        let op = CommentOp::SetResolution {
            thread_id,
            resolution: ResolutionState::Open,
        };
        Some(self.state.apply_local(op, timestamp))
    }

    /// Add a reaction.
    pub fn add_reaction(
        &mut self,
        thread_id: ThreadId,
        comment_id: CommentId,
        emoji: impl Into<String>,
        timestamp: u64,
    ) -> Option<OpEnvelope> {
        let op = CommentOp::AddReaction {
            thread_id,
            comment_id,
            emoji: emoji.into(),
        };
        let perm = self.permissions.check(self.state.local_user_id, &op);
        if perm == CommentPermission::Denied {
            return None;
        }
        Some(self.state.apply_local(op, timestamp))
    }

    /// Set thread priority.
    pub fn set_priority(
        &mut self,
        thread_id: ThreadId,
        priority: Priority,
        timestamp: u64,
    ) -> Option<OpEnvelope> {
        let op = CommentOp::SetPriority {
            thread_id,
            priority,
        };
        let perm = self.permissions.check(self.state.local_user_id, &op);
        if perm == CommentPermission::Denied {
            return None;
        }
        Some(self.state.apply_local(op, timestamp))
    }

    /// Assign a thread.
    pub fn assign_thread(
        &mut self,
        thread_id: ThreadId,
        assignee_id: Uuid,
        timestamp: u64,
    ) -> Option<OpEnvelope> {
        let op = CommentOp::AssignThread {
            thread_id,
            assignee_id,
        };
        let perm = self.permissions.check(self.state.local_user_id, &op);
        if perm == CommentPermission::Denied {
            return None;
        }
        Some(self.state.apply_local(op, timestamp))
    }

    // ── Annotation operations ────────────────────────────────────────

    /// Add an annotation.
    pub fn add_annotation(
        &mut self,
        kind: AnnotationKind,
        style: AnnotationStyle,
        page_id: Uuid,
        thread_id: Option<ThreadId>,
        timestamp: u64,
    ) -> Option<(AnnotationId, OpEnvelope)> {
        let aid = AnnotationId::new();
        let op = CommentOp::AddAnnotation {
            annotation_id: aid,
            kind,
            style,
            page_id,
            thread_id,
        };
        let perm = self.permissions.check(self.state.local_user_id, &op);
        if perm == CommentPermission::Denied {
            return None;
        }
        let env = self.state.apply_local(op, timestamp);
        Some((aid, env))
    }

    // ── Remote operations ────────────────────────────────────────────

    /// Apply a remote operation from a peer.
    pub fn apply_remote(&mut self, envelope: &OpEnvelope) -> ApplyResult {
        self.state.apply_remote(envelope)
    }

    /// Apply a batch of remote operations.
    pub fn apply_remote_batch(&mut self, envelopes: &[OpEnvelope]) -> Vec<ApplyResult> {
        envelopes.iter().map(|e| self.apply_remote(e)).collect()
    }

    // ── Query operations ─────────────────────────────────────────────

    /// Get a thread by ID.
    pub fn get_thread(&self, id: ThreadId) -> Option<&CommentThread> {
        self.state.comments.get_thread(id)
    }

    /// Get all threads matching a filter.
    pub fn filter_threads(&self, filter: &CommentFilter) -> Vec<&CommentThread> {
        filter.apply(self.state.comments.iter())
    }

    /// Get annotations on a page.
    pub fn annotations_on_page(&self, page_id: Uuid) -> Vec<&Annotation> {
        self.state.annotations.on_page(page_id)
    }

    /// Get annotations in a viewport.
    pub fn annotations_in_viewport(
        &self,
        page_id: Uuid,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    ) -> Vec<&Annotation> {
        self.state.annotations.in_viewport(page_id, x, y, w, h)
    }

    /// Get notification store.
    pub fn notifications(&self) -> &NotificationStore {
        &self.state.notifications
    }

    /// Get mutable notification store (for marking read).
    pub fn notifications_mut(&mut self) -> &mut NotificationStore {
        &mut self.state.notifications
    }

    /// Search comments by text.
    pub fn search(&self, query: &str) -> Vec<(ThreadId, CommentId)> {
        self.state.comments.search(query)
    }

    // ── Sync operations ──────────────────────────────────────────────

    /// Get operations since a given clock (for delta sync).
    pub fn ops_since(&self, since: LamportClock) -> Vec<&OpEnvelope> {
        self.state.ops_since(since)
    }

    /// Get all operations (for full sync to a new peer).
    pub fn full_op_log(&self) -> &[OpEnvelope] {
        self.state.full_op_log()
    }

    /// Current clock value.
    pub fn clock(&self) -> LamportClock {
        self.state.clock()
    }

    // ── Statistics ────────────────────────────────────────────────────

    pub fn stats(&self) -> CommentStats {
        CommentStats {
            thread_count: self.state.comments.thread_count(),
            open_threads: self.state.comments.open_threads().len(),
            resolved_threads: self.state.comments.resolved_threads().len(),
            total_comments: self.state.comments.total_comments(),
            annotation_count: self.state.annotations.count(),
            op_count: self.state.op_count(),
        }
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotation::AnnotationKind;
    use crate::model::{CommentAnchor, Priority};

    fn alice() -> Uuid {
        Uuid::from_bytes([1; 16])
    }
    fn bob() -> Uuid {
        Uuid::from_bytes([2; 16])
    }
    fn carol() -> Uuid {
        Uuid::from_bytes([3; 16])
    }
    fn layer_id() -> Uuid {
        Uuid::from_bytes([10; 16])
    }
    fn page_id() -> Uuid {
        Uuid::from_bytes([20; 16])
    }

    fn alice_engine() -> CommentEngine {
        let mut engine = CommentEngine::new(alice(), "Alice");
        engine.register_user("alice", alice(), UserRole::Editor);
        engine.register_user("bob", bob(), UserRole::Commenter);
        engine.register_user("carol", carol(), UserRole::Viewer);
        engine
    }

    #[test]
    fn e2e_create_thread_and_reply() {
        let mut engine = alice_engine();

        let (tid, _env) = engine
            .start_thread(CommentAnchor::layer(layer_id()), "Bug in padding", 1000)
            .unwrap();

        let (cid, _env) = engine
            .reply(tid, "I'll fix this", 1001)
            .unwrap();

        let thread = engine.get_thread(tid).unwrap();
        assert_eq!(thread.comment_count(), 2);
        assert!(thread.get_comment(cid).is_some());
    }

    #[test]
    fn e2e_viewer_cannot_comment() {
        let mut engine = CommentEngine::new(carol(), "Carol");
        engine.register_user("carol", carol(), UserRole::Viewer);

        let result = engine.start_thread(
            CommentAnchor::layer(layer_id()),
            "Viewer comment",
            1000,
        );
        assert!(result.is_none());
    }

    #[test]
    fn e2e_resolve_and_reopen() {
        let mut engine = alice_engine();

        let (tid, _) = engine
            .start_thread(CommentAnchor::layer(layer_id()), "Fix needed", 1000)
            .unwrap();

        engine.resolve_thread(tid, 1001);
        assert!(engine.get_thread(tid).unwrap().is_resolved());

        engine.reopen_thread(tid, 1002);
        assert!(!engine.get_thread(tid).unwrap().is_resolved());
    }

    #[test]
    fn e2e_reactions() {
        let mut engine = alice_engine();

        let (tid, _) = engine
            .start_thread(CommentAnchor::layer(layer_id()), "Great work!", 1000)
            .unwrap();

        let cid = engine.get_thread(tid).unwrap().comments[0].id;

        engine.add_reaction(tid, cid, "👍", 1001);

        let comment = engine.get_thread(tid).unwrap().get_comment(cid).unwrap();
        assert_eq!(comment.reactions.len(), 1);
    }

    #[test]
    fn e2e_annotations_with_comment() {
        let mut engine = alice_engine();

        let (tid, _) = engine
            .start_thread(
                CommentAnchor::canvas(100.0, 200.0, page_id()),
                "This area needs work",
                1000,
            )
            .unwrap();

        let (aid, _env) = engine
            .add_annotation(
                AnnotationKind::Area {
                    x: 80.0,
                    y: 180.0,
                    width: 200.0,
                    height: 100.0,
                },
                AnnotationStyle::default(),
                page_id(),
                Some(tid),
                1001,
            )
            .unwrap();

        let annotations = engine.annotations_on_page(page_id());
        assert_eq!(annotations.len(), 1);
        assert_eq!(annotations[0].id, aid);
    }

    #[test]
    fn e2e_two_peer_sync() {
        let mut alice_eng = alice_engine();
        let mut bob_eng = CommentEngine::new(bob(), "Bob");
        bob_eng.register_user("bob", bob(), UserRole::Editor);
        bob_eng.register_user("alice", alice(), UserRole::Editor);

        // Alice creates thread
        let (tid, env) = alice_eng
            .start_thread(CommentAnchor::layer(layer_id()), "From Alice", 1000)
            .unwrap();
        bob_eng.apply_remote(&env);

        // Bob replies
        let (_, reply_env) = bob_eng
            .reply(tid, "From Bob", 1001)
            .unwrap();
        alice_eng.apply_remote(&reply_env);

        // Both have 2 comments
        assert_eq!(
            alice_eng.get_thread(tid).unwrap().comment_count(),
            2
        );
        assert_eq!(
            bob_eng.get_thread(tid).unwrap().comment_count(),
            2
        );
    }

    #[test]
    fn e2e_filter_threads() {
        let mut engine = alice_engine();

        // Create threads on page
        engine.start_thread(
            CommentAnchor::canvas(50.0, 50.0, page_id()),
            "Thread A",
            1000,
        );
        engine.start_thread(
            CommentAnchor::canvas(150.0, 150.0, page_id()),
            "Thread B",
            1001,
        );
        engine.start_thread(
            CommentAnchor::layer(layer_id()),
            "Layer thread",
            1002,
        );

        let page_filter = CommentFilter::new().on_page(page_id());
        let results = engine.filter_threads(&page_filter);
        assert_eq!(results.len(), 2);

        let viewport_filter = CommentFilter::new().in_viewport(0.0, 0.0, 100.0, 100.0);
        let viewport_results = engine.filter_threads(&viewport_filter);
        assert_eq!(viewport_results.len(), 1);
    }

    #[test]
    fn e2e_priority_and_assignment() {
        let mut engine = alice_engine();

        let (tid, _) = engine
            .start_thread(CommentAnchor::layer(layer_id()), "Important", 1000)
            .unwrap();

        engine.set_priority(tid, Priority::Urgent, 1001);
        engine.assign_thread(tid, bob(), 1001);

        let thread = engine.get_thread(tid).unwrap();
        assert_eq!(thread.priority, Priority::Urgent);
        assert_eq!(thread.assignee, Some(bob()));
    }

    #[test]
    fn e2e_stats() {
        let mut engine = alice_engine();

        engine.start_thread(
            CommentAnchor::layer(layer_id()),
            "Thread 1",
            1000,
        );
        let (tid2, _) = engine
            .start_thread(CommentAnchor::layer(layer_id()), "Thread 2", 1001)
            .unwrap();
        engine.resolve_thread(tid2, 1002);

        let stats = engine.stats();
        assert_eq!(stats.thread_count, 2);
        assert_eq!(stats.open_threads, 1);
        assert_eq!(stats.resolved_threads, 1);
        assert_eq!(stats.total_comments, 2);
    }

    #[test]
    fn e2e_delta_sync_for_late_joiner() {
        let mut engine = alice_engine();

        engine.start_thread(
            CommentAnchor::layer(layer_id()),
            "First",
            1000,
        );
        engine.start_thread(
            CommentAnchor::layer(layer_id()),
            "Second",
            1001,
        );
        engine.start_thread(
            CommentAnchor::layer(layer_id()),
            "Third",
            1002,
        );

        // Late joiner gets full log
        let log = engine.full_op_log();
        assert_eq!(log.len(), 3);

        // Or delta from a checkpoint
        let checkpoint = LamportClock::new(1);
        let delta = engine.ops_since(checkpoint);
        assert_eq!(delta.len(), 2); // ops with clock > 1
    }

    #[test]
    fn e2e_search() {
        let mut engine = alice_engine();

        engine.start_thread(
            CommentAnchor::layer(layer_id()),
            "Fix the padding issue",
            1000,
        );
        engine.start_thread(
            CommentAnchor::layer(layer_id()),
            "Color looks great",
            1001,
        );

        let results = engine.search("padding");
        assert_eq!(results.len(), 1);
    }
}
