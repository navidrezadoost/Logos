//! Write-ahead log for crash-safe delta persistence.
//!
//! Architecture:
//! ```text
//! ┌──────────────────────────────────────────────┐
//! │              WriteAheadLog                    │
//! │                                              │
//! │  Buffer: [ entry | entry | entry | ... ]     │
//! │                                              │
//! │  Flush when:                                 │
//! │    1. Buffer exceeds 64KB (batch flush)      │
//! │    2. sync() called (periodic, every 1s)     │
//! │    3. Explicit flush_all()                   │
//! │                                              │
//! │  Recovery: replay unflushed entries on start │
//! └──────────────────────────────────────────────┘
//! ```
//!
//! Performance targets:
//! - Append latency: <10μs (buffered, no I/O)
//! - Batch flush: <2ms for 64KB
//! - fsync: <2ms (but only every 1s)
//! - Recovery (10k entries): <100ms
//!
//! Reference: Kleppmann — DDIA, Chapter 3 (Write-Ahead Logs)
//! Reference: Patterson & Hennessy — Section 5.7 (Sequential I/O)

use serde::{Deserialize, Serialize};
use std::time::Instant;
use uuid::Uuid;

/// WAL entry type tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum WalEntryType {
    /// CRDT delta update
    Delta = 1,
    /// Full document snapshot
    Snapshot = 2,
    /// Checkpoint marker (all prior entries are durable)
    Checkpoint = 3,
}

/// A single WAL entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalEntry {
    /// Monotonically increasing sequence number
    pub sequence: u64,
    /// Entry type
    pub entry_type: WalEntryType,
    /// Document ID this entry belongs to
    pub doc_id: Uuid,
    /// Payload (delta or snapshot bytes)
    pub payload: Vec<u8>,
    /// CRC32 checksum for integrity verification
    pub checksum: u32,
}

impl WalEntry {
    /// Create a new WAL entry with computed checksum.
    pub fn new(sequence: u64, entry_type: WalEntryType, doc_id: Uuid, payload: Vec<u8>) -> Self {
        let checksum = Self::compute_checksum(sequence, entry_type, &doc_id, &payload);
        Self {
            sequence,
            entry_type,
            doc_id,
            payload,
            checksum,
        }
    }

    /// Verify the entry's checksum.
    pub fn verify(&self) -> bool {
        let expected = Self::compute_checksum(
            self.sequence,
            self.entry_type,
            &self.doc_id,
            &self.payload,
        );
        self.checksum == expected
    }

    /// Compute CRC32 checksum over entry fields.
    fn compute_checksum(
        sequence: u64,
        entry_type: WalEntryType,
        doc_id: &Uuid,
        payload: &[u8],
    ) -> u32 {
        // Simple but effective: XOR-fold all fields
        let mut hash: u32 = 0x811c_9dc5; // FNV offset basis
        // Mix sequence
        hash ^= sequence as u32;
        hash = hash.wrapping_mul(0x0100_0193); // FNV prime
        hash ^= (sequence >> 32) as u32;
        hash = hash.wrapping_mul(0x0100_0193);
        // Mix entry type
        hash ^= entry_type as u32;
        hash = hash.wrapping_mul(0x0100_0193);
        // Mix doc_id
        for byte in doc_id.as_bytes() {
            hash ^= *byte as u32;
            hash = hash.wrapping_mul(0x0100_0193);
        }
        // Mix payload
        for chunk in payload.chunks(4) {
            let mut word = [0u8; 4];
            word[..chunk.len()].copy_from_slice(chunk);
            hash ^= u32::from_le_bytes(word);
            hash = hash.wrapping_mul(0x0100_0193);
        }
        hash
    }

    /// Serialize entry to bytes.
    pub fn encode(&self) -> Result<Vec<u8>, WalError> {
        bincode::serde::encode_to_vec(self, bincode::config::standard())
            .map_err(|e| WalError::SerializationError(e.to_string()))
    }

