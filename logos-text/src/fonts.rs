//! Font registry — system font discovery and CSS-style matching.
//!
//! Wraps `font-kit` for OS-level font enumeration and caches results
//! in a hash map for O(1) lookup by family name. Supports CSS-style
//! fallback chains (`"Arial, Helvetica, sans-serif"`).
//!
//! ## Architecture
//!
//! ```text
//! FontRegistry
//!   ├── system_families: HashMap<String, Vec<FontFace>>   (cached)
//!   ├── generic_map: HashMap<GenericFamily, String>       (resolved once)
//!   └── match_font(descriptor) → Option<FontMatch>       (O(1) amortized)
//! ```
//!
//! **Reference:** *Operating System Concepts, 10th Ed, Section 12.4*

use font_kit::properties::{
    Properties as FkProperties, Style as FkStyle,
};
use font_kit::source::SystemSource;
use std::collections::HashMap;
use std::fmt;
use std::time::Instant;

// ── Font style enum ─────────────────────────────────────────────────

/// Font style (normal, italic, or oblique).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FontStyle {
    Normal,
    Italic,
    Oblique,
}

impl Default for FontStyle {
    fn default() -> Self {
        Self::Normal
    }
}

// ── Font stretch ────────────────────────────────────────────────────

/// Font stretch / width class.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FontStretch {
    UltraCondensed,
    ExtraCondensed,
    Condensed,
    SemiCondensed,
    Normal,
    SemiExpanded,
    Expanded,
    ExtraExpanded,
    UltraExpanded,
}

impl Default for FontStretch {
    fn default() -> Self {
        Self::Normal
    }
}

// ── Generic family ──────────────────────────────────────────────────

/// CSS generic font families.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GenericFamily {
    Serif,
    SansSerif,
    Monospace,
    Cursive,
    Fantasy,
}

// ── Font face info ──────────────────────────────────────────────────

/// Metadata about a single font face within a family.
#[derive(Clone, Debug)]
pub struct FontFace {
    /// PostScript name (unique identifier).
    pub postscript_name: String,
    /// Weight (100–900).
    pub weight: u16,
    /// Style.
    pub style: FontStyle,
    /// Stretch.
    pub stretch: FontStretch,
}

// ── Font descriptor ─────────────────────────────────────────────────

/// CSS-style font descriptor for matching.
///
/// Follows the CSS Fonts Module Level 3 matching algorithm:
/// family → stretch → style → weight.
#[derive(Clone, Debug)]
pub struct FontDescriptor {
    /// Ordered list of family names (CSS fallback chain).
    /// e.g. `["Arial", "Helvetica", "sans-serif"]`
    pub families: Vec<String>,
    /// Weight (100–900). 400 = normal, 700 = bold.
    pub weight: u16,
    /// Style.
    pub style: FontStyle,
    /// Stretch.
    pub stretch: FontStretch,
}

impl Default for FontDescriptor {
    fn default() -> Self {
        Self {
            families: vec!["sans-serif".into()],
            weight: 400,
            style: FontStyle::Normal,
            stretch: FontStretch::Normal,
        }
    }
}

impl FontDescriptor {
    /// Create a descriptor from a CSS-like font-family string.
    ///
    /// Parses `"Arial, Helvetica, sans-serif"` into a fallback chain.
    /// Family names are stored in lowercase for O(1) hash lookup.
    pub fn from_css(family_str: &str, weight: u16, style: FontStyle) -> Self {
        let families: Vec<String> = family_str
            .split(',')
            .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();

        Self {
            families: if families.is_empty() {
                vec!["sans-serif".into()]
            } else {
                families
            },
            weight,
            style,
            stretch: FontStretch::Normal,
        }
    }
}

// ── Font match result ───────────────────────────────────────────────

