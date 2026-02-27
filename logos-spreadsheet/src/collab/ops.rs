//! Cell operations — the unit of change in collaborative spreadsheet editing.
//!
//! Every mutation (set value, set formula, clear) is expressed as a [`CellOp`].
//! Ops carry a logical timestamp ([`LamportClock`]) and a site/peer ID so that
//! concurrent edits can be resolved deterministically using **Last-Writer-Wins
//! (LWW)** semantics: the op with the highest (clock, site_id) pair wins.
//!
//! # CRDT Semantics
//!
//! The spreadsheet uses a **LWW-Element-Register** per cell:
//!
//! - Each cell is an independent CRDT register.
//! - Concurrent edits to the *same* cell are resolved by comparing
//!   `(lamport_clock, site_id)` — higher wins, with `site_id` as tiebreaker.
//! - Concurrent edits to *different* cells are commutative (no conflict).
//!
//! This is simple, efficient, and matches user expectations: if two people
//! edit the same cell at the same time, the "later" edit wins.
//!
//! # Wire-friendliness
//!
//! `CellOp` is designed to be serialized efficiently. It uses only primitive
//! types (`u64`, `u32`, `String`) — no Rc, Arc, or Box. A higher-level
//! transport layer (e.g., `logos-collab`) can encode ops with bincode/serde.

use std::fmt;

// ---------------------------------------------------------------------------
// Site ID
// ---------------------------------------------------------------------------

/// A site (peer/user) identifier.
///
/// Each collaborator gets a unique `SiteId` when they join a session.
/// IDs are compared as a tiebreaker when two ops have the same Lamport clock.
/// Higher ID wins (arbitrary but deterministic).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SiteId(pub u64);

impl SiteId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }
}

impl fmt::Display for SiteId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "site-{}", self.0)
    }
}

// ---------------------------------------------------------------------------
// Lamport clock
// ---------------------------------------------------------------------------

/// A Lamport logical clock for causal ordering.
///
/// Guarantees:
/// - Local events are totally ordered (monotonically increasing).
/// - If op A causally precedes op B, then A.clock < B.clock.
/// - Concurrent ops may have any clock relationship, but `(clock, site_id)`
///   provides a total order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LamportClock(pub u64);

impl LamportClock {
    pub fn new(value: u64) -> Self {
        Self(value)
    }

    /// Increment the clock for a local event.
    pub fn tick(&mut self) -> LamportClock {
        self.0 += 1;
        *self
    }

    /// Merge with a remote clock: `max(local, remote) + 1`.
    pub fn merge(&mut self, remote: LamportClock) -> LamportClock {
        self.0 = self.0.max(remote.0) + 1;
        *self
    }

    /// The clock value.
    pub fn value(&self) -> u64 {
        self.0
    }
}

impl fmt::Display for LamportClock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "t={}", self.0)
    }
}

// ---------------------------------------------------------------------------
// Timestamp (combined ordering key)
// ---------------------------------------------------------------------------

/// Combined `(clock, site_id)` ordering key for LWW resolution.
///
/// Ops are ordered by `clock` first, then `site_id` as tiebreaker.
/// This gives a **total order** over all ops from all sites.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OpTimestamp {
    pub clock: LamportClock,
    pub site_id: SiteId,
}

impl OpTimestamp {
    pub fn new(clock: LamportClock, site_id: SiteId) -> Self {
        Self { clock, site_id }
    }

    /// Returns true if `self` is strictly "later" than `other` in LWW order.
    pub fn is_later_than(&self, other: &OpTimestamp) -> bool {
        (self.clock, self.site_id) > (other.clock, other.site_id)
    }
}

impl Ord for OpTimestamp {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.clock
            .cmp(&other.clock)
            .then(self.site_id.cmp(&other.site_id))
    }
}

impl PartialOrd for OpTimestamp {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for OpTimestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({}, {})", self.clock, self.site_id)
    }
}

// ---------------------------------------------------------------------------
// Cell value payload
// ---------------------------------------------------------------------------

/// The payload of a cell operation — what was written to the cell.
///
/// This is a simplified/serializable representation of `Value`.
/// The full `Value` enum (with arrays, design refs, etc.) is reconstructed
/// by the engine when the op is applied.
#[derive(Debug, Clone, PartialEq)]
pub enum CellPayload {
    /// A plain number.
    Number(f64),
    /// A text string.
    Text(String),
    /// A boolean value.
    Boolean(bool),
    /// A formula string (starts with `=`).
    Formula(String),
    /// Cell cleared (empty).
    Clear,
}

