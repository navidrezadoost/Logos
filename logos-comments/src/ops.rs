//! Operation types for comment sync protocol.
//!
//! Every mutation to comments/annotations is represented as a `CommentOp`
//! wrapped in an `OpEnvelope` with a Lamport clock for causal ordering.
//! This enables operation-based sync across peers.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::annotation::{AnnotationKind, AnnotationStyle};
use crate::model::{CommentAnchor, CommentId, Priority, ResolutionState, ThreadId};

// ── Lamport Clock ────────────────────────────────────────────────────

/// Lamport logical clock for causal ordering of operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct LamportClock(pub u64);

impl LamportClock {
    pub fn new(value: u64) -> Self {
        Self(value)
    }

    pub fn zero() -> Self {
        Self(0)
    }

    /// Increment and return the new value.
    pub fn tick(&mut self) -> Self {
        self.0 += 1;
        *self
    }

    /// Merge with a remote clock (max + 1).
    pub fn merge(&mut self, remote: Self) -> Self {
        self.0 = self.0.max(remote.0) + 1;
        *self
    }

    pub fn value(&self) -> u64 {
        self.0
    }
}

impl Default for LamportClock {
    fn default() -> Self {
        Self::zero()
    }
}

// ── Operation Envelope ───────────────────────────────────────────────

/// Wrapper that gives every operation a globally-ordered identity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpEnvelope {
    /// The operation.
    pub op: CommentOp,
    /// Lamport clock value at the time of this operation.
    pub clock: LamportClock,
    /// The peer who generated this operation.
    pub author_id: Uuid,
    /// Author display name (for notification text).
    pub author_name: String,
    /// Timestamp (epoch millis) — wall-clock hint, not used for ordering.
    pub timestamp: u64,
}

impl OpEnvelope {
    pub fn new(
        op: CommentOp,
        clock: LamportClock,
        author_id: Uuid,
        author_name: impl Into<String>,
        timestamp: u64,
    ) -> Self {
        Self {
            op,
            clock,
            author_id,
            author_name: author_name.into(),
            timestamp,
        }
    }
}

// ── Comment Operations ───────────────────────────────────────────────

/// All possible comment/annotation operations for sync.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CommentOp {
    // ── Thread operations ──
    /// Start a new comment thread.
    StartThread {
        thread_id: ThreadId,
        anchor: CommentAnchor,
        comment_id: CommentId,
        content: String,
    },
    /// Delete (remove) an entire thread.
    DeleteThread {
        thread_id: ThreadId,
    },

    // ── Comment operations ──
    /// Reply to a thread.
    Reply {
        thread_id: ThreadId,
        comment_id: CommentId,
        content: String,
    },
    /// Edit a comment's content.
    EditComment {
        thread_id: ThreadId,
        comment_id: CommentId,
        new_content: String,
    },
    /// Soft-delete a comment.
    DeleteComment {
        thread_id: ThreadId,
        comment_id: CommentId,
    },

    // ── Reaction operations ──
    /// Add a reaction to a comment.
    AddReaction {
        thread_id: ThreadId,
        comment_id: CommentId,
        emoji: String,
    },
    /// Remove a reaction from a comment.
    RemoveReaction {
        thread_id: ThreadId,
        comment_id: CommentId,
        emoji: String,
    },

    // ── Thread state operations ──
    /// Change thread resolution state.
    SetResolution {
        thread_id: ThreadId,
        resolution: ResolutionState,
    },
    /// Set thread priority.
    SetPriority {
        thread_id: ThreadId,
        priority: Priority,
    },
    /// Assign thread to a user.
    AssignThread {
        thread_id: ThreadId,
        assignee_id: Uuid,
    },
    /// Unassign thread.
    UnassignThread {
        thread_id: ThreadId,
    },
    /// Add a tag to a thread.
    AddTag {
        thread_id: ThreadId,
        tag: String,
    },
    /// Remove a tag from a thread.
    RemoveTag {
        thread_id: ThreadId,
        tag: String,
    },

    // ── Annotation operations ──
    /// Add a visual annotation.
    AddAnnotation {
        annotation_id: crate::annotation::AnnotationId,
        kind: AnnotationKind,
        style: AnnotationStyle,
        page_id: Uuid,
        thread_id: Option<ThreadId>,
    },
    /// Remove an annotation.
    RemoveAnnotation {
        annotation_id: crate::annotation::AnnotationId,
    },
    /// Toggle annotation visibility.
    ToggleAnnotationVisibility {
        annotation_id: crate::annotation::AnnotationId,
    },
}

