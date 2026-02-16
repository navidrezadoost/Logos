//! 2D transform and geometry types.

use serde::{Deserialize, Serialize};

/// A 2D affine transform matrix.
///
/// Stored as a 2×3 matrix:
/// ```text
/// | a  b  tx |
/// | c  d  ty |
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Transform2D {
    /// Scale X / Rotation component.
    pub a: f32,
    /// Shear Y component.
    pub b: f32,
    /// Shear X component.
    pub c: f32,
    /// Scale Y / Rotation component.
    pub d: f32,
    /// Translation X.
    pub tx: f32,
    /// Translation Y.
    pub ty: f32,
}

impl Default for Transform2D {
    fn default() -> Self {
        Self::identity()
    }
}

impl Transform2D {
    /// The identity transform (no transformation).
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

    /// A translation transform.
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

    /// A scale transform.
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

    /// A rotation transform (angle in radians).
    pub fn rotate(angle: f32) -> Self {
        let cos = angle.cos();
        let sin = angle.sin();
        Self {
            a: cos,
            b: -sin,
            c: sin,
            d: cos,
            tx: 0.0,
            ty: 0.0,
        }
    }

    /// Multiply two transforms: self × other.
    pub fn multiply(&self, other: &Transform2D) -> Transform2D {
        Transform2D {
            a: self.a * other.a + self.b * other.c,
            b: self.a * other.b + self.b * other.d,
            c: self.c * other.a + self.d * other.c,
            d: self.c * other.b + self.d * other.d,
            tx: self.a * other.tx + self.b * other.ty + self.tx,
            ty: self.c * other.tx + self.d * other.ty + self.ty,
        }
    }

    /// Transform a point.
    pub fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.a * x + self.b * y + self.tx,
            self.c * x + self.d * y + self.ty,
        )
    }

    /// Whether this is the identity transform.
    pub fn is_identity(&self) -> bool {
        (self.a - 1.0).abs() < f32::EPSILON
            && self.b.abs() < f32::EPSILON
            && self.c.abs() < f32::EPSILON
            && (self.d - 1.0).abs() < f32::EPSILON
            && self.tx.abs() < f32::EPSILON
            && self.ty.abs() < f32::EPSILON
    }
}

/// An axis-aligned bounding box.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BoundingBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Default for BoundingBox {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        }
    }
}

impl BoundingBox {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// The right edge (x + width).
    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    /// The bottom edge (y + height).
    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    /// Center point.
    pub fn center(&self) -> (f32, f32) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    /// Area.
    pub fn area(&self) -> f32 {
        self.width * self.height
    }

    /// Whether this box contains a point.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x <= self.right() && y >= self.y && y <= self.bottom()
    }

    /// Compute the union of two bounding boxes.
    pub fn union(&self, other: &BoundingBox) -> BoundingBox {
        let min_x = self.x.min(other.x);
        let min_y = self.y.min(other.y);
        let max_x = self.right().max(other.right());
        let max_y = self.bottom().max(other.bottom());
        BoundingBox {
            x: min_x,
            y: min_y,
            width: max_x - min_x,
            height: max_y - min_y,
        }
    }
}

/// A 2D size (width × height).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Size2D {
    pub width: f32,
    pub height: f32,
}

impl Size2D {
    pub fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }

    pub fn area(&self) -> f32 {
        self.width * self.height
    }
}

impl Default for Size2D {
    fn default() -> Self {
        Self {
            width: 0.0,
            height: 0.0,
        }
    }
}

/// A 2D point / vector.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Vector2D {
    pub x: f32,
    pub y: f32,
}

impl Vector2D {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn zero() -> Self {
        Self { x: 0.0, y: 0.0 }
    }

    pub fn length(&self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    pub fn distance_to(&self, other: &Vector2D) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }
}

impl Default for Vector2D {
    fn default() -> Self {
        Self::zero()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_identity() {
        let t = Transform2D::identity();
        assert!(t.is_identity());
        let (x, y) = t.apply(5.0, 10.0);
        assert!((x - 5.0).abs() < 0.001);
        assert!((y - 10.0).abs() < 0.001);
    }

    #[test]
    fn test_transform_translate() {
        let t = Transform2D::translate(10.0, 20.0);
        let (x, y) = t.apply(5.0, 5.0);
        assert!((x - 15.0).abs() < 0.001);
        assert!((y - 25.0).abs() < 0.001);
    }

    #[test]
    fn test_transform_scale() {
        let t = Transform2D::scale(2.0, 3.0);
        let (x, y) = t.apply(5.0, 10.0);
        assert!((x - 10.0).abs() < 0.001);
        assert!((y - 30.0).abs() < 0.001);
    }

    #[test]
    fn test_transform_rotate_90() {
        let t = Transform2D::rotate(std::f32::consts::FRAC_PI_2);
        let (x, y) = t.apply(1.0, 0.0);
        assert!(x.abs() < 0.001);
        assert!((y - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_transform_multiply() {
        let t1 = Transform2D::translate(10.0, 0.0);
        let t2 = Transform2D::scale(2.0, 2.0);
        let combined = t1.multiply(&t2);
        let (x, y) = combined.apply(5.0, 5.0);
        assert!((x - 20.0).abs() < 0.001);
        assert!((y - 10.0).abs() < 0.001);
    }

    #[test]
    fn test_bounding_box_basics() {
        let bb = BoundingBox::new(10.0, 20.0, 100.0, 50.0);
        assert_eq!(bb.right(), 110.0);
        assert_eq!(bb.bottom(), 70.0);
        assert!((bb.area() - 5000.0).abs() < 0.001);
    }

    #[test]
    fn test_bounding_box_center() {
        let bb = BoundingBox::new(0.0, 0.0, 100.0, 100.0);
        let (cx, cy) = bb.center();
        assert!((cx - 50.0).abs() < 0.001);
        assert!((cy - 50.0).abs() < 0.001);
    }

    #[test]
    fn test_bounding_box_contains() {
        let bb = BoundingBox::new(10.0, 10.0, 100.0, 100.0);
        assert!(bb.contains(50.0, 50.0));
        assert!(bb.contains(10.0, 10.0));
        assert!(!bb.contains(5.0, 50.0));
        assert!(!bb.contains(50.0, 200.0));
    }

    #[test]
    fn test_bounding_box_union() {
        let a = BoundingBox::new(0.0, 0.0, 50.0, 50.0);
        let b = BoundingBox::new(25.0, 25.0, 100.0, 100.0);
        let u = a.union(&b);
        assert!((u.x).abs() < 0.001);
        assert!((u.y).abs() < 0.001);
        assert!((u.width - 125.0).abs() < 0.001);
        assert!((u.height - 125.0).abs() < 0.001);
    }

    #[test]
    fn test_size_2d() {
        let s = Size2D::new(10.0, 20.0);
        assert!((s.area() - 200.0).abs() < 0.001);
    }

    #[test]
    fn test_vector_2d() {
        let v = Vector2D::new(3.0, 4.0);
        assert!((v.length() - 5.0).abs() < 0.001);
    }

    #[test]
    fn test_vector_distance() {
        let a = Vector2D::new(0.0, 0.0);
        let b = Vector2D::new(3.0, 4.0);
        assert!((a.distance_to(&b) - 5.0).abs() < 0.001);
    }
}
