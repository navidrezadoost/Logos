//! Privacy-preserving analytics for the marketplace.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Types of analytics events.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventType {
    /// Plugin downloaded
    Download,
    /// Plugin installed
    Install,
    /// Plugin uninstalled
    Uninstall,
    /// Plugin page viewed
    PageView,
    /// Search performed
    Search,
    /// Plugin rated
    Rating,
    /// Plugin reviewed
    ReviewSubmitted,
}

impl std::fmt::Display for EventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Download => write!(f, "download"),
            Self::Install => write!(f, "install"),
            Self::Uninstall => write!(f, "uninstall"),
            Self::PageView => write!(f, "page_view"),
            Self::Search => write!(f, "search"),
            Self::Rating => write!(f, "rating"),
            Self::ReviewSubmitted => write!(f, "review_submitted"),
        }
    }
}

/// A single analytics event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsEvent {
    pub id: Uuid,
    pub event_type: EventType,
    pub plugin_id: Option<Uuid>,
    pub metadata: HashMap<String, String>,
    pub timestamp: u64,
}

impl AnalyticsEvent {
    /// Create a download event.
    pub fn download(plugin_id: Uuid) -> Self {
        Self::new(EventType::Download, Some(plugin_id))
    }

    /// Create an install event.
    pub fn install(plugin_id: Uuid) -> Self {
        Self::new(EventType::Install, Some(plugin_id))
    }

    /// Create a search event.
    pub fn search(query: &str) -> Self {
        let mut event = Self::new(EventType::Search, None);
        event.metadata.insert("query".into(), query.into());
        event
    }

    /// Create a page view event.
    pub fn page_view(plugin_id: Uuid) -> Self {
        Self::new(EventType::PageView, Some(plugin_id))
    }

    fn new(event_type: EventType, plugin_id: Option<Uuid>) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap_or(std::time::Duration::ZERO)
            .as_secs();
        Self {
            id: Uuid::new_v4(),
            event_type,
            plugin_id,
            metadata: HashMap::new(),
            timestamp: now,
        }
    }
}

/// Download statistics for a plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadStats {
    pub plugin_id: Uuid,
    pub total_downloads: u64,
    pub downloads_today: u64,
    pub downloads_this_week: u64,
    pub downloads_this_month: u64,
}

/// In-memory analytics repository.
pub struct AnalyticsRepo {
    events: Vec<AnalyticsEvent>,
    /// Per-plugin download counts
    download_counts: HashMap<Uuid, u64>,
    /// Per-event-type counts
    event_counts: HashMap<EventType, u64>,
}

