//! Collaborative state — LWW-Register-per-cell CRDT.
//!
//! [`CollabState`] maintains a **Last-Writer-Wins (LWW) register** for every
//! cell that has been edited during a collaborative session. When a remote op
//! arrives, it is compared against the cell's current timestamp:
//!
//! - If the remote op is *later* → apply it (overwrite).
//! - If the remote op is *earlier* → discard it (stale).
//!
//! This guarantees **convergence**: all peers reach the same state regardless
//! of message ordering, because LWW resolution is commutative and idempotent.
//!
//! # Integration with `RecalcEngine`
//!
//! `CollabState` does **not** own the spreadsheet data. It tracks *metadata*
//! (timestamps, op history) and produces [`ApplyResult`] values that tell the
//! caller (typically a `CollabEngine`) what to write into the `RecalcEngine`.

use std::collections::HashMap;

use super::ops::{CellOp, CellPayload, LamportClock, OpTimestamp, SiteId};

/// The coordinate type (col, row).
pub type CellCoord = (u32, u32);

// ---------------------------------------------------------------------------
// Cell register
// ---------------------------------------------------------------------------

/// Per-cell LWW register: stores the "winning" timestamp and payload.
#[derive(Debug, Clone)]
struct CellRegister {
    /// The most recent op timestamp for this cell.
    timestamp: OpTimestamp,
    /// The most recent payload.
    payload: CellPayload,
}

impl CellRegister {
    fn new(timestamp: OpTimestamp, payload: CellPayload) -> Self {
        Self { timestamp, payload }
    }

    /// Try to update this register with a new op.
    /// Returns `true` if the op won (was applied), `false` if discarded.
    fn try_update(&mut self, timestamp: OpTimestamp, payload: CellPayload) -> bool {
        if timestamp.is_later_than(&self.timestamp) {
            self.timestamp = timestamp;
            self.payload = payload;
            true
        } else {
            false
        }
    }
}

// ---------------------------------------------------------------------------
// Apply result
// ---------------------------------------------------------------------------

/// The result of applying a remote operation.
#[derive(Debug, Clone, PartialEq)]
pub enum ApplyResult {
    /// The op was applied — caller should write this to the engine.
    Applied {
        col: u32,
        row: u32,
        payload: CellPayload,
    },
    /// The op was discarded (stale — a later op already exists).
    Discarded,
    /// The op was a duplicate (same timestamp already seen).
    Duplicate,
}

impl ApplyResult {
    /// Whether the op was successfully applied.
    pub fn was_applied(&self) -> bool {
        matches!(self, ApplyResult::Applied { .. })
    }
}

// ---------------------------------------------------------------------------
// Collab state
// ---------------------------------------------------------------------------

/// Per-cell CRDT state for a collaborative session.
///
/// Tracks the winning timestamp for each edited cell and provides
/// conflict resolution via LWW semantics.
#[derive(Debug, Clone)]
pub struct CollabState {
    /// LWW registers: only cells that have been edited during this session
    /// have an entry. Pristine cells (never edited) are not tracked.
    registers: HashMap<CellCoord, CellRegister>,

    /// The local site's Lamport clock.
    clock: LamportClock,

    /// The local site ID.
    site_id: SiteId,

    /// Operation log — all ops applied (both local and remote),
    /// ordered by application time.
    op_log: Vec<CellOp>,

    /// Statistics.
    stats: CollabStats,
}

/// Statistical counters for the collaborative session.
#[derive(Debug, Clone, Default)]
pub struct CollabStats {
    /// Total ops generated locally.
    pub local_ops: u64,
    /// Total remote ops received.
    pub remote_ops: u64,
    /// Remote ops that were applied (won LWW).
    pub remote_applied: u64,
    /// Remote ops that were discarded (lost LWW).
    pub remote_discarded: u64,
    /// Remote ops that were duplicates.
    pub remote_duplicates: u64,
}

impl CollabState {
    /// Create a new collaborative state for the given site.
    pub fn new(site_id: SiteId) -> Self {
        Self {
            registers: HashMap::new(),
            clock: LamportClock::new(0),
            site_id,
            op_log: Vec::new(),
            stats: CollabStats::default(),
        }
    }

