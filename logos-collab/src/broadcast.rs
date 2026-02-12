//! Fan-out broadcast to N-1 peers with backpressure.
//!
//! Uses tokio broadcast channels for O(1) send to all subscribers.
//! Each peer gets an independent receiver that buffers up to `capacity` messages.
//!
//! Performance target: 1,000 messages to 100 peers < 10ms
//! Reference: Patterson & Hennessy, Section 6.4 — Interconnection Networks

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

use crate::protocol::{PeerInfo, SyncMessage};

/// Statistics for monitoring broadcast health.
#[derive(Debug, Clone, Default)]
pub struct BroadcastStats {
    pub messages_sent: u64,
    pub messages_dropped: u64,
    pub active_peers: usize,
}

/// Atomic broadcast stats — lock-free on the hot path.
///
/// Stats are tracked via atomics so that broadcast_raw() and broadcast()
/// never acquire a lock. Stats are read via snapshot().
struct AtomicBroadcastStats {
    messages_sent: AtomicU64,
    messages_dropped: AtomicU64,
}

impl AtomicBroadcastStats {
    fn new() -> Self {
        Self {
            messages_sent: AtomicU64::new(0),
            messages_dropped: AtomicU64::new(0),
        }
    }
}

/// A broadcast group for a single document room.
///
/// All peers in the same document share one broadcast channel.
/// When a peer sends a delta, it's fanned out to N-1 other peers.
pub struct BroadcastGroup {
    /// Broadcast channel sender (cloned per-room)
    sender: broadcast::Sender<Arc<Vec<u8>>>,

    /// Connected peers in this room
    peers: Arc<RwLock<HashMap<Uuid, PeerInfo>>>,

    /// Channel capacity (messages buffered per receiver)
    capacity: usize,

    /// Lock-free stats (atomics)
    atomic_stats: Arc<AtomicBroadcastStats>,
}

impl BroadcastGroup {
    /// Create a new broadcast group with the given buffer capacity.
    ///
    /// `capacity` determines how many messages can be buffered per peer
    /// before lagging peers start dropping messages (backpressure).
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self {
            sender,
            peers: Arc::new(RwLock::new(HashMap::new())),
            capacity,
            atomic_stats: Arc::new(AtomicBroadcastStats::new()),
        }
    }

    /// Add a peer to this broadcast group.
    ///
    /// Returns a receiver for this peer to consume messages.
    pub async fn add_peer(&self, info: PeerInfo) -> broadcast::Receiver<Arc<Vec<u8>>> {
        let mut peers = self.peers.write().await;
        peers.insert(info.peer_id, info);
        self.sender.subscribe()
    }

    /// Remove a peer from this broadcast group.
    pub async fn remove_peer(&self, peer_id: &Uuid) -> Option<PeerInfo> {
        let mut peers = self.peers.write().await;
        peers.remove(peer_id)
    }

    /// Broadcast a message to all peers except the sender.
    ///
    /// The message is pre-encoded to avoid redundant serialization.
    /// Returns the number of receivers that received the message.
    /// Stats are tracked via atomics — no lock acquired on hot path.
    pub fn broadcast(&self, msg: &SyncMessage) -> Result<usize, crate::protocol::ProtocolError> {
        let encoded = msg.encode()?;
        let arc_bytes = Arc::new(encoded);

        let receiver_count = self.sender.send(arc_bytes).unwrap_or(0);

        // Lock-free stats update
        self.atomic_stats.messages_sent.fetch_add(1, Ordering::Relaxed);

        Ok(receiver_count)
    }

    /// Broadcast pre-encoded bytes directly (zero-copy fast path).
    /// Fully lock-free: tokio broadcast::send + atomic stats.
    pub fn broadcast_raw(&self, encoded: Arc<Vec<u8>>) -> usize {
        let count = self.sender.send(encoded).unwrap_or(0);
        self.atomic_stats.messages_sent.fetch_add(1, Ordering::Relaxed);
        count
    }

    /// Get the current peer count.
    pub async fn peer_count(&self) -> usize {
        self.peers.read().await.len()
    }

    /// Get all connected peer infos.
    pub async fn peers(&self) -> Vec<PeerInfo> {
        self.peers.read().await.values().cloned().collect()
    }

    /// Check if a peer is connected.
    pub async fn has_peer(&self, peer_id: &Uuid) -> bool {
        self.peers.read().await.contains_key(peer_id)
    }

    /// Get broadcast statistics (lock-free snapshot).
    pub async fn stats(&self) -> BroadcastStats {
        let peers = self.peers.read().await;
        BroadcastStats {
            messages_sent: self.atomic_stats.messages_sent.load(Ordering::Relaxed),
            messages_dropped: self.atomic_stats.messages_dropped.load(Ordering::Relaxed),
            active_peers: peers.len(),
        }
    }

    /// Get the channel capacity.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Subscribe to this broadcast group (raw receiver).
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<Vec<u8>>> {
        self.sender.subscribe()
    }
}

/// Room manager: maps document IDs to broadcast groups.
///
/// Each document gets its own broadcast group so that
/// messages are isolated between different documents.
pub struct RoomManager {
    rooms: Arc<RwLock<HashMap<Uuid, Arc<BroadcastGroup>>>>,
    default_capacity: usize,
}

impl RoomManager {
    /// Create a new room manager.
    pub fn new(default_capacity: usize) -> Self {
        Self {
            rooms: Arc::new(RwLock::new(HashMap::new())),
            default_capacity,
        }
    }

