//! # Accessibility Checker
//!
//! Analyses design elements for WCAG 2.1 compliance — contrast ratios,
//! minimum touch target sizes, text readability, and color-blindness
//! simulation.
//!
//! All calculations are deterministic and run in O(n) per check.
//!
//! ```
//! use logos_ai::inference::accessibility::{AccessibilityChecker, ContrastResult, WcagLevel};
//! use logos_core::style::Color;
//!
//! let checker = AccessibilityChecker::default();
//! let result = checker.check_contrast(
//!     Color { r: 1.0, g: 1.0, b: 1.0, a: 1.0 },
//!     Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 },
//! );
//! assert!(result.passes(WcagLevel::AAA));
//! ```

use logos_core::style::Color;
use logos_core::Rect;

// ── WCAG Level ───────────────────────────────────────────────

/// WCAG 2.1 conformance level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WcagLevel {
    /// Minimum — contrast ≥ 3:1 for large text, ≥ 4.5:1 for normal.
    A,
    /// Mid-range — contrast ≥ 4.5:1 for normal text.
    AA,
    /// Highest — contrast ≥ 7:1 for normal text.
    AAA,
}

impl WcagLevel {
    /// Required contrast ratio for normal-sized text.
    pub fn normal_text_ratio(&self) -> f32 {
        match self {
            Self::A => 3.0,
            Self::AA => 4.5,
            Self::AAA => 7.0,
        }
    }

    /// Required contrast ratio for large text (≥ 18pt or ≥ 14pt bold).
    pub fn large_text_ratio(&self) -> f32 {
        match self {
            Self::A => 3.0,
            Self::AA => 3.0,
            Self::AAA => 4.5,
        }
    }
}

// ── Contrast Result ──────────────────────────────────────────

/// Result of a contrast check between two colors.
#[derive(Debug, Clone)]
pub struct ContrastResult {
    /// The computed contrast ratio (always ≥ 1.0).
    pub ratio: f32,
    /// Foreground luminance.
    pub fg_luminance: f32,
    /// Background luminance.
    pub bg_luminance: f32,
}

impl ContrastResult {
    /// Check if the ratio passes a given WCAG level for normal text.
    pub fn passes(&self, level: WcagLevel) -> bool {
        self.ratio >= level.normal_text_ratio()
    }

    /// Check for large text.
    pub fn passes_large_text(&self, level: WcagLevel) -> bool {
        self.ratio >= level.large_text_ratio()
    }

    /// Human-readable grade.
    pub fn grade(&self) -> &'static str {
        if self.ratio >= 7.0 {
            "Excellent"
        } else if self.ratio >= 4.5 {
            "Good"
        } else if self.ratio >= 3.0 {
            "Fair"
        } else {
            "Poor"
        }
    }
}

// ── Touch Target ─────────────────────────────────────────────

/// Minimum touch target requirements.
#[derive(Debug, Clone, Copy)]
pub struct TouchTargetSpec {
    /// Minimum width in CSS pixels.
    pub min_width: f32,
    /// Minimum height in CSS pixels.
    pub min_height: f32,
}

impl TouchTargetSpec {
    /// WCAG 2.1 Level AAA: 44×44 CSS px.
    pub fn wcag_aaa() -> Self {
        Self { min_width: 44.0, min_height: 44.0 }
    }

    /// Android Material Design: 48×48 dp.
    pub fn material() -> Self {
        Self { min_width: 48.0, min_height: 48.0 }
    }

    /// Apple HIG: 44×44 pt.
    pub fn apple_hig() -> Self {
        Self { min_width: 44.0, min_height: 44.0 }
    }
}

impl Default for TouchTargetSpec {
    fn default() -> Self {
        Self::wcag_aaa()
    }
}

/// Result of a touch-target size check.
#[derive(Debug, Clone)]
pub struct TouchTargetResult {
    /// Element index.
    pub element_index: usize,
    /// Actual width.
    pub actual_width: f32,
    /// Actual height.
    pub actual_height: f32,
    /// Whether it meets the spec.
    pub passes: bool,
    /// Suggested minimum bounds.
    pub suggested: Rect,
}

