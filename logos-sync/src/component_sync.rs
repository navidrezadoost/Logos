//! # Component Sync
//!
//! Change tracking and merging for component definitions.
//! Records edits as a log of typed changes so they can be
//! broadcast, replayed, and merged across collaborators.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use logos_components::{
    ComponentDefId, InstanceId, PropertyType, VariantKey, VariantValue,
};

// ── Identifiers ──────────────────────────────────────────────────────

/// Unique identifier for a component change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ComponentChangeId(pub Uuid);

impl ComponentChangeId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ComponentChangeId {
    fn default() -> Self {
        Self::new()
    }
}

// ── Property Diff ────────────────────────────────────────────────────

/// A diff describing a single property change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PropertyDiff {
    pub property_id: Uuid,
    pub property_name: String,
    pub old_value: Option<VariantValue>,
    pub new_value: Option<VariantValue>,
}

impl PropertyDiff {
    pub fn new(
        property_id: Uuid,
        name: impl Into<String>,
        old: Option<VariantValue>,
        new: Option<VariantValue>,
    ) -> Self {
        Self {
            property_id,
            property_name: name.into(),
            old_value: old,
            new_value: new,
        }
    }

    /// Is this an addition (no old value)?
    pub fn is_addition(&self) -> bool {
        self.old_value.is_none() && self.new_value.is_some()
    }

    /// Is this a removal (no new value)?
    pub fn is_removal(&self) -> bool {
        self.old_value.is_some() && self.new_value.is_none()
    }

    /// Is this a modification?
    pub fn is_modification(&self) -> bool {
        self.old_value.is_some() && self.new_value.is_some()
    }

    /// Create a reverse diff (for undo).
    pub fn reverse(&self) -> Self {
        Self {
            property_id: self.property_id,
            property_name: self.property_name.clone(),
            old_value: self.new_value.clone(),
            new_value: self.old_value.clone(),
        }
    }
}

// ── Change Types ─────────────────────────────────────────────────────

/// The type of change made to a component.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ComponentChangeType {
    /// Component created.
    Created {
        name: String,
        root_layer_id: Uuid,
    },
    /// Component renamed.
    Renamed {
        old_name: String,
        new_name: String,
    },
    /// Component deleted.
    Deleted {
        name: String,
    },
    /// Property added to component.
    PropertyAdded {
        property_id: Uuid,
        name: String,
        property_type: PropertyType,
        default_value: VariantValue,
    },
    /// Property removed from component.
    PropertyRemoved {
        property_id: Uuid,
        name: String,
    },
    /// Property value changed.
    PropertyChanged {
        diffs: Vec<PropertyDiff>,
    },
    /// Variant axis added.
    VariantAxisAdded {
        axis_name: String,
        values: Vec<String>,
    },
    /// Variant axis removed.
    VariantAxisRemoved {
        axis_name: String,
    },
    /// Variant axis value added.
    VariantAxisValueAdded {
        axis_name: String,
        value: String,
    },
    /// Instance created.
    InstanceCreated {
        instance_id: InstanceId,
        component_id: ComponentDefId,
        name: String,
    },
    /// Instance override changed.
    InstanceOverrideChanged {
        instance_id: InstanceId,
        diffs: Vec<PropertyDiff>,
    },
    /// Instance variant swapped.
    InstanceVariantSwapped {
        instance_id: InstanceId,
        old_key: VariantKey,
        new_key: VariantKey,
    },
    /// Instance deleted.
    InstanceDeleted {
        instance_id: InstanceId,
    },
    /// Component published/unpublished.
    PublishStateChanged {
        published: bool,
    },
    /// Component category changed.
    CategoryChanged {
        old_category: Option<String>,
        new_category: Option<String>,
    },
}

