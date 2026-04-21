// logos-collab/src/notifications.rs
//
//! # Notification & Mention System
//!
//! Delivers in-app notifications when:
//! - a user is @mentioned in a comment
//! - someone replies to a thread the user participated in
//! - a role change affects the user
//! - ownership is transferred
//!
//! The [`NotificationCenter`] is a simple in-memory inbox keyed by user id.
//! In production it should be backed by a persistent store and fanned out
//! through WebSocket push.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

pub type Timestamp = u64;

fn now_ms() -> Timestamp {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Notification kind ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NotificationKind {
    /// You were @mentioned in a comment.
    Mentioned { comment_id: Uuid, by_user_id: Uuid },
    /// Someone replied in a thread you participated in.
    ThreadReply { thread_id: Uuid, comment_id: Uuid, by_user_id: Uuid },
    /// Your role was changed.
    RoleChanged { new_role: String, by_user_id: Uuid },
    /// Project ownership was transferred to you.
    OwnershipTransferred { by_user_id: Uuid },
    /// A member was added to the project you own/manage.
    MemberAdded { new_user_id: Uuid },
    /// A member was removed.
    MemberRemoved { removed_user_id: Uuid },
    /// Generic / custom notification from a plugin or extension.
    Custom { title: String, body: String },
}

impl NotificationKind {
    pub fn title(&self) -> &str {
        match self {
            NotificationKind::Mentioned          { .. } => "You were mentioned",
            NotificationKind::ThreadReply        { .. } => "New reply in your thread",
            NotificationKind::RoleChanged        { .. } => "Your role has changed",
            NotificationKind::OwnershipTransferred{..} => "You are now the owner",
            NotificationKind::MemberAdded        { .. } => "New team member",
            NotificationKind::MemberRemoved      { .. } => "Team member removed",
            NotificationKind::Custom { title, .. }      => title.as_str(),
        }
    }
}

// ── Notification ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Notification {
    pub id: Uuid,
    pub recipient_id: Uuid,
    pub kind: NotificationKind,
    pub created_at: Timestamp,
    pub read: bool,
}

impl Notification {
    pub fn new(recipient_id: Uuid, kind: NotificationKind) -> Self {
        Self {
            id: Uuid::new_v4(),
            recipient_id,
            kind,
            created_at: now_ms(),
            read: false,
        }
    }

    pub fn mark_read(&mut self) {
        self.read = true;
    }
}

// ── Notification center ───────────────────────────────────────────────────────

/// In-memory per-user notification inbox.
#[derive(Debug, Default)]
pub struct NotificationCenter {
    /// user_id → ordered list of notifications (newest last).
    inboxes: HashMap<Uuid, Vec<Notification>>,
}

impl NotificationCenter {
    pub fn new() -> Self { Self::default() }

    /// Deliver a notification to its recipient.
    pub fn deliver(&mut self, n: Notification) {
        self.inboxes.entry(n.recipient_id).or_default().push(n);
    }

