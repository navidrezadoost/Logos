//! # Cross-Crate Profiling Framework
//!
//! Scoped timers, metric accumulators, and a global registry so
//! every crate can contribute timing data to a single report.
//!
//! - [`ScopedTimer`] — RAII timer that records elapsed time on drop.
//! - [`MetricAccumulator`] — sliding-window statistics (mean, min,
//!   max, p95) for a named metric.
//! - [`PerfSnapshot`] — point-in-time snapshot of all registered
//!   metrics.
//! - [`PerfRegistry`] — named collection of accumulators.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::Instant;

// ── Timing Result ────────────────────────────────────────────────────

/// The result of a single timed span.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TimingResult {
    /// Elapsed wall-clock nanoseconds.
    pub elapsed_ns: u64,
    /// Optional item count (for throughput calculation).
    pub item_count: Option<u64>,
}

impl TimingResult {
    pub fn new(elapsed_ns: u64) -> Self {
        Self {
            elapsed_ns,
            item_count: None,
        }
    }

    pub fn with_items(elapsed_ns: u64, items: u64) -> Self {
        Self {
            elapsed_ns,
            item_count: Some(items),
        }
    }

    pub fn elapsed_us(&self) -> f64 {
        self.elapsed_ns as f64 / 1_000.0
    }

    pub fn elapsed_ms(&self) -> f64 {
        self.elapsed_ns as f64 / 1_000_000.0
    }

    /// Throughput: items per second (if item_count is set).
    pub fn throughput(&self) -> Option<f64> {
        self.item_count.map(|n| {
            if self.elapsed_ns == 0 {
                f64::INFINITY
            } else {
                n as f64 / (self.elapsed_ns as f64 / 1_000_000_000.0)
            }
        })
    }
}

// ── Scoped Timer ─────────────────────────────────────────────────────

/// RAII timer that records wall-clock elapsed time.
///
/// Create with [`ScopedTimer::start`], optionally set an item count,
/// and call [`finish`](ScopedTimer::finish) to get the
/// [`TimingResult`].  If dropped without calling `finish`, the
/// elapsed time is silently discarded.
pub struct ScopedTimer {
    start: Instant,
    item_count: Option<u64>,
}

impl ScopedTimer {
    pub fn start() -> Self {
        Self {
            start: Instant::now(),
            item_count: None,
        }
    }

    pub fn set_items(&mut self, n: u64) {
        self.item_count = Some(n);
    }

    /// Finish and return the timing result.
    pub fn finish(self) -> TimingResult {
        let elapsed = self.start.elapsed();
        let ns = elapsed.as_nanos() as u64;
        TimingResult {
            elapsed_ns: ns,
            item_count: self.item_count,
        }
    }

    /// Elapsed so far without consuming the timer.
    pub fn elapsed_ns(&self) -> u64 {
        self.start.elapsed().as_nanos() as u64
    }
}

// ── Metric Accumulator ───────────────────────────────────────────────

/// Sliding-window accumulator for a named metric.
///
/// Keeps the last `window_size` samples and computes summary
/// statistics on demand.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricAccumulator {
    name: String,
    samples: Vec<f64>,
    window_size: usize,
    total_samples: u64,
}

impl MetricAccumulator {
    pub fn new(name: impl Into<String>, window_size: usize) -> Self {
        Self {
            name: name.into(),
            samples: Vec::with_capacity(window_size.min(256)),
            window_size: window_size.max(1),
            total_samples: 0,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Record a new sample.
    pub fn record(&mut self, value: f64) {
        if self.samples.len() >= self.window_size {
            self.samples.remove(0);
        }
        self.samples.push(value);
        self.total_samples += 1;
    }

    /// Record a timing result (elapsed nanoseconds).
    pub fn record_timing(&mut self, result: &TimingResult) {
        self.record(result.elapsed_ns as f64);
    }

    pub fn sample_count(&self) -> usize {
        self.samples.len()
    }

    pub fn total_samples(&self) -> u64 {
        self.total_samples
    }

    pub fn mean(&self) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }
        self.samples.iter().sum::<f64>() / self.samples.len() as f64
    }

