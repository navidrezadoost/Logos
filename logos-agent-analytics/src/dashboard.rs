//! Dashboard layer — alert generation and cross-version comparisons.
//!
//! `Dashboard::build` consumes a `MetricsCollector` and a `FeedbackStore`,
//! runs `Aggregator::compute`, and produces alerts whenever a version falls
//! below configurable thresholds.

use serde::{Deserialize, Serialize};
use crate::aggregator::{Aggregator, AgentVersionStats, TimeWindow};
use crate::feedback::FeedbackStore;
use crate::metrics::MetricsCollector;

// ── Alert kinds ───────────────────────────────────────────────────────────────

/// The category of a dashboard alert.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertKind {
    /// Success rate fell below threshold (0–100).
    LowSuccessRate { rate: f32, threshold: f32 },
    /// Average latency exceeded threshold (ms).
    HighLatency { ms: f64, threshold: f64 },
    /// Average user rating fell below threshold.
    LowRating { rating: f32, threshold: f32 },
    /// No feedback recorded for this version.
    NoFeedback,
}

impl AlertKind {
    pub fn label(&self) -> &str {
        match self {
            Self::LowSuccessRate { .. } => "low-success-rate",
            Self::HighLatency { .. }    => "high-latency",
            Self::LowRating { .. }      => "low-rating",
            Self::NoFeedback            => "no-feedback",
        }
    }
}

// ── Alert ─────────────────────────────────────────────────────────────────────

/// A threshold violation produced by the dashboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardAlert {
    pub agent_id: String,
    pub version: String,
    pub kind: AlertKind,
    /// Human-readable description.
    pub message: String,
}

impl DashboardAlert {
    fn new(agent_id: String, version: String, kind: AlertKind) -> Self {
        let message = format!("[{}] {}/{}", kind.label(), agent_id, version);
        Self { agent_id, version, kind, message }
    }
}

// ── Version comparison ────────────────────────────────────────────────────────

/// Side-by-side stats for two versions of the same agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionComparison {
    pub agent_id: String,
    pub from_version: String,
    pub to_version: String,
    pub from_stats: AgentVersionStats,
    pub to_stats: AgentVersionStats,
}

impl VersionComparison {
    /// Positive value means `to` has higher success rate than `from`.
    pub fn success_rate_delta(&self) -> f32 {
        self.to_stats.success_rate() - self.from_stats.success_rate()
    }

    /// Positive value means latency increased (regression).
    pub fn latency_delta_ms(&self) -> f64 {
        self.to_stats.avg_latency_ms - self.from_stats.avg_latency_ms
    }

    /// `true` if `to` is strictly better by success rate.
    pub fn is_improvement(&self) -> bool {
        self.to_stats.success_rate() > self.from_stats.success_rate()
    }
}

// ── Dashboard config ──────────────────────────────────────────────────────────

/// Thresholds used by `Dashboard::build` when generating alerts.
#[derive(Debug, Clone)]
pub struct DashboardConfig {
    /// Success rate below this value triggers a `LowSuccessRate` alert (0–100).
    pub min_success_rate: f32,
    /// Average latency above this value triggers a `HighLatency` alert (ms).
    pub max_avg_latency_ms: f64,
    /// Average rating below this value triggers a `LowRating` alert.
    pub min_avg_rating: f32,
}

impl Default for DashboardConfig {
    fn default() -> Self {
        Self {
            min_success_rate: 80.0,
            max_avg_latency_ms: 2000.0,
            min_avg_rating: 3.5,
        }
    }
}

// ── Dashboard ─────────────────────────────────────────────────────────────────

/// Full analytics snapshot for the current state of a fleet of agents.
#[derive(Debug, Clone)]
pub struct Dashboard {
    stats: Vec<AgentVersionStats>,
    alerts: Vec<DashboardAlert>,
    config: DashboardConfig,
}

impl Dashboard {
    // ── Constructors ──────────────────────────────────────────────────────────

    /// Build a dashboard with default thresholds and `TimeWindow::All`.
    pub fn build(collector: &MetricsCollector, store: &FeedbackStore) -> Self {
        Self::build_with(collector, store, DashboardConfig::default(), &TimeWindow::All)
    }

    /// Build a dashboard with custom thresholds and a time window.
    pub fn build_with(
        collector: &MetricsCollector,
        store: &FeedbackStore,
        config: DashboardConfig,
        window: &TimeWindow,
    ) -> Self {
        let stats = Aggregator::compute(collector, window);
        let alerts = Self::generate_alerts(&stats, store, &config);
        Self { stats, alerts, config }
    }

