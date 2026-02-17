//! Visual style types: fills, gradients, shadows, strokes.
//!
//! These types describe *how* a layer looks, decoupled from geometry.
//! They are designed for zero-copy GPU upload where possible.
//!
//! References:
//! - Foley et al., §14.10 (shading models)
//! - CSS Backgrounds & Borders Level 3 (gradient stops)
//! - Figma REST API v1 (paint/effect model)

use serde::{Deserialize, Serialize};

// ═══════════════════════════════════════════════════════════════════
// Color
// ═══════════════════════════════════════════════════════════════════

/// RGBA color in linear float space (pre-multiplication is the caller's
/// responsibility; the GPU shader handles it).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const WHITE: Self = Self { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };
    pub const BLACK: Self = Self { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };
    pub const TRANSPARENT: Self = Self { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };

    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Convert from 0–255 sRGB bytes to linear float.
    pub fn from_srgb(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r: srgb_to_linear(r),
            g: srgb_to_linear(g),
            b: srgb_to_linear(b),
            a: a as f32 / 255.0,
        }
    }

    /// Pack to `[f32; 4]` for GPU upload.
    pub fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }

    /// Linearly interpolate between two colors.
    pub fn lerp(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self {
            r: self.r + (other.r - self.r) * t,
            g: self.g + (other.g - self.g) * t,
            b: self.b + (other.b - self.b) * t,
            a: self.a + (other.a - self.a) * t,
        }
    }
}

impl Default for Color {
    fn default() -> Self {
        Self::BLACK
    }
}

/// sRGB byte → linear float conversion (IEC 61966-2-1).
fn srgb_to_linear(c: u8) -> f32 {
    let s = c as f32 / 255.0;
    if s <= 0.04045 {
        s / 12.92
    } else {
        ((s + 0.055) / 1.055).powf(2.4)
    }
}

// ═══════════════════════════════════════════════════════════════════
// Gradient
// ═══════════════════════════════════════════════════════════════════

/// A single color stop in a gradient.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct GradientStop {
    /// Position along the gradient axis, in [0, 1].
    pub position: f32,
    pub color: Color,
}

impl GradientStop {
    pub fn new(position: f32, color: Color) -> Self {
        Self {
            position: position.clamp(0.0, 1.0),
            color,
        }
    }
}

/// Gradient type — linear or radial.
///
/// Angles follow CSS convention: 0° = bottom→top, 90° = left→right.
/// Radial gradients are elliptical, centered at `(cx, cy)` in [0,1]
/// normalised coordinates relative to the layer bounds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Gradient {
    Linear {
        /// Angle in degrees (CSS convention).
        angle_deg: f32,
        stops: Vec<GradientStop>,
    },
    Radial {
        /// Center X in [0, 1] relative to bounds.
        cx: f32,
        /// Center Y in [0, 1] relative to bounds.
        cy: f32,
        /// Horizontal radius ratio (1.0 = touch edges).
        rx: f32,
        /// Vertical radius ratio (1.0 = touch edges).
        ry: f32,
        stops: Vec<GradientStop>,
    },
}

impl Gradient {
    /// Create a simple two-stop linear gradient.
    pub fn linear(angle_deg: f32, start: Color, end: Color) -> Self {
        Self::Linear {
            angle_deg,
            stops: vec![
                GradientStop::new(0.0, start),
                GradientStop::new(1.0, end),
            ],
        }
    }

    /// Create a simple two-stop radial gradient centered at (0.5, 0.5).
    pub fn radial(inner: Color, outer: Color) -> Self {
        Self::Radial {
            cx: 0.5,
            cy: 0.5,
            rx: 1.0,
            ry: 1.0,
            stops: vec![
                GradientStop::new(0.0, inner),
                GradientStop::new(1.0, outer),
            ],
        }
    }

    /// Get the gradient stops.
    pub fn stops(&self) -> &[GradientStop] {
        match self {
            Self::Linear { stops, .. } | Self::Radial { stops, .. } => stops,
        }
    }

    /// Evaluate the gradient color at parameter `t ∈ [0, 1]`.
    pub fn sample(&self, t: f32) -> Color {
        let stops = self.stops();
        if stops.is_empty() {
            return Color::BLACK;
        }
        if stops.len() == 1 || t <= stops[0].position {
            return stops[0].color;
        }
        if t >= stops[stops.len() - 1].position {
            return stops[stops.len() - 1].color;
        }
        for window in stops.windows(2) {
            let (a, b) = (&window[0], &window[1]);
            if t >= a.position && t <= b.position {
                let range = b.position - a.position;
                let local_t = if range > 0.0 {
                    (t - a.position) / range
                } else {
                    0.0
                };
                return a.color.lerp(b.color, local_t);
            }
        }
        stops[stops.len() - 1].color
    }
}