impl CellPayload {
    /// Whether this payload is a formula.
    pub fn is_formula(&self) -> bool {
        matches!(self, CellPayload::Formula(_))
    }

    /// Whether this payload clears the cell.
    pub fn is_clear(&self) -> bool {
        matches!(self, CellPayload::Clear)
    }
}

impl fmt::Display for CellPayload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CellPayload::Number(n) => write!(f, "{}", n),
            CellPayload::Text(s) => write!(f, "\"{}\"", s),
            CellPayload::Boolean(b) => write!(f, "{}", if *b { "TRUE" } else { "FALSE" }),
            CellPayload::Formula(s) => write!(f, "{}", s),
            CellPayload::Clear => write!(f, "<clear>"),
        }
    }
}

// ---------------------------------------------------------------------------
// Cell operation
// ---------------------------------------------------------------------------

/// A single cell operation: write a value/formula or clear a cell.
///
/// This is the fundamental unit of change that is broadcast between peers.
/// Each op is self-contained — you can apply it independently without
/// needing prior context.
#[derive(Debug, Clone, PartialEq)]
pub struct CellOp {
    /// Which cell this op targets.
    pub col: u32,
    pub row: u32,
    /// The value being written.
    pub payload: CellPayload,
    /// Logical timestamp for ordering.
    pub timestamp: OpTimestamp,
}

impl CellOp {
    /// Create a new cell operation.
    pub fn new(col: u32, row: u32, payload: CellPayload, timestamp: OpTimestamp) -> Self {
        Self {
            col,
            row,
            payload,
            timestamp,
        }
    }

    /// The cell coordinate as `(col, row)`.
    pub fn coord(&self) -> (u32, u32) {
        (self.col, self.row)
    }

    /// Whether this op is "later" than another op (for LWW resolution).
    pub fn wins_over(&self, other: &CellOp) -> bool {
        self.timestamp.is_later_than(&other.timestamp)
    }
}

impl fmt::Display for CellOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "CellOp({},{}) = {} @ {}",
            self.col, self.row, self.payload, self.timestamp
        )
    }
}

// ---------------------------------------------------------------------------
// Op batch (for delta sync)
// ---------------------------------------------------------------------------

/// A batch of cell operations from a single site.
///
/// Used for delta synchronisation: a peer sends all ops since the last
/// known clock value.
#[derive(Debug, Clone)]
pub struct OpBatch {
    /// The site that produced these ops.
    pub site_id: SiteId,
    /// The ops in causal order (ascending clock).
    pub ops: Vec<CellOp>,
    /// The clock value *after* all ops in this batch.
    pub end_clock: LamportClock,
}

impl OpBatch {
    pub fn new(site_id: SiteId, ops: Vec<CellOp>, end_clock: LamportClock) -> Self {
        Self {
            site_id,
            ops,
            end_clock,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    pub fn len(&self) -> usize {
        self.ops.len()
    }
}

// ---------------------------------------------------------------------------
// Structural operation (collab-level wrapper)
// ---------------------------------------------------------------------------

/// A structural change broadcast between peers.
///
/// Unlike [`CellOp`] which targets a single cell, this changes the
/// spreadsheet topology. It carries a Lamport timestamp so peers can
/// order structural ops relative to cell ops.
#[derive(Debug, Clone, PartialEq)]
pub struct StructuralCollapOp {
    /// The structural operation.
    pub op: crate::structural::StructuralOp,
    /// Logical timestamp for ordering.
    pub timestamp: OpTimestamp,
    /// Originating site.
    pub site_id: SiteId,
}

impl StructuralCollapOp {
    pub fn new(
        op: crate::structural::StructuralOp,
        timestamp: OpTimestamp,
        site_id: SiteId,
    ) -> Self {
        Self { op, timestamp, site_id }
    }
}

impl fmt::Display for StructuralCollapOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "StructuralOp({:?}) @ {}", self.op, self.timestamp)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lamport_clock_tick() {
        let mut clock = LamportClock::new(0);
        assert_eq!(clock.tick(), LamportClock::new(1));
        assert_eq!(clock.tick(), LamportClock::new(2));
        assert_eq!(clock.value(), 2);
    }

