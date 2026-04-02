//! Span collection — SpanCollector, TraceStore, queries.

use std::collections::HashMap;
use crate::span::{Span, TraceId};
use thiserror::Error;

/// Errors from collector operations.
#[derive(Debug, Error, PartialEq)]
pub enum CollectorError {
    #[error("trace '{0}' not found")]
    TraceNotFound(String),
    #[error("no spans recorded for service '{0}'")]
    NoSpansForService(String),
}

/// Query parameters for filtering stored spans.
#[derive(Debug, Default, Clone)]
pub struct TraceQuery {
    pub service_name: Option<String>,
    pub min_duration_ms: Option<f64>,
    pub errors_only: bool,
    pub operation_name: Option<String>,
}

impl TraceQuery {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn service(mut self, name: impl Into<String>) -> Self {
        self.service_name = Some(name.into());
        self
    }

    pub fn min_duration(mut self, ms: f64) -> Self {
        self.min_duration_ms = Some(ms);
        self
    }

    pub fn errors_only(mut self) -> Self {
        self.errors_only = true;
        self
    }

    pub fn operation(mut self, op: impl Into<String>) -> Self {
        self.operation_name = Some(op.into());
        self
    }
}

/// Groups spans belonging to the same trace.
#[derive(Debug, Clone)]
pub struct TraceStore {
    pub trace_id: TraceId,
    pub spans:    Vec<Span>,
}

impl TraceStore {
    pub fn new(trace_id: TraceId) -> Self {
        Self { trace_id, spans: Vec::new() }
    }

    /// Total duration from first start to last end (finished spans only).
    pub fn total_duration_ms(&self) -> Option<f64> {
        let start = self.spans.iter()
            .map(|s| s.start_ms)
            .fold(f64::INFINITY, f64::min);
        let end = self.spans.iter()
            .filter_map(|s| s.end_ms)
            .fold(f64::NEG_INFINITY, f64::max);
        if end > start { Some(end - start) } else { None }
    }

    /// True if any span in this trace is an error.
    pub fn has_error(&self) -> bool {
        self.spans.iter().any(|s| s.status.is_error())
    }

    /// Number of spans.
    pub fn span_count(&self) -> usize {
        self.spans.len()
    }
}

/// Collects spans, groups them by trace, and supports querying.
#[derive(Debug, Default)]
pub struct SpanCollector {
    // trace_id string → store
    traces: HashMap<String, TraceStore>,
}

