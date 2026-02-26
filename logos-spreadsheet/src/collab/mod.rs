//! Collaborative editing for spreadsheets.
//!
//! This module provides a **pure-Rust** collaboration layer that can be
//! wired to the `logos-collab` transport (WebSocket + Yrs) for real-time
//! multi-user spreadsheet editing.
//!
//! # Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────────┐
//! │               CollabEngine                    │
//! │  ┌─────────────┐   ┌──────────────────────┐  │
//! │  │ CollabState  │   │  PresenceTracker     │  │
//! │  │ (LWW regs)  │   │  (cursors, editing)  │  │
//! │  └──────┬──────┘   └──────┬───────────────┘  │
//! │         │                  │                  │
//! │    CellOp (CRDT)     PeerPresence            │
//! │         │                  │                  │
//! └─────────┼──────────────────┼──────────────────┘
//!           │                  │
//!           ▼                  ▼
//!     logos-collab        logos-collab
//!     (WebSocket)         (Awareness)
//! ```
//!
//! # Modules
//!
//! - [`ops`] — Cell operations and Lamport clocks
//! - [`state`] — LWW-Register-per-cell CRDT
//! - [`presence`] — Spreadsheet presence (cursors, selections)
//! - [`sync`] — `CollabEngine` — top-level orchestrator

pub mod ops;
pub mod state;
pub mod presence;
pub mod sync;

// Re-exports
pub use ops::{CellOp, CellPayload, LamportClock, OpBatch, OpTimestamp, SiteId};
pub use state::{ApplyResult, CollabState, CollabStats};
pub use presence::{
    PeerColor, PeerCursorRenderData, PeerPresence, PresenceTracker,
};
pub use sync::{CollabEngine, SessionInfo};
