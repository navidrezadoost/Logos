//! Timeline — paginated, filterable version history for UI rendering.
//!
//! `Timeline` takes raw `HistoryEntry` data from `logos-replay` and
//! transforms it into a UI-ready paginated feed with filtering,
//! grouping, and display metadata.

use logos_replay::HistoryEntry;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Filter criteria for the timeline.
#[derive(Debug, Clone, Default)]
pub struct TimelineFilter {
    /// Only show entries from this user (matched against HistoryEntry.user_id).
    pub user_id: Option<String>,
    /// Only show entries in this domain.
    pub domain: Option<String>,
    /// Only show entries after this timestamp.
    pub after_timestamp: Option<u64>,
    /// Only show entries before this timestamp.
    pub before_timestamp: Option<u64>,
    /// Only show entries in this version range.
    pub version_range: Option<(u64, u64)>,
    /// Text search in description.
    pub search_text: Option<String>,
}

impl TimelineFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_user(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }

    pub fn with_domain(mut self, domain: impl Into<String>) -> Self {
        self.domain = Some(domain.into());
        self
    }

    pub fn with_after(mut self, ts: u64) -> Self {
        self.after_timestamp = Some(ts);
        self
    }

    pub fn with_before(mut self, ts: u64) -> Self {
        self.before_timestamp = Some(ts);
        self
    }

    pub fn with_version_range(mut self, start: u64, end: u64) -> Self {
        self.version_range = Some((start, end));
        self
    }

    pub fn with_search(mut self, text: impl Into<String>) -> Self {
        self.search_text = Some(text.into());
        self
    }

    /// Check whether a history entry matches this filter.
    pub fn matches(&self, entry: &HistoryEntry) -> bool {
        if let Some(ref uid) = self.user_id {
            if entry.user_id != *uid {
                return false;
            }
        }
        if let Some(ref dom) = self.domain {
            if entry.domain != *dom {
                return false;
            }
        }
        if let Some(after) = self.after_timestamp {
            if entry.timestamp < after {
                return false;
            }
        }
        if let Some(before) = self.before_timestamp {
            if entry.timestamp > before {
                return false;
            }
        }
        if let Some((start, end)) = self.version_range {
            if entry.version < start || entry.version > end {
                return false;
            }
        }
        if let Some(ref text) = self.search_text {
            let lower = text.to_lowercase();
            let desc_match = entry
                .description
                .as_ref()
                .map(|d| d.to_lowercase().contains(&lower))
                .unwrap_or(false);
            let domain_match = entry.domain.to_lowercase().contains(&lower);
            if !desc_match && !domain_match {
                return false;
            }
        }
        true
    }
}

/// A single entry in the timeline — enriched for UI display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEntry {
    /// Version number.
    pub version: u64,
    /// User display name or ID.
    pub user_display: String,
    /// User ID (for avatar lookup, etc.).
    pub user_id: String,
    /// Human-readable timestamp (ISO 8601 or relative).
    pub timestamp: u64,
    /// Relative time description (e.g., "2 hours ago").
    pub relative_time: String,
    /// Domain tag.
    pub domain: String,
    /// Operation description.
    pub description: String,
    /// Whether this version has a bookmark.
    pub is_bookmarked: bool,
    /// Bookmark name if bookmarked.
    pub bookmark_name: Option<String>,
    /// Whether this is the current (latest) version.
    pub is_current: bool,
    /// Whether a snapshot exists at this version (fast restore).
    pub has_snapshot: bool,
}

impl TimelineEntry {
    /// Create from a raw `HistoryEntry`.
    pub fn from_history_entry(
        entry: &HistoryEntry,
        current_time: u64,
        latest_version: u64,
    ) -> Self {
        Self {
            version: entry.version,
            user_display: entry.user_id.clone(),
            user_id: entry.user_id.clone(),
            timestamp: entry.timestamp,
            relative_time: format_relative_time(entry.timestamp, current_time),
            domain: entry.domain.clone(),
            description: entry
                .description
                .clone()
                .unwrap_or_else(|| format!("{} operation", entry.domain)),
            is_bookmarked: false,
            bookmark_name: None,
            is_current: entry.version == latest_version,
            has_snapshot: false,
        }
    }
}

