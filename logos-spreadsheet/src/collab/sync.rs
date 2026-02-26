//! Synchronisation engine — ties CRDT state to the `RecalcEngine`.
//!
//! [`CollabEngine`] is the top-level entry point for collaborative editing.
//! It wraps a [`CollabState`] + [`PresenceTracker`] and provides methods
//! that accept/produce domain objects (not raw bytes). A higher-level
//! transport layer (e.g., `logos-collab`) handles serialization and
//! WebSocket I/O.
//!
//! # Data flow
//!
//! ```text
//! Local user types "=SUM(A1:A3)" into cell B4
//!     │
//!     ▼
//! CollabEngine::local_set_formula(1, 3, "=SUM(A1:A3)")
//!     │  1. Records op in CollabState (LWW register)
//!     │  2. Returns CellOp for broadcast
//!     ▼
//! Broadcast layer sends CellOp to all peers
//!     │
//!     ▼                (on remote peer)
//! CollabEngine::apply_remote_op(op)
//!     │  1. CollabState resolves LWW
//!     │  2. If applied → returns ApplyResult::Applied
//!     │  3. Caller writes to RecalcEngine → e.g. set_formula + recalc
//!     ▼
//! Remote peer sees the same formula
//! ```
//!
//! ## Why `CollabEngine` doesn't own `RecalcEngine`
//!
//! To keep ownership clean, `CollabEngine` does *not* hold a mutable
//! reference to `RecalcEngine`. Instead it returns [`ApplyResult`] values
//! that the caller applies to the engine. This avoids borrow-checker
//! tangles and makes the collaboration layer testable in isolation.

use super::ops::{CellOp, CellPayload, LamportClock, OpBatch, SiteId};
use super::presence::{PeerCursorRenderData, PeerPresence, PresenceTracker};
use super::state::{ApplyResult, CollabState, CollabStats};

// ---------------------------------------------------------------------------
// Session info
// ---------------------------------------------------------------------------

/// Metadata about a collaborative session.
#[derive(Debug, Clone)]
pub struct SessionInfo {
    /// A human-readable session name (e.g., "Budget Q3 2026").
    pub name: String,
    /// Number of peers currently connected.
    pub peer_count: usize,
    /// Whether this peer is the session host (first to join).
    pub is_host: bool,
}

// ---------------------------------------------------------------------------
// Collab engine
// ---------------------------------------------------------------------------

/// Top-level collaborative editing engine.
///
/// Combines CRDT state and presence tracking into a single interface.
/// Does **not** own the `RecalcEngine` — instead returns [`ApplyResult`]
/// values that the caller applies.
#[derive(Debug, Clone)]
pub struct CollabEngine {
    /// CRDT cell state.
    state: CollabState,
    /// Presence tracker.
    presence: PresenceTracker,
    /// Session metadata.
    session_name: String,
    /// Whether the session is active (connected).
    active: bool,
    /// Operation log for undo support: (op, inverse_payload).
    undo_stack: Vec<UndoEntry>,
}

/// An entry in the undo stack.
#[derive(Debug, Clone)]
struct UndoEntry {
    /// The operation that was performed.
    op: CellOp,
    /// The previous payload (to restore on undo). `None` if cell was empty.
    previous: Option<CellPayload>,
}

impl CollabEngine {
    /// Create a new collaborative engine for the given site.
    pub fn new(site_id: SiteId, user_name: impl Into<String>) -> Self {
        let name = user_name.into();
        Self {
            state: CollabState::new(site_id),
            presence: PresenceTracker::new(site_id, &name),
            session_name: String::new(),
            active: false,
            undo_stack: Vec::new(),
        }
    }

    /// Start a collaborative session.
    pub fn start_session(&mut self, name: impl Into<String>) {
        self.session_name = name.into();
        self.active = true;
    }

    /// End the collaborative session.
    pub fn end_session(&mut self) {
        self.active = false;
    }

    /// Whether the session is active.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Get session info.
    pub fn session_info(&self) -> SessionInfo {
        SessionInfo {
            name: self.session_name.clone(),
            peer_count: self.presence.remote_count() + 1,
            is_host: true, // simplified — real impl uses join order
        }
    }

    /// Access the underlying CRDT state.
    pub fn state(&self) -> &CollabState {
        &self.state
    }

    /// Access the presence tracker.
    pub fn presence(&self) -> &PresenceTracker {
        &self.presence
    }

    /// Mutable access to the presence tracker.
    pub fn presence_mut(&mut self) -> &mut PresenceTracker {
        &mut self.presence
    }

    /// Session statistics.
    pub fn stats(&self) -> &CollabStats {
        self.state.stats()
    }

    /// The local site ID.
    pub fn site_id(&self) -> SiteId {
        self.state.site_id()
    }

