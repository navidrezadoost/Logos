//! Model quantization — precision reduction for size and speed optimization.
//!
//! Supports:
//! - **FP16** — Half-precision float (2× smaller, negligible speed impact)
//! - **INT8** — 8-bit integer with scale/zero-point (4× smaller, potential 2× faster)
//! - **Model analysis** — Inspect and report on model precision and size
//!
//! # Architecture
//!
//! ```text
//! ┌───────────────────┐   ┌────────────────┐   ┌─────────────┐
//! │  QuantizedModel   │──▶│ ModelPrecision │──▶│ SizeReport  │
//! │  (load/validate)  │   │ (FP32/FP16/I8) │   │ (analyze)   │
//! └───────────────────┘   └────────────────┘   └─────────────┘
//! ```

use crate::error::{AiError, AiResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Precision level for model weights.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModelPrecision {
    /// Full 32-bit float (default).
    FP32,
    /// Half-precision 16-bit float.
    FP16,
    /// 8-bit unsigned integer with quantization parameters.
    INT8,
    /// Mixed precision (some layers FP32, others FP16/INT8).
    Mixed,
}

impl ModelPrecision {
    /// Bytes per element for this precision.
    pub fn bytes_per_element(&self) -> usize {
        match self {
            ModelPrecision::FP32 => 4,
            ModelPrecision::FP16 => 2,
            ModelPrecision::INT8 => 1,
            ModelPrecision::Mixed => 4, // Conservative estimate
        }
    }

    /// Expected size reduction ratio vs FP32.
    pub fn expected_reduction(&self) -> f64 {
        match self {
            ModelPrecision::FP32 => 1.0,
            ModelPrecision::FP16 => 2.0,
            ModelPrecision::INT8 => 4.0,
            ModelPrecision::Mixed => 1.5,
        }
    }

    /// Human-readable label.
    pub fn label(&self) -> &str {
        match self {
            ModelPrecision::FP32 => "FP32",
            ModelPrecision::FP16 => "FP16",
            ModelPrecision::INT8 => "INT8",
            ModelPrecision::Mixed => "Mixed",
        }
    }

    /// File suffix convention for quantized models.
    pub fn suffix(&self) -> &str {
        match self {
            ModelPrecision::FP32 => "",
            ModelPrecision::FP16 => "_fp16",
            ModelPrecision::INT8 => "_int8",
            ModelPrecision::Mixed => "_mixed",
        }
    }

    /// Parse from suffix string.
    pub fn from_suffix(s: &str) -> Option<Self> {
        match s {
            "" => Some(ModelPrecision::FP32),
            "_fp16" | "fp16" => Some(ModelPrecision::FP16),
            "_int8" | "int8" => Some(ModelPrecision::INT8),
            "_mixed" | "mixed" => Some(ModelPrecision::Mixed),
            _ => None,
        }
    }
}

impl std::fmt::Display for ModelPrecision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// Information about a quantized model variant.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QuantizedModelInfo {
    /// Base model name (without precision suffix).
    pub base_name: String,
    /// Precision of this variant.
    pub precision: ModelPrecision,
    /// File size in bytes.
    pub size_bytes: u64,
    /// Path to the quantized model file.
    pub path: PathBuf,
    /// Size reduction ratio vs the FP32 original.
    pub reduction_ratio: f64,
    /// Whether this model has been validated (loads and runs successfully).
    pub validated: bool,
}

/// Size comparison report across precision levels.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SizeReport {
    /// Model name.
    pub model_name: String,
    /// Size of each variant.
    pub variants: Vec<QuantizedModelInfo>,
    /// Total size reduction from FP32 → smallest variant.
    pub best_reduction: f64,
}

impl SizeReport {
    /// Get the smallest variant.
    pub fn smallest(&self) -> Option<&QuantizedModelInfo> {
        self.variants.iter().min_by_key(|v| v.size_bytes)
    }

    /// Get variant by precision level.
    pub fn get(&self, precision: ModelPrecision) -> Option<&QuantizedModelInfo> {
        self.variants.iter().find(|v| v.precision == precision)
    }

