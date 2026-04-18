//! # logos-migrate
//!
//! Cross-platform design file migration wizard for Logos.
//!
//! Converts Adobe XD (`.xd`), Sketch (`.sketch`), and Figma (`.fig`) files
//! into Logos native format (`.gos`) while preserving design intent.
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use logos_migrate::wizard::{MigrationWizard, SourceFormat};
//!
//! let wizard = MigrationWizard::new();
//! let result = wizard.migrate_bytes(b"...", SourceFormat::Sketch).unwrap();
//! println!("Schema version ok: {}", result.snapshot.is_current_schema());
//! ```

pub mod wizard;
pub mod batch;
pub mod report;

use logos_core::persistence::DocumentSnapshot;
use serde::{Deserialize, Serialize};

/// Result of a single file migration.
#[derive(Debug)]
pub struct MigrationResult {
    /// The migrated document snapshot, ready to save as `.gos`.
    pub snapshot: DocumentSnapshot,
    /// Human-readable migration report.
    pub report: report::MigrationReport,
    /// Source format that was detected/used.
    pub source_format: wizard::SourceFormat,
}

/// Supported source file formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SourceFormatKind {
    AdobeXd,
    Sketch,
    Figma,
}

impl SourceFormatKind {
    /// File extensions (without dot) that map to this format.
    pub fn extensions(&self) -> &'static [&'static str] {
        match self {
            Self::AdobeXd  => &["xd"],
            Self::Sketch   => &["sketch"],
            Self::Figma    => &["fig"],
        }
    }

    /// Human-readable name.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::AdobeXd  => "Adobe XD",
            Self::Sketch   => "Sketch",
            Self::Figma    => "Figma",
        }
    }

    /// Detect format from a file extension string (case-insensitive).
    pub fn from_extension(ext: &str) -> Option<Self> {
        let lower = ext.trim_start_matches('.').to_lowercase();
        match lower.as_str() {
            "xd"     => Some(Self::AdobeXd),
            "sketch" => Some(Self::Sketch),
            "fig"    => Some(Self::Figma),
            _        => None,
        }
    }
}

/// Errors returned by the migration pipeline.
#[derive(Debug)]
pub enum MigrationError {
    /// The file extension is not recognised as a supported source format.
    UnsupportedFormat(String),
    /// The importer failed to parse the source file.
    ImportFailed(logos_import_common::ImportError),
    /// A snapshot could not be created from the imported document.
    SnapshotError(String),
    /// I/O error reading/writing files.
    Io(std::io::Error),
}

impl std::fmt::Display for MigrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedFormat(ext) =>
                write!(f, "Unsupported source format: {ext}"),
            Self::ImportFailed(e) =>
                write!(f, "Import failed: {e}"),
            Self::SnapshotError(msg) =>
                write!(f, "Snapshot error: {msg}"),
            Self::Io(e) =>
                write!(f, "I/O error: {e}"),
        }
    }
}

impl std::error::Error for MigrationError {}

impl From<logos_import_common::ImportError> for MigrationError {
    fn from(e: logos_import_common::ImportError) -> Self {
        Self::ImportFailed(e)
    }
}

impl From<std::io::Error> for MigrationError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, MigrationError>;
