//! Raw event recording — the foundation of the analytics pipeline.
//!
//! Every time an agent is invoked a caller records an `InvocationEvent`.
//! `MetricsCollector` stores those events in memory and exposes
//! query helpers used by `Aggregator` and `Dashboard`.

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ── Error ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MetricsError {
    #[error("agent '{0}' has no recorded events")]
    AgentNotFound(String),
    #[error("invalid latency value: {0}")]
    InvalidLatency(u64),
}

// ── Outcome kind ──────────────────────────────────────────────────────────────

/// Whether an agent invocation succeeded, failed, or timed out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutcomeKind {
    Success,
    Failure,
    Timeout,
    Cancelled,
}

impl OutcomeKind {
    pub fn is_success(&self) -> bool { matches!(self, Self::Success) }
    pub fn label(&self) -> &str {
        match self {
            Self::Success   => "success",
            Self::Failure   => "failure",
            Self::Timeout   => "timeout",
            Self::Cancelled => "cancelled",
        }
    }
}

// ── Invocation event ──────────────────────────────────────────────────────────

/// One recorded agent invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvocationEvent {
    /// Logical agent identifier.
    pub agent_id: String,
    /// Agent version string (e.g. "1.2.3").
    pub version: String,
    /// Outcome of this invocation.
    pub outcome: OutcomeKind,
    /// End-to-end latency in milliseconds.
    pub latency_ms: u64,
    /// Total LLM tokens consumed (input + output).
    pub tokens: u32,
    /// Unix timestamp (seconds) when the event occurred.
    pub ts: u64,
    /// Optional session identifier.
    pub session_id: Option<String>,
}

impl InvocationEvent {
    /// Construct a minimal event.
    pub fn new(
        agent_id: impl Into<String>,
        version: impl Into<String>,
        outcome: OutcomeKind,
        latency_ms: u64,
        tokens: u32,
    ) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            agent_id: agent_id.into(),
            version: version.into(),
            outcome,
            latency_ms,
            tokens,
            ts,
            session_id: None,
        }
    }

    /// Attach a session identifier.
    pub fn with_session(mut self, id: impl Into<String>) -> Self {
        self.session_id = Some(id.into());
        self
    }

    /// Construct with an explicit timestamp (for deterministic tests).
    pub fn with_ts(mut self, ts: u64) -> Self {
        self.ts = ts;
        self
    }
}

// ── Metrics collector ─────────────────────────────────────────────────────────

/// In-memory store of all `InvocationEvent`s.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetricsCollector {
    events: Vec<InvocationEvent>,
}

impl MetricsCollector {
    pub fn new() -> Self { Self::default() }

    // ── Write ─────────────────────────────────────────────────────────────────

    pub fn record(&mut self, event: InvocationEvent) {
        self.events.push(event);
    }

    pub fn record_many(&mut self, events: impl IntoIterator<Item = InvocationEvent>) {
        for e in events { self.events.push(e); }
    }

    // ── Read ──────────────────────────────────────────────────────────────────

    pub fn all_events(&self) -> &[InvocationEvent] { &self.events }

    pub fn total_count(&self) -> usize { self.events.len() }

    /// All events for a specific agent (any version).
    pub fn events_for_agent(&self, agent_id: &str) -> Vec<&InvocationEvent> {
        self.events.iter().filter(|e| e.agent_id == agent_id).collect()
    }

    /// All events for a specific agent + version.
    pub fn events_for_version(&self, agent_id: &str, version: &str) -> Vec<&InvocationEvent> {
        self.events
            .iter()
            .filter(|e| e.agent_id == agent_id && e.version == version)
            .collect()
    }

    /// All events within a half-open timestamp range [from, to).
    pub fn events_in_range(&self, from_ts: u64, to_ts: u64) -> Vec<&InvocationEvent> {
        self.events
            .iter()
            .filter(|e| e.ts >= from_ts && e.ts < to_ts)
            .collect()
    }

    // ── Aggregates ────────────────────────────────────────────────────────────

    /// Success rate (0.0–100.0) across all events for a given agent + version.
    pub fn success_rate(&self, agent_id: &str, version: &str) -> f32 {
        let evts = self.events_for_version(agent_id, version);
        if evts.is_empty() { return 0.0; }
        let ok = evts.iter().filter(|e| e.outcome.is_success()).count();
        ok as f32 / evts.len() as f32 * 100.0
    }

    /// Average latency (ms) for a given agent + version.
    pub fn avg_latency_ms(&self, agent_id: &str, version: &str) -> f64 {
        let evts = self.events_for_version(agent_id, version);
        if evts.is_empty() { return 0.0; }
        evts.iter().map(|e| e.latency_ms as f64).sum::<f64>() / evts.len() as f64
    }

    /// Peak (max) latency (ms) for a given agent + version.
    pub fn peak_latency_ms(&self, agent_id: &str, version: &str) -> u64 {
        self.events_for_version(agent_id, version)
            .iter()
            .map(|e| e.latency_ms)
            .max()
            .unwrap_or(0)
    }

    /// Total tokens consumed for a given agent + version.
    pub fn total_tokens(&self, agent_id: &str, version: &str) -> u64 {
        self.events_for_version(agent_id, version)
            .iter()
            .map(|e| e.tokens as u64)
            .sum()
    }