    // ── Query ─────────────────────────────────────────────────────────────────

    pub fn all_stats(&self) -> &[AgentVersionStats] { &self.stats }

    pub fn alerts(&self) -> &[DashboardAlert] { &self.alerts }

    pub fn has_alerts(&self) -> bool { !self.alerts.is_empty() }

    /// Top N agents by success rate.
    pub fn top_agents(&self, n: usize) -> Vec<&AgentVersionStats> {
        let mut sorted: Vec<&AgentVersionStats> = self.stats.iter().collect();
        sorted.sort_by(|a, b| {
            b.success_rate().partial_cmp(&a.success_rate()).unwrap()
        });
        sorted.truncate(n);
        sorted
    }

    /// Find stats for a specific (agent_id, version).
    pub fn stats_for(&self, agent_id: &str, version: &str) -> Option<&AgentVersionStats> {
        self.stats.iter().find(|s| s.agent_id == agent_id && s.version == version)
    }

    /// Compare two versions of the same agent.
    pub fn compare_versions(
        &self,
        agent_id: &str,
        from: &str,
        to: &str,
    ) -> Option<VersionComparison> {
        let from_stats = self.stats_for(agent_id, from)?.clone();
        let to_stats   = self.stats_for(agent_id, to)?.clone();
        Some(VersionComparison {
            agent_id: agent_id.to_string(),
            from_version: from.to_string(),
            to_version: to.to_string(),
            from_stats,
            to_stats,
        })
    }

    /// Human-readable text summary for quick printing.
    pub fn summary_text(&self) -> String {
        if self.stats.is_empty() {
            return "Dashboard: no data".to_string();
        }
        let mut lines = vec![
            format!("=== Dashboard: {} agent+version pair(s) ===", self.stats.len()),
        ];
        for s in &self.stats {
            lines.push(format!(
                "  [{}/{}] calls={} success={:.1}% avg_lat={:.0}ms tokens={}",
                s.agent_id, s.version, s.call_count, s.success_rate(),
                s.avg_latency_ms, s.total_tokens,
            ));
        }
        if !self.alerts.is_empty() {
            lines.push(format!("  *** {} alert(s) ***", self.alerts.len()));
            for a in &self.alerts {
                lines.push(format!("    ! {}", a.message));
            }
        }
        lines.join("\n")
    }

