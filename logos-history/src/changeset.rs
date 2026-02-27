//! Changeset — human-readable change descriptions from raw diffs.
//!
//! Transforms low-level `VersionDiff` / `DiffEntry` data into
//! descriptions a user can understand: "Added layer 'Background'",
//! "Changed fill color from blue to red", etc.

use logos_replay::{DiffEntry, DiffKind, VersionDiff};
use serde::{Deserialize, Serialize};

/// Category of a change (for UI grouping / icons).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChangeCategory {
    /// Structural changes (add/remove layers, frames, pages).
    Structural,
    /// Content changes (text edits, image swaps).
    Content,
    /// Style changes (fill, stroke, opacity, effects).
    Style,
    /// Layout changes (position, size, rotation).
    Layout,
    /// Comment-related changes.
    Comment,
    /// Metadata changes (name, tags, settings).
    Meta,
    /// Unknown / uncategorisable changes.
    Other,
}

impl std::fmt::Display for ChangeCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Structural => write!(f, "Structural"),
            Self::Content => write!(f, "Content"),
            Self::Style => write!(f, "Style"),
            Self::Layout => write!(f, "Layout"),
            Self::Comment => write!(f, "Comment"),
            Self::Meta => write!(f, "Meta"),
            Self::Other => write!(f, "Other"),
        }
    }
}

/// A human-readable description of a single change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChangeDescription {
    /// Human-readable summary.
    pub text: String,
    /// Category for UI grouping.
    pub category: ChangeCategory,
    /// JSON path of the underlying change.
    pub path: String,
    /// Whether this is a notable change (worth highlighting).
    pub notable: bool,
}

/// A changeset — a complete set of human-readable changes between two versions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Changeset {
    /// Source version.
    pub from_version: u64,
    /// Target version.
    pub to_version: u64,
    /// Human-readable descriptions of all changes.
    pub descriptions: Vec<ChangeDescription>,
}

impl Changeset {
    /// Build a changeset from a version diff.
    pub fn from_diff(diff: &VersionDiff) -> Self {
        let descriptions = diff
            .entries
            .iter()
            .map(|entry| describe_change(entry))
            .collect();

        Self {
            from_version: diff.from_version,
            to_version: diff.to_version,
            descriptions,
        }
    }

    /// Number of changes.
    pub fn len(&self) -> usize {
        self.descriptions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.descriptions.is_empty()
    }

    /// Filter by category.
    pub fn by_category(&self, category: ChangeCategory) -> Vec<&ChangeDescription> {
        self.descriptions
            .iter()
            .filter(|d| d.category == category)
            .collect()
    }

    /// Only notable changes.
    pub fn notable(&self) -> Vec<&ChangeDescription> {
        self.descriptions.iter().filter(|d| d.notable).collect()
    }

    /// Unique categories present in this changeset.
    pub fn categories(&self) -> Vec<ChangeCategory> {
        let mut cats: Vec<ChangeCategory> = self
            .descriptions
            .iter()
            .map(|d| d.category)
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        cats.sort_by_key(|c| format!("{:?}", c));
        cats
    }

    /// Generate a one-line summary of this changeset.
    pub fn summary(&self) -> String {
        if self.descriptions.is_empty() {
            return "No changes".to_string();
        }
        if self.descriptions.len() == 1 {
            return self.descriptions[0].text.clone();
        }
        let cats = self.categories();
        if cats.len() == 1 {
            format!("{} {} changes", self.descriptions.len(), cats[0])
        } else {
            format!(
                "{} changes across {} categories",
                self.descriptions.len(),
                cats.len()
            )
        }
    }
}

/// Categorise a change based on its JSON path.
fn categorise(path: &str) -> ChangeCategory {
    let lower = path.to_lowercase();
    if lower.contains("layer") || lower.contains("frame") || lower.contains("page") {
        ChangeCategory::Structural
    } else if lower.contains("text") || lower.contains("content") || lower.contains("image") {
        ChangeCategory::Content
    } else if lower.contains("fill")
        || lower.contains("stroke")
        || lower.contains("opacity")
        || lower.contains("color")
        || lower.contains("effect")
        || lower.contains("shadow")
        || lower.contains("blur")
    {
        ChangeCategory::Style
    } else if lower.contains("position")
        || lower.contains("size")
        || lower.contains("width")
        || lower.contains("height")
        || lower.contains("rotation")
        || lower.contains("transform")
        || lower.contains("x")
        || lower.contains("y")
    {
        ChangeCategory::Layout
    } else if lower.contains("comment") || lower.contains("annotation") {
        ChangeCategory::Comment
    } else if lower.contains("name")
        || lower.contains("tag")
        || lower.contains("setting")
        || lower.contains("meta")
    {
        ChangeCategory::Meta
    } else {
        ChangeCategory::Other
    }
}

