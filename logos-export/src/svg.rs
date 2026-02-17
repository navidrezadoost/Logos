//! SVG export — generates standalone SVG 1.1 documents from Logos layers.
//!
//! References:
//! - SVG 1.1 Specification, W3C (§5 Shapes, §11 Painting)
//! - Foley et al., Ch. 22 (vector output formats)
//!
//! The exporter produces well-formed XML without external dependencies.
//! All coordinates are in pixels (user units = px at 96 DPI).

use std::fmt::Write;
use std::io;

use logos_core::{Layer, PathCommand};
use logos_layout::engine::LayoutEngine;
use uuid::Uuid;

use crate::{ExportError, ExportPage, collect_export_data};

/// SVG exporter — converts a Logos layer tree to SVG markup.
pub struct SvgExporter {
    page: ExportPage,
    /// Decimal precision for coordinate values (default 2).
    pub precision: usize,
    /// Whether to indent the output for readability.
    pub pretty: bool,
}

impl SvgExporter {
    pub fn new(page: ExportPage) -> Self {
        Self {
            page,
            precision: 2,
            pretty: true,
        }
    }

    pub fn with_precision(mut self, precision: usize) -> Self {
        self.precision = precision;
        self
    }

    pub fn compact(mut self) -> Self {
        self.pretty = false;
        self
    }

    /// Export layers to an SVG string.
    pub fn export_to_string(
        &self,
        engine: &LayoutEngine,
        layers: &[(Uuid, &Layer)],
    ) -> Result<String, ExportError> {
        if layers.is_empty() {
            return Err(ExportError::EmptyDocument);
        }
        if self.page.width <= 0.0 || self.page.height <= 0.0 {
            return Err(ExportError::InvalidDimensions(self.page.width, self.page.height));
        }

        let data = collect_export_data(engine, layers)?;
        let nl = if self.pretty { "\n" } else { "" };
        let indent = if self.pretty { "  " } else { "" };
        let p = self.precision;

        let mut svg = String::with_capacity(layers.len() * 200 + 500);

        // XML declaration
        write!(
            svg,
            r#"<?xml version="1.0" encoding="UTF-8"?>{nl}"#,
            nl = nl
        )
        .unwrap();

        // SVG root element
        write!(
            svg,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w:.0}" height="{h:.0}" viewBox="0 0 {w:.0} {h:.0}">{nl}"#,
            w = self.page.width,
            h = self.page.height,
            nl = nl
        )
        .unwrap();

        // Background rect
        if let Some(bg) = self.page.background {
            write!(
                svg,
                r#"{i}<rect width="{w:.0}" height="{h:.0}" fill="{fill}"/>{nl}"#,
                i = indent,
                w = self.page.width,
                h = self.page.height,
                fill = color_to_css(bg),
                nl = nl
            )
            .unwrap();
        }

        // Layer elements
        for item in &data {
            match item.layer {
                Layer::Rect(_) => {
                    write!(
                        svg,
                        r#"{i}<rect x="{x:.p$}" y="{y:.p$}" width="{w:.p$}" height="{h:.p$}" fill="{fill}" />{nl}"#,
                        i = indent,
                        x = item.x,
                        y = item.y,
                        w = item.width,
                        h = item.height,
                        fill = default_color_for_layer(item.layer),
                        p = p,
                        nl = nl
                    )
                    .unwrap();
                }
                Layer::Ellipse(_) => {
                    let cx = item.x + item.width * 0.5;
                    let cy = item.y + item.height * 0.5;
                    let rx = item.width * 0.5;
                    let ry = item.height * 0.5;
                    write!(
                        svg,
                        r#"{i}<ellipse cx="{cx:.p$}" cy="{cy:.p$}" rx="{rx:.p$}" ry="{ry:.p$}" fill="{fill}" />{nl}"#,
                        i = indent,
                        cx = cx,
                        cy = cy,
                        rx = rx,
                        ry = ry,
                        fill = default_color_for_layer(item.layer),
                        p = p,
                        nl = nl
                    )
                    .unwrap();
                }
                Layer::Text(t) => {
                    let escaped = xml_escape(&t.content);
                    write!(
                        svg,
                        r#"{i}<text x="{x:.p$}" y="{y:.p$}" font-size="16" fill="{fill}">{text}</text>{nl}"#,
                        i = indent,
                        x = item.x,
                        y = item.y + 16.0, // baseline offset
                        fill = default_color_for_layer(item.layer),
                        text = escaped,
                        p = p,
                        nl = nl
                    )
                    .unwrap();
                }
                Layer::Frame(_) => {
                    write!(
                        svg,
                        r#"{i}<rect x="{x:.p$}" y="{y:.p$}" width="{w:.p$}" height="{h:.p$}" fill="{fill}" opacity="0.8" />{nl}"#,
                        i = indent,
                        x = item.x,
                        y = item.y,
                        w = item.width,
                        h = item.height,
                        fill = default_color_for_layer(item.layer),
                        p = p,
                        nl = nl
                    )
                    .unwrap();
                }
                Layer::Path(path_layer) => {
                    let d = path_commands_to_svg_d(&path_layer.commands, p);
                    write!(
                        svg,
                        r#"{i}<path d="{d}" fill="none" stroke="{stroke}" stroke-width="1" />{nl}"#,
                        i = indent,
                        d = d,
                        stroke = default_color_for_layer(item.layer),
                        nl = nl
                    )
                    .unwrap();
                }
            }
        }

