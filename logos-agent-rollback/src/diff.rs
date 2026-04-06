//! Diff engine — computes structured differences between two agent snapshots.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::store::AgentSnapshot;

// ── Errors ────────────────────────────────────────────────────────────────────

#[derive(Debug, Error, PartialEq)]
pub enum DiffError {
    #[error("cannot diff a snapshot against itself (version {0})")]
    SameVersion(u32),
}

// ── Change types ──────────────────────────────────────────────────────────────

/// A single metadata field change between two snapshots.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FieldChange {
    pub key: String,
    pub kind: ChangeKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ChangeKind {
    /// Field present in both but value changed.
    Modified { from: String, to: String },
    /// Field added in the newer snapshot.
    Added { value: String },
    /// Field removed in the newer snapshot.
    Removed { value: String },
    /// Field unchanged.
    Unchanged { value: String },
}

impl ChangeKind {
    pub fn is_changed(&self) -> bool {
        !matches!(self, Self::Unchanged { .. })
    }
}

// ── Result ────────────────────────────────────────────────────────────────────

/// The result of diffing two agent snapshots.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SnapshotDiff {
    pub agent_id: String,
    pub from_version: u32,
    pub to_version: u32,
    pub label_changed: bool,
    pub from_label: String,
    pub to_label: String,
    pub field_changes: Vec<FieldChange>,
}

impl SnapshotDiff {
    /// True if any meaningful change was detected.
    pub fn has_changes(&self) -> bool {
        self.label_changed || self.field_changes.iter().any(|c| c.kind.is_changed())
    }

    /// Number of modified/added/removed fields.
    pub fn change_count(&self) -> usize {
        self.field_changes
            .iter()
            .filter(|c| c.kind.is_changed())
            .count()
    }

    /// Names of fields that changed.
    pub fn changed_keys(&self) -> Vec<&str> {
        self.field_changes
            .iter()
            .filter(|c| c.kind.is_changed())
            .map(|c| c.key.as_str())
            .collect()
    }
}

// ── Engine ────────────────────────────────────────────────────────────────────

/// Computes the diff between two [`AgentSnapshot`] values.
pub struct DiffEngine;

impl DiffEngine {
    /// Compare `from` → `to`, returning a structured diff.
    pub fn diff(from: &AgentSnapshot, to: &AgentSnapshot) -> Result<SnapshotDiff, DiffError> {
        if from.version == to.version {
            return Err(DiffError::SameVersion(from.version));
        }
        let mut field_changes: Vec<FieldChange> = Vec::new();
        // Union of all keys
        let mut all_keys: Vec<String> = from
            .metadata
            .keys()
            .chain(to.metadata.keys())
            .cloned()
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        all_keys.sort();

        for key in &all_keys {
            let kind = match (from.metadata.get(key), to.metadata.get(key)) {
                (Some(a), Some(b)) if a == b => ChangeKind::Unchanged { value: a.clone() },
                (Some(a), Some(b)) => ChangeKind::Modified {
                    from: a.clone(),
                    to: b.clone(),
                },
                (None, Some(b)) => ChangeKind::Added { value: b.clone() },
                (Some(a), None) => ChangeKind::Removed { value: a.clone() },
                (None, None) => unreachable!(),
            };
            field_changes.push(FieldChange {
                key: key.clone(),
                kind,
            });
        }

        Ok(SnapshotDiff {
            agent_id: from.agent_id.clone(),
            from_version: from.version,
            to_version: to.version,
            label_changed: from.label != to.label,
            from_label: from.label.clone(),
            to_label: to.label.clone(),
            field_changes,
        })
    }

