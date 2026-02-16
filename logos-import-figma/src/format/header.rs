//! .fig file header parsing.
//!
//! The .fig file starts with a 24-byte header:
//! - Bytes  0..8:  Magic bytes "fig-kiwi" (8 bytes)
//! - Bytes  8..12: File version (u32 LE)
//! - Bytes 12..16: Schema version (u32 LE)
//! - Bytes 16..20: Payload length in bytes (u32 LE, compressed size)
//! - Bytes 20..24: Uncompressed length (u32 LE)

use crate::error::{FigmaError, FigmaResult};

/// Magic bytes identifying a .fig file.
pub const FIG_MAGIC: &[u8; 8] = b"fig-kiwi";

/// Header size in bytes.
pub const HEADER_SIZE: usize = 24;

/// Supported file format versions.
pub const SUPPORTED_VERSIONS: &[u32] = &[1, 2, 3, 4, 5];

/// Parsed .fig file header.
#[derive(Debug, Clone, PartialEq)]
pub struct FigHeader {
    /// File format version.
    pub version: u32,
    /// Schema version for the Kiwi encoding.
    pub schema_version: u32,
    /// Length of the compressed payload.
    pub compressed_length: u32,
    /// Length of the uncompressed payload.
    pub uncompressed_length: u32,
}

impl FigHeader {
    /// Parse a header from the first 24 bytes of a .fig file.
    pub fn parse(data: &[u8]) -> FigmaResult<Self> {
        if data.len() < HEADER_SIZE {
            return Err(FigmaError::UnexpectedEof(data.len()));
        }

        // Validate magic bytes
        if &data[0..8] != FIG_MAGIC.as_slice() {
            return Err(FigmaError::InvalidMagic(format!(
                "expected {:?}, got {:?}",
                FIG_MAGIC,
                &data[0..8]
            )));
        }

        let version = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
        let schema_version = u32::from_le_bytes([data[12], data[13], data[14], data[15]]);
        let compressed_length = u32::from_le_bytes([data[16], data[17], data[18], data[19]]);
        let uncompressed_length = u32::from_le_bytes([data[20], data[21], data[22], data[23]]);

        if !SUPPORTED_VERSIONS.contains(&version) {
            return Err(FigmaError::UnsupportedVersion(version));
        }

        Ok(Self {
            version,
            schema_version,
            compressed_length,
            uncompressed_length,
        })
    }

    /// Serialize the header to bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(HEADER_SIZE);
        buf.extend_from_slice(FIG_MAGIC);
        buf.extend_from_slice(&self.version.to_le_bytes());
        buf.extend_from_slice(&self.schema_version.to_le_bytes());
        buf.extend_from_slice(&self.compressed_length.to_le_bytes());
        buf.extend_from_slice(&self.uncompressed_length.to_le_bytes());
        buf
    }

    /// The byte offset where the compressed payload starts.
    pub fn payload_offset(&self) -> usize {
        HEADER_SIZE
    }

    /// Total expected file size (header + compressed payload).
    pub fn expected_file_size(&self) -> usize {
        HEADER_SIZE + self.compressed_length as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_header(version: u32, schema: u32, compressed: u32, uncompressed: u32) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(FIG_MAGIC);
        buf.extend_from_slice(&version.to_le_bytes());
        buf.extend_from_slice(&schema.to_le_bytes());
        buf.extend_from_slice(&compressed.to_le_bytes());
        buf.extend_from_slice(&uncompressed.to_le_bytes());
        buf
    }

    #[test]
    fn test_parse_valid_header() {
        let data = make_header(1, 1, 1024, 4096);
        let header = FigHeader::parse(&data).unwrap();
        assert_eq!(header.version, 1);
        assert_eq!(header.schema_version, 1);
        assert_eq!(header.compressed_length, 1024);
        assert_eq!(header.uncompressed_length, 4096);
    }

    #[test]
    fn test_parse_version_3() {
        let data = make_header(3, 2, 512, 2048);
        let header = FigHeader::parse(&data).unwrap();
        assert_eq!(header.version, 3);
        assert_eq!(header.schema_version, 2);
    }

    #[test]
    fn test_parse_invalid_magic() {
        let mut data = make_header(1, 1, 100, 200);
        data[0] = b'X';
        let err = FigHeader::parse(&data).unwrap_err();
        assert!(matches!(err, FigmaError::InvalidMagic(_)));
    }

    #[test]
    fn test_parse_unsupported_version() {
        let data = make_header(99, 1, 100, 200);
        let err = FigHeader::parse(&data).unwrap_err();
        assert!(matches!(err, FigmaError::UnsupportedVersion(99)));
    }

    #[test]
    fn test_parse_truncated() {
        let data = b"fig-kiwi\x01\x00";
        let err = FigHeader::parse(data).unwrap_err();
        assert!(matches!(err, FigmaError::UnexpectedEof(_)));
    }

    #[test]
    fn test_roundtrip() {
        let original = FigHeader {
            version: 2,
            schema_version: 1,
            compressed_length: 999,
            uncompressed_length: 3333,
        };
        let bytes = original.to_bytes();
        let parsed = FigHeader::parse(&bytes).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn test_payload_offset() {
        let header = FigHeader {
            version: 1,
            schema_version: 1,
            compressed_length: 512,
            uncompressed_length: 1024,
        };
        assert_eq!(header.payload_offset(), 24);
    }

    #[test]
    fn test_expected_file_size() {
        let header = FigHeader {
            version: 1,
            schema_version: 1,
            compressed_length: 512,
            uncompressed_length: 1024,
        };
        assert_eq!(header.expected_file_size(), 24 + 512);
    }

    #[test]
    fn test_all_supported_versions() {
        for &v in SUPPORTED_VERSIONS {
            let data = make_header(v, 1, 100, 200);
            assert!(FigHeader::parse(&data).is_ok());
        }
    }
}
