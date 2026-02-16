//! Test fixture generation for .fig files.
//!
//! Generates synthetic .fig files with known content for testing
//! the parser and converter without requiring real Figma files.

use crate::format::header::FigHeader;
use crate::format::kiwi::{KiwiEncoder, KiwiField, KiwiValue};
use crate::parser::field_ids;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::io::Write;

/// Available test fixture types.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TestFixture {
    /// A single rectangle node.
    SingleRectangle,
    /// A single ellipse node.
    SingleEllipse,
    /// Basic shapes: rectangle, ellipse, text.
    BasicShapes,
    /// A frame containing nested children.
    NestedFrames,
    /// A mobile app screen with header, content, cards.
    MobileAppScreen,
    /// Nodes with fills, strokes, and effects.
    StyledShapes,
    /// A component with instances.
    ComponentsAndInstances,
    /// Large document with many nodes (stress test).
    LargeDocument,
    /// Minimal document (just root + empty canvas).
    Minimal,
}

/// Generate a synthetic .fig file for the given fixture type.
pub fn generate_test_fig(fixture: TestFixture) -> Vec<u8> {
    let fields = match fixture {
        TestFixture::SingleRectangle => build_single_rectangle(),
        TestFixture::SingleEllipse => build_single_ellipse(),
        TestFixture::BasicShapes => build_basic_shapes(),
        TestFixture::NestedFrames => build_nested_frames(),
        TestFixture::MobileAppScreen => build_mobile_app_screen(),
        TestFixture::StyledShapes => build_styled_shapes(),
        TestFixture::ComponentsAndInstances => build_components(),
        TestFixture::LargeDocument => build_large_document(),
        TestFixture::Minimal => build_minimal(),
    };

    encode_fig_file(&fields)
}

/// Encode Kiwi fields into a complete .fig file.
fn encode_fig_file(fields: &[KiwiField]) -> Vec<u8> {
    // Encode Kiwi payload
    let mut enc = KiwiEncoder::new();
    for f in fields {
        enc.write_field(f.id, &f.value);
    }
    enc.write_terminator();
    let uncompressed = enc.into_bytes();

    // Compress
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

    let mut file = header.to_bytes();
    file.extend_from_slice(&compressed);
    file
}

// ─── Helpers ────────────────────────────────────────────────────

fn kf(id: u32, value: KiwiValue) -> KiwiField {
    KiwiField { id, value }
}

fn node_fields(
    node_type: u8,
    id: &str,
    name: &str,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) -> Vec<KiwiField> {
    vec![
        kf(field_ids::NODE_TYPE, KiwiValue::UInt(node_type as u64)),
        kf(field_ids::NODE_ID, KiwiValue::String(id.into())),
        kf(field_ids::NODE_NAME, KiwiValue::String(name.into())),
        kf(field_ids::VISIBLE, KiwiValue::Bool(true)),
        kf(field_ids::OPACITY, KiwiValue::Float(1.0)),
        kf(
            field_ids::SIZE,
            KiwiValue::Nested(vec![
                kf(field_ids::WIDTH, KiwiValue::Float(w)),
                kf(field_ids::HEIGHT, KiwiValue::Float(h)),
            ]),
        ),
        kf(
            field_ids::BOUNDING_BOX,
            KiwiValue::Nested(vec![
                kf(field_ids::BB_X, KiwiValue::Float(x)),
                kf(field_ids::BB_Y, KiwiValue::Float(y)),
                kf(field_ids::BB_W, KiwiValue::Float(w)),
                kf(field_ids::BB_H, KiwiValue::Float(h)),
            ]),
        ),
        kf(
            field_ids::TRANSFORM,
            KiwiValue::Nested(vec![
                kf(field_ids::TRANSFORM_A, KiwiValue::Float(1.0)),
                kf(field_ids::TRANSFORM_B, KiwiValue::Float(0.0)),
                kf(field_ids::TRANSFORM_C, KiwiValue::Float(0.0)),
                kf(field_ids::TRANSFORM_D, KiwiValue::Float(1.0)),
                kf(field_ids::TRANSFORM_TX, KiwiValue::Float(x)),
                kf(field_ids::TRANSFORM_TY, KiwiValue::Float(y)),
            ]),
        ),
    ]
}

fn make_node(node_type: u8, id: &str, name: &str, x: f32, y: f32, w: f32, h: f32) -> KiwiValue {
    KiwiValue::Nested(node_fields(node_type, id, name, x, y, w, h))
}

