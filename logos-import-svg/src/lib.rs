//! # logos-import-svg
//!
//! SVG (Scalable Vector Graphics) importer for Logos.
//!
//! Parses SVG 1.1 files and converts them to logos-core `Document` types.
//! Supports basic shapes (rect, circle, ellipse, line, polyline, polygon),
//! path data, text elements, groups, and common presentation attributes.
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use logos_import_svg::import_svg;
//!
//! let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
//!   <rect x="10" y="10" width="80" height="80" fill="red"/>
//! </svg>"#;
//!
//! let doc = import_svg(svg.as_bytes()).unwrap();
//! ```

pub mod parser;
pub mod convert;
pub mod path_data;

use logos_import_common::{ImportError, ImportResult, ImportOptions, Importer, ImportStats};

/// Parse an SVG file from bytes and return a logos-core Document.
pub fn import_svg(data: &[u8]) -> ImportResult<logos_core::Document> {
    import_svg_with_options(data, &ImportOptions::default())
}

/// Parse an SVG file with custom options.
pub fn import_svg_with_options(
    data: &[u8],
    options: &ImportOptions,
) -> ImportResult<logos_core::Document> {
    let svg_tree = parser::parse_svg(data)?;
    convert::convert_svg(&svg_tree, options)
}

/// The SVG importer implementing the common `Importer` trait.
pub struct SvgImporter;

impl Importer for SvgImporter {
    fn name(&self) -> &str {
        "svg"
    }

    fn extensions(&self) -> &[&str] {
        &["svg", "svgz"]
    }

    fn import(
        &self,
        data: &[u8],
        options: &ImportOptions,
    ) -> ImportResult<logos_core::Document> {
        import_svg_with_options(data, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_import_simple_rect() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100">
            <rect x="10" y="20" width="80" height="40"/>
        </svg>"#;
        let doc = import_svg(svg.as_bytes()).unwrap();
        let page = doc.root.read().unwrap();
        assert_eq!(page.layers.len(), 1);
    }

    #[test]
    fn test_import_multiple_shapes() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="300" height="300">
            <rect x="10" y="10" width="100" height="50"/>
            <circle cx="200" cy="50" r="40"/>
            <ellipse cx="100" cy="200" rx="80" ry="40"/>
        </svg>"#;
        let doc = import_svg(svg.as_bytes()).unwrap();
        let page = doc.root.read().unwrap();
        assert_eq!(page.layers.len(), 3);
    }

    #[test]
    fn test_import_group() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">
            <g id="group1">
                <rect x="0" y="0" width="50" height="50"/>
                <rect x="60" y="0" width="50" height="50"/>
            </g>
        </svg>"#;
        let doc = import_svg(svg.as_bytes()).unwrap();
        let page = doc.root.read().unwrap();
        assert_eq!(page.layers.len(), 1); // one frame = one group
        match &page.layers[0] {
            logos_core::Layer::Frame(f) => assert_eq!(f.children.len(), 2),
            _ => panic!("expected frame for group"),
        }
    }

    #[test]
    fn test_import_text() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="50">
            <text x="10" y="30">Hello SVG</text>
        </svg>"#;
        let doc = import_svg(svg.as_bytes()).unwrap();
        let page = doc.root.read().unwrap();
        assert_eq!(page.layers.len(), 1);
        match &page.layers[0] {
            logos_core::Layer::Text(t) => assert_eq!(t.content, "Hello SVG"),
            _ => panic!("expected text layer"),
        }
    }

    #[test]
    fn test_import_path() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">
            <path d="M 10 10 L 100 10 L 100 100 Z"/>
        </svg>"#;
        let doc = import_svg(svg.as_bytes()).unwrap();
        let page = doc.root.read().unwrap();
        assert_eq!(page.layers.len(), 1);
        match &page.layers[0] {
            logos_core::Layer::Path(p) => {
                assert!(p.commands.len() >= 3);
                assert!(p.closed);
            }
            _ => panic!("expected path layer"),
        }
    }

    #[test]
    fn test_svg_importer_trait() {
        let imp = SvgImporter;
        assert_eq!(imp.name(), "svg");
        assert!(imp.can_handle("svg"));
        assert!(imp.can_handle("SVG"));
        assert!(imp.can_handle("svgz"));
        assert!(!imp.can_handle("png"));
    }

    #[test]
    fn test_import_empty_svg() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"></svg>"#;
        let doc = import_svg(svg.as_bytes()).unwrap();
        let page = doc.root.read().unwrap();
        assert_eq!(page.layers.len(), 0);
    }

    #[test]
    fn test_import_line() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">
            <line x1="10" y1="10" x2="190" y2="190"/>
        </svg>"#;
        let doc = import_svg(svg.as_bytes()).unwrap();
        let page = doc.root.read().unwrap();
        assert_eq!(page.layers.len(), 1);
    }

    #[test]
    fn test_import_polyline() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">
            <polyline points="10,10 50,50 90,10 130,50"/>
        </svg>"#;
        let doc = import_svg(svg.as_bytes()).unwrap();
        let page = doc.root.read().unwrap();
        assert_eq!(page.layers.len(), 1);
    }

    #[test]
    fn test_import_polygon() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">
            <polygon points="100,10 40,198 190,78 10,78 160,198"/>
        </svg>"#;
        let doc = import_svg(svg.as_bytes()).unwrap();
        let page = doc.root.read().unwrap();
        assert_eq!(page.layers.len(), 1);
        match &page.layers[0] {
            logos_core::Layer::Path(p) => assert!(p.closed),
            _ => panic!("expected closed path for polygon"),
        }
    }

    #[test]
    fn test_import_nested_groups() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">
            <g id="outer">
                <g id="inner">
                    <rect x="0" y="0" width="50" height="50"/>
                </g>
            </g>
        </svg>"#;
        let doc = import_svg(svg.as_bytes()).unwrap();
        let page = doc.root.read().unwrap();
        assert_eq!(page.layers.len(), 1);
    }

    #[test]
    fn test_import_invalid_xml() {
        let svg = b"this is not xml at all";
        let result = import_svg(svg);
        assert!(result.is_err());
    }
}
