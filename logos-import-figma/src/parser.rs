//! Parser: converts raw .fig binary data into a [`FigmaNode`] tree.
//!
//! The parser handles:
//! 1. Header validation
//! 2. Payload decompression (zlib)
//! 3. Kiwi binary decoding
//! 4. Node tree reconstruction from flat field data

use crate::error::{FigmaError, FigmaResult};
use crate::format::header::FigHeader;
use crate::format::kiwi::{self, KiwiDecoder, KiwiField, KiwiValue};
use crate::model::effect::{Effect, EffectType};
use crate::model::node::*;
use crate::model::paint::*;
use crate::model::transform::*;
use flate2::read::ZlibDecoder;
use std::io::Read;

/// Kiwi field IDs for the Figma schema.
///
/// These constants define the mapping between Kiwi field IDs
/// and node properties.
pub mod field_ids {
    // Node base fields
    pub const NODE_TYPE: u32 = 1;
    pub const NODE_ID: u32 = 2;
    pub const NODE_NAME: u32 = 3;
    pub const VISIBLE: u32 = 4;
    pub const OPACITY: u32 = 5;
    pub const BLEND_MODE: u32 = 6;
    pub const TRANSFORM: u32 = 7;
    pub const SIZE: u32 = 8;
    pub const BOUNDING_BOX: u32 = 9;
    pub const LOCKED: u32 = 10;

    // Shape fields
    pub const FILLS: u32 = 11;
    pub const STROKES: u32 = 12;
    pub const STROKE_WEIGHT: u32 = 13;
    pub const STROKE_ALIGN: u32 = 14;
    pub const STROKE_CAP: u32 = 15;
    pub const STROKE_JOIN: u32 = 16;
    pub const EFFECTS: u32 = 17;
    pub const CORNER_RADII: u32 = 18;
    pub const CONSTRAINTS: u32 = 19;
    pub const CHILDREN: u32 = 20;

    // Text fields
    pub const CHARACTERS: u32 = 21;
    pub const TEXT_STYLE: u32 = 22;

    // Ellipse fields
    pub const ARC_DATA: u32 = 23;

    // Frame fields
    pub const AUTO_LAYOUT: u32 = 24;
    pub const CLIP_CONTENT: u32 = 25;

    // Component/Instance fields
    pub const COMPONENT_ID: u32 = 26;
    pub const DESCRIPTION: u32 = 27;

    // Paint sub-fields
    pub const PAINT_TYPE: u32 = 1;
    pub const PAINT_VISIBLE: u32 = 2;
    pub const PAINT_OPACITY: u32 = 3;
    pub const COLOR_R: u32 = 4;
    pub const COLOR_G: u32 = 5;
    pub const COLOR_B: u32 = 6;
    pub const COLOR_A: u32 = 7;
    pub const GRADIENT_STOPS: u32 = 8;
    pub const IMAGE_REF: u32 = 9;
    pub const SCALE_MODE: u32 = 10;

    // Effect sub-fields
    pub const EFFECT_TYPE: u32 = 1;
    pub const EFFECT_VISIBLE: u32 = 2;
    pub const EFFECT_RADIUS: u32 = 3;
    pub const EFFECT_COLOR: u32 = 4;
    pub const EFFECT_OFFSET_X: u32 = 5;
    pub const EFFECT_OFFSET_Y: u32 = 6;
    pub const EFFECT_SPREAD: u32 = 7;

    // Transform sub-fields
    pub const TRANSFORM_A: u32 = 1;
    pub const TRANSFORM_B: u32 = 2;
    pub const TRANSFORM_C: u32 = 3;
    pub const TRANSFORM_D: u32 = 4;
    pub const TRANSFORM_TX: u32 = 5;
    pub const TRANSFORM_TY: u32 = 6;

    // Size sub-fields
    pub const WIDTH: u32 = 1;
    pub const HEIGHT: u32 = 2;

    // Bounding box sub-fields
    pub const BB_X: u32 = 1;
    pub const BB_Y: u32 = 2;
    pub const BB_W: u32 = 3;
    pub const BB_H: u32 = 4;

    // Text style sub-fields
    pub const FONT_FAMILY: u32 = 1;
    pub const FONT_WEIGHT: u32 = 2;
    pub const FONT_SIZE: u32 = 3;
    pub const FONT_ITALIC: u32 = 4;
    pub const LINE_HEIGHT: u32 = 5;
    pub const LETTER_SPACING: u32 = 6;
    pub const TEXT_ALIGN: u32 = 7;

    // Star/Polygon fields
    pub const POINT_COUNT: u32 = 28;
    pub const INNER_RADIUS_RATIO: u32 = 29;

    // Boolean operation fields
    pub const BOOLEAN_OP: u32 = 30;

    // Vector path fields
    pub const PATHS: u32 = 31;
    pub const PATH_DATA: u32 = 1;
    pub const WINDING_RULE: u32 = 2;

    // Canvas background
    pub const BACKGROUND_COLOR: u32 = 32;
}

/// Import options for customizing the parsing behavior.
#[derive(Debug, Clone)]
pub struct ImportOptions {
    /// Maximum number of nodes to parse (0 = unlimited).
    pub max_nodes: usize,
    /// Whether to parse fills and strokes.
    pub parse_paints: bool,
    /// Whether to parse effects.
    pub parse_effects: bool,
    /// Whether to parse text styles.
    pub parse_text_styles: bool,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            max_nodes: 0,
            parse_paints: true,
            parse_effects: true,
            parse_text_styles: true,
        }
    }
}

/// Statistics from a parse operation.
#[derive(Debug, Clone, Default)]
pub struct ParseStats {
    pub total_nodes: usize,
    pub rectangles: usize,
    pub ellipses: usize,
    pub texts: usize,
    pub frames: usize,
    pub groups: usize,
    pub vectors: usize,
    pub components: usize,
    pub instances: usize,
    pub other: usize,
    pub max_depth: usize,
    pub compressed_size: usize,
    pub uncompressed_size: usize,
}

impl ParseStats {
    fn count_node(&mut self, node_type: NodeType) {
        self.total_nodes += 1;
        match node_type {
            NodeType::Rectangle => self.rectangles += 1,
            NodeType::Ellipse => self.ellipses += 1,
            NodeType::Text => self.texts += 1,
            NodeType::Frame => self.frames += 1,
            NodeType::Group => self.groups += 1,
            NodeType::Vector => self.vectors += 1,
            NodeType::Component => self.components += 1,
            NodeType::Instance => self.instances += 1,
            _ => self.other += 1,
        }
    }

    /// One-line summary string.
    pub fn summary(&self) -> String {
        format!(
            "{} nodes ({} rect, {} ellipse, {} text, {} frame, {} group, {} vec), depth {}",
            self.total_nodes,
            self.rectangles,
            self.ellipses,
            self.texts,
            self.frames,
            self.groups,
            self.vectors,
            self.max_depth,
        )
    }
}

/// The main Figma file parser.
pub struct FigmaParser {
    options: ImportOptions,
    stats: ParseStats,
}