    /// The local site ID.
    pub fn site_id(&self) -> SiteId {
        self.site_id
    }

    /// The current Lamport clock value.
    pub fn clock(&self) -> LamportClock {
        self.clock
    }

    /// Session statistics.
    pub fn stats(&self) -> &CollabStats {
        &self.stats
    }

    /// Number of cells with tracked state.
    pub fn tracked_cells(&self) -> usize {
        self.registers.len()
    }

    /// The full operation log (for debugging/audit).
    pub fn op_log(&self) -> &[CellOp] {
        &self.op_log
    }

    /// Get the current winning payload for a cell, if any.
    pub fn get_cell_payload(&self, col: u32, row: u32) -> Option<&CellPayload> {
        self.registers.get(&(col, row)).map(|r| &r.payload)
    }

    /// Get the winning timestamp for a cell, if any.
    pub fn get_cell_timestamp(&self, col: u32, row: u32) -> Option<OpTimestamp> {
        self.registers.get(&(col, row)).map(|r| r.timestamp)
    }

    // -----------------------------------------------------------------------
    // Local operations
    // -----------------------------------------------------------------------

    /// Record a local edit — generates a new op with an incremented clock.
    ///
    /// Returns the `CellOp` that should be broadcast to peers.
    pub fn local_edit(&mut self, col: u32, row: u32, payload: CellPayload) -> CellOp {
        let ts = OpTimestamp::new(self.clock.tick(), self.site_id);

        // Always apply local ops (they always win since clock is monotonic)
        let coord = (col, row);
        match self.registers.get_mut(&coord) {
            Some(reg) => {
                reg.timestamp = ts;
                reg.payload = payload.clone();
            }
            None => {
                self.registers
                    .insert(coord, CellRegister::new(ts, payload.clone()));
            }
        }

        let op = CellOp::new(col, row, payload, ts);
        self.op_log.push(op.clone());
        self.stats.local_ops += 1;
        op
    }

    // -----------------------------------------------------------------------
    // Remote operations
    // -----------------------------------------------------------------------

    /// Apply a remote operation.
    ///
    /// Returns an [`ApplyResult`] indicating whether the op was applied,
    /// discarded, or a duplicate.
    pub fn apply_remote(&mut self, op: &CellOp) -> ApplyResult {
        self.stats.remote_ops += 1;

        // Merge remote clock to maintain causal ordering
        self.clock.merge(op.timestamp.clock);

        let coord = op.coord();

        match self.registers.get_mut(&coord) {
            Some(reg) => {
                if op.timestamp == reg.timestamp {
                    self.stats.remote_duplicates += 1;
                    return ApplyResult::Duplicate;
                }
                if reg.try_update(op.timestamp, op.payload.clone()) {
                    self.op_log.push(op.clone());
                    self.stats.remote_applied += 1;
                    ApplyResult::Applied {
                        col: op.col,
                        row: op.row,
                        payload: op.payload.clone(),
                    }
                } else {
                    self.stats.remote_discarded += 1;
                    ApplyResult::Discarded
                }
            }
            None => {
                // No prior state — remote op wins unconditionally
                self.registers.insert(
                    coord,
                    CellRegister::new(op.timestamp, op.payload.clone()),
                );
                self.op_log.push(op.clone());
                self.stats.remote_applied += 1;
                ApplyResult::Applied {
                    col: op.col,
                    row: op.row,
                    payload: op.payload.clone(),
                }
            }
        }
    }

    /// Apply a batch of remote operations.
    ///
    /// Returns the list of results, one per op.
    pub fn apply_remote_batch(&mut self, ops: &[CellOp]) -> Vec<ApplyResult> {
        ops.iter().map(|op| self.apply_remote(op)).collect()
    }

    // -----------------------------------------------------------------------
    // State queries
    // -----------------------------------------------------------------------

    /// Get all ops since a given clock value (for delta sync).
    ///
    /// Returns ops from the log where `clock > since_clock`.
    pub fn ops_since(&self, since_clock: LamportClock) -> Vec<CellOp> {
        self.op_log
            .iter()
            .filter(|op| op.timestamp.clock > since_clock)
            .cloned()
            .collect()
    }

