//! Sync protocol — op-based broadcast, acknowledgement, and ordering.
//!
//! Defines the message types for broadcasting `OpEnvelope`s between
//! peers, acknowledging receipt, and maintaining causal ordering
//! via vector clocks from `logos-replay`.

use logos_replay::{LamportClock, VectorClock};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::MultiplayerError;
use crate::peer::PeerId;

/// A sync message exchanged between peers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncMessage {
    /// Broadcast an operation to all peers.
    OpBroadcast(OpBroadcast),
    /// Acknowledge receipt of an operation.
    Ack(SyncAck),
    /// Request catch-up from a specific version.
    CatchUpRequest { from_version: u64, document_id: Uuid },
    /// Offer a snapshot for fast join.
    SnapshotOffer { version: u64, document_id: Uuid },
    /// Presence update (cursor, selection, viewport).
    Presence(PresenceUpdate),
    /// Peer joined the session.
    Join { peer_id: PeerId, document_id: Uuid, display_name: String },
    /// Peer left the session.
    Leave { peer_id: PeerId },
    /// Heartbeat ping.
    Ping { peer_id: PeerId, timestamp: u64 },
    /// Heartbeat pong.
    Pong { peer_id: PeerId, timestamp: u64 },
}

/// Broadcast of a single operation to peers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpBroadcast {
    /// Who sent this operation.
    pub sender: PeerId,
    /// Document this op targets.
    pub document_id: Uuid,
    /// Monotonically increasing version within this document.
    pub version: u64,
    /// Lamport clock for partial ordering.
    pub lamport: u64,
    /// The serialized operation payload.
    pub payload: serde_json::Value,
    /// Timestamp (Unix seconds).
    pub timestamp: u64,
    /// Human-readable description.
    pub description: Option<String>,
    /// Domain tag (e.g., "design", "comment").
    pub domain: String,
}

/// Acknowledgement that a peer received an operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncAck {
    /// Who is acknowledging.
    pub peer_id: PeerId,
    /// The version being acknowledged.
    pub version: u64,
    /// Document ID.
    pub document_id: Uuid,
    /// Peer's Lamport clock after receiving.
    pub lamport: u64,
}

/// A presence update message (lightweight, high-frequency).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresenceUpdate {
    pub peer_id: PeerId,
    pub document_id: Uuid,
    pub cursor_x: f32,
    pub cursor_y: f32,
    pub selection: Vec<Uuid>,
    pub viewport: Option<ViewportRect>,
    pub timestamp: u64,
}

/// A viewport rectangle (what the peer can see).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ViewportRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl ViewportRect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }

    /// Whether a point is inside this viewport.
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px <= self.x + self.width && py >= self.y && py <= self.y + self.height
    }

    /// Whether two viewports overlap.
    pub fn overlaps(&self, other: &ViewportRect) -> bool {
        self.x < other.x + other.width
            && self.x + self.width > other.x
            && self.y < other.y + other.height
            && self.y + self.height > other.y
    }
}

/// The sync protocol engine — manages message ordering and ack tracking.
pub struct SyncProtocol {
    /// Local peer's ID.
    local_peer: PeerId,
    /// Document being synced.
    document_id: Uuid,
    /// Local Lamport clock.
    lamport: LamportClock,
    /// Local vector clock (one entry per peer).
    vector_clock: VectorClock,
    /// Version counter for outgoing ops.
    next_version: u64,
    /// Track which peers have acknowledged which version.
    ack_tracker: std::collections::HashMap<u64, Vec<PeerId>>,
    /// Outbox of messages to send.
    outbox: Vec<SyncMessage>,
}

impl SyncProtocol {
    pub fn new(local_peer: PeerId, document_id: Uuid, initial_version: u64) -> Self {
        Self {
            local_peer,
            document_id,
            lamport: LamportClock::new(),
            vector_clock: VectorClock::new(),
            next_version: initial_version + 1,
            ack_tracker: std::collections::HashMap::new(),
            outbox: Vec::new(),
        }
    }

