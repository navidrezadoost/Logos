//! Figma document node types.
//!
//! These mirror the Figma REST API node model as closely as possible.
//! Each node type has its own struct with type-specific properties,
//! plus shared base properties in [`NodeBase`].

use super::effect::Effect;
pub use super::paint::{BlendMode, Paint, StrokeAlign, StrokeCap, StrokeJoin};
use super::transform::{BoundingBox, Size2D, Transform2D};
use serde::{Deserialize, Serialize};

/// Figma node type identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum NodeType {
    Document = 0,
    Canvas = 1,
    Frame = 2,
    Group = 3,
    Vector = 4,
    BooleanOperation = 5,
    Star = 6,
    Line = 7,
    Ellipse = 8,
    RegularPolygon = 9,
    Rectangle = 10,
    Text = 11,
    Slice = 12,
    Component = 13,
    ComponentSet = 14,
    Instance = 15,
    Sticky = 16,
    ShapeWithText = 17,
    Connector = 18,
    Section = 19,
}

impl NodeType {
    /// Decode from byte value.
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Document),
            1 => Some(Self::Canvas),
            2 => Some(Self::Frame),
            3 => Some(Self::Group),
            4 => Some(Self::Vector),
            5 => Some(Self::BooleanOperation),
            6 => Some(Self::Star),
            7 => Some(Self::Line),
            8 => Some(Self::Ellipse),
            9 => Some(Self::RegularPolygon),
            10 => Some(Self::Rectangle),
            11 => Some(Self::Text),
            12 => Some(Self::Slice),
            13 => Some(Self::Component),
            14 => Some(Self::ComponentSet),
            15 => Some(Self::Instance),
            16 => Some(Self::Sticky),
            17 => Some(Self::ShapeWithText),
            18 => Some(Self::Connector),
            19 => Some(Self::Section),
            _ => None,
        }
    }

    /// Human-readable name.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Document => "DOCUMENT",
            Self::Canvas => "CANVAS",
            Self::Frame => "FRAME",
            Self::Group => "GROUP",
            Self::Vector => "VECTOR",
            Self::BooleanOperation => "BOOLEAN_OPERATION",
            Self::Star => "STAR",
            Self::Line => "LINE",
            Self::Ellipse => "ELLIPSE",
            Self::RegularPolygon => "REGULAR_POLYGON",
            Self::Rectangle => "RECTANGLE",
            Self::Text => "TEXT",
            Self::Slice => "SLICE",
            Self::Component => "COMPONENT",
            Self::ComponentSet => "COMPONENT_SET",
            Self::Instance => "INSTANCE",
            Self::Sticky => "STICKY",
            Self::ShapeWithText => "SHAPE_WITH_TEXT",
            Self::Connector => "CONNECTOR",
            Self::Section => "SECTION",
        }
    }

    /// Whether this node type can have children.
    pub fn can_have_children(&self) -> bool {
        matches!(
            self,
            Self::Document
                | Self::Canvas
                | Self::Frame
                | Self::Group
                | Self::BooleanOperation
                | Self::Component
                | Self::ComponentSet
                | Self::Instance
                | Self::Section
        )
    }

    /// Whether this node type is a shape (has fills/strokes).
    pub fn is_shape(&self) -> bool {
        matches!(
            self,
            Self::Rectangle
                | Self::Ellipse
                | Self::Star
                | Self::RegularPolygon
                | Self::Line
                | Self::Vector
                | Self::BooleanOperation
                | Self::Text
        )
    }
}

/// Layout constraint axis.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ConstraintType {
    /// Fix to left/top edge.
    Min,
    /// Fix to right/bottom edge.
    Max,
    /// Fix to both edges (stretch).
    Stretch,
    /// Center on axis.
    Center,
    /// Scale proportionally.
    Scale,
    /// Fixed size, fixed position.
    Fixed,
}

impl Default for ConstraintType {
    fn default() -> Self {
        Self::Min
    }
}

/// Layout constraints for a node.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Constraints {
    pub horizontal: ConstraintType,
    pub vertical: ConstraintType,
}

