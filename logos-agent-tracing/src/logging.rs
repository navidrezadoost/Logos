//! Structured JSON logging for agent operations.
//!
//! Provides rate-limited, level-filtered, structured log emission
//! that can be wired to any `log`-compatible backend.  All log
//! records are serialisable to JSON for ingestion by Loki, Datadog,
//! or any NDJSON log aggregator.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// ── Log level ────────────────────────────────────────────────────────────────

/// Logging verbosity level (ordered from least to most verbose).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Error = 0,
    Warn  = 1,
    Info  = 2,
    Debug = 3,
    Trace = 4,
}

impl LogLevel {
    /// Return the canonical string label used in JSON output.
    pub fn as_str(self) -> &'static str {
        match self {
            LogLevel::Error => "ERROR",
            LogLevel::Warn  => "WARN",
            LogLevel::Info  => "INFO",
            LogLevel::Debug => "DEBUG",
            LogLevel::Trace => "TRACE",
        }
    }
}

// ── Log record ───────────────────────────────────────────────────────────────

/// A single structured log record ready for JSON serialisation.
#[derive(Debug, Clone)]
pub struct LogRecord {
    /// ISO-8601 timestamp (seconds since Unix epoch for simplicity here).
    pub timestamp_epoch_secs: u64,
    pub level: LogLevel,
    pub target: String,
    pub message: String,
    /// Arbitrary key-value fields (serialised as JSON object).
    pub fields: HashMap<String, String>,
    /// Optional trace-context correlation.
    pub trace_id: Option<String>,
    pub span_id:  Option<String>,
}

impl LogRecord {
    /// Serialise to a compact JSON string (single line, NDJSON-compatible).
    pub fn to_json(&self) -> String {
        let fields_json = self
            .fields
            .iter()
            .map(|(k, v)| format!("\"{}\":\"{}\"", k, v))
            .collect::<Vec<_>>()
            .join(",");

        let trace = self
            .trace_id
            .as_deref()
            .map(|t| format!(",\"trace_id\":\"{}\"", t))
            .unwrap_or_default();

        let span = self
            .span_id
            .as_deref()
            .map(|s| format!(",\"span_id\":\"{}\"", s))
            .unwrap_or_default();

        format!(
            "{{\"ts\":{},\"level\":\"{}\",\"target\":\"{}\",\"msg\":\"{}\"{}{}{}}}",
            self.timestamp_epoch_secs,
            self.level.as_str(),
            self.target,
            self.message.replace('"', "\\\""),
            if fields_json.is_empty() { String::new() } else { format!(",{}", fields_json) },
            trace,
            span,
        )
    }
}

// ── Rate limiter ─────────────────────────────────────────────────────────────

/// Token-bucket rate limiter — allows `capacity` log records per `window`.
#[derive(Debug, Clone)]
pub struct LogRateLimiter {
    capacity: usize,
    window: Duration,
    tokens: usize,
    last_refill: Instant,
}

impl LogRateLimiter {
    pub fn new(capacity: usize, window: Duration) -> Self {
        Self { capacity, window, tokens: capacity, last_refill: Instant::now() }
    }

    /// Returns `true` if a log record is allowed through; `false` if rate-limited.
    pub fn allow(&mut self) -> bool {
        let now = Instant::now();
        if now.duration_since(self.last_refill) >= self.window {
            self.tokens = self.capacity;
            self.last_refill = now;
        }
        if self.tokens > 0 {
            self.tokens -= 1;
            true
        } else {
            false
        }
    }

    /// Current remaining token count.
    pub fn remaining(&self) -> usize {
        self.tokens
    }
}

// ── Structured logger ─────────────────────────────────────────────────────────

/// Builder for constructing a [`LogRecord`] and emitting it through the logger.
pub struct LogBuilder<'a> {
    logger: &'a StructuredLogger,
    level: LogLevel,
    target: String,
    message: String,
    fields: HashMap<String, String>,
    trace_id: Option<String>,
    span_id:  Option<String>,
}