/// A page of timeline entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelinePage {
    /// The entries on this page.
    pub entries: Vec<TimelineEntry>,
    /// Current page number (0-indexed).
    pub page: usize,
    /// Number of entries per page.
    pub page_size: usize,
    /// Total number of entries matching the filter.
    pub total_entries: usize,
    /// Total number of pages.
    pub total_pages: usize,
    /// Whether there is a next page.
    pub has_next: bool,
    /// Whether there is a previous page.
    pub has_prev: bool,
}

/// The timeline — transforms raw history into UI-ready pages.
pub struct Timeline {
    entries: Vec<HistoryEntry>,
    latest_version: u64,
    document_id: Uuid,
}

impl Timeline {
    /// Create a timeline from raw history entries.
    pub fn new(entries: Vec<HistoryEntry>, document_id: Uuid) -> Self {
        let latest_version = entries.iter().map(|e| e.version).max().unwrap_or(0);
        Self {
            entries,
            latest_version,
            document_id,
        }
    }

    /// Total entry count.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Document ID.
    pub fn document_id(&self) -> &Uuid {
        &self.document_id
    }

    /// Get a paginated, filtered view of the timeline.
    pub fn page(
        &self,
        page: usize,
        page_size: usize,
        filter: &TimelineFilter,
        current_time: u64,
    ) -> TimelinePage {
        let filtered: Vec<_> = self
            .entries
            .iter()
            .filter(|e| filter.matches(e))
            .collect();

        let total_entries = filtered.len();
        let total_pages = if total_entries == 0 {
            0
        } else {
            (total_entries + page_size - 1) / page_size
        };

        let start = page * page_size;
        let entries: Vec<TimelineEntry> = filtered
            .iter()
            .rev() // Most recent first
            .skip(start)
            .take(page_size)
            .map(|e| TimelineEntry::from_history_entry(e, current_time, self.latest_version))
            .collect();

        TimelinePage {
            entries,
            page,
            page_size,
            total_entries,
            total_pages,
            has_next: page + 1 < total_pages,
            has_prev: page > 0,
        }
    }

    /// Get all entries as timeline entries (no pagination).
    pub fn all_entries(&self, current_time: u64) -> Vec<TimelineEntry> {
        self.entries
            .iter()
            .rev()
            .map(|e| TimelineEntry::from_history_entry(e, current_time, self.latest_version))
            .collect()
    }

    /// Search entries by description text.
    pub fn search(&self, query: &str, current_time: u64) -> Vec<TimelineEntry> {
        let filter = TimelineFilter::new().with_search(query);
        self.entries
            .iter()
            .filter(|e| filter.matches(e))
            .rev()
            .map(|e| TimelineEntry::from_history_entry(e, current_time, self.latest_version))
            .collect()
    }

    /// Get entries grouped by user.
    pub fn by_user(&self, current_time: u64) -> std::collections::HashMap<String, Vec<TimelineEntry>> {
        let mut grouped: std::collections::HashMap<String, Vec<TimelineEntry>> =
            std::collections::HashMap::new();
        for entry in &self.entries {
            let te = TimelineEntry::from_history_entry(entry, current_time, self.latest_version);
            grouped
                .entry(entry.user_id.clone())
                .or_default()
                .push(te);
        }
        grouped
    }

    /// Get entries grouped by domain.
    pub fn by_domain(&self, current_time: u64) -> std::collections::HashMap<String, Vec<TimelineEntry>> {
        let mut grouped: std::collections::HashMap<String, Vec<TimelineEntry>> =
            std::collections::HashMap::new();
        for entry in &self.entries {
            let te = TimelineEntry::from_history_entry(entry, current_time, self.latest_version);
            grouped
                .entry(entry.domain.clone())
                .or_default()
                .push(te);
        }
        grouped
    }

