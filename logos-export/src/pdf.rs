//! Minimal PDF 1.4 exporter for Logos documents.
//!
//! References:
//! - PDF Reference, Adobe, Version 1.4, Chapter 4 (Graphics)
//! - Foley et al., Ch. 22 (output formats)
//!
//! Produces valid PDF 1.4 files without external dependencies.
//! Pages contain vector drawing operators for each layer.
//! The output is not compressed (for simplicity and debuggability);
//! compression can be added later via deflate.

use std::fmt::Write as FmtWrite;
use std::io::{self, Write};

use logos_core::{Layer, PathCommand};
use logos_layout::engine::LayoutEngine;
use uuid::Uuid;

use crate::{ExportError, ExportPage, collect_export_data};

/// PDF exporter — converts a Logos layer tree to a PDF 1.4 file.
pub struct PdfExporter {
    page: ExportPage,
}

impl PdfExporter {
    pub fn new(page: ExportPage) -> Self {
        Self { page }
    }

    /// Export to a byte vector (complete PDF file).
    pub fn export_to_bytes(
        &self,
        engine: &LayoutEngine,
        layers: &[(Uuid, &Layer)],
    ) -> Result<Vec<u8>, ExportError> {
        if layers.is_empty() {
            return Err(ExportError::EmptyDocument);
        }
        if self.page.width <= 0.0 || self.page.height <= 0.0 {
            return Err(ExportError::InvalidDimensions(self.page.width, self.page.height));
        }

        let data = collect_export_data(engine, layers)?;
        let height = self.page.height;

        // Build the page content stream
        let mut stream = String::with_capacity(data.len() * 100 + 200);

        // Background
        if let Some(bg) = self.page.background {
            write!(
                stream,
                "{r:.3} {g:.3} {b:.3} rg\n0 0 {w:.1} {h:.1} re f\n",
                r = bg[0],
                g = bg[1],
                b = bg[2],
                w = self.page.width,
                h = self.page.height,
            )
            .unwrap();
        }

        // Layer elements
        for item in &data {
            // PDF coordinate system: origin at bottom-left, Y grows up
            let pdf_y = height - item.y - item.height;

            match item.layer {
                Layer::Rect(_) | Layer::Frame(_) => {
                    let (r, g, b) = default_pdf_color(item.layer);
                    write!(
                        stream,
                        "{r:.3} {g:.3} {b:.3} rg\n{x:.2} {y:.2} {w:.2} {h:.2} re f\n",
                        r = r,
                        g = g,
                        b = b,
                        x = item.x,
                        y = pdf_y,
                        w = item.width,
                        h = item.height,
                    )
                    .unwrap();
                }
                Layer::Ellipse(_) => {
                    // Approximate ellipse with 4 Bézier curves (standard PDF technique)
                    let (r, g, b) = default_pdf_color(item.layer);
                    let cx = item.x + item.width * 0.5;
                    let cy = pdf_y + item.height * 0.5;
                    let rx = item.width * 0.5;
                    let ry = item.height * 0.5;
                    // κ ≈ 0.5522847498 for circular Bézier approximation
                    let kx = rx * 0.5522847498;
                    let ky = ry * 0.5522847498;

                    write!(
                        stream,
                        "{r:.3} {g:.3} {b:.3} rg\n\
                         {mx:.2} {my:.2} m\n\
                         {c1x:.2} {c1y:.2} {c2x:.2} {c2y:.2} {e1x:.2} {e1y:.2} c\n\
                         {c3x:.2} {c3y:.2} {c4x:.2} {c4y:.2} {e2x:.2} {e2y:.2} c\n\
                         {c5x:.2} {c5y:.2} {c6x:.2} {c6y:.2} {e3x:.2} {e3y:.2} c\n\
                         {c7x:.2} {c7y:.2} {c8x:.2} {c8y:.2} {e4x:.2} {e4y:.2} c\n\
                         f\n",
                        r = r,
                        g = g,
                        b = b,
                        // Start top
                        mx = cx,
                        my = cy + ry,
                        // Top-right quarter
                        c1x = cx + kx,
                        c1y = cy + ry,
                        c2x = cx + rx,
                        c2y = cy + ky,
                        e1x = cx + rx,
                        e1y = cy,
                        // Right-bottom quarter
                        c3x = cx + rx,
                        c3y = cy - ky,
                        c4x = cx + kx,
                        c4y = cy - ry,
                        e2x = cx,
                        e2y = cy - ry,
                        // Bottom-left quarter
                        c5x = cx - kx,
                        c5y = cy - ry,
                        c6x = cx - rx,
                        c6y = cy - ky,
                        e3x = cx - rx,
                        e3y = cy,
                        // Left-top quarter
                        c7x = cx - rx,
                        c7y = cy + ky,
                        c8x = cx - kx,
                        c8y = cy + ry,
                        e4x = cx,
                        e4y = cy + ry,
                    )
                    .unwrap();
                }
                Layer::Text(text_layer) => {
                    let (r, g, b) = default_pdf_color(item.layer);
                    // Use PDF text operators (Helvetica, 14pt)
                    let escaped = pdf_escape_string(&text_layer.content);
                    write!(
                        stream,
                        "BT\n/F1 14 Tf\n{r:.3} {g:.3} {b:.3} rg\n{x:.2} {y:.2} Td\n({text}) Tj\nET\n",
                        r = r,
                        g = g,
                        b = b,
                        x = item.x,
                        y = pdf_y + 4.0, // baseline adjust
                        text = escaped,
                    )
                    .unwrap();
                }
                Layer::Path(path_layer) => {
                    let (r, g, b) = default_pdf_color(item.layer);
                    write!(stream, "{r:.3} {g:.3} {b:.3} RG\n1 w\n").unwrap();
                    for cmd in &path_layer.commands {
                        match cmd {
                            PathCommand::MoveTo(pt) => {
                                write!(stream, "{:.2} {:.2} m\n", pt.x, height - pt.y).unwrap();
                            }
                            PathCommand::LineTo(pt) => {
                                write!(stream, "{:.2} {:.2} l\n", pt.x, height - pt.y).unwrap();
                            }
                            PathCommand::QuadTo { ctrl, end } => {
                                // PDF doesn't have quadratic curves; promote to cubic
                                write!(
                                    stream,
                                    "{:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c\n",
                                    ctrl.x,
                                    height - ctrl.y,
                                    ctrl.x,
                                    height - ctrl.y,
                                    end.x,
                                    height - end.y,
                                )
                                .unwrap();
                            }
                            PathCommand::BezierTo { cp1, cp2, end } => {
                                write!(
                                    stream,
                                    "{:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c\n",
                                    cp1.x,
                                    height - cp1.y,
                                    cp2.x,
                                    height - cp2.y,
                                    end.x,
                                    height - end.y,
                                )
                                .unwrap();
                            }
                            PathCommand::Close => {
                                stream.push_str("h\n");
                            }
                        }
                    }
                    stream.push_str("S\n"); // stroke
                }
                Layer::Artboard(_) | Layer::Drawer(_) => {
                    // Render as a filled rect (same as Rect/Frame)
                    let (r, g, b) = default_pdf_color(item.layer);
                    write!(
                        stream,
                        "{r:.3} {g:.3} {b:.3} rg\n{x:.2} {y:.2} {w:.2} {h:.2} re f\n",
                        r = r,
                        g = g,
                        b = b,
                        x = item.x,
                        y = pdf_y,
                        w = item.width,
                        h = item.height,
                    )
                    .unwrap();
                }
            }
        }

        // Build PDF structure
        let mut pdf = PdfBuilder::new();
        pdf.build(self.page.width, self.page.height, &stream);
        Ok(pdf.output)
    }

