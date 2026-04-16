// SPDX-License-Identifier: MPL-2.0
// logos-collab/src/storage/memory.rs — In-memory DocumentStore
//
// This module provides the same public API as `storage::rocks` (the
// RocksDB-backed store) but operates purely in memory.  It is compiled
// when the `persistent-storage` feature is **disabled**, which avoids
// the `libclang` / `clang` dependency that RocksDB's `bindgen` step
// requires.
//
// All data is lost when the process exits.  This is intentional for
// testing environments and development machines that do not have LLVM
// installed.
//
// Reference: Knuth, TAOCP Vol.1 – fundamental data structures.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

// ────────────────────────────────────────────────────────────────────────────
// StoreConfig
// ────────────────────────────────────────────────────────────────────────────

/// Mirrors `rocks::StoreConfig`.  Fields are accepted but ignored by
/// the in-memory backend.
#[derive(Debug, Clone)]
pub struct StoreConfig {
    pub path: PathBuf,
    pub block_cache_size: usize,
    pub bloom_filter_bits: i32,
    pub sync_writes: bool,
    pub max_open_files: i32,
    pub write_buffer_size: usize,
}

impl Default for StoreConfig {
    fn default() -> Self {
        Self {
            path: PathBuf::from(":memory:"),
            block_cache_size: 0,
            bloom_filter_bits: 10,
            sync_writes: false,
            max_open_files: 64,
            write_buffer_size: 0,
        }
    }
}

impl StoreConfig {
    /// Convenience constructor used in tests / server code.
    pub fn for_testing(path: impl Into<PathBuf>) -> Self {
        let mut c = Self::default();
        c.path = path.into();
        c
    }
}

// ────────────────────────────────────────────────────────────────────────────
// DocumentMetadata
// ────────────────────────────────────────────────────────────────────────────

/// Mirrors `rocks::DocumentMetadata`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct DocumentMetadata {
    pub doc_id: Uuid,
    pub created_at: u64,
    pub updated_at: u64,
    pub snapshot_version: u64,
    pub delta_count: u64,
    pub snapshot_size: u64,
}

// ────────────────────────────────────────────────────────────────────────────
// StoreError
// ────────────────────────────────────────────────────────────────────────────

/// Mirrors `rocks::StoreError`.
#[derive(Debug, Clone)]
pub enum StoreError {
    NotFound(String),
    Corruption(String),
    IoError(String),
    SerializationError(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(m)          => write!(f, "Not found: {m}"),
            Self::Corruption(m)        => write!(f, "Corruption: {m}"),
            Self::IoError(m)           => write!(f, "I/O error: {m}"),
            Self::SerializationError(m)=> write!(f, "Serialization error: {m}"),
        }
    }
}

impl std::error::Error for StoreError {}

// ────────────────────────────────────────────────────────────────────────────
// Inner store state (behind an Arc<Mutex<…>>)
// ────────────────────────────────────────────────────────────────────────────

#[derive(Default)]
struct Inner {
    /// Full snapshots: doc_id → (version, bytes)
    snapshots: HashMap<Uuid, (u64, Vec<u8>)>,
    /// Deltas: doc_id → Vec<(version, bytes)>
    deltas: HashMap<Uuid, Vec<(u64, Vec<u8>)>>,
    /// Metadata
    metadata: HashMap<Uuid, DocumentMetadata>,
    /// Write-ahead log: monotonic sequence → bytes
    wal: Vec<(u64, Vec<u8>)>,
    /// WAL sequence counter
    wal_seq: u64,
}

// ────────────────────────────────────────────────────────────────────────────
// DocumentStore
// ────────────────────────────────────────────────────────────────────────────

/// Thread-safe in-memory document store.  Drop-in replacement for the
/// RocksDB-backed `rocks::DocumentStore` when the `persistent-storage`
/// feature is disabled.
#[derive(Clone)]
pub struct DocumentStore {
    inner: Arc<Mutex<Inner>>,
    path: PathBuf,
}

