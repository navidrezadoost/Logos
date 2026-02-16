//! Minimal PDF binary parser.
//!
//! Parses PDF structure enough to extract text and vector elements.
//! This is NOT a full PDF parser — it handles the subset needed for
//! design-tool import (text positioning, rectangles, paths).

use logos_import_common::error::{ImportError, ImportResult};

use crate::content::{PathCmd, PdfDocument, PdfElement, PdfPage};

/// Parse PDF binary data into a [`PdfDocument`].
pub fn parse_pdf(data: &[u8]) -> ImportResult<PdfDocument> {
    // Validate the PDF header
    if data.len() < 8 {
        return Err(ImportError::UnexpectedEof(data.len()));
    }
    if !data.starts_with(b"%PDF-") {
        return Err(ImportError::UnrecognizedFormat(
            "Missing %PDF- header".into(),
        ));
    }

    // Extract version string
    let version = extract_version(data);

    // Find and parse page content
    let pages = extract_pages(data)?;

    Ok(PdfDocument { pages, version })
}

/// Extract the PDF version from the header line.
fn extract_version(data: &[u8]) -> String {
    let end = data.iter().position(|&b| b == b'\n' || b == b'\r').unwrap_or(8.min(data.len()));
    let header = &data[5..end];
    String::from_utf8_lossy(header).trim().to_string()
}

/// Extract pages from PDF data.
///
/// This simplified parser looks for stream/endstream blocks and
/// attempts to parse the content-stream operators within them.
fn extract_pages(data: &[u8]) -> ImportResult<Vec<PdfPage>> {
    let text = String::from_utf8_lossy(data);

    // Find MediaBox for page dimensions
    let (page_w, page_h) = extract_media_box(&text).unwrap_or((612.0, 792.0)); // US Letter default

    // Find content streams
    let streams = extract_streams(&text);

    if streams.is_empty() {
        // Return single empty page
        return Ok(vec![PdfPage {
            width: page_w,
            height: page_h,
            elements: vec![],
            page_number: 1,
        }]);
    }

    let mut pages = Vec::new();
    for (i, stream_content) in streams.iter().enumerate() {
        let elements = parse_content_stream(stream_content);
        pages.push(PdfPage {
            width: page_w,
            height: page_h,
            elements,
            page_number: i + 1,
        });
    }

    Ok(pages)
}

/// Extract MediaBox dimensions from PDF text.
fn extract_media_box(text: &str) -> Option<(f32, f32)> {
    let idx = text.find("/MediaBox")?;
    let rest = &text[idx..];
    let open = rest.find('[')?;
    let close = rest.find(']')?;
    let coords = &rest[open + 1..close];
    let nums: Vec<f32> = coords
        .split_whitespace()
        .filter_map(|s| s.parse().ok())
        .collect();
    if nums.len() >= 4 {
        Some((nums[2] - nums[0], nums[3] - nums[1]))
    } else {
        None
    }
}

/// Extract content between `stream` / `endstream` markers.
fn extract_streams(text: &str) -> Vec<String> {
    let mut streams = Vec::new();
    let mut search_from = 0;

    while let Some(start) = text[search_from..].find("stream\n") {
        let abs_start = search_from + start + 7; // after "stream\n"
        if let Some(end) = text[abs_start..].find("endstream") {
            let content = &text[abs_start..abs_start + end];
            streams.push(content.trim().to_string());
            search_from = abs_start + end + 9;
        } else {
            break;
        }
    }

    // Also handle \r\n line endings
    search_from = 0;
    while let Some(start) = text[search_from..].find("stream\r\n") {
        let abs_start = search_from + start + 8;
        if let Some(end) = text[abs_start..].find("endstream") {
            let content = &text[abs_start..abs_start + end];
            let trimmed = content.trim().to_string();
            // Avoid duplicates
            if !streams.contains(&trimmed) {
                streams.push(trimmed);
            }
            search_from = abs_start + end + 9;
        } else {
            break;
        }
    }

    streams
}

