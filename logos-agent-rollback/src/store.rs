//! Version store — persists and retrieves agent version snapshots.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ── Errors ────────────────────────────────────────────────────────────────────

#[derive(Debug, Error, PartialEq)]
pub enum StoreError {
    #[error("agent '{0}' not found")]
    AgentNotFound(String),
    #[error("version {0} not found for agent '{1}'")]
    VersionNotFound(u32, String),
    #[error("duplicate version {0} for agent '{1}'")]
    DuplicateVersion(u32, String),
    #[error("no versions exist for agent '{0}'")]
    NoVersions(String),
}

// ── Data model ────────────────────────────────────────────────────────────────

/// A single point-in-time snapshot of an agent's configuration and code.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentSnapshot {
    pub agent_id: String,
    pub version: u32,
    /// Human-readable label (e.g. "v1.2.0", "release-2026-04")
    pub label: String,
    /// Arbitrary metadata: model name, prompt hash, code digest, …
    pub metadata: HashMap<String, String>,
    /// Unix-epoch timestamp when this snapshot was taken
    pub created_at: u64,
    /// Whether this version is currently the live/active one
    pub is_active: bool,
}

impl AgentSnapshot {
    pub fn new(
        agent_id: impl Into<String>,
        version: u32,
        label: impl Into<String>,
        created_at: u64,
    ) -> Self {
        Self {
            agent_id: agent_id.into(),
            version,
            label: label.into(),
            metadata: HashMap::new(),
            created_at,
            is_active: false,
        }
    }

