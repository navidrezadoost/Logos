//! # Comment System
//!
//! Threaded comments anchored to layers, components, or canvas regions.
//! Supports reactions, resolution state, and edit history.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// ── Identifiers ──────────────────────────────────────────────────────

/// Unique identifier for a comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CommentId(pub Uuid);

impl CommentId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for CommentId {
    fn default() -> Self {
        Self::new()
    }
}

/// Unique identifier for a comment thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ThreadId(pub Uuid);

impl ThreadId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ThreadId {
    fn default() -> Self {
        Self::new()
    }
}

// ── Comment Anchor ───────────────────────────────────────────────────

/// Where a comment thread is anchored in the document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CommentAnchor {
    /// Anchored to a specific layer by ID.
    Layer { layer_id: Uuid },
    /// Anchored to a component definition.
    Component { component_id: Uuid },
    /// Anchored to a component instance.
    Instance { instance_id: Uuid },
    /// Anchored to a canvas position (absolute coordinates).
    Canvas { x: f64, y: f64, page_id: Uuid },
    /// Anchored to a specific property on a layer.
    Property {
        layer_id: Uuid,
        property_path: String,
    },
}

impl CommentAnchor {
    pub fn layer(id: Uuid) -> Self {
        Self::Layer { layer_id: id }
    }

    pub fn component(id: Uuid) -> Self {
        Self::Component { component_id: id }
    }

    pub fn instance(id: Uuid) -> Self {
        Self::Instance { instance_id: id }
    }

    pub fn canvas(x: f64, y: f64, page_id: Uuid) -> Self {
        Self::Canvas { x, y, page_id }
    }

    pub fn property(layer_id: Uuid, path: impl Into<String>) -> Self {
        Self::Property {
            layer_id,
            property_path: path.into(),
        }
    }

    /// Returns the primary target UUID (layer, component, instance, or page).
    pub fn target_id(&self) -> Uuid {
        match self {
            Self::Layer { layer_id } => *layer_id,
            Self::Component { component_id } => *component_id,
            Self::Instance { instance_id } => *instance_id,
            Self::Canvas { page_id, .. } => *page_id,
            Self::Property { layer_id, .. } => *layer_id,
        }
    }
}

// ── Reaction ─────────────────────────────────────────────────────────

/// A reaction (emoji) on a comment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommentReaction {
    pub emoji: String,
    pub user_id: Uuid,
    pub timestamp: u64,
}

impl CommentReaction {
    pub fn new(emoji: impl Into<String>, user_id: Uuid, timestamp: u64) -> Self {
        Self {
            emoji: emoji.into(),
            user_id,
            timestamp,
        }
    }
}

// ── Comment ──────────────────────────────────────────────────────────

/// The resolution state of a comment thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResolutionState {
    Open,
    Resolved,
    WontFix,
}

impl Default for ResolutionState {
    fn default() -> Self {
        Self::Open
    }
}

/// A single comment in a thread.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Comment {
    pub id: CommentId,
    pub author_id: Uuid,
    pub author_name: String,
    pub content: String,
    pub created_at: u64,
    pub edited_at: Option<u64>,
    pub reactions: Vec<CommentReaction>,
    /// Previous versions of the content (edit history).
    pub edit_history: Vec<(u64, String)>,
    pub deleted: bool,
}

impl Comment {
    pub fn new(
        author_id: Uuid,
        author_name: impl Into<String>,
        content: impl Into<String>,
        timestamp: u64,
    ) -> Self {
        Self {
            id: CommentId::new(),
            author_id,
            author_name: author_name.into(),
            content: content.into(),
            created_at: timestamp,
            edited_at: None,
            reactions: Vec::new(),
            edit_history: Vec::new(),
            deleted: false,
        }
    }

    /// Edit the comment contents, preserving history.
    pub fn edit(&mut self, new_content: impl Into<String>, timestamp: u64) {
        self.edit_history
            .push((self.edited_at.unwrap_or(self.created_at), self.content.clone()));
        self.content = new_content.into();
        self.edited_at = Some(timestamp);
    }

    /// Soft-delete the comment.
    pub fn delete(&mut self) {
        self.deleted = true;
        self.content = "[deleted]".into();
    }

