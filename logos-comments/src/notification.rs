//! Notification system for comment events.
//!
//! Generates and stores per-user notifications for:
//! - @mentions
//! - Thread replies
//! - Thread resolution changes
//! - Reactions to the user's comments
//! - Thread assignments

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::model::{CommentId, ThreadId};

// ── Notification ID ──────────────────────────────────────────────────

/// Unique identifier for a notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NotificationId(pub Uuid);

impl NotificationId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for NotificationId {
    fn default() -> Self {
        Self::new()
    }
}

// ── Notification Kind ────────────────────────────────────────────────

/// The type of event that triggered the notification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NotificationKind {
    /// User was @mentioned in a comment.
    Mention {
        mentioned_by: Uuid,
        mentioned_by_name: String,
    },
    /// Someone replied to a thread the user participates in.
    Reply {
        reply_by: Uuid,
        reply_by_name: String,
    },
    /// A thread the user participates in was resolved.
    ThreadResolved {
        resolved_by: Uuid,
        resolved_by_name: String,
    },
    /// A thread the user participates in was reopened.
    ThreadReopened {
        reopened_by: Uuid,
        reopened_by_name: String,
    },
    /// Someone reacted to the user's comment.
    Reaction {
        reacted_by: Uuid,
        reacted_by_name: String,
        emoji: String,
    },
    /// A thread was assigned to the user.
    Assignment {
        assigned_by: Uuid,
        assigned_by_name: String,
    },
    /// A thread the user is assigned to was unassigned.
    Unassignment {
        unassigned_by: Uuid,
        unassigned_by_name: String,
    },
}

impl NotificationKind {
    /// Human-readable label for this notification type.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Mention { .. } => "mentioned you",
            Self::Reply { .. } => "replied",
            Self::ThreadResolved { .. } => "resolved thread",
            Self::ThreadReopened { .. } => "reopened thread",
            Self::Reaction { .. } => "reacted",
            Self::Assignment { .. } => "assigned to you",
            Self::Unassignment { .. } => "unassigned",
        }
    }

    /// The user who triggered this notification.
    pub fn actor_id(&self) -> Uuid {
        match self {
            Self::Mention { mentioned_by, .. } => *mentioned_by,
            Self::Reply { reply_by, .. } => *reply_by,
            Self::ThreadResolved { resolved_by, .. } => *resolved_by,
            Self::ThreadReopened { reopened_by, .. } => *reopened_by,
            Self::Reaction { reacted_by, .. } => *reacted_by,
            Self::Assignment { assigned_by, .. } => *assigned_by,
            Self::Unassignment { unassigned_by, .. } => *unassigned_by,
        }
    }

    pub fn actor_name(&self) -> &str {
        match self {
            Self::Mention { mentioned_by_name, .. } => mentioned_by_name,
            Self::Reply { reply_by_name, .. } => reply_by_name,
            Self::ThreadResolved { resolved_by_name, .. } => resolved_by_name,
            Self::ThreadReopened { reopened_by_name, .. } => reopened_by_name,
            Self::Reaction { reacted_by_name, .. } => reacted_by_name,
            Self::Assignment { assigned_by_name, .. } => assigned_by_name,
            Self::Unassignment { unassigned_by_name, .. } => unassigned_by_name,
        }
    }
}

// ── Notification ─────────────────────────────────────────────────────

/// A notification for a user.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Notification {
    pub id: NotificationId,
    /// The user who should see this notification.
    pub recipient_id: Uuid,
    /// What happened.
    pub kind: NotificationKind,
    /// Related thread.
    pub thread_id: ThreadId,
    /// Related comment (if applicable).
    pub comment_id: Option<CommentId>,
    /// When this notification was created.
    pub timestamp: u64,
    /// Whether the user has read this notification.
    pub read: bool,
}

