//! # Conflict Resolution
//!
//! Detection and resolution strategies for concurrent edits to the
//! same component, instance, or property by multiple users.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use logos_components::ComponentDefId;

// ── Edit Operation ───────────────────────────────────────────────────

/// A single edit operation by a user.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditOperation {
    pub id: Uuid,
    pub user_id: Uuid,
    pub user_name: String,
    pub target_component: ComponentDefId,
    /// The property or field being edited.
    pub field_path: String,
    /// The value before the edit (for conflict detection).
    pub base_value: serde_json::Value,
    /// The new value.
    pub new_value: serde_json::Value,
    pub timestamp: u64,
    /// Logical clock for ordering.
    pub sequence: u64,
}

impl EditOperation {
    pub fn new(
        user_id: Uuid,
        user_name: impl Into<String>,
        target_component: ComponentDefId,
        field_path: impl Into<String>,
        base_value: serde_json::Value,
        new_value: serde_json::Value,
        timestamp: u64,
        sequence: u64,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            user_id,
            user_name: user_name.into(),
            target_component,
            field_path: field_path.into(),
            base_value,
            new_value,
            timestamp,
            sequence,
        }
    }

    /// Whether two operations target the same field from different users.
    pub fn conflicts_with(&self, other: &EditOperation) -> bool {
        self.target_component == other.target_component
            && self.field_path == other.field_path
            && self.id != other.id
            && self.user_id != other.user_id
    }
}

// ── Edit Conflict ────────────────────────────────────────────────────

/// A detected conflict between two concurrent edits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditConflict {
    pub id: Uuid,
    pub component_id: ComponentDefId,
    pub field_path: String,
    pub local_op: EditOperation,
    pub remote_op: EditOperation,
    pub resolution: Option<ConflictResolution>,
    pub detected_at: u64,
}

impl EditConflict {
    pub fn new(local: EditOperation, remote: EditOperation, detected_at: u64) -> Self {
        Self {
            id: Uuid::new_v4(),
            component_id: local.target_component,
            field_path: local.field_path.clone(),
            local_op: local,
            remote_op: remote,
            resolution: None,
            detected_at,
        }
    }

    /// Resolve this conflict.
    pub fn resolve(&mut self, resolution: ConflictResolution) {
        self.resolution = Some(resolution);
    }

    pub fn is_resolved(&self) -> bool {
        self.resolution.is_some()
    }

    /// Get the winning value based on the resolution.
    pub fn winning_value(&self) -> Option<&serde_json::Value> {
        match &self.resolution {
            Some(ConflictResolution::AcceptLocal) => Some(&self.local_op.new_value),
            Some(ConflictResolution::AcceptRemote) => Some(&self.remote_op.new_value),
            Some(ConflictResolution::Merge { merged_value }) => Some(merged_value),
            Some(ConflictResolution::Custom { value }) => Some(value),
            None => None,
        }
    }

    /// Human-readable summary.
    pub fn summary(&self) -> String {
        format!(
            "Conflict on '{}': {} vs {}",
            self.field_path, self.local_op.user_name, self.remote_op.user_name
        )
    }
}

// ── Resolution Strategy ──────────────────────────────────────────────

/// How a conflict should be resolved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ConflictResolution {
    /// Use the local edit.
    AcceptLocal,
    /// Use the remote edit.
    AcceptRemote,
    /// Merge both edits into a combined value.
    Merge { merged_value: serde_json::Value },
    /// Use a custom value.
    Custom { value: serde_json::Value },
}

/// Strategy for automatic conflict resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConflictStrategy {
    /// Last write wins (by timestamp).
    LastWriteWins,
    /// First write wins (by timestamp).
    FirstWriteWins,
    /// Higher sequence number wins.
    HighestSequenceWins,
    /// Always prefer local edits.
    LocalFirst,
    /// Always prefer remote edits.
    RemoteFirst,
    /// Never auto-resolve — always prompt the user.
    Manual,
}

impl Default for ConflictStrategy {
    fn default() -> Self {
        Self::LastWriteWins
    }
}

