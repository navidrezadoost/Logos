//! Error types for the Figma importer.

use thiserror::Error;

/// All errors that can occur during Figma file import.
#[derive(Error, Debug)]
pub enum FigmaError {
    /// The file is not a valid .fig file (wrong magic bytes).
    #[error("invalid .fig file: {0}")]
    InvalidMagic(String),

    /// The file version is not supported by this parser.
    #[error("unsupported .fig version: {0}")]
    UnsupportedVersion(u32),

    /// The compressed payload could not be decompressed.
    #[error("decompression failed: {0}")]
    DecompressionFailed(String),

    /// The binary payload could not be parsed.
    #[error("parse error at byte {offset}: {message}")]
    ParseError { offset: usize, message: String },

    /// A required field was missing from a node.
    #[error("missing field '{field}' on node type {node_type}")]
    MissingField { node_type: String, field: String },

    /// An unexpected node type was encountered.
    #[error("unknown node type: {0}")]
    UnknownNodeType(u8),

    /// The node tree structure is invalid.
    #[error("invalid tree structure: {0}")]
    InvalidTree(String),

    /// A color value was out of range.
    #[error("invalid color value: {0}")]
    InvalidColor(String),

    /// The file could not be read from disk.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// A conversion to the internal document model failed.
    #[error("conversion error: {0}")]
    ConversionError(String),

    /// The file is truncated or incomplete.
    #[error("unexpected end of file at byte {0}")]
    UnexpectedEof(usize),
}

/// Result type alias for Figma import operations.
pub type FigmaResult<T> = Result<T, FigmaError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_invalid_magic() {
        let err = FigmaError::InvalidMagic("bad header".into());
        assert_eq!(err.to_string(), "invalid .fig file: bad header");
    }

    #[test]
    fn test_error_display_parse_error() {
        let err = FigmaError::ParseError {
            offset: 42,
            message: "unexpected byte".into(),
        };
        assert_eq!(err.to_string(), "parse error at byte 42: unexpected byte");
    }

    #[test]
    fn test_error_display_missing_field() {
        let err = FigmaError::MissingField {
            node_type: "RECTANGLE".into(),
            field: "width".into(),
        };
        assert_eq!(
            err.to_string(),
            "missing field 'width' on node type RECTANGLE"
        );
    }

    #[test]
    fn test_error_display_unknown_node_type() {
        let err = FigmaError::UnknownNodeType(255);
        assert_eq!(err.to_string(), "unknown node type: 255");
    }

    #[test]
    fn test_error_display_conversion() {
        let err = FigmaError::ConversionError("unsupported blend mode".into());
        assert_eq!(err.to_string(), "conversion error: unsupported blend mode");
    }

    #[test]
    fn test_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let err: FigmaError = io_err.into();
        assert!(err.to_string().contains("file missing"));
    }
}
