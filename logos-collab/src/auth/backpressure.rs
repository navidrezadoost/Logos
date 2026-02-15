//! Backpressure handling for slow consumers.
//!
//! Provides drop strategies and adaptive rate limiting to prevent
//! slow clients from blocking the broadcast hot path.
//!
//! Architecture:
//! ```text
//! Producer (broadcast)
//!       │
//!       ▼
//! ┌─────────────────┐
//! │ BackpressureTx   │ ── try_send() ── O(1)
//! │  (per-connection)│
//! └────────┬────────┘
//!          │
//!    ┌─────┴─────┐
//!    │ Full?      │
//!    ├─ No ──────► Send to channel
//!    └─ Yes ─────► DropStrategy
//!           ├── DropNew: discard incoming message
//!           └── DropOldest: evict oldest, insert new
//! ```
//!
//! Reference: DDIA, Chapter 11 — Stream Processing (Backpressure)
//! Reference: Software Engineering at Google, Chapter 24 — Load Testing

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Strategy for handling channel overflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropStrategy {
    /// Drop the new message (newest is discarded)
    DropNew,
    /// Drop the oldest message (FIFO eviction)
    DropOldest,
}

impl std::fmt::Display for DropStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DropNew => write!(f, "drop-new"),
            Self::DropOldest => write!(f, "drop-oldest"),
        }
    }
}

/// Backpressure statistics.
#[derive(Debug, Clone, Default)]
pub struct BackpressureStats {
    /// Messages successfully sent
    pub sent: u64,
    /// Messages dropped due to backpressure
    pub dropped: u64,
    /// Peak queue depth observed
    pub peak_depth: usize,
    /// Current queue depth
    pub current_depth: usize,
}

/// A bounded channel with configurable drop behavior.
///
/// Unlike tokio::mpsc, this channel never blocks the sender.
/// When the buffer is full, messages are dropped according to the
/// configured strategy.
///
/// Size: proportional to capacity × message size.
pub struct BackpressureChannel<T> {
    /// Message buffer (bounded ring buffer)
    buffer: VecDeque<T>,
    /// Maximum buffer size
    capacity: usize,
    /// Drop strategy when full
    strategy: DropStrategy,
    /// Stats
    sent: u64,
    dropped: u64,
    peak_depth: usize,
}

impl<T> BackpressureChannel<T> {
    /// Create a new backpressure channel.
    pub fn new(capacity: usize, strategy: DropStrategy) -> Self {
        Self {
            buffer: VecDeque::with_capacity(capacity),
            capacity,
            strategy,
            sent: 0,
            dropped: 0,
            peak_depth: 0,
        }
    }

    /// Try to send a message. Never blocks.
    ///
    /// Returns `true` if the message was accepted,
    /// `false` if it was dropped.
    #[inline]
    pub fn send(&mut self, msg: T) -> bool {
        if self.buffer.len() < self.capacity {
            // Buffer has room
            self.buffer.push_back(msg);
            self.sent += 1;
            if self.buffer.len() > self.peak_depth {
                self.peak_depth = self.buffer.len();
            }
            true
        } else {
            // Buffer is full — apply strategy
            match self.strategy {
                DropStrategy::DropNew => {
                    self.dropped += 1;
                    false
                }
                DropStrategy::DropOldest => {
                    self.buffer.pop_front(); // Evict oldest
                    self.buffer.push_back(msg);
                    self.dropped += 1;
                    self.sent += 1; // New message was sent
                    true
                }
            }
        }
    }

    /// Receive the next message (FIFO).
    ///
    /// Returns `None` if the buffer is empty.
    #[inline]
    pub fn recv(&mut self) -> Option<T> {
        self.buffer.pop_front()
    }

    /// Drain all messages from the buffer.
    pub fn drain(&mut self) -> impl Iterator<Item = T> + '_ {
        self.buffer.drain(..)
    }

    /// Current queue depth.
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Get statistics.
    pub fn stats(&self) -> BackpressureStats {
        BackpressureStats {
            sent: self.sent,
            dropped: self.dropped,
            peak_depth: self.peak_depth,
            current_depth: self.buffer.len(),
        }
    }
}

