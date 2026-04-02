//! Latency histograms and error-rate tracking.

/// Fixed upper bounds for histogram buckets (ms).
#[derive(Debug, Clone)]
pub struct BucketBounds(pub Vec<f64>);

impl BucketBounds {
    /// Standard web-latency buckets.
    pub fn standard() -> Self {
        Self(vec![1.0, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 2500.0, 5000.0])
    }

    pub fn custom(bounds: Vec<f64>) -> Self {
        let mut b = bounds;
        b.sort_by(|a, c| a.partial_cmp(c).unwrap());
        Self(b)
    }
}

/// Snapshot of histogram statistics.
#[derive(Debug, Clone)]
pub struct HistogramSnapshot {
    pub count:   u64,
    pub sum_ms:  f64,
    pub min_ms:  f64,
    pub max_ms:  f64,
    pub mean_ms: f64,
    pub p50_ms:  f64,
    pub p90_ms:  f64,
    pub p99_ms:  f64,
}

/// Accumulates latency samples and computes percentiles.
#[derive(Debug)]
pub struct LatencyHistogram {
    samples: Vec<f64>,
    buckets: BucketBounds,
}

impl LatencyHistogram {
    pub fn new() -> Self {
        Self { samples: Vec::new(), buckets: BucketBounds::standard() }
    }

    pub fn with_buckets(bounds: BucketBounds) -> Self {
        Self { samples: Vec::new(), buckets: bounds }
    }

    /// Record a single latency value in milliseconds.
    pub fn record_ms(&mut self, ms: f64) {
        self.samples.push(ms);
    }

    /// Record multiple values.
    pub fn record_all(&mut self, values: impl IntoIterator<Item = f64>) {
        for v in values { self.samples.push(v); }
    }

    /// Number of recorded samples.
    pub fn count(&self) -> u64 {
        self.samples.len() as u64
    }

    fn percentile(&self, pct: f64) -> f64 {
        if self.samples.is_empty() { return 0.0; }
        let mut sorted = self.samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let idx = ((pct / 100.0) * (sorted.len() as f64 - 1.0)).round() as usize;
        sorted[idx.min(sorted.len() - 1)]
    }

    pub fn p50_ms(&self) -> f64 { self.percentile(50.0) }
    pub fn p90_ms(&self) -> f64 { self.percentile(90.0) }
    pub fn p99_ms(&self) -> f64 { self.percentile(99.0) }

    pub fn mean_ms(&self) -> f64 {
        if self.samples.is_empty() { return 0.0; }
        self.samples.iter().sum::<f64>() / self.samples.len() as f64
    }

    pub fn min_ms(&self) -> f64 {
        self.samples.iter().cloned().fold(f64::INFINITY, f64::min)
            .max(0.0) // guard empty
    }

    pub fn max_ms(&self) -> f64 {
        self.samples.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
            .max(0.0) // guard empty
    }

    /// Number of samples that fell in each bucket (cumulative counts).
    pub fn bucket_counts(&self) -> Vec<(f64, u64)> {
        self.buckets.0.iter().map(|&bound| {
            let count = self.samples.iter().filter(|&&v| v <= bound).count() as u64;
            (bound, count)
        }).collect()
    }

    /// Snapshot of all statistics.
    pub fn snapshot(&self) -> HistogramSnapshot {
        HistogramSnapshot {
            count:   self.count(),
            sum_ms:  self.samples.iter().sum(),
            min_ms:  if self.samples.is_empty() { 0.0 } else { self.min_ms() },
            max_ms:  if self.samples.is_empty() { 0.0 } else { self.max_ms() },
            mean_ms: self.mean_ms(),
            p50_ms:  self.p50_ms(),
            p90_ms:  self.p90_ms(),
            p99_ms:  self.p99_ms(),
        }
    }

    /// Reset all samples.
    pub fn reset(&mut self) {
        self.samples.clear();
    }
}

impl Default for LatencyHistogram {
    fn default() -> Self { Self::new() }
}

/// Tracks a rolling error rate using a fixed-size window of boolean outcomes.
#[derive(Debug)]
pub struct ErrorRateTracker {
    window: std::collections::VecDeque<bool>,
    capacity: usize,
}

