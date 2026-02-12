//! # logos-collab — Real-time collaboration layer for Logos
//!
//! Provides WebSocket-based multiplayer editing using CRDT synchronization.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────┐     WebSocket      ┌─────────────┐
//! │ SyncClient  │ ◄─────────────────► │ SyncServer  │
//! │ (per user)  │     Binary Proto    │ (central)   │
//! └──────┬──────┘                     └──────┬──────┘
//!        │                                   │
//!        ▼                                   ▼
//! ┌─────────────┐                     ┌─────────────┐
//! │ Yrs Doc     │                     │ Yrs Doc     │
//! │ (local)     │                     │ (authority) │
//! └─────────────┘                     └──────┬──────┘
//!                                            │
//!                                    ┌───────┴───────┐
//!                                    │ BroadcastGroup│
//!                                    │ (fan-out)     │
//!                                    └───────────────┘
//! ```
//!
//! ## Modules
//!
//! - [`protocol`] — Binary wire protocol (bincode-encoded SyncMessage)
//! - [`broadcast`] — Room-based fan-out with backpressure
//! - [`server`] — WebSocket sync server
//! - [`client`] — WebSocket sync client with offline queue
//!
//! ## Performance Targets
//!
//! | Metric | Target | Achieved |
//! |--------|--------|----------|
//! | Delta serialization | <500ns | ✅ |
//! | Broadcast 1K msgs × 100 peers | <10ms | ✅ |
//! | Offline queue replay (1K ops) | <50ms | ✅ |
//! | Memory per document | <1MB | ✅ |

pub mod protocol;
pub mod broadcast;
pub mod server;
pub mod client;
pub mod presence;
pub mod storage;

// Re-exports for convenience
pub use protocol::{
    AwarenessState, MessageType, PeerInfo, ProtocolError, SyncMessage,
};
pub use broadcast::{BroadcastGroup, BroadcastStats, RoomManager};
pub use presence::{
    AwarenessMessage, CursorColor, CursorInstance, CursorRenderData,
    PresenceRoom, RemoteCursorState, Vec2, build_cursor_instances,
};
pub use server::{ServerConfig, ServerStats, SyncServer};
pub use client::{ConnectionState, OfflineQueue, SyncClient, SyncEvent};
pub use storage::{
    DocumentStore, StoreConfig, StoreError, DocumentMetadata,
    DeltaLog, CompressedDelta, DeltaStats,
    WriteAheadLog, WalEntry, WalConfig, WalError,
};
