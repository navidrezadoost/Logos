//! Convergence — CRDT-inspired merge and deterministic verification.
//!
//! Ensures that all peers who apply the same set of operations
//! arrive at the same state, regardless of application order (within
//! causal constraints). Provides merge strategies and convergence proofs.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::peer::PeerId;
use crate::sync_protocol::OpBroadcast;

// ══════════════════════════════════════════════════════════════════════
// Merge strategy
// ══════════════════════════════════════════════════════════════════════

/// Strategy for resolving concurrent edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MergeStrategy {
    /// Last-writer-wins based on Lamport timestamp.
    LastWriterWins,
    /// Higher peer ID wins (stable tie-breaker).
    PeerPriority,
    /// Accept all ops, let the domain layer resolve.
    AcceptAll,
    /// Flag conflict for manual resolution.
    Manual,
}

impl Default for MergeStrategy {
    fn default() -> Self {
        MergeStrategy::LastWriterWins
    }
}

// ══════════════════════════════════════════════════════════════════════
// Merge result
// ══════════════════════════════════════════════════════════════════════

/// Result of attempting to merge a set of concurrent operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MergeResult {
    /// All operations merged cleanly.
    Clean {
        /// Operations in causal order.
        ops: Vec<OpBroadcast>,
    },
    /// Conflict detected, resolved automatically.
    Resolved {
        /// Winner operation.
        winner: OpBroadcast,
        /// Loser operations (dropped or transformed).
        losers: Vec<OpBroadcast>,
        strategy: MergeStrategy,
    },
    /// Conflict that requires manual resolution.
    Conflict {
        /// The conflicting operations.
        ops: Vec<OpBroadcast>,
        description: String,
    },
}

impl MergeResult {
    /// Whether the merge was clean.
    pub fn is_clean(&self) -> bool {
        matches!(self, MergeResult::Clean { .. })
    }

    /// Whether the merge resulted in a conflict (manual or resolved).
    pub fn is_conflicted(&self) -> bool {
        matches!(
            self,
            MergeResult::Resolved { .. } | MergeResult::Conflict { .. }
        )
    }

    /// Total number of operations involved.
    pub fn op_count(&self) -> usize {
        match self {
            MergeResult::Clean { ops } => ops.len(),
            MergeResult::Resolved { losers, .. } => 1 + losers.len(),
            MergeResult::Conflict { ops, .. } => ops.len(),
        }
    }
}

// ══════════════════════════════════════════════════════════════════════
// Convergence proof
// ══════════════════════════════════════════════════════════════════════

/// A proof that a peer's state converges with the authority.
///
/// After applying all ops up to a given version, each peer computes
/// a hash of their state. If all hashes match, we have convergence.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConvergenceProof {
    pub document_id: Uuid,
    pub version: u64,
    /// State hash (application-defined, e.g. SHA-256 of serialized state).
    pub state_hash: u64,
    pub peer_id: PeerId,
    pub timestamp: u64,
}

impl ConvergenceProof {
    pub fn new(document_id: Uuid, version: u64, state_hash: u64, peer_id: PeerId) -> Self {
        Self {
            document_id,
            version,
            state_hash,
            peer_id,
            timestamp: now(),
        }
    }
}

// ══════════════════════════════════════════════════════════════════════
// Convergence engine
// ══════════════════════════════════════════════════════════════════════

/// Tracks convergence across peers and resolves concurrent ops.
pub struct ConvergenceEngine {
    strategy: MergeStrategy,
    /// Collected proofs by (document_id, version).
    proofs: HashMap<(Uuid, u64), Vec<ConvergenceProof>>,
    /// Number of peers expected per document.
    expected_peers: HashMap<Uuid, usize>,
}

impl ConvergenceEngine {
    pub fn new(strategy: MergeStrategy) -> Self {
        Self {
            strategy,
            proofs: HashMap::new(),
            expected_peers: HashMap::new(),
        }
    }

    pub fn set_expected_peers(&mut self, document_id: Uuid, count: usize) {
        self.expected_peers.insert(document_id, count);
    }