// ═══════════════════════════════════════════════════════════════════
// Fill
// ═══════════════════════════════════════════════════════════════════

/// How a layer is filled: flat color, gradient, or (future) image.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Fill {
    Solid(Color),
    Gradient(Gradient),
}

impl Default for Fill {
    fn default() -> Self {
        Self::Solid(Color::WHITE)
    }
}

impl Fill {
    pub fn solid(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self::Solid(Color::new(r, g, b, a))
    }

    pub fn linear_gradient(angle_deg: f32, start: Color, end: Color) -> Self {
        Self::Gradient(Gradient::linear(angle_deg, start, end))
    }

    pub fn radial_gradient(inner: Color, outer: Color) -> Self {
        Self::Gradient(Gradient::radial(inner, outer))
    }
}

// ═══════════════════════════════════════════════════════════════════
// Stroke
// ═══════════════════════════════════════════════════════════════════

/// Stroke alignment relative to the path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrokeAlign {
    Center,
    Inside,
    Outside,
}

impl Default for StrokeAlign {
    fn default() -> Self {
        Self::Center
    }
}

/// Line cap style.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineCap {
    Butt,
    Round,
    Square,
}

impl Default for LineCap {
    fn default() -> Self {
        Self::Butt
    }
}

/// Line join style.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineJoin {
    Miter,
    Round,
    Bevel,
}

impl Default for LineJoin {
    fn default() -> Self {
        Self::Miter
    }
}

/// Stroke style for a layer outline.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    pub color: Color,
    pub width: f32,
    pub align: StrokeAlign,
    pub cap: LineCap,
    pub join: LineJoin,
    /// Dash pattern (alternating on/off lengths). Empty = solid.
    pub dash_pattern: Vec<f32>,
    /// Offset into the dash pattern.
    pub dash_offset: f32,
}

impl Default for Stroke {
    fn default() -> Self {
        Self {
            color: Color::BLACK,
            width: 1.0,
            align: StrokeAlign::Center,
            cap: LineCap::Butt,
            join: LineJoin::Miter,
            dash_pattern: Vec::new(),
            dash_offset: 0.0,
        }
    }
}

impl Stroke {
    pub fn new(color: Color, width: f32) -> Self {
        Self {
            color,
            width,
            ..Default::default()
        }
    }

    pub fn with_align(mut self, align: StrokeAlign) -> Self {
        self.align = align;
        self
    }
}

// ═══════════════════════════════════════════════════════════════════
// Shadow (Box / Drop / Inner)
// ═══════════════════════════════════════════════════════════════════

/// Shadow type: drop shadow (outside) or inner shadow (inside).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShadowKind {
    Drop,
    Inner,
}

/// A box shadow (or drop shadow / inner shadow).
///
/// References:
/// - CSS `box-shadow` specification
/// - Figma API `Effect` type
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Shadow {
    pub kind: ShadowKind,
    pub color: Color,
    /// Horizontal offset in pixels.
    pub offset_x: f32,
    /// Vertical offset in pixels.
    pub offset_y: f32,
    /// Gaussian blur radius (σ ≈ radius / 2).
    pub blur_radius: f32,
    /// Spread: positive = expand, negative = shrink.
    pub spread: f32,
}

impl Shadow {
    /// Create a drop shadow.
    pub fn drop_shadow(
        color: Color,
        offset_x: f32,
        offset_y: f32,
        blur_radius: f32,
    ) -> Self {
        Self {
            kind: ShadowKind::Drop,
            color,
            offset_x,
            offset_y,
            blur_radius,
            spread: 0.0,
        }
    }

    /// Create an inner shadow.
    pub fn inner_shadow(
        color: Color,
        offset_x: f32,
        offset_y: f32,
        blur_radius: f32,
    ) -> Self {
        Self {
            kind: ShadowKind::Inner,
            color,
            offset_x,
            offset_y,
            blur_radius,
            spread: 0.0,
        }
    }

    pub fn with_spread(mut self, spread: f32) -> Self {
        self.spread = spread;
        self
    }

    /// The Gaussian sigma derived from the blur radius.
    pub fn sigma(&self) -> f32 {
        self.blur_radius * 0.5
    }

    /// The total extra extent needed around the layer to fit the shadow.
    /// Used for allocating renderable area.
    pub fn extent(&self) -> f32 {
        (self.blur_radius + self.spread.abs() + self.offset_x.abs().max(self.offset_y.abs()))
            .max(0.0)
    }
}

