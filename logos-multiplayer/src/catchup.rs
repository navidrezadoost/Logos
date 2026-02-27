//! Catch-up — replay-based state reconstruction for late joiners.
//!
//! When a peer joins mid-session (or reconnects after a disconnect),
//! it needs to "catch up" to the current version. This module computes
//! the minimal set of operations needed and packages them for replay.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::MultiplayerError;
use crate::peer::PeerId;
use crate::sync_protocol::OpBroadcast;

/// A request from a peer to catch up from a specific version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatchUpRequest {
    /// Who is requesting catch-up.
    pub peer_id: PeerId,
    /// The document to catch up on.
    pub document_id: Uuid,
    /// The last version this peer has.
    pub from_version: u64,
    /// Whether the peer prefers a snapshot (if available).
    pub prefer_snapshot: bool,
}

impl CatchUpRequest {
    pub fn new(peer_id: PeerId, document_id: Uuid, from_version: u64) -> Self {
        Self {
            peer_id,
            document_id,
            from_version,
            prefer_snapshot: false,
        }
    }

    pub fn with_snapshot_preference(mut self) -> Self {
        self.prefer_snapshot = true;
        self
    }

    /// How many versions behind this peer is.
    pub fn versions_behind(&self, current_version: u64) -> u64 {
        current_version.saturating_sub(self.from_version)
    }
}

/// The response to a catch-up request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CatchUpResponse {
    /// Send individual operations to replay.
    Ops {
        document_id: Uuid,
        ops: Vec<OpBroadcast>,
        from_version: u64,
        to_version: u64,
    },
    /// Send a snapshot for fast catch-up.
    Snapshot {
        document_id: Uuid,
        snapshot: serde_json::Value,
        at_version: u64,
        /// Any ops after the snapshot version.
        trailing_ops: Vec<OpBroadcast>,
    },
    /// No catch-up needed — already up to date.
    UpToDate { document_id: Uuid, version: u64 },
}

impl CatchUpResponse {
    /// The final version the peer will be at after applying this response.
    pub fn final_version(&self) -> u64 {
        match self {
            Self::Ops { to_version, .. } => *to_version,
            Self::Snapshot { trailing_ops, at_version, .. } => {
                trailing_ops.last().map(|op| op.version).unwrap_or(*at_version)
            }
            Self::UpToDate { version, .. } => *version,
        }
    }

    /// Total number of operations in this response.
    pub fn op_count(&self) -> usize {
        match self {
            Self::Ops { ops, .. } => ops.len(),
            Self::Snapshot { trailing_ops, .. } => trailing_ops.len() + 1, // snapshot counts as 1
            Self::UpToDate { .. } => 0,
        }
    }
}

/// Engine for generating catch-up responses.
///
/// Maintains a buffer of recent operations that can be sent to
/// late-joining or reconnecting peers.
pub struct CatchUpEngine {
    /// Recent op buffer, keyed by (document_id, version).
    op_buffer: std::collections::HashMap<Uuid, Vec<OpBroadcast>>,
    /// Maximum ops to retain per document.
    max_buffer_size: usize,
    /// Snapshot threshold: if peer is behind by more than this many
    /// versions, prefer sending a snapshot.
    snapshot_threshold: u64,
}

impl CatchUpEngine {
    pub fn new(max_buffer_size: usize, snapshot_threshold: u64) -> Self {
        Self {
            op_buffer: std::collections::HashMap::new(),
            max_buffer_size,
            snapshot_threshold,
        }
    }

    /// Record an operation for potential future catch-up.
    pub fn record_op(&mut self, op: OpBroadcast) {
        let buf = self.op_buffer.entry(op.document_id).or_default();
        buf.push(op);
        // Trim to max_buffer_size.
        if buf.len() > self.max_buffer_size {
            let excess = buf.len() - self.max_buffer_size;
            buf.drain(..excess);
        }
    }

