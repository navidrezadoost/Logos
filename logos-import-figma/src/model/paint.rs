//! Paint / fill / stroke types for the Figma model.

use super::transform::Vector2D;
use serde::{Deserialize, Serialize};

/// A color in RGBA (0.0–1.0 per channel).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub fn black() -> Self {
        Self::new(0.0, 0.0, 0.0, 1.0)
    }

    pub fn white() -> Self {
        Self::new(1.0, 1.0, 1.0, 1.0)
    }

    pub fn transparent() -> Self {
        Self::new(0.0, 0.0, 0.0, 0.0)
    }

    /// Convert to 0–255 RGBA bytes.
    pub fn to_rgba8(&self) -> [u8; 4] {
        [
            (self.r * 255.0).clamp(0.0, 255.0) as u8,
            (self.g * 255.0).clamp(0.0, 255.0) as u8,
            (self.b * 255.0).clamp(0.0, 255.0) as u8,
            (self.a * 255.0).clamp(0.0, 255.0) as u8,
        ]
    }

    /// Create from 0–255 RGBA bytes.
    pub fn from_rgba8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: a as f32 / 255.0,
        }
    }

    /// Create from hex string (e.g. "#FF0000" or "FF0000FF").
    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.trim_start_matches('#');
        let bytes: Vec<u8> = (0..hex.len())
            .step_by(2)
            .filter_map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
            .collect();

        match bytes.len() {
            3 => Some(Self::from_rgba8(bytes[0], bytes[1], bytes[2], 255)),
            4 => Some(Self::from_rgba8(bytes[0], bytes[1], bytes[2], bytes[3])),
            _ => None,
        }
    }
}

impl Default for Color {
    fn default() -> Self {
        Self::black()
    }
}

/// A gradient stop (position + color).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GradientStop {
    /// Position along the gradient (0.0–1.0).
    pub position: f32,
    /// Color at this stop.
    pub color: Color,
}

/// The type of paint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PaintType {
    /// A single solid color.
    Solid,
    /// A linear gradient.
    LinearGradient,
    /// A radial gradient.
    RadialGradient,
    /// An angular (sweep) gradient.
    AngularGradient,
    /// A diamond (four-point) gradient.
    DiamondGradient,
    /// An image fill.
    Image,
}

/// Image scaling mode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ScaleMode {
    Fill,
    Fit,
    Tile,
    Stretch,
}

impl Default for ScaleMode {
    fn default() -> Self {
        Self::Fill
    }
}

/// A paint (fill or stroke definition).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Paint {
    /// The type of paint.
    pub paint_type: PaintType,
    /// Whether this paint is visible/active.
    pub visible: bool,
    /// Global opacity for this paint (0.0–1.0).
    pub opacity: f32,
    /// Color (for Solid type).
    pub color: Option<Color>,
    /// Gradient stops (for gradient types).
    pub gradient_stops: Vec<GradientStop>,
    /// Gradient handle positions (for gradient types).
    /// Typically 3 points: start, end, and width handle.
    pub gradient_handles: Vec<Vector2D>,
    /// Image reference hash (for Image type).
    pub image_ref: Option<String>,
    /// Image scaling mode.
    pub scale_mode: ScaleMode,
}

impl Paint {
    /// Create a solid color paint.
    pub fn solid(color: Color) -> Self {
        Self {
            paint_type: PaintType::Solid,
            visible: true,
            opacity: 1.0,
            color: Some(color),
            gradient_stops: Vec::new(),
            gradient_handles: Vec::new(),
            image_ref: None,
            scale_mode: ScaleMode::Fill,
        }
    }

    /// Create a linear gradient paint.
    pub fn linear_gradient(stops: Vec<GradientStop>, start: Vector2D, end: Vector2D) -> Self {
        Self {
            paint_type: PaintType::LinearGradient,
            visible: true,
            opacity: 1.0,
            color: None,
            gradient_stops: stops,
            gradient_handles: vec![start, end],
            image_ref: None,
            scale_mode: ScaleMode::Fill,
        }
    }

    /// Create an image paint.
    pub fn image(image_ref: String, scale_mode: ScaleMode) -> Self {
        Self {
            paint_type: PaintType::Image,
            visible: true,
            opacity: 1.0,
            color: None,
            gradient_stops: Vec::new(),
            gradient_handles: Vec::new(),
            image_ref: Some(image_ref),
            scale_mode,
        }
    }
}

/// Stroke alignment relative to the path.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum StrokeAlign {
    Inside,
    Outside,
    Center,
}

impl Default for StrokeAlign {
    fn default() -> Self {
        Self::Center
    }
}