impl<'a> LogBuilder<'a> {
    pub fn field(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.fields.insert(key.into(), value.into());
        self
    }

    pub fn trace(mut self, trace_id: impl Into<String>, span_id: impl Into<String>) -> Self {
        self.trace_id = Some(trace_id.into());
        self.span_id  = Some(span_id.into());
        self
    }

    /// Finalise and emit the record.  Returns `true` if the record was accepted
    /// (not filtered by level or rate limiter).
    pub fn emit(self) -> bool {
        self.logger.emit_record(LogRecord {
            timestamp_epoch_secs: epoch_secs(),
            level: self.level,
            target: self.target,
            message: self.message,
            fields: self.fields,
            trace_id: self.trace_id,
            span_id: self.span_id,
        })
    }
}

/// Thread-safe structured logger that stores emitted records in memory.
#[derive(Debug, Clone)]
pub struct StructuredLogger {
    inner: Arc<Mutex<LoggerInner>>,
}

#[derive(Debug)]
struct LoggerInner {
    min_level: LogLevel,
    records: Vec<LogRecord>,
    limiter: Option<LogRateLimiter>,
    dropped: usize,
}

impl StructuredLogger {
    /// Create a logger that accepts records at or above `min_level`.
    pub fn new(min_level: LogLevel) -> Self {
        Self {
            inner: Arc::new(Mutex::new(LoggerInner {
                min_level,
                records: Vec::new(),
                limiter: None,
                dropped: 0,
            })),
        }
    }

    /// Attach a rate limiter (`capacity` records per `window`).
    pub fn with_rate_limit(self, capacity: usize, window: Duration) -> Self {
        self.inner.lock().unwrap().limiter = Some(LogRateLimiter::new(capacity, window));
        self
    }

    /// Start building a log record at the given level.
    pub fn log<'a>(&'a self, level: LogLevel, target: &'a str, message: impl Into<String>) -> LogBuilder<'a> {
        LogBuilder {
            logger: self,
            level,
            target: target.to_string(),
            message: message.into(),
            fields: HashMap::new(),
            trace_id: None,
            span_id: None,
        }
    }

    fn emit_record(&self, record: LogRecord) -> bool {
        let mut inner = self.inner.lock().unwrap();
        if record.level > inner.min_level {
            return false; // filtered by level
        }
        if let Some(ref mut lim) = inner.limiter {
            if !lim.allow() {
                inner.dropped += 1;
                return false;
            }
        }
        inner.records.push(record);
        true
    }

    /// Drain all stored records (clears internal buffer).
    pub fn drain(&self) -> Vec<LogRecord> {
        let mut inner = self.inner.lock().unwrap();
        std::mem::take(&mut inner.records)
    }

    /// Number of stored records (without draining).
    pub fn record_count(&self) -> usize {
        self.inner.lock().unwrap().records.len()
    }

    /// Number of records dropped by the rate limiter since last reset.
    pub fn dropped_count(&self) -> usize {
        self.inner.lock().unwrap().dropped
    }

    /// Change the minimum log level at runtime.
    pub fn set_min_level(&self, level: LogLevel) {
        self.inner.lock().unwrap().min_level = level;
    }
}

