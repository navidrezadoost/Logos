//! Data Collector — production telemetry collection for RL-UX training
//!
//! Captures user interaction events from the Logos editor, validates them,
//! batches them for efficient processing, computes session statistics,
//! and provides data for training the Q-table and reward model.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};

// ── Interaction event ─────────────────────────────────────────────────────────

/// A single captured user action with context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionEvent {
    pub event_id: String,
    pub session_id: String,
    pub action: String,
    pub state_json: String,
    pub duration_ms: u32,
    pub timestamp_secs: u64,
    pub accepted_suggestion: Option<bool>,
    pub suggestion_shown: bool,
    pub metadata: HashMap<String, String>,
}

impl InteractionEvent {
    pub fn new(
        session_id: impl Into<String>,
        action: impl Into<String>,
        state_json: impl Into<String>,
        ts: u64,
    ) -> Self {
        InteractionEvent {
            event_id: uuid::Uuid::new_v4().to_string(),
            session_id: session_id.into(),
            action: action.into(),
            state_json: state_json.into(),
            duration_ms: 0,
            timestamp_secs: ts,
            accepted_suggestion: None,
            suggestion_shown: false,
            metadata: HashMap::new(),
        }
    }

    pub fn with_duration(mut self, ms: u32) -> Self { self.duration_ms = ms; self }

    pub fn with_suggestion_result(mut self, shown: bool, accepted: Option<bool>) -> Self {
        self.suggestion_shown = shown;
        self.accepted_suggestion = accepted;
        self
    }

    pub fn with_meta(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    pub fn is_suggestion_accepted(&self) -> bool {
        self.accepted_suggestion == Some(true)
    }

    pub fn is_suggestion_rejected(&self) -> bool {
        self.accepted_suggestion == Some(false)
    }
}

// ── Session stats ─────────────────────────────────────────────────────────────

/// Per-session aggregated statistics for the collector.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionStats {
    pub session_id: String,
    pub event_count: u32,
    pub actions_taken: HashMap<String, u32>,
    pub suggestions_shown: u32,
    pub suggestions_accepted: u32,
    pub suggestions_rejected: u32,
    pub avg_action_duration_ms: f32,
    pub session_start_ts: u64,
    pub last_event_ts: u64,
}

impl SessionStats {
    pub fn new(session_id: impl Into<String>, ts: u64) -> Self {
        SessionStats {
            session_id: session_id.into(),
            session_start_ts: ts,
            last_event_ts: ts,
            ..Default::default()
        }
    }

    pub fn record(&mut self, event: &InteractionEvent) {
        self.event_count += 1;
        *self.actions_taken.entry(event.action.clone()).or_insert(0) += 1;
        if event.suggestion_shown {
            self.suggestions_shown += 1;
            match event.accepted_suggestion {
                Some(true) => self.suggestions_accepted += 1,
                Some(false) => self.suggestions_rejected += 1,
                None => {}
            }
        }
        // Exponential moving average for duration
        let alpha = 0.2f32;
        self.avg_action_duration_ms = (1.0 - alpha) * self.avg_action_duration_ms
            + alpha * event.duration_ms as f32;
        self.last_event_ts = event.timestamp_secs;
    }

    pub fn acceptance_rate(&self) -> f32 {
        if self.suggestions_shown == 0 { return 0.0; }
        self.suggestions_accepted as f32 / self.suggestions_shown as f32
    }

    pub fn rejection_rate(&self) -> f32 {
        if self.suggestions_shown == 0 { return 0.0; }
        self.suggestions_rejected as f32 / self.suggestions_shown as f32
    }

    pub fn session_duration_secs(&self) -> u64 {
        self.last_event_ts.saturating_sub(self.session_start_ts)
    }

    pub fn most_common_action(&self) -> Option<&str> {
        self.actions_taken.iter()
            .max_by_key(|(_, &v)| v)
            .map(|(k, _)| k.as_str())
    }
}

// ── Data batch ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataBatch {
    pub batch_id: String,
    pub events: Vec<InteractionEvent>,
    pub created_ts: u64,
    pub source_session_ids: Vec<String>,
}

impl DataBatch {
    pub fn new(events: Vec<InteractionEvent>, ts: u64) -> Self {
        let source_session_ids: Vec<String> = {
            let mut ids: Vec<String> = events.iter().map(|e| e.session_id.clone()).collect();
            ids.sort(); ids.dedup();
            ids
        };
        DataBatch {
            batch_id: uuid::Uuid::new_v4().to_string(),
            events,
            created_ts: ts,
            source_session_ids,
        }
    }