impl Default for Constraints {
    fn default() -> Self {
        Self {
            horizontal: ConstraintType::Min,
            vertical: ConstraintType::Min,
        }
    }
}

/// Corner radii for rectangles (can be uniform or per-corner).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CornerRadii {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_right: f32,
    pub bottom_left: f32,
}

impl CornerRadii {
    pub fn uniform(r: f32) -> Self {
        Self {
            top_left: r,
            top_right: r,
            bottom_right: r,
            bottom_left: r,
        }
    }

    pub fn per_corner(tl: f32, tr: f32, br: f32, bl: f32) -> Self {
        Self {
            top_left: tl,
            top_right: tr,
            bottom_right: br,
            bottom_left: bl,
        }
    }

    pub fn is_uniform(&self) -> bool {
        (self.top_left - self.top_right).abs() < f32::EPSILON
            && (self.top_right - self.bottom_right).abs() < f32::EPSILON
            && (self.bottom_right - self.bottom_left).abs() < f32::EPSILON
    }

    pub fn max_radius(&self) -> f32 {
        self.top_left
            .max(self.top_right)
            .max(self.bottom_right)
            .max(self.bottom_left)
    }
}

impl Default for CornerRadii {
    fn default() -> Self {
        Self::uniform(0.0)
    }
}

/// Text style properties.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextStyle {
    pub font_family: String,
    pub font_weight: u32,
    pub font_size: f32,
    pub italic: bool,
    pub line_height: Option<f32>,
    pub letter_spacing: f32,
    pub text_align: TextAlign,
    pub text_decoration: TextDecoration,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font_family: "Inter".to_string(),
            font_weight: 400,
            font_size: 14.0,
            italic: false,
            line_height: None,
            letter_spacing: 0.0,
            text_align: TextAlign::Left,
            text_decoration: TextDecoration::None,
        }
    }
}

/// Text horizontal alignment.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum TextAlign {
    Left,
    Center,
    Right,
    Justified,
}

impl Default for TextAlign {
    fn default() -> Self {
        Self::Left
    }
}

/// Text decoration.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum TextDecoration {
    None,
    Underline,
    Strikethrough,
}

impl Default for TextDecoration {
    fn default() -> Self {
        Self::None
    }
}

/// Boolean operation type.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum BooleanOp {
    Union,
    Intersect,
    Subtract,
    Exclude,
}

/// A vector path segment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorPath {
    /// SVG-like path data string.
    pub data: String,
    /// Winding rule.
    pub winding_rule: WindingRule,
}

/// Path winding rule.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum WindingRule {
    EvenOdd,
    NonZero,
}

impl Default for WindingRule {
    fn default() -> Self {
        Self::NonZero
    }
}

/// Arc data for ellipses.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ArcData {
    /// Starting angle in radians.
    pub starting_angle: f32,
    /// Ending angle in radians.
    pub ending_angle: f32,
    /// Inner radius ratio (0.0 = full ellipse, >0.0 = donut).
    pub inner_radius: f32,
}

impl Default for ArcData {
    fn default() -> Self {
        Self {
            starting_angle: 0.0,
            ending_angle: std::f32::consts::TAU,
            inner_radius: 0.0,
        }
    }
}

/// Auto-layout (Flexbox) properties.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutoLayout {
    /// Layout direction.
    pub direction: LayoutDirection,
    /// Gap between items.
    pub item_spacing: f32,
    /// Padding.
    pub padding_top: f32,
    pub padding_right: f32,
    pub padding_bottom: f32,
    pub padding_left: f32,
    /// Primary axis alignment.
    pub primary_align: LayoutAlign,
    /// Counter-axis alignment.
    pub counter_align: LayoutAlign,
}

/// Layout direction.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum LayoutDirection {
    Horizontal,
    Vertical,
}

/// Layout alignment.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum LayoutAlign {
    Min,
    Center,
    Max,
    SpaceBetween,
}

