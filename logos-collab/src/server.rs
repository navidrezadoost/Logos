//! WebSocket sync server with room-based document routing.
//!
//! Architecture:
//! ```text
//! Client A ──┐
//!             ├── Room (doc_id) ── Yrs Doc ── BroadcastGroup
//! Client B ──┘                        │
//!                                     ├── DocumentStore (RocksDB)
//!                                     │       │
//!                                     │       ├── Snapshots (LZ4)
//!                                     │       ├── Deltas (LZ4)
//!                                     │       └── WAL (sequential)
//!                                     │
//!                          ┌──────────┼───────────┐
//!                          ▼          ▼           ▼
//!                       Client A   Client B    Client C
//! ```
//!
//! Each document room maintains:
//! - A Yrs `Doc` for authoritative state
//! - A `BroadcastGroup` for fan-out to connected peers
//! - Peer presence tracking
//! - Persistent storage via DocumentStore (RocksDB)
//!
//! Reference: Kleppmann — Designing Data-Intensive Applications, Chapters 3 & 8

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;
use yrs::ReadTxn;
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;

use crate::broadcast::{BroadcastGroup, RoomManager};
use crate::presence::AwarenessMessage;
use crate::protocol::{MessageType, PeerInfo, SyncMessage};
use crate::storage::{DocumentStore, StoreConfig};

/// Server configuration.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Address to bind to
    pub bind_addr: String,
    /// Maximum peers per room
    pub max_peers_per_room: usize,
    /// Broadcast channel capacity per room
    pub broadcast_capacity: usize,
    /// Heartbeat interval in seconds
    pub heartbeat_interval_secs: u64,
    /// Persistence storage path (None = in-memory only)
    pub storage_path: Option<PathBuf>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:9090".to_string(),
            max_peers_per_room: 100,
            broadcast_capacity: 256,
            heartbeat_interval_secs: 30,
            storage_path: None,
        }
    }
}

/// Server statistics.
#[derive(Debug, Clone, Default)]
pub struct ServerStats {
    pub total_connections: u64,
    pub active_connections: u64,
    pub total_messages: u64,
    pub total_bytes: u64,
    pub active_rooms: usize,
    pub persisted_deltas: u64,
    pub persisted_snapshots: u64,
    pub storage_bytes: u64,
}

/// Document room: Yrs Doc + broadcast group.
struct DocumentRoom {
    /// Authoritative Yrs document
    doc: yrs::Doc,
    /// Broadcast group for fan-out
    broadcast: Arc<BroadcastGroup>,
}

impl DocumentRoom {
    fn new(broadcast_capacity: usize) -> Self {
        Self {
            doc: yrs::Doc::new(),
            broadcast: Arc::new(BroadcastGroup::new(broadcast_capacity)),
        }
    }
}

/// The sync server.
pub struct SyncServer {
    config: ServerConfig,
    /// Document rooms: doc_id → (Yrs Doc + BroadcastGroup)
    rooms: Arc<RwLock<HashMap<Uuid, DocumentRoom>>>,
    /// Room manager for broadcast routing
    room_manager: Arc<RoomManager>,
    /// Server-wide statistics
    stats: Arc<RwLock<ServerStats>>,
    /// Persistent document store (optional)
    store: Option<Arc<DocumentStore>>,
    /// Global delta version counter for persistence
    delta_version: Arc<AtomicU64>,
}

impl SyncServer {
    /// Create a new sync server with the given configuration.
    pub fn new(config: ServerConfig) -> Self {
        let room_manager = Arc::new(RoomManager::new(config.broadcast_capacity));

        // Open persistent storage if configured
        let store = config.storage_path.as_ref().map(|path| {
            let store_config = StoreConfig {
                path: path.clone(),
                ..StoreConfig::default()
            };
            Arc::new(
                DocumentStore::open(store_config)
                    .expect("Failed to open document store"),
            )
        });

        let delta_version = Arc::new(AtomicU64::new(
            store.as_ref().map_or(0, |s| s.wal_sequence()),
        ));

        Self {
            config,
            rooms: Arc::new(RwLock::new(HashMap::new())),
            room_manager,
            stats: Arc::new(RwLock::new(ServerStats::default())),
            store,
            delta_version,
        }
    }

    /// Create with default configuration (in-memory, no persistence).
    pub fn with_defaults() -> Self {
        Self::new(ServerConfig::default())
    }

