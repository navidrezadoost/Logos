//! Snapshot exchange — fast join via state transfer.
//!
//! Instead of replaying the entire operation log, a joining peer can
//! receive a snapshot of the current state and only replay ops since
//! that snapshot. This is critical for documents with long histories.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::peer::PeerId;

/// A request to receive a snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotRequest {
    /// Who is requesting the snapshot.
    pub peer_id: PeerId,
    /// The document to snapshot.
    pub document_id: Uuid,
    /// Maximum acceptable snapshot age (versions behind current).
    pub max_staleness: Option<u64>,
}

impl SnapshotRequest {
    pub fn new(peer_id: PeerId, document_id: Uuid) -> Self {
        Self {
            peer_id,
            document_id,
            max_staleness: None,
        }
    }

    pub fn with_max_staleness(mut self, max: u64) -> Self {
        self.max_staleness = Some(max);
        self
    }
}

/// An offer to send a snapshot (the authority responded).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotOffer {
    /// The document being offered.
    pub document_id: Uuid,
    /// Version of the snapshot.
    pub version: u64,
    /// Size estimate in bytes.
    pub size_bytes: usize,
    /// Whether the snapshot is compressed.
    pub compressed: bool,
    /// Checksum for integrity verification.
    pub checksum: u64,
}

impl SnapshotOffer {
    pub fn new(document_id: Uuid, version: u64, size_bytes: usize) -> Self {
        Self {
            document_id,
            version,
            size_bytes,
            compressed: false,
            checksum: 0,
        }
    }

    pub fn with_compression(mut self) -> Self {
        self.compressed = true;
        self
    }

    pub fn with_checksum(mut self, checksum: u64) -> Self {
        self.checksum = checksum;
        self
    }

    /// Whether this offer is acceptable given a staleness constraint.
    pub fn is_acceptable(&self, current_version: u64, max_staleness: Option<u64>) -> bool {
        if let Some(max) = max_staleness {
            current_version.saturating_sub(self.version) <= max
        } else {
            true
        }
    }
}

/// A complete snapshot transfer, including the state data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotTransfer {
    /// The document.
    pub document_id: Uuid,
    /// Version of this snapshot.
    pub version: u64,
    /// The full document state as JSON.
    pub state: serde_json::Value,
    /// Checksum for integrity.
    pub checksum: u64,
    /// When this snapshot was created.
    pub created_at: u64,
    /// Who created this snapshot (usually the authority).
    pub created_by: PeerId,
}

impl SnapshotTransfer {
    pub fn new(
        document_id: Uuid,
        version: u64,
        state: serde_json::Value,
        created_by: PeerId,
    ) -> Self {
        let checksum = simple_checksum(&state);
        Self {
            document_id,
            version,
            state,
            checksum,
            created_at: now(),
            created_by,
        }
    }

    /// Verify the checksum.
    pub fn verify_checksum(&self) -> bool {
        simple_checksum(&self.state) == self.checksum
    }

    /// Size of the state in bytes (JSON serialized).
    pub fn estimated_size(&self) -> usize {
        serde_json::to_string(&self.state)
            .map(|s| s.len())
            .unwrap_or(0)
    }
}

/// Simple checksum for integrity verification.
///
/// Not cryptographic — just a quick check for data corruption.
fn simple_checksum(value: &serde_json::Value) -> u64 {
    let s = serde_json::to_string(value).unwrap_or_default();
    let mut hash: u64 = 5381;
    for byte in s.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(byte as u64);
    }
    hash
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

    #[test]
    fn snapshot_request_creation() {
        let req = SnapshotRequest::new(PeerId::new(), Uuid::new_v4());
        assert!(req.max_staleness.is_none());
    }

    #[test]
    fn snapshot_request_staleness() {
        let req = SnapshotRequest::new(PeerId::new(), Uuid::new_v4())
            .with_max_staleness(10);
        assert_eq!(req.max_staleness, Some(10));
    }

    #[test]
    fn snapshot_offer_acceptable() {
        let offer = SnapshotOffer::new(Uuid::new_v4(), 90, 1024);
        assert!(offer.is_acceptable(100, Some(20)));
        assert!(!offer.is_acceptable(100, Some(5)));
        assert!(offer.is_acceptable(100, None));
    }

    #[test]
    fn snapshot_offer_compression() {
        let offer = SnapshotOffer::new(Uuid::new_v4(), 10, 500)
            .with_compression()
            .with_checksum(12345);
        assert!(offer.compressed);
        assert_eq!(offer.checksum, 12345);
    }

    #[test]
    fn snapshot_transfer_checksum() {
        let state = json!({"layers": [1, 2, 3], "name": "Test"});
        let transfer = SnapshotTransfer::new(
            Uuid::new_v4(),
            10,
            state,
            PeerId::new(),
        );
        assert!(transfer.verify_checksum());
    }

    #[test]
    fn snapshot_transfer_corrupted_checksum() {
        let state = json!({"layers": [1, 2, 3]});
        let mut transfer = SnapshotTransfer::new(
            Uuid::new_v4(),
            10,
            state,
            PeerId::new(),
        );
        transfer.checksum = 0; // Corrupt it.
        assert!(!transfer.verify_checksum());
    }

    #[test]
    fn snapshot_transfer_size() {
        let state = json!({"data": "hello world"});
        let transfer = SnapshotTransfer::new(
            Uuid::new_v4(),
            1,
            state,
            PeerId::new(),
        );
        assert!(transfer.estimated_size() > 0);
    }

    #[test]
    fn simple_checksum_deterministic() {
        let val = json!({"a": 1, "b": 2});
        let c1 = simple_checksum(&val);
        let c2 = simple_checksum(&val);
        assert_eq!(c1, c2);
    }

    #[test]
    fn simple_checksum_different_values() {
        let a = json!({"a": 1});
        let b = json!({"a": 2});
        assert_ne!(simple_checksum(&a), simple_checksum(&b));
    }

    #[test]
    fn snapshot_request_serde() {
        let req = SnapshotRequest::new(PeerId::new(), Uuid::new_v4())
            .with_max_staleness(5);
        let json = serde_json::to_string(&req).unwrap();
        let back: SnapshotRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.max_staleness, Some(5));
    }

    #[test]
    fn snapshot_transfer_serde() {
        let transfer = SnapshotTransfer::new(
            Uuid::new_v4(),
            10,
            json!({"test": true}),
            PeerId::new(),
        );
        let s = serde_json::to_string(&transfer).unwrap();
        let back: SnapshotTransfer = serde_json::from_str(&s).unwrap();
        assert_eq!(back.version, 10);
        assert!(back.verify_checksum());
    }
}
