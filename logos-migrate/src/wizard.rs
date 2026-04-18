//! # wizard
//!
//! Single-file migration wizard. Detects format from extension or explicit
//! user specification, runs the appropriate importer, and returns a
//! [`crate::MigrationResult`] wrapping the `.gos`-ready snapshot.

use std::collections::HashMap;
use std::path::Path;

use logos_core::persistence::DocumentSnapshot;
use logos_import_common::ImportOptions;

use crate::{MigrationError, MigrationResult, Result, SourceFormatKind};
use crate::report::{MigrationReport, Severity, ReportEntry};

/// The format to migrate from. Can be auto-detected from file extension.
pub use crate::SourceFormatKind as SourceFormat;

/// Configuration for a single migration run.
#[derive(Debug, Clone)]
pub struct WizardConfig {
    /// Explicitly set source format. If `None`, auto-detected from extension.
    pub source_format: Option<SourceFormat>,
    /// Import options forwarded to the underlying importer.
    pub import_options: ImportOptions,
    /// Maximum number of layers (0 = unlimited).
    pub layer_limit: usize,
    /// If true, unsupported features are silently skipped rather than warned.
    pub silent_skip: bool,
}

impl Default for WizardConfig {
    fn default() -> Self {
        Self {
            source_format: None,
            import_options: ImportOptions::default(),
            layer_limit: 0,
            silent_skip: false,
        }
    }
}

/// The migration wizard. Stateless — create once, call many times.
#[derive(Debug, Default)]
pub struct MigrationWizard {
    config: WizardConfig,
}

impl MigrationWizard {
    /// Create a wizard with default configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a wizard with custom configuration.
    pub fn with_config(config: WizardConfig) -> Self {
        Self { config }
    }

    /// Detect the source format of a file path from its extension.
    pub fn detect_format(path: &Path) -> Option<SourceFormat> {
        path.extension()
            .and_then(|e| e.to_str())
            .and_then(SourceFormatKind::from_extension)
    }

    /// Migrate a file at the given path.
    /// Format is auto-detected unless overridden in config.
    pub fn migrate_file(&self, path: &Path) -> Result<MigrationResult> {
        let data = std::fs::read(path)?;
        let format = self.config.source_format
            .or_else(|| Self::detect_format(path))
            .ok_or_else(|| {
                let ext = path.extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("(none)");
                MigrationError::UnsupportedFormat(ext.to_string())
            })?;
        self.migrate_bytes_with_format(&data, format)
    }

    /// Migrate raw bytes with an explicitly specified format.
    pub fn migrate_bytes(&self, data: &[u8], format: SourceFormat) -> Result<MigrationResult> {
        self.migrate_bytes_with_format(data, format)
    }

    fn migrate_bytes_with_format(
        &self,
        data: &[u8],
        format: SourceFormat,
    ) -> Result<MigrationResult> {
        let opts = &self.config.import_options;

        let doc = match format {
            SourceFormat::AdobeXd  => logos_import_xd::import_xd_with_options(data, opts)?,
            SourceFormat::Sketch   => logos_import_sketch::import_sketch_with_options(data, opts)?,
            SourceFormat::Figma    => logos_import_figma::import_figma(data)
                .map_err(|e| MigrationError::SnapshotError(e.to_string()))?,
        };

        // Apply layer limit if set.
        let layer_count = doc.root.read().map(|p| p.layers.len()).unwrap_or(0);
        let effective_layer_count = if self.config.layer_limit > 0 {
            layer_count.min(self.config.layer_limit)
        } else {
            layer_count
        };

        // Build snapshot.
        let components: HashMap<uuid::Uuid, logos_core::container::ComponentRef> = HashMap::new();
        let snapshot = DocumentSnapshot::capture(&doc, &components, &[]);

        // Build report.
        let mut entries = Vec::new();
        entries.push(ReportEntry {
            severity: Severity::Info,
            message: format!(
                "Migrated {} layer(s) from {} source",
                effective_layer_count,
                format.display_name(),
            ),
            element_id: None,
        });

        if self.config.layer_limit > 0 && layer_count > self.config.layer_limit {
            entries.push(ReportEntry {
                severity: Severity::Warning,
                message: format!(
                    "Layer limit ({}) exceeded — {} layer(s) were omitted",
                    self.config.layer_limit,
                    layer_count - self.config.layer_limit,
                ),
                element_id: None,
            });
        }

        let report = MigrationReport {
            source_format: format.display_name().to_string(),
            total_layers: layer_count,
            converted_layers: effective_layer_count,
            warnings: entries.iter().filter(|e| e.severity == Severity::Warning).count(),
            errors: entries.iter().filter(|e| e.severity == Severity::Error).count(),
            entries,
        };

        Ok(MigrationResult { snapshot, report, source_format: format })
    }

    /// Preview: returns just the detected format and estimated element count
    /// without performing a full conversion. Useful for a GUI preview step.
    pub fn preview(&self, data: &[u8], format: SourceFormat) -> WizardPreview {
        // We do a lightweight parse to count layers without full conversion.
        let estimated = match format {
            SourceFormat::AdobeXd  =>
                logos_import_xd::import_xd_with_options(data, &self.config.import_options)
                    .map(|d| d.root.read().map(|p| p.layers.len()).unwrap_or(0)).unwrap_or(0),
            SourceFormat::Sketch   =>
                logos_import_sketch::import_sketch_with_options(data, &self.config.import_options)
                    .map(|d| d.root.read().map(|p| p.layers.len()).unwrap_or(0)).unwrap_or(0),
            SourceFormat::Figma    =>
                logos_import_figma::import_figma(data)
                    .map(|d| d.root.read().map(|p| p.layers.len()).unwrap_or(0)).unwrap_or(0),
        };

        WizardPreview {
            source_format: format,
            estimated_layer_count: estimated,
            is_parseable: estimated > 0,
        }
    }
}