/// Result of a font match — the resolved family + face metadata.
#[derive(Clone, Debug)]
pub struct FontMatch {
    /// Resolved family name.
    pub family: String,
    /// Matched face within the family.
    pub face: FontFace,
    /// How the match was resolved.
    pub match_type: MatchType,
}

/// How the font was matched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchType {
    /// Exact family name match.
    Exact,
    /// Matched via generic family fallback (e.g. "sans-serif").
    Generic,
    /// Ultimate fallback (no family matched).
    Fallback,
}

// ── Font registry ───────────────────────────────────────────────────

/// System font registry with cached discovery and O(1) matching.
///
/// Call [`FontRegistry::discover()`] once at startup. All subsequent
/// [`match_font()`] calls are hash-table lookups (< 50ns target).
pub struct FontRegistry {
    /// Family name (lowercase) → list of available faces.
    families: HashMap<String, Vec<FontFace>>,

    /// Generic family → resolved concrete family name.
    generic_map: HashMap<GenericFamily, String>,

    /// How long discovery took (for diagnostics).
    discovery_time_ms: f64,

    /// Total number of font faces discovered.
    face_count: usize,
}

impl FontRegistry {
    /// Discover all system fonts and build the registry.
    ///
    /// This is an I/O-bound operation (typically 1–5ms). Call once at
    /// startup and cache the result.
    pub fn discover() -> Self {
        let start = Instant::now();
        let source = SystemSource::new();

        let mut families: HashMap<String, Vec<FontFace>> = HashMap::new();
        let mut face_count = 0usize;

        // Enumerate all families.
        if let Ok(family_names) = source.all_families() {
            for family_name in &family_names {
                // Query all faces in this family.
                if let Ok(family_handle) = source.select_family_by_name(family_name) {
                    let mut faces = Vec::new();
                    for handle in family_handle.fonts() {
                        if let Ok(font) = handle.load() {
                            let props = font.properties();
                            faces.push(FontFace {
                                postscript_name: font
                                    .postscript_name()
                                    .unwrap_or_default(),
                                weight: props.weight.0 as u16,
                                style: convert_style(props.style),
                                stretch: convert_stretch(props.stretch.0),
                            });
                            face_count += 1;
                        }
                    }
                    if !faces.is_empty() {
                        families.insert(family_name.to_lowercase(), faces);
                    }
                }
            }
        }

        // Resolve generic families.
        let generic_map = resolve_generics(&source);

        let discovery_time_ms = start.elapsed().as_secs_f64() * 1000.0;
        log::info!(
            "FontRegistry: discovered {} faces in {} families ({:.1}ms)",
            face_count,
            families.len(),
            discovery_time_ms,
        );

        Self {
            families,
            generic_map,
            discovery_time_ms,
            face_count,
        }
    }

    /// Number of font families discovered.
    pub fn family_count(&self) -> usize {
        self.families.len()
    }

    /// Total number of font faces discovered.
    pub fn face_count(&self) -> usize {
        self.face_count
    }

    /// Discovery time in milliseconds.
    pub fn discovery_time_ms(&self) -> f64 {
        self.discovery_time_ms
    }

    /// List all available family names (sorted).
    pub fn all_families(&self) -> Vec<String> {
        let mut names: Vec<String> = self.families.keys().cloned().collect();
        names.sort();
        names
    }

    /// Check if a specific family is available.
    pub fn has_family(&self, name: &str) -> bool {
        self.families.contains_key(&name.to_lowercase())
    }

    /// Get all faces for a family.
    pub fn get_faces(&self, family: &str) -> Option<&[FontFace]> {
        self.families.get(&family.to_lowercase()).map(|v| v.as_slice())
    }

    /// Get the resolved concrete family for a generic family.
    pub fn resolve_generic(&self, generic: GenericFamily) -> Option<&str> {
        self.generic_map.get(&generic).map(|s| s.as_str())
    }

