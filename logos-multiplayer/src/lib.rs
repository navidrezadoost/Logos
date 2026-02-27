//! # logos-multiplayer — Real-time collaboration prototype
//!
//! Bridges `logos-replay` (deterministic op log) with transport-agnostic
//! multiplayer primitives to deliver real-time collaborative editing.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    logos-multiplayer                         │
//! │                                                             │
//! │  Peer ──────── local identity + version tracking            │
//! │    │                                                        │
//! │    ▼                                                        │
//! │  SyncProtocol ─ op broadcast, ack, ordering                 │
//! │    │                                                        │
//! │    ▼                                                        │
//! │  CatchUp ────── replay-based state reconstruction           │
//! │    │                                                        │
//! │    ▼                                                        │
//! │  SnapshotExchange ── fast join via snapshot transfer         │
//! │    │                                                        │
//! │    ▼                                                        │
//! │  Presence ───── cursor, selection, viewport sharing          │
//! │    │                                                        │
//! │    ▼                                                        │
//! │  Convergence ── CRDT-style conflict-free merge              │
//! │    │                                                        │
//! │    ▼                                                        │
//! │  OfflineQueue ── buffer ops while disconnected              │
//! │    │                                                        │
//! │    ▼                                                        │
//! │  Indicators ── typing, editing, follow-mode                 │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Design Decisions
//!
//! - **Transport-agnostic** — all logic operates on messages, not sockets
//! - **Op-based sync** — broadcasts `OpEnvelope<T>` from logos-replay
//! - **Deterministic replay** — every peer rebuilds identical state
//! - **Append-only** — no history rewriting, even during conflict resolution
//! - **Offline-first** — ops are queued locally and replayed on reconnect

pub mod error;
pub mod peer;
pub mod sync_protocol;
pub mod catchup;
pub mod snapshot_exchange;
pub mod presence;
pub mod convergence;
pub mod offline;
pub mod indicators;

// Re-exports — public API surface
pub use error::MultiplayerError;
pub use peer::{Peer, PeerId, PeerState, PeerRegistry};
pub use sync_protocol::{SyncMessage, SyncProtocol, SyncAck, OpBroadcast};
pub use catchup::{CatchUpRequest, CatchUpResponse, CatchUpEngine};
pub use snapshot_exchange::{SnapshotRequest, SnapshotOffer, SnapshotTransfer};
pub use presence::{CursorPresence, SelectionPresence, ViewportPresence, PresenceManager};
pub use convergence::{MergeResult, MergeStrategy, ConvergenceEngine, ConvergenceProof, ConvergenceStatus};
pub use offline::{OfflineQueue, OfflineOp, ReplayPlan, ReplayStep};
pub use indicators::{EditingIndicator, TypingIndicator, FollowMode, IndicatorManager};