fn make_rect(id: &str, name: &str, x: f32, y: f32, w: f32, h: f32) -> KiwiValue {
    make_node(10, id, name, x, y, w, h)
}

fn make_ellipse(id: &str, name: &str, x: f32, y: f32, w: f32, h: f32) -> KiwiValue {
    make_node(8, id, name, x, y, w, h)
}

fn make_text(
    id: &str,
    name: &str,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    characters: &str,
) -> KiwiValue {
    let mut fields = node_fields(11, id, name, x, y, w, h);
    fields.push(kf(
        field_ids::CHARACTERS,
        KiwiValue::String(characters.into()),
    ));
    fields.push(kf(
        field_ids::TEXT_STYLE,
        KiwiValue::Nested(vec![
            kf(field_ids::FONT_FAMILY, KiwiValue::String("Inter".into())),
            kf(field_ids::FONT_SIZE, KiwiValue::Float(14.0)),
            kf(field_ids::FONT_WEIGHT, KiwiValue::UInt(400)),
        ]),
    ));
    KiwiValue::Nested(fields)
}

fn make_frame(
    id: &str,
    name: &str,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    children: Vec<KiwiValue>,
) -> KiwiValue {
    let mut fields = node_fields(2, id, name, x, y, w, h);
    if !children.is_empty() {
        fields.push(kf(field_ids::CHILDREN, KiwiValue::Array(children)));
    }
    KiwiValue::Nested(fields)
}

fn make_solid_fill(r: f32, g: f32, b: f32, a: f32) -> KiwiValue {
    KiwiValue::Nested(vec![
        kf(field_ids::PAINT_TYPE, KiwiValue::UInt(0)),
        kf(field_ids::PAINT_VISIBLE, KiwiValue::Bool(true)),
        kf(field_ids::PAINT_OPACITY, KiwiValue::Float(1.0)),
        kf(field_ids::COLOR_R, KiwiValue::Float(r)),
        kf(field_ids::COLOR_G, KiwiValue::Float(g)),
        kf(field_ids::COLOR_B, KiwiValue::Float(b)),
        kf(field_ids::COLOR_A, KiwiValue::Float(a)),
    ])
}

fn make_drop_shadow(radius: f32, offset_y: f32) -> KiwiValue {
    KiwiValue::Nested(vec![
        kf(field_ids::EFFECT_TYPE, KiwiValue::UInt(1)),
        kf(field_ids::EFFECT_VISIBLE, KiwiValue::Bool(true)),
        kf(field_ids::EFFECT_RADIUS, KiwiValue::Float(radius)),
        kf(field_ids::EFFECT_OFFSET_X, KiwiValue::Float(0.0)),
        kf(field_ids::EFFECT_OFFSET_Y, KiwiValue::Float(offset_y)),
        kf(field_ids::EFFECT_SPREAD, KiwiValue::Float(0.0)),
    ])
}

fn wrap_document(canvas_children: Vec<KiwiValue>) -> Vec<KiwiField> {
    let canvas = {
        let mut fields = node_fields(1, "0:1", "Page 1", 0.0, 0.0, 0.0, 0.0);
        if !canvas_children.is_empty() {
            fields.push(kf(
                field_ids::CHILDREN,
                KiwiValue::Array(canvas_children),
            ));
        }
        KiwiValue::Nested(fields)
    };

    let mut doc_fields = node_fields(0, "0:0", "Document", 0.0, 0.0, 0.0, 0.0);
    doc_fields.push(kf(
        field_ids::CHILDREN,
        KiwiValue::Array(vec![canvas]),
    ));
    doc_fields
}

fn wrap_document_named(page_name: &str, canvas_children: Vec<KiwiValue>) -> Vec<KiwiField> {
    let canvas = {
        let mut fields = node_fields(1, "0:1", page_name, 0.0, 0.0, 0.0, 0.0);
        if !canvas_children.is_empty() {
            fields.push(kf(
                field_ids::CHILDREN,
                KiwiValue::Array(canvas_children),
            ));
        }
        KiwiValue::Nested(fields)
    };

    let mut doc_fields = node_fields(0, "0:0", "Document", 0.0, 0.0, 0.0, 0.0);
    doc_fields.push(kf(
        field_ids::CHILDREN,
        KiwiValue::Array(vec![canvas]),
    ));
    doc_fields
}

// ─── Fixture builders ───────────────────────────────────────────

fn build_single_rectangle() -> Vec<KiwiField> {
    wrap_document(vec![make_rect("1:1", "Rectangle 1", 100.0, 100.0, 200.0, 150.0)])
}

