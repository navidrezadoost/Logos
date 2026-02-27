//! Export pipeline — orchestrates the full export flow.
//!
//! The pipeline takes a set of layers, an export profile, and produces
//! a complete set of exported artifacts with optional packaging:
//!
//! 1. **Validate** — check inputs (non-empty, valid dimensions)
//! 2. **Resolve** — collect layout data for each layer
//! 3. **Export** — render to each target format/scale
//! 4. **Optimize** — apply SVG optimization if configured
//! 5. **Package** — bundle artifacts with manifest

use logos_core::Layer;
use logos_layout::engine::LayoutEngine;
use uuid::Uuid;

use crate::asset_package::{AssetManifest, AssetPackager, PackagedArtifact};
use crate::batch::{ExportFormat, ExportScale, NamingStrategy};
use crate::optimize::{SvgOptimizer, SvgOptimizerConfig, OptimizationLevel};
use crate::profile::ExportProfile;
use crate::svg::SvgExporter;
use crate::pdf::PdfExporter;
use crate::{collect_export_data, ExportError, ExportPage};

/// Result of a pipeline execution.
#[derive(Debug)]
pub struct PipelineResult {
    /// Successfully exported artifacts.
    pub artifacts: Vec<PackagedArtifact>,
    /// Optional asset manifest (if profile requests it).
    pub manifest: Option<AssetManifest>,
    /// Per-format export stats.
    pub stats: PipelineStats,
}

/// Aggregated statistics from the pipeline.
#[derive(Debug, Clone, Default)]
pub struct PipelineStats {
    pub total_artifacts: usize,
    pub total_bytes: usize,
    pub svg_count: usize,
    pub pdf_count: usize,
    pub errors: Vec<String>,
}

impl PipelineStats {
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

/// Progress callback for long-running exports.
pub type ProgressCallback = Box<dyn Fn(PipelineProgress)>;

/// Progress update.
#[derive(Debug, Clone)]
pub struct PipelineProgress {
    pub current: usize,
    pub total: usize,
    pub message: String,
}

impl PipelineProgress {
    pub fn percent(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        (self.current as f64 / self.total as f64) * 100.0
    }
}

/// Validation error for pipeline inputs.
#[derive(Debug, Clone)]
pub enum ValidationError {
    NoLayers,
    InvalidDimensions { width: f32, height: f32 },
    NoFormats,
    NoScales,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoLayers => write!(f, "No layers provided for export"),
            Self::InvalidDimensions { width, height } => {
                write!(f, "Invalid page dimensions: {width}×{height}")
            }
            Self::NoFormats => write!(f, "No export formats specified"),
            Self::NoScales => write!(f, "No export scales specified"),
        }
    }
}

/// The export pipeline.
pub struct ExportPipeline {
    page: ExportPage,
    profile: ExportProfile,
    project_name: String,
    naming: NamingStrategy,
}

impl ExportPipeline {
    pub fn new(page: ExportPage, profile: ExportProfile) -> Self {
        Self {
            page,
            profile,
            project_name: "export".to_string(),
            naming: NamingStrategy::Suffix,
        }
    }

    pub fn with_project_name(mut self, name: &str) -> Self {
        self.project_name = name.to_string();
        self
    }

    pub fn with_naming(mut self, naming: NamingStrategy) -> Self {
        self.naming = naming;
        self
    }

    /// Validate inputs before running the pipeline.
    pub fn validate(
        &self,
        layers: &[(Uuid, &Layer)],
    ) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();

