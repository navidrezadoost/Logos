//! Export profiles — predefined export configurations for common targets.
//!
//! Profiles encapsulate DPI, format, color space, optimization level,
//! and scale settings for common export workflows so users don't have
//! to configure each parameter individually.

use serde::{Deserialize, Serialize};

use crate::batch::{ExportFormat, ExportScale};
use crate::color::ColorSpace;
use crate::optimize::OptimizationLevel;
use crate::png::RasterConfig;

/// Predefined export profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportProfileKind {
    /// Optimized for web — SVG preferred, sRGB, aggressive optimization.
    Web,
    /// High-quality print — PDF/PNG at 300 DPI, CMYK-ready.
    Print,
    /// iOS assets — PNG at 1×/2×/3×, sRGB.
    IOS,
    /// Android assets — PNG at mdpi/hdpi/xhdpi/xxhdpi, sRGB.
    Android,
    /// Social media — PNG at specific dimensions.
    Social,
    /// User-defined profile.
    Custom(String),
}

/// Full export profile with all settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportProfile {
    pub kind: ExportProfileKind,
    pub name: String,
    pub description: String,
    pub formats: Vec<ExportFormat>,
    pub scales: Vec<ExportScale>,
    pub color_space: ColorSpace,
    pub dpi: f32,
    pub optimization: OptimizationLevel,
    pub include_manifest: bool,
}

impl ExportProfile {
    /// Web export preset: SVG + PNG @1× and @2×, sRGB, aggressive optimization.
    pub fn web() -> Self {
        Self {
            kind: ExportProfileKind::Web,
            name: "Web".to_string(),
            description: "Optimized SVG and PNG for web delivery".to_string(),
            formats: vec![ExportFormat::Svg],
            scales: vec![ExportScale::X1, ExportScale::X2],
            color_space: ColorSpace::Srgb,
            dpi: 72.0,
            optimization: OptimizationLevel::Aggressive,
            include_manifest: false,
        }
    }

    /// Print preset: PDF at 300 DPI, CMYK color space.
    pub fn print() -> Self {
        Self {
            kind: ExportProfileKind::Print,
            name: "Print".to_string(),
            description: "High-quality print-ready PDF at 300 DPI".to_string(),
            formats: vec![ExportFormat::Pdf],
            scales: vec![ExportScale::X1],
            color_space: ColorSpace::Cmyk,
            dpi: 300.0,
            optimization: OptimizationLevel::None,
            include_manifest: false,
        }
    }

    /// iOS preset: PNG @1×/@2×/@3×, sRGB.
    pub fn ios() -> Self {
        Self {
            kind: ExportProfileKind::IOS,
            name: "iOS".to_string(),
            description: "PNG assets at 1×, 2×, 3× for iOS/iPadOS".to_string(),
            formats: vec![ExportFormat::Svg],
            scales: vec![ExportScale::X1, ExportScale::X2, ExportScale::X3],
            color_space: ColorSpace::Srgb,
            dpi: 72.0,
            optimization: OptimizationLevel::Safe,
            include_manifest: true,
        }
    }

    /// Android preset: PNG at standard density buckets.
    pub fn android() -> Self {
        Self {
            kind: ExportProfileKind::Android,
            name: "Android".to_string(),
            description: "PNG assets for Android density buckets".to_string(),
            formats: vec![ExportFormat::Svg],
            scales: vec![
                ExportScale::X1,                       // mdpi
                ExportScale::custom(1.5, "@1.5x"),     // hdpi
                ExportScale::X2,                       // xhdpi
                ExportScale::X3,                       // xxhdpi
            ],
            color_space: ColorSpace::Srgb,
            dpi: 72.0,
            optimization: OptimizationLevel::Safe,
            include_manifest: true,
        }
    }

    /// Social media preset: PNG at common social sizes.
    pub fn social() -> Self {
        Self {
            kind: ExportProfileKind::Social,
            name: "Social Media".to_string(),
            description: "PNG optimized for social media platforms".to_string(),
            formats: vec![ExportFormat::Svg],
            scales: vec![ExportScale::X1, ExportScale::X2],
            color_space: ColorSpace::Srgb,
            dpi: 72.0,
            optimization: OptimizationLevel::Safe,
            include_manifest: false,
        }
    }