// ═══════════════════════════════════════════════════════════════════
// LayerStyle (aggregate)
// ═══════════════════════════════════════════════════════════════════

/// Complete visual style for a layer, separate from geometry.
///
/// Multiple fills and strokes are supported (like Figma).
/// Shadows are rendered in order: drop shadows behind, inner shadows on top.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LayerStyle {
    pub fills: Vec<Fill>,
    pub strokes: Vec<Stroke>,
    pub shadows: Vec<Shadow>,
    /// Overall layer opacity in [0, 1].
    pub opacity: f32,
    /// Whether the layer is visible.
    pub visible: bool,
    /// Corner radii [top-left, top-right, bottom-right, bottom-left].
    pub corner_radii: [f32; 4],
    /// Blend mode (stored as string for now; shader support is future work).
    pub blend_mode: BlendMode,
}

impl Default for LayerStyle {
    fn default() -> Self {
        Self {
            fills: vec![Fill::default()],
            strokes: Vec::new(),
            shadows: Vec::new(),
            opacity: 1.0,
            visible: true,
            corner_radii: [0.0; 4],
            blend_mode: BlendMode::Normal,
        }
    }
}

impl LayerStyle {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_fill(mut self, fill: Fill) -> Self {
        self.fills = vec![fill];
        self
    }

    pub fn with_stroke(mut self, stroke: Stroke) -> Self {
        self.strokes = vec![stroke];
        self
    }

    pub fn with_shadow(mut self, shadow: Shadow) -> Self {
        self.shadows.push(shadow);
        self
    }

    pub fn with_opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity.clamp(0.0, 1.0);
        self
    }

    pub fn with_corner_radii(mut self, radii: [f32; 4]) -> Self {
        self.corner_radii = radii;
        self
    }

    /// Returns true if any shadow requires extra rendering extent.
    pub fn has_shadows(&self) -> bool {
        !self.shadows.is_empty()
    }

    /// Maximum shadow extent needed around this layer.
    pub fn shadow_extent(&self) -> f32 {
        self.shadows.iter().map(|s| s.extent()).fold(0.0f32, f32::max)
    }

    /// The primary fill color (first solid fill), if any.
    pub fn primary_color(&self) -> Option<Color> {
        self.fills.iter().find_map(|f| match f {
            Fill::Solid(c) => Some(*c),
            _ => None,
        })
    }
}

// ═══════════════════════════════════════════════════════════════════
// Blend mode
// ═══════════════════════════════════════════════════════════════════

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    SoftLight,
    HardLight,
    Difference,
    Exclusion,
}

impl Default for BlendMode {
    fn default() -> Self {
        Self::Normal
    }
}

