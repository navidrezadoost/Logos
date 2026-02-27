//! Offline — queue ops while disconnected, replay on reconnect.
//!
//! When a peer loses connection, operations are buffered locally.
//! Upon reconnection the queued ops are merged with remote state
//! via a deterministic replay plan.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use uuid::Uuid;

use crate::error::MultiplayerError;
use crate::peer::PeerId;
use crate::sync_protocol::OpBroadcast;

/// An operation created while offline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfflineOp {
    /// The op as it would have been broadcast.
    pub op: OpBroadcast,
    /// Local sequence number (monotonic within the queue).
    pub local_seq: u64,
    /// Whether this op has been rebased against remote state.
    pub rebased: bool,
}

impl OfflineOp {
    pub fn new(op: OpBroadcast, local_seq: u64) -> Self {
        Self {
            op,
            local_seq,
            rebased: false,
        }
    }

    /// Mark as successfully rebased.
    pub fn mark_rebased(&mut self) {
        self.rebased = true;
    }
}

/// A bounded offline operation queue.
pub struct OfflineQueue {
    ops: VecDeque<OfflineOp>,
    max_size: usize,
    next_seq: u64,
    peer_id: PeerId,
    document_id: Uuid,
}

impl OfflineQueue {
    pub fn new(peer_id: PeerId, document_id: Uuid, max_size: usize) -> Self {
        Self {
            ops: VecDeque::new(),
            max_size,
            next_seq: 0,
            peer_id,
            document_id,
        }
    }

    /// Enqueue an operation created while offline.
    pub fn enqueue(&mut self, op: OpBroadcast) -> Result<u64, MultiplayerError> {
        if self.ops.len() >= self.max_size {
            return Err(MultiplayerError::QueueFull {
                capacity: self.max_size,
            });
        }
        let seq = self.next_seq;
        self.next_seq += 1;
        self.ops.push_back(OfflineOp::new(op, seq));
        Ok(seq)
    }

    /// Number of queued operations.
    pub fn len(&self) -> usize {
        self.ops.len()
    }

    /// Whether the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// Peek at the oldest queued op.
    pub fn peek_front(&self) -> Option<&OfflineOp> {
        self.ops.front()
    }

    /// Drain all operations from the queue for replay.
    pub fn drain_all(&mut self) -> Vec<OfflineOp> {
        self.ops.drain(..).collect()
    }

    /// Clear the queue (discard all pending ops).
    pub fn clear(&mut self) {
        self.ops.clear();
        self.next_seq = 0;
    }

    /// The peer who owns this queue.
    pub fn peer_id(&self) -> PeerId {
        self.peer_id
    }

    /// The document this queue is for.
    pub fn document_id(&self) -> Uuid {
        self.document_id
    }

    /// Iterator over the queued ops.
    pub fn iter(&self) -> impl Iterator<Item = &OfflineOp> {
        self.ops.iter()
    }
}

/// A replay plan that merges offline ops with remote ops.
///
/// After reconnecting, the peer receives remote ops that happened
/// while they were offline. The replay plan interleaves local and
/// remote ops in causal order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayPlan {
    /// Steps to execute in order.
    pub steps: Vec<ReplayStep>,
    /// Number of local ops included.
    pub local_count: usize,
    /// Number of remote ops included.
    pub remote_count: usize,
}

/// A single step in a replay plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReplayStep {
    /// Apply a remote op first (it happened before our local ops).
    ApplyRemote(OpBroadcast),
    /// Apply a local op (possibly rebased).
    ApplyLocal(OpBroadcast),
}

impl ReplayPlan {
    /// Build a replay plan from offline and remote ops.
    ///
    /// Remote ops are applied first (in Lamport order), then local
    /// ops on top (preserving their local sequence).
    pub fn build(
        offline_ops: Vec<OfflineOp>,
        remote_ops: Vec<OpBroadcast>,
    ) -> Self {
        let local_count = offline_ops.len();
        let remote_count = remote_ops.len();

        let mut steps = Vec::with_capacity(local_count + remote_count);

        // Remote ops in Lamport order first.
        let mut remote_sorted = remote_ops;
        remote_sorted.sort_by_key(|op| op.lamport);
        for op in remote_sorted {
            steps.push(ReplayStep::ApplyRemote(op));
        }

        // Then local ops in sequence order.
        let mut local_sorted = offline_ops;
        local_sorted.sort_by_key(|o| o.local_seq);
        for offline in local_sorted {
            steps.push(ReplayStep::ApplyLocal(offline.op));
        }

        Self {
            steps,
            local_count,
            remote_count,
        }
    }

    /// Total steps in the plan.
    pub fn total_steps(&self) -> usize {
        self.steps.len()
    }

    /// Whether this plan has any local ops to rebase.
    pub fn has_rebasing(&self) -> bool {
        self.local_count > 0 && self.remote_count > 0
    }

    /// Extract just the local ops from the plan.
    pub fn local_ops(&self) -> Vec<&OpBroadcast> {
        self.steps
            .iter()
            .filter_map(|s| match s {
                ReplayStep::ApplyLocal(op) => Some(op),
                _ => None,
            })
            .collect()
    }

