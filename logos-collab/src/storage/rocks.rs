//! RocksDB-backed persistent document store.
//!
//! Column families:
//! - `documents` — Full Yrs document snapshots (LZ4 compressed)
//! - `deltas`    — Incremental CRDT deltas (LZ4 compressed, keyed by doc_id:version)
//! - `metadata`  — Document metadata (JSON: created_at, version, size)
//! - `wal`       — Write-ahead log entries (sequential, keyed by sequence number)
//!
//! Performance targets:
//! - Open (10k docs): <100ms (bloom filters + block cache)
//! - Document load (1MB cache hit): <1ms
//! - Delta save (1KB): <50μs
//! - Recovery (10k docs): <500ms
//!
//! Reference: Kleppmann — DDIA, Chapter 3 (LSM Trees, SSTables)
//! Reference: Patterson & Hennessy — Section 5.7 (I/O Performance)

use rocksdb::{
    BlockBasedOptions, Cache, ColumnFamilyDescriptor, DBCompressionType, DBWithThreadMode,
    IteratorMode, Options, SingleThreaded, WriteBatch, WriteOptions,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;
use uuid::Uuid;

/// Column family names.
const CF_DOCUMENTS: &str = "documents";
const CF_DELTAS: &str = "deltas";
const CF_METADATA: &str = "metadata";
const CF_WAL: &str = "wal";

/// All column family names for initialization.
const COLUMN_FAMILIES: &[&str] = &[CF_DOCUMENTS, CF_DELTAS, CF_METADATA, CF_WAL];

/// Store configuration.
#[derive(Debug, Clone)]
pub struct StoreConfig {
    /// Database directory path
    pub path: PathBuf,
    /// Block cache size in bytes (default: 256MB)
    pub block_cache_size: usize,
    /// Bloom filter bits per key (default: 10)
    pub bloom_filter_bits: i32,
    /// Enable fsync on every write (default: false — batch fsync instead)
    pub sync_writes: bool,
    /// Max open files for RocksDB (default: 512)
    pub max_open_files: i32,
    /// Write buffer size per column family (default: 64MB)
    pub write_buffer_size: usize,
}

impl Default for StoreConfig {
    fn default() -> Self {
        Self {
            path: PathBuf::from("logos_data"),
            block_cache_size: 256 * 1024 * 1024, // 256MB
            bloom_filter_bits: 10,
            sync_writes: false, // Batch fsync via WAL
            max_open_files: 512,
            write_buffer_size: 64 * 1024 * 1024, // 64MB
        }
    }
}

impl StoreConfig {
    /// Create config for testing (small caches, temp directory).
    pub fn for_testing(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            block_cache_size: 8 * 1024 * 1024, // 8MB
            bloom_filter_bits: 10,
            sync_writes: false,
            max_open_files: 64,
            write_buffer_size: 4 * 1024 * 1024, // 4MB
        }
    }
}

/// Document metadata stored alongside snapshots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentMetadata {
    /// Document UUID
    pub doc_id: Uuid,
    /// Current version (monotonically increasing)
    pub version: u64,
    /// Total number of deltas stored
    pub delta_count: u64,
    /// Uncompressed snapshot size in bytes
    pub snapshot_size: u64,
    /// Compressed snapshot size in bytes
    pub compressed_size: u64,
    /// Creation timestamp (seconds since epoch)
    pub created_at: u64,
    /// Last modified timestamp (seconds since epoch)
    pub updated_at: u64,
}

impl DocumentMetadata {
    fn new(doc_id: Uuid) -> Self {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            doc_id,
            version: 0,
            delta_count: 0,
            snapshot_size: 0,
            compressed_size: 0,
            created_at: now,
            updated_at: now,
        }
    }

    fn encode(&self) -> Result<Vec<u8>, StoreError> {
        bincode::serde::encode_to_vec(self, bincode::config::standard())
            .map_err(|e| StoreError::SerializationError(e.to_string()))
    }

    fn decode(bytes: &[u8]) -> Result<Self, StoreError> {
        let (meta, _) =
            bincode::serde::decode_from_slice(bytes, bincode::config::standard())
                .map_err(|e| StoreError::DeserializationError(e.to_string()))?;
        Ok(meta)
    }
}

