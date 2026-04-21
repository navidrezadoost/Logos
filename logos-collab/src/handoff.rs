// logos-collab/src/handoff.rs
//
//! # Developer Handoff — Layer Inspector
//!
//! Extracts CSS-style property values from a logical layer description so that
//! Developer-role users can inspect dimensions, fills, typography, shadows,
//! and Auto Layout, and copy individual snippets.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Colour ────────────────────────────────────────────────────────────────────

/// RGBA colour (0.0–1.0 channels).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub r: f32, pub g: f32, pub b: f32, pub a: f32,
}

impl Color {
    pub fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self { Self { r, g, b, a } }
    /// CSS `rgba(…)` string.
    pub fn to_css(&self) -> String {
        let r = (self.r * 255.0).round() as u8;
        let g = (self.g * 255.0).round() as u8;
        let b = (self.b * 255.0).round() as u8;
        if (self.a - 1.0).abs() < 0.001 {
            format!("#{r:02X}{g:02X}{b:02X}")
        } else {
            format!("rgba({r},{g},{b},{:.3})", self.a)
        }
    }
}

// ── Shadow ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shadow {
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur: f32,
    pub spread: f32,
    pub color: Color,
    pub inset: bool,
}

impl Shadow {
    pub fn to_css(&self) -> String {
        let inset = if self.inset { "inset " } else { "" };
        format!(
            "{}{}px {}px {}px {}px {}",
            inset, self.offset_x, self.offset_y, self.blur, self.spread,
            self.color.to_css()
        )
    }
}

// ── Typography ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Typography {
    pub font_family: String,
    pub font_size: f32,
    pub font_weight: u16,
    pub line_height: Option<f32>,   // px; None = "normal"
    pub letter_spacing: f32,        // px
    pub text_transform: Option<String>,
}

// ── Auto-layout (flexbox) ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LayoutDirection { Row, Column }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutoLayout {
    pub direction: LayoutDirection,
    pub gap: f32,
    pub padding_top: f32,
    pub padding_right: f32,
    pub padding_bottom: f32,
    pub padding_left: f32,
    pub align_items: String,
    pub justify_content: String,
    pub wrap: bool,
}

// ── Layer inspection data ─────────────────────────────────────────────────────

/// All inspectable properties of a layer — the source-of-truth for the
/// Developer Handoff inspector panel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayerInspection {
    pub layer_id:   Uuid,
    pub layer_name: String,
    // Box model
    pub x: f32, pub y: f32,
    pub width: f32, pub height: f32,
    pub rotation_deg: f32,
    pub opacity: f32,
    // Borders / corners
    pub border_radius:     [f32; 4],   // TL, TR, BR, BL
    pub border_width:      f32,
    pub border_color:      Option<Color>,
    // Fill
    pub fill_color:        Option<Color>,
    // Shadows
    pub shadows:           Vec<Shadow>,
    // Typography (present only for text layers)
    pub typography:        Option<Typography>,
    // Auto-layout (present only for frame layers with auto-layout)
    pub auto_layout:       Option<AutoLayout>,
}

impl LayerInspection {
    /// Generate a CSS block for this layer.
    pub fn to_css(&self) -> String {
        let mut lines: Vec<String> = vec![
            format!("/* {} */", self.layer_name),
            format!("position: absolute;"),
            format!("left: {}px;", self.x),
            format!("top: {}px;", self.y),
            format!("width: {}px;", self.width),
            format!("height: {}px;", self.height),
        ];
        if (self.opacity - 1.0).abs() > 0.001 {
            lines.push(format!("opacity: {:.3};", self.opacity));
        }
        if self.rotation_deg != 0.0 {
            lines.push(format!("transform: rotate({}deg);", self.rotation_deg));
        }
        let radii = &self.border_radius;
        if radii.iter().all(|&r| (r - radii[0]).abs() < 0.001) {
            if radii[0] != 0.0 {
                lines.push(format!("border-radius: {}px;", radii[0]));
            }
        } else {
            lines.push(format!("border-radius: {}px {}px {}px {}px;",
                radii[0], radii[1], radii[2], radii[3]));
        }
        if let Some(ref fill) = self.fill_color {
            lines.push(format!("background-color: {};", fill.to_css()));
        }
        if let Some(ref bc) = self.border_color {
            lines.push(format!("border: {}px solid {};", self.border_width, bc.to_css()));
        }
        if !self.shadows.is_empty() {
            let s: Vec<_> = self.shadows.iter().map(|s| s.to_css()).collect();
            lines.push(format!("box-shadow: {};", s.join(", ")));
        }
        if let Some(ref ty) = self.typography {
            lines.push(format!("font-family: '{}';", ty.font_family));
            lines.push(format!("font-size: {}px;", ty.font_size));
            lines.push(format!("font-weight: {};", ty.font_weight));
            if let Some(lh) = ty.line_height {
                lines.push(format!("line-height: {}px;", lh));
            }
            if ty.letter_spacing != 0.0 {
                lines.push(format!("letter-spacing: {}px;", ty.letter_spacing));
            }
        }
        if let Some(ref al) = self.auto_layout {
            lines.push("display: flex;".into());
            lines.push(format!("flex-direction: {};",
                match al.direction { LayoutDirection::Row => "row", LayoutDirection::Column => "column" }));
            lines.push(format!("gap: {}px;", al.gap));
            lines.push(format!("padding: {}px {}px {}px {}px;",
                al.padding_top, al.padding_right, al.padding_bottom, al.padding_left));
            lines.push(format!("align-items: {};",     al.align_items));
            lines.push(format!("justify-content: {};", al.justify_content));
            if al.wrap { lines.push("flex-wrap: wrap;".into()); }
        }
        lines.join("\n")
    }