fn build_single_ellipse() -> Vec<KiwiField> {
    wrap_document(vec![make_ellipse("1:1", "Ellipse 1", 50.0, 50.0, 120.0, 120.0)])
}

fn build_basic_shapes() -> Vec<KiwiField> {
    wrap_document(vec![
        make_rect("1:1", "Rectangle 1", 10.0, 10.0, 200.0, 100.0),
        make_ellipse("1:2", "Circle 1", 230.0, 10.0, 100.0, 100.0),
        make_text("1:3", "Label", 10.0, 130.0, 300.0, 24.0, "Hello, Logos!"),
    ])
}

fn build_nested_frames() -> Vec<KiwiField> {
    let inner_frame = make_frame(
        "2:1",
        "Inner Frame",
        20.0,
        20.0,
        260.0,
        260.0,
        vec![
            make_rect("3:1", "Nested Rect", 10.0, 10.0, 80.0, 80.0),
            make_ellipse("3:2", "Nested Circle", 100.0, 10.0, 80.0, 80.0),
        ],
    );

    let outer_frame = make_frame(
        "1:1",
        "Outer Frame",
        0.0,
        0.0,
        300.0,
        300.0,
        vec![inner_frame],
    );

    wrap_document(vec![outer_frame])
}

fn build_mobile_app_screen() -> Vec<KiwiField> {
    let status_bar = make_rect("2:1", "Status Bar", 0.0, 0.0, 393.0, 54.0);
    let time = make_text("2:2", "Time", 20.0, 15.0, 50.0, 20.0, "9:41");

    let header = make_frame(
        "2:3",
        "Header",
        0.0,
        54.0,
        393.0,
        64.0,
        vec![
            make_text("3:1", "Title", 16.0, 16.0, 200.0, 32.0, "Home"),
        ],
    );

    let content = make_frame(
        "2:4",
        "Content",
        0.0,
        118.0,
        393.0,
        734.0,
        vec![
            make_rect("3:2", "Card 1", 16.0, 16.0, 361.0, 200.0),
            make_rect("3:3", "Card 2", 16.0, 232.0, 361.0, 200.0),
            make_ellipse("3:4", "Avatar", 16.0, 448.0, 48.0, 48.0),
            make_text("3:5", "Username", 80.0, 458.0, 200.0, 20.0, "Jane Doe"),
        ],
    );

    let tab_bar = make_frame(
        "2:5",
        "Tab Bar",
        0.0,
        772.0,
        393.0, 
        80.0,
        vec![
            make_text("4:1", "Home Tab", 30.0, 10.0, 60.0, 30.0, "Home"),
            make_text("4:2", "Search Tab", 130.0, 10.0, 60.0, 30.0, "Search"),
            make_text("4:3", "Profile Tab", 250.0, 10.0, 60.0, 30.0, "Profile"),
        ],
    );

    let screen = make_frame(
        "1:1",
        "iPhone 14",
        0.0,
        0.0,
        393.0,
        852.0,
        vec![status_bar, time, header, content, tab_bar],
    );

    wrap_document_named("Home Screen", vec![screen])
}

