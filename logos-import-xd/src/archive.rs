//! XD archive extraction.
//!
//! Adobe XD files are ZIP archives with a known internal layout.
//! The primary content is JSON-based artwork descriptions.

use logos_import_common::error::{ImportError, ImportResult};
use crate::model::{XdArtboard, XdDocument, XdNode};

/// Extract an XD ZIP archive into an [`XdDocument`].
pub fn extract_xd(data: &[u8]) -> ImportResult<XdDocument> {
    let entries = read_zip_entries(data)?;

    let mut artboards = Vec::new();

    for (path, content) in &entries {
        // Look for graphicContent.agc files (or any JSON with artwork)
        if path.ends_with(".agc") || path.contains("graphicContent") {
            if let Ok(json_str) = std::str::from_utf8(content) {
                if let Ok(agc) = serde_json::from_str::<serde_json::Value>(json_str) {
                    if let Some(ab) = parse_agc(&agc) {
                        artboards.push(ab);
                    }
                }
            }
        }
        // Also check for manifest
        if path == "manifest" || path.ends_with("manifest.json") {
            // Manifest parsing for metadata — best-effort
        }
    }

    // If no artboards found from AGC, try parsing content.json
    if artboards.is_empty() {
        for (path, content) in &entries {
            if path.ends_with("content.json") || path == "content" {
                if let Ok(json_str) = std::str::from_utf8(content) {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str) {
                        if let Some(ab) = parse_content_json(&val) {
                            artboards.push(ab);
                        }
                    }
                }
            }
        }
    }

    // Fallback: if still no artboards, try all JSON files
    if artboards.is_empty() {
        for (path, content) in &entries {
            if path.ends_with(".json") {
                if let Ok(json_str) = std::str::from_utf8(content) {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str) {
                        if let Some(nodes) = parse_node_array(&val) {
                            artboards.push(XdArtboard {
                                name: path.clone(),
                                width: 375.0,
                                height: 812.0,
                                children: nodes,
                            });
                        }
                    }
                }
            }
        }
    }

    Ok(XdDocument { artboards })
}

/// Parse a graphicContent.agc JSON value into an artboard.
fn parse_agc(agc: &serde_json::Value) -> Option<XdArtboard> {
    let children_val = agc.get("children")
        .or_else(|| agc.get("artboards"))
        .or_else(|| agc.get("layers"))?;

    let children = parse_node_array(children_val).unwrap_or_default();

    let name = agc.get("name")
        .and_then(|n| n.as_str())
        .unwrap_or("Artboard")
        .to_string();

    let width = agc.get("width")
        .and_then(|w| w.as_f64())
        .unwrap_or(375.0);

    let height = agc.get("height")
        .and_then(|h| h.as_f64())
        .unwrap_or(812.0);

    Some(XdArtboard {
        name,
        width,
        height,
        children,
    })
}

/// Parse a content.json root value.
fn parse_content_json(val: &serde_json::Value) -> Option<XdArtboard> {
    // Try extracting name and children
    let name = val.get("name")
        .and_then(|n| n.as_str())
        .unwrap_or("XD Document")
        .to_string();

    let children_val = val.get("children")
        .or_else(|| val.get("artboards"))
        .or_else(|| val.get("nodes"))?;

    let children = parse_node_array(children_val).unwrap_or_default();

    Some(XdArtboard {
        name,
        width: 375.0,
        height: 812.0,
        children,
    })
}

/// Parse an array of XdNode from a JSON value.
fn parse_node_array(val: &serde_json::Value) -> Option<Vec<XdNode>> {
    let arr = val.as_array()?;
    let mut nodes = Vec::new();

    for item in arr {
        if let Some(node) = parse_single_node(item) {
            nodes.push(node);
        }
    }

    if nodes.is_empty() {
        None
    } else {
        Some(nodes)
    }
}

/// Parse a single XdNode from a JSON value.
fn parse_single_node(val: &serde_json::Value) -> Option<XdNode> {
    let obj = val.as_object()?;

    let id = obj.get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let name = obj.get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let node_type = obj.get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("shape")
        .to_string();

    let shape_type = obj.get("shape_type")
        .or_else(|| obj.get("shapeType"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let text_content = obj.get("text_content")
        .or_else(|| obj.get("text"))
        .or_else(|| obj.get("rawText"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let visible = obj.get("visible")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let opacity = obj.get("opacity")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0);

    // Parse bounds
    let bounds_obj = obj.get("bounds")
        .or_else(|| obj.get("uxdesign#bounds"));

    let (bx, by, bw, bh) = if let Some(b) = bounds_obj {
        (
            b.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0),
            b.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0),
            b.get("width").and_then(|v| v.as_f64()).unwrap_or(0.0),
            b.get("height").and_then(|v| v.as_f64()).unwrap_or(0.0),
        )
    } else {
        // Fallback: try top-level x/y/width/height
        (
            obj.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0),
            obj.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0),
            obj.get("width").and_then(|v| v.as_f64()).unwrap_or(0.0),
            obj.get("height").and_then(|v| v.as_f64()).unwrap_or(0.0),
        )
    };

    let children_val = obj.get("children")
        .or_else(|| obj.get("group"))
        .or_else(|| obj.get("artboard"));

    let children = if let Some(cv) = children_val {
        parse_node_array(cv).unwrap_or_default()
    } else {
        vec![]
    };

    Some(XdNode {
        id,
        name,
        node_type,
        shape_type,
        transform: crate::model::XdTransform::default(),
        bounds: crate::model::XdBounds {
            x: bx,
            y: by,
            width: bw,
            height: bh,
        },
        text_content,
        visible,
        opacity,
        children,
    })
}

