//! 2D affine matrix — column-major 3×3.
//!
//! Faithful Rust port of `common/src/app/common/geom/matrix.cljc`.
//!
//! # Layout convention
//! The matrix is stored as six scalars `[a, b, c, d, e, f]` that represent
//! the 3×3 column-major affine matrix:
//!
//! ```text
//! | a  c  e |   | x |
//! | b  d  f | × | y |
//! | 0  0  1 |   | 1 |
//! ```
//!
//! This matches the CSS/SVG `matrix(a,b,c,d,e,f)` convention and the Clojure
//! record fields.
//!
//! # Example
//! ```
//! use logos_layout::matrix::Matrix;
//! use logos_layout::point::Point;
//!
//! let m = Matrix::rotate_matrix(90.0);
//! let p = Point::new(1.0, 0.0);
//! let q = m.transform_point(p);
//! assert!((q.x).abs() < 1e-10);
//! assert!((q.y - 1.0).abs() < 1e-10);
//! ```

use crate::point::Point;
use std::fmt;
use std::ops::Mul;

/// Threshold for "almost zero" comparisons.
const EPSILON: f64 = 1e-6;

// ─────────────────────────────────────────────────────────────────
// Struct
// ─────────────────────────────────────────────────────────────────

/// A 2D column-major affine matrix `[a, b, c, d, e, f]`.
///
/// Stored flat; the translation component lives in `(e, f)`.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Matrix {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    /// X-translation
    pub e: f64,
    /// Y-translation
    pub f: f64,
}

/// Components extracted by [`Matrix::decompose`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Decomposed {
    /// Translation `(tx, ty)`.
    pub translate: Point,
    /// Counter-clockwise rotation in **degrees**.
    pub rotation: f64,
    /// Non-uniform scale `(sx, sy)`.  Negative values indicate a flip.
    pub scale: Point,
    /// Skew along the X-axis in **degrees**.
    pub skew_x: f64,
}

// ─────────────────────────────────────────────────────────────────
// Constructors
// ─────────────────────────────────────────────────────────────────

impl Matrix {
    /// Direct constructor: `matrix(a, b, c, d, e, f)`.
    /// Clojure: `(matrix a b c d e f)`
    #[inline]
    pub fn new(a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) -> Self {
        Self { a, b, c, d, e, f }
    }

    /// Identity matrix.
    /// Clojure: `(matrix)` / `(matrix 1 0 0 1 0 0)`
    #[inline]
    pub fn identity() -> Self {
        Self::new(1.0, 0.0, 0.0, 1.0, 0.0, 0.0)
    }

    // ── Primitive matrix factories ───────────────────────────────

    /// Translation-only matrix for point `pt`.
    /// Clojure: `(translate-matrix pt)`
    #[inline]
    pub fn translate_matrix(pt: Point) -> Self {
        Self::new(1.0, 0.0, 0.0, 1.0, pt.x, pt.y)
    }

    /// Translation-only matrix for explicit `(x, y)`.
    /// Clojure: `(translate-matrix x y)`
    #[inline]
    pub fn translate_dist(x: f64, y: f64) -> Self {
        Self::new(1.0, 0.0, 0.0, 1.0, x, y)
    }

    /// Negative-translation matrix (convenience for `translate(-pt.x, -pt.y)`).
    /// Clojure: `(translate-matrix-neg pt)`
    #[inline]
    pub fn translate_neg(pt: Point) -> Self {
        Self::new(1.0, 0.0, 0.0, 1.0, -pt.x, -pt.y)
    }

    /// Scale-only matrix (no center).
    /// Clojure: `(scale-matrix pt)` (1-arg)
    #[inline]
    pub fn scale_matrix(pt: Point) -> Self {
        Self::new(pt.x, 0.0, 0.0, pt.y, 0.0, 0.0)
    }