impl Notification {
    pub fn new(
        recipient_id: Uuid,
        kind: NotificationKind,
        thread_id: ThreadId,
        comment_id: Option<CommentId>,
        timestamp: u64,
    ) -> Self {
        Self {
            id: NotificationId::new(),
            recipient_id,
            kind,
            thread_id,
            comment_id,
            timestamp,
            read: false,
        }
    }

    pub fn mark_read(&mut self) {
        self.read = true;
    }

    pub fn is_unread(&self) -> bool {
        !self.read
    }

    /// Summary text for display.
    pub fn summary(&self) -> String {
        format!("{} {}", self.kind.actor_name(), self.kind.label())
    }
}

// ── Notification Store ───────────────────────────────────────────────

/// Per-user notification store with read/unread tracking.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NotificationStore {
    /// user_id → notifications (newest first).
    notifications: HashMap<Uuid, Vec<Notification>>,
}

impl NotificationStore {
    pub fn new() -> Self {
        Self {
            notifications: HashMap::new(),
        }
    }

    /// Add a notification for a user.
    pub fn notify(&mut self, notification: Notification) {
        let uid = notification.recipient_id;
        self.notifications.entry(uid).or_default().push(notification);
    }

    /// Create and store a mention notification.
    pub fn notify_mention(
        &mut self,
        recipient_id: Uuid,
        mentioned_by: Uuid,
        mentioned_by_name: impl Into<String>,
        thread_id: ThreadId,
        comment_id: CommentId,
        timestamp: u64,
    ) {
        let n = Notification::new(
            recipient_id,
            NotificationKind::Mention {
                mentioned_by,
                mentioned_by_name: mentioned_by_name.into(),
            },
            thread_id,
            Some(comment_id),
            timestamp,
        );
        self.notify(n);
    }

    /// Create and store a reply notification.
    pub fn notify_reply(
        &mut self,
        recipient_id: Uuid,
        reply_by: Uuid,
        reply_by_name: impl Into<String>,
        thread_id: ThreadId,
        comment_id: CommentId,
        timestamp: u64,
    ) {
        // Don't notify yourself
        if recipient_id == reply_by {
            return;
        }
        let n = Notification::new(
            recipient_id,
            NotificationKind::Reply {
                reply_by,
                reply_by_name: reply_by_name.into(),
            },
            thread_id,
            Some(comment_id),
            timestamp,
        );
        self.notify(n);
    }

    /// Get all notifications for a user.
    pub fn for_user(&self, user_id: Uuid) -> &[Notification] {
        self.notifications
            .get(&user_id)
            .map_or(&[], |v| v.as_slice())
    }

    /// Get unread notifications for a user.
    pub fn unread_for_user(&self, user_id: Uuid) -> Vec<&Notification> {
        self.for_user(user_id)
            .iter()
            .filter(|n| n.is_unread())
            .collect()
    }

    /// Count unread notifications for a user.
    pub fn unread_count(&self, user_id: Uuid) -> usize {
        self.for_user(user_id)
            .iter()
            .filter(|n| n.is_unread())
            .count()
    }

    /// Mark all notifications as read for a user.
    pub fn mark_all_read(&mut self, user_id: Uuid) {
        if let Some(notifs) = self.notifications.get_mut(&user_id) {
            for n in notifs.iter_mut() {
                n.read = true;
            }
        }
    }

    /// Mark a specific notification as read.
    pub fn mark_read(&mut self, user_id: Uuid, notification_id: NotificationId) {
        if let Some(notifs) = self.notifications.get_mut(&user_id) {
            if let Some(n) = notifs.iter_mut().find(|n| n.id == notification_id) {
                n.read = true;
            }
        }
    }

    /// Mark all notifications for a specific thread as read.
    pub fn mark_thread_read(&mut self, user_id: Uuid, thread_id: ThreadId) {
        if let Some(notifs) = self.notifications.get_mut(&user_id) {
            for n in notifs.iter_mut() {
                if n.thread_id == thread_id {
                    n.read = true;
                }
            }
        }
    }