impl DocumentStore {
    // ── Construction ──────────────────────────────────────────────────────

    pub fn open(config: StoreConfig) -> Result<Self, StoreError> {
        Ok(Self {
            inner: Arc::new(Mutex::new(Inner::default())),
            path: config.path,
        })
    }

    // ── Snapshot API ──────────────────────────────────────────────────────

    pub fn save_snapshot(
        &self,
        doc_id: Uuid,
        version: u64,
        snapshot: &[u8],
    ) -> Result<(), StoreError> {
        let mut g = self.lock();
        g.snapshots.insert(doc_id, (version, snapshot.to_vec()));
        g.metadata.entry(doc_id).and_modify(|m| {
            m.snapshot_version = version;
            m.snapshot_size = snapshot.len() as u64;
            m.updated_at = now_secs();
        }).or_insert_with(|| DocumentMetadata {
            doc_id,
            created_at: now_secs(),
            updated_at: now_secs(),
            snapshot_version: version,
            delta_count: 0,
            snapshot_size: snapshot.len() as u64,
        });
        Ok(())
    }

    pub fn load_snapshot(&self, doc_id: Uuid) -> Result<Vec<u8>, StoreError> {
        self.lock()
            .snapshots
            .get(&doc_id)
            .map(|(_, b)| b.clone())
            .ok_or_else(|| StoreError::NotFound(doc_id.to_string()))
    }

    pub fn document_exists(&self, doc_id: Uuid) -> Result<bool, StoreError> {
        Ok(self.lock().snapshots.contains_key(&doc_id)
            || self.lock().deltas.contains_key(&doc_id))
    }

    // ── Delta API ─────────────────────────────────────────────────────────

    pub fn store_delta(
        &self,
        doc_id: Uuid,
        version: u64,
        data: &[u8],
    ) -> Result<(), StoreError> {
        let mut g = self.lock();
        g.deltas.entry(doc_id).or_default().push((version, data.to_vec()));
        g.metadata.entry(doc_id).and_modify(|m| {
            m.delta_count += 1;
            m.updated_at = now_secs();
        }).or_insert_with(|| DocumentMetadata {
            doc_id,
            created_at: now_secs(),
            updated_at: now_secs(),
            snapshot_version: 0,
            delta_count: 1,
            snapshot_size: 0,
        });
        Ok(())
    }

    pub fn load_deltas_since(
        &self,
        doc_id: Uuid,
        since_version: u64,
    ) -> Result<Vec<(u64, Vec<u8>)>, StoreError> {
        let g = self.lock();
        let result = g.deltas.get(&doc_id).map(|v| {
            v.iter()
                .filter(|(ver, _)| *ver > since_version)
                .cloned()
                .collect()
        }).unwrap_or_default();
        Ok(result)
    }

    pub fn load_all_deltas(&self, doc_id: Uuid) -> Result<Vec<(u64, Vec<u8>)>, StoreError> {
        Ok(self.lock().deltas.get(&doc_id).cloned().unwrap_or_default())
    }

    pub fn delta_count(&self, doc_id: Uuid) -> Result<u64, StoreError> {
        Ok(self.lock().deltas.get(&doc_id).map(|v| v.len() as u64).unwrap_or(0))
    }

    pub fn compact_deltas(&self, doc_id: Uuid, up_to_version: u64) -> Result<u64, StoreError> {
        let mut g = self.lock();
        let removed = if let Some(deltas) = g.deltas.get_mut(&doc_id) {
            let before = deltas.len();
            deltas.retain(|(v, _)| *v > up_to_version);
            (before - deltas.len()) as u64
        } else {
            0
        };
        Ok(removed)
    }

    // ── Metadata API ──────────────────────────────────────────────────────

    pub fn load_metadata(&self, doc_id: Uuid) -> Result<DocumentMetadata, StoreError> {
        self.lock()
            .metadata
            .get(&doc_id)
            .cloned()
            .ok_or_else(|| StoreError::NotFound(doc_id.to_string()))
    }

