//! Span primitives — TraceId, SpanId, Span, Tracer.

use thiserror::Error;

/// Errors from tracer operations.
#[derive(Debug, Error, PartialEq)]
pub enum TracerError {
    #[error("span '{0}' not found")]
    SpanNotFound(String),
    #[error("span '{0}' is already finished")]
    AlreadyFinished(String),
    #[error("tracer service name is empty")]
    EmptyServiceName,
}

/// A 128-bit trace identifier represented as a hex string.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TraceId(pub String);

impl TraceId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A 64-bit span identifier represented as a hex string.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SpanId(pub String);

impl SpanId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Propagation context that links a span to its trace and parent.
#[derive(Debug, Clone)]
pub struct SpanContext {
    pub trace_id:      TraceId,
    pub span_id:       SpanId,
    pub parent_span_id: Option<SpanId>,
    /// Sampling flag: true = record this trace.
    pub sampled:       bool,
}

impl SpanContext {
    pub fn new_root(trace_id: TraceId, span_id: SpanId) -> Self {
        Self { trace_id, span_id, parent_span_id: None, sampled: true }
    }

    pub fn new_child(trace_id: TraceId, span_id: SpanId, parent: SpanId) -> Self {
        Self { trace_id, span_id, parent_span_id: Some(parent), sampled: true }
    }

    pub fn is_root(&self) -> bool {
        self.parent_span_id.is_none()
    }
}

/// Semantic kind of a span.
#[derive(Debug, Clone, PartialEq)]
pub enum SpanKind {
    Client,
    Server,
    Producer,
    Consumer,
    Internal,
}

impl SpanKind {
    pub fn label(&self) -> &'static str {
        match self {
            SpanKind::Client   => "CLIENT",
            SpanKind::Server   => "SERVER",
            SpanKind::Producer => "PRODUCER",
            SpanKind::Consumer => "CONSUMER",
            SpanKind::Internal => "INTERNAL",
        }
    }
}

/// Completion status of a span.
#[derive(Debug, Clone, PartialEq)]
pub enum SpanStatus {
    Unset,
    Ok,
    Error { message: String },
}

impl SpanStatus {
    pub fn is_error(&self) -> bool {
        matches!(self, SpanStatus::Error { .. })
    }
}

/// An annotated event attached to a span.
#[derive(Debug, Clone)]
pub struct SpanEvent {
    pub name:       String,
    pub timestamp_ms: f64,
    pub attributes: Vec<(String, String)>,
}

impl SpanEvent {
    pub fn new(name: impl Into<String>, timestamp_ms: f64) -> Self {
        Self { name: name.into(), timestamp_ms, attributes: Vec::new() }
    }

    pub fn with_attr(mut self, key: impl Into<String>, val: impl Into<String>) -> Self {
        self.attributes.push((key.into(), val.into()));
        self
    }
}

/// A single unit of work with timing and metadata.
#[derive(Debug, Clone)]
pub struct Span {
    pub context:       SpanContext,
    pub span_id:       SpanId,
    pub name:          String,
    pub kind:          SpanKind,
    pub start_ms:      f64,
    pub end_ms:        Option<f64>,
    pub status:        SpanStatus,
    pub events:        Vec<SpanEvent>,
    pub attributes:    Vec<(String, String)>,
    pub service_name:  String,
}

impl Span {
    /// Duration in milliseconds, or None if not finished.
    pub fn duration_ms(&self) -> Option<f64> {
        self.end_ms.map(|e| e - self.start_ms)
    }

    /// Whether the span has been finished.
    pub fn is_finished(&self) -> bool {
        self.end_ms.is_some()
    }

    pub fn add_event(&mut self, event: SpanEvent) {
        self.events.push(event);
    }

    pub fn set_attribute(&mut self, key: impl Into<String>, val: impl Into<String>) {
        self.attributes.push((key.into(), val.into()));
    }

    pub fn set_status_ok(&mut self) {
        self.status = SpanStatus::Ok;
    }

    pub fn set_status_error(&mut self, message: impl Into<String>) {
        self.status = SpanStatus::Error { message: message.into() };
    }
}

/// Monotonic clock tick counter for generating deterministic IDs in tests.
static COUNTER: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(1);

