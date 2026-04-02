//! Rollback manager — restore an agent to a previous version.
//!
//! `RollbackManager` wraps a `VersionRegistry` and adds rollback logic.
//! A `RollbackPolicy` controls which versions are eligible for rollback and
//! whether the registry is trimmed after a restore.

use crate::{
    registry::{RegistryError, VersionRegistry},
    version::{AgentSnapshot, SemVer},
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ── Policy ────────────────────────────────────────────────────────────────────

/// Controls how the registry behaves around rollbacks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RollbackPolicy {
    /// Keep every committed version; never prune.
    KeepAll,
    /// Keep only the most-recent N versions per agent.
    KeepLatestN(usize),
    /// Only allow rollback to snapshots that carry a specific tag.
    RequireTag(String),
}

impl Default for RollbackPolicy {
    fn default() -> Self { Self::KeepAll }
}

// ── Rollback status ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RollbackStatus {
    /// Rollback succeeded; the agent is now at `target_version`.
    Success,
    /// The requested version does not exist in the registry.
    VersionNotFound,
    /// The agent is already at the requested version.
    AlreadyCurrent,
    /// The policy forbids rolling back to this snapshot.
    PolicyDenied { reason: String },
}

// ── Rollback request ──────────────────────────────────────────────────────────

/// Describes a requested rollback operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackRequest {
    pub agent_id: String,
    pub target_version: SemVer,
    pub reason: Option<String>,
}

impl RollbackRequest {
    pub fn new(agent_id: impl Into<String>, target_version: SemVer) -> Self {
        Self { agent_id: agent_id.into(), target_version, reason: None }
    }

    pub fn with_reason(mut self, r: impl Into<String>) -> Self {
        self.reason = Some(r.into());
        self
    }
}

// ── Rollback result ───────────────────────────────────────────────────────────

/// Outcome of a processed `RollbackRequest`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackResult {
    pub request: RollbackRequest,
    pub status: RollbackStatus,
    /// The snapshot that was restored (only set on `Success`).
    pub restored: Option<AgentSnapshot>,
    /// The version that was active before the rollback (if known).
    pub previous_version: Option<SemVer>,
}

impl RollbackResult {
    pub fn success(req: RollbackRequest, restored: AgentSnapshot, prev: SemVer) -> Self {
        Self { request: req, status: RollbackStatus::Success, restored: Some(restored), previous_version: Some(prev) }
    }

    pub fn version_not_found(req: RollbackRequest) -> Self {
        Self { request: req, status: RollbackStatus::VersionNotFound, restored: None, previous_version: None }
    }

    pub fn already_current(req: RollbackRequest, ver: SemVer) -> Self {
        Self { request: req, status: RollbackStatus::AlreadyCurrent, restored: None, previous_version: Some(ver) }
    }

    pub fn policy_denied(req: RollbackRequest, reason: impl Into<String>) -> Self {
        Self {
            request: req,
            status: RollbackStatus::PolicyDenied { reason: reason.into() },
            restored: None,
            previous_version: None,
        }
    }

    pub fn is_success(&self) -> bool { self.status == RollbackStatus::Success }
}