/// Adaptive rate limiter that adjusts based on observed latency.
///
/// When p99 latency exceeds the threshold, the rate is reduced.
/// When latency recovers, the rate is increased back to maximum.
///
/// Uses an exponentially weighted moving average (EWMA) for latency.
///
/// Reference: TCP congestion control (AIMD — Additive Increase,
/// Multiplicative Decrease)
pub struct AdaptiveLimiter {
    /// Current rate limit (messages/sec)
    current_rate: f64,
    /// Maximum rate (ceiling)
    max_rate: f64,
    /// Minimum rate (floor)
    min_rate: f64,
    /// Latency threshold (when exceeded, reduce rate)
    latency_threshold: Duration,
    /// EWMA of recent latencies
    ewma_latency_ns: f64,
    /// EWMA smoothing factor (0.0 = ignore new, 1.0 = ignore history)
    alpha: f64,
    /// Multiplicative decrease factor (e.g., 0.5 = halve)
    decrease_factor: f64,
    /// Additive increase per check interval (e.g., 10 msg/sec)
    increase_step: f64,
    /// Last adjustment time
    last_adjust: Instant,
    /// Adjustment interval
    pub adjust_interval: Duration,
    /// Stats
    adjustments_down: u64,
    adjustments_up: u64,
}

impl AdaptiveLimiter {
    /// Create a new adaptive limiter.
    pub fn new(max_rate: f64, latency_threshold: Duration) -> Self {
        Self {
            current_rate: max_rate,
            max_rate,
            min_rate: max_rate * 0.1, // Floor at 10% of max
            latency_threshold,
            ewma_latency_ns: 0.0,
            alpha: 0.1,            // Smooth — slow reaction
            decrease_factor: 0.5,  // AIMD: halve on congestion
            increase_step: max_rate * 0.05, // Increase 5% of max per interval
            last_adjust: Instant::now(),
            adjust_interval: Duration::from_secs(1),
            adjustments_down: 0,
            adjustments_up: 0,
        }
    }

    /// Record a latency observation (nanoseconds).
    ///
    /// Updates the EWMA. Call this after each message is processed.
    #[inline]
    pub fn record_latency(&mut self, latency: Duration) {
        let ns = latency.as_nanos() as f64;
        self.ewma_latency_ns = self.alpha * ns + (1.0 - self.alpha) * self.ewma_latency_ns;
    }

    /// Maybe adjust the rate based on current latency.
    ///
    /// Called periodically (e.g., every second).
    /// Returns the new rate if adjusted, None if unchanged.
    pub fn maybe_adjust(&mut self) -> Option<f64> {
        if self.last_adjust.elapsed() < self.adjust_interval {
            return None;
        }
        self.last_adjust = Instant::now();

        let current_latency = Duration::from_nanos(self.ewma_latency_ns as u64);

        if current_latency > self.latency_threshold {
            // Multiplicative decrease (AIMD)
            self.current_rate = (self.current_rate * self.decrease_factor).max(self.min_rate);
            self.adjustments_down += 1;
            Some(self.current_rate)
        } else if self.current_rate < self.max_rate {
            // Additive increase (AIMD)
            self.current_rate = (self.current_rate + self.increase_step).min(self.max_rate);
            self.adjustments_up += 1;
            Some(self.current_rate)
        } else {
            None
        }
    }

    /// Current effective rate.
    pub fn current_rate(&self) -> f64 {
        self.current_rate
    }

    /// Current EWMA latency estimate.
    pub fn estimated_latency(&self) -> Duration {
        Duration::from_nanos(self.ewma_latency_ns as u64)
    }

    /// Number of rate decreases triggered.
    pub fn adjustments_down(&self) -> u64 {
        self.adjustments_down
    }

    /// Number of rate increases triggered.
    pub fn adjustments_up(&self) -> u64 {
        self.adjustments_up
    }
}

/// Atomic drop counter for cross-thread monitoring.
///
/// Shared between producer and consumer to track dropped messages
/// without any locking.
#[derive(Debug)]
pub struct AtomicDropCounter {
    sent: AtomicU64,
    dropped: AtomicU64,
}

