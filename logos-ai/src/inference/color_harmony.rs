//! # Color Harmony Engine
//!
//! Generates and evaluates color palettes using classical color theory:
//! complementary, analogous, triadic, split-complementary, and tetradic
//! schemes. Also provides palette scoring and color temperature analysis.
//!
//! All math operates in HSL space (converted from the `Color` RGB values
//! stored in logos-core).
//!
//! ```
//! use logos_ai::inference::color_harmony::{HarmonyScheme, PaletteGenerator, HslColor};
//!
//! let base = HslColor::new(0.0, 0.8, 0.5); // Red
//! let palette = PaletteGenerator::generate(base, HarmonyScheme::Complementary);
//! assert_eq!(palette.colors.len(), 2);
//! ```

use logos_core::style::Color;

// ── HSL Color ────────────────────────────────────────────────

/// Color represented in HSL (Hue-Saturation-Lightness) space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HslColor {
    /// Hue in degrees [0, 360).
    pub h: f32,
    /// Saturation [0, 1].
    pub s: f32,
    /// Lightness [0, 1].
    pub l: f32,
}

impl HslColor {
    /// Create a new HSL color.
    pub fn new(h: f32, s: f32, l: f32) -> Self {
        Self {
            h: h.rem_euclid(360.0),
            s: s.clamp(0.0, 1.0),
            l: l.clamp(0.0, 1.0),
        }
    }

    /// Convert from logos_core::Color (sRGB).
    pub fn from_rgb(c: Color) -> Self {
        let (r, g, b) = (c.r, c.g, c.b);
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let l = (max + min) / 2.0;

        if (max - min).abs() < 1e-6 {
            return Self::new(0.0, 0.0, l);
        }

        let d = max - min;
        let s = if l > 0.5 {
            d / (2.0 - max - min)
        } else {
            d / (max + min)
        };

        let h = if (max - r).abs() < 1e-6 {
            let mut h = (g - b) / d;
            if g < b { h += 6.0; }
            h
        } else if (max - g).abs() < 1e-6 {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        };

        Self::new(h * 60.0, s, l)
    }

    /// Convert to logos_core::Color (sRGB).
    pub fn to_rgb(&self) -> Color {
        if self.s.abs() < 1e-6 {
            return Color { r: self.l, g: self.l, b: self.l, a: 1.0 };
        }

        let q = if self.l < 0.5 {
            self.l * (1.0 + self.s)
        } else {
            self.l + self.s - self.l * self.s
        };
        let p = 2.0 * self.l - q;
        let h = self.h / 360.0;

        Color {
            r: hue_to_rgb(p, q, h + 1.0 / 3.0),
            g: hue_to_rgb(p, q, h),
            b: hue_to_rgb(p, q, h - 1.0 / 3.0),
            a: 1.0,
        }
    }

    /// Rotate hue by `degrees`.
    pub fn rotate(&self, degrees: f32) -> Self {
        Self::new(self.h + degrees, self.s, self.l)
    }

    /// Adjust saturation by factor.
    pub fn saturate(&self, factor: f32) -> Self {
        Self::new(self.h, self.s * factor, self.l)
    }

    /// Adjust lightness by factor.
    pub fn lighten(&self, factor: f32) -> Self {
        Self::new(self.h, self.s, self.l * factor)
    }

    /// Angular distance to another hue (0-180).
    pub fn hue_distance(&self, other: &Self) -> f32 {
        let diff = (self.h - other.h).abs();
        if diff > 180.0 { 360.0 - diff } else { diff }
    }
}

fn hue_to_rgb(p: f32, q: f32, mut t: f32) -> f32 {
    if t < 0.0 { t += 1.0; }
    if t > 1.0 { t -= 1.0; }
    if t < 1.0 / 6.0 {
        return p + (q - p) * 6.0 * t;
    }
    if t < 1.0 / 2.0 {
        return q;
    }
    if t < 2.0 / 3.0 {
        return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
    }
    p
}

// ── Harmony Schemes ──────────────────────────────────────────

/// Classical color harmony scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HarmonyScheme {
    /// Base + opposite (180°).
    Complementary,
    /// Base + adjacent colors (±30°).
    Analogous,
    /// Three equally spaced colors (120° apart).
    Triadic,
    /// Base + two colors adjacent to the complement (±150°).
    SplitComplementary,
    /// Two complementary pairs (90° apart).
    Tetradic,
    /// Five equally spaced (72° apart).
    Pentadic,
}