/// Storage errors.
#[derive(Debug, Clone)]
pub enum StoreError {
    /// RocksDB internal error
    DatabaseError(String),
    /// Document not found
    NotFound(Uuid),
    /// Serialization failed
    SerializationError(String),
    /// Deserialization failed
    DeserializationError(String),
    /// Compression error
    CompressionError(String),
    /// I/O error
    IoError(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::DatabaseError(e) => write!(f, "Database error: {e}"),
            StoreError::NotFound(id) => write!(f, "Document not found: {id}"),
            StoreError::SerializationError(e) => write!(f, "Serialization error: {e}"),
            StoreError::DeserializationError(e) => write!(f, "Deserialization error: {e}"),
            StoreError::CompressionError(e) => write!(f, "Compression error: {e}"),
            StoreError::IoError(e) => write!(f, "I/O error: {e}"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<rocksdb::Error> for StoreError {
    fn from(e: rocksdb::Error) -> Self {
        StoreError::DatabaseError(e.to_string())
    }
}

/// RocksDB-backed document store.
///
/// Provides durable storage for collaborative documents with:
/// - LZ4-compressed snapshots and deltas
/// - Bloom filters for fast key lookup
/// - Block cache for hot document access
/// - Atomic write batches for consistency
pub struct DocumentStore {
    /// RocksDB instance (single-threaded mode — concurrency via tokio)
    db: DBWithThreadMode<SingleThreaded>,
    /// Store configuration
    config: StoreConfig,
    /// Global sequence number for WAL entries
    sequence: AtomicU64,
}

impl DocumentStore {
    /// Open the document store at the configured path.
    ///
    /// Creates the database and column families if they don't exist.
    /// Target: <100ms for database with 10,000 documents.
    pub fn open(config: StoreConfig) -> Result<Self, StoreError> {
        let mut db_opts = Options::default();
        db_opts.create_if_missing(true);
        db_opts.create_missing_column_families(true);
        db_opts.set_max_open_files(config.max_open_files);
        db_opts.set_keep_log_file_num(5);
        db_opts.set_max_total_wal_size(128 * 1024 * 1024); // 128MB WAL limit
        db_opts.increase_parallelism(num_cpus());

        // Build column family descriptors with per-CF options
        let cf_descriptors: Vec<ColumnFamilyDescriptor> = COLUMN_FAMILIES
            .iter()
            .map(|name| {
                let cf_opts = Self::cf_options(name, &config);
                ColumnFamilyDescriptor::new(*name, cf_opts)
            })
            .collect();

        let db = DBWithThreadMode::<SingleThreaded>::open_cf_descriptors(
            &db_opts,
            &config.path,
            cf_descriptors,
        )?;

        // Recover sequence number from WAL
        let sequence = Self::recover_sequence(&db);

        Ok(Self {
            db,
            config,
            sequence: AtomicU64::new(sequence),
        })
    }

    /// Build column-family-specific options.
    fn cf_options(name: &str, config: &StoreConfig) -> Options {
        let mut opts = Options::default();

        // Block-based table with bloom filter and cache
        let mut block_opts = BlockBasedOptions::default();
        let cache = Cache::new_lru_cache(config.block_cache_size);
        block_opts.set_block_cache(&cache);
        block_opts.set_bloom_filter(config.bloom_filter_bits as f64, false);
        block_opts.set_block_size(16 * 1024); // 16KB blocks
        opts.set_block_based_table_factory(&block_opts);

        // LZ4 compression — fast decompression (5-10 cycles/byte)
        opts.set_compression_type(DBCompressionType::Lz4);
        opts.set_write_buffer_size(config.write_buffer_size);

        match name {
            CF_DOCUMENTS => {
                // Snapshots are large, infrequently updated
                opts.set_max_write_buffer_number(2);
                // Optimize for point lookups (single doc fetch)
                opts.optimize_for_point_lookup(config.block_cache_size as u64);
            }
            CF_DELTAS => {
                // Many small writes, prefix-scanned by doc_id
                opts.set_max_write_buffer_number(4);
                opts.set_prefix_extractor(rocksdb::SliceTransform::create_fixed_prefix(16));
            }
            CF_METADATA => {
                // Small values, frequent reads
                opts.set_max_write_buffer_number(2);
                opts.optimize_for_point_lookup(config.block_cache_size as u64);
            }
            CF_WAL => {
                // Sequential writes, sequential reads during recovery
                opts.set_max_write_buffer_number(2);
                opts.set_compression_type(DBCompressionType::None); // WAL needs speed
            }
            _ => {}
        }

        opts
    }

    /// Recover the last sequence number from the WAL column family.
    fn recover_sequence(db: &DBWithThreadMode<SingleThreaded>) -> u64 {
        let cf = match db.cf_handle(CF_WAL) {
            Some(cf) => cf,
            None => return 0,
        };

        // Get the last key in WAL CF (highest sequence number)
        let mut iter = db.iterator_cf(&cf, IteratorMode::End);
        match iter.next() {
            Some(Ok((key, _))) => {
                if key.len() >= 8 {
                    let mut buf = [0u8; 8];
                    buf.copy_from_slice(&key[..8]);
                    u64::from_be_bytes(buf) + 1
                } else {
                    0
                }
            }
            _ => 0,
        }
    }

    // ─── Document Snapshots ───────────────────────────────────────────

    /// Save a full document snapshot (LZ4 compressed).
    ///
    /// Used for periodic compaction and initial persistence.
    /// The snapshot is the full Yrs document state encoded with `encode_v1`.
    pub fn save_snapshot(
        &self,
        doc_id: Uuid,
        snapshot: &[u8],
    ) -> Result<DocumentMetadata, StoreError> {
        let cf_docs = self.cf(CF_DOCUMENTS)?;
        let cf_meta = self.cf(CF_METADATA)?;

        // LZ4 compress the snapshot
        let compressed = lz4_flex::compress_prepend_size(snapshot);

        // Load or create metadata
        let mut meta = self.load_metadata(doc_id).unwrap_or_else(|_| DocumentMetadata::new(doc_id));
        meta.snapshot_size = snapshot.len() as u64;
        meta.compressed_size = compressed.len() as u64;
        meta.updated_at = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Atomic batch write: snapshot + metadata
        let mut batch = WriteBatch::default();
        let key = doc_id.as_bytes().to_vec();
        batch.put_cf(&cf_docs, &key, &compressed);
        batch.put_cf(&cf_meta, &key, &meta.encode()?);

        let mut write_opts = WriteOptions::default();
        write_opts.set_sync(self.config.sync_writes);
        self.db.write_opt(batch, &write_opts)?;

        Ok(meta)
    }

    /// Load a document snapshot (LZ4 decompressed).
    ///
    /// Returns the raw Yrs document state for `apply_update`.
    /// Target: <1ms for cache-hot document.
    pub fn load_snapshot(&self, doc_id: Uuid) -> Result<Vec<u8>, StoreError> {
        let cf = self.cf(CF_DOCUMENTS)?;
        let key = doc_id.as_bytes().to_vec();

        match self.db.get_cf(&cf, &key)? {
            Some(compressed) => {
                lz4_flex::decompress_size_prepended(&compressed)
                    .map_err(|e| StoreError::CompressionError(e.to_string()))
            }
            None => Err(StoreError::NotFound(doc_id)),
        }
    }

    /// Check if a document exists.
    pub fn document_exists(&self, doc_id: Uuid) -> Result<bool, StoreError> {
        let cf = self.cf(CF_METADATA)?;
        let key = doc_id.as_bytes().to_vec();
        Ok(self.db.get_cf(&cf, &key)?.is_some())
    }

    // ─── Deltas ───────────────────────────────────────────────────────

    /// Store a compressed delta for a document.
    ///
    /// Key format: `<doc_id:16 bytes><version:8 bytes big-endian>`
    /// Value: LZ4-compressed delta payload.
    /// Target: <50μs per delta write.
    pub fn store_delta(
        &self,
        doc_id: Uuid,
        version: u64,
        delta: &[u8],
    ) -> Result<u64, StoreError> {
        let cf_deltas = self.cf(CF_DELTAS)?;
        let cf_meta = self.cf(CF_METADATA)?;

        // LZ4 compress the delta
        let compressed = lz4_flex::compress_prepend_size(delta);
        let compressed_len = compressed.len() as u64;

        // Build key: doc_id (16 bytes) + version (8 bytes BE)
        let key = Self::delta_key(doc_id, version);

        // Update metadata atomically
        let mut meta = self.load_metadata(doc_id).unwrap_or_else(|_| DocumentMetadata::new(doc_id));
        meta.version = version;
        meta.delta_count += 1;
        meta.updated_at = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut batch = WriteBatch::default();
        batch.put_cf(&cf_deltas, &key, &compressed);
        batch.put_cf(&cf_meta, doc_id.as_bytes(), &meta.encode()?);

        let mut write_opts = WriteOptions::default();
        write_opts.set_sync(self.config.sync_writes);
        self.db.write_opt(batch, &write_opts)?;

        Ok(compressed_len)
    }

    /// Load all deltas for a document since a given version.
    ///
    /// Returns deltas in version order, LZ4 decompressed.
    pub fn load_deltas_since(
        &self,
        doc_id: Uuid,
        since_version: u64,
    ) -> Result<Vec<(u64, Vec<u8>)>, StoreError> {
        let cf = self.cf(CF_DELTAS)?;

        let start_key = Self::delta_key(doc_id, since_version);
        let end_key = Self::delta_key(doc_id, u64::MAX);

        let mut deltas = Vec::new();
        let iter = self.db.iterator_cf(
            &cf,
            IteratorMode::From(&start_key, rocksdb::Direction::Forward),
        );

        for item in iter {
            let (key, value) = item.map_err(|e| StoreError::DatabaseError(e.to_string()))?;

            // Stop if we've passed this document's key prefix
            if key.len() < 24 || &key[..16] != doc_id.as_bytes() {
                break;
            }
            if key.as_ref() > end_key.as_slice() {
                break;
            }

            // Extract version from key
            let mut ver_buf = [0u8; 8];
            ver_buf.copy_from_slice(&key[16..24]);
            let version = u64::from_be_bytes(ver_buf);

            // Decompress delta
            let decompressed = lz4_flex::decompress_size_prepended(&value)
                .map_err(|e| StoreError::CompressionError(e.to_string()))?;

            deltas.push((version, decompressed));
        }

        Ok(deltas)
    }

    /// Load all deltas for a document.
    pub fn load_all_deltas(&self, doc_id: Uuid) -> Result<Vec<(u64, Vec<u8>)>, StoreError> {
        self.load_deltas_since(doc_id, 0)
    }

    /// Count deltas stored for a document.
    pub fn delta_count(&self, doc_id: Uuid) -> Result<u64, StoreError> {
        let meta = self.load_metadata(doc_id)?;
        Ok(meta.delta_count)
    }

    /// Delete all deltas for a document up to a version (after snapshot compaction).
    pub fn compact_deltas(
        &self,
        doc_id: Uuid,
        up_to_version: u64,
    ) -> Result<u64, StoreError> {
        let cf = self.cf(CF_DELTAS)?;

        let start_key = Self::delta_key(doc_id, 0);
        let end_key = Self::delta_key(doc_id, up_to_version + 1);

        // Count before deleting
        let mut count = 0u64;
        let iter = self.db.iterator_cf(
            &cf,
            IteratorMode::From(&start_key, rocksdb::Direction::Forward),
        );

        let mut batch = WriteBatch::default();
        for item in iter {
            let (key, _) = item.map_err(|e| StoreError::DatabaseError(e.to_string()))?;
            if key.len() < 24 || &key[..16] != doc_id.as_bytes() {
                break;
            }
            if key.as_ref() >= end_key.as_slice() {
                break;
            }
            batch.delete_cf(&cf, &key);
            count += 1;
        }

        if count > 0 {
            self.db.write(batch)?;
        }

        Ok(count)
    }

    // ─── Metadata ─────────────────────────────────────────────────────

    /// Load document metadata.
    pub fn load_metadata(&self, doc_id: Uuid) -> Result<DocumentMetadata, StoreError> {
        let cf = self.cf(CF_METADATA)?;
        let key = doc_id.as_bytes().to_vec();

        match self.db.get_cf(&cf, &key)? {
            Some(bytes) => DocumentMetadata::decode(&bytes),
            None => Err(StoreError::NotFound(doc_id)),
        }
    }

    /// List all document IDs in the store.
    pub fn list_documents(&self) -> Result<Vec<Uuid>, StoreError> {
        let cf = self.cf(CF_METADATA)?;
        let mut doc_ids = Vec::new();

        let iter = self.db.iterator_cf(&cf, IteratorMode::Start);
        for item in iter {
            let (key, _) = item.map_err(|e| StoreError::DatabaseError(e.to_string()))?;
            if key.len() == 16 {
                let id = Uuid::from_bytes(
                    key.as_ref()
                        .try_into()
                        .map_err(|_| StoreError::DeserializationError("Invalid UUID key".into()))?,
                );
                doc_ids.push(id);
            }
        }

        Ok(doc_ids)
    }

    /// Delete a document and all its deltas/metadata.
    pub fn delete_document(&self, doc_id: Uuid) -> Result<(), StoreError> {
        let cf_docs = self.cf(CF_DOCUMENTS)?;
        let cf_meta = self.cf(CF_METADATA)?;

        let key = doc_id.as_bytes().to_vec();

        let mut batch = WriteBatch::default();
        batch.delete_cf(&cf_docs, &key);
        batch.delete_cf(&cf_meta, &key);

        // Delete all deltas for this doc
        let cf_deltas = self.cf(CF_DELTAS)?;
        let start_key = Self::delta_key(doc_id, 0);
        let end_key = Self::delta_key(doc_id, u64::MAX);

        let iter = self.db.iterator_cf(
            &cf_deltas,
            IteratorMode::From(&start_key, rocksdb::Direction::Forward),
        );
        for item in iter {
            let (key, _) = item.map_err(|e| StoreError::DatabaseError(e.to_string()))?;
            if key.len() < 24 || &key[..16] != doc_id.as_bytes() {
                break;
            }
            if key.as_ref() >= end_key.as_slice() {
                break;
            }
            batch.delete_cf(&cf_deltas, &key);
        }

        self.db.write(batch)?;
        Ok(())
    }

    // ─── WAL Operations ───────────────────────────────────────────────

    /// Append a WAL entry. Returns the sequence number assigned.
    ///
    /// Target: <10μs per append (no fsync, buffered).
    pub fn wal_append(
        &self,
        doc_id: Uuid,
        delta: &[u8],
    ) -> Result<u64, StoreError> {
        let cf = self.cf(CF_WAL)?;
        let seq = self.sequence.fetch_add(1, Ordering::SeqCst);

        // Key: sequence number (8 bytes BE) for sequential ordering
        let key = seq.to_be_bytes();

        // Value: doc_id (16 bytes) + delta (variable)
        let mut value = Vec::with_capacity(16 + delta.len());
        value.extend_from_slice(doc_id.as_bytes());
        value.extend_from_slice(delta);

        let mut write_opts = WriteOptions::default();
        write_opts.set_sync(false); // No fsync per-write — batched by caller
        write_opts.disable_wal(false); // Use RocksDB's own WAL for atomicity

        self.db.put_cf_opt(&cf, key, &value, &write_opts)?;

        Ok(seq)
    }

    /// Read all WAL entries since a given sequence number.
    ///
    /// Used during crash recovery to replay uncommitted deltas.
    pub fn wal_read_since(
        &self,
        since_seq: u64,
    ) -> Result<Vec<(u64, Uuid, Vec<u8>)>, StoreError> {
        let cf = self.cf(CF_WAL)?;
        let start_key = since_seq.to_be_bytes();

        let mut entries = Vec::new();
        let iter = self.db.iterator_cf(
            &cf,
            IteratorMode::From(&start_key, rocksdb::Direction::Forward),
        );

        for item in iter {
            let (key, value) = item.map_err(|e| StoreError::DatabaseError(e.to_string()))?;

            if key.len() < 8 || value.len() < 16 {
                continue;
            }

            let mut seq_buf = [0u8; 8];
            seq_buf.copy_from_slice(&key[..8]);
            let seq = u64::from_be_bytes(seq_buf);

            let doc_id = Uuid::from_bytes(
                value[..16]
                    .try_into()
                    .map_err(|_| StoreError::DeserializationError("Invalid UUID in WAL".into()))?,
            );
            let delta = value[16..].to_vec();

            entries.push((seq, doc_id, delta));
        }

        Ok(entries)
    }

    /// Truncate WAL entries up to a sequence number (after successful compaction).
    pub fn wal_truncate(&self, up_to_seq: u64) -> Result<u64, StoreError> {
        let cf = self.cf(CF_WAL)?;

        let mut count = 0u64;
        let mut batch = WriteBatch::default();

        let iter = self.db.iterator_cf(&cf, IteratorMode::Start);
        for item in iter {
            let (key, _) = item.map_err(|e| StoreError::DatabaseError(e.to_string()))?;
            if key.len() < 8 {
                continue;
            }

            let mut seq_buf = [0u8; 8];
            seq_buf.copy_from_slice(&key[..8]);
            let seq = u64::from_be_bytes(seq_buf);

            if seq > up_to_seq {
                break;
            }

            batch.delete_cf(&cf, &key);
            count += 1;
        }

        if count > 0 {
            self.db.write(batch)?;
        }

        Ok(count)
    }

    /// Force fsync on the database (called periodically, e.g., every 1 second).
    pub fn sync(&self) -> Result<(), StoreError> {
        self.db.flush().map_err(|e| StoreError::DatabaseError(e.to_string()))?;
        Ok(())
    }

    /// Get the current WAL sequence number.
    pub fn wal_sequence(&self) -> u64 {
        self.sequence.load(Ordering::SeqCst)
    }

    /// Get the database path.
    pub fn path(&self) -> &Path {
        &self.config.path
    }

    // ─── Helpers ──────────────────────────────────────────────────────

    /// Get a column family handle.
    fn cf(&self, name: &str) -> Result<&rocksdb::ColumnFamily, StoreError> {
        self.db
            .cf_handle(name)
            .ok_or_else(|| StoreError::DatabaseError(format!("Column family '{name}' not found")))
    }

    /// Build a delta key: doc_id (16 bytes) + version (8 bytes big-endian).
    fn delta_key(doc_id: Uuid, version: u64) -> Vec<u8> {
        let mut key = Vec::with_capacity(24);
        key.extend_from_slice(doc_id.as_bytes());
        key.extend_from_slice(&version.to_be_bytes());
        key
    }
}

/// Get number of CPU cores for RocksDB parallelism.
fn num_cpus() -> i32 {
    std::thread::available_parallelism()
        .map(|n| n.get() as i32)
        .unwrap_or(4)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Create a temp directory for test database.
    fn temp_db_path(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("logos_test_rocks_{name}_{}", Uuid::new_v4()));
        path
    }

    /// Clean up test database.
    fn cleanup(path: &Path) {
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn test_store_open_close() {
        let path = temp_db_path("open_close");
        let config = StoreConfig::for_testing(&path);
        let store = DocumentStore::open(config).unwrap();
        assert!(store.path().exists());
        drop(store);
        cleanup(&path);
    }

    #[test]
    fn test_snapshot_save_load() {
        let path = temp_db_path("snapshot");
        let config = StoreConfig::for_testing(&path);
        let store = DocumentStore::open(config).unwrap();

        let doc_id = Uuid::new_v4();
        let data = b"Hello, Logos! This is a test document snapshot with enough data to compress.".to_vec();

        let meta = store.save_snapshot(doc_id, &data).unwrap();
        assert_eq!(meta.doc_id, doc_id);
        assert_eq!(meta.snapshot_size, data.len() as u64);
        assert!(meta.compressed_size > 0);

        let loaded = store.load_snapshot(doc_id).unwrap();
        assert_eq!(loaded, data);

        drop(store);
        cleanup(&path);
    }

    #[test]
    fn test_snapshot_not_found() {
        let path = temp_db_path("not_found");
        let config = StoreConfig::for_testing(&path);
        let store = DocumentStore::open(config).unwrap();

        let result = store.load_snapshot(Uuid::new_v4());
        assert!(result.is_err());

        drop(store);
        cleanup(&path);
    }

    #[test]
    fn test_delta_store_load() {
        let path = temp_db_path("delta");
        let config = StoreConfig::for_testing(&path);
        let store = DocumentStore::open(config).unwrap();

        let doc_id = Uuid::new_v4();

        // Store 10 deltas
        for v in 1..=10 {
            let delta = format!("delta_{v}").into_bytes();
            store.store_delta(doc_id, v, &delta).unwrap();
        }

        // Load all
        let all = store.load_all_deltas(doc_id).unwrap();
        assert_eq!(all.len(), 10);
        assert_eq!(all[0].0, 1);
        assert_eq!(all[0].1, b"delta_1");
        assert_eq!(all[9].0, 10);
        assert_eq!(all[9].1, b"delta_10");

        // Load since version 5
        let since5 = store.load_deltas_since(doc_id, 5).unwrap();
        assert_eq!(since5.len(), 6); // versions 5..=10
        assert_eq!(since5[0].0, 5);

        drop(store);
        cleanup(&path);
    }

    #[test]
    fn test_delta_compact() {
        let path = temp_db_path("compact");
        let config = StoreConfig::for_testing(&path);
        let store = DocumentStore::open(config).unwrap();

        let doc_id = Uuid::new_v4();
        for v in 1..=20 {
            store.store_delta(doc_id, v, &vec![v as u8; 64]).unwrap();
        }

        // Compact deltas 1..=10
        let removed = store.compact_deltas(doc_id, 10).unwrap();
        assert_eq!(removed, 10);

        // Only 11..=20 remain
        let remaining = store.load_all_deltas(doc_id).unwrap();
        assert_eq!(remaining.len(), 10);
        assert_eq!(remaining[0].0, 11);

        drop(store);
        cleanup(&path);
    }

    #[test]
    fn test_document_exists() {
        let path = temp_db_path("exists");
        let config = StoreConfig::for_testing(&path);
        let store = DocumentStore::open(config).unwrap();

        let doc_id = Uuid::new_v4();
        assert!(!store.document_exists(doc_id).unwrap());

        store.save_snapshot(doc_id, b"data").unwrap();
        assert!(store.document_exists(doc_id).unwrap());

        drop(store);
        cleanup(&path);
    }

    #[test]
    fn test_list_documents() {
        let path = temp_db_path("list");
        let config = StoreConfig::for_testing(&path);
        let store = DocumentStore::open(config).unwrap();

        let ids: Vec<Uuid> = (0..5).map(|_| Uuid::new_v4()).collect();
        for id in &ids {
            store.save_snapshot(*id, b"test").unwrap();
        }

        let listed = store.list_documents().unwrap();
        assert_eq!(listed.len(), 5);
        for id in &ids {
            assert!(listed.contains(id));
        }

        drop(store);
        cleanup(&path);
    }

    #[test]
    fn test_delete_document() {
        let path = temp_db_path("delete");
        let config = StoreConfig::for_testing(&path);
        let store = DocumentStore::open(config).unwrap();

        let doc_id = Uuid::new_v4();
        store.save_snapshot(doc_id, b"data").unwrap();
        store.store_delta(doc_id, 1, b"delta1").unwrap();
        store.store_delta(doc_id, 2, b"delta2").unwrap();

        assert!(store.document_exists(doc_id).unwrap());

        store.delete_document(doc_id).unwrap();
        assert!(!store.document_exists(doc_id).unwrap());
        assert!(store.load_snapshot(doc_id).is_err());
        assert_eq!(store.load_all_deltas(doc_id).unwrap().len(), 0);

        drop(store);
        cleanup(&path);
    }

    #[test]
    fn test_wal_append_read() {
        let path = temp_db_path("wal");
        let config = StoreConfig::for_testing(&path);
        let store = DocumentStore::open(config).unwrap();

        let doc_id = Uuid::new_v4();
        let seq1 = store.wal_append(doc_id, b"delta_1").unwrap();
        let seq2 = store.wal_append(doc_id, b"delta_2").unwrap();
        let seq3 = store.wal_append(doc_id, b"delta_3").unwrap();

        assert_eq!(seq1 + 1, seq2);
        assert_eq!(seq2 + 1, seq3);

        let entries = store.wal_read_since(0).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].1, doc_id);
        assert_eq!(entries[0].2, b"delta_1");
        assert_eq!(entries[2].2, b"delta_3");

        drop(store);
        cleanup(&path);
    }

    #[test]
    fn test_wal_truncate() {
        let path = temp_db_path("wal_trunc");
        let config = StoreConfig::for_testing(&path);
        let store = DocumentStore::open(config).unwrap();

        let doc_id = Uuid::new_v4();
        for i in 0..10 {
            store.wal_append(doc_id, format!("d{i}").as_bytes()).unwrap();
        }

        // Truncate first 5
        let removed = store.wal_truncate(4).unwrap();
        assert_eq!(removed, 5);

        let remaining = store.wal_read_since(0).unwrap();
        assert_eq!(remaining.len(), 5);
        assert_eq!(remaining[0].0, 5);

        drop(store);
        cleanup(&path);
    }

    #[test]
    fn test_wal_sequence_recovery() {
        let path = temp_db_path("wal_recovery");
        let config = StoreConfig::for_testing(path.clone());

        // Write some WAL entries
        {
            let store = DocumentStore::open(config.clone()).unwrap();
            let doc_id = Uuid::new_v4();
            store.wal_append(doc_id, b"a").unwrap();
            store.wal_append(doc_id, b"b").unwrap();
            store.wal_append(doc_id, b"c").unwrap();
            assert_eq!(store.wal_sequence(), 3);
        }

        // Reopen — sequence should continue from 3
        {
            let store = DocumentStore::open(config).unwrap();
            assert_eq!(store.wal_sequence(), 3);
            let doc_id = Uuid::new_v4();
            let seq = store.wal_append(doc_id, b"d").unwrap();
            assert_eq!(seq, 3); // Next sequence after 0,1,2
        }

        cleanup(&path);
    }

    #[test]
    fn test_compression_ratio() {
        let path = temp_db_path("compression");
        let config = StoreConfig::for_testing(&path);
        let store = DocumentStore::open(config).unwrap();

        let doc_id = Uuid::new_v4();

        // Create repetitive data (typical for CRDT deltas — lots of zeros and structure)
        let mut data = Vec::with_capacity(10_000);
        for i in 0..1000 {
            data.extend_from_slice(&[0u8; 6]); // zeros
            data.extend_from_slice(&(i as u16).to_le_bytes()); // structure
            data.extend_from_slice(b"tx"); // repeated pattern
        }

        let meta = store.save_snapshot(doc_id, &data).unwrap();
        let ratio = meta.snapshot_size as f64 / meta.compressed_size as f64;

        // LZ4 on structured/repetitive data should achieve >2x easily
        assert!(
            ratio > 2.0,
            "Compression ratio {ratio:.1}x too low (expected >2x)"
        );

        drop(store);
        cleanup(&path);
    }

    #[test]
    fn test_metadata_persistence() {
        let path = temp_db_path("metadata");
        let config = StoreConfig::for_testing(&path);
        let store = DocumentStore::open(config).unwrap();

        let doc_id = Uuid::new_v4();
        store.save_snapshot(doc_id, b"snapshot_data").unwrap();
        store.store_delta(doc_id, 1, b"delta_1").unwrap();
        store.store_delta(doc_id, 2, b"delta_2").unwrap();

        let meta = store.load_metadata(doc_id).unwrap();
        assert_eq!(meta.doc_id, doc_id);
        assert_eq!(meta.version, 2);
        assert_eq!(meta.delta_count, 2);
        assert!(meta.created_at > 0);
        assert!(meta.updated_at >= meta.created_at);

        drop(store);
        cleanup(&path);
    }

    #[test]
    fn test_multiple_documents_isolation() {
        let path = temp_db_path("isolation");
        let config = StoreConfig::for_testing(&path);
        let store = DocumentStore::open(config).unwrap();

        let doc_a = Uuid::new_v4();
        let doc_b = Uuid::new_v4();

        store.save_snapshot(doc_a, b"snapshot_a").unwrap();
        store.save_snapshot(doc_b, b"snapshot_b").unwrap();

        for v in 1..=5 {
            store.store_delta(doc_a, v, format!("a_{v}").as_bytes()).unwrap();
        }
        for v in 1..=3 {
            store.store_delta(doc_b, v, format!("b_{v}").as_bytes()).unwrap();
        }

        assert_eq!(store.load_snapshot(doc_a).unwrap(), b"snapshot_a");
        assert_eq!(store.load_snapshot(doc_b).unwrap(), b"snapshot_b");

        let deltas_a = store.load_all_deltas(doc_a).unwrap();
        let deltas_b = store.load_all_deltas(doc_b).unwrap();
        assert_eq!(deltas_a.len(), 5);
        assert_eq!(deltas_b.len(), 3);
        assert_eq!(deltas_a[0].1, b"a_1");
        assert_eq!(deltas_b[0].1, b"b_1");

        drop(store);
        cleanup(&path);
    }

    #[test]
    fn test_large_snapshot() {
        let path = temp_db_path("large");
        let config = StoreConfig::for_testing(&path);
        let store = DocumentStore::open(config).unwrap();

        let doc_id = Uuid::new_v4();
        // 1MB snapshot
        let data = vec![42u8; 1_000_000];

        let meta = store.save_snapshot(doc_id, &data).unwrap();
        assert_eq!(meta.snapshot_size, 1_000_000);
        // Uniform data compresses extremely well
        assert!(meta.compressed_size < 100_000);

        let loaded = store.load_snapshot(doc_id).unwrap();
        assert_eq!(loaded.len(), 1_000_000);
        assert_eq!(loaded[0], 42);
        assert_eq!(loaded[999_999], 42);

        drop(store);
        cleanup(&path);
    }

    #[test]
    fn test_store_config_default() {
        let config = StoreConfig::default();
        assert_eq!(config.block_cache_size, 256 * 1024 * 1024);
        assert_eq!(config.bloom_filter_bits, 10);
        assert!(!config.sync_writes);
    }

    #[test]
    fn test_store_error_display() {
        let err = StoreError::NotFound(Uuid::nil());
        assert!(err.to_string().contains("not found"));

        let err = StoreError::DatabaseError("test".into());
        assert!(err.to_string().contains("Database error"));
    }
}