impl ConflictStrategy {
    /// Auto-resolve a conflict using this strategy.
    pub fn resolve(&self, conflict: &EditConflict) -> Option<ConflictResolution> {
        match self {
            Self::LastWriteWins => {
                if conflict.local_op.timestamp >= conflict.remote_op.timestamp {
                    Some(ConflictResolution::AcceptLocal)
                } else {
                    Some(ConflictResolution::AcceptRemote)
                }
            }
            Self::FirstWriteWins => {
                if conflict.local_op.timestamp <= conflict.remote_op.timestamp {
                    Some(ConflictResolution::AcceptLocal)
                } else {
                    Some(ConflictResolution::AcceptRemote)
                }
            }
            Self::HighestSequenceWins => {
                if conflict.local_op.sequence >= conflict.remote_op.sequence {
                    Some(ConflictResolution::AcceptLocal)
                } else {
                    Some(ConflictResolution::AcceptRemote)
                }
            }
            Self::LocalFirst => Some(ConflictResolution::AcceptLocal),
            Self::RemoteFirst => Some(ConflictResolution::AcceptRemote),
            Self::Manual => None,
        }
    }
}

// ── Conflict Detector ────────────────────────────────────────────────

/// Detects and manages conflicts between concurrent edits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictDetector {
    /// Pending operations per component per field.
    pending: HashMap<ComponentDefId, HashMap<String, Vec<EditOperation>>>,
    /// Detected conflicts.
    conflicts: Vec<EditConflict>,
    /// Auto-resolution strategy.
    strategy: ConflictStrategy,
    /// How many conflicts were auto-resolved.
    auto_resolved: usize,
}

