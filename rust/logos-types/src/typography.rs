//! Typography definitions.
//!
//! Clojure source: `common/src/app/common/types/typography.cljc`.
//!
//! A `Typography` record describes a reusable text style stored in the
//! shared library.  Text shapes reference it by `:typography-ref-id`.

use uuid::Uuid;

/// A named, reusable set of text-formatting properties (font, size, spacing…).
///
/// Clojure map keys mirror the `:font-*` / `:line-height` fields on text
/// paragraph nodes in the `content` tree.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, rename_all = "camelCase"))]
pub struct Typography {
    pub id: Uuid,
    pub name: String,
    /// CSS font-family string, e.g. `"Source Sans Pro"`.
    pub font_family: String,
    /// Google Fonts / custom font ID.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub font_id: Option<String>,
    /// `"regular"`, `"bold"`, `"700"`, …
    #[cfg_attr(feature = "serde", serde(default = "default_variant"))]
    pub font_variant_id: String,
    /// CSS font-style: `"normal"` | `"italic"`.
    #[cfg_attr(feature = "serde", serde(default = "default_font_style"))]
    pub font_style: String,
    /// Numeric weight: `400`, `700`, …
    #[cfg_attr(feature = "serde", serde(default = "default_font_weight"))]
    pub font_weight: String,
    /// Font size in pixels (stored as string in Clojure).
    #[cfg_attr(feature = "serde", serde(default = "default_font_size"))]
    pub font_size: String,
    /// Line-height multiplier or `"1.2"`.
    #[cfg_attr(feature = "serde", serde(default = "default_line_height"))]
    pub line_height: String,
    /// Letter-spacing in pixels.
    #[cfg_attr(feature = "serde", serde(default = "default_letter_spacing"))]
    pub letter_spacing: String,
    /// CSS text-transform: `"none"` | `"uppercase"` | `"lowercase"` | `"capitalize"`.
    #[cfg_attr(feature = "serde", serde(default = "default_text_transform"))]
    pub text_transform: String,
    /// Path in the library tree (e.g. `"Headings/H1"`).
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub path: Option<String>,
}

fn default_variant()       -> String { "regular".into() }
fn default_font_style()    -> String { "normal".into() }
fn default_font_weight()   -> String { "400".into() }
fn default_font_size()     -> String { "14".into() }
fn default_line_height()   -> String { "1.2".into() }
fn default_letter_spacing() -> String { "0".into() }
fn default_text_transform() -> String { "none".into() }

impl Typography {
    /// Create a minimal typography with sane defaults.
    pub fn new(id: Uuid, name: impl Into<String>, font_family: impl Into<String>) -> Self {
        Typography {
            id,
            name: name.into(),
            font_family: font_family.into(),
            font_id: None,
            font_variant_id: default_variant(),
            font_style: default_font_style(),
            font_weight: default_font_weight(),
            font_size: default_font_size(),
            line_height: default_line_height(),
            letter_spacing: default_letter_spacing(),
            text_transform: default_text_transform(),
            path: None,
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
    fn new_typography_defaults() {
        let id = Uuid::new_v4();
        let t = Typography::new(id, "Body", "Inter");
        assert_eq!(t.font_family, "Inter");
        assert_eq!(t.font_size, "14");
        assert_eq!(t.font_weight, "400");
    }
}