fn epoch_secs() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// LOG-01  Records at or above min_level are accepted.
    #[test]
    fn log01_level_filtering_accepts() {
        let logger = StructuredLogger::new(LogLevel::Info);
        assert!(logger.log(LogLevel::Error, "app", "critical failure").emit());
        assert!(logger.log(LogLevel::Warn,  "app", "degraded").emit());
        assert!(logger.log(LogLevel::Info,  "app", "started").emit());
        assert_eq!(logger.record_count(), 3);
    }

    /// LOG-02  Records below min_level are silently dropped.
    #[test]
    fn log02_level_filtering_rejects() {
        let logger = StructuredLogger::new(LogLevel::Warn);
        logger.log(LogLevel::Info,  "app", "verbose").emit();
        logger.log(LogLevel::Debug, "app", "detail").emit();
        logger.log(LogLevel::Trace, "app", "trace").emit();
        assert_eq!(logger.record_count(), 0);
    }

    /// LOG-03  Structured fields appear in JSON output.
    #[test]
    fn log03_fields_in_json() {
        let logger = StructuredLogger::new(LogLevel::Debug);
        logger.log(LogLevel::Info, "agent", "publish")
            .field("agent_id", "abc-123")
            .field("version", "1.2.3")
            .emit();
        let records = logger.drain();
        let json = records[0].to_json();
        assert!(json.contains("\"agent_id\":\"abc-123\""), "json: {}", json);
        assert!(json.contains("\"version\":\"1.2.3\""), "json: {}", json);
    }

    /// LOG-04  JSON output includes level as expected string.
    #[test]
    fn log04_json_level_label() {
        let logger = StructuredLogger::new(LogLevel::Debug);
        logger.log(LogLevel::Error, "agent", "crashed").emit();
        let json = logger.drain()[0].to_json();
        assert!(json.contains("\"level\":\"ERROR\""), "json: {}", json);
    }

    /// LOG-05  Trace context (trace_id + span_id) is emitted in JSON.
    #[test]
    fn log05_trace_context_json() {
        let logger = StructuredLogger::new(LogLevel::Debug);
        logger.log(LogLevel::Info, "collab", "op accepted")
            .trace("trace-abc", "span-xyz")
            .emit();
        let json = logger.drain()[0].to_json();
        assert!(json.contains("\"trace_id\":\"trace-abc\""), "json: {}", json);
        assert!(json.contains("\"span_id\":\"span-xyz\""), "json: {}", json);
    }

    /// LOG-06  Rate limiter blocks excess records.
    #[test]
    fn log06_rate_limiter_drops_excess() {
        let logger = StructuredLogger::new(LogLevel::Debug)
            .with_rate_limit(3, Duration::from_secs(60));
        for _ in 0..10 {
            logger.log(LogLevel::Info, "app", "burst").emit();
        }
        assert_eq!(logger.record_count(), 3);
        assert_eq!(logger.dropped_count(), 7);
    }

    /// LOG-07  Rate limiter allows records up to capacity.
    #[test]
    fn log07_rate_limiter_within_capacity() {
        let logger = StructuredLogger::new(LogLevel::Debug)
            .with_rate_limit(5, Duration::from_secs(60));
        for _ in 0..5 {
            assert!(logger.log(LogLevel::Info, "app", "ok").emit());
        }
        assert_eq!(logger.dropped_count(), 0);
    }

    /// LOG-08  drain() clears the internal buffer.
    #[test]
    fn log08_drain_clears_buffer() {
        let logger = StructuredLogger::new(LogLevel::Info);
        logger.log(LogLevel::Info, "app", "one").emit();
        logger.log(LogLevel::Warn, "app", "two").emit();
        assert_eq!(logger.drain().len(), 2);
        assert_eq!(logger.record_count(), 0);
    }

    /// LOG-09  set_min_level changes filtering at runtime.
    #[test]
    fn log09_dynamic_level_change() {
        let logger = StructuredLogger::new(LogLevel::Error);
        logger.log(LogLevel::Debug, "app", "ignored").emit();
        assert_eq!(logger.record_count(), 0);
        logger.set_min_level(LogLevel::Debug);
        logger.log(LogLevel::Debug, "app", "accepted").emit();
        assert_eq!(logger.record_count(), 1);
    }

    /// LOG-10  Message with quotes is correctly escaped in JSON.
    #[test]
    fn log10_json_message_escape() {
        let logger = StructuredLogger::new(LogLevel::Info);
        logger.log(LogLevel::Warn, "app", r#"agent said "hello""#).emit();
        let json = logger.drain()[0].to_json();
        // Ensure the JSON string is well-formed (no unescaped quotes mid-value)
        assert!(json.contains(r#"\"hello\""#), "json: {}", json);
    }
}