// ── Color-Blindness Simulation ───────────────────────────────

/// Type of color-vision deficiency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColorBlindnessType {
    /// Red-weak (most common, ~6% of males).
    Protanopia,
    /// Green-weak (~2.7% of males).
    Deuteranopia,
    /// Blue-weak (rare, ~0.01%).
    Tritanopia,
    /// Complete color blindness.
    Achromatopsia,
}

impl ColorBlindnessType {
    /// Prevalence in the general population.
    pub fn prevalence_pct(&self) -> f32 {
        match self {
            Self::Protanopia => 1.0,
            Self::Deuteranopia => 1.2,
            Self::Tritanopia => 0.01,
            Self::Achromatopsia => 0.003,
        }
    }
}

/// Simulate how a color appears under a given deficiency.
///
/// Uses simplified Brettel/Vienot matrix approximations.
pub fn simulate_color_blindness(color: Color, deficiency: ColorBlindnessType) -> Color {
    let (r, g, b) = (color.r, color.g, color.b);
    let (nr, ng, nb) = match deficiency {
        ColorBlindnessType::Protanopia => {
            // Approximate protanopia matrix
            (0.567 * r + 0.433 * g + 0.0 * b,
             0.558 * r + 0.442 * g + 0.0 * b,
             0.0 * r + 0.242 * g + 0.758 * b)
        }
        ColorBlindnessType::Deuteranopia => {
            (0.625 * r + 0.375 * g + 0.0 * b,
             0.7 * r + 0.3 * g + 0.0 * b,
             0.0 * r + 0.3 * g + 0.7 * b)
        }
        ColorBlindnessType::Tritanopia => {
            (0.95 * r + 0.05 * g + 0.0 * b,
             0.0 * r + 0.433 * g + 0.567 * b,
             0.0 * r + 0.475 * g + 0.525 * b)
        }
        ColorBlindnessType::Achromatopsia => {
            let lum = 0.299 * r + 0.587 * g + 0.114 * b;
            (lum, lum, lum)
        }
    };
    Color {
        r: nr.clamp(0.0, 1.0),
        g: ng.clamp(0.0, 1.0),
        b: nb.clamp(0.0, 1.0),
        a: color.a,
    }
}

// ── Text Readability ─────────────────────────────────────────

/// Font-size thresholds for readability.
#[derive(Debug, Clone, Copy)]
pub struct ReadabilitySpec {
    /// Minimum body text size (pt).
    pub min_body_size: f32,
    /// Minimum caption text size (pt).
    pub min_caption_size: f32,
    /// Maximum recommended line length (characters).
    pub max_line_chars: usize,
    /// Minimum recommended line height multiplier.
    pub min_line_height: f32,
}

impl Default for ReadabilitySpec {
    fn default() -> Self {
        Self {
            min_body_size: 16.0,
            min_caption_size: 12.0,
            max_line_chars: 80,
            min_line_height: 1.4,
        }
    }
}

/// Text readability issue.
#[derive(Debug, Clone)]
pub struct ReadabilityIssue {
    /// Human-readable description.
    pub message: String,
    /// Severity: 0=info, 1=warning, 2=error.
    pub severity: u8,
}

// ── Accessibility Checker ────────────────────────────────────

/// Central accessibility checker.
pub struct AccessibilityChecker {
    touch_spec: TouchTargetSpec,
    readability_spec: ReadabilitySpec,
}

impl Default for AccessibilityChecker {
    fn default() -> Self {
        Self {
            touch_spec: TouchTargetSpec::default(),
            readability_spec: ReadabilitySpec::default(),
        }
    }
}

impl AccessibilityChecker {
    /// Create with custom specs.
    pub fn new(touch: TouchTargetSpec, readability: ReadabilitySpec) -> Self {
        Self {
            touch_spec: touch,
            readability_spec: readability,
        }
    }

    /// Check contrast between foreground and background colors.
    pub fn check_contrast(&self, fg: Color, bg: Color) -> ContrastResult {
        let fg_lum = relative_luminance(fg);
        let bg_lum = relative_luminance(bg);
        let lighter = fg_lum.max(bg_lum);
        let darker = fg_lum.min(bg_lum);
        let ratio = (lighter + 0.05) / (darker + 0.05);
        ContrastResult {
            ratio,
            fg_luminance: fg_lum,
            bg_luminance: bg_lum,
        }
    }

