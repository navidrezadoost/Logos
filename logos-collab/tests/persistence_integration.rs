//! Persistence integration tests — Day 15.
//!
//! Verifies:
//! - Document save/load roundtrip through the full server stack
//! - Crash recovery: kill server, restart, data survives
//! - Delta compression meets 10:1 ratio target
//! - WAL append latency within budget
//! - Multi-document isolation under persistence
//! - Snapshot compaction correctness

use logos_collab::storage::{
    DocumentStore, StoreConfig, DeltaLog, CompressedDelta,
    WriteAheadLog, WalConfig,
};
use logos_collab::server::SyncServer;

use std::time::{Duration, Instant};
use tempfile::tempdir;
use uuid::Uuid;
use yrs::{Doc, Text, Transact, ReadTxn, WriteTxn, GetString};
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Create a Yrs Doc with some text content, return (doc, encoded_state).
fn make_doc_with_text(content: &str) -> (Doc, Vec<u8>) {
    let doc = Doc::new();
    {
        let mut txn = doc.transact_mut();
        let text = txn.get_or_insert_text("content");
        text.insert(&mut txn, 0, content);
    }
    let state = {
        let txn = doc.transact();
        txn.encode_state_as_update_v1(&yrs::StateVector::default())
    };
    (doc, state)
}

/// Generate a delta by modifying a doc.
fn make_delta(doc: &Doc, insert_text: &str) -> Vec<u8> {
    let sv = {
        let txn = doc.transact();
        txn.state_vector().encode_v1()
    };
    {
        let mut txn = doc.transact_mut();
        let text = txn.get_or_insert_text("content");
        let len = text.get_string(&txn).len() as u32;
        text.insert(&mut txn, len, insert_text);
    }
    let state = {
        let txn = doc.transact();
        txn.encode_state_as_update_v1(
            &yrs::StateVector::decode_v1(&sv).unwrap(),
        )
    };
    state
}

/// Generate repetitive text of given approximate byte count for compression testing.
fn repetitive_text(approx_bytes: usize) -> String {
    let pattern = "The quick brown fox jumps over the lazy dog. ";
    pattern.repeat(approx_bytes / pattern.len() + 1)
}

// ─── Document Save/Load Roundtrip ────────────────────────────────────────────

#[test]
fn test_document_roundtrip_via_store() {
    let dir = tempdir().unwrap();
    let config = StoreConfig {
        path: dir.path().join("db"),
        ..StoreConfig::default()
    };
    let store = DocumentStore::open(config).unwrap();

    let doc_id = Uuid::new_v4();
    let (_doc, state) = make_doc_with_text("Hello, persistence world!");

    // Save
    store.save_snapshot(doc_id, &state).unwrap();

    // Load into fresh doc
    let loaded = store.load_snapshot(doc_id).unwrap();
    let doc2 = Doc::new();
    {
        let update = yrs::Update::decode_v1(&loaded).unwrap();
        let mut txn = doc2.transact_mut();
        txn.apply_update(update).unwrap();
    }
    {
        let txn = doc2.transact();
        let text = txn.get_text("content").unwrap();
        assert_eq!(text.get_string(&txn), "Hello, persistence world!");
    }
}

