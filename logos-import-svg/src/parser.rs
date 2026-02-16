//! SVG XML parser.
//!
//! A minimal, dependency-free SVG XML parser that extracts the elements
//! and attributes we need without pulling in a full XML library.

use logos_import_common::{ImportError, ImportResult};

/// A parsed SVG element node.
#[derive(Debug, Clone)]
pub struct SvgNode {
    /// Element tag name (e.g. `"rect"`, `"circle"`, `"g"`).
    pub tag: String,
    /// Attributes as key-value pairs.
    pub attrs: Vec<(String, String)>,
    /// Child elements.
    pub children: Vec<SvgNode>,
    /// Text content (for `<text>` elements).
    pub text: String,
}

impl SvgNode {
    pub fn new(tag: &str) -> Self {
        Self {
            tag: tag.to_string(),
            attrs: Vec::new(),
            children: Vec::new(),
            text: String::new(),
        }
    }

    /// Get an attribute value by name.
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// Get an attribute as f32, returning 0.0 if missing or unparseable.
    pub fn attr_f32(&self, name: &str) -> f32 {
        self.attr(name)
            .and_then(|v| parse_length(v))
            .unwrap_or(0.0)
    }

    /// Get an attribute as f32, returning None if missing.
    pub fn attr_f32_opt(&self, name: &str) -> Option<f32> {
        self.attr(name).and_then(|v| parse_length(v))
    }
}

/// Parse a length value, stripping units like "px", "pt", "em", "%".
fn parse_length(s: &str) -> Option<f32> {
    let s = s.trim();
    // Strip known suffixes
    let clean = s
        .trim_end_matches("px")
        .trim_end_matches("pt")
        .trim_end_matches("em")
        .trim_end_matches("rem")
        .trim_end_matches('%')
        .trim();
    clean.parse::<f32>().ok()
}

/// Parse SVG XML bytes into a tree of `SvgNode`s.
pub fn parse_svg(data: &[u8]) -> ImportResult<SvgNode> {
    let text = std::str::from_utf8(data)
        .map_err(|e| ImportError::EncodingError(format!("invalid UTF-8: {}", e)))?;

    let mut parser = XmlParser::new(text);
    parser
        .parse_document()
        .ok_or_else(|| ImportError::ParseError {
            offset: 0,
            message: "failed to parse SVG XML".into(),
        })
}

// ── Minimal XML parser ──────────────────────────────────────────