// ── Rollback error ────────────────────────────────────────────────────────────

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RollbackError {
    #[error("registry error: {0}")]
    Registry(#[from] RegistryError),
}

// ── Rollback manager ──────────────────────────────────────────────────────────

/// Manages version history and rollback for all registered agents.
pub struct RollbackManager {
    registry: VersionRegistry,
    policy: RollbackPolicy,
    /// Audit log of all applied rollbacks (newest last).
    audit_log: Vec<RollbackResult>,
}

impl RollbackManager {
    pub fn new(policy: RollbackPolicy) -> Self {
        let reg = match &policy {
            RollbackPolicy::KeepLatestN(n) => VersionRegistry::with_max_versions(*n),
            _ => VersionRegistry::new(),
        };
        Self { registry: reg, policy, audit_log: Vec::new() }
    }

    /// Commit a new snapshot for the agent. Returns an error if the version
    /// already exists or the policy rejects it.
    pub fn commit_snapshot(&mut self, snap: AgentSnapshot) -> Result<(), RollbackError> {
        self.registry.commit(snap)?;
        Ok(())
    }

    /// Attempt to roll an agent back to a specific version.
    pub fn rollback(&mut self, req: RollbackRequest) -> RollbackResult {
        let agent_id  = req.agent_id.clone();
        let target    = req.target_version.clone();

        // Guard: agent must have some history.
        if self.registry.version_count(&agent_id) == 0 {
            let result = RollbackResult::version_not_found(req);
            self.audit_log.push(result.clone());
            return result;
        }

        // Guard: current version check.
        if let Some(latest) = self.registry.latest(&agent_id) {
            if latest.version() == &target {
                let result = RollbackResult::already_current(req, target.clone());
                self.audit_log.push(result.clone());
                return result;
            }
        }

        // Policy check.
        if let RollbackPolicy::RequireTag(required_tag) = self.policy.clone() {
            let tag_ok = self.registry.history(&agent_id)
                .iter()
                .find(|s| s.version() == &target)
                .map(|s| s.metadata.has_tag(&required_tag));
            match tag_ok {
                None => {
                    let result = RollbackResult::version_not_found(req);
                    self.audit_log.push(result.clone());
                    return result;
                }
                Some(false) => {
                    let reason = format!("target version must have tag '{required_tag}'");
                    let result = RollbackResult::policy_denied(req, reason);
                    self.audit_log.push(result.clone());
                    return result;
                }
                Some(true) => {} // OK
            }
        }

        // Resolve target snapshot.
        let prev_ver = self.registry.latest(&agent_id).map(|s| s.version().clone());
        match self.registry.get_version(&agent_id, &target) {
            Err(_) => {
                let result = RollbackResult::version_not_found(req);
                self.audit_log.push(result.clone());
                result
            }
            Ok(target_snap) => {
                let restored = target_snap.clone();
                let prev = prev_ver.unwrap_or_else(|| target.clone());
                let result = RollbackResult::success(req, restored, prev);
                self.audit_log.push(result.clone());
                result
            }
        }
    }

    // ── Delegation to registry ────────────────────────────────────────────────

    pub fn history(&self, agent_id: &str) -> &[AgentSnapshot] {
        self.registry.history(agent_id)
    }

    pub fn latest(&self, agent_id: &str) -> Option<&AgentSnapshot> {
        self.registry.latest(agent_id)
    }

    pub fn version_count(&self, agent_id: &str) -> usize {
        self.registry.version_count(agent_id)
    }

    pub fn audit_log(&self) -> &[RollbackResult] {
        &self.audit_log
    }
}

impl Default for RollbackManager {
    fn default() -> Self { Self::new(RollbackPolicy::KeepAll) }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::version::{AgentSnapshot, SemVer};

    fn snap(agent: &str, major: u32, minor: u32, patch: u32) -> AgentSnapshot {
        AgentSnapshot::builder(agent, SemVer::new(major, minor, patch))
            .config_str("model", "gpt-4o")
            .build()
    }

    fn snap_tagged(agent: &str, major: u32, tag: &str) -> AgentSnapshot {
        AgentSnapshot::builder(agent, SemVer::new(major, 0, 0))
            .config_str("model", "gpt-4o")
            .tag(tag)
            .build()
    }

    fn setup_two_versions(agent: &str) -> RollbackManager {
        let mut mgr = RollbackManager::default();
        mgr.commit_snapshot(snap(agent, 1, 0, 0)).unwrap();
        mgr.commit_snapshot(snap(agent, 1, 1, 0)).unwrap();
        mgr
    }

    // ── commit ────────────────────────────────────────────────────────────────

    #[test]
    fn manager_commit_stores_snapshot() {
        let mut mgr = RollbackManager::default();
        mgr.commit_snapshot(snap("ag", 1, 0, 0)).unwrap();
        assert_eq!(mgr.version_count("ag"), 1);
    }

    // ── rollback success ──────────────────────────────────────────────────────

    #[test]
    fn rollback_success() {
        let mut mgr = setup_two_versions("ag-rb");
        let req = RollbackRequest::new("ag-rb", SemVer::new(1, 0, 0));
        let result = mgr.rollback(req);
        assert!(result.is_success());
        assert_eq!(result.restored.as_ref().unwrap().version(), &SemVer::new(1, 0, 0));
    }

    #[test]
    fn rollback_success_previous_version_recorded() {
        let mut mgr = setup_two_versions("ag-prev");
        let req = RollbackRequest::new("ag-prev", SemVer::new(1, 0, 0));
        let result = mgr.rollback(req);
        assert_eq!(result.previous_version.as_ref().unwrap(), &SemVer::new(1, 1, 0));
    }

    // ── rollback errors ───────────────────────────────────────────────────────

    #[test]
    fn rollback_version_not_found() {
        let mut mgr = setup_two_versions("ag-nf");
        let req = RollbackRequest::new("ag-nf", SemVer::new(9, 0, 0));
        let result = mgr.rollback(req);
        assert_eq!(result.status, RollbackStatus::VersionNotFound);
    }

    #[test]
    fn rollback_unknown_agent_returns_not_found() {
        let mut mgr = RollbackManager::default();
        let req = RollbackRequest::new("ghost", SemVer::new(1, 0, 0));
        let result = mgr.rollback(req);
        assert_eq!(result.status, RollbackStatus::VersionNotFound);
    }

    #[test]
    fn rollback_already_current() {
        let mut mgr = setup_two_versions("ag-ac");
        let req = RollbackRequest::new("ag-ac", SemVer::new(1, 1, 0));
        let result = mgr.rollback(req);
        assert_eq!(result.status, RollbackStatus::AlreadyCurrent);
    }

    // ── policy ────────────────────────────────────────────────────────────────

    #[test]
    fn rollback_policy_require_tag_denied() {
        let mut mgr = RollbackManager::new(RollbackPolicy::RequireTag("stable".into()));
        mgr.commit_snapshot(snap_tagged("ag-tag", 1, "beta")).unwrap();
        mgr.commit_snapshot(snap_tagged("ag-tag", 2, "stable")).unwrap();
        // Roll back to v1, which only has "beta" tag.
        let req = RollbackRequest::new("ag-tag", SemVer::new(1, 0, 0));
        let result = mgr.rollback(req);
        assert!(matches!(result.status, RollbackStatus::PolicyDenied { .. }));
    }

    #[test]
    fn rollback_policy_require_tag_approved() {
        let mut mgr = RollbackManager::new(RollbackPolicy::RequireTag("stable".into()));
        mgr.commit_snapshot(snap_tagged("ag-tag2", 1, "stable")).unwrap();
        mgr.commit_snapshot(snap_tagged("ag-tag2", 2, "beta")).unwrap();
        let req = RollbackRequest::new("ag-tag2", SemVer::new(1, 0, 0));
        let result = mgr.rollback(req);
        assert!(result.is_success());
    }

    // ── audit log ─────────────────────────────────────────────────────────────

    #[test]
    fn rollback_audit_log_grows() {
        let mut mgr = setup_two_versions("ag-audit");
        mgr.rollback(RollbackRequest::new("ag-audit", SemVer::new(1, 0, 0)));
        assert_eq!(mgr.audit_log().len(), 1);
    }
}