    /// The current Lamport clock.
    pub fn clock(&self) -> LamportClock {
        self.state.clock()
    }

    // -----------------------------------------------------------------------
    // Local operations
    // -----------------------------------------------------------------------

    /// Record a local value edit (number, text, boolean).
    ///
    /// Returns the `CellOp` to broadcast to peers.
    pub fn local_set_value(&mut self, col: u32, row: u32, payload: CellPayload) -> CellOp {
        let previous = self.state.get_cell_payload(col, row).cloned();
        let op = self.state.local_edit(col, row, payload);
        self.undo_stack.push(UndoEntry {
            op: op.clone(),
            previous,
        });
        self.presence.local_mut().set_cursor(col, row);
        op
    }

    /// Record a local formula edit.
    ///
    /// Convenience wrapper — wraps the formula in `CellPayload::Formula`.
    pub fn local_set_formula(&mut self, col: u32, row: u32, formula: &str) -> CellOp {
        self.local_set_value(col, row, CellPayload::Formula(formula.to_string()))
    }

    /// Record a local cell clear.
    pub fn local_clear(&mut self, col: u32, row: u32) -> CellOp {
        self.local_set_value(col, row, CellPayload::Clear)
    }

    // -----------------------------------------------------------------------
    // Remote operations
    // -----------------------------------------------------------------------

    /// Apply a single remote operation.
    ///
    /// Returns [`ApplyResult`] — the caller should check for `Applied` and
    /// write the payload to the `RecalcEngine` accordingly.
    pub fn apply_remote_op(&mut self, op: &CellOp) -> ApplyResult {
        self.state.apply_remote(op)
    }

    /// Apply a batch of remote operations.
    pub fn apply_remote_batch(&mut self, ops: &[CellOp]) -> Vec<ApplyResult> {
        self.state.apply_remote_batch(ops)
    }

    /// Apply a remote op batch from an `OpBatch`.
    pub fn apply_op_batch(&mut self, batch: &OpBatch) -> Vec<ApplyResult> {
        self.state.apply_remote_batch(&batch.ops)
    }

    // -----------------------------------------------------------------------
    // Presence
    // -----------------------------------------------------------------------

    /// Update the local peer's cursor position.
    pub fn set_cursor(&mut self, col: u32, row: u32) {
        self.presence.local_mut().set_cursor(col, row);
    }

    /// Update the local peer's selection.
    pub fn set_selection(
        &mut self,
        start_col: u32,
        start_row: u32,
        end_col: u32,
        end_row: u32,
    ) {
        self.presence
            .local_mut()
            .set_selection(start_col, start_row, end_col, end_row);
    }

    /// Set whether the local peer is editing.
    pub fn set_editing(&mut self, editing: bool) {
        self.presence.local_mut().set_editing(editing);
    }

    /// Handle a remote peer joining.
    pub fn peer_joined(&mut self, presence: PeerPresence) {
        self.presence.update_remote(presence);
    }

    /// Handle a remote peer leaving.
    pub fn peer_left(&mut self, site_id: SiteId) {
        self.presence.remove_remote(site_id);
    }

    /// Update a remote peer's presence.
    pub fn update_remote_presence(&mut self, presence: PeerPresence) {
        self.presence.update_remote(presence);
    }

    /// Get render data for all remote cursors.
    pub fn remote_cursors(&self) -> Vec<PeerCursorRenderData> {
        self.presence.remote_cursors()
    }

    /// Get the local peer's presence for broadcast.
    pub fn local_presence(&self) -> &PeerPresence {
        self.presence.local()
    }

    // -----------------------------------------------------------------------
    // Delta sync
    // -----------------------------------------------------------------------

    /// Get all ops since a given clock (for delta synchronisation).
    pub fn ops_since(&self, since_clock: LamportClock) -> Vec<CellOp> {
        self.state.ops_since(since_clock)
    }

    /// Build an `OpBatch` of all local ops since a clock value.
    pub fn build_delta(&self, since_clock: LamportClock) -> OpBatch {
        let ops = self.state.ops_since(since_clock);
        OpBatch::new(self.state.site_id(), ops, self.state.clock())
    }

    // -----------------------------------------------------------------------
    // Undo
    // -----------------------------------------------------------------------

    /// Undo the last local operation.
    ///
    /// Returns the inverse `CellOp` to broadcast, or `None` if nothing to undo.
    pub fn undo(&mut self) -> Option<CellOp> {
        let entry = self.undo_stack.pop()?;
        let payload = entry.previous.unwrap_or(CellPayload::Clear);
        let op = self.state.local_edit(entry.op.col, entry.op.row, payload);
        Some(op)
    }

    /// Whether undo is available.
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::ops::OpTimestamp;

    fn site(id: u64) -> SiteId {
        SiteId::new(id)
    }