impl Default for LayoutAlign {
    fn default() -> Self {
        Self::Min
    }
}

/// Shared base properties for all Figma nodes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeBase {
    /// Unique node ID (e.g. "1:23").
    pub id: String,
    /// Node name.
    pub name: String,
    /// Whether the node is visible.
    pub visible: bool,
    /// Node opacity (0.0–1.0).
    pub opacity: f32,
    /// Blend mode.
    pub blend_mode: BlendMode,
    /// 2D affine transform relative to parent.
    pub transform: Transform2D,
    /// Absolute bounding box in the document.
    pub absolute_bounding_box: BoundingBox,
    /// Size of the node.
    pub size: Size2D,
    /// Whether the node is locked.
    pub locked: bool,
}

impl NodeBase {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            visible: true,
            opacity: 1.0,
            blend_mode: BlendMode::default(),
            transform: Transform2D::identity(),
            absolute_bounding_box: BoundingBox::default(),
            size: Size2D::default(),
            locked: false,
        }
    }
}

/// A complete Figma node with its type-specific data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FigmaNode {
    /// Shared base properties.
    pub base: NodeBase,
    /// The node type.
    pub node_type: NodeType,
    /// Type-specific data.
    pub data: NodeData,
    /// Child nodes (for container types).
    pub children: Vec<FigmaNode>,
}

/// Type-specific data for each node type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NodeData {
    /// Document root.
    Document,
    /// Canvas / page.
    Canvas {
        background_color: Paint,
    },
    /// A frame (artboard).
    Frame {
        fills: Vec<Paint>,
        strokes: Vec<Paint>,
        stroke_weight: f32,
        stroke_align: StrokeAlign,
        effects: Vec<Effect>,
        corner_radii: CornerRadii,
        constraints: Constraints,
        auto_layout: Option<AutoLayout>,
        clip_content: bool,
    },
    /// A group (no fills/strokes of its own).
    Group,
    /// A rectangle shape.
    Rectangle {
        fills: Vec<Paint>,
        strokes: Vec<Paint>,
        stroke_weight: f32,
        stroke_align: StrokeAlign,
        stroke_cap: StrokeCap,
        stroke_join: StrokeJoin,
        effects: Vec<Effect>,
        corner_radii: CornerRadii,
        constraints: Constraints,
    },
    /// An ellipse shape.
    Ellipse {
        fills: Vec<Paint>,
        strokes: Vec<Paint>,
        stroke_weight: f32,
        stroke_align: StrokeAlign,
        effects: Vec<Effect>,
        arc_data: ArcData,
        constraints: Constraints,
    },
    /// A line segment.
    Line {
        strokes: Vec<Paint>,
        stroke_weight: f32,
        stroke_cap: StrokeCap,
        effects: Vec<Effect>,
        constraints: Constraints,
    },
    /// A star shape.
    Star {
        fills: Vec<Paint>,
        strokes: Vec<Paint>,
        stroke_weight: f32,
        effects: Vec<Effect>,
        point_count: u32,
        inner_radius_ratio: f32,
        constraints: Constraints,
    },
    /// A regular polygon.
    RegularPolygon {
        fills: Vec<Paint>,
        strokes: Vec<Paint>,
        stroke_weight: f32,
        effects: Vec<Effect>,
        point_count: u32,
        constraints: Constraints,
    },
    /// A text node.
    Text {
        characters: String,
        style: TextStyle,
        fills: Vec<Paint>,
        strokes: Vec<Paint>,
        stroke_weight: f32,
        effects: Vec<Effect>,
        constraints: Constraints,
    },
    /// A vector (arbitrary path).
    VectorNode {
        fills: Vec<Paint>,
        strokes: Vec<Paint>,
        stroke_weight: f32,
        stroke_align: StrokeAlign,
        stroke_cap: StrokeCap,
        stroke_join: StrokeJoin,
        effects: Vec<Effect>,
        paths: Vec<VectorPath>,
        constraints: Constraints,
    },
    /// A boolean operation on child shapes.
    BooleanOp {
        operation: BooleanOp,
        fills: Vec<Paint>,
        strokes: Vec<Paint>,
        stroke_weight: f32,
        effects: Vec<Effect>,
    },
    /// A component definition.
    Component {
        fills: Vec<Paint>,
        strokes: Vec<Paint>,
        stroke_weight: f32,
        stroke_align: StrokeAlign,
        effects: Vec<Effect>,
        corner_radii: CornerRadii,
        constraints: Constraints,
        description: String,
    },
    /// A component set (variants container).
    ComponentSet {
        description: String,
    },
    /// An instance of a component.
    Instance {
        component_id: String,
        fills: Vec<Paint>,
        strokes: Vec<Paint>,
        stroke_weight: f32,
        effects: Vec<Effect>,
        constraints: Constraints,
    },
    /// A section (organizational container).
    Section {
        fills: Vec<Paint>,
    },
    /// A slice (export region).
    Slice {
        constraints: Constraints,
    },
}