impl FigmaParser {
    /// Create a parser with default options.
    pub fn new() -> Self {
        Self {
            options: ImportOptions::default(),
            stats: ParseStats::default(),
        }
    }

    /// Create a parser with custom options.
    pub fn with_options(options: ImportOptions) -> Self {
        Self {
            options,
            stats: ParseStats::default(),
        }
    }

    /// Get parse statistics from the last parse operation.
    pub fn stats(&self) -> &ParseStats {
        &self.stats
    }

    /// Parse a complete .fig file from bytes.
    pub fn parse(&mut self, data: &[u8]) -> FigmaResult<FigmaNode> {
        self.stats = ParseStats::default();

        // Step 1: Parse header
        let header = FigHeader::parse(data)?;
        self.stats.compressed_size = header.compressed_length as usize;
        self.stats.uncompressed_size = header.uncompressed_length as usize;

        // Step 2: Extract and decompress payload
        let payload_start = header.payload_offset();
        let payload_end = payload_start + header.compressed_length as usize;

        if data.len() < payload_end {
            return Err(FigmaError::UnexpectedEof(data.len()));
        }

        let compressed = &data[payload_start..payload_end];
        let decompressed = decompress_payload(compressed, header.uncompressed_length as usize)?;

        // Step 3: Decode Kiwi binary format
        let mut decoder = KiwiDecoder::new(&decompressed);
        let fields = decoder.decode_root()?;

        // Step 4: Build node tree from decoded fields
        self.build_node_tree(&fields, 0)
    }

    /// Parse from a file path.
    pub fn parse_file(&mut self, path: &std::path::Path) -> FigmaResult<FigmaNode> {
        let data = std::fs::read(path)?;
        self.parse(&data)
    }

    /// Build a node tree from decoded Kiwi fields.
    fn build_node_tree(&mut self, fields: &[KiwiField], depth: usize) -> FigmaResult<FigmaNode> {
        if self.options.max_nodes > 0 && self.stats.total_nodes >= self.options.max_nodes {
            return Err(FigmaError::ParseError {
                offset: 0,
                message: format!("node limit reached: {}", self.options.max_nodes),
            });
        }

        if depth > self.stats.max_depth {
            self.stats.max_depth = depth;
        }

        // Extract node type
        let node_type_id = kiwi::get_uint(fields, field_ids::NODE_TYPE)
            .unwrap_or(0) as u8;
        let node_type = NodeType::from_u8(node_type_id)
            .ok_or(FigmaError::UnknownNodeType(node_type_id))?;

        self.stats.count_node(node_type);

        // Extract base properties
        let base = self.parse_node_base(fields, node_type)?;

        // Extract children
        let children = self.parse_children(fields, depth)?;

        // Extract type-specific data
        let data = self.parse_node_data(fields, node_type)?;

        Ok(FigmaNode {
            base,
            node_type,
            data,
            children,
        })
    }

    /// Parse shared base properties from fields.
    fn parse_node_base(
        &self,
        fields: &[KiwiField],
        _node_type: NodeType,
    ) -> FigmaResult<NodeBase> {
        let id = kiwi::get_string(fields, field_ids::NODE_ID)
            .unwrap_or("0:0")
            .to_string();
        let name = kiwi::get_string(fields, field_ids::NODE_NAME)
            .unwrap_or("")
            .to_string();
        let visible = kiwi::get_bool(fields, field_ids::VISIBLE).unwrap_or(true);
        let opacity = kiwi::get_float(fields, field_ids::OPACITY).unwrap_or(1.0);
        let blend_mode_id = kiwi::get_uint(fields, field_ids::BLEND_MODE).unwrap_or(1);
        let locked = kiwi::get_bool(fields, field_ids::LOCKED).unwrap_or(false);

        let transform = if let Some(tf) = kiwi::get_nested(fields, field_ids::TRANSFORM) {
            self.parse_transform(tf)
        } else {
            Transform2D::identity()
        };

        let size = if let Some(sf) = kiwi::get_nested(fields, field_ids::SIZE) {
            self.parse_size(sf)
        } else {
            Size2D::default()
        };

        let absolute_bounding_box = if let Some(bb) = kiwi::get_nested(fields, field_ids::BOUNDING_BOX) {
            self.parse_bounding_box(bb)
        } else {
            BoundingBox::default()
        };

        Ok(NodeBase {
            id,
            name,
            visible,
            opacity,
            blend_mode: BlendMode::from_figma_id(blend_mode_id),
            transform,
            absolute_bounding_box,
            size,
            locked,
        })
    }

    /// Parse children from the CHILDREN array field.
    fn parse_children(&mut self, fields: &[KiwiField], depth: usize) -> FigmaResult<Vec<FigmaNode>> {
        let children_value = kiwi::find_field(fields, field_ids::CHILDREN);
        match children_value {
            Some(KiwiValue::Array(items)) => {
                let mut children = Vec::with_capacity(items.len());
                for item in items {
                    if let KiwiValue::Nested(child_fields) = item {
                        children.push(self.build_node_tree(child_fields, depth + 1)?);
                    }
                }
                Ok(children)
            }
            _ => Ok(Vec::new()),
        }
    }