    /// Export to a writer.
    pub fn export_to_writer<W: io::Write>(
        &self,
        engine: &LayoutEngine,
        layers: &[(Uuid, &Layer)],
        writer: &mut W,
    ) -> Result<(), ExportError> {
        let bytes = self.export_to_bytes(engine, layers)?;
        writer.write_all(&bytes)?;
        Ok(())
    }
}

/// Default colors per layer type (RGB float).
fn default_pdf_color(layer: &Layer) -> (f32, f32, f32) {
    match layer {
        Layer::Rect(_) => (0.26, 0.52, 0.96),      // Blue
        Layer::Ellipse(_) => (0.96, 0.26, 0.42),    // Red
        Layer::Text(_) => (0.96, 0.78, 0.26),       // Yellow
        Layer::Frame(_) => (0.22, 0.22, 0.24),      // Dark gray
        Layer::Path(_) => (0.55, 0.24, 0.86),       // Purple
        Layer::Artboard(_) => (0.95, 0.95, 0.95),   // Light gray
        Layer::Drawer(_) => (0.18, 0.20, 0.25),     // Dark blue-gray
    }
}

/// Escape special characters in PDF strings.
fn pdf_escape_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            '\\' => out.push_str("\\\\"),
            _ => out.push(c),
        }
    }
    out
}

