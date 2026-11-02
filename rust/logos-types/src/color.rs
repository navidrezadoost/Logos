//! RGBA color and gradient definitions.
//!
//! Faithful Rust port of `common/src/app/common/types/color.cljc`.
//!
//! # Color encoding
//! Colors in Logos are stored as a CSS hex string (`"#rrggbb"`) paired with an
//! `opacity` in `[0.0, 1.0]`.  The hex representation is used because it is
//! the format stored in the database and transmitted over the wire; Rust keeps
//! it as a heap-allocated `String` to avoid conversion overhead on the hot
//! path.  Use [`Color::to_rgba`] when raw `u8` channel values are needed.
//!
//! Gradients mirror the SVG linear/radial gradient model.

use uuid::Uuid;

/// An RGBA color: hex `"#rrggbb"` (6 digits, lowercase) + alpha in `[0, 1]`.
///
/// Clojure record: `{:color "#rrggbb" :opacity 1.0}` (inside fills/strokes).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, rename_all = "camelCase"))]
pub struct Color {
    /// 6-digit lowercase CSS hex, e.g. `"#aabbcc"`. Always starts with `#`.
    pub color: String,
    /// Alpha channel in `[0.0, 1.0]`.  Defaults to `1.0` (fully opaque).
    #[cfg_attr(feature = "serde", serde(default = "default_opacity"))]
    pub opacity: f64,
    /// Optional library reference: UUID of the color in the shared library.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub ref_id: Option<Uuid>,
    /// Optional library reference: file ID.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub ref_file: Option<Uuid>,
    /// Human-readable name (used in the color library panel).
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub name: Option<String>,
    /// Design-token binding, if any.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub path: Option<String>,
    // UUIDs become `string` in TypeScript via the ts-rs `uuid-impl` feature.
}

#[allow(dead_code)]
fn default_opacity() -> f64 { 1.0 }

impl Color {
    /// Construct from a hex string + opacity.
    ///
    /// `hex` must be exactly `"#rrggbb"` (6 digits, `#` prefix).
    /// **No validation is performed**; call [`Color::is_valid_hex`] first if
    /// coming from untrusted input.
    pub fn new(hex: impl Into<String>, opacity: f64) -> Self {
        Color {
            color: hex.into(),
            opacity: opacity.clamp(0.0, 1.0),
            ref_id: None,
            ref_file: None,
            name: None,
            path: None,
        }
    }

    /// `"#000000"` with `opacity = 1.0`.
    pub fn black() -> Self { Color::new("#000000", 1.0) }

    /// `"#ffffff"` with `opacity = 1.0`.
    pub fn white() -> Self { Color::new("#ffffff", 1.0) }

    /// A fully transparent color.
    pub fn transparent() -> Self { Color::new("#000000", 0.0) }

    /// Decode the hex string into `(r, g, b)` bytes.
    /// Returns `None` if `self.color` is not a valid `#rrggbb` string.
    pub fn to_rgb(&self) -> Option<(u8, u8, u8)> {
        let s = self.color.trim_start_matches('#');
        if s.len() != 6 { return None; }
        let r = u8::from_str_radix(&s[0..2], 16).ok()?;
        let g = u8::from_str_radix(&s[2..4], 16).ok()?;
        let b = u8::from_str_radix(&s[4..6], 16).ok()?;
        Some((r, g, b))
    }

    /// Decode into `(r, g, b, a)` where `a` is `self.opacity * 255` rounded.
    pub fn to_rgba(&self) -> Option<(u8, u8, u8, u8)> {
        let (r, g, b) = self.to_rgb()?;
        let a = (self.opacity * 255.0).round() as u8;
        Some((r, g, b, a))
    }

    /// `true` if `self.color` is a valid `#rrggbb` hex string.
    pub fn is_valid_hex(&self) -> bool {
        self.to_rgb().is_some()
    }

    /// Construct from `(r, g, b)` bytes + opacity.
    pub fn from_rgb(r: u8, g: u8, b: u8, opacity: f64) -> Self {
        Color::new(format!("#{:02x}{:02x}{:02x}", r, g, b), opacity)
    }
}

// ─────────────────────────────────────────────────────────────────
// Gradient
// ─────────────────────────────────────────────────────────────────

/// Gradient variant.
/// Clojure: `:linear` / `:radial` inside `{:type :gradient :gradient {…}}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, rename_all = "lowercase"))]
pub enum GradientType {
    Linear,
    Radial,
}

/// A single color stop in a gradient.
/// Clojure: `{:color "#rrggbb" :opacity 1.0 :offset 0.5}`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, rename_all = "camelCase"))]
pub struct GradientStop {
    pub color: String,
    pub opacity: f64,
    /// Position along the gradient axis in `[0.0, 1.0]`.
    pub offset: f64,
}

/// A linear or radial gradient.
///
/// Clojure: `{:type :linear/:radial :start-x … :stops […]}`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, rename_all = "camelCase"))]
pub struct Gradient {
    #[cfg_attr(feature = "serde", serde(rename = "type"))]
    pub gradient_type: GradientType,
    /// Start point X in shape-local coordinates `[0, 1]`.
    pub start_x: f64,
    /// Start point Y in shape-local coordinates `[0, 1]`.
    pub start_y: f64,
    /// End / outer point X.
    pub end_x: f64,
    /// End / outer point Y.
    pub end_y: f64,
    /// Radial gradient width (ignored for linear).
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub width: Option<f64>,
    /// Color stops.
    pub stops: Vec<GradientStop>,
    /// Overall opacity of the gradient.
    #[cfg_attr(feature = "serde", serde(default = "default_opacity"))]
    pub opacity: f64,
}

// ─────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_to_rgb() {
        let c = Color::new("#ff8040", 1.0);
        assert_eq!(c.to_rgb(), Some((0xff, 0x80, 0x40)));
    }

    #[test]
    fn color_to_rgba_with_half_opacity() {
        let c = Color::new("#ffffff", 0.5);
        let (_, _, _, a) = c.to_rgba().unwrap();
        assert_eq!(a, 128);
    }

    #[test]
    fn color_from_rgb_roundtrip() {
        let c = Color::from_rgb(0x11, 0x22, 0x33, 1.0);
        assert_eq!(c.color, "#112233");
        assert_eq!(c.to_rgb(), Some((0x11, 0x22, 0x33)));
    }

    #[test]
    fn invalid_hex_returns_none() {
        let c = Color::new("not-hex", 1.0);
        assert!(c.to_rgb().is_none());
        assert!(!c.is_valid_hex());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn color_json_roundtrip() {
        let c = Color::new("#aabbcc", 0.8);
        let json = serde_json::to_string(&c).unwrap();
        let back: Color = serde_json::from_str(&json).unwrap();
        assert_eq!(back.color, "#aabbcc");
        assert!((back.opacity - 0.8).abs() < 1e-9);
    }
}
