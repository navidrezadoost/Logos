//! Phase 15.10 — Agent Quota & Rate Limiting integration tests.
//!
//! 45 tests: quota (12), bucket (11), tracker (10), limiter (8), e2e (4).

use logos_agent_quota::{
    TenantQuota, QuotaManager, QuotaPolicy, QuotaError, InvocationQuota,
    TokenBucket, BucketConfig, BucketError,
    UsageTracker, TenantUsage, UsageSnapshot,
    RateLimiter, LimiterError, ThrottleAction,
};
use logos_agent_quota::quota::QuotaPeriod;

// ════════════════════════════════════════════════════════════════════════════
// Helpers
// ════════════════════════════════════════════════════════════════════════════

fn quota(tenant: &str, max_inv: u64, max_tok: u64) -> TenantQuota {
    TenantQuota::new(tenant, max_inv, max_tok)
}

fn limiter_for(tenant: &str, rate: f64, cap: u64) -> RateLimiter {
    let mut l = RateLimiter::new();
    l.add_tenant(tenant, BucketConfig::new(rate, cap));
    l
}

// ════════════════════════════════════════════════════════════════════════════
// §1 Quota manager (12 tests)
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn quota_register_and_retrieve() {
    let mut mgr = QuotaManager::new();
    mgr.register(quota("acme", 500, 10_000)).unwrap();
    assert!(mgr.get("acme").is_some());
}

#[test]
fn quota_duplicate_registration_fails() {
    let mut mgr = QuotaManager::new();
    mgr.register(quota("acme", 500, 10_000)).unwrap();
    assert_eq!(mgr.register(quota("acme", 100, 5_000)).unwrap_err(),
               QuotaError::AlreadyRegistered("acme".into()));
}

#[test]
fn quota_upsert_overwrites() {
    let mut mgr = QuotaManager::new();
    mgr.register(quota("acme", 100, 5_000)).unwrap();
    mgr.upsert(quota("acme", 9_999, 999_999)).unwrap();
    assert_eq!(mgr.get("acme").unwrap().max_invocations, 9_999);
}

#[test]
fn quota_check_within_limits_ok() {
    let mut mgr = QuotaManager::new();
    mgr.register(quota("acme", 1_000, 50_000)).unwrap();
    assert!(mgr.check("acme", 0, 5, 0, 1_000).is_ok());
}

#[test]
fn quota_check_invocation_limit_exact_boundary_ok() {
    let mut mgr = QuotaManager::new();
    mgr.register(quota("acme", 10, 100_000)).unwrap();
    // used = 9, requesting 1 → total = 10 = limit ✓
    assert!(mgr.check("acme", 9, 1, 0, 0).is_ok());
}

#[test]
fn quota_check_invocation_limit_exceeded() {
    let mut mgr = QuotaManager::new();
    mgr.register(quota("acme", 10, 100_000)).unwrap();
    let err = mgr.check("acme", 10, 1, 0, 0).unwrap_err();
    assert!(matches!(err, QuotaError::InvocationLimitExceeded(_, _, _)));
}

#[test]
fn quota_check_token_budget_exceeded() {
    let mut mgr = QuotaManager::new();
    mgr.register(quota("acme", 10_000, 100)).unwrap();
    let err = mgr.check("acme", 0, 1, 100, 1).unwrap_err();
    assert!(matches!(err, QuotaError::TokenBudgetExceeded(_, _, _)));
}

#[test]
fn quota_check_unknown_tenant_error() {
    let mgr = QuotaManager::new();
    assert_eq!(mgr.check("ghost", 0, 1, 0, 0).unwrap_err(),
               QuotaError::TenantNotFound("ghost".into()));
}

#[test]
fn quota_disabled_tenant_always_passes() {
    let mut mgr = QuotaManager::new();
    mgr.register(quota("acme", 1, 1).with_enabled(false)).unwrap();
    assert!(mgr.check("acme", 9_999, 9_999, 9_999, 9_999).is_ok());
}