fn build_styled_shapes() -> Vec<KiwiField> {
    // Rectangle with red fill and drop shadow
    let mut rect_fields = node_fields(10, "1:1", "Styled Rect", 10.0, 10.0, 200.0, 100.0);
    rect_fields.push(kf(
        field_ids::FILLS,
        KiwiValue::Array(vec![make_solid_fill(1.0, 0.0, 0.0, 1.0)]),
    ));
    rect_fields.push(kf(
        field_ids::STROKES,
        KiwiValue::Array(vec![make_solid_fill(0.0, 0.0, 0.0, 1.0)]),
    ));
    rect_fields.push(kf(field_ids::STROKE_WEIGHT, KiwiValue::Float(2.0)));
    rect_fields.push(kf(
        field_ids::EFFECTS,
        KiwiValue::Array(vec![make_drop_shadow(8.0, 4.0)]),
    ));
    rect_fields.push(kf(
        field_ids::CORNER_RADII,
        KiwiValue::Array(vec![
            KiwiValue::Float(8.0),
            KiwiValue::Float(8.0),
            KiwiValue::Float(8.0),
            KiwiValue::Float(8.0),
        ]),
    ));
    let styled_rect = KiwiValue::Nested(rect_fields);

    // Ellipse with blue fill
    let mut ellipse_fields = node_fields(8, "1:2", "Blue Circle", 230.0, 10.0, 100.0, 100.0);
    ellipse_fields.push(kf(
        field_ids::FILLS,
        KiwiValue::Array(vec![make_solid_fill(0.0, 0.0, 1.0, 1.0)]),
    ));
    let styled_ellipse = KiwiValue::Nested(ellipse_fields);

    // Text with custom style
    let mut text_fields = node_fields(11, "1:3", "Bold Text", 10.0, 130.0, 300.0, 32.0);
    text_fields.push(kf(
        field_ids::CHARACTERS,
        KiwiValue::String("Styled Text".into()),
    ));
    text_fields.push(kf(
        field_ids::TEXT_STYLE,
        KiwiValue::Nested(vec![
            kf(field_ids::FONT_FAMILY, KiwiValue::String("Roboto".into())),
            kf(field_ids::FONT_SIZE, KiwiValue::Float(24.0)),
            kf(field_ids::FONT_WEIGHT, KiwiValue::UInt(700)),
            kf(field_ids::FONT_ITALIC, KiwiValue::Bool(true)),
            kf(field_ids::LINE_HEIGHT, KiwiValue::Float(32.0)),
            kf(field_ids::LETTER_SPACING, KiwiValue::Float(0.5)),
            kf(field_ids::TEXT_ALIGN, KiwiValue::UInt(1)), // Center
        ]),
    ));
    text_fields.push(kf(
        field_ids::FILLS,
        KiwiValue::Array(vec![make_solid_fill(0.2, 0.2, 0.2, 1.0)]),
    ));
    let styled_text = KiwiValue::Nested(text_fields);

    wrap_document(vec![styled_rect, styled_ellipse, styled_text])
}

fn build_components() -> Vec<KiwiField> {
    // Component definition: Button
    let mut button_fields = node_fields(13, "10:1", "Button", 0.0, 0.0, 120.0, 40.0);
    button_fields.push(kf(
        field_ids::DESCRIPTION,
        KiwiValue::String("A primary action button".into()),
    ));
    button_fields.push(kf(
        field_ids::FILLS,
        KiwiValue::Array(vec![make_solid_fill(0.0, 0.5, 1.0, 1.0)]),
    ));
    button_fields.push(kf(
        field_ids::CHILDREN,
        KiwiValue::Array(vec![make_text(
            "10:2",
            "Button Label",
            20.0,
            10.0,
            80.0,
            20.0,
            "Click Me",
        )]),
    ));
    let button_component = KiwiValue::Nested(button_fields);

    // Instance 1
    let mut inst1_fields = node_fields(15, "20:1", "Button Instance 1", 50.0, 50.0, 120.0, 40.0);
    inst1_fields.push(kf(
        field_ids::COMPONENT_ID,
        KiwiValue::String("10:1".into()),
    ));
    let instance1 = KiwiValue::Nested(inst1_fields);

    // Instance 2
    let mut inst2_fields =
        node_fields(15, "20:2", "Button Instance 2", 50.0, 110.0, 120.0, 40.0);
    inst2_fields.push(kf(
        field_ids::COMPONENT_ID,
        KiwiValue::String("10:1".into()),
    ));
    let instance2 = KiwiValue::Nested(inst2_fields);

    wrap_document(vec![button_component, instance1, instance2])
}

fn build_large_document() -> Vec<KiwiField> {
    let mut children = Vec::with_capacity(100);
    for i in 0..100 {
        let x = (i % 10) as f32 * 110.0;
        let y = (i / 10) as f32 * 110.0;
        let id = format!("1:{}", i + 1);
        let name = format!("Rect {}", i + 1);

        if i % 3 == 0 {
            children.push(make_rect(&id, &name, x, y, 100.0, 100.0));
        } else if i % 3 == 1 {
            children.push(make_ellipse(&id, &name, x, y, 100.0, 100.0));
        } else {
            children.push(make_text(
                &id,
                &name,
                x,
                y,
                100.0,
                100.0,
                &format!("Text {}", i + 1),
            ));
        }
    }

    wrap_document(children)
}

fn build_minimal() -> Vec<KiwiField> {
    wrap_document(vec![])
}

/// Get the raw file size for a given fixture.
pub fn fixture_size(fixture: TestFixture) -> usize {
    generate_test_fig(fixture).len()
}

#[cfg(test)]
#[allow(unused_variables)]
mod tests {
    use super::*;
    use crate::FigmaParser;