    /// All notifications for `user_id` (oldest first).
    pub fn inbox(&self, user_id: Uuid) -> &[Notification] {
        self.inboxes.get(&user_id).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Unread notifications for `user_id`.
    pub fn unread(&self, user_id: Uuid) -> Vec<&Notification> {
        self.inbox(user_id).iter().filter(|n| !n.read).collect()
    }

    /// Mark a single notification as read.  Returns `true` if found.
    pub fn mark_read(&mut self, user_id: Uuid, notification_id: Uuid) -> bool {
        if let Some(inbox) = self.inboxes.get_mut(&user_id) {
            if let Some(n) = inbox.iter_mut().find(|n| n.id == notification_id) {
                n.mark_read();
                return true;
            }
        }
        false
    }

    /// Mark all notifications for `user_id` as read.
    pub fn mark_all_read(&mut self, user_id: Uuid) {
        if let Some(inbox) = self.inboxes.get_mut(&user_id) {
            for n in inbox.iter_mut() { n.mark_read(); }
        }
    }

    /// Remove all notifications for `user_id`.
    pub fn clear(&mut self, user_id: Uuid) {
        self.inboxes.remove(&user_id);
    }

    /// Unread count for `user_id`.
    pub fn unread_count(&self, user_id: Uuid) -> usize {
        self.unread(user_id).len()
    }
}

// ── Mention dispatcher ────────────────────────────────────────────────────────

/// Parses a comment's `mentions` list and delivers [`NotificationKind::Mentioned`]
/// to each mentioned user.
pub fn dispatch_mention_notifications(
    center: &mut NotificationCenter,
    comment_id: Uuid,
    by_user_id: Uuid,
    mentions: &[Uuid],
) {
    for &recipient_id in mentions {
        if recipient_id == by_user_id { continue; } // don't notify yourself
        center.deliver(Notification::new(
            recipient_id,
            NotificationKind::Mentioned { comment_id, by_user_id },
        ));
    }
}

/// Delivers a [`NotificationKind::ThreadReply`] to all thread participants
/// except the commenter themselves.
pub fn dispatch_thread_reply_notifications(
    center: &mut NotificationCenter,
    thread_id: Uuid,
    comment_id: Uuid,
    by_user_id: Uuid,
    participants: &[Uuid],
) {
    for &recipient_id in participants {
        if recipient_id == by_user_id { continue; }
        center.deliver(Notification::new(
            recipient_id,
            NotificationKind::ThreadReply { thread_id, comment_id, by_user_id },
        ));
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn uid() -> Uuid { Uuid::new_v4() }

    // ── Notification ──────────────────────────────────────────────

    // N-01: New notification is unread.
    #[test]
    fn n_01_new_notification_unread() {
        let n = Notification::new(uid(), NotificationKind::Custom { title: "Hi".into(), body: "Body".into() });
        assert!(!n.read);
    }

    // N-02: mark_read sets read=true.
    #[test]
    fn n_02_mark_read() {
        let mut n = Notification::new(uid(), NotificationKind::Custom { title: "Hi".into(), body: "Body".into() });
        n.mark_read();
        assert!(n.read);
    }

    // N-03: title() is non-empty for every kind.
    #[test]
    fn n_03_title_non_empty() {
        let kinds = vec![
            NotificationKind::Mentioned { comment_id: uid(), by_user_id: uid() },
            NotificationKind::ThreadReply { thread_id: uid(), comment_id: uid(), by_user_id: uid() },
            NotificationKind::RoleChanged { new_role: "editor".into(), by_user_id: uid() },
            NotificationKind::OwnershipTransferred { by_user_id: uid() },
            NotificationKind::MemberAdded { new_user_id: uid() },
            NotificationKind::MemberRemoved { removed_user_id: uid() },
            NotificationKind::Custom { title: "Test".into(), body: "msg".into() },
        ];
        for k in &kinds { assert!(!k.title().is_empty()); }
    }

    // ── NotificationCenter ────────────────────────────────────────

    // N-04: Empty center returns empty inbox.
    #[test]
    fn n_04_empty_inbox() {
        let center = NotificationCenter::new();
        assert!(center.inbox(uid()).is_empty());
    }

    // N-05: deliver adds to recipient's inbox.
    #[test]
    fn n_05_deliver_adds_to_inbox() {
        let mut center = NotificationCenter::new();
        let user = uid();
        center.deliver(Notification::new(user, NotificationKind::Custom { title: "T".into(), body: "B".into() }));
        assert_eq!(center.inbox(user).len(), 1);
    }

    // N-06: deliver to multiple users independently.
    #[test]
    fn n_06_independent_inboxes() {
        let mut center = NotificationCenter::new();
        let a = uid(); let b = uid();
        center.deliver(Notification::new(a, NotificationKind::Custom { title: "A".into(), body: "".into() }));
        center.deliver(Notification::new(b, NotificationKind::Custom { title: "B".into(), body: "".into() }));
        assert_eq!(center.inbox(a).len(), 1);
        assert_eq!(center.inbox(b).len(), 1);
    }

    // N-07: unread() returns only unread.
    #[test]
    fn n_07_unread_filter() {
        let mut center = NotificationCenter::new();
        let user = uid();
        let n1 = Notification::new(user, NotificationKind::Custom { title: "1".into(), body: "".into() });
        let mut n2 = Notification::new(user, NotificationKind::Custom { title: "2".into(), body: "".into() });
        n2.mark_read();
        center.deliver(n1);
        center.deliver(n2);
        assert_eq!(center.unread(user).len(), 1);
    }

    // N-08: mark_read marks single notification.
    #[test]
    fn n_08_mark_read_single() {
        let mut center = NotificationCenter::new();
        let user = uid();
        let n = Notification::new(user, NotificationKind::Custom { title: "X".into(), body: "".into() });
        let nid = n.id;
        center.deliver(n);
        assert!(center.mark_read(user, nid));
        assert_eq!(center.unread_count(user), 0);
    }

    // N-09: mark_read returns false for unknown notification.
    #[test]
    fn n_09_mark_read_unknown_nid() {
        let mut center = NotificationCenter::new();
        assert!(!center.mark_read(uid(), uid()));
    }

    // N-10: mark_all_read clears unread count.
    #[test]
    fn n_10_mark_all_read() {
        let mut center = NotificationCenter::new();
        let user = uid();
        for _ in 0..5 {
            center.deliver(Notification::new(user, NotificationKind::Custom { title: "X".into(), body: "".into() }));
        }
        center.mark_all_read(user);
        assert_eq!(center.unread_count(user), 0);
    }

    // N-11: clear removes all notifications.
    #[test]
    fn n_11_clear_inbox() {
        let mut center = NotificationCenter::new();
        let user = uid();
        center.deliver(Notification::new(user, NotificationKind::Custom { title: "X".into(), body: "".into() }));
        center.clear(user);
        assert!(center.inbox(user).is_empty());
    }

    // N-12: unread_count matches unread().len().
    #[test]
    fn n_12_unread_count_matches() {
        let mut center = NotificationCenter::new();
        let user = uid();
        for _ in 0..3 {
            center.deliver(Notification::new(user, NotificationKind::Custom { title: "".into(), body: "".into() }));
        }
        assert_eq!(center.unread_count(user), center.unread(user).len());
    }

    // ── dispatch helpers ──────────────────────────────────────────

    // N-13: dispatch_mention_notifications delivers to each mention.
    #[test]
    fn n_13_dispatch_mentions() {
        let mut center = NotificationCenter::new();
        let author  = uid();
        let alice   = uid();
        let bob     = uid();
        let cid     = uid();

        dispatch_mention_notifications(&mut center, cid, author, &[alice, bob]);

        assert_eq!(center.unread_count(alice), 1);
        assert_eq!(center.unread_count(bob),   1);
    }

    // N-14: Mentioner does not receive notification.
    #[test]
    fn n_14_no_self_mention() {
        let mut center = NotificationCenter::new();
        let author = uid();
        dispatch_mention_notifications(&mut center, uid(), author, &[author]);
        assert_eq!(center.unread_count(author), 0);
    }

    // N-15: dispatch_thread_reply_notifications notifies participants.
    #[test]
    fn n_15_dispatch_thread_reply() {
        let mut center = NotificationCenter::new();
        let replier     = uid();
        let participant = uid();

        dispatch_thread_reply_notifications(&mut center, uid(), uid(), replier, &[replier, participant]);

        assert_eq!(center.unread_count(participant), 1);
        assert_eq!(center.unread_count(replier),     0); // no self-notify
    }
}