#[test]
fn quota_remaining_invocations() {
    let mut mgr = QuotaManager::new();
    mgr.register(quota("acme", 1_000, 50_000)).unwrap();
    assert_eq!(mgr.remaining_invocations("acme", 300), Some(700));
}

#[test]
fn quota_invocation_usage_pct() {
    let mut mgr = QuotaManager::new();
    mgr.register(quota("acme", 200, 10_000)).unwrap();
    let pct = mgr.invocation_usage_pct("acme", 100).unwrap();
    assert!((pct - 50.0).abs() < 1e-3);
}

#[test]
fn quota_period_label_and_seconds() {
    assert_eq!(QuotaPeriod::Daily.label(), "daily");
    assert_eq!(QuotaPeriod::Hourly.seconds(), 3_600);
    assert_eq!(QuotaPeriod::Monthly.seconds(), 30 * 86_400);
}

// ════════════════════════════════════════════════════════════════════════════
// §2 Token bucket (11 tests)
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn bucket_starts_full() {
    let b = TokenBucket::new(BucketConfig::new(10.0, 20));
    assert_eq!(b.available(), 20);
}

#[test]
fn bucket_acquire_one_ok() {
    let mut b = TokenBucket::new(BucketConfig::new(10.0, 10));
    b.try_acquire(1).unwrap();
    assert_eq!(b.available(), 9);
}

#[test]
fn bucket_acquire_all_ok() {
    let mut b = TokenBucket::new(BucketConfig::new(5.0, 5));
    b.try_acquire(5).unwrap();
    assert_eq!(b.available(), 0);
}

#[test]
fn bucket_throttled_when_empty() {
    let mut b = TokenBucket::new(BucketConfig::new(5.0, 5));
    b.try_acquire(5).unwrap();
    let err = b.try_acquire(1).unwrap_err();
    assert!(matches!(err, BucketError::Throttled { .. }));
}

#[test]
fn bucket_exceeds_capacity_error() {
    let mut b = TokenBucket::new(BucketConfig::new(10.0, 3));
    assert!(matches!(b.try_acquire(10), Err(BucketError::ExceedsCapacity(_, _))));
}

#[test]
fn bucket_refills_over_time() {
    let mut b = TokenBucket::new_at(BucketConfig::new(10.0, 20), 0.0);
    b.try_acquire_at(20, 0.0).unwrap();
    assert!(b.try_acquire_at(10, 1.0).is_ok());
}

#[test]
fn bucket_does_not_exceed_capacity_on_refill() {
    let mut b = TokenBucket::new_at(BucketConfig::new(100.0, 10), 0.0);
    // advance time significantly — tokens should be capped at 10
    b.try_acquire_at(0, 100.0).ok();
    assert_eq!(b.available(), 10);
}

#[test]
fn bucket_wait_secs_zero_when_available() {
    let b = TokenBucket::new(BucketConfig::new(10.0, 10));
    assert_eq!(b.wait_secs_for(5), 0.0);
}

#[test]
fn bucket_wait_secs_positive_when_drained() {
    let mut b = TokenBucket::new_at(BucketConfig::new(10.0, 10), 0.0);
    b.try_acquire_at(10, 0.0).unwrap();
    assert!((b.wait_secs_for(5) - 0.5).abs() < 1e-6);
}

#[test]
fn bucket_fill_level_full() {
    let b = TokenBucket::new(BucketConfig::new(10.0, 10));
    assert!((b.fill_level() - 1.0).abs() < 1e-6);
}

#[test]
fn bucket_per_minute_config() {
    let cfg = BucketConfig::per_minute(120.0);
    assert!((cfg.refill_rate - 2.0).abs() < 1e-6);
    assert_eq!(cfg.capacity, 120);
}

