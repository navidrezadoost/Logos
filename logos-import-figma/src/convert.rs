//! Conversion from Figma nodes to logos-core Document types.
//!
//! This module bridges the Figma import model to the internal Logos
//! document representation used by the rendering engine and CRDT layer.

use crate::error::{FigmaError, FigmaResult};
use crate::model::node::{FigmaNode, NodeData, NodeType};
use logos_core::{
    Document, EllipseLayer, FrameLayer, Layer, Page, PathCommand, PathLayer, Point, Rect,
    RectLayer, TextLayer,
};
use uuid::Uuid;

/// Options for controlling the conversion process.
#[derive(Debug, Clone)]
pub struct ConvertOptions {
    /// Whether to preserve original Figma node IDs (as deterministic UUIDs).
    pub preserve_ids: bool,
    /// Whether to flatten groups into their parent.
    pub flatten_groups: bool,
    /// Maximum tree depth to convert (0 = unlimited).
    pub max_depth: usize,
}

impl Default for ConvertOptions {
    fn default() -> Self {
        Self {
            preserve_ids: true,
            flatten_groups: false,
            max_depth: 0,
        }
    }
}

/// Statistics from a conversion operation.
#[derive(Debug, Clone, Default)]
pub struct ConvertStats {
    pub layers_created: usize,
    pub nodes_skipped: usize,
    pub nodes_unsupported: usize,
    pub pages_created: usize,
}

/// Converts a parsed Figma document tree to a logos-core Document.
pub struct FigmaConverter {
    options: ConvertOptions,
    stats: ConvertStats,
}

impl FigmaConverter {
    pub fn new() -> Self {
        Self {
            options: ConvertOptions::default(),
            stats: ConvertStats::default(),
        }
    }

    pub fn with_options(options: ConvertOptions) -> Self {
        Self {
            options,
            stats: ConvertStats::default(),
        }
    }

    pub fn stats(&self) -> &ConvertStats {
        &self.stats
    }

    /// Convert a full Figma document tree to a logos-core Document.
    pub fn convert(&mut self, figma_doc: &FigmaNode) -> FigmaResult<Document> {
        self.stats = ConvertStats::default();

        if figma_doc.node_type != NodeType::Document {
            return Err(FigmaError::ConversionError(
                "root node must be DOCUMENT type".into(),
            ));
        }

        let doc = Document::new();

        // Process the first canvas (page) if it exists
        if let Some(canvas) = figma_doc.children.first() {
            if canvas.node_type == NodeType::Canvas {
                let layers = self.convert_children(&canvas.children, 0)?;
                self.stats.pages_created = 1;

                // Set the page name
                {
                    let mut page = doc.root.write().map_err(|e| {
                        FigmaError::ConversionError(format!("lock error: {e}"))
                    })?;
                    page.name = canvas.base.name.clone();
                    page.layers = layers;
                }
            }
        }

        Ok(doc)
    }

    /// Convert a single Figma page (Canvas) to a logos-core Page.
    pub fn convert_page(&mut self, canvas: &FigmaNode) -> FigmaResult<Page> {
        if canvas.node_type != NodeType::Canvas {
            return Err(FigmaError::ConversionError(
                "node must be CANVAS type".into(),
            ));
        }

        self.stats = ConvertStats::default();
        self.stats.pages_created = 1;

        let layers = self.convert_children(&canvas.children, 0)?;

        Ok(Page {
            id: self.make_uuid(&canvas.base.id),
            name: canvas.base.name.clone(),
            layers,
            spatial_index: None,
        })
    }

    /// Convert a single Figma node to a logos-core Layer.
    pub fn convert_node(&mut self, node: &FigmaNode) -> FigmaResult<Option<Layer>> {
        self.convert_single(node, 0)
    }

    /// Convert children of a container node.
    fn convert_children(
        &mut self,
        children: &[FigmaNode],
        depth: usize,
    ) -> FigmaResult<Vec<Layer>> {
        let mut layers = Vec::with_capacity(children.len());

        for child in children {
            if let Some(layer) = self.convert_single(child, depth)? {
                layers.push(layer);
            }
        }

        Ok(layers)
    }

