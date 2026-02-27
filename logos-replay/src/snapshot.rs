//! Periodic snapshot storage.
//!
//! Snapshots capture the full reconstructed state at a particular version.
//! During replay, these allow jumping to a known-good state and replaying
//! only the tail of the log rather than the entire history.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::ReplayError;

/// Unique identifier for a snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SnapshotId(pub Uuid);

impl SnapshotId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for SnapshotId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SnapshotId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A snapshot of the document state at a particular version.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    /// Unique snapshot ID.
    pub id: SnapshotId,
    /// The document version this snapshot represents.
    pub version: u64,
    /// The document ID this snapshot is for.
    pub document_id: Uuid,
    /// Serialized state as JSON value (type-erased).
    pub state: serde_json::Value,
    /// Wall-clock timestamp when the snapshot was created.
    pub created_at: u64,
    /// Optional label/description.
    pub label: Option<String>,
    /// Size in bytes of the serialized state.
    pub size_bytes: usize,
}

impl Snapshot {
    /// Create a new snapshot.
    pub fn new(version: u64, document_id: Uuid, state: serde_json::Value) -> Self {
        let size_bytes = state.to_string().len();
        Self {
            id: SnapshotId::new(),
            version,
            document_id,
            state,
            created_at: crate::envelope::current_timestamp(),
            label: None,
            size_bytes,
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Deserialize the state into a typed value.
    pub fn deserialize_state<T: for<'de> Deserialize<'de>>(
        &self,
    ) -> Result<T, serde_json::Error> {
        serde_json::from_value(self.state.clone())
    }
}

/// Policy for when to take snapshots.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotPolicy {
    /// Take a snapshot every N operations.
    pub every_n_ops: u64,
    /// Take a snapshot every N seconds (wall-clock).
    pub every_n_seconds: Option<u64>,
    /// Maximum number of snapshots to retain.
    pub max_snapshots: Option<usize>,
}

impl SnapshotPolicy {
    pub fn every_n_ops(n: u64) -> Self {
        Self {
            every_n_ops: n,
            every_n_seconds: None,
            max_snapshots: None,
        }
    }

    pub fn with_max_snapshots(mut self, max: usize) -> Self {
        self.max_snapshots = Some(max);
        self
    }

    pub fn with_time_interval(mut self, seconds: u64) -> Self {
        self.every_n_seconds = Some(seconds);
        self
    }

    /// Should we snapshot after this version?
    pub fn should_snapshot(&self, version: u64, last_snapshot_version: Option<u64>) -> bool {
        let ops_since = match last_snapshot_version {
            Some(lsv) => version.saturating_sub(lsv),
            None => version,
        };
        ops_since >= self.every_n_ops
    }
}

impl Default for SnapshotPolicy {
    fn default() -> Self {
        Self::every_n_ops(100)
    }
}

/// Trait for snapshot storage.
pub trait SnapshotStore {
    /// Store a snapshot.
    fn save(&mut self, snapshot: Snapshot) -> Result<SnapshotId, ReplayError>;

    /// Load a snapshot by ID.
    fn get(&self, id: &SnapshotId) -> Result<&Snapshot, ReplayError>;

    /// Find the latest snapshot at or before a given version for a document.
    fn find_nearest(
        &self,
        document_id: &Uuid,
        version: u64,
    ) -> Option<&Snapshot>;

    /// Get the latest snapshot for a document.
    fn latest(&self, document_id: &Uuid) -> Option<&Snapshot>;

    /// List all snapshots for a document, ordered by version.
    fn list(&self, document_id: &Uuid) -> Vec<&Snapshot>;

    /// Delete a snapshot.
    fn delete(&mut self, id: &SnapshotId) -> Result<(), ReplayError>;

    /// Number of stored snapshots.
    fn count(&self) -> usize;

