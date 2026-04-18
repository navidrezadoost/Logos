//! Sketch layer model types.

use serde::{Deserialize, Serialize};

/// A Sketch layer from the JSON representation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SketchLayer {
    /// Unique identifier (UUID string).
    #[serde(rename = "do_objectID")]
    pub id: String,
    /// Layer name.
    pub name: String,
    /// Layer class: rectangle, oval, text, group, artboard, etc.
    #[serde(rename = "_class")]
    pub class: String,
    /// Whether the layer is visible.
    #[serde(default = "default_true")]
    pub isVisible: bool,
    /// Position and size.
    pub frame: SketchFrame,
    /// Child layers (for groups, artboards).
    #[serde(default)]
    pub layers: Vec<SketchLayer>,
    /// Text content (for text layers).
    #[serde(rename = "attributedString", default)]
    pub attributed_string: Option<SketchAttributedString>,
    /// Rotation in degrees.
    #[serde(default)]
    pub rotation: f64,
    /// Opacity (0.0–1.0).
    #[serde(default = "default_opacity")]
    pub opacity: f64,
}

impl Default for SketchLayer {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            class: "rectangle".to_string(),
            isVisible: true,
            frame: SketchFrame::default(),
            layers: Vec::new(),
            attributed_string: None,
            rotation: 0.0,
            opacity: 1.0,
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_opacity() -> f64 {
    1.0
}

/// Frame (position + size) in Sketch JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SketchFrame {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    #[serde(rename = "_class", default)]
    pub class: String,
}

impl Default for SketchFrame {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            class: "rect".to_string(),
        }
    }
}

/// Attributed string for text layers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SketchAttributedString {
    #[serde(rename = "_class")]
    pub class: String,
    pub string: String,
}

/// A Sketch page from `pages/<uuid>.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SketchPage {
    #[serde(rename = "do_objectID")]
    pub id: String,
    pub name: String,
    #[serde(rename = "_class")]
    pub class: String,
    #[serde(default)]
    pub layers: Vec<SketchLayer>,
    #[serde(default)]
    pub frame: SketchFrame,
}

/// Top-level Sketch document extracted from `document.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SketchDocument {
    #[serde(rename = "do_objectID", default)]
    pub id: String,
    #[serde(rename = "_class", default)]
    pub class: String,
    /// Pages (populated from `pages/` directory).
    #[serde(default)]
    pub pages: Vec<SketchPage>,
}

impl SketchLayer {
    /// Create a rectangle layer.
    pub fn rect(id: &str, name: &str, x: f64, y: f64, w: f64, h: f64) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            class: "rectangle".to_string(),
            isVisible: true,
            frame: SketchFrame {
                x, y, width: w, height: h,
                class: "rect".to_string(),
            },
            layers: Vec::new(),
            attributed_string: None,
            rotation: 0.0,
            opacity: 1.0,
        }
    }

    /// Create an oval layer.
    pub fn oval(id: &str, name: &str, x: f64, y: f64, w: f64, h: f64) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            class: "oval".to_string(),
            isVisible: true,
            frame: SketchFrame {
                x, y, width: w, height: h,
                class: "rect".to_string(),
            },
            layers: Vec::new(),
            attributed_string: None,
            rotation: 0.0,
            opacity: 1.0,
        }
    }

    /// Create a text layer.
    pub fn text(id: &str, name: &str, x: f64, y: f64, w: f64, h: f64, content: &str) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            class: "text".to_string(),
            isVisible: true,
            frame: SketchFrame {
                x, y, width: w, height: h,
                class: "rect".to_string(),
            },
            layers: Vec::new(),
            attributed_string: Some(SketchAttributedString {
                class: "attributedString".to_string(),
                string: content.to_string(),
            }),
            rotation: 0.0,
            opacity: 1.0,
        }
    }

    /// Create a group layer.
    pub fn group(id: &str, name: &str, children: Vec<SketchLayer>) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            class: "group".to_string(),
            isVisible: true,
            frame: SketchFrame::default(),
            layers: children,
            attributed_string: None,
            rotation: 0.0,
            opacity: 1.0,
        }
    }

    /// Create an artboard layer.
    pub fn artboard(
        id: &str,
        name: &str,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        children: Vec<SketchLayer>,
    ) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            class: "artboard".to_string(),
            isVisible: true,
            frame: SketchFrame {
                x, y, width: w, height: h,
                class: "rect".to_string(),
            },
            layers: children,
            attributed_string: None,
            rotation: 0.0,
            opacity: 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_rect() {
        let r = SketchLayer::rect("1", "R", 10.0, 20.0, 100.0, 50.0);
        assert_eq!(r.class, "rectangle");
        assert_eq!(r.frame.width, 100.0);
    }

    #[test]
    fn test_create_oval() {
        let o = SketchLayer::oval("2", "O", 0.0, 0.0, 80.0, 80.0);
        assert_eq!(o.class, "oval");
    }

    #[test]
    fn test_create_text() {
        let t = SketchLayer::text("3", "T", 0.0, 0.0, 100.0, 20.0, "Hello");
        assert_eq!(t.attributed_string.as_ref().unwrap().string, "Hello");
    }

    #[test]
    fn test_create_group() {
        let g = SketchLayer::group("4", "G", vec![
            SketchLayer::rect("5", "R1", 0.0, 0.0, 50.0, 50.0),
        ]);
        assert_eq!(g.layers.len(), 1);
    }

    #[test]
    fn test_create_artboard() {
        let a = SketchLayer::artboard("6", "A", 0.0, 0.0, 375.0, 812.0, vec![]);
        assert_eq!(a.class, "artboard");
        assert_eq!(a.frame.width, 375.0);
    }

    #[test]
    fn test_serialize_deserialize_layer() {
        let r = SketchLayer::rect("1", "R", 10.0, 20.0, 100.0, 50.0);
        let json = serde_json::to_string(&r).unwrap();
        let r2: SketchLayer = serde_json::from_str(&json).unwrap();
        assert_eq!(r2.name, "R");
        assert_eq!(r2.frame.x, 10.0);
    }
}
