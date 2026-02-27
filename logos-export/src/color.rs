//! Color management — color space conversion and profiles.
//!
//! Handles color representation across different output targets:
//! - sRGB for web/screen display
//! - Display P3 for wide-gamut screens
//! - CMYK for print workflows
//! - Grayscale for monochrome output
//!
//! References:
//! - IEC 61966-2-1 (sRGB standard)
//! - ICC Profile specification

use serde::{Deserialize, Serialize};

/// Supported color spaces for export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ColorSpace {
    /// Standard RGB (IEC 61966-2-1), the default for web.
    Srgb,
    /// Display P3 — wider gamut, used by Apple devices.
    DisplayP3,
    /// Cyan/Magenta/Yellow/Key(Black) — for print.
    Cmyk,
    /// Single-channel grayscale.
    Grayscale,
}

impl ColorSpace {
    /// Human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Srgb => "sRGB",
            Self::DisplayP3 => "Display P3",
            Self::Cmyk => "CMYK",
            Self::Grayscale => "Grayscale",
        }
    }

    /// Whether this color space is suitable for screen display.
    pub fn is_screen(&self) -> bool {
        matches!(self, Self::Srgb | Self::DisplayP3)
    }

    /// Whether this color space is suitable for print.
    pub fn is_print(&self) -> bool {
        matches!(self, Self::Cmyk | Self::Grayscale)
    }

    /// Number of channels (excluding alpha).
    pub fn channels(&self) -> usize {
        match self {
            Self::Srgb | Self::DisplayP3 => 3,
            Self::Cmyk => 4,
            Self::Grayscale => 1,
        }
    }
}

impl Default for ColorSpace {
    fn default() -> Self {
        Self::Srgb
    }
}

/// An RGBA color value in linear float [0.0, 1.0].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const WHITE: Color = Color { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };
    pub const BLACK: Color = Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };
    pub const TRANSPARENT: Color = Color { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };

    pub fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self {
            r: r.clamp(0.0, 1.0),
            g: g.clamp(0.0, 1.0),
            b: b.clamp(0.0, 1.0),
            a: a.clamp(0.0, 1.0),
        }
    }

    pub fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self::new(r, g, b, 1.0)
    }

    /// Create from 8-bit RGBA values (0–255).
    pub fn from_u8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self::new(
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            a as f32 / 255.0,
        )
    }

    /// Convert to 8-bit RGBA.
    pub fn to_u8(&self) -> [u8; 4] {
        [
            (self.r * 255.0).round() as u8,
            (self.g * 255.0).round() as u8,
            (self.b * 255.0).round() as u8,
            (self.a * 255.0).round() as u8,
        ]
    }

    /// Convert to CSS hex string.
    pub fn to_css_hex(&self) -> String {
        let [r, g, b, a] = self.to_u8();
        if a == 255 {
            format!("#{r:02x}{g:02x}{b:02x}")
        } else {
            format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
        }
    }

    /// Parse from CSS hex (#rgb, #rrggbb, #rrggbbaa).
    pub fn from_css_hex(hex: &str) -> Option<Self> {
        let hex = hex.trim_start_matches('#');
        match hex.len() {
            3 => {
                let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
                let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
                let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
                Some(Self::from_u8(r, g, b, 255))
            }
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                Some(Self::from_u8(r, g, b, 255))
            }
            8 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
                Some(Self::from_u8(r, g, b, a))
            }
            _ => None,
        }
    }

    /// Convert sRGB to grayscale using ITU-R BT.709 luminance.
    pub fn to_grayscale(&self) -> f32 {
        0.2126 * self.r + 0.7152 * self.g + 0.0722 * self.b
    }

    /// Convert sRGB to approximate CMYK.
    ///
    /// Uses the simple UCR (Under Color Removal) method:
    /// K = 1 - max(R, G, B); C = (1-R-K)/(1-K); etc.
    pub fn to_cmyk(&self) -> [f32; 4] {
        let k = 1.0 - self.r.max(self.g).max(self.b);
        if k >= 1.0 {
            return [0.0, 0.0, 0.0, 1.0];
        }
        let inv_k = 1.0 / (1.0 - k);
        let c = (1.0 - self.r - k) * inv_k;
        let m = (1.0 - self.g - k) * inv_k;
        let y = (1.0 - self.b - k) * inv_k;
        [c.clamp(0.0, 1.0), m.clamp(0.0, 1.0), y.clamp(0.0, 1.0), k]
    }

    /// Convert from CMYK to sRGB.
    pub fn from_cmyk(c: f32, m: f32, y: f32, k: f32) -> Self {
        let r = (1.0 - c) * (1.0 - k);
        let g = (1.0 - m) * (1.0 - k);
        let b = (1.0 - y) * (1.0 - k);
        Self::rgb(r, g, b)
    }

    /// Apply sRGB gamma encoding (linear → sRGB).
    pub fn srgb_encode(&self) -> Self {
        Self::new(
            linear_to_srgb(self.r),
            linear_to_srgb(self.g),
            linear_to_srgb(self.b),
            self.a,
        )
    }

    /// Apply sRGB gamma decoding (sRGB → linear).
    pub fn srgb_decode(&self) -> Self {
        Self::new(
            srgb_to_linear(self.r),
            srgb_to_linear(self.g),
            srgb_to_linear(self.b),
            self.a,
        )
    }

    /// Pre-multiply alpha.
    pub fn premultiply(&self) -> Self {
        Self {
            r: self.r * self.a,
            g: self.g * self.a,
            b: self.b * self.a,
            a: self.a,
        }
    }

    /// Alpha-blend `self` over `dst` (Porter-Duff "over").
    pub fn blend_over(&self, dst: &Color) -> Color {
        let sa = self.a;
        let da = dst.a * (1.0 - sa);
        let oa = sa + da;
        if oa < 1e-6 {
            return Color::TRANSPARENT;
        }
        Color::new(
            (self.r * sa + dst.r * da) / oa,
            (self.g * sa + dst.g * da) / oa,
            (self.b * sa + dst.b * da) / oa,
            oa,
        )
    }
}

