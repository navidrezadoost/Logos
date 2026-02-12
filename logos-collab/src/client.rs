//! WebSocket sync client for connecting to the collaboration server.
//!
//! Provides:
//! - Connection lifecycle (connect, disconnect, reconnect)
//! - Delta send/receive with automatic Yrs integration
//! - Awareness (cursor/selection) updates
//! - Offline queue for disconnected edits
//!
//! Reference: Kleppmann, Chapter 5 — Replication

use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock, Mutex};
use futures_util::StreamExt;
use uuid::Uuid;

use crate::protocol::{AwarenessState, PeerInfo, ProtocolError, SyncMessage};

/// Client connection state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
}

/// Events emitted by the sync client.
#[derive(Debug, Clone)]
pub enum SyncEvent {
    /// Connection established
    Connected,
    /// Connection lost
    Disconnected,
    /// Received a CRDT delta from a remote peer
    RemoteDelta {
        peer_id: Uuid,
        clock: u64,
        update: Vec<u8>,
    },
    /// Received awareness update from a remote peer
    RemoteAwareness {
        peer_id: Uuid,
        state: AwarenessState,
    },
    /// A peer joined the document
    PeerJoined(PeerInfo),
    /// A peer left the document
    PeerLeft(Uuid),
    /// Initial state sync received
    StateSynced(Vec<u8>),
}

/// Offline queue for edits made while disconnected.
///
/// Queued deltas are replayed on reconnection.
/// Target: 1000 queued ops replay in <50ms.
pub struct OfflineQueue {
    queue: VecDeque<QueuedDelta>,
    max_size: usize,
}

#[derive(Debug, Clone)]
struct QueuedDelta {
    clock: u64,
    payload: Vec<u8>,
    #[allow(dead_code)]
    timestamp: std::time::Instant,
}

impl OfflineQueue {
    /// Create a new offline queue with max capacity.
    pub fn new(max_size: usize) -> Self {
        Self {
            queue: VecDeque::with_capacity(max_size.min(1024)),
            max_size,
        }
    }

    /// Queue a delta for later replay.
    pub fn enqueue(&mut self, clock: u64, payload: Vec<u8>) -> bool {
        if self.queue.len() >= self.max_size {
            return false; // Queue full
        }
        self.queue.push_back(QueuedDelta {
            clock,
            payload,
            timestamp: std::time::Instant::now(),
        });
        true
    }

    /// Drain all queued deltas for replay.
    pub fn drain(&mut self) -> Vec<(u64, Vec<u8>)> {
        self.queue
            .drain(..)
            .map(|d| (d.clock, d.payload))
            .collect()
    }

    /// Number of queued deltas.
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Whether the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Clear all queued deltas.
    pub fn clear(&mut self) {
        self.queue.clear();
    }

    /// Total bytes queued.
    pub fn total_bytes(&self) -> usize {
        self.queue.iter().map(|d| d.payload.len()).sum()
    }
}

/// The sync client.
///
/// Manages a WebSocket connection to the collaboration server,
/// handles delta sync, awareness updates, and offline queueing.
pub struct SyncClient {
    /// Our peer identity
    peer_info: PeerInfo,

    /// Document we're editing
    doc_id: Uuid,

    /// Connection state
    state: Arc<RwLock<ConnectionState>>,

    /// Lamport clock for causal ordering
    clock: Arc<RwLock<u64>>,

    /// Offline queue for disconnected edits
    offline_queue: Arc<Mutex<OfflineQueue>>,

    /// Channel to send messages to the WebSocket writer task
    outgoing_tx: Option<mpsc::Sender<Vec<u8>>>,

    /// Event receiver for the application
    event_rx: Option<mpsc::Receiver<SyncEvent>>,

    /// Event sender (held by connection task)
    event_tx: mpsc::Sender<SyncEvent>,

    /// Server URL
    server_url: String,
}

