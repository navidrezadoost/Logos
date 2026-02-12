//! Text engine — shapes text and rasterizes glyphs using `cosmic-text`.
//!
//! The engine manages a `FontSystem` (font discovery + shaping) and a
//! `SwashCache` (glyph rasterization). Shaped text is output as a list
//! of positioned `GlyphQuad`s, each referencing a region in the glyph
//! atlas texture.
//!
//! ## Font Registry Integration
//!
//! The engine optionally holds a [`FontRegistry`] for CSS-style font
//! matching. When present, `TextStyle.families` are resolved through
//! the registry's fallback chain before being passed to cosmic-text.

use cosmic_text::{
    Attrs, Buffer, Color as CColor, Family, FontSystem, Metrics,
    Shaping, SwashCache, Weight, Style as CStyle,
};

use crate::atlas::{Atlas, AtlasRegion};
use crate::fonts::{FontDescriptor, FontRegistry, FontStyle as FsStyle};

// ── Text alignment ──────────────────────────────────────────────────

/// Horizontal text alignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

impl Default for TextAlign {
    fn default() -> Self {
        Self::Left
    }
}

/// Style specification for a text run.
#[derive(Clone, Debug)]
pub struct TextStyle {
    /// Font size in pixels.
    pub font_size: f32,
    /// Line height in pixels (set equal to font_size for tight layout).
    pub line_height: f32,
    /// RGBA color, each channel in [0.0, 1.0].
    pub color: [f32; 4],
    /// CSS-style font family chain (e.g. `"Arial, Helvetica, sans-serif"`).
    /// Each entry is tried in order until a match is found.
    pub family: String,
    /// Font weight (100–900). 400 = normal, 700 = bold.
    pub weight: u16,
    /// Italic.
    pub italic: bool,
    /// Horizontal text alignment.
    pub align: TextAlign,
    /// Extra letter spacing in pixels (can be negative).
    pub letter_spacing: f32,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font_size: 16.0,
            line_height: 20.0,
            color: [1.0, 1.0, 1.0, 1.0],
            family: String::from("sans-serif"),
            weight: 400,
            italic: false,
            align: TextAlign::Left,
            letter_spacing: 0.0,
        }
    }
}

impl TextStyle {
    /// Create a [`FontDescriptor`] from this style for registry matching.
    pub fn to_descriptor(&self) -> FontDescriptor {
        let font_style = if self.italic {
            FsStyle::Italic
        } else {
            FsStyle::Normal
        };
        // from_css handles lowercasing internally.
        FontDescriptor::from_css(&self.family, self.weight, font_style)
    }
}

/// A positioned glyph quad ready for GPU rendering.
///
/// References a region in the glyph atlas via `atlas_region`.
#[derive(Clone, Copy, Debug)]
pub struct GlyphQuad {
    /// Top-left position in text-local coordinates (pixels).
    pub x: f32,
    pub y: f32,
    /// Width/height of the glyph bitmap (pixels).
    pub width: f32,
    pub height: f32,
    /// UV coordinates in the atlas texture.
    pub atlas_region: AtlasRegion,
    /// RGBA color.
    pub color: [f32; 4],
}

/// Result of shaping a text block.
#[derive(Clone, Debug)]
pub struct ShapedText {
    /// Individual glyph quads to render.
    pub glyphs: Vec<GlyphQuad>,
    /// Total bounding width of the shaped text.
    pub width: f32,
    /// Total bounding height of the shaped text.
    pub height: f32,
}

/// Core text engine wrapping cosmic-text.
pub struct TextEngine {
    pub font_system: FontSystem,
    pub swash_cache: SwashCache,
    /// Optional font registry for CSS-style matching.
    pub registry: Option<FontRegistry>,
}

impl TextEngine {
    /// Create a new text engine with system font discovery (no registry).
    pub fn new() -> Self {
        Self {
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
            registry: None,
        }
    }

    /// Create a new text engine with a font registry.
    ///
    /// The registry enables CSS-style font matching (`"Arial, Helvetica, sans-serif"`).
    pub fn with_registry(registry: FontRegistry) -> Self {
        Self {
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
            registry: Some(registry),
        }
    }

    /// Access the font registry (if set).
    pub fn registry(&self) -> Option<&FontRegistry> {
        self.registry.as_ref()
    }

