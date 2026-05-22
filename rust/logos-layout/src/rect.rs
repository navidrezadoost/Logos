//! Axis-aligned bounding rectangle.
//!
//! Faithful Rust port of `common/src/app/common/geom/rect.cljc`.
//!
//! # Coordinate system
//! The coordinate origin is the top-left corner, positive Y points downward
//! (same as SVG and the browser canvas).  All four fields are stored; `x2`
//! and `y2` are always recomputed as `x + width` / `y + height` and are NOT
//! stored — use [`Rect::x2`] / [`Rect::y2`] accessors.
//!
//! # Example
//! ```
//! use logos_layout::rect::Rect;
//! use logos_layout::point::Point;
//!
//! let r = Rect::new(0.0, 0.0, 100.0, 50.0);
//! assert_eq!(r.center(), Point::new(50.0, 25.0));
//! assert!(r.contains_point(Point::new(50.0, 25.0)));
//! ```

use crate::point::Point;

/// An axis-aligned bounding rectangle `{ x, y, width, height }`.
///
/// All dimensions are in the same coordinate space as [`Point`].
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(C)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

// ─────────────────────────────────────────────────────────────────
// Constructors
// ─────────────────────────────────────────────────────────────────

impl Rect {
    /// Direct constructor.
    /// Clojure: `(make-rect x y width height)`
    #[inline]
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Rect { x, y, width, height }
    }

    /// The zero rect: `{ 0, 0, 0, 0 }`.
    #[inline]
    pub fn zero() -> Self {
        Rect::new(0.0, 0.0, 0.0, 0.0)
    }

    /// Build a Rect from any two corner points.
    /// The resulting rect always has positive width and height.
    /// Clojure: `(make-rect p1 p2)`
    pub fn from_points(p1: Point, p2: Point) -> Self {
        let x = p1.x.min(p2.x);
        let y = p1.y.min(p2.y);
        let w = (p1.x - p2.x).abs();
        let h = (p1.y - p2.y).abs();
        Rect::new(x, y, w, h)
    }

    /// Build the minimum bounding rect over a slice of points.
    /// Returns `None` on empty input.
    /// Clojure: `(points->rect points)`
    pub fn from_points_iter(pts: &[Point]) -> Option<Self> {
        let mut iter = pts.iter();
        let first = iter.next()?;
        let (mut min_x, mut min_y) = (first.x, first.y);
        let (mut max_x, mut max_y) = (first.x, first.y);
        for p in iter {
            if p.x < min_x { min_x = p.x; }
            if p.y < min_y { min_y = p.y; }
            if p.x > max_x { max_x = p.x; }
            if p.y > max_y { max_y = p.y; }
        }
        Some(Rect::new(min_x, min_y, max_x - min_x, max_y - min_y))
    }

    /// Build from `(x1, y1, x2, y2)` (second corner, not width/height).
    /// Clojure: `(make-rect-from-points x1 y1 x2 y2)`
    #[inline]
    pub fn from_corners(x1: f64, y1: f64, x2: f64, y2: f64) -> Self {
        Rect::new(
            x1.min(x2),
            y1.min(y2),
            (x2 - x1).abs(),
            (y2 - y1).abs(),
        )
    }
}

// ─────────────────────────────────────────────────────────────────
// Derived accessors
// ─────────────────────────────────────────────────────────────────

impl Rect {
    /// Right edge: `x + width`.
    /// Clojure: `:x2` / `(+ x width)`
    #[inline]
    pub fn x2(&self) -> f64 {
        self.x + self.width
    }

    /// Bottom edge: `y + height`.
    /// Clojure: `:y2` / `(+ y height)`
    #[inline]
    pub fn y2(&self) -> f64 {
        self.y + self.height
    }

    /// Center point.
    /// Clojure: `(center-rect r)`
    #[inline]
    pub fn center(&self) -> Point {
        Point::new(self.x + self.width * 0.5, self.y + self.height * 0.5)
    }

    /// Area: `width * height`.
    #[inline]
    pub fn area(&self) -> f64 {
        self.width * self.height
    }

    /// `true` if the rect has zero or negative area (degenerate).
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.width <= 0.0 || self.height <= 0.0
    }

    /// The four corner points in order: TL, TR, BR, BL.
    /// Clojure: `(rect->points r)`
    pub fn to_points(&self) -> [Point; 4] {
        [
            Point::new(self.x,          self.y),
            Point::new(self.x2(),       self.y),
            Point::new(self.x2(),       self.y2()),
            Point::new(self.x,          self.y2()),
        ]
    }
}

