//! Deterministic operation log, replay engine, and time-travel for Logos.
//!
//! This crate provides the infrastructure for recording every operation
//! that mutates document state, replaying them to reconstruct state at
//! any point in time, and computing diffs between versions.
//!
//! ## Architecture
//!
//! ```text
//! ┌────────────────────────────────────────────────────────────┐
//! │                      logos-replay                          │
//! │                                                            │
//! │  OpEnvelope<T> ── wraps any operation + metadata           │
//! │      │                                                     │
//! │      ▼                                                     │
//! │  OpLog ── append-only, ordered by version                  │
//! │      │                                                     │
//! │      ▼                                                     │
//! │  SnapshotStore ── periodic full-state snapshots             │
//! │      │                                                     │
//! │      ▼                                                     │
//! │  ReplayEngine ── apply ops to reconstruct state            │
//! │      │                                                     │
//! │      ▼                                                     │
//! │  TimeTravel ── query state at any version                  │
//! │      │                                                     │
//! │      ▼                                                     │
//! │  VersionDiff ── compare two versions                       │
//! └────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Design Decisions
//!
//! - **Generic over operation type** — `OpEnvelope<T>` wraps any `T: Serialize + Deserialize`
//! - **Serialization-agnostic** — stores use `serde_json::Value` as the interchange format
//! - **Deterministic** — replay produces identical state given identical op sequence
//! - **Composition over inheritance** — `ReplayEngine` uses pluggable `OpApplier` trait
//! - **Identity-aware** — every op carries a `UserId` from `logos-identity`

pub mod error;
pub mod envelope;
pub mod clock;
pub mod oplog;
pub mod snapshot;
pub mod engine;
pub mod timetravel;
pub mod diff;
pub mod cursor;
pub mod retention;

// Re-exports — public API surface
pub use error::ReplayError;
pub use envelope::{OpEnvelope, OpId, OpMetadata};
pub use clock::{LamportClock, VectorClock, CausalOrder};
pub use oplog::{OpLog, InMemoryOpLog, OpRange, OpQuery};
pub use snapshot::{Snapshot, SnapshotId, SnapshotStore, InMemorySnapshotStore, SnapshotPolicy};
pub use engine::{ReplayEngine, OpApplier, ReplayResult, StateContainer};
pub use timetravel::{TimeTraveler, VersionQuery, HistoryEntry, HistorySummary};
pub use diff::{VersionDiff, DiffEntry, DiffKind, FieldChange};
pub use cursor::{ReplayCursor, CursorDirection};
pub use retention::{RetentionPolicy, RetentionAction};
