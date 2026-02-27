//! Sync state — applies operations and maintains an ordered op-log
//! with LWW conflict resolution for concurrent edits.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::annotation::{Annotation, AnnotationStore};
use crate::mention::MentionIndex;
use crate::model::{Comment, CommentStore, CommentThread};
use crate::notification::NotificationStore;
use crate::ops::{CommentOp, LamportClock, OpEnvelope};

// ── Apply Result ─────────────────────────────────────────────────────

/// The result of applying an operation.
#[derive(Debug, Clone, PartialEq)]
pub enum ApplyResult {
    /// Operation applied successfully.
    Applied,
    /// Operation was discarded (stale or duplicate).
    Discarded,
    /// Operation targets a non-existent entity.
    NotFound,
    /// Operation was a no-op (e.g., adding a tag that already exists).
    NoOp,
}

// ── Comment Sync State ───────────────────────────────────────────────

/// Manages the synchronized state of comments, annotations, and notifications.
///
/// Applies operations (local or remote) in causal order using Lamport clocks.
/// Maintains an op-log for delta sync with late-joining peers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentSyncState {
    /// All comment threads.
    pub comments: CommentStore,
    /// All visual annotations.
    pub annotations: AnnotationStore,
    /// Per-user notifications.
    pub notifications: NotificationStore,
    /// Mention index for notification routing.
    pub mention_index: MentionIndex,
    /// Ordered operation log.
    op_log: Vec<OpEnvelope>,
    /// Local Lamport clock.
    clock: LamportClock,
    /// Local user ID.
    pub local_user_id: Uuid,
    /// Local user name.
    pub local_user_name: String,
    /// Last-write-wins timestamps for thread-level fields.
    /// Maps (thread_id, field_name) → clock value.
    lww_timestamps: std::collections::HashMap<(crate::model::ThreadId, String), LamportClock>,
}

impl CommentSyncState {
    pub fn new(user_id: Uuid, user_name: impl Into<String>) -> Self {
        Self {
            comments: CommentStore::new(),
            annotations: AnnotationStore::new(),
            notifications: NotificationStore::new(),
            mention_index: MentionIndex::new(),
            op_log: Vec::new(),
            clock: LamportClock::zero(),
            local_user_id: user_id,
            local_user_name: user_name.into(),
            lww_timestamps: std::collections::HashMap::new(),
        }
    }

    /// Current Lamport clock value.
    pub fn clock(&self) -> LamportClock {
        self.clock
    }

    /// Number of operations in the log.
    pub fn op_count(&self) -> usize {
        self.op_log.len()
    }

    /// Get operations since a given clock value (for delta sync).
    pub fn ops_since(&self, since: LamportClock) -> Vec<&OpEnvelope> {
        self.op_log
            .iter()
            .filter(|e| e.clock > since)
            .collect()
    }

    /// All operations (for full sync to a new peer).
    pub fn full_op_log(&self) -> &[OpEnvelope] {
        &self.op_log
    }

    // ── Local operation generation ───────────────────────────────────

    /// Generate a local operation envelope.
    fn local_op(&mut self, op: CommentOp, timestamp: u64) -> OpEnvelope {
        let clock = self.clock.tick();
        OpEnvelope::new(
            op,
            clock,
            self.local_user_id,
            self.local_user_name.clone(),
            timestamp,
        )
    }

    // ── Apply operations ─────────────────────────────────────────────

    /// Apply a local operation: generate envelope, apply, and return the envelope
    /// for broadcasting to peers.
    pub fn apply_local(&mut self, op: CommentOp, timestamp: u64) -> OpEnvelope {
        let envelope = self.local_op(op, timestamp);
        self.apply_envelope(&envelope);
        self.op_log.push(envelope.clone());
        envelope
    }