impl CommentOp {
    /// Get the thread_id affected by this operation, if any.
    pub fn thread_id(&self) -> Option<ThreadId> {
        match self {
            Self::StartThread { thread_id, .. }
            | Self::DeleteThread { thread_id }
            | Self::Reply { thread_id, .. }
            | Self::EditComment { thread_id, .. }
            | Self::DeleteComment { thread_id, .. }
            | Self::AddReaction { thread_id, .. }
            | Self::RemoveReaction { thread_id, .. }
            | Self::SetResolution { thread_id, .. }
            | Self::SetPriority { thread_id, .. }
            | Self::AssignThread { thread_id, .. }
            | Self::UnassignThread { thread_id }
            | Self::AddTag { thread_id, .. }
            | Self::RemoveTag { thread_id, .. } => Some(*thread_id),
            Self::AddAnnotation { thread_id, .. } => *thread_id,
            Self::RemoveAnnotation { .. } | Self::ToggleAnnotationVisibility { .. } => None,
        }
    }

    /// Classify this operation for conflict detection.
    pub fn kind_label(&self) -> &'static str {
        match self {
            Self::StartThread { .. } => "start_thread",
            Self::DeleteThread { .. } => "delete_thread",
            Self::Reply { .. } => "reply",
            Self::EditComment { .. } => "edit_comment",
            Self::DeleteComment { .. } => "delete_comment",
            Self::AddReaction { .. } => "add_reaction",
            Self::RemoveReaction { .. } => "remove_reaction",
            Self::SetResolution { .. } => "set_resolution",
            Self::SetPriority { .. } => "set_priority",
            Self::AssignThread { .. } => "assign_thread",
            Self::UnassignThread { .. } => "unassign_thread",
            Self::AddTag { .. } => "add_tag",
            Self::RemoveTag { .. } => "remove_tag",
            Self::AddAnnotation { .. } => "add_annotation",
            Self::RemoveAnnotation { .. } => "remove_annotation",
            Self::ToggleAnnotationVisibility { .. } => "toggle_annotation",
        }
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lamport_clock_tick() {
        let mut clock = LamportClock::zero();
        assert_eq!(clock.value(), 0);
        let t1 = clock.tick();
        assert_eq!(t1.value(), 1);
        let t2 = clock.tick();
        assert_eq!(t2.value(), 2);
    }

    #[test]
    fn lamport_clock_merge() {
        let mut local = LamportClock::new(5);
        let remote = LamportClock::new(10);
        let merged = local.merge(remote);
        assert_eq!(merged.value(), 11); // max(5,10)+1
    }

    #[test]
    fn lamport_clock_ordering() {
        let a = LamportClock::new(5);
        let b = LamportClock::new(10);
        assert!(a < b);
    }

    #[test]
    fn op_envelope_creation() {
        let op = CommentOp::Reply {
            thread_id: ThreadId::new(),
            comment_id: CommentId::new(),
            content: "Hello".into(),
        };
        let env = OpEnvelope::new(
            op.clone(),
            LamportClock::new(1),
            Uuid::new_v4(),
            "Alice",
            1000,
        );
        assert_eq!(env.clock.value(), 1);
        assert_eq!(env.author_name, "Alice");
    }

    #[test]
    fn op_thread_id_extraction() {
        let tid = ThreadId::new();
        let op = CommentOp::Reply {
            thread_id: tid,
            comment_id: CommentId::new(),
            content: "test".into(),
        };
        assert_eq!(op.thread_id(), Some(tid));

        let op2 = CommentOp::RemoveAnnotation {
            annotation_id: crate::annotation::AnnotationId::new(),
        };
        assert_eq!(op2.thread_id(), None);
    }

    #[test]
    fn op_kind_labels() {
        let tid = ThreadId::new();
        let ops = vec![
            CommentOp::StartThread {
                thread_id: tid,
                anchor: CommentAnchor::layer(Uuid::new_v4()),
                comment_id: CommentId::new(),
                content: "test".into(),
            },
            CommentOp::Reply {
                thread_id: tid,
                comment_id: CommentId::new(),
                content: "reply".into(),
            },
            CommentOp::SetResolution {
                thread_id: tid,
                resolution: ResolutionState::Resolved,
            },
        ];
        for op in &ops {
            assert!(!op.kind_label().is_empty());
        }
    }

    #[test]
    fn op_serde_roundtrip() {
        let op = CommentOp::EditComment {
            thread_id: ThreadId::new(),
            comment_id: CommentId::new(),
            new_content: "edited text".into(),
        };
        let env = OpEnvelope::new(op, LamportClock::new(42), Uuid::new_v4(), "Bob", 2000);

        let json = serde_json::to_string(&env).unwrap();
        let back: OpEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(back.clock.value(), 42);
        assert_eq!(back.author_name, "Bob");
    }
}