    /// Create and queue a broadcast for a local operation.
    pub fn broadcast_op(
        &mut self,
        payload: serde_json::Value,
        domain: impl Into<String>,
        description: Option<String>,
    ) -> OpBroadcast {
        let version = self.next_version;
        self.next_version += 1;
        self.lamport.tick();
        self.vector_clock
            .tick(self.local_peer.0.as_u128() as u64);

        let broadcast = OpBroadcast {
            sender: self.local_peer,
            document_id: self.document_id,
            version,
            lamport: self.lamport.value(),
            payload,
            timestamp: now(),
            description,
            domain: domain.into(),
        };

        self.outbox
            .push(SyncMessage::OpBroadcast(broadcast.clone()));
        broadcast
    }

    /// Handle a received broadcast from a remote peer.
    pub fn receive_broadcast(
        &mut self,
        broadcast: &OpBroadcast,
    ) -> Result<SyncAck, MultiplayerError> {
        // Advance our Lamport clock.
        self.lamport.merge(broadcast.lamport);
        self.vector_clock
            .tick(broadcast.sender.0.as_u128() as u64);

        // Update our next_version if we're behind.
        if broadcast.version >= self.next_version {
            self.next_version = broadcast.version + 1;
        }

        // Generate an ack.
        let ack = SyncAck {
            peer_id: self.local_peer,
            version: broadcast.version,
            document_id: broadcast.document_id,
            lamport: self.lamport.value(),
        };

        self.outbox.push(SyncMessage::Ack(ack.clone()));
        Ok(ack)
    }

    /// Record an acknowledgement from a remote peer.
    pub fn receive_ack(&mut self, ack: &SyncAck) {
        self.lamport.merge(ack.lamport);
        self.ack_tracker
            .entry(ack.version)
            .or_default()
            .push(ack.peer_id);
    }

    /// Check if a version has been acknowledged by all given peers.
    pub fn is_fully_acked(&self, version: u64, peers: &[PeerId]) -> bool {
        if let Some(acked) = self.ack_tracker.get(&version) {
            peers.iter().all(|p| acked.contains(p))
        } else {
            false
        }
    }

    /// Drain the outbox — returns all queued messages.
    pub fn drain_outbox(&mut self) -> Vec<SyncMessage> {
        std::mem::take(&mut self.outbox)
    }

    /// Current Lamport clock value.
    pub fn lamport_value(&self) -> u64 {
        self.lamport.value()
    }

    /// Current version counter.
    pub fn current_version(&self) -> u64 {
        self.next_version.saturating_sub(1)
    }