// ─────────────────────────────────────────────────────────────────
// Set operations
// ─────────────────────────────────────────────────────────────────

impl Rect {
    /// Axis-aligned intersection.
    /// Returns `None` if the rects do not overlap.
    /// Clojure: `(intersection r1 r2)`
    pub fn intersection(&self, other: &Rect) -> Option<Rect> {
        let ix = self.x.max(other.x);
        let iy = self.y.max(other.y);
        let ix2 = self.x2().min(other.x2());
        let iy2 = self.y2().min(other.y2());
        if ix2 > ix && iy2 > iy {
            Some(Rect::new(ix, iy, ix2 - ix, iy2 - iy))
        } else {
            None
        }
    }

    /// Minimum bounding rect enclosing both rects.
    /// Clojure: `(join r1 r2)` / `(merge-rects r1 r2)`
    pub fn union(&self, other: &Rect) -> Rect {
        let x  = self.x.min(other.x);
        let y  = self.y.min(other.y);
        let x2 = self.x2().max(other.x2());
        let y2 = self.y2().max(other.y2());
        Rect::new(x, y, x2 - x, y2 - y)
    }

    /// Minimum bounding rect over a slice of rects.  Returns `None` on empty.
    /// Clojure: `(join-rects rects)`
    pub fn union_all(rects: &[Rect]) -> Option<Rect> {
        let mut iter = rects.iter();
        let first = *iter.next()?;
        Some(iter.fold(first, |acc, r| acc.union(r)))
    }
}

// ─────────────────────────────────────────────────────────────────
// Containment & overlap predicates
// ─────────────────────────────────────────────────────────────────

impl Rect {
    /// `true` if `pt` lies strictly inside or exactly on the border.
    /// Clojure: `(contains-point? r pt)`
    #[inline]
    pub fn contains_point(&self, pt: Point) -> bool {
        pt.x >= self.x && pt.x <= self.x2()
            && pt.y >= self.y && pt.y <= self.y2()
    }

    /// `true` if `other` is fully contained inside `self`.
    /// Clojure: `(contains-rect? outer inner)`
    #[inline]
    pub fn contains_rect(&self, other: &Rect) -> bool {
        other.x >= self.x && other.x2() <= self.x2()
            && other.y >= self.y && other.y2() <= self.y2()
    }

    /// `true` if the two rects overlap (share at least one point).
    /// Clojure: `(overlaps? r1 r2)`
    #[inline]
    pub fn overlaps(&self, other: &Rect) -> bool {
        self.x <= other.x2() && self.x2() >= other.x
            && self.y <= other.y2() && self.y2() >= other.y
    }
}

// ─────────────────────────────────────────────────────────────────
// Geometric transforms
// ─────────────────────────────────────────────────────────────────

impl Rect {
    /// Expand each side outward by `delta`.
    /// Clojure: `(inflate-rect r delta)`
    #[inline]
    pub fn inflate(&self, delta: f64) -> Rect {
        Rect::new(
            self.x - delta,
            self.y - delta,
            self.width + delta * 2.0,
            self.height + delta * 2.0,
        )
    }

    /// Expand each side by independent amounts.
    /// Clojure: `(inflate-rect r dx dy)`
    #[inline]
    pub fn inflate_xy(&self, dx: f64, dy: f64) -> Rect {
        Rect::new(
            self.x - dx,
            self.y - dy,
            self.width + dx * 2.0,
            self.height + dy * 2.0,
        )
    }

    /// Translate the rect (no size change).
    /// Clojure: `(move-rect r dx dy)`
    #[inline]
    pub fn translate(&self, dx: f64, dy: f64) -> Rect {
        Rect::new(self.x + dx, self.y + dy, self.width, self.height)
    }

    /// Scale the rect uniformly around the origin.
    /// Clojure: `(scale-rect r ratio)`
    #[inline]
    pub fn scale(&self, factor: f64) -> Rect {
        Rect::new(
            self.x * factor,
            self.y * factor,
            self.width * factor,
            self.height * factor,
        )
    }

    /// Apply a full 2D affine matrix to all four corners and recompute the
    /// axis-aligned bounding box.
    /// Clojure: `(transform-rect r matrix)`
    pub fn transform(&self, m: &crate::matrix::Matrix) -> Rect {
        let pts = self.to_points();
        let transformed: Vec<Point> = pts.iter().map(|p| m.transform_point(*p)).collect();
        Rect::from_points_iter(&transformed)
            .unwrap_or(Rect::zero())
    }

