//! Replay engine — deterministic state reconstruction.
//!
//! The `ReplayEngine` takes an `OpLog`, an `OpApplier`, and optionally
//! a `SnapshotStore`, and can reconstruct the state at any version by
//! replaying operations from the nearest snapshot forward.

use serde::{Deserialize, Serialize};

use crate::envelope::OpEnvelope;
use crate::error::ReplayError;
use crate::oplog::OpLog;
use crate::snapshot::{InMemorySnapshotStore, Snapshot, SnapshotPolicy, SnapshotStore};

/// Result of replaying operations to a target version.
#[derive(Debug, Clone)]
pub struct ReplayResult<S> {
    /// The reconstructed state.
    pub state: S,
    /// The version we replayed to.
    pub version: u64,
    /// Number of operations applied.
    pub ops_applied: usize,
    /// Whether we started from a snapshot (vs. from scratch).
    pub from_snapshot: bool,
    /// The snapshot version we started from (if any).
    pub snapshot_version: Option<u64>,
}

/// Trait that consumers implement to define how operations are applied
/// to state. This is the core integration point for domain-specific
/// operation types.
pub trait OpApplier<S> {
    type Op: Serialize + for<'de> Deserialize<'de>;

    /// Apply a single operation to the state, producing a new state.
    fn apply(
        &self,
        state: &mut S,
        envelope: &OpEnvelope<Self::Op>,
    ) -> Result<(), ReplayError>;

    /// Reverse-apply an operation (for undo/backward replay).
    /// Default implementation returns an error.
    fn unapply(
        &self,
        state: &mut S,
        envelope: &OpEnvelope<Self::Op>,
    ) -> Result<(), ReplayError> {
        let _ = (state, envelope);
        Err(ReplayError::ApplyError {
            version: 0,
            reason: "unapply not implemented".into(),
        })
    }
}

/// Trait for state containers that can be serialized/deserialized
/// for snapshot storage.
pub trait StateContainer: Clone + Sized {
    /// Serialize this state to a JSON value.
    fn to_value(&self) -> Result<serde_json::Value, serde_json::Error>;
    /// Deserialize from a JSON value.
    fn from_value(value: serde_json::Value) -> Result<Self, serde_json::Error>;
}

/// Blanket implementation for types that implement both Serialize and
/// DeserializeOwned + Clone.
impl<T> StateContainer for T
where
    T: Serialize + for<'de> Deserialize<'de> + Clone,
{
    fn to_value(&self) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::to_value(self)
    }

    fn from_value(value: serde_json::Value) -> Result<Self, serde_json::Error> {
        serde_json::from_value(value)
    }
}

/// The replay engine.
///
/// Parameterized over:
/// - `S`: the state type
/// - `A`: the applier (knows how to apply ops)
/// - `L`: the operation log
pub struct ReplayEngine<S, A, L>
where
    S: StateContainer,
    A: OpApplier<S>,
    L: OpLog<A::Op>,
{
    pub applier: A,
    pub log: L,
    pub snapshots: InMemorySnapshotStore,
    pub policy: SnapshotPolicy,
    initial_state: S,
}

