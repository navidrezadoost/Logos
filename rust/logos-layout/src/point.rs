//! 2D point / vector arithmetic.
//!
//! This is a faithful Rust port of `common/src/app/common/geom/point.cljc`.
//! All function names follow the original Clojure names, adapted to Rust
//! conventions (`snake_case`, operator-trait impls instead of named functions
//! where idiomatic).
//!
//! # Coordinate system
//! Origin is top-left; Y increases downward (matches browser / canvas conventions).
//!
//! # Example
//! ```
//! use logos_layout::point::Point;
//!
//! let a = Point::new(3.0, 4.0);
//! assert_eq!(a.length(), 5.0);
//!
//! let b = Point::new(1.0, 0.0);
//! let c = a + b;
//! assert_eq!(c, Point::new(4.0, 4.0));
//! ```

use std::ops::{Add, Div, Mul, Neg, Sub};

/// Epsilon used for "almost zero" / "close" comparisons.
pub const EPSILON: f64 = 1e-6;

// ─────────────────────────────────────────────────────────────────
// Struct
// ─────────────────────────────────────────────────────────────────

/// A 2-dimensional point or vector.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

// ─────────────────────────────────────────────────────────────────
// Constructors
// ─────────────────────────────────────────────────────────────────

impl Point {
    /// Create a point from explicit `(x, y)` coordinates.
    #[inline]
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// Origin — `(0, 0)`.
    ///
    /// Corresponds to `(point)` / `(point 0 0)` in Clojure.
    #[inline]
    pub fn zero() -> Self {
        Self { x: 0.0, y: 0.0 }
    }

    /// Create a point with both coordinates set to `v` (splat).
    ///
    /// Corresponds to `(point v)` (number overload) in Clojure.
    #[inline]
    pub fn splat(v: f64) -> Self {
        Self { x: v, y: v }
    }
}

// ─────────────────────────────────────────────────────────────────
// Operator trait impls
// ─────────────────────────────────────────────────────────────────

/// `p1 + p2` — component-wise addition.
/// Clojure: `(add p1 p2)`
impl Add for Point {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

/// `p1 - p2` — component-wise subtraction.
/// Clojure: `(subtract p1 p2)`
impl Sub for Point {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

/// `p1 * p2` — component-wise multiplication (Hadamard product).
/// Clojure: `(multiply p1 p2)`
impl Mul for Point {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: Self) -> Self {
        Self::new(self.x * rhs.x, self.y * rhs.y)
    }
}

/// `p1 / p2` — component-wise division.
/// Clojure: `(divide p1 p2)`
impl Div for Point {
    type Output = Self;
    #[inline]
    fn div(self, rhs: Self) -> Self {
        Self::new(self.x / rhs.x, self.y / rhs.y)
    }
}

/// `-p` — negation.
/// Clojure: `(negate pt)`
impl Neg for Point {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y)
    }
}

// ─────────────────────────────────────────────────────────────────
// Named methods (mirroring the Clojure API)
// ─────────────────────────────────────────────────────────────────

impl Point {
    // ── Arithmetic ──────────────────────────────────────────────

    /// Component-wise addition (named form; prefer `+` operator).
    /// Clojure: `(add p1 p2)`
    #[inline]
    pub fn add(self, rhs: Self) -> Self {
        self + rhs
    }

    /// Component-wise subtraction (named form; prefer `-` operator).
    /// Clojure: `(subtract p1 p2)`
    #[inline]
    pub fn subtract(self, rhs: Self) -> Self {
        self - rhs
    }

    /// Scale by a scalar factor.
    /// Clojure: `(scale p scalar)`
    #[inline]
    pub fn scale(self, scalar: f64) -> Self {
        Self::new(self.x * scalar, self.y * scalar)
    }

    /// Component-wise 1/x inverse.
    /// Clojure: `(inverse pt)`
    #[inline]
    pub fn inverse(self) -> Self {
        Self::new(1.0 / self.x, 1.0 / self.y)
    }

    /// Negate both components.
    /// Clojure: `(negate pt)`
    #[inline]
    pub fn negate(self) -> Self {
        -self
    }