    pub fn min(&self) -> f64 {
        self.samples
            .iter()
            .copied()
            .fold(f64::INFINITY, f64::min)
    }

    pub fn max(&self) -> f64 {
        self.samples
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max)
    }

    /// Compute the p-th percentile (0–100).
    pub fn percentile(&self, p: f64) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let mut sorted = self.samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let idx = ((p / 100.0) * (sorted.len() - 1) as f64).round() as usize;
        sorted[idx.min(sorted.len() - 1)]
    }

    pub fn p50(&self) -> f64 {
        self.percentile(50.0)
    }

    pub fn p95(&self) -> f64 {
        self.percentile(95.0)
    }

    pub fn p99(&self) -> f64 {
        self.percentile(99.0)
    }

    /// Standard deviation.
    pub fn std_dev(&self) -> f64 {
        if self.samples.len() < 2 {
            return 0.0;
        }
        let mean = self.mean();
        let variance =
            self.samples.iter().map(|s| (s - mean).powi(2)).sum::<f64>()
                / (self.samples.len() - 1) as f64;
        variance.sqrt()
    }

    /// Latest sample.
    pub fn latest(&self) -> Option<f64> {
        self.samples.last().copied()
    }

    pub fn clear(&mut self) {
        self.samples.clear();
    }

    /// Snapshot as a serialisable structure.
    pub fn snapshot(&self) -> MetricSnapshot {
        MetricSnapshot {
            name: self.name.clone(),
            mean: self.mean(),
            min: if self.samples.is_empty() {
                0.0
            } else {
                self.min()
            },
            max: if self.samples.is_empty() {
                0.0
            } else {
                self.max()
            },
            p50: self.p50(),
            p95: self.p95(),
            p99: self.p99(),
            std_dev: self.std_dev(),
            sample_count: self.samples.len() as u64,
            total_samples: self.total_samples,
        }
    }
}

/// Point-in-time snapshot of a single metric.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricSnapshot {
    pub name: String,
    pub mean: f64,
    pub min: f64,
    pub max: f64,
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
    pub std_dev: f64,
    pub sample_count: u64,
    pub total_samples: u64,
}

// ── Perf Snapshot ────────────────────────────────────────────────────

/// A collection of metric snapshots.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerfSnapshot {
    pub metrics: Vec<MetricSnapshot>,
    pub timestamp: u64,
}

// ── Perf Registry ────────────────────────────────────────────────────

/// A named collection of [`MetricAccumulator`]s.
///
/// Typical usage: create a registry at startup, register metrics
/// by name, record samples during frame processing, and periodically
/// take a [`PerfSnapshot`] for telemetry / UI display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerfRegistry {
    metrics: BTreeMap<String, MetricAccumulator>,
    default_window: usize,
}

impl PerfRegistry {
    pub fn new(default_window: usize) -> Self {
        Self {
            metrics: BTreeMap::new(),
            default_window,
        }
    }

    /// Get or create a metric by name.
    pub fn metric(&mut self, name: &str) -> &mut MetricAccumulator {
        let window = self.default_window;
        self.metrics
            .entry(name.to_string())
            .or_insert_with(|| MetricAccumulator::new(name, window))
    }

    /// Record a sample for a named metric.
    pub fn record(&mut self, name: &str, value: f64) {
        self.metric(name).record(value);
    }

    /// Record a timing result for a named metric.
    pub fn record_timing(&mut self, name: &str, result: &TimingResult) {
        self.metric(name).record_timing(result);
    }

    /// Take a snapshot of all metrics.
    pub fn snapshot(&self, timestamp: u64) -> PerfSnapshot {
        PerfSnapshot {
            metrics: self.metrics.values().map(|m| m.snapshot()).collect(),
            timestamp,
        }
    }

    /// All registered metric names.
    pub fn metric_names(&self) -> Vec<&str> {
        self.metrics.keys().map(|s| s.as_str()).collect()
    }

    pub fn metric_count(&self) -> usize {
        self.metrics.len()
    }

    /// Remove a metric by name.
    pub fn remove(&mut self, name: &str) -> bool {
        self.metrics.remove(name).is_some()
    }

