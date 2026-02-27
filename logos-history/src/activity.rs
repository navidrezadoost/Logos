//! Activity sessions — group raw operations into user-meaningful changes.
//!
//! Instead of showing 47 individual shape-move operations, the activity
//! feed groups them into a single "Alice rearranged the layout" session
//! based on temporal proximity and authorship.

use logos_replay::HistoryEntry;
use serde::{Deserialize, Serialize};

/// A session groups consecutive operations by the same user within a
/// configurable time window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivitySession {
    /// User who performed these operations.
    pub user_id: String,
    /// Domain (e.g., "design", "comment").
    pub domain: String,
    /// First op version in this session.
    pub start_version: u64,
    /// Last op version in this session.
    pub end_version: u64,
    /// Number of operations in this session.
    pub op_count: usize,
    /// Timestamp of the first operation.
    pub started_at: u64,
    /// Timestamp of the last operation.
    pub ended_at: u64,
    /// Descriptions of operations in this session (if available).
    pub descriptions: Vec<String>,
    /// Human-readable summary.
    pub summary: String,
}

impl ActivitySession {
    /// Duration in seconds.
    pub fn duration(&self) -> u64 {
        self.ended_at.saturating_sub(self.started_at)
    }

    /// Whether this session is a single operation.
    pub fn is_single(&self) -> bool {
        self.op_count == 1
    }
}

/// Configuration for grouping operations into sessions.
#[derive(Debug, Clone)]
pub struct SessionGrouper {
    /// Maximum gap (seconds) between consecutive ops in the same session.
    pub max_gap_secs: u64,
    /// Maximum number of ops per session (forces a split).
    pub max_session_ops: usize,
    /// Whether to split sessions when the domain changes.
    pub split_on_domain_change: bool,
}

impl Default for SessionGrouper {
    fn default() -> Self {
        Self {
            max_gap_secs: 300, // 5 minutes
            max_session_ops: 100,
            split_on_domain_change: true,
        }
    }
}

impl SessionGrouper {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_max_gap(mut self, secs: u64) -> Self {
        self.max_gap_secs = secs;
        self
    }

    pub fn with_max_ops(mut self, max: usize) -> Self {
        self.max_session_ops = max;
        self
    }

    pub fn without_domain_split(mut self) -> Self {
        self.split_on_domain_change = false;
        self
    }

    /// Group a slice of history entries into sessions.
    ///
    /// Entries should be sorted by version ascending.
    pub fn group(&self, entries: &[HistoryEntry]) -> Vec<ActivitySession> {
        if entries.is_empty() {
            return Vec::new();
        }

        let mut sessions = Vec::new();
        let mut current_user = &entries[0].user_id;
        let mut current_domain = &entries[0].domain;
        let mut start_idx = 0;
        let mut last_ts = entries[0].timestamp;

        for i in 1..entries.len() {
            let entry = &entries[i];
            let user_changed = entry.user_id != *current_user;
            let domain_changed =
                self.split_on_domain_change && entry.domain != *current_domain;
            let gap_exceeded =
                entry.timestamp.saturating_sub(last_ts) > self.max_gap_secs;
            let ops_exceeded = (i - start_idx) >= self.max_session_ops;

            if user_changed || domain_changed || gap_exceeded || ops_exceeded {
                sessions.push(self.build_session(&entries[start_idx..i]));
                start_idx = i;
                current_user = &entry.user_id;
                current_domain = &entry.domain;
            }
            last_ts = entry.timestamp;
        }

        // Final session.
        sessions.push(self.build_session(&entries[start_idx..]));
        sessions
    }

    fn build_session(&self, entries: &[HistoryEntry]) -> ActivitySession {
        let first = &entries[0];
        let last = entries.last().unwrap();
        let descriptions: Vec<String> = entries
            .iter()
            .filter_map(|e| e.description.clone())
            .collect();

        let summary = if entries.len() == 1 {
            descriptions
                .first()
                .cloned()
                .unwrap_or_else(|| format!("{} made a change", first.user_id))
        } else {
            format!(
                "{} made {} changes in {}",
                first.user_id,
                entries.len(),
                first.domain,
            )
        };

        ActivitySession {
            user_id: first.user_id.clone(),
            domain: first.domain.clone(),
            start_version: first.version,
            end_version: last.version,
            op_count: entries.len(),
            started_at: first.timestamp,
            ended_at: last.timestamp,
            descriptions,
            summary,
        }
    }
}

/// Paginated activity feed.
#[derive(Debug, Clone)]
pub struct ActivityFeed {
    sessions: Vec<ActivitySession>,
}

impl ActivityFeed {
    /// Create an activity feed from history entries using the given grouper.
    pub fn from_entries(entries: &[HistoryEntry], grouper: &SessionGrouper) -> Self {
        let sessions = grouper.group(entries);
        Self { sessions }
    }

    /// Total number of sessions.
    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    /// Get a page of sessions (0-indexed).
    pub fn page(&self, page: usize, page_size: usize) -> Vec<&ActivitySession> {
        self.sessions
            .iter()
            .skip(page * page_size)
            .take(page_size)
            .collect()
    }

    /// All sessions.
    pub fn all(&self) -> &[ActivitySession] {
        &self.sessions
    }

    /// Sessions by a specific user.
    pub fn by_user(&self, user_id: &str) -> Vec<&ActivitySession> {
        self.sessions
            .iter()
            .filter(|s| s.user_id == user_id)
            .collect()
    }

