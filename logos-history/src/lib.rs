//! Version history data layer for Logos.
//!
//! Builds on `logos-replay` to provide the UI-facing abstractions
//! that make time travel, version comparison, and collaboration
//! history visible to end users.
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────┐
//! │                     logos-history                         │
//! │                                                          │
//! │  Timeline ── paginated, filterable version timeline       │
//! │      │                                                   │
//! │      ▼                                                   │
//! │  Bookmark ── named versions / milestones                 │
//! │      │                                                   │
//! │      ▼                                                   │
//! │  Activity ── group ops into sessions / change batches    │
//! │      │                                                   │
//! │      ▼                                                   │
//! │  Changeset ── human-readable change descriptions         │
//! │      │                                                   │
//! │      ▼                                                   │
//! │  Restore ── safely restore to any historical version     │
//! │      │                                                   │
//! │      ▼                                                   │
//! │  Branch ── fork / branch from any version                │
//! └──────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Design Decisions
//!
//! - **View-model oriented** — types are designed for UI rendering
//! - **Session grouping** — raw ops are grouped into user-meaningful sessions
//! - **Bookmark persistence** — named versions stored alongside the op log
//! - **Branch model** — lightweight branches from any historical version
//! - **Restore safety** — restoration records a new op (never mutates history)

pub mod error;
pub mod timeline;
pub mod bookmark;
pub mod activity;
pub mod changeset;
pub mod restore;
pub mod branch;

// Re-exports — public API surface
pub use error::HistoryError;
pub use timeline::{Timeline, TimelineEntry, TimelineFilter, TimelinePage};
pub use bookmark::{Bookmark, BookmarkId, BookmarkStore, InMemoryBookmarkStore};
pub use activity::{ActivitySession, ActivityFeed, SessionGrouper};
pub use changeset::{Changeset, ChangeDescription, ChangeCategory};
pub use restore::{RestoreRequest, RestoreResult, RestoreStrategy, RestoreEngine};
pub use branch::{Branch, BranchId, BranchStore, InMemoryBranchStore, BranchStatus};

/// Return the current Unix timestamp in seconds.
///
/// Uses `std::time::SystemTime::now()`. Falls back to 0 on clock error.
pub(crate) fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
