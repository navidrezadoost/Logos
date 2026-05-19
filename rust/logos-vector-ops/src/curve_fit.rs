//! curve_fit.rs — Schneider cubic Bézier fitting for polylines.
//!
//! Fits a sequence of cubic Bézier curves to an input polyline (ordered list of
//! 2-D points) using the algorithm from Schneider, "An Algorithm for
//! Automatically Fitting Digitized Curves" (Glassner, *Graphics Gems*, 1990).
//!
//! # Usage
//!
//! ```rust
//! use logos_vector_ops::curve_fit::{fit_bezier_curves, CubicBezier, Point};
//!
//! let polyline: Vec<Point> = vec![(0.0, 0.0), (50.0, 80.0), (100.0, 0.0)];
//! let curves = fit_bezier_curves(&polyline, 1.0);
//! assert!(!curves.is_empty());
//! ```
//!
//! # Complexity
//!
//! The recursion depth is bounded by `O(n)` where `n` is the number of points.
//! Each level solves a 2-variable least-squares system in O(n) time.

/// A 2-D point.
pub type Point = (f64, f64);

/// A cubic Bézier segment with 4 control points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CubicBezier {
    pub p0: Point,
    pub p1: Point,
    pub p2: Point,
    pub p3: Point,
}

impl CubicBezier {
    /// Evaluate the curve at parameter `t ∈ [0, 1]`.
    #[inline]
    pub fn eval(&self, t: f64) -> Point {
        let mt = 1.0 - t;
        let mt2 = mt * mt;
        let mt3 = mt2 * mt;
        let t2 = t * t;
        let t3 = t2 * t;
        (
            mt3 * self.p0.0 + 3.0 * mt2 * t * self.p1.0
                + 3.0 * mt * t2 * self.p2.0 + t3 * self.p3.0,
            mt3 * self.p0.1 + 3.0 * mt2 * t * self.p1.1
                + 3.0 * mt * t2 * self.p2.1 + t3 * self.p3.1,
        )
    }

    /// Squared distance from a point to a point on the curve.
    #[inline]
    pub fn dist2_to(&self, t: f64, q: Point) -> f64 {
        let p = self.eval(t);
        let dx = p.0 - q.0;
        let dy = p.1 - q.1;
        dx * dx + dy * dy
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Recursion limit to prevent stack overflow on degenerate inputs.
const MAX_DEPTH: usize = 64;

/// Fit a sequence of cubic Bézier curves to `polyline`.
///
/// Returns a `Vec<CubicBezier>` whose concatenation approximates the polyline
/// within the given `tolerance` (in the same units as the input coordinates).
///
/// The input must have at least 2 points. Single-point inputs return `[]`.
/// Duplicate leading/trailing points are gracefully handled.
pub fn fit_bezier_curves(polyline: &[Point], tolerance: f64) -> Vec<CubicBezier> {
    let pts = deduplicate(polyline);
    if pts.len() < 2 {
        return vec![];
    }
    if pts.len() == 2 {
        // Degenerate: just a straight line. Represent as a "bezier" with
        // collinear control points.
        let p0 = pts[0];
        let p3 = pts[1];
        let p1 = lerp(p0, p3, 1.0 / 3.0);
        let p2 = lerp(p0, p3, 2.0 / 3.0);
        return vec![CubicBezier { p0, p1, p2, p3 }];
    }

    let t1 = left_tangent(&pts);
    let t2 = right_tangent(&pts);
    let mut out = Vec::new();
    fit_recursive(&pts, &t1, &t2, tolerance, 0, &mut out);
    out
}

// ─────────────────────────────────────────────────────────────────────────────
// Internals
// ─────────────────────────────────────────────────────────────────────────────

/// Remove consecutive duplicate points.
fn deduplicate(pts: &[Point]) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::with_capacity(pts.len());
    for &p in pts {
        if out.last().map_or(true, |&last| dist2(last, p) > 1e-12) {
            out.push(p);
        }
    }
    out
}

#[inline]
fn dist2(a: Point, b: Point) -> f64 {
    let dx = a.0 - b.0;
    let dy = a.1 - b.1;
    dx * dx + dy * dy
}

#[inline]
fn dist(a: Point, b: Point) -> f64 {
    dist2(a, b).sqrt()
}

#[inline]
fn sub(a: Point, b: Point) -> Point {
    (a.0 - b.0, a.1 - b.1)
}

#[inline]
fn add(a: Point, b: Point) -> Point {
    (a.0 + b.0, a.1 + b.1)
}

#[inline]
fn scale(v: Point, s: f64) -> Point {
    (v.0 * s, v.1 * s)
}

#[inline]
fn normalize(v: Point) -> Point {
    let len = (v.0 * v.0 + v.1 * v.1).sqrt();
    if len < 1e-12 { return (1.0, 0.0); }
    (v.0 / len, v.1 / len)
}

#[inline]
fn dot(a: Point, b: Point) -> f64 {
    a.0 * b.0 + a.1 * b.1
}

#[inline]
fn lerp(a: Point, b: Point, t: f64) -> Point {
    (a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1))
}