    pub fn list_documents(&self) -> Result<Vec<Uuid>, StoreError> {
        let g = self.lock();
        let mut ids: std::collections::HashSet<Uuid> = g.snapshots.keys().cloned().collect();
        ids.extend(g.deltas.keys());
        Ok(ids.into_iter().collect())
    }

    pub fn delete_document(&self, doc_id: Uuid) -> Result<(), StoreError> {
        let mut g = self.lock();
        g.snapshots.remove(&doc_id);
        g.deltas.remove(&doc_id);
        g.metadata.remove(&doc_id);
        Ok(())
    }

    // ── WAL API ───────────────────────────────────────────────────────────

    pub fn wal_append(&self, data: &[u8]) -> Result<u64, StoreError> {
        let mut g = self.lock();
        g.wal_seq += 1;
        let seq = g.wal_seq;
        g.wal.push((seq, data.to_vec()));
        Ok(seq)
    }

    pub fn wal_read_since(&self, since_seq: u64) -> Result<Vec<(u64, Vec<u8>)>, StoreError> {
        let g = self.lock();
        let result = g.wal.iter()
            .filter(|(seq, _)| *seq > since_seq)
            .cloned()
            .collect();
        Ok(result)
    }

    pub fn wal_truncate(&self, up_to_seq: u64) -> Result<u64, StoreError> {
        let mut g = self.lock();
        let before = g.wal.len();
        g.wal.retain(|(seq, _)| *seq > up_to_seq);
        Ok((before - g.wal.len()) as u64)
    }

    pub fn sync(&self) -> Result<(), StoreError> {
        // No-op for in-memory store.
        Ok(())
    }