    pub fn len(&self) -> usize { self.events.len() }
    pub fn is_empty(&self) -> bool { self.events.is_empty() }

    pub fn accepted_suggestions(&self) -> usize {
        self.events.iter().filter(|e| e.is_suggestion_accepted()).count()
    }

    pub fn rejected_suggestions(&self) -> usize {
        self.events.iter().filter(|e| e.is_suggestion_rejected()).count()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".into())
    }
}

// ── Collector config ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CollectorConfig {
    /// Maximum events in the ring buffer before auto-flushing.
    pub max_buffer_size: usize,
    /// Batch size for export.
    pub batch_size: usize,
    /// Minimum duration_ms to consider an event intentional.
    pub min_event_duration_ms: u32,
    /// Whether to collect events (can be disabled for opt-out).
    pub enabled: bool,
    /// Whether to anonymize session IDs before export.
    pub anonymize: bool,
}

impl Default for CollectorConfig {
    fn default() -> Self {
        CollectorConfig {
            max_buffer_size: 10_000,
            batch_size: 250,
            min_event_duration_ms: 0,
            enabled: true,
            anonymize: false,
        }
    }
}

// ── Data collector ────────────────────────────────────────────────────────────

/// Ring-buffer event collector for production telemetry.
pub struct DataCollector {
    buffer: VecDeque<InteractionEvent>,
    sessions: HashMap<String, SessionStats>,
    config: CollectorConfig,
    total_events_collected: u64,
    total_batches_exported: u32,
    dropped_count: u64,
}

impl DataCollector {
    pub fn new(config: CollectorConfig) -> Self {
        DataCollector {
            buffer: VecDeque::new(),
            sessions: HashMap::new(),
            config,
            total_events_collected: 0,
            total_batches_exported: 0,
            dropped_count: 0,
        }
    }

    /// Record a new interaction event.
    pub fn record(&mut self, event: InteractionEvent) -> bool {
        if !self.config.enabled { return false; }
        if event.duration_ms < self.config.min_event_duration_ms { return false; }

        // Update session stats
        let ts = event.timestamp_secs;
        let sid = event.session_id.clone();
        self.sessions
            .entry(sid.clone())
            .or_insert_with(|| SessionStats::new(sid, ts))
            .record(&event);

        // Ring buffer
        if self.buffer.len() >= self.config.max_buffer_size {
            self.buffer.pop_front();
            self.dropped_count += 1;
        }
        self.buffer.push_back(event);
        self.total_events_collected += 1;
        true
    }

    /// Drain up to `batch_size` events into a DataBatch.
    pub fn flush(&mut self, ts: u64) -> Option<DataBatch> {
        if self.buffer.is_empty() { return None; }
        let n = self.config.batch_size.min(self.buffer.len());
        let events: Vec<InteractionEvent> = self.buffer.drain(..n).collect();
        self.total_batches_exported += 1;
        Some(DataBatch::new(events, ts))
    }

    /// Flush all remaining events into one batch.
    pub fn flush_all(&mut self, ts: u64) -> Option<DataBatch> {
        if self.buffer.is_empty() { return None; }
        let events: Vec<InteractionEvent> = self.buffer.drain(..).collect();
        self.total_batches_exported += 1;
        Some(DataBatch::new(events, ts))
    }

    pub fn buffer_len(&self) -> usize { self.buffer.len() }
    pub fn session_count(&self) -> usize { self.sessions.len() }
    pub fn total_collected(&self) -> u64 { self.total_events_collected }
    pub fn dropped_count(&self) -> u64 { self.dropped_count }
    pub fn batches_exported(&self) -> u32 { self.total_batches_exported }

    pub fn session_stats(&self, session_id: &str) -> Option<&SessionStats> {
        self.sessions.get(session_id)
    }

    pub fn global_acceptance_rate(&self) -> f32 {
        let total_shown: u32 = self.sessions.values().map(|s| s.suggestions_shown).sum();
        let total_accepted: u32 = self.sessions.values().map(|s| s.suggestions_accepted).sum();
        if total_shown == 0 { return 0.0; }
        total_accepted as f32 / total_shown as f32
    }

    pub fn is_enabled(&self) -> bool { self.config.enabled }
    pub fn set_enabled(&mut self, enabled: bool) { self.config.enabled = enabled; }
}