    /// Convert a single node to a Layer (or None if unsupported/skipped).
    fn convert_single(
        &mut self,
        node: &FigmaNode,
        depth: usize,
    ) -> FigmaResult<Option<Layer>> {
        // Check depth limit
        if self.options.max_depth > 0 && depth >= self.options.max_depth {
            self.stats.nodes_skipped += 1;
            return Ok(None);
        }

        // Skip invisible nodes
        if !node.base.visible {
            self.stats.nodes_skipped += 1;
            return Ok(None);
        }

        let layer = match node.node_type {
            NodeType::Rectangle => {
                let rect = self.convert_rect(node);
                self.stats.layers_created += 1;
                Some(Layer::Rect(rect))
            }

            NodeType::Ellipse => {
                let ellipse = self.convert_ellipse(node);
                self.stats.layers_created += 1;
                Some(Layer::Ellipse(ellipse))
            }

            NodeType::Text => {
                let text = self.convert_text(node);
                self.stats.layers_created += 1;
                Some(Layer::Text(text))
            }

            NodeType::Frame | NodeType::Component | NodeType::Instance | NodeType::Section => {
                let frame = self.convert_frame(node, depth)?;
                self.stats.layers_created += 1;
                Some(Layer::Frame(frame))
            }

            NodeType::Group => {
                if self.options.flatten_groups {
                    // Flatten: add children directly
                    let children = self.convert_children(&node.children, depth)?;
                    // Return the first child or skip if empty
                    if children.len() == 1 {
                        return Ok(children.into_iter().next());
                    } else if children.is_empty() {
                        self.stats.nodes_skipped += 1;
                        return Ok(None);
                    } else {
                        // Wrap in a frame
                        let frame = FrameLayer {
                            id: self.make_uuid(&node.base.id),
                            children,
                            bounds: self.convert_bounds(node),
                        };
                        self.stats.layers_created += 1;
                        return Ok(Some(Layer::Frame(frame)));
                    }
                } else {
                    let frame = self.convert_frame(node, depth)?;
                    self.stats.layers_created += 1;
                    Some(Layer::Frame(frame))
                }
            }

            NodeType::Line | NodeType::Vector => {
                let path = self.convert_vector(node);
                self.stats.layers_created += 1;
                Some(Layer::Path(path))
            }

            NodeType::Star | NodeType::RegularPolygon => {
                // Approximate as ellipse bounding shape
                let ellipse = self.convert_ellipse(node);
                self.stats.layers_created += 1;
                Some(Layer::Ellipse(ellipse))
            }

            NodeType::BooleanOperation => {
                // Convert boolean ops as a frame containing children
                let frame = self.convert_frame(node, depth)?;
                self.stats.layers_created += 1;
                Some(Layer::Frame(frame))
            }

            _ => {
                self.stats.nodes_unsupported += 1;
                None
            }
        };

        Ok(layer)
    }

    fn convert_rect(&self, node: &FigmaNode) -> RectLayer {
        RectLayer {
            id: self.make_uuid(&node.base.id),
            bounds: self.convert_bounds(node),
            corner_radius: 0.0,
            corner_smoothing: 0.0,
        }
    }

    fn convert_ellipse(&self, node: &FigmaNode) -> EllipseLayer {
        EllipseLayer {
            id: self.make_uuid(&node.base.id),
            bounds: self.convert_bounds(node),
        }
    }

    fn convert_text(&self, node: &FigmaNode) -> TextLayer {
        let characters = match &node.data {
            NodeData::Text { characters, .. } => characters.clone(),
            _ => String::new(),
        };

        TextLayer {
            id: self.make_uuid(&node.base.id),
            content: characters,
            bounds: self.convert_bounds(node),
        }
    }