/// Whether a change is notable enough to highlight.
fn is_notable(path: &str, kind: &DiffKind) -> bool {
    // Structural adds/removes are always notable.
    if matches!(kind, DiffKind::Added | DiffKind::Removed) {
        let lower = path.to_lowercase();
        if lower.contains("layer") || lower.contains("frame") || lower.contains("page") {
            return true;
        }
    }
    false
}

/// Generate a human-readable description of a single diff entry.
fn describe_change(entry: &DiffEntry) -> ChangeDescription {
    let last_segment = entry
        .path
        .rsplit('.')
        .next()
        .unwrap_or(&entry.path);

    let text = match entry.kind {
        DiffKind::Added => {
            if let Some(ref val) = entry.change.new {
                format!("Added {} ({})", last_segment, value_preview(val))
            } else {
                format!("Added {}", last_segment)
            }
        }
        DiffKind::Removed => format!("Removed {}", last_segment),
        DiffKind::Changed => {
            let old_preview = entry
                .change
                .old
                .as_ref()
                .map(|v| value_preview(v))
                .unwrap_or_default();
            let new_preview = entry
                .change
                .new
                .as_ref()
                .map(|v| value_preview(v))
                .unwrap_or_default();
            if !old_preview.is_empty() && !new_preview.is_empty() {
                format!(
                    "Changed {} from {} to {}",
                    last_segment, old_preview, new_preview
                )
            } else {
                format!("Changed {}", last_segment)
            }
        }
    };

    ChangeDescription {
        text,
        category: categorise(&entry.path),
        path: entry.path.clone(),
        notable: is_notable(&entry.path, &entry.kind),
    }
}

