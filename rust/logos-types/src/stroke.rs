//! Stroke definitions.
//!
//! Clojure source: stroke sub-map on shape records:
//! `{:stroke-color "#rrggbb" :stroke-opacity 1.0 :stroke-width 1.0
//!   :stroke-style :solid :stroke-position :center :stroke-cap-start :none
//!   :stroke-cap-end :none}`.

/// Stroke line-style.
/// Clojure: `:solid`, `:dashed`, `:dotted`, `:mixed`, `:svg`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, rename_all = "lowercase"))]
pub enum StrokeStyle {
    #[default]
    Solid,
    Dashed,
    Dotted,
    Mixed,
    /// Raw SVG `stroke-dasharray` passthrough.
    Svg,
    None,
}

/// Stroke endpoint cap.
/// Clojure: `:none`, `:line`, `:triangle`, `:square`, `:circle`, `:diamond`, `:round`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, rename_all = "lowercase"))]
pub enum StrokeCap {
    #[default]
    None,
    Line,
    Triangle,
    Square,
    Circle,
    Diamond,
    Round,
}

/// Whether the stroke is drawn inside, outside, or centered on the path.
/// Clojure: `:inner`, `:outer`, `:center`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, rename_all = "lowercase"))]
pub enum StrokePosition {
    #[default]
    Center,
    Inner,
    Outer,
}

/// A single stroke applied to a shape.
///
/// Logos supports multiple strokes per shape (`Vec<Stroke>`).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, rename_all = "camelCase"))]
pub struct Stroke {
    /// Hex `"#rrggbb"`.
    pub stroke_color: String,
    /// Alpha in `[0.0, 1.0]`.
    #[cfg_attr(feature = "serde", serde(default = "default_opacity"))]
    pub stroke_opacity: f64,
    /// Width in pixels.
    #[cfg_attr(feature = "serde", serde(default = "default_width"))]
    pub stroke_width: f64,
    pub stroke_style: StrokeStyle,
    pub stroke_position: StrokePosition,
    #[cfg_attr(feature = "serde", serde(default))]
    pub stroke_cap_start: StrokeCap,
    #[cfg_attr(feature = "serde", serde(default))]
    pub stroke_cap_end: StrokeCap,
}

#[allow(dead_code)]
fn default_opacity() -> f64 { 1.0 }
#[allow(dead_code)]
fn default_width() -> f64 { 1.0 }

impl Stroke {
    /// Default 1 px solid black center stroke.
    pub fn default_solid() -> Self {
        Stroke {
            stroke_color: "#000000".into(),
            stroke_opacity: 1.0,
            stroke_width: 1.0,
            stroke_style: StrokeStyle::Solid,
            stroke_position: StrokePosition::Center,
            stroke_cap_start: StrokeCap::None,
            stroke_cap_end: StrokeCap::None,
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
    fn default_stroke_caps_are_none() {
        let s = Stroke::default_solid();
        assert_eq!(s.stroke_cap_start, StrokeCap::None);
        assert_eq!(s.stroke_cap_end, StrokeCap::None);
    }

    #[test]
    fn stroke_style_default() {
        let style = StrokeStyle::default();
        assert_eq!(style, StrokeStyle::Solid);
    }
}
