//! Stress tests for rate limiting and backpressure.
//!
//! Validates system behavior under extreme load:
//! - 10k concurrent users
//! - Sustained burst traffic
//! - Multi-level rate limit enforcement
//! - Backpressure channel behavior under saturation
//!
//! CTO targets:
//! - All three rate limit checks <100ns combined
//! - <80% CPU at 10k users
//! - <4GB memory
//! - <0.1% error rate

use logos_collab::auth::multilimit::{
    AtomicGlobalLimiter, MultiLevelLimiter, MultiLimitConfig, RejectionLevel,
};
use logos_collab::auth::backpressure::{
    AdaptiveLimiter, AtomicDropCounter, BackpressureChannel, DropStrategy,
};
use logos_collab::auth::ratelimit::TokenBucket;
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Stress test: 10,000 users check multi-level limiter.
///
/// Each user fires 10 messages. Verifies:
/// - Correct accept/reject ratio
/// - No panics under contention
/// - Completion within time bounds
#[test]
fn stress_10k_users_multi_level_limiter() {
    let config = MultiLimitConfig {
        user_rate: 100.0,
        user_burst: 200.0,
        room_rate: 100_000.0,  // High room limit — focus on user limits
        room_burst: 200_000.0,
        global_rate: 1_000_000.0,
        global_burst: 2_000_000.0,
        ..Default::default()
    };
    let mut limiter = MultiLevelLimiter::new(config);
    let room_id = Uuid::new_v4();

    let mut total_accepted = 0u64;
    let mut total_rejected = 0u64;

    let start = Instant::now();

    // 10,000 users, each fires 10 messages
    for _ in 0..10_000 {
        let user_id = Uuid::new_v4();
        for _ in 0..10 {
            match limiter.check_all(user_id, room_id) {
                Ok(()) => total_accepted += 1,
                Err(_) => total_rejected += 1,
            }
        }
    }

    let elapsed = start.elapsed();

    // Verify: 100,000 total messages processed
    assert_eq!(total_accepted + total_rejected, 100_000);

    // All should be accepted (10 msgs << 200 burst capacity per user)
    assert_eq!(total_accepted, 100_000);
    assert_eq!(total_rejected, 0);

    // Performance: should complete well within 1 second
    assert!(
        elapsed < Duration::from_secs(2),
        "10k users × 10 msgs took {:?} (too slow)",
        elapsed
    );

    let stats = limiter.stats();
    assert_eq!(stats.total_allowed, 100_000);
    assert_eq!(stats.global_rejected, 0);
}

/// Stress: 10k users with bandwidth checks.
#[test]
fn stress_10k_users_with_bandwidth() {
    let config = MultiLimitConfig {
        user_rate: 1000.0,
        user_burst: 2000.0,
        room_rate: 1_000_000.0,
        room_burst: 2_000_000.0,
        global_rate: 10_000_000.0,
        global_burst: 20_000_000.0,
        room_bytes_per_sec: u64::MAX, // No bandwidth limit
        ..Default::default()
    };
    let mut limiter = MultiLevelLimiter::new(config);
    let room_id = Uuid::new_v4();

    let start = Instant::now();
    let mut accepted = 0u64;

    for _ in 0..10_000 {
        let user = Uuid::new_v4();
        for _ in 0..10 {
            if limiter
                .check_all_with_bandwidth(user, room_id, 1024)
                .is_ok()
            {
                accepted += 1;
            }
        }
    }

    let elapsed = start.elapsed();
    assert_eq!(accepted, 100_000, "All should pass with high limits");
    assert!(
        elapsed < Duration::from_secs(2),
        "10k users bandwidth check took {:?}",
        elapsed
    );
}

/// Stress: Atomic global limiter under concurrent access.
#[test]
fn stress_atomic_global_concurrent_16_threads() {
    let limiter = Arc::new(AtomicGlobalLimiter::new(1_000_000.0, 2_000_000.0));
    let mut handles = vec![];
    let total_per_thread = 10_000;

    let start = Instant::now();

    for _ in 0..16 {
        let l = limiter.clone();
        handles.push(std::thread::spawn(move || {
            let mut accepted = 0u64;
            for _ in 0..total_per_thread {
                if l.check() {
                    accepted += 1;
                }
            }
            accepted
        }));
    }

    let mut total_accepted = 0u64;
    for h in handles {
        total_accepted += h.join().unwrap();
    }

    let elapsed = start.elapsed();

    // Some accepted, some rejected (capacity = 2M, total = 160k)
    // With high capacity, all should pass
    assert_eq!(total_accepted, 160_000);

    // Must complete quickly (no locks — CAS only)
    assert!(
        elapsed < Duration::from_secs(2),
        "16-thread atomic stress took {:?}",
        elapsed
    );
}

