//! Aggregation layer — groups raw `InvocationEvent`s by agent + version.
//!
//! `Aggregator::compute` returns one `AgentVersionStats` per (agent, version)
//! pair, covering a configurable `TimeWindow`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::metrics::{MetricsCollector, OutcomeKind};

// ── Time window ───────────────────────────────────────────────────────────────

/// Which events to include when aggregating.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimeWindow {
    /// All available events.
    All,
    /// Only the most recent N events (per agent+version).
    LastN(usize),
    /// Only events with `ts >= threshold`.
    SinceTs(u64),
}

impl Default for TimeWindow {
    fn default() -> Self { Self::All }
}

// ── Per-version statistics ────────────────────────────────────────────────────

/// Aggregated metrics for one (agent_id, version) pair.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentVersionStats {
    pub agent_id: String,
    pub version: String,
    pub call_count: usize,
    pub success_count: usize,
    pub failure_count: usize,
    pub timeout_count: usize,
    /// Arithmetic mean latency in milliseconds.
    pub avg_latency_ms: f64,
    /// p95 latency in milliseconds.
    pub p95_latency_ms: u64,
    /// Peak (maximum) latency in milliseconds.
    pub peak_latency_ms: u64,
    /// Sum of all tokens consumed.
    pub total_tokens: u64,
    /// Average tokens per invocation.
    pub avg_tokens: f64,
}

impl AgentVersionStats {
    /// Success rate expressed as a value in `[0.0, 100.0]`.
    pub fn success_rate(&self) -> f32 {
        if self.call_count == 0 { return 0.0; }
        self.success_count as f32 / self.call_count as f32 * 100.0
    }

    /// `true` when this version performed better than `other` by success rate.
    pub fn is_better_than(&self, other: &Self) -> bool {
        self.success_rate() > other.success_rate()
    }
}

// ── Aggregator ────────────────────────────────────────────────────────────────

pub struct Aggregator;

impl Aggregator {
    /// Compute stats for every (agent_id, version) combination visible
    /// through `window`.
    pub fn compute(collector: &MetricsCollector, window: &TimeWindow) -> Vec<AgentVersionStats> {
        let mut map: HashMap<(String, String), Vec<(u64, u32, bool, bool, bool)>> = HashMap::new();

        for e in collector.all_events() {
            match window {
                TimeWindow::SinceTs(ts) if e.ts < *ts => continue,
                _ => {}
            }
            let key = (e.agent_id.clone(), e.version.clone());
            let entry = map.entry(key).or_default();
            entry.push((
                e.latency_ms,
                e.tokens,
                e.outcome == OutcomeKind::Success,
                e.outcome == OutcomeKind::Failure,
                e.outcome == OutcomeKind::Timeout,
            ));
        }

        let mut out: Vec<AgentVersionStats> = map
            .into_iter()
            .map(|((agent_id, version), mut rows)| {
                if let TimeWindow::LastN(n) = window {
                    let skip = rows.len().saturating_sub(*n);
                    rows.drain(..skip);
                }
                build_stats(agent_id, version, rows)
            })
            .collect();

        out.sort_by(|a, b| a.agent_id.cmp(&b.agent_id).then(a.version.cmp(&b.version)));
        out
    }

    /// Stats for a single agent across all its versions.
    pub fn for_agent(
        collector: &MetricsCollector,
        agent_id: &str,
        window: &TimeWindow,
    ) -> Vec<AgentVersionStats> {
        Self::compute(collector, window)
            .into_iter()
            .filter(|s| s.agent_id == agent_id)
            .collect()
    }