    /// Generate a catch-up response for a request.
    pub fn handle_request(
        &self,
        request: &CatchUpRequest,
        current_version: u64,
        snapshot: Option<(u64, serde_json::Value)>,
    ) -> Result<CatchUpResponse, MultiplayerError> {
        // Already up to date.
        if request.from_version >= current_version {
            return Ok(CatchUpResponse::UpToDate {
                document_id: request.document_id,
                version: current_version,
            });
        }

        let versions_behind = current_version - request.from_version;

        // If far behind and snapshot available, use snapshot path.
        if request.prefer_snapshot || versions_behind > self.snapshot_threshold {
            if let Some((snap_version, snap_data)) = snapshot {
                let trailing = self.ops_after(request.document_id, snap_version);
                return Ok(CatchUpResponse::Snapshot {
                    document_id: request.document_id,
                    snapshot: snap_data,
                    at_version: snap_version,
                    trailing_ops: trailing,
                });
            }
        }

        // Otherwise, send ops.
        let ops = self.ops_between(request.document_id, request.from_version, current_version);
        if ops.is_empty() {
            return Err(MultiplayerError::CatchUpFailed {
                reason: format!(
                    "No ops found for document {} between versions {} and {}",
                    request.document_id, request.from_version, current_version
                ),
            });
        }

        Ok(CatchUpResponse::Ops {
            document_id: request.document_id,
            ops,
            from_version: request.from_version,
            to_version: current_version,
        })
    }