    /// Parse type-specific node data.
    fn parse_node_data(
        &self,
        fields: &[KiwiField],
        node_type: NodeType,
    ) -> FigmaResult<NodeData> {
        match node_type {
            NodeType::Document => Ok(NodeData::Document),

            NodeType::Canvas => {
                let bg = if let Some(pf) = kiwi::get_nested(fields, field_ids::BACKGROUND_COLOR) {
                    self.parse_paint(pf)?
                } else {
                    Paint::solid(Color::from_rgba8(242, 242, 242, 255))
                };
                Ok(NodeData::Canvas { background_color: bg })
            }

            NodeType::Frame | NodeType::Section => {
                let fills = self.parse_paints_array(fields)?;
                let strokes = self.parse_strokes_array(fields)?;
                let effects = self.parse_effects_array(fields)?;
                let corner_radii = self.parse_corner_radii(fields);
                let constraints = self.parse_constraints(fields);

                if node_type == NodeType::Section {
                    return Ok(NodeData::Section { fills });
                }

                Ok(NodeData::Frame {
                    fills,
                    strokes,
                    stroke_weight: kiwi::get_float(fields, field_ids::STROKE_WEIGHT)
                        .unwrap_or(0.0),
                    stroke_align: self.parse_stroke_align(fields),
                    effects,
                    corner_radii,
                    constraints,
                    auto_layout: self.parse_auto_layout(fields),
                    clip_content: kiwi::get_bool(fields, field_ids::CLIP_CONTENT)
                        .unwrap_or(true),
                })
            }

            NodeType::Group => Ok(NodeData::Group),

            NodeType::Rectangle => {
                let fills = self.parse_paints_array(fields)?;
                let strokes = self.parse_strokes_array(fields)?;
                let effects = self.parse_effects_array(fields)?;

                Ok(NodeData::Rectangle {
                    fills,
                    strokes,
                    stroke_weight: kiwi::get_float(fields, field_ids::STROKE_WEIGHT)
                        .unwrap_or(0.0),
                    stroke_align: self.parse_stroke_align(fields),
                    stroke_cap: self.parse_stroke_cap(fields),
                    stroke_join: self.parse_stroke_join(fields),
                    effects,
                    corner_radii: self.parse_corner_radii(fields),
                    constraints: self.parse_constraints(fields),
                })
            }

            NodeType::Ellipse => {
                let fills = self.parse_paints_array(fields)?;
                let strokes = self.parse_strokes_array(fields)?;
                let effects = self.parse_effects_array(fields)?;

                Ok(NodeData::Ellipse {
                    fills,
                    strokes,
                    stroke_weight: kiwi::get_float(fields, field_ids::STROKE_WEIGHT)
                        .unwrap_or(0.0),
                    stroke_align: self.parse_stroke_align(fields),
                    effects,
                    arc_data: self.parse_arc_data(fields),
                    constraints: self.parse_constraints(fields),
                })
            }

            NodeType::Line => {
                let strokes = self.parse_strokes_array(fields)?;
                let effects = self.parse_effects_array(fields)?;

                Ok(NodeData::Line {
                    strokes,
                    stroke_weight: kiwi::get_float(fields, field_ids::STROKE_WEIGHT)
                        .unwrap_or(1.0),
                    stroke_cap: self.parse_stroke_cap(fields),
                    effects,
                    constraints: self.parse_constraints(fields),
                })
            }

            NodeType::Star => {
                let fills = self.parse_paints_array(fields)?;
                let strokes = self.parse_strokes_array(fields)?;
                let effects = self.parse_effects_array(fields)?;

                Ok(NodeData::Star {
                    fills,
                    strokes,
                    stroke_weight: kiwi::get_float(fields, field_ids::STROKE_WEIGHT)
                        .unwrap_or(0.0),
                    effects,
                    point_count: kiwi::get_uint(fields, field_ids::POINT_COUNT)
                        .unwrap_or(5) as u32,
                    inner_radius_ratio: kiwi::get_float(fields, field_ids::INNER_RADIUS_RATIO)
                        .unwrap_or(0.382),
                    constraints: self.parse_constraints(fields),
                })
            }

            NodeType::RegularPolygon => {
                let fills = self.parse_paints_array(fields)?;
                let strokes = self.parse_strokes_array(fields)?;
                let effects = self.parse_effects_array(fields)?;

                Ok(NodeData::RegularPolygon {
                    fills,
                    strokes,
                    stroke_weight: kiwi::get_float(fields, field_ids::STROKE_WEIGHT)
                        .unwrap_or(0.0),
                    effects,
                    point_count: kiwi::get_uint(fields, field_ids::POINT_COUNT)
                        .unwrap_or(3) as u32,
                    constraints: self.parse_constraints(fields),
                })
            }

            NodeType::Text => {
                let fills = self.parse_paints_array(fields)?;
                let strokes = self.parse_strokes_array(fields)?;
                let effects = self.parse_effects_array(fields)?;
                let characters = kiwi::get_string(fields, field_ids::CHARACTERS)
                    .unwrap_or("")
                    .to_string();
                let style = self.parse_text_style(fields);

                Ok(NodeData::Text {
                    characters,
                    style,
                    fills,
                    strokes,
                    stroke_weight: kiwi::get_float(fields, field_ids::STROKE_WEIGHT)
                        .unwrap_or(0.0),
                    effects,
                    constraints: self.parse_constraints(fields),
                })
            }

            NodeType::Vector | NodeType::BooleanOperation => {
                let fills = self.parse_paints_array(fields)?;
                let strokes = self.parse_strokes_array(fields)?;
                let effects = self.parse_effects_array(fields)?;

                if node_type == NodeType::BooleanOperation {
                    let op_id = kiwi::get_uint(fields, field_ids::BOOLEAN_OP).unwrap_or(0);
                    let operation = match op_id {
                        0 => BooleanOp::Union,
                        1 => BooleanOp::Intersect,
                        2 => BooleanOp::Subtract,
                        3 => BooleanOp::Exclude,
                        _ => BooleanOp::Union,
                    };
                    return Ok(NodeData::BooleanOp {
                        operation,
                        fills,
                        strokes,
                        stroke_weight: kiwi::get_float(fields, field_ids::STROKE_WEIGHT)
                            .unwrap_or(0.0),
                        effects,
                    });
                }

                Ok(NodeData::VectorNode {
                    fills,
                    strokes,
                    stroke_weight: kiwi::get_float(fields, field_ids::STROKE_WEIGHT)
                        .unwrap_or(0.0),
                    stroke_align: self.parse_stroke_align(fields),
                    stroke_cap: self.parse_stroke_cap(fields),
                    stroke_join: self.parse_stroke_join(fields),
                    effects,
                    paths: self.parse_vector_paths(fields),
                    constraints: self.parse_constraints(fields),
                })
            }

            NodeType::Component => {
                let fills = self.parse_paints_array(fields)?;
                let strokes = self.parse_strokes_array(fields)?;
                let effects = self.parse_effects_array(fields)?;

                Ok(NodeData::Component {
                    fills,
                    strokes,
                    stroke_weight: kiwi::get_float(fields, field_ids::STROKE_WEIGHT)
                        .unwrap_or(0.0),
                    stroke_align: self.parse_stroke_align(fields),
                    effects,
                    corner_radii: self.parse_corner_radii(fields),
                    constraints: self.parse_constraints(fields),
                    description: kiwi::get_string(fields, field_ids::DESCRIPTION)
                        .unwrap_or("")
                        .to_string(),
                })
            }

            NodeType::ComponentSet => {
                Ok(NodeData::ComponentSet {
                    description: kiwi::get_string(fields, field_ids::DESCRIPTION)
                        .unwrap_or("")
                        .to_string(),
                })
            }

            NodeType::Instance => {
                let fills = self.parse_paints_array(fields)?;
                let strokes = self.parse_strokes_array(fields)?;
                let effects = self.parse_effects_array(fields)?;

                Ok(NodeData::Instance {
                    component_id: kiwi::get_string(fields, field_ids::COMPONENT_ID)
                        .unwrap_or("")
                        .to_string(),
                    fills,
                    strokes,
                    stroke_weight: kiwi::get_float(fields, field_ids::STROKE_WEIGHT)
                        .unwrap_or(0.0),
                    effects,
                    constraints: self.parse_constraints(fields),
                })
            }

            NodeType::Slice => {
                Ok(NodeData::Slice {
                    constraints: self.parse_constraints(fields),
                })
            }

            _ => {
                // For unsupported types (Sticky, ShapeWithText, Connector),
                // fall back to Group-like behavior
                Ok(NodeData::Group)
            }
        }
    }

    // ─── Sub-parsers ─────────────────────────────────────────────

