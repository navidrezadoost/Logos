//! Replay cursor — step-by-step traversal of operation history.
//!
//! `ReplayCursor` allows stepping forward and backward through
//! operations one at a time, maintaining a current position
//! and the corresponding state.

use crate::engine::{OpApplier, ReplayEngine, StateContainer};
use crate::envelope::OpEnvelope;
use crate::error::ReplayError;
use crate::oplog::OpLog;

/// Direction of cursor movement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorDirection {
    Forward,
    Backward,
}

/// A cursor for stepping through replay history.
///
/// The cursor maintains the current version position and can
/// step forward/backward, applying or unapplying operations.
pub struct ReplayCursor<S, A, L>
where
    S: StateContainer,
    A: OpApplier<S>,
    L: OpLog<A::Op>,
{
    engine: ReplayEngine<S, A, L>,
    document_id: uuid::Uuid,
    current_version: u64,
    current_state: S,
    min_version: u64,
    max_version: u64,
}

impl<S, A, L> ReplayCursor<S, A, L>
where
    S: StateContainer,
    A: OpApplier<S>,
    L: OpLog<A::Op>,
{
    /// Create a cursor starting at a specific version.
    pub fn new(
        engine: ReplayEngine<S, A, L>,
        document_id: uuid::Uuid,
        start_version: u64,
    ) -> Result<Self, ReplayError> {
        let max_version = engine
            .log
            .latest_version()
            .ok_or(ReplayError::EmptyLog)?;

        if start_version > max_version {
            return Err(ReplayError::VersionOutOfRange {
                requested: start_version,
                max: max_version,
            });
        }

        let result = engine.replay_to(start_version, &document_id)?;

        Ok(Self {
            engine,
            document_id,
            current_version: start_version,
            current_state: result.state,
            min_version: 1,
            max_version,
        })
    }

    /// Create a cursor at the beginning (version 1).
    pub fn at_start(
        engine: ReplayEngine<S, A, L>,
        document_id: uuid::Uuid,
    ) -> Result<Self, ReplayError> {
        Self::new(engine, document_id, 1)
    }

    /// Create a cursor at the latest version.
    pub fn at_end(
        engine: ReplayEngine<S, A, L>,
        document_id: uuid::Uuid,
    ) -> Result<Self, ReplayError> {
        let max = engine
            .log
            .latest_version()
            .ok_or(ReplayError::EmptyLog)?;
        Self::new(engine, document_id, max)
    }

    /// Current version position.
    pub fn version(&self) -> u64 {
        self.current_version
    }

    /// Current state.
    pub fn state(&self) -> &S {
        &self.current_state
    }

    /// Whether we can step forward.
    pub fn can_forward(&self) -> bool {
        self.current_version < self.max_version
    }

    /// Whether we can step backward.
    pub fn can_backward(&self) -> bool {
        self.current_version > self.min_version
    }

    /// Step forward by one operation.
    pub fn step_forward(&mut self) -> Result<&OpEnvelope<A::Op>, ReplayError> {
        if !self.can_forward() {
            return Err(ReplayError::VersionOutOfRange {
                requested: self.current_version + 1,
                max: self.max_version,
            });
        }

        let next_version = self.current_version + 1;
        let env = self.engine.log.get(next_version)?;
        self.engine.applier.apply(&mut self.current_state, env)?;
        self.current_version = next_version;
        self.engine.log.get(self.current_version)
    }

    /// Step backward by one operation (requires unapply support).
    pub fn step_backward(&mut self) -> Result<(), ReplayError> {
        if !self.can_backward() {
            return Err(ReplayError::VersionOutOfRange {
                requested: self.current_version.saturating_sub(1),
                max: self.max_version,
            });
        }

        // Try unapply first.
        let env = self.engine.log.get(self.current_version)?;
        let unapply_result = self.engine.applier.unapply(&mut self.current_state, env);

        if unapply_result.is_ok() {
            self.current_version -= 1;
            return Ok(());
        }

        // Fallback: re-replay from scratch to (current - 1).
        let target = self.current_version - 1;
        let result = self.engine.replay_to(target, &self.document_id)?;
        self.current_state = result.state;
        self.current_version = target;
        Ok(())
    }

    /// Step N operations in a direction.
    pub fn step_n(
        &mut self,
        direction: CursorDirection,
        n: usize,
    ) -> Result<usize, ReplayError> {
        let mut stepped = 0;
        for _ in 0..n {
            let result = match direction {
                CursorDirection::Forward => self.step_forward().map(|_| ()),
                CursorDirection::Backward => self.step_backward(),
            };
            match result {
                Ok(()) => stepped += 1,
                Err(ReplayError::VersionOutOfRange { .. }) => break,
                Err(e) => return Err(e),
            }
        }
        Ok(stepped)
    }

    /// Jump to a specific version (re-replays from nearest snapshot).
    pub fn jump_to(&mut self, version: u64) -> Result<(), ReplayError> {
        if version > self.max_version {
            return Err(ReplayError::VersionOutOfRange {
                requested: version,
                max: self.max_version,
            });
        }
        let result = self.engine.replay_to(version, &self.document_id)?;
        self.current_state = result.state;
        self.current_version = version;
        Ok(())
    }

    /// Get the operation at the current version.
    pub fn current_op(&self) -> Result<&OpEnvelope<A::Op>, ReplayError> {
        self.engine.log.get(self.current_version)
    }

    /// Get a window of operations around the current position.
    pub fn window(
        &self,
        before: u64,
        after: u64,
    ) -> Result<Vec<&OpEnvelope<A::Op>>, ReplayError> {
        let start = self.current_version.saturating_sub(before);
        let end = (self.current_version + after).min(self.max_version);
        self.engine.log.range(start, end)
    }

    /// Fraction of progress through the log (0.0 to 1.0).
    pub fn progress(&self) -> f64 {
        if self.max_version == self.min_version {
            return 1.0;
        }
        (self.current_version - self.min_version) as f64
            / (self.max_version - self.min_version) as f64
    }

    /// Remaining ops until the end.
    pub fn remaining(&self) -> u64 {
        self.max_version - self.current_version
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
    use serde::{Deserialize, Serialize};
    use uuid::Uuid;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct Acc {
        total: i64,
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    enum AccOp {
        Add(i64),
    }

    struct AccApplier;

    impl OpApplier<Acc> for AccApplier {
        type Op = AccOp;

        fn apply(
            &self,
            state: &mut Acc,
            envelope: &OpEnvelope<AccOp>,
        ) -> Result<(), ReplayError> {
            match &envelope.op {
                AccOp::Add(n) => state.total += n,
            }
            Ok(())
        }

        fn unapply(
            &self,
            state: &mut Acc,
            envelope: &OpEnvelope<AccOp>,
        ) -> Result<(), ReplayError> {
            match &envelope.op {
                AccOp::Add(n) => state.total -= n,
            }
            Ok(())
        }
    }

    fn make_env(version: u64, n: i64, doc: Uuid) -> OpEnvelope<AccOp> {
        let meta = OpMetadata::new(UserId::new(), doc, LamportClock::new());
        OpEnvelope::new(version, AccOp::Add(n), meta, "acc")
    }

    fn setup_cursor(
        ops: usize,
        start: u64,
    ) -> (ReplayCursor<Acc, AccApplier, InMemoryOpLog<AccOp>>, Uuid) {
        let doc = Uuid::new_v4();
        let mut log = InMemoryOpLog::new();
        for v in 1..=(ops as u64) {
            log.append(make_env(v, v as i64, doc)).unwrap();
        }
        let engine = ReplayEngine::new(Acc { total: 0 }, AccApplier, log);
        let cursor = ReplayCursor::new(engine, doc, start).unwrap();
        (cursor, doc)
    }

    #[test]
    fn cursor_at_start() {
        let (cursor, _) = setup_cursor(5, 1);
        assert_eq!(cursor.version(), 1);
        assert_eq!(cursor.state().total, 1); // Add(1)
        assert!(cursor.can_forward());
    }

    #[test]
    fn cursor_at_end() {
        let doc = Uuid::new_v4();
        let mut log = InMemoryOpLog::new();
        for v in 1..=5 {
            log.append(make_env(v, v as i64, doc)).unwrap();
        }
        let engine = ReplayEngine::new(Acc { total: 0 }, AccApplier, log);
        let cursor = ReplayCursor::at_end(engine, doc).unwrap();
        assert_eq!(cursor.version(), 5);
        assert_eq!(cursor.state().total, 15); // 1+2+3+4+5
        assert!(!cursor.can_forward());
        assert!(cursor.can_backward());
    }

    #[test]
    fn step_forward() {
        let (mut cursor, _) = setup_cursor(5, 1);
        assert_eq!(cursor.state().total, 1);

        cursor.step_forward().unwrap();
        assert_eq!(cursor.version(), 2);
        assert_eq!(cursor.state().total, 3); // 1+2

        cursor.step_forward().unwrap();
        assert_eq!(cursor.version(), 3);
        assert_eq!(cursor.state().total, 6); // 1+2+3
    }

    #[test]
    fn step_backward_with_unapply() {
        let (mut cursor, _) = setup_cursor(5, 3);
        assert_eq!(cursor.state().total, 6); // 1+2+3

        cursor.step_backward().unwrap();
        assert_eq!(cursor.version(), 2);
        assert_eq!(cursor.state().total, 3); // 1+2
    }

    #[test]
    fn step_forward_at_end() {
        let (mut cursor, _) = setup_cursor(3, 3);
        assert!(!cursor.can_forward());
        assert!(matches!(
            cursor.step_forward(),
            Err(ReplayError::VersionOutOfRange { .. })
        ));
    }

    #[test]
    fn step_backward_at_start() {
        let (mut cursor, _) = setup_cursor(3, 1);
        assert!(!cursor.can_backward());
        assert!(matches!(
            cursor.step_backward(),
            Err(ReplayError::VersionOutOfRange { .. })
        ));
    }

    #[test]
    fn step_n_forward() {
        let (mut cursor, _) = setup_cursor(10, 1);
        let stepped = cursor.step_n(CursorDirection::Forward, 5).unwrap();
        assert_eq!(stepped, 5);
        assert_eq!(cursor.version(), 6);
        assert_eq!(cursor.state().total, 21); // 1+2+3+4+5+6
    }

    #[test]
    fn step_n_beyond_end() {
        let (mut cursor, _) = setup_cursor(5, 3);
        let stepped = cursor.step_n(CursorDirection::Forward, 10).unwrap();
        assert_eq!(stepped, 2); // Only 2 more ops (4, 5)
        assert_eq!(cursor.version(), 5);
    }

    #[test]
    fn jump_to() {
        let (mut cursor, _) = setup_cursor(10, 1);
        cursor.jump_to(7).unwrap();
        assert_eq!(cursor.version(), 7);
        assert_eq!(cursor.state().total, 28); // 1+2+3+4+5+6+7
    }

    #[test]
    fn jump_out_of_range() {
        let (mut cursor, _) = setup_cursor(5, 1);
        assert!(matches!(
            cursor.jump_to(100),
            Err(ReplayError::VersionOutOfRange { .. })
        ));
    }

    #[test]
    fn current_op() {
        let (cursor, _) = setup_cursor(5, 3);
        let op = cursor.current_op().unwrap();
        assert_eq!(op.version, 3);
    }

    #[test]
    fn window() {
        let (cursor, _) = setup_cursor(10, 5);
        let w = cursor.window(2, 2).unwrap();
        assert_eq!(w.len(), 5); // versions 3,4,5,6,7
        assert_eq!(w[0].version, 3);
        assert_eq!(w[4].version, 7);
    }

    #[test]
    fn progress() {
        let (mut cursor, _) = setup_cursor(10, 1);
        assert!((cursor.progress() - 0.0).abs() < 0.01);
        cursor.jump_to(5).unwrap();
        let p = cursor.progress();
        // (5-1)/(10-1) ≈ 0.444
        assert!(p > 0.4 && p < 0.5);
    }

    #[test]
    fn remaining() {
        let (cursor, _) = setup_cursor(10, 7);
        assert_eq!(cursor.remaining(), 3); // 8, 9, 10
    }
}
