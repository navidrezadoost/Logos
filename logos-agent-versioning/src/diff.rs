//! Version diff — compare two `AgentSnapshot` configs and surface changes.
//!
//! `VersionDiff::compute` walks the config keys of two snapshots and
//! classifies each as Added, Removed, or Modified. Unchanged keys are omitted.

use crate::version::AgentSnapshot;
use serde::{Deserialize, Serialize};

// ── Change kind ───────────────────────────────────────────────────────────────

/// The kind of change for a single config key between two snapshots.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ChangeKind {
    /// Key exists in `to` but not in `from`.
    Added { value: serde_json::Value },
    /// Key exists in `from` but not in `to`.
    Removed { old_value: serde_json::Value },
    /// Key exists in both but its value changed.
    Modified { old_value: serde_json::Value, new_value: serde_json::Value },
}

impl ChangeKind {
    pub fn label(&self) -> &str {
        match self {
            Self::Added    { .. } => "added",
            Self::Removed  { .. } => "removed",
            Self::Modified { .. } => "modified",
        }
    }
}

// ── Diff entry ────────────────────────────────────────────────────────────────

/// A single changed key and its `ChangeKind`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffEntry {
    pub key: String,
    pub change: ChangeKind,
}

// ── Version diff ──────────────────────────────────────────────────────────────

/// Complete diff between two agent configuration snapshots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionDiff {
    pub agent_id: String,
    pub from_version: String,
    pub to_version: String,
    pub entries: Vec<DiffEntry>,
}

impl VersionDiff {
    /// Compute the diff between `from` and `to`.
    ///
    /// Both snapshots must belong to the same agent; if they differ the
    /// function still runs but the `agent_id` field reflects `from`'s ID.
    pub fn compute(from: &AgentSnapshot, to: &AgentSnapshot) -> Self {
        let mut entries = Vec::new();

        // Keys in `from`
        for (key, old_val) in &from.config {
            if let Some(new_val) = to.config.get(key) {
                if old_val != new_val {
                    entries.push(DiffEntry {
                        key: key.clone(),
                        change: ChangeKind::Modified {
                            old_value: old_val.clone(),
                            new_value: new_val.clone(),
                        },
                    });
                }
                // Else: unchanged, omit.
            } else {
                entries.push(DiffEntry {
                    key: key.clone(),
                    change: ChangeKind::Removed { old_value: old_val.clone() },
                });
            }
        }

        // Keys only in `to`
        for (key, new_val) in &to.config {
            if !from.config.contains_key(key) {
                entries.push(DiffEntry {
                    key: key.clone(),
                    change: ChangeKind::Added { value: new_val.clone() },
                });
            }
        }

        // Sort for deterministic output.
        entries.sort_by(|a, b| a.key.cmp(&b.key));

        Self {
            agent_id: from.agent_id().to_string(),
            from_version: from.version().to_string(),
            to_version: to.version().to_string(),
            entries,
        }
    }

    pub fn is_empty(&self) -> bool { self.entries.is_empty() }

    pub fn len(&self) -> usize { self.entries.len() }

    /// Keys that were added in `to`.
    pub fn added_keys(&self) -> Vec<&str> {
        self.entries
            .iter()
            .filter(|e| matches!(e.change, ChangeKind::Added { .. }))
            .map(|e| e.key.as_str())
            .collect()
    }

    /// Keys that were removed in `to`.
    pub fn removed_keys(&self) -> Vec<&str> {
        self.entries
            .iter()
            .filter(|e| matches!(e.change, ChangeKind::Removed { .. }))
            .map(|e| e.key.as_str())
            .collect()
    }

    /// Keys whose value changed between versions.
    pub fn modified_keys(&self) -> Vec<&str> {
        self.entries
            .iter()
            .filter(|e| matches!(e.change, ChangeKind::Modified { .. }))
            .map(|e| e.key.as_str())
            .collect()
    }

    /// Human-readable one-line summary.
    pub fn summary(&self) -> String {
        format!(
            "{} → {}: +{} ~{} -{}",
            self.from_version,
            self.to_version,
            self.added_keys().len(),
            self.modified_keys().len(),
            self.removed_keys().len(),
        )
    }

    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::version::{AgentSnapshot, SemVer};

    fn base() -> AgentSnapshot {
        AgentSnapshot::builder("ag", SemVer::new(1, 0, 0))
            .config_str("model", "gpt-4")
            .config_str("system_prompt", "You are helpful")
            .config_bool("debug", false)
            .build()
    }

    // ── empty diff ────────────────────────────────────────────────────────────

    #[test]
    fn diff_identical_snapshots_is_empty() {
        let a = base();
        let b = AgentSnapshot::builder("ag", SemVer::new(1, 0, 1))
            .config_str("model", "gpt-4")
            .config_str("system_prompt", "You are helpful")
            .config_bool("debug", false)
            .build();
        let diff = VersionDiff::compute(&a, &b);
        assert!(diff.is_empty());
    }

    // ── added keys ────────────────────────────────────────────────────────────

    #[test]
    fn diff_added_keys() {
        let a = base();
        let b = AgentSnapshot::builder("ag", SemVer::new(1, 1, 0))
            .config_str("model", "gpt-4")
            .config_str("system_prompt", "You are helpful")
            .config_bool("debug", false)
            .config_str("temperature", "0.7") // NEW
            .build();
        let diff = VersionDiff::compute(&a, &b);
        assert!(diff.added_keys().contains(&"temperature"));
        assert_eq!(diff.removed_keys().len(), 0);
    }

    // ── removed keys ─────────────────────────────────────────────────────────

    #[test]
    fn diff_removed_keys() {
        let a = base();
        let b = AgentSnapshot::builder("ag", SemVer::new(1, 1, 0))
            .config_str("model", "gpt-4")
            // "system_prompt" and "debug" removed
            .build();
        let diff = VersionDiff::compute(&a, &b);
        assert!(diff.removed_keys().contains(&"system_prompt"));
        assert!(diff.removed_keys().contains(&"debug"));
        assert_eq!(diff.added_keys().len(), 0);
    }

    // ── modified keys ─────────────────────────────────────────────────────────

    #[test]
    fn diff_modified_keys() {
        let a = base();
        let b = AgentSnapshot::builder("ag", SemVer::new(1, 1, 0))
            .config_str("model", "gpt-4o")          // CHANGED
            .config_str("system_prompt", "You are helpful")
            .config_bool("debug", true)              // CHANGED
            .build();
        let diff = VersionDiff::compute(&a, &b);
        let modified = diff.modified_keys();
        assert!(modified.contains(&"model"));
        assert!(modified.contains(&"debug"));
    }

    // ── summary ───────────────────────────────────────────────────────────────

    #[test]
    fn diff_summary_format() {
        let a = base();
        let b = AgentSnapshot::builder("ag", SemVer::new(2, 0, 0))
            .config_str("model", "gpt-4o")
            .config_str("new_key", "value")
            .build();
        let diff = VersionDiff::compute(&a, &b);
        let s = diff.summary();
        assert!(s.starts_with("1.0.0 → 2.0.0:"));
        assert!(s.contains('+'));
    }
}
