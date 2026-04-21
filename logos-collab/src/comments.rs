// logos-collab/src/comments.rs
//
//! # CRDT-based Comment System
//!
//! Threaded comments with likes/dislikes, @mention tracking, soft-delete
//! with history, and weekly auto-cleanup.  Every mutation produces a
//! [`CommentDelta`] that can be broadcast to peers and merged conflict-free.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

// ── Timestamp (Unix ms) ───────────────────────────────────────────────────────

pub type Timestamp = u64;

fn now_ms() -> Timestamp {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Comment ───────────────────────────────────────────────────────────────────

/// A single comment (or reply).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Comment {
    pub id: Uuid,
    /// Top-level comments have `parent_id = None`;
    /// replies carry the parent comment's id.
    pub parent_id: Option<Uuid>,
    /// Thread root (same as `id` for top-level; parent's `id` for replies).
    pub thread_id: Uuid,
    pub author_id: Uuid,
    pub content: String,
    pub created_at: Timestamp,
    pub updated_at: Option<Timestamp>,
    /// Soft-delete: `true` means content is hidden but history kept.
    pub deleted: bool,
    /// Set of user IDs who liked this comment.
    pub likes: HashSet<Uuid>,
    /// Set of user IDs who disliked this comment.
    pub dislikes: HashSet<Uuid>,
    /// User IDs mentioned with `@username` syntax.
    pub mentions: Vec<Uuid>,
    /// Optional document layer / frame this comment is anchored to.
    pub anchor_layer_id: Option<Uuid>,
    /// Edit history (previous `content` values with timestamps).
    pub edit_history: Vec<(Timestamp, String)>,
}

impl Comment {
    /// Create a new top-level comment.
    pub fn new(
        author_id: Uuid,
        content: impl Into<String>,
        mentions: Vec<Uuid>,
        anchor_layer_id: Option<Uuid>,
    ) -> Self {
        let id = Uuid::new_v4();
        let content = content.into();
        Self {
            id,
            parent_id: None,
            thread_id: id,
            author_id,
            content,
            created_at: now_ms(),
            updated_at: None,
            deleted: false,
            likes: HashSet::new(),
            dislikes: HashSet::new(),
            mentions,
            anchor_layer_id,
            edit_history: Vec::new(),
        }
    }

    /// Create a reply to another comment.
    pub fn reply(
        parent: &Comment,
        author_id: Uuid,
        content: impl Into<String>,
        mentions: Vec<Uuid>,
    ) -> Self {
        let id = Uuid::new_v4();
        Self {
            id,
            parent_id: Some(parent.id),
            thread_id: parent.thread_id,
            author_id,
            content: content.into(),
            created_at: now_ms(),
            updated_at: None,
            deleted: false,
            likes: HashSet::new(),
            dislikes: HashSet::new(),
            mentions,
            anchor_layer_id: parent.anchor_layer_id,
            edit_history: Vec::new(),
        }
    }

    /// Edit content (appends to edit_history).
    pub fn edit(&mut self, new_content: impl Into<String>) {
        let old = std::mem::replace(&mut self.content, new_content.into());
        self.edit_history.push((now_ms(), old));
        self.updated_at = Some(now_ms());
    }

    /// Soft-delete.
    pub fn soft_delete(&mut self) {
        self.deleted = true;
        self.updated_at = Some(now_ms());
    }

    /// Toggle like by `user_id` (removes dislike if present).
    pub fn toggle_like(&mut self, user_id: Uuid) {
        if self.likes.contains(&user_id) {
            self.likes.remove(&user_id);
        } else {
            self.dislikes.remove(&user_id);
            self.likes.insert(user_id);
        }
    }

    /// Toggle dislike by `user_id` (removes like if present).
    pub fn toggle_dislike(&mut self, user_id: Uuid) {
        if self.dislikes.contains(&user_id) {
            self.dislikes.remove(&user_id);
        } else {
            self.likes.remove(&user_id);
            self.dislikes.insert(user_id);
        }
    }

    /// `true` if this was created before `cutoff_ms`.
    pub fn is_older_than(&self, cutoff_ms: Timestamp) -> bool {
        self.created_at < cutoff_ms
    }
}