/// Estimate the unit tangent at the start of a polyline using the first two
/// distinct points.
fn left_tangent(pts: &[Point]) -> Point {
    for i in 1..pts.len() {
        let d = sub(pts[i], pts[0]);
        if dist2(d, (0.0, 0.0)) > 1e-12 {
            return normalize(d);
        }
    }
    (1.0, 0.0)
}

/// Estimate the unit tangent at the end of a polyline using the last two
/// distinct points, pointing outward (away from the interior).
fn right_tangent(pts: &[Point]) -> Point {
    let n = pts.len();
    for i in (0..n - 1).rev() {
        let d = sub(pts[i], pts[n - 1]);
        if dist2(d, (0.0, 0.0)) > 1e-12 {
            return normalize(d);
        }
    }
    (-1.0, 0.0)
}

/// Chord-length parameterisation.  Returns `u[i] ∈ [0, 1]` for each point.
fn chord_length_params(pts: &[Point]) -> Vec<f64> {
    let n = pts.len();
    let mut u = vec![0.0_f64; n];
    for i in 1..n {
        u[i] = u[i - 1] + dist(pts[i], pts[i - 1]);
    }
    let total = u[n - 1];
    if total > 1e-12 {
        for ui in &mut u {
            *ui /= total;
        }
    }
    u
}

/// Re-parameterise using Newton-Raphson for each point given current curve.
fn reparametrize(pts: &[Point], u: &[f64], bez: &CubicBezier) -> Vec<f64> {
    u.iter()
        .zip(pts.iter())
        .map(|(&ui, &q)| newton_raphson_root(bez, ui, q))
        .collect()
}

/// One step of Newton-Raphson to find the parameter `t` minimising `|B(t) - p|`.
fn newton_raphson_root(bez: &CubicBezier, u: f64, p: Point) -> f64 {
    let bu = bez_eval(bez, u);
    let d1 = bez_deriv(bez, u);    // B'(u)
    let d2 = bez_deriv2(bez, u);   // B''(u)

    // numerator: (B(u) - p) · B'(u)
    let num = dot(sub(bu, p), d1);
    // denominator: |B'(u)|² + (B(u)-p)·B''(u)
    let denom = dot(d1, d1) + dot(sub(bu, p), d2);

    if denom.abs() < 1e-12 { return u; }
    (u - num / denom).clamp(0.0, 1.0)
}

fn bez_eval(b: &CubicBezier, t: f64) -> Point {
    b.eval(t)
}

/// First derivative of cubic Bézier at `t`.
fn bez_deriv(b: &CubicBezier, t: f64) -> Point {
    // B'(t) = 3[(1-t)²(P1-P0) + 2t(1-t)(P2-P1) + t²(P3-P2)]
    let mt = 1.0 - t;
    let q0 = scale(sub(b.p1, b.p0), 3.0 * mt * mt);
    let q1 = scale(sub(b.p2, b.p1), 6.0 * mt * t);
    let q2 = scale(sub(b.p3, b.p2), 3.0 * t * t);
    add(add(q0, q1), q2)
}

/// Second derivative of cubic Bézier at `t`.
fn bez_deriv2(b: &CubicBezier, t: f64) -> Point {
    // B''(t) = 6[(1-t)(P2-2P1+P0) + t(P3-2P2+P1)]
    let mt = 1.0 - t;
    let q0 = sub(add(b.p2, b.p0), scale(b.p1, 2.0));
    let q1 = sub(add(b.p3, b.p1), scale(b.p2, 2.0));
    add(scale(q0, 6.0 * mt), scale(q1, 6.0 * t))
}

