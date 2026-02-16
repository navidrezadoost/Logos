//! Analytics dashboard — publisher metrics and insights.
//!
//! Provides:
//! - Download statistics over time
//! - Review trends and sentiment
//! - Revenue tracking (if applicable)
//! - User engagement metrics
//! - Configurable widgets and time ranges

use serde::{Deserialize, Serialize};

// ─── Time Range ─────────────────────────────────────────────────

/// Time range for analytics queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeRange {
    Today,
    Last7Days,
    Last30Days,
    Last90Days,
    LastYear,
    AllTime,
    Custom { start: u64, end: u64 },
}

impl TimeRange {
    pub fn label(&self) -> &str {
        match self {
            Self::Today => "Today",
            Self::Last7Days => "Last 7 Days",
            Self::Last30Days => "Last 30 Days",
            Self::Last90Days => "Last 90 Days",
            Self::LastYear => "Last Year",
            Self::AllTime => "All Time",
            Self::Custom { .. } => "Custom",
        }
    }

    pub fn all_preset() -> Vec<Self> {
        vec![
            Self::Today,
            Self::Last7Days,
            Self::Last30Days,
            Self::Last90Days,
            Self::LastYear,
            Self::AllTime,
        ]
    }

    /// Duration in seconds (0 for AllTime/Custom).
    pub fn duration_secs(&self) -> u64 {
        match self {
            Self::Today => 86_400,
            Self::Last7Days => 7 * 86_400,
            Self::Last30Days => 30 * 86_400,
            Self::Last90Days => 90 * 86_400,
            Self::LastYear => 365 * 86_400,
            Self::AllTime => 0,
            Self::Custom { start, end } => end.saturating_sub(*start),
        }
    }
}

impl Default for TimeRange {
    fn default() -> Self {
        Self::Last30Days
    }
}

impl std::fmt::Display for TimeRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

// ─── Metrics ────────────────────────────────────────────────────

/// A single data point in a time series.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataPoint {
    pub timestamp: u64,
    pub value: f64,
    pub label: String,
}

/// Summary statistics for a metric.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetricSummary {
    pub current: f64,
    pub previous: f64,
    pub change_pct: f64,
    pub trend: Trend,
}

impl MetricSummary {
    pub fn new(current: f64, previous: f64) -> Self {
        let change_pct = if previous > 0.0 {
            ((current - previous) / previous) * 100.0
        } else if current > 0.0 {
            100.0
        } else {
            0.0
        };

        let trend = if change_pct > 5.0 {
            Trend::Up
        } else if change_pct < -5.0 {
            Trend::Down
        } else {
            Trend::Stable
        };

        Self {
            current,
            previous,
            change_pct,
            trend,
        }
    }
}

/// Trend direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Trend {
    Up,
    Down,
    #[default]
    Stable,
}

impl Trend {
    pub fn icon(&self) -> &str {
        match self {
            Self::Up => "↑",
            Self::Down => "↓",
            Self::Stable => "→",
        }
    }
}

impl std::fmt::Display for Trend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Up => write!(f, "Up"),
            Self::Down => write!(f, "Down"),
            Self::Stable => write!(f, "Stable"),
        }
    }
}

// ─── Dashboard Widgets ──────────────────────────────────────────

/// Types of dashboard widgets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DashboardWidget {
    DownloadStats,
    ReviewSummary,
    RatingOverview,
    ActiveUsers,
    RevenueChart,
    TopPlugins,
    RecentActivity,
    GeographicDistribution,
}

impl DashboardWidget {
    pub fn label(&self) -> &str {
        match self {
            Self::DownloadStats => "Downloads",
            Self::ReviewSummary => "Reviews",
            Self::RatingOverview => "Ratings",
            Self::ActiveUsers => "Active Users",
            Self::RevenueChart => "Revenue",
            Self::TopPlugins => "Top Plugins",
            Self::RecentActivity => "Recent Activity",
            Self::GeographicDistribution => "Geographic Distribution",
        }
    }