/// Compact preview of a JSON value for display.
fn value_preview(val: &serde_json::Value) -> String {
    match val {
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => {
            if s.len() > 30 {
                format!("\"{}...\"", &s[..27])
            } else {
                format!("\"{}\"", s)
            }
        }
        serde_json::Value::Array(arr) => format!("[{} items]", arr.len()),
        serde_json::Value::Object(obj) => format!("{{{} fields}}", obj.len()),
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use logos_replay::FieldChange;
    use serde_json::json;

    fn make_diff(entries: Vec<DiffEntry>) -> VersionDiff {
        VersionDiff {
            from_version: 1,
            to_version: 2,
            entries,
        }
    }

    fn added_entry(path: &str, val: serde_json::Value) -> DiffEntry {
        DiffEntry {
            path: path.to_string(),
            kind: DiffKind::Added,
            change: FieldChange {
                old: None,
                new: Some(val),
            },
        }
    }

    fn removed_entry(path: &str) -> DiffEntry {
        DiffEntry {
            path: path.to_string(),
            kind: DiffKind::Removed,
            change: FieldChange {
                old: Some(json!("old")),
                new: None,
            },
        }
    }

    fn changed_entry(path: &str, old: serde_json::Value, new: serde_json::Value) -> DiffEntry {
        DiffEntry {
            path: path.to_string(),
            kind: DiffKind::Changed,
            change: FieldChange {
                old: Some(old),
                new: Some(new),
            },
        }
    }

    #[test]
    fn changeset_from_diff() {
        let diff = make_diff(vec![
            added_entry("data.layers[0]", json!({"name": "Background"})),
            changed_entry("data.fill_color", json!("blue"), json!("red")),
        ]);
        let cs = Changeset::from_diff(&diff);
        assert_eq!(cs.len(), 2);
        assert_eq!(cs.from_version, 1);
        assert_eq!(cs.to_version, 2);
    }

    #[test]
    fn changeset_empty() {
        let diff = make_diff(vec![]);
        let cs = Changeset::from_diff(&diff);
        assert!(cs.is_empty());
        assert_eq!(cs.summary(), "No changes");
    }

    #[test]
    fn category_structural() {
        assert_eq!(categorise("data.layers[0].name"), ChangeCategory::Structural);
        assert_eq!(categorise("frames[2]"), ChangeCategory::Structural);
    }

    #[test]
    fn category_style() {
        assert_eq!(categorise("data.fill_color"), ChangeCategory::Style);
        assert_eq!(categorise("stroke_width"), ChangeCategory::Style);
        assert_eq!(categorise("opacity"), ChangeCategory::Style);
    }

    #[test]
    fn category_layout() {
        assert_eq!(categorise("data.position.x"), ChangeCategory::Layout);
        assert_eq!(categorise("data.size.width"), ChangeCategory::Layout);
    }

    #[test]
    fn category_content() {
        assert_eq!(categorise("data.text_content"), ChangeCategory::Content);
    }

    #[test]
    fn category_meta() {
        assert_eq!(categorise("settings.name"), ChangeCategory::Meta);
    }

    #[test]
    fn describe_added() {
        let entry = added_entry("data.layers[0]", json!({"name": "BG"}));
        let desc = describe_change(&entry);
        assert!(desc.text.contains("Added"));
        assert!(desc.text.contains("layers[0]"));
        assert!(desc.notable); // structural add
    }

    #[test]
    fn describe_removed() {
        let entry = removed_entry("data.layers[1]");
        let desc = describe_change(&entry);
        assert!(desc.text.contains("Removed"));
        assert!(desc.notable); // structural remove
    }

    #[test]
    fn describe_changed() {
        let entry = changed_entry("data.fill_color", json!("blue"), json!("red"));
        let desc = describe_change(&entry);
        assert!(desc.text.contains("Changed"));
        assert!(desc.text.contains("\"blue\""));
        assert!(desc.text.contains("\"red\""));
    }

    #[test]
    fn value_preview_string_truncation() {
        let long = "a".repeat(50);
        let preview = value_preview(&json!(long));
        assert!(preview.len() < 40);
        assert!(preview.ends_with("...\""));
    }

    #[test]
    fn value_preview_array() {
        let preview = value_preview(&json!([1, 2, 3]));
        assert_eq!(preview, "[3 items]");
    }

    #[test]
    fn value_preview_object() {
        let preview = value_preview(&json!({"a": 1, "b": 2}));
        assert_eq!(preview, "{2 fields}");
    }

    #[test]
    fn changeset_by_category() {
        let diff = make_diff(vec![
            added_entry("data.layers[0]", json!("x")),
            changed_entry("data.fill_color", json!("a"), json!("b")),
            changed_entry("data.stroke_width", json!(1), json!(2)),
        ]);
        let cs = Changeset::from_diff(&diff);
        assert_eq!(cs.by_category(ChangeCategory::Style).len(), 2);
        assert_eq!(cs.by_category(ChangeCategory::Structural).len(), 1);
    }

    #[test]
    fn changeset_notable() {
        let diff = make_diff(vec![
            added_entry("data.layers[0]", json!("x")),         // notable
            changed_entry("data.fill_color", json!(1), json!(2)), // not notable
        ]);
        let cs = Changeset::from_diff(&diff);
        let notable = cs.notable();
        assert_eq!(notable.len(), 1);
        assert!(notable[0].text.contains("Added"));
    }

    #[test]
    fn changeset_categories() {
        let diff = make_diff(vec![
            added_entry("data.layers[0]", json!("x")),
            changed_entry("data.fill_color", json!(1), json!(2)),
        ]);
        let cs = Changeset::from_diff(&diff);
        let cats = cs.categories();
        assert_eq!(cats.len(), 2);
    }

    #[test]
    fn changeset_summary_single() {
        let diff = make_diff(vec![added_entry("data.layers[0]", json!("x"))]);
        let cs = Changeset::from_diff(&diff);
        assert!(cs.summary().contains("Added"));
    }

    #[test]
    fn changeset_summary_multi_same_category() {
        let diff = make_diff(vec![
            changed_entry("data.fill_color", json!(1), json!(2)),
            changed_entry("data.stroke_color", json!(1), json!(2)),
        ]);
        let cs = Changeset::from_diff(&diff);
        assert!(cs.summary().contains("2 Style changes"));
    }

    #[test]
    fn changeset_serde_roundtrip() {
        let diff = make_diff(vec![added_entry("data.layers[0]", json!("x"))]);
        let cs = Changeset::from_diff(&diff);
        let json = serde_json::to_string(&cs).unwrap();
        let back: Changeset = serde_json::from_str(&json).unwrap();
        assert_eq!(back.len(), 1);
    }
}
