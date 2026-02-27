//! Core data model for comments and threads.
//!
//! Enhanced from the base `logos-sync::comment` types with:
//! - Mention tracking within comment content
//! - Richer anchor types (Region for area selection)
//! - Assignment tracking on threads
//! - Priority levels

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::mention::{Mention, parse_mentions};

// ── Identifiers ──────────────────────────────────────────────────────

/// Unique identifier for a comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CommentId(pub Uuid);

impl CommentId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }
}

impl Default for CommentId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for CommentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "comment:{}", &self.0.to_string()[..8])
    }
}

/// Unique identifier for a comment thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ThreadId(pub Uuid);

impl ThreadId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }
}

impl Default for ThreadId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ThreadId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "thread:{}", &self.0.to_string()[..8])
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
    /// Anchored to a canvas position (point pin).
    Canvas { x: f64, y: f64, page_id: Uuid },
    /// Anchored to a rectangular region on the canvas.
    Region {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        page_id: Uuid,
    },
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

    pub fn region(x: f64, y: f64, w: f64, h: f64, page_id: Uuid) -> Self {
        Self::Region {
            x,
            y,
            width: w,
            height: h,
            page_id,
        }
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
            Self::Region { page_id, .. } => *page_id,
            Self::Property { layer_id, .. } => *layer_id,
        }
    }

    /// Returns the page_id if this anchor is page-specific.
    pub fn page_id(&self) -> Option<Uuid> {
        match self {
            Self::Canvas { page_id, .. } | Self::Region { page_id, .. } => Some(*page_id),
            _ => None,
        }
    }

    /// Check if a point (x, y) is within this anchor's spatial area.
    /// For point anchors, uses a proximity radius. For regions, checks containment.
    pub fn contains_point(&self, px: f64, py: f64, radius: f64) -> bool {
        match self {
            Self::Canvas { x, y, .. } => {
                let dx = px - x;
                let dy = py - y;
                (dx * dx + dy * dy) <= radius * radius
            }
            Self::Region {
                x,
                y,
                width,
                height,
                ..
            } => px >= *x && px <= x + width && py >= *y && py <= y + height,
            _ => false,
        }
    }
}

// ── Reaction ─────────────────────────────────────────────────────────

/// A reaction (emoji) on a comment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommentReaction {
    pub emoji: String,
    pub user_id: Uuid,
    pub user_name: String,
    pub timestamp: u64,
}

impl CommentReaction {
    pub fn new(
        emoji: impl Into<String>,
        user_id: Uuid,
        user_name: impl Into<String>,
        timestamp: u64,
    ) -> Self {
        Self {
            emoji: emoji.into(),
            user_id,
            user_name: user_name.into(),
            timestamp,
        }
    }
}

// ── Resolution State ─────────────────────────────────────────────────

/// The resolution state of a comment thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResolutionState {
    Open,
    Resolved,
    WontFix,
    Duplicate,
}

impl Default for ResolutionState {
    fn default() -> Self {
        Self::Open
    }
}

impl ResolutionState {
    /// Whether this state is considered "closed".
    pub fn is_closed(&self) -> bool {
        !matches!(self, Self::Open)
    }

    /// Display label for the resolution state.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::Resolved => "Resolved",
            Self::WontFix => "Won't Fix",
            Self::Duplicate => "Duplicate",
        }
    }
}

// ── Priority ─────────────────────────────────────────────────────────

/// Priority level for a comment thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Priority {
    Low,
    Normal,
    High,
    Urgent,
}

impl Default for Priority {
    fn default() -> Self {
        Self::Normal
    }
}

// ── Comment ──────────────────────────────────────────────────────────

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
    /// Previous versions of the content (timestamp, old_content).
    pub edit_history: Vec<(u64, String)>,
    pub deleted: bool,
    /// Parsed @mentions in this comment.
    pub mentions: Vec<Mention>,
}

impl Comment {
    pub fn new(
        author_id: Uuid,
        author_name: impl Into<String>,
        content: impl Into<String>,
        timestamp: u64,
    ) -> Self {
        let content = content.into();
        let mentions = parse_mentions(&content);
        Self {
            id: CommentId::new(),
            author_id,
            author_name: author_name.into(),
            content,
            created_at: timestamp,
            edited_at: None,
            reactions: Vec::new(),
            edit_history: Vec::new(),
            deleted: false,
            mentions,
        }
    }

