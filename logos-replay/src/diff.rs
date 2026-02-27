//! Version diff — compare states between two versions.
//!
//! `VersionDiff` compares two `serde_json::Value` snapshots and
//! produces a structured list of changes (added, removed, changed fields).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Kind of change between two versions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiffKind {
    /// A new field/element was added.
    Added,
    /// A field/element was removed.
    Removed,
    /// A field/element changed value.
    Changed,
}

/// A change to a specific field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldChange {
    /// The old value (None for Added).
    pub old: Option<Value>,
    /// The new value (None for Removed).
    pub new: Option<Value>,
}

/// A single diff entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiffEntry {
    /// JSON path of the changed element (e.g., "data.layers[0].name").
    pub path: String,
    /// Kind of change.
    pub kind: DiffKind,
    /// The old and new values.
    pub change: FieldChange,
}

/// A full diff between two document versions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VersionDiff {
    /// Source version.
    pub from_version: u64,
    /// Target version.
    pub to_version: u64,
    /// All detected changes.
    pub entries: Vec<DiffEntry>,
}

impl VersionDiff {
    /// Compute the diff between two JSON values.
    pub fn compute(from_version: u64, to_version: u64, old: &Value, new: &Value) -> Self {
        let mut entries = Vec::new();
        diff_values("", old, new, &mut entries);
        Self {
            from_version,
            to_version,
            entries,
        }
    }

    /// Number of changes.
    pub fn change_count(&self) -> usize {
        self.entries.len()
    }

    /// Whether there are no differences.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Filter entries by kind.
    pub fn filter_by_kind(&self, kind: DiffKind) -> Vec<&DiffEntry> {
        self.entries.iter().filter(|e| e.kind == kind).collect()
    }

    /// Filter entries by path prefix.
    pub fn filter_by_path(&self, prefix: &str) -> Vec<&DiffEntry> {
        self.entries
            .iter()
            .filter(|e| e.path.starts_with(prefix))
            .collect()
    }

    /// Count additions.
    pub fn additions(&self) -> usize {
        self.entries.iter().filter(|e| e.kind == DiffKind::Added).count()
    }

    /// Count removals.
    pub fn removals(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| e.kind == DiffKind::Removed)
            .count()
    }

    /// Count changes.
    pub fn changes(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| e.kind == DiffKind::Changed)
            .count()
    }

    /// All unique path prefixes at depth 1.
    pub fn affected_top_level_keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self
            .entries
            .iter()
            .filter_map(|e| {
                let trimmed = e.path.trim_start_matches('.');
                trimmed.split('.').next().map(|s| s.to_string())
            })
            .collect();
        keys.sort();
        keys.dedup();
        keys
    }
}

