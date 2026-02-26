//! Batch export engine — orchestrates multi-format, multi-scale exports.
//!
//! Allows exporting a design document (or selected layers) into multiple
//! formats and scales in one pass.
//!
//! ## Architecture
//!
//! ```text
//!  BatchExportConfig
//!   ├── format:  Svg | Pdf | Css | SwiftUI | Compose
//!   ├── scales:  [1x, 2x, 3x]
//!   └── suffix:  naming strategy
//!       │
//!       ▼
//!  BatchExporter
//!   ├── collects ExportLayerData (from layout engine)
//!   ├── iterates (format × scale) combinations
//!   └── produces Vec<ExportArtifact>
//! ```
//!
//! ## References
//!
//! - Figma "Export" panel conventions (format × scale matrix)
//! - Android drawable-density naming (mdpi, hdpi, xhdpi, …)

use crate::ExportPage;
use std::fmt;

// ───────────────────────────────────────────────────────────────────
// Export format
// ───────────────────────────────────────────────────────────────────

/// All supported export formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExportFormat {
    Svg,
    Pdf,
    Css,
    SwiftUI,
    Compose,
}

impl ExportFormat {
    /// Human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Svg => "SVG",
            Self::Pdf => "PDF",
            Self::Css => "CSS",
            Self::SwiftUI => "SwiftUI",
            Self::Compose => "Compose",
        }
    }

    /// File extension.
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Svg => "svg",
            Self::Pdf => "pdf",
            Self::Css => "css",
            Self::SwiftUI => "swift",
            Self::Compose => "kt",
        }
    }

    /// Whether this is a raster-scalable format (affected by DPI).
    pub fn is_scalable(&self) -> bool {
        matches!(self, Self::Svg | Self::Pdf)
    }

    /// Whether this is a code-gen format.
    pub fn is_code(&self) -> bool {
        matches!(self, Self::Css | Self::SwiftUI | Self::Compose)
    }

    /// All built-in formats.
    pub fn all() -> &'static [ExportFormat] {
        &[
            Self::Svg,
            Self::Pdf,
            Self::Css,
            Self::SwiftUI,
            Self::Compose,
        ]
    }
}

impl fmt::Display for ExportFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

// ───────────────────────────────────────────────────────────────────
// Export scale
// ───────────────────────────────────────────────────────────────────

/// Export scale preset.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExportScale {
    /// Multiplier (e.g. 1.0, 2.0, 3.0).
    pub factor: f32,
    /// Suffix appended to filenames (e.g. "@2x").
    pub suffix: &'static str,
    /// Dots-per-inch at this scale (96 * factor).
    pub dpi: f32,
}

impl ExportScale {
    pub const X1: ExportScale = ExportScale {
        factor: 1.0,
        suffix: "",
        dpi: 96.0,
    };

    pub const X2: ExportScale = ExportScale {
        factor: 2.0,
        suffix: "@2x",
        dpi: 192.0,
    };

    pub const X3: ExportScale = ExportScale {
        factor: 3.0,
        suffix: "@3x",
        dpi: 288.0,
    };

    /// All standard presets for Apple-style assets.
    pub fn presets() -> &'static [ExportScale] {
        &[Self::X1, Self::X2, Self::X3]
    }

    /// Custom scale.
    pub fn custom(factor: f32, suffix: &'static str) -> Self {
        Self {
            factor,
            suffix,
            dpi: 96.0 * factor,
        }
    }

    /// Compute scaled page dimensions.
    pub fn scale_page(&self, page: &ExportPage) -> ExportPage {
        ExportPage {
            width: page.width * self.factor,
            height: page.height * self.factor,
            background: page.background,
        }
    }
}

impl Default for ExportScale {
    fn default() -> Self {
        Self::X1
    }
}

// ───────────────────────────────────────────────────────────────────
// Suffix / naming strategy
// ───────────────────────────────────────────────────────────────────

/// Naming strategy for exported files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamingStrategy {
    /// Use layer name + scale suffix + format extension.
    /// e.g.  "icon@2x.svg"
    Suffix,
    /// Separate directories per scale.
    /// e.g.  "2x/icon.svg"
    Directory,
}

impl Default for NamingStrategy {
    fn default() -> Self {
        Self::Suffix
    }
}

impl NamingStrategy {
    /// Resolve filename for a given base name, scale and format.
    pub fn resolve(&self, base: &str, scale: &ExportScale, format: ExportFormat) -> String {
        match self {
            Self::Suffix => format!(
                "{}{}.{}",
                base,
                scale.suffix,
                format.extension()
            ),
            Self::Directory => {
                let dir_name = if scale.factor == 1.0 {
                    "1x".to_string()
                } else {
                    format!("{}x", scale.factor as u32)
                };
                format!("{dir_name}/{base}.{}", format.extension())
            }
        }
    }
}

// ───────────────────────────────────────────────────────────────────
// Batch config
// ───────────────────────────────────────────────────────────────────