    pub fn wal_sequence(&self) -> u64 {
        self.lock().wal_seq
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    // ── Internal ──────────────────────────────────────────────────────────

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("DocumentStore mutex poisoned")
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Helpers
// ────────────────────────────────────────────────────────────────────────────

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ────────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> DocumentStore {
        DocumentStore::open(StoreConfig::default()).unwrap()
    }

    #[test]
    fn test_open_succeeds() {
        let _ = store();
    }

    #[test]
    fn test_save_and_load_snapshot() {
        let s = store();
        let id = Uuid::new_v4();
        s.save_snapshot(id, 1, b"hello").unwrap();
        assert_eq!(s.load_snapshot(id).unwrap(), b"hello");
    }

    #[test]
    fn test_load_snapshot_missing_returns_not_found() {
        let s = store();
        let err = s.load_snapshot(Uuid::new_v4());
        assert!(matches!(err, Err(StoreError::NotFound(_))));
    }

    #[test]
    fn test_document_exists_after_snapshot() {
        let s = store();
        let id = Uuid::new_v4();
        assert!(!s.document_exists(id).unwrap());
        s.save_snapshot(id, 0, b"data").unwrap();
        assert!(s.document_exists(id).unwrap());
    }

    #[test]
    fn test_store_and_load_deltas() {
        let s = store();
        let id = Uuid::new_v4();
        s.store_delta(id, 1, b"d1").unwrap();
        s.store_delta(id, 2, b"d2").unwrap();
        s.store_delta(id, 3, b"d3").unwrap();

        let all = s.load_all_deltas(id).unwrap();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_load_deltas_since_filters() {
        let s = store();
        let id = Uuid::new_v4();
        s.store_delta(id, 1, b"a").unwrap();
        s.store_delta(id, 2, b"b").unwrap();
        s.store_delta(id, 3, b"c").unwrap();

        let newer = s.load_deltas_since(id, 1).unwrap();
        assert_eq!(newer.len(), 2);
        assert!(newer.iter().all(|(v, _)| *v > 1));
    }

    #[test]
    fn test_delta_count() {
        let s = store();
        let id = Uuid::new_v4();
        assert_eq!(s.delta_count(id).unwrap(), 0);
        s.store_delta(id, 1, b"x").unwrap();
        s.store_delta(id, 2, b"y").unwrap();
        assert_eq!(s.delta_count(id).unwrap(), 2);
    }

    #[test]
    fn test_compact_deltas_removes_old() {
        let s = store();
        let id = Uuid::new_v4();
        for v in 1..=5u64 {
            s.store_delta(id, v, b"d").unwrap();
        }
        let removed = s.compact_deltas(id, 3).unwrap();
        assert_eq!(removed, 3); // versions 1,2,3 removed
        assert_eq!(s.delta_count(id).unwrap(), 2); // versions 4,5 remain
    }

    #[test]
    fn test_metadata_created_on_snapshot() {
        let s = store();
        let id = Uuid::new_v4();
        s.save_snapshot(id, 7, b"snap").unwrap();
        let meta = s.load_metadata(id).unwrap();
        assert_eq!(meta.doc_id, id);
        assert_eq!(meta.snapshot_version, 7);
        assert_eq!(meta.snapshot_size, 4);
    }

    #[test]
    fn test_metadata_not_found() {
        let s = store();
        let err = s.load_metadata(Uuid::new_v4());
        assert!(matches!(err, Err(StoreError::NotFound(_))));
    }

    #[test]
    fn test_list_documents() {
        let s = store();
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        s.save_snapshot(id1, 0, b"a").unwrap();
        s.store_delta(id2, 1, b"b").unwrap();

        let mut docs = s.list_documents().unwrap();
        docs.sort();
        assert!(docs.contains(&id1));
        assert!(docs.contains(&id2));
    }

    #[test]
    fn test_delete_document() {
        let s = store();
        let id = Uuid::new_v4();
        s.save_snapshot(id, 0, b"data").unwrap();
        s.store_delta(id, 1, b"d").unwrap();
        s.delete_document(id).unwrap();
        assert!(!s.document_exists(id).unwrap());
        assert!(s.load_metadata(id).is_err());
    }

    #[test]
    fn test_wal_append_and_read() {
        let s = store();
        let seq1 = s.wal_append(b"op1").unwrap();
        let seq2 = s.wal_append(b"op2").unwrap();
        assert!(seq2 > seq1);

        let entries = s.wal_read_since(0).unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_wal_read_since_filters() {
        let s = store();
        s.wal_append(b"a").unwrap();
        let seq = s.wal_append(b"b").unwrap();
        s.wal_append(b"c").unwrap();

        let after = s.wal_read_since(seq).unwrap();
        assert_eq!(after.len(), 1);
    }

    #[test]
    fn test_wal_truncate() {
        let s = store();
        let seq1 = s.wal_append(b"x").unwrap();
        let _seq2 = s.wal_append(b"y").unwrap();
        let removed = s.wal_truncate(seq1).unwrap();
        assert_eq!(removed, 1);
        let remaining = s.wal_read_since(0).unwrap();
        assert_eq!(remaining.len(), 1);
    }

    #[test]
    fn test_wal_sequence_increments() {
        let s = store();
        assert_eq!(s.wal_sequence(), 0);
        s.wal_append(b"a").unwrap();
        assert_eq!(s.wal_sequence(), 1);
        s.wal_append(b"b").unwrap();
        assert_eq!(s.wal_sequence(), 2);
    }

    #[test]
    fn test_sync_succeeds() {
        let s = store();
        assert!(s.sync().is_ok());
    }

    #[test]
    fn test_path_returns_configured_path() {
        let s = DocumentStore::open(StoreConfig {
            path: PathBuf::from("/tmp/logos-test"),
            ..StoreConfig::default()
        }).unwrap();
        assert_eq!(s.path(), Path::new("/tmp/logos-test"));
    }

    #[test]
    fn test_clone_shares_state() {
        let s1 = store();
        let id = Uuid::new_v4();
        s1.save_snapshot(id, 1, b"shared").unwrap();

        let s2 = s1.clone();
        assert_eq!(s2.load_snapshot(id).unwrap(), b"shared");

        // Writes through either clone are visible from both
        s2.store_delta(id, 2, b"delta").unwrap();
        assert_eq!(s1.delta_count(id).unwrap(), 1);
    }
}
