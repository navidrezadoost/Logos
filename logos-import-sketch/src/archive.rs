//! Sketch ZIP archive extractor.
//!
//! .sketch files are ZIP archives. This module provides a minimal
//! ZIP reader focused on extracting the JSON payloads without
//! needing a full ZIP library dependency.

use crate::model::{SketchDocument, SketchLayer, SketchPage, SketchFrame};
use logos_import_common::{ImportError, ImportResult};
use std::io::Read;

/// Extract a Sketch document from a ZIP archive.
pub fn extract_sketch(data: &[u8]) -> ImportResult<SketchDocument> {
    let entries = read_zip_entries(data)?;

    // Find document.json
    let _doc_json = entries
        .iter()
        .find(|e| e.name == "document.json")
        .map(|e| &e.data);

    // Find all page files
    let mut pages = Vec::new();
    for entry in &entries {
        if entry.name.starts_with("pages/") && entry.name.ends_with(".json") {
            let page: SketchPage = serde_json::from_slice(&entry.data).map_err(|e| {
                ImportError::ParseError {
                    offset: 0,
                    message: format!("invalid page JSON '{}': {}", entry.name, e),
                }
            })?;
            pages.push(page);
        }
    }

    Ok(SketchDocument {
        id: String::new(),
        class: "document".to_string(),
        pages,
    })
}

/// Build a synthetic .sketch file for testing.
pub fn build_test_sketch(layers: &[SketchLayer]) -> Vec<u8> {
    let page = SketchPage {
        id: "page-1".to_string(),
        name: "Page 1".to_string(),
        class: "page".to_string(),
        layers: layers.to_vec(),
        frame: SketchFrame::default(),
    };

    let page_json = serde_json::to_vec(&page).unwrap();
    let doc_json = br#"{"_class":"document","do_objectID":"doc-1","pages":[]}"#;

    build_zip(&[
        ("document.json", doc_json),
        ("pages/page-1.json", &page_json),
    ])
}

// ── Minimal ZIP implementation ──────────────────────────────────

struct ZipEntry {
    name: String,
    data: Vec<u8>,
}

/// Read ZIP entries from raw bytes (minimal implementation).
fn read_zip_entries(data: &[u8]) -> ImportResult<Vec<ZipEntry>> {
    // Find End of Central Directory record
    let eocd_pos = find_eocd(data).ok_or_else(|| {
        ImportError::UnrecognizedFormat("not a valid ZIP file (no EOCD)".into())
    })?;

    if eocd_pos + 22 > data.len() {
        return Err(ImportError::UnexpectedEof(eocd_pos));
    }

    let cd_offset = read_u32_le(data, eocd_pos + 16) as usize;
    let entry_count = read_u16_le(data, eocd_pos + 10) as usize;

    let mut entries = Vec::with_capacity(entry_count);
    let mut pos = cd_offset;

    for _ in 0..entry_count {
        if pos + 46 > data.len() {
            break;
        }
        // Central directory file header signature: 0x02014b50
        let sig = read_u32_le(data, pos);
        if sig != 0x02014b50 {
            break;
        }

        let compression = read_u16_le(data, pos + 10);
        let compressed_size = read_u32_le(data, pos + 20) as usize;
        let uncompressed_size = read_u32_le(data, pos + 24) as usize;
        let name_len = read_u16_le(data, pos + 28) as usize;
        let extra_len = read_u16_le(data, pos + 30) as usize;
        let comment_len = read_u16_le(data, pos + 32) as usize;
        let local_offset = read_u32_le(data, pos + 42) as usize;

        let name_bytes = &data[pos + 46..pos + 46 + name_len];
        let name = String::from_utf8_lossy(name_bytes).to_string();

        pos += 46 + name_len + extra_len + comment_len;

        // Read from local file header
        if local_offset + 30 > data.len() {
            continue;
        }
        let local_name_len = read_u16_le(data, local_offset + 26) as usize;
        let local_extra_len = read_u16_le(data, local_offset + 28) as usize;
        let file_data_start = local_offset + 30 + local_name_len + local_extra_len;

        if file_data_start + compressed_size > data.len() {
            continue;
        }

        let file_data = &data[file_data_start..file_data_start + compressed_size];

        let decompressed = match compression {
            0 => file_data.to_vec(), // stored
            8 => {
                // deflate
                let mut decoder =
                    flate2::read::DeflateDecoder::new(file_data);
                let mut buf = Vec::with_capacity(uncompressed_size);
                decoder.read_to_end(&mut buf).map_err(|e| {
                    ImportError::DecompressionFailed(format!("{}: {}", name, e))
                })?;
                buf
            }
            _ => {
                continue; // skip unsupported compression
            }
        };

        entries.push(ZipEntry {
            name,
            data: decompressed,
        });
    }

    Ok(entries)
}