impl ComponentChangeType {
    /// Human-readable summary of this change.
    pub fn summary(&self) -> String {
        match self {
            Self::Created { name, .. } => format!("Created component '{}'", name),
            Self::Renamed { old_name, new_name } => {
                format!("Renamed '{}' to '{}'", old_name, new_name)
            }
            Self::Deleted { name } => format!("Deleted component '{}'", name),
            Self::PropertyAdded { name, .. } => format!("Added property '{}'", name),
            Self::PropertyRemoved { name, .. } => format!("Removed property '{}'", name),
            Self::PropertyChanged { diffs } => {
                format!("Changed {} properties", diffs.len())
            }
            Self::VariantAxisAdded { axis_name, values } => {
                format!("Added axis '{}' with {} values", axis_name, values.len())
            }
            Self::VariantAxisRemoved { axis_name } => {
                format!("Removed axis '{}'", axis_name)
            }
            Self::VariantAxisValueAdded { axis_name, value } => {
                format!("Added value '{}' to axis '{}'", value, axis_name)
            }
            Self::InstanceCreated { name, .. } => {
                format!("Created instance '{}'", name)
            }
            Self::InstanceOverrideChanged { diffs, .. } => {
                format!("Changed {} instance overrides", diffs.len())
            }
            Self::InstanceVariantSwapped { .. } => "Swapped instance variant".into(),
            Self::InstanceDeleted { .. } => "Deleted instance".into(),
            Self::PublishStateChanged { published } => {
                if *published {
                    "Published component".into()
                } else {
                    "Unpublished component".into()
                }
            }
            Self::CategoryChanged { new_category, .. } => {
                match new_category {
                    Some(c) => format!("Moved to category '{}'", c),
                    None => "Removed category".into(),
                }
            }
        }
    }
}

// ── Component Change ─────────────────────────────────────────────────

/// A single tracked change to a component definition or instance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentChange {
    pub id: ComponentChangeId,
    pub component_id: ComponentDefId,
    pub change_type: ComponentChangeType,
    pub user_id: Uuid,
    pub user_name: String,
    pub timestamp: u64,
    /// Logical clock for ordering.
    pub sequence: u64,
}

impl ComponentChange {
    pub fn new(
        component_id: ComponentDefId,
        change_type: ComponentChangeType,
        user_id: Uuid,
        user_name: impl Into<String>,
        timestamp: u64,
        sequence: u64,
    ) -> Self {
        Self {
            id: ComponentChangeId::new(),
            component_id,
            change_type,
            user_id,
            user_name: user_name.into(),
            timestamp,
            sequence,
        }
    }

    /// Human-readable summary.
    pub fn summary(&self) -> String {
        format!(
            "[{}] {}: {}",
            self.sequence,
            self.user_name,
            self.change_type.summary()
        )
    }
}

// ── Change Log ───────────────────────────────────────────────────────

/// A log of component changes for a document, supporting replay and
/// filtering by component, user, or time range.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComponentChangeLog {
    entries: Vec<ComponentChange>,
    next_sequence: u64,
}