/// Parse a PDF content stream into elements.
///
/// Handles operators: `re` (rectangle), `m` (moveto), `l` (lineto),
/// `c` (curveto), `h` (close), `BT`/`ET` (text blocks), `Tm` (text matrix),
/// `Tf` (font size), `Tj`/`TJ` (show text).
fn parse_content_stream(content: &str) -> Vec<PdfElement> {
    let mut elements = Vec::new();
    let mut stack: Vec<f32> = Vec::new();
    let mut in_text_block = false;
    let mut text_x: f32 = 0.0;
    let mut text_y: f32 = 0.0;
    let mut font_size: f32 = 12.0;
    let mut path_cmds: Vec<PathCmd> = Vec::new();

    for token in tokenize_content_stream(content) {
        match token.as_str() {
            // Text block
            "BT" => {
                in_text_block = true;
                text_x = 0.0;
                text_y = 0.0;
            }
            "ET" => {
                in_text_block = false;
            }
            // Text matrix (a b c d tx ty Tm)
            "Tm" => {
                if stack.len() >= 6 {
                    let len = stack.len();
                    text_x = stack[len - 2];
                    text_y = stack[len - 1];
                    stack.truncate(len - 6);
                }
            }
            // Text position (tx ty Td)
            "Td" | "TD" => {
                if stack.len() >= 2 {
                    let len = stack.len();
                    text_x += stack[len - 2];
                    text_y += stack[len - 1];
                    stack.truncate(len - 2);
                }
            }
            // Font selection (fontname size Tf)
            "Tf" => {
                if !stack.is_empty() {
                    font_size = stack.pop().unwrap_or(12.0);
                }
            }
            // Show text
            "Tj" => {
                // Text was already captured if preceded by a string
            }
            // Rectangle (x y w h re)
            "re" => {
                if stack.len() >= 4 {
                    let len = stack.len();
                    elements.push(PdfElement::Rect {
                        x: stack[len - 4],
                        y: stack[len - 3],
                        width: stack[len - 2],
                        height: stack[len - 1],
                    });
                    stack.truncate(len - 4);
                }
            }
            // Path: moveto
            "m" => {
                if stack.len() >= 2 {
                    let len = stack.len();
                    path_cmds.push(PathCmd::MoveTo(stack[len - 2], stack[len - 1]));
                    stack.truncate(len - 2);
                }
            }
            // Path: lineto
            "l" => {
                if stack.len() >= 2 {
                    let len = stack.len();
                    path_cmds.push(PathCmd::LineTo(stack[len - 2], stack[len - 1]));
                    stack.truncate(len - 2);
                }
            }
            // Path: curveto
            "c" => {
                if stack.len() >= 6 {
                    let len = stack.len();
                    path_cmds.push(PathCmd::CurveTo(
                        stack[len - 6],
                        stack[len - 5],
                        stack[len - 4],
                        stack[len - 3],
                        stack[len - 2],
                        stack[len - 1],
                    ));
                    stack.truncate(len - 6);
                }
            }
            // Path: close
            "h" => {
                path_cmds.push(PathCmd::Close);
            }
            // Stroke/Fill — finalize path
            "S" | "s" | "f" | "F" | "f*" | "B" | "B*" | "b" | "b*" | "n" => {
                if !path_cmds.is_empty() {
                    elements.push(PdfElement::Path {
                        commands: std::mem::take(&mut path_cmds),
                    });
                }
            }
            other => {
                // Try parsing as number
                if let Ok(n) = other.parse::<f32>() {
                    stack.push(n);
                }
                // Check for parenthesized text strings
                else if other.starts_with('(') && other.ends_with(')') && in_text_block {
                    let text_content = &other[1..other.len() - 1];
                    if !text_content.is_empty() {
                        elements.push(PdfElement::Text {
                            content: text_content.to_string(),
                            x: text_x,
                            y: text_y,
                            font_size,
                        });
                    }
                }
            }
        }
    }

    // Flush remaining path commands
    if !path_cmds.is_empty() {
        elements.push(PdfElement::Path {
            commands: path_cmds,
        });
    }

    elements
}