// ── Minimal ZIP reader ──

fn read_zip_entries(data: &[u8]) -> ImportResult<Vec<(String, Vec<u8>)>> {
    if data.len() < 22 {
        return Err(ImportError::UnrecognizedFormat(
            "Data too small for ZIP archive".into(),
        ));
    }

    // Find EOCD (End of Central Directory)
    let eocd_sig: [u8; 4] = [0x50, 0x4b, 0x05, 0x06];
    let eocd_pos = find_signature(data, &eocd_sig)
        .ok_or_else(|| ImportError::UnrecognizedFormat("No ZIP EOCD found".into()))?;

    if eocd_pos + 22 > data.len() {
        return Err(ImportError::InvalidStructure("Truncated EOCD".into()));
    }

    let cd_offset = u32::from_le_bytes([
        data[eocd_pos + 16],
        data[eocd_pos + 17],
        data[eocd_pos + 18],
        data[eocd_pos + 19],
    ]) as usize;

    let entry_count = u16::from_le_bytes([
        data[eocd_pos + 10],
        data[eocd_pos + 11],
    ]) as usize;

    let mut entries = Vec::new();
    let mut pos = cd_offset;

    for _ in 0..entry_count {
        if pos + 46 > data.len() {
            break;
        }

        // Verify central directory signature
        if data[pos..pos + 4] != [0x50, 0x4b, 0x01, 0x02] {
            break;
        }

        let compression = u16::from_le_bytes([data[pos + 10], data[pos + 11]]);
        let compressed_size = u32::from_le_bytes([
            data[pos + 20], data[pos + 21], data[pos + 22], data[pos + 23],
        ]) as usize;
        let name_len = u16::from_le_bytes([data[pos + 28], data[pos + 29]]) as usize;
        let extra_len = u16::from_le_bytes([data[pos + 30], data[pos + 31]]) as usize;
        let comment_len = u16::from_le_bytes([data[pos + 32], data[pos + 33]]) as usize;
        let local_offset = u32::from_le_bytes([
            data[pos + 42], data[pos + 43], data[pos + 44], data[pos + 45],
        ]) as usize;

        let name_start = pos + 46;
        if name_start + name_len > data.len() {
            break;
        }
        let name = String::from_utf8_lossy(&data[name_start..name_start + name_len]).to_string();

        // Read from local file header
        if local_offset + 30 <= data.len() {
            let local_name_len = u16::from_le_bytes([
                data[local_offset + 26],
                data[local_offset + 27],
            ]) as usize;
            let local_extra_len = u16::from_le_bytes([
                data[local_offset + 28],
                data[local_offset + 29],
            ]) as usize;
            let data_start = local_offset + 30 + local_name_len + local_extra_len;

            if data_start + compressed_size <= data.len() {
                let raw = &data[data_start..data_start + compressed_size];
                let content = if compression == 8 {
                    // Deflate
                    use flate2::read::DeflateDecoder;
                    use std::io::Read;
                    let mut decoder = DeflateDecoder::new(raw);
                    let mut out = Vec::new();
                    if decoder.read_to_end(&mut out).is_ok() {
                        out
                    } else {
                        raw.to_vec()
                    }
                } else {
                    raw.to_vec()
                };

                entries.push((name, content));
            }
        }

        pos = name_start + name_len + extra_len + comment_len;
    }

    Ok(entries)
}

fn find_signature(data: &[u8], sig: &[u8; 4]) -> Option<usize> {
    let start = if data.len() > 65557 { data.len() - 65557 } else { 0 };
    for i in (start..data.len().saturating_sub(3)).rev() {
        if data[i..i + 4] == *sig {
            return Some(i);
        }
    }
    None
}

// ── Test fixture builder ──

/// Build a test XD archive with predetermined content.
pub fn build_test_xd(nodes: &[XdNode], artboard_name: &str) -> Vec<u8> {
    // Build content JSON
    let content = serde_json::json!({
        "name": artboard_name,
        "children": nodes,
    });
    let json_bytes = serde_json::to_vec(&content).unwrap();

    // Wrap in a ZIP with a content.json entry
    build_zip(&[("content.json", &json_bytes)])
}