impl FigmaNode {
    /// Create a rectangle node.
    pub fn rectangle(
        id: impl Into<String>,
        name: impl Into<String>,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) -> Self {
        let mut base = NodeBase::new(id, name);
        base.size = Size2D::new(width, height);
        base.absolute_bounding_box = BoundingBox::new(x, y, width, height);
        base.transform = Transform2D::translate(x, y);

        Self {
            base,
            node_type: NodeType::Rectangle,
            data: NodeData::Rectangle {
                fills: vec![Paint::solid(super::paint::Color::white())],
                strokes: Vec::new(),
                stroke_weight: 0.0,
                stroke_align: StrokeAlign::default(),
                stroke_cap: StrokeCap::default(),
                stroke_join: StrokeJoin::default(),
                effects: Vec::new(),
                corner_radii: CornerRadii::default(),
                constraints: Constraints::default(),
            },
            children: Vec::new(),
        }
    }

    /// Create an ellipse node.
    pub fn ellipse(
        id: impl Into<String>,
        name: impl Into<String>,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) -> Self {
        let mut base = NodeBase::new(id, name);
        base.size = Size2D::new(width, height);
        base.absolute_bounding_box = BoundingBox::new(x, y, width, height);
        base.transform = Transform2D::translate(x, y);

        Self {
            base,
            node_type: NodeType::Ellipse,
            data: NodeData::Ellipse {
                fills: vec![Paint::solid(super::paint::Color::white())],
                strokes: Vec::new(),
                stroke_weight: 0.0,
                stroke_align: StrokeAlign::default(),
                effects: Vec::new(),
                arc_data: ArcData::default(),
                constraints: Constraints::default(),
            },
            children: Vec::new(),
        }
    }

    /// Create a text node.
    pub fn text(
        id: impl Into<String>,
        name: impl Into<String>,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        characters: impl Into<String>,
    ) -> Self {
        let mut base = NodeBase::new(id, name);
        base.size = Size2D::new(width, height);
        base.absolute_bounding_box = BoundingBox::new(x, y, width, height);
        base.transform = Transform2D::translate(x, y);

        Self {
            base,
            node_type: NodeType::Text,
            data: NodeData::Text {
                characters: characters.into(),
                style: TextStyle::default(),
                fills: vec![Paint::solid(super::paint::Color::black())],
                strokes: Vec::new(),
                stroke_weight: 0.0,
                effects: Vec::new(),
                constraints: Constraints::default(),
            },
            children: Vec::new(),
        }
    }

    /// Create a frame node with children.
    pub fn frame(
        id: impl Into<String>,
        name: impl Into<String>,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        children: Vec<FigmaNode>,
    ) -> Self {
        let mut base = NodeBase::new(id, name);
        base.size = Size2D::new(width, height);
        base.absolute_bounding_box = BoundingBox::new(x, y, width, height);
        base.transform = Transform2D::translate(x, y);

        Self {
            base,
            node_type: NodeType::Frame,
            data: NodeData::Frame {
                fills: vec![Paint::solid(super::paint::Color::white())],
                strokes: Vec::new(),
                stroke_weight: 0.0,
                stroke_align: StrokeAlign::default(),
                effects: Vec::new(),
                corner_radii: CornerRadii::default(),
                constraints: Constraints::default(),
                auto_layout: None,
                clip_content: true,
            },
            children,
        }
    }