#[test]
fn test_document_roundtrip_with_deltas() {
    let dir = tempdir().unwrap();
    let config = StoreConfig {
        path: dir.path().join("db"),
        ..StoreConfig::default()
    };
    let store = DocumentStore::open(config).unwrap();
    let doc_id = Uuid::new_v4();

    // Create initial doc and save snapshot
    let (doc, state) = make_doc_with_text("Initial");
    store.save_snapshot(doc_id, &state).unwrap();

    // Make several deltas
    for i in 0..10 {
        let delta = make_delta(&doc, &format!(" edit_{i}"));
        store.store_delta(doc_id, i, &delta).unwrap();
    }

    // Reconstruct from snapshot + deltas
    let snapshot = store.load_snapshot(doc_id).unwrap();
    let deltas = store.load_all_deltas(doc_id).unwrap();

    let doc2 = Doc::new();
    {
        let update = yrs::Update::decode_v1(&snapshot).unwrap();
        let mut txn = doc2.transact_mut();
        txn.apply_update(update).unwrap();
    }
    for (_version, delta_data) in &deltas {
        if let Ok(update) = yrs::Update::decode_v1(delta_data) {
            let mut txn = doc2.transact_mut();
            let _ = txn.apply_update(update);
        }
    }

    // Verify content
    let txn = doc2.transact();
    let text = txn.get_text("content").unwrap();
    let content = text.get_string(&txn);
    assert!(content.starts_with("Initial"));
    assert!(content.contains("edit_0"));
    assert!(content.contains("edit_9"));
}

// ─── Crash Recovery ──────────────────────────────────────────────────────────

#[tokio::test]
async fn test_crash_recovery_snapshot_survives_restart() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("db");
    let doc_id = Uuid::new_v4();

    // Phase 1: Write data directly to store then drop (simulates crash)
    {
        let store_config = StoreConfig {
            path: db_path.clone(),
            ..StoreConfig::default()
        };
        let store = DocumentStore::open(store_config).unwrap();

        let (_, state) = make_doc_with_text("Data that must survive a crash");
        store.save_snapshot(doc_id, &state).unwrap();

        // Store dropped here — simulates crash
    }

    // Phase 2: New server starts, recovers data
    {
        let server = SyncServer::with_storage("127.0.0.1:0", &db_path);
        let recovered = server.recover().await.unwrap();
        assert_eq!(recovered, 1, "Should recover exactly 1 document");

        // Verify the store still has the data
        let store = server.store().unwrap();
        let loaded = store.load_snapshot(doc_id).unwrap();

        let doc = Doc::new();
        {
            let update = yrs::Update::decode_v1(&loaded).unwrap();
            let mut txn = doc.transact_mut();
            txn.apply_update(update).unwrap();
        }
        let txn = doc.transact();
        let text = txn.get_text("content").unwrap();
        assert_eq!(text.get_string(&txn), "Data that must survive a crash");
    }
}

#[tokio::test]
async fn test_crash_recovery_deltas_survive() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("db");
    let doc_id = Uuid::new_v4();

    // Phase 1: Write snapshot + deltas, then crash
    {
        let store_config = StoreConfig {
            path: db_path.clone(),
            ..StoreConfig::default()
        };
        let store = DocumentStore::open(store_config).unwrap();

        let (doc, state) = make_doc_with_text("Base");
        store.save_snapshot(doc_id, &state).unwrap();

        for i in 0..5 {
            let delta = make_delta(&doc, &format!(" delta{i}"));
            store.store_delta(doc_id, i, &delta).unwrap();
        }
        // Dropped — crash
    }

    // Phase 2: Recover
    {
        let store_config = StoreConfig {
            path: db_path.clone(),
            ..StoreConfig::default()
        };
        let store = DocumentStore::open(store_config).unwrap();

        assert!(store.document_exists(doc_id).unwrap());
        let snapshot = store.load_snapshot(doc_id).unwrap();
        assert!(!snapshot.is_empty());

        let deltas = store.load_all_deltas(doc_id).unwrap();
        assert_eq!(deltas.len(), 5, "All 5 deltas should survive crash");
    }
}

#[tokio::test]
async fn test_crash_recovery_multiple_documents() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("db");
    let doc_ids: Vec<Uuid> = (0..5).map(|_| Uuid::new_v4()).collect();

    // Phase 1: Write multiple docs
    {
        let store_config = StoreConfig {
            path: db_path.clone(),
            ..StoreConfig::default()
        };
        let store = DocumentStore::open(store_config).unwrap();

        for (i, doc_id) in doc_ids.iter().enumerate() {
            let (_, state) = make_doc_with_text(&format!("Document {i} content"));
            store.save_snapshot(*doc_id, &state).unwrap();
        }
    }

    // Phase 2: Recover all
    {
        let server = SyncServer::with_storage("127.0.0.1:0", &db_path);
        let recovered = server.recover().await.unwrap();
        assert_eq!(recovered, 5, "All 5 documents should be recovered");
    }
}

