//! Per-tenant invocation quotas and token budgets.
//!
//! A `TenantQuota` declares how many agent invocations and how many LLM
//! tokens a tenant may consume per rolling period.  `QuotaManager` holds
//! all tenant quotas and evaluates whether a request should be allowed.

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ── Error ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Error, PartialEq, Eq)]
pub enum QuotaError {
    #[error("invocation quota exceeded for tenant '{0}': used {1}, limit {2}")]
    InvocationLimitExceeded(String, u64, u64),
    #[error("token budget exceeded for tenant '{0}': used {1}, limit {2}")]
    TokenBudgetExceeded(String, u64, u64),
    #[error("tenant '{0}' not registered")]
    TenantNotFound(String),
    #[error("quota already exists for tenant '{0}'")]
    AlreadyRegistered(String),
    #[error("invalid quota: {0}")]
    InvalidQuota(String),
}

// ── Quota period ──────────────────────────────────────────────────────────────

/// The rolling window over which quota limits apply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuotaPeriod {
    Minutely,
    Hourly,
    Daily,
    Monthly,
}

impl QuotaPeriod {
    /// Duration of this period in seconds.
    pub fn seconds(&self) -> u64 {
        match self {
            Self::Minutely => 60,
            Self::Hourly   => 3_600,
            Self::Daily    => 86_400,
            Self::Monthly  => 30 * 86_400,
        }
    }
    pub fn label(&self) -> &str {
        match self {
            Self::Minutely => "minutely",
            Self::Hourly   => "hourly",
            Self::Daily    => "daily",
            Self::Monthly  => "monthly",
        }
    }
}

// ── Invocation quota ──────────────────────────────────────────────────────────

/// Quota for a single agent scoped to a tenant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvocationQuota {
    pub agent_id: String,
    /// Max invocations per period.
    pub max_invocations: u64,
    /// Max LLM tokens per period.
    pub max_tokens: u64,
    pub period: QuotaPeriod,
}

impl InvocationQuota {
    pub fn new(
        agent_id: impl Into<String>,
        max_invocations: u64,
        max_tokens: u64,
    ) -> Self {
        Self {
            agent_id: agent_id.into(),
            max_invocations,
            max_tokens,
            period: QuotaPeriod::Daily,
        }
    }

    pub fn with_period(mut self, p: QuotaPeriod) -> Self {
        self.period = p;
        self
    }
}

// ── Quota policy ──────────────────────────────────────────────────────────────

/// What to do when a tenant exceeds their quota.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuotaPolicy {
    /// Return an error immediately.
    Reject,
    /// Allow but log a warning.
    Warn,
    /// Queue the request until quota resets.
    Queue,
}

impl Default for QuotaPolicy {
    fn default() -> Self { Self::Reject }
}

// ── Tenant quota ──────────────────────────────────────────────────────────────

/// Global quota settings for an entire tenant (organisation).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantQuota {
    pub tenant_id: String,
    /// Max total invocations per period across all agents.
    pub max_invocations: u64,
    /// Max total LLM tokens per period.
    pub max_tokens: u64,
    pub period: QuotaPeriod,
    pub policy: QuotaPolicy,
    /// Per-agent overrides.
    pub agent_quotas: Vec<InvocationQuota>,
    pub enabled: bool,
}

impl TenantQuota {
    /// Create a tenant quota with daily period and Reject policy.
    pub fn new(
        tenant_id: impl Into<String>,
        max_invocations: u64,
        max_tokens: u64,
    ) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            max_invocations,
            max_tokens,
            period: QuotaPeriod::Daily,
            policy: QuotaPolicy::Reject,
            agent_quotas: vec![],
            enabled: true,
        }
    }

    pub fn with_period(mut self, p: QuotaPeriod) -> Self {
        self.period = p;
        self
    }

    pub fn with_policy(mut self, p: QuotaPolicy) -> Self {
        self.policy = p;
        self
    }

    pub fn with_agent_quota(mut self, q: InvocationQuota) -> Self {
        self.agent_quotas.push(q);
        self
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Look up the per-agent quota for `agent_id` if one exists.
    pub fn agent_quota(&self, agent_id: &str) -> Option<&InvocationQuota> {
        self.agent_quotas.iter().find(|q| q.agent_id == agent_id)
    }

    /// Validate configuration.
    pub fn validate(&self) -> Result<(), QuotaError> {
        if self.tenant_id.is_empty() {
            return Err(QuotaError::InvalidQuota("tenant_id is empty".into()));
        }
        if self.max_invocations == 0 {
            return Err(QuotaError::InvalidQuota("max_invocations must be > 0".into()));
        }
        Ok(())
    }
}

// ── Quota manager ─────────────────────────────────────────────────────────────

/// Holds all registered `TenantQuota`s and evaluates requests.
#[derive(Debug, Default)]
pub struct QuotaManager {
    quotas: std::collections::HashMap<String, TenantQuota>,
}

impl QuotaManager {
    pub fn new() -> Self { Self::default() }

    // ── Registration ──────────────────────────────────────────────────────────

    pub fn register(&mut self, quota: TenantQuota) -> Result<(), QuotaError> {
        quota.validate()?;
        if self.quotas.contains_key(&quota.tenant_id) {
            return Err(QuotaError::AlreadyRegistered(quota.tenant_id.clone()));
        }
        self.quotas.insert(quota.tenant_id.clone(), quota);
        Ok(())
    }

    pub fn upsert(&mut self, quota: TenantQuota) -> Result<(), QuotaError> {
        quota.validate()?;
        self.quotas.insert(quota.tenant_id.clone(), quota);
        Ok(())
    }

    pub fn remove(&mut self, tenant_id: &str) -> bool {
        self.quotas.remove(tenant_id).is_some()
    }

