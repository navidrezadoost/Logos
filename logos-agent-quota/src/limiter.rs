//! Composite rate limiter — wraps per-tenant `TokenBucket`s and integrates
//! with `QuotaManager` + `UsageTracker` for a unified enforcement point.

use std::collections::HashMap;
use thiserror::Error;

use crate::bucket::{BucketConfig, BucketError, TokenBucket};
use crate::quota::QuotaManager;
use crate::tracker::UsageTracker;

// ── Error ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Error, PartialEq)]
pub enum LimiterError {
    #[error("rate limit exceeded for tenant '{0}'")]
    RateLimited(String),
    #[error("quota exceeded for tenant '{0}': {1}")]
    QuotaExceeded(String, String),
    #[error("tenant '{0}' not configured in rate limiter")]
    TenantNotConfigured(String),
}

impl From<BucketError> for LimiterError {
    fn from(_: BucketError) -> Self {
        LimiterError::RateLimited("(unknown)".to_string())
    }
}

// ── Throttle action ───────────────────────────────────────────────────────────

/// How the limiter should handle a request that violates its policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThrottleAction {
    /// Return `LimiterError` immediately.
    Reject,
    /// Allow the request but emit a warning log.
    WarnAndAllow,
}

impl Default for ThrottleAction {
    fn default() -> Self { Self::Reject }
}

// ── Rate limiter ──────────────────────────────────────────────────────────────

/// Composite limiter: per-tenant token-bucket + optional quota check.
#[derive(Debug)]
pub struct RateLimiter {
    buckets: HashMap<String, TokenBucket>,
    action: ThrottleAction,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self { buckets: HashMap::new(), action: ThrottleAction::Reject }
    }

    pub fn with_action(mut self, action: ThrottleAction) -> Self {
        self.action = action;
        self
    }

    // ── Configuration ─────────────────────────────────────────────────────────

    /// Register a token bucket for `tenant_id`.
    pub fn add_tenant(&mut self, tenant_id: impl Into<String>, cfg: BucketConfig) {
        let bucket = TokenBucket::new(cfg);
        self.buckets.insert(tenant_id.into(), bucket);
    }

    /// Register with an explicit start time (for deterministic tests).
    pub fn add_tenant_at(
        &mut self,
        tenant_id: impl Into<String>,
        cfg: BucketConfig,
        now_secs: f64,
    ) {
        let bucket = TokenBucket::new_at(cfg, now_secs);
        self.buckets.insert(tenant_id.into(), bucket);
    }

    pub fn remove_tenant(&mut self, tenant_id: &str) -> bool {
        self.buckets.remove(tenant_id).is_some()
    }

    pub fn tenant_count(&self) -> usize { self.buckets.len() }

    pub fn has_tenant(&self, tenant_id: &str) -> bool {
        self.buckets.contains_key(tenant_id)
    }

    // ── Check ─────────────────────────────────────────────────────────────────

    /// Check (and consume) `n` tokens for `tenant_id` using current bucket state.
    pub fn check(&mut self, tenant_id: &str, n: u64) -> Result<(), LimiterError> {
        self.check_at(tenant_id, n, self.bucket_last_refill(tenant_id))
    }

    /// Check at explicit time (for deterministic tests).
    pub fn check_at(&mut self, tenant_id: &str, n: u64, now_secs: f64) -> Result<(), LimiterError> {
        let bucket = self.buckets.get_mut(tenant_id)
            .ok_or_else(|| LimiterError::TenantNotConfigured(tenant_id.to_string()))?;

        match bucket.try_acquire_at(n, now_secs) {
            Ok(_) => Ok(()),
            Err(_) => match &self.action {
                ThrottleAction::Reject => Err(LimiterError::RateLimited(tenant_id.to_string())),
                ThrottleAction::WarnAndAllow => {
                    log::warn!("rate limit exceeded for tenant '{}' — allowing (warn mode)", tenant_id);
                    Ok(())
                }
            },
        }
    }

    /// Combined check: rate-limit bucket first, then quota.
    pub fn check_with_quota(
        &mut self,
        tenant_id: &str,
        invocations: u64,
        tokens: u64,
        quota_mgr: &QuotaManager,
        tracker: &UsageTracker,
    ) -> Result<(), LimiterError> {
        // 1. Rate-limit check (consumes tokens from bucket)
        self.check(tenant_id, invocations)?;

        // 2. Quota check (read-only, tracker provides usage)
        let usage = tracker.usage_for(tenant_id);
        quota_mgr
            .check(tenant_id, usage.invocations, invocations, usage.tokens, tokens)
            .map_err(|e| LimiterError::QuotaExceeded(tenant_id.to_string(), e.to_string()))?;

        Ok(())
    }

    /// Available tokens for `tenant_id`; `None` if tenant not registered.
    pub fn available_tokens(&self, tenant_id: &str) -> Option<u64> {
        self.buckets.get(tenant_id).map(|b| b.available())
    }

    // ── Internal ──────────────────────────────────────────────────────────────

    fn bucket_last_refill(&self, tenant_id: &str) -> f64 {
        self.buckets.get(tenant_id)
            .map(|b| b.last_refill_secs)
            .unwrap_or(0.0)
    }
}

