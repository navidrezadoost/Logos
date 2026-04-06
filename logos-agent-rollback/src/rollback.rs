//! Rollback engine — activates a previous agent version and records history.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::store::{StoreError, VersionStore};

// ── Errors ────────────────────────────────────────────────────────────────────

#[derive(Debug, Error, PartialEq)]
pub enum RollbackError {
    #[error("store error: {0}")]
    Store(#[from] StoreError),
    #[error("cannot roll back to the currently active version {0}")]
    AlreadyActive(u32),
    #[error("rollback history is empty for agent '{0}'")]
    NoHistory(String),
}

// ── History record ────────────────────────────────────────────────────────────

/// One entry in the rollback audit log.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RollbackRecord {
    pub agent_id: String,
    pub from_version: u32,
    pub to_version: u32,
    pub reason: String,
    pub timestamp: u64,
}

/// Reason codes for structured rollback reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RollbackReason {
    Regression,
    PerformanceDegradation,
    SecurityPatch,
    ManualOverride,
}

impl RollbackReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Regression => "regression",
            Self::PerformanceDegradation => "performance_degradation",
            Self::SecurityPatch => "security_patch",
            Self::ManualOverride => "manual_override",
        }
    }
}

// ── Engine ────────────────────────────────────────────────────────────────────

/// Manages rollback operations on top of a [`VersionStore`].
#[derive(Debug, Default)]
pub struct RollbackEngine {
    history: HashMap<String, Vec<RollbackRecord>>,
}

impl RollbackEngine {
    pub fn new() -> Self {
        Self::default()
    }

    // ── Core rollback ─────────────────────────────────────────────────────────

    /// Roll back `agent_id` to `target_version`.
    ///
    /// Records the transition in the audit log and updates the active marker
    /// in the store.
    pub fn rollback(
        &mut self,
        store: &mut VersionStore,
        agent_id: &str,
        target_version: u32,
        reason: &str,
        timestamp: u64,
    ) -> Result<RollbackRecord, RollbackError> {
        let from_version = store.active(agent_id)?.version;
        if from_version == target_version {
            return Err(RollbackError::AlreadyActive(target_version));
        }
        store.set_active(agent_id, target_version)?;
        let record = RollbackRecord {
            agent_id: agent_id.to_owned(),
            from_version,
            to_version: target_version,
            reason: reason.to_owned(),
            timestamp,
        };
        self.history
            .entry(agent_id.to_owned())
            .or_default()
            .push(record.clone());
        Ok(record)
    }

    /// Roll back to the immediately preceding version.
    pub fn rollback_one(
        &mut self,
        store: &mut VersionStore,
        agent_id: &str,
        reason: &str,
        timestamp: u64,
    ) -> Result<RollbackRecord, RollbackError> {
        let current = store.active(agent_id)?.version;
        let versions = store.list(agent_id)?;
        let prev_version = versions
            .iter()
            .rev()
            .skip_while(|s| s.version >= current)
            .next()
            .map(|s| s.version)
            .ok_or_else(|| RollbackError::NoHistory(agent_id.to_owned()))?;
        self.rollback(store, agent_id, prev_version, reason, timestamp)
    }

    // ── History queries ───────────────────────────────────────────────────────

    /// Audit log for an agent (oldest first).
    pub fn history(&self, agent_id: &str) -> &[RollbackRecord] {
        self.history
            .get(agent_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Total rollbacks performed for an agent.
    pub fn rollback_count(&self, agent_id: &str) -> usize {
        self.history(agent_id).len()
    }

    /// Most recent rollback record.
    pub fn last_rollback(&self, agent_id: &str) -> Option<&RollbackRecord> {
        self.history(agent_id).last()
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::AgentSnapshot;

    fn snap(agent: &str, ver: u32) -> AgentSnapshot {
        AgentSnapshot::new(agent, ver, format!("v{ver}"), 1_000 + ver as u64)
    }

    fn store_with(snaps: &[(&str, u32)]) -> VersionStore {
        let mut s = VersionStore::new();
        for &(agent, ver) in snaps {
            s.save(snap(agent, ver)).unwrap();
        }
        s
    }

    #[test]
    fn rollback_changes_active() {
        let mut store = store_with(&[("bot", 1), ("bot", 2), ("bot", 3)]);
        store.set_active("bot", 3).unwrap();
        let mut engine = RollbackEngine::new();
        engine.rollback(&mut store, "bot", 1, "regression", 9000).unwrap();
        assert_eq!(store.active("bot").unwrap().version, 1);
    }

    #[test]
    fn rollback_records_history() {
        let mut store = store_with(&[("bot", 1), ("bot", 2)]);
        store.set_active("bot", 2).unwrap();
        let mut engine = RollbackEngine::new();
        engine.rollback(&mut store, "bot", 1, "bug", 5000).unwrap();
        assert_eq!(engine.rollback_count("bot"), 1);
        let rec = engine.last_rollback("bot").unwrap();
        assert_eq!(rec.from_version, 2);
        assert_eq!(rec.to_version, 1);
        assert_eq!(rec.reason, "bug");
        assert_eq!(rec.timestamp, 5000);
    }

    #[test]
    fn rollback_to_active_errors() {
        let mut store = store_with(&[("bot", 1)]);
        let mut engine = RollbackEngine::new();
        assert_eq!(
            engine.rollback(&mut store, "bot", 1, "x", 0),
            Err(RollbackError::AlreadyActive(1))
        );
    }

    #[test]
    fn rollback_to_unknown_version_errors() {
        let mut store = store_with(&[("bot", 1)]);
        let mut engine = RollbackEngine::new();
        assert!(engine.rollback(&mut store, "bot", 99, "x", 0).is_err());
    }

    #[test]
    fn rollback_one_step_back() {
        let mut store = store_with(&[("bot", 1), ("bot", 2), ("bot", 3)]);
        store.set_active("bot", 3).unwrap();
        let mut engine = RollbackEngine::new();
        engine.rollback_one(&mut store, "bot", "perf", 1).unwrap();
        assert_eq!(store.active("bot").unwrap().version, 2);
    }

    #[test]
    fn rollback_one_no_previous_errors() {
        let mut store = store_with(&[("bot", 1)]);
        let mut engine = RollbackEngine::new();
        assert_eq!(
            engine.rollback_one(&mut store, "bot", "x", 0),
            Err(RollbackError::NoHistory("bot".into()))
        );
    }

    #[test]
    fn multiple_rollbacks_accumulate_history() {
        let mut store = store_with(&[("bot", 1), ("bot", 2), ("bot", 3)]);
        store.set_active("bot", 3).unwrap();
        let mut engine = RollbackEngine::new();
        engine.rollback(&mut store, "bot", 2, "r1", 1).unwrap();
        engine.rollback(&mut store, "bot", 1, "r2", 2).unwrap();
        assert_eq!(engine.rollback_count("bot"), 2);
    }

    #[test]
    fn history_empty_for_unknown_agent() {
        let engine = RollbackEngine::new();
        assert!(engine.history("ghost").is_empty());
    }

    #[test]
    fn reason_enum_as_str() {
        assert_eq!(RollbackReason::Regression.as_str(), "regression");
        assert_eq!(RollbackReason::ManualOverride.as_str(), "manual_override");
        assert_eq!(RollbackReason::SecurityPatch.as_str(), "security_patch");
        assert_eq!(
            RollbackReason::PerformanceDegradation.as_str(),
            "performance_degradation"
        );
    }
}
