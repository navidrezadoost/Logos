//! Text engine — shapes text and rasterizes glyphs using `cosmic-text`.
//!
//! The engine manages a `FontSystem` (font discovery + shaping) and a
//! `SwashCache` (glyph rasterization). Shaped text is output as a list
//! of positioned `GlyphQuad`s, each referencing a region in the glyph
//! atlas texture.

use cosmic_text::{
    Attrs, Buffer, Color as CColor, Family, FontSystem, Metrics,
    Shaping, SwashCache, Weight, Style as CStyle,
};

use crate::atlas::{Atlas, AtlasRegion};

/// Style specification for a text run.
#[derive(Clone, Debug)]
pub struct TextStyle {
    /// Font size in pixels.
    pub font_size: f32,
    /// Line height in pixels (set equal to font_size for tight layout).
    pub line_height: f32,
    /// RGBA color, each channel in [0.0, 1.0].
    pub color: [f32; 4],
    /// Font family name (e.g. "Inter", "Arial", "sans-serif").
    pub family: String,
    /// Font weight (400 = normal, 700 = bold).
    pub weight: u16,
    /// Italic.
    pub italic: bool,
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
        }
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
}

impl TextEngine {
    /// Create a new text engine with system font discovery.
    pub fn new() -> Self {
        Self {
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
        }
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

        let family = match style.family.as_str() {
            "sans-serif" => Family::SansSerif,
            "serif" => Family::Serif,
            "monospace" => Family::Monospace,
            name => Family::Name(name),
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

            for glyph in run.glyphs.iter() {
                let physical = glyph.physical((0.0, 0.0), 1.0);

                total_width = total_width.max(glyph.x + glyph.w);

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
                    x: physical.x as f32 + image.placement.left as f32,
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
}
