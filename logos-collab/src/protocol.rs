//! Binary protocol for CRDT delta synchronization.
//!
//! Wire format (bincode-encoded):
//! ```text
//! ┌──────────┬───────────┬──────────┬──────────┬──────────┐
//! │ msg_type │ peer_id   │ doc_id   │ clock    │ payload  │
//! │ 1 byte   │ 16 bytes  │ 16 bytes │ 8 bytes  │ variable │
//! └──────────┴───────────┴──────────┴──────────┴──────────┘
//! ```
//!
//! Performance target: serialization < 500ns for typical delta.
//! Reference: Patterson & Hennessy, Section 5.7 — Data Compression

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Message types for the sync protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum MessageType {
    /// Yrs state vector for initial sync handshake
    SyncStep1 = 1,
    /// Yrs state diff response
    SyncStep2 = 2,
    /// Incremental CRDT delta update
    Delta = 3,
    /// Cursor/selection awareness update
    Awareness = 4,
    /// Peer joined notification
    PeerJoined = 5,
    /// Peer left notification
    PeerLeft = 6,
    /// Heartbeat ping
    Ping = 7,
    /// Heartbeat pong
    Pong = 8,
}

/// Peer identity with display metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PeerInfo {
    pub peer_id: Uuid,
    pub name: String,
    /// RGBA color for cursor/selection rendering
    pub color: [f32; 4],
}

impl PeerInfo {
    pub fn new(name: impl Into<String>) -> Self {
        let peer_id = Uuid::new_v4();
        // Stable color from peer_id hash
        let hash = peer_id.as_u128();
        let r = ((hash >> 0) & 0xFF) as f32 / 255.0;
        let g = ((hash >> 8) & 0xFF) as f32 / 255.0;
        let b = ((hash >> 16) & 0xFF) as f32 / 255.0;
        Self {
            peer_id,
            name: name.into(),
            color: [r, g, b, 1.0],
        }
    }

    /// Create with explicit peer_id (for testing)
    pub fn with_id(peer_id: Uuid, name: impl Into<String>) -> Self {
        let hash = peer_id.as_u128();
        let r = ((hash >> 0) & 0xFF) as f32 / 255.0;
        let g = ((hash >> 8) & 0xFF) as f32 / 255.0;
        let b = ((hash >> 16) & 0xFF) as f32 / 255.0;
        Self {
            peer_id,
            name: name.into(),
            color: [r, g, b, 1.0],
        }
    }
}

/// Awareness state for cursor/selection presence.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AwarenessState {
    /// Cursor position in document coordinates
    pub cursor_x: f32,
    pub cursor_y: f32,
    /// Selected object IDs (empty = no selection)
    pub selection: Vec<Uuid>,
    /// Currently editing text layer (None = not editing)
    pub editing: Option<Uuid>,
}

impl Default for AwarenessState {
    fn default() -> Self {
        Self {
            cursor_x: 0.0,
            cursor_y: 0.0,
            selection: Vec::new(),
            editing: None,
        }
    }
}

/// Top-level protocol message.
///
/// Serialized with bincode for minimal overhead.
/// Typical Delta message: 41 bytes header + payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncMessage {
    pub msg_type: MessageType,
    pub peer_id: Uuid,
    pub doc_id: Uuid,
    /// Lamport clock for causal ordering
    pub clock: u64,
    /// Message payload (varies by msg_type)
    pub payload: Vec<u8>,
}

impl SyncMessage {
    /// Create a delta update message.
    pub fn delta(peer_id: Uuid, doc_id: Uuid, clock: u64, yrs_update: Vec<u8>) -> Self {
        Self {
            msg_type: MessageType::Delta,
            peer_id,
            doc_id,
            clock,
            payload: yrs_update,
        }
    }