    /// Add a reaction.
    pub fn add_reaction(&mut self, emoji: impl Into<String>, user_id: Uuid, timestamp: u64) {
        let emoji = emoji.into();
        // Don't allow duplicate reactions from same user with same emoji
        if !self
            .reactions
            .iter()
            .any(|r| r.user_id == user_id && r.emoji == emoji)
        {
            self.reactions
                .push(CommentReaction::new(emoji, user_id, timestamp));
        }
    }

    /// Remove a reaction.
    pub fn remove_reaction(&mut self, emoji: &str, user_id: Uuid) {
        self.reactions
            .retain(|r| !(r.user_id == user_id && r.emoji == emoji));
    }

    /// Count reactions by emoji.
    pub fn reaction_counts(&self) -> HashMap<String, usize> {
        let mut counts = HashMap::new();
        for r in &self.reactions {
            *counts.entry(r.emoji.clone()).or_insert(0) += 1;
        }
        counts
    }

    pub fn is_edited(&self) -> bool {
        self.edited_at.is_some()
    }

    pub fn is_deleted(&self) -> bool {
        self.deleted
    }

    pub fn edit_count(&self) -> usize {
        self.edit_history.len()
    }
}

// ── Comment Thread ───────────────────────────────────────────────────

/// A comment thread anchored to a document element.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommentThread {
    pub id: ThreadId,
    pub anchor: CommentAnchor,
    pub comments: Vec<Comment>,
    pub resolution: ResolutionState,
    pub resolved_by: Option<Uuid>,
    pub resolved_at: Option<u64>,
    pub created_at: u64,
    pub participants: Vec<Uuid>,
}

impl CommentThread {
    pub fn new(anchor: CommentAnchor, timestamp: u64) -> Self {
        Self {
            id: ThreadId::new(),
            anchor,
            comments: Vec::new(),
            resolution: ResolutionState::Open,
            resolved_by: None,
            resolved_at: None,
            created_at: timestamp,
            participants: Vec::new(),
        }
    }

    /// Start a new thread with an initial comment.
    pub fn start(
        anchor: CommentAnchor,
        author_id: Uuid,
        author_name: impl Into<String>,
        content: impl Into<String>,
        timestamp: u64,
    ) -> Self {
        let mut thread = Self::new(anchor, timestamp);
        thread.add_comment(
            Comment::new(author_id, author_name, content, timestamp),
        );
        thread
    }

    /// Add a comment to this thread.
    pub fn add_comment(&mut self, comment: Comment) {
        if !self.participants.contains(&comment.author_id) {
            self.participants.push(comment.author_id);
        }
        self.comments.push(comment);
    }

    /// Reply to the thread.
    pub fn reply(
        &mut self,
        author_id: Uuid,
        author_name: impl Into<String>,
        content: impl Into<String>,
        timestamp: u64,
    ) -> CommentId {
        let comment = Comment::new(author_id, author_name, content, timestamp);
        let id = comment.id;
        self.add_comment(comment);
        id
    }

    /// Find a comment by ID.
    pub fn get_comment(&self, id: CommentId) -> Option<&Comment> {
        self.comments.iter().find(|c| c.id == id)
    }

    /// Find a comment by ID (mutable).
    pub fn get_comment_mut(&mut self, id: CommentId) -> Option<&mut Comment> {
        self.comments.iter_mut().find(|c| c.id == id)
    }

    /// Resolve the thread.
    pub fn resolve(&mut self, user_id: Uuid, timestamp: u64) {
        self.resolution = ResolutionState::Resolved;
        self.resolved_by = Some(user_id);
        self.resolved_at = Some(timestamp);
    }

    /// Reopen the thread.
    pub fn reopen(&mut self) {
        self.resolution = ResolutionState::Open;
        self.resolved_by = None;
        self.resolved_at = None;
    }

    /// Mark as won't fix.
    pub fn wont_fix(&mut self, user_id: Uuid, timestamp: u64) {
        self.resolution = ResolutionState::WontFix;
        self.resolved_by = Some(user_id);
        self.resolved_at = Some(timestamp);
    }

    pub fn is_resolved(&self) -> bool {
        matches!(
            self.resolution,
            ResolutionState::Resolved | ResolutionState::WontFix
        )
    }

