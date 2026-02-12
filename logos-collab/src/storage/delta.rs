//! Delta compression and compaction for collaborative documents.
//!
//! Manages incremental CRDT deltas with LZ4 compression and periodic
//! compaction into base snapshots. Designed for efficient storage of
//! edit histories.
//!
//! Architecture:
//! ```text
//! ┌──────────────────────────────────────────┐
//! │              DeltaLog                     │
//! │                                          │
//! │  Base Snapshot ◄── delta ◄── delta ◄── δ │
//! │  (compressed)      (LZ4)    (LZ4)       │
//! │                                          │
//! │  Compaction: merge N deltas → new base   │
//! └──────────────────────────────────────────┘
//! ```
//!
//! Performance targets:
//! - Compress 1KB delta: <10μs (LZ4 block mode)
//! - Decompress 1KB delta: <5μs
//! - Compression ratio: 10:1 on typical CRDT edits
//! - Apply 1000 deltas: <1ms
//!
//! Reference: Patterson & Hennessy — Section 5.7 (Data Compression)
//! Reference: Kleppmann — DDIA, Chapter 3 (Log-Structured Storage)

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A compressed delta entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressedDelta {
    /// Version number (monotonically increasing)
    pub version: u64,
    /// Original uncompressed size in bytes
    pub original_size: u32,
    /// LZ4-compressed payload
    pub compressed: Vec<u8>,
}

impl CompressedDelta {
    /// Create a new compressed delta from raw bytes.
    ///
    /// Uses LZ4 block compression for minimal overhead.
    /// Target: <10μs for 1KB input.
    pub fn compress(version: u64, data: &[u8]) -> Self {
        let compressed = lz4_flex::compress_prepend_size(data);
        Self {
            version,
            original_size: data.len() as u32,
            compressed,
        }
    }

    /// Decompress the delta payload.
    ///
    /// Target: <5μs for 1KB compressed input.
    pub fn decompress(&self) -> Result<Vec<u8>, DeltaError> {
        lz4_flex::decompress_size_prepended(&self.compressed)
            .map_err(|e| DeltaError::DecompressionFailed(e.to_string()))
    }

    /// Compression ratio (original / compressed).
    pub fn compression_ratio(&self) -> f64 {
        if self.compressed.is_empty() {
            return 0.0;
        }
        self.original_size as f64 / self.compressed.len() as f64
    }

    /// Compressed size in bytes.
    pub fn compressed_size(&self) -> usize {
        self.compressed.len()
    }
}

/// Delta compression errors.
#[derive(Debug, Clone)]
pub enum DeltaError {
    /// LZ4 decompression failed
    DecompressionFailed(String),
    /// Delta log is empty
    EmptyLog,
    /// Version mismatch during compaction
    VersionMismatch { expected: u64, got: u64 },
}

impl std::fmt::Display for DeltaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeltaError::DecompressionFailed(e) => write!(f, "Decompression failed: {e}"),
            DeltaError::EmptyLog => write!(f, "Delta log is empty"),
            DeltaError::VersionMismatch { expected, got } => {
                write!(f, "Version mismatch: expected {expected}, got {got}")
            }
        }
    }
}

impl std::error::Error for DeltaError {}

/// Statistics for a delta log.
#[derive(Debug, Clone, Default)]
pub struct DeltaStats {
    /// Number of deltas stored
    pub delta_count: u64,
    /// Total uncompressed size of all deltas
    pub total_original_bytes: u64,
    /// Total compressed size of all deltas
    pub total_compressed_bytes: u64,
    /// Base snapshot version (last compaction point)
    pub base_version: u64,
    /// Current head version
    pub head_version: u64,
}

impl DeltaStats {
    /// Overall compression ratio across all deltas.
    pub fn compression_ratio(&self) -> f64 {
        if self.total_compressed_bytes == 0 {
            return 0.0;
        }
        self.total_original_bytes as f64 / self.total_compressed_bytes as f64
    }
}

/// In-memory delta log with LZ4 compression and periodic compaction.
///
/// Manages a sequence of compressed deltas on top of a base snapshot.
/// When the delta count exceeds a threshold, compaction merges deltas
/// into a new base snapshot.
pub struct DeltaLog {
    /// Document identifier
    doc_id: Uuid,
    /// Base snapshot (full document state, uncompressed)
    base_snapshot: Vec<u8>,
    /// Base snapshot version
    base_version: u64,
    /// Compressed deltas since base snapshot
    deltas: Vec<CompressedDelta>,
    /// Current version counter
    current_version: u64,
    /// Compaction threshold (number of deltas before auto-compact)
    compaction_threshold: usize,
}