    /// Create a sync step 1 (state vector request).
    pub fn sync_step1(peer_id: Uuid, doc_id: Uuid, state_vector: Vec<u8>) -> Self {
        Self {
            msg_type: MessageType::SyncStep1,
            peer_id,
            doc_id,
            clock: 0,
            payload: state_vector,
        }
    }

    /// Create a sync step 2 (state diff response).
    pub fn sync_step2(peer_id: Uuid, doc_id: Uuid, state_diff: Vec<u8>) -> Self {
        Self {
            msg_type: MessageType::SyncStep2,
            peer_id,
            doc_id,
            clock: 0,
            payload: state_diff,
        }
    }

    /// Create an awareness update message.
    pub fn awareness(
        peer_id: Uuid,
        doc_id: Uuid,
        clock: u64,
        state: &AwarenessState,
    ) -> Self {
        let payload = bincode::serde::encode_to_vec(state, bincode::config::standard())
            .unwrap_or_default();
        Self {
            msg_type: MessageType::Awareness,
            peer_id,
            doc_id,
            clock,
            payload,
        }
    }

    /// Create a peer joined notification.
    pub fn peer_joined(peer_id: Uuid, doc_id: Uuid, info: &PeerInfo) -> Self {
        let payload = bincode::serde::encode_to_vec(info, bincode::config::standard())
            .unwrap_or_default();
        Self {
            msg_type: MessageType::PeerJoined,
            peer_id,
            doc_id,
            clock: 0,
            payload,
        }
    }

    /// Create a peer left notification.
    pub fn peer_left(peer_id: Uuid, doc_id: Uuid) -> Self {
        Self {
            msg_type: MessageType::PeerLeft,
            peer_id,
            doc_id,
            clock: 0,
            payload: Vec::new(),
        }
    }

    /// Create a ping message.
    pub fn ping(peer_id: Uuid) -> Self {
        Self {
            msg_type: MessageType::Ping,
            peer_id,
            doc_id: Uuid::nil(),
            clock: 0,
            payload: Vec::new(),
        }
    }

    /// Create a pong message.
    pub fn pong(peer_id: Uuid) -> Self {
        Self {
            msg_type: MessageType::Pong,
            peer_id,
            doc_id: Uuid::nil(),
            clock: 0,
            payload: Vec::new(),
        }
    }

    /// Serialize to binary wire format.
    pub fn encode(&self) -> Result<Vec<u8>, ProtocolError> {
        bincode::serde::encode_to_vec(self, bincode::config::standard())
            .map_err(|e| ProtocolError::SerializationError(e.to_string()))
    }

    /// Deserialize from binary wire format.
    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        let (msg, _) = bincode::serde::decode_from_slice(bytes, bincode::config::standard())
            .map_err(|e| ProtocolError::DeserializationError(e.to_string()))?;
        Ok(msg)
    }

    /// Parse awareness payload.
    pub fn awareness_state(&self) -> Result<AwarenessState, ProtocolError> {
        if self.msg_type != MessageType::Awareness {
            return Err(ProtocolError::InvalidMessageType);
        }
        let (state, _) = bincode::serde::decode_from_slice(&self.payload, bincode::config::standard())
            .map_err(|e| ProtocolError::DeserializationError(e.to_string()))?;
        Ok(state)
    }

    /// Parse peer info payload.
    pub fn peer_info(&self) -> Result<PeerInfo, ProtocolError> {
        if self.msg_type != MessageType::PeerJoined {
            return Err(ProtocolError::InvalidMessageType);
        }
        let (info, _) = bincode::serde::decode_from_slice(&self.payload, bincode::config::standard())
            .map_err(|e| ProtocolError::DeserializationError(e.to_string()))?;
        Ok(info)
    }
}

/// Protocol errors.
#[derive(Debug, Clone)]
pub enum ProtocolError {
    SerializationError(String),
    DeserializationError(String),
    InvalidMessageType,
    ConnectionClosed,
    Timeout,
}

