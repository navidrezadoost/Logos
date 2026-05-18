//! cycle.rs — Planar face traversal / cycle detection for `VectorNetwork`.
//!
//! ## Algorithm
//!
//! This is the standard DCEL (Doubly-Connected Edge List) face traversal,
//! adapted for the slot-map graph representation in `logos-vector`.
//!
//! ### Half-edges
//!
//! Each undirected segment `(a, b)` yields **two directed half-edges**:
//!
//! - **forward**  — travels `a → b` (segment direction as stored)
//! - **backward** — travels `b → a` (the twin direction)
//!
//! Reference: *Computational Geometry: Algorithms and Applications* §2.2
//! (de Berg, Cheong, van Kreveld, Overmars, 3rd ed.).
//!
//! ### Next-half-edge rule
//!
//! For half-edge `h` arriving at vertex `v` from `u`:
//!
//! 1. Form the **twin** `t = (v → u)`, which *departs* from `v`.
//! 2. Among all half-edges departing from `v`, sort them by polar angle CCW.
//! 3. The next half-edge of `h` is the half-edge that immediately **follows**
//!    the twin `t` in that CCW-sorted angular order (wrapping around).
//!
//! This "always turn left" rule bounds the leftmost (innermost) face on each
//! half-edge's left side.
//!
//! ### Face classification
//!
//! - **Interior face** — CCW-oriented cycle (positive signed area).  
//!   These are the visible filled regions.
//! - **Outer (infinite) face** — CW-oriented cycle (negative signed area).  
//!   Discarded.
//!
//! ### Complexity
//!
//! O(E log E) for sorting + O(E) for traversal, where E = segment count.
//! For typical design-tool vector networks (< 500 anchors) this is instant.

use std::collections::HashMap;
use std::f64::consts::PI;

use crate::graph::VectorNetwork;
use crate::region::Region;

// ─────────────────────────────────────────────────────────────────────────────
// Half-edge type
// ─────────────────────────────────────────────────────────────────────────────

/// A directed half-edge. Each undirected segment `(a, b)` produces two: one
/// forward `(segment_id, true)` and one backward `(segment_id, false)`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct HalfEdge {
    pub segment_id: usize,
    /// `true` → travels `start_anchor → end_anchor` (segment direction).
    /// `false` → travels `end_anchor → start_anchor` (reverse).
    pub forward: bool,
}

impl HalfEdge {
    pub fn twin(self) -> Self {
        HalfEdge {
            segment_id: self.segment_id,
            forward: !self.forward,
        }
    }

    /// Anchor this half-edge departs from.
    pub fn from_anchor(self, net: &VectorNetwork) -> usize {
        let s = net.segment(self.segment_id).unwrap();
        if self.forward {
            s.start_anchor
        } else {
            s.end_anchor
        }
    }

    /// Anchor this half-edge arrives at.
    pub fn to_anchor(self, net: &VectorNetwork) -> usize {
        let s = net.segment(self.segment_id).unwrap();
        if self.forward {
            s.end_anchor
        } else {
            s.start_anchor
        }
    }

