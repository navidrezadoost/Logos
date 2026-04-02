//! Performance profiler — measure latency, token usage, and memory for sandbox runs.
//!
//! `PerformanceProfiler` wraps a sandbox run and records:
//! - Wall-clock elapsed time (start/stop)
//! - Token usage (input tokens, output tokens, total)
//! - Synthetic memory snapshots (taken at checkpoints)
//!
//! Results are collected in a [`RunMetrics`] summary.

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

// ── Profiler config ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfilerConfig {
    /// Warn when total latency exceeds this threshold (ms).
    pub latency_warn_ms: u64,
    /// Warn when total tokens exceed this threshold.
    pub token_warn_count: u32,
    /// Warn when memory snapshot exceeds this value (bytes).
    pub memory_warn_bytes: usize,
}

impl Default for ProfilerConfig {
    fn default() -> Self {
        Self {
            latency_warn_ms: 5_000,
            token_warn_count: 4_096,
            memory_warn_bytes: 32 * 1024 * 1024, // 32 MiB
        }
    }
}

// ── Token stats ───────────────────────────────────────────────────────────────

/// Token usage statistics for a single sandbox run.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct TokenStats {
    pub input_tokens: u32,
    pub output_tokens: u32,
    /// Any tokens charged for system-prompt overhead.
    pub overhead_tokens: u32,
}

impl TokenStats {
    pub fn new(input: u32, output: u32, overhead: u32) -> Self {
        Self { input_tokens: input, output_tokens: output, overhead_tokens: overhead }
    }

    pub fn total(&self) -> u32 {
        self.input_tokens + self.output_tokens + self.overhead_tokens
    }

    pub fn cost_estimate_usd(&self, price_per_1k: f64) -> f64 {
        self.total() as f64 / 1_000.0 * price_per_1k
    }
}

// ── Memory snapshot ───────────────────────────────────────────────────────────

/// A synthetic memory reading taken at a named checkpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySnapshot {
    pub label: String,
    /// Simulated heap usage in bytes.
    pub heap_bytes: usize,
    /// Timestamp of snapshot (seconds since epoch).
    pub ts_secs: u64,
}

impl MemorySnapshot {
    pub fn new(label: impl Into<String>, heap_bytes: usize) -> Self {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self { label: label.into(), heap_bytes, ts_secs: ts }
    }

    /// Convenience constructor for tests (deterministic timestamp).
    pub fn with_ts(label: impl Into<String>, heap_bytes: usize, ts_secs: u64) -> Self {
        Self { label: label.into(), heap_bytes, ts_secs }
    }
}

// ── Run metrics ───────────────────────────────────────────────────────────────

/// Aggregated performance metrics for a completed sandbox run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunMetrics {
    pub run_id: String,
    /// Elapsed wall-clock time in milliseconds.
    pub elapsed_ms: u64,
    pub tokens: TokenStats,
    pub memory_snapshots: Vec<MemorySnapshot>,
    /// Whether any threshold was exceeded.
    pub has_warnings: bool,
    pub warnings: Vec<String>,
}

impl RunMetrics {
    pub fn peak_memory_bytes(&self) -> usize {
        self.memory_snapshots.iter().map(|s| s.heap_bytes).max().unwrap_or(0)
    }

    pub fn average_memory_bytes(&self) -> usize {
        if self.memory_snapshots.is_empty() { return 0; }
        let total: usize = self.memory_snapshots.iter().map(|s| s.heap_bytes).sum();
        total / self.memory_snapshots.len()
    }

    pub fn time_over_budget(&self, budget_ms: u64) -> bool {
        self.elapsed_ms > budget_ms
    }

    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }
}

// ── Performance profiler ──────────────────────────────────────────────────────

/// Records performance data during a sandbox run.
pub struct PerformanceProfiler {
    pub config: ProfilerConfig,
    run_id: String,
    start_ts_ms: Option<u64>,
    end_ts_ms: Option<u64>,
    tokens: TokenStats,
    snapshots: Vec<MemorySnapshot>,
}