    /// Serialize the full dashboard to a JSON string.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        #[derive(Serialize)]
        struct Payload<'a> {
            stats: &'a [AgentVersionStats],
            alert_count: usize,
        }
        serde_json::to_string(&Payload { stats: &self.stats, alert_count: self.alerts.len() })
    }

    // ── Internal ──────────────────────────────────────────────────────────────

    fn generate_alerts(
        stats: &[AgentVersionStats],
        store: &FeedbackStore,
        cfg: &DashboardConfig,
    ) -> Vec<DashboardAlert> {
        let mut alerts = Vec::new();

        for s in stats {
            let rate = s.success_rate();
            if rate < cfg.min_success_rate {
                alerts.push(DashboardAlert::new(
                    s.agent_id.clone(), s.version.clone(),
                    AlertKind::LowSuccessRate { rate, threshold: cfg.min_success_rate },
                ));
            }

            if s.avg_latency_ms > cfg.max_avg_latency_ms {
                alerts.push(DashboardAlert::new(
                    s.agent_id.clone(), s.version.clone(),
                    AlertKind::HighLatency {
                        ms: s.avg_latency_ms,
                        threshold: cfg.max_avg_latency_ms,
                    },
                ));
            }

            let summary = store.summary_for(&s.agent_id, &s.version);
            if summary.count == 0 {
                alerts.push(DashboardAlert::new(
                    s.agent_id.clone(), s.version.clone(),
                    AlertKind::NoFeedback,
                ));
            } else if summary.avg_rating < cfg.min_avg_rating {
                alerts.push(DashboardAlert::new(
                    s.agent_id.clone(), s.version.clone(),
                    AlertKind::LowRating {
                        rating: summary.avg_rating,
                        threshold: cfg.min_avg_rating,
                    },
                ));
            }
        }

        alerts
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::{InvocationEvent, OutcomeKind};
    use crate::feedback::UserFeedback;

    fn good_event(agent: &str, ver: &str) -> InvocationEvent {
        InvocationEvent::new(agent, ver, OutcomeKind::Success, 100, 200)
    }
    fn bad_event(agent: &str, ver: &str) -> InvocationEvent {
        InvocationEvent::new(agent, ver, OutcomeKind::Failure, 100, 200)
    }

    #[test]
    fn no_data_summary() {
        let col   = MetricsCollector::new();
        let store = FeedbackStore::new();
        let dash  = Dashboard::build(&col, &store);
        assert_eq!(dash.summary_text(), "Dashboard: no data");
    }

    #[test]
    fn summary_text_contains_agent() {
        let mut col = MetricsCollector::new();
        let mut store = FeedbackStore::new();
        col.record(good_event("agent-z", "1.0.0"));
        store.submit(UserFeedback::new("agent-z", "1.0.0", "s1", 5));
        let dash = Dashboard::build(&col, &store);
        assert!(dash.summary_text().contains("agent-z"));
    }

    #[test]
    fn low_success_rate_alert_fired() {
        let mut col   = MetricsCollector::new();
        let mut store = FeedbackStore::new();
        col.record(bad_event("agent-fail", "1.0.0"));
        store.submit(UserFeedback::new("agent-fail", "1.0.0", "s1", 4));
        let dash = Dashboard::build(&col, &store);
        let alert_labels: Vec<&str> = dash.alerts().iter().map(|a| a.kind.label()).collect();
        assert!(alert_labels.contains(&"low-success-rate"));
    }

    #[test]
    fn high_latency_alert_fired() {
        let mut col   = MetricsCollector::new();
        let mut store = FeedbackStore::new();
        col.record(InvocationEvent::new("slow-agent", "1.0.0", OutcomeKind::Success, 99_999, 0));
        store.submit(UserFeedback::new("slow-agent", "1.0.0", "s1", 4));
        let dash = Dashboard::build(&col, &store);
        let labels: Vec<&str> = dash.alerts().iter().map(|a| a.kind.label()).collect();
        assert!(labels.contains(&"high-latency"));
    }

    #[test]
    fn no_feedback_alert_fired_when_missing() {
        let mut col = MetricsCollector::new();
        col.record(good_event("agent-nofb", "1.0.0"));
        let store = FeedbackStore::new();
        let dash  = Dashboard::build(&col, &store);
        let labels: Vec<&str> = dash.alerts().iter().map(|a| a.kind.label()).collect();
        assert!(labels.contains(&"no-feedback"));
    }

    #[test]
    fn top_agents_sorted_by_success_rate() {
        let mut col   = MetricsCollector::new();
        let mut store = FeedbackStore::new();
        // agent-hi: 100 %
        col.record(good_event("agent-hi", "1.0.0"));
        // agent-lo: 50 %
        col.record(good_event("agent-lo", "1.0.0"));
        col.record(bad_event("agent-lo", "1.0.0"));
        for (a, v) in [("agent-hi", "1.0.0"), ("agent-lo", "1.0.0")] {
            store.submit(UserFeedback::new(a, v, "s", 5));
        }
        let dash = Dashboard::build(&col, &store);
        let top = dash.top_agents(2);
        assert_eq!(top[0].agent_id, "agent-hi");
    }

    #[test]
    fn compare_versions_returns_some() {
        let mut col   = MetricsCollector::new();
        let mut store = FeedbackStore::new();
        col.record(good_event("my-agent", "1.0.0"));
        col.record(bad_event("my-agent",  "2.0.0"));
        for v in ["1.0.0", "2.0.0"] {
            store.submit(UserFeedback::new("my-agent", v, "s", 4));
        }
        let dash = Dashboard::build(&col, &store);
        let cmp = dash.compare_versions("my-agent", "1.0.0", "2.0.0").unwrap();
        assert!(cmp.success_rate_delta() < 0.0); // v2 is worse
    }

    #[test]
    fn compare_versions_missing_returns_none() {
        let col   = MetricsCollector::new();
        let store = FeedbackStore::new();
        let dash  = Dashboard::build(&col, &store);
        assert!(dash.compare_versions("ghost", "1.0.0", "2.0.0").is_none());
    }

    #[test]
    fn to_json_is_valid_json() {
        let mut col   = MetricsCollector::new();
        let mut store = FeedbackStore::new();
        col.record(good_event("ag", "1.0.0"));
        store.submit(UserFeedback::new("ag", "1.0.0", "s", 5));
        let dash = Dashboard::build(&col, &store);
        let json = dash.to_json().unwrap();
        assert!(json.starts_with('{'));
        assert!(json.contains("ag"));
    }
}
