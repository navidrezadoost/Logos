//! # Logos Sync
//!
//! Collaboration sync layer for Logos — builds on `logos-collab`'s transport
//! infrastructure to provide high-level sync primitives:
//!
//! - **Comments** — threaded comments anchored to layers, components, or canvas regions
//! - **Component sync** — change tracking and merging for component definitions
//! - **Sessions** — collaborative session state (users, permissions, activity)
//! - **Prototype sync** — real-time synchronization of prototype execution state
//! - **Conflict resolution** — detection and strategies for concurrent edits
//!
//! This crate is transport-agnostic: it operates on pure data models that can
//! be serialized and sent over any channel (WebSocket, gRPC, local IPC).

pub mod comment;
pub mod component_sync;
pub mod session;
pub mod prototype_sync;
pub mod conflict;

// Re-exports
pub use comment::{Comment, CommentAnchor, CommentId, CommentReaction, CommentThread, ThreadId};
pub use component_sync::{
    ComponentChange, ComponentChangeId, ComponentChangeLog, ComponentChangeType, PropertyDiff,
};
pub use session::{
    CollabSession, SessionConfig, SessionEvent, SessionId, SessionPermission, SessionUser,
    SessionUserRole,
};
pub use prototype_sync::{
    PrototypeAction, PrototypeSyncMessage, PrototypeSyncRoom, ViewerState, ViewerId,
};
pub use conflict::{
    ConflictDetector, ConflictResolution, ConflictStrategy, EditConflict, EditOperation,
};