impl DeltaLog {
    /// Create a new delta log with an initial base snapshot.
    ///
    /// `compaction_threshold`: number of deltas before triggering compaction.
    /// Default: 100 deltas → compact.
    pub fn new(doc_id: Uuid, base_snapshot: Vec<u8>, compaction_threshold: usize) -> Self {
        Self {
            doc_id,
            base_snapshot,
            base_version: 0,
            deltas: Vec::with_capacity(compaction_threshold),
            current_version: 0,
            compaction_threshold,
        }
    }

    /// Create with default compaction threshold (100 deltas).
    pub fn with_defaults(doc_id: Uuid, base_snapshot: Vec<u8>) -> Self {
        Self::new(doc_id, base_snapshot, 100)
    }

    /// Append a new delta to the log.
    ///
    /// The delta is LZ4-compressed before storage.
    /// Returns `true` if compaction is needed (caller should call `compact`).
    pub fn append(&mut self, delta: &[u8]) -> bool {
        self.current_version += 1;
        let compressed = CompressedDelta::compress(self.current_version, delta);
        self.deltas.push(compressed);
        self.needs_compaction()
    }

    /// Check if compaction is needed.
    pub fn needs_compaction(&self) -> bool {
        self.deltas.len() >= self.compaction_threshold
    }

    /// Compact all deltas into a new base snapshot.
    ///
    /// The `apply_fn` closure receives (base_snapshot, decompressed_delta)
    /// and returns the new document state after applying the delta.
    ///
    /// After compaction, the delta log is reset with the new base.
    pub fn compact<F>(&mut self, apply_fn: F) -> Result<DeltaStats, DeltaError>
    where
        F: Fn(&[u8], &[u8]) -> Vec<u8>,
    {
        if self.deltas.is_empty() {
            return Err(DeltaError::EmptyLog);
        }

        let stats_before = self.stats();

        // Apply all deltas sequentially to build new base
        let mut current = self.base_snapshot.clone();
        for delta in &self.deltas {
            let decompressed = delta.decompress()?;
            current = apply_fn(&current, &decompressed);
        }

        // Update base
        self.base_snapshot = current;
        self.base_version = self.current_version;
        self.deltas.clear();

        Ok(stats_before)
    }

    /// Get the current base snapshot (uncompressed).
    pub fn base_snapshot(&self) -> &[u8] {
        &self.base_snapshot
    }

    /// Get the compressed base snapshot (for persistence).
    pub fn compressed_base_snapshot(&self) -> Vec<u8> {
        lz4_flex::compress_prepend_size(&self.base_snapshot)
    }

    /// Restore from a compressed base snapshot.
    pub fn set_base_from_compressed(&mut self, compressed: &[u8]) -> Result<(), DeltaError> {
        self.base_snapshot = lz4_flex::decompress_size_prepended(compressed)
            .map_err(|e| DeltaError::DecompressionFailed(e.to_string()))?;
        Ok(())
    }

    /// Get all compressed deltas (for persistence).
    pub fn compressed_deltas(&self) -> &[CompressedDelta] {
        &self.deltas
    }

    /// Restore deltas from persistence.
    pub fn restore_deltas(&mut self, deltas: Vec<CompressedDelta>) {
        if let Some(last) = deltas.last() {
            self.current_version = last.version;
        }
        self.deltas = deltas;
    }

    /// Get current version.
    pub fn version(&self) -> u64 {
        self.current_version
    }

    /// Get base version (last compaction point).
    pub fn base_version(&self) -> u64 {
        self.base_version
    }

    /// Get the document ID.
    pub fn doc_id(&self) -> Uuid {
        self.doc_id
    }

    /// Number of pending deltas since last compaction.
    pub fn pending_delta_count(&self) -> usize {
        self.deltas.len()
    }

    /// Get delta log statistics.
    pub fn stats(&self) -> DeltaStats {
        let mut total_original = 0u64;
        let mut total_compressed = 0u64;

        for delta in &self.deltas {
            total_original += delta.original_size as u64;
            total_compressed += delta.compressed.len() as u64;
        }

        DeltaStats {
            delta_count: self.deltas.len() as u64,
            total_original_bytes: total_original,
            total_compressed_bytes: total_compressed,
            base_version: self.base_version,
            head_version: self.current_version,
        }
    }