    /// Clear all samples in all metrics (keeps registrations).
    pub fn clear_samples(&mut self) {
        for m in self.metrics.values_mut() {
            m.clear();
        }
    }

    /// Clear everything.
    pub fn clear_all(&mut self) {
        self.metrics.clear();
    }
}

impl Default for PerfRegistry {
    fn default() -> Self {
        Self::new(128)
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── TimingResult tests ──────────────────────────────────────────

    #[test]
    fn test_timing_result_conversions() {
        let t = TimingResult::new(1_000_000); // 1ms
        assert!((t.elapsed_us() - 1_000.0).abs() < f64::EPSILON);
        assert!((t.elapsed_ms() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_timing_result_throughput() {
        let t = TimingResult::with_items(1_000_000_000, 100); // 1s, 100 items
        let tp = t.throughput().unwrap();
        assert!((tp - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_timing_result_no_items() {
        let t = TimingResult::new(500);
        assert!(t.throughput().is_none());
    }

    #[test]
    fn test_timing_result_serde() {
        let t = TimingResult::with_items(42000, 10);
        let json = serde_json::to_string(&t).unwrap();
        let back: TimingResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back, t);
    }

    // ── ScopedTimer tests ───────────────────────────────────────────

    #[test]
    fn test_scoped_timer_measures_time() {
        let timer = ScopedTimer::start();
        // Do some trivial work
        let _sum: u64 = (0..1000).sum();
        let result = timer.finish();
        assert!(result.elapsed_ns > 0);
    }

    #[test]
    fn test_scoped_timer_with_items() {
        let mut timer = ScopedTimer::start();
        timer.set_items(500);
        let result = timer.finish();
        assert_eq!(result.item_count, Some(500));
    }

    #[test]
    fn test_scoped_timer_elapsed_ns() {
        let timer = ScopedTimer::start();
        let _sum: u64 = (0..1000).sum();
        let ns = timer.elapsed_ns();
        assert!(ns > 0);
    }

    // ── MetricAccumulator tests ─────────────────────────────────────

    #[test]
    fn test_accumulator_mean() {
        let mut acc = MetricAccumulator::new("test", 100);
        acc.record(10.0);
        acc.record(20.0);
        acc.record(30.0);
        assert!((acc.mean() - 20.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_accumulator_min_max() {
        let mut acc = MetricAccumulator::new("test", 100);
        acc.record(5.0);
        acc.record(100.0);
        acc.record(50.0);
        assert!((acc.min() - 5.0).abs() < f64::EPSILON);
        assert!((acc.max() - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_accumulator_percentile() {
        let mut acc = MetricAccumulator::new("test", 200);
        for i in 1..=100 {
            acc.record(i as f64);
        }
        let p50 = acc.p50();
        assert!((p50 - 50.0).abs() < 1.1); // within rounding
        let p95 = acc.p95();
        assert!((p95 - 95.0).abs() < 1.1);
    }

    #[test]
    fn test_accumulator_sliding_window() {
        let mut acc = MetricAccumulator::new("test", 3);
        acc.record(1.0);
        acc.record(2.0);
        acc.record(3.0);
        acc.record(100.0); // pushes out 1.0
        assert_eq!(acc.sample_count(), 3);
        assert!((acc.mean() - 35.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_accumulator_total_samples() {
        let mut acc = MetricAccumulator::new("test", 3);
        acc.record(1.0);
        acc.record(2.0);
        acc.record(3.0);
        acc.record(4.0);
        assert_eq!(acc.total_samples(), 4);
        assert_eq!(acc.sample_count(), 3);
    }

    #[test]
    fn test_accumulator_std_dev() {
        let mut acc = MetricAccumulator::new("test", 100);
        acc.record(2.0);
        acc.record(4.0);
        acc.record(4.0);
        acc.record(4.0);
        acc.record(5.0);
        acc.record(5.0);
        acc.record(7.0);
        acc.record(9.0);
        let sd = acc.std_dev();
        // Sample std dev of [2,4,4,4,5,5,7,9] ≈ 2.138
        assert!((sd - 2.138).abs() < 0.1);
    }

    #[test]
    fn test_accumulator_latest() {
        let mut acc = MetricAccumulator::new("test", 100);
        acc.record(1.0);
        acc.record(2.0);
        assert_eq!(acc.latest(), Some(2.0));
    }

    #[test]
    fn test_accumulator_empty() {
        let acc = MetricAccumulator::new("test", 100);
        assert_eq!(acc.mean(), 0.0);
        assert_eq!(acc.p50(), 0.0);
        assert_eq!(acc.latest(), None);
    }

    #[test]
    fn test_accumulator_clear() {
        let mut acc = MetricAccumulator::new("test", 100);
        acc.record(1.0);
        acc.clear();
        assert_eq!(acc.sample_count(), 0);
    }

    #[test]
    fn test_accumulator_record_timing() {
        let mut acc = MetricAccumulator::new("test", 100);
        let t = TimingResult::new(5000);
        acc.record_timing(&t);
        assert!((acc.latest().unwrap() - 5000.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_accumulator_snapshot() {
        let mut acc = MetricAccumulator::new("frame_time", 100);
        acc.record(16.0);
        acc.record(17.0);
        let snap = acc.snapshot();
        assert_eq!(snap.name, "frame_time");
        assert_eq!(snap.sample_count, 2);
        assert!((snap.mean - 16.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_metric_snapshot_serde() {
        let snap = MetricSnapshot {
            name: "test".into(),
            mean: 1.0,
            min: 0.5,
            max: 1.5,
            p50: 1.0,
            p95: 1.4,
            p99: 1.5,
            std_dev: 0.3,
            sample_count: 100,
            total_samples: 200,
        };
        let json = serde_json::to_string(&snap).unwrap();
        let back: MetricSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(back, snap);
    }

    // ── PerfRegistry tests ──────────────────────────────────────────

    #[test]
    fn test_registry_record() {
        let mut reg = PerfRegistry::new(64);
        reg.record("render.frame", 16.6);
        reg.record("render.frame", 16.7);
        assert_eq!(reg.metric("render.frame").sample_count(), 2);
    }

    #[test]
    fn test_registry_auto_creates() {
        let mut reg = PerfRegistry::new(64);
        reg.record("new_metric", 1.0);
        assert_eq!(reg.metric_count(), 1);
        assert_eq!(reg.metric_names(), vec!["new_metric"]);
    }

    #[test]
    fn test_registry_snapshot() {
        let mut reg = PerfRegistry::new(64);
        reg.record("a", 1.0);
        reg.record("b", 2.0);
        let snap = reg.snapshot(12345);
        assert_eq!(snap.metrics.len(), 2);
        assert_eq!(snap.timestamp, 12345);
    }

    #[test]
    fn test_registry_remove() {
        let mut reg = PerfRegistry::new(64);
        reg.record("x", 1.0);
        assert!(reg.remove("x"));
        assert!(!reg.remove("x"));
        assert_eq!(reg.metric_count(), 0);
    }

    #[test]
    fn test_registry_clear_samples() {
        let mut reg = PerfRegistry::new(64);
        reg.record("a", 1.0);
        reg.record("b", 2.0);
        reg.clear_samples();
        assert_eq!(reg.metric("a").sample_count(), 0);
        assert_eq!(reg.metric("b").sample_count(), 0);
        // Registrations remain
        assert_eq!(reg.metric_count(), 2);
    }

    #[test]
    fn test_registry_clear_all() {
        let mut reg = PerfRegistry::new(64);
        reg.record("a", 1.0);
        reg.clear_all();
        assert_eq!(reg.metric_count(), 0);
    }

    #[test]
    fn test_registry_record_timing() {
        let mut reg = PerfRegistry::new(64);
        let t = TimingResult::new(8333333); // ~8.3ms
        reg.record_timing("frame", &t);
        let snap = reg.metric("frame").snapshot();
        assert!((snap.mean - 8333333.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_perf_snapshot_serde() {
        let snap = PerfSnapshot {
            metrics: vec![],
            timestamp: 999,
        };
        let json = serde_json::to_string(&snap).unwrap();
        let back: PerfSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(back, snap);
    }
}