impl AnalyticsRepo {
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            download_counts: HashMap::new(),
            event_counts: HashMap::new(),
        }
    }

    /// Record an analytics event.
    pub fn record(&mut self, event: AnalyticsEvent) {
        *self.event_counts.entry(event.event_type.clone()).or_insert(0) += 1;

        if event.event_type == EventType::Download {
            if let Some(pid) = event.plugin_id {
                *self.download_counts.entry(pid).or_insert(0) += 1;
            }
        }

        self.events.push(event);
    }

    /// Get total events.
    pub fn total_events(&self) -> usize {
        self.events.len()
    }

    /// Get count for a specific event type.
    pub fn count_by_type(&self, event_type: &EventType) -> u64 {
        self.event_counts.get(event_type).copied().unwrap_or(0)
    }

    /// Get download count for a plugin.
    pub fn download_count(&self, plugin_id: &Uuid) -> u64 {
        self.download_counts.get(plugin_id).copied().unwrap_or(0)
    }

    /// Get download stats for a plugin.
    pub fn download_stats(&self, plugin_id: &Uuid) -> DownloadStats {
        let total = self.download_count(plugin_id);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap_or(std::time::Duration::ZERO)
            .as_secs();

        let day_ago = now.saturating_sub(86400);
        let week_ago = now.saturating_sub(604800);
        let month_ago = now.saturating_sub(2592000);

        let downloads_today = self.events.iter()
            .filter(|e| e.event_type == EventType::Download
                && e.plugin_id == Some(*plugin_id)
                && e.timestamp >= day_ago)
            .count() as u64;

        let downloads_this_week = self.events.iter()
            .filter(|e| e.event_type == EventType::Download
                && e.plugin_id == Some(*plugin_id)
                && e.timestamp >= week_ago)
            .count() as u64;

        let downloads_this_month = self.events.iter()
            .filter(|e| e.event_type == EventType::Download
                && e.plugin_id == Some(*plugin_id)
                && e.timestamp >= month_ago)
            .count() as u64;

        DownloadStats {
            plugin_id: *plugin_id,
            total_downloads: total,
            downloads_today,
            downloads_this_week,
            downloads_this_month,
        }
    }

    /// Get top downloaded plugins.
    pub fn top_downloads(&self, limit: usize) -> Vec<(Uuid, u64)> {
        let mut sorted: Vec<_> = self.download_counts.iter().map(|(k, v)| (*k, *v)).collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));
        sorted.truncate(limit);
        sorted
    }

    /// Search events by query string in metadata.
    pub fn recent_searches(&self, limit: usize) -> Vec<String> {
        self.events.iter()
            .rev()
            .filter(|e| e.event_type == EventType::Search)
            .filter_map(|e| e.metadata.get("query").cloned())
            .take(limit)
            .collect()
    }
}

impl Default for AnalyticsRepo {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analytics_event_download() {
        let pid = Uuid::new_v4();
        let event = AnalyticsEvent::download(pid);
        assert_eq!(event.event_type, EventType::Download);
        assert_eq!(event.plugin_id, Some(pid));
    }

    #[test]
    fn test_analytics_event_search() {
        let event = AnalyticsEvent::search("color picker");
        assert_eq!(event.event_type, EventType::Search);
        assert_eq!(event.metadata.get("query"), Some(&"color picker".to_string()));
    }

    #[test]
    fn test_analytics_repo_record() {
        let mut repo = AnalyticsRepo::new();
        let pid = Uuid::new_v4();

        repo.record(AnalyticsEvent::download(pid));
        repo.record(AnalyticsEvent::download(pid));
        repo.record(AnalyticsEvent::page_view(pid));

        assert_eq!(repo.total_events(), 3);
        assert_eq!(repo.count_by_type(&EventType::Download), 2);
        assert_eq!(repo.count_by_type(&EventType::PageView), 1);
        assert_eq!(repo.download_count(&pid), 2);
    }

    #[test]
    fn test_analytics_repo_top_downloads() {
        let mut repo = AnalyticsRepo::new();
        let p1 = Uuid::new_v4();
        let p2 = Uuid::new_v4();

        for _ in 0..10 { repo.record(AnalyticsEvent::download(p1)); }
        for _ in 0..5 { repo.record(AnalyticsEvent::download(p2)); }

        let top = repo.top_downloads(2);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].0, p1);
        assert_eq!(top[0].1, 10);
    }

    #[test]
    fn test_analytics_repo_recent_searches() {
        let mut repo = AnalyticsRepo::new();
        repo.record(AnalyticsEvent::search("icons"));
        repo.record(AnalyticsEvent::search("charts"));
        repo.record(AnalyticsEvent::search("color"));

        let recent = repo.recent_searches(2);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0], "color");
        assert_eq!(recent[1], "charts");
    }

    #[test]
    fn test_analytics_download_stats() {
        let mut repo = AnalyticsRepo::new();
        let pid = Uuid::new_v4();

        repo.record(AnalyticsEvent::download(pid));
        repo.record(AnalyticsEvent::download(pid));

        let stats = repo.download_stats(&pid);
        assert_eq!(stats.total_downloads, 2);
        assert_eq!(stats.downloads_today, 2);
    }
}