    pub fn comment_count(&self) -> usize {
        self.comments.len()
    }

    pub fn active_comment_count(&self) -> usize {
        self.comments.iter().filter(|c| !c.deleted).count()
    }

    pub fn participant_count(&self) -> usize {
        self.participants.len()
    }

    /// Get the most recent non-deleted comment.
    pub fn latest_comment(&self) -> Option<&Comment> {
        self.comments.iter().rev().find(|c| !c.deleted)
    }
}

// ── Comment Store ────────────────────────────────────────────────────

/// In-memory store for all comment threads in a document.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommentStore {
    pub threads: HashMap<ThreadId, CommentThread>,
}

impl CommentStore {
    pub fn new() -> Self {
        Self {
            threads: HashMap::new(),
        }
    }

    /// Start a new thread and return its ID.
    pub fn start_thread(
        &mut self,
        anchor: CommentAnchor,
        author_id: Uuid,
        author_name: impl Into<String>,
        content: impl Into<String>,
        timestamp: u64,
    ) -> ThreadId {
        let thread = CommentThread::start(anchor, author_id, author_name, content, timestamp);
        let id = thread.id;
        self.threads.insert(id, thread);
        id
    }

    /// Get a thread by ID.
    pub fn get_thread(&self, id: ThreadId) -> Option<&CommentThread> {
        self.threads.get(&id)
    }

    /// Get a mutable thread by ID.
    pub fn get_thread_mut(&mut self, id: ThreadId) -> Option<&mut CommentThread> {
        self.threads.get_mut(&id)
    }

    /// Remove a thread.
    pub fn remove_thread(&mut self, id: ThreadId) -> Option<CommentThread> {
        self.threads.remove(&id)
    }

    /// Find threads anchored to a specific target.
    pub fn threads_for_target(&self, target_id: Uuid) -> Vec<&CommentThread> {
        self.threads
            .values()
            .filter(|t| t.anchor.target_id() == target_id)
            .collect()
    }

    /// Find all open threads.
    pub fn open_threads(&self) -> Vec<&CommentThread> {
        self.threads
            .values()
            .filter(|t| !t.is_resolved())
            .collect()
    }

    /// Find all resolved threads.
    pub fn resolved_threads(&self) -> Vec<&CommentThread> {
        self.threads
            .values()
            .filter(|t| t.is_resolved())
            .collect()
    }

    /// Total threads.
    pub fn thread_count(&self) -> usize {
        self.threads.len()
    }

    /// Total comments across all threads.
    pub fn total_comments(&self) -> usize {
        self.threads.values().map(|t| t.comment_count()).sum()
    }

