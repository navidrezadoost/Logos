//! `logos-agent-versioning` — Agent version snapshot, history, diff, and rollback.
//!
//! # Quick start
//!
//! ```rust
//! use logos_agent_versioning::{AgentSnapshot, RollbackManager, RollbackPolicy, SemVer};
//!
//! let mut mgr = RollbackManager::new(RollbackPolicy::KeepAll);
//! let v1 = SemVer::new(1, 0, 0);
//! let snap = AgentSnapshot::builder("my-agent", v1)
//!     .config_str("model", "gpt-4o")
//!     .build();
//! mgr.commit_snapshot(snap).unwrap();
//! ```

pub mod version;
pub mod registry;
pub mod rollback;
pub mod diff;
pub mod storage;

// ── Flat re-exports ───────────────────────────────────────────────────────────

pub use version::{AgentSnapshot, SnapshotBuilder, SemVer, VersionMetadata, VersionError};
pub use registry::{VersionRegistry, RegistryError};
pub use rollback::{
    RollbackManager, RollbackPolicy, RollbackRequest, RollbackResult, RollbackStatus,
};
pub use diff::{VersionDiff, DiffEntry, ChangeKind};
pub use storage::{BlobStore, StorageError};