impl HarmonyScheme {
    /// Hue offsets for each scheme.
    pub fn offsets(&self) -> Vec<f32> {
        match self {
            Self::Complementary => vec![0.0, 180.0],
            Self::Analogous => vec![-30.0, 0.0, 30.0],
            Self::Triadic => vec![0.0, 120.0, 240.0],
            Self::SplitComplementary => vec![0.0, 150.0, 210.0],
            Self::Tetradic => vec![0.0, 90.0, 180.0, 270.0],
            Self::Pentadic => vec![0.0, 72.0, 144.0, 216.0, 288.0],
        }
    }

    /// Number of colors in the palette.
    pub fn palette_size(&self) -> usize {
        self.offsets().len()
    }
}

// ── Palette ──────────────────────────────────────────────────

/// A generated color palette.
#[derive(Debug, Clone)]
pub struct Palette {
    /// Colors in HSL space.
    pub colors: Vec<HslColor>,
    /// Which scheme generated this palette.
    pub scheme: HarmonyScheme,
    /// Base color.
    pub base: HslColor,
}

impl Palette {
    /// Convert all colors to RGB.
    pub fn to_rgb(&self) -> Vec<Color> {
        self.colors.iter().map(|c| c.to_rgb()).collect()
    }

    /// Evaluate visual harmony (0-1). Higher = more harmonious.
    pub fn harmony_score(&self) -> f32 {
        if self.colors.len() < 2 {
            return 1.0;
        }

        // Check angular distribution
        let expected_offsets = self.scheme.offsets();
        let mut total_deviation = 0.0;
        for (i, offset) in expected_offsets.iter().enumerate() {
            if i >= self.colors.len() { break; }
            let expected_hue = (self.base.h + offset).rem_euclid(360.0);
            let actual_hue = self.colors[i].h;
            let diff = (expected_hue - actual_hue).abs();
            let angular_diff = if diff > 180.0 { 360.0 - diff } else { diff };
            total_deviation += angular_diff;
        }

        let max_deviation = 180.0 * self.colors.len() as f32;
        1.0 - (total_deviation / max_deviation).min(1.0)
    }

    /// Average saturation.
    pub fn avg_saturation(&self) -> f32 {
        if self.colors.is_empty() { return 0.0; }
        self.colors.iter().map(|c| c.s).sum::<f32>() / self.colors.len() as f32
    }

    /// Average lightness.
    pub fn avg_lightness(&self) -> f32 {
        if self.colors.is_empty() { return 0.0; }
        self.colors.iter().map(|c| c.l).sum::<f32>() / self.colors.len() as f32
    }
}

// ── Color Temperature ────────────────────────────────────────

/// Perceived color temperature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorTemperature {
    Cool,
    Neutral,
    Warm,
}

/// Classify a color's temperature based on hue.
pub fn classify_temperature(hsl: HslColor) -> ColorTemperature {
    // Warm: red, orange, yellow (0-60° and 300-360°)
    // Cool: green, blue, purple (120-270°)
    let h = hsl.h;
    if h < 60.0 || h > 300.0 {
        ColorTemperature::Warm
    } else if (60.0..=120.0).contains(&h) || (270.0..=300.0).contains(&h) {
        ColorTemperature::Neutral
    } else {
        ColorTemperature::Cool
    }
}

// ── Generator ────────────────────────────────────────────────

/// Palette generator.
pub struct PaletteGenerator;

impl PaletteGenerator {
    /// Generate a palette from a base color and scheme.
    pub fn generate(base: HslColor, scheme: HarmonyScheme) -> Palette {
        let colors: Vec<HslColor> = scheme
            .offsets()
            .iter()
            .map(|&offset| base.rotate(offset))
            .collect();

        Palette {
            colors,
            scheme,
            base,
        }
    }

    /// Generate with lightness/saturation variations.
    pub fn generate_with_variations(
        base: HslColor,
        scheme: HarmonyScheme,
        light_variations: &[f32],
    ) -> Vec<Palette> {
        light_variations
            .iter()
            .map(|&factor| {
                let varied_base = base.lighten(factor);
                Self::generate(varied_base, scheme)
            })
            .collect()
    }

    /// Score how harmonious two colors are (0-1).
    pub fn pair_harmony(a: HslColor, b: HslColor) -> f32 {
        let dist = a.hue_distance(&b);
        // Best harmony at canonical angles
        let ideal_angles = [0.0, 30.0, 60.0, 120.0, 150.0, 180.0];
        let min_deviation = ideal_angles
            .iter()
            .map(|&angle| (dist - angle).abs())
            .fold(f32::MAX, f32::min);
        1.0 - (min_deviation / 90.0).min(1.0)
    }
}