    fn engine(id: u64, name: &str) -> CollabEngine {
        CollabEngine::new(site(id), name)
    }

    // --- Session lifecycle ---

    #[test]
    fn session_lifecycle() {
        let mut e = engine(1, "Alice");
        assert!(!e.is_active());
        e.start_session("Test Sheet");
        assert!(e.is_active());
        assert_eq!(e.session_info().name, "Test Sheet");
        e.end_session();
        assert!(!e.is_active());
    }

    // --- Local operations ---

    #[test]
    fn local_set_value() {
        let mut e = engine(1, "Alice");
        let op = e.local_set_value(0, 0, CellPayload::Number(42.0));
        assert_eq!(op.col, 0);
        assert_eq!(op.row, 0);
        assert_eq!(op.payload, CellPayload::Number(42.0));
        assert_eq!(op.timestamp.site_id, site(1));
    }

    #[test]
    fn local_set_formula() {
        let mut e = engine(1, "Alice");
        let op = e.local_set_formula(1, 2, "=A1+B1");
        assert_eq!(op.payload, CellPayload::Formula("=A1+B1".into()));
    }

    #[test]
    fn local_clear() {
        let mut e = engine(1, "Alice");
        e.local_set_value(0, 0, CellPayload::Number(1.0));
        let op = e.local_clear(0, 0);
        assert_eq!(op.payload, CellPayload::Clear);
    }

    #[test]
    fn local_ops_increment_clock() {
        let mut e = engine(1, "Alice");
        let op1 = e.local_set_value(0, 0, CellPayload::Number(1.0));
        let op2 = e.local_set_value(0, 1, CellPayload::Number(2.0));
        assert!(op2.timestamp.clock > op1.timestamp.clock);
    }

    // --- Remote operations ---

    #[test]
    fn remote_op_applied() {
        let mut e = engine(1, "Alice");
        let remote_op = CellOp::new(
            0, 0,
            CellPayload::Number(99.0),
            OpTimestamp::new(LamportClock::new(5), site(2)),
        );
        let result = e.apply_remote_op(&remote_op);
        assert!(result.was_applied());
    }

    #[test]
    fn remote_op_discarded_when_stale() {
        let mut e = engine(1, "Alice");
        e.local_set_value(0, 0, CellPayload::Number(1.0)); // clock=1

        let stale = CellOp::new(
            0, 0,
            CellPayload::Number(0.5),
            OpTimestamp::new(LamportClock::new(0), site(2)),
        );
        let result = e.apply_remote_op(&stale);
        assert_eq!(result, ApplyResult::Discarded);
    }

    // --- Two-peer simulation ---

    #[test]
    fn two_peers_converge_no_conflict() {
        let mut alice = engine(1, "Alice");
        let mut bob = engine(2, "Bob");

        // Alice edits A1
        let op_a = alice.local_set_value(0, 0, CellPayload::Number(10.0));
        // Bob edits B1
        let op_b = bob.local_set_value(1, 0, CellPayload::Number(20.0));

        // Exchange ops
        bob.apply_remote_op(&op_a);
        alice.apply_remote_op(&op_b);

        // Both should see both values
        assert_eq!(
            *alice.state().get_cell_payload(0, 0).unwrap(),
            CellPayload::Number(10.0)
        );
        assert_eq!(
            *alice.state().get_cell_payload(1, 0).unwrap(),
            CellPayload::Number(20.0)
        );
        assert_eq!(
            *bob.state().get_cell_payload(0, 0).unwrap(),
            CellPayload::Number(10.0)
        );
        assert_eq!(
            *bob.state().get_cell_payload(1, 0).unwrap(),
            CellPayload::Number(20.0)
        );
    }

    #[test]
    fn two_peers_concurrent_same_cell_converge() {
        let mut alice = engine(1, "Alice");
        let mut bob = engine(2, "Bob");

        // Both edit the same cell concurrently
        let op_a = alice.local_set_value(0, 0, CellPayload::Text("Alice's value".into()));
        let op_b = bob.local_set_value(0, 0, CellPayload::Text("Bob's value".into()));

        // Exchange ops (in any order — LWW is commutative)
        alice.apply_remote_op(&op_b);
        bob.apply_remote_op(&op_a);

        // Both should converge to the same value
        // Bob wins: same clock (1), but site_id(2) > site_id(1)
        let alice_val = alice.state().get_cell_payload(0, 0).unwrap().clone();
        let bob_val = bob.state().get_cell_payload(0, 0).unwrap().clone();
        assert_eq!(alice_val, bob_val);
        assert_eq!(alice_val, CellPayload::Text("Bob's value".into()));
    }

