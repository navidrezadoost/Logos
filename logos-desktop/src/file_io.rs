//! # File I/O — Save and load `.logos` documents
//!
//! Provides serialization and deserialization of Logos documents using
//! a compact binary format with optional compression.
//!
//! ## File Format
//!
//! ```text
//! ┌──────────── .logos file ────────────────┐
//! │ Magic: "LGOS" (4 bytes)                 │
//! │ Version: u32 LE (4 bytes)               │
//! │ Flags: u32 LE (4 bytes)                 │
//! │   bit 0: compressed (LZ4)              │
//! │ Payload length: u64 LE (8 bytes)        │
//! │ Payload: JSON or LZ4(JSON)              │
//! └─────────────────────────────────────────┘
//! ```
//!
//! JSON is chosen for the serialized payload because it leverages the
//! existing `serde::Serialize` + `Deserialize` derives on Document and
//! all Layer variants. LZ4 compression reduces typical file sizes by
//! 60-75% with negligible latency (<1 ms for 1 MB payloads).

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use logos_core::Document;
use serde_json;

/// Magic bytes identifying a `.logos` file.
const MAGIC: &[u8; 4] = b"LGOS";

/// Current file format version.
const FORMAT_VERSION: u32 = 1;

/// Flag: payload is LZ4-compressed.
const FLAG_COMPRESSED: u32 = 1 << 0;

/// Header size in bytes (magic + version + flags + payload_len).
const HEADER_SIZE: usize = 4 + 4 + 4 + 8;

/// Errors from file I/O operations.
#[derive(Debug)]
pub enum FileError {
    /// Underlying I/O error.
    Io(io::Error),
    /// File is not a valid `.logos` file (bad magic).
    InvalidMagic,
    /// Unsupported file format version.
    UnsupportedVersion(u32),
    /// JSON serialization/deserialization error.
    Serialization(String),
    /// Decompression error.
    Decompression(String),
    /// Compression error.
    Compression(String),
    /// File is truncated or corrupted.
    Truncated,
}

impl std::fmt::Display for FileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileError::Io(e) => write!(f, "I/O error: {e}"),
            FileError::InvalidMagic => write!(f, "Not a valid .logos file"),
            FileError::UnsupportedVersion(v) => write!(f, "Unsupported version: {v}"),
            FileError::Serialization(e) => write!(f, "Serialization error: {e}"),
            FileError::Decompression(e) => write!(f, "Decompression error: {e}"),
            FileError::Compression(e) => write!(f, "Compression error: {e}"),
            FileError::Truncated => write!(f, "File is truncated or corrupted"),
        }
    }
}

impl std::error::Error for FileError {}

impl From<io::Error> for FileError {
    fn from(e: io::Error) -> Self {
        FileError::Io(e)
    }
}

/// Save a document to a `.logos` file.
///
/// If `compress` is true, the JSON payload is LZ4-compressed before
/// writing. This typically reduces file size by 60-75%.
///
/// # Errors
///
/// Returns `FileError` if serialization or writing fails.
pub fn save_document(doc: &Document, path: impl AsRef<Path>, compress: bool) -> Result<PathBuf, FileError> {
    let path = path.as_ref();

    // Serialize document to JSON.
    let json = serde_json::to_vec(doc)
        .map_err(|e| FileError::Serialization(e.to_string()))?;

    let (payload, flags) = if compress {
        let compressed = lz4_flex::compress_prepend_size(&json);
        (compressed, FLAG_COMPRESSED)
    } else {
        (json, 0u32)
    };

    // Build header.
    let mut file = fs::File::create(path)?;
    file.write_all(MAGIC)?;
    file.write_all(&FORMAT_VERSION.to_le_bytes())?;
    file.write_all(&flags.to_le_bytes())?;
    file.write_all(&(payload.len() as u64).to_le_bytes())?;
    file.write_all(&payload)?;
    file.flush()?;

    log::info!(
        "Saved document to {}: {} bytes (compressed={compress})",
        path.display(),
        HEADER_SIZE + payload.len(),
    );

    Ok(path.to_path_buf())
}