    /// Uniform scale around a center point.
    /// Clojure: `(scale-matrix pt center)`
    #[inline]
    pub fn scale_matrix_center(pt: Point, center: Point) -> Self {
        let sx = pt.x;
        let sy = pt.y;
        let cx = center.x;
        let cy = center.y;
        Self::new(sx, 0.0, 0.0, sy, cx - cx * sx, cy - cy * sy)
    }

    /// Pure rotation matrix (angle in **degrees**, CCW).
    /// Clojure: `(rotate-matrix angle)`
    pub fn rotate_matrix(angle_deg: f64) -> Self {
        let rad = angle_deg.to_radians();
        let c = rad.cos();
        let s = rad.sin();
        Self::new(c, s, -s, c, 0.0, 0.0)
    }

    /// Rotation matrix around an arbitrary center point.
    /// Clojure: `(rotate-matrix angle center)`
    pub fn rotate_matrix_center(angle_deg: f64, cx: f64, cy: f64) -> Self {
        let rad = angle_deg.to_radians();
        let c = rad.cos();
        let s = rad.sin();
        let nx = -cx;
        let ny = -cy;
        let tx = c * nx + (-s) * ny + cx;
        let ty = s * nx + c * ny + cy;
        Self::new(c, s, -s, c, tx, ty)
    }

    /// Skew matrix from two skew angles in **degrees**.
    /// Clojure: `(skew-matrix angle-x angle-y)`
    ///
    /// `[1, tan(ay), tan(ax), 1, 0, 0]`
    pub fn skew_matrix(angle_x_deg: f64, angle_y_deg: f64) -> Self {
        let mx = angle_x_deg.to_radians().tan();
        let my = angle_y_deg.to_radians().tan();
        Self::new(1.0, my, mx, 1.0, 0.0, 0.0)
    }

    /// Skew matrix around a center point.
    /// Clojure: `(skew-matrix angle-x angle-y point)`
    pub fn skew_matrix_center(angle_x_deg: f64, angle_y_deg: f64, center: Point) -> Self {
        Self::translate_matrix(center)
            * Self::skew_matrix(angle_x_deg, angle_y_deg)
            * Self::translate_neg(center)
    }
}

// ─────────────────────────────────────────────────────────────────
// Builder-style transform application
// ─────────────────────────────────────────────────────────────────

impl Matrix {
    /// Apply a translation by `pt` to `self`: `self * T(pt)`.
    /// Clojure: `(translate m pt)`
    #[inline]
    pub fn translate(self, pt: Point) -> Self {
        self * Self::translate_matrix(pt)
    }

    /// Apply a scale by `pt` to `self`: `self * S(pt)`.
    /// Clojure: `(scale m scale)`
    #[inline]
    pub fn scale(self, pt: Point) -> Self {
        self * Self::scale_matrix(pt)
    }

    /// Apply a scale by `pt` around `center` to `self`.
    /// Clojure: `(scale m scale center)`
    #[inline]
    pub fn scale_center(self, pt: Point, center: Point) -> Self {
        self * Self::scale_matrix_center(pt, center)
    }

    /// Apply a rotation (degrees) to `self`: `self * R(angle)`.
    /// Clojure: `(rotate m angle)`
    #[inline]
    pub fn rotate(self, angle_deg: f64) -> Self {
        self * Self::rotate_matrix(angle_deg)
    }

    /// Apply a rotation around a center to `self`.
    /// Clojure: `(rotate m angle center)`
    #[inline]
    pub fn rotate_center(self, angle_deg: f64, cx: f64, cy: f64) -> Self {
        self * Self::rotate_matrix_center(angle_deg, cx, cy)
    }

    /// Apply a skew (degrees) to `self`.
    /// Clojure: `(skew m angle-x angle-y)`
    #[inline]
    pub fn skew(self, angle_x_deg: f64, angle_y_deg: f64) -> Self {
        self * Self::skew_matrix(angle_x_deg, angle_y_deg)
    }

