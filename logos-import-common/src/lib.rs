//! # logos-import-common
//!
//! Shared traits, types, and utilities for all Logos file format importers.
//!
//! Every importer crate (`logos-import-figma`, `logos-import-svg`, etc.) depends
//! on this crate for the [`Importer`] trait, unified error handling, and
//! common color/transform helpers.
//!
//! ## Implementing an Importer
//!
//! ```rust,no_run
//! use logos_import_common::{Importer, ImportError, ImportResult, ImportOptions};
//! use logos_core::Document;
//!
//! struct MyImporter;
//!
//! impl Importer for MyImporter {
//!     fn name(&self) -> &str { "my-format" }
//!     fn extensions(&self) -> &[&str] { &["myf"] }
//!     fn import(&self, data: &[u8], opts: &ImportOptions) -> ImportResult<Document> {
//!         todo!()
//!     }
//! }
//! ```

pub mod error;
pub mod color;
pub mod transform;
pub mod options;
pub mod stats;

// Re-exports
pub use error::{ImportError, ImportResult};
pub use options::ImportOptions;
pub use stats::ImportStats;
pub use color::Color4f;
pub use transform::Matrix2D;

/// Trait implemented by every file format importer.
///
/// This is the core abstraction that lets Logos treat all external
/// file formats uniformly.
pub trait Importer: Send + Sync {
    /// Human-readable importer name (e.g. `"figma"`, `"svg"`).
    fn name(&self) -> &str;

    /// File extensions this importer handles (without dot), e.g. `["fig"]`.
    fn extensions(&self) -> &[&str];

    /// Import raw file bytes into a logos-core [`Document`].
    fn import(&self, data: &[u8], options: &ImportOptions) -> ImportResult<logos_core::Document>;

    /// Check if this importer can handle a file based on extension.
    fn can_handle(&self, extension: &str) -> bool {
        let ext_lower = extension.to_lowercase();
        self.extensions().iter().any(|e| *e == ext_lower)
    }

    /// Import from a file path.
    fn import_file(
        &self,
        path: &std::path::Path,
        options: &ImportOptions,
    ) -> ImportResult<logos_core::Document> {
        let data = std::fs::read(path)?;
        self.import(&data, options)
    }

    /// Importer version string.
    fn version(&self) -> &str {
        "0.1.0"
    }
}

/// Detect the right importer for a given file extension.
pub fn detect_format(extension: &str, importers: &[&dyn Importer]) -> Option<usize> {
    let ext = extension.to_lowercase();
    importers.iter().position(|imp| imp.can_handle(&ext))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyImporter;

    impl Importer for DummyImporter {
        fn name(&self) -> &str {
            "dummy"
        }

        fn extensions(&self) -> &[&str] {
            &["dum", "dummy"]
        }

        fn import(&self, _data: &[u8], _options: &ImportOptions) -> ImportResult<logos_core::Document> {
            Ok(logos_core::Document::new())
        }
    }

    #[test]
    fn test_importer_can_handle() {
        let imp = DummyImporter;
        assert!(imp.can_handle("dum"));
        assert!(imp.can_handle("DUM"));
        assert!(imp.can_handle("dummy"));
        assert!(!imp.can_handle("png"));
    }

    #[test]
    fn test_importer_name_and_version() {
        let imp = DummyImporter;
        assert_eq!(imp.name(), "dummy");
        assert_eq!(imp.version(), "0.1.0");
    }

    #[test]
    fn test_detect_format() {
        let imp = DummyImporter;
        let importers: Vec<&dyn Importer> = vec![&imp];
        assert_eq!(detect_format("dum", &importers), Some(0));
        assert_eq!(detect_format("png", &importers), None);
    }

    #[test]
    fn test_dummy_import() {
        let imp = DummyImporter;
        let doc = imp.import(b"hello", &ImportOptions::default()).unwrap();
        assert_eq!(doc.version, 1);
    }
}