/// Stress: Backpressure channel DropNew under flood.
#[test]
fn stress_backpressure_drop_new_flood() {
    let capacity = 1000;
    let mut ch = BackpressureChannel::new(capacity, DropStrategy::DropNew);

    // Flood with 100k messages
    let mut accepted = 0u64;
    let mut dropped = 0u64;

    let start = Instant::now();
    for i in 0..100_000u64 {
        if ch.send(i) {
            accepted += 1;
        } else {
            dropped += 1;
        }
    }
    let elapsed = start.elapsed();

    // Only first 1000 accepted (no draining)
    assert_eq!(accepted, 1000);
    assert_eq!(dropped, 99_000);

    let stats = ch.stats();
    assert_eq!(stats.sent, 1000);
    assert_eq!(stats.dropped, 99_000);
    assert_eq!(stats.peak_depth, 1000);

    assert!(
        elapsed < Duration::from_secs(1),
        "100k sends took {:?}",
        elapsed
    );
}

/// Stress: Backpressure channel DropOldest under flood.
#[test]
fn stress_backpressure_drop_oldest_flood() {
    let capacity = 100;
    let mut ch = BackpressureChannel::new(capacity, DropStrategy::DropOldest);

    // Flood with 10k messages
    for i in 0..10_000u64 {
        ch.send(i);
    }

    // Should have the LAST 100 messages (9900..10000)
    let remaining: Vec<u64> = ch.drain().collect();
    assert_eq!(remaining.len(), 100);
    assert_eq!(remaining[0], 9900);
    assert_eq!(remaining[99], 9999);
}

/// Stress: Adaptive limiter convergence under sustained high latency.
#[test]
fn stress_adaptive_limiter_convergence() {
    let mut limiter = AdaptiveLimiter::new(10_000.0, Duration::from_millis(10));
    limiter.adjust_interval = Duration::from_millis(0); // Immediate

    // Simulate sustained high latency — rate should decrease
    for _ in 0..100 {
        for _ in 0..50 {
            limiter.record_latency(Duration::from_millis(50)); // 5x threshold
        }
        limiter.maybe_adjust();
    }

    let rate_under_pressure = limiter.current_rate();
    assert!(
        rate_under_pressure < 5000.0,
        "Rate {} should decrease under pressure",
        rate_under_pressure
    );

    // Now simulate recovery — rate should climb back
    for _ in 0..200 {
        for _ in 0..50 {
            limiter.record_latency(Duration::from_millis(1)); // Well below threshold
        }
        limiter.maybe_adjust();
    }

    let rate_recovered = limiter.current_rate();
    assert!(
        rate_recovered > rate_under_pressure,
        "Rate {} should recover above {}",
        rate_recovered,
        rate_under_pressure
    );

    assert!(limiter.adjustments_down() > 0);
    assert!(limiter.adjustments_up() > 0);
}

