//! # Logos Comments
//!
//! Comprehensive comments, annotations, @mentions, and notifications for
//! real-time collaborative design feedback. This crate provides:
//!
//! - **Comments & Threads** — threaded discussions anchored to layers, components,
//!   canvas regions, or properties
//! - **Annotations** — visual markup (pins, arrows, area highlights, stamps, freehand)
//! - **@Mentions** — parse, index, and route mentions to users
//! - **Notifications** — per-user notification store with read/unread tracking
//! - **Sync Protocol** — operation-based sync with Lamport clocks and LWW conflict resolution
//! - **Permissions** — role-based access control for comment operations
//! - **Filtering** — spatial, temporal, page-based, and author-based filtering
//!
//! # Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────────┐
//! │              CommentEngine                    │
//! │  ┌───────────┐  ┌──────────┐  ┌───────────┐ │
//! │  │ CommentStore│ │AnnotStore│  │NotifyStore│ │
//! │  └─────┬─────┘  └────┬─────┘  └────┬──────┘ │
//! │        │             │              │        │
//! │  ┌─────▼─────────────▼──────────────▼─────┐  │
//! │  │          MentionIndex                  │  │
//! │  └────────────────────────────────────────┘  │
//! │                                              │
//! │  ┌───────────┐  ┌──────────┐  ┌───────────┐ │
//! │  │ OpLog/Sync │  │  Filter  │  │ Permissions│ │
//! │  └───────────┘  └──────────┘  └───────────┘ │
//! └──────────────────────────────────────────────┘
//! ```
//!
//! All types are transport-agnostic and fully `serde`-serializable.

pub mod model;
pub mod mention;
pub mod notification;
pub mod annotation;
pub mod ops;
pub mod state;
pub mod sync;
pub mod filter;
pub mod permission;

// Re-exports
pub use model::{
    Comment, CommentAnchor, CommentId, CommentReaction, CommentThread,
    ResolutionState, ThreadId, CommentStore,
};
pub use mention::{Mention, MentionIndex, parse_mentions};
pub use notification::{
    Notification, NotificationId, NotificationKind, NotificationStore,
};
pub use annotation::{
    Annotation, AnnotationId, AnnotationKind, AnnotationStore, AnnotationStyle,
    ArrowHead, StampKind,
};
pub use ops::{CommentOp, OpEnvelope, LamportClock};
pub use state::CommentSyncState;
pub use sync::CommentEngine;
pub use filter::CommentFilter;
pub use permission::{CommentPermission, PermissionChecker, UserRole};