    /// Reset session statistics.
    pub fn reset_stats(&mut self) {
        self.stats = CollabStats::default();
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn site(id: u64) -> SiteId {
        SiteId::new(id)
    }

    fn ts(clock: u64, site: u64) -> OpTimestamp {
        OpTimestamp::new(LamportClock::new(clock), SiteId::new(site))
    }

    // --- Local edits ---

    #[test]
    fn local_edit_increments_clock() {
        let mut state = CollabState::new(site(1));
        let op1 = state.local_edit(0, 0, CellPayload::Number(1.0));
        let op2 = state.local_edit(0, 1, CellPayload::Number(2.0));
        assert!(op2.timestamp.clock > op1.timestamp.clock);
    }

    #[test]
    fn local_edit_updates_register() {
        let mut state = CollabState::new(site(1));
        state.local_edit(0, 0, CellPayload::Number(1.0));
        assert_eq!(
            *state.get_cell_payload(0, 0).unwrap(),
            CellPayload::Number(1.0)
        );

        state.local_edit(0, 0, CellPayload::Number(2.0));
        assert_eq!(
            *state.get_cell_payload(0, 0).unwrap(),
            CellPayload::Number(2.0)
        );
    }

    #[test]
    fn local_edit_returns_broadcastable_op() {
        let mut state = CollabState::new(site(1));
        let op = state.local_edit(3, 7, CellPayload::Formula("=A1+B1".into()));
        assert_eq!(op.col, 3);
        assert_eq!(op.row, 7);
        assert_eq!(op.payload, CellPayload::Formula("=A1+B1".into()));
        assert_eq!(op.timestamp.site_id, site(1));
    }

    // --- Remote ops (LWW) ---

    #[test]
    fn remote_op_applied_on_empty_cell() {
        let mut state = CollabState::new(site(1));
        let op = CellOp::new(0, 0, CellPayload::Number(42.0), ts(5, 2));
        let result = state.apply_remote(&op);
        assert!(result.was_applied());
        assert_eq!(
            *state.get_cell_payload(0, 0).unwrap(),
            CellPayload::Number(42.0)
        );
    }

    #[test]
    fn remote_op_wins_over_earlier_local() {
        let mut state = CollabState::new(site(1));
        state.local_edit(0, 0, CellPayload::Number(1.0)); // clock=1

        let remote = CellOp::new(0, 0, CellPayload::Number(99.0), ts(10, 2));
        let result = state.apply_remote(&remote);
        assert!(result.was_applied());
        assert_eq!(
            *state.get_cell_payload(0, 0).unwrap(),
            CellPayload::Number(99.0)
        );
    }

    #[test]
    fn remote_op_discarded_when_stale() {
        let mut state = CollabState::new(site(1));
        // Clock starts at 0, local edit bumps to 1
        state.local_edit(0, 0, CellPayload::Number(1.0));

        // Remote op with *lower* clock — should lose
        let stale = CellOp::new(0, 0, CellPayload::Number(0.5), ts(0, 2));
        let result = state.apply_remote(&stale);
        assert_eq!(result, ApplyResult::Discarded);
        assert_eq!(
            *state.get_cell_payload(0, 0).unwrap(),
            CellPayload::Number(1.0)
        );
    }

    #[test]
    fn concurrent_ops_same_clock_tiebreak_by_site_id() {
        let mut state = CollabState::new(site(1));
        // Local edit at clock=1, site=1
        state.local_edit(0, 0, CellPayload::Text("from site 1".into()));

        // Remote op at clock=1 (same!) but site=2 (higher) → wins
        let remote = CellOp::new(
            0, 0,
            CellPayload::Text("from site 2".into()),
            ts(1, 2),
        );
        let result = state.apply_remote(&remote);
        assert!(result.was_applied());
        assert_eq!(
            *state.get_cell_payload(0, 0).unwrap(),
            CellPayload::Text("from site 2".into())
        );
    }

    #[test]
    fn concurrent_ops_same_clock_lower_site_loses() {
        let mut state = CollabState::new(site(5));
        state.local_edit(0, 0, CellPayload::Text("from site 5".into()));
        // clock is now 1, site=5

        // Remote op at clock=1, site=3 (lower) → loses
        let remote = CellOp::new(
            0, 0,
            CellPayload::Text("from site 3".into()),
            ts(1, 3),
        );
        let result = state.apply_remote(&remote);
        assert_eq!(result, ApplyResult::Discarded);
    }

    #[test]
    fn duplicate_op_detected() {
        let mut state = CollabState::new(site(1));
        let op = CellOp::new(0, 0, CellPayload::Number(1.0), ts(5, 2));
        state.apply_remote(&op);

        // Same op again
        let result = state.apply_remote(&op);
        assert_eq!(result, ApplyResult::Duplicate);
    }

    // --- Batch ops ---

    #[test]
    fn apply_remote_batch() {
        let mut state = CollabState::new(site(1));
        let ops = vec![
            CellOp::new(0, 0, CellPayload::Number(1.0), ts(1, 2)),
            CellOp::new(1, 0, CellPayload::Number(2.0), ts(2, 2)),
            CellOp::new(2, 0, CellPayload::Number(3.0), ts(3, 2)),
        ];
        let results = state.apply_remote_batch(&ops);
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|r| r.was_applied()));
        assert_eq!(state.tracked_cells(), 3);
    }

    // --- Clock merge ---

    #[test]
    fn clock_merges_on_remote_receive() {
        let mut state = CollabState::new(site(1));
        assert_eq!(state.clock().value(), 0);

        let remote = CellOp::new(0, 0, CellPayload::Number(1.0), ts(10, 2));
        state.apply_remote(&remote);

        // Clock should merge: max(0, 10) + 1 = 11
        assert_eq!(state.clock().value(), 11);
    }

    // --- Ops since ---

    #[test]
    fn ops_since_returns_later_ops() {
        let mut state = CollabState::new(site(1));
        state.local_edit(0, 0, CellPayload::Number(1.0)); // clock=1
        state.local_edit(1, 0, CellPayload::Number(2.0)); // clock=2
        state.local_edit(2, 0, CellPayload::Number(3.0)); // clock=3

        let since = state.ops_since(LamportClock::new(1));
        assert_eq!(since.len(), 2); // ops at clock=2 and clock=3
    }

    // --- Stats ---

    #[test]
    fn stats_tracking() {
        let mut state = CollabState::new(site(1));
        state.local_edit(0, 0, CellPayload::Number(1.0));
        state.local_edit(1, 0, CellPayload::Number(2.0));

        let remote_win = CellOp::new(0, 0, CellPayload::Number(99.0), ts(10, 2));
        state.apply_remote(&remote_win);

        let remote_lose = CellOp::new(0, 0, CellPayload::Number(0.0), ts(1, 3));
        state.apply_remote(&remote_lose);

        let stats = state.stats();
        assert_eq!(stats.local_ops, 2);
        assert_eq!(stats.remote_ops, 2);
        assert_eq!(stats.remote_applied, 1);
        assert_eq!(stats.remote_discarded, 1);
    }

    // --- Multiple cells ---

    #[test]
    fn different_cells_no_conflict() {
        let mut state = CollabState::new(site(1));
        state.local_edit(0, 0, CellPayload::Number(1.0));

        // Remote op to a different cell — no conflict
        let remote = CellOp::new(1, 1, CellPayload::Number(2.0), ts(1, 2));
        let result = state.apply_remote(&remote);
        assert!(result.was_applied());
        assert_eq!(state.tracked_cells(), 2);
    }

    #[test]
    fn formula_payload_via_remote() {
        let mut state = CollabState::new(site(1));
        let op = CellOp::new(
            0, 0,
            CellPayload::Formula("=SUM(A2:A10)".into()),
            ts(1, 2),
        );
        let result = state.apply_remote(&op);
        assert!(result.was_applied());
        assert_eq!(
            *state.get_cell_payload(0, 0).unwrap(),
            CellPayload::Formula("=SUM(A2:A10)".into())
        );
    }

    #[test]
    fn clear_payload_via_remote() {
        let mut state = CollabState::new(site(1));
        state.local_edit(0, 0, CellPayload::Number(42.0));

        let clear_op = CellOp::new(0, 0, CellPayload::Clear, ts(10, 2));
        let result = state.apply_remote(&clear_op);
        assert!(result.was_applied());
        assert_eq!(
            *state.get_cell_payload(0, 0).unwrap(),
            CellPayload::Clear
        );
    }
}