    /// Apply a remote operation: merge clock, apply if not stale.
    pub fn apply_remote(&mut self, envelope: &OpEnvelope) -> ApplyResult {
        // Merge clock
        self.clock.merge(envelope.clock);

        // Check for LWW conflicts on thread-level state changes
        if let Some(tid) = envelope.op.thread_id() {
            let field = envelope.op.kind_label().to_string();
            let key = (tid, field);
            if let Some(existing) = self.lww_timestamps.get(&key) {
                if envelope.clock <= *existing {
                    return ApplyResult::Discarded;
                }
            }
            self.lww_timestamps.insert(key, envelope.clock);
        }

        let result = self.apply_envelope(envelope);
        if result == ApplyResult::Applied {
            self.op_log.push(envelope.clone());
        }
        result
    }

    /// Apply an operation envelope to the internal state.
    fn apply_envelope(&mut self, envelope: &OpEnvelope) -> ApplyResult {
        match &envelope.op {
            CommentOp::StartThread {
                thread_id,
                anchor,
                comment_id,
                content,
            } => {
                let comment = Comment::with_id(
                    *comment_id,
                    envelope.author_id,
                    &envelope.author_name,
                    content,
                    envelope.timestamp,
                );
                // Index mentions
                for m in &comment.mentions {
                    if let Some(uid) = m.user_id {
                        self.mention_index.add_mention(uid, *thread_id, *comment_id);
                        self.notifications.notify_mention(
                            uid,
                            envelope.author_id,
                            &envelope.author_name,
                            *thread_id,
                            *comment_id,
                            envelope.timestamp,
                        );
                    }
                }
                let mut thread = CommentThread::new(anchor.clone(), envelope.timestamp);
                thread.id = *thread_id;
                thread.add_comment(comment);
                self.comments.insert_thread(thread);
                ApplyResult::Applied
            }

            CommentOp::DeleteThread { thread_id } => {
                if self.comments.remove_thread(*thread_id).is_some() {
                    ApplyResult::Applied
                } else {
                    ApplyResult::NotFound
                }
            }

            CommentOp::Reply {
                thread_id,
                comment_id,
                content,
            } => {
                if let Some(thread) = self.comments.get_thread_mut(*thread_id) {
                    let comment = Comment::with_id(
                        *comment_id,
                        envelope.author_id,
                        &envelope.author_name,
                        content,
                        envelope.timestamp,
                    );
                    // Notify participants
                    for &participant in &thread.participants {
                        self.notifications.notify_reply(
                            participant,
                            envelope.author_id,
                            &envelope.author_name,
                            *thread_id,
                            *comment_id,
                            envelope.timestamp,
                        );
                    }
                    // Index mentions
                    for m in &comment.mentions {
                        if let Some(uid) = m.user_id {
                            self.mention_index.add_mention(uid, *thread_id, *comment_id);
                            self.notifications.notify_mention(
                                uid,
                                envelope.author_id,
                                &envelope.author_name,
                                *thread_id,
                                *comment_id,
                                envelope.timestamp,
                            );
                        }
                    }
                    thread.add_comment(comment);
                    ApplyResult::Applied
                } else {
                    ApplyResult::NotFound
                }
            }

            CommentOp::EditComment {
                thread_id,
                comment_id,
                new_content,
            } => {
                if let Some(thread) = self.comments.get_thread_mut(*thread_id) {
                    if let Some(comment) = thread.get_comment_mut(*comment_id) {
                        // Remove old mention index entries
                        self.mention_index.remove_comment_mentions(*thread_id, *comment_id);
                        comment.edit(new_content, envelope.timestamp);
                        // Re-index new mentions
                        for m in &comment.mentions {
                            if let Some(uid) = m.user_id {
                                self.mention_index.add_mention(uid, *thread_id, *comment_id);
                            }
                        }
                        ApplyResult::Applied
                    } else {
                        ApplyResult::NotFound
                    }
                } else {
                    ApplyResult::NotFound
                }
            }

            CommentOp::DeleteComment {
                thread_id,
                comment_id,
            } => {
                if let Some(thread) = self.comments.get_thread_mut(*thread_id) {
                    if let Some(comment) = thread.get_comment_mut(*comment_id) {
                        self.mention_index.remove_comment_mentions(*thread_id, *comment_id);
                        comment.delete();
                        ApplyResult::Applied
                    } else {
                        ApplyResult::NotFound
                    }
                } else {
                    ApplyResult::NotFound
                }
            }

            CommentOp::AddReaction {
                thread_id,
                comment_id,
                emoji,
            } => {
                if let Some(thread) = self.comments.get_thread_mut(*thread_id) {
                    if let Some(comment) = thread.get_comment_mut(*comment_id) {
                        let before = comment.reactions.len();
                        comment.add_reaction(
                            emoji,
                            envelope.author_id,
                            &envelope.author_name,
                            envelope.timestamp,
                        );
                        if comment.reactions.len() > before {
                            // Notify comment author
                            if comment.author_id != envelope.author_id {
                                self.notifications.notify(
                                    crate::notification::Notification::new(
                                        comment.author_id,
                                        crate::notification::NotificationKind::Reaction {
                                            reacted_by: envelope.author_id,
                                            reacted_by_name: envelope.author_name.clone(),
                                            emoji: emoji.clone(),
                                        },
                                        *thread_id,
                                        Some(*comment_id),
                                        envelope.timestamp,
                                    ),
                                );
                            }
                            ApplyResult::Applied
                        } else {
                            ApplyResult::NoOp
                        }
                    } else {
                        ApplyResult::NotFound
                    }
                } else {
                    ApplyResult::NotFound
                }
            }

            CommentOp::RemoveReaction {
                thread_id,
                comment_id,
                emoji,
            } => {
                if let Some(thread) = self.comments.get_thread_mut(*thread_id) {
                    if let Some(comment) = thread.get_comment_mut(*comment_id) {
                        comment.remove_reaction(emoji, envelope.author_id);
                        ApplyResult::Applied
                    } else {
                        ApplyResult::NotFound
                    }
                } else {
                    ApplyResult::NotFound
                }
            }

            CommentOp::SetResolution {
                thread_id,
                resolution,
            } => {
                if let Some(thread) = self.comments.get_thread_mut(*thread_id) {
                    match resolution {
                        crate::model::ResolutionState::Open => {
                            thread.reopen(envelope.timestamp);
                        }
                        crate::model::ResolutionState::Resolved => {
                            thread.resolve(envelope.author_id, envelope.timestamp);
                        }
                        crate::model::ResolutionState::WontFix => {
                            thread.wont_fix(envelope.author_id, envelope.timestamp);
                        }
                        crate::model::ResolutionState::Duplicate => {
                            thread.duplicate(envelope.author_id, envelope.timestamp);
                        }
                    }
                    ApplyResult::Applied
                } else {
                    ApplyResult::NotFound
                }
            }

            CommentOp::SetPriority {
                thread_id,
                priority,
            } => {
                if let Some(thread) = self.comments.get_thread_mut(*thread_id) {
                    thread.set_priority(*priority, envelope.timestamp);
                    ApplyResult::Applied
                } else {
                    ApplyResult::NotFound
                }
            }

            CommentOp::AssignThread {
                thread_id,
                assignee_id,
            } => {
                if let Some(thread) = self.comments.get_thread_mut(*thread_id) {
                    thread.assign(*assignee_id, envelope.timestamp);
                    // Notify assignee
                    if *assignee_id != envelope.author_id {
                        self.notifications.notify(
                            crate::notification::Notification::new(
                                *assignee_id,
                                crate::notification::NotificationKind::Assignment {
                                    assigned_by: envelope.author_id,
                                    assigned_by_name: envelope.author_name.clone(),
                                },
                                *thread_id,
                                None,
                                envelope.timestamp,
                            ),
                        );
                    }
                    ApplyResult::Applied
                } else {
                    ApplyResult::NotFound
                }
            }

            CommentOp::UnassignThread { thread_id } => {
                if let Some(thread) = self.comments.get_thread_mut(*thread_id) {
                    thread.unassign(envelope.timestamp);
                    ApplyResult::Applied
                } else {
                    ApplyResult::NotFound
                }
            }

            CommentOp::AddTag { thread_id, tag } => {
                if let Some(thread) = self.comments.get_thread_mut(*thread_id) {
                    thread.add_tag(tag, envelope.timestamp);
                    ApplyResult::Applied
                } else {
                    ApplyResult::NotFound
                }
            }

            CommentOp::RemoveTag { thread_id, tag } => {
                if let Some(thread) = self.comments.get_thread_mut(*thread_id) {
                    thread.remove_tag(tag, envelope.timestamp);
                    ApplyResult::Applied
                } else {
                    ApplyResult::NotFound
                }
            }

            CommentOp::AddAnnotation {
                annotation_id,
                kind,
                style,
                page_id,
                thread_id,
            } => {
                let mut ann = Annotation::new(
                    kind.clone(),
                    envelope.author_id,
                    &envelope.author_name,
                    *page_id,
                    envelope.timestamp,
                );
                ann.id = *annotation_id;
                ann.style = style.clone();
                if let Some(tid) = thread_id {
                    ann.thread_id = Some(*tid);
                }
                self.annotations.add(ann);
                ApplyResult::Applied
            }

            CommentOp::RemoveAnnotation { annotation_id } => {
                if self.annotations.remove(*annotation_id).is_some() {
                    ApplyResult::Applied
                } else {
                    ApplyResult::NotFound
                }
            }

            CommentOp::ToggleAnnotationVisibility { annotation_id } => {
                self.annotations.toggle_visibility(*annotation_id);
                ApplyResult::Applied
            }
        }
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{CommentAnchor, CommentId, ResolutionState, ThreadId};

    fn alice() -> Uuid {
        Uuid::from_bytes([1; 16])
    }
    fn bob() -> Uuid {
        Uuid::from_bytes([2; 16])
    }
    fn layer_id() -> Uuid {
        Uuid::from_bytes([10; 16])
    }

    #[test]
    fn apply_local_start_thread() {
        let mut state = CommentSyncState::new(alice(), "Alice");
        let tid = ThreadId::new();
        let cid = CommentId::new();

        let env = state.apply_local(
            CommentOp::StartThread {
                thread_id: tid,
                anchor: CommentAnchor::layer(layer_id()),
                comment_id: cid,
                content: "Hello world".into(),
            },
            1000,
        );

        assert_eq!(state.comments.thread_count(), 1);
        assert_eq!(state.op_count(), 1);
        assert_eq!(env.clock.value(), 1);
    }

    #[test]
    fn apply_local_reply() {
        let mut state = CommentSyncState::new(alice(), "Alice");
        let tid = ThreadId::new();
        let cid = CommentId::new();

        state.apply_local(
            CommentOp::StartThread {
                thread_id: tid,
                anchor: CommentAnchor::layer(layer_id()),
                comment_id: cid,
                content: "First".into(),
            },
            1000,
        );

        state.apply_local(
            CommentOp::Reply {
                thread_id: tid,
                comment_id: CommentId::new(),
                content: "Reply".into(),
            },
            1001,
        );

        let thread = state.comments.get_thread(tid).unwrap();
        assert_eq!(thread.comment_count(), 2);
    }

    #[test]
    fn apply_remote_op() {
        let mut alice_state = CommentSyncState::new(alice(), "Alice");
        let mut bob_state = CommentSyncState::new(bob(), "Bob");

        let tid = ThreadId::new();
        let cid = CommentId::new();

        // Alice creates thread
        let env = alice_state.apply_local(
            CommentOp::StartThread {
                thread_id: tid,
                anchor: CommentAnchor::layer(layer_id()),
                comment_id: cid,
                content: "From Alice".into(),
            },
            1000,
        );

        // Bob receives it
        let result = bob_state.apply_remote(&env);
        assert_eq!(result, ApplyResult::Applied);
        assert_eq!(bob_state.comments.thread_count(), 1);
    }

    #[test]
    fn remote_reply_generates_notification() {
        let mut state = CommentSyncState::new(alice(), "Alice");
        let tid = ThreadId::new();
        let cid = CommentId::new();

        // Alice creates thread
        state.apply_local(
            CommentOp::StartThread {
                thread_id: tid,
                anchor: CommentAnchor::layer(layer_id()),
                comment_id: cid,
                content: "Thread".into(),
            },
            1000,
        );

        // Bob replies (as remote)
        let bob_reply = OpEnvelope::new(
            CommentOp::Reply {
                thread_id: tid,
                comment_id: CommentId::new(),
                content: "Bob's reply".into(),
            },
            LamportClock::new(2),
            bob(),
            "Bob",
            1001,
        );
        state.apply_remote(&bob_reply);

        // Alice should be notified (she's a participant)
        assert!(state.notifications.unread_count(alice()) > 0);
    }

    #[test]
    fn lww_conflict_resolution() {
        let mut state = CommentSyncState::new(alice(), "Alice");
        let tid = ThreadId::new();
        let cid = CommentId::new();

        state.apply_local(
            CommentOp::StartThread {
                thread_id: tid,
                anchor: CommentAnchor::layer(layer_id()),
                comment_id: cid,
                content: "Thread".into(),
            },
            1000,
        );

        // Remote resolve at clock=5
        let env_resolve = OpEnvelope::new(
            CommentOp::SetResolution {
                thread_id: tid,
                resolution: ResolutionState::Resolved,
            },
            LamportClock::new(5),
            bob(),
            "Bob",
            1001,
        );
        state.apply_remote(&env_resolve);

        // Stale reopen at clock=3 → should be discarded
        let env_reopen = OpEnvelope::new(
            CommentOp::SetResolution {
                thread_id: tid,
                resolution: ResolutionState::Open,
            },
            LamportClock::new(3),
            alice(),
            "Alice",
            1002,
        );
        let result = state.apply_remote(&env_reopen);
        assert_eq!(result, ApplyResult::Discarded);

        // Thread should still be resolved
        let thread = state.comments.get_thread(tid).unwrap();
        assert!(thread.is_resolved());
    }

    #[test]
    fn delta_sync() {
        let mut state = CommentSyncState::new(alice(), "Alice");
        let tid = ThreadId::new();

        state.apply_local(
            CommentOp::StartThread {
                thread_id: tid,
                anchor: CommentAnchor::layer(layer_id()),
                comment_id: CommentId::new(),
                content: "First".into(),
            },
            1000,
        );

        let checkpoint = state.clock();

        state.apply_local(
            CommentOp::Reply {
                thread_id: tid,
                comment_id: CommentId::new(),
                content: "Second".into(),
            },
            1001,
        );

        state.apply_local(
            CommentOp::Reply {
                thread_id: tid,
                comment_id: CommentId::new(),
                content: "Third".into(),
            },
            1002,
        );

        let delta = state.ops_since(checkpoint);
        assert_eq!(delta.len(), 2);
    }

    #[test]
    fn annotation_through_ops() {
        let mut state = CommentSyncState::new(alice(), "Alice");
        let aid = crate::annotation::AnnotationId::new();
        let pid = Uuid::from_bytes([20; 16]);

        state.apply_local(
            CommentOp::AddAnnotation {
                annotation_id: aid,
                kind: crate::annotation::AnnotationKind::Pin { x: 10.0, y: 20.0 },
                style: crate::annotation::AnnotationStyle::default(),
                page_id: pid,
                thread_id: None,
            },
            1000,
        );

        assert_eq!(state.annotations.count(), 1);

        state.apply_local(
            CommentOp::RemoveAnnotation { annotation_id: aid },
            1001,
        );

        assert_eq!(state.annotations.count(), 0);
    }
}
