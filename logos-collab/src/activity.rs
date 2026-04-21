// logos-collab/src/activity.rs
//
//! # Activity Log
//!
//! Non-blocking append-only activity log for a Logos document.
//!
//! Every user action (layer change, comment, role change, …) produces an
//! [`ActivityEntry`].  Entries are pushed onto a [`tokio::sync::mpsc`] channel
//! and drained by a background writer, keeping the hot path lock-free.
//!
//! ## Retention policy
//! - Entries older than 7 days are eligible for removal.
//! - [`ActivityLog::cleanup_older_than`] removes them from the in-memory store.
//! - Persistent backends (RocksDB / SQLite) should run this nightly.
//!
//! ## Search
//! [`ActivityLog::search`] filters in-memory entries by a [`SearchQuery`].

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use uuid::Uuid;

pub type Timestamp = u64;

fn now_ms() -> Timestamp {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Seven-day retention window in milliseconds.
pub const RETENTION_MS: u64 = 7 * 24 * 60 * 60 * 1_000;

// ── Action kind ───────────────────────────────────────────────────────────────

/// What the user did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ActivityKind {
    // Layer operations
    LayerCreated   { layer_id: Uuid, layer_name: String },
    LayerDeleted   { layer_id: Uuid },
    LayerMoved     { layer_id: Uuid },
    LayerRenamed   { layer_id: Uuid, old_name: String, new_name: String },
    PropertyChanged{ layer_id: Uuid, property: String },
    // Frame operations
    FrameCreated   { frame_id: Uuid },
    FrameDeleted   { frame_id: Uuid },
    // Comment operations
    CommentAdded   { comment_id: Uuid },
    CommentEdited  { comment_id: Uuid },
    CommentDeleted { comment_id: Uuid },
    CommentLiked   { comment_id: Uuid },
    // Role changes
    RoleChanged    { target_user_id: Uuid, new_role: String },
    MemberAdded    { new_user_id: Uuid },
    MemberRemoved  { removed_user_id: Uuid },
    OwnershipTransferred { from: Uuid, to: Uuid },
    // Session events
    UserJoined,
    UserLeft,
    PrototypeRun   { frame_id: Uuid },
    CodeExported   { format: String, layer_id: Uuid },
    /// Catch-all for custom / extension actions.
    Custom         { kind: String, payload: serde_json::Value },
}

impl ActivityKind {
    /// Short human-readable summary.
    pub fn summary(&self) -> String {
        match self {
            ActivityKind::LayerCreated   { layer_name, .. } => format!("Created layer «{layer_name}»"),
            ActivityKind::LayerDeleted   { .. }             => "Deleted a layer".into(),
            ActivityKind::LayerMoved     { .. }             => "Moved a layer".into(),
            ActivityKind::LayerRenamed   { old_name, new_name, .. } =>
                format!("Renamed «{old_name}» to «{new_name}»"),
            ActivityKind::PropertyChanged{ property, .. }   => format!("Changed {property}"),
            ActivityKind::FrameCreated   { .. }             => "Created a frame".into(),
            ActivityKind::FrameDeleted   { .. }             => "Deleted a frame".into(),
            ActivityKind::CommentAdded   { .. }             => "Added a comment".into(),
            ActivityKind::CommentEdited  { .. }             => "Edited a comment".into(),
            ActivityKind::CommentDeleted { .. }             => "Deleted a comment".into(),
            ActivityKind::CommentLiked   { .. }             => "Liked a comment".into(),
            ActivityKind::RoleChanged    { new_role, .. }   => format!("Role changed to {new_role}"),
            ActivityKind::MemberAdded    { .. }             => "New member added".into(),
            ActivityKind::MemberRemoved  { .. }             => "Member removed".into(),
            ActivityKind::OwnershipTransferred { .. }       => "Ownership transferred".into(),
            ActivityKind::UserJoined                        => "Joined the session".into(),
            ActivityKind::UserLeft                          => "Left the session".into(),
            ActivityKind::PrototypeRun   { .. }             => "Ran a prototype".into(),
            ActivityKind::CodeExported   { format, .. }     => format!("Exported {format} code"),
            ActivityKind::Custom         { kind, .. }       => format!("Custom: {kind}"),
        }
    }
}

// ── Entry ─────────────────────────────────────────────────────────────────────

/// A single activity log entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivityEntry {
    /// Unique entry id.
    pub id: Uuid,
    /// Which document this belongs to.
    pub document_id: Uuid,
    /// Who performed the action.
    pub user_id: Uuid,
    /// When (Unix ms).
    pub timestamp: Timestamp,
    /// What happened.
    pub kind: ActivityKind,
}

