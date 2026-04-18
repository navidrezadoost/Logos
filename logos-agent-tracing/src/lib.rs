//! # logos-agent-tracing — Agent Observability & Tracing
//!
//! Distributed span collection, latency histograms, error-rate tracking,
//! and webhook alerting for Logos agent operations.
//!
//! ## Quick start
//!
//! ```rust
//! use logos_agent_tracing::{
//!     Tracer, SpanKind,
//!     SpanCollector,
//!     LatencyHistogram,
//!     AlertCondition, AlertEvaluator,
//! };
//!
//! // Record a span
//! let mut tracer = Tracer::new("agent-invoke");
//! let span = tracer.start_span("invoke", SpanKind::Internal);
//! let span_id = span.span_id.clone();
//! tracer.finish_span(span_id, true);
//!
//! // Collect spans into a store
//! let mut collector = SpanCollector::new();
//! for s in tracer.drain() { collector.record(s); }
//! assert!(collector.span_count() > 0);
//!
//! // Track latency
//! let mut hist = LatencyHistogram::new();
//! hist.record_ms(42.0);
//! assert!(hist.p50_ms() >= 0.0);
//!
//! // Evaluate an alert
//! let cond = AlertCondition::error_rate_exceeds(0.5);
//! let eval = AlertEvaluator::new();
//! let fired = eval.evaluate(&cond, 0.6);
//! assert!(fired);
//! ```

pub mod span;
pub mod collector;
pub mod histogram;
pub mod alert;
pub mod logging;

pub use span::{Tracer, Span, SpanContext, SpanId, TraceId, SpanKind, SpanStatus, SpanEvent, TracerError};
pub use collector::{SpanCollector, TraceStore, TraceQuery, CollectorError};
pub use histogram::{LatencyHistogram, ErrorRateTracker, BucketBounds, HistogramSnapshot};
pub use alert::{AlertCondition, AlertEvaluator, AlertSeverity, AlertFired, WebhookNotifier, NotifierError, ConditionKind};
pub use logging::{StructuredLogger, LogRecord, LogLevel, LogRateLimiter};
