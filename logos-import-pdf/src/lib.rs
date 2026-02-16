//! # logos-import-pdf
//!
//! PDF file format importer for Logos.
//!
//! Parses PDF 1.x files and extracts text and vector content into
//! logos-core Document types. The parser handles:
//! - Cross-reference tables (xref)
//! - Object streams
//! - Page tree navigation
//! - Basic content stream operators (text, path drawing)
//!
//! ## Limitations
//! - Images are represented as placeholder rectangles
//! - Complex shading/patterns are simplified
//! - Font metrics are approximated

pub mod parser;
pub mod content;
pub mod convert;

use logos_import_common::{ImportResult, ImportOptions, Importer};

/// Parse a PDF file from bytes and return a logos-core Document.
pub fn import_pdf(data: &[u8]) -> ImportResult<logos_core::Document> {
    import_pdf_with_options(data, &ImportOptions::default())
}

/// Parse a PDF file with custom options.
pub fn import_pdf_with_options(
    data: &[u8],
    options: &ImportOptions,
) -> ImportResult<logos_core::Document> {
    let pdf_doc = parser::parse_pdf(data)?;
    convert::convert_pdf(&pdf_doc, options)
}

/// The PDF importer implementing the common `Importer` trait.
pub struct PdfImporter;

impl Importer for PdfImporter {
    fn name(&self) -> &str {
        "pdf"
    }

    fn extensions(&self) -> &[&str] {
        &["pdf"]
    }

    fn import(
        &self,
        data: &[u8],
        options: &ImportOptions,
    ) -> ImportResult<logos_core::Document> {
        import_pdf_with_options(data, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::build_test_pdf;

    #[test]
    fn test_import_empty_pdf() {
        let data = build_test_pdf(&[], 612.0, 792.0);
        let doc = import_pdf(&data).unwrap();
        let page = doc.root.read().unwrap();
        assert_eq!(page.name, "PDF Document");
    }

    #[test]
    fn test_import_pdf_with_text() {
        let data = build_test_pdf(
            &[content::PdfElement::Text {
                content: "Hello PDF".into(),
                x: 72.0,
                y: 720.0,
                font_size: 12.0,
            }],
            612.0,
            792.0,
        );
        let doc = import_pdf(&data).unwrap();
        let page = doc.root.read().unwrap();
        assert!(!page.layers.is_empty());
        let has_text = page.layers.iter().any(|l| matches!(l, logos_core::Layer::Text(_)));
        assert!(has_text);
    }

    #[test]
    fn test_import_pdf_with_rect() {
        let data = build_test_pdf(
            &[content::PdfElement::Rect {
                x: 100.0,
                y: 200.0,
                width: 300.0,
                height: 150.0,
            }],
            612.0,
            792.0,
        );
        let doc = import_pdf(&data).unwrap();
        let page = doc.root.read().unwrap();
        assert!(!page.layers.is_empty());
    }

    #[test]
    fn test_import_pdf_with_path() {
        let data = build_test_pdf(
            &[content::PdfElement::Path {
                commands: vec![
                    content::PathCmd::MoveTo(10.0, 10.0),
                    content::PathCmd::LineTo(100.0, 10.0),
                    content::PathCmd::LineTo(100.0, 100.0),
                    content::PathCmd::Close,
                ],
            }],
            612.0,
            792.0,
        );
        let doc = import_pdf(&data).unwrap();
        let page = doc.root.read().unwrap();
        assert!(!page.layers.is_empty());
    }

    #[test]
    fn test_pdf_importer_trait() {
        let imp = PdfImporter;
        assert_eq!(imp.name(), "pdf");
        assert!(imp.can_handle("pdf"));
        assert!(!imp.can_handle("svg"));
    }

    #[test]
    fn test_import_invalid_pdf() {
        let result = import_pdf(b"not a pdf");
        assert!(result.is_err());
    }

    #[test]
    fn test_import_multiple_elements() {
        let data = build_test_pdf(
            &[
                content::PdfElement::Rect {
                    x: 50.0,
                    y: 50.0,
                    width: 200.0,
                    height: 100.0,
                },
                content::PdfElement::Text {
                    content: "Title".into(),
                    x: 72.0,
                    y: 700.0,
                    font_size: 24.0,
                },
                content::PdfElement::Text {
                    content: "Body text".into(),
                    x: 72.0,
                    y: 650.0,
                    font_size: 12.0,
                },
            ],
            612.0,
            792.0,
        );
        let doc = import_pdf(&data).unwrap();
        let page = doc.root.read().unwrap();
        assert!(page.layers.len() >= 3);
    }
}