impl AtomicDropCounter {
    pub fn new() -> Self {
        Self {
            sent: AtomicU64::new(0),
            dropped: AtomicU64::new(0),
        }
    }

    /// Record a successful send.
    #[inline(always)]
    pub fn record_sent(&self) {
        self.sent.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a dropped message.
    #[inline(always)]
    pub fn record_dropped(&self) {
        self.dropped.fetch_add(1, Ordering::Relaxed);
    }

    /// Total sent.
    pub fn sent(&self) -> u64 {
        self.sent.load(Ordering::Relaxed)
    }

    /// Total dropped.
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Drop rate (0.0 to 1.0).
    pub fn drop_rate(&self) -> f64 {
        let s = self.sent() as f64;
        let d = self.dropped() as f64;
        let total = s + d;
        if total == 0.0 { 0.0 } else { d / total }
    }
}

impl Default for AtomicDropCounter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    // ── BackpressureChannel tests ───────────────────────────

    #[test]
    fn test_channel_send_recv() {
        let mut ch = BackpressureChannel::new(10, DropStrategy::DropNew);
        assert!(ch.send(42));
        assert_eq!(ch.recv(), Some(42));
        assert_eq!(ch.recv(), None);
    }

    #[test]
    fn test_channel_fifo_order() {
        let mut ch = BackpressureChannel::new(5, DropStrategy::DropNew);
        for i in 0..5 {
            ch.send(i);
        }
        for i in 0..5 {
            assert_eq!(ch.recv(), Some(i));
        }
    }

    #[test]
    fn test_channel_drop_new() {
        let mut ch = BackpressureChannel::new(3, DropStrategy::DropNew);
        assert!(ch.send(1));
        assert!(ch.send(2));
        assert!(ch.send(3));
        assert!(!ch.send(4)); // Dropped

        let stats = ch.stats();
        assert_eq!(stats.sent, 3);
        assert_eq!(stats.dropped, 1);
    }

    #[test]
    fn test_channel_drop_oldest() {
        let mut ch = BackpressureChannel::new(3, DropStrategy::DropOldest);
        ch.send(1);
        ch.send(2);
        ch.send(3);
        assert!(ch.send(4)); // Accepted, evicts 1

        // Should see 2, 3, 4 (1 was evicted)
        assert_eq!(ch.recv(), Some(2));
        assert_eq!(ch.recv(), Some(3));
        assert_eq!(ch.recv(), Some(4));
    }

    #[test]
    fn test_channel_stats() {
        let mut ch = BackpressureChannel::<u32>::new(2, DropStrategy::DropNew);
        ch.send(1);
        ch.send(2);
        ch.send(3); // Dropped

        let stats = ch.stats();
        assert_eq!(stats.sent, 2);
        assert_eq!(stats.dropped, 1);
        assert_eq!(stats.peak_depth, 2);
        assert_eq!(stats.current_depth, 2);
    }

    #[test]
    fn test_channel_drain() {
        let mut ch = BackpressureChannel::new(5, DropStrategy::DropNew);
        for i in 0..5 {
            ch.send(i);
        }
        let drained: Vec<_> = ch.drain().collect();
        assert_eq!(drained, vec![0, 1, 2, 3, 4]);
        assert!(ch.is_empty());
    }

    #[test]
    fn test_channel_len_empty() {
        let ch = BackpressureChannel::<u32>::new(10, DropStrategy::DropNew);
        assert_eq!(ch.len(), 0);
        assert!(ch.is_empty());
    }

    #[test]
    fn test_drop_strategy_display() {
        assert_eq!(DropStrategy::DropNew.to_string(), "drop-new");
        assert_eq!(DropStrategy::DropOldest.to_string(), "drop-oldest");
    }

    // ── AdaptiveLimiter tests ───────────────────────────────

    #[test]
    fn test_adaptive_starts_at_max() {
        let limiter = AdaptiveLimiter::new(1000.0, Duration::from_millis(10));
        assert_eq!(limiter.current_rate(), 1000.0);
    }