    fn parse_transform(&self, fields: &[KiwiField]) -> Transform2D {
        Transform2D {
            a: kiwi::get_float(fields, field_ids::TRANSFORM_A).unwrap_or(1.0),
            b: kiwi::get_float(fields, field_ids::TRANSFORM_B).unwrap_or(0.0),
            c: kiwi::get_float(fields, field_ids::TRANSFORM_C).unwrap_or(0.0),
            d: kiwi::get_float(fields, field_ids::TRANSFORM_D).unwrap_or(1.0),
            tx: kiwi::get_float(fields, field_ids::TRANSFORM_TX).unwrap_or(0.0),
            ty: kiwi::get_float(fields, field_ids::TRANSFORM_TY).unwrap_or(0.0),
        }
    }

    fn parse_size(&self, fields: &[KiwiField]) -> Size2D {
        Size2D {
            width: kiwi::get_float(fields, field_ids::WIDTH).unwrap_or(0.0),
            height: kiwi::get_float(fields, field_ids::HEIGHT).unwrap_or(0.0),
        }
    }

    fn parse_bounding_box(&self, fields: &[KiwiField]) -> BoundingBox {
        BoundingBox {
            x: kiwi::get_float(fields, field_ids::BB_X).unwrap_or(0.0),
            y: kiwi::get_float(fields, field_ids::BB_Y).unwrap_or(0.0),
            width: kiwi::get_float(fields, field_ids::BB_W).unwrap_or(0.0),
            height: kiwi::get_float(fields, field_ids::BB_H).unwrap_or(0.0),
        }
    }

    fn parse_paint(&self, fields: &[KiwiField]) -> FigmaResult<Paint> {
        let paint_type_id = kiwi::get_uint(fields, field_ids::PAINT_TYPE).unwrap_or(0);
        let visible = kiwi::get_bool(fields, field_ids::PAINT_VISIBLE).unwrap_or(true);
        let opacity = kiwi::get_float(fields, field_ids::PAINT_OPACITY).unwrap_or(1.0);

        let color = {
            let r = kiwi::get_float(fields, field_ids::COLOR_R).unwrap_or(0.0);
            let g = kiwi::get_float(fields, field_ids::COLOR_G).unwrap_or(0.0);
            let b = kiwi::get_float(fields, field_ids::COLOR_B).unwrap_or(0.0);
            let a = kiwi::get_float(fields, field_ids::COLOR_A).unwrap_or(1.0);
            Color::new(r, g, b, a)
        };

        let paint_type = match paint_type_id {
            0 => PaintType::Solid,
            1 => PaintType::LinearGradient,
            2 => PaintType::RadialGradient,
            3 => PaintType::AngularGradient,
            4 => PaintType::DiamondGradient,
            5 => PaintType::Image,
            _ => PaintType::Solid,
        };

        let image_ref = kiwi::get_string(fields, field_ids::IMAGE_REF)
            .map(|s| s.to_string());

        let scale_mode_id = kiwi::get_uint(fields, field_ids::SCALE_MODE).unwrap_or(0);
        let scale_mode = match scale_mode_id {
            0 => ScaleMode::Fill,
            1 => ScaleMode::Fit,
            2 => ScaleMode::Tile,
            3 => ScaleMode::Stretch,
            _ => ScaleMode::Fill,
        };

        Ok(Paint {
            paint_type,
            visible,
            opacity,
            color: Some(color),
            gradient_stops: Vec::new(), // TODO: parse gradient stops
            gradient_handles: Vec::new(),
            image_ref,
            scale_mode,
        })
    }

    fn parse_paints_array(&self, fields: &[KiwiField]) -> FigmaResult<Vec<Paint>> {
        if !self.options.parse_paints {
            return Ok(Vec::new());
        }
        let fills_value = kiwi::find_field(fields, field_ids::FILLS);
        match fills_value {
            Some(KiwiValue::Array(items)) => {
                let mut paints = Vec::with_capacity(items.len());
                for item in items {
                    if let KiwiValue::Nested(pf) = item {
                        paints.push(self.parse_paint(pf)?);
                    }
                }
                Ok(paints)
            }
            _ => Ok(Vec::new()),
        }
    }

    fn parse_strokes_array(&self, fields: &[KiwiField]) -> FigmaResult<Vec<Paint>> {
        if !self.options.parse_paints {
            return Ok(Vec::new());
        }
        let strokes_value = kiwi::find_field(fields, field_ids::STROKES);
        match strokes_value {
            Some(KiwiValue::Array(items)) => {
                let mut paints = Vec::with_capacity(items.len());
                for item in items {
                    if let KiwiValue::Nested(pf) = item {
                        paints.push(self.parse_paint(pf)?);
                    }
                }
                Ok(paints)
            }
            _ => Ok(Vec::new()),
        }
    }

    fn parse_effect(&self, fields: &[KiwiField]) -> FigmaResult<Effect> {
        let effect_type_id = kiwi::get_uint(fields, field_ids::EFFECT_TYPE).unwrap_or(1);
        let effect_type = EffectType::from_figma_id(effect_type_id)
            .unwrap_or(EffectType::DropShadow);

        let visible = kiwi::get_bool(fields, field_ids::EFFECT_VISIBLE).unwrap_or(true);
        let radius = kiwi::get_float(fields, field_ids::EFFECT_RADIUS).unwrap_or(0.0);
        let spread = kiwi::get_float(fields, field_ids::EFFECT_SPREAD).unwrap_or(0.0);

        let color = if let Some(cf) = kiwi::get_nested(fields, field_ids::EFFECT_COLOR) {
            let r = kiwi::get_float(cf, field_ids::COLOR_R).unwrap_or(0.0);
            let g = kiwi::get_float(cf, field_ids::COLOR_G).unwrap_or(0.0);
            let b = kiwi::get_float(cf, field_ids::COLOR_B).unwrap_or(0.0);
            let a = kiwi::get_float(cf, field_ids::COLOR_A).unwrap_or(0.25);
            Some(Color::new(r, g, b, a))
        } else {
            None
        };

        let offset_x = kiwi::get_float(fields, field_ids::EFFECT_OFFSET_X).unwrap_or(0.0);
        let offset_y = kiwi::get_float(fields, field_ids::EFFECT_OFFSET_Y).unwrap_or(0.0);
        let offset = if offset_x.abs() > f32::EPSILON || offset_y.abs() > f32::EPSILON {
            Some(Vector2D::new(offset_x, offset_y))
        } else {
            None
        };

        Ok(Effect {
            effect_type,
            visible,
            radius,
            color,
            offset,
            spread,
        })
    }

    fn parse_effects_array(&self, fields: &[KiwiField]) -> FigmaResult<Vec<Effect>> {
        if !self.options.parse_effects {
            return Ok(Vec::new());
        }
        let effects_value = kiwi::find_field(fields, field_ids::EFFECTS);
        match effects_value {
            Some(KiwiValue::Array(items)) => {
                let mut effects = Vec::with_capacity(items.len());
                for item in items {
                    if let KiwiValue::Nested(ef) = item {
                        effects.push(self.parse_effect(ef)?);
                    }
                }
                Ok(effects)
            }
            _ => Ok(Vec::new()),
        }
    }