impl Default for RateLimiter {
    fn default() -> Self { Self::new() }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn limiter_with_tenant(rate: f64, cap: u64) -> RateLimiter {
        let mut l = RateLimiter::new();
        l.add_tenant("acme", BucketConfig::new(rate, cap));
        l
    }

    #[test]
    fn check_within_limit_ok() {
        let mut l = limiter_with_tenant(10.0, 10);
        assert!(l.check("acme", 1).is_ok());
    }

    #[test]
    fn check_drains_bucket() {
        let mut l = limiter_with_tenant(10.0, 5);
        for _ in 0..5 { l.check("acme", 1).unwrap(); }
        assert_eq!(l.check("acme", 1).unwrap_err(), LimiterError::RateLimited("acme".into()));
    }

    #[test]
    fn check_unknown_tenant_err() {
        let mut l = RateLimiter::new();
        assert_eq!(
            l.check("ghost", 1).unwrap_err(),
            LimiterError::TenantNotConfigured("ghost".into()),
        );
    }

    #[test]
    fn warn_and_allow_mode_never_errors() {
        let mut l = RateLimiter::new().with_action(ThrottleAction::WarnAndAllow);
        l.add_tenant("acme", BucketConfig::new(1.0, 1));
        l.check("acme", 1).unwrap(); // drain
        // Must succeed even though bucket is empty
        assert!(l.check("acme", 1).is_ok());
    }

    #[test]
    fn remove_tenant_removes() {
        let mut l = limiter_with_tenant(10.0, 10);
        assert!(l.remove_tenant("acme"));
        assert!(!l.has_tenant("acme"));
    }

    #[test]
    fn available_tokens_full_bucket() {
        let l = limiter_with_tenant(10.0, 20);
        assert_eq!(l.available_tokens("acme"), Some(20));
    }

    #[test]
    fn available_tokens_none_for_unknown() {
        let l = RateLimiter::new();
        assert!(l.available_tokens("ghost").is_none());
    }

    #[test]
    fn refill_over_time_allows_more() {
        let mut l = RateLimiter::new();
        l.add_tenant_at("acme", BucketConfig::new(10.0, 10), 0.0);
        l.check_at("acme", 10, 0.0).unwrap(); // drain
        assert!(l.check_at("acme", 10, 1.0).is_ok()); // 10 tokens refilled in 1s
    }

    #[test]
    fn combined_quota_check_passes() {
        let mut l = limiter_with_tenant(100.0, 100);
        let mut mgr = QuotaManager::new();
        mgr.register(crate::quota::TenantQuota::new("acme", 1_000, 100_000)).unwrap();
        let tracker = UsageTracker::new();
        assert!(l.check_with_quota("acme", 1, 200, &mgr, &tracker).is_ok());
    }

    #[test]
    fn combined_quota_check_fails_on_quota() {
        let mut l = limiter_with_tenant(100.0, 100);
        let mut mgr = QuotaManager::new();
        mgr.register(crate::quota::TenantQuota::new("acme", 10, 100_000)).unwrap();
        let mut tracker = UsageTracker::new();
        tracker.record("acme", "ag", 10, 0); // already at limit
        let err = l.check_with_quota("acme", 1, 0, &mgr, &tracker).unwrap_err();
        assert!(matches!(err, LimiterError::QuotaExceeded(_, _)));
    }
}