impl<S, A, L> ReplayEngine<S, A, L>
where
    S: StateContainer,
    A: OpApplier<S>,
    L: OpLog<A::Op>,
{
    /// Create a new replay engine.
    pub fn new(initial_state: S, applier: A, log: L) -> Self {
        Self {
            applier,
            log,
            snapshots: InMemorySnapshotStore::new(),
            policy: SnapshotPolicy::default(),
            initial_state,
        }
    }

    /// Set snapshot policy.
    pub fn with_policy(mut self, policy: SnapshotPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// Replay from the beginning (or nearest snapshot) up to `target_version`.
    pub fn replay_to(
        &self,
        target_version: u64,
        document_id: &uuid::Uuid,
    ) -> Result<ReplayResult<S>, ReplayError> {
        // Validate target.
        let latest = self.log.latest_version().ok_or(ReplayError::EmptyLog)?;
        if target_version > latest {
            return Err(ReplayError::VersionOutOfRange {
                requested: target_version,
                max: latest,
            });
        }

        // Find nearest snapshot.
        let (mut state, start_version, from_snapshot, snap_ver) =
            if let Some(snap) = self.snapshots.find_nearest(document_id, target_version) {
                let s = S::from_value(snap.state.clone()).map_err(|e| {
                    ReplayError::SnapshotCorrupted {
                        reason: e.to_string(),
                    }
                })?;
                (s, snap.version + 1, true, Some(snap.version))
            } else {
                (self.initial_state.clone(), 1, false, None)
            };

        // Replay ops from start to target.
        let ops = self.log.range(start_version, target_version)?;
        let ops_applied = ops.len();

        for env in ops {
            self.applier.apply(&mut state, env).map_err(|_| {
                ReplayError::ApplyError {
                    version: env.version,
                    reason: "apply failed during replay".into(),
                }
            })?;
        }

        Ok(ReplayResult {
            state,
            version: target_version,
            ops_applied,
            from_snapshot,
            snapshot_version: snap_ver,
        })
    }

    /// Replay to the latest version.
    pub fn replay_latest(
        &self,
        document_id: &uuid::Uuid,
    ) -> Result<ReplayResult<S>, ReplayError> {
        let latest = self.log.latest_version().ok_or(ReplayError::EmptyLog)?;
        self.replay_to(latest, document_id)
    }

    /// Append an operation and potentially take a snapshot.
    pub fn append_and_snapshot(
        &mut self,
        env: OpEnvelope<A::Op>,
        document_id: &uuid::Uuid,
    ) -> Result<u64, ReplayError> {
        let version = self.log.append(env)?;

        let last_snap_ver = self.snapshots.latest(document_id).map(|s| s.version);

        if self.policy.should_snapshot(version, last_snap_ver) {
            // Build state up to this version for snapshot.
            let result = self.replay_to(version, document_id)?;
            let state_value = result.state.to_value().map_err(|e| {
                ReplayError::SerializationError {
                    reason: e.to_string(),
                }
            })?;
            let snapshot = Snapshot::new(version, *document_id, state_value);
            self.snapshots.save(snapshot)?;

            // Enforce limit.
            if let Some(max) = self.policy.max_snapshots {
                self.snapshots.enforce_limit(document_id, max);
            }
        }

        Ok(version)
    }

    /// Verify replay determinism: replay to version and compare with expected.
    pub fn verify_determinism(
        &self,
        target_version: u64,
        document_id: &uuid::Uuid,
        expected: &serde_json::Value,
    ) -> Result<bool, ReplayError> {
        let result = self.replay_to(target_version, document_id)?;
        let actual = result.state.to_value().map_err(|e| {
            ReplayError::SerializationError {
                reason: e.to_string(),
            }
        })?;
        Ok(actual == *expected)
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::LamportClock;
    use crate::envelope::OpMetadata;
    use crate::oplog::InMemoryOpLog;
    use logos_identity::UserId;
    use uuid::Uuid;

    // ── Test domain ──────────────────────────────────────────────────

    /// Simple key-value store as our test state.
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct KvState {
        data: std::collections::HashMap<String, i64>,
    }

    impl KvState {
        fn new() -> Self {
            Self {
                data: std::collections::HashMap::new(),
            }
        }
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    enum KvOp {
        Set { key: String, value: i64 },
        Delete { key: String },
        Increment { key: String, amount: i64 },
    }

    struct KvApplier;

    impl OpApplier<KvState> for KvApplier {
        type Op = KvOp;

        fn apply(
            &self,
            state: &mut KvState,
            envelope: &OpEnvelope<KvOp>,
        ) -> Result<(), ReplayError> {
            match &envelope.op {
                KvOp::Set { key, value } => {
                    state.data.insert(key.clone(), *value);
                }
                KvOp::Delete { key } => {
                    state.data.remove(key);
                }
                KvOp::Increment { key, amount } => {
                    let entry = state.data.entry(key.clone()).or_insert(0);
                    *entry += amount;
                }
            }
            Ok(())
        }

        fn unapply(
            &self,
            state: &mut KvState,
            envelope: &OpEnvelope<KvOp>,
        ) -> Result<(), ReplayError> {
            match &envelope.op {
                KvOp::Set { key, .. } => {
                    state.data.remove(key);
                    Ok(())
                }
                KvOp::Delete { key } => {
                    // Can't fully undo without stored previous value.
                    state.data.insert(key.clone(), 0);
                    Ok(())
                }
                KvOp::Increment { key, amount } => {
                    let entry = state.data.entry(key.clone()).or_insert(0);
                    *entry -= amount;
                    Ok(())
                }
            }
        }
    }

    fn make_env(version: u64, op: KvOp, doc: Uuid) -> OpEnvelope<KvOp> {
        let meta = OpMetadata::new(UserId::new(), doc, LamportClock::new());
        OpEnvelope::new(version, op, meta, "kv")
    }

    fn make_engine(
        doc: Uuid,
    ) -> (
        ReplayEngine<KvState, KvApplier, InMemoryOpLog<KvOp>>,
        Uuid,
    ) {
        let engine = ReplayEngine::new(
            KvState::new(),
            KvApplier,
            InMemoryOpLog::new(),
        );
        (engine, doc)
    }

    // ── Engine tests ─────────────────────────────────────────────────

    #[test]
    fn replay_empty_log() {
        let doc = Uuid::new_v4();
        let (engine, _) = make_engine(doc);
        assert!(matches!(
            engine.replay_to(1, &doc),
            Err(ReplayError::EmptyLog)
        ));
    }

    #[test]
    fn replay_single_op() {
        let doc = Uuid::new_v4();
        let (mut engine, _) = make_engine(doc);
        engine
            .log
            .append(make_env(1, KvOp::Set { key: "x".into(), value: 42 }, doc))
            .unwrap();

        let result = engine.replay_to(1, &doc).unwrap();
        assert_eq!(result.state.data.get("x"), Some(&42));
        assert_eq!(result.ops_applied, 1);
        assert!(!result.from_snapshot);
    }

    #[test]
    fn replay_multiple_ops() {
        let doc = Uuid::new_v4();
        let (mut engine, _) = make_engine(doc);

        engine
            .log
            .append(make_env(1, KvOp::Set { key: "a".into(), value: 1 }, doc))
            .unwrap();
        engine
            .log
            .append(make_env(2, KvOp::Set { key: "b".into(), value: 2 }, doc))
            .unwrap();
        engine
            .log
            .append(make_env(3, KvOp::Increment { key: "a".into(), amount: 10 }, doc))
            .unwrap();
        engine
            .log
            .append(make_env(4, KvOp::Delete { key: "b".into() }, doc))
            .unwrap();

        let result = engine.replay_to(4, &doc).unwrap();
        assert_eq!(result.state.data.get("a"), Some(&11));
        assert_eq!(result.state.data.get("b"), None);
        assert_eq!(result.ops_applied, 4);
    }

    #[test]
    fn replay_to_intermediate_version() {
        let doc = Uuid::new_v4();
        let (mut engine, _) = make_engine(doc);

        for v in 1..=10 {
            engine
                .log
                .append(make_env(
                    v,
                    KvOp::Set {
                        key: format!("k{}", v),
                        value: v as i64,
                    },
                    doc,
                ))
                .unwrap();
        }

        let r5 = engine.replay_to(5, &doc).unwrap();
        assert_eq!(r5.state.data.len(), 5);
        assert_eq!(r5.ops_applied, 5);
        assert_eq!(r5.state.data.get("k5"), Some(&5));
        assert_eq!(r5.state.data.get("k6"), None);
    }

    #[test]
    fn replay_version_out_of_range() {
        let doc = Uuid::new_v4();
        let (mut engine, _) = make_engine(doc);
        engine
            .log
            .append(make_env(1, KvOp::Set { key: "x".into(), value: 1 }, doc))
            .unwrap();

        assert!(matches!(
            engine.replay_to(100, &doc),
            Err(ReplayError::VersionOutOfRange { .. })
        ));
    }

    #[test]
    fn replay_latest() {
        let doc = Uuid::new_v4();
        let (mut engine, _) = make_engine(doc);
        engine
            .log
            .append(make_env(1, KvOp::Set { key: "x".into(), value: 1 }, doc))
            .unwrap();
        engine
            .log
            .append(make_env(2, KvOp::Set { key: "y".into(), value: 2 }, doc))
            .unwrap();

        let result = engine.replay_latest(&doc).unwrap();
        assert_eq!(result.version, 2);
        assert_eq!(result.state.data.len(), 2);
    }

    #[test]
    fn replay_with_snapshot() {
        let doc = Uuid::new_v4();
        let (mut engine, _) = make_engine(doc);

        // Add some ops.
        for v in 1..=10 {
            engine
                .log
                .append(make_env(
                    v,
                    KvOp::Set {
                        key: format!("k{}", v),
                        value: v as i64,
                    },
                    doc,
                ))
                .unwrap();
        }

        // Take a snapshot at version 5.
        let state_at_5 = engine.replay_to(5, &doc).unwrap().state;
        let snap = Snapshot::new(5, doc, state_at_5.to_value().unwrap());
        engine.snapshots.save(snap).unwrap();

        // Replay to version 8 — should start from snapshot.
        let result = engine.replay_to(8, &doc).unwrap();
        assert_eq!(result.ops_applied, 3); // versions 6, 7, 8
        assert!(result.from_snapshot);
        assert_eq!(result.snapshot_version, Some(5));
        assert_eq!(result.state.data.len(), 8);
    }

    #[test]
    fn append_and_snapshot_auto() {
        let doc = Uuid::new_v4();
        let mut engine = ReplayEngine::new(
            KvState::new(),
            KvApplier,
            InMemoryOpLog::new(),
        )
        .with_policy(SnapshotPolicy::every_n_ops(5));

        for v in 1..=12 {
            let env = make_env(
                v,
                KvOp::Set {
                    key: format!("k{}", v),
                    value: v as i64,
                },
                doc,
            );
            engine.append_and_snapshot(env, &doc).unwrap();
        }

        // Should have snapshots at version 5 and 10.
        let snaps = engine.snapshots.list(&doc);
        assert_eq!(snaps.len(), 2);
        assert_eq!(snaps[0].version, 5);
        assert_eq!(snaps[1].version, 10);
    }

    #[test]
    fn verify_determinism_matches() {
        let doc = Uuid::new_v4();
        let (mut engine, _) = make_engine(doc);
        engine
            .log
            .append(make_env(1, KvOp::Set { key: "x".into(), value: 42 }, doc))
            .unwrap();

        let expected = serde_json::json!({
            "data": { "x": 42 }
        });
        assert!(engine.verify_determinism(1, &doc, &expected).unwrap());
    }

    #[test]
    fn verify_determinism_mismatch() {
        let doc = Uuid::new_v4();
        let (mut engine, _) = make_engine(doc);
        engine
            .log
            .append(make_env(1, KvOp::Set { key: "x".into(), value: 42 }, doc))
            .unwrap();

        let wrong = serde_json::json!({
            "data": { "x": 999 }
        });
        assert!(!engine.verify_determinism(1, &doc, &wrong).unwrap());
    }

    #[test]
    fn deterministic_replay_same_result() {
        let doc = Uuid::new_v4();
        let (mut engine, _) = make_engine(doc);

        for v in 1..=20 {
            engine
                .log
                .append(make_env(
                    v,
                    KvOp::Set {
                        key: format!("k{}", v),
                        value: v as i64 * 10,
                    },
                    doc,
                ))
                .unwrap();
        }

        // Replay twice — must yield identical states.
        let r1 = engine.replay_to(15, &doc).unwrap();
        let r2 = engine.replay_to(15, &doc).unwrap();
        assert_eq!(r1.state, r2.state);
    }
}