        if layers.is_empty() {
            errors.push(ValidationError::NoLayers);
        }
        if self.page.width <= 0.0 || self.page.height <= 0.0 {
            errors.push(ValidationError::InvalidDimensions {
                width: self.page.width,
                height: self.page.height,
            });
        }
        if self.profile.formats.is_empty() {
            errors.push(ValidationError::NoFormats);
        }
        if self.profile.scales.is_empty() {
            errors.push(ValidationError::NoScales);
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Execute the full pipeline.
    pub fn execute(
        &self,
        engine: &LayoutEngine,
        layers: &[(Uuid, &Layer)],
    ) -> Result<PipelineResult, ExportError> {
        let _export_data = collect_export_data(engine, layers);
        let mut stats = PipelineStats::default();
        let mut packager = AssetPackager::new(&self.project_name)
            .with_naming(self.naming);

        let _total_ops = self.profile.formats.len() * self.profile.scales.len();

        for format in &self.profile.formats {
            for scale in &self.profile.scales {
                let name = &self.project_name;

                match self.export_single(format, scale, engine, layers) {
                    Ok(data) => {
                        let optimized = self.maybe_optimize(format, &data);
                        let source_id = layers.first().map(|(id, _)| *id).unwrap_or_else(Uuid::nil);
                        packager.add_artifact(
                            source_id,
                            name,
                            *format,
                            *scale,
                            optimized,
                        );

                        match format {
                            ExportFormat::Svg => stats.svg_count += 1,
                            ExportFormat::Pdf => stats.pdf_count += 1,
                            _ => {}
                        }
                    }
                    Err(e) => {
                        stats.errors.push(format!(
                            "Failed {format:?} @{scale:?}: {e}"
                        ));
                    }
                }
            }
        }

        let (manifest, artifacts) = packager.finalize();
        stats.total_artifacts = artifacts.len();
        stats.total_bytes = artifacts.iter().map(|a| a.data.len()).sum();

        Ok(PipelineResult {
            artifacts,
            manifest: if self.profile.include_manifest {
                Some(manifest)
            } else {
                None
            },
            stats,
        })
    }

    fn export_single(
        &self,
        format: &ExportFormat,
        _scale: &ExportScale,
        engine: &LayoutEngine,
        layers: &[(Uuid, &Layer)],
    ) -> Result<Vec<u8>, ExportError> {
        match format {
            ExportFormat::Svg => {
                let exporter = SvgExporter::new(self.page.clone());
                let svg = exporter.export_to_string(engine, layers)?;
                Ok(svg.into_bytes())
            }
            ExportFormat::Pdf => {
                let exporter = PdfExporter::new(self.page.clone());
                exporter.export_to_bytes(engine, layers)
            }
            ExportFormat::Css | ExportFormat::SwiftUI | ExportFormat::Compose => {
                // Code generation formats produce text
                Ok(format!("/* {format:?} export placeholder */").into_bytes())
            }
        }
    }

    fn maybe_optimize(&self, format: &ExportFormat, data: &[u8]) -> Vec<u8> {
        if *format != ExportFormat::Svg {
            return data.to_vec();
        }
        if self.profile.optimization == OptimizationLevel::None {
            return data.to_vec();
        }

        let svg = String::from_utf8_lossy(data);
        let config = match self.profile.optimization {
            OptimizationLevel::Aggressive => SvgOptimizerConfig::aggressive(),
            OptimizationLevel::Safe => SvgOptimizerConfig::default(),
            OptimizationLevel::None => return data.to_vec(),
        };
        let optimizer = SvgOptimizer::new(config);
        let (optimized, _stats) = optimizer.optimize(&svg);
        optimized.into_bytes()
    }
}

// ═══════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::ExportProfile;
    use logos_core::{Layer, RectLayer};

    fn make_test_layer_with_engine() -> (LayoutEngine, Uuid, Layer) {
        let layer = Layer::Rect(RectLayer::new(0.0, 0.0, 100.0, 50.0));
        let id = layer.id();
        let mut engine = LayoutEngine::new();
        engine.add_or_update_layer(&layer).unwrap();
        engine.compute_layout(id).unwrap();
        (engine, id, layer)
    }

    fn make_test_layer() -> (Uuid, Layer) {
        let layer = Layer::Rect(RectLayer::new(0.0, 0.0, 100.0, 50.0));
        let id = layer.id();
        (id, layer)
    }

    #[test]
    fn pipeline_creation() {
        let page = ExportPage::new(800.0, 600.0);
        let profile = ExportProfile::web();
        let pipeline = ExportPipeline::new(page, profile);
        assert_eq!(pipeline.project_name, "export");
    }

    #[test]
    fn pipeline_with_project_name() {
        let page = ExportPage::new(800.0, 600.0);
        let profile = ExportProfile::web();
        let pipeline = ExportPipeline::new(page, profile)
            .with_project_name("my-design");
        assert_eq!(pipeline.project_name, "my-design");
    }

