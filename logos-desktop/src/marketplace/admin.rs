//! Moderation panel and admin dashboard.
//!
//! Provides:
//! - Moderation queue with approve/reject workflows
//! - Content review with policy checks
//! - Admin dashboard with global marketplace stats
//! - Moderator activity logging

use serde::{Deserialize, Serialize};

// ─── Moderation Filters ─────────────────────────────────────────

/// Filter for the moderation queue.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModerationFilter {
    pub status: Option<ModerationItemStatus>,
    pub category: Option<String>,
    pub search_query: String,
    pub sort: ModerationSort,
    pub page: usize,
    pub per_page: usize,
    pub flagged_only: bool,
    pub priority_only: bool,
}

impl ModerationFilter {
    pub fn new() -> Self {
        Self {
            per_page: 20,
            page: 1,
            ..Default::default()
        }
    }

    /// Show only pending items.
    pub fn pending(mut self) -> Self {
        self.status = Some(ModerationItemStatus::Pending);
        self
    }

    /// Show only flagged items.
    pub fn flagged(mut self) -> Self {
        self.flagged_only = true;
        self
    }

    /// Show only priority items.
    pub fn priority(mut self) -> Self {
        self.priority_only = true;
        self
    }

    /// Filter by category.
    pub fn with_category(mut self, category: &str) -> Self {
        self.category = Some(category.to_string());
        self
    }

    /// Search within queue.
    pub fn with_search(mut self, query: &str) -> Self {
        self.search_query = query.to_string();
        self
    }

    /// Check if any filters are active.
    pub fn has_active_filters(&self) -> bool {
        self.status.is_some()
            || self.category.is_some()
            || !self.search_query.is_empty()
            || self.flagged_only
            || self.priority_only
    }

    /// Reset to defaults.
    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

/// Sort order for moderation queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ModerationSort {
    #[default]
    OldestFirst,
    NewestFirst,
    Priority,
    Publisher,
}

impl ModerationSort {
    pub fn label(&self) -> &str {
        match self {
            Self::OldestFirst => "Oldest First",
            Self::NewestFirst => "Newest First",
            Self::Priority => "Priority",
            Self::Publisher => "By Publisher",
        }
    }
}

impl std::fmt::Display for ModerationSort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

// ─── Moderation Items ───────────────────────────────────────────

/// Status of a moderation queue item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModerationItemStatus {
    Pending,
    InReview,
    Approved,
    Rejected,
    Flagged,
    Escalated,
}

impl std::fmt::Display for ModerationItemStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "Pending"),
            Self::InReview => write!(f, "In Review"),
            Self::Approved => write!(f, "Approved"),
            Self::Rejected => write!(f, "Rejected"),
            Self::Flagged => write!(f, "Flagged"),
            Self::Escalated => write!(f, "Escalated"),
        }
    }
}

/// A moderation queue item for display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModerationQueueItem {
    pub plugin_id: String,
    pub plugin_name: String,
    pub publisher_name: String,
    pub version: String,
    pub submitted_at: u64,
    pub status: ModerationItemStatus,
    pub category: String,
    pub description: String,
    pub flags: Vec<ModerationFlag>,
    pub reviewer: Option<String>,
    pub notes: Vec<ModerationNote>,
}

impl ModerationQueueItem {
    /// Check if the item is actionable (can be approved/rejected).
    pub fn is_actionable(&self) -> bool {
        matches!(
            self.status,
            ModerationItemStatus::Pending
                | ModerationItemStatus::InReview
                | ModerationItemStatus::Flagged
        )
    }

    /// Check if the item has flags.
    pub fn is_flagged(&self) -> bool {
        !self.flags.is_empty()
    }
}

/// A flag on a moderation item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModerationFlag {
    pub kind: FlagKind,
    pub reason: String,
    pub flagged_by: String,
    pub flagged_at: u64,
}

/// Types of moderation flags.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlagKind {
    SecurityConcern,
    PolicyViolation,
    InappropriateContent,
    Malware,
    CopyrightIssue,
    QualityConcern,
    Other(String),
}

impl std::fmt::Display for FlagKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SecurityConcern => write!(f, "Security Concern"),
            Self::PolicyViolation => write!(f, "Policy Violation"),
            Self::InappropriateContent => write!(f, "Inappropriate Content"),
            Self::Malware => write!(f, "Malware"),
            Self::CopyrightIssue => write!(f, "Copyright Issue"),
            Self::QualityConcern => write!(f, "Quality Concern"),
            Self::Other(s) => write!(f, "{}", s),
        }
    }
}

/// A moderator note.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModerationNote {
    pub author: String,
    pub content: String,
    pub created_at: u64,
}

/// Review decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewDecision {
    pub action: ReviewAction,
    pub reason: String,
    pub reviewer: String,
    pub decided_at: u64,
}