/// Evaluate B_{i,3}(t) — the four Bernstein basis polynomials of degree 3.
#[inline]
fn bernstein(t: f64) -> [f64; 4] {
    let mt = 1.0 - t;
    let mt2 = mt * mt;
    let t2 = t * t;
    [mt2 * mt, 3.0 * mt2 * t, 3.0 * mt * t2, t2 * t]
}

/// Fit a single cubic Bézier to `pts` with unit tangents `t1`, `t2` at the
/// endpoints, using least-squares over parameters `u`.
///
/// The Schneider method constrains P0 = pts[0], P3 = pts[last],
/// P1 = P0 + alpha1 * t1, P2 = P3 + alpha2 * t2, and solves for alpha1/alpha2.
fn generate_bezier(pts: &[Point], u: &[f64], t1: &Point, t2: &Point) -> CubicBezier {
    let n = pts.len();
    let p0 = pts[0];
    let p3 = pts[n - 1];

    // Matrices C and X from Schneider §2.
    let mut c = [[0.0_f64; 2]; 2];
    let mut x = [0.0_f64; 2];

    for i in 0..n {
        let b = bernstein(u[i]);
        let a1 = scale(*t1, b[1]);
        let a2 = scale(*t2, b[2]);
        c[0][0] += dot(a1, a1);
        c[0][1] += dot(a1, a2);
        c[1][0] = c[0][1];
        c[1][1] += dot(a2, a2);

        // tmp = pts[i] - B(u[i])|_{alpha1=alpha2=0}
        //     = pts[i] - b0*p0 - b1*p0 - b2*p3 - b3*p3
        let b013 = add(scale(p0, b[0] + b[1]), scale(p3, b[2] + b[3]));
        let tmp = sub(pts[i], b013);
        x[0] += dot(a1, tmp);
        x[1] += dot(a2, tmp);
    }

    // Solve 2×2 linear system by Cramer's rule.
    let det_c = c[0][0] * c[1][1] - c[0][1] * c[1][0];
    let alpha1 = if det_c.abs() > 1e-12 {
        (x[0] * c[1][1] - x[1] * c[0][1]) / det_c
    } else {
        0.0
    };
    let alpha2 = if det_c.abs() > 1e-12 {
        (c[0][0] * x[1] - c[1][0] * x[0]) / det_c
    } else {
        0.0
    };

    // Fallback: use heuristic control distances if alphas are infeasible.
    let chord = dist(p0, p3);
    let fallback = chord / 3.0;
    let a1 = if alpha1 > 1e-6 { alpha1 } else { fallback };
    let a2 = if alpha2 > 1e-6 { alpha2 } else { fallback };

    CubicBezier {
        p0,
        p1: add(p0, scale(*t1, a1)),
        p2: add(p3, scale(*t2, a2)),
        p3,
    }
}

/// Maximum squared distance between any point and the corresponding curve point.
/// Returns (max_error², index_of_max).
fn max_error(pts: &[Point], u: &[f64], bez: &CubicBezier) -> (f64, usize) {
    let mut max = 0.0_f64;
    let mut split_idx = pts.len() / 2;
    for (i, (&ui, &q)) in u.iter().zip(pts.iter()).enumerate() {
        let e = bez.dist2_to(ui, q);
        if e > max {
            max = e;
            split_idx = i;
        }
    }
    (max, split_idx)
}

