//! Drop / inner shadow definitions.
//!
//! Clojure source: `{:style :drop-shadow/:inner-shadow
//!                   :color {:color "#000000" :opacity 0.3}
//!                   :offset-x 4 :offset-y 4
//!                   :blur 4 :spread 0 :hidden false}`.

use crate::color::Color;

/// Shadow variant.
/// Clojure: `:drop-shadow` | `:inner-shadow`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, rename_all = "kebab-case"))]
pub enum ShadowStyle {
    #[default]
    DropShadow,
    InnerShadow,
}

/// A single shadow applied to a shape.
/// Multiple shadows are stored as `Vec<Shadow>` on the shape.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, rename_all = "camelCase"))]
pub struct Shadow {
    #[cfg_attr(feature = "serde", serde(default))]
    pub style: ShadowStyle,
    pub color: Color,
    /// Horizontal offset in pixels.
    #[cfg_attr(feature = "serde", serde(default))]
    pub offset_x: f64,
    /// Vertical offset in pixels.
    #[cfg_attr(feature = "serde", serde(default))]
    pub offset_y: f64,
    /// Gaussian blur radius in pixels.
    #[cfg_attr(feature = "serde", serde(default))]
    pub blur: f64,
    /// Expansion (positive) or contraction (negative) in pixels.
    #[cfg_attr(feature = "serde", serde(default))]
    pub spread: f64,
    /// When `true` the shadow is defined but not rendered.
    #[cfg_attr(feature = "serde", serde(default))]
    pub hidden: bool,
}

impl Shadow {
    /// Default drop shadow: semi-transparent black, offset (4,4), blur 4.
    pub fn default_drop() -> Self {
        Shadow {
            style: ShadowStyle::DropShadow,
            color: Color::new("#000000", 0.3),
            offset_x: 4.0,
            offset_y: 4.0,
            blur: 4.0,
            spread: 0.0,
            hidden: false,
        }
    }
}

// ─────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_drop_shadow() {
        let s = Shadow::default_drop();
        assert_eq!(s.style, ShadowStyle::DropShadow);
        assert_eq!(s.offset_x, 4.0);
        assert!(!s.hidden);
    }
}