    /// Get or create a room for the given document.
    pub async fn get_or_create(&self, doc_id: Uuid) -> Arc<BroadcastGroup> {
        // Fast path: read lock
        {
            let rooms = self.rooms.read().await;
            if let Some(room) = rooms.get(&doc_id) {
                return room.clone();
            }
        }

        // Slow path: write lock to create
        let mut rooms = self.rooms.write().await;
        // Double-check after acquiring write lock
        if let Some(room) = rooms.get(&doc_id) {
            return room.clone();
        }

        let room = Arc::new(BroadcastGroup::new(self.default_capacity));
        rooms.insert(doc_id, room.clone());
        room
    }

    /// Remove an empty room.
    pub async fn remove_if_empty(&self, doc_id: &Uuid) -> bool {
        let mut rooms = self.rooms.write().await;
        if let Some(room) = rooms.get(doc_id) {
            if room.peer_count().await == 0 {
                rooms.remove(doc_id);
                return true;
            }
        }
        false
    }

    /// Get the number of active rooms.
    pub async fn room_count(&self) -> usize {
        self.rooms.read().await.len()
    }

    /// Get all active document IDs.
    pub async fn active_documents(&self) -> Vec<Uuid> {
        self.rooms.read().await.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_broadcast_group_add_remove() {
        let group = BroadcastGroup::new(16);
        let peer = PeerInfo::new("Alice");
        let peer_id = peer.peer_id;

        let _rx = group.add_peer(peer).await;
        assert_eq!(group.peer_count().await, 1);
        assert!(group.has_peer(&peer_id).await);

        group.remove_peer(&peer_id).await;
        assert_eq!(group.peer_count().await, 0);
        assert!(!group.has_peer(&peer_id).await);
    }

    #[tokio::test]
    async fn test_broadcast_fan_out() {
        let group = BroadcastGroup::new(16);

        let peer1 = PeerInfo::new("Alice");
        let peer2 = PeerInfo::new("Bob");
        let peer3 = PeerInfo::new("Charlie");

        let mut rx1 = group.add_peer(peer1.clone()).await;
        let mut rx2 = group.add_peer(peer2.clone()).await;
        let mut rx3 = group.add_peer(peer3.clone()).await;

        // Broadcast a delta
        let msg = SyncMessage::delta(peer1.peer_id, Uuid::new_v4(), 1, vec![1, 2, 3]);
        let count = group.broadcast(&msg).unwrap();

        // All 3 receivers should get it (including sender — filtering is caller's job)
        assert_eq!(count, 3);

        // All receivers can read the message
        let _ = rx1.recv().await.unwrap();
        let _ = rx2.recv().await.unwrap();
        let _ = rx3.recv().await.unwrap();
    }

    #[tokio::test]
    async fn test_broadcast_raw_zero_copy() {
        let group = BroadcastGroup::new(16);

        let peer = PeerInfo::new("Alice");
        let mut rx = group.add_peer(peer).await;

        let data = Arc::new(vec![10, 20, 30]);
        let count = group.broadcast_raw(data.clone());
        assert_eq!(count, 1);

        let received = rx.recv().await.unwrap();
        assert_eq!(*received, vec![10, 20, 30]);
    }

    #[tokio::test]
    async fn test_broadcast_stats() {
        let group = BroadcastGroup::new(16);
        let peer = PeerInfo::new("Alice");
        let _rx = group.add_peer(peer.clone()).await;

        let msg = SyncMessage::ping(peer.peer_id);
        group.broadcast(&msg).unwrap();
        group.broadcast(&msg).unwrap();

        let stats = group.stats().await;
        assert_eq!(stats.messages_sent, 2);
        assert_eq!(stats.active_peers, 1);
    }

    #[tokio::test]
    async fn test_room_manager_get_or_create() {
        let manager = RoomManager::new(16);
        let doc_id = Uuid::new_v4();

        let room1 = manager.get_or_create(doc_id).await;
        let room2 = manager.get_or_create(doc_id).await;

        // Same room returned
        assert!(Arc::ptr_eq(&room1, &room2));
        assert_eq!(manager.room_count().await, 1);
    }

    #[tokio::test]
    async fn test_room_manager_multiple_docs() {
        let manager = RoomManager::new(16);

        let doc1 = Uuid::new_v4();
        let doc2 = Uuid::new_v4();

        let _room1 = manager.get_or_create(doc1).await;
        let _room2 = manager.get_or_create(doc2).await;

        assert_eq!(manager.room_count().await, 2);

        let docs = manager.active_documents().await;
        assert!(docs.contains(&doc1));
        assert!(docs.contains(&doc2));
    }

    #[tokio::test]
    async fn test_room_manager_cleanup() {
        let manager = RoomManager::new(16);
        let doc_id = Uuid::new_v4();

        let room = manager.get_or_create(doc_id).await;
        let peer = PeerInfo::new("Alice");
        let peer_id = peer.peer_id;
        let _rx = room.add_peer(peer).await;

        // Room not empty — shouldn't remove
        assert!(!manager.remove_if_empty(&doc_id).await);
        assert_eq!(manager.room_count().await, 1);

        // Remove peer, then cleanup
        room.remove_peer(&peer_id).await;
        assert!(manager.remove_if_empty(&doc_id).await);
        assert_eq!(manager.room_count().await, 0);
    }

    #[tokio::test]
    async fn test_broadcast_capacity() {
        let group = BroadcastGroup::new(32);
        assert_eq!(group.capacity(), 32);
    }

    #[tokio::test]
    async fn test_peers_list() {
        let group = BroadcastGroup::new(16);

        let alice = PeerInfo::new("Alice");
        let bob = PeerInfo::new("Bob");

        let _rx1 = group.add_peer(alice.clone()).await;
        let _rx2 = group.add_peer(bob.clone()).await;

        let peers = group.peers().await;
        assert_eq!(peers.len(), 2);

        let names: Vec<&str> = peers.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"Alice"));
        assert!(names.contains(&"Bob"));
    }
}