    /// Create a group node wrapping children.
    pub fn group(
        id: impl Into<String>,
        name: impl Into<String>,
        children: Vec<FigmaNode>,
    ) -> Self {
        Self {
            base: NodeBase::new(id, name),
            node_type: NodeType::Group,
            data: NodeData::Group,
            children,
        }
    }

    /// Create a document root node.
    pub fn document(children: Vec<FigmaNode>) -> Self {
        Self {
            base: NodeBase::new("0:0", "Document"),
            node_type: NodeType::Document,
            data: NodeData::Document,
            children,
        }
    }

    /// Create a canvas (page) node.
    pub fn canvas(
        id: impl Into<String>,
        name: impl Into<String>,
        children: Vec<FigmaNode>,
    ) -> Self {
        Self {
            base: NodeBase::new(id, name),
            node_type: NodeType::Canvas,
            data: NodeData::Canvas {
                background_color: Paint::solid(super::paint::Color::from_rgba8(242, 242, 242, 255)),
            },
            children,
        }
    }

    /// Total number of nodes in this subtree (including self).
    pub fn node_count(&self) -> usize {
        1 + self.children.iter().map(|c| c.node_count()).sum::<usize>()
    }

    /// Iterate all nodes in depth-first order.
    pub fn iter_dfs(&self) -> DfsIterator<'_> {
        DfsIterator {
            stack: vec![self],
        }
    }

    /// Get fills (if this node type has them).
    pub fn fills(&self) -> &[Paint] {
        match &self.data {
            NodeData::Rectangle { fills, .. }
            | NodeData::Ellipse { fills, .. }
            | NodeData::Frame { fills, .. }
            | NodeData::Star { fills, .. }
            | NodeData::RegularPolygon { fills, .. }
            | NodeData::Text { fills, .. }
            | NodeData::VectorNode { fills, .. }
            | NodeData::BooleanOp { fills, .. }
            | NodeData::Component { fills, .. }
            | NodeData::Instance { fills, .. }
            | NodeData::Section { fills, .. } => fills,
            _ => &[],
        }
    }

    /// Get strokes (if this node type has them).
    pub fn strokes(&self) -> &[Paint] {
        match &self.data {
            NodeData::Rectangle { strokes, .. }
            | NodeData::Ellipse { strokes, .. }
            | NodeData::Frame { strokes, .. }
            | NodeData::Star { strokes, .. }
            | NodeData::RegularPolygon { strokes, .. }
            | NodeData::Text { strokes, .. }
            | NodeData::VectorNode { strokes, .. }
            | NodeData::BooleanOp { strokes, .. }
            | NodeData::Component { strokes, .. }
            | NodeData::Instance { strokes, .. } => strokes,
            _ => &[],
        }
    }

    /// Get effects (if this node type has them).
    pub fn effects(&self) -> &[Effect] {
        match &self.data {
            NodeData::Rectangle { effects, .. }
            | NodeData::Ellipse { effects, .. }
            | NodeData::Frame { effects, .. }
            | NodeData::Star { effects, .. }
            | NodeData::RegularPolygon { effects, .. }
            | NodeData::Text { effects, .. }
            | NodeData::VectorNode { effects, .. }
            | NodeData::BooleanOp { effects, .. }
            | NodeData::Component { effects, .. }
            | NodeData::Instance { effects, .. }
            | NodeData::Line { effects, .. } => effects,
            _ => &[],
        }
    }
}

/// Depth-first iterator over a node tree.
pub struct DfsIterator<'a> {
    stack: Vec<&'a FigmaNode>,
}

impl<'a> Iterator for DfsIterator<'a> {
    type Item = &'a FigmaNode;