    /// Total notifications across all users.
    pub fn total_count(&self) -> usize {
        self.notifications.values().map(|v| v.len()).sum()
    }

    /// Clear all notifications for a user.
    pub fn clear_for_user(&mut self, user_id: Uuid) {
        self.notifications.remove(&user_id);
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn alice() -> Uuid {
        Uuid::from_bytes([1; 16])
    }

    fn bob() -> Uuid {
        Uuid::from_bytes([2; 16])
    }

    fn carol() -> Uuid {
        Uuid::from_bytes([3; 16])
    }

    #[test]
    fn notification_creation() {
        let tid = ThreadId::new();
        let cid = CommentId::new();
        let n = Notification::new(
            alice(),
            NotificationKind::Mention {
                mentioned_by: bob(),
                mentioned_by_name: "Bob".into(),
            },
            tid,
            Some(cid),
            1000,
        );
        assert!(n.is_unread());
        assert_eq!(n.summary(), "Bob mentioned you");
    }

    #[test]
    fn notification_mark_read() {
        let tid = ThreadId::new();
        let mut n = Notification::new(
            alice(),
            NotificationKind::Reply {
                reply_by: bob(),
                reply_by_name: "Bob".into(),
            },
            tid,
            None,
            1000,
        );
        assert!(n.is_unread());
        n.mark_read();
        assert!(!n.is_unread());
    }

    #[test]
    fn store_notify_and_query() {
        let mut store = NotificationStore::new();
        let tid = ThreadId::new();
        let cid = CommentId::new();

        store.notify_mention(alice(), bob(), "Bob", tid, cid, 1000);
        store.notify_reply(alice(), carol(), "Carol", tid, cid, 1001);

        assert_eq!(store.for_user(alice()).len(), 2);
        assert_eq!(store.unread_count(alice()), 2);
        assert_eq!(store.for_user(bob()).len(), 0);
    }

    #[test]
    fn store_mark_all_read() {
        let mut store = NotificationStore::new();
        let tid = ThreadId::new();
        let cid = CommentId::new();

        store.notify_mention(alice(), bob(), "Bob", tid, cid, 1000);
        store.notify_reply(alice(), carol(), "Carol", tid, cid, 1001);
        assert_eq!(store.unread_count(alice()), 2);

        store.mark_all_read(alice());
        assert_eq!(store.unread_count(alice()), 0);
    }

    #[test]
    fn store_mark_thread_read() {
        let mut store = NotificationStore::new();
        let tid1 = ThreadId::new();
        let tid2 = ThreadId::new();
        let cid = CommentId::new();

        store.notify_mention(alice(), bob(), "Bob", tid1, cid, 1000);
        store.notify_reply(alice(), carol(), "Carol", tid2, cid, 1001);

        store.mark_thread_read(alice(), tid1);
        assert_eq!(store.unread_count(alice()), 1);
    }

    #[test]
    fn store_self_reply_not_notified() {
        let mut store = NotificationStore::new();
        let tid = ThreadId::new();
        let cid = CommentId::new();

        store.notify_reply(alice(), alice(), "Alice", tid, cid, 1000);
        assert_eq!(store.for_user(alice()).len(), 0);
    }

    #[test]
    fn notification_kind_labels() {
        let kinds = vec![
            NotificationKind::Mention {
                mentioned_by: alice(),
                mentioned_by_name: "A".into(),
            },
            NotificationKind::Reply {
                reply_by: alice(),
                reply_by_name: "A".into(),
            },
            NotificationKind::ThreadResolved {
                resolved_by: alice(),
                resolved_by_name: "A".into(),
            },
            NotificationKind::Reaction {
                reacted_by: alice(),
                reacted_by_name: "A".into(),
                emoji: "👍".into(),
            },
            NotificationKind::Assignment {
                assigned_by: alice(),
                assigned_by_name: "A".into(),
            },
        ];
        for kind in &kinds {
            assert!(!kind.label().is_empty());
            assert!(!kind.actor_name().is_empty());
        }
    }
}
