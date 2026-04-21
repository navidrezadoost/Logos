// logos-collab/src/stress/metrics.rs
//
//! Latency histograms, throughput counters, and aggregated metrics for
//! the collaboration stress-test suite.

// ── LatencyHistogram ──────────────────────────────────────────────────────────

/// A simple reservoir-based latency histogram (stores raw samples, sorted
/// lazily).  Adequate for test-time reporting; not intended for production use.
#[derive(Debug, Default, Clone)]
pub struct LatencyHistogram {
    samples: Vec<u64>,
}

impl LatencyHistogram {
    /// Create a new, empty histogram.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a single latency sample in **microseconds**.
    pub fn record(&mut self, us: u64) {
        self.samples.push(us);
    }

    /// Number of recorded samples.
    pub fn count(&self) -> usize {
        self.samples.len()
    }

    /// Minimum recorded latency, or `None` if empty.
    pub fn min(&self) -> Option<u64> {
        self.samples.iter().copied().min()
    }

    /// Maximum recorded latency, or `None` if empty.
    pub fn max(&self) -> Option<u64> {
        self.samples.iter().copied().max()
    }

    /// Arithmetic mean in microseconds, or `None` if empty.
    pub fn mean(&self) -> Option<f64> {
        if self.samples.is_empty() {
            return None;
        }
        let sum: u64 = self.samples.iter().sum();
        Some(sum as f64 / self.samples.len() as f64)
    }

    /// Returns the value at the given percentile (0.0–100.0), or `None` if
    /// there are no samples.
    pub fn percentile(&self, pct: f64) -> Option<u64> {
        if self.samples.is_empty() {
            return None;
        }
        let mut sorted = self.samples.clone();
        sorted.sort_unstable();
        let idx = ((pct / 100.0) * (sorted.len() - 1) as f64).round() as usize;
        Some(sorted[idx.min(sorted.len() - 1)])
    }

    /// Convenience helpers.
    pub fn p50(&self) -> Option<u64> { self.percentile(50.0) }
    pub fn p95(&self) -> Option<u64> { self.percentile(95.0) }
    pub fn p99(&self) -> Option<u64> { self.percentile(99.0) }

    /// Merge another histogram into this one.
    pub fn merge(&mut self, other: &LatencyHistogram) {
        self.samples.extend_from_slice(&other.samples);
    }
}

// ── ThroughputCounter ─────────────────────────────────────────────────────────

/// Counts completed operations and tracks the wall-clock window in which they
/// occurred (start/end in milliseconds).
#[derive(Debug, Default, Clone)]
pub struct ThroughputCounter {
    total_ops: u64,
    start_ms:  u64,
    end_ms:    u64,
}

impl ThroughputCounter {
    pub fn new(start_ms: u64) -> Self {
        Self { total_ops: 0, start_ms, end_ms: start_ms }
    }

    /// Record that `n` operations completed at time `now_ms`.
    pub fn add(&mut self, n: u64, now_ms: u64) {
        self.total_ops += n;
        if now_ms > self.end_ms {
            self.end_ms = now_ms;
        }
    }

    /// Total operations recorded.
    pub fn total(&self) -> u64 { self.total_ops }

    /// Elapsed window in milliseconds.  Returns 1 ms minimum to avoid divide-by-zero.
    pub fn elapsed_ms(&self) -> u64 {
        (self.end_ms.saturating_sub(self.start_ms)).max(1)
    }

    /// Average operations per second over the recorded window.
    pub fn ops_per_sec(&self) -> f64 {
        self.total_ops as f64 / (self.elapsed_ms() as f64 / 1_000.0)
    }
}

// ── StressMetrics ─────────────────────────────────────────────────────────────

/// Aggregated metrics produced by a single stress-test run.
#[derive(Debug, Default, Clone)]
pub struct StressMetrics {
    pub latency:     LatencyHistogram,
    pub throughput:  ThroughputCounter,
    pub error_count: u64,
    pub total_ops:   u64,
}

impl StressMetrics {
    pub fn new(start_ms: u64) -> Self {
        Self {
            latency:     LatencyHistogram::new(),
            throughput:  ThroughputCounter::new(start_ms),
            error_count: 0,
            total_ops:   0,
        }
    }

    /// Record a successful operation with latency `us` at wall-clock `now_ms`.
    pub fn record_ok(&mut self, us: u64, now_ms: u64) {
        self.latency.record(us);
        self.throughput.add(1, now_ms);
        self.total_ops += 1;
    }

    /// Record a failed operation.
    pub fn record_error(&mut self) {
        self.error_count += 1;
        self.total_ops += 1;
    }

    /// Merge another `StressMetrics` into this one (used to aggregate
    /// per-user metrics into a global summary).
    pub fn merge(&mut self, other: &StressMetrics) {
        self.latency.merge(&other.latency);
        self.throughput.add(other.throughput.total(), other.throughput.end_ms);
        self.error_count += other.error_count;
        self.total_ops   += other.total_ops;
    }

    /// Error rate as a fraction in 0.0–1.0.
    pub fn error_rate(&self) -> f64 {
        if self.total_ops == 0 { return 0.0; }
        self.error_count as f64 / self.total_ops as f64
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // M-01: Empty histogram returns None for all aggregations.
    #[test]
    fn m_01_empty_histogram_returns_none() {
        let h = LatencyHistogram::new();
        assert!(h.min().is_none());
        assert!(h.max().is_none());
        assert!(h.mean().is_none());
        assert!(h.p99().is_none());
    }

    // M-02: Single sample yields identical min, max, mean, p50, p99.
    #[test]
    fn m_02_single_sample() {
        let mut h = LatencyHistogram::new();
        h.record(42);
        assert_eq!(h.min(), Some(42));
        assert_eq!(h.max(), Some(42));
        assert!((h.mean().unwrap() - 42.0).abs() < f64::EPSILON);
        assert_eq!(h.p50(), Some(42));
        assert_eq!(h.p99(), Some(42));
    }

    // M-03: Percentile ordering is correct for a known set.
    #[test]
    fn m_03_percentile_ordering() {
        let mut h = LatencyHistogram::new();
        for v in [10, 20, 30, 40, 50, 60, 70, 80, 90, 100] {
            h.record(v);
        }
        assert!(h.p50().unwrap() >= 50, "p50 should be >= median");
        assert!(h.p95().unwrap() >= h.p50().unwrap(), "p95 >= p50");
        assert!(h.p99().unwrap() >= h.p95().unwrap(), "p99 >= p95");
    }

    // M-04: ThroughputCounter ops_per_sec is sane.
    #[test]
    fn m_04_throughput_ops_per_sec() {
        let mut t = ThroughputCounter::new(0);
        t.add(1_000, 1_000); // 1 000 ops in 1 000 ms → 1 000 ops/s
        let ops = t.ops_per_sec();
        assert!((ops - 1_000.0).abs() < 1.0, "Expected ~1000 ops/s, got {ops}");
    }

    // M-05: StressMetrics merge aggregates correctly.
    #[test]
    fn m_05_stress_metrics_merge() {
        let mut a = StressMetrics::new(0);
        a.record_ok(100, 500);
        a.record_error();

        let mut b = StressMetrics::new(0);
        b.record_ok(200, 800);
        b.record_ok(300, 900);

        a.merge(&b);
        assert_eq!(a.total_ops, 4);
        assert_eq!(a.error_count, 1);
        assert!((a.error_rate() - 0.25).abs() < f64::EPSILON);
        assert_eq!(a.latency.count(), 3);
    }
}