    #[test]
    fn test_fixture_single_rect_parseable() {
        let data = generate_test_fig(TestFixture::SingleRectangle);
        let mut parser = FigmaParser::new();
        let node = parser.parse(&data).unwrap();
        assert_eq!(node.node_type, crate::model::node::NodeType::Document);
        assert_eq!(parser.stats().total_nodes, 3); // doc + canvas + rect
    }

    #[test]
    fn test_fixture_single_ellipse_parseable() {
        let data = generate_test_fig(TestFixture::SingleEllipse);
        let mut parser = FigmaParser::new();
        let node = parser.parse(&data).unwrap();
        assert_eq!(parser.stats().ellipses, 1);
    }

    #[test]
    fn test_fixture_basic_shapes() {
        let data = generate_test_fig(TestFixture::BasicShapes);
        let mut parser = FigmaParser::new();
        let node = parser.parse(&data).unwrap();
        assert_eq!(parser.stats().rectangles, 1);
        assert_eq!(parser.stats().ellipses, 1);
        assert_eq!(parser.stats().texts, 1);
    }

    #[test]
    fn test_fixture_nested_frames() {
        let data = generate_test_fig(TestFixture::NestedFrames);
        let mut parser = FigmaParser::new();
        let node = parser.parse(&data).unwrap();
        assert_eq!(parser.stats().frames, 2);
        assert_eq!(parser.stats().max_depth, 4); // doc > canvas > outer > inner > shapes
    }

    #[test]
    fn test_fixture_mobile_app_screen() {
        let data = generate_test_fig(TestFixture::MobileAppScreen);
        let mut parser = FigmaParser::new();
        let node = parser.parse(&data).unwrap();

        assert!(parser.stats().total_nodes >= 15);
        assert!(parser.stats().frames >= 4);
        assert!(parser.stats().texts >= 5);
    }

    #[test]
    fn test_fixture_styled_shapes() {
        let data = generate_test_fig(TestFixture::StyledShapes);
        let mut parser = FigmaParser::new();
        let node = parser.parse(&data).unwrap();

        // Find the styled rectangle
        let canvas = &node.children[0];
        let styled_rect = &canvas.children[0];
        assert_eq!(styled_rect.fills().len(), 1);
        assert_eq!(styled_rect.strokes().len(), 1);
        assert_eq!(styled_rect.effects().len(), 1);

        // Check text style
        let text_node = &canvas.children[2];
        match &text_node.data {
            crate::model::node::NodeData::Text { style, characters, .. } => {
                assert_eq!(characters, "Styled Text");
                assert_eq!(style.font_family, "Roboto");
                assert!((style.font_size - 24.0).abs() < 0.001);
                assert_eq!(style.font_weight, 700);
                assert!(style.italic);
            }
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn test_fixture_components() {
        let data = generate_test_fig(TestFixture::ComponentsAndInstances);
        let mut parser = FigmaParser::new();
        let node = parser.parse(&data).unwrap();

        assert_eq!(parser.stats().components, 1);
        assert_eq!(parser.stats().instances, 2);
    }

    #[test]
    fn test_fixture_large_document() {
        let data = generate_test_fig(TestFixture::LargeDocument);
        let mut parser = FigmaParser::new();
        let node = parser.parse(&data).unwrap();

        assert_eq!(parser.stats().total_nodes, 102); // doc + canvas + 100 shapes
    }

    #[test]
    fn test_fixture_minimal() {
        let data = generate_test_fig(TestFixture::Minimal);
        let mut parser = FigmaParser::new();
        let node = parser.parse(&data).unwrap();
        assert_eq!(parser.stats().total_nodes, 2); // doc + canvas
    }

    #[test]
    fn test_fixture_sizes() {
        let sizes: Vec<(TestFixture, usize)> = vec![
            TestFixture::Minimal,
            TestFixture::SingleRectangle,
            TestFixture::BasicShapes,
            TestFixture::NestedFrames,
            TestFixture::MobileAppScreen,
            TestFixture::StyledShapes,
            TestFixture::ComponentsAndInstances,
            TestFixture::LargeDocument,
        ]
        .into_iter()
        .map(|f| (f, fixture_size(f)))
        .collect();

        // All should be non-zero
        for (fixture, size) in &sizes {
            assert!(*size > 24, "{:?} too small: {}", fixture, size);
        }

        // Large should be largest
        let large_size = fixture_size(TestFixture::LargeDocument);
        let minimal_size = fixture_size(TestFixture::Minimal);
        assert!(large_size > minimal_size * 5);
    }
}