impl Default for DataCollector {
    fn default() -> Self { Self::new(CollectorConfig::default()) }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn event(session: &str, action: &str, ts: u64) -> InteractionEvent {
        InteractionEvent::new(session, action, r#"{"sel":1}"#, ts)
            .with_duration(50)
    }

    #[test]
    fn collector_records_event() {
        let mut c = DataCollector::default();
        assert!(c.record(event("s1", "CreateLayer", 0)));
        assert_eq!(c.buffer_len(), 1);
        assert_eq!(c.total_collected(), 1);
    }

    #[test]
    fn collector_disabled_drops_events() {
        let config = CollectorConfig { enabled: false, ..Default::default() };
        let mut c = DataCollector::new(config);
        assert!(!c.record(event("s1", "CreateLayer", 0)));
        assert_eq!(c.buffer_len(), 0);
    }

    #[test]
    fn collector_min_duration_filter() {
        let config = CollectorConfig { min_event_duration_ms: 100, ..Default::default() };
        let mut c = DataCollector::new(config);
        let e = InteractionEvent::new("s1", "SetFill", "{}", 0).with_duration(50);
        assert!(!c.record(e)); // 50ms < 100ms threshold
    }

    #[test]
    fn flush_returns_batch() {
        let mut c = DataCollector::default();
        c.record(event("s1", "A", 0));
        c.record(event("s1", "B", 1));
        let batch = c.flush(100).unwrap();
        assert_eq!(batch.len(), 2);
        assert_eq!(c.buffer_len(), 0);
    }

    #[test]
    fn flush_empty_returns_none() {
        let mut c = DataCollector::default();
        assert!(c.flush(0).is_none());
    }

    #[test]
    fn buffer_ring_drops_oldest() {
        let config = CollectorConfig { max_buffer_size: 3, ..Default::default() };
        let mut c = DataCollector::new(config);
        for i in 0..5u64 { c.record(event("s1", "A", i)); }
        assert!(c.buffer_len() <= 3);
        assert!(c.dropped_count() > 0);
    }

    #[test]
    fn session_stats_recorded() {
        let mut c = DataCollector::default();
        c.record(event("sess-1", "CreateLayer", 0));
        c.record(event("sess-1", "SetFill", 10));
        c.record(event("sess-2", "MoveLayer", 20));
        let stats = c.session_stats("sess-1").unwrap();
        assert_eq!(stats.event_count, 2);
        assert_eq!(c.session_count(), 2);
    }

    #[test]
    fn suggestion_acceptance_rate() {
        let mut c = DataCollector::default();
        let e1 = InteractionEvent::new("s1", "SetFill", "{}", 0)
            .with_duration(50)
            .with_suggestion_result(true, Some(true));
        let e2 = InteractionEvent::new("s1", "CreateLayer", "{}", 1)
            .with_duration(50)
            .with_suggestion_result(true, Some(false));
        c.record(e1); c.record(e2);
        let stats = c.session_stats("s1").unwrap();
        assert_eq!(stats.suggestions_shown, 2);
        assert_eq!(stats.suggestions_accepted, 1);
        assert_eq!(stats.suggestions_rejected, 1);
        assert!((stats.acceptance_rate() - 0.5).abs() < 0.01);
    }

    #[test]
    fn most_common_action() {
        let mut stats = SessionStats::new("s", 0);
        for _ in 0..3 { stats.record(&event("s", "SetFill", 0)); }
        stats.record(&event("s", "CreateLayer", 1));
        assert_eq!(stats.most_common_action(), Some("SetFill"));
    }

    #[test]
    fn data_batch_counts() {
        let events = vec![
            InteractionEvent::new("s1", "A", "{}", 0).with_suggestion_result(true, Some(true)),
            InteractionEvent::new("s1", "B", "{}", 1).with_suggestion_result(true, Some(false)),
            InteractionEvent::new("s2", "C", "{}", 2),
        ];
        let batch = DataBatch::new(events, 100);
        assert_eq!(batch.len(), 3);
        assert_eq!(batch.accepted_suggestions(), 1);
        assert_eq!(batch.rejected_suggestions(), 1);
        assert_eq!(batch.source_session_ids.len(), 2);
    }

    #[test]
    fn global_acceptance_rate() {
        let mut c = DataCollector::default();
        let e1 = InteractionEvent::new("s1", "A", "{}", 0).with_duration(10).with_suggestion_result(true, Some(true));
        let e2 = InteractionEvent::new("s2", "B", "{}", 1).with_duration(10).with_suggestion_result(true, Some(true));
        let e3 = InteractionEvent::new("s3", "C", "{}", 2).with_duration(10).with_suggestion_result(true, Some(false));
        c.record(e1); c.record(e2); c.record(e3);
        let rate = c.global_acceptance_rate();
        assert!((rate - 2.0/3.0).abs() < 0.01, "Rate: {}", rate);
    }
}
