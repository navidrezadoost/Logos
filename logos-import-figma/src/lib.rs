//! # logos-import-figma
//!
//! Figma (.fig) file format importer for Logos.
//!
//! This crate provides a complete pipeline for importing Figma design files:
//!
//! 1. **Binary parsing** — reads the .fig file header and decompresses the payload
//! 2. **Kiwi decoding** — decodes the binary node tree using the Kiwi format
//! 3. **Model construction** — builds a rich Figma node tree with full properties
//! 4. **Conversion** — transforms Figma nodes into logos-core Document types
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use logos_import_figma::{FigmaParser, FigmaConverter};
//! use std::path::Path;
//!
//! let mut parser = FigmaParser::new();
//! let figma_doc = parser.parse_file(Path::new("design.fig")).unwrap();
//!
//! let mut converter = FigmaConverter::new();
//! let logos_doc = converter.convert(&figma_doc).unwrap();
//! ```
//!
//! ## Test Fixtures
//!
//! Use [`fixtures::generate_test_fig`] to create synthetic .fig files for testing.

pub mod error;
pub mod format;
pub mod model;
pub mod parser;
pub mod convert;
pub mod fixtures;

// Re-exports for convenience
pub use error::{FigmaError, FigmaResult};
pub use parser::{FigmaParser, ImportOptions, ParseStats};
pub use convert::{FigmaConverter, ConvertOptions, ConvertStats};
pub use model::node::{FigmaNode, NodeType, NodeBase, NodeData};
pub use model::paint::{Color, Paint, PaintType, BlendMode};
pub use model::effect::{Effect, EffectType};
pub use model::transform::{Transform2D, BoundingBox, Size2D, Vector2D};

/// Import a .fig file and convert to a logos-core Document in a single call.
///
/// This is the highest-level API for importing Figma files.
pub fn import_figma(data: &[u8]) -> FigmaResult<logos_core::Document> {
    let mut parser = FigmaParser::new();
    let figma_doc = parser.parse(data)?;

    let mut converter = FigmaConverter::new();
    converter.convert(&figma_doc)
}

/// Import a .fig file from a path.
pub fn import_figma_file(path: &std::path::Path) -> FigmaResult<logos_core::Document> {
    let data = std::fs::read(path)?;
    import_figma(&data)
}

/// Import and return both the Figma tree and logos Document.
pub fn import_figma_with_tree(
    data: &[u8],
) -> FigmaResult<(FigmaNode, logos_core::Document)> {
    let mut parser = FigmaParser::new();
    let figma_doc = parser.parse(data)?;

    let mut converter = FigmaConverter::new();
    let logos_doc = converter.convert(&figma_doc)?;

    Ok((figma_doc, logos_doc))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_import_figma_roundtrip() {
        let fig_data = fixtures::generate_test_fig(fixtures::TestFixture::SingleRectangle);

        let doc = import_figma(&fig_data).unwrap();
        let page = doc.root.read().unwrap();
        assert_eq!(page.layers.len(), 1);
    }

    #[test]
    fn test_import_figma_with_tree() {
        let fig_data = fixtures::generate_test_fig(fixtures::TestFixture::BasicShapes);

        let (tree, doc) = import_figma_with_tree(&fig_data).unwrap();
        assert_eq!(tree.node_type, NodeType::Document);
        assert!(tree.node_count() > 1);

        let page = doc.root.read().unwrap();
        assert!(!page.layers.is_empty());
    }

    #[test]
    fn test_import_invalid_data() {
        let bad_data = b"this is not a .fig file";
        assert!(import_figma(bad_data).is_err());
    }

    #[test]
    fn test_import_complex_fixture() {
        let fig_data = fixtures::generate_test_fig(fixtures::TestFixture::MobileAppScreen);

        let (tree, doc) = import_figma_with_tree(&fig_data).unwrap();

        // Verify tree structure
        assert!(tree.node_count() >= 10);

        // Verify conversion
        let page = doc.root.read().unwrap();
        assert_eq!(page.name, "Home Screen");
        assert!(!page.layers.is_empty());
    }
}