    /// Deserialize entry from bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, WalError> {
        let (entry, _): (Self, _) =
            bincode::serde::decode_from_slice(bytes, bincode::config::standard())
                .map_err(|e| WalError::DeserializationError(e.to_string()))?;
        Ok(entry)
    }

    /// Serialized size in bytes.
    pub fn encoded_size(&self) -> usize {
        // Approximate: fixed fields + payload
        8 + 1 + 16 + 4 + self.payload.len() + 8 // bincode overhead
    }
}

/// WAL configuration.
#[derive(Debug, Clone)]
pub struct WalConfig {
    /// Buffer size before auto-flush (bytes). Default: 64KB.
    pub flush_threshold: usize,
    /// Maximum entries before auto-flush. Default: 1000.
    pub max_buffered_entries: usize,
    /// Sync interval hint (caller responsible for periodic sync). Default: 1s.
    pub sync_interval_ms: u64,
}

impl Default for WalConfig {
    fn default() -> Self {
        Self {
            flush_threshold: 64 * 1024, // 64KB
            max_buffered_entries: 1000,
            sync_interval_ms: 1000, // 1 second
        }
    }
}

impl WalConfig {
    /// Config for testing (small buffers, immediate flush).
    pub fn for_testing() -> Self {
        Self {
            flush_threshold: 1024, // 1KB
            max_buffered_entries: 10,
            sync_interval_ms: 100,
        }
    }
}

/// WAL errors.
#[derive(Debug, Clone)]
pub enum WalError {
    /// Serialization failed
    SerializationError(String),
    /// Deserialization failed
    DeserializationError(String),
    /// Buffer overflow
    BufferOverflow,
    /// Checksum verification failed
    ChecksumMismatch { sequence: u64 },
    /// WAL is closed
    Closed,
}

impl std::fmt::Display for WalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WalError::SerializationError(e) => write!(f, "WAL serialization error: {e}"),
            WalError::DeserializationError(e) => write!(f, "WAL deserialization error: {e}"),
            WalError::BufferOverflow => write!(f, "WAL buffer overflow"),
            WalError::ChecksumMismatch { sequence } => {
                write!(f, "WAL checksum mismatch at sequence {sequence}")
            }
            WalError::Closed => write!(f, "WAL is closed"),
        }
    }
}

impl std::error::Error for WalError {}

/// In-memory write-ahead log with batched flushing.
///
/// Entries are buffered in memory for fast append (<10μs).
/// The buffer is flushed when:
/// 1. Buffer size exceeds `flush_threshold` (64KB default)
/// 2. Entry count exceeds `max_buffered_entries` (1000 default)
/// 3. Caller explicitly calls `flush()` or `sync()`
///
/// The flushed entries are returned to the caller for persistence
/// (e.g., writing to DocumentStore's WAL column family).
pub struct WriteAheadLog {
    /// Configuration
    config: WalConfig,
    /// Buffered entries (not yet flushed)
    buffer: Vec<WalEntry>,
    /// Current buffer size in bytes (approximate)
    buffer_bytes: usize,
    /// Next sequence number
    next_sequence: u64,
    /// Total entries appended since creation
    total_appended: u64,
    /// Total entries flushed since creation
    total_flushed: u64,
    /// Last flush timestamp
    last_flush: Instant,
    /// Whether the WAL is accepting writes
    open: bool,
}

impl WriteAheadLog {
    /// Create a new WAL with the given configuration.
    pub fn new(config: WalConfig) -> Self {
        Self {
            buffer: Vec::with_capacity(config.max_buffered_entries),
            buffer_bytes: 0,
            next_sequence: 0,
            total_appended: 0,
            total_flushed: 0,
            last_flush: Instant::now(),
            open: true,
            config,
        }
    }