/// Configuration for a batch export job.
#[derive(Debug, Clone)]
pub struct BatchExportConfig {
    /// Output formats.
    pub formats: Vec<ExportFormat>,
    /// Scales to produce.
    pub scales: Vec<ExportScale>,
    /// Naming strategy.
    pub naming: NamingStrategy,
    /// Base name for output files.
    pub base_name: String,
}

impl BatchExportConfig {
    /// New config with a single format at 1× scale.
    pub fn new(base_name: impl Into<String>, format: ExportFormat) -> Self {
        Self {
            formats: vec![format],
            scales: vec![ExportScale::X1],
            naming: NamingStrategy::Suffix,
            base_name: base_name.into(),
        }
    }

    /// Builder: add a format.
    pub fn with_format(mut self, format: ExportFormat) -> Self {
        if !self.formats.contains(&format) {
            self.formats.push(format);
        }
        self
    }

    /// Builder: set scales.
    pub fn with_scales(mut self, scales: Vec<ExportScale>) -> Self {
        self.scales = scales;
        self
    }

    /// Builder: add Apple-standard 1×/2×/3× scales.
    pub fn with_apple_scales(self) -> Self {
        self.with_scales(ExportScale::presets().to_vec())
    }

    /// Builder: set naming strategy.
    pub fn with_naming(mut self, naming: NamingStrategy) -> Self {
        self.naming = naming;
        self
    }

    /// Total number of artifacts this config will produce.
    pub fn artifact_count(&self) -> usize {
        // Code formats ignore scale — only produce 1× output
        let scalable = self.formats.iter().filter(|f| f.is_scalable()).count();
        let code = self.formats.iter().filter(|f| f.is_code()).count();
        scalable * self.scales.len() + code
    }

    /// Iterate all (format, scale) tuples this config emits.
    pub fn iter_targets(&self) -> Vec<(ExportFormat, ExportScale)> {
        let mut targets = Vec::with_capacity(self.artifact_count());
        for &fmt in &self.formats {
            if fmt.is_code() {
                // Code gen is scale-independent — always 1×
                targets.push((fmt, ExportScale::X1));
            } else {
                for &scale in &self.scales {
                    targets.push((fmt, scale));
                }
            }
        }
        targets
    }
}

/// A single output artifact produced by a batch export.
#[derive(Debug)]
pub struct ExportArtifact {
    /// Relative filename (according to naming strategy).
    pub filename: String,
    /// Export format.
    pub format: ExportFormat,
    /// Scale used.
    pub scale: ExportScale,
    /// Raw bytes of the exported content.
    pub data: Vec<u8>,
}

// ───────────────────────────────────────────────────────────────────
// Batch exporter
// ───────────────────────────────────────────────────────────────────

/// Batch export orchestrator.
///
/// Currently executes targets sequentially; can be extended with
/// `rayon` for parallel rendering of independent scales.
pub struct BatchExporter {
    config: BatchExportConfig,
}

impl BatchExporter {
    pub fn new(config: BatchExportConfig) -> Self {
        Self { config }
    }

    /// Return the computed config for inspection.
    pub fn config(&self) -> &BatchExportConfig {
        &self.config
    }

    /// Plan the output filenames (without executing export).
    pub fn plan(&self) -> Vec<(String, ExportFormat, ExportScale)> {
        self.config
            .iter_targets()
            .into_iter()
            .map(|(fmt, scale)| {
                let filename =
                    self.config.naming.resolve(&self.config.base_name, &scale, fmt);
                (filename, fmt, scale)
            })
            .collect()
    }

    /// Execute a batch export using code generators only.
    ///
    /// For SVG/PDF, callers should use the existing `SvgExporter` /
    /// `PdfExporter` with the appropriate scaled page; this method
    /// handles the code-gen formats.
    pub fn export_code(
        &self,
        data: &crate::codegen::LayerStyleData,
    ) -> Vec<ExportArtifact> {
        let mut artifacts = Vec::new();

        for (fmt, scale) in self.config.iter_targets() {
            if fmt.is_code() {
                let target = match fmt {
                    ExportFormat::Css => crate::codegen::CodeGenTarget::Css,
                    ExportFormat::SwiftUI => crate::codegen::CodeGenTarget::SwiftUI,
                    ExportFormat::Compose => crate::codegen::CodeGenTarget::Compose,
                    _ => continue,
                };
                let generator = crate::codegen::generator_for(target);
                let code = generator.generate(data);
                let filename =
                    self.config.naming.resolve(&self.config.base_name, &scale, fmt);

                artifacts.push(ExportArtifact {
                    filename,
                    format: fmt,
                    scale,
                    data: code.into_bytes(),
                });
            }
        }

        artifacts
    }
}

