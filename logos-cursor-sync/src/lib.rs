//! # logos-cursor-sync — Live cursor synchronization
//!
//! Real-time cursor presence syncing across collaborating users.  
//! Page-aware filtering, editing indicators, idle fade-out,
//! viewport culling, presence snapshots, and cursor trail history.
//!
//! ## Architecture
//!
//! ```text
//! Local cursor move
//!       │
//!       ▼
//! ┌─────────────┐   AwarenessMessage    ┌─────────────┐
//! │ PresenceRoom│ ──────────────────►  │ SyncClient  │
//! │ (local)     │   (bincode, ~33B)    │ → WebSocket │
//! └──────┬──────┘                      └──────┬──────┘
//!        │                                    │
//!        │ 30fps throttle                     │ broadcast
//!        ▼                                    ▼
//! ┌─────────────┐                      ┌─────────────┐
//! │ Interpolator│◄──────────────────── │ SyncServer  │
//! │ (60fps)     │   Remote cursors     │ → BroadcastG│
//! └──────┬──────┘                      └─────────────┘
//!        │
//!        ▼
//! ┌─────────────┐
//! │ GPU Upload  │  CursorInstance × N
//! │ (instanced) │  40 bytes each
//! └─────────────┘
//! ```

pub mod presence;

pub use presence::{
    AwarenessMessage, CursorColor, CursorInstance, CursorRenderData,
    EditingState, PresenceRoom, RemoteCursorState, Vec2, ViewportRect,
    build_cursor_instances,
};
