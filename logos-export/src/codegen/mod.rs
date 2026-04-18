//! Code generation framework — convert layer data into platform code.
//!
//! Each generator implements the [`CodeGenerator`] trait and transforms
//! `LayerStyleData` (geometry + style) into a formatted code string for
//! a target platform.
//!
//! Supported targets:
//! - **CSS** — standard web properties
//! - **SwiftUI** — Apple declarative UI
//! - **Compose** — Jetpack Compose (Android)

pub mod css;
pub mod swiftui;
pub mod compose;

use logos_core::style::{Color, LayerStyle};
use logos_core::Layer;
use serde::{Deserialize, Serialize};

// ═══════════════════════════════════════════════════════════════════
// Target enum
// ═══════════════════════════════════════════════════════════════════

/// Code generation target platform.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CodeGenTarget {
    Css,
    SwiftUI,
    Compose,
}

impl CodeGenTarget {
    /// Human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Css => "CSS",
            Self::SwiftUI => "SwiftUI",
            Self::Compose => "Compose",
        }
    }

    /// File extension typically used.
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Css => "css",
            Self::SwiftUI => "swift",
            Self::Compose => "kt",
        }
    }

    /// All available targets.
    pub fn all() -> &'static [CodeGenTarget] {
        &[Self::Css, Self::SwiftUI, Self::Compose]
    }
}

// ═══════════════════════════════════════════════════════════════════
// Style input
// ═══════════════════════════════════════════════════════════════════

/// Combined geometry + style data for code generation.
///
/// This struct gathers everything a code generator needs to produce
/// a complete code snippet for a single layer.
#[derive(Clone, Debug)]
pub struct LayerStyleData {
    /// Layer type name (e.g., "rect", "frame", "text").
    pub layer_type: String,
    /// Human-readable name / identifier.
    pub name: String,
    /// Position X.
    pub x: f32,
    /// Position Y.
    pub y: f32,
    /// Width.
    pub width: f32,
    /// Height.
    pub height: f32,
    /// Visual style.
    pub style: LayerStyle,
    /// Text content (only for text layers).
    pub text_content: Option<String>,
}

impl LayerStyleData {
    /// Build from an `ExportLayerData` + style.
    pub fn from_layer(layer: &Layer, x: f32, y: f32, w: f32, h: f32, style: LayerStyle) -> Self {
        let (layer_type, name, text) = match layer {
            Layer::Rect(r) => ("rect".to_string(), format!("rect_{}", &r.id.to_string()[..8]), None),
            Layer::Ellipse(e) => ("ellipse".to_string(), format!("ellipse_{}", &e.id.to_string()[..8]), None),
            Layer::Text(t) => ("text".to_string(), format!("text_{}", &t.id.to_string()[..8]), Some(t.content.clone())),
            Layer::Frame(f) => ("frame".to_string(), format!("frame_{}", &f.id.to_string()[..8]), None),
            Layer::Path(p) => ("path".to_string(), format!("path_{}", &p.id.to_string()[..8]), None),
            Layer::Artboard(a) => ("artboard".to_string(), a.name.clone(), None),
            Layer::Drawer(d) => ("drawer".to_string(), d.name.clone(), None),
            Layer::Section(s) => ("section".to_string(), s.name.clone(), None),
            Layer::Line(_) => ("line".to_string(), "line".to_string(), None),
            Layer::Polygon(_) => ("polygon".to_string(), "polygon".to_string(), None),
            Layer::Star(_) => ("star".to_string(), "star".to_string(), None),
            Layer::BooleanGroup(_) => ("boolean_group".to_string(), "boolean_group".to_string(), None),
            Layer::VectorNetwork(_) => ("vector_network".to_string(), "vector_network".to_string(), None),
        };

        Self {
            layer_type,
            name,
            x,
            y,
            width: w,
            height: h,
            style,
            text_content: text,
        }
    }

    /// Whether all corner radii are equal (uniform border-radius).
    pub fn has_uniform_radius(&self) -> bool {
        let r = self.style.corner_radii;
        r[0] == r[1] && r[1] == r[2] && r[2] == r[3]
    }

    /// Whether any corner has a non-zero radius.
    pub fn has_radius(&self) -> bool {
        self.style.corner_radii.iter().any(|&r| r > 0.0)
    }
}

// ═══════════════════════════════════════════════════════════════════
// CodeGenerator trait
// ═══════════════════════════════════════════════════════════════════

/// Trait for platform-specific code generation.
pub trait CodeGenerator {
    /// Target platform.
    fn target(&self) -> CodeGenTarget;

    /// Generate code for a single layer.
    fn generate(&self, data: &LayerStyleData) -> String;

    /// Generate code for multiple layers (default: concatenate).
    fn generate_all(&self, layers: &[LayerStyleData]) -> String {
        layers.iter().map(|d| self.generate(d)).collect::<Vec<_>>().join("\n\n")
    }
}