    /// Create with persistence enabled at the given path.
    pub fn with_storage(bind_addr: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        let config = ServerConfig {
            bind_addr: bind_addr.into(),
            storage_path: Some(path.into()),
            ..ServerConfig::default()
        };
        Self::new(config)
    }

    /// Recover persisted documents from storage on startup.
    ///
    /// Loads all previously persisted documents into rooms so they are
    /// immediately available when peers reconnect.
    pub async fn recover(&self) -> Result<usize, Box<dyn std::error::Error>> {
        let store = match &self.store {
            Some(s) => s,
            None => return Ok(0),
        };

        let doc_ids = store.list_documents()?;
        let mut recovered = 0;

        for doc_id in &doc_ids {
            if let Ok(snapshot) = store.load_snapshot(*doc_id) {
                let mut rooms_w = self.rooms.write().await;
                let room = rooms_w
                    .entry(*doc_id)
                    .or_insert_with(|| DocumentRoom::new(self.config.broadcast_capacity));

                // Apply the persisted snapshot to the Yrs doc
                if let Ok(update) = yrs::Update::decode_v1(&snapshot) {
                    let mut txn = yrs::Transact::transact_mut(&room.doc);
                    let _ = txn.apply_update(update);
                }
                recovered += 1;
                log::info!("Recovered document {doc_id} from storage");
            }
        }

        log::info!("Recovery complete: {recovered}/{} documents restored", doc_ids.len());
        Ok(recovered)
    }

    /// Start listening for WebSocket connections.
    ///
    /// This runs the server event loop. Call from an async runtime.
    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Recover persisted documents on startup
        let recovered = self.recover().await?;
        if recovered > 0 {
            log::info!("Recovered {recovered} documents from persistent storage");
        }

        let listener = TcpListener::bind(&self.config.bind_addr).await?;
        log::info!("Sync server listening on {}", self.config.bind_addr);

