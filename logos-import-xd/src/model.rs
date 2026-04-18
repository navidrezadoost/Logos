//! Adobe XD document model types.

use serde::{Deserialize, Serialize};

/// A node in the XD document tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XdNode {
    /// Unique identifier.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Node type: "shape", "text", "group", "artboard".
    #[serde(rename = "type")]
    pub node_type: String,
    /// Shape sub-type: "rect", "ellipse", "path", "line" etc.
    #[serde(default)]
    pub shape_type: String,
    /// Transform / position.
    #[serde(default)]
    pub transform: XdTransform,
    /// Bounding box.
    #[serde(default)]
    pub bounds: XdBounds,
    /// Text content (for text nodes).
    #[serde(default)]
    pub text_content: String,
    /// Visibility.
    #[serde(default = "default_visible")]
    pub visible: bool,
    /// Opacity (0.0 - 1.0).
    #[serde(default = "default_opacity")]
    pub opacity: f64,
    /// Child nodes.
    #[serde(default)]
    pub children: Vec<XdNode>,
}

impl Default for XdNode {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            node_type: "shape".to_string(),
            shape_type: String::new(),
            transform: XdTransform::default(),
            bounds: XdBounds { x: 0.0, y: 0.0, width: 100.0, height: 100.0 },
            text_content: String::new(),
            visible: true,
            opacity: 1.0,
            children: Vec::new(),
        }
    }
}

fn default_visible() -> bool {
    true
}

fn default_opacity() -> f64 {
    1.0
}

/// Transform applied to an XD node.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct XdTransform {
    #[serde(default)]
    pub tx: f64,
    #[serde(default)]
    pub ty: f64,
    #[serde(default)]
    pub rotation: f64,
}

/// Bounding box of an XD node.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct XdBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Represents a parsed XD artboard.
#[derive(Debug, Clone)]
pub struct XdArtboard {
    pub name: String,
    pub width: f64,
    pub height: f64,
    pub children: Vec<XdNode>,
}

/// Represents a parsed XD document (collection of artboards).
#[derive(Debug, Clone)]
pub struct XdDocument {
    pub artboards: Vec<XdArtboard>,
}

// ── Constructors for testing ──

impl XdNode {
    pub fn rect(name: &str, x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            node_type: "shape".into(),
            shape_type: "rect".into(),
            transform: XdTransform::default(),
            bounds: XdBounds {
                x,
                y,
                width,
                height,
            },
            text_content: String::new(),
            visible: true,
            opacity: 1.0,
            children: vec![],
        }
    }

    pub fn ellipse(name: &str, x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            node_type: "shape".into(),
            shape_type: "ellipse".into(),
            transform: XdTransform::default(),
            bounds: XdBounds {
                x,
                y,
                width,
                height,
            },
            text_content: String::new(),
            visible: true,
            opacity: 1.0,
            children: vec![],
        }
    }

    pub fn text(name: &str, x: f64, y: f64, width: f64, height: f64, content: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            node_type: "text".into(),
            shape_type: String::new(),
            transform: XdTransform::default(),
            bounds: XdBounds {
                x,
                y,
                width,
                height,
            },
            text_content: content.into(),
            visible: true,
            opacity: 1.0,
            children: vec![],
        }
    }

    pub fn group(name: &str, children: Vec<XdNode>) -> Self {
        // Compute bounds from children
        let mut min_x = f64::MAX;
        let mut min_y = f64::MAX;
        let mut max_x = f64::MIN;
        let mut max_y = f64::MIN;
        for c in &children {
            min_x = min_x.min(c.bounds.x);
            min_y = min_y.min(c.bounds.y);
            max_x = max_x.max(c.bounds.x + c.bounds.width);
            max_y = max_y.max(c.bounds.y + c.bounds.height);
        }
        let (x, y, w, h) = if children.is_empty() {
            (0.0, 0.0, 0.0, 0.0)
        } else {
            (min_x, min_y, max_x - min_x, max_y - min_y)
        };

        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            node_type: "group".into(),
            shape_type: String::new(),
            transform: XdTransform::default(),
            bounds: XdBounds {
                x,
                y,
                width: w,
                height: h,
            },
            text_content: String::new(),
            visible: true,
            opacity: 1.0,
            children,
        }
    }

    pub fn line(name: &str, x1: f64, y1: f64, x2: f64, y2: f64) -> Self {
        let min_x = x1.min(x2);
        let min_y = y1.min(y2);
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            node_type: "shape".into(),
            shape_type: "line".into(),
            transform: XdTransform::default(),
            bounds: XdBounds {
                x: min_x,
                y: min_y,
                width: (x2 - x1).abs(),
                height: (y2 - y1).abs(),
            },
            text_content: String::new(),
            visible: true,
            opacity: 1.0,
            children: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rect_constructor() {
        let n = XdNode::rect("bg", 0.0, 0.0, 100.0, 50.0);
        assert_eq!(n.node_type, "shape");
        assert_eq!(n.shape_type, "rect");
        assert_eq!(n.bounds.width, 100.0);
    }

    #[test]
    fn test_text_constructor() {
        let n = XdNode::text("title", 10.0, 20.0, 200.0, 30.0, "Hello");
        assert_eq!(n.text_content, "Hello");
        assert_eq!(n.node_type, "text");
    }

    #[test]
    fn test_group_computes_bounds() {
        let g = XdNode::group("grp", vec![
            XdNode::rect("a", 0.0, 0.0, 50.0, 50.0),
            XdNode::rect("b", 100.0, 100.0, 50.0, 50.0),
        ]);
        assert_eq!(g.bounds.width, 150.0);
        assert_eq!(g.bounds.height, 150.0);
    }

    #[test]
    fn test_ellipse_constructor() {
        let n = XdNode::ellipse("circle", 10.0, 10.0, 80.0, 80.0);
        assert_eq!(n.shape_type, "ellipse");
        assert_eq!(n.bounds.width, 80.0);
    }

    #[test]
    fn test_serialize_roundtrip() {
        let n = XdNode::rect("test", 5.0, 10.0, 100.0, 50.0);
        let json = serde_json::to_string(&n).unwrap();
        let parsed: XdNode = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "test");
        assert_eq!(parsed.bounds.x, 5.0);
    }
}
