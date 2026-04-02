//! Version registry — per-agent ordered history of `AgentSnapshot`s.
//!
//! `VersionRegistry` is the in-memory store for all committed snapshots.
//! It is consumed by `RollbackManager` but can also be used standalone for
//! read-only inspection of version history.

use crate::version::{AgentSnapshot, SemVer};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

// ── Error ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RegistryError {
    #[error("agent '{0}' has no committed versions")]
    AgentNotFound(String),

    #[error("version {0} already exists for agent '{1}'")]
    DuplicateVersion(String, String),

    #[error("version {0} not found for agent '{1}'")]
    VersionNotFound(String, String),
}

// ── Version registry ──────────────────────────────────────────────────────────

/// In-memory store of all committed `AgentSnapshot`s, organised by agent ID.
///
/// Snapshots are kept in insertion order. `max_versions` controls the maximum
/// number of snapshots retained per agent (0 = unlimited).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionRegistry {
    /// agent_id → snapshots (most-recent last)
    history: HashMap<String, Vec<AgentSnapshot>>,
    /// 0 = unlimited
    pub max_versions: usize,
}

impl VersionRegistry {
    pub fn new() -> Self {
        Self { history: HashMap::new(), max_versions: 0 }
    }

    pub fn with_max_versions(n: usize) -> Self {
        Self { history: HashMap::new(), max_versions: n }
    }

    /// Commit a snapshot. Returns error if the same version already exists.
    pub fn commit(&mut self, snapshot: AgentSnapshot) -> Result<(), RegistryError> {
        let agent_id = snapshot.agent_id().to_string();
        let version = snapshot.version().clone();

        let entries = self.history.entry(agent_id.clone()).or_default();

        // Reject duplicates.
        if entries.iter().any(|s| s.version() == &version) {
            return Err(RegistryError::DuplicateVersion(version.to_string(), agent_id));
        }

        entries.push(snapshot);

        // Trim to max_versions if set.
        if self.max_versions > 0 && entries.len() > self.max_versions {
            // Remove oldest (front).
            let excess = entries.len() - self.max_versions;
            entries.drain(0..excess);
        }

        Ok(())
    }

    /// Full history slice for an agent (oldest → newest).
    pub fn history(&self, agent_id: &str) -> &[AgentSnapshot] {
        self.history.get(agent_id).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Most-recently committed snapshot for an agent.
    pub fn latest(&self, agent_id: &str) -> Option<&AgentSnapshot> {
        self.history.get(agent_id)?.last()
    }

    /// Retrieve a specific version of an agent.
    pub fn get_version(&self, agent_id: &str, ver: &SemVer) -> Result<&AgentSnapshot, RegistryError> {
        let entries = self.history.get(agent_id)
            .ok_or_else(|| RegistryError::AgentNotFound(agent_id.into()))?;
        entries
            .iter()
            .find(|s| s.version() == ver)
            .ok_or_else(|| RegistryError::VersionNotFound(ver.to_string(), agent_id.into()))
    }

    /// All agent IDs that have at least one committed version.
    pub fn agent_ids(&self) -> Vec<&str> {
        self.history
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(k, _)| k.as_str())
            .collect()
    }

    /// Number of snapshots stored for an agent.
    pub fn version_count(&self, agent_id: &str) -> usize {
        self.history.get(agent_id).map(|v| v.len()).unwrap_or(0)
    }

    /// Sorted list of committed versions for an agent (ascending).
    pub fn versions(&self, agent_id: &str) -> Vec<SemVer> {
        let mut vs: Vec<SemVer> = self.history(agent_id).iter().map(|s| s.version().clone()).collect();
        vs.sort();
        vs
    }
}

impl Default for VersionRegistry {
    fn default() -> Self { Self::new() }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::version::AgentSnapshot;

    fn snap(agent: &str, major: u32, minor: u32, patch: u32) -> AgentSnapshot {
        AgentSnapshot::builder(agent, SemVer::new(major, minor, patch))
            .config_str("model", "gpt-4o")
            .build()
    }

    // ── commit ────────────────────────────────────────────────────────────────

