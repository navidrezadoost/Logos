//! `VectorNetwork` — the top-level half-edge graph container.
//!
//! All mutations go through this struct. It is the single owner of all
//! `Anchor` and `Segment` data and is responsible for keeping the incident
//! lists consistent on every operation.
//!
//! # Slot-map layout
//!
//! Anchors and segments are stored in `Vec`s and addressed by index (`usize`).
//! Deleted entries are replaced with `None` (tombstone). Indices are stable
//! across mutations: removing anchor 2 does NOT shift anchor 3 to index 2.
//! This matches the slot-map / generational-arena pattern used by game engines
//! and CAD tools.
//!
//! # Invariants
//!
//! 1. `segments[i].start_anchor` and `segments[i].end_anchor` both exist and
//!    are not tombstoned.
//! 2. For every live segment `s` at index `i`, `anchors[s.start].incident`
//!    and `anchors[s.end].incident` both contain `i`.
//! 3. No self-loops: `start_anchor != end_anchor`.
//! 4. Duplicate directed segments (same start, same end) are rejected.

use crate::anchor::Anchor;
use crate::error::VectorError;
use crate::region::Region;
use crate::segment::Segment;

/// A vector network: a half-edge graph of anchors connected by cubic Bézier
/// segments, with optionally filled closed regions.
#[derive(Debug, Clone, Default)]
pub struct VectorNetwork {
    /// Sparse anchor list. `None` entries are tombstoned (deleted) slots.
    anchors: Vec<Option<Anchor>>,
    /// Sparse segment list. `None` entries are tombstoned (deleted) slots.
    segments: Vec<Option<Segment>>,
    /// Cached closed regions (populated by cycle detection in V2).
    regions: Vec<Region>,
}

impl VectorNetwork {
    // ── Construction ──────────────────────────────────────────────────────────

    /// Create an empty vector network.
    pub fn new() -> Self {
        Self::default()
    }

    // ── Anchor CRUD ───────────────────────────────────────────────────────────

    /// Add a new anchor at `(x, y)` with no handles.
    /// Returns the stable anchor index.
    pub fn add_anchor(&mut self, x: f64, y: f64) -> usize {
        self.push_anchor(Anchor::new(x, y))
    }

    /// Add a new anchor with explicit Bézier handles.
    /// Returns the stable anchor index.
    pub fn add_anchor_with_handles(
        &mut self,
        x: f64,
        y: f64,
        handle_in: Option<(f64, f64)>,
        handle_out: Option<(f64, f64)>,
    ) -> usize {
        self.push_anchor(Anchor::with_handles(x, y, handle_in, handle_out))
    }

    fn push_anchor(&mut self, anchor: Anchor) -> usize {
        // Reuse tombstone slot if available, otherwise extend.
        if let Some(slot) = self.anchors.iter().position(|s| s.is_none()) {
            self.anchors[slot] = Some(anchor);
            slot
        } else {
            let id = self.anchors.len();
            self.anchors.push(Some(anchor));
            id
        }
    }

    /// Move an anchor to a new position.
    ///
    /// Does **not** move the anchor's handles or the segment control points.
    /// If you need to keep handles relative to the anchor, update them
    /// separately via `set_handles`.
    pub fn move_anchor(&mut self, id: usize, x: f64, y: f64) -> Result<(), VectorError> {
        let anchor = self
            .anchors
            .get_mut(id)
            .and_then(|s| s.as_mut())
            .ok_or(VectorError::AnchorNotFound(id))?;
        anchor.x = x;
        anchor.y = y;
        Ok(())
    }

    /// Set (or clear) the Bézier handles of an anchor.
    pub fn set_handles(
        &mut self,
        anchor_id: usize,
        handle_in: Option<(f64, f64)>,
        handle_out: Option<(f64, f64)>,
    ) -> Result<(), VectorError> {
        let anchor = self
            .anchors
            .get_mut(anchor_id)
            .and_then(|s| s.as_mut())
            .ok_or(VectorError::AnchorNotFound(anchor_id))?;
        anchor.handle_in = handle_in;
        anchor.handle_out = handle_out;
        Ok(())
    }