    fn parse_stroke_align(&self, fields: &[KiwiField]) -> StrokeAlign {
        match kiwi::get_uint(fields, field_ids::STROKE_ALIGN).unwrap_or(2) {
            0 => StrokeAlign::Inside,
            1 => StrokeAlign::Outside,
            _ => StrokeAlign::Center,
        }
    }

    fn parse_stroke_cap(&self, fields: &[KiwiField]) -> StrokeCap {
        match kiwi::get_uint(fields, field_ids::STROKE_CAP).unwrap_or(0) {
            0 => StrokeCap::None,
            1 => StrokeCap::Round,
            2 => StrokeCap::Square,
            _ => StrokeCap::None,
        }
    }

    fn parse_stroke_join(&self, fields: &[KiwiField]) -> StrokeJoin {
        match kiwi::get_uint(fields, field_ids::STROKE_JOIN).unwrap_or(0) {
            0 => StrokeJoin::Miter,
            1 => StrokeJoin::Bevel,
            2 => StrokeJoin::Round,
            _ => StrokeJoin::Miter,
        }
    }

    fn parse_corner_radii(&self, fields: &[KiwiField]) -> CornerRadii {
        if let Some(KiwiValue::Array(items)) = kiwi::find_field(fields, field_ids::CORNER_RADII) {
            let radii: Vec<f32> = items
                .iter()
                .filter_map(|v| v.as_float())
                .collect();
            match radii.len() {
                1 => CornerRadii::uniform(radii[0]),
                4 => CornerRadii::per_corner(radii[0], radii[1], radii[2], radii[3]),
                _ => CornerRadii::default(),
            }
        } else {
            CornerRadii::default()
        }
    }

    fn parse_constraints(&self, fields: &[KiwiField]) -> Constraints {
        if let Some(cf) = kiwi::get_nested(fields, field_ids::CONSTRAINTS) {
            let h = kiwi::get_uint(cf, 1).unwrap_or(0);
            let v = kiwi::get_uint(cf, 2).unwrap_or(0);
            Constraints {
                horizontal: self.constraint_from_id(h),
                vertical: self.constraint_from_id(v),
            }
        } else {
            Constraints::default()
        }
    }

    fn constraint_from_id(&self, id: u64) -> ConstraintType {
        match id {
            0 => ConstraintType::Min,
            1 => ConstraintType::Max,
            2 => ConstraintType::Stretch,
            3 => ConstraintType::Center,
            4 => ConstraintType::Scale,
            5 => ConstraintType::Fixed,
            _ => ConstraintType::Min,
        }
    }

    fn parse_arc_data(&self, fields: &[KiwiField]) -> ArcData {
        if let Some(af) = kiwi::get_nested(fields, field_ids::ARC_DATA) {
            ArcData {
                starting_angle: kiwi::get_float(af, 1).unwrap_or(0.0),
                ending_angle: kiwi::get_float(af, 2).unwrap_or(std::f32::consts::TAU),
                inner_radius: kiwi::get_float(af, 3).unwrap_or(0.0),
            }
        } else {
            ArcData::default()
        }
    }

    fn parse_text_style(&self, fields: &[KiwiField]) -> TextStyle {
        if !self.options.parse_text_styles {
            return TextStyle::default();
        }
        if let Some(ts) = kiwi::get_nested(fields, field_ids::TEXT_STYLE) {
            TextStyle {
                font_family: kiwi::get_string(ts, field_ids::FONT_FAMILY)
                    .unwrap_or("Inter")
                    .to_string(),
                font_weight: kiwi::get_uint(ts, field_ids::FONT_WEIGHT).unwrap_or(400) as u32,
                font_size: kiwi::get_float(ts, field_ids::FONT_SIZE).unwrap_or(14.0),
                italic: kiwi::get_bool(ts, field_ids::FONT_ITALIC).unwrap_or(false),
                line_height: kiwi::get_float(ts, field_ids::LINE_HEIGHT),
                letter_spacing: kiwi::get_float(ts, field_ids::LETTER_SPACING).unwrap_or(0.0),
                text_align: match kiwi::get_uint(ts, field_ids::TEXT_ALIGN).unwrap_or(0) {
                    0 => TextAlign::Left,
                    1 => TextAlign::Center,
                    2 => TextAlign::Right,
                    3 => TextAlign::Justified,
                    _ => TextAlign::Left,
                },
                text_decoration: TextDecoration::None,
            }
        } else {
            TextStyle::default()
        }
    }

    fn parse_auto_layout(&self, fields: &[KiwiField]) -> Option<AutoLayout> {
        let af = kiwi::get_nested(fields, field_ids::AUTO_LAYOUT)?;
        let direction = match kiwi::get_uint(af, 1).unwrap_or(0) {
            0 => LayoutDirection::Horizontal,
            _ => LayoutDirection::Vertical,
        };
        Some(AutoLayout {
            direction,
            item_spacing: kiwi::get_float(af, 2).unwrap_or(0.0),
            padding_top: kiwi::get_float(af, 3).unwrap_or(0.0),
            padding_right: kiwi::get_float(af, 4).unwrap_or(0.0),
            padding_bottom: kiwi::get_float(af, 5).unwrap_or(0.0),
            padding_left: kiwi::get_float(af, 6).unwrap_or(0.0),
            primary_align: LayoutAlign::Min,
            counter_align: LayoutAlign::Min,
        })
    }

    fn parse_vector_paths(&self, fields: &[KiwiField]) -> Vec<VectorPath> {
        let paths_value = kiwi::find_field(fields, field_ids::PATHS);
        match paths_value {
            Some(KiwiValue::Array(items)) => {
                items
                    .iter()
                    .filter_map(|item| {
                        if let KiwiValue::Nested(pf) = item {
                            let data = kiwi::get_string(pf, field_ids::PATH_DATA)
                                .unwrap_or("")
                                .to_string();
                            let winding = match kiwi::get_uint(pf, field_ids::WINDING_RULE).unwrap_or(0) {
                                0 => WindingRule::NonZero,
                                _ => WindingRule::EvenOdd,
                            };
                            Some(VectorPath {
                                data,
                                winding_rule: winding,
                            })
                        } else {
                            None
                        }
                    })
                    .collect()
            }
            _ => Vec::new(),
        }
    }
}

impl Default for FigmaParser {
    fn default() -> Self {
        Self::new()
    }
}

/// Decompress a zlib-compressed payload.
fn decompress_payload(compressed: &[u8], expected_size: usize) -> FigmaResult<Vec<u8>> {
    let mut decoder = ZlibDecoder::new(compressed);
    let mut decompressed = Vec::with_capacity(expected_size);
    decoder
        .read_to_end(&mut decompressed)
        .map_err(|e| FigmaError::DecompressionFailed(e.to_string()))?;
    Ok(decompressed)
}

#[cfg(test)]
#[allow(unused_variables)]
mod tests {
    use super::*;
    use crate::format::kiwi::KiwiEncoder;