    /// Extract just the remote ops from the plan.
    pub fn remote_ops(&self) -> Vec<&OpBroadcast> {
        self.steps
            .iter()
            .filter_map(|s| match s {
                ReplayStep::ApplyRemote(op) => Some(op),
                _ => None,
            })
            .collect()
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_op(sender: PeerId, lamport: u64) -> OpBroadcast {
        OpBroadcast {
            sender,
            document_id: Uuid::nil(),
            version: lamport,
            lamport,
            payload: json!({}),
            timestamp: 0,
            description: None,
            domain: String::new(),
        }
    }

    #[test]
    fn enqueue_dequeue() {
        let mut q = OfflineQueue::new(PeerId::new(), Uuid::new_v4(), 100);
        let seq = q.enqueue(make_op(PeerId::new(), 1)).unwrap();
        assert_eq!(seq, 0);
        assert_eq!(q.len(), 1);
    }

    #[test]
    fn queue_full() {
        let mut q = OfflineQueue::new(PeerId::new(), Uuid::new_v4(), 2);
        q.enqueue(make_op(PeerId::new(), 1)).unwrap();
        q.enqueue(make_op(PeerId::new(), 2)).unwrap();
        let result = q.enqueue(make_op(PeerId::new(), 3));
        assert!(result.is_err());
    }

    #[test]
    fn drain_clears() {
        let mut q = OfflineQueue::new(PeerId::new(), Uuid::new_v4(), 100);
        q.enqueue(make_op(PeerId::new(), 1)).unwrap();
        q.enqueue(make_op(PeerId::new(), 2)).unwrap();
        let ops = q.drain_all();
        assert_eq!(ops.len(), 2);
        assert!(q.is_empty());
    }

    #[test]
    fn peek_front() {
        let mut q = OfflineQueue::new(PeerId::new(), Uuid::new_v4(), 100);
        q.enqueue(make_op(PeerId::new(), 1)).unwrap();
        q.enqueue(make_op(PeerId::new(), 2)).unwrap();
        assert_eq!(q.peek_front().unwrap().local_seq, 0);
    }

    #[test]
    fn clear_queue() {
        let mut q = OfflineQueue::new(PeerId::new(), Uuid::new_v4(), 100);
        q.enqueue(make_op(PeerId::new(), 1)).unwrap();
        q.clear();
        assert!(q.is_empty());
    }

    #[test]
    fn offline_op_rebase() {
        let mut op = OfflineOp::new(make_op(PeerId::new(), 1), 0);
        assert!(!op.rebased);
        op.mark_rebased();
        assert!(op.rebased);
    }

    #[test]
    fn replay_plan_remote_only() {
        let remote = vec![make_op(PeerId::new(), 3), make_op(PeerId::new(), 1)];
        let plan = ReplayPlan::build(vec![], remote);
        assert_eq!(plan.total_steps(), 2);
        assert_eq!(plan.remote_count, 2);
        assert_eq!(plan.local_count, 0);
        assert!(!plan.has_rebasing());
        // Remote ops should be in Lamport order.
        if let ReplayStep::ApplyRemote(first) = &plan.steps[0] {
            assert_eq!(first.lamport, 1);
        }
    }

    #[test]
    fn replay_plan_local_only() {
        let local = vec![
            OfflineOp::new(make_op(PeerId::new(), 1), 0),
            OfflineOp::new(make_op(PeerId::new(), 2), 1),
        ];
        let plan = ReplayPlan::build(local, vec![]);
        assert_eq!(plan.total_steps(), 2);
        assert!(!plan.has_rebasing());
    }

    #[test]
    fn replay_plan_mixed() {
        let local = vec![
            OfflineOp::new(make_op(PeerId::new(), 10), 0),
        ];
        let remote = vec![
            make_op(PeerId::new(), 5),
            make_op(PeerId::new(), 7),
        ];
        let plan = ReplayPlan::build(local, remote);
        assert_eq!(plan.total_steps(), 3);
        assert!(plan.has_rebasing());
        assert_eq!(plan.local_ops().len(), 1);
        assert_eq!(plan.remote_ops().len(), 2);
    }

    #[test]
    fn replay_plan_serde() {
        let plan = ReplayPlan::build(
            vec![OfflineOp::new(make_op(PeerId::new(), 1), 0)],
            vec![make_op(PeerId::new(), 2)],
        );
        let json = serde_json::to_string(&plan).unwrap();
        let back: ReplayPlan = serde_json::from_str(&json).unwrap();
        assert_eq!(back.total_steps(), 2);
    }

    #[test]
    fn queue_iter() {
        let mut q = OfflineQueue::new(PeerId::new(), Uuid::new_v4(), 100);
        q.enqueue(make_op(PeerId::new(), 1)).unwrap();
        q.enqueue(make_op(PeerId::new(), 2)).unwrap();
        let seqs: Vec<u64> = q.iter().map(|o| o.local_seq).collect();
        assert_eq!(seqs, vec![0, 1]);
    }
}