impl PerformanceProfiler {
    pub fn new(run_id: impl Into<String>) -> Self {
        Self::with_config(run_id, ProfilerConfig::default())
    }

    pub fn with_config(run_id: impl Into<String>, config: ProfilerConfig) -> Self {
        Self {
            config,
            run_id: run_id.into(),
            start_ts_ms: None,
            end_ts_ms: None,
            tokens: TokenStats::default(),
            snapshots: Vec::new(),
        }
    }

    // ── Control ───────────────────────────────────────────────────────────────

    /// Start the timer.
    pub fn start(&mut self) {
        self.start_ts_ms = Some(Self::now_ms());
    }

    /// Stop the timer.
    pub fn stop(&mut self) {
        self.end_ts_ms = Some(Self::now_ms());
    }

    /// Record token usage for this run.
    pub fn record_tokens(&mut self, input: u32, output: u32, overhead: u32) {
        self.tokens = TokenStats::new(input, output, overhead);
    }

    /// Take a memory snapshot at a named checkpoint.
    pub fn snapshot_memory(&mut self, label: impl Into<String>, heap_bytes: usize) {
        self.snapshots.push(MemorySnapshot::new(label, heap_bytes));
    }

    /// Take a memory snapshot with an explicit timestamp (for deterministic tests).
    pub fn snapshot_memory_at(&mut self, label: impl Into<String>, heap_bytes: usize, ts: u64) {
        self.snapshots.push(MemorySnapshot::with_ts(label, heap_bytes, ts));
    }

    // ── Results ───────────────────────────────────────────────────────────────

    /// Elapsed time between start and stop (ms). Returns 0 if not yet stopped.
    pub fn elapsed_ms(&self) -> u64 {
        match (self.start_ts_ms, self.end_ts_ms) {
            (Some(s), Some(e)) => e.saturating_sub(s),
            _ => 0,
        }
    }

    /// Produce a `RunMetrics` summary, including threshold warnings.
    pub fn metrics(&self) -> RunMetrics {
        let elapsed = self.elapsed_ms();
        let mut warnings = Vec::new();

        if elapsed > self.config.latency_warn_ms {
            warnings.push(format!(
                "latency {}ms exceeds threshold {}ms",
                elapsed, self.config.latency_warn_ms
            ));
        }
        if self.tokens.total() > self.config.token_warn_count {
            warnings.push(format!(
                "tokens {} exceeds threshold {}",
                self.tokens.total(),
                self.config.token_warn_count
            ));
        }
        let peak = self
            .snapshots
            .iter()
            .map(|s| s.heap_bytes)
            .max()
            .unwrap_or(0);
        if peak > self.config.memory_warn_bytes {
            warnings.push(format!(
                "memory {}B exceeds threshold {}B",
                peak, self.config.memory_warn_bytes
            ));
        }

        RunMetrics {
            run_id: self.run_id.clone(),
            elapsed_ms: elapsed,
            tokens: self.tokens.clone(),
            memory_snapshots: self.snapshots.clone(),
            has_warnings: !warnings.is_empty(),
            warnings,
        }
    }

    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── TokenStats ────────────────────────────────────────────────────────────

    #[test]
    fn token_stats_total() {
        let t = TokenStats::new(100, 200, 50);
        assert_eq!(t.total(), 350);
    }

    #[test]
    fn token_stats_cost_estimate() {
        let t = TokenStats::new(500, 500, 0); // 1000 tokens
        let cost = t.cost_estimate_usd(0.002); // $0.002 / 1k tokens
        assert!((cost - 0.002).abs() < 1e-9);
    }

    #[test]
    fn token_stats_default_zero() {
        let t = TokenStats::default();
        assert_eq!(t.total(), 0);
    }

    // ── MemorySnapshot ────────────────────────────────────────────────────────