    /// Merge a set of concurrent operations using the configured strategy.
    pub fn merge(&self, mut ops: Vec<OpBroadcast>) -> MergeResult {
        if ops.len() <= 1 {
            return MergeResult::Clean { ops };
        }

        // Check if they target the same domain (potential conflict).
        let same_domain = ops
            .windows(2)
            .all(|w| w[0].domain == w[1].domain);

        if !same_domain {
            // Different domains don't conflict.
            ops.sort_by_key(|op| op.lamport);
            return MergeResult::Clean { ops };
        }

        match self.strategy {
            MergeStrategy::LastWriterWins => {
                ops.sort_by(|a, b| a.lamport.cmp(&b.lamport).then(a.sender.cmp(&b.sender)));
                let winner = ops.pop().unwrap();
                MergeResult::Resolved {
                    winner,
                    losers: ops,
                    strategy: MergeStrategy::LastWriterWins,
                }
            }
            MergeStrategy::PeerPriority => {
                ops.sort_by_key(|op| op.sender);
                let winner = ops.pop().unwrap();
                MergeResult::Resolved {
                    winner,
                    losers: ops,
                    strategy: MergeStrategy::PeerPriority,
                }
            }
            MergeStrategy::AcceptAll => {
                ops.sort_by_key(|op| op.lamport);
                MergeResult::Clean { ops }
            }
            MergeStrategy::Manual => MergeResult::Conflict {
                description: format!(
                    "{} concurrent operations in same domain require manual resolution",
                    ops.len()
                ),
                ops,
            },
        }
    }

    /// Submit a convergence proof from a peer.
    pub fn submit_proof(&mut self, proof: ConvergenceProof) {
        let key = (proof.document_id, proof.version);
        self.proofs.entry(key).or_default().push(proof);
    }

    /// Check if all expected peers have converged at a given version.
    pub fn check_convergence(&self, document_id: Uuid, version: u64) -> ConvergenceStatus {
        let key = (document_id, version);
        let proofs = match self.proofs.get(&key) {
            Some(p) => p,
            None => return ConvergenceStatus::Waiting { have: 0, need: self.need(document_id) },
        };

        let need = self.need(document_id);
        if proofs.len() < need {
            return ConvergenceStatus::Waiting {
                have: proofs.len(),
                need,
            };
        }

        // Check all hashes match.
        let first_hash = proofs[0].state_hash;
        let all_match = proofs.iter().all(|p| p.state_hash == first_hash);

        if all_match {
            ConvergenceStatus::Converged {
                version,
                state_hash: first_hash,
                peer_count: proofs.len(),
            }
        } else {
            let divergent: Vec<PeerId> = proofs
                .iter()
                .filter(|p| p.state_hash != first_hash)
                .map(|p| p.peer_id)
                .collect();
            ConvergenceStatus::Diverged {
                version,
                divergent_peers: divergent,
            }
        }
    }

    fn need(&self, document_id: Uuid) -> usize {
        self.expected_peers.get(&document_id).copied().unwrap_or(2)
    }

    /// Clear proofs for versions older than the given version.
    pub fn gc_proofs(&mut self, document_id: Uuid, before_version: u64) {
        self.proofs
            .retain(|&(doc, ver), _| doc != document_id || ver >= before_version);
    }

    /// Total number of stored proofs (for monitoring).
    pub fn proof_count(&self) -> usize {
        self.proofs.values().map(|v| v.len()).sum()
    }
}

/// Status of convergence verification.
#[derive(Debug, Clone, PartialEq)]
pub enum ConvergenceStatus {
    /// Still waiting for more peer proofs.
    Waiting { have: usize, need: usize },
    /// All peers converged to same state.
    Converged {
        version: u64,
        state_hash: u64,
        peer_count: usize,
    },
    /// Peers diverged — state hashes don't match.
    Diverged {
        version: u64,
        divergent_peers: Vec<PeerId>,
    },
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

    fn make_op(sender: PeerId, lamport: u64, domain: &str) -> OpBroadcast {
        OpBroadcast {
            sender,
            document_id: Uuid::nil(),
            version: lamport,
            lamport,
            payload: json!({}),
            timestamp: 0,
            description: None,
            domain: domain.to_string(),
        }
    }

    #[test]
    fn merge_single_op() {
        let engine = ConvergenceEngine::new(MergeStrategy::LastWriterWins);
        let op = make_op(PeerId::new(), 1, "shapes");
        let result = engine.merge(vec![op]);
        assert!(result.is_clean());
        assert_eq!(result.op_count(), 1);
    }

    #[test]
    fn merge_different_domains() {
        let engine = ConvergenceEngine::new(MergeStrategy::LastWriterWins);
        let ops = vec![
            make_op(PeerId::new(), 1, "shapes"),
            make_op(PeerId::new(), 2, "text"),
        ];
        let result = engine.merge(ops);
        assert!(result.is_clean());
    }

    #[test]
    fn merge_lww() {
        let engine = ConvergenceEngine::new(MergeStrategy::LastWriterWins);
        let ops = vec![
            make_op(PeerId::new(), 1, "shapes"),
            make_op(PeerId::new(), 5, "shapes"),
        ];
        let result = engine.merge(ops);
        assert!(result.is_conflicted());
        if let MergeResult::Resolved { winner, losers, strategy } = result {
            assert_eq!(winner.lamport, 5);
            assert_eq!(losers.len(), 1);
            assert_eq!(strategy, MergeStrategy::LastWriterWins);
        } else {
            panic!("Expected Resolved");
        }
    }