    /// Helper: build a minimal .fig file with the given Kiwi payload.
    fn build_fig_file(payload_fields: &[KiwiField]) -> Vec<u8> {
        // Encode the Kiwi payload
        let mut enc = KiwiEncoder::new();
        for f in payload_fields {
            enc.write_field(f.id, &f.value);
        }
        enc.write_terminator();
        let uncompressed = enc.into_bytes();

        // Compress with zlib
        use flate2::write::ZlibEncoder;
        use flate2::Compression;
        use std::io::Write;

        let mut compressor = ZlibEncoder::new(Vec::new(), Compression::default());
        compressor.write_all(&uncompressed).unwrap();
        let compressed = compressor.finish().unwrap();

        // Build header
        let header = FigHeader {
            version: 1,
            schema_version: 1,
            compressed_length: compressed.len() as u32,
            uncompressed_length: uncompressed.len() as u32,
        };

        let mut file_data = header.to_bytes();
        file_data.extend_from_slice(&compressed);
        file_data
    }

    fn kf(id: u32, value: KiwiValue) -> KiwiField {
        KiwiField { id, value }
    }

    #[test]
    fn test_parse_document_root() {
        let fig_data = build_fig_file(&[
            kf(field_ids::NODE_TYPE, KiwiValue::UInt(0)), // DOCUMENT
            kf(field_ids::NODE_ID, KiwiValue::String("0:0".into())),
            kf(field_ids::NODE_NAME, KiwiValue::String("Document".into())),
        ]);

        let mut parser = FigmaParser::new();
        let node = parser.parse(&fig_data).unwrap();
        assert_eq!(node.node_type, NodeType::Document);
        assert_eq!(node.base.name, "Document");
        assert_eq!(parser.stats().total_nodes, 1);
    }

    #[test]
    fn test_parse_rectangle() {
        let fig_data = build_fig_file(&[
            kf(field_ids::NODE_TYPE, KiwiValue::UInt(10)), // RECTANGLE
            kf(field_ids::NODE_ID, KiwiValue::String("1:1".into())),
            kf(field_ids::NODE_NAME, KiwiValue::String("Rect 1".into())),
            kf(field_ids::VISIBLE, KiwiValue::Bool(true)),
            kf(field_ids::OPACITY, KiwiValue::Float(0.8)),
            kf(
                field_ids::SIZE,
                KiwiValue::Nested(vec![
                    kf(field_ids::WIDTH, KiwiValue::Float(100.0)),
                    kf(field_ids::HEIGHT, KiwiValue::Float(50.0)),
                ]),
            ),
            kf(
                field_ids::BOUNDING_BOX,
                KiwiValue::Nested(vec![
                    kf(field_ids::BB_X, KiwiValue::Float(10.0)),
                    kf(field_ids::BB_Y, KiwiValue::Float(20.0)),
                    kf(field_ids::BB_W, KiwiValue::Float(100.0)),
                    kf(field_ids::BB_H, KiwiValue::Float(50.0)),
                ]),
            ),
        ]);

        let mut parser = FigmaParser::new();
        let node = parser.parse(&fig_data).unwrap();

        assert_eq!(node.node_type, NodeType::Rectangle);
        assert_eq!(node.base.name, "Rect 1");
        assert!(node.base.visible);
        assert!((node.base.opacity - 0.8).abs() < 0.001);
        assert!((node.base.size.width - 100.0).abs() < 0.001);
        assert!((node.base.size.height - 50.0).abs() < 0.001);
        assert!((node.base.absolute_bounding_box.x - 10.0).abs() < 0.001);
        assert_eq!(parser.stats().rectangles, 1);
    }

    #[test]
    fn test_parse_ellipse() {
        let fig_data = build_fig_file(&[
            kf(field_ids::NODE_TYPE, KiwiValue::UInt(8)), // ELLIPSE
            kf(field_ids::NODE_ID, KiwiValue::String("1:2".into())),
            kf(field_ids::NODE_NAME, KiwiValue::String("Circle".into())),
            kf(
                field_ids::SIZE,
                KiwiValue::Nested(vec![
                    kf(field_ids::WIDTH, KiwiValue::Float(80.0)),
                    kf(field_ids::HEIGHT, KiwiValue::Float(80.0)),
                ]),
            ),
        ]);

        let mut parser = FigmaParser::new();
        let node = parser.parse(&fig_data).unwrap();
        assert_eq!(node.node_type, NodeType::Ellipse);
        assert_eq!(node.base.name, "Circle");
        assert_eq!(parser.stats().ellipses, 1);
    }

    #[test]
    fn test_parse_text() {
        let fig_data = build_fig_file(&[
            kf(field_ids::NODE_TYPE, KiwiValue::UInt(11)), // TEXT
            kf(field_ids::NODE_NAME, KiwiValue::String("Label".into())),
            kf(field_ids::CHARACTERS, KiwiValue::String("Hello World".into())),
            kf(
                field_ids::TEXT_STYLE,
                KiwiValue::Nested(vec![
                    kf(field_ids::FONT_FAMILY, KiwiValue::String("Roboto".into())),
                    kf(field_ids::FONT_SIZE, KiwiValue::Float(24.0)),
                    kf(field_ids::FONT_WEIGHT, KiwiValue::UInt(700)),
                ]),
            ),
        ]);

        let mut parser = FigmaParser::new();
        let node = parser.parse(&fig_data).unwrap();
        assert_eq!(node.node_type, NodeType::Text);
        match &node.data {
            NodeData::Text {
                characters, style, ..
            } => {
                assert_eq!(characters, "Hello World");
                assert_eq!(style.font_family, "Roboto");
                assert!((style.font_size - 24.0).abs() < 0.001);
                assert_eq!(style.font_weight, 700);
            }
            _ => panic!("wrong data type"),
        }
        assert_eq!(parser.stats().texts, 1);
    }

    #[test]
    fn test_parse_frame_with_children() {
        let child_rect = KiwiValue::Nested(vec![
            kf(field_ids::NODE_TYPE, KiwiValue::UInt(10)),
            kf(field_ids::NODE_NAME, KiwiValue::String("Child Rect".into())),
            kf(
                field_ids::SIZE,
                KiwiValue::Nested(vec![
                    kf(field_ids::WIDTH, KiwiValue::Float(50.0)),
                    kf(field_ids::HEIGHT, KiwiValue::Float(50.0)),
                ]),
            ),
        ]);

        let child_ellipse = KiwiValue::Nested(vec![
            kf(field_ids::NODE_TYPE, KiwiValue::UInt(8)),
            kf(field_ids::NODE_NAME, KiwiValue::String("Child Circle".into())),
        ]);

        let fig_data = build_fig_file(&[
            kf(field_ids::NODE_TYPE, KiwiValue::UInt(2)), // FRAME
            kf(field_ids::NODE_NAME, KiwiValue::String("Frame 1".into())),
            kf(
                field_ids::SIZE,
                KiwiValue::Nested(vec![
                    kf(field_ids::WIDTH, KiwiValue::Float(375.0)),
                    kf(field_ids::HEIGHT, KiwiValue::Float(812.0)),
                ]),
            ),
            kf(
                field_ids::CHILDREN,
                KiwiValue::Array(vec![child_rect, child_ellipse]),
            ),
        ]);

        let mut parser = FigmaParser::new();
        let node = parser.parse(&fig_data).unwrap();

        assert_eq!(node.node_type, NodeType::Frame);
        assert_eq!(node.children.len(), 2);
        assert_eq!(node.children[0].base.name, "Child Rect");
        assert_eq!(node.children[1].base.name, "Child Circle");
        assert_eq!(parser.stats().total_nodes, 3);
        assert_eq!(parser.stats().frames, 1);
        assert_eq!(parser.stats().rectangles, 1);
        assert_eq!(parser.stats().ellipses, 1);
        assert_eq!(parser.stats().max_depth, 1);
    }

