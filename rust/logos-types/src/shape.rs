//! Core shape type.
//!
//! Clojure source: `common/src/app/common/types/shape.cljc` and
//! `common/src/app/common/types/shape/*.cljc`.
//!
//! `Shape` is the central domain object: every visible element in a Logos
//! design (rectangles, paths, text, components, …) is a `Shape`.

use std::collections::HashMap;
use uuid::Uuid;
use logos_layout::{Matrix, Rect};

use crate::blur::Blur;
use crate::fill::Fill;
use crate::shadow::Shadow;
use crate::stroke::Stroke;

// ─────────────────────────────────────────────────────────────────
// ShapeType
// ─────────────────────────────────────────────────────────────────

/// The discriminated kind of a shape.
/// Mirrors Clojure `:type` keyword on every shape map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, rename_all = "kebab-case"))]
pub enum ShapeType {
    /// Root frame (the canvas / artboard root — one per page).
    Frame,
    /// Rectangle.
    Rect,
    /// Ellipse / circle.
    Circle,
    /// Compound SVG path.
    Path,
    /// Raster image.
    Image,
    /// Text block.
    Text,
    /// Group (children rendered in order).
    Group,
    /// Boolean operation (union/intersection/…) result.
    Bool,
    /// Main component definition.
    Component,
}

// ─────────────────────────────────────────────────────────────────
// Constraints
// ─────────────────────────────────────────────────────────────────

/// Horizontal resize constraint.
/// Clojure: `:constraints-h` — `:left`, `:right`, `:leftright`, `:center`, `:scale`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, rename_all = "lowercase"))]
pub enum ConstraintH {
    #[default]
    Left,
    Right,
    Leftright,
    Center,
    Scale,
}

/// Vertical resize constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, rename_all = "lowercase"))]
pub enum ConstraintV {
    #[default]
    Top,
    Bottom,
    Topbottom,
    Center,
    Scale,
}

/// Horizontal + vertical constraint pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct Constraint {
    pub h: ConstraintH,
    pub v: ConstraintV,
}

// ─────────────────────────────────────────────────────────────────
// Boolean op kind
// ─────────────────────────────────────────────────────────────────

/// Boolean operation for `:bool` shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, rename_all = "lowercase"))]
pub enum BoolType {
    Union,
    Difference,
    Intersection,
    Exclusion,
}

// ─────────────────────────────────────────────────────────────────
// Shape — core struct
// ─────────────────────────────────────────────────────────────────

/// Common fields of every shape.
///
/// Optional collections only allocate when non-empty; use
/// [`Shape::has_fills`] / [`Shape::has_strokes`] etc. before dereferencing.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, rename_all = "camelCase"))]
pub struct Shape {
    pub id: Uuid,
    #[cfg_attr(feature = "serde", serde(rename = "type"))]
    pub shape_type: ShapeType,
    pub name: String,

    // ── Geometry ────────────────────────────────────────────────
    /// Axis-aligned bounding rect in *parent-local* coordinates.
    #[cfg_attr(feature = "ts", ts(type = "Bounds"))]
    pub selrect: Rect,
    /// Full affine transform (includes position, rotation, scale, skew).
    /// Defaults to identity.
    #[cfg_attr(feature = "serde", serde(default = "Matrix::identity"))]
    #[cfg_attr(feature = "ts", ts(type = "Transform"))]
    pub transform: Matrix,
    /// Inverse of `transform`.
    #[cfg_attr(feature = "serde", serde(default = "Matrix::identity"))]
    #[cfg_attr(feature = "ts", ts(type = "Transform"))]
    pub transform_inverse: Matrix,
    /// Rotation around shape center in **degrees**.
    #[cfg_attr(feature = "serde", serde(default))]
    pub rotation: f64,
    /// Corner points in page coordinates (cached, derived from transform + selrect).
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    #[cfg_attr(feature = "ts", ts(type = "Array<Point> | null"))]
    pub points: Option<[logos_layout::Point; 4]>,

    // ── Hierarchy ───────────────────────────────────────────────
    /// UUID of the parent frame / group.  `None` for the root frame.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub parent_id: Option<Uuid>,
    /// Frame this shape belongs to (may equal `parent_id`).
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub frame_id: Option<Uuid>,
    /// Ordered children IDs (only meaningful for Frame/Group/Bool).
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "Vec::is_empty"))]
    pub shapes: Vec<Uuid>,

    // ── Visibility ──────────────────────────────────────────────
    #[cfg_attr(feature = "serde", serde(default))]
    pub hidden: bool,
    /// Opacity in `[0.0, 1.0]`.
    #[cfg_attr(feature = "serde", serde(default = "default_opacity"))]
    pub opacity: f64,
    /// Whether the shape clips its children.
    #[cfg_attr(feature = "serde", serde(default))]
    pub masked_group: bool,

    // ── Appearance ──────────────────────────────────────────────
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "Vec::is_empty"))]
    pub fills: Vec<Fill>,
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "Vec::is_empty"))]
    pub strokes: Vec<Stroke>,
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "Vec::is_empty"))]
    pub shadow: Vec<Shadow>,
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "Vec::is_empty"))]
    pub blur: Vec<Blur>,

    // ── Border radius (rect / frame) ────────────────────────────
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub rx: Option<f64>,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub ry: Option<f64>,
    /// Per-corner radii `[tl, tr, br, bl]` (overrides `rx`/`ry`).
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub r1: Option<f64>,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub r2: Option<f64>,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub r3: Option<f64>,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub r4: Option<f64>,

    // ── Constraints ─────────────────────────────────────────────
    #[cfg_attr(feature = "serde", serde(default))]
    pub constraints_h: ConstraintH,
    #[cfg_attr(feature = "serde", serde(default))]
    pub constraints_v: ConstraintV,
    #[cfg_attr(feature = "serde", serde(default))]
    pub fixed_scroll: bool,

    // ── Component link ──────────────────────────────────────────
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub component_id: Option<Uuid>,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub component_file: Option<Uuid>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub component_root: bool,
    #[cfg_attr(feature = "serde", serde(default))]
    pub main_instance: bool,

    // ── Image-specific ──────────────────────────────────────────
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub metadata: Option<ImageMetadata>,

    // ── Bool-specific ───────────────────────────────────────────
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub bool_type: Option<BoolType>,

    // ── Misc / open set ─────────────────────────────────────────
    /// Arbitrary string→string plugin data (extensible).
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "HashMap::is_empty"))]
    pub plugin_data: HashMap<String, String>,
}