    /// Copy-snippet: single property value as a CSS string (e.g. for a copy button).
    pub fn snippet(&self, property: &str) -> Option<String> {
        match property {
            "width"        => Some(format!("{}px", self.width)),
            "height"       => Some(format!("{}px", self.height)),
            "x" | "left"   => Some(format!("{}px", self.x)),
            "y" | "top"    => Some(format!("{}px", self.y)),
            "opacity"      => Some(format!("{:.3}", self.opacity)),
            "background-color" | "fill" => self.fill_color.as_ref().map(|c| c.to_css()),
            "border-radius" => {
                let r = &self.border_radius;
                if r.iter().all(|&v| (v - r[0]).abs() < 0.001) {
                    Some(format!("{}px", r[0]))
                } else {
                    Some(format!("{}px {}px {}px {}px", r[0], r[1], r[2], r[3]))
                }
            }
            "box-shadow"   => {
                if self.shadows.is_empty() { return None; }
                Some(self.shadows.iter().map(|s| s.to_css()).collect::<Vec<_>>().join(", "))
            }
            "font-size"    => self.typography.as_ref().map(|t| format!("{}px", t.font_size)),
            "font-family"  => self.typography.as_ref().map(|t| format!("'{}'", t.font_family)),
            "font-weight"  => self.typography.as_ref().map(|t| t.font_weight.to_string()),
            "line-height"  => self.typography.as_ref().and_then(|t| t.line_height.map(|lh| format!("{}px", lh))),
            "letter-spacing" => self.typography.as_ref().map(|t| format!("{}px", t.letter_spacing)),
            _ => None,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn simple_layer() -> LayerInspection {
        LayerInspection {
            layer_id: Uuid::new_v4(),
            layer_name: "Button".into(),
            x: 10.0, y: 20.0,
            width: 120.0, height: 40.0,
            rotation_deg: 0.0,
            opacity: 1.0,
            border_radius: [8.0; 4],
            border_width: 0.0,
            border_color: None,
            fill_color: Some(Color::rgba(0.2, 0.4, 1.0, 1.0)),
            shadows: vec![],
            typography: None,
            auto_layout: None,
        }
    }

    // H-01: Color::to_css hex when fully opaque.
    #[test]
    fn h_01_color_css_hex() {
        let c = Color::rgba(1.0, 0.0, 0.0, 1.0);
        assert_eq!(c.to_css(), "#FF0000");
    }

    // H-02: Color::to_css rgba when semi-transparent.
    #[test]
    fn h_02_color_css_rgba() {
        let c = Color::rgba(0.0, 0.0, 0.0, 0.5);
        assert!(c.to_css().starts_with("rgba("));
    }

    // H-03: Shadow::to_css normal shadow.
    #[test]
    fn h_03_shadow_css() {
        let s = Shadow { offset_x: 2.0, offset_y: 4.0, blur: 6.0, spread: 0.0,
            color: Color::rgba(0.0, 0.0, 0.0, 0.3), inset: false };
        let css = s.to_css();
        assert!(css.contains("2px"));
        assert!(css.contains("4px"));
        assert!(!css.contains("inset"));
    }

    // H-04: Shadow::to_css inset shadow.
    #[test]
    fn h_04_shadow_css_inset() {
        let s = Shadow { offset_x: 0.0, offset_y: 1.0, blur: 3.0, spread: 0.0,
            color: Color::rgba(0.0, 0.0, 0.0, 1.0), inset: true };
        assert!(s.to_css().starts_with("inset "));
    }

    // H-05: to_css contains position and size.
    #[test]
    fn h_05_css_contains_position_size() {
        let css = simple_layer().to_css();
        assert!(css.contains("left: 10px"));
        assert!(css.contains("top: 20px"));
        assert!(css.contains("width: 120px"));
        assert!(css.contains("height: 40px"));
    }

    // H-06: to_css contains fill color.
    #[test]
    fn h_06_css_contains_fill() {
        let css = simple_layer().to_css();
        assert!(css.contains("background-color:"));
    }

    // H-07: to_css contains uniform border-radius.
    #[test]
    fn h_07_css_uniform_radius() {
        let css = simple_layer().to_css();
        assert!(css.contains("border-radius: 8px;"));
    }

    // H-08: to_css per-corner radius.
    #[test]
    fn h_08_css_per_corner_radius() {
        let mut l = simple_layer();
        l.border_radius = [4.0, 8.0, 12.0, 0.0];
        let css = l.to_css();
        assert!(css.contains("4px 8px 12px 0px"));
    }

    // H-09: to_css includes opacity when not 1.0.
    #[test]
    fn h_09_css_opacity() {
        let mut l = simple_layer();
        l.opacity = 0.5;
        assert!(l.to_css().contains("opacity:"));
    }

    // H-10: to_css skips opacity when 1.0.
    #[test]
    fn h_10_css_no_opacity_when_full() {
        assert!(!simple_layer().to_css().contains("opacity:"));
    }

    // H-11: to_css includes box-shadow.
    #[test]
    fn h_11_css_box_shadow() {
        let mut l = simple_layer();
        l.shadows = vec![Shadow { offset_x: 0.0, offset_y: 2.0, blur: 4.0, spread: 0.0,
            color: Color::rgba(0.0,0.0,0.0,0.2), inset: false }];
        assert!(l.to_css().contains("box-shadow:"));
    }

    // H-12: to_css includes typography.
    #[test]
    fn h_12_css_typography() {
        let mut l = simple_layer();
        l.typography = Some(Typography {
            font_family: "Inter".into(), font_size: 16.0, font_weight: 600,
            line_height: Some(24.0), letter_spacing: 0.5, text_transform: None,
        });
        let css = l.to_css();
        assert!(css.contains("font-family: 'Inter'"));
        assert!(css.contains("font-size: 16px"));
        assert!(css.contains("font-weight: 600"));
        assert!(css.contains("line-height: 24px"));
        assert!(css.contains("letter-spacing: 0.5px"));
    }

    // H-13: to_css includes auto-layout.
    #[test]
    fn h_13_css_auto_layout() {
        let mut l = simple_layer();
        l.auto_layout = Some(AutoLayout {
            direction: LayoutDirection::Row, gap: 8.0,
            padding_top: 4.0, padding_right: 8.0, padding_bottom: 4.0, padding_left: 8.0,
            align_items: "center".into(), justify_content: "space-between".into(), wrap: false,
        });
        let css = l.to_css();
        assert!(css.contains("display: flex"));
        assert!(css.contains("flex-direction: row"));
        assert!(css.contains("gap: 8px"));
    }

    // H-14: snippet("width") returns px string.
    #[test]
    fn h_14_snippet_width() {
        assert_eq!(simple_layer().snippet("width"), Some("120px".into()));
    }

    // H-15: snippet("height") returns px string.
    #[test]
    fn h_15_snippet_height() {
        assert_eq!(simple_layer().snippet("height"), Some("40px".into()));
    }

    // H-16: snippet("opacity") returns 1 decimal.
    #[test]
    fn h_16_snippet_opacity() {
        assert_eq!(simple_layer().snippet("opacity"), Some("1.000".into()));
    }

    // H-17: snippet("background-color") returns CSS color.
    #[test]
    fn h_17_snippet_fill() {
        assert!(simple_layer().snippet("background-color").is_some());
    }

    // H-18: snippet("box-shadow") returns None when no shadows.
    #[test]
    fn h_18_snippet_no_shadow() {
        assert!(simple_layer().snippet("box-shadow").is_none());
    }

    // H-19: snippet("font-size") returns None for non-text layer.
    #[test]
    fn h_19_snippet_font_size_no_text() {
        assert!(simple_layer().snippet("font-size").is_none());
    }

    // H-20: snippet("font-size") returns px for text layer.
    #[test]
    fn h_20_snippet_font_size_text() {
        let mut l = simple_layer();
        l.typography = Some(Typography {
            font_family: "Inter".into(), font_size: 14.0, font_weight: 400,
            line_height: None, letter_spacing: 0.0, text_transform: None,
        });
        assert_eq!(l.snippet("font-size"), Some("14px".into()));
    }

    // H-21: snippet unknown property returns None.
    #[test]
    fn h_21_snippet_unknown() {
        assert!(simple_layer().snippet("color").is_none());
    }

    // H-22: to_css includes rotation when non-zero.
    #[test]
    fn h_22_css_rotation() {
        let mut l = simple_layer();
        l.rotation_deg = 45.0;
        assert!(l.to_css().contains("rotate(45deg)"));
    }

    // H-23: to_css omits rotation when zero.
    #[test]
    fn h_23_css_no_rotation_zero() {
        assert!(!simple_layer().to_css().contains("transform:"));
    }

    // H-24: to_css includes border when border_color is set.
    #[test]
    fn h_24_css_border() {
        let mut l = simple_layer();
        l.border_width = 1.0;
        l.border_color = Some(Color::rgba(0.0, 0.0, 0.0, 1.0));
        assert!(l.to_css().contains("border:"));
    }

    // H-25: Color black hex is #000000.
    #[test]
    fn h_25_black_hex() {
        assert_eq!(Color::rgba(0.0, 0.0, 0.0, 1.0).to_css(), "#000000");
    }
}
