//! Persistent storage layer for collaborative documents.
//!
//! Architecture:
//! ```text
//! ┌─────────────┐     deltas      ┌──────────────┐
//! │ SyncServer  │ ──────────────► │ DocumentStore│
//! │ (in-memory) │                 │ (RocksDB)    │
//! └──────┬──────┘                 └──────┬───────┘
//!        │                               │
//!        │ on startup                    │ column families
//!        ▼                               ▼
//! ┌─────────────┐     ┌──────────────────────────────────┐
//! │ Yrs Doc     │     │ CF "documents" — full snapshots   │
//! │ (restored)  │     │ CF "deltas"    — compressed edits │
//! └─────────────┘     │ CF "metadata"  — doc metadata     │
//!                     │ CF "wal"       — write-ahead log  │
//!                     └──────────────────────────────────┘
//! ```
//!
//! ## Performance Targets
//!
//! | Metric               | Target  | Reference                          |
//! |----------------------|---------|------------------------------------|
//! | Open (10k docs)      | <100ms  | DDIA Ch.3 — LSM Trees              |
//! | Document load (1MB)  | <1ms    | Patterson §5.7 — Cache Hierarchy   |
//! | Delta save (1KB)     | <50μs   | DDIA Ch.3 — Write-Ahead Logs       |
//! | WAL append           | <10μs   | Patterson §5.7 — Sequential I/O    |
//! | Recovery (10k docs)  | <500ms  | DDIA Ch.3 — Crash Recovery          |
//! | Compression ratio    | 10:1    | Patterson §5.7 — Data Compression  |
//!
//! Reference: Kleppmann — Designing Data-Intensive Applications, Chapter 3

pub mod rocks;
pub mod delta;
pub mod wal;

pub use rocks::{DocumentStore, StoreConfig, StoreError, DocumentMetadata};
pub use delta::{DeltaLog, CompressedDelta, DeltaStats};
pub use wal::{WriteAheadLog, WalEntry, WalConfig, WalError};