    /// Apply an in-place transform: `T(center) * self * T(-center)`.
    /// Clojure: `(transform-in pt mtx)`
    pub fn transform_in(self, center: Point) -> Self {
        Self::translate_matrix(center) * self * Self::translate_neg(center)
    }
}

// ─────────────────────────────────────────────────────────────────
// Matrix arithmetic
// ─────────────────────────────────────────────────────────────────

/// Matrix–matrix multiplication.
/// Clojure: `(multiply m1 m2)`
///
/// Note: follows standard column-major affine composition.
impl Mul for Matrix {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self {
        let (m1a, m1b, m1c, m1d, m1e, m1f) = (self.a, self.b, self.c, self.d, self.e, self.f);
        let (m2a, m2b, m2c, m2d, m2e, m2f) = (rhs.a, rhs.b, rhs.c, rhs.d, rhs.e, rhs.f);
        Self::new(
            m1a * m2a + m1c * m2b,
            m1b * m2a + m1d * m2b,
            m1a * m2c + m1c * m2d,
            m1b * m2c + m1d * m2d,
            m1a * m2e + m1c * m2f + m1e,
            m1b * m2e + m1d * m2f + m1f,
        )
    }
}

// ─────────────────────────────────────────────────────────────────
// Geometric operations
// ─────────────────────────────────────────────────────────────────

impl Matrix {
    /// Determinant of the linear part `(a*d − c*b)`.
    /// Clojure: `(determinant mtx)`
    #[inline]
    pub fn determinant(&self) -> f64 {
        self.a * self.d - self.c * self.b
    }

    /// Inverse affine matrix, or `None` if the matrix is singular.
    /// Clojure: `(inverse mtx)`
    ///
    /// Uses the 2D affine inverse formula:
    /// ```text
    /// det = a*d - c*b
    /// a' =  d/det
    /// b' = -b/det
    /// c' = -c/det
    /// d' =  a/det
    /// e' =  (c*f - d*e)/det
    /// f' =  (b*e - a*f)/det
    /// ```
    pub fn inverse(&self) -> Option<Self> {
        let det = self.determinant();
        if det.abs() < EPSILON {
            return None;
        }
        Some(Self::new(
            self.d / det,
            -self.b / det,
            -self.c / det,
            self.a / det,
            (self.c * self.f - self.d * self.e) / det,
            (self.b * self.e - self.a * self.f) / det,
        ))
    }

    /// Apply this matrix to a point.
    /// Clojure: `(gpt/transform pt mtx)`
    ///
    /// `x' = a·x + c·y + e`
    /// `y' = b·x + d·y + f`
    #[inline]
    pub fn transform_point(&self, p: Point) -> Point {
        p.transform(self.a, self.b, self.c, self.d, self.e, self.f)
    }

    /// Transform point `pt` around `center`.
    /// Clojure: `(transform-point-center point center matrix)`
    pub fn transform_point_center(&self, pt: Point, center: Point) -> Point {
        let m = Self::translate_matrix(center) * *self * Self::translate_neg(center);
        m.transform_point(pt)
    }

    /// `true` if `self` is within `eps` of another matrix in all six components.
    /// Clojure: `(m-equal m1 m2 threshold)` / `(close? m1 m2)`
    pub fn close_to(&self, other: &Self, eps: f64) -> bool {
        (self.a - other.a).abs() <= eps
            && (self.b - other.b).abs() <= eps
            && (self.c - other.c).abs() <= eps
            && (self.d - other.d).abs() <= eps
            && (self.e - other.e).abs() <= eps
            && (self.f - other.f).abs() <= eps
    }

    /// `true` if `self` is the identity (up to `EPSILON`).
    /// Clojure: `(unit? m1)`
    #[inline]
    pub fn is_identity(&self) -> bool {
        self.close_to(&Self::identity(), EPSILON)
    }