/// Minimal PDF 1.4 file builder.
///
/// Produces valid PDF with:
///   - Catalog (obj 1)
///   - Pages (obj 2)
///   - Page (obj 3)
///   - Content stream (obj 4)
///   - Font (obj 5) — Helvetica (built-in, no embedding)
///   - xref table
///   - trailer
struct PdfBuilder {
    output: Vec<u8>,
    offsets: Vec<usize>,
}

impl PdfBuilder {
    fn new() -> Self {
        Self {
            output: Vec::with_capacity(4096),
            offsets: Vec::with_capacity(6),
        }
    }

    fn build(&mut self, width: f32, height: f32, content: &str) {
        // Header
        self.output.extend_from_slice(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n");

        // Object 1: Catalog
        self.start_obj();
        write!(self.output, "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n").unwrap();

        // Object 2: Pages
        self.start_obj();
        write!(
            self.output,
            "2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n"
        )
        .unwrap();

        // Object 3: Page
        self.start_obj();
        write!(
            self.output,
            "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w:.1} {h:.1}] \
             /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>\nendobj\n",
            w = width,
            h = height,
        )
        .unwrap();

        // Object 4: Content stream
        let stream_bytes = content.as_bytes();
        self.start_obj();
        write!(
            self.output,
            "4 0 obj\n<< /Length {} >>\nstream\n",
            stream_bytes.len()
        )
        .unwrap();
        self.output.extend_from_slice(stream_bytes);
        write!(self.output, "\nendstream\nendobj\n").unwrap();

        // Object 5: Font (Helvetica, built-in)
        self.start_obj();
        write!(
            self.output,
            "5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n"
        )
        .unwrap();

        // Cross-reference table
        let xref_offset = self.output.len();
        write!(self.output, "xref\n0 {}\n", self.offsets.len() + 1).unwrap();
        write!(self.output, "0000000000 65535 f \n").unwrap();
        for offset in &self.offsets {
            write!(self.output, "{:010} 00000 n \n", offset).unwrap();
        }

        // Trailer
        write!(
            self.output,
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
            self.offsets.len() + 1,
            xref_offset
        )
        .unwrap();
    }

    fn start_obj(&mut self) {
        self.offsets.push(self.output.len());
    }
}

// ═══════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use logos_core::{EllipseLayer, PathCommand, PathLayer, Point, RectLayer, TextLayer};

    fn setup_engine_with_rect() -> (LayoutEngine, Layer) {
        let mut engine = LayoutEngine::new();
        let layer = Layer::Rect(RectLayer::new(10.0, 20.0, 200.0, 100.0));
        engine.add_or_update_layer(&layer).unwrap();
        engine.compute_layout(layer.id()).unwrap();
        (engine, layer)
    }

    #[test]
    fn test_pdf_header() {
        let (engine, layer) = setup_engine_with_rect();
        let exporter = PdfExporter::new(ExportPage::default());
        let layers = vec![(layer.id(), &layer)];
        let bytes = exporter.export_to_bytes(&engine, &layers).unwrap();
        assert!(bytes.starts_with(b"%PDF-1.4"));
        assert!(bytes.ends_with(b"%%EOF\n"));
    }