    /// Search comments by content substring (case-insensitive).
    pub fn search(&self, query: &str) -> Vec<(ThreadId, CommentId)> {
        let query_lower = query.to_lowercase();
        let mut results = Vec::new();
        for (tid, thread) in &self.threads {
            for comment in &thread.comments {
                if !comment.deleted && comment.content.to_lowercase().contains(&query_lower) {
                    results.push((*tid, comment.id));
                }
            }
        }
        results
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

    fn layer_id() -> Uuid {
        Uuid::from_bytes([10; 16])
    }

    #[test]
    fn test_comment_creation() {
        let c = Comment::new(alice(), "Alice", "Hello world", 1000);
        assert_eq!(c.author_name, "Alice");
        assert_eq!(c.content, "Hello world");
        assert_eq!(c.created_at, 1000);
        assert!(!c.is_edited());
        assert!(!c.is_deleted());
    }

    #[test]
    fn test_comment_edit() {
        let mut c = Comment::new(alice(), "Alice", "Typo", 1000);
        c.edit("Fixed typo", 1001);
        assert_eq!(c.content, "Fixed typo");
        assert!(c.is_edited());
        assert_eq!(c.edit_count(), 1);
        assert_eq!(c.edit_history[0], (1000, "Typo".into()));
    }

    #[test]
    fn test_comment_multiple_edits() {
        let mut c = Comment::new(alice(), "Alice", "v1", 1000);
        c.edit("v2", 1001);
        c.edit("v3", 1002);
        assert_eq!(c.content, "v3");
        assert_eq!(c.edit_count(), 2);
    }

    #[test]
    fn test_comment_delete() {
        let mut c = Comment::new(alice(), "Alice", "Secret", 1000);
        c.delete();
        assert!(c.is_deleted());
        assert_eq!(c.content, "[deleted]");
    }

    #[test]
    fn test_comment_reactions() {
        let mut c = Comment::new(alice(), "Alice", "Nice!", 1000);
        c.add_reaction("👍", bob(), 1001);
        c.add_reaction("❤️", alice(), 1002);
        assert_eq!(c.reactions.len(), 2);

        let counts = c.reaction_counts();
        assert_eq!(counts.get("👍"), Some(&1));
        assert_eq!(counts.get("❤️"), Some(&1));
    }

    #[test]
    fn test_comment_duplicate_reaction_prevented() {
        let mut c = Comment::new(alice(), "Alice", "Yep", 1000);
        c.add_reaction("👍", bob(), 1001);
        c.add_reaction("👍", bob(), 1002); // duplicate
        assert_eq!(c.reactions.len(), 1);
    }

    #[test]
    fn test_comment_remove_reaction() {
        let mut c = Comment::new(alice(), "Alice", "Ok", 1000);
        c.add_reaction("👍", bob(), 1001);
        c.remove_reaction("👍", bob());
        assert!(c.reactions.is_empty());
    }

    #[test]
    fn test_thread_creation() {
        let thread = CommentThread::new(CommentAnchor::layer(layer_id()), 1000);
        assert!(thread.comments.is_empty());
        assert!(!thread.is_resolved());
        assert_eq!(thread.comment_count(), 0);
    }

    #[test]
    fn test_thread_start_with_comment() {
        let thread = CommentThread::start(
            CommentAnchor::layer(layer_id()),
            alice(),
            "Alice",
            "First!",
            1000,
        );
        assert_eq!(thread.comment_count(), 1);
        assert_eq!(thread.participant_count(), 1);
    }

    #[test]
    fn test_thread_reply() {
        let mut thread = CommentThread::start(
            CommentAnchor::layer(layer_id()),
            alice(),
            "Alice",
            "Initial",
            1000,
        );
        thread.reply(bob(), "Bob", "Reply!", 1001);
        assert_eq!(thread.comment_count(), 2);
        assert_eq!(thread.participant_count(), 2);
    }

    #[test]
    fn test_thread_resolve_reopen() {
        let mut thread = CommentThread::start(
            CommentAnchor::layer(layer_id()),
            alice(),
            "Alice",
            "Bug",
            1000,
        );
        thread.resolve(bob(), 1001);
        assert!(thread.is_resolved());
        assert_eq!(thread.resolved_by, Some(bob()));

        thread.reopen();
        assert!(!thread.is_resolved());
        assert!(thread.resolved_by.is_none());
    }

    #[test]
    fn test_thread_wont_fix() {
        let mut thread = CommentThread::start(
            CommentAnchor::layer(layer_id()),
            alice(),
            "Alice",
            "Edge case",
            1000,
        );
        thread.wont_fix(bob(), 1001);
        assert!(thread.is_resolved());
        assert_eq!(thread.resolution, ResolutionState::WontFix);
    }

    #[test]
    fn test_thread_latest_comment() {
        let mut thread = CommentThread::start(
            CommentAnchor::layer(layer_id()),
            alice(),
            "Alice",
            "First",
            1000,
        );
        thread.reply(bob(), "Bob", "Second", 1001);
        assert_eq!(thread.latest_comment().unwrap().content, "Second");
    }

    #[test]
    fn test_thread_latest_skips_deleted() {
        let mut thread = CommentThread::start(
            CommentAnchor::layer(layer_id()),
            alice(),
            "Alice",
            "First",
            1000,
        );
        let reply_id = thread.reply(bob(), "Bob", "Deleted", 1001);
        thread.get_comment_mut(reply_id).unwrap().delete();
        assert_eq!(thread.latest_comment().unwrap().content, "First");
    }

    #[test]
    fn test_thread_active_count() {
        let mut thread = CommentThread::start(
            CommentAnchor::layer(layer_id()),
            alice(),
            "Alice",
            "Active",
            1000,
        );
        let id = thread.reply(bob(), "Bob", "Will delete", 1001);
        thread.get_comment_mut(id).unwrap().delete();
        assert_eq!(thread.active_comment_count(), 1);
        assert_eq!(thread.comment_count(), 2);
    }

    #[test]
    fn test_comment_anchor_target_id() {
        let lid = Uuid::new_v4();
        assert_eq!(CommentAnchor::layer(lid).target_id(), lid);

        let cid = Uuid::new_v4();
        assert_eq!(CommentAnchor::component(cid).target_id(), cid);

        let iid = Uuid::new_v4();
        assert_eq!(CommentAnchor::instance(iid).target_id(), iid);

        let pid = Uuid::new_v4();
        assert_eq!(CommentAnchor::canvas(10.0, 20.0, pid).target_id(), pid);

        assert_eq!(
            CommentAnchor::property(lid, "fill.color").target_id(),
            lid
        );
    }

    #[test]
    fn test_comment_store_start_thread() {
        let mut store = CommentStore::new();
        let tid = store.start_thread(
            CommentAnchor::layer(layer_id()),
            alice(),
            "Alice",
            "Hello",
            1000,
        );
        assert_eq!(store.thread_count(), 1);
        assert_eq!(store.total_comments(), 1);
        assert!(store.get_thread(tid).is_some());
    }

    #[test]
    fn test_comment_store_threads_for_target() {
        let mut store = CommentStore::new();
        let lid = layer_id();
        store.start_thread(CommentAnchor::layer(lid), alice(), "Alice", "A", 1000);
        store.start_thread(CommentAnchor::layer(lid), bob(), "Bob", "B", 1001);
        store.start_thread(
            CommentAnchor::component(Uuid::new_v4()),
            alice(),
            "Alice",
            "C",
            1002,
        );

        let threads = store.threads_for_target(lid);
        assert_eq!(threads.len(), 2);
    }

    #[test]
    fn test_comment_store_open_resolved() {
        let mut store = CommentStore::new();
        let _t1 = store.start_thread(
            CommentAnchor::layer(layer_id()),
            alice(),
            "Alice",
            "Open",
            1000,
        );
        let _t2 = store.start_thread(
            CommentAnchor::layer(layer_id()),
            bob(),
            "Bob",
            "Resolved",
            1001,
        );
        store.get_thread_mut(_t2).unwrap().resolve(alice(), 1002);

        assert_eq!(store.open_threads().len(), 1);
        assert_eq!(store.resolved_threads().len(), 1);
    }

    #[test]
    fn test_comment_store_search() {
        let mut store = CommentStore::new();
        store.start_thread(
            CommentAnchor::layer(layer_id()),
            alice(),
            "Alice",
            "Fix the padding issue",
            1000,
        );
        store.start_thread(
            CommentAnchor::layer(layer_id()),
            bob(),
            "Bob",
            "Color looks good",
            1001,
        );

        let results = store.search("padding");
        assert_eq!(results.len(), 1);

        let results = store.search("PADDING"); // case-insensitive
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_comment_store_remove_thread() {
        let mut store = CommentStore::new();
        let tid = store.start_thread(
            CommentAnchor::layer(layer_id()),
            alice(),
            "Alice",
            "Remove me",
            1000,
        );
        let removed = store.remove_thread(tid);
        assert!(removed.is_some());
        assert_eq!(store.thread_count(), 0);
    }

    #[test]
    fn test_comment_serde_roundtrip() {
        let mut store = CommentStore::new();
        let tid = store.start_thread(
            CommentAnchor::canvas(10.0, 20.0, Uuid::new_v4()),
            alice(),
            "Alice",
            "Canvas comment",
            1000,
        );
        store.get_thread_mut(tid).unwrap().reply(bob(), "Bob", "Reply", 1001);

        let json = serde_json::to_string(&store).unwrap();
        let back: CommentStore = serde_json::from_str(&json).unwrap();
        assert_eq!(back.thread_count(), 1);
        assert_eq!(back.total_comments(), 2);
    }

    #[test]
    fn test_resolution_state_default() {
        assert_eq!(ResolutionState::default(), ResolutionState::Open);
    }
}
