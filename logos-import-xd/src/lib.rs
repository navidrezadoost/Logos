//! # logos-import-xd
//!
//! Adobe XD file format importer for Logos.
//!
//! XD files are ZIP archives containing JSON artwork descriptions with
//! a known internal structure:
//! - `manifest` — document metadata
//! - `artwork/*/graphics/graphicContent.agc` — artboard content
//!
//! This importer extracts artboards, shapes, text, and groups.

pub mod model;
pub mod archive;
pub mod convert;

use logos_import_common::{ImportResult, ImportOptions, Importer};

/// Parse an XD file from bytes and return a logos-core Document.
pub fn import_xd(data: &[u8]) -> ImportResult<logos_core::Document> {
    import_xd_with_options(data, &ImportOptions::default())
}

/// Parse an XD file with custom options.
pub fn import_xd_with_options(
    data: &[u8],
    options: &ImportOptions,
) -> ImportResult<logos_core::Document> {
    let xd_doc = archive::extract_xd(data)?;
    convert::convert_xd(&xd_doc, options)
}

/// The XD importer implementing the common `Importer` trait.
pub struct XdImporter;

impl Importer for XdImporter {
    fn name(&self) -> &str {
        "xd"
    }

    fn extensions(&self) -> &[&str] {
        &["xd"]
    }

    fn import(
        &self,
        data: &[u8],
        options: &ImportOptions,
    ) -> ImportResult<logos_core::Document> {
        import_xd_with_options(data, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::build_test_xd;
    use crate::model::*;

    #[test]
    fn test_import_empty_xd() {
        let data = build_test_xd(&[], "Test Artboard");
        let doc = import_xd(&data).unwrap();
        let page = doc.root.read().unwrap();
        assert_eq!(page.name, "Test Artboard");
    }

    #[test]
    fn test_import_xd_with_rect() {
        let data = build_test_xd(
            &[XdNode::rect("bg", 0.0, 0.0, 375.0, 812.0)],
            "Screen",
        );
        let doc = import_xd(&data).unwrap();
        let page = doc.root.read().unwrap();
        assert_eq!(page.layers.len(), 1);
        match &page.layers[0] {
            logos_core::Layer::Rect(r) => {
                assert_eq!(r.bounds.width, 375.0);
            }
            _ => panic!("expected rect"),
        }
    }

    #[test]
    fn test_import_xd_with_text() {
        let data = build_test_xd(
            &[XdNode::text("title", 10.0, 20.0, 200.0, 30.0, "Hello XD")],
            "Screen",
        );
        let doc = import_xd(&data).unwrap();
        let page = doc.root.read().unwrap();
        assert_eq!(page.layers.len(), 1);
        match &page.layers[0] {
            logos_core::Layer::Text(t) => assert_eq!(t.content, "Hello XD"),
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn test_import_xd_with_group() {
        let data = build_test_xd(
            &[XdNode::group(
                "grp",
                vec![
                    XdNode::rect("r1", 0.0, 0.0, 50.0, 50.0),
                    XdNode::rect("r2", 60.0, 0.0, 50.0, 50.0),
                ],
            )],
            "Screen",
        );
        let doc = import_xd(&data).unwrap();
        let page = doc.root.read().unwrap();
        assert_eq!(page.layers.len(), 1);
        match &page.layers[0] {
            logos_core::Layer::Frame(f) => assert_eq!(f.children.len(), 2),
            _ => panic!("expected frame"),
        }
    }

    #[test]
    fn test_import_xd_with_ellipse() {
        let data = build_test_xd(
            &[XdNode::ellipse("e1", 10.0, 10.0, 100.0, 80.0)],
            "Screen",
        );
        let doc = import_xd(&data).unwrap();
        let page = doc.root.read().unwrap();
        assert_eq!(page.layers.len(), 1);
        match &page.layers[0] {
            logos_core::Layer::Ellipse(e) => {
                assert_eq!(e.bounds.width, 100.0);
            }
            _ => panic!("expected ellipse"),
        }
    }

    #[test]
    fn test_xd_importer_trait() {
        let imp = XdImporter;
        assert_eq!(imp.name(), "xd");
        assert!(imp.can_handle("xd"));
        assert!(!imp.can_handle("sketch"));
    }

    #[test]
    fn test_import_invalid_xd() {
        let result = import_xd(b"not a zip file");
        assert!(result.is_err());
    }
}