fn find_eocd(data: &[u8]) -> Option<usize> {
    // EOCD signature: 0x06054b50
    let sig = [0x50, 0x4b, 0x05, 0x06];
    // Search backwards from end (EOCD is at most 65535+22 bytes from end)
    let search_start = if data.len() > 65557 {
        data.len() - 65557
    } else {
        0
    };

    for i in (search_start..data.len().saturating_sub(3)).rev() {
        if data[i] == sig[0]
            && data[i + 1] == sig[1]
            && data[i + 2] == sig[2]
            && data[i + 3] == sig[3]
        {
            return Some(i);
        }
    }
    None
}

fn read_u16_le(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([data[offset], data[offset + 1]])
}

fn read_u32_le(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}

/// Build a minimal ZIP file from name/data pairs.
fn build_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    use flate2::write::DeflateEncoder;
    use flate2::Compression;
    use std::io::Write;

    let mut output = Vec::new();
    let mut central_dir = Vec::new();
    let mut offsets = Vec::new();

    for (name, data) in entries {
        let name_bytes = name.as_bytes();
        let offset = output.len();
        offsets.push(offset);

        // Compress
        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(data).unwrap();
        let compressed = encoder.finish().unwrap();

        // Local file header
        output.extend_from_slice(&[0x50, 0x4b, 0x03, 0x04]); // signature
        output.extend_from_slice(&20u16.to_le_bytes()); // version needed
        output.extend_from_slice(&0u16.to_le_bytes()); // flags
        output.extend_from_slice(&8u16.to_le_bytes()); // compression: deflate
        output.extend_from_slice(&0u16.to_le_bytes()); // mod time
        output.extend_from_slice(&0u16.to_le_bytes()); // mod date
        output.extend_from_slice(&0u32.to_le_bytes()); // crc32 (simplified)
        output.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
        output.extend_from_slice(&(data.len() as u32).to_le_bytes());
        output.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        output.extend_from_slice(&0u16.to_le_bytes()); // extra field len
        output.extend_from_slice(name_bytes);
        output.extend_from_slice(&compressed);

        // Central directory entry
        central_dir.extend_from_slice(&[0x50, 0x4b, 0x01, 0x02]); // signature
        central_dir.extend_from_slice(&20u16.to_le_bytes()); // version made by
        central_dir.extend_from_slice(&20u16.to_le_bytes()); // version needed
        central_dir.extend_from_slice(&0u16.to_le_bytes()); // flags
        central_dir.extend_from_slice(&8u16.to_le_bytes()); // compression
        central_dir.extend_from_slice(&0u16.to_le_bytes()); // mod time
        central_dir.extend_from_slice(&0u16.to_le_bytes()); // mod date
        central_dir.extend_from_slice(&0u32.to_le_bytes()); // crc32
        central_dir.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
        central_dir.extend_from_slice(&(data.len() as u32).to_le_bytes());
        central_dir.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        central_dir.extend_from_slice(&0u16.to_le_bytes()); // extra field len
        central_dir.extend_from_slice(&0u16.to_le_bytes()); // comment len
        central_dir.extend_from_slice(&0u16.to_le_bytes()); // disk number
        central_dir.extend_from_slice(&0u16.to_le_bytes()); // internal attr
        central_dir.extend_from_slice(&0u32.to_le_bytes()); // external attr
        central_dir.extend_from_slice(&(offset as u32).to_le_bytes());
        central_dir.extend_from_slice(name_bytes);
    }

    let cd_offset = output.len();
    output.extend_from_slice(&central_dir);

    // End of Central Directory
    output.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06]); // signature
    output.extend_from_slice(&0u16.to_le_bytes()); // disk number
    output.extend_from_slice(&0u16.to_le_bytes()); // cd disk
    output.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    output.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    output.extend_from_slice(&(central_dir.len() as u32).to_le_bytes());
    output.extend_from_slice(&(cd_offset as u32).to_le_bytes());
    output.extend_from_slice(&0u16.to_le_bytes()); // comment len

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_and_read_zip() {
        let data = build_zip(&[
            ("hello.txt", b"Hello, world!"),
            ("nested/file.json", b"{\"key\": \"value\"}"),
        ]);
        let entries = read_zip_entries(&data).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "hello.txt");
        assert_eq!(entries[0].data, b"Hello, world!");
        assert_eq!(entries[1].name, "nested/file.json");
    }

    #[test]
    fn test_build_test_sketch() {
        let layers = vec![
            SketchLayer::rect("1", "R1", 0.0, 0.0, 100.0, 50.0),
        ];
        let data = build_test_sketch(&layers);
        let doc = extract_sketch(&data).unwrap();
        assert_eq!(doc.pages.len(), 1);
        assert_eq!(doc.pages[0].layers.len(), 1);
    }

    #[test]
    fn test_invalid_zip() {
        let result = read_zip_entries(b"not a zip");
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_page_name() {
        let layers = vec![];
        let data = build_test_sketch(&layers);
        let doc = extract_sketch(&data).unwrap();
        assert_eq!(doc.pages[0].name, "Page 1");
    }
}