impl From<[f32; 4]> for Color {
    fn from(c: [f32; 4]) -> Self {
        Self::new(c[0], c[1], c[2], c[3])
    }
}

impl From<Color> for [f32; 4] {
    fn from(c: Color) -> Self {
        [c.r, c.g, c.b, c.a]
    }
}

/// An export color profile — wraps color space + optional ICC metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorProfile {
    pub space: ColorSpace,
    pub name: String,
    /// Optional rendering intent for ICC.
    pub rendering_intent: RenderingIntent,
}

impl ColorProfile {
    pub fn srgb() -> Self {
        Self {
            space: ColorSpace::Srgb,
            name: "sRGB IEC61966-2.1".to_string(),
            rendering_intent: RenderingIntent::Perceptual,
        }
    }

    pub fn display_p3() -> Self {
        Self {
            space: ColorSpace::DisplayP3,
            name: "Display P3".to_string(),
            rendering_intent: RenderingIntent::Perceptual,
        }
    }

    pub fn cmyk_default() -> Self {
        Self {
            space: ColorSpace::Cmyk,
            name: "Generic CMYK".to_string(),
            rendering_intent: RenderingIntent::RelativeColorimetric,
        }
    }
}

/// ICC rendering intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenderingIntent {
    Perceptual,
    RelativeColorimetric,
    Saturation,
    AbsoluteColorimetric,
}

impl Default for RenderingIntent {
    fn default() -> Self {
        Self::Perceptual
    }
}

// ── Gamma functions ─────────────────────────────────────────────────

/// sRGB companding: linear → sRGB gamma.
fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// sRGB inverse companding: sRGB gamma → linear.
fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

