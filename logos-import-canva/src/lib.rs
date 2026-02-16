//! # logos-import-canva
//!
//! Canva template / export file format importer for Logos.
//!
//! Canva exports templates as JSON documents describing pages,
//! elements (text, shapes, images), and their properties. This
//! importer parses that JSON structure into logos-core Documents.

pub mod model;
pub mod convert;

use logos_import_common::{ImportError, ImportResult, ImportOptions, Importer};

/// Parse a Canva export JSON from bytes and return a logos-core Document.
pub fn import_canva(data: &[u8]) -> ImportResult<logos_core::Document> {
    import_canva_with_options(data, &ImportOptions::default())
}

/// Parse a Canva export with custom options.
pub fn import_canva_with_options(
    data: &[u8],
    options: &ImportOptions,
) -> ImportResult<logos_core::Document> {
    let json_str = std::str::from_utf8(data)
        .map_err(|_| ImportError::EncodingError("Invalid UTF-8".into()))?;

    let canva_doc: model::CanvaDocument = serde_json::from_str(json_str)
        .map_err(|e| ImportError::ParseError {
            offset: 0,
            message: format!("Invalid Canva JSON: {}", e),
        })?;

    convert::convert_canva(&canva_doc, options)
}

/// The Canva importer implementing the common `Importer` trait.
pub struct CanvaImporter;

impl Importer for CanvaImporter {
    fn name(&self) -> &str {
        "canva"
    }

    fn extensions(&self) -> &[&str] {
        &["canva", "json"]
    }

    fn import(
        &self,
        data: &[u8],
        options: &ImportOptions,
    ) -> ImportResult<logos_core::Document> {
        import_canva_with_options(data, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    fn build_canva_json(doc: &CanvaDocument) -> Vec<u8> {
        serde_json::to_vec(doc).unwrap()
    }

    #[test]
    fn test_import_empty() {
        let doc = CanvaDocument::new("Test", 800.0, 600.0, vec![]);
        let data = build_canva_json(&doc);
        let result = import_canva(&data).unwrap();
        let page = result.root.read().unwrap();
        assert_eq!(page.name, "Test");
    }

    #[test]
    fn test_import_with_rect() {
        let doc = CanvaDocument::new("Design", 800.0, 600.0, vec![
            CanvaElement::rect("bg", 0.0, 0.0, 800.0, 600.0),
        ]);
        let data = build_canva_json(&doc);
        let result = import_canva(&data).unwrap();
        let page = result.root.read().unwrap();
        assert_eq!(page.layers.len(), 1);
        match &page.layers[0] {
            logos_core::Layer::Rect(r) => {
                assert_eq!(r.bounds.width, 800.0);
            }
            _ => panic!("expected rect"),
        }
    }

    #[test]
    fn test_import_with_text() {
        let doc = CanvaDocument::new("Design", 800.0, 600.0, vec![
            CanvaElement::text("title", 100.0, 50.0, 400.0, 60.0, "Hello Canva"),
        ]);
        let data = build_canva_json(&doc);
        let result = import_canva(&data).unwrap();
        let page = result.root.read().unwrap();
        match &page.layers[0] {
            logos_core::Layer::Text(t) => assert_eq!(t.content, "Hello Canva"),
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn test_import_with_ellipse() {
        let doc = CanvaDocument::new("Design", 800.0, 600.0, vec![
            CanvaElement::ellipse("circle", 100.0, 100.0, 200.0, 200.0),
        ]);
        let data = build_canva_json(&doc);
        let result = import_canva(&data).unwrap();
        let page = result.root.read().unwrap();
        match &page.layers[0] {
            logos_core::Layer::Ellipse(e) => assert_eq!(e.bounds.width, 200.0),
            _ => panic!("expected ellipse"),
        }
    }

    #[test]
    fn test_import_with_group() {
        let doc = CanvaDocument::new("Design", 800.0, 600.0, vec![
            CanvaElement::group("grp", vec![
                CanvaElement::rect("r1", 0.0, 0.0, 50.0, 50.0),
                CanvaElement::rect("r2", 60.0, 0.0, 50.0, 50.0),
            ]),
        ]);
        let data = build_canva_json(&doc);
        let result = import_canva(&data).unwrap();
        let page = result.root.read().unwrap();
        match &page.layers[0] {
            logos_core::Layer::Frame(f) => assert_eq!(f.children.len(), 2),
            _ => panic!("expected frame"),
        }
    }

    #[test]
    fn test_import_invalid_json() {
        let result = import_canva(b"not json at all");
        assert!(result.is_err());
    }

    #[test]
    fn test_canva_importer_trait() {
        let imp = CanvaImporter;
        assert_eq!(imp.name(), "canva");
        assert!(imp.can_handle("canva"));
        assert!(imp.can_handle("json"));
        assert!(!imp.can_handle("pdf"));
    }

    #[test]
    fn test_import_multiple_elements() {
        let doc = CanvaDocument::new("Multi", 800.0, 600.0, vec![
            CanvaElement::rect("bg", 0.0, 0.0, 800.0, 600.0),
            CanvaElement::text("title", 100.0, 50.0, 400.0, 40.0, "Title"),
            CanvaElement::ellipse("dot", 350.0, 300.0, 100.0, 100.0),
            CanvaElement::image("photo", 50.0, 150.0, 300.0, 200.0),
        ]);
        let data = build_canva_json(&doc);
        let result = import_canva(&data).unwrap();
        let page = result.root.read().unwrap();
        assert_eq!(page.layers.len(), 4);
    }
}