/// Possible review actions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReviewAction {
    Approve,
    Reject,
    RequestChanges,
    Escalate,
    Flag,
}

impl std::fmt::Display for ReviewAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Approve => write!(f, "Approve"),
            Self::Reject => write!(f, "Reject"),
            Self::RequestChanges => write!(f, "Request Changes"),
            Self::Escalate => write!(f, "Escalate"),
            Self::Flag => write!(f, "Flag"),
        }
    }
}

// ─── Moderation Panel ───────────────────────────────────────────

/// Moderation panel state.
pub struct ModerationPanel {
    filter: ModerationFilter,
    queue: Vec<ModerationQueueItem>,
    selected: Option<usize>,
    total_count: usize,
    decisions: Vec<ReviewDecision>,
}

impl ModerationPanel {
    /// Create new moderation panel.
    pub fn new() -> Self {
        Self {
            filter: ModerationFilter::new(),
            queue: Vec::new(),
            selected: None,
            total_count: 0,
            decisions: Vec::new(),
        }
    }

    /// Get the current filter.
    pub fn filter(&self) -> &ModerationFilter {
        &self.filter
    }

    /// Get mutable filter.
    pub fn filter_mut(&mut self) -> &mut ModerationFilter {
        &mut self.filter
    }

    /// Set queue items (from API response).
    pub fn set_queue(&mut self, items: Vec<ModerationQueueItem>, total: usize) {
        self.queue = items;
        self.total_count = total;
        self.selected = None;
    }

    /// Get queue items.
    pub fn queue(&self) -> &[ModerationQueueItem] {
        &self.queue
    }

    /// Get total count.
    pub fn total_count(&self) -> usize {
        self.total_count
    }

    /// Get pending count.
    pub fn pending_count(&self) -> usize {
        self.queue
            .iter()
            .filter(|i| i.status == ModerationItemStatus::Pending)
            .count()
    }

    /// Get flagged count.
    pub fn flagged_count(&self) -> usize {
        self.queue.iter().filter(|i| i.is_flagged()).count()
    }

    /// Select an item.
    pub fn select(&mut self, index: usize) {
        if index < self.queue.len() {
            self.selected = Some(index);
        }
    }

    /// Get selected item.
    pub fn selected_item(&self) -> Option<&ModerationQueueItem> {
        self.selected.and_then(|i| self.queue.get(i))
    }

    /// Deselect.
    pub fn deselect(&mut self) {
        self.selected = None;
    }

    /// Record a moderation decision.
    pub fn record_decision(&mut self, decision: ReviewDecision) {
        self.decisions.push(decision);
    }

    /// Get decision history.
    pub fn decisions(&self) -> &[ReviewDecision] {
        &self.decisions
    }

    /// Get count of decisions made this session.
    pub fn decisions_count(&self) -> usize {
        self.decisions.len()
    }

    /// Reset filters.
    pub fn reset_filters(&mut self) {
        self.filter.reset();
        self.queue.clear();
        self.selected = None;
        self.total_count = 0;
    }
}

impl Default for ModerationPanel {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Admin Dashboard ────────────────────────────────────────────

/// Global marketplace statistics for admins.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MarketplaceGlobalStats {
    pub total_publishers: u64,
    pub total_plugins: u64,
    pub total_downloads: u64,
    pub total_reviews: u64,
    pub total_templates: u64,
    pub pending_moderation: u64,
    pub flagged_items: u64,
    pub active_moderators: u64,
    pub avg_review_time_hours: f64,
}

/// Moderator activity summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModeratorActivity {
    pub moderator_name: String,
    pub reviews_today: u32,
    pub reviews_week: u32,
    pub reviews_total: u32,
    pub avg_time_per_review_mins: f64,
    pub approval_rate: f64,
}

/// Admin dashboard for marketplace overview.
pub struct AdminDashboard {
    global_stats: MarketplaceGlobalStats,
    moderator_activity: Vec<ModeratorActivity>,
    recent_actions: Vec<ReviewDecision>,
    health_checks: Vec<HealthCheck>,
}

/// System health check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    pub name: String,
    pub status: HealthStatus,
    pub message: String,
    pub checked_at: u64,
}

/// Health check status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Down,
    Unknown,
}