#[allow(dead_code)]
fn default_opacity() -> f64 { 1.0 }

/// Raster image metadata (for `:image` shapes).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, rename_all = "camelCase"))]
pub struct ImageMetadata {
    pub id: Uuid,
    pub width: u32,
    pub height: u32,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub mtype: Option<String>,
}

// ─────────────────────────────────────────────────────────────────
// Shape constructors / helpers
// ─────────────────────────────────────────────────────────────────

impl Shape {
    /// Minimum required fields; everything else defaults.
    pub fn new(id: Uuid, shape_type: ShapeType, name: impl Into<String>) -> Self {
        Shape {
            id,
            shape_type,
            name: name.into(),
            selrect: Rect::zero(),
            transform: Matrix::identity(),
            transform_inverse: Matrix::identity(),
            rotation: 0.0,
            points: None,
            parent_id: None,
            frame_id: None,
            shapes: Vec::new(),
            hidden: false,
            opacity: 1.0,
            masked_group: false,
            fills: Vec::new(),
            strokes: Vec::new(),
            shadow: Vec::new(),
            blur: Vec::new(),
            rx: None,
            ry: None,
            r1: None,
            r2: None,
            r3: None,
            r4: None,
            constraints_h: ConstraintH::Left,
            constraints_v: ConstraintV::Top,
            fixed_scroll: false,
            component_id: None,
            component_file: None,
            component_root: false,
            main_instance: false,
            metadata: None,
            bool_type: None,
            plugin_data: HashMap::new(),
        }
    }

    /// Convenience: create a rect shape positioned at `(x, y, w, h)`.
    pub fn rect(id: Uuid, x: f64, y: f64, w: f64, h: f64) -> Self {
        let mut s = Shape::new(id, ShapeType::Rect, "Rectangle");
        s.selrect = Rect::new(x, y, w, h);
        s
    }

    /// Convenience: create a frame (artboard).
    pub fn frame(id: Uuid, name: impl Into<String>, x: f64, y: f64, w: f64, h: f64) -> Self {
        let mut s = Shape::new(id, ShapeType::Frame, name);
        s.selrect = Rect::new(x, y, w, h);
        s
    }

    /// Returns `true` if this shape has any visible fills.
    pub fn has_fills(&self) -> bool { !self.fills.is_empty() }

    /// Returns `true` if this shape has any visible strokes.
    pub fn has_strokes(&self) -> bool { !self.strokes.is_empty() }

    /// Returns `true` if this shape can have children (Frame/Group/Bool).
    pub fn is_container(&self) -> bool {
        matches!(self.shape_type, ShapeType::Frame | ShapeType::Group | ShapeType::Bool)
    }

    /// Returns `true` if this is a component instance (linked copy).
    pub fn is_component_instance(&self) -> bool {
        self.component_id.is_some()
    }

    /// Returns the shape's center point.
    pub fn center(&self) -> logos_layout::Point {
        self.selrect.center()
    }
}

// ─────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_shape_selrect() {
        let id = Uuid::new_v4();
        let s = Shape::rect(id, 10.0, 20.0, 100.0, 50.0);
        assert_eq!(s.selrect.x, 10.0);
        assert_eq!(s.selrect.width, 100.0);
        assert_eq!(s.shape_type, ShapeType::Rect);
    }

    #[test]
    fn frame_is_container() {
        let id = Uuid::new_v4();
        let s = Shape::frame(id, "Page", 0.0, 0.0, 1920.0, 1080.0);
        assert!(s.is_container());
    }

    #[test]
    fn center() {
        let id = Uuid::new_v4();
        let s = Shape::rect(id, 0.0, 0.0, 100.0, 80.0);
        let c = s.center();
        assert_eq!(c.x, 50.0);
        assert_eq!(c.y, 40.0);
    }

    #[test]
    fn no_fills_by_default() {
        let id = Uuid::new_v4();
        let s = Shape::new(id, ShapeType::Circle, "Ellipse");
        assert!(!s.has_fills());
    }
}