// ── Tests ────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() < tol
    }

    // ── HSL Conversions ──────────────────────────────────────

    #[test]
    fn rgb_to_hsl_red() {
        let red = Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 };
        let hsl = HslColor::from_rgb(red);
        assert!(approx_eq(hsl.h, 0.0, 1.0));
        assert!(approx_eq(hsl.s, 1.0, 0.01));
        assert!(approx_eq(hsl.l, 0.5, 0.01));
    }

    #[test]
    fn rgb_to_hsl_green() {
        let green = Color { r: 0.0, g: 1.0, b: 0.0, a: 1.0 };
        let hsl = HslColor::from_rgb(green);
        assert!(approx_eq(hsl.h, 120.0, 1.0));
    }

    #[test]
    fn rgb_to_hsl_blue() {
        let blue = Color { r: 0.0, g: 0.0, b: 1.0, a: 1.0 };
        let hsl = HslColor::from_rgb(blue);
        assert!(approx_eq(hsl.h, 240.0, 1.0));
    }

    #[test]
    fn rgb_to_hsl_white() {
        let white = Color { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };
        let hsl = HslColor::from_rgb(white);
        assert!(approx_eq(hsl.s, 0.0, 0.01));
        assert!(approx_eq(hsl.l, 1.0, 0.01));
    }

    #[test]
    fn rgb_to_hsl_black() {
        let black = Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };
        let hsl = HslColor::from_rgb(black);
        assert!(approx_eq(hsl.l, 0.0, 0.01));
    }

    #[test]
    fn hsl_roundtrip() {
        let original = Color { r: 0.8, g: 0.3, b: 0.5, a: 1.0 };
        let hsl = HslColor::from_rgb(original);
        let back = hsl.to_rgb();
        assert!(approx_eq(original.r, back.r, 0.02));
        assert!(approx_eq(original.g, back.g, 0.02));
        assert!(approx_eq(original.b, back.b, 0.02));
    }

    #[test]
    fn hsl_gray_roundtrip() {
        let hsl = HslColor::new(0.0, 0.0, 0.5);
        let rgb = hsl.to_rgb();
        assert!(approx_eq(rgb.r, 0.5, 0.01));
        assert!(approx_eq(rgb.g, 0.5, 0.01));
        assert!(approx_eq(rgb.b, 0.5, 0.01));
    }

    // ── Hue Operations ───────────────────────────────────────

    #[test]
    fn rotate_hue() {
        let c = HslColor::new(30.0, 0.8, 0.5);
        let rotated = c.rotate(180.0);
        assert!(approx_eq(rotated.h, 210.0, 0.01));
    }

    #[test]
    fn rotate_wraps() {
        let c = HslColor::new(350.0, 0.8, 0.5);
        let rotated = c.rotate(30.0);
        assert!(approx_eq(rotated.h, 20.0, 0.01));
    }

    #[test]
    fn hue_distance_simple() {
        let a = HslColor::new(10.0, 0.8, 0.5);
        let b = HslColor::new(50.0, 0.8, 0.5);
        assert!(approx_eq(a.hue_distance(&b), 40.0, 0.01));
    }

    #[test]
    fn hue_distance_wraps() {
        let a = HslColor::new(10.0, 0.8, 0.5);
        let b = HslColor::new(350.0, 0.8, 0.5);
        assert!(approx_eq(a.hue_distance(&b), 20.0, 0.01));
    }

    // ── Harmony Schemes ──────────────────────────────────────

    #[test]
    fn complementary_palette() {
        let base = HslColor::new(0.0, 0.8, 0.5);
        let palette = PaletteGenerator::generate(base, HarmonyScheme::Complementary);
        assert_eq!(palette.colors.len(), 2);
        assert!(approx_eq(palette.colors[1].h, 180.0, 0.01));
    }

    #[test]
    fn analogous_palette() {
        let base = HslColor::new(120.0, 0.7, 0.5);
        let palette = PaletteGenerator::generate(base, HarmonyScheme::Analogous);
        assert_eq!(palette.colors.len(), 3);
        assert!(approx_eq(palette.colors[0].h, 90.0, 0.01));
        assert!(approx_eq(palette.colors[1].h, 120.0, 0.01));
        assert!(approx_eq(palette.colors[2].h, 150.0, 0.01));
    }

    #[test]
    fn triadic_palette() {
        let base = HslColor::new(0.0, 0.8, 0.5);
        let palette = PaletteGenerator::generate(base, HarmonyScheme::Triadic);
        assert_eq!(palette.colors.len(), 3);
        assert!(approx_eq(palette.colors[1].h, 120.0, 0.01));
        assert!(approx_eq(palette.colors[2].h, 240.0, 0.01));
    }

    #[test]
    fn tetradic_palette() {
        let base = HslColor::new(45.0, 0.8, 0.5);
        let palette = PaletteGenerator::generate(base, HarmonyScheme::Tetradic);
        assert_eq!(palette.colors.len(), 4);
    }

    #[test]
    fn pentadic_palette_size() {
        let base = HslColor::new(0.0, 0.8, 0.5);
        let palette = PaletteGenerator::generate(base, HarmonyScheme::Pentadic);
        assert_eq!(palette.colors.len(), 5);
    }

    #[test]
    fn scheme_palette_size() {
        assert_eq!(HarmonyScheme::Complementary.palette_size(), 2);
        assert_eq!(HarmonyScheme::Triadic.palette_size(), 3);
        assert_eq!(HarmonyScheme::Tetradic.palette_size(), 4);
        assert_eq!(HarmonyScheme::Pentadic.palette_size(), 5);
    }

    // ── Palette Analysis ─────────────────────────────────────

    #[test]
    fn harmony_score_perfect() {
        let base = HslColor::new(0.0, 0.8, 0.5);
        let palette = PaletteGenerator::generate(base, HarmonyScheme::Complementary);
        assert!(palette.harmony_score() > 0.95);
    }

    #[test]
    fn avg_saturation() {
        let base = HslColor::new(0.0, 0.8, 0.5);
        let palette = PaletteGenerator::generate(base, HarmonyScheme::Complementary);
        assert!(approx_eq(palette.avg_saturation(), 0.8, 0.01));
    }

    #[test]
    fn to_rgb_palette() {
        let base = HslColor::new(0.0, 1.0, 0.5);
        let palette = PaletteGenerator::generate(base, HarmonyScheme::Complementary);
        let rgbs = palette.to_rgb();
        assert_eq!(rgbs.len(), 2);
        // First should be red-ish
        assert!(rgbs[0].r > 0.9);
    }

    // ── Temperature ──────────────────────────────────────────

    #[test]
    fn temperature_warm() {
        let warm = HslColor::new(30.0, 0.8, 0.5); // Orange
        assert_eq!(classify_temperature(warm), ColorTemperature::Warm);
    }

    #[test]
    fn temperature_cool() {
        let cool = HslColor::new(200.0, 0.8, 0.5); // Blue
        assert_eq!(classify_temperature(cool), ColorTemperature::Cool);
    }

    #[test]
    fn temperature_neutral() {
        let neutral = HslColor::new(90.0, 0.8, 0.5); // Chartreuse
        assert_eq!(classify_temperature(neutral), ColorTemperature::Neutral);
    }

    // ── Pair Harmony ─────────────────────────────────────────

    #[test]
    fn pair_harmony_complementary() {
        let a = HslColor::new(0.0, 0.8, 0.5);
        let b = HslColor::new(180.0, 0.8, 0.5);
        assert!(PaletteGenerator::pair_harmony(a, b) > 0.9);
    }

    #[test]
    fn pair_harmony_identical() {
        let c = HslColor::new(60.0, 0.8, 0.5);
        assert!(PaletteGenerator::pair_harmony(c, c) > 0.9);
    }

    // ── Variations ───────────────────────────────────────────

    #[test]
    fn generate_with_variations() {
        let base = HslColor::new(0.0, 0.8, 0.5);
        let variants = PaletteGenerator::generate_with_variations(
            base,
            HarmonyScheme::Complementary,
            &[0.8, 1.0, 1.2],
        );
        assert_eq!(variants.len(), 3);
        // Lighter variant should have lighter colors
        assert!(variants[2].colors[0].l > variants[0].colors[0].l);
    }

    // ── Clamping ─────────────────────────────────────────────

    #[test]
    fn hsl_clamps_values() {
        let c = HslColor::new(400.0, 1.5, -0.5);
        assert!(c.h >= 0.0 && c.h < 360.0);
        assert!(c.s >= 0.0 && c.s <= 1.0);
        assert!(c.l >= 0.0 && c.l <= 1.0);
    }
}