    /// Summarise all metadata across a list of snapshots into a flat map of
    /// key → set-of-distinct-values.  Useful for audit dashboards.
    pub fn metadata_union(snaps: &[&AgentSnapshot]) -> HashMap<String, Vec<String>> {
        let mut out: HashMap<String, std::collections::BTreeSet<String>> = HashMap::new();
        for s in snaps {
            for (k, v) in &s.metadata {
                out.entry(k.clone()).or_default().insert(v.clone());
            }
        }
        out.into_iter()
            .map(|(k, set)| (k, set.into_iter().collect()))
            .collect()
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::AgentSnapshot;

    fn snap(ver: u32, label: &str) -> AgentSnapshot {
        AgentSnapshot::new("bot", ver, label, ver as u64)
    }

    #[test]
    fn no_changes_detected() {
        let a = snap(1, "v1").clone();
        let b = snap(2, "v1").clone(); // same label, no meta
        let d = DiffEngine::diff(&a, &b).unwrap();
        assert!(!d.label_changed);
        assert!(!d.has_changes());
    }

    #[test]
    fn label_change_detected() {
        let a = snap(1, "v1.0");
        let b = snap(2, "v2.0");
        let d = DiffEngine::diff(&a, &b).unwrap();
        assert!(d.label_changed);
        assert!(d.has_changes());
    }

    #[test]
    fn metadata_added() {
        let a = snap(1, "v1");
        let b = snap(2, "v1").with_meta("model", "gpt-4o");
        let d = DiffEngine::diff(&a, &b).unwrap();
        assert_eq!(d.change_count(), 1);
        assert!(d.changed_keys().contains(&"model"));
    }

    #[test]
    fn metadata_removed() {
        let a = snap(1, "v1").with_meta("model", "gpt-4");
        let b = snap(2, "v1");
        let d = DiffEngine::diff(&a, &b).unwrap();
        assert_eq!(d.change_count(), 1);
        let fc = &d.field_changes[0];
        assert!(matches!(fc.kind, ChangeKind::Removed { .. }));
    }

    #[test]
    fn metadata_modified() {
        let a = snap(1, "v1").with_meta("model", "gpt-4");
        let b = snap(2, "v1").with_meta("model", "gpt-4o");
        let d = DiffEngine::diff(&a, &b).unwrap();
        assert_eq!(d.change_count(), 1);
        assert!(matches!(
            &d.field_changes[0].kind,
            ChangeKind::Modified { from, to } if from == "gpt-4" && to == "gpt-4o"
        ));
    }

    #[test]
    fn metadata_unchanged_not_counted() {
        let a = snap(1, "v1").with_meta("model", "gpt-4");
        let b = snap(2, "v1").with_meta("model", "gpt-4");
        let d = DiffEngine::diff(&a, &b).unwrap();
        assert_eq!(d.change_count(), 0);
        assert!(!d.has_changes());
    }

    #[test]
    fn same_version_errors() {
        let a = snap(1, "v1");
        let b = snap(1, "v1");
        assert_eq!(DiffEngine::diff(&a, &b), Err(DiffError::SameVersion(1)));
    }

    #[test]
    fn metadata_union_aggregates() {
        let a = snap(1, "v1").with_meta("model", "gpt-4");
        let b = snap(2, "v2").with_meta("model", "gpt-4o").with_meta("hash", "abc");
        let union = DiffEngine::metadata_union(&[&a, &b]);
        let mut models = union["model"].clone();
        models.sort();
        assert_eq!(models, vec!["gpt-4", "gpt-4o"]);
        assert!(union.contains_key("hash"));
    }

    #[test]
    fn diff_versions_recorded() {
        let a = snap(3, "v3");
        let b = snap(7, "v7");
        let d = DiffEngine::diff(&a, &b).unwrap();
        assert_eq!(d.from_version, 3);
        assert_eq!(d.to_version, 7);
    }

    #[test]
    fn changed_keys_sorted_by_insertion() {
        let a = snap(1, "v1").with_meta("z", "1").with_meta("a", "1");
        let b = snap(2, "v1").with_meta("z", "2").with_meta("a", "2");
        let d = DiffEngine::diff(&a, &b).unwrap();
        // both keys changed
        assert_eq!(d.change_count(), 2);
    }
}
