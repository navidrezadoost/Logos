//! Usage tracking — records per-tenant, per-agent invocation and token counts.
//!
//! `UsageTracker` accumulates `TenantUsage` snapshots.  The `QuotaManager`
//! (in `quota.rs`) receives these values when evaluating whether a request
//! should be permitted.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Snapshot ──────────────────────────────────────────────────────────────────

/// A point-in-time summary of one agent's usage within a tenant.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageSnapshot {
    pub agent_id: String,
    pub invocations: u64,
    pub tokens: u64,
}

// ── Tenant usage ──────────────────────────────────────────────────────────────

/// Aggregated usage for one tenant (across all agents).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TenantUsage {
    pub tenant_id: String,
    /// Total invocations this period.
    pub invocations: u64,
    /// Total LLM tokens consumed this period.
    pub tokens: u64,
    /// Per-agent breakdown.
    pub by_agent: Vec<UsageSnapshot>,
}

impl TenantUsage {
    fn new(tenant_id: impl Into<String>) -> Self {
        Self { tenant_id: tenant_id.into(), ..Default::default() }
    }

    /// Usage for a specific agent.
    pub fn agent_snapshot(&self, agent_id: &str) -> Option<&UsageSnapshot> {
        self.by_agent.iter().find(|s| s.agent_id == agent_id)
    }

    /// Percentage of `max_invocations` used (0–100).
    pub fn invocation_pct(&self, max_invocations: u64) -> f32 {
        if max_invocations == 0 { return 100.0; }
        self.invocations as f32 / max_invocations as f32 * 100.0
    }
}

// ── Usage tracker ─────────────────────────────────────────────────────────────

/// Accumulates usage across all tenants and agents.
#[derive(Debug, Default)]
pub struct UsageTracker {
    usage: HashMap<String, HashMap<String, UsageSnapshot>>,
}

impl UsageTracker {
    pub fn new() -> Self { Self::default() }

    // ── Write ─────────────────────────────────────────────────────────────────

    /// Record `invocations` invocations consuming `tokens` tokens for
    /// `(tenant_id, agent_id)`.
    pub fn record(
        &mut self,
        tenant_id: &str,
        agent_id: &str,
        invocations: u64,
        tokens: u64,
    ) {
        let snap = self.usage
            .entry(tenant_id.to_string())
            .or_default()
            .entry(agent_id.to_string())
            .or_insert_with(|| UsageSnapshot {
                agent_id: agent_id.to_string(),
                ..Default::default()
            });
        snap.invocations += invocations;
        snap.tokens += tokens;
    }

    /// Reset all usage for a tenant (e.g. at period rollover).
    pub fn reset_tenant(&mut self, tenant_id: &str) {
        self.usage.remove(tenant_id);
    }

    /// Reset all usage across all tenants.
    pub fn reset_all(&mut self) {
        self.usage.clear();
    }

    // ── Read ──────────────────────────────────────────────────────────────────

    /// Aggregate `TenantUsage` for `tenant_id`.
    pub fn usage_for(&self, tenant_id: &str) -> TenantUsage {
        let agents = match self.usage.get(tenant_id) {
            None => return TenantUsage::new(tenant_id),
            Some(m) => m,
        };
        let mut usage = TenantUsage::new(tenant_id);
        for snap in agents.values() {
            usage.invocations += snap.invocations;
            usage.tokens += snap.tokens;
            usage.by_agent.push(snap.clone());
        }
        usage.by_agent.sort_by(|a, b| a.agent_id.cmp(&b.agent_id));
        usage
    }

    /// All tenant IDs that have any recorded usage.
    pub fn active_tenants(&self) -> Vec<&str> {
        let mut ids: Vec<&str> = self.usage.keys().map(|s| s.as_str()).collect();
        ids.sort_unstable();
        ids
    }

    /// Total invocations across all tenants.
    pub fn global_invocations(&self) -> u64 {
        self.usage.values()
            .flat_map(|m| m.values())
            .map(|s| s.invocations)
            .sum()
    }

    /// Total tokens across all tenants.
    pub fn global_tokens(&self) -> u64 {
        self.usage.values()
            .flat_map(|m| m.values())
            .map(|s| s.tokens)
            .sum()
    }

    /// Whether `tenant_id` would exceed `max_invocations` if `n` more requests
    /// were added.
    pub fn would_exceed_invocations(&self, tenant_id: &str, n: u64, max: u64) -> bool {
        let current = self.usage_for(tenant_id).invocations;
        current + n > max
    }

    /// Whether `tenant_id` would exceed `max_tokens` if `t` more tokens
    /// were consumed.
    pub fn would_exceed_tokens(&self, tenant_id: &str, t: u64, max: u64) -> bool {
        let current = self.usage_for(tenant_id).tokens;
        current + t > max
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_and_retrieve() {
        let mut t = UsageTracker::new();
        t.record("acme", "agent-x", 5, 1_000);
        let u = t.usage_for("acme");
        assert_eq!(u.invocations, 5);
        assert_eq!(u.tokens, 1_000);
    }

    #[test]
    fn accumulates_multiple_records() {
        let mut t = UsageTracker::new();
        t.record("acme", "agent-x", 3, 300);
        t.record("acme", "agent-x", 2, 200);
        let u = t.usage_for("acme");
        assert_eq!(u.invocations, 5);
        assert_eq!(u.tokens, 500);
    }

    #[test]
    fn by_agent_breakdown() {
        let mut t = UsageTracker::new();
        t.record("acme", "agent-a", 2, 100);
        t.record("acme", "agent-b", 3, 200);
        let u = t.usage_for("acme");
        assert_eq!(u.by_agent.len(), 2);
        assert!(u.agent_snapshot("agent-a").is_some());
    }

    #[test]
    fn empty_tenant_returns_zero() {
        let t = UsageTracker::new();
        let u = t.usage_for("nobody");
        assert_eq!(u.invocations, 0);
        assert_eq!(u.tokens, 0);
    }

    #[test]
    fn reset_tenant_clears() {
        let mut t = UsageTracker::new();
        t.record("acme", "ag", 10, 1_000);
        t.reset_tenant("acme");
        assert_eq!(t.usage_for("acme").invocations, 0);
    }

    #[test]
    fn reset_all_clears() {
        let mut t = UsageTracker::new();
        t.record("a", "ag", 5, 500);
        t.record("b", "ag", 5, 500);
        t.reset_all();
        assert_eq!(t.global_invocations(), 0);
    }

    #[test]
    fn active_tenants() {
        let mut t = UsageTracker::new();
        t.record("a", "ag", 1, 0);
        t.record("b", "ag", 1, 0);
        assert_eq!(t.active_tenants().len(), 2);
    }

    #[test]
    fn global_invocations() {
        let mut t = UsageTracker::new();
        t.record("a", "ag",  3, 0);
        t.record("b", "ag",  7, 0);
        assert_eq!(t.global_invocations(), 10);
    }

    #[test]
    fn would_exceed_invocations_true() {
        let mut t = UsageTracker::new();
        t.record("acme", "ag", 99, 0);
        assert!(t.would_exceed_invocations("acme", 2, 100));
    }

    #[test]
    fn would_exceed_tokens_false_at_boundary() {
        let mut t = UsageTracker::new();
        t.record("acme", "ag", 0, 500);
        assert!(!t.would_exceed_tokens("acme", 500, 1_000));
    }
}