    /// Check all color pairs for contrast compliance at a given WCAG level.
    pub fn check_contrast_pairs(&self, pairs: &[(Color, Color)], level: WcagLevel) -> Vec<(usize, ContrastResult)> {
        pairs
            .iter()
            .enumerate()
            .map(|(i, &(fg, bg))| (i, self.check_contrast(fg, bg)))
            .filter(|(_, r)| !r.passes(level))
            .collect()
    }

    /// Check touch target sizes for a list of interactive elements.
    pub fn check_touch_targets(&self, elements: &[Rect]) -> Vec<TouchTargetResult> {
        elements
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let passes = e.width >= self.touch_spec.min_width
                    && e.height >= self.touch_spec.min_height;
                let suggested = Rect {
                    x: e.x - ((self.touch_spec.min_width - e.width).max(0.0) / 2.0),
                    y: e.y - ((self.touch_spec.min_height - e.height).max(0.0) / 2.0),
                    width: e.width.max(self.touch_spec.min_width),
                    height: e.height.max(self.touch_spec.min_height),
                };
                TouchTargetResult {
                    element_index: i,
                    actual_width: e.width,
                    actual_height: e.height,
                    passes,
                    suggested,
                }
            })
            .collect()
    }

    /// Check font size against readability thresholds.
    pub fn check_font_size(&self, size_pt: f32, is_caption: bool) -> Option<ReadabilityIssue> {
        let min = if is_caption {
            self.readability_spec.min_caption_size
        } else {
            self.readability_spec.min_body_size
        };
        if size_pt < min {
            Some(ReadabilityIssue {
                message: format!(
                    "Font size {:.1}pt is below minimum {:.1}pt for {}",
                    size_pt,
                    min,
                    if is_caption { "captions" } else { "body text" },
                ),
                severity: if size_pt < min * 0.75 { 2 } else { 1 },
            })
        } else {
            None
        }
    }

    /// Check line length.
    pub fn check_line_length(&self, chars: usize) -> Option<ReadabilityIssue> {
        if chars > self.readability_spec.max_line_chars {
            Some(ReadabilityIssue {
                message: format!(
                    "Line length {} exceeds maximum {} characters",
                    chars, self.readability_spec.max_line_chars,
                ),
                severity: 1,
            })
        } else {
            None
        }
    }

    /// Simulate a color under a specific color vision deficiency.
    pub fn simulate_color_blindness(&self, color: Color, deficiency: ColorBlindnessType) -> Color {
        simulate_color_blindness(color, deficiency)
    }

    /// Check if two colors are distinguishable under all major CVD types.
    pub fn colors_distinguishable(&self, a: Color, b: Color, min_delta: f32) -> Vec<ColorBlindnessType> {
        let deficiencies = [
            ColorBlindnessType::Protanopia,
            ColorBlindnessType::Deuteranopia,
            ColorBlindnessType::Tritanopia,
        ];
        deficiencies
            .iter()
            .filter(|&&d| {
                let sa = simulate_color_blindness(a, d);
                let sb = simulate_color_blindness(b, d);
                color_distance(sa, sb) < min_delta
            })
            .copied()
            .collect()
    }
}

// ── Color Math ───────────────────────────────────────────────