    pub fn with_meta(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

// ── Store ─────────────────────────────────────────────────────────────────────

/// In-memory store for agent version snapshots.
///
/// Each agent has an ordered list of snapshots keyed by version number.
/// Exactly one version per agent can be active at a time.
#[derive(Debug, Default)]
pub struct VersionStore {
    // agent_id → version → snapshot
    data: HashMap<String, HashMap<u32, AgentSnapshot>>,
}

impl VersionStore {
    pub fn new() -> Self {
        Self::default()
    }

    // ── Write operations ─────────────────────────────────────────────────────

    /// Save a new snapshot. Errors on duplicate (agent_id, version) pairs.
    pub fn save(&mut self, mut snap: AgentSnapshot) -> Result<(), StoreError> {
        let entry = self.data.entry(snap.agent_id.clone()).or_default();
        if entry.contains_key(&snap.version) {
            return Err(StoreError::DuplicateVersion(snap.version, snap.agent_id));
        }
        // First saved version is automatically active
        if entry.is_empty() {
            snap.is_active = true;
        }
        entry.insert(snap.version, snap);
        Ok(())
    }

    /// Mark exactly one version as active for the given agent.
    pub fn set_active(&mut self, agent_id: &str, version: u32) -> Result<(), StoreError> {
        let entry = self
            .data
            .get_mut(agent_id)
            .ok_or_else(|| StoreError::AgentNotFound(agent_id.to_owned()))?;
        if !entry.contains_key(&version) {
            return Err(StoreError::VersionNotFound(version, agent_id.to_owned()));
        }
        for snap in entry.values_mut() {
            snap.is_active = snap.version == version;
        }
        Ok(())
    }

    /// Delete a specific snapshot.
    pub fn delete(&mut self, agent_id: &str, version: u32) -> Result<(), StoreError> {
        let entry = self
            .data
            .get_mut(agent_id)
            .ok_or_else(|| StoreError::AgentNotFound(agent_id.to_owned()))?;
        if entry.remove(&version).is_none() {
            return Err(StoreError::VersionNotFound(version, agent_id.to_owned()));
        }
        Ok(())
    }

    // ── Read operations ──────────────────────────────────────────────────────

    pub fn get(&self, agent_id: &str, version: u32) -> Result<&AgentSnapshot, StoreError> {
        self.data
            .get(agent_id)
            .ok_or_else(|| StoreError::AgentNotFound(agent_id.to_owned()))?
            .get(&version)
            .ok_or_else(|| StoreError::VersionNotFound(version, agent_id.to_owned()))
    }

    pub fn active(&self, agent_id: &str) -> Result<&AgentSnapshot, StoreError> {
        self.data
            .get(agent_id)
            .ok_or_else(|| StoreError::AgentNotFound(agent_id.to_owned()))?
            .values()
            .find(|s| s.is_active)
            .ok_or_else(|| StoreError::NoVersions(agent_id.to_owned()))
    }

    /// All versions for an agent, sorted ascending.
    pub fn list(&self, agent_id: &str) -> Result<Vec<&AgentSnapshot>, StoreError> {
        let map = self
            .data
            .get(agent_id)
            .ok_or_else(|| StoreError::AgentNotFound(agent_id.to_owned()))?;
        let mut snaps: Vec<&AgentSnapshot> = map.values().collect();
        snaps.sort_by_key(|s| s.version);
        Ok(snaps)
    }

    /// Latest (highest) version for an agent.
    pub fn latest(&self, agent_id: &str) -> Result<&AgentSnapshot, StoreError> {
        self.list(agent_id)?
            .into_iter()
            .last()
            .ok_or_else(|| StoreError::NoVersions(agent_id.to_owned()))
    }

    /// How many snapshots are stored for an agent.
    pub fn count(&self, agent_id: &str) -> usize {
        self.data
            .get(agent_id)
            .map(|m| m.len())
            .unwrap_or(0)
    }

    /// All agent IDs known to the store.
    pub fn agent_ids(&self) -> Vec<&str> {
        self.data.keys().map(String::as_str).collect()
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(agent: &str, ver: u32) -> AgentSnapshot {
        AgentSnapshot::new(agent, ver, format!("v{ver}"), 1_000 + ver as u64)
    }

    #[test]
    fn save_and_get() {
        let mut s = VersionStore::new();
        s.save(snap("bot", 1)).unwrap();
        assert_eq!(s.get("bot", 1).unwrap().version, 1);
    }

    #[test]
    fn first_version_auto_active() {
        let mut s = VersionStore::new();
        s.save(snap("bot", 1)).unwrap();
        assert!(s.active("bot").unwrap().is_active);
    }

    #[test]
    fn duplicate_version_errors() {
        let mut s = VersionStore::new();
        s.save(snap("bot", 1)).unwrap();
        assert_eq!(
            s.save(snap("bot", 1)),
            Err(StoreError::DuplicateVersion(1, "bot".into()))
        );
    }

    #[test]
    fn set_active_changes_marker() {
        let mut s = VersionStore::new();
        s.save(snap("bot", 1)).unwrap();
        s.save(snap("bot", 2)).unwrap();
        s.set_active("bot", 2).unwrap();
        assert_eq!(s.active("bot").unwrap().version, 2);
        assert!(!s.get("bot", 1).unwrap().is_active);
    }

    #[test]
    fn set_active_unknown_agent_errors() {
        let mut s = VersionStore::new();
        assert_eq!(
            s.set_active("nope", 1),
            Err(StoreError::AgentNotFound("nope".into()))
        );
    }

    #[test]
    fn set_active_unknown_version_errors() {
        let mut s = VersionStore::new();
        s.save(snap("bot", 1)).unwrap();
        assert_eq!(
            s.set_active("bot", 99),
            Err(StoreError::VersionNotFound(99, "bot".into()))
        );
    }

    #[test]
    fn delete_removes_version() {
        let mut s = VersionStore::new();
        s.save(snap("bot", 1)).unwrap();
        s.save(snap("bot", 2)).unwrap();
        s.delete("bot", 1).unwrap();
        assert_eq!(s.count("bot"), 1);
    }

    #[test]
    fn delete_missing_errors() {
        let mut s = VersionStore::new();
        s.save(snap("bot", 1)).unwrap();
        assert_eq!(
            s.delete("bot", 9),
            Err(StoreError::VersionNotFound(9, "bot".into()))
        );
    }

    #[test]
    fn list_sorted_ascending() {
        let mut s = VersionStore::new();
        s.save(snap("bot", 3)).unwrap();
        s.save(snap("bot", 1)).unwrap();
        s.save(snap("bot", 2)).unwrap();
        let versions: Vec<u32> = s.list("bot").unwrap().iter().map(|s| s.version).collect();
        assert_eq!(versions, vec![1, 2, 3]);
    }

    #[test]
    fn latest_returns_highest() {
        let mut s = VersionStore::new();
        s.save(snap("bot", 1)).unwrap();
        s.save(snap("bot", 5)).unwrap();
        s.save(snap("bot", 3)).unwrap();
        assert_eq!(s.latest("bot").unwrap().version, 5);
    }

    #[test]
    fn count_and_agent_ids() {
        let mut s = VersionStore::new();
        s.save(snap("alpha", 1)).unwrap();
        s.save(snap("alpha", 2)).unwrap();
        s.save(snap("beta", 1)).unwrap();
        assert_eq!(s.count("alpha"), 2);
        assert_eq!(s.count("beta"), 1);
        assert_eq!(s.count("gamma"), 0);
        let mut ids = s.agent_ids();
        ids.sort();
        assert_eq!(ids, vec!["alpha", "beta"]);
    }

    #[test]
    fn with_meta_stores_metadata() {
        let snap = AgentSnapshot::new("bot", 1, "v1", 0)
            .with_meta("model", "gpt-4o")
            .with_meta("hash", "abc123");
        assert_eq!(snap.metadata["model"], "gpt-4o");
        assert_eq!(snap.metadata["hash"], "abc123");
    }

    #[test]
    fn no_versions_active_error() {
        let s = VersionStore::new();
        assert_eq!(s.active("ghost"), Err(StoreError::AgentNotFound("ghost".into())));
    }

    #[test]
    fn get_unknown_agent_errors() {
        let s = VersionStore::new();
        assert_eq!(s.get("ghost", 1), Err(StoreError::AgentNotFound("ghost".into())));
    }
}
