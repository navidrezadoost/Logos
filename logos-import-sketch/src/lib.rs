//! # logos-import-sketch
//!
//! Sketch (.sketch) file format importer for Logos.
//!
//! Sketch files are ZIP archives containing JSON documents that describe
//! the design. The main structure is:
//! - `document.json` — document metadata, page references
//! - `meta.json` — app version, fonts, page list
//! - `pages/<uuid>.json` — page content with layer hierarchy
//!
//! This importer extracts the ZIP, parses the JSON, and converts
//! Sketch layers to logos-core Document types.

pub mod archive;
pub mod model;
pub mod convert;

use logos_import_common::{ImportError, ImportResult, ImportOptions, Importer};

/// Parse a .sketch file from bytes and return a logos-core Document.
pub fn import_sketch(data: &[u8]) -> ImportResult<logos_core::Document> {
    import_sketch_with_options(data, &ImportOptions::default())
}

/// Parse a .sketch file with custom options.
pub fn import_sketch_with_options(
    data: &[u8],
    options: &ImportOptions,
) -> ImportResult<logos_core::Document> {
    let sketch_doc = archive::extract_sketch(data)?;
    convert::convert_sketch(&sketch_doc, options)
}

/// The Sketch importer implementing the common `Importer` trait.
pub struct SketchImporter;

impl Importer for SketchImporter {
    fn name(&self) -> &str {
        "sketch"
    }

    fn extensions(&self) -> &[&str] {
        &["sketch"]
    }

    fn import(
        &self,
        data: &[u8],
        options: &ImportOptions,
    ) -> ImportResult<logos_core::Document> {
        import_sketch_with_options(data, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::build_test_sketch;

    #[test]
    fn test_import_simple_sketch() {
        let data = build_test_sketch(&[model::SketchLayer::rect(
            "rect-1", "Rectangle 1", 10.0, 20.0, 100.0, 50.0,
        )]);
        let doc = import_sketch(&data).unwrap();
        let page = doc.root.read().unwrap();
        assert_eq!(page.layers.len(), 1);
    }

    #[test]
    fn test_import_multiple_layers() {
        let data = build_test_sketch(&[
            model::SketchLayer::rect("r1", "Rect", 0.0, 0.0, 50.0, 50.0),
            model::SketchLayer::oval("o1", "Oval", 60.0, 0.0, 50.0, 50.0),
            model::SketchLayer::text("t1", "Label", 0.0, 60.0, 100.0, 20.0, "Hello"),
        ]);
        let doc = import_sketch(&data).unwrap();
        let page = doc.root.read().unwrap();
        assert_eq!(page.layers.len(), 3);
    }

    #[test]
    fn test_import_group() {
        let data = build_test_sketch(&[model::SketchLayer::group(
            "g1",
            "Group 1",
            vec![
                model::SketchLayer::rect("r1", "R1", 0.0, 0.0, 50.0, 50.0),
                model::SketchLayer::rect("r2", "R2", 60.0, 0.0, 50.0, 50.0),
            ],
        )]);
        let doc = import_sketch(&data).unwrap();
        let page = doc.root.read().unwrap();
        assert_eq!(page.layers.len(), 1);
        match &page.layers[0] {
            logos_core::Layer::Frame(f) => assert_eq!(f.children.len(), 2),
            _ => panic!("expected frame"),
        }
    }

    #[test]
    fn test_sketch_importer_trait() {
        let imp = SketchImporter;
        assert_eq!(imp.name(), "sketch");
        assert!(imp.can_handle("sketch"));
        assert!(!imp.can_handle("fig"));
    }

    #[test]
    fn test_import_invalid_data() {
        let result = import_sketch(b"not a zip file");
        assert!(result.is_err());
    }

    #[test]
    fn test_import_artboard() {
        let data = build_test_sketch(&[model::SketchLayer::artboard(
            "a1",
            "Artboard 1",
            0.0,
            0.0,
            375.0,
            812.0,
            vec![model::SketchLayer::rect("r1", "BG", 0.0, 0.0, 375.0, 812.0)],
        )]);
        let doc = import_sketch(&data).unwrap();
        let page = doc.root.read().unwrap();
        assert_eq!(page.layers.len(), 1);
        match &page.layers[0] {
            logos_core::Layer::Frame(f) => assert_eq!(f.children.len(), 1),
            _ => panic!("expected frame"),
        }
    }
}