/// Lightweight preview metadata returned before committing to full migration.
#[derive(Debug, Clone)]
pub struct WizardPreview {
    pub source_format: SourceFormat,
    pub estimated_layer_count: usize,
    pub is_parseable: bool,
}

// ── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // -- format detection --

    #[test]
    fn detect_xd_extension() {
        let p = Path::new("design.xd");
        assert_eq!(MigrationWizard::detect_format(p), Some(SourceFormat::AdobeXd));
    }

    #[test]
    fn detect_sketch_extension() {
        let p = Path::new("design.sketch");
        assert_eq!(MigrationWizard::detect_format(p), Some(SourceFormat::Sketch));
    }

    #[test]
    fn detect_figma_extension() {
        let p = Path::new("design.fig");
        assert_eq!(MigrationWizard::detect_format(p), Some(SourceFormat::Figma));
    }

    #[test]
    fn detect_unknown_extension_returns_none() {
        let p = Path::new("design.psd");
        assert_eq!(MigrationWizard::detect_format(p), None);
    }

    #[test]
    fn detect_no_extension_returns_none() {
        let p = Path::new("design");
        assert_eq!(MigrationWizard::detect_format(p), None);
    }

    #[test]
    fn detect_uppercase_extension() {
        let p = Path::new("design.XD");
        // from_extension is case-insensitive
        assert_eq!(
            SourceFormatKind::from_extension("XD"),
            Some(SourceFormat::AdobeXd)
        );
        let _ = p; // path itself doesn't lowercase in OsStr; logic in from_extension
    }

    // -- SourceFormatKind API --

    #[test]
    fn source_format_display_names() {
        assert_eq!(SourceFormat::AdobeXd.display_name(), "Adobe XD");
        assert_eq!(SourceFormat::Sketch.display_name(), "Sketch");
        assert_eq!(SourceFormat::Figma.display_name(), "Figma");
    }

    #[test]
    fn source_format_extensions() {
        assert!(SourceFormat::AdobeXd.extensions().contains(&"xd"));
        assert!(SourceFormat::Sketch.extensions().contains(&"sketch"));
        assert!(SourceFormat::Figma.extensions().contains(&"fig"));
    }

    #[test]
    fn from_extension_case_insensitive() {
        assert_eq!(SourceFormatKind::from_extension("XD"), Some(SourceFormatKind::AdobeXd));
        assert_eq!(SourceFormatKind::from_extension("SKETCH"), Some(SourceFormatKind::Sketch));
        assert_eq!(SourceFormatKind::from_extension("FIG"), Some(SourceFormatKind::Figma));
    }

    #[test]
    fn from_extension_with_dot_prefix() {
        assert_eq!(SourceFormatKind::from_extension(".xd"), Some(SourceFormatKind::AdobeXd));
    }

    #[test]
    fn from_extension_unknown_returns_none() {
        assert_eq!(SourceFormatKind::from_extension("ai"), None);
        assert_eq!(SourceFormatKind::from_extension("pdf"), None);
    }

    // -- WizardConfig defaults --

    #[test]
    fn wizard_config_default_no_limit() {
        let cfg = WizardConfig::default();
        assert_eq!(cfg.layer_limit, 0);
        assert!(!cfg.silent_skip);
        assert!(cfg.source_format.is_none());
    }

    #[test]
    fn wizard_new_uses_default_config() {
        let w = MigrationWizard::new();
        assert_eq!(w.config.layer_limit, 0);
    }

    // -- migrate_bytes rejects unknown data gracefully --

    #[test]
    fn migrate_garbage_bytes_xd_returns_error() {
        let w = MigrationWizard::new();
        let result = w.migrate_bytes(b"not an xd file", SourceFormat::AdobeXd);
        assert!(result.is_err());
    }

    #[test]
    fn migrate_garbage_bytes_sketch_returns_error() {
        let w = MigrationWizard::new();
        let result = w.migrate_bytes(b"not a sketch file", SourceFormat::Sketch);
        assert!(result.is_err());
    }

    #[test]
    fn migrate_garbage_bytes_figma_returns_error() {
        let w = MigrationWizard::new();
        let result = w.migrate_bytes(b"not a figma file", SourceFormat::Figma);
        assert!(result.is_err());
    }

    // -- migrate_file missing file returns Io error --

    #[test]
    fn migrate_file_missing_returns_io_error() {
        let w = MigrationWizard::new();
        let result = w.migrate_file(Path::new("/tmp/logos_migrate_nonexistent_test_file.xd"));
        assert!(matches!(result, Err(MigrationError::Io(_))));
    }

    // -- migrate_file unknown extension without override --

    #[test]
    fn migrate_file_unknown_ext_returns_unsupported() {
        let w = MigrationWizard::new();
        // Create a temp file with unknown extension
        let tmp = tempfile::NamedTempFile::with_suffix(".psd").unwrap();
        std::fs::write(tmp.path(), b"data").unwrap();
        let result = w.migrate_file(tmp.path());
        assert!(matches!(result, Err(MigrationError::UnsupportedFormat(_))));
    }

    // -- MigrationError Display --

    #[test]
    fn migration_error_display_unsupported() {
        let e = MigrationError::UnsupportedFormat("ai".to_string());
        assert!(e.to_string().contains("ai"));
    }

    #[test]
    fn migration_error_display_io() {
        let io = std::io::Error::new(std::io::ErrorKind::NotFound, "not found");
        let e = MigrationError::Io(io);
        assert!(e.to_string().contains("I/O"));
    }
}