    /// Enforce max-snapshots policy by removing oldest.
    fn enforce_limit(&mut self, document_id: &Uuid, max: usize) -> usize;
}

/// In-memory snapshot store.
#[derive(Debug, Default)]
pub struct InMemorySnapshotStore {
    snapshots: Vec<Snapshot>,
}

impl InMemorySnapshotStore {
    pub fn new() -> Self {
        Self {
            snapshots: Vec::new(),
        }
    }

    pub fn all(&self) -> &[Snapshot] {
        &self.snapshots
    }
}

impl SnapshotStore for InMemorySnapshotStore {
    fn save(&mut self, snapshot: Snapshot) -> Result<SnapshotId, ReplayError> {
        let id = snapshot.id;
        self.snapshots.push(snapshot);
        // Keep sorted by version.
        self.snapshots.sort_by_key(|s| s.version);
        Ok(id)
    }

    fn get(&self, id: &SnapshotId) -> Result<&Snapshot, ReplayError> {
        self.snapshots
            .iter()
            .find(|s| s.id == *id)
            .ok_or(ReplayError::SnapshotNotFound {
                id: id.to_string(),
            })
    }

    fn find_nearest(&self, document_id: &Uuid, version: u64) -> Option<&Snapshot> {
        self.snapshots
            .iter()
            .filter(|s| s.document_id == *document_id && s.version <= version)
            .last() // sorted by version, so last matching is nearest
    }

    fn latest(&self, document_id: &Uuid) -> Option<&Snapshot> {
        self.snapshots
            .iter()
            .filter(|s| s.document_id == *document_id)
            .last()
    }

    fn list(&self, document_id: &Uuid) -> Vec<&Snapshot> {
        self.snapshots
            .iter()
            .filter(|s| s.document_id == *document_id)
            .collect()
    }

    fn delete(&mut self, id: &SnapshotId) -> Result<(), ReplayError> {
        let before = self.snapshots.len();
        self.snapshots.retain(|s| s.id != *id);
        if self.snapshots.len() < before {
            Ok(())
        } else {
            Err(ReplayError::SnapshotNotFound {
                id: id.to_string(),
            })
        }
    }

    fn count(&self) -> usize {
        self.snapshots.len()
    }

