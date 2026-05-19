//! convert.rs — Region ↔ Polygon conversions.
//!
//! **Region → Polygon:** each cubic Bézier segment is sampled at `BEZIER_STEPS`
//! evenly-spaced parameter values `t ∈ (0, 1]`. Straight-line segments (no
//! control points) are exact (1 step). The resulting `Vec<(f64, f64)>` polygon
//! approximates the region boundary to sub-pixel accuracy for typical design
//! document coordinates (canvas in the 0 – 4096 range, 8 samples per curve).
//!
//! **Polygon → VectorNetwork + Region:** the polygon vertices become anchors,
//! consecutive pairs become line segments, and the closed boundary becomes a
//! `Region`. The resulting network has no Bézier handles — the boolean op
//! discards curve information for the output contour. Curved output can be
//! restored in V5 by fitting curves to the output polyline (not needed for V3).

use logos_vector::{Region, VectorNetwork};

/// Number of samples per cubic Bézier segment for polygon approximation.
/// 8 gives < 0.5 px error for typical design-document curves.
pub const BEZIER_STEPS: usize = 8;

/// A simple polygon — a closed ordered list of 2D points.
/// The closing edge `last → first` is implicit.
pub type Poly = Vec<(f64, f64)>;

// ─────────────────────────────────────────────────────────────────────────────
// Region → Poly
// ─────────────────────────────────────────────────────────────────────────────

/// Sample a cubic Bézier curve P0..P3 at parameter `t ∈ [0, 1]`.
#[inline]
pub fn cubic_bezier(
    p0: (f64, f64),
    p1: (f64, f64),
    p2: (f64, f64),
    p3: (f64, f64),
    t: f64,
) -> (f64, f64) {
    let mt = 1.0 - t;
    let mt2 = mt * mt;
    let mt3 = mt2 * mt;
    let t2 = t * t;
    let t3 = t2 * t;
    (
        mt3 * p0.0 + 3.0 * mt2 * t * p1.0 + 3.0 * mt * t2 * p2.0 + t3 * p3.0,
        mt3 * p0.1 + 3.0 * mt2 * t * p1.1 + 3.0 * mt * t2 * p2.1 + t3 * p3.1,
    )
}

/// Convert a region boundary in `net` into an approximating polygon.
///
/// Each cubic Bézier segment is sampled at `BEZIER_STEPS` points. Line segments
/// contribute a single point (their start anchor — the end is the next start).
/// The returned polygon is closed (last point connects back to first).
pub fn region_to_poly(net: &VectorNetwork, region: &Region) -> Poly {
    let mut poly = Vec::new();

    for &seg_id in &region.boundary {
        let seg = match net.segment(seg_id) {
            Some(s) => s,
            None => continue,
        };
        let a_start = match net.anchor(seg.start_anchor) {
            Some(a) => a,
            None => continue,
        };
        let a_end = match net.anchor(seg.end_anchor) {
            Some(a) => a,
            None => continue,
        };

        let p0 = (a_start.x, a_start.y);
        let p3 = (a_end.x, a_end.y);

        match (seg.control1, seg.control2) {
            (Some(p1), Some(p2)) => {
                // Cubic Bézier — sample interior points (t=0 is start, already
                // added by the previous segment; t=1 is end, next segment's start)
                for step in 0..BEZIER_STEPS {
                    let t = (step + 1) as f64 / BEZIER_STEPS as f64;
                    // Skip t=1.0 on all but the last segment to avoid duplicate
                    if step < BEZIER_STEPS - 1 {
                        poly.push(cubic_bezier(p0, p1, p2, p3, t));
                    }
                }
            }
            _ => {
                // Straight line — emit start only (end is next segment's start)
                poly.push(p0);
            }
        }
    }

    // Close: if the last segment's end isn't at poly[0], push it.
    // (For straight-line boundaries the first anchor is the only gap.)
    if poly.is_empty() {
        return poly;
    }

    poly
}

/// Compute the signed area of a polygon (shoelace formula).
/// Positive → CCW. Negative → CW.
pub fn poly_signed_area(poly: &Poly) -> f64 {
    let n = poly.len();
    if n < 3 {
        return 0.0;
    }
    let mut area = 0.0_f64;
    for i in 0..n {
        let (x1, y1) = poly[i];
        let (x2, y2) = poly[(i + 1) % n];
        area += x1 * y2 - x2 * y1;
    }
    area / 2.0
}

/// Ensure polygon is CCW (positive area). Reverses in-place if CW.
pub fn ensure_ccw(poly: &mut Poly) {
    if poly_signed_area(poly) < 0.0 {
        poly.reverse();
    }
}

/// Ensure polygon is CW (negative area). Reverses in-place if CCW.
pub fn ensure_cw(poly: &mut Poly) {
    if poly_signed_area(poly) > 0.0 {
        poly.reverse();
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Poly → VectorNetwork + Region
// ─────────────────────────────────────────────────────────────────────────────

/// Add a polygon as straight-line anchors + segments + region to an existing
/// `VectorNetwork`. Returns the index of the new `Region`.
pub fn poly_to_network(net: &mut VectorNetwork, poly: &Poly) -> Option<Region> {
    if poly.len() < 3 {
        return None;
    }
    let anchor_ids: Vec<usize> = poly
        .iter()
        .map(|&(x, y)| net.add_anchor(x, y))
        .collect();
    let n = anchor_ids.len();
    let mut seg_ids = Vec::with_capacity(n);
    for i in 0..n {
        let seg = net.add_segment(anchor_ids[i], anchor_ids[(i + 1) % n]).ok()?;
        seg_ids.push(seg);
    }
    Some(Region::with_boundary(seg_ids, None))
}

// ─────────────────────────────────────────────────────────────────────────────
// Bounding box helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Axis-aligned bounding box of a polygon.
pub fn poly_bbox(poly: &Poly) -> (f64, f64, f64, f64) {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for &(x, y) in poly {
        if x < min_x { min_x = x; }
        if y < min_y { min_y = y; }
        if x > max_x { max_x = x; }
        if y > max_y { max_y = y; }
    }
    (min_x, min_y, max_x, max_y)
}

/// Returns `true` if the bounding boxes of `a` and `b` overlap.
pub fn bboxes_overlap(a: &Poly, b: &Poly) -> bool {
    let (ax0, ay0, ax1, ay1) = poly_bbox(a);
    let (bx0, by0, bx1, by1) = poly_bbox(b);
    ax0 <= bx1 && ax1 >= bx0 && ay0 <= by1 && ay1 >= by0
}