struct XmlParser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> XmlParser<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    fn remaining(&self) -> &'a str {
        &self.input[self.pos..]
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.input.len() {
            let ch = self.input.as_bytes()[self.pos];
            if ch == b' ' || ch == b'\t' || ch == b'\n' || ch == b'\r' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn skip_xml_declaration(&mut self) {
        self.skip_whitespace();
        if self.remaining().starts_with("<?") {
            if let Some(end) = self.remaining().find("?>") {
                self.pos += end + 2;
            }
        }
    }

    fn skip_doctype(&mut self) {
        self.skip_whitespace();
        if self.remaining().starts_with("<!DOCTYPE") || self.remaining().starts_with("<!doctype") {
            if let Some(end) = self.remaining().find('>') {
                self.pos += end + 1;
            }
        }
    }

    fn skip_comment(&mut self) -> bool {
        if self.remaining().starts_with("<!--") {
            if let Some(end) = self.remaining().find("-->") {
                self.pos += end + 3;
                return true;
            }
        }
        false
    }

    fn parse_document(&mut self) -> Option<SvgNode> {
        self.skip_xml_declaration();
        self.skip_whitespace();
        self.skip_doctype();
        self.skip_whitespace();
        self.parse_element()
    }

    fn parse_element(&mut self) -> Option<SvgNode> {
        self.skip_whitespace();
        while self.skip_comment() {
            self.skip_whitespace();
        }

        if !self.remaining().starts_with('<') {
            return None;
        }
        if self.remaining().starts_with("</") {
            return None;
        }

        self.pos += 1; // skip '<'

        // Read tag name
        let tag_start = self.pos;
        while self.pos < self.input.len() {
            let ch = self.input.as_bytes()[self.pos];
            if ch == b' ' || ch == b'/' || ch == b'>' || ch == b'\t' || ch == b'\n' || ch == b'\r'
            {
                break;
            }
            self.pos += 1;
        }
        let tag = self.input[tag_start..self.pos].to_string();

        // Parse attributes
        let mut attrs = Vec::new();
        loop {
            self.skip_whitespace();
            if self.pos >= self.input.len() {
                break;
            }
            let ch = self.input.as_bytes()[self.pos];
            if ch == b'/' || ch == b'>' {
                break;
            }
            if let Some((key, value)) = self.parse_attribute() {
                attrs.push((key, value));
            } else {
                self.pos += 1; // skip problematic char
            }
        }

        // Self-closing or open tag?
        if self.remaining().starts_with("/>") {
            self.pos += 2;
            return Some(SvgNode {
                tag,
                attrs,
                children: Vec::new(),
                text: String::new(),
            });
        }

        if self.remaining().starts_with('>') {
            self.pos += 1;
        } else {
            return None;
        }

        // Parse children and text content
        let mut children = Vec::new();
        let mut text_content = String::new();

        loop {
            self.skip_whitespace();
            while self.skip_comment() {
                self.skip_whitespace();
            }

            if self.pos >= self.input.len() {
                break;
            }

            // Closing tag?
            if self.remaining().starts_with("</") {
                // Skip closing tag
                if let Some(end) = self.remaining().find('>') {
                    self.pos += end + 1;
                }
                break;
            }

            // CDATA section
            if self.remaining().starts_with("<![CDATA[") {
                self.pos += 9;
                if let Some(end) = self.remaining().find("]]>") {
                    text_content.push_str(&self.input[self.pos..self.pos + end]);
                    self.pos += end + 3;
                }
                continue;
            }

            // Child element
            if self.remaining().starts_with('<') {
                if let Some(child) = self.parse_element() {
                    children.push(child);
                }
                continue;
            }

            // Text content
            let text_start = self.pos;
            while self.pos < self.input.len() && self.input.as_bytes()[self.pos] != b'<' {
                self.pos += 1;
            }
            let raw = self.input[text_start..self.pos].trim();
            if !raw.is_empty() {
                text_content.push_str(&decode_xml_entities(raw));
            }
        }

        Some(SvgNode {
            tag,
            attrs,
            children,
            text: text_content,
        })
    }

    fn parse_attribute(&mut self) -> Option<(String, String)> {
        let key_start = self.pos;
        while self.pos < self.input.len() {
            let ch = self.input.as_bytes()[self.pos];
            if ch == b'=' || ch == b' ' || ch == b'/' || ch == b'>' {
                break;
            }
            self.pos += 1;
        }
        let key = self.input[key_start..self.pos].to_string();
        if key.is_empty() {
            return None;
        }

        self.skip_whitespace();
        if self.pos >= self.input.len() || self.input.as_bytes()[self.pos] != b'=' {
            return Some((key, String::new()));
        }
        self.pos += 1; // skip '='
        self.skip_whitespace();

        if self.pos >= self.input.len() {
            return Some((key, String::new()));
        }

        let quote = self.input.as_bytes()[self.pos];
        if quote == b'"' || quote == b'\'' {
            self.pos += 1;
            let val_start = self.pos;
            while self.pos < self.input.len() && self.input.as_bytes()[self.pos] != quote {
                self.pos += 1;
            }
            let value = self.input[val_start..self.pos].to_string();
            if self.pos < self.input.len() {
                self.pos += 1; // skip closing quote
            }
            Some((key, decode_xml_entities(&value)))
        } else {
            // Unquoted value
            let val_start = self.pos;
            while self.pos < self.input.len() {
                let ch = self.input.as_bytes()[self.pos];
                if ch == b' ' || ch == b'/' || ch == b'>' {
                    break;
                }
                self.pos += 1;
            }
            let value = self.input[val_start..self.pos].to_string();
            Some((key, value))
        }
    }
}

fn decode_xml_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

/// Parse a CSS/SVG color value to (r, g, b) in 0-255.
pub fn parse_color(s: &str) -> Option<(u8, u8, u8)> {
    let s = s.trim();
    if s.starts_with('#') {
        let hex = &s[1..];
        match hex.len() {
            3 => {
                let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
                let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
                let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
                Some((r, g, b))
            }
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                Some((r, g, b))
            }
            _ => None,
        }
    } else {
        match s.to_lowercase().as_str() {
            "black" => Some((0, 0, 0)),
            "white" => Some((255, 255, 255)),
            "red" => Some((255, 0, 0)),
            "green" => Some((0, 128, 0)),
            "blue" => Some((0, 0, 255)),
            "yellow" => Some((255, 255, 0)),
            "cyan" | "aqua" => Some((0, 255, 255)),
            "magenta" | "fuchsia" => Some((255, 0, 255)),
            "gray" | "grey" => Some((128, 128, 128)),
            "orange" => Some((255, 165, 0)),
            "purple" => Some((128, 0, 128)),
            "lime" => Some((0, 255, 0)),
            "navy" => Some((0, 0, 128)),
            "teal" => Some((0, 128, 128)),
            "silver" => Some((192, 192, 192)),
            "maroon" => Some((128, 0, 0)),
            "olive" => Some((128, 128, 0)),
            "none" | "transparent" => None,
            _ => None,
        }
    }
}