    #[test]
    fn lamport_clock_merge() {
        let mut local = LamportClock::new(3);
        let remote = LamportClock::new(7);
        let merged = local.merge(remote);
        assert_eq!(merged, LamportClock::new(8)); // max(3,7)+1 = 8
    }

    #[test]
    fn lamport_clock_merge_local_higher() {
        let mut local = LamportClock::new(10);
        let remote = LamportClock::new(5);
        let merged = local.merge(remote);
        assert_eq!(merged, LamportClock::new(11)); // max(10,5)+1 = 11
    }

    #[test]
    fn op_timestamp_ordering() {
        let t1 = OpTimestamp::new(LamportClock::new(1), SiteId::new(1));
        let t2 = OpTimestamp::new(LamportClock::new(2), SiteId::new(1));
        assert!(t2.is_later_than(&t1));
        assert!(!t1.is_later_than(&t2));
    }

    #[test]
    fn op_timestamp_tiebreak_by_site_id() {
        let t1 = OpTimestamp::new(LamportClock::new(5), SiteId::new(1));
        let t2 = OpTimestamp::new(LamportClock::new(5), SiteId::new(2));
        assert!(t2.is_later_than(&t1)); // same clock, higher site_id wins
        assert!(!t1.is_later_than(&t2));
    }

    #[test]
    fn op_timestamp_equal() {
        let t = OpTimestamp::new(LamportClock::new(3), SiteId::new(1));
        assert!(!t.is_later_than(&t)); // not strictly later
    }

    #[test]
    fn cell_op_wins_over() {
        let op1 = CellOp::new(
            0, 0,
            CellPayload::Number(1.0),
            OpTimestamp::new(LamportClock::new(1), SiteId::new(1)),
        );
        let op2 = CellOp::new(
            0, 0,
            CellPayload::Number(2.0),
            OpTimestamp::new(LamportClock::new(2), SiteId::new(1)),
        );
        assert!(op2.wins_over(&op1));
        assert!(!op1.wins_over(&op2));
    }

    #[test]
    fn cell_payload_display() {
        assert_eq!(format!("{}", CellPayload::Number(42.0)), "42");
        assert_eq!(format!("{}", CellPayload::Text("hi".into())), "\"hi\"");
        assert_eq!(format!("{}", CellPayload::Boolean(true)), "TRUE");
        assert_eq!(format!("{}", CellPayload::Formula("=A1+B1".into())), "=A1+B1");
        assert_eq!(format!("{}", CellPayload::Clear), "<clear>");
    }

    #[test]
    fn cell_payload_predicates() {
        assert!(CellPayload::Formula("=1".into()).is_formula());
        assert!(!CellPayload::Number(1.0).is_formula());
        assert!(CellPayload::Clear.is_clear());
        assert!(!CellPayload::Number(1.0).is_clear());
    }

    #[test]
    fn cell_op_coord() {
        let op = CellOp::new(
            3, 7,
            CellPayload::Text("x".into()),
            OpTimestamp::new(LamportClock::new(1), SiteId::new(1)),
        );
        assert_eq!(op.coord(), (3, 7));
    }

    #[test]
    fn cell_op_display() {
        let op = CellOp::new(
            0, 0,
            CellPayload::Number(42.0),
            OpTimestamp::new(LamportClock::new(1), SiteId::new(2)),
        );
        let s = format!("{}", op);
        assert!(s.contains("42"));
        assert!(s.contains("t=1"));
    }

    #[test]
    fn op_batch_basics() {
        let batch = OpBatch::new(SiteId::new(1), vec![], LamportClock::new(0));
        assert!(batch.is_empty());
        assert_eq!(batch.len(), 0);

        let op = CellOp::new(
            0, 0,
            CellPayload::Number(1.0),
            OpTimestamp::new(LamportClock::new(1), SiteId::new(1)),
        );
        let batch2 = OpBatch::new(SiteId::new(1), vec![op], LamportClock::new(1));
        assert!(!batch2.is_empty());
        assert_eq!(batch2.len(), 1);
    }

    #[test]
    fn site_id_display() {
        assert_eq!(format!("{}", SiteId::new(42)), "site-42");
    }

    #[test]
    fn op_timestamp_display() {
        let ts = OpTimestamp::new(LamportClock::new(5), SiteId::new(3));
        let s = format!("{}", ts);
        assert!(s.contains("t=5"));
        assert!(s.contains("site-3"));
    }
}