    /// Component-wise minimum.
    /// Clojure: `(min p1 p2)`
    #[inline]
    pub fn min(self, rhs: Self) -> Self {
        Self::new(self.x.min(rhs.x), self.y.min(rhs.y))
    }

    /// Component-wise maximum.
    /// Clojure: `(max p1 p2)`
    #[inline]
    pub fn max(self, rhs: Self) -> Self {
        Self::new(self.x.max(rhs.x), self.y.max(rhs.y))
    }

    // ── Geometry ────────────────────────────────────────────────

    /// Euclidean length (magnitude) of the vector.
    /// Clojure: `(length pt)`
    #[inline]
    pub fn length(self) -> f64 {
        self.x.hypot(self.y)
    }

    /// Euclidean distance between two points.
    /// Clojure: `(distance p1 p2)`
    #[inline]
    pub fn distance(self, other: Self) -> f64 {
        (self - other).length()
    }

    /// Component-wise absolute distance between two points.
    /// Clojure: `(distance-vector p1 p2)`
    #[inline]
    pub fn distance_vector(self, other: Self) -> Self {
        let d = self - other;
        Self::new(d.x.abs(), d.y.abs())
    }

    /// Dot product.
    /// Clojure: `(dot p1 p2)`
    #[inline]
    pub fn dot(self, rhs: Self) -> f64 {
        self.x * rhs.x + self.y * rhs.y
    }

    /// Perpendicular vector (90° counter-clockwise): `(-y, x)`.
    /// Clojure: `(perpendicular pt)`
    #[inline]
    pub fn perpendicular(self) -> Self {
        Self::new(-self.y, self.x)
    }

    /// Unit vector (normalize). Returns `(0, 0)` for the zero vector.
    /// Clojure: `(unit p1)`
    #[inline]
    pub fn unit(self) -> Self {
        let len = self.length();
        if len < EPSILON {
            Self::zero()
        } else {
            self.scale(1.0 / len)
        }
    }

    /// Vector from this point to `other`.
    /// Clojure: `(to-vec p1 p2)`  → `(subtract p2 p1)`
    #[inline]
    pub fn to_vec(self, other: Self) -> Self {
        other - self
    }

    /// Project `self` onto `onto_vec`.
    /// Clojure: `(project v1 v2)`
    #[inline]
    pub fn project(self, onto_vec: Self) -> Self {
        let u = onto_vec.unit();
        u.scale(self.dot(u))
    }

    // ── Angles ──────────────────────────────────────────────────

    /// Angle of the vector relative to the positive x-axis, in **degrees**.
    /// Clojure: `(angle pt)`
    #[inline]
    pub fn angle(self) -> f64 {
        self.y.atan2(self.x).to_degrees()
    }

    /// Angle of `self` relative to `center`, in **degrees**.
    /// Clojure: `(angle pt center)`
    #[inline]
    pub fn angle_from(self, center: Self) -> f64 {
        (self - center).angle()
    }

    /// Smaller unsigned angle between two vectors, in **degrees** `[0, 180]`.
    /// Clojure: `(angle-with-other p1 p2)`
    pub fn angle_with_other(self, other: Self) -> f64 {
        let l1 = self.length();
        let l2 = other.length();
        if l1 < EPSILON || l2 < EPSILON {
            return 0.0;
        }
        let cos_a = (self.dot(other) / (l1 * l2)).clamp(-1.0, 1.0);
        let d = cos_a.acos().to_degrees();
        if d.is_nan() {
            0.0
        } else {
            d
        }
    }

    /// Sign of the angle between two vectors: `+1` or `−1`.
    /// Clojure: `(angle-sign p1 p2)`
    #[inline]
    pub fn angle_sign(self, other: Self) -> f64 {
        if self.y * other.x > self.x * other.y {
            -1.0
        } else {
            1.0
        }
    }

    /// Signed angle between two vectors, in **degrees**.
    /// Clojure: `(signed-angle-with-other v1 v2)`
    #[inline]
    pub fn signed_angle_with_other(self, other: Self) -> f64 {
        self.angle_sign(other) * self.angle_with_other(other)
    }