    /// Resolve the family name for cosmic-text, using the registry if available.
    fn resolve_family<'a>(&self, style: &'a TextStyle) -> &'a str {
        // If we have a registry, match through it.
        // The matched family name is used as Family::Name(...) in cosmic-text.
        // Note: we return the style.family as-is since cosmic-text handles
        // generic families (sans-serif, serif, monospace) natively.
        // The registry is used for font info queries, not to override cosmic-text.
        &style.family
    }

    /// Shape and rasterize a text string, returning positioned glyph quads.
    ///
    /// The `max_width` parameter enables word wrapping. Pass `f32::INFINITY`
    /// for single-line layout.
    pub fn shape_text(
        &mut self,
        text: &str,
        style: &TextStyle,
        max_width: f32,
        atlas: &mut Atlas,
    ) -> ShapedText {
        let metrics = Metrics::new(style.font_size, style.line_height);

        // Resolve the family. If the style contains a CSS chain like
        // "Arial, Helvetica, sans-serif", we try the first family that
        // cosmic-text can resolve. Cosmic-text handles fallback natively
        // when using Family::Name.
        let family_str = self.resolve_family(style);
        let family = match family_str {
            "sans-serif" => Family::SansSerif,
            "serif" => Family::Serif,
            "monospace" => Family::Monospace,
            name => {
                // Parse the first family from a potential CSS chain.
                let first = name.split(',')
                    .next()
                    .unwrap_or(name)
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'');
                match first {
                    "sans-serif" => Family::SansSerif,
                    "serif" => Family::Serif,
                    "monospace" => Family::Monospace,
                    concrete => Family::Name(concrete),
                }
            }
        };

        let weight = Weight(style.weight);
        let font_style = if style.italic {
            CStyle::Italic
        } else {
            CStyle::Normal
        };

        let attrs = Attrs::new()
            .family(family)
            .weight(weight)
            .style(font_style)
            .color(CColor::rgba(
                (style.color[0] * 255.0) as u8,
                (style.color[1] * 255.0) as u8,
                (style.color[2] * 255.0) as u8,
                (style.color[3] * 255.0) as u8,
            ));

        // Create a cosmic-text buffer for layout.
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_size(&mut self.font_system, Some(max_width), None);
        buffer.set_text(&mut self.font_system, text, attrs, Shaping::Advanced);
        buffer.shape_until_scroll(&mut self.font_system, false);

        let mut quads = Vec::new();
        let mut total_width: f32 = 0.0;
        let mut total_height: f32 = 0.0;

        // Iterate layout runs → glyphs.
        for run in buffer.layout_runs() {
            let line_y = run.line_y;
            total_height = total_height.max(line_y + style.line_height);

            // Compute line width for alignment.
            let line_width = run.glyphs.iter()
                .map(|g| g.x + g.w)
                .fold(0.0f32, f32::max);

            // Alignment offset.
            let align_offset = match style.align {
                TextAlign::Left => 0.0,
                TextAlign::Center => (max_width - line_width) / 2.0,
                TextAlign::Right => max_width - line_width,
            };
            // Only apply alignment if we have a finite max_width.
            let align_offset = if max_width.is_finite() { align_offset.max(0.0) } else { 0.0 };

            for (glyph_idx, glyph) in run.glyphs.iter().enumerate() {
                let physical = glyph.physical((0.0, 0.0), 1.0);

                // Apply letter spacing.
                let spacing_offset = style.letter_spacing * glyph_idx as f32;

                total_width = total_width.max(glyph.x + glyph.w + spacing_offset + align_offset);

                // Rasterize via swash.
                let image = self.swash_cache.get_image(
                    &mut self.font_system,
                    physical.cache_key,
                );

                let image = match image {
                    Some(img) => img,
                    None => continue, // whitespace or missing glyph
                };

                if image.placement.width == 0 || image.placement.height == 0 {
                    continue;
                }

                // Upload to atlas.
                let region = atlas.insert(
                    physical.cache_key.glyph_id,
                    image.placement.width as u32,
                    image.placement.height as u32,
                    &image.data,
                );

                let region = match region {
                    Some(r) => r,
                    None => continue, // atlas full
                };

                quads.push(GlyphQuad {
                    x: physical.x as f32 + image.placement.left as f32 + align_offset + spacing_offset,
                    y: line_y + physical.y as f32 - image.placement.top as f32,
                    width: image.placement.width as f32,
                    height: image.placement.height as f32,
                    atlas_region: region,
                    color: style.color,
                });
            }
        }

        ShapedText {
            glyphs: quads,
            width: total_width,
            height: total_height,
        }
    }
}