    #[test]
    fn three_peers_all_converge() {
        let mut alice = engine(1, "Alice");
        let mut bob = engine(2, "Bob");
        let mut carol = engine(3, "Carol");

        // Each edits a different cell
        let op_a = alice.local_set_value(0, 0, CellPayload::Number(1.0));
        let op_b = bob.local_set_value(1, 0, CellPayload::Number(2.0));
        let op_c = carol.local_set_value(2, 0, CellPayload::Number(3.0));

        // Full exchange
        alice.apply_remote_op(&op_b);
        alice.apply_remote_op(&op_c);
        bob.apply_remote_op(&op_a);
        bob.apply_remote_op(&op_c);
        carol.apply_remote_op(&op_a);
        carol.apply_remote_op(&op_b);

        // All three should have the same state
        for col in 0..3 {
            let a = alice.state().get_cell_payload(col, 0).unwrap().clone();
            let b = bob.state().get_cell_payload(col, 0).unwrap().clone();
            let c = carol.state().get_cell_payload(col, 0).unwrap().clone();
            assert_eq!(a, b);
            assert_eq!(b, c);
        }
    }

    // --- Presence ---

    #[test]
    fn cursor_updates_on_local_edit() {
        let mut e = engine(1, "Alice");
        e.local_set_value(3, 7, CellPayload::Number(1.0));
        assert_eq!(e.local_presence().cursor, (3, 7));
    }

    #[test]
    fn remote_peer_join_leave() {
        let mut e = engine(1, "Alice");
        let bob = PeerPresence::new(site(2), "Bob");
        e.peer_joined(bob);
        assert_eq!(e.presence().remote_count(), 1);

        e.peer_left(site(2));
        assert_eq!(e.presence().remote_count(), 0);
    }

    #[test]
    fn remote_cursor_render_data() {
        let mut e = engine(1, "Alice");
        let mut bob = PeerPresence::new(site(2), "Bob");
        bob.set_cursor(5, 5);
        e.peer_joined(bob);

        let cursors = e.remote_cursors();
        assert_eq!(cursors.len(), 1);
        assert_eq!(cursors[0].cursor, (5, 5));
        assert_eq!(cursors[0].name, "Bob");
    }

    // --- Delta sync ---

    #[test]
    fn build_delta() {
        let mut e = engine(1, "Alice");
        e.local_set_value(0, 0, CellPayload::Number(1.0)); // clock 1
        e.local_set_value(1, 0, CellPayload::Number(2.0)); // clock 2
        e.local_set_value(2, 0, CellPayload::Number(3.0)); // clock 3

        let delta = e.build_delta(LamportClock::new(1));
        assert_eq!(delta.len(), 2); // ops at clock 2 and 3
        assert_eq!(delta.site_id, site(1));
    }

    #[test]
    fn apply_op_batch() {
        let mut alice = engine(1, "Alice");
        let mut bob = engine(2, "Bob");

        // Bob makes several edits
        bob.local_set_value(0, 0, CellPayload::Number(1.0));
        bob.local_set_value(1, 0, CellPayload::Number(2.0));
        bob.local_set_value(2, 0, CellPayload::Number(3.0));

        // Build delta from Bob and apply to Alice
        let batch = bob.build_delta(LamportClock::new(0));
        let results = alice.apply_op_batch(&batch);
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|r| r.was_applied()));
    }

    // --- Undo ---

    #[test]
    fn undo_restores_previous_value() {
        let mut e = engine(1, "Alice");
        e.local_set_value(0, 0, CellPayload::Number(1.0));
        e.local_set_value(0, 0, CellPayload::Number(2.0));

        assert!(e.can_undo());
        let undo_op = e.undo().unwrap();
        assert_eq!(undo_op.payload, CellPayload::Number(1.0));
    }

    #[test]
    fn undo_clear_when_no_previous() {
        let mut e = engine(1, "Alice");
        e.local_set_value(0, 0, CellPayload::Number(42.0));

        let undo_op = e.undo().unwrap();
        assert_eq!(undo_op.payload, CellPayload::Clear);
    }

    #[test]
    fn undo_empty_stack() {
        let mut e = engine(1, "Alice");
        assert!(!e.can_undo());
        assert!(e.undo().is_none());
    }

    // --- Stats ---

    #[test]
    fn stats_from_engine() {
        let mut e = engine(1, "Alice");
        e.local_set_value(0, 0, CellPayload::Number(1.0));
        e.local_set_value(1, 0, CellPayload::Number(2.0));
        assert_eq!(e.stats().local_ops, 2);
    }

    // --- Session info ---

    #[test]
    fn session_info_peer_count() {
        let mut e = engine(1, "Alice");
        e.start_session("Test");

        let bob = PeerPresence::new(site(2), "Bob");
        e.peer_joined(bob);

        let info = e.session_info();
        assert_eq!(info.peer_count, 2); // Alice + Bob
        assert_eq!(info.name, "Test");
    }
}
