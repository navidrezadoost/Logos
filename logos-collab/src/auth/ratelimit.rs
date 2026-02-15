//! Token bucket rate limiter with per-user and per-room limits.
//!
//! Uses the classic token bucket algorithm for O(1) rate limiting.
//! Each user gets a bucket that refills at a configurable rate.
//!
//! Performance target: <200ns per check (cache hit)
//!
//! Memory: 64 bytes per active user bucket:
//! - tokens: f64 (8 bytes)
//! - last_refill: Instant (8 bytes)
//! - capacity: f64 (8 bytes)
//! - refill_rate: f64 (8 bytes)
//! - HashMap overhead: ~32 bytes
//!
//! Reference: DDIA, Chapter 11 — Stream Processing
//! Reference: RFC 6585 — 429 Too Many Requests

use std::collections::HashMap;
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Configuration for the rate limiter.
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum messages per second per user
    pub messages_per_second: f64,
    /// Burst capacity (max tokens in bucket)
    pub burst_capacity: f64,
    /// Maximum bytes per second per room
    pub room_bytes_per_second: u64,
    /// How often to garbage-collect expired buckets
    pub gc_interval: Duration,
    /// Time after which an idle bucket is removed
    pub bucket_ttl: Duration,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            messages_per_second: 100.0,
            burst_capacity: 200.0,
            room_bytes_per_second: 10 * 1024 * 1024, // 10 MB/s
            gc_interval: Duration::from_secs(300),    // 5 minutes
            bucket_ttl: Duration::from_secs(600),     // 10 minutes
        }
    }
}

/// A single token bucket for rate limiting.
///
/// Size: 32 bytes (fits in half a cache line).
/// All operations are O(1) with no allocation.
#[derive(Debug, Clone)]
pub struct TokenBucket {
    /// Current token count
    tokens: f64,
    /// When tokens were last refilled
    last_refill: Instant,
    /// Maximum tokens (burst capacity)
    capacity: f64,
    /// Tokens added per second
    refill_rate: f64,
}

impl TokenBucket {
    /// Create a new token bucket.
    ///
    /// Starts full (at capacity).
    pub fn new(capacity: f64, refill_rate: f64) -> Self {
        Self {
            tokens: capacity,
            last_refill: Instant::now(),
            capacity,
            refill_rate,
        }
    }

    /// Try to consume `count` tokens from the bucket.
    ///
    /// Returns `true` if tokens were available, `false` if rate limited.
    ///
    /// Performance: ~20ns (refill calculation + comparison).
    #[inline(always)]
    pub fn take(&mut self, count: f64) -> bool {
        self.refill();
        if self.tokens >= count {
            self.tokens -= count;
            true
        } else {
            false
        }
    }

    /// Refill tokens based on elapsed time.
    ///
    /// Uses f64 arithmetic for sub-second precision.
    #[inline(always)]
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill);
        let new_tokens = elapsed.as_secs_f64() * self.refill_rate;

        if new_tokens > 0.0 {
            self.tokens = (self.tokens + new_tokens).min(self.capacity);
            self.last_refill = now;
        }
    }

    /// Current token count (for monitoring).
    pub fn available(&mut self) -> f64 {
        self.refill();
        self.tokens
    }

    /// Check if the bucket is full.
    pub fn is_full(&mut self) -> bool {
        self.refill();
        self.tokens >= self.capacity
    }

    /// Time since last activity.
    pub fn idle_time(&self) -> Duration {
        self.last_refill.elapsed()
    }
}

/// Room-level bandwidth tracker.
///
/// Tracks bytes per second per room using a sliding window.
#[derive(Debug, Clone)]
pub struct RoomBandwidth {
    /// Bytes sent in current window
    bytes_in_window: u64,
    /// Window start time
    window_start: Instant,
    /// Maximum bytes per second
    limit: u64,
}

impl RoomBandwidth {
    /// Create a new bandwidth tracker.
    pub fn new(limit: u64) -> Self {
        Self {
            bytes_in_window: 0,
            window_start: Instant::now(),
            limit,
        }
    }

