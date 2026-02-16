//! Unified error types for all importers.

use thiserror::Error;

/// Errors that can occur during any file format import.
#[derive(Error, Debug)]
pub enum ImportError {
    /// The file format is not recognized or the magic bytes are wrong.
    #[error("unrecognized format: {0}")]
    UnrecognizedFormat(String),

    /// The file version is not supported.
    #[error("unsupported version: {0}")]
    UnsupportedVersion(String),

    /// A required structure or field is missing.
    #[error("missing required element: {0}")]
    MissingElement(String),

    /// General parse error with byte offset.
    #[error("parse error at offset {offset}: {message}")]
    ParseError { offset: usize, message: String },

    /// A structural constraint was violated.
    #[error("invalid structure: {0}")]
    InvalidStructure(String),

    /// The file could not be decompressed.
    #[error("decompression failed: {0}")]
    DecompressionFailed(String),

    /// An encoding or decoding error occurred.
    #[error("encoding error: {0}")]
    EncodingError(String),

    /// A color value was out of the valid range.
    #[error("invalid color: {0}")]
    InvalidColor(String),

    /// Conversion to the internal model failed.
    #[error("conversion error: {0}")]
    ConversionError(String),

    /// I/O error from the filesystem.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON deserialization error.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// The file is truncated or incomplete.
    #[error("unexpected end of file at byte {0}")]
    UnexpectedEof(usize),

    /// Operation timed out.
    #[error("import timed out after {0} ms")]
    Timeout(u64),

    /// The file exceeds the allowed maximum size.
    #[error("file too large: {size} bytes (max {max})")]
    FileTooLarge { size: usize, max: usize },

    /// A feature is not yet implemented for this format.
    #[error("unsupported feature: {0}")]
    UnsupportedFeature(String),
}

/// Result type alias for import operations.
pub type ImportResult<T> = Result<T, ImportError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let e = ImportError::UnrecognizedFormat("bad magic".into());
        assert!(e.to_string().contains("bad magic"));
    }

    #[test]
    fn test_error_parse() {
        let e = ImportError::ParseError {
            offset: 42,
            message: "unexpected byte".into(),
        };
        assert!(e.to_string().contains("42"));
        assert!(e.to_string().contains("unexpected byte"));
    }

    #[test]
    fn test_error_file_too_large() {
        let e = ImportError::FileTooLarge {
            size: 100_000_000,
            max: 50_000_000,
        };
        let s = e.to_string();
        assert!(s.contains("100000000"));
        assert!(s.contains("50000000"));
    }

    #[test]
    fn test_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "gone");
        let e: ImportError = io_err.into();
        assert!(e.to_string().contains("gone"));
    }

    #[test]
    fn test_error_unsupported_feature() {
        let e = ImportError::UnsupportedFeature("blend modes".into());
        assert!(e.to_string().contains("blend modes"));
    }
}