impl SpanCollector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a single finished span.
    pub fn record(&mut self, span: Span) {
        let key = span.context.trace_id.0.clone();
        self.traces
            .entry(key.clone())
            .or_insert_with(|| TraceStore::new(span.context.trace_id.clone()))
            .spans
            .push(span);
    }

    /// Record multiple spans at once.
    pub fn record_all(&mut self, spans: impl IntoIterator<Item = Span>) {
        for s in spans { self.record(s); }
    }

    /// Look up a specific trace.
    pub fn trace(&self, trace_id: &str) -> Result<&TraceStore, CollectorError> {
        self.traces.get(trace_id)
            .ok_or_else(|| CollectorError::TraceNotFound(trace_id.to_owned()))
    }

    /// Total spans across all traces.
    pub fn span_count(&self) -> usize {
        self.traces.values().map(|t| t.span_count()).sum()
    }

    /// Number of distinct traces.
    pub fn trace_count(&self) -> usize {
        self.traces.len()
    }

    /// Run a query, returning matching spans (cloned).
    pub fn query(&self, q: &TraceQuery) -> Vec<Span> {
        self.traces.values()
            .flat_map(|t| t.spans.iter())
            .filter(|s| {
                if let Some(ref svc) = q.service_name {
                    if &s.service_name != svc { return false; }
                }
                if let Some(min_ms) = q.min_duration_ms {
                    match s.duration_ms() {
                        Some(d) if d >= min_ms => {}
                        _ => return false,
                    }
                }
                if q.errors_only && !s.status.is_error() {
                    return false;
                }
                if let Some(ref op) = q.operation_name {
                    if &s.name != op { return false; }
                }
                true
            })
            .cloned()
            .collect()
    }

    /// Error rate across all recorded finished spans (errors / total).
    pub fn global_error_rate(&self) -> f64 {
        let total: usize = self.traces.values().map(|t| t.span_count()).sum();
        if total == 0 { return 0.0; }
        let errors: usize = self.traces.values()
            .flat_map(|t| t.spans.iter())
            .filter(|s| s.status.is_error())
            .count();
        errors as f64 / total as f64
    }

    /// Average duration of all finished spans (ms).
    pub fn avg_duration_ms(&self) -> f64 {
        let durations: Vec<f64> = self.traces.values()
            .flat_map(|t| t.spans.iter())
            .filter_map(|s| s.duration_ms())
            .collect();
        if durations.is_empty() { return 0.0; }
        durations.iter().sum::<f64>() / durations.len() as f64
    }

    /// Slowest trace by total duration.
    pub fn slowest_trace(&self) -> Option<&TraceStore> {
        self.traces.values()
            .filter_map(|t| t.total_duration_ms().map(|d| (d, t)))
            .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap())
            .map(|(_, t)| t)
    }

    /// Clear all data.
    pub fn reset(&mut self) {
        self.traces.clear();
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::{Tracer, SpanKind};

    fn make_spans(svc: &str, op: &str, duration_ms: f64, success: bool) -> Vec<Span> {
        let mut t = Tracer::new_at(svc, 0.0);
        let id = t.start_span(op, SpanKind::Internal).span_id.clone();
        t.finish_span_at(id, duration_ms, success).unwrap();
        t.drain()
    }

    #[test]
    fn record_and_count_span() {
        let mut c = SpanCollector::new();
        c.record_all(make_spans("svc", "op", 50.0, true));
        assert_eq!(c.span_count(), 1);
    }

    #[test]
    fn record_multiple_spans_same_tracer() {
        let mut t = Tracer::new_at("svc", 0.0);
        let id1 = t.start_span("a", SpanKind::Internal).span_id.clone();
        let id2 = t.start_span("b", SpanKind::Internal).span_id.clone();
        t.finish_span_at(id1, 10.0, true).unwrap();
        t.finish_span_at(id2, 20.0, true).unwrap();
        let spans = t.drain();
        let mut c = SpanCollector::new();
        c.record_all(spans);
        assert_eq!(c.span_count(), 2);
    }

    #[test]
    fn trace_lookup() {
        let mut c = SpanCollector::new();
        let spans = make_spans("svc", "op", 10.0, true);
        let tid = spans[0].context.trace_id.0.clone();
        c.record_all(spans);
        assert!(c.trace(&tid).is_ok());
    }

    #[test]
    fn trace_not_found_error() {
        let c = SpanCollector::new();
        assert!(matches!(c.trace("missing"), Err(CollectorError::TraceNotFound(_))));
    }

    #[test]
    fn query_by_service() {
        let mut c = SpanCollector::new();
        c.record_all(make_spans("svc-a", "op", 10.0, true));
        c.record_all(make_spans("svc-b", "op", 10.0, true));
        let q = TraceQuery::new().service("svc-a");
        assert_eq!(c.query(&q).len(), 1);
    }

    #[test]
    fn query_by_min_duration() {
        let mut c = SpanCollector::new();
        c.record_all(make_spans("svc", "op", 10.0, true));
        c.record_all(make_spans("svc", "op", 500.0, true));
        let q = TraceQuery::new().min_duration(100.0);
        assert_eq!(c.query(&q).len(), 1);
    }

    #[test]
    fn query_errors_only() {
        let mut c = SpanCollector::new();
        c.record_all(make_spans("svc", "op", 10.0, true));
        c.record_all(make_spans("svc", "op", 10.0, false));
        let q = TraceQuery::new().errors_only();
        assert_eq!(c.query(&q).len(), 1);
    }

    #[test]
    fn global_error_rate() {
        let mut c = SpanCollector::new();
        c.record_all(make_spans("svc", "op", 10.0, true));
        c.record_all(make_spans("svc", "op", 10.0, false));
        // 1 error out of 2 spans = 0.5
        assert!((c.global_error_rate() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn avg_duration_ms() {
        let mut c = SpanCollector::new();
        c.record_all(make_spans("svc", "op", 100.0, true));
        c.record_all(make_spans("svc", "op", 200.0, true));
        assert!((c.avg_duration_ms() - 150.0).abs() < 1e-9);
    }

    #[test]
    fn slowest_trace_identified() {
        let mut c = SpanCollector::new();
        c.record_all(make_spans("svc", "fast", 10.0, true));
        c.record_all(make_spans("svc", "slow", 999.0, true));
        let slowest = c.slowest_trace().unwrap();
        assert_eq!(slowest.spans[0].name, "slow");
    }

    #[test]
    fn reset_clears_all() {
        let mut c = SpanCollector::new();
        c.record_all(make_spans("svc", "op", 10.0, true));
        c.reset();
        assert_eq!(c.span_count(), 0);
        assert_eq!(c.trace_count(), 0);
    }
}
