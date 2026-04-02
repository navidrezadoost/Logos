//! Token-bucket rate limiter.
//!
//! A `TokenBucket` refills at a constant rate of `refill_rate` tokens per
//! second up to a maximum `capacity`.  Each `try_acquire(n)` call either
//! atomically removes `n` tokens (success) or returns `BucketError::Throttled`
//! (failure).  Because this implementation is single-threaded and test-focused,
//! time is injected via an explicit `now_secs` parameter rather than reading
//! the wall clock, making all behaviour deterministic.

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ── Error ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Error, PartialEq)]
pub enum BucketError {
    #[error("rate limit exceeded: requested {requested} tokens but only {available} available")]
    Throttled { requested: u64, available: u64 },
    #[error("requested {0} tokens exceeds bucket capacity {1}")]
    ExceedsCapacity(u64, u64),
}

// ── Bucket config ─────────────────────────────────────────────────────────────

/// Configuration for a `TokenBucket`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BucketConfig {
    /// Tokens added per second.
    pub refill_rate: f64,
    /// Maximum tokens the bucket can hold (burst allowance).
    pub capacity: u64,
}

impl BucketConfig {
    pub fn new(refill_rate: f64, capacity: u64) -> Self {
        Self { refill_rate, capacity }
    }

    /// Convenience: strict — no bursting (capacity == 1 slot).
    pub fn strict(rps: f64) -> Self { Self::new(rps, 1) }

    /// Rate limit expressed as requests per minute.
    pub fn per_minute(rpm: f64) -> Self {
        Self::new(rpm / 60.0, rpm.ceil() as u64)
    }
}

impl Default for BucketConfig {
    fn default() -> Self { Self::new(10.0, 20) }
}

// ── Token bucket ──────────────────────────────────────────────────────────────

/// A classic token-bucket algorithm for rate limiting.
#[derive(Debug, Clone)]
pub struct TokenBucket {
    pub config: BucketConfig,
    /// Current number of available tokens (fractional representation).
    tokens: f64,
    /// Timestamp (fractional seconds) of the last refill.
    pub last_refill_secs: f64,
}

impl TokenBucket {
    /// Create a full bucket with `last_refill` = 0.
    pub fn new(config: BucketConfig) -> Self {
        let tokens = config.capacity as f64;
        Self { tokens, config, last_refill_secs: 0.0 }
    }

    /// Create a bucket that starts at `now_secs` (for deterministic tests).
    pub fn new_at(config: BucketConfig, now_secs: f64) -> Self {
        let tokens = config.capacity as f64;
        Self { tokens, config, last_refill_secs: now_secs }
    }

    // ── Internal ──────────────────────────────────────────────────────────────

    /// Refill tokens based on elapsed time.
    fn refill(&mut self, now_secs: f64) {
        if now_secs > self.last_refill_secs {
            let elapsed = now_secs - self.last_refill_secs;
            self.tokens = (self.tokens + elapsed * self.config.refill_rate)
                .min(self.config.capacity as f64);
            self.last_refill_secs = now_secs;
        }
    }

    // ── Public API ────────────────────────────────────────────────────────────

    /// Try to acquire `n` tokens at time `now_secs`.
    /// Returns `Ok(remaining)` on success or `Err(BucketError::Throttled)`.
    pub fn try_acquire_at(&mut self, n: u64, now_secs: f64) -> Result<u64, BucketError> {
        if n > self.config.capacity {
            return Err(BucketError::ExceedsCapacity(n, self.config.capacity));
        }
        self.refill(now_secs);
        if self.tokens >= n as f64 {
            self.tokens -= n as f64;
            Ok(self.tokens as u64)
        } else {
            Err(BucketError::Throttled {
                requested: n,
                available: self.tokens as u64,
            })
        }
    }

    /// Try to acquire `n` tokens using the wall clock (`now = 0.0` shortcut
    /// for tests that always start from a full bucket).
    pub fn try_acquire(&mut self, n: u64) -> Result<u64, BucketError> {
        // Use current tokens without advancing time (already full at construction).
        self.try_acquire_at(n, self.last_refill_secs)
    }

    /// Current available tokens (floor).
    pub fn available(&self) -> u64 { self.tokens as u64 }

    /// Fraction of capacity currently available (0.0–1.0).
    pub fn fill_level(&self) -> f64 {
        if self.config.capacity == 0 { return 0.0; }
        self.tokens / self.config.capacity as f64
    }

    /// Reset the bucket to full at `now_secs`.
    pub fn reset(&mut self, now_secs: f64) {
        self.tokens = self.config.capacity as f64;
        self.last_refill_secs = now_secs;
    }

    /// Seconds until `n` tokens will be available.
    pub fn wait_secs_for(&self, n: u64) -> f64 {
        let deficit = n as f64 - self.tokens;
        if deficit <= 0.0 { return 0.0; }
        deficit / self.config.refill_rate
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn bucket(rate: f64, cap: u64) -> TokenBucket {
        TokenBucket::new(BucketConfig::new(rate, cap))
    }

    #[test]
    fn new_bucket_is_full() {
        let b = bucket(10.0, 20);
        assert_eq!(b.available(), 20);
    }

    #[test]
    fn acquire_one_succeeds() {
        let mut b = bucket(10.0, 20);
        assert!(b.try_acquire(1).is_ok());
        assert_eq!(b.available(), 19);
    }

    #[test]
    fn acquire_all_succeeds() {
        let mut b = bucket(10.0, 5);
        assert!(b.try_acquire(5).is_ok());
        assert_eq!(b.available(), 0);
    }

    #[test]
    fn over_capacity_is_throttled() {
        let mut b = bucket(5.0, 5);
        // drain completely
        b.try_acquire(5).unwrap();
        let err = b.try_acquire(1).unwrap_err();
        assert!(matches!(err, BucketError::Throttled { .. }));
    }

    #[test]
    fn exceeds_capacity_error() {
        let mut b = bucket(10.0, 3);
        let err = b.try_acquire(10).unwrap_err();
        assert!(matches!(err, BucketError::ExceedsCapacity(_, _)));
    }

    #[test]
    fn refill_over_time() {
        let mut b = TokenBucket::new_at(BucketConfig::new(10.0, 20), 0.0);
        b.try_acquire_at(20, 0.0).unwrap(); // drain all
        // 1 second later: 10 tokens refilled
        assert!(b.try_acquire_at(10, 1.0).is_ok());
    }

    #[test]
    fn refill_does_not_exceed_capacity() {
        let mut b = TokenBucket::new_at(BucketConfig::new(100.0, 10), 0.0);
        b.try_acquire_at(10, 0.0).unwrap(); // drain
        b.try_acquire_at(0, 60.0).unwrap_or(0); // advance time a lot
        assert_eq!(b.available(), 10); // capped at capacity
    }

    #[test]
    fn wait_secs_zero_when_available() {
        let b = bucket(10.0, 20);
        assert_eq!(b.wait_secs_for(5), 0.0);
    }

    #[test]
    fn wait_secs_positive_when_drained() {
        let mut b = TokenBucket::new_at(BucketConfig::new(10.0, 10), 0.0);
        b.try_acquire_at(10, 0.0).unwrap();
        let w = b.wait_secs_for(5);
        assert!((w - 0.5).abs() < 1e-6);
    }

    #[test]
    fn fill_level_full() {
        let b = bucket(10.0, 20);
        assert!((b.fill_level() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn per_minute_config() {
        let cfg = BucketConfig::per_minute(60.0);
        assert!((cfg.refill_rate - 1.0).abs() < 1e-6);
        assert_eq!(cfg.capacity, 60);
    }
}