// ═══════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_space_properties() {
        assert!(ColorSpace::Srgb.is_screen());
        assert!(!ColorSpace::Srgb.is_print());
        assert!(ColorSpace::Cmyk.is_print());
        assert_eq!(ColorSpace::Srgb.channels(), 3);
        assert_eq!(ColorSpace::Cmyk.channels(), 4);
        assert_eq!(ColorSpace::Grayscale.channels(), 1);
    }

    #[test]
    fn color_from_u8_roundtrip() {
        let c = Color::from_u8(128, 64, 32, 255);
        let [r, g, b, a] = c.to_u8();
        assert_eq!(r, 128);
        assert_eq!(g, 64);
        assert_eq!(b, 32);
        assert_eq!(a, 255);
    }

    #[test]
    fn color_css_hex_roundtrip() {
        let c = Color::rgb(1.0, 0.0, 0.0);
        assert_eq!(c.to_css_hex(), "#ff0000");

        let parsed = Color::from_css_hex("#ff0000").unwrap();
        assert!((parsed.r - 1.0).abs() < 0.01);
    }

    #[test]
    fn color_css_hex_short() {
        let c = Color::from_css_hex("#f0a").unwrap();
        assert_eq!(c.to_u8()[0], 255);
        assert_eq!(c.to_u8()[1], 0);
        assert_eq!(c.to_u8()[2], 170);
    }

    #[test]
    fn color_css_hex_with_alpha() {
        let c = Color::from_css_hex("#ff000080").unwrap();
        assert!((c.a - 0.502).abs() < 0.01);
        assert!(c.to_css_hex().contains("80"));
    }

    #[test]
    fn color_css_hex_invalid() {
        assert!(Color::from_css_hex("xyz").is_none());
        assert!(Color::from_css_hex("#12345").is_none());
    }

    #[test]
    fn color_grayscale() {
        let white = Color::WHITE;
        assert!((white.to_grayscale() - 1.0).abs() < 0.001);

        let black = Color::BLACK;
        assert!((black.to_grayscale() - 0.0).abs() < 0.001);

        // Pure green should be brightest contributor
        let green = Color::rgb(0.0, 1.0, 0.0);
        assert!(green.to_grayscale() > 0.7);
    }

    #[test]
    fn color_cmyk_roundtrip() {
        let c = Color::rgb(0.5, 0.3, 0.2);
        let [cy, m, y, k] = c.to_cmyk();
        let back = Color::from_cmyk(cy, m, y, k);
        assert!((back.r - c.r).abs() < 0.01);
        assert!((back.g - c.g).abs() < 0.01);
        assert!((back.b - c.b).abs() < 0.01);
    }

    #[test]
    fn color_cmyk_black() {
        let [c, _m, _y, k] = Color::BLACK.to_cmyk();
        assert!((k - 1.0).abs() < 0.001);
        assert!((c - 0.0).abs() < 0.001);
    }

    #[test]
    fn color_srgb_gamma_roundtrip() {
        let original = Color::rgb(0.5, 0.3, 0.8);
        let encoded = original.srgb_encode();
        let decoded = encoded.srgb_decode();
        assert!((decoded.r - original.r).abs() < 0.001);
        assert!((decoded.g - original.g).abs() < 0.001);
        assert!((decoded.b - original.b).abs() < 0.001);
    }

    #[test]
    fn color_premultiply() {
        let c = Color::new(1.0, 0.5, 0.0, 0.5);
        let pm = c.premultiply();
        assert!((pm.r - 0.5).abs() < 0.001);
        assert!((pm.g - 0.25).abs() < 0.001);
        assert!((pm.a - 0.5).abs() < 0.001);
    }

    #[test]
    fn color_blend_over() {
        let fg = Color::new(1.0, 0.0, 0.0, 0.5);
        let bg = Color::new(0.0, 0.0, 1.0, 1.0);
        let result = fg.blend_over(&bg);
        // Red over blue at 50% alpha
        assert!(result.r > 0.3);
        assert!(result.b > 0.3);
        assert!((result.a - 1.0).abs() < 0.001);
    }

    #[test]
    fn color_blend_transparent_over() {
        let fg = Color::TRANSPARENT;
        let bg = Color::WHITE;
        let result = fg.blend_over(&bg);
        assert!((result.r - 1.0).abs() < 0.001);
    }

    #[test]
    fn color_profile_srgb() {
        let p = ColorProfile::srgb();
        assert_eq!(p.space, ColorSpace::Srgb);
        assert!(p.name.contains("sRGB"));
    }

    #[test]
    fn color_from_array() {
        let c: Color = [0.5, 0.3, 0.1, 1.0].into();
        let arr: [f32; 4] = c.into();
        assert!((arr[0] - 0.5).abs() < 0.001);
    }
}