    fn next(&mut self) -> Option<Self::Item> {
        let node = self.stack.pop()?;
        // Push children in reverse order so the first child is popped next
        for child in node.children.iter().rev() {
            self.stack.push(child);
        }
        Some(node)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_type_from_u8() {
        assert_eq!(NodeType::from_u8(0), Some(NodeType::Document));
        assert_eq!(NodeType::from_u8(10), Some(NodeType::Rectangle));
        assert_eq!(NodeType::from_u8(11), Some(NodeType::Text));
        assert_eq!(NodeType::from_u8(255), None);
    }

    #[test]
    fn test_node_type_name() {
        assert_eq!(NodeType::Rectangle.name(), "RECTANGLE");
        assert_eq!(NodeType::Ellipse.name(), "ELLIPSE");
        assert_eq!(NodeType::Text.name(), "TEXT");
    }

    #[test]
    fn test_node_type_can_have_children() {
        assert!(NodeType::Document.can_have_children());
        assert!(NodeType::Frame.can_have_children());
        assert!(NodeType::Group.can_have_children());
        assert!(!NodeType::Rectangle.can_have_children());
        assert!(!NodeType::Ellipse.can_have_children());
        assert!(!NodeType::Text.can_have_children());
    }

    #[test]
    fn test_node_type_is_shape() {
        assert!(NodeType::Rectangle.is_shape());
        assert!(NodeType::Ellipse.is_shape());
        assert!(NodeType::Text.is_shape());
        assert!(!NodeType::Frame.is_shape());
        assert!(!NodeType::Document.is_shape());
    }

    #[test]
    fn test_create_rectangle() {
        let rect = FigmaNode::rectangle("1:1", "Rect 1", 10.0, 20.0, 100.0, 50.0);
        assert_eq!(rect.node_type, NodeType::Rectangle);
        assert_eq!(rect.base.name, "Rect 1");
        assert!((rect.base.size.width - 100.0).abs() < 0.001);
        assert!((rect.base.size.height - 50.0).abs() < 0.001);
        assert!((rect.base.absolute_bounding_box.x - 10.0).abs() < 0.001);
        assert_eq!(rect.fills().len(), 1);
    }

    #[test]
    fn test_create_ellipse() {
        let ell = FigmaNode::ellipse("1:2", "Circle", 0.0, 0.0, 80.0, 80.0);
        assert_eq!(ell.node_type, NodeType::Ellipse);
        assert_eq!(ell.base.name, "Circle");
    }

    #[test]
    fn test_create_text() {
        let txt = FigmaNode::text("1:3", "Label", 0.0, 0.0, 200.0, 24.0, "Hello World");
        assert_eq!(txt.node_type, NodeType::Text);
        match &txt.data {
            NodeData::Text { characters, style, .. } => {
                assert_eq!(characters, "Hello World");
                assert_eq!(style.font_family, "Inter");
                assert!((style.font_size - 14.0).abs() < 0.001);
            }
            _ => panic!("wrong node data"),
        }
    }

    #[test]
    fn test_create_frame_with_children() {
        let child1 = FigmaNode::rectangle("1:2", "Rect", 10.0, 10.0, 50.0, 50.0);
        let child2 = FigmaNode::ellipse("1:3", "Circle", 70.0, 10.0, 50.0, 50.0);
        let frame = FigmaNode::frame("1:1", "Frame 1", 0.0, 0.0, 200.0, 100.0, vec![child1, child2]);

        assert_eq!(frame.node_type, NodeType::Frame);
        assert_eq!(frame.children.len(), 2);
        assert_eq!(frame.node_count(), 3);
    }

    #[test]
    fn test_create_group() {
        let group = FigmaNode::group("1:4", "Group 1", vec![
            FigmaNode::rectangle("1:5", "Rect", 0.0, 0.0, 50.0, 50.0),
        ]);
        assert_eq!(group.node_type, NodeType::Group);
        assert_eq!(group.children.len(), 1);
    }

    #[test]
    fn test_document_structure() {
        let page = FigmaNode::canvas("0:1", "Page 1", vec![
            FigmaNode::frame("1:1", "Frame 1", 0.0, 0.0, 375.0, 812.0, vec![
                FigmaNode::rectangle("2:1", "Background", 0.0, 0.0, 375.0, 812.0),
                FigmaNode::text("2:2", "Title", 20.0, 40.0, 335.0, 32.0, "Hello"),
            ]),
        ]);
        let doc = FigmaNode::document(vec![page]);

        assert_eq!(doc.node_type, NodeType::Document);
        assert_eq!(doc.node_count(), 5);
    }

    #[test]
    fn test_dfs_iterator() {
        let doc = FigmaNode::document(vec![
            FigmaNode::canvas("0:1", "Page 1", vec![
                FigmaNode::rectangle("1:1", "Rect", 0.0, 0.0, 50.0, 50.0),
                FigmaNode::ellipse("1:2", "Ellipse", 0.0, 0.0, 50.0, 50.0),
            ]),
        ]);

        let names: Vec<&str> = doc.iter_dfs().map(|n| n.base.name.as_str()).collect();
        assert_eq!(names, vec!["Document", "Page 1", "Rect", "Ellipse"]);
    }

    #[test]
    fn test_dfs_iterator_count() {
        let doc = FigmaNode::document(vec![
            FigmaNode::canvas("0:1", "Page 1", vec![
                FigmaNode::frame("1:1", "Frame", 0.0, 0.0, 100.0, 100.0, vec![
                    FigmaNode::rectangle("2:1", "R1", 0.0, 0.0, 50.0, 50.0),
                    FigmaNode::rectangle("2:2", "R2", 0.0, 0.0, 50.0, 50.0),
                ]),
            ]),
        ]);
        assert_eq!(doc.iter_dfs().count(), 5);
    }

    #[test]
    fn test_node_fills_strokes() {
        let rect = FigmaNode::rectangle("1:1", "Rect", 0.0, 0.0, 50.0, 50.0);
        assert_eq!(rect.fills().len(), 1);
        assert_eq!(rect.strokes().len(), 0);
        assert_eq!(rect.effects().len(), 0);
    }

    #[test]
    fn test_corner_radii_uniform() {
        let cr = CornerRadii::uniform(10.0);
        assert!(cr.is_uniform());
        assert!((cr.max_radius() - 10.0).abs() < 0.001);
    }

    #[test]
    fn test_corner_radii_per_corner() {
        let cr = CornerRadii::per_corner(5.0, 10.0, 15.0, 20.0);
        assert!(!cr.is_uniform());
        assert!((cr.max_radius() - 20.0).abs() < 0.001);
    }

    #[test]
    fn test_constraints_default() {
        let c = Constraints::default();
        assert_eq!(c.horizontal, ConstraintType::Min);
        assert_eq!(c.vertical, ConstraintType::Min);
    }

    #[test]
    fn test_arc_data_default() {
        let arc = ArcData::default();
        assert!((arc.starting_angle).abs() < 0.001);
        assert!((arc.ending_angle - std::f32::consts::TAU).abs() < 0.001);
        assert!((arc.inner_radius).abs() < 0.001);
    }

    #[test]
    fn test_text_style_default() {
        let ts = TextStyle::default();
        assert_eq!(ts.font_family, "Inter");
        assert_eq!(ts.font_weight, 400);
        assert!((ts.font_size - 14.0).abs() < 0.001);
    }

    #[test]
    fn test_node_strokes_empty_on_group() {
        let group = FigmaNode::group("1:1", "G", vec![]);
        assert!(group.strokes().is_empty());
        assert!(group.fills().is_empty());
        assert!(group.effects().is_empty());
    }

    #[test]
    fn test_node_base_new() {
        let base = NodeBase::new("1:1", "Test");
        assert_eq!(base.id, "1:1");
        assert_eq!(base.name, "Test");
        assert!(base.visible);
        assert!((base.opacity - 1.0).abs() < 0.001);
        assert!(base.transform.is_identity());
    }
}