    /// Create with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(WalConfig::default())
    }

    /// Create starting from a given sequence number (for recovery).
    pub fn from_sequence(config: WalConfig, start_sequence: u64) -> Self {
        let mut wal = Self::new(config);
        wal.next_sequence = start_sequence;
        wal
    }

    /// Append a delta entry to the WAL buffer.
    ///
    /// Target: <10μs (buffer append, no I/O).
    /// Returns the sequence number assigned and whether flush is needed.
    pub fn append_delta(
        &mut self,
        doc_id: Uuid,
        delta: Vec<u8>,
    ) -> Result<(u64, bool), WalError> {
        self.append_entry(WalEntryType::Delta, doc_id, delta)
    }

    /// Append a snapshot entry to the WAL buffer.
    pub fn append_snapshot(
        &mut self,
        doc_id: Uuid,
        snapshot: Vec<u8>,
    ) -> Result<(u64, bool), WalError> {
        self.append_entry(WalEntryType::Snapshot, doc_id, snapshot)
    }

    /// Append a checkpoint marker.
    pub fn append_checkpoint(&mut self, doc_id: Uuid) -> Result<(u64, bool), WalError> {
        self.append_entry(WalEntryType::Checkpoint, doc_id, Vec::new())
    }

    /// Core append logic.
    fn append_entry(
        &mut self,
        entry_type: WalEntryType,
        doc_id: Uuid,
        payload: Vec<u8>,
    ) -> Result<(u64, bool), WalError> {
        if !self.open {
            return Err(WalError::Closed);
        }

        let seq = self.next_sequence;
        self.next_sequence += 1;

        let entry = WalEntry::new(seq, entry_type, doc_id, payload);
        self.buffer_bytes += entry.encoded_size();
        self.buffer.push(entry);
        self.total_appended += 1;

        let needs_flush = self.should_flush();
        Ok((seq, needs_flush))
    }

    /// Check if the buffer should be flushed.
    fn should_flush(&self) -> bool {
        self.buffer_bytes >= self.config.flush_threshold
            || self.buffer.len() >= self.config.max_buffered_entries
    }

    /// Flush all buffered entries, returning them for persistence.
    ///
    /// After this call, the buffer is empty.
    /// The caller is responsible for writing entries to durable storage.
    pub fn flush(&mut self) -> Vec<WalEntry> {
        if self.buffer.is_empty() {
            return Vec::new();
        }

        let entries = std::mem::take(&mut self.buffer);
        let count = entries.len() as u64;
        self.buffer_bytes = 0;
        self.total_flushed += count;
        self.last_flush = Instant::now();

        self.buffer = Vec::with_capacity(self.config.max_buffered_entries);

        entries
    }

    /// Check if a flush is needed based on thresholds.
    pub fn needs_flush(&self) -> bool {
        self.should_flush()
    }

    /// Check if a sync (periodic fsync) is due based on time.
    pub fn needs_sync(&self) -> bool {
        self.last_flush.elapsed().as_millis() as u64 >= self.config.sync_interval_ms
    }

    /// Get the number of buffered entries.
    pub fn buffered_count(&self) -> usize {
        self.buffer.len()
    }

    /// Get the approximate buffered size in bytes.
    pub fn buffered_bytes(&self) -> usize {
        self.buffer_bytes
    }

    /// Get the next sequence number that will be assigned.
    pub fn next_sequence(&self) -> u64 {
        self.next_sequence
    }

    /// Get total entries appended since creation.
    pub fn total_appended(&self) -> u64 {
        self.total_appended
    }

    /// Get total entries flushed since creation.
    pub fn total_flushed(&self) -> u64 {
        self.total_flushed
    }

    /// Get the time since last flush.
    pub fn time_since_flush(&self) -> std::time::Duration {
        self.last_flush.elapsed()
    }

    /// Close the WAL (no more writes accepted).
    pub fn close(&mut self) -> Vec<WalEntry> {
        self.open = false;
        self.flush()
    }

    /// Whether the WAL is open for writes.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Recover entries from serialized bytes.
    ///
    /// Verifies checksums and skips corrupted entries.
    /// Returns valid entries and count of corrupted entries skipped.
    pub fn recover_entries(serialized: &[Vec<u8>]) -> (Vec<WalEntry>, usize) {
        let mut valid = Vec::with_capacity(serialized.len());
        let mut corrupted = 0;

        for bytes in serialized {
            match WalEntry::decode(bytes) {
                Ok(entry) => {
                    if entry.verify() {
                        valid.push(entry);
                    } else {
                        corrupted += 1;
                    }
                }
                Err(_) => {
                    corrupted += 1;
                }
            }
        }

        // Sort by sequence number for replay order
        valid.sort_by_key(|e| e.sequence);

        (valid, corrupted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wal_entry_create_verify() {
        let entry = WalEntry::new(1, WalEntryType::Delta, Uuid::new_v4(), b"test_delta".to_vec());
        assert_eq!(entry.sequence, 1);
        assert_eq!(entry.entry_type, WalEntryType::Delta);
        assert!(entry.verify());
    }

    #[test]
    fn test_wal_entry_checksum_integrity() {
        let entry = WalEntry::new(42, WalEntryType::Snapshot, Uuid::new_v4(), vec![1, 2, 3, 4]);
        assert!(entry.verify());

        // Corrupt the payload
        let mut corrupted = entry.clone();
        corrupted.payload[0] = 255;
        assert!(!corrupted.verify());

        // Corrupt the sequence
        let mut corrupted = entry.clone();
        corrupted.sequence = 99;
        assert!(!corrupted.verify());
    }

    #[test]
    fn test_wal_entry_encode_decode() {
        let entry = WalEntry::new(5, WalEntryType::Delta, Uuid::new_v4(), b"payload".to_vec());
        let encoded = entry.encode().unwrap();
        let decoded = WalEntry::decode(&encoded).unwrap();

        assert_eq!(decoded.sequence, entry.sequence);
        assert_eq!(decoded.entry_type, entry.entry_type);
        assert_eq!(decoded.doc_id, entry.doc_id);
        assert_eq!(decoded.payload, entry.payload);
        assert_eq!(decoded.checksum, entry.checksum);
        assert!(decoded.verify());
    }

    #[test]
    fn test_wal_append_delta() {
        let config = WalConfig::for_testing();
        let mut wal = WriteAheadLog::new(config);

        let doc_id = Uuid::new_v4();
        let (seq, _flush) = wal.append_delta(doc_id, b"delta_1".to_vec()).unwrap();
        assert_eq!(seq, 0);
        assert_eq!(wal.buffered_count(), 1);
        assert_eq!(wal.next_sequence(), 1);
    }

    #[test]
    fn test_wal_append_multiple_types() {
        let config = WalConfig {
            flush_threshold: 1024 * 1024, // Large threshold
            max_buffered_entries: 1000,
            sync_interval_ms: 1000,
        };
        let mut wal = WriteAheadLog::new(config);
        let doc_id = Uuid::new_v4();

        let (s1, _) = wal.append_delta(doc_id, b"delta".to_vec()).unwrap();
        let (s2, _) = wal.append_snapshot(doc_id, b"snapshot".to_vec()).unwrap();
        let (s3, _) = wal.append_checkpoint(doc_id).unwrap();

        assert_eq!(s1, 0);
        assert_eq!(s2, 1);
        assert_eq!(s3, 2);
        assert_eq!(wal.buffered_count(), 3);
    }

    #[test]
    fn test_wal_flush() {
        let config = WalConfig {
            flush_threshold: 1024 * 1024,
            max_buffered_entries: 1000,
            sync_interval_ms: 1000,
        };
        let mut wal = WriteAheadLog::new(config);
        let doc_id = Uuid::new_v4();

        for i in 0..5 {
            wal.append_delta(doc_id, format!("delta_{i}").into_bytes()).unwrap();
        }
        assert_eq!(wal.buffered_count(), 5);

        let flushed = wal.flush();
        assert_eq!(flushed.len(), 5);
        assert_eq!(wal.buffered_count(), 0);
        assert_eq!(wal.total_flushed(), 5);
        assert_eq!(wal.total_appended(), 5);

        // Verify entries are correct
        for (i, entry) in flushed.iter().enumerate() {
            assert_eq!(entry.sequence, i as u64);
            assert!(entry.verify());
            assert_eq!(entry.entry_type, WalEntryType::Delta);
        }
    }

    #[test]
    fn test_wal_auto_flush_threshold() {
        let config = WalConfig {
            flush_threshold: 100, // Very small: trigger on size
            max_buffered_entries: 1000,
            sync_interval_ms: 1000,
        };
        let mut wal = WriteAheadLog::new(config);
        let doc_id = Uuid::new_v4();

        // Append until flush is needed
        let mut flush_needed = false;
        for _ in 0..20 {
            let (_, needs) = wal.append_delta(doc_id, vec![0u8; 32]).unwrap();
            if needs {
                flush_needed = true;
                break;
            }
        }
        assert!(flush_needed, "Flush should be triggered by size threshold");
    }

    #[test]
    fn test_wal_auto_flush_count() {
        let config = WalConfig {
            flush_threshold: 1024 * 1024,
            max_buffered_entries: 5, // Small count threshold
            sync_interval_ms: 1000,
        };
        let mut wal = WriteAheadLog::new(config);
        let doc_id = Uuid::new_v4();

        for i in 0..4 {
            let (_, needs) = wal.append_delta(doc_id, format!("d{i}").into_bytes()).unwrap();
            assert!(!needs);
        }

        // 5th entry should trigger flush flag
        let (_, needs) = wal.append_delta(doc_id, b"d4".to_vec()).unwrap();
        assert!(needs);
    }

    #[test]
    fn test_wal_close() {
        let mut wal = WriteAheadLog::with_defaults();
        let doc_id = Uuid::new_v4();

        wal.append_delta(doc_id, b"data".to_vec()).unwrap();
        assert!(wal.is_open());

        let remaining = wal.close();
        assert_eq!(remaining.len(), 1);
        assert!(!wal.is_open());

        // Further appends should fail
        let result = wal.append_delta(doc_id, b"more".to_vec());
        assert!(result.is_err());
    }

    #[test]
    fn test_wal_from_sequence() {
        let config = WalConfig::for_testing();
        let mut wal = WriteAheadLog::from_sequence(config, 100);

        let doc_id = Uuid::new_v4();
        let (seq, _) = wal.append_delta(doc_id, b"data".to_vec()).unwrap();
        assert_eq!(seq, 100);
        assert_eq!(wal.next_sequence(), 101);
    }

    #[test]
    fn test_wal_recover_entries() {
        // Create and encode some entries
        let entries: Vec<Vec<u8>> = (0..5)
            .map(|i| {
                let entry = WalEntry::new(i, WalEntryType::Delta, Uuid::new_v4(), vec![i as u8; 10]);
                entry.encode().unwrap()
            })
            .collect();

        let (recovered, corrupted) = WriteAheadLog::recover_entries(&entries);
        assert_eq!(recovered.len(), 5);
        assert_eq!(corrupted, 0);

        // Verify order
        for (i, entry) in recovered.iter().enumerate() {
            assert_eq!(entry.sequence, i as u64);
            assert!(entry.verify());
        }
    }

    #[test]
    fn test_wal_recover_with_corruption() {
        let mut entries: Vec<Vec<u8>> = (0..5)
            .map(|i| {
                let entry = WalEntry::new(i, WalEntryType::Delta, Uuid::new_v4(), vec![i as u8; 10]);
                entry.encode().unwrap()
            })
            .collect();

        // Corrupt entry #2
        entries[2] = vec![0xFF; 10]; // Garbage data

        let (recovered, corrupted) = WriteAheadLog::recover_entries(&entries);
        assert_eq!(recovered.len(), 4);
        assert_eq!(corrupted, 1);
    }

    #[test]
    fn test_wal_empty_flush() {
        let mut wal = WriteAheadLog::with_defaults();
        let flushed = wal.flush();
        assert!(flushed.is_empty());
    }

    #[test]
    fn test_wal_buffered_bytes_tracking() {
        let config = WalConfig {
            flush_threshold: 1024 * 1024,
            max_buffered_entries: 1000,
            sync_interval_ms: 1000,
        };
        let mut wal = WriteAheadLog::new(config);
        let doc_id = Uuid::new_v4();

        assert_eq!(wal.buffered_bytes(), 0);

        wal.append_delta(doc_id, vec![0u8; 100]).unwrap();
        assert!(wal.buffered_bytes() > 100);

        wal.flush();
        assert_eq!(wal.buffered_bytes(), 0);
    }

    #[test]
    fn test_wal_multiple_documents() {
        let config = WalConfig {
            flush_threshold: 1024 * 1024,
            max_buffered_entries: 1000,
            sync_interval_ms: 1000,
        };
        let mut wal = WriteAheadLog::new(config);

        let doc_a = Uuid::new_v4();
        let doc_b = Uuid::new_v4();

        wal.append_delta(doc_a, b"delta_a_1".to_vec()).unwrap();
        wal.append_delta(doc_b, b"delta_b_1".to_vec()).unwrap();
        wal.append_delta(doc_a, b"delta_a_2".to_vec()).unwrap();

        let flushed = wal.flush();
        assert_eq!(flushed.len(), 3);

        let a_entries: Vec<_> = flushed.iter().filter(|e| e.doc_id == doc_a).collect();
        let b_entries: Vec<_> = flushed.iter().filter(|e| e.doc_id == doc_b).collect();
        assert_eq!(a_entries.len(), 2);
        assert_eq!(b_entries.len(), 1);
    }

    #[test]
    fn test_wal_config_default() {
        let config = WalConfig::default();
        assert_eq!(config.flush_threshold, 64 * 1024);
        assert_eq!(config.max_buffered_entries, 1000);
        assert_eq!(config.sync_interval_ms, 1000);
    }

    #[test]
    fn test_wal_error_display() {
        let err = WalError::BufferOverflow;
        assert!(err.to_string().contains("overflow"));

        let err = WalError::ChecksumMismatch { sequence: 42 };
        assert!(err.to_string().contains("42"));

        let err = WalError::Closed;
        assert!(err.to_string().contains("closed"));
    }

    #[test]
    fn test_wal_entry_types() {
        let doc_id = Uuid::new_v4();

        let delta = WalEntry::new(0, WalEntryType::Delta, doc_id, b"d".to_vec());
        let snap = WalEntry::new(1, WalEntryType::Snapshot, doc_id, b"s".to_vec());
        let ckpt = WalEntry::new(2, WalEntryType::Checkpoint, doc_id, vec![]);

        assert_eq!(delta.entry_type, WalEntryType::Delta);
        assert_eq!(snap.entry_type, WalEntryType::Snapshot);
        assert_eq!(ckpt.entry_type, WalEntryType::Checkpoint);

        assert!(delta.verify());
        assert!(snap.verify());
        assert!(ckpt.verify());
    }

    #[test]
    fn test_wal_sequential_sequences() {
        let mut wal = WriteAheadLog::with_defaults();
        let doc_id = Uuid::new_v4();

        let mut sequences = Vec::new();
        for _ in 0..100 {
            let (seq, _) = wal.append_delta(doc_id, b"x".to_vec()).unwrap();
            sequences.push(seq);
        }

        // Verify monotonically increasing
        for i in 1..sequences.len() {
            assert_eq!(sequences[i], sequences[i - 1] + 1);
        }
    }
}