    fn convert_frame(
        &mut self,
        node: &FigmaNode,
        depth: usize,
    ) -> FigmaResult<FrameLayer> {
        let children = self.convert_children(&node.children, depth + 1)?;

        Ok(FrameLayer {
            id: self.make_uuid(&node.base.id),
            children,
            bounds: self.convert_bounds(node),
        })
    }

    fn convert_vector(&self, node: &FigmaNode) -> PathLayer {
        let bb = &node.base.absolute_bounding_box;

        // Create a simple path from the bounding box
        // For Line nodes, draw a line; for vectors, approximate with a rect path
        let commands = match node.node_type {
            NodeType::Line => vec![
                PathCommand::MoveTo(Point::new(bb.x, bb.y)),
                PathCommand::LineTo(Point::new(bb.x + bb.width, bb.y + bb.height)),
            ],
            _ => vec![
                PathCommand::MoveTo(Point::new(bb.x, bb.y)),
                PathCommand::LineTo(Point::new(bb.x + bb.width, bb.y)),
                PathCommand::LineTo(Point::new(bb.x + bb.width, bb.y + bb.height)),
                PathCommand::LineTo(Point::new(bb.x, bb.y + bb.height)),
                PathCommand::Close,
            ],
        };

        PathLayer {
            id: self.make_uuid(&node.base.id),
            commands,
            bounds: Rect {
                x: bb.x,
                y: bb.y,
                width: bb.width,
                height: bb.height,
            },
            closed: node.node_type != NodeType::Line,
        }
    }

    fn convert_bounds(&self, node: &FigmaNode) -> Rect {
        let bb = &node.base.absolute_bounding_box;
        Rect {
            x: bb.x,
            y: bb.y,
            width: if bb.width > 0.0 {
                bb.width
            } else {
                node.base.size.width
            },
            height: if bb.height > 0.0 {
                bb.height
            } else {
                node.base.size.height
            },
        }
    }

    /// Generate a deterministic UUID from a Figma node ID.
    fn make_uuid(&self, figma_id: &str) -> Uuid {
        if self.options.preserve_ids {
            // Create a deterministic UUID from the Figma ID string
            let hash = simple_hash(figma_id.as_bytes());
            let bytes: [u8; 16] = [
                (hash >> 56) as u8,
                (hash >> 48) as u8,
                (hash >> 40) as u8,
                (hash >> 32) as u8,
                (hash >> 24) as u8,
                (hash >> 16) as u8,
                (hash >> 8) as u8,
                hash as u8,
                // Second half from reversed hash
                (!hash >> 56) as u8,
                (!hash >> 48) as u8,
                (!hash >> 40) as u8,
                (!hash >> 32) as u8,
                (!hash >> 24) as u8,
                (!hash >> 16) as u8,
                (!hash >> 8) as u8,
                (!hash) as u8,
            ];
            // Set UUID version 4 and variant bits
            let mut b = bytes;
            b[6] = (b[6] & 0x0F) | 0x40; // version 4
            b[8] = (b[8] & 0x3F) | 0x80; // variant
            Uuid::from_bytes(b)
        } else {
            Uuid::new_v4()
        }
    }
}