/// Load a document from a `.logos` file.
///
/// Validates the magic bytes and format version before deserializing.
///
/// # Errors
///
/// Returns `FileError` on invalid files, unsupported versions, or I/O failures.
pub fn load_document(path: impl AsRef<Path>) -> Result<Document, FileError> {
    let path = path.as_ref();
    let mut file = fs::File::open(path)?;

    // Read header.
    let mut header = [0u8; HEADER_SIZE];
    file.read_exact(&mut header).map_err(|_| FileError::Truncated)?;

    // Validate magic.
    if &header[0..4] != MAGIC {
        return Err(FileError::InvalidMagic);
    }

    // Parse version.
    let version = u32::from_le_bytes(header[4..8].try_into().unwrap());
    if version > FORMAT_VERSION {
        return Err(FileError::UnsupportedVersion(version));
    }

    // Parse flags.
    let flags = u32::from_le_bytes(header[8..12].try_into().unwrap());
    let compressed = (flags & FLAG_COMPRESSED) != 0;

    // Parse payload length.
    let payload_len = u64::from_le_bytes(header[12..20].try_into().unwrap()) as usize;

    // Read payload.
    let mut payload = vec![0u8; payload_len];
    file.read_exact(&mut payload).map_err(|_| FileError::Truncated)?;

    // Decompress if needed.
    let json_bytes = if compressed {
        lz4_flex::decompress_size_prepended(&payload)
            .map_err(|e| FileError::Decompression(format!("{e}")))?
    } else {
        payload
    };

    // Deserialize.
    let doc: Document = serde_json::from_slice(&json_bytes)
        .map_err(|e| FileError::Serialization(e.to_string()))?;

    log::info!(
        "Loaded document from {}: {} layers",
        path.display(),
        doc.root.read().map(|p| p.layers.len()).unwrap_or(0),
    );

    Ok(doc)
}

/// Get the default save directory (XDG-compliant on Linux).
pub fn default_save_dir() -> PathBuf {
    if let Some(data) = dirs::data_dir() {
        data.join("logos").join("documents")
    } else {
        PathBuf::from(".")
    }
}

