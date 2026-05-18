use std::fmt;

use crate::uuid::Uuid;

/// A single OpenType variation axis value.
///
/// `tag` is a 4-byte ASCII axis identifier packed into a u32 (big-endian),
/// matching `SkFontArguments::VariationPosition::Coordinate::axis`.
///
/// Well-known registered axes:
/// | Tag  | u32 hex    | Meaning          |
/// |------|------------|------------------|
/// | wght | 0x77676874 | Weight (100–900) |
/// | wdth | 0x77647468 | Width  (75–125)  |
/// | slnt | 0x736c6e74 | Slant  (-90–90°) |
/// | opsz | 0x6f70737a | Optical size     |
/// | ital | 0x6974616c | Italic (0–1)     |
#[derive(Debug, Clone, PartialEq, Copy)]
pub struct FontVariationAxis {
    /// 4-byte ASCII tag packed as big-endian u32, e.g. `b"wght"` → `0x77676874`.
    pub tag: u32,
    /// Axis value within the font's defined range for this axis.
    pub value: f32,
}

impl FontVariationAxis {
    /// Construct from a 4-character ASCII tag string and a value.
    ///
    /// # Panics
    /// Panics in debug builds if `tag` is not exactly 4 ASCII bytes.
    pub fn new(tag: &str, value: f32) -> Self {
        let bytes = tag.as_bytes();
        debug_assert_eq!(bytes.len(), 4, "OpenType axis tag must be exactly 4 bytes");
        let packed = ((bytes[0] as u32) << 24)
            | ((bytes[1] as u32) << 16)
            | ((bytes[2] as u32) << 8)
            | (bytes[3] as u32);
        Self { tag: packed, value }
    }

    /// Return the tag as a 4-character ASCII string (for Display / debugging).
    pub fn tag_str(&self) -> String {
        let b = [
            ((self.tag >> 24) & 0xff) as u8,
            ((self.tag >> 16) & 0xff) as u8,
            ((self.tag >> 8) & 0xff) as u8,
            (self.tag & 0xff) as u8,
        ];
        String::from_utf8_lossy(&b).into_owned()
    }
}

impl fmt::Display for FontVariationAxis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "'{}' {}", self.tag_str(), self.value)
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum FontStyle {
    Normal,
    Italic,
}

impl fmt::Display for FontStyle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let txt = match self {
            Self::Normal => "normal",
            Self::Italic => "italic",
        };
        write!(f, "{}", txt)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FontFamily {
    id: Uuid,
    style: FontStyle,
    weight: u32,
    /// OpenType variable font axis overrides.
    /// Empty for static fonts; populated for variable fonts.
    variation_axes: Vec<FontVariationAxis>,
}

impl FontFamily {
    pub fn new(id: Uuid, weight: u32, style: FontStyle) -> Self {
        Self {
            id,
            style,
            weight,
            variation_axes: Vec::new(),
        }
    }

    /// Set all variation axes at once (replaces any previous axes).
    pub fn with_variation_axes(mut self, axes: Vec<FontVariationAxis>) -> Self {
        self.variation_axes = axes;
        self
    }

    /// Set a single axis by 4-char tag, adding or updating it.
    pub fn set_axis(&mut self, tag: &str, value: f32) {
        let axis = FontVariationAxis::new(tag, value);
        if let Some(existing) = self.variation_axes.iter_mut().find(|a| a.tag == axis.tag) {
            existing.value = value;
        } else {
            self.variation_axes.push(axis);
        }
    }

    /// Returns the variation axes as a slice suitable for constructing
    /// `SkFontArguments::VariationPosition::Coordinate` entries.
    ///
    /// Each entry is `(tag: u32, value: f32)` — drop directly into:
    /// ```ignore
    /// let coords: Vec<_> = font_family.variation_coordinates()
    ///     .iter()
    ///     .map(|(tag, value)| Coordinate { axis: *tag, value: *value })
    ///     .collect();
    /// let args = SkFontArguments::new()
    ///     .set_variation_design_position(&coords);
    /// ```
    pub fn variation_coordinates(&self) -> Vec<(u32, f32)> {
        self.variation_axes
            .iter()
            .map(|a| (a.tag, a.value))
            .collect()
    }

    /// True if this font has any variation axes set.
    pub fn is_variable(&self) -> bool {
        !self.variation_axes.is_empty()
    }

    pub fn alias(&self) -> String {
        format!("{}", self)
    }
}

impl fmt::Display for FontFamily {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} {}", self.id, self.weight, self.style)?;
        for axis in &self.variation_axes {
            write!(f, " [{}]", axis)?;
        }
        Ok(())
    }
}

/// Parse a CSS `font-variation-settings` string such as
/// `"'wght' 750, 'wdth' 100"` into a `Vec<FontVariationAxis>`.
///
/// This is the wire format used by the ClojureScript / TypeScript frontend
/// to pass variable font settings from the typography store to the Rust
/// renderer. Returns an empty vec on parse failure (fail-open).
pub fn parse_variation_settings(s: &str) -> Vec<FontVariationAxis> {
    let mut axes = Vec::new();
    for token in s.split(',') {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        // Expected form: `'wght' 750` or `"wght" 750`
        let token = token.trim_matches(|c| c == '\'' || c == '"');
        // After stripping leading quote the form is: `wght' 750` — split on whitespace
        let token = token.trim();
        let mut parts = token.splitn(2, |c: char| c == '\'' || c == '"' || c.is_whitespace());
        let raw_tag = parts
            .next()
            .unwrap_or("")
            .trim()
            .trim_matches(|c| c == '\'' || c == '"');
        let raw_val = parts
            .next()
            .unwrap_or("")
            .trim()
            .trim_matches(|c| c == '\'' || c == '"');
        if raw_tag.len() == 4 {
            if let Ok(value) = raw_val.parse::<f32>() {
                axes.push(FontVariationAxis::new(raw_tag, value));
            }
        }
    }
    axes
}