    /// Custom profile builder.
    pub fn custom(name: &str) -> Self {
        Self {
            kind: ExportProfileKind::Custom(name.to_string()),
            name: name.to_string(),
            description: String::new(),
            formats: vec![ExportFormat::Svg],
            scales: vec![ExportScale::X1],
            color_space: ColorSpace::Srgb,
            dpi: 72.0,
            optimization: OptimizationLevel::Safe,
            include_manifest: false,
        }
    }

    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = desc.to_string();
        self
    }

    pub fn with_formats(mut self, formats: Vec<ExportFormat>) -> Self {
        self.formats = formats;
        self
    }

    pub fn with_scales(mut self, scales: Vec<ExportScale>) -> Self {
        self.scales = scales;
        self
    }

    pub fn with_color_space(mut self, cs: ColorSpace) -> Self {
        self.color_space = cs;
        self
    }

    pub fn with_dpi(mut self, dpi: f32) -> Self {
        self.dpi = dpi;
        self
    }

    pub fn with_optimization(mut self, level: OptimizationLevel) -> Self {
        self.optimization = level;
        self
    }

    pub fn with_manifest(mut self, include: bool) -> Self {
        self.include_manifest = include;
        self
    }

    /// Generate a `RasterConfig` from this profile at the given scale.
    pub fn raster_config(&self, scale: &ExportScale) -> RasterConfig {
        let factor = scale.factor;
        RasterConfig {
            dpi: self.dpi * factor,
            scale: factor,
            anti_alias: self.dpi >= 150.0,
            bits_per_channel: 8,
        }
    }

    /// Total number of export artifacts per source element.
    pub fn artifacts_per_source(&self) -> usize {
        self.formats.len() * self.scales.len()
    }

    /// All predefined profiles.
    pub fn all_presets() -> Vec<ExportProfile> {
        vec![
            Self::web(),
            Self::print(),
            Self::ios(),
            Self::android(),
            Self::social(),
        ]
    }
}

// ═══════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_profile_defaults() {
        let p = ExportProfile::web();
        assert_eq!(p.kind, ExportProfileKind::Web);
        assert!(p.formats.contains(&ExportFormat::Svg));
        assert_eq!(p.color_space, ColorSpace::Srgb);
        assert!((p.dpi - 72.0).abs() < 0.01);
        assert_eq!(p.optimization, OptimizationLevel::Aggressive);
    }

    #[test]
    fn print_profile_defaults() {
        let p = ExportProfile::print();
        assert_eq!(p.color_space, ColorSpace::Cmyk);
        assert!((p.dpi - 300.0).abs() < 0.01);
        assert!(p.formats.contains(&ExportFormat::Pdf));
    }

    #[test]
    fn ios_profile_scales() {
        let p = ExportProfile::ios();
        assert_eq!(p.scales.len(), 3);
        assert!(p.include_manifest);
    }

    #[test]
    fn android_profile_scales() {
        let p = ExportProfile::android();
        assert_eq!(p.scales.len(), 4); // mdpi, hdpi, xhdpi, xxhdpi
    }

    #[test]
    fn social_profile() {
        let p = ExportProfile::social();
        assert_eq!(p.kind, ExportProfileKind::Social);
    }

    #[test]
    fn custom_profile_builder() {
        let p = ExportProfile::custom("My Export")
            .with_description("Test")
            .with_formats(vec![ExportFormat::Pdf, ExportFormat::Svg])
            .with_dpi(150.0)
            .with_color_space(ColorSpace::DisplayP3)
            .with_optimization(OptimizationLevel::Aggressive)
            .with_manifest(true);
        assert_eq!(p.name, "My Export");
        assert_eq!(p.formats.len(), 2);
        assert!((p.dpi - 150.0).abs() < 0.01);
        assert!(p.include_manifest);
    }

    #[test]
    fn raster_config_from_profile() {
        let p = ExportProfile::print();
        let rc = p.raster_config(&ExportScale::X1);
        assert!((rc.dpi - 300.0).abs() < 0.01);
        assert!((rc.scale - 1.0).abs() < 0.01);
        assert!(rc.anti_alias);
    }

    #[test]
    fn raster_config_retina_scale() {
        let p = ExportProfile::web();
        let rc = p.raster_config(&ExportScale::X2);
        assert!((rc.scale - 2.0).abs() < 0.01);
    }

    #[test]
    fn artifacts_per_source() {
        let p = ExportProfile::ios();
        // 1 format × 3 scales
        assert_eq!(p.artifacts_per_source(), 3);

        let p2 = ExportProfile::custom("x")
            .with_formats(vec![ExportFormat::Svg, ExportFormat::Pdf])
            .with_scales(vec![ExportScale::X1, ExportScale::X2]);
        assert_eq!(p2.artifacts_per_source(), 4);
    }

    #[test]
    fn all_presets_count() {
        let presets = ExportProfile::all_presets();
        assert_eq!(presets.len(), 5);
    }

    #[test]
    fn profile_with_scales() {
        let p = ExportProfile::custom("test")
            .with_scales(vec![ExportScale::X1, ExportScale::X3]);
        assert_eq!(p.scales.len(), 2);
    }

    #[test]
    fn profile_kind_equality() {
        assert_eq!(ExportProfileKind::Web, ExportProfileKind::Web);
        assert_ne!(ExportProfileKind::Web, ExportProfileKind::Print);
        assert_eq!(
            ExportProfileKind::Custom("a".into()),
            ExportProfileKind::Custom("a".into())
        );
    }
}