    #[test]
    fn registry_commit_stores_snapshot() {
        let mut reg = VersionRegistry::new();
        reg.commit(snap("ag-1", 1, 0, 0)).unwrap();
        assert_eq!(reg.version_count("ag-1"), 1);
    }

    #[test]
    fn registry_commit_duplicate_returns_error() {
        let mut reg = VersionRegistry::new();
        reg.commit(snap("ag-1", 1, 0, 0)).unwrap();
        let err = reg.commit(snap("ag-1", 1, 0, 0)).unwrap_err();
        assert!(matches!(err, RegistryError::DuplicateVersion(_, _)));
    }

    #[test]
    fn registry_commit_multiple_versions() {
        let mut reg = VersionRegistry::new();
        reg.commit(snap("ag-1", 1, 0, 0)).unwrap();
        reg.commit(snap("ag-1", 1, 1, 0)).unwrap();
        reg.commit(snap("ag-1", 2, 0, 0)).unwrap();
        assert_eq!(reg.version_count("ag-1"), 3);
    }

    // ── history & latest ──────────────────────────────────────────────────────

    #[test]
    fn registry_history_order() {
        let mut reg = VersionRegistry::new();
        reg.commit(snap("ag-2", 1, 0, 0)).unwrap();
        reg.commit(snap("ag-2", 1, 0, 1)).unwrap();
        let h = reg.history("ag-2");
        assert_eq!(h[0].version(), &SemVer::new(1, 0, 0));
        assert_eq!(h[1].version(), &SemVer::new(1, 0, 1));
    }

    #[test]
    fn registry_latest_returns_last_committed() {
        let mut reg = VersionRegistry::new();
        reg.commit(snap("ag-3", 1, 0, 0)).unwrap();
        reg.commit(snap("ag-3", 1, 1, 0)).unwrap();
        assert_eq!(reg.latest("ag-3").unwrap().version(), &SemVer::new(1, 1, 0));
    }

    // ── get_version ───────────────────────────────────────────────────────────

    #[test]
    fn registry_get_version_found() {
        let mut reg = VersionRegistry::new();
        reg.commit(snap("ag-4", 1, 0, 0)).unwrap();
        reg.commit(snap("ag-4", 1, 1, 0)).unwrap();
        let s = reg.get_version("ag-4", &SemVer::new(1, 0, 0)).unwrap();
        assert_eq!(s.version(), &SemVer::new(1, 0, 0));
    }

    #[test]
    fn registry_get_version_not_found() {
        let mut reg = VersionRegistry::new();
        reg.commit(snap("ag-5", 1, 0, 0)).unwrap();
        let err = reg.get_version("ag-5", &SemVer::new(9, 9, 9)).unwrap_err();
        assert!(matches!(err, RegistryError::VersionNotFound(_, _)));
    }

    // ── max_versions ──────────────────────────────────────────────────────────

    #[test]
    fn registry_max_versions_caps_history() {
        let mut reg = VersionRegistry::with_max_versions(2);
        reg.commit(snap("ag-6", 1, 0, 0)).unwrap();
        reg.commit(snap("ag-6", 1, 1, 0)).unwrap();
        reg.commit(snap("ag-6", 1, 2, 0)).unwrap();
        // Oldest (1.0.0) should be pruned.
        assert_eq!(reg.version_count("ag-6"), 2);
        assert!(reg.get_version("ag-6", &SemVer::new(1, 0, 0)).is_err());
    }

    // ── agent_ids & versions ──────────────────────────────────────────────────

    #[test]
    fn registry_agent_ids() {
        let mut reg = VersionRegistry::new();
        reg.commit(snap("alpha", 1, 0, 0)).unwrap();
        reg.commit(snap("beta",  1, 0, 0)).unwrap();
        let ids = reg.agent_ids();
        assert!(ids.contains(&"alpha"));
        assert!(ids.contains(&"beta"));
    }

    #[test]
    fn registry_versions_sorted() {
        let mut reg = VersionRegistry::new();
        reg.commit(snap("ag-7", 1, 2, 0)).unwrap();
        reg.commit(snap("ag-7", 1, 0, 0)).unwrap();
        let vs = reg.versions("ag-7");
        assert_eq!(vs[0], SemVer::new(1, 0, 0));
        assert_eq!(vs[1], SemVer::new(1, 2, 0));
    }
}