    /// Round all four components to `decimals` decimal places.
    /// Clojure: `(round-rect r)`
    pub fn round(&self, decimals: i32) -> Rect {
        let s = 10_f64.powi(decimals);
        Rect::new(
            (self.x * s).round() / s,
            (self.y * s).round() / s,
            (self.width  * s).round() / s,
            (self.height * s).round() / s,
        )
    }

    /// `true` if both rects are within `eps` of each other in all four fields.
    pub fn close_to(&self, other: &Rect, eps: f64) -> bool {
        (self.x - other.x).abs() <= eps
            && (self.y - other.y).abs() <= eps
            && (self.width  - other.width).abs()  <= eps
            && (self.height - other.height).abs() <= eps
    }
}

// ─────────────────────────────────────────────────────────────────
// C-ABI exports
// ─────────────────────────────────────────────────────────────────

/// Compute the axis-aligned bounding box of four corner points.
///
/// `pts` must point to exactly 8 f64 values: `[x0,y0, x1,y1, x2,y2, x3,y3]`.
/// Result is written into `*out` as `[x, y, width, height]`.
#[no_mangle]
pub unsafe extern "C" fn logos_rect_from_points(pts: *const f64, out: *mut [f64; 4]) {
    let corners = [
        Point::new(*pts.add(0), *pts.add(1)),
        Point::new(*pts.add(2), *pts.add(3)),
        Point::new(*pts.add(4), *pts.add(5)),
        Point::new(*pts.add(6), *pts.add(7)),
    ];
    let r = Rect::from_points_iter(&corners).unwrap_or(Rect::zero());
    *out = [r.x, r.y, r.width, r.height];
}

/// Union of two rects `[x,y,w,h]` written into `*out`.
#[no_mangle]
pub unsafe extern "C" fn logos_rect_union(
    ax: f64, ay: f64, aw: f64, ah: f64,
    bx: f64, by: f64, bw: f64, bh: f64,
    out: *mut [f64; 4],
) {
    let r = Rect::new(ax, ay, aw, ah).union(&Rect::new(bx, by, bw, bh));
    *out = [r.x, r.y, r.width, r.height];
}