        loop {
            let (stream, addr) = listener.accept().await?;
            log::debug!("New TCP connection from {addr}");

            let rooms = self.rooms.clone();
            let stats = self.stats.clone();
            let config = self.config.clone();
            let room_manager = self.room_manager.clone();
            let store = self.store.clone();
            let delta_version = self.delta_version.clone();

            tokio::spawn(async move {
                if let Err(e) =
                    Self::handle_connection(
                        stream, addr, rooms, stats, config, room_manager,
                        store, delta_version,
                    ).await
                {
                    log::error!("Connection error from {addr}: {e}");
                }
            });
        }
    }

    /// Handle a single WebSocket connection.
    async fn handle_connection(
        stream: TcpStream,
        addr: SocketAddr,
        rooms: Arc<RwLock<HashMap<Uuid, DocumentRoom>>>,
        stats: Arc<RwLock<ServerStats>>,
        config: ServerConfig,
        _room_manager: Arc<RoomManager>,
        store: Option<Arc<DocumentStore>>,
        delta_version: Arc<AtomicU64>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let ws_stream = tokio_tungstenite::accept_async(stream).await?;
        let (mut ws_sender, mut ws_receiver) = ws_stream.split();

        log::info!("WebSocket connection established from {addr}");

        {
            let mut s = stats.write().await;
            s.total_connections += 1;
            s.active_connections += 1;
        }

        // State for this connection
        let mut peer_id: Option<Uuid> = None;
        let mut doc_id: Option<Uuid> = None;
        let mut broadcast_rx: Option<tokio::sync::broadcast::Receiver<Arc<Vec<u8>>>> = None;

        // Process incoming messages
        loop {
            tokio::select! {
                // Incoming WebSocket message
                msg = ws_receiver.next() => {
                    match msg {
                        Some(Ok(Message::Binary(data))) => {
                            let bytes: Vec<u8> = data.into();
                            match SyncMessage::decode(&bytes) {
                                Ok(sync_msg) => {
                                    {
                                        let mut s = stats.write().await;
                                        s.total_messages += 1;
                                        s.total_bytes += bytes.len() as u64;
                                    }

                                    match sync_msg.msg_type {
                                        MessageType::PeerJoined => {
                                            // First message: peer joins a document room
                                            peer_id = Some(sync_msg.peer_id);
                                            doc_id = Some(sync_msg.doc_id);

                                            let info = sync_msg.peer_info().unwrap_or_else(|_| {
                                                PeerInfo::with_id(sync_msg.peer_id, "Anonymous")
                                            });

                                            // Get or create room
                                            let mut rooms_w = rooms.write().await;
                                            let is_new_room = !rooms_w.contains_key(&sync_msg.doc_id);
                                            let room = rooms_w
                                                .entry(sync_msg.doc_id)
                                                .or_insert_with(|| DocumentRoom::new(config.broadcast_capacity));

                                            // Load persisted snapshot into new room
                                            if is_new_room {
                                                if let Some(ref s) = store {
                                                    if let Ok(snapshot) = s.load_snapshot(sync_msg.doc_id) {
                                                        if let Ok(update) = yrs::Update::decode_v1(&snapshot) {
                                                            let mut txn = yrs::Transact::transact_mut(&room.doc);
                                                            let _ = txn.apply_update(update);
                                                            log::info!("Loaded persisted snapshot for doc {}", sync_msg.doc_id);
                                                        }
                                                    }
                                                }
                                            }

                                            // Add peer to broadcast group
                                            let rx = room.broadcast.add_peer(info.clone()).await;
                                            broadcast_rx = Some(rx);

                                            // Send current state (SyncStep2)
                                            // Scope the transaction so it's dropped before await
                                            let sv = {
                                                let txn = yrs::Transact::transact(&room.doc);
                                                txn.state_vector().encode_v1()
                                            };

                                            // Broadcast peer joined to others
                                            let join_msg = SyncMessage::peer_joined(
                                                info.peer_id,
                                                sync_msg.doc_id,
                                                &info,
                                            );
                                            let broadcast_clone = room.broadcast.clone();
                                            let room_count = rooms_w.len();
                                            drop(rooms_w); // Release lock before await

                                            let state_msg = SyncMessage::sync_step2(
                                                Uuid::nil(),
                                                sync_msg.doc_id,
                                                sv,
                                            );
                                            let encoded = state_msg.encode()?;
                                            ws_sender.send(Message::Binary(encoded.into())).await?;

                                            let _ = broadcast_clone.broadcast(&join_msg).await;

                                            {
                                                let mut s = stats.write().await;
                                                s.active_rooms = room_count;
                                            }

                                            log::info!(
                                                "Peer {} ({}) joined doc {}",
                                                info.name,
                                                info.peer_id,
                                                sync_msg.doc_id
                                            );
                                        }

                                        MessageType::Delta => {
                                            // Apply delta to server's Yrs doc, then broadcast
                                            if let Some(did) = doc_id {
                                                let broadcast_clone = {
                                                    let mut rooms_w = rooms.write().await;
                                                    if let Some(room) = rooms_w.get_mut(&did) {
                                                        // Apply to authoritative doc (sync, no await)
                                                        if let Ok(update) = yrs::Update::decode_v1(&sync_msg.payload) {
                                                            let mut txn = yrs::Transact::transact_mut(&room.doc);
                                                            let _ = txn.apply_update(update);
                                                        }
                                                        Some(room.broadcast.clone())
                                                    } else {
                                                        None
                                                    }
                                                };

                                                // Persist delta to storage (outside of room lock)
                                                if let Some(ref s) = store {
                                                    let version = delta_version.fetch_add(1, Ordering::SeqCst);
                                                    if let Err(e) = s.store_delta(did, version, &sync_msg.payload) {
                                                        log::error!("Failed to persist delta for doc {did}: {e}");
                                                    } else {
                                                        let mut st = stats.write().await;
                                                        st.persisted_deltas += 1;
                                                    }
                                                }

                                                // Broadcast outside of lock
                                                if let Some(bc) = broadcast_clone {
                                                    let _ = bc.broadcast(&sync_msg).await;
                                                }
                                            }
                                        }

                                        MessageType::SyncStep1 => {
                                            // Client requesting state diff
                                            if let Some(did) = doc_id {
                                                let diff_result = {
                                                    let rooms_r = rooms.read().await;
                                                    if let Some(room) = rooms_r.get(&did) {
                                                        let txn = yrs::Transact::transact(&room.doc);
                                                        if let Ok(remote_sv) = yrs::StateVector::decode_v1(&sync_msg.payload) {
                                                            Some(txn.encode_diff_v1(&remote_sv))
                                                        } else {
                                                            None
                                                        }
                                                    } else {
                                                        None
                                                    }
                                                };
                                                if let Some(diff) = diff_result {
                                                    let response = SyncMessage::sync_step2(
                                                        Uuid::nil(),
                                                        did,
                                                        diff,
                                                    );
                                                    let encoded = response.encode()?;
                                                    ws_sender.send(Message::Binary(encoded.into())).await?;
                                                }
                                            }
                                        }

                                        MessageType::Awareness => {
                                            // Decode presence message and broadcast to all peers in room
                                            if let Some(did) = doc_id {
                                                // Log presence activity for monitoring
                                                if let Ok(awareness_msg) = AwarenessMessage::decode(&sync_msg.payload) {
                                                    match &awareness_msg {
                                                        AwarenessMessage::Join { user_name, .. } => {
                                                            log::info!("Presence: {} joined room {}", user_name, did);
                                                        }
                                                        AwarenessMessage::Leave { user_id } => {
                                                            log::info!("Presence: {} left room {}", user_id, did);
                                                        }
                                                        AwarenessMessage::Cursor { .. } => {
                                                            log::trace!("Presence: cursor update in room {}", did);
                                                        }
                                                        AwarenessMessage::Selection { user_id, layer_ids } => {
                                                            log::debug!("Presence: {} selected {} layers in room {}", user_id, layer_ids.len(), did);
                                                        }
                                                    }
                                                }

                                                let broadcast_clone = {
                                                    let rooms_r = rooms.read().await;
                                                    rooms_r.get(&did).map(|r| r.broadcast.clone())
                                                };
                                                if let Some(bc) = broadcast_clone {
                                                    let _ = bc.broadcast(&sync_msg).await;
                                                }
                                            }
                                        }

                                        MessageType::Ping => {
                                            // Respond with pong
                                            if let Some(pid) = peer_id {
                                                let pong = SyncMessage::pong(pid);
                                                let encoded = pong.encode()?;
                                                ws_sender.send(Message::Binary(encoded.into())).await?;
                                            }
                                        }

                                        _ => {
                                            log::debug!("Unhandled message type: {:?}", sync_msg.msg_type);
                                        }
                                    }
                                }
                                Err(e) => {
                                    log::warn!("Failed to decode message from {addr}: {e}");
                                }
                            }
                        }

                        Some(Ok(Message::Close(_))) | None => {
                            log::info!("Connection closed from {addr}");
                            break;
                        }

                        Some(Ok(Message::Ping(data))) => {
                            ws_sender.send(Message::Pong(data)).await?;
                        }

                        Some(Err(e)) => {
                            log::error!("WebSocket error from {addr}: {e}");
                            break;
                        }

                        _ => {}
                    }
                }

                // Outgoing broadcast message
                msg = async {
                    if let Some(ref mut rx) = broadcast_rx {
                        rx.recv().await
                    } else {
                        // No broadcast receiver yet — wait forever
                        std::future::pending().await
                    }
                } => {
                    match msg {
                        Ok(data) => {
                            // Don't echo back to sender
                            if let Ok(sync_msg) = SyncMessage::decode(&data) {
                                if Some(sync_msg.peer_id) == peer_id {
                                    continue; // Skip own messages
                                }
                            }
                            ws_sender.send(Message::Binary(data.to_vec().into())).await?;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            log::warn!("Peer {peer_id:?} lagged by {n} messages");
                        }
                        Err(_) => break,
                    }
                }
            }
        }

        // Cleanup: remove peer from room
        if let (Some(pid), Some(did)) = (peer_id, doc_id) {
            let mut rooms_w = rooms.write().await;
            if let Some(room) = rooms_w.get_mut(&did) {
                room.broadcast.remove_peer(&pid).await;

                // Broadcast peer left
                let leave_msg = SyncMessage::peer_left(pid, did);
                let _ = room.broadcast.broadcast(&leave_msg).await;

                // Remove empty rooms — persist snapshot before removal
                if room.broadcast.peer_count().await == 0 {
                    // Save snapshot to persistent storage
                    if let Some(ref s) = store {
                        let snapshot = {
                            let txn = yrs::Transact::transact(&room.doc);
                            txn.encode_state_as_update_v1(&yrs::StateVector::default())
                        };
                        match s.save_snapshot(did, &snapshot) {
                            Ok(_) => {
                                // Compact deltas after snapshot (compact all versions)
                                let current_version = delta_version.load(Ordering::SeqCst);
                                let _ = s.compact_deltas(did, current_version);
                                let mut st = stats.write().await;
                                st.persisted_snapshots += 1;
                                log::info!("Persisted snapshot for doc {did} (room closing)");
                            }
                            Err(e) => {
                                log::error!("Failed to persist snapshot for doc {did}: {e}");
                            }
                        }
                    }

                    rooms_w.remove(&did);
                    log::info!("Room {did} removed (empty)");
                }
            }

            let mut s = stats.write().await;
            s.active_connections -= 1;
            s.active_rooms = rooms_w.len();
        }

        Ok(())
    }

    /// Get server statistics.
    pub async fn stats(&self) -> ServerStats {
        self.stats.read().await.clone()
    }

    /// Get the configured bind address.
    pub fn bind_addr(&self) -> &str {
        &self.config.bind_addr
    }

    /// Get room manager reference.
    pub fn room_manager(&self) -> &Arc<RoomManager> {
        &self.room_manager
    }

    /// Get the persistent store (if configured).
    pub fn store(&self) -> Option<&Arc<DocumentStore>> {
        self.store.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yrs::{WriteTxn, GetString, Text};

    #[test]
    fn test_server_config_default() {
        let config = ServerConfig::default();
        assert_eq!(config.bind_addr, "127.0.0.1:9090");
        assert_eq!(config.max_peers_per_room, 100);
        assert_eq!(config.broadcast_capacity, 256);
        assert_eq!(config.heartbeat_interval_secs, 30);
        assert!(config.storage_path.is_none());
    }

    #[test]
    fn test_server_creation() {
        let server = SyncServer::with_defaults();
        assert_eq!(server.bind_addr(), "127.0.0.1:9090");
        assert!(server.store.is_none());
    }

    #[test]
    fn test_server_custom_config() {
        let config = ServerConfig {
            bind_addr: "0.0.0.0:8080".to_string(),
            max_peers_per_room: 50,
            broadcast_capacity: 512,
            heartbeat_interval_secs: 15,
            storage_path: None,
        };
        let server = SyncServer::new(config);
        assert_eq!(server.bind_addr(), "0.0.0.0:8080");
    }

    #[tokio::test]
    async fn test_server_with_storage() {
        let dir = tempfile::tempdir().unwrap();
        let server = SyncServer::with_storage("127.0.0.1:0", dir.path().join("db"));
        assert!(server.store.is_some());
    }

    #[tokio::test]
    async fn test_server_stats_initial() {
        let server = SyncServer::with_defaults();
        let stats = server.stats().await;
        assert_eq!(stats.total_connections, 0);
        assert_eq!(stats.active_connections, 0);
        assert_eq!(stats.total_messages, 0);
        assert_eq!(stats.total_bytes, 0);
        assert_eq!(stats.active_rooms, 0);
        assert_eq!(stats.persisted_deltas, 0);
        assert_eq!(stats.persisted_snapshots, 0);
        assert_eq!(stats.storage_bytes, 0);
    }

    #[tokio::test]
    async fn test_server_recovery_empty() {
        let server = SyncServer::with_defaults();
        let recovered = server.recover().await.unwrap();
        assert_eq!(recovered, 0);
    }

    #[tokio::test]
    async fn test_server_recovery_with_storage() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("db");
        let doc_id = Uuid::new_v4();

        // Write a snapshot to storage
        {
            let store_config = StoreConfig {
                path: db_path.clone(),
                ..StoreConfig::default()
            };
            let store = DocumentStore::open(store_config).unwrap();

            // Create a Yrs doc, make some changes, encode
            let doc = yrs::Doc::new();
            {
                let mut txn = yrs::Transact::transact_mut(&doc);
                let text = txn.get_or_insert_text("test");
                text.insert(&mut txn, 0, "Hello, persistence!");
            }
            let snapshot = {
                let txn = yrs::Transact::transact(&doc);
                txn.encode_state_as_update_v1(&yrs::StateVector::default())
            };
            store.save_snapshot(doc_id, &snapshot).unwrap();
        }

        // Create server pointing to same storage and recover
        let server = SyncServer::with_storage("127.0.0.1:0", &db_path);
        let recovered = server.recover().await.unwrap();
        assert_eq!(recovered, 1);

        // Verify the room exists and has content
        let rooms = server.rooms.read().await;
        assert!(rooms.contains_key(&doc_id));
        let room = rooms.get(&doc_id).unwrap();
        let txn = yrs::Transact::transact(&room.doc);
        let text = txn.get_text("test").unwrap();
        assert_eq!(text.get_string(&txn), "Hello, persistence!");
    }

    #[tokio::test]
    async fn test_document_room_creation() {
        let room = DocumentRoom::new(64);
        assert_eq!(room.broadcast.peer_count().await, 0);
        assert_eq!(room.broadcast.capacity(), 64);
    }
}