    /// Try to record `bytes` of bandwidth usage.
    ///
    /// Returns `true` if within limit, `false` if throttled.
    #[inline(always)]
    pub fn record(&mut self, bytes: u64) -> bool {
        let elapsed = self.window_start.elapsed();
        if elapsed >= Duration::from_secs(1) {
            // Reset window
            self.bytes_in_window = 0;
            self.window_start = Instant::now();
        }

        if self.bytes_in_window + bytes <= self.limit {
            self.bytes_in_window += bytes;
            true
        } else {
            false
        }
    }

    /// Current usage in the window.
    pub fn current_usage(&self) -> u64 {
        self.bytes_in_window
    }
}

/// Per-user and per-room rate limiter.
///
/// Provides O(1) rate limit checks with automatic bucket management.
/// Idle buckets are garbage-collected periodically.
pub struct RateLimiter {
    /// Per-user message rate buckets
    user_buckets: HashMap<Uuid, TokenBucket>,
    /// Per-room bandwidth trackers
    room_bandwidth: HashMap<Uuid, RoomBandwidth>,
    /// Configuration
    config: RateLimitConfig,
    /// Last GC time
    last_gc: Instant,
    /// Stats
    total_allowed: u64,
    total_rejected: u64,
}

impl RateLimiter {
    /// Create a new rate limiter with the given configuration.
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            user_buckets: HashMap::new(),
            room_bandwidth: HashMap::new(),
            config,
            last_gc: Instant::now(),
            total_allowed: 0,
            total_rejected: 0,
        }
    }

    /// Create with default configuration (100 msg/sec, 10MB/s per room).
    pub fn with_defaults() -> Self {
        Self::new(RateLimitConfig::default())
    }

    /// Check if a user is allowed to send a message.
    ///
    /// Performance: <200ns (cache hit — bucket exists in HashMap).
    /// Performance: <1μs (cache miss — new bucket allocation).
    ///
    /// Returns `true` if allowed, `false` if rate limited.
    #[inline(always)]
    pub fn check_user(&mut self, user_id: Uuid) -> bool {
        let config = &self.config;
        let bucket = self.user_buckets
            .entry(user_id)
            .or_insert_with(|| TokenBucket::new(
                config.burst_capacity,
                config.messages_per_second,
            ));

        if bucket.take(1.0) {
            self.total_allowed += 1;
            true
        } else {
            self.total_rejected += 1;
            false
        }
    }

    /// Check if a room is within its bandwidth limit.
    ///
    /// `bytes` is the message size being sent.
    /// Returns `true` if within limit, `false` if throttled.
    #[inline(always)]
    pub fn check_room_bandwidth(&mut self, room_id: Uuid, bytes: u64) -> bool {
        let limit = self.config.room_bytes_per_second;
        let tracker = self.room_bandwidth
            .entry(room_id)
            .or_insert_with(|| RoomBandwidth::new(limit));

        tracker.record(bytes)
    }

    /// Combined check: user rate + room bandwidth.
    ///
    /// Returns `true` only if both checks pass.
    #[inline]
    pub fn check(&mut self, user_id: Uuid, room_id: Uuid, message_bytes: u64) -> bool {
        self.check_user(user_id) && self.check_room_bandwidth(room_id, message_bytes)
    }

    /// Run garbage collection to remove idle buckets.
    ///
    /// Called periodically (default: every 5 minutes).
    /// Removes buckets that have been idle longer than `bucket_ttl`.
    pub fn gc(&mut self) -> usize {
        let ttl = self.config.bucket_ttl;
        let before = self.user_buckets.len();

        self.user_buckets.retain(|_, bucket| bucket.idle_time() < ttl);
        self.room_bandwidth.retain(|_, bw| {
            bw.window_start.elapsed() < ttl
        });

        self.last_gc = Instant::now();
        before - self.user_buckets.len()
    }

    /// Run GC if the gc_interval has elapsed.
    pub fn maybe_gc(&mut self) -> usize {
        if self.last_gc.elapsed() >= self.config.gc_interval {
            self.gc()
        } else {
            0
        }
    }

    /// Get the number of active user buckets.
    pub fn active_users(&self) -> usize {
        self.user_buckets.len()
    }

    /// Get the number of active room bandwidth trackers.
    pub fn active_rooms(&self) -> usize {
        self.room_bandwidth.len()
    }

    /// Get rate limiter stats.
    pub fn stats(&self) -> RateLimitStats {
        RateLimitStats {
            active_users: self.user_buckets.len(),
            active_rooms: self.room_bandwidth.len(),
            total_allowed: self.total_allowed,
            total_rejected: self.total_rejected,
        }
    }
}