    pub fn description(&self) -> &str {
        match self {
            Self::DownloadStats => "Download counts and trends over time",
            Self::ReviewSummary => "Review count, sentiment, and highlights",
            Self::RatingOverview => "Average ratings and distribution",
            Self::ActiveUsers => "Daily and monthly active users",
            Self::RevenueChart => "Revenue tracking and projections",
            Self::TopPlugins => "Your most popular plugins",
            Self::RecentActivity => "Latest events and notifications",
            Self::GeographicDistribution => "Where your users are located",
        }
    }

    pub fn all() -> Vec<Self> {
        vec![
            Self::DownloadStats,
            Self::ReviewSummary,
            Self::RatingOverview,
            Self::ActiveUsers,
            Self::RevenueChart,
            Self::TopPlugins,
            Self::RecentActivity,
            Self::GeographicDistribution,
        ]
    }
}

impl std::fmt::Display for DashboardWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// Per-plugin analytics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginAnalytics {
    pub plugin_id: String,
    pub plugin_name: String,
    pub total_downloads: u64,
    pub total_reviews: u64,
    pub average_rating: f64,
    pub active_installs: u64,
    pub download_trend: Vec<DataPoint>,
}

/// Activity event for the feed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityEvent {
    pub timestamp: u64,
    pub kind: ActivityKind,
    pub message: String,
    pub plugin_id: Option<String>,
}

/// Activity type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActivityKind {
    Download,
    Review,
    Install,
    Uninstall,
    Update,
    Milestone,
}

impl std::fmt::Display for ActivityKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Download => write!(f, "Download"),
            Self::Review => write!(f, "Review"),
            Self::Install => write!(f, "Install"),
            Self::Uninstall => write!(f, "Uninstall"),
            Self::Update => write!(f, "Update"),
            Self::Milestone => write!(f, "Milestone"),
        }
    }
}

// ─── Analytics Dashboard ────────────────────────────────────────

/// The analytics dashboard state.
pub struct AnalyticsDashboard {
    time_range: TimeRange,
    active_widgets: Vec<DashboardWidget>,
    overview: DashboardOverview,
    plugin_analytics: Vec<PluginAnalytics>,
    activity_feed: Vec<ActivityEvent>,
}

/// Top-level dashboard overview metrics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DashboardOverview {
    pub total_downloads: MetricSummary,
    pub total_reviews: MetricSummary,
    pub average_rating: MetricSummary,
    pub active_users: MetricSummary,
    pub revenue: MetricSummary,
    pub published_plugins: u32,
}

impl AnalyticsDashboard {
    /// Create new dashboard with default widgets.
    pub fn new() -> Self {
        Self {
            time_range: TimeRange::default(),
            active_widgets: vec![
                DashboardWidget::DownloadStats,
                DashboardWidget::ReviewSummary,
                DashboardWidget::RatingOverview,
                DashboardWidget::TopPlugins,
                DashboardWidget::RecentActivity,
            ],
            overview: DashboardOverview::default(),
            plugin_analytics: Vec::new(),
            activity_feed: Vec::new(),
        }
    }

    /// Get the current time range.
    pub fn time_range(&self) -> &TimeRange {
        &self.time_range
    }

    /// Set the time range (triggers data reload).
    pub fn set_time_range(&mut self, range: TimeRange) {
        self.time_range = range;
    }

    /// Get active widgets.
    pub fn active_widgets(&self) -> &[DashboardWidget] {
        &self.active_widgets
    }

    /// Add a widget to the dashboard.
    pub fn add_widget(&mut self, widget: DashboardWidget) {
        if !self.active_widgets.contains(&widget) {
            self.active_widgets.push(widget);
        }
    }

    /// Remove a widget from the dashboard.
    pub fn remove_widget(&mut self, widget: &DashboardWidget) {
        self.active_widgets.retain(|w| w != widget);
    }