    /// Update the angle of the vector, preserving its magnitude.
    /// Clojure: `(update-angle p angle-degrees)`
    #[inline]
    pub fn update_angle(self, angle_degrees: f64) -> Self {
        let len = self.length();
        let rad = angle_degrees.to_radians();
        Self::new(rad.cos() * len, rad.sin() * len)
    }

    /// Create a point at `distance` from `self` in the given `angle_degrees`.
    /// Clojure: `(angle->point pt angle distance)`
    #[inline]
    pub fn angle_to_point(self, angle_degrees: f64, distance: f64) -> Self {
        let rad = angle_degrees.to_radians();
        Self::new(
            self.x + distance * rad.cos(),
            self.y - distance * rad.sin(),
        )
    }

    /// Quadrant of the point angle: 1 (++), 2 (-+), 3 (--), 4 (+-).
    /// Clojure: `(quadrant p)`
    #[inline]
    pub fn quadrant(self) -> u8 {
        match (self.x >= 0.0, self.y >= 0.0) {
            (true, true)   => 1,
            (false, true)  => 2,
            (false, false) => 3,
            (true, false)  => 4,
        }
    }

    // ── Rounding ────────────────────────────────────────────────

    /// Round both coordinates to `decimals` decimal places.
    /// `decimals = 0` rounds to integers.
    /// Clojure: `(round pt decimals)`
    pub fn round(self, decimals: i32) -> Self {
        let scale = 10_f64.powi(decimals);
        Self::new(
            (self.x * scale).round() / scale,
            (self.y * scale).round() / scale,
        )
    }

    /// Round both coordinates to the nearest multiple of `step`.
    /// Clojure: `(round-step pt step)`
    #[inline]
    pub fn round_step(self, step: f64) -> Self {
        Self::new(
            (self.x / step).round() * step,
            (self.y / step).round() * step,
        )
    }

    // ── Affine transform ────────────────────────────────────────

    /// Apply a 2D affine matrix `[a b c d e f]` to this point.
    ///
    /// Matrix convention (column-major, matches canvas/SVG/CSS):
    /// ```text
    /// | a  c  e |   | x |
    /// | b  d  f | × | y |
    /// | 0  0  1 |   | 1 |
    /// ```
    /// x' = ax + cy + e
    /// y' = bx + dy + f
    ///
    /// Clojure: `(transform p m)`
    #[inline]
    pub fn transform(self, a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) -> Self {
        Self::new(
            self.x * a + self.y * c + e,
            self.x * b + self.y * d + f,
        )
    }

    // ── Predicates ──────────────────────────────────────────────

    /// `true` if both components are within `EPSILON` of zero.
    #[inline]
    pub fn is_zero(self) -> bool {
        self.x.abs() < EPSILON && self.y.abs() < EPSILON
    }

    /// `true` if `self` and `other` are within `EPSILON` of each other.
    /// Clojure: `(close? p1 p2)`
    #[inline]
    pub fn close(self, other: Self) -> bool {
        (self.x - other.x).abs() < EPSILON && (self.y - other.y).abs() < EPSILON
    }
}

// ─────────────────────────────────────────────────────────────────
// Free functions
// ─────────────────────────────────────────────────────────────────

/// Centroid of a slice of points.
/// Clojure: `(center-points points)`
///
/// Returns `None` for an empty slice.
pub fn center_points(points: &[Point]) -> Option<Point> {
    if points.is_empty() {
        return None;
    }
    let sum = points.iter().fold(Point::zero(), |acc, &p| acc + p);
    Some(sum.scale(1.0 / points.len() as f64))
}

// ─────────────────────────────────────────────────────────────────
// C-ABI exports (native FFI / WASM)
// ─────────────────────────────────────────────────────────────────

/// Add two points and return the result.
///
/// Exported as `logos_point_add` for FFI consumers (JVM JNIF / WASM host).
#[no_mangle]
pub extern "C" fn logos_point_add(ax: f64, ay: f64, bx: f64, by: f64, out_x: *mut f64, out_y: *mut f64) {
    let r = Point::new(ax, ay) + Point::new(bx, by);
    unsafe {
        *out_x = r.x;
        *out_y = r.y;
    }
}