impl ConflictDetector {
    pub fn new(strategy: ConflictStrategy) -> Self {
        Self {
            pending: HashMap::new(),
            conflicts: Vec::new(),
            strategy,
            auto_resolved: 0,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(ConflictStrategy::LastWriteWins)
    }

    /// Submit a local edit. Returns any conflicts detected.
    pub fn submit_local(&mut self, op: EditOperation, now: u64) -> Vec<EditConflict> {
        self.submit_op(op, true, now)
    }

    /// Submit a remote edit. Returns any conflicts detected.
    pub fn submit_remote(&mut self, op: EditOperation, now: u64) -> Vec<EditConflict> {
        self.submit_op(op, false, now)
    }

    fn submit_op(
        &mut self,
        op: EditOperation,
        is_local: bool,
        now: u64,
    ) -> Vec<EditConflict> {
        let comp = op.target_component;
        let field = op.field_path.clone();
        let mut new_conflicts = Vec::new();

        // Check pending ops for the same field
        let fields = self.pending.entry(comp).or_default();
        let pending_ops = fields.entry(field).or_default();

        for existing in pending_ops.iter() {
            if existing.user_id != op.user_id && existing.conflicts_with(&op) {
                let conflict = if is_local {
                    EditConflict::new(op.clone(), existing.clone(), now)
                } else {
                    EditConflict::new(existing.clone(), op.clone(), now)
                };
                new_conflicts.push(conflict);
            }
        }

        pending_ops.push(op);

        // Auto-resolve if strategy permits
        for conflict in &mut new_conflicts {
            if let Some(resolution) = self.strategy.resolve(conflict) {
                conflict.resolve(resolution);
                self.auto_resolved += 1;
            }
        }

        self.conflicts.extend(new_conflicts.clone());
        new_conflicts
    }

    /// Acknowledge an operation (remove from pending).
    pub fn acknowledge(&mut self, component_id: ComponentDefId, field_path: &str, op_id: Uuid) {
        if let Some(fields) = self.pending.get_mut(&component_id) {
            if let Some(ops) = fields.get_mut(field_path) {
                ops.retain(|o| o.id != op_id);
                if ops.is_empty() {
                    fields.remove(field_path);
                }
            }
            if fields.is_empty() {
                self.pending.remove(&component_id);
            }
        }
    }

    /// Get all unresolved conflicts.
    pub fn unresolved(&self) -> Vec<&EditConflict> {
        self.conflicts.iter().filter(|c| !c.is_resolved()).collect()
    }

    /// Get all conflicts.
    pub fn all_conflicts(&self) -> &[EditConflict] {
        &self.conflicts
    }

    /// Resolve a specific conflict.
    pub fn resolve_conflict(
        &mut self,
        conflict_id: Uuid,
        resolution: ConflictResolution,
    ) -> bool {
        if let Some(c) = self.conflicts.iter_mut().find(|c| c.id == conflict_id) {
            c.resolve(resolution);
            true
        } else {
            false
        }
    }

    /// Clear resolved conflicts.
    pub fn clear_resolved(&mut self) {
        self.conflicts.retain(|c| !c.is_resolved());
    }

    pub fn conflict_count(&self) -> usize {
        self.conflicts.len()
    }

    pub fn unresolved_count(&self) -> usize {
        self.conflicts.iter().filter(|c| !c.is_resolved()).count()
    }

    pub fn auto_resolved_count(&self) -> usize {
        self.auto_resolved
    }

    pub fn pending_op_count(&self) -> usize {
        self.pending
            .values()
            .flat_map(|f| f.values())
            .map(|ops| ops.len())
            .sum()
    }

    pub fn strategy(&self) -> ConflictStrategy {
        self.strategy
    }

    pub fn set_strategy(&mut self, strategy: ConflictStrategy) {
        self.strategy = strategy;
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

    fn make_op(user_id: Uuid, name: &str, field: &str, value: i32, ts: u64, seq: u64) -> EditOperation {
        EditOperation::new(
            user_id,
            name,
            comp_id(),
            field,
            serde_json::json!(0),
            serde_json::json!(value),
            ts,
            seq,
        )
    }

    #[test]
    fn test_edit_operation_conflicts_with() {
        let op1 = make_op(alice(), "Alice", "fill.color", 1, 1000, 1);
        let op2 = make_op(bob(), "Bob", "fill.color", 2, 1001, 2);
        assert!(op1.conflicts_with(&op2));
    }

    #[test]
    fn test_edit_operation_no_conflict_different_field() {
        let op1 = make_op(alice(), "Alice", "fill.color", 1, 1000, 1);
        let op2 = make_op(bob(), "Bob", "border.width", 2, 1001, 2);
        assert!(!op1.conflicts_with(&op2));
    }

    #[test]
    fn test_edit_operation_no_conflict_same_user() {
        let op1 = make_op(alice(), "Alice", "fill.color", 1, 1000, 1);
        let op2 = make_op(alice(), "Alice", "fill.color", 2, 1001, 2);
        // Same user — not a conflict (sequential edits)
        assert!(!op1.conflicts_with(&op2));
    }

    #[test]
    fn test_conflict_detection() {
        let mut detector = ConflictDetector::new(ConflictStrategy::Manual);
        let op1 = make_op(alice(), "Alice", "fill.color", 1, 1000, 1);
        let op2 = make_op(bob(), "Bob", "fill.color", 2, 1001, 2);

        let c1 = detector.submit_local(op1, 1000);
        assert!(c1.is_empty());

        let c2 = detector.submit_remote(op2, 1001);
        assert_eq!(c2.len(), 1);
        assert!(!c2[0].is_resolved()); // Manual strategy
    }

    #[test]
    fn test_last_write_wins() {
        let mut detector = ConflictDetector::new(ConflictStrategy::LastWriteWins);
        let op1 = make_op(alice(), "Alice", "fill.color", 1, 1000, 1);
        let op2 = make_op(bob(), "Bob", "fill.color", 2, 1001, 2);

        detector.submit_local(op1, 1000);
        let conflicts = detector.submit_remote(op2, 1001);
        assert_eq!(conflicts.len(), 1);
        assert!(conflicts[0].is_resolved());
        assert_eq!(
            conflicts[0].resolution,
            Some(ConflictResolution::AcceptRemote) // Bob's ts > Alice's
        );
    }

    #[test]
    fn test_first_write_wins() {
        let mut detector = ConflictDetector::new(ConflictStrategy::FirstWriteWins);
        let op1 = make_op(alice(), "Alice", "fill.color", 1, 1000, 1);
        let op2 = make_op(bob(), "Bob", "fill.color", 2, 1001, 2);

        detector.submit_local(op1, 1000);
        let conflicts = detector.submit_remote(op2, 1001);
        assert!(conflicts[0].is_resolved());
        // Alice was first
        assert_eq!(
            conflicts[0].winning_value(),
            Some(&serde_json::json!(1))
        );
    }

    #[test]
    fn test_highest_sequence_wins() {
        let mut detector = ConflictDetector::new(ConflictStrategy::HighestSequenceWins);
        let op1 = make_op(alice(), "Alice", "fill.color", 1, 1000, 5);
        let op2 = make_op(bob(), "Bob", "fill.color", 2, 999, 10);

        detector.submit_local(op1, 1000);
        let conflicts = detector.submit_remote(op2, 1001);
        assert_eq!(
            conflicts[0].resolution,
            Some(ConflictResolution::AcceptRemote) // Bob's seq=10 > Alice's seq=5
        );
    }

    #[test]
    fn test_local_first_strategy() {
        let mut detector = ConflictDetector::new(ConflictStrategy::LocalFirst);
        let op1 = make_op(alice(), "Alice", "fill.color", 1, 1000, 1);
        let op2 = make_op(bob(), "Bob", "fill.color", 2, 1001, 2);

        detector.submit_local(op1, 1000);
        let conflicts = detector.submit_remote(op2, 1001);
        assert_eq!(
            conflicts[0].resolution,
            Some(ConflictResolution::AcceptLocal)
        );
    }

    #[test]
    fn test_no_conflict_different_components() {
        let mut detector = ConflictDetector::with_defaults();
        let op1 = make_op(alice(), "Alice", "fill.color", 1, 1000, 1);
        let mut op2 = make_op(bob(), "Bob", "fill.color", 2, 1001, 2);
        op2.target_component = ComponentDefId(Uuid::from_bytes([99; 16]));

        detector.submit_local(op1, 1000);
        let c = detector.submit_remote(op2, 1001);
        assert!(c.is_empty());
    }

    #[test]
    fn test_acknowledge_removes_pending() {
        let mut detector = ConflictDetector::with_defaults();
        let op = make_op(alice(), "Alice", "fill.color", 1, 1000, 1);
        let op_id = op.id;
        detector.submit_local(op, 1000);
        assert_eq!(detector.pending_op_count(), 1);

        detector.acknowledge(comp_id(), "fill.color", op_id);
        assert_eq!(detector.pending_op_count(), 0);
    }

    #[test]
    fn test_resolve_conflict_manually() {
        let mut detector = ConflictDetector::new(ConflictStrategy::Manual);
        let op1 = make_op(alice(), "Alice", "fill.color", 1, 1000, 1);
        let op2 = make_op(bob(), "Bob", "fill.color", 2, 1001, 2);

        detector.submit_local(op1, 1000);
        let conflicts = detector.submit_remote(op2, 1001);
        let cid = conflicts[0].id;

        assert_eq!(detector.unresolved_count(), 1);
        detector.resolve_conflict(cid, ConflictResolution::Merge {
            merged_value: serde_json::json!(3),
        });
        assert_eq!(detector.unresolved_count(), 0);
    }

    #[test]
    fn test_clear_resolved() {
        let mut detector = ConflictDetector::with_defaults();
        let op1 = make_op(alice(), "Alice", "fill.color", 1, 1000, 1);
        let op2 = make_op(bob(), "Bob", "fill.color", 2, 1001, 2);

        detector.submit_local(op1, 1000);
        detector.submit_remote(op2, 1001);
        assert_eq!(detector.conflict_count(), 1);

        detector.clear_resolved();
        assert_eq!(detector.conflict_count(), 0);
    }

    #[test]
    fn test_conflict_summary() {
        let op1 = make_op(alice(), "Alice", "fill.color", 1, 1000, 1);
        let op2 = make_op(bob(), "Bob", "fill.color", 2, 1001, 2);
        let conflict = EditConflict::new(op1, op2, 1001);
        let summary = conflict.summary();
        assert!(summary.contains("fill.color"));
        assert!(summary.contains("Alice"));
        assert!(summary.contains("Bob"));
    }

    #[test]
    fn test_auto_resolved_count() {
        let mut detector = ConflictDetector::new(ConflictStrategy::LastWriteWins);
        let op1 = make_op(alice(), "Alice", "fill.color", 1, 1000, 1);
        let op2 = make_op(bob(), "Bob", "fill.color", 2, 1001, 2);

        detector.submit_local(op1, 1000);
        detector.submit_remote(op2, 1001);
        assert_eq!(detector.auto_resolved_count(), 1);
    }

    #[test]
    fn test_conflict_serde_roundtrip() {
        let op1 = make_op(alice(), "Alice", "x", 1, 1000, 1);
        let op2 = make_op(bob(), "Bob", "x", 2, 1001, 2);
        let mut conflict = EditConflict::new(op1, op2, 1001);
        conflict.resolve(ConflictResolution::AcceptLocal);

        let json = serde_json::to_string(&conflict).unwrap();
        let back: EditConflict = serde_json::from_str(&json).unwrap();
        assert!(back.is_resolved());
    }

    #[test]
    fn test_strategy_default() {
        assert_eq!(ConflictStrategy::default(), ConflictStrategy::LastWriteWins);
    }

    #[test]
    fn test_set_strategy() {
        let mut detector = ConflictDetector::with_defaults();
        detector.set_strategy(ConflictStrategy::Manual);
        assert_eq!(detector.strategy(), ConflictStrategy::Manual);
    }
}