    /// Find the version of `agent_id` with the highest success rate.
    pub fn best_version(
        collector: &MetricsCollector,
        agent_id: &str,
        window: &TimeWindow,
    ) -> Option<AgentVersionStats> {
        Self::for_agent(collector, agent_id, window)
            .into_iter()
            .max_by(|a, b| a.success_rate().partial_cmp(&b.success_rate()).unwrap())
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn build_stats(
    agent_id: String,
    version: String,
    rows: Vec<(u64, u32, bool, bool, bool)>,
) -> AgentVersionStats {
    let call_count = rows.len();
    let mut success_count = 0usize;
    let mut failure_count = 0usize;
    let mut timeout_count = 0usize;
    let mut total_lat: u64 = 0;
    let mut total_tokens: u64 = 0;
    let mut lats: Vec<u64> = Vec::with_capacity(rows.len());

    for (lat, tok, ok, fail, tout) in &rows {
        if *ok   { success_count += 1; }
        if *fail  { failure_count += 1; }
        if *tout  { timeout_count += 1; }
        total_lat += lat;
        total_tokens += *tok as u64;
        lats.push(*lat);
    }

    let avg_latency_ms = if call_count > 0 { total_lat as f64 / call_count as f64 } else { 0.0 };
    let avg_tokens     = if call_count > 0 { total_tokens as f64 / call_count as f64 } else { 0.0 };

    lats.sort_unstable();
    let peak_latency_ms = lats.last().copied().unwrap_or(0);
    let p95_latency_ms = if lats.is_empty() {
        0
    } else {
        let idx = ((lats.len() as f64 * 0.95).ceil() as usize).min(lats.len()) - 1;
        lats[idx]
    };

    AgentVersionStats {
        agent_id,
        version,
        call_count,
        success_count,
        failure_count,
        timeout_count,
        avg_latency_ms,
        p95_latency_ms,
        peak_latency_ms,
        total_tokens,
        avg_tokens,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::{InvocationEvent, OutcomeKind};

    fn col_with_events() -> MetricsCollector {
        let mut col = MetricsCollector::new();
        for (ok, lat) in [(true, 100u64), (true, 200), (false, 300)] {
            col.record(InvocationEvent::new(
                "agent-x", "1.0.0",
                if ok { OutcomeKind::Success } else { OutcomeKind::Failure },
                lat, 100,
            ));
        }
        col
    }

    #[test]
    fn compute_basic_stats() {
        let col = col_with_events();
        let stats = Aggregator::compute(&col, &TimeWindow::All);
        assert_eq!(stats.len(), 1);
        let s = &stats[0];
        assert_eq!(s.call_count, 3);
        assert_eq!(s.success_count, 2);
        assert_eq!(s.failure_count, 1);
    }

    #[test]
    fn success_rate_correct() {
        let col = col_with_events();
        let stats = Aggregator::compute(&col, &TimeWindow::All);
        let rate = stats[0].success_rate();
        assert!((rate - 66.666_67).abs() < 0.01);
    }

    #[test]
    fn avg_latency_correct() {
        let col = col_with_events();
        let stats = Aggregator::compute(&col, &TimeWindow::All);
        assert!((stats[0].avg_latency_ms - 200.0).abs() < 1e-6);
    }

    #[test]
    fn last_n_window() {
        let mut col = MetricsCollector::new();
        for lat in 1u64..=10 {
            col.record(InvocationEvent::new("a", "1.0.0", OutcomeKind::Success, lat * 100, 0));
        }
        let stats = Aggregator::compute(&col, &TimeWindow::LastN(3));
        assert_eq!(stats[0].call_count, 3);
    }

    #[test]
    fn since_ts_window() {
        let mut col = MetricsCollector::new();
        col.record(InvocationEvent::new("b", "1.0.0", OutcomeKind::Success, 100, 0).with_ts(1000));
        col.record(InvocationEvent::new("b", "1.0.0", OutcomeKind::Success, 100, 0).with_ts(2000));
        col.record(InvocationEvent::new("b", "1.0.0", OutcomeKind::Success, 100, 0).with_ts(3000));
        let stats = Aggregator::compute(&col, &TimeWindow::SinceTs(2000));
        assert_eq!(stats[0].call_count, 2);
    }

    #[test]
    fn multi_agent_multi_version() {
        let mut col = MetricsCollector::new();
        col.record(InvocationEvent::new("a1", "1.0.0", OutcomeKind::Success, 100, 0));
        col.record(InvocationEvent::new("a1", "2.0.0", OutcomeKind::Failure, 200, 0));
        col.record(InvocationEvent::new("a2", "1.0.0", OutcomeKind::Success, 150, 0));
        let stats = Aggregator::compute(&col, &TimeWindow::All);
        assert_eq!(stats.len(), 3);
    }

    #[test]
    fn for_agent_filter() {
        let mut col = MetricsCollector::new();
        col.record(InvocationEvent::new("foo", "1.0.0", OutcomeKind::Success, 100, 0));
        col.record(InvocationEvent::new("bar", "1.0.0", OutcomeKind::Success, 100, 0));
        let foos = Aggregator::for_agent(&col, "foo", &TimeWindow::All);
        assert_eq!(foos.len(), 1);
        assert_eq!(foos[0].agent_id, "foo");
    }

    #[test]
    fn best_version_selected() {
        let mut col = MetricsCollector::new();
        // v1: 50 %
        col.record(InvocationEvent::new("ag", "1.0.0", OutcomeKind::Success, 100, 0));
        col.record(InvocationEvent::new("ag", "1.0.0", OutcomeKind::Failure, 100, 0));
        // v2: 100 %
        col.record(InvocationEvent::new("ag", "2.0.0", OutcomeKind::Success, 100, 0));
        let best = Aggregator::best_version(&col, "ag", &TimeWindow::All).unwrap();
        assert_eq!(best.version, "2.0.0");
    }

    #[test]
    fn best_version_none_for_empty() {
        let col = MetricsCollector::new();
        assert!(Aggregator::best_version(&col, "ghost", &TimeWindow::All).is_none());
    }

    #[test]
    fn is_better_than() {
        let good = AgentVersionStats { success_count: 9, call_count: 10, ..Default::default() };
        let bad  = AgentVersionStats { success_count: 5, call_count: 10, ..Default::default() };
        assert!(good.is_better_than(&bad));
        assert!(!bad.is_better_than(&good));
    }

    #[test]
    fn total_tokens_aggregated() {
        let mut col = MetricsCollector::new();
        col.record(InvocationEvent::new("t", "1.0.0", OutcomeKind::Success, 0, 300));
        col.record(InvocationEvent::new("t", "1.0.0", OutcomeKind::Success, 0, 700));
        let stats = Aggregator::compute(&col, &TimeWindow::All);
        assert_eq!(stats[0].total_tokens, 1000);
    }
}