impl ComponentChangeLog {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            next_sequence: 1,
        }
    }

    /// Record a new change.
    pub fn record(
        &mut self,
        component_id: ComponentDefId,
        change_type: ComponentChangeType,
        user_id: Uuid,
        user_name: impl Into<String>,
        timestamp: u64,
    ) -> ComponentChangeId {
        let change = ComponentChange::new(
            component_id,
            change_type,
            user_id,
            user_name,
            timestamp,
            self.next_sequence,
        );
        let id = change.id;
        self.entries.push(change);
        self.next_sequence += 1;
        id
    }

    /// Apply a remote change (already has a sequence number).
    pub fn apply_remote(&mut self, change: ComponentChange) {
        if change.sequence >= self.next_sequence {
            self.next_sequence = change.sequence + 1;
        }
        self.entries.push(change);
        // Keep sorted by sequence
        self.entries.sort_by_key(|c| c.sequence);
    }

    /// Get all changes.
    pub fn all(&self) -> &[ComponentChange] {
        &self.entries
    }

    /// Get changes since a given sequence number.
    pub fn since(&self, sequence: u64) -> Vec<&ComponentChange> {
        self.entries.iter().filter(|c| c.sequence > sequence).collect()
    }

    /// Get changes for a specific component.
    pub fn for_component(&self, id: ComponentDefId) -> Vec<&ComponentChange> {
        self.entries
            .iter()
            .filter(|c| c.component_id == id)
            .collect()
    }

    /// Get changes by a specific user.
    pub fn by_user(&self, user_id: Uuid) -> Vec<&ComponentChange> {
        self.entries.iter().filter(|c| c.user_id == user_id).collect()
    }

    /// Get changes in a time range (inclusive).
    pub fn in_time_range(&self, start: u64, end: u64) -> Vec<&ComponentChange> {
        self.entries
            .iter()
            .filter(|c| c.timestamp >= start && c.timestamp <= end)
            .collect()
    }

    /// Get the latest change for a component.
    pub fn latest_for_component(&self, id: ComponentDefId) -> Option<&ComponentChange> {
        self.entries
            .iter()
            .rev()
            .find(|c| c.component_id == id)
    }

    /// Get unique components that have been modified.
    pub fn modified_components(&self) -> Vec<ComponentDefId> {
        let mut seen = Vec::new();
        for entry in &self.entries {
            if !seen.contains(&entry.component_id) {
                seen.push(entry.component_id);
            }
        }
        seen
    }

    /// Get unique users who made changes.
    pub fn active_users(&self) -> Vec<Uuid> {
        let mut seen = Vec::new();
        for entry in &self.entries {
            if !seen.contains(&entry.user_id) {
                seen.push(entry.user_id);
            }
        }
        seen
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn next_sequence(&self) -> u64 {
        self.next_sequence
    }

    /// Clear all entries.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.next_sequence = 1;
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn alice() -> Uuid {
        Uuid::from_bytes([1; 16])
    }

    fn bob() -> Uuid {
        Uuid::from_bytes([2; 16])
    }

    fn comp_id() -> ComponentDefId {
        ComponentDefId(Uuid::from_bytes([10; 16]))
    }

    fn comp_id_2() -> ComponentDefId {
        ComponentDefId(Uuid::from_bytes([11; 16]))
    }

    #[test]
    fn test_property_diff_addition() {
        let diff = PropertyDiff::new(
            Uuid::new_v4(),
            "Label",
            None,
            Some(VariantValue::Text("Hello".into())),
        );
        assert!(diff.is_addition());
        assert!(!diff.is_removal());
        assert!(!diff.is_modification());
    }

    #[test]
    fn test_property_diff_removal() {
        let diff = PropertyDiff::new(
            Uuid::new_v4(),
            "Label",
            Some(VariantValue::Text("Hello".into())),
            None,
        );
        assert!(diff.is_removal());
    }

    #[test]
    fn test_property_diff_modification() {
        let diff = PropertyDiff::new(
            Uuid::new_v4(),
            "Color",
            Some(VariantValue::Number(0.0)),
            Some(VariantValue::Number(1.0)),
        );
        assert!(diff.is_modification());
    }

    #[test]
    fn test_property_diff_reverse() {
        let diff = PropertyDiff::new(
            Uuid::new_v4(),
            "X",
            Some(VariantValue::Number(1.0)),
            Some(VariantValue::Number(2.0)),
        );
        let rev = diff.reverse();
        assert_eq!(rev.old_value, Some(VariantValue::Number(2.0)));
        assert_eq!(rev.new_value, Some(VariantValue::Number(1.0)));
    }

    #[test]
    fn test_change_type_summary() {
        let t = ComponentChangeType::Created {
            name: "Button".into(),
            root_layer_id: Uuid::new_v4(),
        };
        assert!(t.summary().contains("Button"));

        let t = ComponentChangeType::Renamed {
            old_name: "Btn".into(),
            new_name: "Button".into(),
        };
        assert!(t.summary().contains("Btn"));

        let t = ComponentChangeType::PublishStateChanged { published: true };
        assert!(t.summary().contains("Published"));
    }

    #[test]
    fn test_change_log_record() {
        let mut log = ComponentChangeLog::new();
        log.record(
            comp_id(),
            ComponentChangeType::Created {
                name: "Button".into(),
                root_layer_id: Uuid::new_v4(),
            },
            alice(),
            "Alice",
            1000,
        );
        assert_eq!(log.len(), 1);
        assert_eq!(log.next_sequence(), 2);
    }

    #[test]
    fn test_change_log_sequence_order() {
        let mut log = ComponentChangeLog::new();
        log.record(
            comp_id(),
            ComponentChangeType::Created {
                name: "A".into(),
                root_layer_id: Uuid::new_v4(),
            },
            alice(),
            "Alice",
            1000,
        );
        log.record(
            comp_id(),
            ComponentChangeType::Renamed {
                old_name: "A".into(),
                new_name: "B".into(),
            },
            bob(),
            "Bob",
            1001,
        );
        assert_eq!(log.all()[0].sequence, 1);
        assert_eq!(log.all()[1].sequence, 2);
    }

    #[test]
    fn test_change_log_since() {
        let mut log = ComponentChangeLog::new();
        for i in 0..5 {
            log.record(
                comp_id(),
                ComponentChangeType::Renamed {
                    old_name: format!("v{}", i),
                    new_name: format!("v{}", i + 1),
                },
                alice(),
                "Alice",
                1000 + i,
            );
        }
        assert_eq!(log.since(3).len(), 2); // sequences 4 and 5
    }

    #[test]
    fn test_change_log_for_component() {
        let mut log = ComponentChangeLog::new();
        log.record(
            comp_id(),
            ComponentChangeType::Created {
                name: "A".into(),
                root_layer_id: Uuid::new_v4(),
            },
            alice(),
            "Alice",
            1000,
        );
        log.record(
            comp_id_2(),
            ComponentChangeType::Created {
                name: "B".into(),
                root_layer_id: Uuid::new_v4(),
            },
            alice(),
            "Alice",
            1001,
        );
        assert_eq!(log.for_component(comp_id()).len(), 1);
    }

    #[test]
    fn test_change_log_by_user() {
        let mut log = ComponentChangeLog::new();
        log.record(
            comp_id(),
            ComponentChangeType::Created {
                name: "A".into(),
                root_layer_id: Uuid::new_v4(),
            },
            alice(),
            "Alice",
            1000,
        );
        log.record(
            comp_id(),
            ComponentChangeType::Renamed {
                old_name: "A".into(),
                new_name: "B".into(),
            },
            bob(),
            "Bob",
            1001,
        );
        assert_eq!(log.by_user(alice()).len(), 1);
        assert_eq!(log.by_user(bob()).len(), 1);
    }

    #[test]
    fn test_change_log_time_range() {
        let mut log = ComponentChangeLog::new();
        for i in 0..10 {
            log.record(
                comp_id(),
                ComponentChangeType::Renamed {
                    old_name: "".into(),
                    new_name: format!("{}", i),
                },
                alice(),
                "Alice",
                1000 + i,
            );
        }
        assert_eq!(log.in_time_range(1003, 1006).len(), 4);
    }

    #[test]
    fn test_change_log_modified_components() {
        let mut log = ComponentChangeLog::new();
        log.record(
            comp_id(),
            ComponentChangeType::Created {
                name: "A".into(),
                root_layer_id: Uuid::new_v4(),
            },
            alice(),
            "Alice",
            1000,
        );
        log.record(
            comp_id_2(),
            ComponentChangeType::Created {
                name: "B".into(),
                root_layer_id: Uuid::new_v4(),
            },
            alice(),
            "Alice",
            1001,
        );
        log.record(
            comp_id(),
            ComponentChangeType::Renamed {
                old_name: "A".into(),
                new_name: "A2".into(),
            },
            alice(),
            "Alice",
            1002,
        );
        assert_eq!(log.modified_components().len(), 2);
    }

    #[test]
    fn test_change_log_active_users() {
        let mut log = ComponentChangeLog::new();
        log.record(
            comp_id(),
            ComponentChangeType::Created {
                name: "A".into(),
                root_layer_id: Uuid::new_v4(),
            },
            alice(),
            "Alice",
            1000,
        );
        log.record(
            comp_id(),
            ComponentChangeType::Renamed {
                old_name: "A".into(),
                new_name: "B".into(),
            },
            bob(),
            "Bob",
            1001,
        );
        assert_eq!(log.active_users().len(), 2);
    }

    #[test]
    fn test_change_log_apply_remote() {
        let mut log = ComponentChangeLog::new();
        log.record(
            comp_id(),
            ComponentChangeType::Created {
                name: "Local".into(),
                root_layer_id: Uuid::new_v4(),
            },
            alice(),
            "Alice",
            1000,
        );

        // Remote change with higher sequence
        let remote = ComponentChange::new(
            comp_id_2(),
            ComponentChangeType::Created {
                name: "Remote".into(),
                root_layer_id: Uuid::new_v4(),
            },
            bob(),
            "Bob",
            999,
            5,
        );
        log.apply_remote(remote);
        assert_eq!(log.len(), 2);
        assert_eq!(log.next_sequence(), 6); // advances past remote seq
        // Keeps sorted by sequence
        assert!(log.all()[0].sequence < log.all()[1].sequence);
    }

    #[test]
    fn test_change_log_clear() {
        let mut log = ComponentChangeLog::new();
        log.record(
            comp_id(),
            ComponentChangeType::Created {
                name: "A".into(),
                root_layer_id: Uuid::new_v4(),
            },
            alice(),
            "Alice",
            1000,
        );
        log.clear();
        assert!(log.is_empty());
        assert_eq!(log.next_sequence(), 1);
    }

    #[test]
    fn test_change_summary() {
        let mut log = ComponentChangeLog::new();
        log.record(
            comp_id(),
            ComponentChangeType::Created {
                name: "Button".into(),
                root_layer_id: Uuid::new_v4(),
            },
            alice(),
            "Alice",
            1000,
        );
        let summary = log.all()[0].summary();
        assert!(summary.contains("Alice"));
        assert!(summary.contains("Button"));
    }

    #[test]
    fn test_change_log_serde_roundtrip() {
        let mut log = ComponentChangeLog::new();
        log.record(
            comp_id(),
            ComponentChangeType::PropertyChanged {
                diffs: vec![PropertyDiff::new(
                    Uuid::new_v4(),
                    "Color",
                    Some(VariantValue::Number(0.0)),
                    Some(VariantValue::Number(1.0)),
                )],
            },
            alice(),
            "Alice",
            1000,
        );
        let json = serde_json::to_string(&log).unwrap();
        let back: ComponentChangeLog = serde_json::from_str(&json).unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back.next_sequence(), 2);
    }

    #[test]
    fn test_latest_for_component() {
        let mut log = ComponentChangeLog::new();
        log.record(
            comp_id(),
            ComponentChangeType::Created {
                name: "A".into(),
                root_layer_id: Uuid::new_v4(),
            },
            alice(),
            "Alice",
            1000,
        );
        log.record(
            comp_id(),
            ComponentChangeType::Renamed {
                old_name: "A".into(),
                new_name: "B".into(),
            },
            bob(),
            "Bob",
            1001,
        );
        let latest = log.latest_for_component(comp_id()).unwrap();
        assert_eq!(latest.user_name, "Bob");
    }
}