fn next_id() -> u64 {
    COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Creates spans associated with a service.
#[derive(Debug)]
pub struct Tracer {
    pub service_name: String,
    pub spans: Vec<Span>,
    /// Current time in ms (injectable for testing).
    pub now_ms: f64,
}

impl Tracer {
    /// Create a tracer for the given service name.
    pub fn new(service_name: impl Into<String>) -> Self {
        Self { service_name: service_name.into(), spans: Vec::new(), now_ms: 0.0 }
    }

    /// Create with explicit start time (deterministic tests).
    pub fn new_at(service_name: impl Into<String>, now_ms: f64) -> Self {
        Self { service_name: service_name.into(), spans: Vec::new(), now_ms }
    }

    /// Start a new root span.
    pub fn start_span(&mut self, name: impl Into<String>, kind: SpanKind) -> &Span {
        let trace_id = TraceId::new(format!("{:032x}", next_id()));
        let span_id  = SpanId::new(format!("{:016x}", next_id()));
        let ctx      = SpanContext::new_root(trace_id, span_id.clone());
        let span = Span {
            context: ctx,
            span_id,
            name: name.into(),
            kind,
            start_ms: self.now_ms,
            end_ms: None,
            status: SpanStatus::Unset,
            events: Vec::new(),
            attributes: Vec::new(),
            service_name: self.service_name.clone(),
        };
        self.spans.push(span);
        self.spans.last().unwrap()
    }

    /// Start a child span under a parent span id.
    pub fn start_child_span(
        &mut self,
        name: impl Into<String>,
        kind: SpanKind,
        parent_span_id: SpanId,
        trace_id: TraceId,
    ) -> &Span {
        let span_id = SpanId::new(format!("{:016x}", next_id()));
        let ctx = SpanContext::new_child(trace_id, span_id.clone(), parent_span_id);
        let span = Span {
            context: ctx,
            span_id,
            name: name.into(),
            kind,
            start_ms: self.now_ms,
            end_ms: None,
            status: SpanStatus::Unset,
            events: Vec::new(),
            attributes: Vec::new(),
            service_name: self.service_name.clone(),
        };
        self.spans.push(span);
        self.spans.last().unwrap()
    }

    /// Finish a span by id. `success` sets Ok or Error accordingly.
    pub fn finish_span(&mut self, span_id: SpanId, success: bool) -> Result<(), TracerError> {
        let span = self.spans.iter_mut()
            .find(|s| s.span_id == span_id)
            .ok_or_else(|| TracerError::SpanNotFound(span_id.0.clone()))?;
        if span.is_finished() {
            return Err(TracerError::AlreadyFinished(span_id.0));
        }
        span.end_ms = Some(self.now_ms);
        if success { span.set_status_ok() } else { span.set_status_error("operation failed") }
        Ok(())
    }

    /// Finish at an explicit time.
    pub fn finish_span_at(&mut self, span_id: SpanId, end_ms: f64, success: bool) -> Result<(), TracerError> {
        let span = self.spans.iter_mut()
            .find(|s| s.span_id == span_id)
            .ok_or_else(|| TracerError::SpanNotFound(span_id.0.clone()))?;
        if span.is_finished() {
            return Err(TracerError::AlreadyFinished(span_id.0));
        }
        span.end_ms = Some(end_ms);
        if success { span.set_status_ok() } else { span.set_status_error("operation failed") }
        Ok(())
    }

    /// Drain all finished spans out of the tracer.
    pub fn drain(&mut self) -> Vec<Span> {
        let (finished, open): (Vec<_>, Vec<_>) =
            self.spans.drain(..).partition(|s| s.is_finished());
        self.spans = open;
        finished
    }

    /// Number of open (not yet finished) spans.
    pub fn open_span_count(&self) -> usize {
        self.spans.iter().filter(|s| !s.is_finished()).count()
    }

    /// Total span count (open + finished, not yet drained).
    pub fn span_count(&self) -> usize {
        self.spans.len()
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracer_starts_span() {
        let mut t = Tracer::new("svc");
        let s = t.start_span("op", SpanKind::Internal);
        assert_eq!(s.name, "op");
        assert!(!s.is_finished());
    }

    #[test]
    fn tracer_finishes_span() {
        let mut t = Tracer::new_at("svc", 100.0);
        let id = t.start_span("op", SpanKind::Client).span_id.clone();
        t.now_ms = 200.0;
        t.finish_span(id, true).unwrap();
        assert_eq!(t.spans[0].duration_ms(), Some(100.0));
    }

    #[test]
    fn finish_unknown_span_errors() {
        let mut t = Tracer::new("svc");
        let err = t.finish_span(SpanId::new("bad"), true);
        assert!(matches!(err, Err(TracerError::SpanNotFound(_))));
    }

    #[test]
    fn double_finish_errors() {
        let mut t = Tracer::new("svc");
        let id = t.start_span("op", SpanKind::Internal).span_id.clone();
        t.finish_span(id.clone(), true).unwrap();
        assert!(matches!(t.finish_span(id, true), Err(TracerError::AlreadyFinished(_))));
    }

    #[test]
    fn drain_returns_only_finished() {
        let mut t = Tracer::new("svc");
        let id = t.start_span("a", SpanKind::Internal).span_id.clone();
        t.start_span("b", SpanKind::Internal);
        t.finish_span(id, true).unwrap();
        let drained = t.drain();
        assert_eq!(drained.len(), 1);
        assert_eq!(t.open_span_count(), 1);
    }

    #[test]
    fn child_span_references_parent() {
        let mut t = Tracer::new("svc");
        let parent = t.start_span("parent", SpanKind::Server).span_id.clone();
        let trace_id = t.spans[0].context.trace_id.clone();
        let child = t.start_child_span("child", SpanKind::Internal, parent.clone(), trace_id);
        assert_eq!(child.context.parent_span_id, Some(parent));
    }

    #[test]
    fn span_status_error_flag() {
        let mut t = Tracer::new("svc");
        let id = t.start_span("op", SpanKind::Internal).span_id.clone();
        t.finish_span(id, false).unwrap();
        assert!(t.spans[0].status.is_error());
    }

    #[test]
    fn span_kind_labels() {
        for (k, label) in &[
            (SpanKind::Client,   "CLIENT"),
            (SpanKind::Server,   "SERVER"),
            (SpanKind::Internal, "INTERNAL"),
        ] {
            assert_eq!(k.label(), *label);
        }
    }

    #[test]
    fn span_context_is_root_when_no_parent() {
        let ctx = SpanContext::new_root(TraceId::new("t1"), SpanId::new("s1"));
        assert!(ctx.is_root());
    }

    #[test]
    fn span_event_attributes() {
        let ev = SpanEvent::new("db.query", 10.0)
            .with_attr("sql", "SELECT 1");
        assert_eq!(ev.attributes[0].0, "sql");
    }

    #[test]
    fn finish_span_at_explicit_time() {
        let mut t = Tracer::new_at("svc", 0.0);
        let id = t.start_span("x", SpanKind::Internal).span_id.clone();
        t.finish_span_at(id, 500.0, true).unwrap();
        assert_eq!(t.spans[0].duration_ms(), Some(500.0));
    }
}