    /// Number of unacknowledged versions.
    pub fn pending_acks(&self) -> usize {
        self.ack_tracker.len()
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn setup_two_peers() -> (SyncProtocol, SyncProtocol, Uuid) {
        let doc = Uuid::new_v4();
        let peer_a = PeerId::new();
        let peer_b = PeerId::new();
        let proto_a = SyncProtocol::new(peer_a, doc, 0);
        let proto_b = SyncProtocol::new(peer_b, doc, 0);
        (proto_a, proto_b, doc)
    }

    #[test]
    fn broadcast_op_increments_version() {
        let (mut proto, _, _) = setup_two_peers();
        let op1 = proto.broadcast_op(json!({"add": "rect"}), "design", None);
        assert_eq!(op1.version, 1);
        let op2 = proto.broadcast_op(json!({"add": "circle"}), "design", None);
        assert_eq!(op2.version, 2);
        assert_eq!(proto.current_version(), 2);
    }

    #[test]
    fn broadcast_op_increments_lamport() {
        let (mut proto, _, _) = setup_two_peers();
        proto.broadcast_op(json!({}), "design", None);
        assert!(proto.lamport_value() > 0);
    }

    #[test]
    fn receive_broadcast_generates_ack() {
        let (mut a, mut b, _) = setup_two_peers();
        let op = a.broadcast_op(json!({"move": "layer1"}), "design", Some("Move layer".into()));
        let ack = b.receive_broadcast(&op).unwrap();
        assert_eq!(ack.version, op.version);
    }

    #[test]
    fn receive_broadcast_advances_clock() {
        let (mut a, mut b, _) = setup_two_peers();
        let op = a.broadcast_op(json!({}), "design", None);
        let before = b.lamport_value();
        b.receive_broadcast(&op).unwrap();
        assert!(b.lamport_value() > before);
    }

    #[test]
    fn receive_broadcast_updates_version() {
        let (mut a, mut b, _) = setup_two_peers();
        let op = a.broadcast_op(json!({}), "design", None);
        b.receive_broadcast(&op).unwrap();
        assert_eq!(b.current_version(), 1);
    }

    #[test]
    fn ack_tracking() {
        let (mut a, mut b, _) = setup_two_peers();
        let peer_b_id = b.local_peer;
        let op = a.broadcast_op(json!({}), "design", None);
        let ack = b.receive_broadcast(&op).unwrap();
        a.receive_ack(&ack);
        assert!(a.is_fully_acked(1, &[peer_b_id]));
    }

    #[test]
    fn not_fully_acked_without_all_peers() {
        let (mut a, mut b, _doc) = setup_two_peers();
        let peer_b_id = b.local_peer;
        let peer_c = PeerId::new();
        let op = a.broadcast_op(json!({}), "design", None);
        let ack = b.receive_broadcast(&op).unwrap();
        a.receive_ack(&ack);
        assert!(!a.is_fully_acked(1, &[peer_b_id, peer_c]));
    }

    #[test]
    fn drain_outbox() {
        let (mut a, _, _) = setup_two_peers();
        a.broadcast_op(json!({}), "design", None);
        a.broadcast_op(json!({}), "design", None);
        let msgs = a.drain_outbox();
        assert_eq!(msgs.len(), 2);
        assert!(a.drain_outbox().is_empty());
    }

    #[test]
    fn two_way_sync() {
        let (mut a, mut b, _) = setup_two_peers();
        // A sends to B.
        let op1 = a.broadcast_op(json!({"from": "A"}), "design", None);
        let ack1 = b.receive_broadcast(&op1).unwrap();
        a.receive_ack(&ack1);
        // B sends to A.
        let op2 = b.broadcast_op(json!({"from": "B"}), "design", None);
        let ack2 = a.receive_broadcast(&op2).unwrap();
        b.receive_ack(&ack2);
        // Both at version 2.
        assert_eq!(a.current_version(), 2);
        assert_eq!(b.current_version(), 2);
    }

    #[test]
    fn op_broadcast_serde() {
        let (mut a, _, _) = setup_two_peers();
        let op = a.broadcast_op(json!({"key": "value"}), "design", Some("Test".into()));
        let json = serde_json::to_string(&op).unwrap();
        let back: OpBroadcast = serde_json::from_str(&json).unwrap();
        assert_eq!(back.version, op.version);
        assert_eq!(back.domain, "design");
    }

    #[test]
    fn sync_message_variants_serde() {
        let msg = SyncMessage::Join {
            peer_id: PeerId::new(),
            document_id: Uuid::new_v4(),
            display_name: "Alice".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: SyncMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, SyncMessage::Join { .. }));
    }

    #[test]
    fn viewport_contains() {
        let vp = ViewportRect::new(0.0, 0.0, 100.0, 100.0);
        assert!(vp.contains(50.0, 50.0));
        assert!(!vp.contains(150.0, 50.0));
    }

    #[test]
    fn viewport_overlaps() {
        let a = ViewportRect::new(0.0, 0.0, 100.0, 100.0);
        let b = ViewportRect::new(50.0, 50.0, 100.0, 100.0);
        let c = ViewportRect::new(200.0, 200.0, 50.0, 50.0);
        assert!(a.overlaps(&b));
        assert!(!a.overlaps(&c));
    }

    #[test]
    fn presence_update_serde() {
        let update = PresenceUpdate {
            peer_id: PeerId::new(),
            document_id: Uuid::new_v4(),
            cursor_x: 100.0,
            cursor_y: 200.0,
            selection: vec![Uuid::new_v4()],
            viewport: Some(ViewportRect::new(0.0, 0.0, 800.0, 600.0)),
            timestamp: 12345,
        };
        let json = serde_json::to_string(&update).unwrap();
        let back: PresenceUpdate = serde_json::from_str(&json).unwrap();
        assert_eq!(back.cursor_x, 100.0);
    }
}