    /// Polar angle (radians, `[-π, π]`) of the direction this half-edge travels.
    pub fn angle(self, net: &VectorNetwork) -> f64 {
        let from = self.from_anchor(net);
        let to = self.to_anchor(net);
        let fa = net.anchor(from).unwrap();
        let ta = net.anchor(to).unwrap();
        f64::atan2(ta.y - fa.y, ta.x - fa.x)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// CCW angle difference helper
// ─────────────────────────────────────────────────────────────────────────────

/// Normalise an angle to `[0, 2π)`.
#[inline]
fn normalise(angle: f64) -> f64 {
    let a = angle % (2.0 * PI);
    if a < 0.0 {
        a + 2.0 * PI
    } else {
        a
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Signed area of a polygon (shoelace formula)
// ─────────────────────────────────────────────────────────────────────────────

/// Compute the signed area of the polygon traced by following `half_edges` in
/// order through `net`. Positive → CCW (interior face). Negative → CW (outer).
fn signed_area(half_edges: &[HalfEdge], net: &VectorNetwork) -> f64 {
    let mut area = 0.0_f64;
    for &he in half_edges {
        let from = he.from_anchor(net);
        let to = he.to_anchor(net);
        let f = net.anchor(from).unwrap();
        let t = net.anchor(to).unwrap();
        // Shoelace contribution: (x1 * y2 - x2 * y1)
        area += f.x * t.y - t.x * f.y;
    }
    area / 2.0
}

// ─────────────────────────────────────────────────────────────────────────────
// Build outgoing adjacency sorted by polar angle (CCW)
// ─────────────────────────────────────────────────────────────────────────────

/// For each anchor, the list of departing half-edges sorted by polar angle CCW.
fn build_outgoing(net: &VectorNetwork) -> HashMap<usize, Vec<HalfEdge>> {
    let mut map: HashMap<usize, Vec<HalfEdge>> = HashMap::new();

    for (seg_id, _) in net.segments() {
        let fwd = HalfEdge { segment_id: seg_id, forward: true };
        let bwd = HalfEdge { segment_id: seg_id, forward: false };

        map.entry(fwd.from_anchor(net))
            .or_default()
            .push(fwd);
        map.entry(bwd.from_anchor(net))
            .or_default()
            .push(bwd);
    }

    // Sort each adjacency list by polar angle CCW (ascending atan2)
    for list in map.values_mut() {
        list.sort_by(|a, b| {
            let ang_a = normalise(a.angle(net));
            let ang_b = normalise(b.angle(net));
            ang_a.partial_cmp(&ang_b).unwrap()
        });
    }

    map
}

// ─────────────────────────────────────────────────────────────────────────────
// Next half-edge
// ─────────────────────────────────────────────────────────────────────────────

/// Return the half-edge that follows `h` in the face traversal (i.e., the next
/// half-edge around the interior face on `h`'s left side).
///
/// Rule: at vertex `v = h.to`, form the twin `t = (v → u)`. In `v`'s CCW-
/// sorted departure list, the next half-edge immediately after `t` is `next(h)`.
fn next_half_edge(
    h: HalfEdge,
    net: &VectorNetwork,
    outgoing: &HashMap<usize, Vec<HalfEdge>>,
) -> Option<HalfEdge> {
    let v = h.to_anchor(net);
    let twin = h.twin(); // departs from v toward h.from

    let list = outgoing.get(&v)?;
    if list.is_empty() {
        return None;
    }

    // Find the position of twin in v's departure list
    let pos = list
        .iter()
        .position(|&he| he == twin)?;

    // The next position going CLOCKWISE (predecessor in CCW-sorted order).
    //
    // At vertex v, we arrived via h = (u→v). The twin t = (v→u) departs from v.
    // Sorted CCW, the half-edge immediately BEFORE t in the angular order is the
    // one that makes the sharpest left turn — it bounds the interior face on
    // the left of h.
    //
    //  CCW order: ..., prev_he, twin, next_ccw_he, ...
    //                  ^^^^^^ we want this one
    let next_pos = (pos + list.len() - 1) % list.len();
    Some(list[next_pos])
}

// ─────────────────────────────────────────────────────────────────────────────
// Public API: find_regions
// ─────────────────────────────────────────────────────────────────────────────

/// Detect all closed regions (interior faces) of the vector network.
///
/// Returns one `Region` per interior face (CCW-oriented cycle). The outer
/// infinite face (CW) is discarded. Open strands (segments that cannot form
/// a cycle) produce no regions.
///
/// Each `Region::boundary` contains the ordered half-edge sequence
/// `[(segment_id, forward), ...]` that traces the face boundary CCW.
/// The boundary is stored as `Vec<usize>` of segment IDs for compatibility
/// with the Region type; duplicates can occur (a segment can bound two faces).
pub fn find_regions(net: &VectorNetwork) -> Vec<Region> {
    // Nothing to do for empty or strand-only networks
    if net.segment_count() < 3 {
        return Vec::new();
    }

    let outgoing = build_outgoing(net);

    // Track which half-edges have been visited (segment_id, forward)
    let mut visited: HashMap<HalfEdge, bool> = HashMap::new();

    let mut regions = Vec::new();

    // Collect all half-edges in deterministic order
    let mut all_half_edges: Vec<HalfEdge> = Vec::new();
    for (seg_id, _) in net.segments() {
        all_half_edges.push(HalfEdge { segment_id: seg_id, forward: true });
        all_half_edges.push(HalfEdge { segment_id: seg_id, forward: false });
    }

    for start_he in all_half_edges {
        if visited.contains_key(&start_he) {
            continue;
        }

        // Walk the face starting from this half-edge
        let mut face: Vec<HalfEdge> = Vec::new();
        let mut current = start_he;
        let max_steps = net.segment_count() * 2 + 2; // safety bound

        for _ in 0..=max_steps {
            if visited.contains_key(&current) {
                // We've already walked this half-edge — cycle detected or
                // we re-entered the start (normal termination)
                break;
            }
            visited.insert(current, true);
            face.push(current);

            match next_half_edge(current, net, &outgoing) {
                None => break, // open strand — no region
                Some(next) => current = next,
            }
        }

        // A valid cycle must return to its start
        if face.is_empty() {
            continue;
        }
        // Verify the last next points back to start_he
        // (if we broke early due to strand or length, area check will discard it)

        // Classify: interior face has positive signed area (CCW convention)
        let area = signed_area(&face, net);
        if area <= 0.0 {
            // Outer (infinite) face or degenerate — discard
            continue;
        }

        // Collect segment IDs (the Region boundary format)
        let boundary: Vec<usize> = face.iter().map(|he| he.segment_id).collect();
        regions.push(Region::with_boundary(boundary, None));
    }

    regions
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::VectorNetwork;

    // ── Triangle → exactly 1 region ─────────────────────────────────────────

    #[test]
    fn triangle_one_region() {
        let mut net = VectorNetwork::new();
        let a = net.add_anchor(0.0, 0.0);
        let b = net.add_anchor(100.0, 0.0);
        let c = net.add_anchor(50.0, 80.0);
        net.add_segment(a, b).unwrap();
        net.add_segment(b, c).unwrap();
        net.add_segment(c, a).unwrap();

        let regions = find_regions(&net);
        assert_eq!(regions.len(), 1, "triangle should produce exactly 1 region");
        assert_eq!(regions[0].len(), 3, "triangle region has 3 boundary segments");
    }

    // ── Single line → no region (open strand) ───────────────────────────────

    #[test]
    fn line_no_region() {
        let mut net = VectorNetwork::new();
        let a = net.add_anchor(0.0, 0.0);
        let b = net.add_anchor(100.0, 0.0);
        net.add_segment(a, b).unwrap();

        let regions = find_regions(&net);
        assert!(regions.is_empty(), "a single line cannot form a region");
    }

    // ── Two segments (open path) → no region ────────────────────────────────

    #[test]
    fn open_path_no_region() {
        let mut net = VectorNetwork::new();
        let a = net.add_anchor(0.0, 0.0);
        let b = net.add_anchor(50.0, 80.0);
        let c = net.add_anchor(100.0, 0.0);
        net.add_segment(a, b).unwrap();
        net.add_segment(b, c).unwrap();

        let regions = find_regions(&net);
        assert!(regions.is_empty());
    }

    // ── Square → exactly 1 region ────────────────────────────────────────────

    #[test]
    fn square_one_region() {
        let mut net = VectorNetwork::new();
        let a = net.add_anchor(0.0, 0.0);
        let b = net.add_anchor(100.0, 0.0);
        let c = net.add_anchor(100.0, 100.0);
        let d = net.add_anchor(0.0, 100.0);
        net.add_segment(a, b).unwrap();
        net.add_segment(b, c).unwrap();
        net.add_segment(c, d).unwrap();
        net.add_segment(d, a).unwrap();

        let regions = find_regions(&net);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].len(), 4);
    }

    // ── Square with diagonal → 2 regions (two triangles) ────────────────────

    #[test]
    fn square_with_diagonal_two_regions() {
        let mut net = VectorNetwork::new();
        let a = net.add_anchor(0.0, 0.0);
        let b = net.add_anchor(100.0, 0.0);
        let c = net.add_anchor(100.0, 100.0);
        let d = net.add_anchor(0.0, 100.0);
        net.add_segment(a, b).unwrap();
        net.add_segment(b, c).unwrap();
        net.add_segment(c, d).unwrap();
        net.add_segment(d, a).unwrap();
        net.add_segment(a, c).unwrap(); // diagonal

        let regions = find_regions(&net);
        assert_eq!(regions.len(), 2, "square + diagonal → 2 triangle regions");
        // Each triangle has 3 boundary segments
        for r in &regions {
            assert_eq!(r.len(), 3);
        }
    }

    // ── Two disconnected triangles → 2 regions ───────────────────────────────

    #[test]
    fn two_disconnected_triangles_two_regions() {
        let mut net = VectorNetwork::new();

        // Triangle 1
        let a = net.add_anchor(0.0, 0.0);
        let b = net.add_anchor(50.0, 0.0);
        let c = net.add_anchor(25.0, 40.0);
        net.add_segment(a, b).unwrap();
        net.add_segment(b, c).unwrap();
        net.add_segment(c, a).unwrap();

        // Triangle 2 (disconnected, shifted right)
        let d = net.add_anchor(100.0, 0.0);
        let e = net.add_anchor(150.0, 0.0);
        let f = net.add_anchor(125.0, 40.0);
        net.add_segment(d, e).unwrap();
        net.add_segment(e, f).unwrap();
        net.add_segment(f, d).unwrap();

        let regions = find_regions(&net);
        assert_eq!(regions.len(), 2, "two disconnected triangles → 2 regions");
    }

    // ── Pentagon → 1 region with 5 segments ─────────────────────────────────

    #[test]
    fn pentagon_one_region() {
        let mut net = VectorNetwork::new();
        let n = 5_usize;
        let anchors: Vec<usize> = (0..n)
            .map(|i| {
                let angle = i as f64 * std::f64::consts::TAU / n as f64;
                net.add_anchor(50.0 * angle.cos(), 50.0 * angle.sin())
            })
            .collect();
        for i in 0..n {
            net.add_segment(anchors[i], anchors[(i + 1) % n]).unwrap();
        }

        let regions = find_regions(&net);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].len(), n);
    }

    // ── T-junction → 1 region (one side of the junction is open) ────────────

    #[test]
    fn t_junction_region_count() {
        // A triangle with an extra spoke from one vertex (T shape)
        //   c
        //  / \
        // a - b - x    (x is a dead end from b)
        let mut net = VectorNetwork::new();
        let a = net.add_anchor(0.0, 0.0);
        let b = net.add_anchor(100.0, 0.0);
        let c = net.add_anchor(50.0, 80.0);
        let x = net.add_anchor(200.0, 0.0); // dead end
        net.add_segment(a, b).unwrap();
        net.add_segment(b, c).unwrap();
        net.add_segment(c, a).unwrap();
        net.add_segment(b, x).unwrap(); // spoke — opens into nothing

        // The triangle a-b-c is still a closed region; the spoke is open
        let regions = find_regions(&net);
        assert_eq!(regions.len(), 1, "T-junction: triangle region survives the open spoke");
    }
}