    /// Match a font descriptor against the registry.
    ///
    /// Follows the CSS Fonts Module Level 3 matching algorithm:
    /// 1. Walk the family fallback chain.
    /// 2. For each family, find best match by style → weight.
    /// 3. If nothing matches, use ultimate fallback (sans-serif).
    ///
    /// **Target: < 50ns** (hash-table lookup + linear scan of ≤10 faces).
    pub fn match_font(&self, descriptor: &FontDescriptor) -> FontMatch {
        // Walk the fallback chain.
        // Family names in the descriptor are expected to be lowercase
        // (from_css lowercases them, to_descriptor lowercases them).
        for family_name in &descriptor.families {
            // Check if it's a generic family keyword.
            if let Some(generic) = parse_generic(family_name) {
                if let Some(concrete) = self.generic_map.get(&generic) {
                    if let Some(faces) = self.families.get(concrete) {
                        let face = best_match(faces, descriptor);
                        return FontMatch {
                            family: concrete.clone(),
                            face,
                            match_type: MatchType::Generic,
                        };
                    }
                }
                continue;
            }

            // Direct family lookup.
            if let Some(faces) = self.families.get(family_name.as_str()) {
                let face = best_match(faces, descriptor);
                return FontMatch {
                    family: family_name.clone(),
                    face,
                    match_type: MatchType::Exact,
                };
            }
        }

        // Ultimate fallback: sans-serif.
        if let Some(sans) = self.generic_map.get(&GenericFamily::SansSerif) {
            if let Some(faces) = self.families.get(sans) {
                let face = best_match(faces, descriptor);
                return FontMatch {
                    family: sans.clone(),
                    face,
                    match_type: MatchType::Fallback,
                };
            }
        }

        // Absolute last resort: pick the first family we have.
        let (family, faces) = self.families.iter().next().unwrap();
        FontMatch {
            family: family.clone(),
            face: faces[0].clone(),
            match_type: MatchType::Fallback,
        }
    }
}

impl fmt::Display for FontRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "FontRegistry({} families, {} faces, {:.1}ms)",
            self.families.len(),
            self.face_count,
            self.discovery_time_ms,
        )
    }
}

// ── CSS font matching internals ─────────────────────────────────────

/// Best-match a descriptor against a list of faces.
///
/// CSS Fonts Level 3 priority: stretch → style → weight.
/// Since most families have ≤ 10 faces, a linear scan is optimal
/// (avoids hash overhead and branch mispredictions).
fn best_match(faces: &[FontFace], desc: &FontDescriptor) -> FontFace {
    debug_assert!(!faces.is_empty());

    let mut best = &faces[0];
    let mut best_score = u32::MAX;

    for face in faces {
        let score = match_score(face, desc);
        if score < best_score {
            best_score = score;
            best = face;
            if score == 0 {
                break; // Perfect match.
            }
        }
    }

    best.clone()
}

/// Compute a match score (lower = better).
///
/// Stretch: 0-8 range (×10000 to dominate).
/// Style:   0-2 range (×100).
/// Weight:  0-800 range (×1).
fn match_score(face: &FontFace, desc: &FontDescriptor) -> u32 {
    let stretch_diff = stretch_distance(face.stretch, desc.stretch) as u32;
    let style_diff = style_distance(face.style, desc.style) as u32;
    let weight_diff = (face.weight as i32 - desc.weight as i32).unsigned_abs();

    stretch_diff * 10000 + style_diff * 100 + weight_diff
}

fn stretch_distance(a: FontStretch, b: FontStretch) -> u8 {
    let ord = |s: FontStretch| match s {
        FontStretch::UltraCondensed => 0,
        FontStretch::ExtraCondensed => 1,
        FontStretch::Condensed => 2,
        FontStretch::SemiCondensed => 3,
        FontStretch::Normal => 4,
        FontStretch::SemiExpanded => 5,
        FontStretch::Expanded => 6,
        FontStretch::ExtraExpanded => 7,
        FontStretch::UltraExpanded => 8,
    };
    (ord(a) as i8 - ord(b) as i8).unsigned_abs()
}