    /// Decompress all pending deltas (for replay).
    pub fn decompress_all(&self) -> Result<Vec<(u64, Vec<u8>)>, DeltaError> {
        self.deltas
            .iter()
            .map(|d| {
                let data = d.decompress()?;
                Ok((d.version, data))
            })
            .collect()
    }
}

/// Compress raw bytes with LZ4 (standalone utility).
pub fn lz4_compress(data: &[u8]) -> Vec<u8> {
    lz4_flex::compress_prepend_size(data)
}

/// Decompress LZ4 bytes (standalone utility).
pub fn lz4_decompress(compressed: &[u8]) -> Result<Vec<u8>, DeltaError> {
    lz4_flex::decompress_size_prepended(compressed)
        .map_err(|e| DeltaError::DecompressionFailed(e.to_string()))
}

/// Calculate compression ratio for arbitrary data.
pub fn compression_ratio(data: &[u8]) -> f64 {
    let compressed = lz4_compress(data);
    if compressed.is_empty() {
        return 0.0;
    }
    data.len() as f64 / compressed.len() as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compressed_delta_roundtrip() {
        let data = b"Hello, this is a CRDT delta with some structured content!";
        let delta = CompressedDelta::compress(1, data);

        assert_eq!(delta.version, 1);
        assert_eq!(delta.original_size, data.len() as u32);
        assert!(delta.compressed_size() > 0);

        let decompressed = delta.decompress().unwrap();
        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_compression_ratio_structured_data() {
        // Simulate typical CRDT delta: structured, repetitive
        let mut data = Vec::with_capacity(10_000);
        for i in 0..1000 {
            data.extend_from_slice(&[0u8; 6]); // padding/zeros
            data.extend_from_slice(&(i as u16).to_le_bytes());
            data.extend_from_slice(b"op"); // repeated tag
        }

        let delta = CompressedDelta::compress(1, &data);
        let ratio = delta.compression_ratio();

        // Structured data should compress at least 2x with LZ4
        assert!(
            ratio > 2.0,
            "Compression ratio {ratio:.2}x too low (expected >2x for structured data)"
        );
    }

    #[test]
    fn test_compression_ratio_highly_repetitive() {
        // Highly repetitive data (best case for LZ4)
        let data = vec![0u8; 10_000];
        let delta = CompressedDelta::compress(1, &data);
        let ratio = delta.compression_ratio();

        // Uniform data should compress >10x
        assert!(
            ratio > 10.0,
            "Compression ratio {ratio:.2}x too low for uniform data (expected >10x)"
        );
    }

    #[test]
    fn test_delta_log_append() {
        let doc_id = Uuid::new_v4();
        let mut log = DeltaLog::new(doc_id, b"base".to_vec(), 5);

        assert_eq!(log.version(), 0);
        assert_eq!(log.pending_delta_count(), 0);

        // Append 4 deltas (below threshold)
        for _ in 0..4 {
            assert!(!log.append(b"delta_data"));
        }
        assert_eq!(log.version(), 4);
        assert_eq!(log.pending_delta_count(), 4);
        assert!(!log.needs_compaction());

        // 5th delta triggers compaction flag
        assert!(log.append(b"delta_5"));
        assert!(log.needs_compaction());
    }

    #[test]
    fn test_delta_log_compact() {
        let doc_id = Uuid::new_v4();
        let mut log = DeltaLog::new(doc_id, b"base".to_vec(), 3);

        log.append(b"_a");
        log.append(b"_b");
        log.append(b"_c");

        // Compact with simple concatenation apply function
        let stats = log.compact(|base, delta| {
            let mut result = base.to_vec();
            result.extend_from_slice(delta);
            result
        }).unwrap();

        assert_eq!(stats.delta_count, 3);
        assert_eq!(log.pending_delta_count(), 0);
        assert_eq!(log.base_version(), 3);
        assert_eq!(log.base_snapshot(), b"base_a_b_c");
    }

    #[test]
    fn test_delta_log_compact_empty_errors() {
        let doc_id = Uuid::new_v4();
        let mut log = DeltaLog::with_defaults(doc_id, b"base".to_vec());

        let result = log.compact(|base, _delta| base.to_vec());
        assert!(result.is_err());
    }

    #[test]
    fn test_delta_log_stats() {
        let doc_id = Uuid::new_v4();
        let mut log = DeltaLog::new(doc_id, b"base".to_vec(), 100);

        for i in 0..10 {
            log.append(&vec![i; 100]); // 100 bytes each
        }

        let stats = log.stats();
        assert_eq!(stats.delta_count, 10);
        assert_eq!(stats.total_original_bytes, 1000);
        assert!(stats.total_compressed_bytes < 1000); // Must be compressed
        assert!(stats.compression_ratio() > 1.0);
        assert_eq!(stats.head_version, 10);
        assert_eq!(stats.base_version, 0);
    }

    #[test]
    fn test_delta_log_decompress_all() {
        let doc_id = Uuid::new_v4();
        let mut log = DeltaLog::new(doc_id, b"base".to_vec(), 100);

        log.append(b"delta_1");
        log.append(b"delta_2");
        log.append(b"delta_3");

        let all = log.decompress_all().unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0], (1, b"delta_1".to_vec()));
        assert_eq!(all[1], (2, b"delta_2".to_vec()));
        assert_eq!(all[2], (3, b"delta_3".to_vec()));
    }

    #[test]
    fn test_compressed_base_snapshot_roundtrip() {
        let doc_id = Uuid::new_v4();
        let base = b"This is a base snapshot with enough content to test compression properly.";
        let mut log = DeltaLog::with_defaults(doc_id, base.to_vec());

        let compressed = log.compressed_base_snapshot();
        assert!(compressed.len() > 0);

        // Clear and restore
        log.set_base_from_compressed(&compressed).unwrap();
        assert_eq!(log.base_snapshot(), base);
    }

    #[test]
    fn test_restore_deltas() {
        let doc_id = Uuid::new_v4();
        let mut log = DeltaLog::new(doc_id, b"base".to_vec(), 100);

        // Create some compressed deltas
        let deltas: Vec<CompressedDelta> = (1..=5)
            .map(|v| CompressedDelta::compress(v, format!("delta_{v}").as_bytes()))
            .collect();

        log.restore_deltas(deltas);
        assert_eq!(log.version(), 5);
        assert_eq!(log.pending_delta_count(), 5);

        let all = log.decompress_all().unwrap();
        assert_eq!(all[0].1, b"delta_1");
        assert_eq!(all[4].1, b"delta_5");
    }

    #[test]
    fn test_lz4_standalone_compress_decompress() {
        let data = b"Standalone compression test with some repeating content. Repeating content!";
        let compressed = lz4_compress(data);
        let decompressed = lz4_decompress(&compressed).unwrap();
        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_compression_ratio_utility() {
        let data = vec![42u8; 10_000];
        let ratio = compression_ratio(&data);
        assert!(ratio > 10.0, "Ratio {ratio:.2}x too low for uniform data");
    }

    #[test]
    fn test_empty_delta_compress() {
        let delta = CompressedDelta::compress(0, &[]);
        assert_eq!(delta.original_size, 0);
        let decompressed = delta.decompress().unwrap();
        assert!(decompressed.is_empty());
    }

    #[test]
    fn test_many_deltas_compression_efficiency() {
        // Simulate 1000 edit deltas with realistic sizes (100+ bytes each)
        // Real CRDT deltas contain structured data with repetitive patterns
        let doc_id = Uuid::new_v4();
        let mut log = DeltaLog::new(doc_id, b"initial_doc_state".to_vec(), 2000);

        let mut total_original = 0usize;

        for i in 0..1000 {
            // Simulate a realistic CRDT delta: header + repetitive structure
            let mut delta = Vec::new();
            delta.extend_from_slice(&[0u8; 16]); // CRDT header
            delta.extend_from_slice(&(i as u64).to_le_bytes()); // lamport clock
            delta.extend_from_slice(&(i as u32).to_le_bytes()); // position
            // Repetitive operation structure (typical in Yrs updates)
            for _ in 0..4 {
                delta.extend_from_slice(b"insert_content_block_");
                delta.extend_from_slice(&[0u8; 8]); // padding
            }

            total_original += delta.len();
            log.append(&delta);
        }

        let stats = log.stats();
        let total_compressed = stats.total_compressed_bytes as usize;

        // Larger deltas with repetitive content should compress well
        assert!(
            total_compressed < total_original,
            "Compressed ({total_compressed}) should be less than original ({total_original})"
        );
    }

    #[test]
    fn test_delta_error_display() {
        let err = DeltaError::EmptyLog;
        assert!(err.to_string().contains("empty"));

        let err = DeltaError::VersionMismatch { expected: 5, got: 3 };
        assert!(err.to_string().contains("5"));
        assert!(err.to_string().contains("3"));
    }
}