/// Stroke cap type.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum StrokeCap {
    None,
    Round,
    Square,
    ArrowLines,
    ArrowEquilateral,
}

impl Default for StrokeCap {
    fn default() -> Self {
        Self::None
    }
}

/// Stroke join type.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum StrokeJoin {
    Miter,
    Bevel,
    Round,
}

impl Default for StrokeJoin {
    fn default() -> Self {
        Self::Miter
    }
}

/// Blend mode for compositing.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum BlendMode {
    PassThrough,
    Normal,
    Darken,
    Multiply,
    LinearBurn,
    ColorBurn,
    Lighten,
    Screen,
    LinearDodge,
    ColorDodge,
    Overlay,
    SoftLight,
    HardLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    ColorBlend,
    Luminosity,
}

impl Default for BlendMode {
    fn default() -> Self {
        Self::Normal
    }
}

impl BlendMode {
    /// Decode from the Figma type ID.
    pub fn from_figma_id(id: u64) -> Self {
        match id {
            0 => Self::PassThrough,
            1 => Self::Normal,
            2 => Self::Darken,
            3 => Self::Multiply,
            4 => Self::LinearBurn,
            5 => Self::ColorBurn,
            6 => Self::Lighten,
            7 => Self::Screen,
            8 => Self::LinearDodge,
            9 => Self::ColorDodge,
            10 => Self::Overlay,
            11 => Self::SoftLight,
            12 => Self::HardLight,
            13 => Self::Difference,
            14 => Self::Exclusion,
            15 => Self::Hue,
            16 => Self::Saturation,
            17 => Self::ColorBlend,
            18 => Self::Luminosity,
            _ => Self::Normal,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_black() {
        let c = Color::black();
        assert_eq!(c.to_rgba8(), [0, 0, 0, 255]);
    }

    #[test]
    fn test_color_white() {
        let c = Color::white();
        assert_eq!(c.to_rgba8(), [255, 255, 255, 255]);
    }

    #[test]
    fn test_color_from_rgba8() {
        let c = Color::from_rgba8(128, 64, 32, 200);
        assert!((c.r - 128.0 / 255.0).abs() < 0.01);
        assert!((c.a - 200.0 / 255.0).abs() < 0.01);
    }

    #[test]
    fn test_color_from_hex() {
        let c = Color::from_hex("#FF0000").unwrap();
        assert!((c.r - 1.0).abs() < 0.01);
        assert!(c.g.abs() < 0.01);
        assert!(c.b.abs() < 0.01);
    }

    #[test]
    fn test_color_from_hex_with_alpha() {
        let c = Color::from_hex("FF000080").unwrap();
        assert!((c.r - 1.0).abs() < 0.01);
        assert!((c.a - 128.0 / 255.0).abs() < 0.01);
    }

    #[test]
    fn test_color_from_hex_invalid() {
        assert!(Color::from_hex("XY").is_none());
    }

    #[test]
    fn test_paint_solid() {
        let p = Paint::solid(Color::black());
        assert_eq!(p.paint_type, PaintType::Solid);
        assert!(p.visible);
        assert_eq!(p.color, Some(Color::black()));
    }

    #[test]
    fn test_paint_linear_gradient() {
        let stops = vec![
            GradientStop {
                position: 0.0,
                color: Color::black(),
            },
            GradientStop {
                position: 1.0,
                color: Color::white(),
            },
        ];
        let p = Paint::linear_gradient(
            stops,
            Vector2D::new(0.0, 0.0),
            Vector2D::new(1.0, 0.0),
        );
        assert_eq!(p.paint_type, PaintType::LinearGradient);
        assert_eq!(p.gradient_stops.len(), 2);
    }

    #[test]
    fn test_paint_image() {
        let p = Paint::image("abc123".into(), ScaleMode::Fit);
        assert_eq!(p.paint_type, PaintType::Image);
        assert_eq!(p.image_ref.as_deref(), Some("abc123"));
        assert_eq!(p.scale_mode, ScaleMode::Fit);
    }

    #[test]
    fn test_blend_mode_from_id() {
        assert_eq!(BlendMode::from_figma_id(0), BlendMode::PassThrough);
        assert_eq!(BlendMode::from_figma_id(3), BlendMode::Multiply);
        assert_eq!(BlendMode::from_figma_id(7), BlendMode::Screen);
        assert_eq!(BlendMode::from_figma_id(255), BlendMode::Normal);
    }

    #[test]
    fn test_stroke_align_default() {
        assert_eq!(StrokeAlign::default(), StrokeAlign::Center);
    }
}