/// Rate limiter statistics.
#[derive(Debug, Clone, Default)]
pub struct RateLimitStats {
    pub active_users: usize,
    pub active_rooms: usize,
    pub total_allowed: u64,
    pub total_rejected: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_bucket_basic() {
        let mut bucket = TokenBucket::new(10.0, 10.0);

        // Should succeed 10 times (initial capacity)
        for _ in 0..10 {
            assert!(bucket.take(1.0));
        }
        // 11th should fail
        assert!(!bucket.take(1.0));
    }

    #[test]
    fn test_token_bucket_refill() {
        let mut bucket = TokenBucket::new(10.0, 1000.0); // Fast refill

        // Drain
        for _ in 0..10 {
            assert!(bucket.take(1.0));
        }
        assert!(!bucket.take(1.0));

        // Wait for refill
        std::thread::sleep(Duration::from_millis(20));

        // Should have refilled some tokens
        assert!(bucket.take(1.0));
    }

    #[test]
    fn test_token_bucket_capacity_limit() {
        let mut bucket = TokenBucket::new(5.0, 1000.0);

        // Wait for potential over-refill
        std::thread::sleep(Duration::from_millis(100));

        // Available should not exceed capacity
        let available = bucket.available();
        assert!(
            available <= 5.0 + 0.001, // tiny float tolerance
            "Available {available} exceeds capacity 5.0"
        );
    }

    #[test]
    fn test_token_bucket_burst() {
        let mut bucket = TokenBucket::new(100.0, 10.0);

        // Should allow burst up to capacity
        assert!(bucket.take(50.0));
        assert!(bucket.take(50.0));
        assert!(!bucket.take(1.0)); // No more burst
    }

    #[test]
    fn test_rate_limiter_allows_normal_traffic() {
        let mut limiter = RateLimiter::new(RateLimitConfig {
            messages_per_second: 100.0,
            burst_capacity: 200.0,
            ..Default::default()
        });

        let user = Uuid::new_v4();

        // 100 messages should all be allowed (within burst)
        for _ in 0..100 {
            assert!(limiter.check_user(user));
        }
    }

    #[test]
    fn test_rate_limiter_rejects_excess() {
        let mut limiter = RateLimiter::new(RateLimitConfig {
            messages_per_second: 10.0,
            burst_capacity: 10.0,
            ..Default::default()
        });

        let user = Uuid::new_v4();

        // 10 messages should succeed
        for _ in 0..10 {
            assert!(limiter.check_user(user));
        }
        // 11th should be rejected
        assert!(!limiter.check_user(user));
    }

    #[test]
    fn test_rate_limiter_per_user_isolation() {
        let mut limiter = RateLimiter::new(RateLimitConfig {
            messages_per_second: 5.0,
            burst_capacity: 5.0,
            ..Default::default()
        });

        let user1 = Uuid::new_v4();
        let user2 = Uuid::new_v4();

        // Exhaust user1's budget
        for _ in 0..5 {
            limiter.check_user(user1);
        }
        assert!(!limiter.check_user(user1));

        // User2 should still have full budget
        assert!(limiter.check_user(user2));
    }