        write!(svg, "</svg>{nl}", nl = nl).unwrap();
        Ok(svg)
    }

    /// Export layers to a writer (file, buffer, etc.).
    pub fn export_to_writer<W: io::Write>(
        &self,
        engine: &LayoutEngine,
        layers: &[(Uuid, &Layer)],
        writer: &mut W,
    ) -> Result<(), ExportError> {
        let svg = self.export_to_string(engine, layers)?;
        writer.write_all(svg.as_bytes())?;
        Ok(())
    }
}

/// Convert path commands to SVG `d` attribute string.
fn path_commands_to_svg_d(commands: &[PathCommand], precision: usize) -> String {
    let p = precision;
    let mut d = String::with_capacity(commands.len() * 30);
    for cmd in commands {
        match cmd {
            PathCommand::MoveTo(pt) => {
                write!(d, "M{:.p$},{:.p$} ", pt.x, pt.y, p = p).unwrap();
            }
            PathCommand::LineTo(pt) => {
                write!(d, "L{:.p$},{:.p$} ", pt.x, pt.y, p = p).unwrap();
            }
            PathCommand::QuadTo { ctrl, end } => {
                write!(
                    d,
                    "Q{:.p$},{:.p$} {:.p$},{:.p$} ",
                    ctrl.x, ctrl.y, end.x, end.y,
                    p = p
                )
                .unwrap();
            }
            PathCommand::BezierTo { cp1, cp2, end } => {
                write!(
                    d,
                    "C{:.p$},{:.p$} {:.p$},{:.p$} {:.p$},{:.p$} ",
                    cp1.x, cp1.y, cp2.x, cp2.y, end.x, end.y,
                    p = p
                )
                .unwrap();
            }
            PathCommand::Close => {
                d.push_str("Z ");
            }
        }
    }
    d.trim_end().to_string()
}

/// Layer-type default color (matches render bridge colors).
fn default_color_for_layer(layer: &Layer) -> &'static str {
    match layer {
        Layer::Rect(_) => "#4285f5",    // Blue
        Layer::Ellipse(_) => "#f5426b", // Red
        Layer::Text(_) => "#f5c842",    // Yellow
        Layer::Frame(_) => "#38383d",   // Dark gray
        Layer::Path(_) => "#8c3edb",    // Purple
    }
}

/// Convert RGBA float to CSS hex color.
fn color_to_css(color: [f32; 4]) -> String {
    let r = (color[0] * 255.0).round() as u8;
    let g = (color[1] * 255.0).round() as u8;
    let b = (color[2] * 255.0).round() as u8;
    if color[3] >= 0.999 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        let a = (color[3] * 255.0).round() as u8;
        format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
    }
}

/// XML-escape text content.
fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