    /// Version range covered by this timeline.
    pub fn version_range(&self) -> Option<(u64, u64)> {
        if self.entries.is_empty() {
            return None;
        }
        let min = self.entries.iter().map(|e| e.version).min().unwrap();
        let max = self.entries.iter().map(|e| e.version).max().unwrap();
        Some((min, max))
    }
}

/// Format a timestamp relative to current time.
pub fn format_relative_time(timestamp: u64, current_time: u64) -> String {
    let diff = current_time.saturating_sub(timestamp);
    if diff < 60 {
        "just now".to_string()
    } else if diff < 3600 {
        let mins = diff / 60;
        if mins == 1 {
            "1 minute ago".to_string()
        } else {
            format!("{} minutes ago", mins)
        }
    } else if diff < 86400 {
        let hours = diff / 3600;
        if hours == 1 {
            "1 hour ago".to_string()
        } else {
            format!("{} hours ago", hours)
        }
    } else if diff < 604800 {
        let days = diff / 86400;
        if days == 1 {
            "1 day ago".to_string()
        } else {
            format!("{} days ago", days)
        }
    } else {
        let weeks = diff / 604800;
        if weeks == 1 {
            "1 week ago".to_string()
        } else {
            format!("{} weeks ago", weeks)
        }
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(version: u64, user: &str, domain: &str, ts: u64, desc: Option<&str>) -> HistoryEntry {
        HistoryEntry {
            version,
            user_id: user.to_string(),
            timestamp: ts,
            domain: domain.to_string(),
            description: desc.map(|s| s.to_string()),
            acknowledged: true,
        }
    }

    fn sample_entries() -> Vec<HistoryEntry> {
        vec![
            make_entry(1, "alice", "design", 1000, Some("Add background layer")),
            make_entry(2, "alice", "design", 1100, Some("Add rectangle")),
            make_entry(3, "bob", "comment", 1200, Some("Leave feedback")),
            make_entry(4, "alice", "design", 1300, Some("Resize canvas")),
            make_entry(5, "bob", "design", 1400, Some("Add circle")),
            make_entry(6, "alice", "comment", 1500, Some("Reply to feedback")),
            make_entry(7, "charlie", "design", 1600, Some("Add text layer")),
            make_entry(8, "alice", "design", 1700, Some("Change color")),
            make_entry(9, "bob", "design", 1800, Some("Move element")),
            make_entry(10, "alice", "design", 1900, Some("Final touches")),
        ]
    }

    #[test]
    fn timeline_creation() {
        let tl = Timeline::new(sample_entries(), Uuid::new_v4());
        assert_eq!(tl.len(), 10);
        assert!(!tl.is_empty());
    }

    #[test]
    fn timeline_version_range() {
        let tl = Timeline::new(sample_entries(), Uuid::new_v4());
        assert_eq!(tl.version_range(), Some((1, 10)));
    }

    #[test]
    fn timeline_empty() {
        let tl = Timeline::new(vec![], Uuid::new_v4());
        assert!(tl.is_empty());
        assert_eq!(tl.version_range(), None);
    }

    #[test]
    fn page_first() {
        let tl = Timeline::new(sample_entries(), Uuid::new_v4());
        let p = tl.page(0, 3, &TimelineFilter::new(), 2000);
        assert_eq!(p.entries.len(), 3);
        assert_eq!(p.total_entries, 10);
        assert_eq!(p.total_pages, 4);
        assert!(p.has_next);
        assert!(!p.has_prev);
        // Most recent first
        assert_eq!(p.entries[0].version, 10);
        assert_eq!(p.entries[2].version, 8);
    }

    #[test]
    fn page_last() {
        let tl = Timeline::new(sample_entries(), Uuid::new_v4());
        let p = tl.page(3, 3, &TimelineFilter::new(), 2000);
        assert_eq!(p.entries.len(), 1); // Only 1 entry on last page
        assert!(!p.has_next);
        assert!(p.has_prev);
        assert_eq!(p.entries[0].version, 1);
    }

    #[test]
    fn page_with_user_filter() {
        let tl = Timeline::new(sample_entries(), Uuid::new_v4());
        let filter = TimelineFilter::new().with_user("bob");
        let p = tl.page(0, 10, &filter, 2000);
        assert_eq!(p.total_entries, 3); // Bob has 3 entries
        for e in &p.entries {
            assert_eq!(e.user_id, "bob");
        }
    }

    #[test]
    fn page_with_domain_filter() {
        let tl = Timeline::new(sample_entries(), Uuid::new_v4());
        let filter = TimelineFilter::new().with_domain("comment");
        let p = tl.page(0, 10, &filter, 2000);
        assert_eq!(p.total_entries, 2);
    }

    #[test]
    fn page_with_time_filter() {
        let tl = Timeline::new(sample_entries(), Uuid::new_v4());
        let filter = TimelineFilter::new().with_after(1500).with_before(1800);
        let p = tl.page(0, 10, &filter, 2000);
        // Entries at ts 1500, 1600, 1700, 1800
        assert_eq!(p.total_entries, 4);
    }

    #[test]
    fn page_with_version_range_filter() {
        let tl = Timeline::new(sample_entries(), Uuid::new_v4());
        let filter = TimelineFilter::new().with_version_range(3, 7);
        let p = tl.page(0, 10, &filter, 2000);
        assert_eq!(p.total_entries, 5);
    }

    #[test]
    fn page_with_search_filter() {
        let tl = Timeline::new(sample_entries(), Uuid::new_v4());
        let filter = TimelineFilter::new().with_search("feedback");
        let p = tl.page(0, 10, &filter, 2000);
        assert_eq!(p.total_entries, 2); // "Leave feedback" and "Reply to feedback"
    }

    #[test]
    fn all_entries() {
        let tl = Timeline::new(sample_entries(), Uuid::new_v4());
        let all = tl.all_entries(2000);
        assert_eq!(all.len(), 10);
        assert_eq!(all[0].version, 10); // Most recent first
        assert_eq!(all[9].version, 1);
    }

    #[test]
    fn search() {
        let tl = Timeline::new(sample_entries(), Uuid::new_v4());
        let results = tl.search("canvas", 2000);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].version, 4);
    }

    #[test]
    fn by_user() {
        let tl = Timeline::new(sample_entries(), Uuid::new_v4());
        let grouped = tl.by_user(2000);
        assert_eq!(grouped.len(), 3); // alice, bob, charlie
        assert_eq!(grouped["alice"].len(), 6);
        assert_eq!(grouped["bob"].len(), 3);
        assert_eq!(grouped["charlie"].len(), 1);
    }

    #[test]
    fn by_domain() {
        let tl = Timeline::new(sample_entries(), Uuid::new_v4());
        let grouped = tl.by_domain(2000);
        assert_eq!(grouped.len(), 2); // design, comment
        assert_eq!(grouped["design"].len(), 8);
        assert_eq!(grouped["comment"].len(), 2);
    }

    #[test]
    fn timeline_entry_is_current() {
        let tl = Timeline::new(sample_entries(), Uuid::new_v4());
        let all = tl.all_entries(2000);
        assert!(all[0].is_current); // version 10 = latest
        assert!(!all[1].is_current);
    }

    #[test]
    fn relative_time_just_now() {
        assert_eq!(format_relative_time(1000, 1030), "just now");
    }

    #[test]
    fn relative_time_minutes() {
        assert_eq!(format_relative_time(1000, 1300), "5 minutes ago");
        assert_eq!(format_relative_time(1000, 1060), "1 minute ago");
    }

    #[test]
    fn relative_time_hours() {
        assert_eq!(format_relative_time(1000, 4600), "1 hour ago");
        assert_eq!(format_relative_time(1000, 11800), "3 hours ago");
    }

    #[test]
    fn relative_time_days() {
        assert_eq!(format_relative_time(1000, 87400), "1 day ago");
    }

    #[test]
    fn relative_time_weeks() {
        assert_eq!(format_relative_time(0, 1_209_600), "2 weeks ago");
    }
}