impl std::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SerializationError(e) => write!(f, "Serialization error: {e}"),
            Self::DeserializationError(e) => write!(f, "Deserialization error: {e}"),
            Self::InvalidMessageType => write!(f, "Invalid message type"),
            Self::ConnectionClosed => write!(f, "Connection closed"),
            Self::Timeout => write!(f, "Connection timeout"),
        }
    }
}

impl std::error::Error for ProtocolError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_message_roundtrip() {
        let peer = Uuid::new_v4();
        let doc = Uuid::new_v4();
        let payload = vec![1, 2, 3, 4, 5];

        let msg = SyncMessage::delta(peer, doc, 42, payload.clone());
        let encoded = msg.encode().unwrap();
        let decoded = SyncMessage::decode(&encoded).unwrap();

        assert_eq!(decoded.msg_type, MessageType::Delta);
        assert_eq!(decoded.peer_id, peer);
        assert_eq!(decoded.doc_id, doc);
        assert_eq!(decoded.clock, 42);
        assert_eq!(decoded.payload, payload);
    }

    #[test]
    fn test_sync_step1_roundtrip() {
        let peer = Uuid::new_v4();
        let doc = Uuid::new_v4();
        let sv = vec![10, 20, 30];

        let msg = SyncMessage::sync_step1(peer, doc, sv.clone());
        let encoded = msg.encode().unwrap();
        let decoded = SyncMessage::decode(&encoded).unwrap();

        assert_eq!(decoded.msg_type, MessageType::SyncStep1);
        assert_eq!(decoded.payload, sv);
    }

    #[test]
    fn test_sync_step2_roundtrip() {
        let peer = Uuid::new_v4();
        let doc = Uuid::new_v4();
        let diff = vec![100, 200];

        let msg = SyncMessage::sync_step2(peer, doc, diff.clone());
        let encoded = msg.encode().unwrap();
        let decoded = SyncMessage::decode(&encoded).unwrap();

        assert_eq!(decoded.msg_type, MessageType::SyncStep2);
        assert_eq!(decoded.payload, diff);
    }

    #[test]
    fn test_awareness_roundtrip() {
        let peer = Uuid::new_v4();
        let doc = Uuid::new_v4();
        let state = AwarenessState {
            cursor_x: 100.5,
            cursor_y: 200.3,
            selection: vec![Uuid::new_v4()],
            editing: Some(Uuid::new_v4()),
        };

        let msg = SyncMessage::awareness(peer, doc, 7, &state);
        let encoded = msg.encode().unwrap();
        let decoded = SyncMessage::decode(&encoded).unwrap();

        assert_eq!(decoded.msg_type, MessageType::Awareness);
        let parsed = decoded.awareness_state().unwrap();
        assert_eq!(parsed.cursor_x, state.cursor_x);
        assert_eq!(parsed.cursor_y, state.cursor_y);
        assert_eq!(parsed.selection, state.selection);
        assert_eq!(parsed.editing, state.editing);
    }

    #[test]
    fn test_peer_joined_roundtrip() {
        let info = PeerInfo::new("Alice");
        let doc = Uuid::new_v4();

        let msg = SyncMessage::peer_joined(info.peer_id, doc, &info);
        let encoded = msg.encode().unwrap();
        let decoded = SyncMessage::decode(&encoded).unwrap();

        assert_eq!(decoded.msg_type, MessageType::PeerJoined);
        let parsed = decoded.peer_info().unwrap();
        assert_eq!(parsed.name, "Alice");
        assert_eq!(parsed.peer_id, info.peer_id);
    }

    #[test]
    fn test_peer_left_roundtrip() {
        let peer = Uuid::new_v4();
        let doc = Uuid::new_v4();

        let msg = SyncMessage::peer_left(peer, doc);
        let encoded = msg.encode().unwrap();
        let decoded = SyncMessage::decode(&encoded).unwrap();

        assert_eq!(decoded.msg_type, MessageType::PeerLeft);
        assert_eq!(decoded.peer_id, peer);
        assert!(decoded.payload.is_empty());
    }

    #[test]
    fn test_ping_pong_roundtrip() {
        let peer = Uuid::new_v4();

        let ping = SyncMessage::ping(peer);
        let pong = SyncMessage::pong(peer);

        let ping_bytes = ping.encode().unwrap();
        let pong_bytes = pong.encode().unwrap();

        let decoded_ping = SyncMessage::decode(&ping_bytes).unwrap();
        let decoded_pong = SyncMessage::decode(&pong_bytes).unwrap();

        assert_eq!(decoded_ping.msg_type, MessageType::Ping);
        assert_eq!(decoded_pong.msg_type, MessageType::Pong);
    }

    #[test]
    fn test_delta_size_efficient() {
        let peer = Uuid::new_v4();
        let doc = Uuid::new_v4();
        // Typical small Yrs delta: ~50 bytes
        let delta = vec![0u8; 50];

        let msg = SyncMessage::delta(peer, doc, 1, delta);
        let encoded = msg.encode().unwrap();

        // Header should be ~41 bytes (1 type + 16 peer + 16 doc + 8 clock)
        // + payload length prefix + 50 bytes payload
        // Total should be well under 150 bytes
        assert!(
            encoded.len() < 150,
            "Encoded size {} too large for 50-byte delta",
            encoded.len()
        );
    }

    #[test]
    fn test_peer_info_stable_color() {
        let id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let info1 = PeerInfo::with_id(id, "Test");
        let info2 = PeerInfo::with_id(id, "Test");

        // Same peer_id always produces same color
        assert_eq!(info1.color, info2.color);
    }

    #[test]
    fn test_invalid_message_type_error() {
        let msg = SyncMessage::ping(Uuid::new_v4());
        assert!(msg.awareness_state().is_err());
        assert!(msg.peer_info().is_err());
    }

    #[test]
    fn test_decode_invalid_bytes() {
        let garbage = vec![0xFF, 0xFE, 0xFD];
        assert!(SyncMessage::decode(&garbage).is_err());
    }

    #[test]
    fn test_awareness_default() {
        let state = AwarenessState::default();
        assert_eq!(state.cursor_x, 0.0);
        assert_eq!(state.cursor_y, 0.0);
        assert!(state.selection.is_empty());
        assert!(state.editing.is_none());
    }

    #[test]
    fn test_message_type_values() {
        assert_eq!(MessageType::SyncStep1 as u8, 1);
        assert_eq!(MessageType::SyncStep2 as u8, 2);
        assert_eq!(MessageType::Delta as u8, 3);
        assert_eq!(MessageType::Awareness as u8, 4);
        assert_eq!(MessageType::PeerJoined as u8, 5);
        assert_eq!(MessageType::PeerLeft as u8, 6);
        assert_eq!(MessageType::Ping as u8, 7);
        assert_eq!(MessageType::Pong as u8, 8);
    }

    #[test]
    fn test_empty_delta() {
        let peer = Uuid::new_v4();
        let doc = Uuid::new_v4();

        let msg = SyncMessage::delta(peer, doc, 0, Vec::new());
        let encoded = msg.encode().unwrap();
        let decoded = SyncMessage::decode(&encoded).unwrap();

        assert!(decoded.payload.is_empty());
        assert_eq!(decoded.clock, 0);
    }

    #[test]
    fn test_large_delta() {
        let peer = Uuid::new_v4();
        let doc = Uuid::new_v4();
        // Simulate a large batch update: 64KB
        let delta = vec![42u8; 65536];

        let msg = SyncMessage::delta(peer, doc, 999, delta.clone());
        let encoded = msg.encode().unwrap();
        let decoded = SyncMessage::decode(&encoded).unwrap();

        assert_eq!(decoded.payload.len(), 65536);
        assert_eq!(decoded.payload, delta);
    }
}