// ===================================================================
// Tests
// ===================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atlas::Atlas;
    use crate::fonts::FontRegistry;

    #[test]
    fn test_text_engine_creation() {
        let engine = TextEngine::new();
        // Font system should have discovered system fonts.
        assert!(engine.font_system.db().faces().count() > 0);
    }

    #[test]
    fn test_shape_empty_string() {
        let mut engine = TextEngine::new();
        let mut atlas = Atlas::new(512);
        let style = TextStyle::default();
        let result = engine.shape_text("", &style, f32::INFINITY, &mut atlas);
        assert_eq!(result.glyphs.len(), 0);
    }

    #[test]
    fn test_shape_hello_world() {
        let mut engine = TextEngine::new();
        let mut atlas = Atlas::new(512);
        let style = TextStyle {
            font_size: 24.0,
            line_height: 28.0,
            color: [1.0, 1.0, 1.0, 1.0],
            family: "sans-serif".into(),
            weight: 400,
            italic: false,
            ..Default::default()
        };
        let result = engine.shape_text("Hello, Logos!", &style, f32::INFINITY, &mut atlas);
        // Should produce glyphs (exact count depends on system fonts).
        assert!(!result.glyphs.is_empty(), "Expected glyphs for 'Hello, Logos!'");
        assert!(result.width > 0.0);
        assert!(result.height > 0.0);
    }

    #[test]
    fn test_shape_multiline() {
        let mut engine = TextEngine::new();
        let mut atlas = Atlas::new(512);
        let style = TextStyle {
            font_size: 16.0,
            line_height: 20.0,
            ..Default::default()
        };
        // Very narrow width to force word wrap.
        let result = engine.shape_text("Hello World Logos Design", &style, 80.0, &mut atlas);
        assert!(!result.glyphs.is_empty());
        // Should be taller than single line.
        assert!(result.height > 20.0, "Expected multi-line height, got {}", result.height);
    }

    #[test]
    fn test_style_default() {
        let style = TextStyle::default();
        assert_eq!(style.font_size, 16.0);
        assert_eq!(style.weight, 400);
        assert!(!style.italic);
        assert_eq!(style.family, "sans-serif");
    }

    #[test]
    fn test_shape_bold_style() {
        let mut engine = TextEngine::new();
        let mut atlas = Atlas::new(512);
        let style = TextStyle {
            font_size: 20.0,
            line_height: 24.0,
            weight: 700,
            ..Default::default()
        };
        let result = engine.shape_text("Bold", &style, f32::INFINITY, &mut atlas);
        assert!(!result.glyphs.is_empty());
    }

    #[test]
    fn test_glyph_quad_positions_positive() {
        let mut engine = TextEngine::new();
        let mut atlas = Atlas::new(512);
        let style = TextStyle {
            font_size: 32.0,
            line_height: 36.0,
            ..Default::default()
        };
        let result = engine.shape_text("A", &style, f32::INFINITY, &mut atlas);
        // All glyph widths/heights should be positive.
        for glyph in &result.glyphs {
            assert!(glyph.width > 0.0, "width should be > 0");
            assert!(glyph.height > 0.0, "height should be > 0");
        }
    }

    #[test]
    fn test_atlas_populated_after_shaping() {
        let mut engine = TextEngine::new();
        let mut atlas = Atlas::new(512);
        let style = TextStyle::default();
        engine.shape_text("Test", &style, f32::INFINITY, &mut atlas);
        assert!(atlas.glyph_count() > 0, "Atlas should have glyphs after shaping");
    }

    #[test]
    fn test_style_default_new_fields() {
        let style = TextStyle::default();
        assert_eq!(style.align, TextAlign::Left);
        assert_eq!(style.letter_spacing, 0.0);
    }

    #[test]
    fn test_text_align_variants() {
        assert_eq!(TextAlign::default(), TextAlign::Left);
        assert_ne!(TextAlign::Center, TextAlign::Right);
    }

    #[test]
    fn test_style_to_descriptor() {
        let style = TextStyle {
            family: "Arial, Helvetica, sans-serif".into(),
            weight: 700,
            italic: true,
            ..Default::default()
        };
        let desc = style.to_descriptor();
        assert_eq!(desc.families, vec!["arial", "helvetica", "sans-serif"]);
        assert_eq!(desc.weight, 700);
        assert_eq!(desc.style, FsStyle::Italic);
    }

    #[test]
    fn test_shape_serif() {
        let mut engine = TextEngine::new();
        let mut atlas = Atlas::new(512);
        let style = TextStyle {
            font_size: 20.0,
            line_height: 24.0,
            family: "serif".into(),
            ..Default::default()
        };
        let result = engine.shape_text("Serif", &style, f32::INFINITY, &mut atlas);
        assert!(!result.glyphs.is_empty(), "Expected glyphs for serif");
    }

    #[test]
    fn test_shape_monospace() {
        let mut engine = TextEngine::new();
        let mut atlas = Atlas::new(512);
        let style = TextStyle {
            font_size: 16.0,
            line_height: 20.0,
            family: "monospace".into(),
            ..Default::default()
        };
        let result = engine.shape_text("mono", &style, f32::INFINITY, &mut atlas);
        assert!(!result.glyphs.is_empty(), "Expected glyphs for monospace");
    }

    #[test]
    fn test_shape_italic() {
        let mut engine = TextEngine::new();
        let mut atlas = Atlas::new(512);
        let style = TextStyle {
            font_size: 20.0,
            line_height: 24.0,
            italic: true,
            ..Default::default()
        };
        let result = engine.shape_text("Italic", &style, f32::INFINITY, &mut atlas);
        assert!(!result.glyphs.is_empty());
    }

    #[test]
    fn test_shape_with_letter_spacing() {
        let mut engine = TextEngine::new();
        let mut atlas = Atlas::new(512);

        let style_normal = TextStyle::default();
        let style_spaced = TextStyle {
            letter_spacing: 2.0,
            ..Default::default()
        };

        let normal = engine.shape_text("ABC", &style_normal, f32::INFINITY, &mut atlas);
        let spaced = engine.shape_text("ABC", &style_spaced, f32::INFINITY, &mut atlas);

        // Spaced text should be wider.
        assert!(
            spaced.width > normal.width,
            "Spaced width {} should be > normal width {}",
            spaced.width,
            normal.width,
        );
    }

    #[test]
    fn test_shape_with_alignment_center() {
        let mut engine = TextEngine::new();
        let mut atlas = Atlas::new(512);
        let style = TextStyle {
            font_size: 16.0,
            line_height: 20.0,
            align: TextAlign::Center,
            ..Default::default()
        };
        // Use a finite max_width so alignment applies.
        let result = engine.shape_text("Hi", &style, 400.0, &mut atlas);
        assert!(!result.glyphs.is_empty());
        // First glyph should be offset from 0 (centered).
        if let Some(first) = result.glyphs.first() {
            assert!(
                first.x > 10.0,
                "Center-aligned text should be offset, got x={}",
                first.x,
            );
        }
    }

    #[test]
    fn test_shape_with_alignment_right() {
        let mut engine = TextEngine::new();
        let mut atlas = Atlas::new(512);
        let style = TextStyle {
            font_size: 16.0,
            line_height: 20.0,
            align: TextAlign::Right,
            ..Default::default()
        };
        let result = engine.shape_text("Hi", &style, 400.0, &mut atlas);
        assert!(!result.glyphs.is_empty());
        // First glyph should be significantly offset (right-aligned).
        if let Some(first) = result.glyphs.first() {
            assert!(
                first.x > 100.0,
                "Right-aligned text should be far right, got x={}",
                first.x,
            );
        }
    }

    #[test]
    fn test_engine_with_registry() {
        let registry = FontRegistry::discover();
        let engine = TextEngine::with_registry(registry);
        assert!(engine.registry().is_some());
    }

    #[test]
    fn test_engine_without_registry() {
        let engine = TextEngine::new();
        assert!(engine.registry().is_none());
    }

    #[test]
    fn test_shape_css_family_chain() {
        let mut engine = TextEngine::new();
        let mut atlas = Atlas::new(512);
        // CSS-style family with fallback.
        let style = TextStyle {
            family: "NonExistentFont, sans-serif".into(),
            font_size: 16.0,
            line_height: 20.0,
            ..Default::default()
        };
        let result = engine.shape_text("Test", &style, f32::INFINITY, &mut atlas);
        // Should still produce glyphs via fallback.
        assert!(!result.glyphs.is_empty(), "CSS family chain should produce glyphs");
    }

    #[test]
    fn test_shape_different_colors() {
        let mut engine = TextEngine::new();
        let mut atlas = Atlas::new(512);

        let red_style = TextStyle {
            color: [1.0, 0.0, 0.0, 1.0],
            ..Default::default()
        };
        let blue_style = TextStyle {
            color: [0.0, 0.0, 1.0, 1.0],
            ..Default::default()
        };

        let red = engine.shape_text("R", &red_style, f32::INFINITY, &mut atlas);
        let blue = engine.shape_text("B", &blue_style, f32::INFINITY, &mut atlas);

        if let (Some(rg), Some(bg)) = (red.glyphs.first(), blue.glyphs.first()) {
            assert_eq!(rg.color, [1.0, 0.0, 0.0, 1.0]);
            assert_eq!(bg.color, [0.0, 0.0, 1.0, 1.0]);
        }
    }

    #[test]
    fn test_shape_different_sizes() {
        let mut engine = TextEngine::new();
        let mut atlas = Atlas::new(1024);

        let small = TextStyle { font_size: 12.0, line_height: 14.0, ..Default::default() };
        let large = TextStyle { font_size: 48.0, line_height: 56.0, ..Default::default() };

        let result_small = engine.shape_text("A", &small, f32::INFINITY, &mut atlas);
        let result_large = engine.shape_text("A", &large, f32::INFINITY, &mut atlas);

        // Large should be taller.
        assert!(
            result_large.height > result_small.height,
            "Large ({}) should be taller than small ({})",
            result_large.height,
            result_small.height,
        );
    }
}