    /// Get ops between two versions for a document.
    fn ops_between(&self, doc_id: Uuid, from: u64, to: u64) -> Vec<OpBroadcast> {
        self.op_buffer
            .get(&doc_id)
            .map(|buf| {
                buf.iter()
                    .filter(|op| op.version > from && op.version <= to)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get ops after a given version for a document.
    fn ops_after(&self, doc_id: Uuid, after_version: u64) -> Vec<OpBroadcast> {
        self.op_buffer
            .get(&doc_id)
            .map(|buf| {
                buf.iter()
                    .filter(|op| op.version > after_version)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Number of buffered ops for a document.
    pub fn buffer_size(&self, doc_id: &Uuid) -> usize {
        self.op_buffer.get(doc_id).map(|b| b.len()).unwrap_or(0)
    }

    /// Oldest version in the buffer for a document.
    pub fn oldest_version(&self, doc_id: &Uuid) -> Option<u64> {
        self.op_buffer
            .get(doc_id)
            .and_then(|b| b.first().map(|op| op.version))
    }
}

impl Default for CatchUpEngine {
    fn default() -> Self {
        Self::new(10_000, 100)
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_op(doc: Uuid, version: u64) -> OpBroadcast {
        OpBroadcast {
            sender: PeerId::new(),
            document_id: doc,
            version,
            lamport: version,
            payload: json!({"v": version}),
            timestamp: 1000 + version,
            description: Some(format!("Op {}", version)),
            domain: "design".to_string(),
        }
    }

    fn populated_engine(doc: Uuid, versions: std::ops::RangeInclusive<u64>) -> CatchUpEngine {
        let mut engine = CatchUpEngine::new(1000, 50);
        for v in versions {
            engine.record_op(make_op(doc, v));
        }
        engine
    }

    #[test]
    fn catch_up_request_creation() {
        let req = CatchUpRequest::new(PeerId::new(), Uuid::new_v4(), 5);
        assert_eq!(req.from_version, 5);
        assert!(!req.prefer_snapshot);
        assert_eq!(req.versions_behind(10), 5);
    }

    #[test]
    fn catch_up_up_to_date() {
        let doc = Uuid::new_v4();
        let engine = populated_engine(doc, 1..=10);
        let req = CatchUpRequest::new(PeerId::new(), doc, 10);
        let resp = engine.handle_request(&req, 10, None).unwrap();
        assert!(matches!(resp, CatchUpResponse::UpToDate { .. }));
        assert_eq!(resp.final_version(), 10);
        assert_eq!(resp.op_count(), 0);
    }

    #[test]
    fn catch_up_with_ops() {
        let doc = Uuid::new_v4();
        let engine = populated_engine(doc, 1..=10);
        let req = CatchUpRequest::new(PeerId::new(), doc, 5);
        let resp = engine.handle_request(&req, 10, None).unwrap();
        match resp {
            CatchUpResponse::Ops { ops, from_version, to_version, .. } => {
                assert_eq!(ops.len(), 5); // versions 6..=10
                assert_eq!(from_version, 5);
                assert_eq!(to_version, 10);
            }
            _ => panic!("Expected Ops response"),
        }
    }

    #[test]
    fn catch_up_with_snapshot() {
        let doc = Uuid::new_v4();
        let engine = populated_engine(doc, 1..=100);
        let req = CatchUpRequest::new(PeerId::new(), doc, 0); // 100 versions behind
        let snapshot = Some((90, json!({"state": "at_90"})));
        let resp = engine.handle_request(&req, 100, snapshot).unwrap();
        match resp {
            CatchUpResponse::Snapshot { at_version, trailing_ops, .. } => {
                assert_eq!(at_version, 90);
                assert_eq!(trailing_ops.len(), 10); // versions 91..=100
            }
            _ => panic!("Expected Snapshot response"),
        }
    }

    #[test]
    fn catch_up_prefer_snapshot() {
        let doc = Uuid::new_v4();
        let engine = populated_engine(doc, 1..=10);
        let req = CatchUpRequest::new(PeerId::new(), doc, 5)
            .with_snapshot_preference();
        let snapshot = Some((8, json!({"state": "at_8"})));
        let resp = engine.handle_request(&req, 10, snapshot).unwrap();
        assert!(matches!(resp, CatchUpResponse::Snapshot { .. }));
    }

    #[test]
    fn catch_up_no_ops_available() {
        let engine = CatchUpEngine::new(1000, 50);
        let doc = Uuid::new_v4();
        let req = CatchUpRequest::new(PeerId::new(), doc, 5);
        let result = engine.handle_request(&req, 10, None);
        assert!(matches!(result, Err(MultiplayerError::CatchUpFailed { .. })));
    }

    #[test]
    fn buffer_trimming() {
        let doc = Uuid::new_v4();
        let mut engine = CatchUpEngine::new(5, 50);
        for v in 1..=10 {
            engine.record_op(make_op(doc, v));
        }
        // Only last 5 ops should remain.
        assert_eq!(engine.buffer_size(&doc), 5);
        assert_eq!(engine.oldest_version(&doc), Some(6));
    }

    #[test]
    fn response_final_version() {
        let resp = CatchUpResponse::Ops {
            document_id: Uuid::new_v4(),
            ops: vec![],
            from_version: 5,
            to_version: 10,
        };
        assert_eq!(resp.final_version(), 10);
    }

    #[test]
    fn response_op_count_snapshot() {
        let doc = Uuid::new_v4();
        let resp = CatchUpResponse::Snapshot {
            document_id: doc,
            snapshot: json!({}),
            at_version: 8,
            trailing_ops: vec![make_op(doc, 9), make_op(doc, 10)],
        };
        assert_eq!(resp.op_count(), 3); // snapshot + 2 trailing
    }

    #[test]
    fn catch_up_request_serde() {
        let req = CatchUpRequest::new(PeerId::new(), Uuid::new_v4(), 5);
        let json = serde_json::to_string(&req).unwrap();
        let back: CatchUpRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.from_version, 5);
    }

    #[test]
    fn catch_up_response_serde() {
        let resp = CatchUpResponse::UpToDate {
            document_id: Uuid::new_v4(),
            version: 10,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: CatchUpResponse = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, CatchUpResponse::UpToDate { .. }));
    }
}