    /// `true` if `self` is a pure translation (linear part equals identity).
    /// Clojure: `(move? m)`
    #[inline]
    pub fn is_translate_only(&self) -> bool {
        (self.a - 1.0).abs() < EPSILON
            && self.b.abs() < EPSILON
            && self.c.abs() < EPSILON
            && (self.d - 1.0).abs() < EPSILON
    }

    /// Round each component to `decimals` decimal places.
    /// Clojure: `(round mtx)`
    pub fn round(&self, decimals: i32) -> Self {
        let s = 10_f64.powi(decimals);
        Self::new(
            (self.a * s).round() / s,
            (self.b * s).round() / s,
            (self.c * s).round() / s,
            (self.d * s).round() / s,
            (self.e * s).round() / s,
            (self.f * s).round() / s,
        )
    }

    // ── Decomposition ────────────────────────────────────────────

    /// Decompose the affine matrix into translation, rotation, scale, and
    /// skew components.
    ///
    /// Uses the standard CSS / SVG polar decomposition algorithm:
    ///
    /// 1. Translation  = `(e, f)`.
    /// 2. Scale X      = `hypot(a, b)`.  Negated if `det < 0`.
    /// 3. Normalise first column: `a' = a/sx`, `b' = b/sx`.
    /// 4. Skew X       = dot(normalised-col1, col2) = `a'*c + b'*d`, then `/= sy`.
    /// 5. Orthogonalise col2: `c'' = c - a'*skew_dot`, `d'' = d - b'*skew_dot`.
    /// 6. Scale Y      = `hypot(c'', d'')` × sign of sub-determinant.
    /// 7. Rotation     = `atan2(b', a')` in **degrees**.
    ///
    /// The returned `skew_x` and `rotation` are in **degrees**.
    pub fn decompose(&self) -> Decomposed {
        let tx = self.e;
        let ty = self.f;

        let mut sx = (self.a * self.a + self.b * self.b).sqrt();
        // Flip scale-x sign if the determinant is negative
        if self.determinant() < 0.0 {
            sx = -sx;
        }

        // Normalised first column
        let (na, nb) = if sx.abs() < EPSILON {
            (0.0, 0.0)
        } else {
            (self.a / sx, self.b / sx)
        };

        // Skew-x dot product (pre-normalisation by sy)
        let skew_dot = na * self.c + nb * self.d;

        // Orthogonalise second column
        let c2 = self.c - na * skew_dot;
        let d2 = self.d - nb * skew_dot;

        let sub_det = na * d2 - nb * c2;
        let sy = (c2 * c2 + d2 * d2).sqrt() * sub_det.signum();
        let skew_x_rad = if sy.abs() < EPSILON { 0.0 } else { skew_dot / sy };

        let rotation_deg = nb.atan2(na).to_degrees();
        let skew_x_deg = skew_x_rad.atan().to_degrees();

        Decomposed {
            translate: Point::new(tx, ty),
            rotation: rotation_deg,
            scale: Point::new(sx, sy),
            skew_x: skew_x_deg,
        }
    }
}

// ─────────────────────────────────────────────────────────────────
// Display
// ─────────────────────────────────────────────────────────────────

impl fmt::Display for Matrix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "matrix({:.6}, {:.6}, {:.6}, {:.6}, {:.6}, {:.6})",
            self.a, self.b, self.c, self.d, self.e, self.f
        )
    }
}

// ─────────────────────────────────────────────────────────────────
// C-ABI exports
// ─────────────────────────────────────────────────────────────────

/// Invert the affine matrix `[a,b,c,d,e,f]`.
///
/// Writes the result into `*out` and returns `true`.
/// Returns `false` (and leaves `*out` unchanged) if the matrix is singular.
#[no_mangle]
pub extern "C" fn logos_matrix_inverse(
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    f: f64,
    out: *mut [f64; 6],
) -> bool {
    let m = Matrix::new(a, b, c, d, e, f);
    match m.inverse() {
        Some(inv) => {
            unsafe {
                *out = [inv.a, inv.b, inv.c, inv.d, inv.e, inv.f];
            }
            true
        }
        None => false,
    }
}

