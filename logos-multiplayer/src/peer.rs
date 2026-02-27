//! Peer — identity and state tracking for each connected participant.
//!
//! Every user in a collaboration session is a `Peer`. Peers carry
//! their identity, current version, connection state, and metadata.

use logos_identity::UserId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::MultiplayerError;

/// Unique identifier for a peer in a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PeerId(pub Uuid);

impl PeerId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }
}

impl Default for PeerId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for PeerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Connection state of a peer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PeerState {
    /// Connected and actively syncing.
    Active,
    /// Connected but no activity recently.
    Idle,
    /// Disconnected — may have offline ops.
    Disconnected,
    /// Reconnecting after a disconnect.
    Reconnecting,
}

impl std::fmt::Display for PeerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Active => write!(f, "Active"),
            Self::Idle => write!(f, "Idle"),
            Self::Disconnected => write!(f, "Disconnected"),
            Self::Reconnecting => write!(f, "Reconnecting"),
        }
    }
}

/// A peer in a collaboration session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Peer {
    /// Unique peer identifier.
    pub id: PeerId,
    /// The user behind this peer.
    pub user_id: UserId,
    /// Display name.
    pub display_name: String,
    /// Current connection state.
    pub state: PeerState,
    /// The document this peer is editing.
    pub document_id: Uuid,
    /// Last known version this peer has.
    pub last_version: u64,
    /// When the peer joined (Unix seconds).
    pub joined_at: u64,
    /// When the peer was last active.
    pub last_active_at: u64,
    /// RGBA color for cursor/selection rendering.
    pub color: [f32; 4],
}

impl Peer {
    pub fn new(
        user_id: UserId,
        display_name: impl Into<String>,
        document_id: Uuid,
    ) -> Self {
        let id = PeerId::new();
        let color = Self::color_from_id(&id);
        let now = now();
        Self {
            id,
            user_id,
            display_name: display_name.into(),
            state: PeerState::Active,
            document_id,
            last_version: 0,
            joined_at: now,
            last_active_at: now,
            color,
        }
    }

    /// Generate a stable color from the peer ID.
    fn color_from_id(id: &PeerId) -> [f32; 4] {
        let hash = id.0.as_u128();
        let r = ((hash >> 0) & 0xFF) as f32 / 255.0;
        let g = ((hash >> 8) & 0xFF) as f32 / 255.0;
        let b = ((hash >> 16) & 0xFF) as f32 / 255.0;
        [r, g, b, 1.0]
    }

    /// Update the peer's version.
    pub fn advance_to(&mut self, version: u64) {
        self.last_version = version;
        self.last_active_at = now();
    }

    /// Mark peer as idle.
    pub fn mark_idle(&mut self) {
        self.state = PeerState::Idle;
    }

    /// Mark peer as disconnected.
    pub fn disconnect(&mut self) {
        self.state = PeerState::Disconnected;
    }

    /// Mark peer as reconnecting.
    pub fn reconnecting(&mut self) {
        self.state = PeerState::Reconnecting;
    }

    /// Mark peer as active.
    pub fn activate(&mut self) {
        self.state = PeerState::Active;
        self.last_active_at = now();
    }

    /// Whether the peer is connected (Active or Idle).
    pub fn is_connected(&self) -> bool {
        matches!(self.state, PeerState::Active | PeerState::Idle)
    }

    /// How far behind the peer is from the given version.
    pub fn versions_behind(&self, current_version: u64) -> u64 {
        current_version.saturating_sub(self.last_version)
    }
}

/// Registry of all peers in a document session.
#[derive(Debug, Default)]
pub struct PeerRegistry {
    peers: Vec<Peer>,
}

impl PeerRegistry {
    pub fn new() -> Self {
        Self { peers: Vec::new() }
    }

    /// Add a peer. Errors if peer_id is already registered.
    pub fn add(&mut self, peer: Peer) -> Result<PeerId, MultiplayerError> {
        if self.peers.iter().any(|p| p.id == peer.id) {
            return Err(MultiplayerError::DuplicatePeer {
                id: peer.id.to_string(),
            });
        }
        let id = peer.id;
        self.peers.push(peer);
        Ok(id)
    }

    /// Get a peer by ID.
    pub fn get(&self, id: &PeerId) -> Result<&Peer, MultiplayerError> {
        self.peers
            .iter()
            .find(|p| p.id == *id)
            .ok_or(MultiplayerError::PeerNotFound {
                id: id.to_string(),
            })
    }

    /// Get a mutable peer by ID.
    pub fn get_mut(&mut self, id: &PeerId) -> Result<&mut Peer, MultiplayerError> {
        self.peers
            .iter_mut()
            .find(|p| p.id == *id)
            .ok_or(MultiplayerError::PeerNotFound {
                id: id.to_string(),
            })
    }

    /// Remove a peer.
    pub fn remove(&mut self, id: &PeerId) -> Result<Peer, MultiplayerError> {
        let idx = self
            .peers
            .iter()
            .position(|p| p.id == *id)
            .ok_or(MultiplayerError::PeerNotFound {
                id: id.to_string(),
            })?;
        Ok(self.peers.remove(idx))
    }

    /// All connected peers (Active or Idle).
    pub fn connected(&self) -> Vec<&Peer> {
        self.peers.iter().filter(|p| p.is_connected()).collect()
    }

    /// All peers for a specific document.
    pub fn for_document(&self, doc_id: &Uuid) -> Vec<&Peer> {
        self.peers
            .iter()
            .filter(|p| p.document_id == *doc_id)
            .collect()
    }

    /// Total number of peers.
    pub fn len(&self) -> usize {
        self.peers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.peers.is_empty()
    }