// ─── Delta Compression ──────────────────────────────────────────────────────

#[test]
fn test_delta_compression_10_to_1_ratio() {
    // CTO target: 10:1 compression ratio for highly repetitive design data
    let repetitive_data = repetitive_text(10_000);
    let compressed = CompressedDelta::compress(1, repetitive_data.as_bytes());
    let ratio = compressed.compression_ratio();

    assert!(
        ratio >= 10.0,
        "Compression ratio {ratio:.1}x is below 10:1 target for repetitive data"
    );

    // Verify roundtrip
    let decompressed = compressed.decompress().unwrap();
    assert_eq!(decompressed, repetitive_data.as_bytes());
}

#[test]
fn test_10000_deltas_compressed_under_1mb() {
    // CTO target: 10,000 deltas compressed to <1MB
    let doc_id = Uuid::new_v4();
    let base = b"initial state".to_vec();
    let mut delta_log = DeltaLog::new(doc_id, base, 20_000);

    for i in 0..10_000 {
        // Each delta is a small edit (~50 bytes typical for design tools)
        let delta = format!(
            "{{\"op\":\"move\",\"layer\":{},\"x\":{},\"y\":{}}}",
            i % 100,
            i * 3,
            i * 7
        );
        delta_log.append(delta.as_bytes());
    }

    let stats = delta_log.stats();
    let total_compressed = stats.total_compressed_bytes as usize;

    assert!(
        total_compressed < 1_000_000,
        "10,000 deltas compressed to {} bytes ({}KB), exceeds 1MB target",
        total_compressed,
        total_compressed / 1024
    );

    assert_eq!(stats.delta_count, 10_000);
}

#[test]
fn test_delta_compaction_reduces_storage() {
    let dir = tempdir().unwrap();
    let config = StoreConfig {
        path: dir.path().join("db"),
        ..StoreConfig::default()
    };
    let store = DocumentStore::open(config).unwrap();
    let doc_id = Uuid::new_v4();

    // Store 100 deltas
    for i in 0..100u64 {
        let delta = format!("delta_payload_{i}_padding_data_here").into_bytes();
        store.store_delta(doc_id, i, &delta).unwrap();
    }

    let before = store.load_all_deltas(doc_id).unwrap().len();
    assert_eq!(before, 100);

    // Compact up to version 50
    store.compact_deltas(doc_id, 50).unwrap();

    let after = store.load_all_deltas(doc_id).unwrap().len();
    assert!(
        after < before,
        "Compaction should reduce delta count: before={before}, after={after}"
    );
}

// ─── WAL Performance ─────────────────────────────────────────────────────────

#[test]
fn test_wal_append_latency_under_10_microseconds() {
    // CTO target: WAL batch writes <10μs append latency
    let config = WalConfig::default();
    let mut wal = WriteAheadLog::new(config);
    let doc_id = Uuid::new_v4();
    let payload = vec![42u8; 64]; // Typical small delta

    // Warm up
    for _ in 0..100 {
        let _ = wal.append_delta(doc_id, payload.clone());
    }
    let _ = wal.flush();

    // Measure
    let iterations = 10_000u32;
    let mut wal2 = WriteAheadLog::new(WalConfig::default());
    let start = Instant::now();
    for _ in 0..iterations {
        let _ = wal2.append_delta(doc_id, payload.clone());
    }
    let elapsed = start.elapsed();
    let per_append = elapsed / iterations;

    assert!(
        per_append < Duration::from_micros(10),
        "WAL append latency {:?} exceeds 10μs target",
        per_append
    );
}