    #[test]
    fn test_parse_with_fills() {
        let fill = KiwiValue::Nested(vec![
            kf(field_ids::PAINT_TYPE, KiwiValue::UInt(0)), // SOLID
            kf(field_ids::PAINT_VISIBLE, KiwiValue::Bool(true)),
            kf(field_ids::COLOR_R, KiwiValue::Float(1.0)),
            kf(field_ids::COLOR_G, KiwiValue::Float(0.0)),
            kf(field_ids::COLOR_B, KiwiValue::Float(0.0)),
            kf(field_ids::COLOR_A, KiwiValue::Float(1.0)),
        ]);

        let fig_data = build_fig_file(&[
            kf(field_ids::NODE_TYPE, KiwiValue::UInt(10)),
            kf(field_ids::NODE_NAME, KiwiValue::String("Red Rect".into())),
            kf(field_ids::FILLS, KiwiValue::Array(vec![fill])),
        ]);

        let mut parser = FigmaParser::new();
        let node = parser.parse(&fig_data).unwrap();

        let fills = node.fills();
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].paint_type, PaintType::Solid);
        let color = fills[0].color.unwrap();
        assert!((color.r - 1.0).abs() < 0.001);
        assert!(color.g.abs() < 0.001);
    }

    #[test]
    fn test_parse_with_effects() {
        let shadow = KiwiValue::Nested(vec![
            kf(field_ids::EFFECT_TYPE, KiwiValue::UInt(1)), // DROP_SHADOW
            kf(field_ids::EFFECT_VISIBLE, KiwiValue::Bool(true)),
            kf(field_ids::EFFECT_RADIUS, KiwiValue::Float(8.0)),
            kf(field_ids::EFFECT_OFFSET_X, KiwiValue::Float(0.0)),
            kf(field_ids::EFFECT_OFFSET_Y, KiwiValue::Float(4.0)),
            kf(field_ids::EFFECT_SPREAD, KiwiValue::Float(0.0)),
        ]);

        let fig_data = build_fig_file(&[
            kf(field_ids::NODE_TYPE, KiwiValue::UInt(10)),
            kf(field_ids::EFFECTS, KiwiValue::Array(vec![shadow])),
        ]);

        let mut parser = FigmaParser::new();
        let node = parser.parse(&fig_data).unwrap();

        let effects = node.effects();
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0].effect_type, EffectType::DropShadow);
        assert!((effects[0].radius - 8.0).abs() < 0.001);
    }

    #[test]
    fn test_parse_with_transform() {
        let fig_data = build_fig_file(&[
            kf(field_ids::NODE_TYPE, KiwiValue::UInt(10)),
            kf(
                field_ids::TRANSFORM,
                KiwiValue::Nested(vec![
                    kf(field_ids::TRANSFORM_A, KiwiValue::Float(2.0)),
                    kf(field_ids::TRANSFORM_D, KiwiValue::Float(2.0)),
                    kf(field_ids::TRANSFORM_TX, KiwiValue::Float(100.0)),
                    kf(field_ids::TRANSFORM_TY, KiwiValue::Float(200.0)),
                ]),
            ),
        ]);

        let mut parser = FigmaParser::new();
        let node = parser.parse(&fig_data).unwrap();
        let t = &node.base.transform;
        assert!((t.a - 2.0).abs() < 0.001);
        assert!((t.tx - 100.0).abs() < 0.001);
        assert!(!t.is_identity());
    }

    #[test]
    fn test_parse_invalid_magic() {
        let data = b"not-a-fig-file-at-all!!!!";
        let mut parser = FigmaParser::new();
        let err = parser.parse(data).unwrap_err();
        assert!(matches!(err, FigmaError::InvalidMagic(_)));
    }

    #[test]
    fn test_parse_truncated() {
        let data = b"fig-kiwi";
        let mut parser = FigmaParser::new();
        let err = parser.parse(data).unwrap_err();
        assert!(matches!(err, FigmaError::UnexpectedEof(_)));
    }

    #[test]
    fn test_parse_node_limit() {
        let fig_data = build_fig_file(&[
            kf(field_ids::NODE_TYPE, KiwiValue::UInt(2)),
            kf(
                field_ids::CHILDREN,
                KiwiValue::Array(vec![
                    KiwiValue::Nested(vec![kf(field_ids::NODE_TYPE, KiwiValue::UInt(10))]),
                    KiwiValue::Nested(vec![kf(field_ids::NODE_TYPE, KiwiValue::UInt(10))]),
                    KiwiValue::Nested(vec![kf(field_ids::NODE_TYPE, KiwiValue::UInt(10))]),
                ]),
            ),
        ]);

        let mut parser = FigmaParser::with_options(ImportOptions {
            max_nodes: 2,
            ..Default::default()
        });
        // Should fail because we hit the limit after 2 nodes (frame + first child)
        let err = parser.parse(&fig_data).unwrap_err();
        assert!(err.to_string().contains("node limit"));
    }

    #[test]
    fn test_parse_stats_summary() {
        let fig_data = build_fig_file(&[
            kf(field_ids::NODE_TYPE, KiwiValue::UInt(0)), // DOCUMENT
            kf(
                field_ids::CHILDREN,
                KiwiValue::Array(vec![KiwiValue::Nested(vec![
                    kf(field_ids::NODE_TYPE, KiwiValue::UInt(1)), // CANVAS
                    kf(
                        field_ids::CHILDREN,
                        KiwiValue::Array(vec![
                            KiwiValue::Nested(vec![kf(field_ids::NODE_TYPE, KiwiValue::UInt(10))]), // RECT
                            KiwiValue::Nested(vec![kf(field_ids::NODE_TYPE, KiwiValue::UInt(8))]),  // ELLIPSE
                            KiwiValue::Nested(vec![
                                kf(field_ids::NODE_TYPE, KiwiValue::UInt(11)), // TEXT
                                kf(field_ids::CHARACTERS, KiwiValue::String("Hi".into())),
                            ]),
                        ]),
                    ),
                ])]),
            ),
        ]);

        let mut parser = FigmaParser::new();
        let node = parser.parse(&fig_data).unwrap();
        assert_eq!(parser.stats().total_nodes, 5);
        assert_eq!(parser.stats().rectangles, 1);
        assert_eq!(parser.stats().ellipses, 1);
        assert_eq!(parser.stats().texts, 1);
        assert_eq!(parser.stats().max_depth, 2);

        let summary = parser.stats().summary();
        assert!(summary.contains("5 nodes"));
    }

    #[test]
    fn test_parse_group() {
        let fig_data = build_fig_file(&[
            kf(field_ids::NODE_TYPE, KiwiValue::UInt(3)), // GROUP
            kf(field_ids::NODE_NAME, KiwiValue::String("Group 1".into())),
        ]);

        let mut parser = FigmaParser::new();
        let node = parser.parse(&fig_data).unwrap();
        assert_eq!(node.node_type, NodeType::Group);
        assert!(matches!(node.data, NodeData::Group));
    }

    #[test]
    fn test_parse_component() {
        let fig_data = build_fig_file(&[
            kf(field_ids::NODE_TYPE, KiwiValue::UInt(13)), // COMPONENT
            kf(field_ids::NODE_NAME, KiwiValue::String("Button".into())),
            kf(field_ids::DESCRIPTION, KiwiValue::String("A primary button".into())),
        ]);

        let mut parser = FigmaParser::new();
        let node = parser.parse(&fig_data).unwrap();
        assert_eq!(node.node_type, NodeType::Component);
        match &node.data {
            NodeData::Component { description, .. } => {
                assert_eq!(description, "A primary button");
            }
            _ => panic!("wrong data type"),
        }
    }

    #[test]
    fn test_parse_instance() {
        let fig_data = build_fig_file(&[
            kf(field_ids::NODE_TYPE, KiwiValue::UInt(15)), // INSTANCE
            kf(field_ids::COMPONENT_ID, KiwiValue::String("42:1".into())),
        ]);

        let mut parser = FigmaParser::new();
        let node = parser.parse(&fig_data).unwrap();
        assert_eq!(node.node_type, NodeType::Instance);
        match &node.data {
            NodeData::Instance { component_id, .. } => {
                assert_eq!(component_id, "42:1");
            }
            _ => panic!("wrong data type"),
        }
    }

    #[test]
    fn test_parse_line() {
        let fig_data = build_fig_file(&[
            kf(field_ids::NODE_TYPE, KiwiValue::UInt(7)), // LINE
            kf(field_ids::STROKE_WEIGHT, KiwiValue::Float(2.0)),
        ]);

        let mut parser = FigmaParser::new();
        let node = parser.parse(&fig_data).unwrap();
        assert_eq!(node.node_type, NodeType::Line);
        match &node.data {
            NodeData::Line { stroke_weight, .. } => {
                assert!((stroke_weight - 2.0).abs() < 0.001);
            }
            _ => panic!("wrong data type"),
        }
    }

    #[test]
    fn test_parse_star() {
        let fig_data = build_fig_file(&[
            kf(field_ids::NODE_TYPE, KiwiValue::UInt(6)), // STAR
            kf(field_ids::POINT_COUNT, KiwiValue::UInt(5)),
            kf(field_ids::INNER_RADIUS_RATIO, KiwiValue::Float(0.5)),
        ]);

        let mut parser = FigmaParser::new();
        let node = parser.parse(&fig_data).unwrap();
        assert_eq!(node.node_type, NodeType::Star);
        match &node.data {
            NodeData::Star {
                point_count,
                inner_radius_ratio,
                ..
            } => {
                assert_eq!(*point_count, 5);
                assert!((*inner_radius_ratio - 0.5).abs() < 0.001);
            }
            _ => panic!("wrong data type"),
        }
    }

    #[test]
    fn test_parse_corner_radii_uniform() {
        let fig_data = build_fig_file(&[
            kf(field_ids::NODE_TYPE, KiwiValue::UInt(10)),
            kf(
                field_ids::CORNER_RADII,
                KiwiValue::Array(vec![KiwiValue::Float(10.0)]),
            ),
        ]);

        let mut parser = FigmaParser::new();
        let node = parser.parse(&fig_data).unwrap();
        match &node.data {
            NodeData::Rectangle { corner_radii, .. } => {
                assert!(corner_radii.is_uniform());
                assert!((corner_radii.top_left - 10.0).abs() < 0.001);
            }
            _ => panic!("wrong data type"),
        }
    }

    #[test]
    fn test_parse_corner_radii_per_corner() {
        let fig_data = build_fig_file(&[
            kf(field_ids::NODE_TYPE, KiwiValue::UInt(10)),
            kf(
                field_ids::CORNER_RADII,
                KiwiValue::Array(vec![
                    KiwiValue::Float(5.0),
                    KiwiValue::Float(10.0),
                    KiwiValue::Float(15.0),
                    KiwiValue::Float(20.0),
                ]),
            ),
        ]);

        let mut parser = FigmaParser::new();
        let node = parser.parse(&fig_data).unwrap();
        match &node.data {
            NodeData::Rectangle { corner_radii, .. } => {
                assert!(!corner_radii.is_uniform());
                assert!((corner_radii.top_left - 5.0).abs() < 0.001);
                assert!((corner_radii.bottom_left - 20.0).abs() < 0.001);
            }
            _ => panic!("wrong data type"),
        }
    }

    #[test]
    fn test_parse_boolean_operation() {
        let fig_data = build_fig_file(&[
            kf(field_ids::NODE_TYPE, KiwiValue::UInt(5)), // BOOLEAN_OPERATION
            kf(field_ids::BOOLEAN_OP, KiwiValue::UInt(2)), // SUBTRACT
        ]);

        let mut parser = FigmaParser::new();
        let node = parser.parse(&fig_data).unwrap();
        match &node.data {
            NodeData::BooleanOp { operation, .. } => {
                assert_eq!(*operation, BooleanOp::Subtract);
            }
            _ => panic!("wrong data type"),
        }
    }

    #[test]
    fn test_parse_without_paints() {
        let fill = KiwiValue::Nested(vec![
            kf(field_ids::PAINT_TYPE, KiwiValue::UInt(0)),
            kf(field_ids::COLOR_R, KiwiValue::Float(1.0)),
        ]);

        let fig_data = build_fig_file(&[
            kf(field_ids::NODE_TYPE, KiwiValue::UInt(10)),
            kf(field_ids::FILLS, KiwiValue::Array(vec![fill])),
        ]);

        let mut parser = FigmaParser::with_options(ImportOptions {
            parse_paints: false,
            ..Default::default()
        });
        let node = parser.parse(&fig_data).unwrap();
        assert!(node.fills().is_empty());
    }

    #[test]
    fn test_decompress_valid() {
        use flate2::write::ZlibEncoder;
        use flate2::Compression;
        use std::io::Write;

        let original = b"Hello, world! This is test data.";
        let mut compressor = ZlibEncoder::new(Vec::new(), Compression::default());
        compressor.write_all(original).unwrap();
        let compressed = compressor.finish().unwrap();

        let result = decompress_payload(&compressed, original.len()).unwrap();
        assert_eq!(result, original);
    }

    #[test]
    fn test_decompress_invalid() {
        let bad_data = b"this is not compressed data";
        let result = decompress_payload(bad_data, 100);
        assert!(result.is_err());
    }
}