    /// Find the peer that is most behind.
    pub fn slowest_peer(&self, doc_id: &Uuid) -> Option<&Peer> {
        self.peers
            .iter()
            .filter(|p| p.document_id == *doc_id && p.is_connected())
            .min_by_key(|p| p.last_version)
    }

    /// Detect idle peers (not active for `threshold_secs`).
    pub fn detect_idle(&mut self, threshold_secs: u64) {
        let cutoff = now().saturating_sub(threshold_secs);
        for peer in &mut self.peers {
            if peer.state == PeerState::Active && peer.last_active_at < cutoff {
                peer.state = PeerState::Idle;
            }
        }
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

    fn make_peer(name: &str, doc: Uuid) -> Peer {
        Peer::new(UserId::new(), name, doc)
    }

    #[test]
    fn peer_creation() {
        let doc = Uuid::new_v4();
        let p = make_peer("Alice", doc);
        assert_eq!(p.display_name, "Alice");
        assert_eq!(p.document_id, doc);
        assert_eq!(p.state, PeerState::Active);
        assert_eq!(p.last_version, 0);
        assert!(p.is_connected());
    }

    #[test]
    fn peer_advance() {
        let mut p = make_peer("Bob", Uuid::new_v4());
        p.advance_to(10);
        assert_eq!(p.last_version, 10);
    }

    #[test]
    fn peer_versions_behind() {
        let mut p = make_peer("Charlie", Uuid::new_v4());
        p.advance_to(5);
        assert_eq!(p.versions_behind(10), 5);
        assert_eq!(p.versions_behind(3), 0);
    }

    #[test]
    fn peer_state_transitions() {
        let mut p = make_peer("Alice", Uuid::new_v4());
        assert!(p.is_connected());
        p.mark_idle();
        assert_eq!(p.state, PeerState::Idle);
        assert!(p.is_connected()); // idle is still connected
        p.disconnect();
        assert!(!p.is_connected());
        p.reconnecting();
        assert_eq!(p.state, PeerState::Reconnecting);
        p.activate();
        assert!(p.is_connected());
    }

    #[test]
    fn peer_color_deterministic() {
        let id = PeerId::new();
        let c1 = Peer::color_from_id(&id);
        let c2 = Peer::color_from_id(&id);
        assert_eq!(c1, c2);
    }

    #[test]
    fn peer_serde_roundtrip() {
        let p = make_peer("Alice", Uuid::new_v4());
        let json = serde_json::to_string(&p).unwrap();
        let back: Peer = serde_json::from_str(&json).unwrap();
        assert_eq!(back.display_name, "Alice");
    }

    #[test]
    fn state_display() {
        assert_eq!(PeerState::Active.to_string(), "Active");
        assert_eq!(PeerState::Disconnected.to_string(), "Disconnected");
    }

    #[test]
    fn registry_add_and_get() {
        let mut reg = PeerRegistry::new();
        let doc = Uuid::new_v4();
        let p = make_peer("Alice", doc);
        let id = reg.add(p).unwrap();
        assert_eq!(reg.get(&id).unwrap().display_name, "Alice");
    }

    #[test]
    fn registry_duplicate() {
        let mut reg = PeerRegistry::new();
        let p = make_peer("Alice", Uuid::new_v4());
        let p2 = p.clone();
        reg.add(p).unwrap();
        assert!(reg.add(p2).is_err());
    }

    #[test]
    fn registry_remove() {
        let mut reg = PeerRegistry::new();
        let p = make_peer("Alice", Uuid::new_v4());
        let id = reg.add(p).unwrap();
        assert_eq!(reg.len(), 1);
        reg.remove(&id).unwrap();
        assert!(reg.is_empty());
    }

    #[test]
    fn registry_connected() {
        let mut reg = PeerRegistry::new();
        let doc = Uuid::new_v4();
        let id1 = reg.add(make_peer("Alice", doc)).unwrap();
        let _id2 = reg.add(make_peer("Bob", doc)).unwrap();
        reg.get_mut(&id1).unwrap().disconnect();
        assert_eq!(reg.connected().len(), 1);
    }

    #[test]
    fn registry_for_document() {
        let mut reg = PeerRegistry::new();
        let doc1 = Uuid::new_v4();
        let doc2 = Uuid::new_v4();
        reg.add(make_peer("Alice", doc1)).unwrap();
        reg.add(make_peer("Bob", doc1)).unwrap();
        reg.add(make_peer("Charlie", doc2)).unwrap();
        assert_eq!(reg.for_document(&doc1).len(), 2);
        assert_eq!(reg.for_document(&doc2).len(), 1);
    }

    #[test]
    fn registry_slowest_peer() {
        let mut reg = PeerRegistry::new();
        let doc = Uuid::new_v4();
        let id1 = reg.add(make_peer("Alice", doc)).unwrap();
        let id2 = reg.add(make_peer("Bob", doc)).unwrap();
        reg.get_mut(&id1).unwrap().advance_to(10);
        reg.get_mut(&id2).unwrap().advance_to(5);
        let slowest = reg.slowest_peer(&doc).unwrap();
        assert_eq!(slowest.display_name, "Bob");
    }

    #[test]
    fn registry_detect_idle() {
        let mut reg = PeerRegistry::new();
        let doc = Uuid::new_v4();
        let id = reg.add(make_peer("Alice", doc)).unwrap();
        // Artificially set last_active_at to the past.
        reg.get_mut(&id).unwrap().last_active_at = 0;
        reg.detect_idle(60);
        assert_eq!(reg.get(&id).unwrap().state, PeerState::Idle);
    }
}