    /// Remove an anchor and **all segments incident on it**.
    ///
    /// Returns the IDs of all removed segments.
    pub fn remove_anchor(&mut self, id: usize) -> Result<Vec<usize>, VectorError> {
        // Validate
        let _ = self
            .anchors
            .get(id)
            .and_then(|s| s.as_ref())
            .ok_or(VectorError::AnchorNotFound(id))?;

        // Collect incident segment IDs before modifying anything.
        let incident: Vec<usize> = self.anchors[id]
            .as_ref()
            .unwrap()
            .incident_segments()
            .to_vec();

        // Remove each incident segment first (also cleans up the other endpoint).
        for &seg_id in &incident {
            self.remove_segment_internal(seg_id);
        }

        // Tombstone the anchor.
        self.anchors[id] = None;

        Ok(incident)
    }

    // ── Segment CRUD ──────────────────────────────────────────────────────────

    /// Add a straight-line segment from `start` to `end`.
    ///
    /// Returns `Err` if either anchor does not exist, if `start == end`
    /// (self-loop), or if a segment with the same direction already exists.
    pub fn add_segment(&mut self, start: usize, end: usize) -> Result<usize, VectorError> {
        self.add_segment_inner(Segment::line(start, end))
    }

    /// Add a cubic Bézier segment.
    ///
    /// Same validation as `add_segment`.
    pub fn add_cubic_segment(
        &mut self,
        start: usize,
        end: usize,
        control1: (f64, f64),
        control2: (f64, f64),
    ) -> Result<usize, VectorError> {
        self.add_segment_inner(Segment::cubic(start, end, control1, control2))
    }

    fn add_segment_inner(&mut self, seg: Segment) -> Result<usize, VectorError> {
        let start = seg.start_anchor;
        let end = seg.end_anchor;

        // Validate anchors
        if self.anchors.get(start).and_then(|s| s.as_ref()).is_none() {
            return Err(VectorError::AnchorNotFound(start));
        }
        if self.anchors.get(end).and_then(|s| s.as_ref()).is_none() {
            return Err(VectorError::AnchorNotFound(end));
        }
        // No self-loops
        if start == end {
            return Err(VectorError::SelfLoop(start));
        }
        // No duplicate directed segments
        if self.segments.iter().flatten().any(|s| {
            s.start_anchor == start && s.end_anchor == end
        }) {
            return Err(VectorError::DuplicateSegment { start, end });
        }

        // Assign slot
        let id = if let Some(slot) = self.segments.iter().position(|s| s.is_none()) {
            self.segments[slot] = Some(seg);
            slot
        } else {
            let id = self.segments.len();
            self.segments.push(Some(seg));
            id
        };

        // Register incidence on both endpoints
        self.anchors[start].as_mut().unwrap().add_incident(id);
        self.anchors[end].as_mut().unwrap().add_incident(id);

        Ok(id)
    }

    /// Remove a segment by ID.
    pub fn remove_segment(&mut self, id: usize) -> Result<(), VectorError> {
        if self.segments.get(id).and_then(|s| s.as_ref()).is_none() {
            return Err(VectorError::SegmentNotFound(id));
        }
        self.remove_segment_internal(id);
        Ok(())
    }

    /// Internal: remove a segment unconditionally — caller must ensure it exists.
    fn remove_segment_internal(&mut self, id: usize) {
        if let Some(seg) = self.segments[id].take() {
            // Clean up incident lists on both endpoints (best-effort: anchors
            // may already be tombstoned if called from remove_anchor).
            if let Some(Some(a)) = self.anchors.get_mut(seg.start_anchor) {
                a.remove_incident(id);
            }
            if let Some(Some(a)) = self.anchors.get_mut(seg.end_anchor) {
                a.remove_incident(id);
            }
        }
    }

    // ── Accessors ─────────────────────────────────────────────────────────────

    /// Get a shared reference to an anchor by index.
    pub fn anchor(&self, id: usize) -> Option<&Anchor> {
        self.anchors.get(id).and_then(|s| s.as_ref())
    }

    /// Get a mutable reference to an anchor by index.
    pub fn anchor_mut(&mut self, id: usize) -> Option<&mut Anchor> {
        self.anchors.get_mut(id).and_then(|s| s.as_mut())
    }

    /// Get a shared reference to a segment by index.
    pub fn segment(&self, id: usize) -> Option<&Segment> {
        self.segments.get(id).and_then(|s| s.as_ref())
    }