#[test]
fn test_wal_recovery_with_checksum_verification() {
    let config = WalConfig::default();
    let mut wal = WriteAheadLog::new(config);
    let doc_id = Uuid::new_v4();

    // Write entries
    for i in 0..50 {
        let _ = wal.append_delta(doc_id, vec![i as u8; 32]);
    }
    let entries = wal.flush();

    // Encode all entries
    let mut encoded_entries: Vec<Vec<u8>> = Vec::new();
    for entry in &entries {
        encoded_entries.push(entry.encode().unwrap());
    }

    // Recover and verify checksums
    let (recovered, corrupted) = WriteAheadLog::recover_entries(&encoded_entries);
    assert_eq!(recovered.len(), 50);
    assert_eq!(corrupted, 0);

    for (i, entry) in recovered.iter().enumerate() {
        assert!(entry.verify(), "Entry {i} checksum verification failed");
    }
}

// ─── Multi-Document Isolation ────────────────────────────────────────────────

#[test]
fn test_multi_document_isolation() {
    let dir = tempdir().unwrap();
    let config = StoreConfig {
        path: dir.path().join("db"),
        ..StoreConfig::default()
    };
    let store = DocumentStore::open(config).unwrap();

    let doc_a = Uuid::new_v4();
    let doc_b = Uuid::new_v4();

    let (_, state_a) = make_doc_with_text("Document A exclusive content");
    let (_, state_b) = make_doc_with_text("Document B exclusive content");

    store.save_snapshot(doc_a, &state_a).unwrap();
    store.save_snapshot(doc_b, &state_b).unwrap();

    // Store deltas for each
    for i in 0..10 {
        store.store_delta(doc_a, i, &format!("delta_a_{i}").into_bytes()).unwrap();
        store.store_delta(doc_b, i, &format!("delta_b_{i}").into_bytes()).unwrap();
    }

    // Verify isolation
    let deltas_a = store.load_all_deltas(doc_a).unwrap();
    let deltas_b = store.load_all_deltas(doc_b).unwrap();

    assert_eq!(deltas_a.len(), 10);
    assert_eq!(deltas_b.len(), 10);

    // Deltas should not leak between documents
    for (_ver, data) in &deltas_a {
        let s = String::from_utf8_lossy(data);
        assert!(s.contains("delta_a_"), "Doc A delta contains foreign data: {s}");
    }
    for (_ver, data) in &deltas_b {
        let s = String::from_utf8_lossy(data);
        assert!(s.contains("delta_b_"), "Doc B delta contains foreign data: {s}");
    }

    // Delete one, other survives
    store.delete_document(doc_a).unwrap();
    assert!(!store.document_exists(doc_a).unwrap());
    assert!(store.document_exists(doc_b).unwrap());
    assert_eq!(store.load_all_deltas(doc_b).unwrap().len(), 10);
}

// ─── Server Integration ─────────────────────────────────────────────────────

#[tokio::test]
async fn test_server_persistence_config() {
    let dir = tempdir().unwrap();
    let server = SyncServer::with_storage("127.0.0.1:0", dir.path().join("db"));
    assert!(server.store().is_some());

    let stats = server.stats().await;
    assert_eq!(stats.persisted_deltas, 0);
    assert_eq!(stats.persisted_snapshots, 0);
}

#[tokio::test]
async fn test_server_in_memory_mode_no_store() {
    let server = SyncServer::with_defaults();
    assert!(server.store().is_none());
}

#[tokio::test]
async fn test_server_recovery_preserves_content() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("db");
    let doc_id = Uuid::new_v4();

    // Pre-populate store
    {
        let store_config = StoreConfig {
            path: db_path.clone(),
            ..StoreConfig::default()
        };
        let store = DocumentStore::open(store_config).unwrap();
        let (_, state) = make_doc_with_text("Recoverable design file content");
        store.save_snapshot(doc_id, &state).unwrap();
    }

    // Server recovery
    let server = SyncServer::with_storage("127.0.0.1:0", &db_path);
    let recovered = server.recover().await.unwrap();
    assert_eq!(recovered, 1);

    // Verify store still accessible
    let store = server.store().unwrap();
    let loaded = store.load_snapshot(doc_id).unwrap();
    assert!(!loaded.is_empty());
}