    /// Format as a human-readable table.
    pub fn format_table(&self) -> String {
        let mut out = format!("Model: {}\n", self.model_name);
        out.push_str(&format!(
            "{:<10} {:>12} {:>10}\n",
            "Precision", "Size (bytes)", "Reduction"
        ));
        out.push_str(&"-".repeat(34));
        out.push('\n');
        for v in &self.variants {
            out.push_str(&format!(
                "{:<10} {:>12} {:>9.2}x\n",
                v.precision.label(),
                v.size_bytes,
                v.reduction_ratio
            ));
        }
        out
    }
}

/// Manager for quantized model variants.
///
/// Discovers and manages FP32/FP16/INT8 versions of models.
/// Provides automatic fallback: prefer INT8 → FP16 → FP32.
pub struct QuantizationManager {
    /// Base directory containing models.
    models_dir: PathBuf,
    /// Known model variants.
    variants: Vec<QuantizedModelInfo>,
}

impl QuantizationManager {
    /// Create a new manager for the given directory.
    pub fn new(models_dir: impl Into<PathBuf>) -> Self {
        Self {
            models_dir: models_dir.into(),
            variants: Vec::new(),
        }
    }

    /// Scan the models directory for all variants of a given base model.
    pub fn scan_variants(&mut self, base_name: &str) -> AiResult<Vec<QuantizedModelInfo>> {
        let mut found = Vec::new();

        for precision in &[
            ModelPrecision::FP32,
            ModelPrecision::FP16,
            ModelPrecision::INT8,
        ] {
            let filename = format!("{}{}.onnx", base_name, precision.suffix());
            let path = self.models_dir.join(&filename);

            if path.exists() {
                let size = std::fs::metadata(&path)
                    .map(|m| m.len())
                    .map_err(AiError::Io)?;

                let info = QuantizedModelInfo {
                    base_name: base_name.to_string(),
                    precision: *precision,
                    size_bytes: size,
                    path,
                    reduction_ratio: 1.0, // Will be computed below
                    validated: false,
                };
                found.push(info);
            }
        }

        // Compute reduction ratios relative to FP32
        let fp32_size = found
            .iter()
            .find(|v| v.precision == ModelPrecision::FP32)
            .map(|v| v.size_bytes)
            .unwrap_or(1);

        for variant in &mut found {
            variant.reduction_ratio = if variant.size_bytes > 0 {
                fp32_size as f64 / variant.size_bytes as f64
            } else {
                0.0
            };
        }

        self.variants.extend(found.clone());
        Ok(found)
    }

    /// Get the best (smallest validated) variant for a model.
    /// Priority: INT8 → FP16 → FP32.
    pub fn best_variant(&self, base_name: &str) -> Option<&QuantizedModelInfo> {
        let candidates: Vec<_> = self
            .variants
            .iter()
            .filter(|v| v.base_name == base_name)
            .collect();

        // Prefer smallest validated, then smallest unvalidated
        candidates
            .iter()
            .filter(|v| v.validated)
            .min_by_key(|v| v.size_bytes)
            .copied()
            .or_else(|| {
                // Fallback: prefer by precision priority
                for prec in &[
                    ModelPrecision::INT8,
                    ModelPrecision::FP16,
                    ModelPrecision::FP32,
                ] {
                    if let Some(v) = candidates.iter().find(|v| v.precision == *prec) {
                        return Some(*v);
                    }
                }
                None
            })
    }

    /// Get a specific variant by name and precision.
    pub fn get_variant(
        &self,
        base_name: &str,
        precision: ModelPrecision,
    ) -> Option<&QuantizedModelInfo> {
        self.variants
            .iter()
            .find(|v| v.base_name == base_name && v.precision == precision)
    }

    /// Mark a variant as validated (confirmed loadable and runnable).
    pub fn mark_validated(&mut self, base_name: &str, precision: ModelPrecision) {
        for v in &mut self.variants {
            if v.base_name == base_name && v.precision == precision {
                v.validated = true;
            }
        }
    }

    /// Generate a size report for a given model across all discovered variants.
    pub fn size_report(&self, base_name: &str) -> SizeReport {
        let variants: Vec<_> = self
            .variants
            .iter()
            .filter(|v| v.base_name == base_name)
            .cloned()
            .collect();

        let best_reduction = variants
            .iter()
            .map(|v| v.reduction_ratio)
            .fold(1.0f64, f64::max);

        SizeReport {
            model_name: base_name.to_string(),
            variants,
            best_reduction,
        }
    }

    /// Number of tracked variants.
    pub fn variant_count(&self) -> usize {
        self.variants.len()
    }