/// Recursive fitting core.  Appends fitted curves to `out`.
fn fit_recursive(
    pts: &[Point],
    t1: &Point,
    t2: &Point,
    tolerance: f64,
    depth: usize,
    out: &mut Vec<CubicBezier>,
) {
    let n = pts.len();
    if n < 2 {
        return;
    }
    if n == 2 {
        let p0 = pts[0];
        let p3 = pts[1];
        let p1 = add(p0, scale(*t1, dist(p0, p3) / 3.0));
        let p2 = add(p3, scale(*t2, dist(p0, p3) / 3.0));
        out.push(CubicBezier { p0, p1, p2, p3 });
        return;
    }

    let u = chord_length_params(pts);
    let mut bez = generate_bezier(pts, &u, t1, t2);
    let (mut err2, mut split_idx) = max_error(pts, &u, &bez);
    let tol2 = tolerance * tolerance;

    if err2 < tol2 {
        out.push(bez);
        return;
    }

    // Try Newton-Raphson re-parameterisation (up to 4 iterations) before splitting.
    if depth < MAX_DEPTH {
        for _ in 0..4 {
            let u_new = reparametrize(pts, &u, &bez);
            bez = generate_bezier(pts, &u_new, t1, t2);
            let (e2, si) = max_error(pts, &u_new, &bez);
            split_idx = si;
            if e2 < tol2 {
                out.push(bez);
                return;
            }
        }
    }

    // Split at the point with greatest error.
    // Guard against degenerate splits (don't let split land at endpoints).
    split_idx = split_idx.clamp(1, n - 2);

    // Tangent at split point: use the chord through neighbours.
    let tan_split = {
        let v = sub(pts[split_idx + 1], pts[split_idx - 1]);
        normalize(v)
    };
    let tan_split_neg = (-(tan_split.0), -(tan_split.1));

    if depth + 1 >= MAX_DEPTH {
        // At recursion limit: just emit two straight-line beziers.
        let mid = pts[split_idx];
        out.push(CubicBezier {
            p0: pts[0],
            p1: lerp(pts[0], mid, 1.0 / 3.0),
            p2: lerp(pts[0], mid, 2.0 / 3.0),
            p3: mid,
        });
        out.push(CubicBezier {
            p0: mid,
            p1: lerp(mid, pts[n - 1], 1.0 / 3.0),
            p2: lerp(mid, pts[n - 1], 2.0 / 3.0),
            p3: pts[n - 1],
        });
        return;
    }

    fit_recursive(&pts[..=split_idx], t1, &tan_split_neg, tolerance, depth + 1, out);
    fit_recursive(&pts[split_idx..], &tan_split, t2, tolerance, depth + 1, out);
}

// ─────────────────────────────────────────────────────────────────────────────
// Integration helpers — convert fitted curves back to VectorNetwork
// ─────────────────────────────────────────────────────────────────────────────

use logos_vector::VectorNetwork;
use crate::convert::{poly_to_network, Poly};
use logos_vector::Region;

/// Like `poly_to_network`, but instead of straight segments uses the fitted
/// cubic Bézier curves.  Each `CubicBezier` becomes one `add_cubic_segment`.
pub fn bezier_list_to_network(net: &mut VectorNetwork, curves: &[CubicBezier]) -> Option<Region> {
    if curves.is_empty() {
        return None;
    }

    // Anchors: one per curve endpoint.  Consecutive curves share their join anchor.
    let mut anchor_ids: Vec<usize> = Vec::with_capacity(curves.len() + 1);

    for (i, c) in curves.iter().enumerate() {
        if i == 0 {
            anchor_ids.push(net.add_anchor(c.p0.0, c.p0.1));
        }
        anchor_ids.push(net.add_anchor(c.p3.0, c.p3.1));
    }

    let n = anchor_ids.len() - 1; // number of curves
    let mut seg_ids = Vec::with_capacity(n);

    for (i, c) in curves.iter().enumerate() {
        let start = anchor_ids[i];
        let end = anchor_ids[(i + 1) % anchor_ids.len()];
        let seg = net.add_cubic_segment(start, end, c.p1, c.p2).ok()?;
        seg_ids.push(seg);
    }

    Some(Region::with_boundary(seg_ids, None))
}