/// Create a code generator for the specified target.
pub fn generator_for(target: CodeGenTarget) -> Box<dyn CodeGenerator> {
    match target {
        CodeGenTarget::Css => Box::new(css::CssGenerator),
        CodeGenTarget::SwiftUI => Box::new(swiftui::SwiftUiGenerator),
        CodeGenTarget::Compose => Box::new(compose::ComposeGenerator),
    }
}

// ═══════════════════════════════════════════════════════════════════
// Shared helpers
// ═══════════════════════════════════════════════════════════════════

/// Format a Color as `rgba(r, g, b, a)` CSS-style.
pub(crate) fn color_to_rgba_string(c: &Color) -> String {
    let r = (c.r * 255.0).round() as u8;
    let g = (c.g * 255.0).round() as u8;
    let b = (c.b * 255.0).round() as u8;
    if (c.a - 1.0).abs() < 0.001 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("rgba({r}, {g}, {b}, {:.2})", c.a)
    }
}

/// Format a Color as hex string (no alpha).
#[allow(dead_code)]
pub(crate) fn color_to_hex(c: &Color) -> String {
    let r = (c.r * 255.0).round() as u8;
    let g = (c.g * 255.0).round() as u8;
    let b = (c.b * 255.0).round() as u8;
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// Format a float as a clean pixel value (trim trailing zeros).
pub(crate) fn px(v: f32) -> String {
    if v == 0.0 {
        "0".to_string()
    } else if v.fract() == 0.0 {
        format!("{}px", v as i32)
    } else {
        format!("{:.1}px", v)
    }
}

/// Format a float as a clean number (no unit).
pub(crate) fn num(v: f32) -> String {
    if v.fract() == 0.0 {
        format!("{}", v as i32)
    } else {
        format!("{:.1}", v)
    }
}

// ═══════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_codegen_target_labels() {
        assert_eq!(CodeGenTarget::Css.label(), "CSS");
        assert_eq!(CodeGenTarget::SwiftUI.label(), "SwiftUI");
        assert_eq!(CodeGenTarget::Compose.label(), "Compose");
    }

    #[test]
    fn test_codegen_target_extensions() {
        assert_eq!(CodeGenTarget::Css.extension(), "css");
        assert_eq!(CodeGenTarget::SwiftUI.extension(), "swift");
        assert_eq!(CodeGenTarget::Compose.extension(), "kt");
    }

    #[test]
    fn test_all_targets() {
        assert_eq!(CodeGenTarget::all().len(), 3);
    }

    #[test]
    fn test_color_to_rgba_opaque() {
        let c = Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 };
        assert_eq!(color_to_rgba_string(&c), "#ff0000");
    }

    #[test]
    fn test_color_to_rgba_translucent() {
        let c = Color { r: 0.0, g: 0.5, b: 1.0, a: 0.5 };
        let s = color_to_rgba_string(&c);
        assert!(s.starts_with("rgba("));
        assert!(s.contains("0.50"));
    }

    #[test]
    fn test_color_to_hex() {
        let c = Color { r: 0.0, g: 1.0, b: 0.0, a: 1.0 };
        assert_eq!(color_to_hex(&c), "#00ff00");
    }

    #[test]
    fn test_px_formatting() {
        assert_eq!(px(0.0), "0");
        assert_eq!(px(100.0), "100px");
        assert_eq!(px(10.5), "10.5px");
    }

    #[test]
    fn test_num_formatting() {
        assert_eq!(num(0.0), "0");
        assert_eq!(num(42.0), "42");
        assert_eq!(num(3.5), "3.5");
    }

    #[test]
    fn test_uniform_radius() {
        let data = LayerStyleData {
            layer_type: "rect".into(),
            name: "test".into(),
            x: 0.0, y: 0.0, width: 100.0, height: 50.0,
            style: LayerStyle::default().with_corner_radii([8.0; 4]),
            text_content: None,
        };
        assert!(data.has_uniform_radius());
        assert!(data.has_radius());
    }

    #[test]
    fn test_non_uniform_radius() {
        let data = LayerStyleData {
            layer_type: "rect".into(),
            name: "test".into(),
            x: 0.0, y: 0.0, width: 100.0, height: 50.0,
            style: LayerStyle::default().with_corner_radii([8.0, 0.0, 8.0, 0.0]),
            text_content: None,
        };
        assert!(!data.has_uniform_radius());
        assert!(data.has_radius());
    }

    #[test]
    fn test_generator_for_creates_correct_target() {
        let gen = generator_for(CodeGenTarget::Css);
        assert_eq!(gen.target(), CodeGenTarget::Css);

        let gen = generator_for(CodeGenTarget::SwiftUI);
        assert_eq!(gen.target(), CodeGenTarget::SwiftUI);

        let gen = generator_for(CodeGenTarget::Compose);
        assert_eq!(gen.target(), CodeGenTarget::Compose);
    }
}