impl ActivityEntry {
    pub fn new(document_id: Uuid, user_id: Uuid, kind: ActivityKind) -> Self {
        Self {
            id: Uuid::new_v4(),
            document_id,
            user_id,
            timestamp: now_ms(),
            kind,
        }
    }

    /// Override timestamp (for testing / replay).
    pub fn with_timestamp(mut self, ts: Timestamp) -> Self {
        self.timestamp = ts;
        self
    }
}

// ── Search query ──────────────────────────────────────────────────────────────

/// Filter criteria for [`ActivityLog::search`].
#[derive(Debug, Clone, Default)]
pub struct SearchQuery {
    /// Filter to a specific user.
    pub user_id:    Option<Uuid>,
    /// Earliest timestamp (inclusive).
    pub from_ms:    Option<Timestamp>,
    /// Latest timestamp (inclusive).
    pub to_ms:      Option<Timestamp>,
    /// Only return entries whose summary contains this substring (case-insensitive).
    pub text:       Option<String>,
    /// Maximum entries to return (0 = unlimited).
    pub limit:      usize,
    /// Skip this many results (for pagination).
    pub offset:     usize,
}

impl SearchQuery {
    pub fn new() -> Self { Self::default() }

    pub fn by_user(mut self, user_id: Uuid) -> Self { self.user_id = Some(user_id); self }
    pub fn from(mut self, ms: Timestamp)    -> Self { self.from_ms = Some(ms); self }
    pub fn to(mut self, ms: Timestamp)      -> Self { self.to_ms   = Some(ms); self }
    pub fn text(mut self, s: impl Into<String>) -> Self { self.text = Some(s.into()); self }
    pub fn limit(mut self, n: usize)        -> Self { self.limit  = n; self }
    pub fn offset(mut self, n: usize)       -> Self { self.offset = n; self }

    fn matches(&self, e: &ActivityEntry) -> bool {
        if let Some(uid) = self.user_id {
            if e.user_id != uid { return false; }
        }
        if let Some(from) = self.from_ms {
            if e.timestamp < from { return false; }
        }
        if let Some(to) = self.to_ms {
            if e.timestamp > to { return false; }
        }
        if let Some(ref text) = self.text {
            let summary = e.kind.summary().to_lowercase();
            if !summary.contains(&text.to_lowercase()) { return false; }
        }
        true
    }
}

// ── In-memory log ─────────────────────────────────────────────────────────────

/// In-memory activity log.
///
/// Entries are stored in a [`VecDeque`] (oldest first) which supports
/// efficient front-removal during cleanup.
///
/// For production use this wraps a persistent backend; the non-blocking
/// write path uses a `tokio::sync::mpsc` sender — see [`ActivityWriter`].
#[derive(Debug, Default)]
pub struct ActivityLog {
    entries: VecDeque<ActivityEntry>,
    /// Maximum entries to keep in memory (0 = unlimited).
    pub capacity: usize,
}

impl ActivityLog {
    pub fn new() -> Self { Self::default() }

    pub fn with_capacity(capacity: usize) -> Self {
        Self { capacity, ..Default::default() }
    }

    /// Append a new entry (called by the background writer OR synchronously in tests).
    pub fn push(&mut self, entry: ActivityEntry) {
        if self.capacity > 0 && self.entries.len() >= self.capacity {
            self.entries.pop_front(); // evict oldest
        }
        self.entries.push_back(entry);
    }

    /// Remove entries older than `cutoff_ms`.  Returns count removed.
    pub fn cleanup_older_than(&mut self, cutoff_ms: Timestamp) -> usize {
        let before = self.entries.len();
        self.entries.retain(|e| e.timestamp >= cutoff_ms);
        before - self.entries.len()
    }

    /// Apply the 7-day default retention.
    pub fn cleanup_default(&mut self) -> usize {
        let cutoff = now_ms().saturating_sub(RETENTION_MS);
        self.cleanup_older_than(cutoff)
    }

    /// Search / filter entries.  Returns a page of matching entries.
    pub fn search(&self, q: &SearchQuery) -> Vec<&ActivityEntry> {
        let matched: Vec<_> = self.entries.iter()
            .filter(|e| q.matches(e))
            .collect();

        let start = q.offset.min(matched.len());
        let end   = if q.limit == 0 { matched.len() } else { (start + q.limit).min(matched.len()) };
        matched[start..end].to_vec()
    }