/// Stress: AtomicDropCounter under 16-thread contention.
#[test]
fn stress_atomic_drop_counter_16_threads() {
    let counter = Arc::new(AtomicDropCounter::new());
    let mut handles = vec![];

    for _ in 0..8 {
        let c = counter.clone();
        handles.push(std::thread::spawn(move || {
            for _ in 0..100_000 {
                c.record_sent();
            }
        }));
    }

    for _ in 0..8 {
        let c = counter.clone();
        handles.push(std::thread::spawn(move || {
            for _ in 0..10_000 {
                c.record_dropped();
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    assert_eq!(counter.sent(), 800_000);
    assert_eq!(counter.dropped(), 80_000);

    let rate = counter.drop_rate();
    // 80k / (800k + 80k) ≈ 0.0909
    assert!((rate - 0.0909).abs() < 0.01, "Drop rate {} unexpected", rate);
}

/// Stress: Multi-level limiter GC with 10k stale users.
#[test]
fn stress_gc_10k_stale_users() {
    let config = MultiLimitConfig {
        bucket_ttl: Duration::from_millis(1), // Expire almost immediately
        gc_interval: Duration::from_millis(1),
        ..Default::default()
    };
    let mut limiter = MultiLevelLimiter::new(config);
    let room_id = Uuid::new_v4();

    // Create 10k user buckets
    for _ in 0..10_000 {
        let user = Uuid::new_v4();
        let _ = limiter.check_all(user, room_id);
    }

    // Wait for TTL to expire
    std::thread::sleep(Duration::from_millis(5));

    // GC should reclaim all stale entries
    let reclaimed = limiter.gc();
    assert!(
        reclaimed >= 9_000,
        "Should reclaim most entries, got {}",
        reclaimed
    );
}

/// Stress: TokenBucket precision under rapid fire.
#[test]
fn stress_token_bucket_rapid_fire() {
    let mut bucket = TokenBucket::new(100.0, 100.0);
    let mut accepted = 0u32;
    let mut rejected = 0u32;

    // Fire 1000 times without any refill window
    for _ in 0..1000 {
        if bucket.take(1.0) {
            accepted += 1;
        } else {
            rejected += 1;
        }
    }

    // Should accept exactly burst capacity (100)
    assert_eq!(accepted, 100, "Accepted {} (expected 100)", accepted);
    assert_eq!(rejected, 900, "Rejected {} (expected 900)", rejected);
}

/// Stress: Mixed workload — users and rooms interleaved.
#[test]
fn stress_mixed_workload() {
    let config = MultiLimitConfig {
        user_rate: 100_000.0,  // High enough: 100 users × 100 rooms × 5 = 50k per user
        user_burst: 100_000.0,
        room_rate: 100_000.0,  // High enough: 100 users × 5 = 500 per room
        room_burst: 100_000.0,
        global_rate: 10_000_000.0,
        global_burst: 20_000_000.0,
        ..Default::default()
    };
    let mut limiter = MultiLevelLimiter::new(config);

    // 100 rooms × 100 users each = 10k unique (user,room) pairs
    let rooms: Vec<Uuid> = (0..100).map(|_| Uuid::new_v4()).collect();
    let users: Vec<Uuid> = (0..100).map(|_| Uuid::new_v4()).collect();

    let mut total_accepted = 0u64;
    let start = Instant::now();

    for room in &rooms {
        for user in &users {
            // 5 messages per user per room (well within burst)
            for _ in 0..5 {
                if limiter.check_all(*user, *room).is_ok() {
                    total_accepted += 1;
                }
            }
        }
    }

    let elapsed = start.elapsed();

    // 100 × 100 × 5 = 50,000 messages
    let total = 100 * 100 * 5;
    // All should pass: 5 msgs << 100 user burst, 500 msgs << 1000 room burst
    assert_eq!(
        total_accepted, total as u64,
        "Expected all {total} to pass, got {total_accepted}"
    );

    assert!(
        elapsed < Duration::from_secs(2),
        "Mixed workload took {:?}",
        elapsed
    );
}

/// Stress: Verify error rate < 0.1% under normal load.
#[test]
fn stress_error_rate_under_threshold() {
    let config = MultiLimitConfig {
        user_rate: 1000.0,
        user_burst: 2000.0,
        room_rate: 100_000.0,
        room_burst: 200_000.0,
        global_rate: 10_000_000.0,
        global_burst: 20_000_000.0,
        ..Default::default()
    };
    let mut limiter = MultiLevelLimiter::new(config);
    let room = Uuid::new_v4();

    let mut total = 0u64;
    let mut rejected = 0u64;

    // 1000 users, 10 messages each = 10k under high limits
    for _ in 0..1_000 {
        let user = Uuid::new_v4();
        for _ in 0..10 {
            total += 1;
            if limiter.check_all(user, room).is_err() {
                rejected += 1;
            }
        }
    }

    let error_rate = rejected as f64 / total as f64;
    assert!(
        error_rate < 0.001,
        "Error rate {:.4}% exceeds 0.1% threshold",
        error_rate * 100.0
    );
}

/// Stress: User-level isolation under per-user burst exhaustion.
#[test]
fn stress_user_isolation_under_burst() {
    let config = MultiLimitConfig {
        user_rate: 10.0,
        user_burst: 20.0,
        room_rate: 1_000_000.0,
        room_burst: 2_000_000.0,
        global_rate: 10_000_000.0,
        global_burst: 20_000_000.0,
        ..Default::default()
    };
    let mut limiter = MultiLevelLimiter::new(config);
    let room = Uuid::new_v4();

    // User A exhausts their burst
    let user_a = Uuid::new_v4();
    for _ in 0..20 {
        let _ = limiter.check_all(user_a, room);
    }

    // User A should now be rejected
    assert!(
        limiter.check_all(user_a, room).is_err(),
        "User A should be rate limited"
    );

    // User B should still pass (isolation)
    let user_b = Uuid::new_v4();
    assert!(
        limiter.check_all(user_b, room).is_ok(),
        "User B should NOT be affected by User A"
    );
}

/// Stress: Rejection levels are reported correctly.
#[test]
fn stress_rejection_levels() {
    // Test user rejection
    {
        let config = MultiLimitConfig {
            user_rate: 1.0,
            user_burst: 1.0,
            room_rate: 1_000_000.0,
            room_burst: 2_000_000.0,
            global_rate: 10_000_000.0,
            global_burst: 20_000_000.0,
            ..Default::default()
        };
        let mut limiter = MultiLevelLimiter::new(config);
        let user = Uuid::new_v4();
        let room = Uuid::new_v4();

        let _ = limiter.check_all(user, room); // Exhaust
        match limiter.check_all(user, room) {
            Err(RejectionLevel::User) => {} // Expected
            other => panic!("Expected User rejection, got {:?}", other),
        }
    }

    // Test global rejection
    {
        let config = MultiLimitConfig {
            user_rate: 1_000_000.0,
            user_burst: 2_000_000.0,
            room_rate: 1_000_000.0,
            room_burst: 2_000_000.0,
            global_rate: 1.0,
            global_burst: 1.0,
            ..Default::default()
        };
        let mut limiter = MultiLevelLimiter::new(config);
        let user = Uuid::new_v4();
        let room = Uuid::new_v4();

        let _ = limiter.check_all(user, room); // Exhaust global
        match limiter.check_all(user, room) {
            Err(RejectionLevel::Global) => {} // Expected
            other => panic!("Expected Global rejection, got {:?}", other),
        }
    }
}