    /// Reorder widgets (move widget at `from` to `to`).
    pub fn reorder_widget(&mut self, from: usize, to: usize) {
        if from < self.active_widgets.len() && to < self.active_widgets.len() {
            let widget = self.active_widgets.remove(from);
            self.active_widgets.insert(to, widget);
        }
    }

    /// Get overview metrics.
    pub fn overview(&self) -> &DashboardOverview {
        &self.overview
    }

    /// Set overview data (from API).
    pub fn set_overview(&mut self, overview: DashboardOverview) {
        self.overview = overview;
    }

    /// Get per-plugin analytics.
    pub fn plugin_analytics(&self) -> &[PluginAnalytics] {
        &self.plugin_analytics
    }

    /// Set plugin analytics data.
    pub fn set_plugin_analytics(&mut self, analytics: Vec<PluginAnalytics>) {
        self.plugin_analytics = analytics;
    }

    /// Get the activity feed.
    pub fn activity_feed(&self) -> &[ActivityEvent] {
        &self.activity_feed
    }

    /// Add an activity event.
    pub fn add_activity(&mut self, event: ActivityEvent) {
        self.activity_feed.insert(0, event); // newest first
        if self.activity_feed.len() > 100 {
            self.activity_feed.truncate(100);
        }
    }

    /// Set activity feed data.
    pub fn set_activity_feed(&mut self, events: Vec<ActivityEvent>) {
        self.activity_feed = events;
    }

    /// Get top plugin by downloads.
    pub fn top_plugin(&self) -> Option<&PluginAnalytics> {
        self.plugin_analytics
            .iter()
            .max_by_key(|p| p.total_downloads)
    }

    /// Get total downloads across all plugins.
    pub fn total_downloads(&self) -> u64 {
        self.plugin_analytics.iter().map(|p| p.total_downloads).sum()
    }

    /// Reset the dashboard.
    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

impl Default for AnalyticsDashboard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_range_presets() {
        let presets = TimeRange::all_preset();
        assert_eq!(presets.len(), 6);
        assert_eq!(TimeRange::default(), TimeRange::Last30Days);
    }

    #[test]
    fn test_time_range_duration() {
        assert_eq!(TimeRange::Today.duration_secs(), 86_400);
        assert_eq!(TimeRange::Last7Days.duration_secs(), 7 * 86_400);
        assert_eq!(TimeRange::AllTime.duration_secs(), 0);

        let custom = TimeRange::Custom {
            start: 1000,
            end: 2000,
        };
        assert_eq!(custom.duration_secs(), 1000);
    }

    #[test]
    fn test_metric_summary() {
        let s = MetricSummary::new(150.0, 100.0);
        assert_eq!(s.change_pct, 50.0);
        assert_eq!(s.trend, Trend::Up);

        let s2 = MetricSummary::new(90.0, 100.0);
        assert_eq!(s2.trend, Trend::Down);

        let s3 = MetricSummary::new(100.0, 100.0);
        assert_eq!(s3.trend, Trend::Stable);
    }

    #[test]
    fn test_metric_summary_from_zero() {
        let s = MetricSummary::new(50.0, 0.0);
        assert_eq!(s.change_pct, 100.0);
        assert_eq!(s.trend, Trend::Up);

        let s2 = MetricSummary::new(0.0, 0.0);
        assert_eq!(s2.change_pct, 0.0);
        assert_eq!(s2.trend, Trend::Stable);
    }

    #[test]
    fn test_trend_icon() {
        assert_eq!(Trend::Up.icon(), "↑");
        assert_eq!(Trend::Down.icon(), "↓");
        assert_eq!(Trend::Stable.icon(), "→");
    }

    #[test]
    fn test_dashboard_widgets() {
        let widgets = DashboardWidget::all();
        assert_eq!(widgets.len(), 8);
        assert_eq!(DashboardWidget::DownloadStats.label(), "Downloads");
    }