    /// Total entries.
    pub fn len(&self) -> usize { self.entries.len() }
    pub fn is_empty(&self) -> bool { self.entries.is_empty() }

    /// Most recent N entries.
    pub fn recent(&self, n: usize) -> Vec<&ActivityEntry> {
        let start = self.entries.len().saturating_sub(n);
        self.entries.range(start..).collect()
    }
}

// ── Async writer bridge ───────────────────────────────────────────────────────

/// Sender half of the non-blocking activity write path.
///
/// Clone this and hand it to every operation context.  Entries are batched
/// and written to the log by a background task.
#[derive(Clone, Debug)]
pub struct ActivityWriter {
    tx: tokio::sync::mpsc::UnboundedSender<ActivityEntry>,
}

impl ActivityWriter {
    /// Create a writer + the receiver that drives the background loop.
    pub fn channel() -> (Self, tokio::sync::mpsc::UnboundedReceiver<ActivityEntry>) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        (Self { tx }, rx)
    }

    /// Fire-and-forget: push an entry without blocking.
    /// Returns `Err` only if the background task has stopped.
    pub fn record(&self, entry: ActivityEntry) -> Result<(), ActivityEntry> {
        self.tx.send(entry).map_err(|e| e.0)
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn uid() -> Uuid { Uuid::new_v4() }

    fn entry(doc: Uuid, user: Uuid) -> ActivityEntry {
        ActivityEntry::new(doc, user, ActivityKind::UserJoined)
    }

    // ── ActivityEntry ─────────────────────────────────────────────

    // A-01: New entry has unique id.
    #[test]
    fn a_01_unique_id() {
        let doc = uid(); let u = uid();
        let e1 = entry(doc, u);
        let e2 = entry(doc, u);
        assert_ne!(e1.id, e2.id);
    }

    // A-02: with_timestamp overrides ts.
    #[test]
    fn a_02_with_timestamp() {
        let e = entry(uid(), uid()).with_timestamp(42);
        assert_eq!(e.timestamp, 42);
    }

    // A-03: summary is non-empty for every kind.
    #[test]
    fn a_03_summary_non_empty() {
        let kinds = vec![
            ActivityKind::UserJoined,
            ActivityKind::UserLeft,
            ActivityKind::LayerCreated { layer_id: uid(), layer_name: "Rect".into() },
            ActivityKind::CodeExported { format: "css".into(), layer_id: uid() },
            ActivityKind::Custom { kind: "test".into(), payload: serde_json::json!(null) },
        ];
        for k in kinds {
            assert!(!k.summary().is_empty(), "{k:?} has empty summary");
        }
    }

    // ── ActivityLog::push ─────────────────────────────────────────

    // A-04: push adds an entry.
    #[test]
    fn a_04_push_adds() {
        let mut log = ActivityLog::new();
        log.push(entry(uid(), uid()));
        assert_eq!(log.len(), 1);
    }

    // A-05: Capacity limit evicts oldest.
    #[test]
    fn a_05_capacity_evicts_oldest() {
        let mut log = ActivityLog::with_capacity(3);
        let doc = uid(); let u = uid();
        let old = entry(doc, u).with_timestamp(1);
        let old_id = old.id;
        log.push(old);
        log.push(entry(doc, u).with_timestamp(2));
        log.push(entry(doc, u).with_timestamp(3));
        log.push(entry(doc, u).with_timestamp(4)); // evicts ts=1
        assert_eq!(log.len(), 3);
        assert!(!log.entries.iter().any(|e| e.id == old_id));
    }

    // ── ActivityLog::cleanup ──────────────────────────────────────

    // A-06: cleanup_older_than removes old entries.
    #[test]
    fn a_06_cleanup_removes_old() {
        let mut log = ActivityLog::new();
        log.push(entry(uid(), uid()).with_timestamp(100));
        log.push(entry(uid(), uid()).with_timestamp(200));
        log.push(entry(uid(), uid()).with_timestamp(300));
        let removed = log.cleanup_older_than(200); // removes ts=100
        assert_eq!(removed, 1);
        assert_eq!(log.len(), 2);
    }

    // A-07: cleanup_older_than keeps all if cutoff is 0.
    #[test]
    fn a_07_cleanup_keeps_all() {
        let mut log = ActivityLog::new();
        log.push(entry(uid(), uid()));
        let removed = log.cleanup_older_than(0);
        assert_eq!(removed, 0);
    }

    // ── ActivityLog::search ───────────────────────────────────────

    // A-08: search with no filter returns all.
    #[test]
    fn a_08_search_no_filter() {
        let mut log = ActivityLog::new();
        for _ in 0..5 { log.push(entry(uid(), uid())); }
        let results = log.search(&SearchQuery::new());
        assert_eq!(results.len(), 5);
    }

    // A-09: search by user filters correctly.
    #[test]
    fn a_09_search_by_user() {
        let mut log = ActivityLog::new();
        let alice = uid(); let bob = uid();
        let doc = uid();
        log.push(entry(doc, alice));
        log.push(entry(doc, alice));
        log.push(entry(doc, bob));
        let q = SearchQuery::new().by_user(alice);
        assert_eq!(log.search(&q).len(), 2);
    }

    // A-10: search by date range.
    #[test]
    fn a_10_search_by_date_range() {
        let mut log = ActivityLog::new();
        let doc = uid(); let u = uid();
        log.push(entry(doc, u).with_timestamp(100));
        log.push(entry(doc, u).with_timestamp(200));
        log.push(entry(doc, u).with_timestamp(300));
        let q = SearchQuery::new().from(150).to(250);
        let r = log.search(&q);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].timestamp, 200);
    }

    // A-11: search text filter (case-insensitive).
    #[test]
    fn a_11_search_text_filter() {
        let mut log = ActivityLog::new();
        let doc = uid(); let u = uid();
        log.push(ActivityEntry::new(doc, u, ActivityKind::LayerCreated { layer_id: uid(), layer_name: "Header".into() }));
        log.push(ActivityEntry::new(doc, u, ActivityKind::UserJoined));
        let q = SearchQuery::new().text("header");
        assert_eq!(log.search(&q).len(), 1);
    }

    // A-12: search limit.
    #[test]
    fn a_12_search_limit() {
        let mut log = ActivityLog::new();
        for _ in 0..10 { log.push(entry(uid(), uid())); }
        let q = SearchQuery::new().limit(3);
        assert_eq!(log.search(&q).len(), 3);
    }

    // A-13: search offset + limit.
    #[test]
    fn a_13_search_offset_limit() {
        let mut log = ActivityLog::new();
        let doc = uid(); let u = uid();
        for ts in 0u64..10 { log.push(entry(doc, u).with_timestamp(ts)); }
        let q = SearchQuery::new().offset(5).limit(3);
        let r = log.search(&q);
        assert_eq!(r.len(), 3);
        assert_eq!(r[0].timestamp, 5);
    }

    // A-14: search offset beyond end returns empty.
    #[test]
    fn a_14_search_offset_beyond_end() {
        let mut log = ActivityLog::new();
        log.push(entry(uid(), uid()));
        let q = SearchQuery::new().offset(100);
        assert!(log.search(&q).is_empty());
    }

    // ── ActivityLog::recent ───────────────────────────────────────

    // A-15: recent(3) returns last 3 entries.
    #[test]
    fn a_15_recent() {
        let mut log = ActivityLog::new();
        let doc = uid(); let u = uid();
        for ts in 0u64..5 { log.push(entry(doc, u).with_timestamp(ts)); }
        let r = log.recent(3);
        assert_eq!(r.len(), 3);
        assert_eq!(r[2].timestamp, 4);
    }

    // A-16: recent(0) returns empty.
    #[test]
    fn a_16_recent_zero() {
        let mut log = ActivityLog::new();
        log.push(entry(uid(), uid()));
        assert!(log.recent(0).is_empty());
    }

    // ── ActivityWriter ────────────────────────────────────────────

    // A-17: Writer channel sends and receiver gets.
    #[tokio::test]
    async fn a_17_writer_channel() {
        let (writer, mut rx) = ActivityWriter::channel();
        let e = entry(uid(), uid());
        let id = e.id;
        writer.record(e).unwrap();
        let received = rx.recv().await.unwrap();
        assert_eq!(received.id, id);
    }

    // A-18: Writer clone also sends.
    #[tokio::test]
    async fn a_18_writer_clone_sends() {
        let (writer, mut rx) = ActivityWriter::channel();
        let w2 = writer.clone();
        w2.record(entry(uid(), uid())).unwrap();
        assert!(rx.recv().await.is_some());
    }

    // A-19: Record returns Err when receiver dropped.
    #[test]
    fn a_19_record_err_when_rx_dropped() {
        let (writer, rx) = ActivityWriter::channel();
        drop(rx);
        let result = writer.record(entry(uid(), uid()));
        assert!(result.is_err());
    }

    // A-20: RETENTION_MS is 7 days.
    #[test]
    fn a_20_retention_constant() {
        assert_eq!(RETENTION_MS, 7 * 24 * 60 * 60 * 1_000);
    }
}