/// Relative luminance per WCAG 2.1 (sRGB linearization).
pub fn relative_luminance(c: Color) -> f32 {
    fn linearize(v: f32) -> f32 {
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * linearize(c.r) + 0.7152 * linearize(c.g) + 0.0722 * linearize(c.b)
}

/// Euclidean distance in RGB space.
fn color_distance(a: Color, b: Color) -> f32 {
    let dr = a.r - b.r;
    let dg = a.g - b.g;
    let db = a.b - b.b;
    (dr * dr + dg * dg + db * db).sqrt()
}

// ── Tests ────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn white() -> Color { Color { r: 1.0, g: 1.0, b: 1.0, a: 1.0 } }
    fn black() -> Color { Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 } }
    fn red()   -> Color { Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 } }
    fn gray()  -> Color { Color { r: 0.5, g: 0.5, b: 0.5, a: 1.0 } }

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, width: w, height: h }
    }

    // ── Contrast ─────────────────────────────────────────────

    #[test]
    fn black_on_white_max_contrast() {
        let checker = AccessibilityChecker::default();
        let r = checker.check_contrast(black(), white());
        assert!(r.ratio >= 20.0); // Should be ~21:1
        assert!(r.passes(WcagLevel::AAA));
        assert_eq!(r.grade(), "Excellent");
    }

    #[test]
    fn white_on_white_min_contrast() {
        let checker = AccessibilityChecker::default();
        let r = checker.check_contrast(white(), white());
        assert!((r.ratio - 1.0).abs() < 0.01);
        assert!(!r.passes(WcagLevel::A));
        assert_eq!(r.grade(), "Poor");
    }

    #[test]
    fn gray_on_white_mid_contrast() {
        let checker = AccessibilityChecker::default();
        let r = checker.check_contrast(gray(), white());
        assert!(r.ratio > 1.0);
        assert!(r.ratio < 21.0);
    }

    #[test]
    fn contrast_is_symmetric() {
        let checker = AccessibilityChecker::default();
        let r1 = checker.check_contrast(red(), white());
        let r2 = checker.check_contrast(white(), red());
        assert!((r1.ratio - r2.ratio).abs() < 0.01);
    }

    #[test]
    fn contrast_pairs_filters_failing() {
        let checker = AccessibilityChecker::default();
        let pairs = vec![
            (black(), white()), // passes
            (white(), white()), // fails
        ];
        let fails = checker.check_contrast_pairs(&pairs, WcagLevel::AA);
        assert_eq!(fails.len(), 1);
        assert_eq!(fails[0].0, 1);
    }

    #[test]
    fn large_text_looser_threshold() {
        let checker = AccessibilityChecker::default();
        let r = checker.check_contrast(gray(), white());
        // May fail normal but pass large text
        let passes_large = r.passes_large_text(WcagLevel::AA);
        let passes_normal = r.passes(WcagLevel::AA);
        // Large text threshold (3:1) is always ≤ normal (4.5:1)
        assert!(passes_large || !passes_normal);
    }

    // ── Touch Targets ────────────────────────────────────────

    #[test]
    fn adequate_touch_targets() {
        let checker = AccessibilityChecker::default();
        let elements = vec![rect(0.0, 0.0, 48.0, 48.0)];
        let results = checker.check_touch_targets(&elements);
        assert_eq!(results.len(), 1);
        assert!(results[0].passes);
    }

    #[test]
    fn too_small_touch_target() {
        let checker = AccessibilityChecker::default();
        let elements = vec![rect(10.0, 10.0, 20.0, 20.0)];
        let results = checker.check_touch_targets(&elements);
        assert!(!results[0].passes);
        assert!(results[0].suggested.width >= 44.0);
        assert!(results[0].suggested.height >= 44.0);
    }

    #[test]
    fn touch_target_suggested_centered() {
        let checker = AccessibilityChecker::default();
        let elements = vec![rect(100.0, 100.0, 20.0, 20.0)];
        let results = checker.check_touch_targets(&elements);
        let s = &results[0].suggested;
        // Center should be close to original center (110, 110)
        let center_x = s.x + s.width / 2.0;
        let center_y = s.y + s.height / 2.0;
        assert!((center_x - 110.0).abs() < 1.0);
        assert!((center_y - 110.0).abs() < 1.0);
    }

    // ── Color Blindness ──────────────────────────────────────

    #[test]
    fn achromatopsia_produces_gray() {
        let c = simulate_color_blindness(red(), ColorBlindnessType::Achromatopsia);
        // R=1,G=0,B=0 → luminance ≈ 0.299
        assert!((c.r - c.g).abs() < 0.001);
        assert!((c.g - c.b).abs() < 0.001);
    }

    #[test]
    fn white_unchanged_by_cvd() {
        for &cvd in &[
            ColorBlindnessType::Protanopia,
            ColorBlindnessType::Deuteranopia,
            ColorBlindnessType::Tritanopia,
            ColorBlindnessType::Achromatopsia,
        ] {
            let c = simulate_color_blindness(white(), cvd);
            assert!((c.r - 1.0).abs() < 0.05, "{:?}: r={}", cvd, c.r);
            assert!((c.g - 1.0).abs() < 0.05, "{:?}: g={}", cvd, c.g);
            assert!((c.b - 1.0).abs() < 0.05, "{:?}: b={}", cvd, c.b);
        }
    }

    #[test]
    fn black_unchanged_by_cvd() {
        for &cvd in &[
            ColorBlindnessType::Protanopia,
            ColorBlindnessType::Deuteranopia,
            ColorBlindnessType::Tritanopia,
            ColorBlindnessType::Achromatopsia,
        ] {
            let c = simulate_color_blindness(black(), cvd);
            assert!(c.r.abs() < 0.01);
            assert!(c.g.abs() < 0.01);
            assert!(c.b.abs() < 0.01);
        }
    }

    #[test]
    fn colors_distinguishable_red_green_issue() {
        let checker = AccessibilityChecker::default();
        let green = Color { r: 0.0, g: 0.8, b: 0.0, a: 1.0 };
        // Use a larger min_delta threshold to catch the reduced distinction
        let problematic = checker.colors_distinguishable(red(), green, 0.5);
        // Red-green should be problematic for protanopia or deuteranopia
        assert!(!problematic.is_empty());
    }

    #[test]
    fn prevalence_values() {
        assert!(ColorBlindnessType::Protanopia.prevalence_pct() > 0.0);
        assert!(ColorBlindnessType::Achromatopsia.prevalence_pct() < 0.01);
    }

    // ── Readability ──────────────────────────────────────────

    #[test]
    fn font_size_too_small() {
        let checker = AccessibilityChecker::default();
        let issue = checker.check_font_size(10.0, false);
        assert!(issue.is_some());
        assert!(issue.unwrap().severity >= 1);
    }

    #[test]
    fn font_size_ok() {
        let checker = AccessibilityChecker::default();
        assert!(checker.check_font_size(16.0, false).is_none());
        assert!(checker.check_font_size(12.0, true).is_none());
    }

    #[test]
    fn very_small_font_high_severity() {
        let checker = AccessibilityChecker::default();
        let issue = checker.check_font_size(8.0, false).unwrap();
        assert_eq!(issue.severity, 2); // 8 < 16 * 0.75 = 12
    }

    #[test]
    fn line_length_too_long() {
        let checker = AccessibilityChecker::default();
        let issue = checker.check_line_length(120);
        assert!(issue.is_some());
    }

    #[test]
    fn line_length_ok() {
        let checker = AccessibilityChecker::default();
        assert!(checker.check_line_length(60).is_none());
    }

    // ── Luminance ────────────────────────────────────────────

    #[test]
    fn luminance_white() {
        let l = relative_luminance(white());
        assert!((l - 1.0).abs() < 0.01);
    }

    #[test]
    fn luminance_black() {
        let l = relative_luminance(black());
        assert!(l.abs() < 0.01);
    }

    // ── Spec Presets ─────────────────────────────────────────

    #[test]
    fn touch_target_presets() {
        let wcag = TouchTargetSpec::wcag_aaa();
        let mat = TouchTargetSpec::material();
        let apple = TouchTargetSpec::apple_hig();
        assert!(wcag.min_width >= 44.0);
        assert!(mat.min_width >= 48.0);
        assert!(apple.min_width >= 44.0);
    }

    #[test]
    fn wcag_level_ordering() {
        assert!(WcagLevel::A < WcagLevel::AA);
        assert!(WcagLevel::AA < WcagLevel::AAA);
    }

    #[test]
    fn wcag_ratios_increase() {
        assert!(WcagLevel::A.normal_text_ratio() <= WcagLevel::AA.normal_text_ratio());
        assert!(WcagLevel::AA.normal_text_ratio() <= WcagLevel::AAA.normal_text_ratio());
    }
}