/// Parse a `points` attribute (for polyline/polygon).
pub fn parse_points(s: &str) -> Vec<(f32, f32)> {
    let mut points = Vec::new();
    let nums: Vec<f32> = s
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|t| !t.is_empty())
        .filter_map(|t| t.parse::<f32>().ok())
        .collect();
    for pair in nums.chunks(2) {
        if pair.len() == 2 {
            points.push((pair[0], pair[1]));
        }
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_svg() {
        let svg = r#"<svg width="100" height="100"><rect x="10" y="20" width="80" height="60"/></svg>"#;
        let node = parse_svg(svg.as_bytes()).unwrap();
        assert_eq!(node.tag, "svg");
        assert_eq!(node.children.len(), 1);
        assert_eq!(node.children[0].tag, "rect");
        assert_eq!(node.children[0].attr_f32("x"), 10.0);
    }

    #[test]
    fn test_parse_nested_groups() {
        let svg = r#"<svg><g id="g1"><g id="g2"><rect/></g></g></svg>"#;
        let node = parse_svg(svg.as_bytes()).unwrap();
        assert_eq!(node.children.len(), 1);
        assert_eq!(node.children[0].tag, "g");
        assert_eq!(node.children[0].children[0].tag, "g");
    }

    #[test]
    fn test_parse_text_content() {
        let svg = r#"<svg><text x="10" y="20">Hello World</text></svg>"#;
        let node = parse_svg(svg.as_bytes()).unwrap();
        assert_eq!(node.children[0].text, "Hello World");
    }

    #[test]
    fn test_parse_self_closing() {
        let svg = r#"<svg><circle cx="50" cy="50" r="40"/></svg>"#;
        let node = parse_svg(svg.as_bytes()).unwrap();
        assert_eq!(node.children[0].tag, "circle");
        assert_eq!(node.children[0].attr_f32("r"), 40.0);
    }

    #[test]
    fn test_parse_with_xml_declaration() {
        let svg = r#"<?xml version="1.0" encoding="UTF-8"?><svg><rect/></svg>"#;
        let node = parse_svg(svg.as_bytes()).unwrap();
        assert_eq!(node.tag, "svg");
    }

    #[test]
    fn test_parse_color_hex() {
        assert_eq!(parse_color("#ff0000"), Some((255, 0, 0)));
        assert_eq!(parse_color("#00FF00"), Some((0, 255, 0)));
        assert_eq!(parse_color("#f00"), Some((255, 0, 0)));
    }

    #[test]
    fn test_parse_color_named() {
        assert_eq!(parse_color("red"), Some((255, 0, 0)));
        assert_eq!(parse_color("blue"), Some((0, 0, 255)));
        assert_eq!(parse_color("none"), None);
    }

    #[test]
    fn test_parse_points() {
        let pts = parse_points("10,20 30,40 50,60");
        assert_eq!(pts.len(), 3);
        assert_eq!(pts[0], (10.0, 20.0));
        assert_eq!(pts[2], (50.0, 60.0));
    }

    #[test]
    fn test_parse_length_with_units() {
        assert_eq!(parse_length("100px"), Some(100.0));
        assert_eq!(parse_length("50pt"), Some(50.0));
        assert_eq!(parse_length("24"), Some(24.0));
    }

    #[test]
    fn test_attr_f32() {
        let mut node = SvgNode::new("rect");
        node.attrs.push(("width".into(), "100".into()));
        assert_eq!(node.attr_f32("width"), 100.0);
        assert_eq!(node.attr_f32("height"), 0.0);
    }

    #[test]
    fn test_xml_entities() {
        let svg = r#"<svg><text>&amp; &lt; &gt;</text></svg>"#;
        let node = parse_svg(svg.as_bytes()).unwrap();
        assert_eq!(node.children[0].text, "& < >");
    }

    #[test]
    fn test_parse_comments() {
        let svg = r#"<svg><!-- a comment --><rect/></svg>"#;
        let node = parse_svg(svg.as_bytes()).unwrap();
        assert_eq!(node.children.len(), 1);
        assert_eq!(node.children[0].tag, "rect");
    }

    #[test]
    fn test_parse_multiple_attributes() {
        let svg = r#"<svg><rect x="1" y="2" width="3" height="4" fill="red" stroke="blue"/></svg>"#;
        let node = parse_svg(svg.as_bytes()).unwrap();
        let rect = &node.children[0];
        assert_eq!(rect.attr("fill"), Some("red"));
        assert_eq!(rect.attr("stroke"), Some("blue"));
        assert_eq!(rect.attr_f32("width"), 3.0);
    }
}