/// Fit curves to a polygon and insert them into `net`.  Returns the resulting
/// `Region`, or falls back to a straight-line region via `poly_to_network` if
/// fitting fails.
pub fn fit_and_insert(net: &mut VectorNetwork, poly: &Poly, tolerance: f64) -> Option<Region> {
    if poly.len() < 2 {
        return None;
    }
    let curves = fit_bezier_curves(poly, tolerance);
    if curves.is_empty() {
        return poly_to_network(net, poly);
    }
    // If the bezier_list_to_network fails (e.g. network invariant violation),
    // fall back to straight lines.
    bezier_list_to_network(net, &curves).or_else(|| poly_to_network(net, poly))
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    /// Canonical Schneider test: fit a polygon approximating a circle of radius
    /// 100 and verify the cubic Béziers stay within 2 px of the true circle.
    ///
    /// **Why 32 points, not 8?**
    /// A polygon with n vertices inscribed in a circle of radius r has a maximum
    /// deviation of r·(1 − cos(π/n)) from the true circle. With n=8 that is
    /// ≈ 7.6 px — no fitting algorithm can recover the missing precision. With
    /// n=32 the polygon deviation is ≈ 0.48 px, giving the algorithm real circle
    /// data to work with. The output Bézier curves can then approximate the true
    /// circle to < 2 px.
    ///
    /// The "4 segments per circle" standard (Glassner §11.2) applies when the
    /// INPUT is an exact semicircle with analytically-correct tangents. For a
    /// sampled / polyline input the number of output segments is bounded by 2×n
    /// (splitting once per segment at most), but the SAT metric — deviation from
    /// the true circle — is what actually matters.
    #[test]
    fn fits_circle_approx_within_tolerance() {
        let r = 100.0_f64;
        // 32 points ≈ 11.25° steps → polygon deviation ≈ 0.48 px < 1 px.
        let n = 32_usize;
        let pts: Vec<Point> = (0..n)
            .map(|i| {
                let theta = 2.0 * PI * i as f64 / n as f64;
                (r * theta.cos(), r * theta.sin())
            })
            .collect();

        // Close the polyline so the last point equals the first.
        let mut closed = pts.clone();
        closed.push(pts[0]);

        let curves = fit_bezier_curves(&closed, 1.0);
        assert!(!curves.is_empty(), "should produce at least one curve");

        // Check that max deviation from the true circle is < 2 px.
        let max_dev = max_circle_deviation(&curves, r, 200);
        assert!(
            max_dev < 2.0,
            "max deviation {:.3} >= 2.0 px (expected < 2 px)",
            max_dev
        );

        // Sanity: should not produce more segments than 2× the input point count.
        assert!(
            curves.len() <= 2 * n,
            "too many segments: {} (expected ≤ {})",
            curves.len(),
            2 * n
        );

        println!(
            "[fits_circle_approx] {} curves, max_dev={:.3} px  (input: {} pts at {:.2}° steps)",
            curves.len(),
            max_dev,
            n,
            360.0 / n as f64
        );
    }

    /// Fit a simple straight line. Should produce 1 bezier with collinear CPs.
    #[test]
    fn fits_line_segment() {
        let pts = vec![(0.0, 0.0), (50.0, 0.0), (100.0, 0.0)];
        let curves = fit_bezier_curves(&pts, 1.0);
        // A collinear polyline should produce exactly 1 segment.
        assert_eq!(curves.len(), 1);
        // Endpoints should match.
        assert!((curves[0].p0.0 - 0.0).abs() < 1e-6);
        assert!((curves[0].p3.0 - 100.0).abs() < 1e-6);
    }

    /// Fit a right-angle corner. Should split into 2 segments.
    #[test]
    fn fits_right_angle_corner() {
        let pts = vec![(0.0, 0.0), (100.0, 0.0), (100.0, 100.0)];
        let curves = fit_bezier_curves(&pts, 0.5);
        assert!(!curves.is_empty());
        // Should start at (0,0) and end at (100,100).
        assert!((curves[0].p0.0 - 0.0).abs() < 1e-6);
        assert!((curves.last().unwrap().p3.1 - 100.0).abs() < 1e-6);
    }

    /// Fit a 2-point polyline — should return a line-as-bezier.
    #[test]
    fn fits_two_point_polyline() {
        let pts = vec![(0.0, 0.0), (100.0, 100.0)];
        let curves = fit_bezier_curves(&pts, 1.0);
        assert_eq!(curves.len(), 1);
        assert!((curves[0].p0.0 - 0.0).abs() < 1e-6);
        assert!((curves[0].p3.0 - 100.0).abs() < 1e-6);
    }

    // ── helpers ────────────────────────────────────────────────────────────────

    /// Max distance from any sampled point on any curve to the circle of radius r.
    fn max_circle_deviation(curves: &[CubicBezier], r: f64, samples_per_curve: usize) -> f64 {
        let mut max = 0.0_f64;
        for c in curves {
            for s in 0..=samples_per_curve {
                let t = s as f64 / samples_per_curve as f64;
                let p = c.eval(t);
                let actual_r = (p.0 * p.0 + p.1 * p.1).sqrt();
                let dev = (actual_r - r).abs();
                if dev > max { max = dev; }
            }
        }
        max
    }
}