// ════════════════════════════════════════════════════════════════════════════
// §3 Usage tracker (10 tests)
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn tracker_empty_returns_zero() {
    let t = UsageTracker::new();
    let u = t.usage_for("acme");
    assert_eq!(u.invocations, 0);
    assert_eq!(u.tokens, 0);
}

#[test]
fn tracker_record_and_retrieve() {
    let mut t = UsageTracker::new();
    t.record("acme", "ag-x", 3, 600);
    let u = t.usage_for("acme");
    assert_eq!(u.invocations, 3);
    assert_eq!(u.tokens, 600);
}

#[test]
fn tracker_accumulates_across_agents() {
    let mut t = UsageTracker::new();
    t.record("acme", "ag-a", 2, 200);
    t.record("acme", "ag-b", 3, 300);
    let u = t.usage_for("acme");
    assert_eq!(u.invocations, 5);
    assert_eq!(u.tokens, 500);
}

#[test]
fn tracker_by_agent_breakdown() {
    let mut t = UsageTracker::new();
    t.record("acme", "ag-a", 1, 100);
    t.record("acme", "ag-b", 2, 200);
    let u = t.usage_for("acme");
    assert!(u.agent_snapshot("ag-a").is_some());
    assert!(u.agent_snapshot("ag-b").is_some());
}

#[test]
fn tracker_reset_tenant() {
    let mut t = UsageTracker::new();
    t.record("acme", "ag", 10, 1_000);
    t.reset_tenant("acme");
    assert_eq!(t.usage_for("acme").invocations, 0);
}

#[test]
fn tracker_reset_all() {
    let mut t = UsageTracker::new();
    t.record("a", "ag", 5, 500);
    t.record("b", "ag", 5, 500);
    t.reset_all();
    assert_eq!(t.global_invocations(), 0);
    assert_eq!(t.global_tokens(), 0);
}

#[test]
fn tracker_active_tenants_list() {
    let mut t = UsageTracker::new();
    t.record("x", "ag", 1, 0);
    t.record("y", "ag", 1, 0);
    let tenants = t.active_tenants();
    assert!(tenants.contains(&"x"));
    assert!(tenants.contains(&"y"));
}

#[test]
fn tracker_global_invocations() {
    let mut t = UsageTracker::new();
    t.record("a", "ag", 4, 0);
    t.record("b", "ag", 6, 0);
    assert_eq!(t.global_invocations(), 10);
}

#[test]
fn tracker_would_exceed_invocations_true() {
    let mut t = UsageTracker::new();
    t.record("acme", "ag", 99, 0);
    assert!(t.would_exceed_invocations("acme", 2, 100));
}

#[test]
fn tracker_would_exceed_tokens_false() {
    let mut t = UsageTracker::new();
    t.record("acme", "ag", 0, 400);
    assert!(!t.would_exceed_tokens("acme", 600, 1_000));
}

// ════════════════════════════════════════════════════════════════════════════
// §4 Rate limiter (8 tests)
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn limiter_check_ok() {
    let mut l = limiter_for("acme", 10.0, 10);
    assert!(l.check("acme", 1).is_ok());
}

#[test]
fn limiter_exceeds_bucket_rejected() {
    let mut l = limiter_for("acme", 10.0, 3);
    for _ in 0..3 { l.check("acme", 1).unwrap(); }
    assert_eq!(l.check("acme", 1).unwrap_err(), LimiterError::RateLimited("acme".into()));
}

#[test]
fn limiter_unknown_tenant_not_configured() {
    let mut l = RateLimiter::new();
    assert_eq!(l.check("ghost", 1).unwrap_err(), LimiterError::TenantNotConfigured("ghost".into()));
}

#[test]
fn limiter_warn_mode_allows_over_limit() {
    let mut l = RateLimiter::new().with_action(ThrottleAction::WarnAndAllow);
    l.add_tenant("acme", BucketConfig::new(1.0, 1));
    l.check("acme", 1).unwrap(); // fill drained
    assert!(l.check("acme", 1).is_ok()); // warn mode → still ok
}