    #[test]
    fn merge_peer_priority() {
        let engine = ConvergenceEngine::new(MergeStrategy::PeerPriority);
        let p1 = PeerId::new();
        let p2 = PeerId::new();
        let ops = vec![
            make_op(p1, 1, "shapes"),
            make_op(p2, 2, "shapes"),
        ];
        let result = engine.merge(ops);
        assert!(result.is_conflicted());
        assert_eq!(result.op_count(), 2);
    }

    #[test]
    fn merge_accept_all() {
        let engine = ConvergenceEngine::new(MergeStrategy::AcceptAll);
        let ops = vec![
            make_op(PeerId::new(), 1, "shapes"),
            make_op(PeerId::new(), 2, "shapes"),
        ];
        let result = engine.merge(ops);
        assert!(result.is_clean());
        assert_eq!(result.op_count(), 2);
    }

    #[test]
    fn merge_manual() {
        let engine = ConvergenceEngine::new(MergeStrategy::Manual);
        let ops = vec![
            make_op(PeerId::new(), 1, "shapes"),
            make_op(PeerId::new(), 2, "shapes"),
        ];
        let result = engine.merge(ops);
        if let MergeResult::Conflict { description, ops } = result {
            assert!(description.contains("2 concurrent"));
            assert_eq!(ops.len(), 2);
        } else {
            panic!("Expected Conflict");
        }
    }

    #[test]
    fn convergence_waiting() {
        let engine = ConvergenceEngine::new(MergeStrategy::LastWriterWins);
        let doc = Uuid::new_v4();
        let status = engine.check_convergence(doc, 1);
        assert!(matches!(status, ConvergenceStatus::Waiting { have: 0, .. }));
    }

    #[test]
    fn convergence_converged() {
        let mut engine = ConvergenceEngine::new(MergeStrategy::LastWriterWins);
        let doc = Uuid::new_v4();
        engine.set_expected_peers(doc, 2);

        let hash = 0xDEADBEEF;
        engine.submit_proof(ConvergenceProof::new(doc, 10, hash, PeerId::new()));
        engine.submit_proof(ConvergenceProof::new(doc, 10, hash, PeerId::new()));

        let status = engine.check_convergence(doc, 10);
        assert!(matches!(
            status,
            ConvergenceStatus::Converged {
                version: 10,
                peer_count: 2,
                ..
            }
        ));
    }

    #[test]
    fn convergence_diverged() {
        let mut engine = ConvergenceEngine::new(MergeStrategy::LastWriterWins);
        let doc = Uuid::new_v4();
        engine.set_expected_peers(doc, 2);

        engine.submit_proof(ConvergenceProof::new(doc, 10, 0xAAAA, PeerId::new()));
        engine.submit_proof(ConvergenceProof::new(doc, 10, 0xBBBB, PeerId::new()));

        let status = engine.check_convergence(doc, 10);
        if let ConvergenceStatus::Diverged { divergent_peers, .. } = status {
            assert_eq!(divergent_peers.len(), 1);
        } else {
            panic!("Expected Diverged");
        }
    }

    #[test]
    fn gc_proofs() {
        let mut engine = ConvergenceEngine::new(MergeStrategy::LastWriterWins);
        let doc = Uuid::new_v4();
        engine.submit_proof(ConvergenceProof::new(doc, 5, 1, PeerId::new()));
        engine.submit_proof(ConvergenceProof::new(doc, 10, 2, PeerId::new()));
        assert_eq!(engine.proof_count(), 2);
        engine.gc_proofs(doc, 8);
        assert_eq!(engine.proof_count(), 1);
    }

    #[test]
    fn merge_result_serde() {
        let result = MergeResult::Clean {
            ops: vec![make_op(PeerId::new(), 1, "test")],
        };
        let json = serde_json::to_string(&result).unwrap();
        let back: MergeResult = serde_json::from_str(&json).unwrap();
        assert!(back.is_clean());
    }

    #[test]
    fn convergence_proof_serde() {
        let proof = ConvergenceProof::new(Uuid::new_v4(), 10, 0xBEEF, PeerId::new());
        let json = serde_json::to_string(&proof).unwrap();
        let back: ConvergenceProof = serde_json::from_str(&json).unwrap();
        assert_eq!(back.version, 10);
        assert_eq!(back.state_hash, 0xBEEF);
    }

    #[test]
    fn strategy_default() {
        assert_eq!(MergeStrategy::default(), MergeStrategy::LastWriterWins);
    }

    #[test]
    fn merge_empty() {
        let engine = ConvergenceEngine::new(MergeStrategy::LastWriterWins);
        let result = engine.merge(vec![]);
        assert!(result.is_clean());
        assert_eq!(result.op_count(), 0);
    }
}
