//! Fill types (solid, gradient, image).
//!
//! Clojure source: `common/src/app/common/types/shape/interactions.cljc`
//! and the fill sub-map stored on every shape:
//! `{:fill-color "#rrggbb" :fill-opacity 1.0}` for solids,
//! `{:fill-image {:id … :width … :height …}}` for images,
//! `{:fill-color-gradient {…}}` for gradients.

use uuid::Uuid;
use crate::color::{Color, Gradient};

/// How to fill a shape.
///
/// Logos supports up to ∞ fills per shape stored in a `Vec<Fill>`.
/// Clojure tag: `:fill-type` / inferred from which keys are present.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "fill-type", rename_all = "kebab-case"))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, tag = "fillType", rename_all = "lowercase"))]
pub enum Fill {
    /// Uniform RGBA color fill.
    Solid {
        /// Hex `"#rrggbb"`.
        #[cfg_attr(feature = "serde", serde(rename = "fill-color"))]
        color: String,
        /// Opacity in `[0.0, 1.0]`.
        #[cfg_attr(feature = "serde", serde(rename = "fill-opacity", default = "default_opacity"))]
        opacity: f64,
    },
    /// Gradient fill.
    Gradient {
        #[cfg_attr(feature = "serde", serde(rename = "fill-color-gradient"))]
        gradient: Gradient,
        #[cfg_attr(feature = "serde", serde(rename = "fill-opacity", default = "default_opacity"))]
        opacity: f64,
    },
    /// Raster image fill.
    Image {
        #[cfg_attr(feature = "serde", serde(rename = "fill-image"))]
        image: FillImage,
    },
}

#[allow(dead_code)]
fn default_opacity() -> f64 { 1.0 }

impl Fill {
    /// Convenience constructor: opaque solid color from a `Color`.
    pub fn solid_from_color(c: &Color) -> Self {
        Fill::Solid { color: c.color.clone(), opacity: c.opacity }
    }

    /// Convenience: transparent/no fill sentinel.
    pub fn none() -> Self {
        Fill::Solid { color: "#000000".into(), opacity: 0.0 }
    }
}

/// Raster image embedded in a fill.
/// Clojure: `{:id uuid :name "…" :width px :height px :mtype "image/png" …}`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, rename_all = "camelCase"))]
pub struct FillImage {
    pub id: Uuid,
    /// Width in pixels of the original raster.
    pub width: u32,
    /// Height in pixels of the original raster.
    pub height: u32,
    /// Optional MIME type, e.g. `"image/png"`.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub mtype: Option<String>,
    /// Human-readable name / filename.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub name: Option<String>,
    /// If present, the image is kept-aspect proportional inside the shape.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub keep_aspect_ratio: Option<bool>,
}

// ─────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solid_fill_none_is_transparent() {
        match Fill::none() {
            Fill::Solid { opacity, .. } => assert_eq!(opacity, 0.0),
            _ => panic!("expected solid"),
        }
    }

    #[test]
    fn solid_from_color() {
        let c = Color::new("#ff0000", 0.9);
        match Fill::solid_from_color(&c) {
            Fill::Solid { color, opacity } => {
                assert_eq!(color, "#ff0000");
                assert!((opacity - 0.9).abs() < 1e-9);
            }
            _ => panic!("expected solid"),
        }
    }
}