#[test]
fn limiter_refill_allows_after_delay() {
    let mut l = RateLimiter::new();
    l.add_tenant_at("acme", BucketConfig::new(10.0, 10), 0.0);
    l.check_at("acme", 10, 0.0).unwrap(); // drain
    assert!(l.check_at("acme", 10, 1.0).is_ok());
}

#[test]
fn limiter_available_tokens_decrements() {
    let mut l = limiter_for("acme", 10.0, 10);
    l.check("acme", 3).unwrap();
    assert_eq!(l.available_tokens("acme"), Some(7));
}

#[test]
fn limiter_combined_check_ok() {
    let mut l = limiter_for("acme", 100.0, 100);
    let mut mgr = QuotaManager::new();
    mgr.register(quota("acme", 1_000, 100_000)).unwrap();
    let tracker = UsageTracker::new();
    assert!(l.check_with_quota("acme", 5, 1_000, &mgr, &tracker).is_ok());
}

#[test]
fn limiter_combined_check_quota_exceeded() {
    let mut l = limiter_for("acme", 100.0, 100);
    let mut mgr = QuotaManager::new();
    mgr.register(quota("acme", 10, 100_000)).unwrap();
    let mut tracker = UsageTracker::new();
    tracker.record("acme", "ag", 10, 0);
    let err = l.check_with_quota("acme", 1, 0, &mgr, &tracker).unwrap_err();
    assert!(matches!(err, LimiterError::QuotaExceeded(_, _)));
}

// ════════════════════════════════════════════════════════════════════════════
// §5 End-to-end (4 tests)
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn e2e_tenant_invokes_within_all_limits() {
    // Setup
    let mut mgr = QuotaManager::new();
    mgr.register(quota("corp", 1_000, 100_000)).unwrap();
    let mut tracker = UsageTracker::new();
    let mut limiter = limiter_for("corp", 50.0, 50);

    // 10 successful invocations
    for _ in 0..10 {
        limiter.check_with_quota("corp", 1, 500, &mgr, &tracker).unwrap();
        tracker.record("corp", "agent-a", 1, 500);
    }
    let u = tracker.usage_for("corp");
    assert_eq!(u.invocations, 10);
    assert_eq!(u.tokens, 5_000);
}

#[test]
fn e2e_tenant_hits_invocation_cap_then_reset() {
    let mut mgr = QuotaManager::new();
    mgr.register(quota("corp", 5, 100_000)).unwrap();
    let mut tracker = UsageTracker::new();

    for _ in 0..5 { tracker.record("corp", "ag", 1, 0); }

    // Next request should exceed quota
    let err = mgr.check("corp", tracker.usage_for("corp").invocations, 1, 0, 0).unwrap_err();
    assert!(matches!(err, QuotaError::InvocationLimitExceeded(_, _, _)));

    // After reset, works again
    tracker.reset_tenant("corp");
    assert!(mgr.check("corp", 0, 1, 0, 0).is_ok());
}

#[test]
fn e2e_bucket_burst_then_wait() {
    let mut b = TokenBucket::new_at(BucketConfig::new(10.0, 10), 0.0);
    for _ in 0..10 { b.try_acquire_at(1, 0.0).unwrap(); }
    assert!(b.try_acquire_at(1, 0.0).is_err());
    // Half a second restores 5 tokens
    assert!(b.try_acquire_at(5, 0.5).is_ok());
}

#[test]
fn e2e_multi_tenant_isolation() {
    let mut l = RateLimiter::new();
    l.add_tenant("corp-a", BucketConfig::new(5.0, 5));
    l.add_tenant("corp-b", BucketConfig::new(100.0, 100));

    // Drain corp-a
    for _ in 0..5 { l.check("corp-a", 1).unwrap(); }
    assert!(l.check("corp-a", 1).is_err());

    // corp-b is unaffected
    assert!(l.check("corp-b", 1).is_ok());
}