// ═══════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_constants() {
        assert_eq!(Color::WHITE.to_array(), [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(Color::BLACK.to_array(), [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(Color::TRANSPARENT.a, 0.0);
    }

    #[test]
    fn test_color_from_srgb() {
        let c = Color::from_srgb(255, 255, 255, 255);
        assert!((c.r - 1.0).abs() < 0.001);
        assert!((c.a - 1.0).abs() < 0.001);

        let black = Color::from_srgb(0, 0, 0, 255);
        assert!((black.r).abs() < 0.001);
    }

    #[test]
    fn test_color_lerp() {
        let a = Color::BLACK;
        let b = Color::WHITE;
        let mid = a.lerp(b, 0.5);
        assert!((mid.r - 0.5).abs() < 0.001);
        assert!((mid.g - 0.5).abs() < 0.001);
        assert!((mid.b - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_gradient_stop_clamp() {
        let stop = GradientStop::new(-0.5, Color::BLACK);
        assert_eq!(stop.position, 0.0);
        let stop2 = GradientStop::new(1.5, Color::WHITE);
        assert_eq!(stop2.position, 1.0);
    }

    #[test]
    fn test_linear_gradient_sample() {
        let g = Gradient::linear(90.0, Color::BLACK, Color::WHITE);
        let mid = g.sample(0.5);
        assert!((mid.r - 0.5).abs() < 0.01);
        let start = g.sample(0.0);
        assert!((start.r).abs() < 0.01);
        let end = g.sample(1.0);
        assert!((end.r - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_gradient_sample_multi_stop() {
        let g = Gradient::Linear {
            angle_deg: 0.0,
            stops: vec![
                GradientStop::new(0.0, Color::new(1.0, 0.0, 0.0, 1.0)),
                GradientStop::new(0.5, Color::new(0.0, 1.0, 0.0, 1.0)),
                GradientStop::new(1.0, Color::new(0.0, 0.0, 1.0, 1.0)),
            ],
        };
        let at_quarter = g.sample(0.25);
        // Should be midway between red and green
        assert!((at_quarter.r - 0.5).abs() < 0.01);
        assert!((at_quarter.g - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_fill_default() {
        let fill = Fill::default();
        assert!(matches!(fill, Fill::Solid(Color { r, .. }) if (r - 1.0).abs() < 0.01));
    }

    #[test]
    fn test_stroke_default() {
        let stroke = Stroke::default();
        assert_eq!(stroke.width, 1.0);
        assert_eq!(stroke.align, StrokeAlign::Center);
        assert_eq!(stroke.cap, LineCap::Butt);
        assert!(stroke.dash_pattern.is_empty());
    }

    #[test]
    fn test_shadow_drop() {
        let s = Shadow::drop_shadow(Color::BLACK, 4.0, 4.0, 8.0);
        assert_eq!(s.kind, ShadowKind::Drop);
        assert!((s.sigma() - 4.0).abs() < 0.01);
        assert!(s.extent() > 0.0);
    }

    #[test]
    fn test_shadow_inner() {
        let s = Shadow::inner_shadow(Color::BLACK, 0.0, 2.0, 4.0);
        assert_eq!(s.kind, ShadowKind::Inner);
    }

    #[test]
    fn test_shadow_extent() {
        let s = Shadow::drop_shadow(Color::BLACK, 10.0, 5.0, 20.0).with_spread(4.0);
        // extent = blur_radius + spread + max(|ox|, |oy|) = 20 + 4 + 10 = 34
        assert!((s.extent() - 34.0).abs() < 0.01);
    }

    #[test]
    fn test_layer_style_default() {
        let style = LayerStyle::default();
        assert_eq!(style.fills.len(), 1);
        assert!(style.strokes.is_empty());
        assert!(style.shadows.is_empty());
        assert_eq!(style.opacity, 1.0);
        assert!(style.visible);
    }

    #[test]
    fn test_layer_style_builder() {
        let style = LayerStyle::new()
            .with_fill(Fill::solid(1.0, 0.0, 0.0, 1.0))
            .with_stroke(Stroke::new(Color::BLACK, 2.0))
            .with_shadow(Shadow::drop_shadow(Color::BLACK, 4.0, 4.0, 8.0))
            .with_opacity(0.8)
            .with_corner_radii([8.0, 8.0, 0.0, 0.0]);

        assert_eq!(style.fills.len(), 1);
        assert_eq!(style.strokes.len(), 1);
        assert_eq!(style.shadows.len(), 1);
        assert!((style.opacity - 0.8).abs() < 0.01);
        assert_eq!(style.corner_radii, [8.0, 8.0, 0.0, 0.0]);
    }

    #[test]
    fn test_layer_style_shadow_extent() {
        let style = LayerStyle::new()
            .with_shadow(Shadow::drop_shadow(Color::BLACK, 2.0, 2.0, 4.0))
            .with_shadow(Shadow::drop_shadow(Color::BLACK, 8.0, 8.0, 16.0));

        assert!(style.has_shadows());
        // Max extent should come from the second shadow
        assert!(style.shadow_extent() > 20.0);
    }

    #[test]
    fn test_layer_style_primary_color() {
        let style = LayerStyle::new()
            .with_fill(Fill::linear_gradient(90.0, Color::BLACK, Color::WHITE));
        assert!(style.primary_color().is_none()); // gradient, not solid

        let style2 = LayerStyle::new()
            .with_fill(Fill::solid(1.0, 0.0, 0.0, 1.0));
        let c = style2.primary_color().unwrap();
        assert!((c.r - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_blend_mode_default() {
        assert_eq!(BlendMode::default(), BlendMode::Normal);
    }

    #[test]
    fn test_radial_gradient() {
        let g = Gradient::radial(Color::WHITE, Color::BLACK);
        let center = g.sample(0.0);
        assert!((center.r - 1.0).abs() < 0.01); // white at center
        let edge = g.sample(1.0);
        assert!((edge.r).abs() < 0.01); // black at edge
    }

    #[test]
    fn test_srgb_to_linear_extremes() {
        assert!((srgb_to_linear(0)).abs() < 0.001);
        assert!((srgb_to_linear(255) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_color_lerp_clamped() {
        let a = Color::BLACK;
        let b = Color::WHITE;
        let over = a.lerp(b, 2.0);
        assert!((over.r - 1.0).abs() < 0.001); // clamped to 1.0
        let under = a.lerp(b, -1.0);
        assert!((under.r).abs() < 0.001); // clamped to 0.0
    }
}