// ─────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    const EPS: f64 = 1e-10;

    // ── construction ─────────────────────────────────────────────

    #[test]
    fn from_points_gives_correct_extent() {
        let r = Rect::from_points(Point::new(3.0, 7.0), Point::new(1.0, 2.0));
        assert_abs_diff_eq!(r.x, 1.0, epsilon = EPS);
        assert_abs_diff_eq!(r.y, 2.0, epsilon = EPS);
        assert_abs_diff_eq!(r.width,  2.0, epsilon = EPS);
        assert_abs_diff_eq!(r.height, 5.0, epsilon = EPS);
    }

    #[test]
    fn from_points_iter_single_point_is_zero_rect() {
        let r = Rect::from_points_iter(&[Point::new(5.0, 3.0)]).unwrap();
        assert_abs_diff_eq!(r.width,  0.0, epsilon = EPS);
        assert_abs_diff_eq!(r.height, 0.0, epsilon = EPS);
    }

    #[test]
    fn from_points_iter_none_on_empty() {
        assert!(Rect::from_points_iter(&[]).is_none());
    }

    // ── accessors ────────────────────────────────────────────────

    #[test]
    fn x2_and_y2_are_right_and_bottom() {
        let r = Rect::new(10.0, 20.0, 30.0, 40.0);
        assert_abs_diff_eq!(r.x2(), 40.0, epsilon = EPS);
        assert_abs_diff_eq!(r.y2(), 60.0, epsilon = EPS);
    }

    #[test]
    fn center_is_midpoint() {
        let r = Rect::new(0.0, 0.0, 100.0, 50.0);
        let c = r.center();
        assert_abs_diff_eq!(c.x, 50.0, epsilon = EPS);
        assert_abs_diff_eq!(c.y, 25.0, epsilon = EPS);
    }

    #[test]
    fn to_points_returns_four_corners() {
        let r = Rect::new(1.0, 2.0, 3.0, 4.0);
        let pts = r.to_points();
        // TL
        assert_eq!(pts[0], Point::new(1.0, 2.0));
        // TR
        assert_eq!(pts[1], Point::new(4.0, 2.0));
        // BR
        assert_eq!(pts[2], Point::new(4.0, 6.0));
        // BL
        assert_eq!(pts[3], Point::new(1.0, 6.0));
    }

    // ── intersection ─────────────────────────────────────────────

    #[test]
    fn intersection_of_overlapping_rects() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(5.0, 5.0, 10.0, 10.0);
        let i = a.intersection(&b).unwrap();
        assert_abs_diff_eq!(i.x, 5.0, epsilon = EPS);
        assert_abs_diff_eq!(i.y, 5.0, epsilon = EPS);
        assert_abs_diff_eq!(i.width,  5.0, epsilon = EPS);
        assert_abs_diff_eq!(i.height, 5.0, epsilon = EPS);
    }

    #[test]
    fn intersection_of_non_overlapping_rects_is_none() {
        let a = Rect::new(0.0, 0.0, 5.0, 5.0);
        let b = Rect::new(10.0, 10.0, 5.0, 5.0);
        assert!(a.intersection(&b).is_none());
    }

    // ── union ────────────────────────────────────────────────────

    #[test]
    fn union_encloses_both() {
        let a = Rect::new(0.0, 0.0, 5.0, 5.0);
        let b = Rect::new(3.0, 3.0, 5.0, 5.0);
        let u = a.union(&b);
        assert_abs_diff_eq!(u.x, 0.0, epsilon = EPS);
        assert_abs_diff_eq!(u.y, 0.0, epsilon = EPS);
        assert_abs_diff_eq!(u.x2(), 8.0, epsilon = EPS);
        assert_abs_diff_eq!(u.y2(), 8.0, epsilon = EPS);
    }

    // ── containment ──────────────────────────────────────────────

    #[test]
    fn contains_point_inside() {
        let r = Rect::new(0.0, 0.0, 10.0, 10.0);
        assert!(r.contains_point(Point::new(5.0, 5.0)));
    }

    #[test]
    fn does_not_contain_point_outside() {
        let r = Rect::new(0.0, 0.0, 10.0, 10.0);
        assert!(!r.contains_point(Point::new(11.0, 5.0)));
    }

    #[test]
    fn contains_rect_true_for_inner() {
        let outer = Rect::new(0.0, 0.0, 100.0, 100.0);
        let inner = Rect::new(10.0, 10.0, 20.0, 20.0);
        assert!(outer.contains_rect(&inner));
    }

    #[test]
    fn overlaps_true_for_partial_overlap() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(5.0, 5.0, 10.0, 10.0);
        assert!(a.overlaps(&b));
    }

    // ── inflate ──────────────────────────────────────────────────

    #[test]
    fn inflate_increases_all_sides() {
        let r = Rect::new(10.0, 10.0, 80.0, 60.0).inflate(5.0);
        assert_abs_diff_eq!(r.x, 5.0, epsilon = EPS);
        assert_abs_diff_eq!(r.y, 5.0, epsilon = EPS);
        assert_abs_diff_eq!(r.width,  90.0, epsilon = EPS);
        assert_abs_diff_eq!(r.height, 70.0, epsilon = EPS);
    }

    // ── transform ────────────────────────────────────────────────

    #[test]
    fn transform_with_identity_is_noop() {
        use crate::matrix::Matrix;
        let r = Rect::new(5.0, 10.0, 20.0, 30.0);
        let r2 = r.transform(&Matrix::identity());
        assert!(r.close_to(&r2, 1e-9));
    }

    #[test]
    fn transform_translate_moves_rect() {
        use crate::matrix::Matrix;
        let r = Rect::new(0.0, 0.0, 10.0, 10.0);
        let r2 = r.transform(&Matrix::translate_dist(5.0, 7.0));
        assert_abs_diff_eq!(r2.x, 5.0, epsilon = EPS);
        assert_abs_diff_eq!(r2.y, 7.0, epsilon = EPS);
        assert_abs_diff_eq!(r2.width,  10.0, epsilon = EPS);
        assert_abs_diff_eq!(r2.height, 10.0, epsilon = EPS);
    }

    #[test]
    fn transform_rotate_90_square_keeps_size() {
        use crate::matrix::Matrix;
        // A square rotated 90° around its own center stays the same size
        let r = Rect::new(0.0, 0.0, 10.0, 10.0);
        let center = r.center();
        let m = Matrix::rotate_matrix_center(90.0, center.x, center.y);
        let r2 = r.transform(&m);
        assert_abs_diff_eq!(r2.width,  10.0, epsilon = 1e-9);
        assert_abs_diff_eq!(r2.height, 10.0, epsilon = 1e-9);
    }
}