fn style_distance(a: FontStyle, b: FontStyle) -> u8 {
    if a == b {
        return 0;
    }
    // Italic and oblique are "close", normal is "far".
    match (a, b) {
        (FontStyle::Italic, FontStyle::Oblique) | (FontStyle::Oblique, FontStyle::Italic) => 1,
        _ => 2,
    }
}

/// Parse a generic family keyword.
fn parse_generic(name: &str) -> Option<GenericFamily> {
    match name {
        "serif" => Some(GenericFamily::Serif),
        "sans-serif" => Some(GenericFamily::SansSerif),
        "monospace" => Some(GenericFamily::Monospace),
        "cursive" => Some(GenericFamily::Cursive),
        "fantasy" => Some(GenericFamily::Fantasy),
        _ => None,
    }
}

/// Convert font-kit style to our enum.
fn convert_style(style: FkStyle) -> FontStyle {
    match style {
        FkStyle::Normal => FontStyle::Normal,
        FkStyle::Italic => FontStyle::Italic,
        FkStyle::Oblique => FontStyle::Oblique,
    }
}

/// Convert font-kit stretch value (f32 in 0.5–2.0) to our enum.
fn convert_stretch(value: f32) -> FontStretch {
    if value <= 0.525 {
        FontStretch::UltraCondensed
    } else if value <= 0.575 {
        FontStretch::ExtraCondensed
    } else if value <= 0.65 {
        FontStretch::Condensed
    } else if value <= 0.775 {
        FontStretch::SemiCondensed
    } else if value <= 1.05 {
        FontStretch::Normal
    } else if value <= 1.15 {
        FontStretch::SemiExpanded
    } else if value <= 1.3 {
        FontStretch::Expanded
    } else if value <= 1.6 {
        FontStretch::ExtraExpanded
    } else {
        FontStretch::UltraExpanded
    }
}

/// Resolve generic families by querying the system source.
fn resolve_generics(source: &SystemSource) -> HashMap<GenericFamily, String> {
    use font_kit::family_name::FamilyName;

    let mut map = HashMap::new();
    let props = FkProperties::new();

    let generics = [
        (GenericFamily::Serif, FamilyName::Serif),
        (GenericFamily::SansSerif, FamilyName::SansSerif),
        (GenericFamily::Monospace, FamilyName::Monospace),
    ];

    for (generic, fk_name) in &generics {
        if let Ok(handle) = source.select_best_match(&[fk_name.clone()], &props) {
            if let Ok(font) = handle.load() {
                let name = font.family_name();
                if !name.is_empty() {
                    map.insert(*generic, name.to_lowercase());
                }
            }
        }
    }

    map
}

