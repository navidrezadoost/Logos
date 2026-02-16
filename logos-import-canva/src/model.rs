//! Canva document model types.

use serde::{Deserialize, Serialize};

/// A Canva document / template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvaDocument {
    /// Document / design name.
    pub name: String,
    /// Canvas width in pixels.
    pub width: f64,
    /// Canvas height in pixels.
    pub height: f64,
    /// Top-level elements.
    pub elements: Vec<CanvaElement>,
}

/// A single element in a Canva design.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvaElement {
    /// Element name / label.
    pub name: String,
    /// Element type: "rect", "ellipse", "text", "image", "group".
    #[serde(rename = "type")]
    pub element_type: String,
    /// X position (left).
    #[serde(default)]
    pub x: f64,
    /// Y position (top).
    #[serde(default)]
    pub y: f64,
    /// Width.
    #[serde(default)]
    pub width: f64,
    /// Height.
    #[serde(default)]
    pub height: f64,
    /// Rotation in degrees.
    #[serde(default)]
    pub rotation: f64,
    /// Opacity (0.0 - 1.0).
    #[serde(default = "default_opacity")]
    pub opacity: f64,
    /// Visibility.
    #[serde(default = "default_visible")]
    pub visible: bool,
    /// Text content (for text elements).
    #[serde(default)]
    pub text: String,
    /// Font size (for text elements).
    #[serde(default = "default_font_size")]
    pub font_size: f64,
    /// Fill color as hex string (e.g. "#FF0000").
    #[serde(default)]
    pub fill: String,
    /// Corner radius (for rects).
    #[serde(default)]
    pub corner_radius: f64,
    /// Children (for groups).
    #[serde(default)]
    pub children: Vec<CanvaElement>,
}

fn default_opacity() -> f64 {
    1.0
}

fn default_visible() -> bool {
    true
}

fn default_font_size() -> f64 {
    16.0
}

// ── Constructors for testing ──

impl CanvaDocument {
    pub fn new(name: &str, width: f64, height: f64, elements: Vec<CanvaElement>) -> Self {
        Self {
            name: name.into(),
            width,
            height,
            elements,
        }
    }
}

impl CanvaElement {
    pub fn rect(name: &str, x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            name: name.into(),
            element_type: "rect".into(),
            x,
            y,
            width,
            height,
            rotation: 0.0,
            opacity: 1.0,
            visible: true,
            text: String::new(),
            font_size: 16.0,
            fill: String::new(),
            corner_radius: 0.0,
            children: vec![],
        }
    }

    pub fn ellipse(name: &str, x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            element_type: "ellipse".into(),
            ..Self::rect(name, x, y, width, height)
        }
    }

    pub fn text(name: &str, x: f64, y: f64, width: f64, height: f64, content: &str) -> Self {
        Self {
            element_type: "text".into(),
            text: content.into(),
            ..Self::rect(name, x, y, width, height)
        }
    }

    pub fn image(name: &str, x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            element_type: "image".into(),
            ..Self::rect(name, x, y, width, height)
        }
    }

    pub fn group(name: &str, children: Vec<CanvaElement>) -> Self {
        // Compute bounds from children
        let mut min_x = f64::MAX;
        let mut min_y = f64::MAX;
        let mut max_x = f64::MIN;
        let mut max_y = f64::MIN;
        for c in &children {
            min_x = min_x.min(c.x);
            min_y = min_y.min(c.y);
            max_x = max_x.max(c.x + c.width);
            max_y = max_y.max(c.y + c.height);
        }
        let (x, y, w, h) = if children.is_empty() {
            (0.0, 0.0, 0.0, 0.0)
        } else {
            (min_x, min_y, max_x - min_x, max_y - min_y)
        };

        Self {
            element_type: "group".into(),
            children,
            x,
            y,
            width: w,
            height: h,
            ..Self::rect(name, 0.0, 0.0, 0.0, 0.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rect_constructor() {
        let e = CanvaElement::rect("bg", 0.0, 0.0, 100.0, 50.0);
        assert_eq!(e.element_type, "rect");
        assert_eq!(e.width, 100.0);
    }

    #[test]
    fn test_text_constructor() {
        let e = CanvaElement::text("t", 10.0, 20.0, 200.0, 30.0, "Hello");
        assert_eq!(e.text, "Hello");
        assert_eq!(e.element_type, "text");
    }

    #[test]
    fn test_group_bounds() {
        let g = CanvaElement::group("grp", vec![
            CanvaElement::rect("a", 0.0, 0.0, 50.0, 50.0),
            CanvaElement::rect("b", 100.0, 100.0, 50.0, 50.0),
        ]);
        assert_eq!(g.width, 150.0);
        assert_eq!(g.height, 150.0);
    }

    #[test]
    fn test_serialize_roundtrip() {
        let doc = CanvaDocument::new("Test", 800.0, 600.0, vec![
            CanvaElement::rect("bg", 0.0, 0.0, 800.0, 600.0),
        ]);
        let json = serde_json::to_string(&doc).unwrap();
        let parsed: CanvaDocument = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "Test");
        assert_eq!(parsed.elements.len(), 1);
    }

    #[test]
    fn test_image_constructor() {
        let e = CanvaElement::image("photo", 50.0, 50.0, 300.0, 200.0);
        assert_eq!(e.element_type, "image");
    }

    #[test]
    fn test_ellipse_constructor() {
        let e = CanvaElement::ellipse("circle", 10.0, 10.0, 80.0, 80.0);
        assert_eq!(e.element_type, "ellipse");
    }
}