    #[test]
    fn test_dashboard_creation() {
        let dash = AnalyticsDashboard::new();
        assert_eq!(*dash.time_range(), TimeRange::Last30Days);
        assert_eq!(dash.active_widgets().len(), 5);
        assert_eq!(dash.plugin_analytics().len(), 0);
    }

    #[test]
    fn test_dashboard_widget_management() {
        let mut dash = AnalyticsDashboard::new();
        let initial = dash.active_widgets().len();

        dash.add_widget(DashboardWidget::GeographicDistribution);
        assert_eq!(dash.active_widgets().len(), initial + 1);

        // No duplicate
        dash.add_widget(DashboardWidget::GeographicDistribution);
        assert_eq!(dash.active_widgets().len(), initial + 1);

        dash.remove_widget(&DashboardWidget::GeographicDistribution);
        assert_eq!(dash.active_widgets().len(), initial);
    }

    #[test]
    fn test_dashboard_reorder() {
        let mut dash = AnalyticsDashboard::new();
        let first = dash.active_widgets()[0].clone();
        let second = dash.active_widgets()[1].clone();

        dash.reorder_widget(0, 1);
        assert_eq!(dash.active_widgets()[0], second);
        assert_eq!(dash.active_widgets()[1], first);
    }

    #[test]
    fn test_activity_feed() {
        let mut dash = AnalyticsDashboard::new();
        dash.add_activity(ActivityEvent {
            timestamp: 1000,
            kind: ActivityKind::Download,
            message: "Plugin downloaded".into(),
            plugin_id: Some("p1".into()),
        });
        dash.add_activity(ActivityEvent {
            timestamp: 2000,
            kind: ActivityKind::Review,
            message: "New review received".into(),
            plugin_id: Some("p1".into()),
        });

        assert_eq!(dash.activity_feed().len(), 2);
        assert_eq!(dash.activity_feed()[0].timestamp, 2000); // newest first
    }

    #[test]
    fn test_plugin_analytics() {
        let mut dash = AnalyticsDashboard::new();
        dash.set_plugin_analytics(vec![
            PluginAnalytics {
                plugin_id: "p1".into(),
                plugin_name: "Plugin A".into(),
                total_downloads: 500,
                total_reviews: 10,
                average_rating: 4.2,
                active_installs: 200,
                download_trend: vec![],
            },
            PluginAnalytics {
                plugin_id: "p2".into(),
                plugin_name: "Plugin B".into(),
                total_downloads: 1000,
                total_reviews: 25,
                average_rating: 4.8,
                active_installs: 600,
                download_trend: vec![],
            },
        ]);

        assert_eq!(dash.total_downloads(), 1500);
        assert_eq!(dash.top_plugin().unwrap().plugin_name, "Plugin B");
    }

    #[test]
    fn test_dashboard_overview() {
        let mut dash = AnalyticsDashboard::new();
        let overview = DashboardOverview {
            total_downloads: MetricSummary::new(1500.0, 1000.0),
            total_reviews: MetricSummary::new(50.0, 40.0),
            average_rating: MetricSummary::new(4.5, 4.3),
            active_users: MetricSummary::new(300.0, 250.0),
            revenue: MetricSummary::new(0.0, 0.0),
            published_plugins: 5,
        };
        dash.set_overview(overview);

        assert_eq!(dash.overview().published_plugins, 5);
        assert_eq!(dash.overview().total_downloads.trend, Trend::Up);
    }

    #[test]
    fn test_dashboard_reset() {
        let mut dash = AnalyticsDashboard::new();
        dash.set_time_range(TimeRange::LastYear);
        dash.add_widget(DashboardWidget::GeographicDistribution);
        dash.reset();
        assert_eq!(*dash.time_range(), TimeRange::Last30Days);
        assert_eq!(dash.active_widgets().len(), 5);
    }
}