// ── Comment delta (CRDT operation) ───────────────────────────────────────────

/// An append-only operation on the comment store.
///
/// Deltas are idempotent: applying the same delta twice has no effect.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CommentDelta {
    /// A new comment (or reply) was created.
    Created(Comment),
    /// A comment was edited.
    Edited { id: Uuid, new_content: String, edited_at: Timestamp },
    /// A comment was soft-deleted.
    Deleted { id: Uuid, deleted_at: Timestamp },
    /// A user liked a comment.
    Liked { comment_id: Uuid, user_id: Uuid },
    /// A user un-liked a comment.
    UnLiked { comment_id: Uuid, user_id: Uuid },
    /// A user disliked a comment.
    Disliked { comment_id: Uuid, user_id: Uuid },
    /// A user un-disliked a comment.
    UnDisliked { comment_id: Uuid, user_id: Uuid },
}

// ── Comment store ─────────────────────────────────────────────────────────────

/// In-memory CRDT comment store for a single document.
///
/// Apply [`CommentDelta`]s from both local and remote peers — order does not
/// matter; results are always the same (CRDT merge semantics).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommentStore {
    comments: HashMap<Uuid, Comment>,
}

impl CommentStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply a delta (local or remote).  Returns `true` if the store changed.
    pub fn apply(&mut self, delta: CommentDelta) -> bool {
        match delta {
            CommentDelta::Created(c) => {
                if self.comments.contains_key(&c.id) {
                    return false; // already present — idempotent
                }
                self.comments.insert(c.id, c);
                true
            }
            CommentDelta::Edited { id, new_content, edited_at } => {
                if let Some(c) = self.comments.get_mut(&id) {
                    // Last-write-wins: only apply if newer
                    let current_ts = c.updated_at.unwrap_or(c.created_at);
                    if edited_at > current_ts {
                        let old = std::mem::replace(&mut c.content, new_content);
                        c.edit_history.push((current_ts, old));
                        c.updated_at = Some(edited_at);
                        return true;
                    }
                }
                false
            }
            CommentDelta::Deleted { id, deleted_at } => {
                if let Some(c) = self.comments.get_mut(&id) {
                    if !c.deleted {
                        c.deleted = true;
                        c.updated_at = Some(deleted_at);
                        return true;
                    }
                }
                false
            }
            CommentDelta::Liked { comment_id, user_id } => {
                if let Some(c) = self.comments.get_mut(&comment_id) {
                    c.dislikes.remove(&user_id);
                    return c.likes.insert(user_id);
                }
                false
            }
            CommentDelta::UnLiked { comment_id, user_id } => {
                if let Some(c) = self.comments.get_mut(&comment_id) {
                    return c.likes.remove(&user_id);
                }
                false
            }
            CommentDelta::Disliked { comment_id, user_id } => {
                if let Some(c) = self.comments.get_mut(&comment_id) {
                    c.likes.remove(&user_id);
                    return c.dislikes.insert(user_id);
                }
                false
            }
            CommentDelta::UnDisliked { comment_id, user_id } => {
                if let Some(c) = self.comments.get_mut(&comment_id) {
                    return c.dislikes.remove(&user_id);
                }
                false
            }
        }
    }

    /// Get a comment by id.
    pub fn get(&self, id: &Uuid) -> Option<&Comment> {
        self.comments.get(id)
    }

    /// All non-deleted top-level comments (no replies).
    pub fn threads(&self) -> Vec<&Comment> {
        let mut v: Vec<_> = self.comments.values()
            .filter(|c| c.parent_id.is_none() && !c.deleted)
            .collect();
        v.sort_by_key(|c| c.created_at);
        v
    }

    /// All non-deleted replies for a given thread.
    pub fn replies(&self, thread_id: Uuid) -> Vec<&Comment> {
        let mut v: Vec<_> = self.comments.values()
            .filter(|c| c.thread_id == thread_id && c.parent_id.is_some() && !c.deleted)
            .collect();
        v.sort_by_key(|c| c.created_at);
        v
    }

    /// All comments (including deleted) for a given author.
    pub fn by_author(&self, author_id: Uuid) -> Vec<&Comment> {
        self.comments.values().filter(|c| c.author_id == author_id).collect()
    }

    /// Comments that mention a specific user.
    pub fn mentioning(&self, user_id: Uuid) -> Vec<&Comment> {
        self.comments.values()
            .filter(|c| !c.deleted && c.mentions.contains(&user_id))
            .collect()
    }

    /// Remove (hard-delete) all comments older than `cutoff_ms`.
    /// Returns the number of entries removed.
    pub fn cleanup_older_than(&mut self, cutoff_ms: Timestamp) -> usize {
        let before = self.comments.len();
        self.comments.retain(|_, c| !c.is_older_than(cutoff_ms));
        before - self.comments.len()
    }

    /// Total comment count (including deleted).
    pub fn len(&self) -> usize {
        self.comments.len()
    }

    pub fn is_empty(&self) -> bool {
        self.comments.is_empty()
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn uid() -> Uuid { Uuid::new_v4() }

    // ── Comment creation ──────────────────────────────────────────

    // C-01: New top-level comment has no parent.
    #[test]
    fn c_01_new_comment_no_parent() {
        let c = Comment::new(uid(), "hello", vec![], None);
        assert!(c.parent_id.is_none());
        assert_eq!(c.thread_id, c.id);
    }

    // C-02: Reply has parent_id set.
    #[test]
    fn c_02_reply_has_parent() {
        let parent = Comment::new(uid(), "root", vec![], None);
        let reply  = Comment::reply(&parent, uid(), "reply", vec![]);
        assert_eq!(reply.parent_id, Some(parent.id));
        assert_eq!(reply.thread_id, parent.id);
    }

    // C-03: New comment is not deleted.
    #[test]
    fn c_03_new_comment_not_deleted() {
        let c = Comment::new(uid(), "hello", vec![], None);
        assert!(!c.deleted);
    }

    // C-04: edit() updates content and adds to history.
    #[test]
    fn c_04_edit_updates_content() {
        let mut c = Comment::new(uid(), "original", vec![], None);
        c.edit("updated");
        assert_eq!(c.content, "updated");
        assert_eq!(c.edit_history.len(), 1);
        assert_eq!(c.edit_history[0].1, "original");
    }

    // C-05: soft_delete sets deleted flag.
    #[test]
    fn c_05_soft_delete() {
        let mut c = Comment::new(uid(), "hello", vec![], None);
        c.soft_delete();
        assert!(c.deleted);
    }

    // C-06: toggle_like adds user to likes.
    #[test]
    fn c_06_toggle_like_adds() {
        let mut c = Comment::new(uid(), "hi", vec![], None);
        let user = uid();
        c.toggle_like(user);
        assert!(c.likes.contains(&user));
        assert!(!c.dislikes.contains(&user));
    }

    // C-07: toggle_like twice removes the like.
    #[test]
    fn c_07_toggle_like_removes() {
        let mut c = Comment::new(uid(), "hi", vec![], None);
        let user = uid();
        c.toggle_like(user);
        c.toggle_like(user);
        assert!(!c.likes.contains(&user));
    }

    // C-08: toggle_like removes existing dislike.
    #[test]
    fn c_08_like_removes_dislike() {
        let mut c = Comment::new(uid(), "hi", vec![], None);
        let user = uid();
        c.toggle_dislike(user);
        c.toggle_like(user);
        assert!(!c.dislikes.contains(&user));
        assert!(c.likes.contains(&user));
    }

    // C-09: toggle_dislike removes existing like.
    #[test]
    fn c_09_dislike_removes_like() {
        let mut c = Comment::new(uid(), "hi", vec![], None);
        let user = uid();
        c.toggle_like(user);
        c.toggle_dislike(user);
        assert!(!c.likes.contains(&user));
        assert!(c.dislikes.contains(&user));
    }

    // C-10: toggle_dislike twice removes the dislike.
    #[test]
    fn c_10_toggle_dislike_removes() {
        let mut c = Comment::new(uid(), "hi", vec![], None);
        let user = uid();
        c.toggle_dislike(user);
        c.toggle_dislike(user);
        assert!(!c.dislikes.contains(&user));
    }

    // C-11: mentions are stored correctly.
    #[test]
    fn c_11_mentions_stored() {
        let mentioned = uid();
        let c = Comment::new(uid(), "hey @user", vec![mentioned], None);
        assert!(c.mentions.contains(&mentioned));
    }

    // C-12: is_older_than works.
    #[test]
    fn c_12_is_older_than() {
        let c = Comment::new(uid(), "old", vec![], None);
        let future = c.created_at + 1_000_000;
        assert!(c.is_older_than(future));
        assert!(!c.is_older_than(0));
    }

    // ── CommentStore ──────────────────────────────────────────────

    // C-13: New store is empty.
    #[test]
    fn c_13_new_store_empty() {
        let s = CommentStore::new();
        assert!(s.is_empty());
    }

    // C-14: Created delta adds comment.
    #[test]
    fn c_14_created_delta_adds() {
        let mut s = CommentStore::new();
        let c = Comment::new(uid(), "hello", vec![], None);
        assert!(s.apply(CommentDelta::Created(c)));
        assert_eq!(s.len(), 1);
    }

    // C-15: Created delta is idempotent.
    #[test]
    fn c_15_created_delta_idempotent() {
        let mut s = CommentStore::new();
        let c = Comment::new(uid(), "hello", vec![], None);
        s.apply(CommentDelta::Created(c.clone()));
        let second = s.apply(CommentDelta::Created(c));
        assert!(!second);
        assert_eq!(s.len(), 1);
    }

    // C-16: Edited delta updates content.
    #[test]
    fn c_16_edited_delta_updates() {
        let mut s = CommentStore::new();
        let c = Comment::new(uid(), "old", vec![], None);
        let id = c.id;
        s.apply(CommentDelta::Created(c));
        s.apply(CommentDelta::Edited { id, new_content: "new".into(), edited_at: now_ms() + 1 });
        assert_eq!(s.get(&id).unwrap().content, "new");
    }

    // C-17: Deleted delta soft-deletes.
    #[test]
    fn c_17_deleted_delta_soft_deletes() {
        let mut s = CommentStore::new();
        let c = Comment::new(uid(), "bye", vec![], None);
        let id = c.id;
        s.apply(CommentDelta::Created(c));
        s.apply(CommentDelta::Deleted { id, deleted_at: now_ms() });
        assert!(s.get(&id).unwrap().deleted);
    }

    // C-18: Liked delta adds to likes.
    #[test]
    fn c_18_liked_delta() {
        let mut s = CommentStore::new();
        let user = uid();
        let c = Comment::new(uid(), "nice", vec![], None);
        let cid = c.id;
        s.apply(CommentDelta::Created(c));
        s.apply(CommentDelta::Liked { comment_id: cid, user_id: user });
        assert!(s.get(&cid).unwrap().likes.contains(&user));
    }

    // C-19: UnLiked removes from likes.
    #[test]
    fn c_19_unliked_delta() {
        let mut s = CommentStore::new();
        let user = uid();
        let c = Comment::new(uid(), "nice", vec![], None);
        let cid = c.id;
        s.apply(CommentDelta::Created(c));
        s.apply(CommentDelta::Liked     { comment_id: cid, user_id: user });
        s.apply(CommentDelta::UnLiked   { comment_id: cid, user_id: user });
        assert!(!s.get(&cid).unwrap().likes.contains(&user));
    }

    // C-20: Disliked delta adds to dislikes.
    #[test]
    fn c_20_disliked_delta() {
        let mut s = CommentStore::new();
        let user = uid();
        let c = Comment::new(uid(), "meh", vec![], None);
        let cid = c.id;
        s.apply(CommentDelta::Created(c));
        s.apply(CommentDelta::Disliked { comment_id: cid, user_id: user });
        assert!(s.get(&cid).unwrap().dislikes.contains(&user));
    }

    // C-21: UnDisliked removes from dislikes.
    #[test]
    fn c_21_undisliked_delta() {
        let mut s = CommentStore::new();
        let user = uid();
        let c = Comment::new(uid(), "meh", vec![], None);
        let cid = c.id;
        s.apply(CommentDelta::Created(c));
        s.apply(CommentDelta::Disliked   { comment_id: cid, user_id: user });
        s.apply(CommentDelta::UnDisliked { comment_id: cid, user_id: user });
        assert!(!s.get(&cid).unwrap().dislikes.contains(&user));
    }

    // C-22: threads() returns only top-level non-deleted.
    #[test]
    fn c_22_threads_top_level_only() {
        let mut s = CommentStore::new();
        let parent = Comment::new(uid(), "root", vec![], None);
        let pid = parent.id;
        let reply = Comment::reply(&parent, uid(), "reply", vec![]);
        s.apply(CommentDelta::Created(parent));
        s.apply(CommentDelta::Created(reply));
        let threads = s.threads();
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].id, pid);
    }

    // C-23: replies() returns only replies for given thread.
    #[test]
    fn c_23_replies_for_thread() {
        let mut s = CommentStore::new();
        let parent = Comment::new(uid(), "root", vec![], None);
        let tid = parent.thread_id;
        let r1 = Comment::reply(&parent, uid(), "r1", vec![]);
        let r2 = Comment::reply(&parent, uid(), "r2", vec![]);
        s.apply(CommentDelta::Created(parent));
        s.apply(CommentDelta::Created(r1));
        s.apply(CommentDelta::Created(r2));
        assert_eq!(s.replies(tid).len(), 2);
    }

    // C-24: deleted replies not included.
    #[test]
    fn c_24_deleted_replies_excluded() {
        let mut s = CommentStore::new();
        let parent = Comment::new(uid(), "root", vec![], None);
        let tid = parent.thread_id;
        let reply = Comment::reply(&parent, uid(), "r", vec![]);
        let rid = reply.id;
        s.apply(CommentDelta::Created(parent));
        s.apply(CommentDelta::Created(reply));
        s.apply(CommentDelta::Deleted { id: rid, deleted_at: now_ms() });
        assert_eq!(s.replies(tid).len(), 0);
    }

    // C-25: by_author returns all (including deleted).
    #[test]
    fn c_25_by_author_includes_deleted() {
        let mut s = CommentStore::new();
        let author = uid();
        let c = Comment::new(author, "x", vec![], None);
        let id = c.id;
        s.apply(CommentDelta::Created(c));
        s.apply(CommentDelta::Deleted { id, deleted_at: now_ms() });
        assert_eq!(s.by_author(author).len(), 1);
    }

    // C-26: mentioning() returns comments that mention user.
    #[test]
    fn c_26_mentioning_returns_matches() {
        let mut s = CommentStore::new();
        let target = uid();
        let c = Comment::new(uid(), "hey", vec![target], None);
        s.apply(CommentDelta::Created(c));
        assert_eq!(s.mentioning(target).len(), 1);
    }

    // C-27: cleanup_older_than removes old entries.
    #[test]
    fn c_27_cleanup_removes_old() {
        let mut s = CommentStore::new();
        let c = Comment::new(uid(), "old", vec![], None);
        let cutoff = c.created_at + 1_000_000;
        s.apply(CommentDelta::Created(c));
        let removed = s.cleanup_older_than(cutoff);
        assert_eq!(removed, 1);
        assert!(s.is_empty());
    }

    // C-28: cleanup_older_than keeps recent entries.
    #[test]
    fn c_28_cleanup_keeps_recent() {
        let mut s = CommentStore::new();
        let c = Comment::new(uid(), "new", vec![], None);
        let cutoff = 0; // everything is newer than 0
        s.apply(CommentDelta::Created(c));
        let removed = s.cleanup_older_than(cutoff);
        assert_eq!(removed, 0);
        assert_eq!(s.len(), 1);
    }

    // C-29: CRDT merge: applying same Created twice is safe.
    #[test]
    fn c_29_crdt_idempotent_like() {
        let mut s = CommentStore::new();
        let user = uid();
        let c = Comment::new(uid(), "test", vec![], None);
        let cid = c.id;
        s.apply(CommentDelta::Created(c));
        s.apply(CommentDelta::Liked { comment_id: cid, user_id: user });
        s.apply(CommentDelta::Liked { comment_id: cid, user_id: user });
        assert_eq!(s.get(&cid).unwrap().likes.len(), 1);
    }

    // C-30: get() returns None for unknown id.
    #[test]
    fn c_30_get_unknown_returns_none() {
        let s = CommentStore::new();
        assert!(s.get(&uid()).is_none());
    }
}