    /// Total operations across all sessions.
    pub fn total_ops(&self) -> usize {
        self.sessions.iter().map(|s| s.op_count).sum()
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(version: u64, user: &str, domain: &str, ts: u64) -> HistoryEntry {
        HistoryEntry {
            version,
            user_id: user.to_string(),
            timestamp: ts,
            domain: domain.to_string(),
            description: Some(format!("Op {}", version)),
            acknowledged: true,
        }
    }

    fn sample_entries() -> Vec<HistoryEntry> {
        vec![
            // Alice's design session (within 5-min window)
            make_entry(1, "alice", "design", 1000),
            make_entry(2, "alice", "design", 1060),
            make_entry(3, "alice", "design", 1120),
            // Bob interrupts
            make_entry(4, "bob", "comment", 1200),
            // Alice continues design (new session, user changed back)
            make_entry(5, "alice", "design", 1250),
            make_entry(6, "alice", "design", 1310),
            // Big gap — forces new session
            make_entry(7, "alice", "design", 2000),
            // Alice switches domain
            make_entry(8, "alice", "comment", 2060),
        ]
    }

    #[test]
    fn session_grouping_basic() {
        let grouper = SessionGrouper::new();
        let sessions = grouper.group(&sample_entries());
        // Expected sessions:
        // 1: alice, design, v1-v3
        // 2: bob, comment, v4
        // 3: alice, design, v5-v6
        // 4: alice, design, v7 (gap > 300s from v6@1310 to v7@2000)
        // 5: alice, comment, v8 (domain change)
        assert_eq!(sessions.len(), 5);
    }

    #[test]
    fn session_user_split() {
        let grouper = SessionGrouper::new();
        let sessions = grouper.group(&sample_entries());
        assert_eq!(sessions[0].user_id, "alice");
        assert_eq!(sessions[0].op_count, 3);
        assert_eq!(sessions[1].user_id, "bob");
        assert_eq!(sessions[1].op_count, 1);
    }

    #[test]
    fn session_version_range() {
        let grouper = SessionGrouper::new();
        let sessions = grouper.group(&sample_entries());
        assert_eq!(sessions[0].start_version, 1);
        assert_eq!(sessions[0].end_version, 3);
    }

    #[test]
    fn session_duration() {
        let grouper = SessionGrouper::new();
        let sessions = grouper.group(&sample_entries());
        assert_eq!(sessions[0].duration(), 120); // 1120 - 1000
        assert_eq!(sessions[1].duration(), 0); // single op
    }

    #[test]
    fn session_is_single() {
        let grouper = SessionGrouper::new();
        let sessions = grouper.group(&sample_entries());
        assert!(!sessions[0].is_single());
        assert!(sessions[1].is_single());
    }

    #[test]
    fn session_summary() {
        let grouper = SessionGrouper::new();
        let sessions = grouper.group(&sample_entries());
        assert!(sessions[0].summary.contains("3 changes"));
        assert_eq!(sessions[1].summary, "Op 4"); // single → description
    }

    #[test]
    fn session_descriptions_collected() {
        let grouper = SessionGrouper::new();
        let sessions = grouper.group(&sample_entries());
        assert_eq!(sessions[0].descriptions.len(), 3);
        assert_eq!(sessions[0].descriptions[0], "Op 1");
    }

    #[test]
    fn custom_gap() {
        let grouper = SessionGrouper::new().with_max_gap(10000);
        let sessions = grouper.group(&sample_entries());
        // With 10000s gap, alice sessions won't split on time.
        // Splits: user change (bob), domain change (comment)
        assert_eq!(sessions.len(), 4);
    }

    #[test]
    fn no_domain_split() {
        let grouper = SessionGrouper::new().without_domain_split();
        let sessions = grouper.group(&sample_entries());
        // Without domain split: alice design+comment merge at end
        assert_eq!(sessions.len(), 4);
    }

    #[test]
    fn max_ops_split() {
        let grouper = SessionGrouper::new().with_max_ops(2);
        let entries = vec![
            make_entry(1, "alice", "design", 1000),
            make_entry(2, "alice", "design", 1010),
            make_entry(3, "alice", "design", 1020),
            make_entry(4, "alice", "design", 1030),
        ];
        let sessions = grouper.group(&entries);
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].op_count, 2);
        assert_eq!(sessions[1].op_count, 2);
    }

    #[test]
    fn empty_entries() {
        let grouper = SessionGrouper::new();
        let sessions = grouper.group(&[]);
        assert!(sessions.is_empty());
    }

    #[test]
    fn activity_feed_creation() {
        let feed = ActivityFeed::from_entries(&sample_entries(), &SessionGrouper::new());
        assert!(!feed.is_empty());
        assert_eq!(feed.total_ops(), 8);
    }

    #[test]
    fn activity_feed_pagination() {
        let feed = ActivityFeed::from_entries(&sample_entries(), &SessionGrouper::new());
        let page0 = feed.page(0, 2);
        assert_eq!(page0.len(), 2);
        let page1 = feed.page(1, 2);
        assert_eq!(page1.len(), 2);
    }

    #[test]
    fn activity_feed_by_user() {
        let feed = ActivityFeed::from_entries(&sample_entries(), &SessionGrouper::new());
        let alice = feed.by_user("alice");
        assert_eq!(alice.len(), 4); // 4 alice sessions
        let bob = feed.by_user("bob");
        assert_eq!(bob.len(), 1);
    }

    #[test]
    fn session_serde_roundtrip() {
        let grouper = SessionGrouper::new();
        let sessions = grouper.group(&sample_entries());
        let json = serde_json::to_string(&sessions[0]).unwrap();
        let back: ActivitySession = serde_json::from_str(&json).unwrap();
        assert_eq!(back.user_id, sessions[0].user_id);
        assert_eq!(back.op_count, sessions[0].op_count);
    }
}