    #[test]
    fn memory_snapshot_label_and_bytes() {
        let snap = MemorySnapshot::with_ts("init", 4096, 1000);
        assert_eq!(snap.label, "init");
        assert_eq!(snap.heap_bytes, 4096);
        assert_eq!(snap.ts_secs, 1000);
    }

    // ── PerformanceProfiler ───────────────────────────────────────────────────

    #[test]
    fn profiler_elapsed_before_start_is_zero() {
        let p = PerformanceProfiler::new("run-0");
        assert_eq!(p.elapsed_ms(), 0);
    }

    #[test]
    fn profiler_records_tokens() {
        let mut p = PerformanceProfiler::new("run-1");
        p.record_tokens(100, 200, 0);
        let m = p.metrics();
        assert_eq!(m.tokens.total(), 300);
    }

    #[test]
    fn profiler_memory_snapshots_peak() {
        let mut p = PerformanceProfiler::new("run-2");
        p.snapshot_memory_at("start", 1_000, 0);
        p.snapshot_memory_at("peak",  8_000, 1);
        p.snapshot_memory_at("end",   3_000, 2);
        let m = p.metrics();
        assert_eq!(m.peak_memory_bytes(), 8_000);
    }

    #[test]
    fn profiler_average_memory() {
        let mut p = PerformanceProfiler::new("r");
        p.snapshot_memory_at("a", 1000, 0);
        p.snapshot_memory_at("b", 3000, 1);
        let m = p.metrics();
        assert_eq!(m.average_memory_bytes(), 2000);
    }

    #[test]
    fn profiler_no_snapshots_peak_zero() {
        let p = PerformanceProfiler::new("r");
        assert_eq!(p.metrics().peak_memory_bytes(), 0);
    }

    #[test]
    fn profiler_token_warning_triggered() {
        let config = ProfilerConfig { token_warn_count: 100, ..ProfilerConfig::default() };
        let mut p = PerformanceProfiler::with_config("r", config);
        p.record_tokens(50, 60, 0); // total = 110 > 100
        let m = p.metrics();
        assert!(m.has_warnings);
        assert!(m.warnings.iter().any(|w| w.contains("tokens")));
    }

    #[test]
    fn profiler_memory_warning_triggered() {
        let config = ProfilerConfig { memory_warn_bytes: 1000, ..ProfilerConfig::default() };
        let mut p = PerformanceProfiler::with_config("r", config);
        p.snapshot_memory_at("peak", 2000, 0);
        let m = p.metrics();
        assert!(m.has_warnings);
        assert!(m.warnings.iter().any(|w| w.contains("memory")));
    }

    #[test]
    fn profiler_no_warnings_clean_run() {
        let mut p = PerformanceProfiler::new("clean");
        p.record_tokens(10, 20, 0);
        p.snapshot_memory_at("it", 1024, 0);
        let m = p.metrics();
        assert!(!m.has_warnings);
    }

    #[test]
    fn run_metrics_time_over_budget() {
        let mut p = PerformanceProfiler::new("r");
        // Manually inject start/end by using the internal field via a helper
        let m = RunMetrics {
            run_id: "r".into(),
            elapsed_ms: 6000,
            tokens: TokenStats::default(),
            memory_snapshots: vec![],
            has_warnings: false,
            warnings: vec![],
        };
        assert!(m.time_over_budget(5000));
        assert!(!m.time_over_budget(7000));
    }

    #[test]
    fn run_metrics_to_json() {
        let m = RunMetrics {
            run_id: "r1".into(),
            elapsed_ms: 123,
            tokens: TokenStats::new(10, 20, 0),
            memory_snapshots: vec![],
            has_warnings: false,
            warnings: vec![],
        };
        let json = m.to_json().unwrap();
        assert!(json.contains("\"run_id\""));
        assert!(json.contains("\"elapsed_ms\""));
    }
}
