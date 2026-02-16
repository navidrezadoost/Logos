//! 2D affine transform shared across importers.

use serde::{Deserialize, Serialize};

/// A 2×3 affine transformation matrix.
///
/// ```text
/// | a  c  tx |
/// | b  d  ty |
/// | 0  0   1 |
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Matrix2D {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub tx: f32,
    pub ty: f32,
}

impl Matrix2D {
    pub fn identity() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            tx: 0.0,
            ty: 0.0,
        }
    }

    pub fn translate(x: f32, y: f32) -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            tx: x,
            ty: y,
        }
    }

    pub fn scale(sx: f32, sy: f32) -> Self {
        Self {
            a: sx,
            b: 0.0,
            c: 0.0,
            d: sy,
            tx: 0.0,
            ty: 0.0,
        }
    }

    pub fn rotate(angle_rad: f32) -> Self {
        let (s, c) = angle_rad.sin_cos();
        Self {
            a: c,
            b: s,
            c: -s,
            d: c,
            tx: 0.0,
            ty: 0.0,
        }
    }

    /// Multiply self × other.
    pub fn multiply(&self, other: &Self) -> Self {
        Self {
            a: self.a * other.a + self.c * other.b,
            b: self.b * other.a + self.d * other.b,
            c: self.a * other.c + self.c * other.d,
            d: self.b * other.c + self.d * other.d,
            tx: self.a * other.tx + self.c * other.ty + self.tx,
            ty: self.b * other.tx + self.d * other.ty + self.ty,
        }
    }

    /// Apply this transform to a point.
    pub fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.a * x + self.c * y + self.tx,
            self.b * x + self.d * y + self.ty,
        )
    }

    pub fn is_identity(&self) -> bool {
        (self.a - 1.0).abs() < 1e-6
            && self.b.abs() < 1e-6
            && self.c.abs() < 1e-6
            && (self.d - 1.0).abs() < 1e-6
            && self.tx.abs() < 1e-6
            && self.ty.abs() < 1e-6
    }
}

impl Default for Matrix2D {
    fn default() -> Self {
        Self::identity()
    }
}

/// An axis-aligned bounding box.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BoundingRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl BoundingRect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            x,
            y,
            width: w,
            height: h,
        }
    }

    pub fn zero() -> Self {
        Self::new(0.0, 0.0, 0.0, 0.0)
    }

    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    pub fn center(&self) -> (f32, f32) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    /// Convert to a logos-core Rect.
    pub fn to_core_rect(&self) -> logos_core::Rect {
        logos_core::Rect {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
        }
    }

    /// Compute the union of two bounding rects.
    pub fn union(&self, other: &Self) -> Self {
        let min_x = self.x.min(other.x);
        let min_y = self.y.min(other.y);
        let max_x = self.right().max(other.right());
        let max_y = self.bottom().max(other.bottom());
        Self::new(min_x, min_y, max_x - min_x, max_y - min_y)
    }
}

impl Default for BoundingRect {
    fn default() -> Self {
        Self::zero()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identity() {
        let m = Matrix2D::identity();
        assert!(m.is_identity());
        let (x, y) = m.apply(10.0, 20.0);
        assert!((x - 10.0).abs() < 1e-6);
        assert!((y - 20.0).abs() < 1e-6);
    }

    #[test]
    fn test_translate() {
        let m = Matrix2D::translate(5.0, 10.0);
        let (x, y) = m.apply(1.0, 2.0);
        assert!((x - 6.0).abs() < 1e-6);
        assert!((y - 12.0).abs() < 1e-6);
    }

    #[test]
    fn test_scale() {
        let m = Matrix2D::scale(2.0, 3.0);
        let (x, y) = m.apply(4.0, 5.0);
        assert!((x - 8.0).abs() < 1e-6);
        assert!((y - 15.0).abs() < 1e-6);
    }

    #[test]
    fn test_rotate_90() {
        let m = Matrix2D::rotate(std::f32::consts::FRAC_PI_2);
        let (x, y) = m.apply(1.0, 0.0);
        assert!(x.abs() < 1e-5);
        assert!((y - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_multiply() {
        let t = Matrix2D::translate(10.0, 0.0);
        let s = Matrix2D::scale(2.0, 2.0);
        let m = t.multiply(&s);
        let (x, y) = m.apply(5.0, 3.0);
        assert!((x - 20.0).abs() < 1e-5);
        assert!((y - 6.0).abs() < 1e-5);
    }

    #[test]
    fn test_bounding_rect_union() {
        let a = BoundingRect::new(0.0, 0.0, 10.0, 10.0);
        let b = BoundingRect::new(5.0, 5.0, 20.0, 20.0);
        let u = a.union(&b);
        assert!((u.x - 0.0).abs() < 1e-6);
        assert!((u.y - 0.0).abs() < 1e-6);
        assert!((u.width - 25.0).abs() < 1e-6);
        assert!((u.height - 25.0).abs() < 1e-6);
    }

    #[test]
    fn test_bounding_rect_center() {
        let r = BoundingRect::new(10.0, 20.0, 100.0, 50.0);
        let (cx, cy) = r.center();
        assert!((cx - 60.0).abs() < 1e-6);
        assert!((cy - 45.0).abs() < 1e-6);
    }

    #[test]
    fn test_bounding_rect_to_core() {
        let r = BoundingRect::new(1.0, 2.0, 3.0, 4.0);
        let cr = r.to_core_rect();
        assert_eq!(cr.x, 1.0);
        assert_eq!(cr.y, 2.0);
        assert_eq!(cr.width, 3.0);
        assert_eq!(cr.height, 4.0);
    }
}