// ═══════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use logos_core::{EllipseLayer, FrameLayer, PathCommand, PathLayer, Point, RectLayer, TextLayer};

    fn setup_engine_with_rect() -> (LayoutEngine, Layer) {
        let mut engine = LayoutEngine::new();
        let layer = Layer::Rect(RectLayer::new(10.0, 20.0, 200.0, 100.0));
        engine.add_or_update_layer(&layer).unwrap();
        engine.compute_layout(layer.id()).unwrap();
        (engine, layer)
    }

    #[test]
    fn test_svg_single_rect() {
        let (engine, layer) = setup_engine_with_rect();
        let exporter = SvgExporter::new(ExportPage::default());
        let layers = vec![(layer.id(), &layer)];
        let svg = exporter.export_to_string(&engine, &layers).unwrap();
        assert!(svg.contains("<?xml"));
        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
        assert!(svg.contains("<rect"));
        assert!(svg.contains("width=\"200"));
    }

    #[test]
    fn test_svg_ellipse() {
        let mut engine = LayoutEngine::new();
        let layer = Layer::Ellipse(EllipseLayer::new(0.0, 0.0, 100.0, 80.0));
        engine.add_or_update_layer(&layer).unwrap();
        engine.compute_layout(layer.id()).unwrap();
        let exporter = SvgExporter::new(ExportPage::default());
        let layers = vec![(layer.id(), &layer)];
        let svg = exporter.export_to_string(&engine, &layers).unwrap();
        assert!(svg.contains("<ellipse"));
        assert!(svg.contains("rx=\"50"));
        assert!(svg.contains("ry=\"40"));
    }

    #[test]
    fn test_svg_text() {
        let mut engine = LayoutEngine::new();
        let layer = Layer::Text(TextLayer::new("Hello <World>", 10.0, 20.0, 200.0, 30.0));
        engine.add_or_update_layer(&layer).unwrap();
        engine.compute_layout(layer.id()).unwrap();
        let exporter = SvgExporter::new(ExportPage::default());
        let layers = vec![(layer.id(), &layer)];
        let svg = exporter.export_to_string(&engine, &layers).unwrap();
        assert!(svg.contains("<text"));
        assert!(svg.contains("Hello &lt;World&gt;")); // XML escaped
    }

    #[test]
    fn test_svg_path() {
        let mut engine = LayoutEngine::new();
        let layer = Layer::Path(PathLayer::new(vec![
            PathCommand::MoveTo(Point::new(0.0, 0.0)),
            PathCommand::LineTo(Point::new(100.0, 0.0)),
            PathCommand::LineTo(Point::new(50.0, 86.0)),
            PathCommand::Close,
        ]));
        engine.add_or_update_layer(&layer).unwrap();
        engine.compute_layout(layer.id()).unwrap();
        let exporter = SvgExporter::new(ExportPage::default());
        let layers = vec![(layer.id(), &layer)];
        let svg = exporter.export_to_string(&engine, &layers).unwrap();
        assert!(svg.contains("<path"));
        assert!(svg.contains("M0.00,0.00"));
        assert!(svg.contains("Z"));
    }

    #[test]
    fn test_svg_frame() {
        let mut engine = LayoutEngine::new();
        let layer = Layer::Frame(FrameLayer {
            id: Uuid::new_v4(),
            children: vec![],
            bounds: logos_core::Rect {
                x: 0.0,
                y: 0.0,
                width: 400.0,
                height: 300.0,
            },
        });
        engine.add_or_update_layer(&layer).unwrap();
        engine.compute_layout(layer.id()).unwrap();
        let exporter = SvgExporter::new(ExportPage::default());
        let layers = vec![(layer.id(), &layer)];
        let svg = exporter.export_to_string(&engine, &layers).unwrap();
        assert!(svg.contains("opacity=\"0.8\""));
    }

    #[test]
    fn test_svg_compact() {
        let (engine, layer) = setup_engine_with_rect();
        let exporter = SvgExporter::new(ExportPage::default()).compact();
        let layers = vec![(layer.id(), &layer)];
        let svg = exporter.export_to_string(&engine, &layers).unwrap();
        // Compact mode: no newlines between elements
        assert!(!svg.contains(">\n  <"));
    }

    #[test]
    fn test_svg_custom_precision() {
        let (engine, layer) = setup_engine_with_rect();
        let exporter = SvgExporter::new(ExportPage::default()).with_precision(0);
        let layers = vec![(layer.id(), &layer)];
        let svg = exporter.export_to_string(&engine, &layers).unwrap();
        // Precision 0: integer coordinates like width="200"
        assert!(svg.contains("width=\"200\""),
            "expected integer precision, got:\n{svg}");
    }

    #[test]
    fn test_svg_empty_errors() {
        let engine = LayoutEngine::new();
        let exporter = SvgExporter::new(ExportPage::default());
        let layers: Vec<(Uuid, &Layer)> = vec![];
        let result = exporter.export_to_string(&engine, &layers);
        assert!(result.is_err());
    }

    #[test]
    fn test_svg_transparent_background() {
        let (engine, layer) = setup_engine_with_rect();
        let page = ExportPage::new(800.0, 600.0).transparent();
        let exporter = SvgExporter::new(page);
        let layers = vec![(layer.id(), &layer)];
        let svg = exporter.export_to_string(&engine, &layers).unwrap();
        // Should NOT have a background rect
        let rect_count = svg.matches("<rect").count();
        assert_eq!(rect_count, 1); // only the layer rect
    }

    #[test]
    fn test_svg_multiple_layers() {
        let mut engine = LayoutEngine::new();
        let r1 = Layer::Rect(RectLayer::new(0.0, 0.0, 100.0, 50.0));
        let r2 = Layer::Rect(RectLayer::new(200.0, 0.0, 80.0, 80.0));
        let e1 = Layer::Ellipse(EllipseLayer::new(100.0, 100.0, 60.0, 60.0));

        for l in [&r1, &r2, &e1] {
            engine.add_or_update_layer(l).unwrap();
            engine.compute_layout(l.id()).unwrap();
        }

        let exporter = SvgExporter::new(ExportPage::default());
        let layers = vec![(r1.id(), &r1), (r2.id(), &r2), (e1.id(), &e1)];
        let svg = exporter.export_to_string(&engine, &layers).unwrap();

        // 2 layer rects + 1 background rect = 3 rects total
        assert_eq!(svg.matches("<rect").count(), 3);
        assert_eq!(svg.matches("<ellipse").count(), 1);
    }

    #[test]
    fn test_path_commands_to_svg_d() {
        let commands = vec![
            PathCommand::MoveTo(Point::new(10.0, 20.0)),
            PathCommand::LineTo(Point::new(100.0, 20.0)),
            PathCommand::QuadTo {
                ctrl: Point::new(150.0, 0.0),
                end: Point::new(200.0, 20.0),
            },
            PathCommand::BezierTo {
                cp1: Point::new(220.0, 40.0),
                cp2: Point::new(240.0, 40.0),
                end: Point::new(260.0, 20.0),
            },
            PathCommand::Close,
        ];
        let d = path_commands_to_svg_d(&commands, 1);
        assert!(d.starts_with("M10.0,20.0"));
        assert!(d.contains("L100.0,20.0"));
        assert!(d.contains("Q150.0,0.0 200.0,20.0"));
        assert!(d.contains("C220.0,40.0 240.0,40.0 260.0,20.0"));
        assert!(d.ends_with("Z"));
    }

    #[test]
    fn test_color_to_css() {
        assert_eq!(color_to_css([1.0, 1.0, 1.0, 1.0]), "#ffffff");
        assert_eq!(color_to_css([0.0, 0.0, 0.0, 1.0]), "#000000");
        assert_eq!(color_to_css([1.0, 0.0, 0.0, 0.5]), "#ff000080");
    }

    #[test]
    fn test_xml_escape() {
        assert_eq!(xml_escape("a & b"), "a &amp; b");
        assert_eq!(xml_escape("<tag>"), "&lt;tag&gt;");
        assert_eq!(xml_escape("he said \"hi\""), "he said &quot;hi&quot;");
    }

    #[test]
    fn test_svg_write_to_vec() {
        let (engine, layer) = setup_engine_with_rect();
        let exporter = SvgExporter::new(ExportPage::default());
        let layers = vec![(layer.id(), &layer)];
        let mut buf = Vec::new();
        exporter.export_to_writer(&engine, &layers, &mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("<svg"));
    }
}