    #[test]
    fn test_room_bandwidth_basic() {
        let mut bw = RoomBandwidth::new(1_000_000); // 1MB/s

        assert!(bw.record(500_000));
        assert!(bw.record(400_000));
        assert!(!bw.record(200_000)); // Exceeds 1MB
    }

    #[test]
    fn test_room_bandwidth_window_reset() {
        let mut bw = RoomBandwidth::new(1000);

        // Fill up
        assert!(bw.record(1000));
        assert!(!bw.record(1));

        // Wait for window reset
        std::thread::sleep(Duration::from_secs(1));

        // Should be allowed again
        assert!(bw.record(500));
    }

    #[test]
    fn test_combined_check() {
        let mut limiter = RateLimiter::with_defaults();
        let user = Uuid::new_v4();
        let room = Uuid::new_v4();

        assert!(limiter.check(user, room, 100));
    }

    #[test]
    fn test_gc_removes_idle_buckets() {
        let mut limiter = RateLimiter::new(RateLimitConfig {
            bucket_ttl: Duration::from_millis(50),
            ..Default::default()
        });

        let user = Uuid::new_v4();
        limiter.check_user(user);
        assert_eq!(limiter.active_users(), 1);

        std::thread::sleep(Duration::from_millis(100));
        let removed = limiter.gc();
        assert_eq!(removed, 1);
        assert_eq!(limiter.active_users(), 0);
    }

    #[test]
    fn test_gc_keeps_active_buckets() {
        let mut limiter = RateLimiter::new(RateLimitConfig {
            bucket_ttl: Duration::from_secs(60),
            ..Default::default()
        });

        let user = Uuid::new_v4();
        limiter.check_user(user);

        let removed = limiter.gc();
        assert_eq!(removed, 0);
        assert_eq!(limiter.active_users(), 1);
    }

    #[test]
    fn test_stats() {
        let mut limiter = RateLimiter::new(RateLimitConfig {
            messages_per_second: 2.0,
            burst_capacity: 2.0,
            ..Default::default()
        });

        let user = Uuid::new_v4();
        limiter.check_user(user); // allowed
        limiter.check_user(user); // allowed
        limiter.check_user(user); // rejected

        let stats = limiter.stats();
        assert_eq!(stats.total_allowed, 2);
        assert_eq!(stats.total_rejected, 1);
        assert_eq!(stats.active_users, 1);
    }

    #[test]
    fn test_config_defaults() {
        let config = RateLimitConfig::default();
        assert_eq!(config.messages_per_second, 100.0);
        assert_eq!(config.burst_capacity, 200.0);
        assert_eq!(config.room_bytes_per_second, 10 * 1024 * 1024);
    }

    #[test]
    fn test_bucket_idle_time() {
        let bucket = TokenBucket::new(10.0, 10.0);
        std::thread::sleep(Duration::from_millis(10));
        assert!(bucket.idle_time() >= Duration::from_millis(10));
    }

    #[test]
    fn test_maybe_gc_respects_interval() {
        let mut limiter = RateLimiter::new(RateLimitConfig {
            gc_interval: Duration::from_secs(300), // 5 min
            ..Default::default()
        });

        let user = Uuid::new_v4();
        limiter.check_user(user);

        // Should not GC yet (interval hasn't elapsed)
        let removed = limiter.maybe_gc();
        assert_eq!(removed, 0);
        assert_eq!(limiter.active_users(), 1);
    }

    #[test]
    fn test_room_bandwidth_usage() {
        let mut bw = RoomBandwidth::new(10_000);
        bw.record(3000);
        assert_eq!(bw.current_usage(), 3000);
    }

    #[test]
    fn test_many_users_rate_limit() {
        let mut limiter = RateLimiter::with_defaults();

        // 1000 different users should each get their own bucket
        for _ in 0..1000 {
            let user = Uuid::new_v4();
            assert!(limiter.check_user(user));
        }

        assert_eq!(limiter.active_users(), 1000);
    }
}