/// Tokenize a PDF content stream into individual tokens.
/// Handles parenthesized strings as single tokens.
fn tokenize_content_stream(content: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = content.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];

        // Skip whitespace
        if ch.is_whitespace() {
            i += 1;
            continue;
        }

        // Parenthesized string
        if ch == '(' {
            let mut depth = 1;
            let mut s = String::from("(");
            i += 1;
            while i < chars.len() && depth > 0 {
                let c = chars[i];
                if c == '(' && (i == 0 || chars[i - 1] != '\\') {
                    depth += 1;
                } else if c == ')' && (i == 0 || chars[i - 1] != '\\') {
                    depth -= 1;
                }
                s.push(c);
                i += 1;
            }
            tokens.push(s);
            continue;
        }

        // Skip comments
        if ch == '%' {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }

        // Regular token
        let start = i;
        while i < chars.len() && !chars[i].is_whitespace() && chars[i] != '(' && chars[i] != ')' {
            i += 1;
        }
        if i > start {
            tokens.push(chars[start..i].iter().collect());
        }
    }

    tokens
}

/// Build a minimal valid PDF binary for testing.
///
/// Creates a single-page PDF with the given elements encoded as a content stream.
pub fn build_test_pdf(elements: &[PdfElement], width: f32, height: f32) -> Vec<u8> {
    let mut content_stream = String::new();

    for el in elements {
        match el {
            PdfElement::Text {
                content,
                x,
                y,
                font_size,
            } => {
                content_stream.push_str("BT\n");
                content_stream.push_str(&format!("/F1 {} Tf\n", font_size));
                content_stream.push_str(&format!("1 0 0 1 {} {} Tm\n", x, y));
                content_stream.push_str(&format!("({}) Tj\n", content));
                content_stream.push_str("ET\n");
            }
            PdfElement::Rect {
                x,
                y,
                width: w,
                height: h,
            } => {
                content_stream.push_str(&format!("{} {} {} {} re S\n", x, y, w, h));
            }
            PdfElement::Path { commands } => {
                for cmd in commands {
                    match cmd {
                        PathCmd::MoveTo(x, y) => {
                            content_stream.push_str(&format!("{} {} m\n", x, y));
                        }
                        PathCmd::LineTo(x, y) => {
                            content_stream.push_str(&format!("{} {} l\n", x, y));
                        }
                        PathCmd::CurveTo(x1, y1, x2, y2, x3, y3) => {
                            content_stream.push_str(&format!(
                                "{} {} {} {} {} {} c\n",
                                x1, y1, x2, y2, x3, y3
                            ));
                        }
                        PathCmd::Close => {
                            content_stream.push_str("h\n");
                        }
                    }
                }
                content_stream.push_str("S\n");
            }
            PdfElement::Image {
                x,
                y,
                width: w,
                height: h,
            } => {
                // Placeholder rectangle for images
                content_stream.push_str(&format!("{} {} {} {} re S\n", x, y, w, h));
            }
        }
    }

    // Build a minimal PDF structure
    let mut pdf = String::new();
    pdf.push_str("%PDF-1.4\n");

    // Object 1: Catalog
    pdf.push_str("1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    // Object 2: Pages
    pdf.push_str("2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    // Object 3: Page
    pdf.push_str(&format!(
        "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>\nendobj\n",
        width, height
    ));

    // Object 4: Content stream
    let stream_len = content_stream.len();
    pdf.push_str(&format!(
        "4 0 obj\n<< /Length {} >>\nstream\n{}endstream\nendobj\n",
        stream_len, content_stream
    ));

    // Object 5: Font
    pdf.push_str(
        "5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n",
    );

    // Cross-reference table (simplified)
    let xref_offset = pdf.len();
    pdf.push_str("xref\n0 6\n");
    pdf.push_str("0000000000 65535 f \n");
    // Simplified — real PDFs track exact byte offsets
    pdf.push_str("0000000009 00000 n \n");
    pdf.push_str("0000000058 00000 n \n");
    pdf.push_str("0000000115 00000 n \n");
    pdf.push_str("0000000300 00000 n \n");
    pdf.push_str("0000000500 00000 n \n");

    // Trailer
    pdf.push_str(&format!(
        "trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
        xref_offset
    ));

    pdf.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{PathCmd, PdfElement};

    #[test]
    fn test_parse_valid_pdf() {
        let elements = vec![PdfElement::Text {
            content: "Hello".into(),
            x: 72.0,
            y: 720.0,
            font_size: 12.0,
        }];
        let data = build_test_pdf(&elements, 612.0, 792.0);
        let doc = parse_pdf(&data).unwrap();
        assert_eq!(doc.version, "1.4");
        assert!(!doc.pages.is_empty());
    }

    #[test]
    fn test_parse_invalid_header() {
        let data = b"NOT A PDF";
        let result = parse_pdf(data);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_too_short() {
        let data = b"%PDF";
        let result = parse_pdf(data);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_version() {
        let data = b"%PDF-1.7\n";
        assert_eq!(extract_version(data), "1.7");
    }

    #[test]
    fn test_extract_media_box() {
        let text = "/MediaBox [0 0 595 842]";
        let (w, h) = extract_media_box(text).unwrap();
        assert!((w - 595.0).abs() < 0.01);
        assert!((h - 842.0).abs() < 0.01);
    }

    #[test]
    fn test_content_stream_rect() {
        let content = "10 20 100 50 re S";
        let elements = parse_content_stream(content);
        assert_eq!(elements.len(), 1);
        match &elements[0] {
            PdfElement::Rect { x, y, width, height } => {
                assert_eq!(*x, 10.0);
                assert_eq!(*y, 20.0);
                assert_eq!(*width, 100.0);
                assert_eq!(*height, 50.0);
            }
            _ => panic!("Expected Rect"),
        }
    }

    #[test]
    fn test_content_stream_path() {
        let content = "0 0 m 100 0 l 100 100 l h S";
        let elements = parse_content_stream(content);
        assert_eq!(elements.len(), 1);
        match &elements[0] {
            PdfElement::Path { commands } => {
                assert_eq!(commands.len(), 4);
            }
            _ => panic!("Expected Path"),
        }
    }

    #[test]
    fn test_content_stream_text() {
        let content = "BT\n/F1 14 Tf\n1 0 0 1 72 700 Tm\n(Hello World) Tj\nET";
        let elements = parse_content_stream(content);
        assert!(!elements.is_empty());
        let has_text = elements.iter().any(|e| matches!(e, PdfElement::Text { .. }));
        assert!(has_text);
    }

    #[test]
    fn test_build_test_pdf_roundtrip() {
        let elements = vec![
            PdfElement::Rect {
                x: 10.0,
                y: 20.0,
                width: 100.0,
                height: 50.0,
            },
            PdfElement::Text {
                content: "Test".into(),
                x: 72.0,
                y: 700.0,
                font_size: 16.0,
            },
        ];
        let data = build_test_pdf(&elements, 612.0, 792.0);
        let doc = parse_pdf(&data).unwrap();
        assert_eq!(doc.pages.len(), 1);
        assert!(doc.pages[0].elements.len() >= 2);
    }

    #[test]
    fn test_tokenize_strings() {
        let tokens = tokenize_content_stream("(Hello World) Tj");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0], "(Hello World)");
        assert_eq!(tokens[1], "Tj");
    }
}