    /// All tracked variants.
    pub fn all_variants(&self) -> &[QuantizedModelInfo] {
        &self.variants
    }

    /// Models directory path.
    pub fn models_dir(&self) -> &Path {
        &self.models_dir
    }

    /// Load the best model bytes for a given base name.
    /// Returns (bytes, precision) — prefers smallest validated model.
    pub fn load_best_bytes(
        &self,
        base_name: &str,
    ) -> AiResult<(Vec<u8>, ModelPrecision)> {
        let variant = self.best_variant(base_name).ok_or_else(|| {
            AiError::ModelNotFound(format!("no variant found for '{base_name}'"))
        })?;
        let bytes = std::fs::read(&variant.path).map_err(AiError::Io)?;
        Ok((bytes, variant.precision))
    }
}

/// Validate an ONNX model file on disk (checks it exists and has valid header).
pub fn validate_onnx_file(path: &Path) -> AiResult<u64> {
    if !path.exists() {
        return Err(AiError::ModelNotFound(path.display().to_string()));
    }

    let metadata = std::fs::metadata(path).map_err(AiError::Io)?;
    let size = metadata.len();

    if size < 8 {
        return Err(AiError::ModelLoadFailed(format!(
            "file too small ({size} bytes): {}",
            path.display()
        )));
    }

    // Check ONNX protobuf magic (first few bytes)
    let bytes = std::fs::read(path).map_err(AiError::Io)?;
    // ONNX files start with protobuf encoding — a valid ONNX model
    // begins with field tags. We just verify it's non-empty and reasonable.
    if bytes[0] == 0 && bytes[1] == 0 {
        return Err(AiError::ModelLoadFailed(format!(
            "invalid ONNX header: {}",
            path.display()
        )));
    }

    Ok(size)
}