    #[test]
    fn validate_empty_layers() {
        let page = ExportPage::new(800.0, 600.0);
        let profile = ExportProfile::web();
        let pipeline = ExportPipeline::new(page, profile);
        let result = pipeline.validate(&[]);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| matches!(e, ValidationError::NoLayers)));
    }

    #[test]
    fn validate_invalid_dimensions() {
        let page = ExportPage::new(-1.0, 0.0);
        let profile = ExportProfile::web();
        let pipeline = ExportPipeline::new(page, profile);
        let (id, layer) = make_test_layer();
        let layers = vec![(id, &layer)];
        let result = pipeline.validate(&layers);
        assert!(result.is_err());
    }

    #[test]
    fn validate_valid_input() {
        let page = ExportPage::new(800.0, 600.0);
        let profile = ExportProfile::web();
        let pipeline = ExportPipeline::new(page, profile);
        let (id, layer) = make_test_layer();
        let layers = vec![(id, &layer)];
        assert!(pipeline.validate(&layers).is_ok());
    }

    #[test]
    fn execute_svg_pipeline() {
        let page = ExportPage::new(200.0, 100.0);
        let profile = ExportProfile::custom("test")
            .with_formats(vec![ExportFormat::Svg])
            .with_scales(vec![ExportScale::X1]);
        let pipeline = ExportPipeline::new(page, profile);
        let (engine, id, layer) = make_test_layer_with_engine();
        let layers = vec![(id, &layer)];
        let result = pipeline.execute(&engine, &layers).unwrap();
        assert_eq!(result.stats.total_artifacts, 1);
        assert!(result.stats.svg_count >= 1);
        assert!(!result.artifacts.is_empty());
    }

    #[test]
    fn execute_pdf_pipeline() {
        let page = ExportPage::new(200.0, 100.0);
        let profile = ExportProfile::custom("test")
            .with_formats(vec![ExportFormat::Pdf])
            .with_scales(vec![ExportScale::X1]);
        let pipeline = ExportPipeline::new(page, profile);
        let (engine, id, layer) = make_test_layer_with_engine();
        let layers = vec![(id, &layer)];
        let result = pipeline.execute(&engine, &layers).unwrap();
        assert_eq!(result.stats.pdf_count, 1);
    }

    #[test]
    fn execute_multi_format_pipeline() {
        let page = ExportPage::new(200.0, 100.0);
        let profile = ExportProfile::custom("test")
            .with_formats(vec![ExportFormat::Svg, ExportFormat::Pdf])
            .with_scales(vec![ExportScale::X1, ExportScale::X2]);
        let pipeline = ExportPipeline::new(page, profile);
        let (engine, id, layer) = make_test_layer_with_engine();
        let layers = vec![(id, &layer)];
        let result = pipeline.execute(&engine, &layers).unwrap();
        // 2 formats × 2 scales = 4 artifacts
        assert_eq!(result.stats.total_artifacts, 4);
    }

    #[test]
    fn execute_with_manifest() {
        let page = ExportPage::new(200.0, 100.0);
        let profile = ExportProfile::custom("test")
            .with_formats(vec![ExportFormat::Svg])
            .with_scales(vec![ExportScale::X1])
            .with_manifest(true);
        let pipeline = ExportPipeline::new(page, profile);
        let (engine, id, layer) = make_test_layer_with_engine();
        let layers = vec![(id, &layer)];
        let result = pipeline.execute(&engine, &layers).unwrap();
        assert!(result.manifest.is_some());
    }

    #[test]
    fn execute_without_manifest() {
        let page = ExportPage::new(200.0, 100.0);
        let profile = ExportProfile::web(); // no manifest
        let pipeline = ExportPipeline::new(page, profile);
        let (engine, id, layer) = make_test_layer_with_engine();
        let layers = vec![(id, &layer)];
        let result = pipeline.execute(&engine, &layers).unwrap();
        assert!(result.manifest.is_none());
    }

    #[test]
    fn pipeline_progress_percent() {
        let p = PipelineProgress { current: 3, total: 10, message: "test".into() };
        assert!((p.percent() - 30.0).abs() < 0.1);
    }

    #[test]
    fn pipeline_progress_zero_total() {
        let p = PipelineProgress { current: 0, total: 0, message: String::new() };
        assert!((p.percent() - 0.0).abs() < 0.1);
    }

    #[test]
    fn validation_error_display() {
        let e = ValidationError::NoLayers;
        assert!(e.to_string().contains("No layers"));
        let e2 = ValidationError::InvalidDimensions { width: -1.0, height: 0.0 };
        assert!(e2.to_string().contains("-1"));
    }

    #[test]
    fn stats_has_errors() {
        let mut stats = PipelineStats::default();
        assert!(!stats.has_errors());
        stats.errors.push("fail".into());
        assert!(stats.has_errors());
    }
}