// ─── Snapshot Versioning ─────────────────────────────────────────────────────

#[test]
fn test_snapshot_overwrite_preserves_latest() {
    let dir = tempdir().unwrap();
    let config = StoreConfig {
        path: dir.path().join("db"),
        ..StoreConfig::default()
    };
    let store = DocumentStore::open(config).unwrap();
    let doc_id = Uuid::new_v4();

    // Save v1
    let (_, state_v1) = make_doc_with_text("Version 1");
    store.save_snapshot(doc_id, &state_v1).unwrap();

    // Save v2 (overwrites)
    let (_, state_v2) = make_doc_with_text("Version 2 — latest");
    store.save_snapshot(doc_id, &state_v2).unwrap();

    // Load should get v2
    let loaded = store.load_snapshot(doc_id).unwrap();
    let doc = Doc::new();
    {
        let update = yrs::Update::decode_v1(&loaded).unwrap();
        let mut txn = doc.transact_mut();
        txn.apply_update(update).unwrap();
    }
    let txn = doc.transact();
    let text = txn.get_text("content").unwrap();
    assert_eq!(text.get_string(&txn), "Version 2 — latest");
}

// ─── Large Document Stress ───────────────────────────────────────────────────

#[test]
fn test_large_document_persistence() {
    let dir = tempdir().unwrap();
    let config = StoreConfig {
        path: dir.path().join("db"),
        ..StoreConfig::default()
    };
    let store = DocumentStore::open(config).unwrap();
    let doc_id = Uuid::new_v4();

    // Create a large document (~500KB of text)
    let large_content = repetitive_text(500_000);
    let (_, state) = make_doc_with_text(&large_content);

    store.save_snapshot(doc_id, &state).unwrap();

    let loaded = store.load_snapshot(doc_id).unwrap();
    let doc2 = Doc::new();
    {
        let update = yrs::Update::decode_v1(&loaded).unwrap();
        let mut txn = doc2.transact_mut();
        txn.apply_update(update).unwrap();
    }
    let txn = doc2.transact();
    let text = txn.get_text("content").unwrap();
    let recovered_text = text.get_string(&txn);
    assert_eq!(recovered_text.len(), large_content.len());
}

// ─── DeltaLog Integration ────────────────────────────────────────────────────

#[test]
fn test_delta_log_full_lifecycle() {
    let doc_id = Uuid::new_v4();
    let (doc, state) = make_doc_with_text("Base document");
    let mut log = DeltaLog::new(doc_id, state, 100);

    // Accumulate deltas
    for i in 0..50 {
        let delta = make_delta(&doc, &format!(" edit_{i}"));
        log.append(&delta);
    }

    let stats = log.stats();
    assert_eq!(stats.delta_count, 50);
    assert!(stats.total_original_bytes > 0);
    assert!(stats.total_compressed_bytes > 0);
    // Note: small deltas may compress larger due to LZ4 4-byte header overhead.
    // Compression wins emerge at larger payload sizes (>100 bytes).

    // Decompress all
    let decompressed = log.decompress_all().unwrap();
    assert_eq!(decompressed.len(), 50);

    // Compact — provide an apply function that merges Yrs updates
    let result = log.compact(|base, delta| {
        // Simple concat for testing — in production this would be Yrs merge
        let mut merged = base.to_vec();
        merged.extend_from_slice(delta);
        merged
    });
    assert!(result.is_ok());
    let after_stats = log.stats();
    assert_eq!(after_stats.delta_count, 0, "Deltas cleared after compaction");
}