/// Detect the precision of an ONNX model by examining the file name suffix.
pub fn detect_precision(path: &Path) -> ModelPrecision {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    if stem.ends_with("_fp16") {
        ModelPrecision::FP16
    } else if stem.ends_with("_int8") {
        ModelPrecision::INT8
    } else if stem.ends_with("_mixed") {
        ModelPrecision::Mixed
    } else {
        ModelPrecision::FP32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // ── ModelPrecision ──

    #[test]
    fn test_precision_bytes_per_element() {
        assert_eq!(ModelPrecision::FP32.bytes_per_element(), 4);
        assert_eq!(ModelPrecision::FP16.bytes_per_element(), 2);
        assert_eq!(ModelPrecision::INT8.bytes_per_element(), 1);
        assert_eq!(ModelPrecision::Mixed.bytes_per_element(), 4);
    }

    #[test]
    fn test_precision_expected_reduction() {
        assert_eq!(ModelPrecision::FP32.expected_reduction(), 1.0);
        assert_eq!(ModelPrecision::FP16.expected_reduction(), 2.0);
        assert_eq!(ModelPrecision::INT8.expected_reduction(), 4.0);
    }

    #[test]
    fn test_precision_label() {
        assert_eq!(ModelPrecision::FP32.label(), "FP32");
        assert_eq!(ModelPrecision::FP16.label(), "FP16");
        assert_eq!(ModelPrecision::INT8.label(), "INT8");
        assert_eq!(ModelPrecision::Mixed.label(), "Mixed");
    }

    #[test]
    fn test_precision_suffix() {
        assert_eq!(ModelPrecision::FP32.suffix(), "");
        assert_eq!(ModelPrecision::FP16.suffix(), "_fp16");
        assert_eq!(ModelPrecision::INT8.suffix(), "_int8");
        assert_eq!(ModelPrecision::Mixed.suffix(), "_mixed");
    }

    #[test]
    fn test_precision_from_suffix() {
        assert_eq!(ModelPrecision::from_suffix(""), Some(ModelPrecision::FP32));
        assert_eq!(ModelPrecision::from_suffix("fp16"), Some(ModelPrecision::FP16));
        assert_eq!(ModelPrecision::from_suffix("_fp16"), Some(ModelPrecision::FP16));
        assert_eq!(ModelPrecision::from_suffix("int8"), Some(ModelPrecision::INT8));
        assert_eq!(ModelPrecision::from_suffix("_int8"), Some(ModelPrecision::INT8));
        assert_eq!(ModelPrecision::from_suffix("_mixed"), Some(ModelPrecision::Mixed));
        assert_eq!(ModelPrecision::from_suffix("unknown"), None);
    }

    #[test]
    fn test_precision_display() {
        assert_eq!(format!("{}", ModelPrecision::FP32), "FP32");
        assert_eq!(format!("{}", ModelPrecision::INT8), "INT8");
    }

    // ── detect_precision ──

    #[test]
    fn test_detect_precision_fp32() {
        assert_eq!(
            detect_precision(Path::new("model.onnx")),
            ModelPrecision::FP32
        );
        assert_eq!(
            detect_precision(Path::new("/path/to/layout_gen.onnx")),
            ModelPrecision::FP32
        );
    }

    #[test]
    fn test_detect_precision_fp16() {
        assert_eq!(
            detect_precision(Path::new("model_fp16.onnx")),
            ModelPrecision::FP16
        );
        assert_eq!(
            detect_precision(Path::new("/path/to/layout_gen_fp16.onnx")),
            ModelPrecision::FP16
        );
    }

    #[test]
    fn test_detect_precision_int8() {
        assert_eq!(
            detect_precision(Path::new("model_int8.onnx")),
            ModelPrecision::INT8
        );
    }

    #[test]
    fn test_detect_precision_mixed() {
        assert_eq!(
            detect_precision(Path::new("model_mixed.onnx")),
            ModelPrecision::Mixed
        );
    }

    // ── validate_onnx_file ──

    #[test]
    fn test_validate_existing_model() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("test-models")
            .join("layout_gen.onnx");
        if path.exists() {
            let size = validate_onnx_file(&path).unwrap();
            assert!(size > 0);
        }
    }

    #[test]
    fn test_validate_missing_file() {
        let result = validate_onnx_file(Path::new("/nonexistent/model.onnx"));
        assert!(result.is_err());
    }

    // ── QuantizationManager ──

    #[test]
    fn test_manager_new() {
        let mgr = QuantizationManager::new("/tmp/models");
        assert_eq!(mgr.variant_count(), 0);
        assert_eq!(mgr.models_dir(), Path::new("/tmp/models"));
    }

    #[test]
    fn test_manager_scan_variants() {
        let test_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-models");
        let mut mgr = QuantizationManager::new(&test_dir);

        let variants = mgr.scan_variants("layout_gen").unwrap();
        // Should find at least FP32 (always present)
        assert!(!variants.is_empty());

        // Check FP32 is present
        assert!(variants.iter().any(|v| v.precision == ModelPrecision::FP32));

        // If quantized models exist, check them
        if variants.len() > 1 {
            let fp32 = variants
                .iter()
                .find(|v| v.precision == ModelPrecision::FP32)
                .unwrap();
            for v in &variants {
                if v.precision != ModelPrecision::FP32 {
                    assert!(v.size_bytes < fp32.size_bytes);
                    assert!(v.reduction_ratio > 1.0);
                }
            }
        }
    }

    #[test]
    fn test_manager_scan_all_three_models() {
        let test_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-models");
        let mut mgr = QuantizationManager::new(&test_dir);

        for base in &["layout_gen", "style_encoder", "asset_decoder"] {
            let variants = mgr.scan_variants(base).unwrap();
            assert!(!variants.is_empty(), "no variants found for {base}");
        }

        // Should have found variants for all models
        assert!(mgr.variant_count() >= 3);
    }

    #[test]
    fn test_manager_best_variant_prefers_smallest() {
        let test_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-models");
        let mut mgr = QuantizationManager::new(&test_dir);
        mgr.scan_variants("layout_gen").unwrap();

        let best = mgr.best_variant("layout_gen");
        assert!(best.is_some());

        // If INT8 exists, it should be preferred (smallest)
        if mgr
            .get_variant("layout_gen", ModelPrecision::INT8)
            .is_some()
        {
            assert_eq!(best.unwrap().precision, ModelPrecision::INT8);
        }
    }

    #[test]
    fn test_manager_get_variant() {
        let test_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-models");
        let mut mgr = QuantizationManager::new(&test_dir);
        mgr.scan_variants("layout_gen").unwrap();

        let fp32 = mgr.get_variant("layout_gen", ModelPrecision::FP32);
        assert!(fp32.is_some());
        assert_eq!(fp32.unwrap().precision, ModelPrecision::FP32);
    }

    #[test]
    fn test_manager_mark_validated() {
        let test_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-models");
        let mut mgr = QuantizationManager::new(&test_dir);
        mgr.scan_variants("layout_gen").unwrap();

        mgr.mark_validated("layout_gen", ModelPrecision::FP32);

        let fp32 = mgr.get_variant("layout_gen", ModelPrecision::FP32).unwrap();
        assert!(fp32.validated);
    }

    #[test]
    fn test_manager_best_variant_prefers_validated() {
        let test_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-models");
        let mut mgr = QuantizationManager::new(&test_dir);
        mgr.scan_variants("layout_gen").unwrap();

        // Mark FP16 as validated — it should be preferred over unvalidated INT8
        mgr.mark_validated("layout_gen", ModelPrecision::FP16);

        let best = mgr.best_variant("layout_gen");
        assert!(best.is_some());
        // Should prefer validated FP16 over unvalidated INT8
        if mgr
            .get_variant("layout_gen", ModelPrecision::FP16)
            .is_some()
        {
            assert_eq!(best.unwrap().precision, ModelPrecision::FP16);
        }
    }

    #[test]
    fn test_manager_size_report() {
        let test_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-models");
        let mut mgr = QuantizationManager::new(&test_dir);
        mgr.scan_variants("layout_gen").unwrap();

        let report = mgr.size_report("layout_gen");
        assert_eq!(report.model_name, "layout_gen");
        assert!(!report.variants.is_empty());
        assert!(report.best_reduction >= 1.0);

        // Format table should not panic
        let table = report.format_table();
        assert!(table.contains("layout_gen"));
    }

    #[test]
    fn test_manager_load_best_bytes() {
        let test_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-models");
        let mut mgr = QuantizationManager::new(&test_dir);
        mgr.scan_variants("layout_gen").unwrap();

        let (bytes, precision) = mgr.load_best_bytes("layout_gen").unwrap();
        assert!(!bytes.is_empty());
        assert!(
            precision == ModelPrecision::INT8
                || precision == ModelPrecision::FP16
                || precision == ModelPrecision::FP32
        );
    }

    #[test]
    fn test_manager_load_best_bytes_missing() {
        let mgr = QuantizationManager::new("/tmp/nonexistent");
        let result = mgr.load_best_bytes("nonexistent");
        assert!(result.is_err());
    }

    // ── SizeReport ──

    #[test]
    fn test_size_report_smallest() {
        let test_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-models");
        let mut mgr = QuantizationManager::new(&test_dir);
        mgr.scan_variants("layout_gen").unwrap();

        let report = mgr.size_report("layout_gen");
        let smallest = report.smallest();
        assert!(smallest.is_some());

        // Smallest should have the highest reduction ratio
        for v in &report.variants {
            assert!(smallest.unwrap().size_bytes <= v.size_bytes);
        }
    }

    #[test]
    fn test_size_report_get_by_precision() {
        let test_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-models");
        let mut mgr = QuantizationManager::new(&test_dir);
        mgr.scan_variants("layout_gen").unwrap();

        let report = mgr.size_report("layout_gen");
        let fp32 = report.get(ModelPrecision::FP32);
        assert!(fp32.is_some());
        assert_eq!(fp32.unwrap().precision, ModelPrecision::FP32);
    }

    // ── QuantizedModelInfo serialization ──

    #[test]
    fn test_quantized_model_info_serialization() {
        let info = QuantizedModelInfo {
            base_name: "layout_gen".to_string(),
            precision: ModelPrecision::FP16,
            size_bytes: 48000,
            path: PathBuf::from("/models/layout_gen_fp16.onnx"),
            reduction_ratio: 2.0,
            validated: true,
        };
        let json = serde_json::to_string(&info).unwrap();
        let back: QuantizedModelInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back.base_name, "layout_gen");
        assert_eq!(back.precision, ModelPrecision::FP16);
        assert_eq!(back.size_bytes, 48000);
        assert!(back.validated);
    }

    #[test]
    fn test_model_precision_serialization() {
        for prec in &[
            ModelPrecision::FP32,
            ModelPrecision::FP16,
            ModelPrecision::INT8,
            ModelPrecision::Mixed,
        ] {
            let json = serde_json::to_string(prec).unwrap();
            let back: ModelPrecision = serde_json::from_str(&json).unwrap();
            assert_eq!(&back, prec);
        }
    }
}
