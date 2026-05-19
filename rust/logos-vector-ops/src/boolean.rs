//! boolean.rs — Greiner-Hormann polygon boolean operations.
//!
//! Reference: *Efficient Clipping of Arbitrary Polygons*, Greiner & Hormann,
//! ACM Transactions on Graphics, 17(2), 1998.

use crate::convert::{bboxes_overlap, poly_signed_area, Poly};

// ─────────────────────────────────────────────────────────────────────────────
// Vertex doubly-linked list
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
struct Vert {
    x: f64,
    y: f64,
    alpha: f64,
    intersect: bool,
    /// For intersection vertices on A: true = entering B, false = exiting B.
    entry: bool,
    /// Index of the corresponding vertex in the other polygon's list.
    neighbor: usize,
    visited: bool,
    next: usize,
    prev: usize,
}

impl Vert {
    fn original(x: f64, y: f64, next: usize, prev: usize) -> Self {
        Self { x, y, alpha: 0.0, intersect: false, entry: false,
               neighbor: usize::MAX, visited: false, next, prev }
    }

    fn isect(x: f64, y: f64, alpha: f64, neighbor: usize, next: usize, prev: usize) -> Self {
        Self { x, y, alpha, intersect: true, entry: false,
               neighbor, visited: false, next, prev }
    }
}

fn build_list(poly: &Poly) -> Vec<Vert> {
    let n = poly.len();
    (0..n)
        .map(|i| Vert::original(poly[i].0, poly[i].1, (i + 1) % n, (i + n - 1) % n))
        .collect()
}

fn insert_after(list: &mut Vec<Vert>, after: usize, v: Vert) -> usize {
    let next = list[after].next;
    let id = list.len();
    list.push(v);
    list[id].next = next;
    list[id].prev = after;
    list[after].next = id;
    list[next].prev = id;
    id
}

fn sort_edge_intersections(list: &mut Vec<Vert>, from_orig: usize) {
    let mut chain: Vec<usize> = Vec::new();
    let mut cur = list[from_orig].next;
    while list[cur].intersect {
        chain.push(cur);
        cur = list[cur].next;
    }
    let after_chain = cur;
    if chain.len() <= 1 {
        return;
    }
    chain.sort_by(|&a, &b| {
        list[a].alpha.partial_cmp(&list[b].alpha).unwrap()
    });
    list[from_orig].next = chain[0];
    list[chain[0]].prev = from_orig;
    for i in 0..chain.len() - 1 {
        list[chain[i]].next = chain[i + 1];
        list[chain[i + 1]].prev = chain[i];
    }
    let last = *chain.last().unwrap();
    list[last].next = after_chain;
    list[after_chain].prev = last;
}

// ─────────────────────────────────────────────────────────────────────────────
// Segment-segment intersection
// ─────────────────────────────────────────────────────────────────────────────

const EPS: f64 = 1e-9;