/// Recursively diff two JSON values.
fn diff_values(path: &str, old: &Value, new: &Value, entries: &mut Vec<DiffEntry>) {
    if old == new {
        return;
    }

    match (old, new) {
        (Value::Object(old_map), Value::Object(new_map)) => {
            // Check for removed and changed keys.
            for (key, old_val) in old_map {
                let child_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{}.{}", path, key)
                };

                match new_map.get(key) {
                    Some(new_val) => {
                        diff_values(&child_path, old_val, new_val, entries);
                    }
                    None => {
                        entries.push(DiffEntry {
                            path: child_path,
                            kind: DiffKind::Removed,
                            change: FieldChange {
                                old: Some(old_val.clone()),
                                new: None,
                            },
                        });
                    }
                }
            }
            // Check for added keys.
            for (key, new_val) in new_map {
                if !old_map.contains_key(key) {
                    let child_path = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{}.{}", path, key)
                    };
                    entries.push(DiffEntry {
                        path: child_path,
                        kind: DiffKind::Added,
                        change: FieldChange {
                            old: None,
                            new: Some(new_val.clone()),
                        },
                    });
                }
            }
        }
        (Value::Array(old_arr), Value::Array(new_arr)) => {
            let max_len = old_arr.len().max(new_arr.len());
            for i in 0..max_len {
                let child_path = if path.is_empty() {
                    format!("[{}]", i)
                } else {
                    format!("{}[{}]", path, i)
                };

                match (old_arr.get(i), new_arr.get(i)) {
                    (Some(ov), Some(nv)) => {
                        diff_values(&child_path, ov, nv, entries);
                    }
                    (Some(ov), None) => {
                        entries.push(DiffEntry {
                            path: child_path,
                            kind: DiffKind::Removed,
                            change: FieldChange {
                                old: Some(ov.clone()),
                                new: None,
                            },
                        });
                    }
                    (None, Some(nv)) => {
                        entries.push(DiffEntry {
                            path: child_path,
                            kind: DiffKind::Added,
                            change: FieldChange {
                                old: None,
                                new: Some(nv.clone()),
                            },
                        });
                    }
                    (None, None) => unreachable!(),
                }
            }
        }
        _ => {
            // Leaf-level change.
            entries.push(DiffEntry {
                path: path.to_string(),
                kind: DiffKind::Changed,
                change: FieldChange {
                    old: Some(old.clone()),
                    new: Some(new.clone()),
                },
            });
        }
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn diff_identical() {
        let a = json!({"x": 1, "y": 2});
        let diff = VersionDiff::compute(1, 2, &a, &a);
        assert!(diff.is_empty());
    }

    #[test]
    fn diff_added_field() {
        let a = json!({"x": 1});
        let b = json!({"x": 1, "y": 2});
        let diff = VersionDiff::compute(1, 2, &a, &b);
        assert_eq!(diff.additions(), 1);
        assert_eq!(diff.entries[0].path, "y");
        assert_eq!(diff.entries[0].kind, DiffKind::Added);
    }

    #[test]
    fn diff_removed_field() {
        let a = json!({"x": 1, "y": 2});
        let b = json!({"x": 1});
        let diff = VersionDiff::compute(1, 2, &a, &b);
        assert_eq!(diff.removals(), 1);
        assert_eq!(diff.entries[0].path, "y");
    }

    #[test]
    fn diff_changed_field() {
        let a = json!({"x": 1});
        let b = json!({"x": 99});
        let diff = VersionDiff::compute(1, 2, &a, &b);
        assert_eq!(diff.changes(), 1);
        assert_eq!(diff.entries[0].path, "x");
        assert_eq!(diff.entries[0].change.old, Some(json!(1)));
        assert_eq!(diff.entries[0].change.new, Some(json!(99)));
    }

    #[test]
    fn diff_nested_object() {
        let a = json!({"data": {"a": 1, "b": 2}});
        let b = json!({"data": {"a": 1, "b": 3, "c": 4}});
        let diff = VersionDiff::compute(1, 2, &a, &b);
        assert_eq!(diff.change_count(), 2);
        let paths: Vec<_> = diff.entries.iter().map(|e| e.path.as_str()).collect();
        assert!(paths.contains(&"data.b"));
        assert!(paths.contains(&"data.c"));
    }

    #[test]
    fn diff_array_added_element() {
        let a = json!([1, 2]);
        let b = json!([1, 2, 3]);
        let diff = VersionDiff::compute(1, 2, &a, &b);
        assert_eq!(diff.additions(), 1);
        assert_eq!(diff.entries[0].path, "[2]");
    }

    #[test]
    fn diff_array_removed_element() {
        let a = json!([1, 2, 3]);
        let b = json!([1, 2]);
        let diff = VersionDiff::compute(1, 2, &a, &b);
        assert_eq!(diff.removals(), 1);
        assert_eq!(diff.entries[0].path, "[2]");
    }

    #[test]
    fn diff_array_changed_element() {
        let a = json!([1, 2, 3]);
        let b = json!([1, 99, 3]);
        let diff = VersionDiff::compute(1, 2, &a, &b);
        assert_eq!(diff.changes(), 1);
        assert_eq!(diff.entries[0].path, "[1]");
    }

    #[test]
    fn diff_type_change() {
        let a = json!({"x": 1});
        let b = json!({"x": "hello"});
        let diff = VersionDiff::compute(1, 2, &a, &b);
        assert_eq!(diff.changes(), 1);
    }

    #[test]
    fn diff_null_handling() {
        let a = json!({"x": null});
        let b = json!({"x": 42});
        let diff = VersionDiff::compute(1, 2, &a, &b);
        assert_eq!(diff.changes(), 1);
        assert_eq!(diff.entries[0].change.old, Some(Value::Null));
    }

    #[test]
    fn diff_filter_by_kind() {
        let a = json!({"x": 1, "y": 2});
        let b = json!({"x": 99, "z": 3});
        let diff = VersionDiff::compute(1, 2, &a, &b);
        let added = diff.filter_by_kind(DiffKind::Added);
        let removed = diff.filter_by_kind(DiffKind::Removed);
        let changed = diff.filter_by_kind(DiffKind::Changed);
        assert_eq!(added.len(), 1);
        assert_eq!(removed.len(), 1);
        assert_eq!(changed.len(), 1);
    }

    #[test]
    fn diff_filter_by_path() {
        let a = json!({"data": {"a": 1}, "meta": {"b": 2}});
        let b = json!({"data": {"a": 99}, "meta": {"b": 2}});
        let diff = VersionDiff::compute(1, 2, &a, &b);
        let data_changes = diff.filter_by_path("data");
        let meta_changes = diff.filter_by_path("meta");
        assert_eq!(data_changes.len(), 1);
        assert_eq!(meta_changes.len(), 0);
    }

    #[test]
    fn diff_affected_top_level_keys() {
        let a = json!({"x": 1, "y": {"a": 1}});
        let b = json!({"x": 2, "y": {"a": 2}, "z": 3});
        let diff = VersionDiff::compute(1, 2, &a, &b);
        let keys = diff.affected_top_level_keys();
        assert!(keys.contains(&"x".to_string()));
        assert!(keys.contains(&"y".to_string()));
        assert!(keys.contains(&"z".to_string()));
    }

    #[test]
    fn diff_deeply_nested() {
        let a = json!({"l1": {"l2": {"l3": {"value": 1}}}});
        let b = json!({"l1": {"l2": {"l3": {"value": 2}}}});
        let diff = VersionDiff::compute(1, 2, &a, &b);
        assert_eq!(diff.change_count(), 1);
        assert_eq!(diff.entries[0].path, "l1.l2.l3.value");
    }

    #[test]
    fn diff_serde_roundtrip() {
        let a = json!({"x": 1});
        let b = json!({"x": 2, "y": 3});
        let diff = VersionDiff::compute(1, 2, &a, &b);
        let json_str = serde_json::to_string(&diff).unwrap();
        let back: VersionDiff = serde_json::from_str(&json_str).unwrap();
        assert_eq!(back.change_count(), diff.change_count());
    }

    #[test]
    fn diff_complex_mixed() {
        let a = json!({
            "layers": [
                {"name": "Background", "visible": true},
                {"name": "Layer 1", "visible": true}
            ],
            "width": 800,
            "height": 600
        });
        let b = json!({
            "layers": [
                {"name": "Background", "visible": false},
                {"name": "Layer 1", "visible": true},
                {"name": "Layer 2", "visible": true}
            ],
            "width": 1024,
            "height": 600
        });
        let diff = VersionDiff::compute(1, 2, &a, &b);

        // visible changed, width changed, Layer 2 added
        assert!(diff.change_count() >= 3);
    }
}
