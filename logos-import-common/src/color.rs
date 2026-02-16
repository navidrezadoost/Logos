//! Color types shared across importers.

use serde::{Deserialize, Serialize};

/// A color in RGBA floating-point format (0.0–1.0 per channel).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Color4f {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color4f {
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

    /// Create from 8-bit RGBA (0–255).
    pub fn from_rgba8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: a as f32 / 255.0,
        }
    }

    /// Convert to 8-bit RGBA tuple.
    pub fn to_rgba8(self) -> (u8, u8, u8, u8) {
        (
            (self.r * 255.0).round() as u8,
            (self.g * 255.0).round() as u8,
            (self.b * 255.0).round() as u8,
            (self.a * 255.0).round() as u8,
        )
    }

    /// Create from a hex string (`"#RRGGBB"` or `"#RRGGBBAA"`).
    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.trim_start_matches('#');
        match hex.len() {
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                Some(Self::from_rgba8(r, g, b, 255))
            }
            8 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
                Some(Self::from_rgba8(r, g, b, a))
            }
            _ => None,
        }
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

    /// Pre-multiply alpha into RGB channels.
    pub fn premultiply(self) -> Self {
        Self {
            r: self.r * self.a,
            g: self.g * self.a,
            b: self.b * self.a,
            a: self.a,
        }
    }
}

impl Default for Color4f {
    fn default() -> Self {
        Self::black()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_black_white() {
        let b = Color4f::black();
        assert_eq!(b.to_rgba8(), (0, 0, 0, 255));
        let w = Color4f::white();
        assert_eq!(w.to_rgba8(), (255, 255, 255, 255));
    }

    #[test]
    fn test_color_from_rgba8() {
        let c = Color4f::from_rgba8(128, 64, 32, 200);
        let (r, g, b, a) = c.to_rgba8();
        assert_eq!(r, 128);
        assert_eq!(g, 64);
        assert_eq!(b, 32);
        assert_eq!(a, 200);
    }

    #[test]
    fn test_color_from_hex() {
        let c = Color4f::from_hex("#FF8800").unwrap();
        assert_eq!(c.to_rgba8(), (255, 136, 0, 255));
    }

    #[test]
    fn test_color_from_hex_with_alpha() {
        let c = Color4f::from_hex("#FF880080").unwrap();
        assert_eq!(c.to_rgba8(), (255, 136, 0, 128));
    }

    #[test]
    fn test_color_from_hex_invalid() {
        assert!(Color4f::from_hex("#ZZ").is_none());
        assert!(Color4f::from_hex("#12345").is_none());
    }

    #[test]
    fn test_color_lerp() {
        let a = Color4f::black();
        let b = Color4f::white();
        let mid = a.lerp(b, 0.5);
        assert!((mid.r - 0.5).abs() < 0.01);
        assert!((mid.g - 0.5).abs() < 0.01);
        assert!((mid.b - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_color_premultiply() {
        let c = Color4f::new(1.0, 0.5, 0.0, 0.5);
        let pm = c.premultiply();
        assert!((pm.r - 0.5).abs() < 0.001);
        assert!((pm.g - 0.25).abs() < 0.001);
        assert!((pm.a - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_color_transparent() {
        let t = Color4f::transparent();
        assert_eq!(t.a, 0.0);
    }
}