    #[test]
    fn test_adaptive_decreases_on_high_latency() {
        let mut limiter = AdaptiveLimiter::new(1000.0, Duration::from_millis(10));
        limiter.adjust_interval = Duration::from_millis(0); // Immediate adjustment

        // Record high latency
        for _ in 0..100 {
            limiter.record_latency(Duration::from_millis(20));
        }

        let new_rate = limiter.maybe_adjust();
        assert!(new_rate.is_some());
        assert!(limiter.current_rate() < 1000.0, "Rate should decrease");
        assert_eq!(limiter.adjustments_down(), 1);
    }

    #[test]
    fn test_adaptive_increases_on_low_latency() {
        let mut limiter = AdaptiveLimiter::new(1000.0, Duration::from_millis(10));
        limiter.adjust_interval = Duration::from_millis(0);

        // Force rate down first
        limiter.current_rate = 500.0;

        // Record low latency
        for _ in 0..100 {
            limiter.record_latency(Duration::from_millis(1));
        }

        let new_rate = limiter.maybe_adjust();
        assert!(new_rate.is_some());
        assert!(limiter.current_rate() > 500.0, "Rate should increase");
        assert_eq!(limiter.adjustments_up(), 1);
    }

    #[test]
    fn test_adaptive_respects_floor() {
        let mut limiter = AdaptiveLimiter::new(1000.0, Duration::from_millis(10));
        limiter.adjust_interval = Duration::from_millis(0);

        // Repeatedly decrease
        for _ in 0..50 {
            for _ in 0..100 {
                limiter.record_latency(Duration::from_millis(50));
            }
            limiter.maybe_adjust();
        }

        assert!(
            limiter.current_rate() >= 100.0, // min_rate = 10% of max
            "Rate {} fell below floor",
            limiter.current_rate()
        );
    }

    #[test]
    fn test_adaptive_estimated_latency() {
        let mut limiter = AdaptiveLimiter::new(1000.0, Duration::from_millis(10));

        for _ in 0..100 {
            limiter.record_latency(Duration::from_millis(5));
        }

        let est = limiter.estimated_latency();
        // EWMA should converge toward 5ms
        assert!(est.as_millis() >= 3 && est.as_millis() <= 6);
    }

    #[test]
    fn test_adaptive_respects_interval() {
        let mut limiter = AdaptiveLimiter::new(1000.0, Duration::from_millis(10));
        limiter.adjust_interval = Duration::from_secs(60); // Long interval

        for _ in 0..100 {
            limiter.record_latency(Duration::from_millis(50));
        }

        // Should not adjust yet
        assert!(limiter.maybe_adjust().is_none());
    }

    // ── AtomicDropCounter tests ─────────────────────────────

    #[test]
    fn test_atomic_counter_basic() {
        let counter = AtomicDropCounter::new();
        counter.record_sent();
        counter.record_sent();
        counter.record_dropped();

        assert_eq!(counter.sent(), 2);
        assert_eq!(counter.dropped(), 1);
    }

    #[test]
    fn test_atomic_counter_drop_rate() {
        let counter = AtomicDropCounter::new();

        // 0/0 = 0.0
        assert_eq!(counter.drop_rate(), 0.0);

        // 3 sent, 1 dropped = 25%
        counter.record_sent();
        counter.record_sent();
        counter.record_sent();
        counter.record_dropped();

        let rate = counter.drop_rate();
        assert!((rate - 0.25).abs() < 0.01);
    }

    #[test]
    fn test_atomic_counter_concurrent() {
        let counter = Arc::new(AtomicDropCounter::new());
        let mut handles = vec![];

        for _ in 0..4 {
            let c = counter.clone();
            handles.push(std::thread::spawn(move || {
                for _ in 0..10_000 {
                    c.record_sent();
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(counter.sent(), 40_000);
    }

    #[test]
    fn test_atomic_counter_default() {
        let counter = AtomicDropCounter::default();
        assert_eq!(counter.sent(), 0);
        assert_eq!(counter.dropped(), 0);
    }
}