// ═══════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_format_label_and_extension() {
        assert_eq!(ExportFormat::Svg.label(), "SVG");
        assert_eq!(ExportFormat::Svg.extension(), "svg");
        assert_eq!(ExportFormat::SwiftUI.extension(), "swift");
        assert_eq!(ExportFormat::Compose.extension(), "kt");
    }

    #[test]
    fn test_format_scalable_vs_code() {
        assert!(ExportFormat::Svg.is_scalable());
        assert!(ExportFormat::Pdf.is_scalable());
        assert!(!ExportFormat::Css.is_scalable());
        assert!(ExportFormat::Css.is_code());
        assert!(ExportFormat::SwiftUI.is_code());
        assert!(ExportFormat::Compose.is_code());
    }

    #[test]
    fn test_format_all() {
        assert_eq!(ExportFormat::all().len(), 5);
    }

    #[test]
    fn test_export_scale_constants() {
        assert_eq!(ExportScale::X1.factor, 1.0);
        assert_eq!(ExportScale::X1.dpi, 96.0);
        assert_eq!(ExportScale::X2.dpi, 192.0);
        assert_eq!(ExportScale::X3.suffix, "@3x");
    }

    #[test]
    fn test_scale_page() {
        let page = ExportPage::new(100.0, 200.0);
        let scaled = ExportScale::X2.scale_page(&page);
        assert_eq!(scaled.width, 200.0);
        assert_eq!(scaled.height, 400.0);
    }

    #[test]
    fn test_custom_scale() {
        let s = ExportScale::custom(4.0, "@4x");
        assert_eq!(s.factor, 4.0);
        assert_eq!(s.dpi, 384.0);
    }

    #[test]
    fn test_naming_suffix() {
        let s = NamingStrategy::Suffix;
        assert_eq!(
            s.resolve("icon", &ExportScale::X1, ExportFormat::Svg),
            "icon.svg"
        );
        assert_eq!(
            s.resolve("icon", &ExportScale::X2, ExportFormat::Svg),
            "icon@2x.svg"
        );
    }

    #[test]
    fn test_naming_directory() {
        let s = NamingStrategy::Directory;
        assert_eq!(
            s.resolve("icon", &ExportScale::X1, ExportFormat::Svg),
            "1x/icon.svg"
        );
        assert_eq!(
            s.resolve("icon", &ExportScale::X2, ExportFormat::Svg),
            "2x/icon.svg"
        );
    }

    #[test]
    fn test_config_builder() {
        let cfg = BatchExportConfig::new("my_icon", ExportFormat::Svg)
            .with_format(ExportFormat::Pdf)
            .with_apple_scales()
            .with_naming(NamingStrategy::Directory);
        assert_eq!(cfg.formats.len(), 2);
        assert_eq!(cfg.scales.len(), 3);
        assert_eq!(cfg.naming, NamingStrategy::Directory);
    }

    #[test]
    fn test_artifact_count_scalable_only() {
        let cfg = BatchExportConfig::new("icon", ExportFormat::Svg)
            .with_apple_scales();
        // 1 scalable format × 3 scales = 3
        assert_eq!(cfg.artifact_count(), 3);
    }

    #[test]
    fn test_artifact_count_mixed() {
        let cfg = BatchExportConfig::new("icon", ExportFormat::Svg)
            .with_format(ExportFormat::Css)
            .with_apple_scales();
        // Svg: 3 scales, Css: 1 (code is scale-independent)
        assert_eq!(cfg.artifact_count(), 4);
    }

    #[test]
    fn test_iter_targets_code_always_1x() {
        let cfg = BatchExportConfig::new("btn", ExportFormat::Css)
            .with_format(ExportFormat::SwiftUI)
            .with_apple_scales();
        let targets = cfg.iter_targets();
        assert_eq!(targets.len(), 2); // 2 code formats, each 1×
        for (_, scale) in &targets {
            assert_eq!(scale.factor, 1.0);
        }
    }

    #[test]
    fn test_batch_plan() {
        let cfg = BatchExportConfig::new("card", ExportFormat::Svg)
            .with_format(ExportFormat::Css)
            .with_apple_scales();
        let exporter = BatchExporter::new(cfg);
        let plan = exporter.plan();
        // 3 SVG (1x/2x/3x) + 1 CSS
        assert_eq!(plan.len(), 4);
        assert_eq!(plan[0].0, "card.svg");
        assert_eq!(plan[1].0, "card@2x.svg");
        assert_eq!(plan[2].0, "card@3x.svg");
        assert_eq!(plan[3].0, "card.css");
    }

    #[test]
    fn test_batch_export_code() {
        use crate::codegen::LayerStyleData;
        use logos_core::style::LayerStyle;

        let cfg = BatchExportConfig::new("header", ExportFormat::Css)
            .with_format(ExportFormat::SwiftUI);
        let exporter = BatchExporter::new(cfg);

        let data = LayerStyleData {
            layer_type: "rect".into(),
            name: "header".into(),
            x: 0.0,
            y: 0.0,
            width: 375.0,
            height: 64.0,
            style: LayerStyle::default(),
            text_content: None,
        };

        let artifacts = exporter.export_code(&data);
        assert_eq!(artifacts.len(), 2);
        let css = String::from_utf8_lossy(&artifacts[0].data);
        assert!(css.contains("width:"));
        let swift = String::from_utf8_lossy(&artifacts[1].data);
        assert!(swift.contains(".frame"));
    }
}