    #[test]
    fn test_pdf_contains_rect_operator() {
        let (engine, layer) = setup_engine_with_rect();
        let exporter = PdfExporter::new(ExportPage::default());
        let layers = vec![(layer.id(), &layer)];
        let bytes = exporter.export_to_bytes(&engine, &layers).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("re f")); // rectangle fill
    }

    #[test]
    fn test_pdf_ellipse() {
        let mut engine = LayoutEngine::new();
        let layer = Layer::Ellipse(EllipseLayer::new(0.0, 0.0, 100.0, 80.0));
        engine.add_or_update_layer(&layer).unwrap();
        engine.compute_layout(layer.id()).unwrap();
        let exporter = PdfExporter::new(ExportPage::default());
        let layers = vec![(layer.id(), &layer)];
        let bytes = exporter.export_to_bytes(&engine, &layers).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains(" c\n")); // bezier curve for ellipse
        assert!(text.contains("f\n")); // fill
    }

    #[test]
    fn test_pdf_text() {
        let mut engine = LayoutEngine::new();
        let layer = Layer::Text(TextLayer::new("Hello (World)", 10.0, 20.0, 200.0, 30.0));
        engine.add_or_update_layer(&layer).unwrap();
        engine.compute_layout(layer.id()).unwrap();
        let exporter = PdfExporter::new(ExportPage::default());
        let layers = vec![(layer.id(), &layer)];
        let bytes = exporter.export_to_bytes(&engine, &layers).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("BT"));
        assert!(text.contains("Hello \\(World\\)"));
        assert!(text.contains("ET"));
    }

    #[test]
    fn test_pdf_path() {
        let mut engine = LayoutEngine::new();
        let layer = Layer::Path(PathLayer::new(vec![
            PathCommand::MoveTo(Point::new(0.0, 0.0)),
            PathCommand::LineTo(Point::new(100.0, 0.0)),
            PathCommand::Close,
        ]));
        engine.add_or_update_layer(&layer).unwrap();
        engine.compute_layout(layer.id()).unwrap();
        let exporter = PdfExporter::new(ExportPage::default());
        let layers = vec![(layer.id(), &layer)];
        let bytes = exporter.export_to_bytes(&engine, &layers).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains(" m\n")); // moveto
        assert!(text.contains(" l\n")); // lineto
        assert!(text.contains("S\n")); // stroke
    }

    #[test]
    fn test_pdf_empty_errors() {
        let engine = LayoutEngine::new();
        let exporter = PdfExporter::new(ExportPage::default());
        let layers: Vec<(Uuid, &Layer)> = vec![];
        assert!(exporter.export_to_bytes(&engine, &layers).is_err());
    }

    #[test]
    fn test_pdf_has_xref() {
        let (engine, layer) = setup_engine_with_rect();
        let exporter = PdfExporter::new(ExportPage::default());
        let layers = vec![(layer.id(), &layer)];
        let bytes = exporter.export_to_bytes(&engine, &layers).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("xref"));
        assert!(text.contains("trailer"));
        assert!(text.contains("startxref"));
    }

    #[test]
    fn test_pdf_has_font() {
        let (engine, layer) = setup_engine_with_rect();
        let exporter = PdfExporter::new(ExportPage::default());
        let layers = vec![(layer.id(), &layer)];
        let bytes = exporter.export_to_bytes(&engine, &layers).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("/Helvetica"));
    }

    #[test]
    fn test_pdf_escape_string() {
        assert_eq!(pdf_escape_string("Hello"), "Hello");
        assert_eq!(pdf_escape_string("a(b)c"), "a\\(b\\)c");
        assert_eq!(pdf_escape_string("a\\b"), "a\\\\b");
    }

    #[test]
    fn test_pdf_write_to_vec() {
        let (engine, layer) = setup_engine_with_rect();
        let exporter = PdfExporter::new(ExportPage::default());
        let layers = vec![(layer.id(), &layer)];
        let mut buf = Vec::new();
        exporter.export_to_writer(&engine, &layers, &mut buf).unwrap();
        assert!(buf.starts_with(b"%PDF-1.4"));
    }

    #[test]
    fn test_pdf_multiple_layers() {
        let mut engine = LayoutEngine::new();
        let r1 = Layer::Rect(RectLayer::new(0.0, 0.0, 100.0, 50.0));
        let e1 = Layer::Ellipse(EllipseLayer::new(200.0, 0.0, 80.0, 80.0));
        for l in [&r1, &e1] {
            engine.add_or_update_layer(l).unwrap();
            engine.compute_layout(l.id()).unwrap();
        }
        let exporter = PdfExporter::new(ExportPage::default());
        let layers = vec![(r1.id(), &r1), (e1.id(), &e1)];
        let bytes = exporter.export_to_bytes(&engine, &layers).unwrap();
        assert!(!bytes.is_empty());
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("re f")); // rect
        assert!(text.contains(" c\n")); // ellipse beziers
    }
}