/// Ensure the default save directory exists.
pub fn ensure_save_dir() -> io::Result<PathBuf> {
    let dir = default_save_dir();
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

// ===================================================================
// Tests
// ===================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use logos_core::{Document, Layer, RectLayer, TextLayer, EllipseLayer};
    use std::io::Write;

    fn make_test_doc() -> Document {
        let doc = Document::new();
        doc.add_layer(Layer::Rect(RectLayer::new(10.0, 20.0, 100.0, 50.0))).unwrap();
        doc.add_layer(Layer::Rect(RectLayer::new(200.0, 300.0, 80.0, 60.0))).unwrap();
        doc.add_layer(Layer::Text(TextLayer::new("Hello, Logos!", 50.0, 50.0, 200.0, 30.0))).unwrap();
        doc
    }

    #[test]
    fn test_save_and_load_uncompressed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.logos");

        let doc = make_test_doc();
        save_document(&doc, &path, false).unwrap();

        let loaded = load_document(&path).unwrap();
        let page = loaded.root.read().unwrap();
        assert_eq!(page.layers.len(), 3);
    }

    #[test]
    fn test_save_and_load_compressed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test_compressed.logos");

        let doc = make_test_doc();
        save_document(&doc, &path, true).unwrap();

        let loaded = load_document(&path).unwrap();
        let page = loaded.root.read().unwrap();
        assert_eq!(page.layers.len(), 3);
    }

    #[test]
    fn test_compressed_smaller_than_uncompressed() {
        let dir = tempfile::tempdir().unwrap();
        let path_plain = dir.path().join("plain.logos");
        let path_comp = dir.path().join("compressed.logos");

        // Create a document with many layers to make compression worthwhile.
        let doc = Document::new();
        for i in 0..100 {
            doc.add_layer(Layer::Rect(RectLayer::new(
                i as f32 * 10.0, i as f32 * 5.0, 100.0, 50.0,
            ))).unwrap();
        }

        save_document(&doc, &path_plain, false).unwrap();
        save_document(&doc, &path_comp, true).unwrap();

        let plain_size = fs::metadata(&path_plain).unwrap().len();
        let comp_size = fs::metadata(&path_comp).unwrap().len();
        assert!(comp_size < plain_size, "Compressed ({comp_size}) should be smaller than plain ({plain_size})");
    }

    #[test]
    fn test_invalid_magic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad_magic.logos");

        let mut file = fs::File::create(&path).unwrap();
        file.write_all(b"BADM\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00").unwrap();

        let result = load_document(&path);
        assert!(matches!(result, Err(FileError::InvalidMagic)));
    }

    #[test]
    fn test_unsupported_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("future.logos");

        let mut file = fs::File::create(&path).unwrap();
        file.write_all(MAGIC).unwrap();
        file.write_all(&999u32.to_le_bytes()).unwrap(); // Future version.
        file.write_all(&0u32.to_le_bytes()).unwrap(); // flags
        file.write_all(&0u64.to_le_bytes()).unwrap(); // len

        let result = load_document(&path);
        assert!(matches!(result, Err(FileError::UnsupportedVersion(999))));
    }

    #[test]
    fn test_truncated_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("truncated.logos");

        let mut file = fs::File::create(&path).unwrap();
        file.write_all(MAGIC).unwrap();
        // Missing rest of header.

        let result = load_document(&path);
        assert!(matches!(result, Err(FileError::Truncated)));
    }

    #[test]
    fn test_empty_document_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.logos");

        let doc = Document::new();
        save_document(&doc, &path, false).unwrap();
        let loaded = load_document(&path).unwrap();
        let page = loaded.root.read().unwrap();
        assert_eq!(page.layers.len(), 0);
    }

    #[test]
    fn test_layer_types_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("layers.logos");

        let doc = Document::new();
        doc.add_layer(Layer::Rect(RectLayer::new(0.0, 0.0, 50.0, 50.0))).unwrap();
        doc.add_layer(Layer::Ellipse(EllipseLayer::new(100.0, 100.0, 60.0, 40.0))).unwrap();
        doc.add_layer(Layer::Text(TextLayer::new("Test", 0.0, 0.0, 100.0, 20.0))).unwrap();

        save_document(&doc, &path, true).unwrap();
        let loaded = load_document(&path).unwrap();
        let page = loaded.root.read().unwrap();
        assert_eq!(page.layers.len(), 3);

        // Check layer types.
        assert!(matches!(&page.layers[0], Layer::Rect(_)));
        assert!(matches!(&page.layers[1], Layer::Ellipse(_)));
        assert!(matches!(&page.layers[2], Layer::Text(_)));
    }

    #[test]
    fn test_text_content_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("text.logos");

        let doc = Document::new();
        doc.add_layer(Layer::Text(TextLayer::new(
            "Hello, 世界! café 🎨",
            10.0, 20.0, 300.0, 40.0,
        ))).unwrap();

        save_document(&doc, &path, true).unwrap();
        let loaded = load_document(&path).unwrap();
        let page = loaded.root.read().unwrap();

        if let Layer::Text(t) = &page.layers[0] {
            assert_eq!(t.content, "Hello, 世界! café 🎨");
        } else {
            panic!("Expected Text layer");
        }
    }

    #[test]
    fn test_file_error_display() {
        let e = FileError::InvalidMagic;
        assert_eq!(format!("{e}"), "Not a valid .logos file");

        let e = FileError::UnsupportedVersion(42);
        assert_eq!(format!("{e}"), "Unsupported version: 42");
    }
}