    pub fn get(&self, tenant_id: &str) -> Option<&TenantQuota> {
        self.quotas.get(tenant_id)
    }

    pub fn tenant_count(&self) -> usize { self.quotas.len() }

    // ── Evaluation ────────────────────────────────────────────────────────────

    /// Check whether `tenant_id` may perform `invocations` calls consuming
    /// `tokens` LLM tokens given that they have already used `used_invocations`
    /// and `used_tokens` this period.
    pub fn check(
        &self,
        tenant_id: &str,
        used_invocations: u64,
        requested_invocations: u64,
        used_tokens: u64,
        requested_tokens: u64,
    ) -> Result<(), QuotaError> {
        let quota = self.quotas.get(tenant_id)
            .ok_or_else(|| QuotaError::TenantNotFound(tenant_id.to_string()))?;

        if !quota.enabled { return Ok(()); }

        let new_inv = used_invocations + requested_invocations;
        if new_inv > quota.max_invocations {
            return Err(QuotaError::InvocationLimitExceeded(
                tenant_id.to_string(), new_inv, quota.max_invocations,
            ));
        }

        let new_tok = used_tokens + requested_tokens;
        if new_tok > quota.max_tokens {
            return Err(QuotaError::TokenBudgetExceeded(
                tenant_id.to_string(), new_tok, quota.max_tokens,
            ));
        }

        Ok(())
    }

    /// Remaining invocations for a tenant given current usage.
    pub fn remaining_invocations(&self, tenant_id: &str, used: u64) -> Option<u64> {
        let quota = self.quotas.get(tenant_id)?;
        Some(quota.max_invocations.saturating_sub(used))
    }

    /// Remaining token budget for a tenant given current usage.
    pub fn remaining_tokens(&self, tenant_id: &str, used: u64) -> Option<u64> {
        let quota = self.quotas.get(tenant_id)?;
        Some(quota.max_tokens.saturating_sub(used))
    }

    /// Usage percentage (0–100) for invocations.
    pub fn invocation_usage_pct(&self, tenant_id: &str, used: u64) -> Option<f32> {
        let quota = self.quotas.get(tenant_id)?;
        if quota.max_invocations == 0 { return Some(100.0); }
        Some(used as f32 / quota.max_invocations as f32 * 100.0)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn quota(tenant: &str) -> TenantQuota {
        TenantQuota::new(tenant, 1_000, 100_000)
    }

    #[test]
    fn register_and_get() {
        let mut mgr = QuotaManager::new();
        mgr.register(quota("acme")).unwrap();
        assert!(mgr.get("acme").is_some());
    }

    #[test]
    fn register_duplicate_err() {
        let mut mgr = QuotaManager::new();
        mgr.register(quota("acme")).unwrap();
        assert_eq!(mgr.register(quota("acme")).unwrap_err(), QuotaError::AlreadyRegistered("acme".into()));
    }

    #[test]
    fn check_within_limits_ok() {
        let mut mgr = QuotaManager::new();
        mgr.register(quota("acme")).unwrap();
        assert!(mgr.check("acme", 0, 1, 0, 100).is_ok());
    }

    #[test]
    fn check_invocation_limit_exceeded() {
        let mut mgr = QuotaManager::new();
        mgr.register(quota("acme")).unwrap();
        let err = mgr.check("acme", 1_000, 1, 0, 0).unwrap_err();
        assert!(matches!(err, QuotaError::InvocationLimitExceeded(_, _, _)));
    }

    #[test]
    fn check_token_budget_exceeded() {
        let mut mgr = QuotaManager::new();
        mgr.register(quota("acme")).unwrap();
        let err = mgr.check("acme", 0, 1, 100_000, 1).unwrap_err();
        assert!(matches!(err, QuotaError::TokenBudgetExceeded(_, _, _)));
    }

    #[test]
    fn check_unknown_tenant_err() {
        let mgr = QuotaManager::new();
        assert_eq!(mgr.check("ghost", 0, 1, 0, 0).unwrap_err(), QuotaError::TenantNotFound("ghost".into()));
    }

    #[test]
    fn disabled_quota_always_ok() {
        let mut mgr = QuotaManager::new();
        mgr.register(quota("acme").with_enabled(false)).unwrap();
        // Even way over limits — disabled quota passes
        assert!(mgr.check("acme", 999_999, 1, 999_999, 1).is_ok());
    }

    #[test]
    fn remaining_invocations() {
        let mut mgr = QuotaManager::new();
        mgr.register(quota("acme")).unwrap();
        assert_eq!(mgr.remaining_invocations("acme", 400), Some(600));
    }

    #[test]
    fn remaining_tokens() {
        let mut mgr = QuotaManager::new();
        mgr.register(quota("acme")).unwrap();
        assert_eq!(mgr.remaining_tokens("acme", 10_000), Some(90_000));
    }

    #[test]
    fn invocation_usage_pct() {
        let mut mgr = QuotaManager::new();
        mgr.register(quota("acme")).unwrap();
        let pct = mgr.invocation_usage_pct("acme", 500).unwrap();
        assert!((pct - 50.0).abs() < 1e-3);
    }

    #[test]
    fn quota_period_seconds() {
        assert_eq!(QuotaPeriod::Hourly.seconds(), 3_600);
        assert_eq!(QuotaPeriod::Daily.seconds(), 86_400);
    }

    #[test]
    fn per_agent_quota_lookup() {
        let q = quota("acme").with_agent_quota(InvocationQuota::new("agent-x", 100, 5_000));
        assert!(q.agent_quota("agent-x").is_some());
        assert!(q.agent_quota("agent-y").is_none());
    }
}