    fn enforce_limit(&mut self, document_id: &Uuid, max: usize) -> usize {
        let doc_snapshots: Vec<_> = self
            .snapshots
            .iter()
            .filter(|s| s.document_id == *document_id)
            .cloned()
            .collect();

        if doc_snapshots.len() <= max {
            return 0;
        }

        let to_remove = doc_snapshots.len() - max;
        let remove_ids: Vec<_> = doc_snapshots
            .iter()
            .take(to_remove)
            .map(|s| s.id)
            .collect();

        self.snapshots.retain(|s| !remove_ids.contains(&s.id));
        to_remove
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn make_snapshot(version: u64, doc: Uuid) -> Snapshot {
        Snapshot::new(
            version,
            doc,
            serde_json::json!({ "version": version, "data": "test" }),
        )
    }

    // ── Snapshot ─────────────────────────────────────────────────────

    #[test]
    fn snapshot_creation() {
        let doc = Uuid::new_v4();
        let s = make_snapshot(5, doc);
        assert_eq!(s.version, 5);
        assert_eq!(s.document_id, doc);
        assert!(s.size_bytes > 0);
    }

    #[test]
    fn snapshot_with_label() {
        let s = make_snapshot(1, Uuid::new_v4()).with_label("Initial");
        assert_eq!(s.label.as_deref(), Some("Initial"));
    }

    #[test]
    fn snapshot_deserialize_state() {
        #[derive(Debug, PartialEq, Deserialize)]
        struct State {
            version: u64,
            data: String,
        }
        let s = make_snapshot(3, Uuid::new_v4());
        let state: State = s.deserialize_state().unwrap();
        assert_eq!(state.version, 3);
        assert_eq!(state.data, "test");
    }

    #[test]
    fn snapshot_serde_roundtrip() {
        let s = make_snapshot(7, Uuid::new_v4());
        let json = serde_json::to_string(&s).unwrap();
        let back: Snapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(back.version, s.version);
        assert_eq!(back.state, s.state);
    }

    // ── SnapshotPolicy ──────────────────────────────────────────────

    #[test]
    fn policy_should_snapshot() {
        let policy = SnapshotPolicy::every_n_ops(10);
        assert!(!policy.should_snapshot(5, None));
        assert!(policy.should_snapshot(10, None));
        assert!(policy.should_snapshot(15, None));
        assert!(!policy.should_snapshot(15, Some(10)));
        assert!(policy.should_snapshot(20, Some(10)));
    }

    // ── InMemorySnapshotStore ───────────────────────────────────────

    #[test]
    fn store_save_and_get() {
        let mut store = InMemorySnapshotStore::new();
        let doc = Uuid::new_v4();
        let s = make_snapshot(1, doc);
        let id = store.save(s).unwrap();
        let loaded = store.get(&id).unwrap();
        assert_eq!(loaded.version, 1);
    }

    #[test]
    fn store_get_missing() {
        let store = InMemorySnapshotStore::new();
        let id = SnapshotId::new();
        assert!(store.get(&id).is_err());
    }

    #[test]
    fn store_find_nearest() {
        let mut store = InMemorySnapshotStore::new();
        let doc = Uuid::new_v4();
        store.save(make_snapshot(10, doc)).unwrap();
        store.save(make_snapshot(20, doc)).unwrap();
        store.save(make_snapshot(30, doc)).unwrap();

        let s = store.find_nearest(&doc, 25).unwrap();
        assert_eq!(s.version, 20);

        let s = store.find_nearest(&doc, 30).unwrap();
        assert_eq!(s.version, 30);

        assert!(store.find_nearest(&doc, 5).is_none());
    }

    #[test]
    fn store_latest() {
        let mut store = InMemorySnapshotStore::new();
        let doc = Uuid::new_v4();
        store.save(make_snapshot(5, doc)).unwrap();
        store.save(make_snapshot(15, doc)).unwrap();
        assert_eq!(store.latest(&doc).unwrap().version, 15);
    }

    #[test]
    fn store_list() {
        let mut store = InMemorySnapshotStore::new();
        let doc1 = Uuid::new_v4();
        let doc2 = Uuid::new_v4();
        store.save(make_snapshot(1, doc1)).unwrap();
        store.save(make_snapshot(2, doc2)).unwrap();
        store.save(make_snapshot(3, doc1)).unwrap();
        assert_eq!(store.list(&doc1).len(), 2);
        assert_eq!(store.list(&doc2).len(), 1);
    }

    #[test]
    fn store_delete() {
        let mut store = InMemorySnapshotStore::new();
        let doc = Uuid::new_v4();
        let id = store.save(make_snapshot(1, doc)).unwrap();
        assert_eq!(store.count(), 1);
        store.delete(&id).unwrap();
        assert_eq!(store.count(), 0);
    }

    #[test]
    fn store_delete_missing() {
        let mut store = InMemorySnapshotStore::new();
        let id = SnapshotId::new();
        assert!(store.delete(&id).is_err());
    }

    #[test]
    fn store_enforce_limit() {
        let mut store = InMemorySnapshotStore::new();
        let doc = Uuid::new_v4();
        for v in 1..=10 {
            store.save(make_snapshot(v, doc)).unwrap();
        }
        let removed = store.enforce_limit(&doc, 3);
        assert_eq!(removed, 7);
        assert_eq!(store.list(&doc).len(), 3);
        // Should keep the latest 3: versions 8, 9, 10
        let versions: Vec<_> = store.list(&doc).iter().map(|s| s.version).collect();
        assert_eq!(versions, vec![8, 9, 10]);
    }

    #[test]
    fn store_enforce_limit_no_op() {
        let mut store = InMemorySnapshotStore::new();
        let doc = Uuid::new_v4();
        store.save(make_snapshot(1, doc)).unwrap();
        let removed = store.enforce_limit(&doc, 5);
        assert_eq!(removed, 0);
    }
}