impl ErrorRateTracker {
    /// Create a tracker with the given window size.
    pub fn new(window_size: usize) -> Self {
        Self {
            window: std::collections::VecDeque::with_capacity(window_size),
            capacity: window_size,
        }
    }

    /// Record an outcome: `was_error = true` for errors.
    pub fn record(&mut self, was_error: bool) {
        if self.window.len() == self.capacity {
            self.window.pop_front();
        }
        self.window.push_back(was_error);
    }

    /// Current error rate (errors / window size).
    pub fn rate(&self) -> f64 {
        if self.window.is_empty() { return 0.0; }
        let errors = self.window.iter().filter(|&&e| e).count();
        errors as f64 / self.window.len() as f64
    }

    /// Number of outcomes in the window.
    pub fn window_len(&self) -> usize {
        self.window.len()
    }

    /// True if the window is full.
    pub fn is_full(&self) -> bool {
        self.window.len() == self.capacity
    }

    pub fn reset(&mut self) { self.window.clear(); }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn histogram_records_and_counts() {
        let mut h = LatencyHistogram::new();
        h.record_ms(10.0);
        h.record_ms(20.0);
        assert_eq!(h.count(), 2);
    }

    #[test]
    fn histogram_p50() {
        let mut h = LatencyHistogram::new();
        h.record_all(vec![10.0, 20.0, 30.0, 40.0, 50.0]);
        assert!((h.p50_ms() - 30.0).abs() < 1.0);
    }

    #[test]
    fn histogram_p99_at_max() {
        let mut h = LatencyHistogram::new();
        for i in 1..=100 { h.record_ms(i as f64); }
        assert!(h.p99_ms() >= 99.0);
    }

    #[test]
    fn histogram_mean_correct() {
        let mut h = LatencyHistogram::new();
        h.record_all(vec![10.0, 20.0, 30.0]);
        assert!((h.mean_ms() - 20.0).abs() < 1e-9);
    }

    #[test]
    fn histogram_min_max() {
        let mut h = LatencyHistogram::new();
        h.record_all(vec![5.0, 50.0, 500.0]);
        assert!((h.min_ms() - 5.0).abs() < 1e-9);
        assert!((h.max_ms() - 500.0).abs() < 1e-9);
    }

    #[test]
    fn histogram_bucket_counts_monotonic() {
        let mut h = LatencyHistogram::new();
        h.record_all(vec![1.0, 10.0, 100.0, 1000.0]);
        let counts = h.bucket_counts();
        // Cumulative counts should be non-decreasing
        let mut prev = 0;
        for (_, c) in counts {
            assert!(c >= prev);
            prev = c;
        }
    }

    #[test]
    fn histogram_empty_returns_zero() {
        let h = LatencyHistogram::new();
        assert_eq!(h.p50_ms(), 0.0);
        assert_eq!(h.mean_ms(), 0.0);
    }

    #[test]
    fn histogram_reset_clears() {
        let mut h = LatencyHistogram::new();
        h.record_ms(100.0);
        h.reset();
        assert_eq!(h.count(), 0);
    }

    #[test]
    fn error_rate_tracker_correct_rate() {
        let mut t = ErrorRateTracker::new(10);
        for _ in 0..7 { t.record(false); }
        for _ in 0..3 { t.record(true); }
        assert!((t.rate() - 0.3).abs() < 1e-9);
    }

    #[test]
    fn error_rate_tracker_window_rolls() {
        let mut t = ErrorRateTracker::new(3);
        t.record(true);
        t.record(true);
        t.record(true);
        t.record(false); // evicts first true
        assert!((t.rate() - 2.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn error_rate_zero_when_empty() {
        let t = ErrorRateTracker::new(10);
        assert_eq!(t.rate(), 0.0);
    }

    #[test]
    fn histogram_snapshot_fields() {
        let mut h = LatencyHistogram::new();
        h.record_all(vec![10.0, 20.0, 30.0]);
        let s = h.snapshot();
        assert_eq!(s.count, 3);
        assert!((s.sum_ms - 60.0).abs() < 1e-9);
    }
}