/// Subtract point B from point A.
#[no_mangle]
pub extern "C" fn logos_point_sub(ax: f64, ay: f64, bx: f64, by: f64, out_x: *mut f64, out_y: *mut f64) {
    let r = Point::new(ax, ay) - Point::new(bx, by);
    unsafe {
        *out_x = r.x;
        *out_y = r.y;
    }
}

/// Euclidean distance between two points.
#[no_mangle]
pub extern "C" fn logos_point_distance(ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    Point::new(ax, ay).distance(Point::new(bx, by))
}

/// Length (magnitude) of the vector (ax, ay).
#[no_mangle]
pub extern "C" fn logos_point_length(x: f64, y: f64) -> f64 {
    Point::new(x, y).length()
}

/// Apply affine transform [a b c d e f] to point (x, y).
#[no_mangle]
pub extern "C" fn logos_point_transform(
    x: f64, y: f64,
    a: f64, b: f64, c: f64, d: f64, e: f64, f: f64,
    out_x: *mut f64, out_y: *mut f64,
) {
    let r = Point::new(x, y).transform(a, b, c, d, e, f);
    unsafe {
        *out_x = r.x;
        *out_y = r.y;
    }
}

// ─────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    // ── Constructors ────────────────────────────────────────────

    #[test]
    fn zero_has_both_coordinates_zero() {
        let p = Point::zero();
        assert_eq!(p.x, 0.0);
        assert_eq!(p.y, 0.0);
    }

    #[test]
    fn splat_sets_both_coordinates() {
        let p = Point::splat(7.0);
        assert_eq!(p.x, 7.0);
        assert_eq!(p.y, 7.0);
    }

    // ── Arithmetic operators ─────────────────────────────────────

    #[test]
    fn add_operator() {
        let a = Point::new(1.0, 2.0);
        let b = Point::new(3.0, 4.0);
        assert_eq!(a + b, Point::new(4.0, 6.0));
    }

    #[test]
    fn sub_operator() {
        let a = Point::new(5.0, 6.0);
        let b = Point::new(2.0, 3.0);
        assert_eq!(a - b, Point::new(3.0, 3.0));
    }

    #[test]
    fn mul_operator_hadamard() {
        let a = Point::new(2.0, 3.0);
        let b = Point::new(4.0, 5.0);
        assert_eq!(a * b, Point::new(8.0, 15.0));
    }

    #[test]
    fn div_operator() {
        let a = Point::new(8.0, 9.0);
        let b = Point::new(2.0, 3.0);
        assert_eq!(a / b, Point::new(4.0, 3.0));
    }

    #[test]
    fn neg_operator() {
        let a = Point::new(3.0, -5.0);
        assert_eq!(-a, Point::new(-3.0, 5.0));
    }

    // ── scale ────────────────────────────────────────────────────

    #[test]
    fn scale_by_scalar() {
        let p = Point::new(2.0, 3.0);
        assert_eq!(p.scale(2.0), Point::new(4.0, 6.0));
    }

    // ── min / max ────────────────────────────────────────────────

    #[test]
    fn min_component_wise() {
        let a = Point::new(1.0, 5.0);
        let b = Point::new(3.0, 2.0);
        assert_eq!(a.min(b), Point::new(1.0, 2.0));
    }

    #[test]
    fn max_component_wise() {
        let a = Point::new(1.0, 5.0);
        let b = Point::new(3.0, 2.0);
        assert_eq!(a.max(b), Point::new(3.0, 5.0));
    }

    // ── length / distance ────────────────────────────────────────

    #[test]
    fn length_3_4_is_5() {
        let p = Point::new(3.0, 4.0);
        assert_abs_diff_eq!(p.length(), 5.0, epsilon = 1e-12);
    }

    #[test]
    fn distance_between_two_points() {
        let a = Point::new(0.0, 0.0);
        let b = Point::new(3.0, 4.0);
        assert_abs_diff_eq!(a.distance(b), 5.0, epsilon = 1e-12);
    }

    #[test]
    fn distance_vector_is_component_abs_diff() {
        let a = Point::new(1.0, 5.0);
        let b = Point::new(4.0, 2.0);
        assert_eq!(a.distance_vector(b), Point::new(3.0, 3.0));
    }

    // ── dot / unit ───────────────────────────────────────────────

    #[test]
    fn dot_product() {
        let a = Point::new(1.0, 2.0);
        let b = Point::new(3.0, 4.0);
        assert_abs_diff_eq!(a.dot(b), 11.0, epsilon = 1e-12);
    }

    #[test]
    fn unit_of_3_4_is_normalised() {
        let p = Point::new(3.0, 4.0).unit();
        assert_abs_diff_eq!(p.length(), 1.0, epsilon = 1e-12);
        assert_abs_diff_eq!(p.x, 0.6, epsilon = 1e-12);
        assert_abs_diff_eq!(p.y, 0.8, epsilon = 1e-12);
    }

    #[test]
    fn unit_of_zero_is_zero() {
        assert_eq!(Point::zero().unit(), Point::zero());
    }

    // ── perpendicular ────────────────────────────────────────────

    #[test]
    fn perpendicular_x_axis_is_neg_y_axis() {
        let x = Point::new(1.0, 0.0);
        assert_eq!(x.perpendicular(), Point::new(0.0, 1.0));
    }

    // ── angles ───────────────────────────────────────────────────

    #[test]
    fn angle_of_x_axis_is_zero() {
        assert_abs_diff_eq!(Point::new(1.0, 0.0).angle(), 0.0, epsilon = 1e-12);
    }

    #[test]
    fn angle_of_y_axis_is_90() {
        assert_abs_diff_eq!(Point::new(0.0, 1.0).angle(), 90.0, epsilon = 1e-12);
    }

    #[test]
    fn angle_with_other_perpendicular_is_90() {
        let x = Point::new(1.0, 0.0);
        let y = Point::new(0.0, 1.0);
        assert_abs_diff_eq!(x.angle_with_other(y), 90.0, epsilon = 1e-10);
    }

    #[test]
    fn angle_with_other_parallel_is_0() {
        let a = Point::new(1.0, 0.0);
        let b = Point::new(5.0, 0.0);
        assert_abs_diff_eq!(a.angle_with_other(b), 0.0, epsilon = 1e-10);
    }

    #[test]
    fn angle_with_zero_vector_is_zero() {
        let a = Point::new(1.0, 0.0);
        assert_eq!(a.angle_with_other(Point::zero()), 0.0);
    }

    // ── affine transform ─────────────────────────────────────────

    #[test]
    fn identity_transform_is_noop() {
        // Identity: a=1 b=0 c=0 d=1 e=0 f=0
        let p = Point::new(3.0, 4.0);
        assert_eq!(p.transform(1.0, 0.0, 0.0, 1.0, 0.0, 0.0), p);
    }

    #[test]
    fn translate_by_e_f() {
        // Pure translation: a=1 b=0 c=0 d=1 e=10 f=20
        let p = Point::new(1.0, 2.0);
        assert_eq!(p.transform(1.0, 0.0, 0.0, 1.0, 10.0, 20.0), Point::new(11.0, 22.0));
    }

    #[test]
    fn scale_transform() {
        // Uniform scale 2×: a=2 b=0 c=0 d=2 e=0 f=0
        let p = Point::new(3.0, 4.0);
        assert_eq!(p.transform(2.0, 0.0, 0.0, 2.0, 0.0, 0.0), Point::new(6.0, 8.0));
    }

    #[test]
    fn rotation_90_degrees() {
        // 90° CCW in canvas coords: a=0 b=1 c=-1 d=0 e=0 f=0
        let p = Point::new(1.0, 0.0);
        let rotated = p.transform(0.0, 1.0, -1.0, 0.0, 0.0, 0.0);
        assert_abs_diff_eq!(rotated.x, 0.0, epsilon = 1e-12);
        assert_abs_diff_eq!(rotated.y, 1.0, epsilon = 1e-12);
    }

    // ── round / round_step ───────────────────────────────────────

    #[test]
    fn round_to_integers() {
        let p = Point::new(1.4, 2.6);
        assert_eq!(p.round(0), Point::new(1.0, 3.0));
    }

    #[test]
    fn round_to_one_decimal() {
        let p = Point::new(1.45, 2.64);
        let r = p.round(1);
        assert_abs_diff_eq!(r.x, 1.5, epsilon = 1e-10);
        assert_abs_diff_eq!(r.y, 2.6, epsilon = 1e-10);
    }

    #[test]
    fn round_step_half() {
        let p = Point::new(1.3, 2.7);
        let r = p.round_step(0.5);
        assert_abs_diff_eq!(r.x, 1.5, epsilon = 1e-10);
        assert_abs_diff_eq!(r.y, 2.5, epsilon = 1e-10);
    }

    // ── center_points ────────────────────────────────────────────

    #[test]
    fn center_of_empty_is_none() {
        assert_eq!(super::center_points(&[]), None);
    }

    #[test]
    fn center_of_two_points() {
        let pts = [Point::new(0.0, 0.0), Point::new(4.0, 6.0)];
        let c = super::center_points(&pts).unwrap();
        assert_abs_diff_eq!(c.x, 2.0, epsilon = 1e-12);
        assert_abs_diff_eq!(c.y, 3.0, epsilon = 1e-12);
    }

    #[test]
    fn center_of_four_corners() {
        let pts = [
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(0.0, 10.0),
        ];
        let c = super::center_points(&pts).unwrap();
        assert_abs_diff_eq!(c.x, 5.0, epsilon = 1e-12);
        assert_abs_diff_eq!(c.y, 5.0, epsilon = 1e-12);
    }

    // ── predicates ───────────────────────────────────────────────

    #[test]
    fn is_zero_for_zero_point() {
        assert!(Point::zero().is_zero());
    }

    #[test]
    fn is_zero_false_for_nonzero() {
        assert!(!Point::new(0.0, 1e-5).is_zero());
    }

    #[test]
    fn close_within_epsilon() {
        let a = Point::new(1.0, 2.0);
        let b = Point::new(1.0 + 1e-7, 2.0 - 1e-7);
        assert!(a.close(b));
    }

    #[test]
    fn close_false_for_distant_points() {
        let a = Point::new(0.0, 0.0);
        let b = Point::new(1.0, 0.0);
        assert!(!a.close(b));
    }

    // ── quadrant ─────────────────────────────────────────────────

    #[test]
    fn quadrant_positive_xy_is_1() {
        assert_eq!(Point::new(1.0, 1.0).quadrant(), 1);
    }

    #[test]
    fn quadrant_negative_x_positive_y_is_2() {
        assert_eq!(Point::new(-1.0, 1.0).quadrant(), 2);
    }

    #[test]
    fn quadrant_negative_xy_is_3() {
        assert_eq!(Point::new(-1.0, -1.0).quadrant(), 3);
    }

    #[test]
    fn quadrant_positive_x_negative_y_is_4() {
        assert_eq!(Point::new(1.0, -1.0).quadrant(), 4);
    }

    // ── to_vec / project ─────────────────────────────────────────

    #[test]
    fn to_vec_gives_difference_in_order_other_minus_self() {
        let a = Point::new(1.0, 2.0);
        let b = Point::new(4.0, 6.0);
        assert_eq!(a.to_vec(b), Point::new(3.0, 4.0));
    }

    #[test]
    fn project_onto_x_axis() {
        let v = Point::new(3.0, 4.0);
        let x_axis = Point::new(1.0, 0.0);
        let proj = v.project(x_axis);
        assert_abs_diff_eq!(proj.x, 3.0, epsilon = 1e-12);
        assert_abs_diff_eq!(proj.y, 0.0, epsilon = 1e-12);
    }

    // ── inverse ──────────────────────────────────────────────────

    #[test]
    fn inverse_of_2_4() {
        let p = Point::new(2.0, 4.0);
        let inv = p.inverse();
        assert_abs_diff_eq!(inv.x, 0.5, epsilon = 1e-12);
        assert_abs_diff_eq!(inv.y, 0.25, epsilon = 1e-12);
    }
}