impl SyncClient {
    /// Create a new sync client.
    pub fn new(peer_info: PeerInfo, doc_id: Uuid, server_url: impl Into<String>) -> Self {
        let (event_tx, event_rx) = mpsc::channel(256);
        Self {
            peer_info,
            doc_id,
            state: Arc::new(RwLock::new(ConnectionState::Disconnected)),
            clock: Arc::new(RwLock::new(0)),
            offline_queue: Arc::new(Mutex::new(OfflineQueue::new(10_000))),
            outgoing_tx: None,
            event_rx: Some(event_rx),
            event_tx,
            server_url: server_url.into(),
        }
    }

    /// Take the event receiver (can only be called once).
    pub fn take_event_rx(&mut self) -> Option<mpsc::Receiver<SyncEvent>> {
        self.event_rx.take()
    }

    /// Connect to the server.
    ///
    /// Spawns background tasks for reading/writing WebSocket messages.
    pub async fn connect(&mut self) -> Result<(), ProtocolError> {
        *self.state.write().await = ConnectionState::Connecting;

        let url = format!("{}/{}", self.server_url, self.doc_id);
        let ws_result = tokio_tungstenite::connect_async(&url).await;

        match ws_result {
            Ok((ws_stream, _)) => {
                let (ws_writer, mut ws_reader) = futures_util::StreamExt::split(ws_stream);

                // Outgoing message channel
                let (out_tx, mut out_rx) = mpsc::channel::<Vec<u8>>(256);
                self.outgoing_tx = Some(out_tx);

                // Writer task: forward outgoing channel to WebSocket
                let ws_writer = Arc::new(tokio::sync::Mutex::new(ws_writer));
                let writer = ws_writer.clone();
                tokio::spawn(async move {
                    while let Some(data) = out_rx.recv().await {
                        let mut w = writer.lock().await;
                        use futures_util::SinkExt;
                        if w.send(tokio_tungstenite::tungstenite::Message::Binary(data.into()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                });

                // Send PeerJoined message
                let join_msg = SyncMessage::peer_joined(
                    self.peer_info.peer_id,
                    self.doc_id,
                    &self.peer_info,
                );
                if let Ok(encoded) = join_msg.encode() {
                    if let Some(ref tx) = self.outgoing_tx {
                        let _ = tx.send(encoded).await;
                    }
                }

                *self.state.write().await = ConnectionState::Connected;
                let _ = self.event_tx.send(SyncEvent::Connected).await;

                // Replay offline queue
                {
                    let mut queue = self.offline_queue.lock().await;
                    let queued = queue.drain();
                    if !queued.is_empty() {
                        log::info!("Replaying {} queued deltas", queued.len());
                        for (clock, payload) in queued {
                            let msg = SyncMessage::delta(
                                self.peer_info.peer_id,
                                self.doc_id,
                                clock,
                                payload,
                            );
                            if let Ok(encoded) = msg.encode() {
                                if let Some(ref tx) = self.outgoing_tx {
                                    let _ = tx.send(encoded).await;
                                }
                            }
                        }
                    }
                }

                // Reader task: process incoming WebSocket messages
                let event_tx = self.event_tx.clone();
                let state = self.state.clone();
                let peer_id = self.peer_info.peer_id;
                tokio::spawn(async move {
                    while let Some(msg) = ws_reader.next().await {
                        match msg {
                            Ok(tokio_tungstenite::tungstenite::Message::Binary(data)) => {
                                let bytes: Vec<u8> = data.into();
                                if let Ok(sync_msg) = SyncMessage::decode(&bytes) {
                                    // Skip our own messages
                                    if sync_msg.peer_id == peer_id {
                                        continue;
                                    }

                                    let event = match sync_msg.msg_type {
                                        crate::protocol::MessageType::Delta => {
                                            Some(SyncEvent::RemoteDelta {
                                                peer_id: sync_msg.peer_id,
                                                clock: sync_msg.clock,
                                                update: sync_msg.payload,
                                            })
                                        }
                                        crate::protocol::MessageType::SyncStep2 => {
                                            Some(SyncEvent::StateSynced(sync_msg.payload))
                                        }
                                        crate::protocol::MessageType::Awareness => {
                                            if let Ok(awareness_state) = sync_msg.awareness_state() {
                                                Some(SyncEvent::RemoteAwareness {
                                                    peer_id: sync_msg.peer_id,
                                                    state: awareness_state,
                                                })
                                            } else {
                                                None
                                            }
                                        }
                                        crate::protocol::MessageType::PeerJoined => {
                                            if let Ok(info) = sync_msg.peer_info() {
                                                Some(SyncEvent::PeerJoined(info))
                                            } else {
                                                None
                                            }
                                        }
                                        crate::protocol::MessageType::PeerLeft => {
                                            Some(SyncEvent::PeerLeft(sync_msg.peer_id))
                                        }
                                        _ => None,
                                    };

                                    if let Some(evt) = event {
                                        let _ = event_tx.send(evt).await;
                                    }
                                }
                            }
                            Ok(tokio_tungstenite::tungstenite::Message::Close(_)) | Err(_) => {
                                break;
                            }
                            _ => {}
                        }
                    }

                    // Connection lost
                    *state.write().await = ConnectionState::Disconnected;
                    let _ = event_tx.send(SyncEvent::Disconnected).await;
                });

                Ok(())
            }
            Err(_e) => {
                *self.state.write().await = ConnectionState::Disconnected;
                Err(ProtocolError::ConnectionClosed)
            }
        }
    }

    /// Send a CRDT delta to the server.
    ///
    /// If disconnected, queues the delta for later replay.
    pub async fn send_delta(&self, yrs_update: Vec<u8>) -> Result<(), ProtocolError> {
        let mut clock = self.clock.write().await;
        *clock += 1;
        let current_clock = *clock;

        let state = *self.state.read().await;
        if state != ConnectionState::Connected {
            // Queue for offline replay
            let mut queue = self.offline_queue.lock().await;
            if !queue.enqueue(current_clock, yrs_update) {
                return Err(ProtocolError::ConnectionClosed);
            }
            return Ok(());
        }

        let msg = SyncMessage::delta(self.peer_info.peer_id, self.doc_id, current_clock, yrs_update);
        let encoded = msg.encode()?;

        if let Some(ref tx) = self.outgoing_tx {
            tx.send(encoded)
                .await
                .map_err(|_| ProtocolError::ConnectionClosed)?;
        }

        Ok(())
    }

    /// Send an awareness update (cursor position, selection).
    pub async fn send_awareness(&self, awareness_state: &AwarenessState) -> Result<(), ProtocolError> {
        let state = *self.state.read().await;
        if state != ConnectionState::Connected {
            return Ok(()); // Silently drop awareness when offline
        }

        let clock = *self.clock.read().await;
        let msg = SyncMessage::awareness(self.peer_info.peer_id, self.doc_id, clock, awareness_state);
        let encoded = msg.encode()?;

        if let Some(ref tx) = self.outgoing_tx {
            tx.send(encoded)
                .await
                .map_err(|_| ProtocolError::ConnectionClosed)?;
        }

        Ok(())
    }

    /// Send a ping to the server.
    pub async fn send_ping(&self) -> Result<(), ProtocolError> {
        let msg = SyncMessage::ping(self.peer_info.peer_id);
        let encoded = msg.encode()?;

        if let Some(ref tx) = self.outgoing_tx {
            tx.send(encoded)
                .await
                .map_err(|_| ProtocolError::ConnectionClosed)?;
        }

        Ok(())
    }

    /// Get the current connection state.
    pub async fn connection_state(&self) -> ConnectionState {
        *self.state.read().await
    }

    /// Get our peer info.
    pub fn peer_info(&self) -> &PeerInfo {
        &self.peer_info
    }

    /// Get the document ID.
    pub fn doc_id(&self) -> Uuid {
        self.doc_id
    }

    /// Get the server URL.
    pub fn server_url(&self) -> &str {
        &self.server_url
    }

    /// Get the current Lamport clock value.
    pub async fn clock(&self) -> u64 {
        *self.clock.read().await
    }

    /// Get offline queue length.
    pub async fn offline_queue_len(&self) -> usize {
        self.offline_queue.lock().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let info = PeerInfo::new("TestUser");
        let doc_id = Uuid::new_v4();
        let client = SyncClient::new(info.clone(), doc_id, "ws://localhost:9090");

        assert_eq!(client.peer_info().name, "TestUser");
        assert_eq!(client.doc_id(), doc_id);
        assert_eq!(client.server_url(), "ws://localhost:9090");
    }

    #[tokio::test]
    async fn test_client_initial_state() {
        let info = PeerInfo::new("TestUser");
        let client = SyncClient::new(info, Uuid::new_v4(), "ws://localhost:9090");

        assert_eq!(client.connection_state().await, ConnectionState::Disconnected);
        assert_eq!(client.clock().await, 0);
        assert_eq!(client.offline_queue_len().await, 0);
    }

    #[tokio::test]
    async fn test_send_delta_offline_queues() {
        let info = PeerInfo::new("TestUser");
        let client = SyncClient::new(info, Uuid::new_v4(), "ws://localhost:9090");

        // Not connected — delta should be queued
        client.send_delta(vec![1, 2, 3]).await.unwrap();
        assert_eq!(client.offline_queue_len().await, 1);

        client.send_delta(vec![4, 5, 6]).await.unwrap();
        assert_eq!(client.offline_queue_len().await, 2);

        // Clock should have incremented
        assert_eq!(client.clock().await, 2);
    }

    #[tokio::test]
    async fn test_send_awareness_offline_noop() {
        let info = PeerInfo::new("TestUser");
        let client = SyncClient::new(info, Uuid::new_v4(), "ws://localhost:9090");

        let state = AwarenessState::default();
        // Should not error when offline
        client.send_awareness(&state).await.unwrap();
    }

    #[test]
    fn test_offline_queue() {
        let mut queue = OfflineQueue::new(100);
        assert!(queue.is_empty());
        assert_eq!(queue.len(), 0);

        queue.enqueue(1, vec![1, 2, 3]);
        queue.enqueue(2, vec![4, 5, 6, 7]);

        assert_eq!(queue.len(), 2);
        assert_eq!(queue.total_bytes(), 7);

        let drained = queue.drain();
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].0, 1); // clock
        assert_eq!(drained[0].1, vec![1, 2, 3]); // payload
        assert!(queue.is_empty());
    }

    #[test]
    fn test_offline_queue_capacity() {
        let mut queue = OfflineQueue::new(3);

        assert!(queue.enqueue(1, vec![1]));
        assert!(queue.enqueue(2, vec![2]));
        assert!(queue.enqueue(3, vec![3]));
        assert!(!queue.enqueue(4, vec![4])); // Full

        assert_eq!(queue.len(), 3);
    }

    #[test]
    fn test_offline_queue_clear() {
        let mut queue = OfflineQueue::new(100);
        queue.enqueue(1, vec![1]);
        queue.enqueue(2, vec![2]);
        queue.clear();
        assert!(queue.is_empty());
    }

    #[test]
    fn test_connection_state_values() {
        assert_ne!(ConnectionState::Disconnected, ConnectionState::Connected);
        assert_ne!(ConnectionState::Connecting, ConnectionState::Reconnecting);
    }

    #[tokio::test]
    async fn test_take_event_rx() {
        let info = PeerInfo::new("TestUser");
        let mut client = SyncClient::new(info, Uuid::new_v4(), "ws://localhost:9090");

        // First take should succeed
        assert!(client.take_event_rx().is_some());
        // Second take should return None
        assert!(client.take_event_rx().is_none());
    }
}
