//! Blur effect definitions.
//!
//! Clojure source: `{:type :layer-blur/:background-blur :value 4.0 :hidden false}`.

/// Blur variant.
/// Clojure: `:layer-blur` | `:background-blur`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, rename_all = "kebab-case"))]
pub enum BlurType {
    #[default]
    LayerBlur,
    BackgroundBlur,
}

/// A Gaussian blur effect applied to a shape.
/// Multiple blurs are stored as `Vec<Blur>`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, rename_all = "camelCase"))]
pub struct Blur {
    #[cfg_attr(feature = "serde", serde(rename = "type", default))]
    pub blur_type: BlurType,
    /// Blur radius in pixels.
    pub value: f64,
    /// When `true` the blur is defined but not rendered.
    #[cfg_attr(feature = "serde", serde(default))]
    pub hidden: bool,
}

impl Blur {
    /// Default 4 px layer blur, visible.
    pub fn layer(value: f64) -> Self {
        Blur { blur_type: BlurType::LayerBlur, value, hidden: false }
    }

    /// Background blur (`backdrop-filter: blur(…)`).
    pub fn background(value: f64) -> Self {
        Blur { blur_type: BlurType::BackgroundBlur, value, hidden: false }
    }
}

// ─────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_blur() {
        let b = Blur::layer(8.0);
        assert_eq!(b.value, 8.0);
        assert_eq!(b.blur_type, BlurType::LayerBlur);
        assert!(!b.hidden);
    }

    #[test]
    fn background_blur_type() {
        let b = Blur::background(4.0);
        assert_eq!(b.blur_type, BlurType::BackgroundBlur);
    }
}