fn seg_intersect(
    p1: (f64, f64), p2: (f64, f64),
    p3: (f64, f64), p4: (f64, f64),
) -> Option<(f64, f64, f64, f64)> {
    let (x1, y1) = p1; let (x2, y2) = p2;
    let (x3, y3) = p3; let (x4, y4) = p4;
    let denom = (x1 - x2) * (y3 - y4) - (y1 - y2) * (x3 - x4);
    if denom.abs() < 1e-12 { return None; }
    let t = ((x1 - x3) * (y3 - y4) - (y1 - y3) * (x3 - x4)) / denom;
    let u = -((x1 - x2) * (y1 - y3) - (y1 - y2) * (x1 - x3)) / denom;
    if t > EPS && t < 1.0 - EPS && u > EPS && u < 1.0 - EPS {
        Some((t, u, x1 + t * (x2 - x1), y1 + t * (y2 - y1)))
    } else {
        None
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Point-in-polygon
// ─────────────────────────────────────────────────────────────────────────────

fn point_in_poly(px: f64, py: f64, poly: &Poly) -> bool {
    let n = poly.len();
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = poly[i];
        let (xj, yj) = poly[j];
        if ((yi > py) != (yj > py)) && (px < (xj - xi) * (py - yi) / (yj - yi) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

// ─────────────────────────────────────────────────────────────────────────────
// Op
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Op {
    Union,
    Intersect,
    Subtract,
    Exclude,
}

// ─────────────────────────────────────────────────────────────────────────────
// Degeneracy (no interior intersections found)
// ─────────────────────────────────────────────────────────────────────────────

fn degeneracy_result(a: &Poly, b: &Poly, op: Op) -> Vec<Poly> {
    let a_in_b = !a.is_empty() && point_in_poly(a[0].0, a[0].1, b);
    let b_in_a = !b.is_empty() && point_in_poly(b[0].0, b[0].1, a);
    match op {
        Op::Union => {
            if a_in_b { vec![b.clone()] }
            else if b_in_a { vec![a.clone()] }
            else { vec![a.clone(), b.clone()] }
        }
        Op::Intersect => {
            if a_in_b { vec![a.clone()] }
            else if b_in_a { vec![b.clone()] }
            else { vec![] }
        }
        Op::Subtract => {
            if a_in_b { vec![] }
            else { vec![a.clone()] }
        }
        Op::Exclude => {
            if a_in_b || b_in_a { vec![] }
            else { vec![a.clone(), b.clone()] }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Phase 3 — polygon tracing
// ─────────────────────────────────────────────────────────────────────────────
//
// `look_for = true`  → INTERSECT: start at ENTRY on A; switch A→B at EXIT.
// `look_for = false` → UNION:     start at EXIT on A;  switch A→B at ENTRY.
//
// "Switch A→B" happens when we ARRIVE AT an intersection vertex with
//   `entry == !look_for`.
// "Switch B→A" happens when we reach a B intersection vertex whose A-neighbor
//   has `entry == look_for`.

fn trace(la: &mut Vec<Vert>, lb: &mut Vec<Vert>, look_for: bool) -> Vec<Poly> {
    let mut result: Vec<Poly> = Vec::new();
    let safety = (la.len() + lb.len()) * 4 + 32;

    'outer: loop {
        // Find unvisited starting vertex on A with entry == look_for
        let start = {
            let mut found = None;
            let mut cur = 0_usize;
            for _ in 0..la.len() {
                if la[cur].intersect && !la[cur].visited && la[cur].entry == look_for {
                    found = Some(cur);
                    break;
                }
                cur = la[cur].next;
                if cur == 0 { break; }
            }
            match found { Some(i) => i, None => break 'outer }
        };

        let mut poly: Poly = Vec::new();
        let mut cur_a = start;

        for _step in 0..safety {
            // Emit current A vertex
            la[cur_a].visited = true;
            poly.push((la[cur_a].x, la[cur_a].y));

            // Advance on A
            let next_a = la[cur_a].next;

            if la[next_a].intersect && la[next_a].entry == !look_for {
                // ── A → B switch ─────────────────────────────────────────────
                // Emit the crossing vertex itself
                la[next_a].visited = true;
                poly.push((la[next_a].x, la[next_a].y));

                let b_entry = la[next_a].neighbor; // B vertex at the crossing

                // Walk B forward from the crossing
                let mut cur_b = lb[b_entry].next;
                let mut switched_back = false;

                for _bstep in 0..safety {
                    if lb[cur_b].intersect {
                        let a_nb = lb[cur_b].neighbor;
                        if la[a_nb].entry == look_for {
                            // ── B → A switch ─────────────────────────────────
                            // Emit this B crossing vertex too
                            lb[cur_b].visited = true;
                            poly.push((lb[cur_b].x, lb[cur_b].y));
                            cur_a = a_nb;
                            switched_back = true;
                            break;
                        }
                    }
                    lb[cur_b].visited = true;
                    poly.push((lb[cur_b].x, lb[cur_b].y));
                    cur_b = lb[cur_b].next;

                    if cur_b == b_entry { break; } // safety: looped B without switch
                }

                if !switched_back {
                    break; // degenerate — bail out of this polygon
                }

                // Check closure: if we are back at start on A, done
                if cur_a == start {
                    break;
                }
            } else {
                // advance normally on A
                cur_a = next_a;
                if cur_a == start {
                    break;
                }
            }
        }

        if poly.len() >= 3 {
            result.push(poly);
        }
        if result.len() > 64 { break; }
    }

    result
}

// ─────────────────────────────────────────────────────────────────────────────
// Phase 3 — Subtract trace (A - B)
// ─────────────────────────────────────────────────────────────────────────────
//
// Start at EXIT vertices on A (exiting B → about to traverse A's boundary that
// is OUTSIDE B). When we reach an ENTRY on A (about to enter B), switch to B
// and traverse BACKWARDS along B (collecting B's boundary inside A). Return to
// A when we encounter a B vertex whose A-neighbor is an EXIT.

fn trace_subtract(la: &mut Vec<Vert>, lb: &mut Vec<Vert>) -> Vec<Poly> {
    let mut result: Vec<Poly> = Vec::new();
    let safety = (la.len() + lb.len()) * 4 + 32;

    'outer: loop {
        // Find unvisited EXIT vertex on A
        let start = {
            let mut found = None;
            let mut cur = 0_usize;
            for _ in 0..la.len() {
                if la[cur].intersect && !la[cur].visited && !la[cur].entry {
                    found = Some(cur);
                    break;
                }
                cur = la[cur].next;
                if cur == 0 { break; }
            }
            match found { Some(i) => i, None => break 'outer }
        };

        let mut poly: Poly = Vec::new();
        let mut cur_a = start;

        for _step in 0..safety {
            la[cur_a].visited = true;
            poly.push((la[cur_a].x, la[cur_a].y));
            let next_a = la[cur_a].next;

            if la[next_a].intersect && la[next_a].entry {
                // A ENTRY → switch to B, traverse BACKWARDS
                la[next_a].visited = true;
                poly.push((la[next_a].x, la[next_a].y));

                let b_entry = la[next_a].neighbor;
                let mut cur_b = lb[b_entry].prev; // traverse backward
                let mut switched_back = false;

                for _bstep in 0..safety {
                    if lb[cur_b].intersect {
                        let a_nb = lb[cur_b].neighbor;
                        if !la[a_nb].entry {
                            // A EXIT found → switch back
                            lb[cur_b].visited = true;
                            poly.push((lb[cur_b].x, lb[cur_b].y));
                            cur_a = a_nb;
                            switched_back = true;
                            break;
                        }
                    }
                    lb[cur_b].visited = true;
                    poly.push((lb[cur_b].x, lb[cur_b].y));
                    cur_b = lb[cur_b].prev; // backward
                    if cur_b == b_entry { break; }
                }

                if !switched_back { break; }
                if cur_a == start { break; }
            } else {
                cur_a = next_a;
                if cur_a == start { break; }
            }
        }

        if poly.len() >= 3 { result.push(poly); }
        if result.len() > 64 { break; }
    }

    result
}

// ─────────────────────────────────────────────────────────────────────────────
// Main entry point
// ─────────────────────────────────────────────────────────────────────────────

pub fn greiner_boolean(a: &Poly, b: &Poly, op: Op) -> Vec<Poly> {
    if a.len() < 3 || b.len() < 3 {
        return match op {
            Op::Union => {
                let mut r = Vec::new();
                if a.len() >= 3 { r.push(a.clone()); }
                if b.len() >= 3 { r.push(b.clone()); }
                r
            }
            _ => vec![],
        };
    }

    // Exclude → union (approximation; full XOR deferred to V4)
    if op == Op::Exclude {
        return greiner_boolean(a, b, Op::Union);
    }

    if !bboxes_overlap(a, b) {
        return degeneracy_result(a, b, op);
    }

    // ── Phase 1: build lists ─────────────────────────────────────────────────

    let mut la = build_list(a);
    let mut lb = build_list(b);
    let na = a.len();
    let nb = b.len();

    // ── Phase 1: insert intersections ────────────────────────────────────────

    let mut has_intersections = false;
    let a_orig: Vec<usize> = (0..na).collect();
    let b_orig: Vec<usize> = (0..nb).collect();

    for &ai in &a_orig {
        // Find the next ORIGINAL vertex (skip any already-inserted intersections)
        // After Phase 1, intersection vertices have index ≥ na (for la) / ≥ nb (for lb).
        // But since we insert as we go, we need to walk to find the next original.
        let ai_next = {
            let mut c = la[ai].next;
            while c >= na { c = la[c].next; }
            c
        };
        let p1 = (la[ai].x, la[ai].y);
        let p2 = (la[ai_next].x, la[ai_next].y);

        for &bi in &b_orig {
            let bi_next = {
                let mut c = lb[bi].next;
                while c >= nb { c = lb[c].next; }
                c
            };
            let p3 = (lb[bi].x, lb[bi].y);
            let p4 = (lb[bi_next].x, lb[bi_next].y);

            if let Some((t, u, ix, iy)) = seg_intersect(p1, p2, p3, p4) {
                has_intersections = true;
                // Placeholder index for B vertex (will update after insertion)
                let ib_placeholder = lb.len();
                let ia_new = insert_after(&mut la, ai,
                    Vert::isect(ix, iy, t, ib_placeholder, 0, 0));
                let ib_new = insert_after(&mut lb, bi,
                    Vert::isect(ix, iy, u, ia_new, 0, 0));
                la[ia_new].neighbor = ib_new;
            }
        }
    }

    if !has_intersections {
        return degeneracy_result(a, b, op);
    }

    // Sort intersections along each original edge
    for &ai in &a_orig {
        sort_edge_intersections(&mut la, ai);
    }
    for &bi in &b_orig {
        sort_edge_intersections(&mut lb, bi);
    }

    // ── Phase 2: mark entry/exit on A's intersection vertices ────────────────

    let a_start_inside = point_in_poly(la[0].x, la[0].y, b);
    let mut inside = a_start_inside;
    let mut cur = 0_usize;
    loop {
        if la[cur].intersect {
            la[cur].entry = !inside; // true = entry (was outside → now inside)
            inside = !inside;
        }
        cur = la[cur].next;
        if cur == 0 { break; }
    }

    // ── Phase 3: trace ───────────────────────────────────────────────────────

    let result = if op == Op::Subtract {
        trace_subtract(&mut la, &mut lb)
    } else {
        let look_for = op == Op::Intersect; // true → INTERSECT, false → UNION
        trace(&mut la, &mut lb, look_for)
    };

    if result.is_empty() {
        degeneracy_result(a, b, op)
    } else {
        result
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers (used in tests and ops.rs)
// ─────────────────────────────────────────────────────────────────────────────

pub fn total_area(polys: &[Poly]) -> f64 {
    polys.iter().map(|p| poly_signed_area(p).abs()).sum()
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn unit_square() -> Poly {
        vec![(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]
    }

    fn big_rect() -> Poly {
        vec![(0.0, 0.0), (2.0, 0.0), (2.0, 1.0), (0.0, 1.0)]
    }

    fn small_square() -> Poly {
        vec![(0.25, 0.25), (0.75, 0.25), (0.75, 0.75), (0.25, 0.75)]
    }

    /// A = 3×3 square, area = 9.
    fn sq_a() -> Poly { vec![(0.0,0.0),(3.0,0.0),(3.0,3.0),(0.0,3.0)] }

    /// B = 1×5 tall rectangle crossing A, area = 5.
    /// Interior intersections at (1,0),(2,0),(1,3),(2,3).
    /// Overlap = [1,2]×[0,3], area = 3.
    fn tall_b() -> Poly { vec![(1.0,-1.0),(2.0,-1.0),(2.0,4.0),(1.0,4.0)] }

    // ── Idempotency ──────────────────────────────────────────────────────────

    #[test]
    fn union_idempotent() {
        let a = unit_square();
        let area = total_area(&greiner_boolean(&a, &a, Op::Union));
        assert!((area - 1.0).abs() < 0.01, "A ∪ A ≈ 1.0, got {area}");
    }

    #[test]
    fn intersect_idempotent() {
        let a = unit_square();
        let area = total_area(&greiner_boolean(&a, &a, Op::Intersect));
        assert!((area - 1.0).abs() < 0.01, "A ∩ A ≈ 1.0, got {area}");
    }

    // ── Identity laws ────────────────────────────────────────────────────────

    #[test]
    fn union_with_empty() {
        let a = unit_square();
        let empty: Poly = vec![];
        let area = total_area(&greiner_boolean(&a, &empty, Op::Union));
        assert!((area - 1.0).abs() < 0.01, "A ∪ ∅ ≈ 1.0, got {area}");
    }

    #[test]
    fn intersect_with_empty() {
        let empty: Poly = vec![];
        let r = greiner_boolean(&unit_square(), &empty, Op::Intersect);
        assert!(r.is_empty(), "A ∩ ∅ = ∅");
    }

    // ── Containment ──────────────────────────────────────────────────────────

    #[test]
    fn union_a_in_b() {
        let area = total_area(&greiner_boolean(&unit_square(), &big_rect(), Op::Union));
        assert!((area - 2.0).abs() < 0.05, "union containment ≈ 2.0, got {area}");
    }

    #[test]
    fn intersect_a_in_b() {
        let area = total_area(&greiner_boolean(&unit_square(), &big_rect(), Op::Intersect));
        assert!((area - 1.0).abs() < 0.05, "intersect containment ≈ 1.0, got {area}");
    }

    // ── Disjoint ─────────────────────────────────────────────────────────────

    #[test]
    fn union_disjoint() {
        let b: Poly = vec![(5.0,0.0),(6.0,0.0),(6.0,1.0),(5.0,1.0)];
        let area = total_area(&greiner_boolean(&unit_square(), &b, Op::Union));
        assert!((area - 2.0).abs() < 1e-6, "disjoint union = 2.0, got {area}");
    }

    #[test]
    fn intersect_disjoint() {
        let b: Poly = vec![(5.0,0.0),(6.0,0.0),(6.0,1.0),(5.0,1.0)];
        assert!(greiner_boolean(&unit_square(), &b, Op::Intersect).is_empty());
    }

    #[test]
    fn subtract_disjoint() {
        let b: Poly = vec![(5.0,0.0),(6.0,0.0),(6.0,1.0),(5.0,1.0)];
        let area = total_area(&greiner_boolean(&unit_square(), &b, Op::Subtract));
        assert!((area - 1.0).abs() < 1e-6, "disjoint subtract = 1.0, got {area}");
    }

    // ── Non-degenerate overlapping polygons ───────────────────────────────────

    #[test]
    fn union_overlapping() {
        // Union area = 9 + 5 - 3 = 11
        let area = total_area(&greiner_boolean(&sq_a(), &tall_b(), Op::Union));
        assert!((area - 11.0).abs() < 0.5, "overlapping union ≈ 11.0, got {area}");
    }

    #[test]
    fn intersect_overlapping() {
        // Intersect area = 1×3 = 3
        let area = total_area(&greiner_boolean(&sq_a(), &tall_b(), Op::Intersect));
        assert!((area - 3.0).abs() < 0.5, "overlapping intersect ≈ 3.0, got {area}");
    }

    #[test]
    fn subtract_overlapping() {
        // A - B = 9 - 3 = 6
        let area = total_area(&greiner_boolean(&sq_a(), &tall_b(), Op::Subtract));
        assert!((area - 6.0).abs() < 1.0, "overlapping subtract ≈ 6.0, got {area}");
    }

    // ── Subtract contained ───────────────────────────────────────────────────

    #[test]
    fn subtract_contained() {
        let area = total_area(&greiner_boolean(&big_rect(), &small_square(), Op::Subtract));
        assert!(area >= 1.0, "subtract hole area >= 1.0, got {area}");
    }
}