impl Default for FigmaConverter {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple FNV-1a hash for deterministic UUID generation.
fn simple_hash(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::node::FigmaNode;
    use crate::model::paint::{Color, Paint};

    #[test]
    fn test_convert_empty_document() {
        let figma_doc = FigmaNode::document(vec![
            FigmaNode::canvas("0:1", "Page 1", vec![]),
        ]);

        let mut converter = FigmaConverter::new();
        let doc = converter.convert(&figma_doc).unwrap();
        let page = doc.root.read().unwrap();
        assert_eq!(page.name, "Page 1");
        assert_eq!(page.layers.len(), 0);
        assert_eq!(converter.stats().pages_created, 1);
    }

    #[test]
    fn test_convert_single_rectangle() {
        let figma_doc = FigmaNode::document(vec![
            FigmaNode::canvas("0:1", "Page 1", vec![
                FigmaNode::rectangle("1:1", "Rect 1", 10.0, 20.0, 100.0, 50.0),
            ]),
        ]);

        let mut converter = FigmaConverter::new();
        let doc = converter.convert(&figma_doc).unwrap();
        let page = doc.root.read().unwrap();

        assert_eq!(page.layers.len(), 1);
        match &page.layers[0] {
            Layer::Rect(r) => {
                assert!((r.bounds.x - 10.0).abs() < 0.001);
                assert!((r.bounds.y - 20.0).abs() < 0.001);
                assert!((r.bounds.width - 100.0).abs() < 0.001);
                assert!((r.bounds.height - 50.0).abs() < 0.001);
            }
            _ => panic!("expected Rect layer"),
        }
        assert_eq!(converter.stats().layers_created, 1);
    }

    #[test]
    fn test_convert_ellipse() {
        let figma_doc = FigmaNode::document(vec![
            FigmaNode::canvas("0:1", "Page 1", vec![
                FigmaNode::ellipse("1:1", "Circle", 0.0, 0.0, 80.0, 80.0),
            ]),
        ]);

        let mut converter = FigmaConverter::new();
        let doc = converter.convert(&figma_doc).unwrap();
        let page = doc.root.read().unwrap();

        assert_eq!(page.layers.len(), 1);
        assert!(matches!(page.layers[0], Layer::Ellipse(_)));
    }

    #[test]
    fn test_convert_text() {
        let figma_doc = FigmaNode::document(vec![
            FigmaNode::canvas("0:1", "Page 1", vec![
                FigmaNode::text("1:1", "Label", 0.0, 0.0, 200.0, 24.0, "Hello World"),
            ]),
        ]);

        let mut converter = FigmaConverter::new();
        let doc = converter.convert(&figma_doc).unwrap();
        let page = doc.root.read().unwrap();

        assert_eq!(page.layers.len(), 1);
        match &page.layers[0] {
            Layer::Text(t) => {
                assert_eq!(t.content, "Hello World");
            }
            _ => panic!("expected Text layer"),
        }
    }

    #[test]
    fn test_convert_frame_with_children() {
        let figma_doc = FigmaNode::document(vec![
            FigmaNode::canvas("0:1", "Page 1", vec![
                FigmaNode::frame(
                    "1:1",
                    "Frame 1",
                    0.0,
                    0.0,
                    375.0,
                    812.0,
                    vec![
                        FigmaNode::rectangle("2:1", "BG", 0.0, 0.0, 375.0, 812.0),
                        FigmaNode::text("2:2", "Title", 20.0, 40.0, 335.0, 32.0, "Title"),
                    ],
                ),
            ]),
        ]);

        let mut converter = FigmaConverter::new();
        let doc = converter.convert(&figma_doc).unwrap();
        let page = doc.root.read().unwrap();

        assert_eq!(page.layers.len(), 1);
        match &page.layers[0] {
            Layer::Frame(f) => {
                assert_eq!(f.children.len(), 2);
                assert!(matches!(f.children[0], Layer::Rect(_)));
                assert!(matches!(f.children[1], Layer::Text(_)));
            }
            _ => panic!("expected Frame layer"),
        }
        assert_eq!(converter.stats().layers_created, 3);
    }

    #[test]
    fn test_convert_group() {
        let figma_doc = FigmaNode::document(vec![
            FigmaNode::canvas("0:1", "Page 1", vec![
                FigmaNode::group("1:1", "Group 1", vec![
                    FigmaNode::rectangle("2:1", "R1", 0.0, 0.0, 50.0, 50.0),
                ]),
            ]),
        ]);

        let mut converter = FigmaConverter::new();
        let doc = converter.convert(&figma_doc).unwrap();
        let page = doc.root.read().unwrap();

        // Group becomes a Frame
        assert_eq!(page.layers.len(), 1);
        assert!(matches!(page.layers[0], Layer::Frame(_)));
    }

    #[test]
    fn test_convert_flatten_group() {
        let figma_doc = FigmaNode::document(vec![
            FigmaNode::canvas("0:1", "Page 1", vec![
                FigmaNode::group("1:1", "Group 1", vec![
                    FigmaNode::rectangle("2:1", "R1", 0.0, 0.0, 50.0, 50.0),
                ]),
            ]),
        ]);

        let mut converter = FigmaConverter::with_options(ConvertOptions {
            flatten_groups: true,
            ..Default::default()
        });
        let doc = converter.convert(&figma_doc).unwrap();
        let page = doc.root.read().unwrap();

        // Single child group gets flattened to just the child
        assert_eq!(page.layers.len(), 1);
        assert!(matches!(page.layers[0], Layer::Rect(_)));
    }

    #[test]
    fn test_convert_invisible_skipped() {
        let mut rect = FigmaNode::rectangle("1:1", "Hidden", 0.0, 0.0, 50.0, 50.0);
        rect.base.visible = false;

        let figma_doc = FigmaNode::document(vec![
            FigmaNode::canvas("0:1", "Page 1", vec![rect]),
        ]);

        let mut converter = FigmaConverter::new();
        let doc = converter.convert(&figma_doc).unwrap();
        let page = doc.root.read().unwrap();

        assert_eq!(page.layers.len(), 0);
        assert_eq!(converter.stats().nodes_skipped, 1);
    }

    #[test]
    fn test_convert_depth_limit() {
        let deep = FigmaNode::frame(
            "3:1",
            "Deep",
            0.0,
            0.0,
            50.0,
            50.0,
            vec![FigmaNode::rectangle("4:1", "Too Deep", 0.0, 0.0, 25.0, 25.0)],
        );
        let figma_doc = FigmaNode::document(vec![
            FigmaNode::canvas("0:1", "Page 1", vec![
                FigmaNode::frame("1:1", "Frame", 0.0, 0.0, 100.0, 100.0, vec![deep]),
            ]),
        ]);

        let mut converter = FigmaConverter::with_options(ConvertOptions {
            max_depth: 2,
            ..Default::default()
        });
        let doc = converter.convert(&figma_doc).unwrap();
        let page = doc.root.read().unwrap();

        assert_eq!(page.layers.len(), 1);
        match &page.layers[0] {
            Layer::Frame(f) => {
                // Only depth=0 (Frame) and depth=1 (Deep frame) converted
                assert_eq!(f.children.len(), 1);
                match &f.children[0] {
                    Layer::Frame(inner) => {
                        assert_eq!(inner.children.len(), 0); // depth=2 skipped
                    }
                    _ => panic!("expected inner Frame"),
                }
            }
            _ => panic!("expected Frame"),
        }
    }

    #[test]
    fn test_convert_line_to_path() {
        let mut line = FigmaNode::rectangle("1:1", "Line 1", 0.0, 0.0, 100.0, 0.0);
        line.node_type = NodeType::Line;
        line.data = crate::model::node::NodeData::Line {
            strokes: vec![Paint::solid(Color::black())],
            stroke_weight: 2.0,
            stroke_cap: crate::model::paint::StrokeCap::Round,
            effects: vec![],
            constraints: crate::model::node::Constraints::default(),
        };
        line.base.absolute_bounding_box =
            crate::model::transform::BoundingBox::new(10.0, 20.0, 100.0, 0.0);

        let mut converter = FigmaConverter::new();
        let layer = converter.convert_node(&line).unwrap().unwrap();

        match layer {
            Layer::Path(p) => {
                assert_eq!(p.commands.len(), 2);
                assert!(!p.closed);
            }
            _ => panic!("expected Path layer"),
        }
    }

    #[test]
    fn test_convert_preserves_ids_deterministic() {
        let _rect1 = FigmaNode::rectangle("1:1", "R1", 0.0, 0.0, 50.0, 50.0);
        let _rect2 = FigmaNode::rectangle("1:1", "R1", 0.0, 0.0, 50.0, 50.0);

        let converter = FigmaConverter::new();
        let id1 = converter.make_uuid("1:1");
        let id2 = converter.make_uuid("1:1");
        assert_eq!(id1, id2, "same Figma ID should produce same UUID");

        let id3 = converter.make_uuid("2:1");
        assert_ne!(id1, id3, "different Figma IDs should produce different UUIDs");
    }

    #[test]
    fn test_convert_non_document_root_fails() {
        let rect = FigmaNode::rectangle("1:1", "Rect", 0.0, 0.0, 50.0, 50.0);
        let mut converter = FigmaConverter::new();
        assert!(converter.convert(&rect).is_err());
    }

    #[test]
    fn test_convert_page_directly() {
        let canvas = FigmaNode::canvas("0:1", "My Page", vec![
            FigmaNode::rectangle("1:1", "R1", 0.0, 0.0, 50.0, 50.0),
        ]);

        let mut converter = FigmaConverter::new();
        let page = converter.convert_page(&canvas).unwrap();
        assert_eq!(page.name, "My Page");
        assert_eq!(page.layers.len(), 1);
    }

    #[test]
    fn test_convert_page_wrong_type_fails() {
        let rect = FigmaNode::rectangle("1:1", "Rect", 0.0, 0.0, 50.0, 50.0);
        let mut converter = FigmaConverter::new();
        assert!(converter.convert_page(&rect).is_err());
    }

    #[test]
    fn test_convert_complex_layout() {
        // Simulates a typical mobile app screen
        let figma_doc = FigmaNode::document(vec![
            FigmaNode::canvas("0:1", "Home Screen", vec![
                FigmaNode::frame(
                    "1:1",
                    "iPhone 14",
                    0.0,
                    0.0,
                    393.0,
                    852.0,
                    vec![
                        FigmaNode::rectangle("2:1", "Status Bar BG", 0.0, 0.0, 393.0, 54.0),
                        FigmaNode::text("2:2", "Time", 20.0, 15.0, 50.0, 20.0, "9:41"),
                        FigmaNode::frame(
                            "2:3",
                            "Header",
                            0.0,
                            54.0,
                            393.0,
                            64.0,
                            vec![
                                FigmaNode::text("3:1", "Title", 16.0, 16.0, 200.0, 32.0, "Home"),
                            ],
                        ),
                        FigmaNode::frame(
                            "2:4",
                            "Content",
                            0.0,
                            118.0,
                            393.0,
                            734.0,
                            vec![
                                FigmaNode::rectangle("3:2", "Card 1", 16.0, 16.0, 361.0, 200.0),
                                FigmaNode::rectangle("3:3", "Card 2", 16.0, 232.0, 361.0, 200.0),
                                FigmaNode::ellipse("3:4", "Avatar", 16.0, 448.0, 48.0, 48.0),
                            ],
                        ),
                    ],
                ),
            ]),
        ]);

        let mut converter = FigmaConverter::new();
        let doc = converter.convert(&figma_doc).unwrap();
        let page = doc.root.read().unwrap();

        assert_eq!(page.name, "Home Screen");
        assert_eq!(page.layers.len(), 1); // One top-level frame

        let stats = converter.stats();
        assert_eq!(stats.layers_created, 9);
        assert_eq!(stats.pages_created, 1);
    }

    #[test]
    fn test_simple_hash_deterministic() {
        let h1 = simple_hash(b"1:1");
        let h2 = simple_hash(b"1:1");
        assert_eq!(h1, h2);

        let h3 = simple_hash(b"1:2");
        assert_ne!(h1, h3);
    }

    #[test]
    fn test_simple_hash_distribution() {
        // Verify hashes are different for sequential IDs
        let hashes: Vec<u64> = (0..100)
            .map(|i| simple_hash(format!("1:{i}").as_bytes()))
            .collect();
        let unique: std::collections::HashSet<u64> = hashes.iter().copied().collect();
        assert_eq!(unique.len(), 100, "all 100 hashes should be unique");
    }
}