fn build_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut directory = Vec::new();
    let mut offsets = Vec::new();

    // Write local file headers + data
    for (name, data) in entries {
        offsets.push(buf.len());
        let name_bytes = name.as_bytes();

        // Local file header signature
        buf.extend_from_slice(&[0x50, 0x4b, 0x03, 0x04]);
        buf.extend_from_slice(&20u16.to_le_bytes()); // version needed
        buf.extend_from_slice(&0u16.to_le_bytes()); // flags
        buf.extend_from_slice(&0u16.to_le_bytes()); // compression: stored
        buf.extend_from_slice(&0u16.to_le_bytes()); // mod time
        buf.extend_from_slice(&0u16.to_le_bytes()); // mod date
        buf.extend_from_slice(&0u32.to_le_bytes()); // crc32 (skip for test)
        buf.extend_from_slice(&(data.len() as u32).to_le_bytes()); // compressed
        buf.extend_from_slice(&(data.len() as u32).to_le_bytes()); // uncompressed
        buf.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes()); // name len
        buf.extend_from_slice(&0u16.to_le_bytes()); // extra len
        buf.extend_from_slice(name_bytes);
        buf.extend_from_slice(data);
    }

    // Central directory
    let cd_offset = buf.len();
    for (i, (name, data)) in entries.iter().enumerate() {
        let name_bytes = name.as_bytes();
        let mut cd = Vec::new();
        cd.extend_from_slice(&[0x50, 0x4b, 0x01, 0x02]);
        cd.extend_from_slice(&20u16.to_le_bytes()); // version made by
        cd.extend_from_slice(&20u16.to_le_bytes()); // version needed
        cd.extend_from_slice(&0u16.to_le_bytes()); // flags
        cd.extend_from_slice(&0u16.to_le_bytes()); // compression
        cd.extend_from_slice(&0u16.to_le_bytes()); // mod time
        cd.extend_from_slice(&0u16.to_le_bytes()); // mod date
        cd.extend_from_slice(&0u32.to_le_bytes()); // crc32
        cd.extend_from_slice(&(data.len() as u32).to_le_bytes()); // compressed
        cd.extend_from_slice(&(data.len() as u32).to_le_bytes()); // uncompressed
        cd.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        cd.extend_from_slice(&0u16.to_le_bytes()); // extra len
        cd.extend_from_slice(&0u16.to_le_bytes()); // comment len
        cd.extend_from_slice(&0u16.to_le_bytes()); // disk number
        cd.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
        cd.extend_from_slice(&0u32.to_le_bytes()); // external attrs
        cd.extend_from_slice(&(offsets[i] as u32).to_le_bytes()); // local header offset
        cd.extend_from_slice(name_bytes);

        directory.extend_from_slice(&cd);
    }

    let cd_size = directory.len();
    buf.extend_from_slice(&directory);

    // EOCD
    buf.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06]);
    buf.extend_from_slice(&0u16.to_le_bytes()); // disk number
    buf.extend_from_slice(&0u16.to_le_bytes()); // disk with cd
    buf.extend_from_slice(&(entries.len() as u16).to_le_bytes()); // entries on disk
    buf.extend_from_slice(&(entries.len() as u16).to_le_bytes()); // total entries
    buf.extend_from_slice(&(cd_size as u32).to_le_bytes()); // cd size
    buf.extend_from_slice(&(cd_offset as u32).to_le_bytes()); // cd offset
    buf.extend_from_slice(&0u16.to_le_bytes()); // comment len

    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_and_extract_xd() {
        let nodes = vec![XdNode::rect("bg", 0.0, 0.0, 375.0, 812.0)];
        let data = build_test_xd(&nodes, "Test Screen");
        let doc = extract_xd(&data).unwrap();
        assert!(!doc.artboards.is_empty());
        assert_eq!(doc.artboards[0].name, "Test Screen");
    }

    #[test]
    fn test_extract_invalid_zip() {
        let result = extract_xd(b"not a zip");
        assert!(result.is_err());
    }

    #[test]
    fn test_build_zip_roundtrip() {
        let zip = build_zip(&[("hello.txt", b"Hello World")]);
        let entries = read_zip_entries(&zip).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, "hello.txt");
        assert_eq!(entries[0].1, b"Hello World");
    }

    #[test]
    fn test_extract_with_children() {
        let nodes = vec![
            XdNode::rect("r1", 0.0, 0.0, 100.0, 50.0),
            XdNode::text("t1", 10.0, 60.0, 200.0, 30.0, "Hello"),
        ];
        let data = build_test_xd(&nodes, "My Screen");
        let doc = extract_xd(&data).unwrap();
        assert_eq!(doc.artboards[0].children.len(), 2);
    }
}