    /// Iterate over all live anchors as `(index, &Anchor)`.
    pub fn anchors(&self) -> impl Iterator<Item = (usize, &Anchor)> {
        self.anchors
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.as_ref().map(|a| (i, a)))
    }

    /// Iterate over all live segments as `(index, &Segment)`.
    pub fn segments(&self) -> impl Iterator<Item = (usize, &Segment)> {
        self.segments
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.as_ref().map(|seg| (i, seg)))
    }

    /// All cached regions (populated by V2 cycle detection).
    pub fn regions(&self) -> &[Region] {
        &self.regions
    }

    /// Total number of live anchors.
    pub fn anchor_count(&self) -> usize {
        self.anchors.iter().filter(|s| s.is_some()).count()
    }

    /// Total number of live segments.
    pub fn segment_count(&self) -> usize {
        self.segments.iter().filter(|s| s.is_some()).count()
    }

    /// Returns `true` if the network is empty (no anchors, no segments).
    pub fn is_empty(&self) -> bool {
        self.anchor_count() == 0
    }

    // ── Validation ────────────────────────────────────────────────────────────

    /// Verify all graph invariants. Returns a list of violation descriptions.
    ///
    /// Primarily for testing and debugging. Should return `[]` at all times in
    /// production if the CRUD API is used correctly.
    #[cfg(any(test, debug_assertions))]
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();

        for (seg_id, seg) in self.segments() {
            // Invariant 1: both endpoints exist
            if self.anchor(seg.start_anchor).is_none() {
                errors.push(format!(
                    "segment {seg_id}: start_anchor {} is tombstoned",
                    seg.start_anchor
                ));
            }
            if self.anchor(seg.end_anchor).is_none() {
                errors.push(format!(
                    "segment {seg_id}: end_anchor {} is tombstoned",
                    seg.end_anchor
                ));
            }

            // Invariant 2: incident lists are symmetric
            if let Some(a) = self.anchor(seg.start_anchor) {
                if !a.incident_segments().contains(&seg_id) {
                    errors.push(format!(
                        "segment {seg_id}: not in incident list of start_anchor {}",
                        seg.start_anchor
                    ));
                }
            }
            if let Some(a) = self.anchor(seg.end_anchor) {
                if !a.incident_segments().contains(&seg_id) {
                    errors.push(format!(
                        "segment {seg_id}: not in incident list of end_anchor {}",
                        seg.end_anchor
                    ));
                }
            }

            // Invariant 3: no self-loops
            if seg.start_anchor == seg.end_anchor {
                errors.push(format!("segment {seg_id}: self-loop on anchor {}", seg.start_anchor));
            }
        }

        // Invariant: incident lists only reference live segments
        for (anchor_id, anchor) in self.anchors() {
            for &seg_id in anchor.incident_segments() {
                if self.segment(seg_id).is_none() {
                    errors.push(format!(
                        "anchor {anchor_id}: incident segment {seg_id} is tombstoned"
                    ));
                }
            }
        }

        errors
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Basic construction ────────────────────────────────────────────────────

    #[test]
    fn empty_network() {
        let net = VectorNetwork::new();
        assert!(net.is_empty());
        assert_eq!(net.anchor_count(), 0);
        assert_eq!(net.segment_count(), 0);
    }

    // ── Anchor CRUD ───────────────────────────────────────────────────────────

    #[test]
    fn add_anchors_stable_ids() {
        let mut net = VectorNetwork::new();
        let a = net.add_anchor(0.0, 0.0);
        let b = net.add_anchor(100.0, 0.0);
        let c = net.add_anchor(50.0, 86.6);
        assert_eq!((a, b, c), (0, 1, 2));
        assert_eq!(net.anchor_count(), 3);
    }

    #[test]
    fn move_anchor_updates_position() {
        let mut net = VectorNetwork::new();
        let a = net.add_anchor(0.0, 0.0);
        net.move_anchor(a, 42.0, 7.0).unwrap();
        let anchor = net.anchor(a).unwrap();
        assert_eq!(anchor.x, 42.0);
        assert_eq!(anchor.y, 7.0);
    }

    #[test]
    fn move_nonexistent_anchor_errors() {
        let mut net = VectorNetwork::new();
        assert!(matches!(
            net.move_anchor(99, 0.0, 0.0),
            Err(VectorError::AnchorNotFound(99))
        ));
    }

    #[test]
    fn set_handles() {
        let mut net = VectorNetwork::new();
        let a = net.add_anchor(50.0, 50.0);
        net.set_handles(a, Some((40.0, 50.0)), Some((60.0, 50.0))).unwrap();
        let anchor = net.anchor(a).unwrap();
        assert_eq!(anchor.handle_in, Some((40.0, 50.0)));
        assert_eq!(anchor.handle_out, Some((60.0, 50.0)));
    }

    #[test]
    fn remove_anchor_tombstones_slot() {
        let mut net = VectorNetwork::new();
        let a = net.add_anchor(0.0, 0.0);
        let b = net.add_anchor(100.0, 0.0);
        net.remove_anchor(a).unwrap();
        assert!(net.anchor(a).is_none());
        assert!(net.anchor(b).is_some());
        assert_eq!(net.anchor_count(), 1);
    }

    #[test]
    fn remove_anchor_cascades_to_segments() {
        let mut net = VectorNetwork::new();
        let a = net.add_anchor(0.0, 0.0);
        let b = net.add_anchor(100.0, 0.0);
        let c = net.add_anchor(50.0, 80.0);
        net.add_segment(a, b).unwrap();
        net.add_segment(b, c).unwrap();
        net.add_segment(a, c).unwrap();

        // Removing 'a' should cascade and remove segments a→b and a→c.
        let removed = net.remove_anchor(a).unwrap();
        assert_eq!(removed.len(), 2);
        assert_eq!(net.segment_count(), 1); // only b→c survives

        // b still exists and its incident list is clean.
        let b_anchor = net.anchor(b).unwrap();
        assert_eq!(b_anchor.degree(), 1); // only b→c
        assert!(net.validate().is_empty());
    }

    #[test]
    fn tombstone_slot_reuse() {
        let mut net = VectorNetwork::new();
        let a = net.add_anchor(0.0, 0.0);
        net.add_anchor(1.0, 0.0); // id 1
        net.remove_anchor(a).unwrap();
        // Slot 0 should be reused
        let new_id = net.add_anchor(99.0, 99.0);
        assert_eq!(new_id, 0);
        assert_eq!(net.anchor(0).unwrap().x, 99.0);
    }

    // ── Segment CRUD ──────────────────────────────────────────────────────────

    #[test]
    fn add_segment_updates_incident_lists() {
        let mut net = VectorNetwork::new();
        let a = net.add_anchor(0.0, 0.0);
        let b = net.add_anchor(100.0, 0.0);
        let seg = net.add_segment(a, b).unwrap();

        assert_eq!(net.anchor(a).unwrap().incident_segments(), &[seg]);
        assert_eq!(net.anchor(b).unwrap().incident_segments(), &[seg]);
        assert_eq!(net.segment_count(), 1);
        assert!(net.validate().is_empty());
    }

    #[test]
    fn add_segment_missing_anchor_errors() {
        let mut net = VectorNetwork::new();
        let a = net.add_anchor(0.0, 0.0);
        assert!(matches!(
            net.add_segment(a, 99),
            Err(VectorError::AnchorNotFound(99))
        ));
    }

    #[test]
    fn add_segment_self_loop_errors() {
        let mut net = VectorNetwork::new();
        let a = net.add_anchor(0.0, 0.0);
        assert!(matches!(
            net.add_segment(a, a),
            Err(VectorError::SelfLoop(_))
        ));
    }

    #[test]
    fn add_duplicate_segment_errors() {
        let mut net = VectorNetwork::new();
        let a = net.add_anchor(0.0, 0.0);
        let b = net.add_anchor(100.0, 0.0);
        net.add_segment(a, b).unwrap();
        assert!(matches!(
            net.add_segment(a, b),
            Err(VectorError::DuplicateSegment { .. })
        ));
    }

    #[test]
    fn reverse_direction_is_not_duplicate() {
        // a→b and b→a are distinct directed segments.
        let mut net = VectorNetwork::new();
        let a = net.add_anchor(0.0, 0.0);
        let b = net.add_anchor(100.0, 0.0);
        net.add_segment(a, b).unwrap();
        net.add_segment(b, a).unwrap(); // should succeed
        assert_eq!(net.segment_count(), 2);
        assert!(net.validate().is_empty());
    }

    #[test]
    fn remove_segment_cleans_incident_lists() {
        let mut net = VectorNetwork::new();
        let a = net.add_anchor(0.0, 0.0);
        let b = net.add_anchor(100.0, 0.0);
        let seg = net.add_segment(a, b).unwrap();
        net.remove_segment(seg).unwrap();
        assert_eq!(net.segment_count(), 0);
        assert!(net.anchor(a).unwrap().is_isolated());
        assert!(net.anchor(b).unwrap().is_isolated());
        assert!(net.validate().is_empty());
    }

    #[test]
    fn remove_nonexistent_segment_errors() {
        let mut net = VectorNetwork::new();
        assert!(matches!(
            net.remove_segment(0),
            Err(VectorError::SegmentNotFound(0))
        ));
    }

    // ── Multi-anchor topology ─────────────────────────────────────────────────

    #[test]
    fn triangle_topology() {
        let mut net = VectorNetwork::new();
        let a = net.add_anchor(0.0, 0.0);
        let b = net.add_anchor(100.0, 0.0);
        let c = net.add_anchor(50.0, 80.0);
        net.add_segment(a, b).unwrap();
        net.add_segment(b, c).unwrap();
        net.add_segment(c, a).unwrap();

        assert_eq!(net.anchor_count(), 3);
        assert_eq!(net.segment_count(), 3);
        // Each anchor has degree 2 (one incoming, one outgoing segment)
        assert_eq!(net.anchor(a).unwrap().degree(), 2);
        assert_eq!(net.anchor(b).unwrap().degree(), 2);
        assert_eq!(net.anchor(c).unwrap().degree(), 2);
        assert!(net.validate().is_empty());
    }

    #[test]
    fn star_topology_high_degree() {
        // One hub connected to 5 spokes — hub has degree 5.
        let mut net = VectorNetwork::new();
        let hub = net.add_anchor(50.0, 50.0);
        let spokes: Vec<usize> = (0..5)
            .map(|i| {
                let angle = i as f64 * std::f64::consts::TAU / 5.0;
                net.add_anchor(50.0 + 40.0 * angle.cos(), 50.0 + 40.0 * angle.sin())
            })
            .collect();
        for &spoke in &spokes {
            net.add_segment(hub, spoke).unwrap();
        }
        assert_eq!(net.anchor(hub).unwrap().degree(), 5);
        assert_eq!(net.segment_count(), 5);
        assert!(net.validate().is_empty());
    }

    #[test]
    fn cubic_segment() {
        let mut net = VectorNetwork::new();
        let a = net.add_anchor(0.0, 0.0);
        let b = net.add_anchor(100.0, 0.0);
        let seg = net.add_cubic_segment(a, b, (20.0, -30.0), (80.0, -30.0)).unwrap();
        let s = net.segment(seg).unwrap();
        assert!(s.is_cubic());
        assert_eq!(s.control1, Some((20.0, -30.0)));
        assert!(net.validate().is_empty());
    }

    // ── Iterator ─────────────────────────────────────────────────────────────

    #[test]
    fn iterators_skip_tombstones() {
        let mut net = VectorNetwork::new();
        let a = net.add_anchor(0.0, 0.0);
        let b = net.add_anchor(1.0, 0.0);
        let c = net.add_anchor(2.0, 0.0);
        net.add_segment(a, b).unwrap();
        net.add_segment(b, c).unwrap();
        let seg_ac = net.add_segment(a, c).unwrap();

        net.remove_anchor(a).unwrap(); // also removes a→b and a→c

        let anchor_ids: Vec<usize> = net.anchors().map(|(i, _)| i).collect();
        assert!(!anchor_ids.contains(&a));
        assert!(anchor_ids.contains(&b));
        assert!(anchor_ids.contains(&c));

        let seg_ids: Vec<usize> = net.segments().map(|(i, _)| i).collect();
        assert!(!seg_ids.contains(&seg_ac));
        assert_eq!(seg_ids.len(), 1); // only b→c

        assert!(net.validate().is_empty());
    }
}