    /// Create with a specific ID (for deserialization or sync).
    pub fn with_id(
        id: CommentId,
        author_id: Uuid,
        author_name: impl Into<String>,
        content: impl Into<String>,
        timestamp: u64,
    ) -> Self {
        let content = content.into();
        let mentions = parse_mentions(&content);
        Self {
            id,
            author_id,
            author_name: author_name.into(),
            content,
            created_at: timestamp,
            edited_at: None,
            reactions: Vec::new(),
            edit_history: Vec::new(),
            deleted: false,
            mentions,
        }
    }

    /// Edit the comment contents, preserving history and re-parsing mentions.
    pub fn edit(&mut self, new_content: impl Into<String>, timestamp: u64) {
        self.edit_history
            .push((self.edited_at.unwrap_or(self.created_at), self.content.clone()));
        self.content = new_content.into();
        self.mentions = parse_mentions(&self.content);
        self.edited_at = Some(timestamp);
    }

    /// Soft-delete the comment.
    pub fn delete(&mut self) {
        self.deleted = true;
        self.content = "[deleted]".into();
        self.mentions.clear();
    }

    /// Add a reaction (prevents duplicate emoji+user).
    pub fn add_reaction(
        &mut self,
        emoji: impl Into<String>,
        user_id: Uuid,
        user_name: impl Into<String>,
        timestamp: u64,
    ) {
        let emoji = emoji.into();
        if !self
            .reactions
            .iter()
            .any(|r| r.user_id == user_id && r.emoji == emoji)
        {
            self.reactions.push(CommentReaction::new(
                emoji,
                user_id,
                user_name,
                timestamp,
            ));
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

    /// All unique users who reacted to this comment.
    pub fn reacting_users(&self) -> Vec<Uuid> {
        let mut users: Vec<Uuid> = self.reactions.iter().map(|r| r.user_id).collect();
        users.sort();
        users.dedup();
        users
    }

    /// All mentioned user IDs.
    pub fn mentioned_user_ids(&self) -> Vec<Uuid> {
        self.mentions
            .iter()
            .filter_map(|m| m.user_id)
            .collect()
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

    pub fn has_mentions(&self) -> bool {
        !self.mentions.is_empty()
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
    pub updated_at: u64,
    pub participants: Vec<Uuid>,
    pub priority: Priority,
    /// User assigned to address this thread (reviewer/assignee).
    pub assignee: Option<Uuid>,
    /// Tags for categorization.
    pub tags: Vec<String>,
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
            updated_at: timestamp,
            participants: Vec::new(),
            priority: Priority::default(),
            assignee: None,
            tags: Vec::new(),
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
        thread.add_comment(Comment::new(author_id, author_name, content, timestamp));
        thread
    }

    /// Add a comment to this thread.
    pub fn add_comment(&mut self, comment: Comment) {
        if !self.participants.contains(&comment.author_id) {
            self.participants.push(comment.author_id);
        }
        self.updated_at = comment.created_at;
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
        self.updated_at = timestamp;
    }

    /// Reopen the thread.
    pub fn reopen(&mut self, timestamp: u64) {
        self.resolution = ResolutionState::Open;
        self.resolved_by = None;
        self.resolved_at = None;
        self.updated_at = timestamp;
    }

    /// Mark as won't fix.
    pub fn wont_fix(&mut self, user_id: Uuid, timestamp: u64) {
        self.resolution = ResolutionState::WontFix;
        self.resolved_by = Some(user_id);
        self.resolved_at = Some(timestamp);
        self.updated_at = timestamp;
    }

    /// Mark as duplicate.
    pub fn duplicate(&mut self, user_id: Uuid, timestamp: u64) {
        self.resolution = ResolutionState::Duplicate;
        self.resolved_by = Some(user_id);
        self.resolved_at = Some(timestamp);
        self.updated_at = timestamp;
    }

    /// Set priority.
    pub fn set_priority(&mut self, priority: Priority, timestamp: u64) {
        self.priority = priority;
        self.updated_at = timestamp;
    }

    /// Assign this thread to a user.
    pub fn assign(&mut self, user_id: Uuid, timestamp: u64) {
        self.assignee = Some(user_id);
        self.updated_at = timestamp;
    }

    /// Unassign.
    pub fn unassign(&mut self, timestamp: u64) {
        self.assignee = None;
        self.updated_at = timestamp;
    }

    /// Add a tag.
    pub fn add_tag(&mut self, tag: impl Into<String>, timestamp: u64) {
        let tag = tag.into();
        if !self.tags.contains(&tag) {
            self.tags.push(tag);
            self.updated_at = timestamp;
        }
    }

    /// Remove a tag.
    pub fn remove_tag(&mut self, tag: &str, timestamp: u64) {
        let before = self.tags.len();
        self.tags.retain(|t| t != tag);
        if self.tags.len() != before {
            self.updated_at = timestamp;
        }
    }

    pub fn is_resolved(&self) -> bool {
        self.resolution.is_closed()
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

    /// All unique mentioned users across all comments in this thread.
    pub fn all_mentioned_users(&self) -> Vec<Uuid> {
        let mut ids: Vec<Uuid> = self
            .comments
            .iter()
            .flat_map(|c| c.mentioned_user_ids())
            .collect();
        ids.sort();
        ids.dedup();
        ids
    }
}

// ── Comment Store ────────────────────────────────────────────────────

/// In-memory store for all comment threads in a document.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommentStore {
    threads: HashMap<ThreadId, CommentThread>,
}

impl CommentStore {
    pub fn new() -> Self {
        Self {
            threads: HashMap::new(),
        }
    }

    /// Insert a thread directly.
    pub fn insert_thread(&mut self, thread: CommentThread) {
        self.threads.insert(thread.id, thread);
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

    pub fn get_thread(&self, id: ThreadId) -> Option<&CommentThread> {
        self.threads.get(&id)
    }

    pub fn get_thread_mut(&mut self, id: ThreadId) -> Option<&mut CommentThread> {
        self.threads.get_mut(&id)
    }

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

    /// Find threads on a specific page (canvas or region anchors).
    pub fn threads_on_page(&self, page_id: Uuid) -> Vec<&CommentThread> {
        self.threads
            .values()
            .filter(|t| t.anchor.page_id() == Some(page_id))
            .collect()
    }

    /// Find open threads.
    pub fn open_threads(&self) -> Vec<&CommentThread> {
        self.threads
            .values()
            .filter(|t| !t.is_resolved())
            .collect()
    }

    /// Find resolved threads.
    pub fn resolved_threads(&self) -> Vec<&CommentThread> {
        self.threads
            .values()
            .filter(|t| t.is_resolved())
            .collect()
    }

    /// Find threads assigned to a specific user.
    pub fn threads_assigned_to(&self, user_id: Uuid) -> Vec<&CommentThread> {
        self.threads
            .values()
            .filter(|t| t.assignee == Some(user_id))
            .collect()
    }

    /// Find threads with a specific tag.
    pub fn threads_with_tag(&self, tag: &str) -> Vec<&CommentThread> {
        self.threads
            .values()
            .filter(|t| t.tags.iter().any(|t| t == tag))
            .collect()
    }

    /// Search comments by content substring (case-insensitive).
    pub fn search(&self, query: &str) -> Vec<(ThreadId, CommentId)> {
        let query_lower = query.to_lowercase();
        let mut results = Vec::new();
        for (_, thread) in &self.threads {
            for comment in &thread.comments {
                if !comment.deleted && comment.content.to_lowercase().contains(&query_lower) {
                    results.push((thread.id, comment.id));
                }
            }
        }
        results
    }

    pub fn thread_count(&self) -> usize {
        self.threads.len()
    }

    pub fn total_comments(&self) -> usize {
        self.threads.values().map(|t| t.comment_count()).sum()
    }

    /// All threads, sorted by most recently updated first.
    pub fn threads_by_recency(&self) -> Vec<&CommentThread> {
        let mut threads: Vec<_> = self.threads.values().collect();
        threads.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        threads
    }

    /// Iterate over all threads.
    pub fn iter(&self) -> impl Iterator<Item = &CommentThread> {
        self.threads.values()
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

    fn layer_id() -> Uuid {
        Uuid::from_bytes([10; 16])
    }

    fn page_id() -> Uuid {
        Uuid::from_bytes([20; 16])
    }

    // --- Comment ---

    #[test]
    fn comment_creation() {
        let c = Comment::new(alice(), "Alice", "Hello world", 1000);
        assert_eq!(c.author_name, "Alice");
        assert_eq!(c.content, "Hello world");
        assert_eq!(c.created_at, 1000);
        assert!(!c.is_edited());
        assert!(!c.is_deleted());
        assert!(!c.has_mentions());
    }

    #[test]
    fn comment_with_mention() {
        let c = Comment::new(alice(), "Alice", "Hey @bob check this", 1000);
        assert!(c.has_mentions());
        assert_eq!(c.mentions.len(), 1);
        assert_eq!(c.mentions[0].username, "bob");
    }

    #[test]
    fn comment_edit_reparses_mentions() {
        let mut c = Comment::new(alice(), "Alice", "No mentions", 1000);
        assert!(!c.has_mentions());

        c.edit("Updated @carol please review", 1001);
        assert!(c.has_mentions());
        assert_eq!(c.mentions[0].username, "carol");
        assert!(c.is_edited());
        assert_eq!(c.edit_count(), 1);
    }

    #[test]
    fn comment_delete_clears_mentions() {
        let mut c = Comment::new(alice(), "Alice", "@bob urgent", 1000);
        assert!(c.has_mentions());
        c.delete();
        assert!(c.is_deleted());
        assert!(!c.has_mentions());
    }

    #[test]
    fn comment_reactions() {
        let mut c = Comment::new(alice(), "Alice", "Good work!", 1000);
        c.add_reaction("👍", bob(), "Bob", 1001);
        c.add_reaction("❤️", carol(), "Carol", 1002);
        c.add_reaction("👍", carol(), "Carol", 1003);
        assert_eq!(c.reactions.len(), 3);

        let counts = c.reaction_counts();
        assert_eq!(counts.get("👍"), Some(&2));
        assert_eq!(counts.get("❤️"), Some(&1));

        let users = c.reacting_users();
        assert_eq!(users.len(), 2);
    }

    #[test]
    fn comment_duplicate_reaction_prevented() {
        let mut c = Comment::new(alice(), "Alice", "Ok", 1000);
        c.add_reaction("👍", bob(), "Bob", 1001);
        c.add_reaction("👍", bob(), "Bob", 1002);
        assert_eq!(c.reactions.len(), 1);
    }

    // --- Thread ---

    #[test]
    fn thread_start_with_reply() {
        let mut t = CommentThread::start(
            CommentAnchor::layer(layer_id()),
            alice(),
            "Alice",
            "Fix padding",
            1000,
        );
        assert_eq!(t.comment_count(), 1);
        assert_eq!(t.participant_count(), 1);

        t.reply(bob(), "Bob", "On it!", 1001);
        assert_eq!(t.comment_count(), 2);
        assert_eq!(t.participant_count(), 2);
        assert_eq!(t.updated_at, 1001);
    }

    #[test]
    fn thread_resolve_reopen_cycle() {
        let mut t = CommentThread::start(
            CommentAnchor::layer(layer_id()),
            alice(),
            "Alice",
            "Bug",
            1000,
        );
        t.resolve(bob(), 1001);
        assert!(t.is_resolved());
        assert_eq!(t.resolution, ResolutionState::Resolved);

        t.reopen(1002);
        assert!(!t.is_resolved());

        t.wont_fix(carol(), 1003);
        assert!(t.is_resolved());
        assert_eq!(t.resolution, ResolutionState::WontFix);
    }

    #[test]
    fn thread_priority_and_assignment() {
        let mut t = CommentThread::start(
            CommentAnchor::canvas(50.0, 60.0, page_id()),
            alice(),
            "Alice",
            "Urgent fix needed",
            1000,
        );
        t.set_priority(Priority::Urgent, 1001);
        t.assign(bob(), 1001);
        assert_eq!(t.priority, Priority::Urgent);
        assert_eq!(t.assignee, Some(bob()));

        t.unassign(1002);
        assert_eq!(t.assignee, None);
    }

    #[test]
    fn thread_tags() {
        let mut t = CommentThread::start(
            CommentAnchor::layer(layer_id()),
            alice(),
            "Alice",
            "Design review",
            1000,
        );
        t.add_tag("design", 1001);
        t.add_tag("review", 1001);
        t.add_tag("design", 1001); // duplicate
        assert_eq!(t.tags.len(), 2);

        t.remove_tag("design", 1002);
        assert_eq!(t.tags, vec!["review"]);
    }

    #[test]
    fn thread_mentioned_users() {
        let mut t = CommentThread::start(
            CommentAnchor::layer(layer_id()),
            alice(),
            "Alice",
            "Hey @bob take a look",
            1000,
        );
        t.reply(bob(), "Bob", "Sure @carol can help", 1001);
        let mentioned = t.all_mentioned_users();
        // mentions are unresolved (no user_id), so this will be empty unless resolved
        assert_eq!(mentioned.len(), 0);
    }

    // --- Anchor ---

    #[test]
    fn anchor_target_ids() {
        let lid = Uuid::new_v4();
        assert_eq!(CommentAnchor::layer(lid).target_id(), lid);
        assert_eq!(CommentAnchor::component(lid).target_id(), lid);
        assert_eq!(CommentAnchor::instance(lid).target_id(), lid);

        let pid = Uuid::new_v4();
        assert_eq!(CommentAnchor::canvas(1.0, 2.0, pid).target_id(), pid);
        assert_eq!(
            CommentAnchor::region(0.0, 0.0, 100.0, 100.0, pid).target_id(),
            pid
        );
    }

    #[test]
    fn anchor_spatial_containment() {
        let a = CommentAnchor::canvas(50.0, 50.0, page_id());
        assert!(a.contains_point(50.0, 50.0, 10.0)); // exact match
        assert!(a.contains_point(55.0, 50.0, 10.0)); // within radius
        assert!(!a.contains_point(100.0, 100.0, 10.0)); // too far

        let r = CommentAnchor::region(10.0, 10.0, 100.0, 50.0, page_id());
        assert!(r.contains_point(50.0, 30.0, 0.0)); // inside
        assert!(!r.contains_point(200.0, 200.0, 0.0)); // outside
    }

    // --- Store ---

    #[test]
    fn store_crud() {
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

        store.get_thread_mut(tid).unwrap().reply(bob(), "Bob", "Hi", 1001);
        assert_eq!(store.total_comments(), 2);

        store.remove_thread(tid);
        assert_eq!(store.thread_count(), 0);
    }

    #[test]
    fn store_page_query() {
        let mut store = CommentStore::new();
        let pid = page_id();
        store.start_thread(
            CommentAnchor::canvas(10.0, 20.0, pid),
            alice(),
            "Alice",
            "Pin A",
            1000,
        );
        store.start_thread(
            CommentAnchor::region(0.0, 0.0, 100.0, 100.0, pid),
            bob(),
            "Bob",
            "Area B",
            1001,
        );
        store.start_thread(
            CommentAnchor::layer(layer_id()),
            carol(),
            "Carol",
            "Layer C",
            1002,
        );

        let on_page = store.threads_on_page(pid);
        assert_eq!(on_page.len(), 2);
    }

    #[test]
    fn store_search() {
        let mut store = CommentStore::new();
        store.start_thread(
            CommentAnchor::layer(layer_id()),
            alice(),
            "Alice",
            "Fix padding issue",
            1000,
        );
        store.start_thread(
            CommentAnchor::layer(layer_id()),
            bob(),
            "Bob",
            "Color looks great",
            1001,
        );

        assert_eq!(store.search("padding").len(), 1);
        assert_eq!(store.search("PADDING").len(), 1); // case-insensitive
        assert_eq!(store.search("looks").len(), 1);
        assert_eq!(store.search("xyz").len(), 0);
    }

    #[test]
    fn store_assignment_query() {
        let mut store = CommentStore::new();
        let tid = store.start_thread(
            CommentAnchor::layer(layer_id()),
            alice(),
            "Alice",
            "Review",
            1000,
        );
        store.get_thread_mut(tid).unwrap().assign(bob(), 1001);

        assert_eq!(store.threads_assigned_to(bob()).len(), 1);
        assert_eq!(store.threads_assigned_to(carol()).len(), 0);
    }

    #[test]
    fn store_tag_query() {
        let mut store = CommentStore::new();
        let tid = store.start_thread(
            CommentAnchor::layer(layer_id()),
            alice(),
            "Alice",
            "Bug",
            1000,
        );
        store.get_thread_mut(tid).unwrap().add_tag("bug", 1001);
        store.get_thread_mut(tid).unwrap().add_tag("urgent", 1001);

        assert_eq!(store.threads_with_tag("bug").len(), 1);
        assert_eq!(store.threads_with_tag("feature").len(), 0);
    }

    #[test]
    fn store_serde_roundtrip() {
        let mut store = CommentStore::new();
        let tid = store.start_thread(
            CommentAnchor::canvas(10.0, 20.0, page_id()),
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

    // --- Resolution ---

    #[test]
    fn resolution_labels() {
        assert_eq!(ResolutionState::Open.label(), "Open");
        assert!(!ResolutionState::Open.is_closed());
        assert!(ResolutionState::Resolved.is_closed());
        assert!(ResolutionState::WontFix.is_closed());
        assert!(ResolutionState::Duplicate.is_closed());
    }

    // --- Priority ordering ---

    #[test]
    fn priority_ordering() {
        assert!(Priority::Urgent > Priority::High);
        assert!(Priority::High > Priority::Normal);
        assert!(Priority::Normal > Priority::Low);
    }
}