impl std::fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Healthy => write!(f, "Healthy"),
            Self::Degraded => write!(f, "Degraded"),
            Self::Down => write!(f, "Down"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

impl AdminDashboard {
    /// Create new admin dashboard.
    pub fn new() -> Self {
        Self {
            global_stats: MarketplaceGlobalStats::default(),
            moderator_activity: Vec::new(),
            recent_actions: Vec::new(),
            health_checks: Vec::new(),
        }
    }

    /// Get global stats.
    pub fn global_stats(&self) -> &MarketplaceGlobalStats {
        &self.global_stats
    }

    /// Set global stats (from API).
    pub fn set_global_stats(&mut self, stats: MarketplaceGlobalStats) {
        self.global_stats = stats;
    }

    /// Get moderator activity.
    pub fn moderator_activity(&self) -> &[ModeratorActivity] {
        &self.moderator_activity
    }

    /// Set moderator activity.
    pub fn set_moderator_activity(&mut self, activity: Vec<ModeratorActivity>) {
        self.moderator_activity = activity;
    }

    /// Get recent moderation actions.
    pub fn recent_actions(&self) -> &[ReviewDecision] {
        &self.recent_actions
    }

    /// Add a recent action.
    pub fn add_recent_action(&mut self, decision: ReviewDecision) {
        self.recent_actions.insert(0, decision);
        if self.recent_actions.len() > 50 {
            self.recent_actions.truncate(50);
        }
    }

    /// Get health checks.
    pub fn health_checks(&self) -> &[HealthCheck] {
        &self.health_checks
    }

    /// Set health checks.
    pub fn set_health_checks(&mut self, checks: Vec<HealthCheck>) {
        self.health_checks = checks;
    }

    /// Check overall system health.
    pub fn overall_health(&self) -> HealthStatus {
        if self.health_checks.is_empty() {
            return HealthStatus::Unknown;
        }

        if self
            .health_checks
            .iter()
            .any(|c| c.status == HealthStatus::Down)
        {
            return HealthStatus::Down;
        }

        if self
            .health_checks
            .iter()
            .any(|c| c.status == HealthStatus::Degraded)
        {
            return HealthStatus::Degraded;
        }

        HealthStatus::Healthy
    }

    /// Get top moderator by total reviews.
    pub fn top_moderator(&self) -> Option<&ModeratorActivity> {
        self.moderator_activity
            .iter()
            .max_by_key(|m| m.reviews_total)
    }

    /// Reset the admin dashboard.
    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

impl Default for AdminDashboard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_moderation_filter_default() {
        let f = ModerationFilter::new();
        assert_eq!(f.page, 1);
        assert_eq!(f.per_page, 20);
        assert!(!f.has_active_filters());
    }

    #[test]
    fn test_moderation_filter_builders() {
        let f = ModerationFilter::new()
            .pending()
            .with_category("utility")
            .with_search("test");

        assert_eq!(f.status, Some(ModerationItemStatus::Pending));
        assert_eq!(f.category, Some("utility".into()));
        assert_eq!(f.search_query, "test");
        assert!(f.has_active_filters());
    }

    #[test]
    fn test_moderation_filter_flagged_priority() {
        let f = ModerationFilter::new().flagged().priority();
        assert!(f.flagged_only);
        assert!(f.priority_only);
        assert!(f.has_active_filters());
    }

    #[test]
    fn test_moderation_sort() {
        assert_eq!(ModerationSort::default(), ModerationSort::OldestFirst);
        assert_eq!(ModerationSort::OldestFirst.label(), "Oldest First");
        assert_eq!(ModerationSort::Priority.label(), "Priority");
    }

    #[test]
    fn test_moderation_queue() {
        let mut panel = ModerationPanel::new();
        assert_eq!(panel.total_count(), 0);

        let items = vec![
            ModerationQueueItem {
                plugin_id: "p1".into(),
                plugin_name: "Test Plugin".into(),
                publisher_name: "alice".into(),
                version: "1.0.0".into(),
                submitted_at: 1000,
                status: ModerationItemStatus::Pending,
                category: "utility".into(),
                description: "A test plugin".into(),
                flags: vec![],
                reviewer: None,
                notes: vec![],
            },
            ModerationQueueItem {
                plugin_id: "p2".into(),
                plugin_name: "Flagged Plugin".into(),
                publisher_name: "bob".into(),
                version: "0.1.0".into(),
                submitted_at: 2000,
                status: ModerationItemStatus::Flagged,
                category: "design".into(),
                description: "A flagged plugin".into(),
                flags: vec![ModerationFlag {
                    kind: FlagKind::SecurityConcern,
                    reason: "Suspicious API calls".into(),
                    flagged_by: "system".into(),
                    flagged_at: 2500,
                }],
                reviewer: None,
                notes: vec![],
            },
        ];

        panel.set_queue(items, 2);
        assert_eq!(panel.queue().len(), 2);
        assert_eq!(panel.pending_count(), 1);
        assert_eq!(panel.flagged_count(), 1);

        panel.select(0);
        let item = panel.selected_item().unwrap();
        assert!(item.is_actionable());
        assert!(!item.is_flagged());
    }

    #[test]
    fn test_moderation_decisions() {
        let mut panel = ModerationPanel::new();
        panel.record_decision(ReviewDecision {
            action: ReviewAction::Approve,
            reason: "Looks good".into(),
            reviewer: "mod1".into(),
            decided_at: 5000,
        });
        panel.record_decision(ReviewDecision {
            action: ReviewAction::Reject,
            reason: "Policy violation".into(),
            reviewer: "mod1".into(),
            decided_at: 6000,
        });

        assert_eq!(panel.decisions_count(), 2);
        assert_eq!(panel.decisions()[0].action, ReviewAction::Approve);
    }

    #[test]
    fn test_item_status_display() {
        assert_eq!(ModerationItemStatus::Pending.to_string(), "Pending");
        assert_eq!(ModerationItemStatus::Escalated.to_string(), "Escalated");
    }

    #[test]
    fn test_flag_kinds() {
        assert_eq!(FlagKind::SecurityConcern.to_string(), "Security Concern");
        assert_eq!(FlagKind::Malware.to_string(), "Malware");
        assert_eq!(
            FlagKind::Other("Custom Flag".into()).to_string(),
            "Custom Flag"
        );
    }

    #[test]
    fn test_review_actions() {
        assert_eq!(ReviewAction::Approve.to_string(), "Approve");
        assert_eq!(ReviewAction::RequestChanges.to_string(), "Request Changes");
    }

    #[test]
    fn test_admin_dashboard() {
        let mut admin = AdminDashboard::new();

        let stats = MarketplaceGlobalStats {
            total_publishers: 50,
            total_plugins: 200,
            total_downloads: 10000,
            total_reviews: 500,
            total_templates: 30,
            pending_moderation: 5,
            flagged_items: 2,
            active_moderators: 3,
            avg_review_time_hours: 4.5,
        };
        admin.set_global_stats(stats);

        assert_eq!(admin.global_stats().total_publishers, 50);
        assert_eq!(admin.global_stats().pending_moderation, 5);
    }

    #[test]
    fn test_admin_moderator_activity() {
        let mut admin = AdminDashboard::new();
        admin.set_moderator_activity(vec![
            ModeratorActivity {
                moderator_name: "mod1".into(),
                reviews_today: 5,
                reviews_week: 20,
                reviews_total: 100,
                avg_time_per_review_mins: 3.5,
                approval_rate: 0.85,
            },
            ModeratorActivity {
                moderator_name: "mod2".into(),
                reviews_today: 3,
                reviews_week: 15,
                reviews_total: 200,
                avg_time_per_review_mins: 5.0,
                approval_rate: 0.90,
            },
        ]);

        assert_eq!(admin.moderator_activity().len(), 2);
        assert_eq!(admin.top_moderator().unwrap().moderator_name, "mod2");
    }

    #[test]
    fn test_health_checks() {
        let mut admin = AdminDashboard::new();

        // Unknown when empty
        assert_eq!(admin.overall_health(), HealthStatus::Unknown);

        admin.set_health_checks(vec![
            HealthCheck {
                name: "Database".into(),
                status: HealthStatus::Healthy,
                message: "OK".into(),
                checked_at: 1000,
            },
            HealthCheck {
                name: "Storage".into(),
                status: HealthStatus::Healthy,
                message: "OK".into(),
                checked_at: 1000,
            },
        ]);
        assert_eq!(admin.overall_health(), HealthStatus::Healthy);

        admin.set_health_checks(vec![
            HealthCheck {
                name: "Database".into(),
                status: HealthStatus::Healthy,
                message: "OK".into(),
                checked_at: 1000,
            },
            HealthCheck {
                name: "Storage".into(),
                status: HealthStatus::Degraded,
                message: "Slow response".into(),
                checked_at: 1000,
            },
        ]);
        assert_eq!(admin.overall_health(), HealthStatus::Degraded);

        admin.set_health_checks(vec![HealthCheck {
            name: "Database".into(),
            status: HealthStatus::Down,
            message: "Connection failed".into(),
            checked_at: 1000,
        }]);
        assert_eq!(admin.overall_health(), HealthStatus::Down);
    }

    #[test]
    fn test_admin_recent_actions() {
        let mut admin = AdminDashboard::new();
        admin.add_recent_action(ReviewDecision {
            action: ReviewAction::Approve,
            reason: "Good".into(),
            reviewer: "mod1".into(),
            decided_at: 1000,
        });
        admin.add_recent_action(ReviewDecision {
            action: ReviewAction::Reject,
            reason: "Bad".into(),
            reviewer: "mod2".into(),
            decided_at: 2000,
        });

        assert_eq!(admin.recent_actions().len(), 2);
        assert_eq!(admin.recent_actions()[0].action, ReviewAction::Reject); // newest first
    }

    #[test]
    fn test_admin_reset() {
        let mut admin = AdminDashboard::new();
        admin.set_global_stats(MarketplaceGlobalStats {
            total_publishers: 100,
            ..Default::default()
        });
        admin.reset();
        assert_eq!(admin.global_stats().total_publishers, 0);
    }
}