    /// Unique agent IDs that have at least one event.
    pub fn agent_ids(&self) -> Vec<&str> {
        let mut ids: Vec<&str> = self.events.iter().map(|e| e.agent_id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    /// Unique versions for an agent.
    pub fn versions_for_agent(&self, agent_id: &str) -> Vec<&str> {
        let mut vs: Vec<&str> = self
            .events
            .iter()
            .filter(|e| e.agent_id == agent_id)
            .map(|e| e.version.as_str())
            .collect();
        vs.sort_unstable();
        vs.dedup();
        vs
    }

    /// p95 latency (ms) for agent + version; returns 0 if no events.
    pub fn p95_latency_ms(&self, agent_id: &str, version: &str) -> u64 {
        let mut lats: Vec<u64> = self
            .events_for_version(agent_id, version)
            .iter()
            .map(|e| e.latency_ms)
            .collect();
        if lats.is_empty() { return 0; }
        lats.sort_unstable();
        let idx = ((lats.len() as f64 * 0.95).ceil() as usize).min(lats.len()) - 1;
        lats[idx]
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn evt(agent: &str, ver: &str, ok: bool, lat: u64, tok: u32) -> InvocationEvent {
        InvocationEvent::new(
            agent, ver,
            if ok { OutcomeKind::Success } else { OutcomeKind::Failure },
            lat, tok,
        )
    }

    #[test]
    fn record_and_count() {
        let mut col = MetricsCollector::new();
        col.record(evt("a", "1.0.0", true, 100, 500));
        col.record(evt("a", "1.0.0", false, 200, 300));
        assert_eq!(col.total_count(), 2);
    }

    #[test]
    fn success_rate_100_percent() {
        let mut col = MetricsCollector::new();
        col.record(evt("a", "1.0.0", true, 50, 100));
        col.record(evt("a", "1.0.0", true, 60, 100));
        assert!((col.success_rate("a", "1.0.0") - 100.0).abs() < 1e-3);
    }

    #[test]
    fn success_rate_50_percent() {
        let mut col = MetricsCollector::new();
        col.record(evt("b", "1.0.0", true, 50, 100));
        col.record(evt("b", "1.0.0", false, 50, 100));
        assert!((col.success_rate("b", "1.0.0") - 50.0).abs() < 1e-3);
    }

    #[test]
    fn success_rate_zero_for_unknown() {
        let col = MetricsCollector::new();
        assert_eq!(col.success_rate("ghost", "1.0.0"), 0.0);
    }

    #[test]
    fn avg_latency() {
        let mut col = MetricsCollector::new();
        col.record(evt("c", "1.0.0", true, 100, 0));
        col.record(evt("c", "1.0.0", true, 300, 0));
        assert!((col.avg_latency_ms("c", "1.0.0") - 200.0).abs() < 1e-6);
    }

    #[test]
    fn peak_latency() {
        let mut col = MetricsCollector::new();
        col.record(evt("d", "1.0.0", true, 100, 0));
        col.record(evt("d", "1.0.0", true, 999, 0));
        assert_eq!(col.peak_latency_ms("d", "1.0.0"), 999);
    }

    #[test]
    fn total_tokens() {
        let mut col = MetricsCollector::new();
        col.record(evt("e", "1.0.0", true, 0, 300));
        col.record(evt("e", "1.0.0", true, 0, 500));
        assert_eq!(col.total_tokens("e", "1.0.0"), 800);
    }

    #[test]
    fn agent_ids_deduplicated() {
        let mut col = MetricsCollector::new();
        col.record(evt("alpha", "1.0.0", true, 0, 0));
        col.record(evt("alpha", "1.0.0", true, 0, 0));
        col.record(evt("beta",  "1.0.0", true, 0, 0));
        let ids = col.agent_ids();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"alpha"));
        assert!(ids.contains(&"beta"));
    }

    #[test]
    fn versions_for_agent_deduplicated() {
        let mut col = MetricsCollector::new();
        col.record(evt("f", "1.0.0", true, 0, 0));
        col.record(evt("f", "1.0.0", true, 0, 0));
        col.record(evt("f", "2.0.0", true, 0, 0));
        assert_eq!(col.versions_for_agent("f").len(), 2);
    }

    #[test]
    fn p95_latency_single_event() {
        let mut col = MetricsCollector::new();
        col.record(evt("g", "1.0.0", true, 123, 0));
        assert_eq!(col.p95_latency_ms("g", "1.0.0"), 123);
    }

    #[test]
    fn events_in_range_filters_correctly() {
        let mut col = MetricsCollector::new();
        col.record(evt("h", "1.0.0", true, 10, 0).with_ts(100));
        col.record(evt("h", "1.0.0", true, 10, 0).with_ts(200));
        col.record(evt("h", "1.0.0", true, 10, 0).with_ts(300));
        assert_eq!(col.events_in_range(100, 200).len(), 1);
        assert_eq!(col.events_in_range(100, 301).len(), 3);
    }

    #[test]
    fn outcome_kind_labels() {
        assert_eq!(OutcomeKind::Success.label(), "success");
        assert_eq!(OutcomeKind::Timeout.label(), "timeout");
        assert!(OutcomeKind::Success.is_success());
        assert!(!OutcomeKind::Failure.is_success());
    }
}