/// Multiply two affine matrices and write the result into `*out`.
#[no_mangle]
pub extern "C" fn logos_matrix_multiply(
    a1: f64, b1: f64, c1: f64, d1: f64, e1: f64, f1: f64,
    a2: f64, b2: f64, c2: f64, d2: f64, e2: f64, f2: f64,
    out: *mut [f64; 6],
) {
    let result = Matrix::new(a1, b1, c1, d1, e1, f1) * Matrix::new(a2, b2, c2, d2, e2, f2);
    unsafe {
        *out = [result.a, result.b, result.c, result.d, result.e, result.f];
    }
}

/// Apply matrix `[a,b,c,d,e,f]` to point `(x, y)`.
///
/// This mirrors `logos_point_transform` but takes a flat matrix array.
#[no_mangle]
pub extern "C" fn logos_matrix_transform_point(
    a: f64, b: f64, c: f64, d: f64, e: f64, f: f64,
    x: f64, y: f64,
    out_x: *mut f64,
    out_y: *mut f64,
) {
    let m = Matrix::new(a, b, c, d, e, f);
    let p = m.transform_point(Point::new(x, y));
    unsafe {
        *out_x = p.x;
        *out_y = p.y;
    }
}

// ─────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    const EPS: f64 = 1e-9;

    fn matrix_close(m1: Matrix, m2: Matrix) {
        assert_abs_diff_eq!(m1.a, m2.a, epsilon = EPS);
        assert_abs_diff_eq!(m1.b, m2.b, epsilon = EPS);
        assert_abs_diff_eq!(m1.c, m2.c, epsilon = EPS);
        assert_abs_diff_eq!(m1.d, m2.d, epsilon = EPS);
        assert_abs_diff_eq!(m1.e, m2.e, epsilon = EPS);
        assert_abs_diff_eq!(m1.f, m2.f, epsilon = EPS);
    }

    // ── Identity ─────────────────────────────────────────────────

    #[test]
    fn identity_is_unit() {
        assert!(Matrix::identity().is_identity());
    }

    #[test]
    fn identity_times_anything_is_anything() {
        let m = Matrix::new(2.0, 3.0, 4.0, 5.0, 10.0, 20.0);
        matrix_close(Matrix::identity() * m, m);
        matrix_close(m * Matrix::identity(), m);
    }

    // ── Multiply ─────────────────────────────────────────────────

    #[test]
    fn multiply_two_translations_adds_offsets() {
        let t1 = Matrix::translate_dist(3.0, 4.0);
        let t2 = Matrix::translate_dist(7.0, -2.0);
        let r = t1 * t2;
        assert_abs_diff_eq!(r.e, 10.0, epsilon = EPS);
        assert_abs_diff_eq!(r.f, 2.0, epsilon = EPS);
    }

    // ── Translate ────────────────────────────────────────────────

    #[test]
    fn translate_then_inverse_is_identity() {
        let m = Matrix::translate_dist(5.0, -3.0);
        let inv = m.inverse().expect("translate is always invertible");
        matrix_close(m * inv, Matrix::identity());
        matrix_close(inv * m, Matrix::identity());
    }

    #[test]
    fn translate_matrix_moves_point() {
        let m = Matrix::translate_dist(10.0, 20.0);
        let p = Point::new(1.0, 2.0);
        let q = m.transform_point(p);
        assert_abs_diff_eq!(q.x, 11.0, epsilon = EPS);
        assert_abs_diff_eq!(q.y, 22.0, epsilon = EPS);
    }

    // ── Scale ────────────────────────────────────────────────────

    #[test]
    fn scale_then_inverse_is_identity() {
        let m = Matrix::scale_matrix(Point::new(2.0, 3.0));
        let inv = m.inverse().expect("non-zero scale is invertible");
        matrix_close(m * inv, Matrix::identity());
    }

    #[test]
    fn scale_2_then_half_is_identity() {
        let s2 = Matrix::scale_matrix(Point::new(2.0, 2.0));
        let sh = Matrix::scale_matrix(Point::new(0.5, 0.5));
        matrix_close(s2 * sh, Matrix::identity());
    }

    #[test]
    fn zero_scale_is_singular() {
        let m = Matrix::scale_matrix(Point::new(0.0, 1.0));
        assert!(m.inverse().is_none());
    }

    // ── Rotate ───────────────────────────────────────────────────

    #[test]
    fn rotate_90_four_times_is_identity() {
        let r90 = Matrix::rotate_matrix(90.0);
        let r360 = r90 * r90 * r90 * r90;
        matrix_close(r360, Matrix::identity());
    }

    #[test]
    fn rotate_then_inverse_is_identity() {
        let r = Matrix::rotate_matrix(37.3);
        let inv = r.inverse().expect("rotation is always invertible");
        matrix_close(r * inv, Matrix::identity());
        matrix_close(inv * r, Matrix::identity());
    }

    #[test]
    fn rotate_90_maps_x_axis_to_y_axis() {
        let m = Matrix::rotate_matrix(90.0);
        let p = m.transform_point(Point::new(1.0, 0.0));
        assert_abs_diff_eq!(p.x, 0.0, epsilon = 1e-10);
        assert_abs_diff_eq!(p.y, 1.0, epsilon = 1e-10);
    }

    #[test]
    fn rotate_180_negates_both_axes() {
        let m = Matrix::rotate_matrix(180.0);
        let p = m.transform_point(Point::new(1.0, 1.0));
        assert_abs_diff_eq!(p.x, -1.0, epsilon = 1e-10);
        assert_abs_diff_eq!(p.y, -1.0, epsilon = 1e-10);
    }

    #[test]
    fn rotate_around_center_leaves_center_fixed() {
        let cx = 5.0;
        let cy = 7.0;
        let m = Matrix::rotate_matrix_center(45.0, cx, cy);
        let center = Point::new(cx, cy);
        let transformed = m.transform_point(center);
        assert_abs_diff_eq!(transformed.x, cx, epsilon = 1e-10);
        assert_abs_diff_eq!(transformed.y, cy, epsilon = 1e-10);
    }

    // ── Skew ─────────────────────────────────────────────────────

    #[test]
    fn skew_45_x_shears_y_axis() {
        let m = Matrix::skew_matrix(45.0, 0.0);
        let p = m.transform_point(Point::new(0.0, 1.0));
        // skew-x 45° → c = tan(45°) ≈ 1.0 → x' = 0 + 1*1 + 0 = 1
        assert_abs_diff_eq!(p.x, 1.0, epsilon = 1e-10);
        assert_abs_diff_eq!(p.y, 1.0, epsilon = 1e-10);
    }

    #[test]
    fn zero_skew_is_identity() {
        matrix_close(Matrix::skew_matrix(0.0, 0.0), Matrix::identity());
    }

    // ── Inverse ──────────────────────────────────────────────────

    #[test]
    fn inverse_of_identity_is_identity() {
        let inv = Matrix::identity().inverse().unwrap();
        matrix_close(inv, Matrix::identity());
    }

    #[test]
    fn inverse_singular_returns_none() {
        // det = a*d - c*b = 0*4 - 2*0 = 0  (degenerate)
        let m = Matrix::new(0.0, 0.0, 0.0, 0.0, 5.0, 5.0);
        assert!(m.inverse().is_none());
    }

    #[test]
    fn inverse_of_general_matrix() {
        let m = Matrix::new(2.0, 1.0, 3.0, 4.0, 5.0, 6.0);
        let inv = m.inverse().unwrap();
        matrix_close(m * inv, Matrix::identity());
        matrix_close(inv * m, Matrix::identity());
    }

    // ── Decompose ────────────────────────────────────────────────

    #[test]
    fn decompose_identity() {
        let d = Matrix::identity().decompose();
        assert_abs_diff_eq!(d.translate.x, 0.0, epsilon = 1e-9);
        assert_abs_diff_eq!(d.translate.y, 0.0, epsilon = 1e-9);
        assert_abs_diff_eq!(d.rotation, 0.0, epsilon = 1e-9);
        assert_abs_diff_eq!(d.scale.x, 1.0, epsilon = 1e-9);
        assert_abs_diff_eq!(d.scale.y, 1.0, epsilon = 1e-9);
        assert_abs_diff_eq!(d.skew_x, 0.0, epsilon = 1e-9);
    }

    #[test]
    fn decompose_pure_translation() {
        let m = Matrix::translate_dist(10.0, -5.0);
        let d = m.decompose();
        assert_abs_diff_eq!(d.translate.x, 10.0, epsilon = 1e-9);
        assert_abs_diff_eq!(d.translate.y, -5.0, epsilon = 1e-9);
        assert_abs_diff_eq!(d.rotation, 0.0, epsilon = 1e-9);
        assert_abs_diff_eq!(d.scale.x, 1.0, epsilon = 1e-9);
        assert_abs_diff_eq!(d.scale.y, 1.0, epsilon = 1e-9);
    }

    #[test]
    fn decompose_pure_scale() {
        let m = Matrix::scale_matrix(Point::new(3.0, 2.0));
        let d = m.decompose();
        assert_abs_diff_eq!(d.translate.x, 0.0, epsilon = 1e-9);
        assert_abs_diff_eq!(d.scale.x, 3.0, epsilon = 1e-9);
        assert_abs_diff_eq!(d.scale.y, 2.0, epsilon = 1e-9);
        assert_abs_diff_eq!(d.rotation, 0.0, epsilon = 1e-9);
    }

    #[test]
    fn decompose_pure_rotation() {
        let angle = 37.0_f64;
        let m = Matrix::rotate_matrix(angle);
        let d = m.decompose();
        assert_abs_diff_eq!(d.rotation, angle, epsilon = 1e-6);
        assert_abs_diff_eq!(d.scale.x, 1.0, epsilon = 1e-9);
        assert_abs_diff_eq!(d.scale.y, 1.0, epsilon = 1e-9);
        assert_abs_diff_eq!(d.translate.x, 0.0, epsilon = 1e-9);
    }

    /// Core invariant: `decompose(T * R * S)` recovers the original T, R, S.
    #[test]
    fn decompose_translate_rotate_scale_recovers_components() {
        let tx = 15.0_f64;
        let ty = -8.0_f64;
        let angle = 47.0_f64;
        let sx = 2.5_f64;
        let sy = 0.75_f64;

        // T * R * S (same order as CSS transforms)
        let m = Matrix::translate_dist(tx, ty)
            * Matrix::rotate_matrix(angle)
            * Matrix::scale_matrix(Point::new(sx, sy));

        let d = m.decompose();

        assert_abs_diff_eq!(d.translate.x, tx, epsilon = 1e-6);
        assert_abs_diff_eq!(d.translate.y, ty, epsilon = 1e-6);
        assert_abs_diff_eq!(d.rotation, angle, epsilon = 1e-6);
        assert_abs_diff_eq!(d.scale.x, sx, epsilon = 1e-6);
        assert_abs_diff_eq!(d.scale.y, sy, epsilon = 1e-6);
        assert_abs_diff_eq!(d.skew_x, 0.0, epsilon = 1e-6);
    }

    #[test]
    fn decompose_negative_scale_det() {
        // Reflect across Y axis: scale(-1, 1)
        let m = Matrix::scale_matrix(Point::new(-1.0, 1.0));
        let d = m.decompose();
        assert_abs_diff_eq!(d.scale.x, -1.0, epsilon = 1e-9);
        assert_abs_diff_eq!(d.scale.y, 1.0, epsilon = 1e-9);
    }

    // ── C-ABI exports ────────────────────────────────────────────

    #[test]
    fn c_inverse_returns_true_for_invertible() {
        let mut out = [0.0_f64; 6];
        let ok = logos_matrix_inverse(2.0, 1.0, 3.0, 4.0, 0.0, 0.0, &mut out);
        assert!(ok);
        // verify by multiplying
        let m = Matrix::new(2.0, 1.0, 3.0, 4.0, 0.0, 0.0);
        let inv = Matrix::new(out[0], out[1], out[2], out[3], out[4], out[5]);
        matrix_close(m * inv, Matrix::identity());
    }

    #[test]
    fn c_inverse_returns_false_for_singular() {
        let mut out = [0.0_f64; 6];
        let ok = logos_matrix_inverse(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, &mut out);
        assert!(!ok);
    }

    #[test]
    fn c_multiply_two_translations() {
        let mut out = [0.0_f64; 6];
        logos_matrix_multiply(
            1.0, 0.0, 0.0, 1.0, 3.0, 4.0,
            1.0, 0.0, 0.0, 1.0, 7.0, -2.0,
            &mut out,
        );
        assert_abs_diff_eq!(out[4], 10.0, epsilon = EPS);
        assert_abs_diff_eq!(out[5], 2.0, epsilon = EPS);
    }

    #[test]
    fn c_transform_point_identity() {
        let (mut ox, mut oy) = (0.0_f64, 0.0_f64);
        logos_matrix_transform_point(
            1.0, 0.0, 0.0, 1.0, 5.0, 7.0,
            2.0, 3.0,
            &mut ox, &mut oy,
        );
        assert_abs_diff_eq!(ox, 7.0, epsilon = EPS);
        assert_abs_diff_eq!(oy, 10.0, epsilon = EPS);
    }

    // ── Predicates ───────────────────────────────────────────────

    #[test]
    fn is_identity_true_for_identity() {
        assert!(Matrix::identity().is_identity());
    }

    #[test]
    fn is_identity_false_for_translate() {
        assert!(!Matrix::translate_dist(1.0, 0.0).is_identity());
    }

    #[test]
    fn is_translate_only_true_for_translate() {
        assert!(Matrix::translate_dist(100.0, -50.0).is_translate_only());
    }

    #[test]
    fn is_translate_only_false_for_rotate() {
        assert!(!Matrix::rotate_matrix(45.0).is_translate_only());
    }

    // ── Round ────────────────────────────────────────────────────

    #[test]
    fn round_clips_to_4_decimals() {
        let m = Matrix::new(1.123456, 0.0, 0.0, 1.123456, 0.0, 0.0);
        let r = m.round(4);
        assert_abs_diff_eq!(r.a, 1.1235, epsilon = 1e-10);
        assert_abs_diff_eq!(r.d, 1.1235, epsilon = 1e-10);
    }

    // ── transform_in ─────────────────────────────────────────────

    #[test]
    fn transform_in_rotation_leaves_center_fixed() {
        let center = Point::new(10.0, 10.0);
        let rot = Matrix::rotate_matrix(90.0).transform_in(center);
        let c2 = rot.transform_point(center);
        assert_abs_diff_eq!(c2.x, center.x, epsilon = 1e-10);
        assert_abs_diff_eq!(c2.y, center.y, epsilon = 1e-10);
    }

    // ── Display ──────────────────────────────────────────────────

    #[test]
    fn display_matches_css_format() {
        let m = Matrix::identity();
        let s = format!("{m}");
        assert!(s.starts_with("matrix("));
        assert!(s.contains("1.000000"));
    }
}