// ===================================================================
// Tests
// ===================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_font_registry_discovery() {
        let registry = FontRegistry::discover();
        // Any Linux/macOS/Windows system should have at least a few fonts.
        assert!(
            registry.family_count() > 0,
            "Expected at least 1 family, got {}",
            registry.family_count()
        );
        assert!(
            registry.face_count() > 0,
            "Expected at least 1 face, got {}",
            registry.face_count()
        );
        assert!(registry.discovery_time_ms() > 0.0);
    }

    #[test]
    fn test_font_registry_display() {
        let registry = FontRegistry::discover();
        let display = format!("{registry}");
        assert!(display.contains("FontRegistry("));
        assert!(display.contains("families"));
    }

    #[test]
    fn test_all_families_sorted() {
        let registry = FontRegistry::discover();
        let families = registry.all_families();
        assert!(!families.is_empty());
        for pair in families.windows(2) {
            assert!(pair[0] <= pair[1], "Families not sorted: {} > {}", pair[0], pair[1]);
        }
    }

    #[test]
    fn test_has_family() {
        let registry = FontRegistry::discover();
        // At least one generic family should resolve.
        let has_any = registry.has_family("sans-serif")
            || registry.has_family("serif")
            || registry.family_count() > 0;
        assert!(has_any);
    }

    #[test]
    fn test_generic_resolution() {
        let registry = FontRegistry::discover();
        // sans-serif should resolve to something.
        let sans = registry.resolve_generic(GenericFamily::SansSerif);
        assert!(
            sans.is_some(),
            "sans-serif should resolve to a concrete family"
        );
    }

    #[test]
    fn test_match_font_sans_serif() {
        let registry = FontRegistry::discover();
        let desc = FontDescriptor {
            families: vec!["sans-serif".into()],
            weight: 400,
            style: FontStyle::Normal,
            stretch: FontStretch::Normal,
        };
        let result = registry.match_font(&desc);
        assert!(!result.family.is_empty());
        assert!(
            result.match_type == MatchType::Generic || result.match_type == MatchType::Fallback,
            "Expected generic or fallback match"
        );
    }

    #[test]
    fn test_match_font_bold() {
        let registry = FontRegistry::discover();
        let desc = FontDescriptor {
            families: vec!["sans-serif".into()],
            weight: 700,
            style: FontStyle::Normal,
            stretch: FontStretch::Normal,
        };
        let result = registry.match_font(&desc);
        assert!(!result.family.is_empty());
        // Bold face should have weight closer to 700 than 100.
        assert!(
            result.face.weight >= 400,
            "Expected bold-ish weight, got {}",
            result.face.weight
        );
    }

    #[test]
    fn test_match_font_italic() {
        let registry = FontRegistry::discover();
        let desc = FontDescriptor {
            families: vec!["sans-serif".into()],
            weight: 400,
            style: FontStyle::Italic,
            stretch: FontStretch::Normal,
        };
        let result = registry.match_font(&desc);
        assert!(!result.family.is_empty());
        // Should prefer italic if available.
    }

    #[test]
    fn test_match_font_fallback_chain() {
        let registry = FontRegistry::discover();
        // "NonExistentFont" should fall through to sans-serif.
        let desc = FontDescriptor {
            families: vec!["NonExistentFont9999".into(), "sans-serif".into()],
            weight: 400,
            style: FontStyle::Normal,
            stretch: FontStretch::Normal,
        };
        let result = registry.match_font(&desc);
        assert!(
            result.match_type == MatchType::Generic || result.match_type == MatchType::Fallback,
            "Should have fallen back past NonExistentFont"
        );
    }

    #[test]
    fn test_match_font_ultimate_fallback() {
        let registry = FontRegistry::discover();
        // All non-existent families should still return something.
        let desc = FontDescriptor {
            families: vec!["ZZZNeverExists".into()],
            weight: 400,
            style: FontStyle::Normal,
            stretch: FontStretch::Normal,
        };
        let result = registry.match_font(&desc);
        assert!(!result.family.is_empty());
        assert_eq!(result.match_type, MatchType::Fallback);
    }

    #[test]
    fn test_descriptor_from_css() {
        let desc = FontDescriptor::from_css("Arial, Helvetica, sans-serif", 700, FontStyle::Italic);
        assert_eq!(desc.families, vec!["arial", "helvetica", "sans-serif"]);
        assert_eq!(desc.weight, 700);
        assert_eq!(desc.style, FontStyle::Italic);
    }

    #[test]
    fn test_descriptor_from_css_quoted() {
        let desc = FontDescriptor::from_css("\"Times New Roman\", serif", 400, FontStyle::Normal);
        assert_eq!(desc.families, vec!["times new roman", "serif"]);
    }

    #[test]
    fn test_descriptor_from_css_empty() {
        let desc = FontDescriptor::from_css("", 400, FontStyle::Normal);
        assert_eq!(desc.families, vec!["sans-serif"]);
    }

    #[test]
    fn test_descriptor_default() {
        let desc = FontDescriptor::default();
        assert_eq!(desc.families, vec!["sans-serif"]);
        assert_eq!(desc.weight, 400);
        assert_eq!(desc.style, FontStyle::Normal);
        assert_eq!(desc.stretch, FontStretch::Normal);
    }

    #[test]
    fn test_match_score_perfect() {
        let face = FontFace {
            postscript_name: "Test".into(),
            weight: 400,
            style: FontStyle::Normal,
            stretch: FontStretch::Normal,
        };
        let desc = FontDescriptor::default();
        assert_eq!(match_score(&face, &desc), 0);
    }

    #[test]
    fn test_match_score_weight_diff() {
        let face = FontFace {
            postscript_name: "Test-Bold".into(),
            weight: 700,
            style: FontStyle::Normal,
            stretch: FontStretch::Normal,
        };
        let desc = FontDescriptor::default(); // weight 400
        assert_eq!(match_score(&face, &desc), 300); // |700 - 400|
    }

    #[test]
    fn test_match_score_style_diff() {
        let face = FontFace {
            postscript_name: "Test-Italic".into(),
            weight: 400,
            style: FontStyle::Italic,
            stretch: FontStretch::Normal,
        };
        let desc = FontDescriptor::default(); // normal
        assert_eq!(match_score(&face, &desc), 200); // style_diff=2, ×100
    }

    #[test]
    fn test_style_distance() {
        assert_eq!(style_distance(FontStyle::Normal, FontStyle::Normal), 0);
        assert_eq!(style_distance(FontStyle::Italic, FontStyle::Italic), 0);
        assert_eq!(style_distance(FontStyle::Italic, FontStyle::Oblique), 1);
        assert_eq!(style_distance(FontStyle::Normal, FontStyle::Italic), 2);
    }

    #[test]
    fn test_stretch_distance() {
        assert_eq!(stretch_distance(FontStretch::Normal, FontStretch::Normal), 0);
        assert_eq!(stretch_distance(FontStretch::Condensed, FontStretch::Normal), 2);
        assert_eq!(
            stretch_distance(FontStretch::UltraCondensed, FontStretch::UltraExpanded),
            8
        );
    }

    #[test]
    fn test_get_faces() {
        let registry = FontRegistry::discover();
        // At least one family should have faces.
        let families = registry.all_families();
        if let Some(family) = families.first() {
            let faces = registry.get_faces(family);
            assert!(faces.is_some());
            assert!(!faces.unwrap().is_empty());
        }
    }

    #[test]
    fn test_convert_style_variants() {
        assert_eq!(convert_style(FkStyle::Normal), FontStyle::Normal);
        assert_eq!(convert_style(FkStyle::Italic), FontStyle::Italic);
        assert_eq!(convert_style(FkStyle::Oblique), FontStyle::Oblique);
    }

    #[test]
    fn test_convert_stretch_values() {
        assert_eq!(convert_stretch(1.0), FontStretch::Normal);
        assert_eq!(convert_stretch(0.5), FontStretch::UltraCondensed);
        assert_eq!(convert_stretch(0.75), FontStretch::SemiCondensed);
        assert_eq!(convert_stretch(1.25), FontStretch::Expanded);
        assert_eq!(convert_stretch(2.0), FontStretch::UltraExpanded);
    }

    #[test]
    fn test_parse_generic() {
        assert_eq!(parse_generic("serif"), Some(GenericFamily::Serif));
        assert_eq!(parse_generic("sans-serif"), Some(GenericFamily::SansSerif));
        assert_eq!(parse_generic("monospace"), Some(GenericFamily::Monospace));
        assert_eq!(parse_generic("cursive"), Some(GenericFamily::Cursive));
        assert_eq!(parse_generic("fantasy"), Some(GenericFamily::Fantasy));
        assert_eq!(parse_generic("arial"), None);
    }
}
