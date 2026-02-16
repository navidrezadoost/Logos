//! WASM compatibility layer for model loading and inference.
//!
//! Provides platform-agnostic abstractions that work in both native
//! and WebAssembly environments.
//!
//! # Platform Detection
//!
//! ```text
//! ┌──────────────────────────────────────────────────┐
//! │                 WasmRuntime                      │
//! │                                                  │
//! │  Native:  ort::Session       (full ONNX Runtime) │
//! │  WASM:    SimulatedSession   (fallback)          │
//! │  Future:  ort-wasm           (WASM SIMD backend) │
//! └──────────────────────────────────────────────────┘
//! ```

use crate::error::{AiError, AiResult};
use crate::models::quantization::ModelPrecision;
use serde::{Deserialize, Serialize};

/// Target platform for deployment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Platform {
    /// Native x86_64/ARM64 with full ONNX Runtime.
    Native,
    /// WebAssembly (browser or WASI).
    Wasm,
    /// WebAssembly with SIMD extensions.
    WasmSimd,
}

impl Platform {
    /// Detect the current platform at compile time.
    pub fn current() -> Self {
        if cfg!(target_arch = "wasm32") {
            if cfg!(target_feature = "simd128") {
                Platform::WasmSimd
            } else {
                Platform::Wasm
            }
        } else {
            Platform::Native
        }
    }

    /// Whether ONNX Runtime is available on this platform.
    pub fn has_onnx_runtime(&self) -> bool {
        matches!(self, Platform::Native)
    }

    /// Maximum recommended model size in bytes for this platform.
    pub fn max_model_size(&self) -> u64 {
        match self {
            Platform::Native => 2 * 1024 * 1024 * 1024,   // 2 GB
            Platform::WasmSimd => 50 * 1024 * 1024,        // 50 MB
            Platform::Wasm => 10 * 1024 * 1024,            // 10 MB
        }
    }

    /// Recommended precision for this platform.
    pub fn recommended_precision(&self) -> ModelPrecision {
        match self {
            Platform::Native => ModelPrecision::FP32,
            Platform::WasmSimd => ModelPrecision::FP16,
            Platform::Wasm => ModelPrecision::INT8,
        }
    }

    /// Label for this platform.
    pub fn label(&self) -> &str {
        match self {
            Platform::Native => "native",
            Platform::Wasm => "wasm32",
            Platform::WasmSimd => "wasm32-simd",
        }
    }
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// WASM deployment constraints for model packaging.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WasmConstraints {
    /// Maximum total model size in bytes.
    pub max_total_size: u64,
    /// Maximum individual model size in bytes.
    pub max_model_size: u64,
    /// Required precision level.
    pub required_precision: ModelPrecision,
    /// Whether SIMD is expected to be available.
    pub simd_available: bool,
}

impl Default for WasmConstraints {
    fn default() -> Self {
        Self {
            max_total_size: 5 * 1024 * 1024, // 5 MB total
            max_model_size: 3 * 1024 * 1024,  // 3 MB per model
            required_precision: ModelPrecision::INT8,
            simd_available: false,
        }
    }
}

impl WasmConstraints {
    /// Create constraints for a SIMD-enabled WASM target.
    pub fn with_simd() -> Self {
        Self {
            max_total_size: 20 * 1024 * 1024,  // 20 MB
            max_model_size: 10 * 1024 * 1024,   // 10 MB per model
            required_precision: ModelPrecision::FP16,
            simd_available: true,
        }
    }

    /// Check if a model meets these constraints.
    pub fn validate_model(&self, name: &str, size: u64, precision: ModelPrecision) -> AiResult<()> {
        if size > self.max_model_size {
            return Err(AiError::ResourceLimit(format!(
                "model '{name}' ({size} bytes) exceeds WASM limit ({} bytes)",
                self.max_model_size
            )));
        }

        // Check precision is at least as compact as required
        if precision.bytes_per_element() > self.required_precision.bytes_per_element() {
            return Err(AiError::InvalidInput(format!(
                "model '{name}' uses {precision} but WASM requires {} or smaller",
                self.required_precision
            )));
        }

        Ok(())
    }

    /// Check if a batch of models (by sizes) fits within total budget.
    pub fn validate_total(&self, models: &[(&str, u64)]) -> AiResult<()> {
        let total: u64 = models.iter().map(|(_, s)| s).sum();
        if total > self.max_total_size {
            return Err(AiError::ResourceLimit(format!(
                "total model size ({total} bytes) exceeds WASM budget ({} bytes)",
                self.max_total_size
            )));
        }
        Ok(())
    }
}

/// Report on WASM deployment readiness.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WasmReadinessReport {
    /// Platform target.
    pub platform: Platform,
    /// Constraints applied.
    pub constraints: WasmConstraints,
    /// Per-model status.
    pub model_status: Vec<WasmModelStatus>,
    /// Overall readiness.
    pub ready: bool,
    /// Total size of WASM-compatible models.
    pub total_size: u64,
}

/// Status of a single model for WASM deployment.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WasmModelStatus {
    /// Model name.
    pub name: String,
    /// Current size.
    pub size_bytes: u64,
    /// Current precision.
    pub precision: ModelPrecision,
    /// Whether it meets WASM constraints.
    pub compatible: bool,
    /// Issue description (if not compatible).
    pub issue: Option<String>,
}

/// Check WASM readiness for a set of models.
pub fn check_wasm_readiness(
    models: &[(&str, u64, ModelPrecision)],
    constraints: &WasmConstraints,
) -> WasmReadinessReport {
    let mut statuses = Vec::new();
    let mut all_ok = true;

    for (name, size, precision) in models {
        let result = constraints.validate_model(name, *size, *precision);
        let (compatible, issue) = match result {
            Ok(()) => (true, None),
            Err(e) => {
                all_ok = false;
                (false, Some(e.to_string()))
            }
        };

        statuses.push(WasmModelStatus {
            name: name.to_string(),
            size_bytes: *size,
            precision: *precision,
            compatible,
            issue,
        });
    }

    // Check total size
    let total: u64 = models.iter().map(|(_, s, _)| s).sum();
    if total > constraints.max_total_size {
        all_ok = false;
    }

    WasmReadinessReport {
        platform: if constraints.simd_available {
            Platform::WasmSimd
        } else {
            Platform::Wasm
        },
        constraints: constraints.clone(),
        model_status: statuses,
        ready: all_ok,
        total_size: total,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Platform ──

    #[test]
    fn test_platform_current() {
        // On native, should be Native
        let p = Platform::current();
        assert_eq!(p, Platform::Native);
    }

    #[test]
    fn test_platform_has_onnx_runtime() {
        assert!(Platform::Native.has_onnx_runtime());
        assert!(!Platform::Wasm.has_onnx_runtime());
        assert!(!Platform::WasmSimd.has_onnx_runtime());
    }

    #[test]
    fn test_platform_max_model_size() {
        assert!(Platform::Native.max_model_size() > Platform::Wasm.max_model_size());
        assert!(Platform::WasmSimd.max_model_size() > Platform::Wasm.max_model_size());
    }

    #[test]
    fn test_platform_recommended_precision() {
        assert_eq!(Platform::Native.recommended_precision(), ModelPrecision::FP32);
        assert_eq!(Platform::WasmSimd.recommended_precision(), ModelPrecision::FP16);
        assert_eq!(Platform::Wasm.recommended_precision(), ModelPrecision::INT8);
    }

    #[test]
    fn test_platform_label() {
        assert_eq!(Platform::Native.label(), "native");
        assert_eq!(Platform::Wasm.label(), "wasm32");
        assert_eq!(Platform::WasmSimd.label(), "wasm32-simd");
    }

    #[test]
    fn test_platform_display() {
        assert_eq!(format!("{}", Platform::Native), "native");
    }

    #[test]
    fn test_platform_serialization() {
        for p in &[Platform::Native, Platform::Wasm, Platform::WasmSimd] {
            let json = serde_json::to_string(p).unwrap();
            let back: Platform = serde_json::from_str(&json).unwrap();
            assert_eq!(&back, p);
        }
    }

    // ── WasmConstraints ──

    #[test]
    fn test_constraints_default() {
        let c = WasmConstraints::default();
        assert_eq!(c.max_total_size, 5 * 1024 * 1024);
        assert_eq!(c.max_model_size, 3 * 1024 * 1024);
        assert_eq!(c.required_precision, ModelPrecision::INT8);
        assert!(!c.simd_available);
    }

    #[test]
    fn test_constraints_with_simd() {
        let c = WasmConstraints::with_simd();
        assert_eq!(c.max_total_size, 20 * 1024 * 1024);
        assert!(c.simd_available);
        assert_eq!(c.required_precision, ModelPrecision::FP16);
    }

    #[test]
    fn test_constraints_validate_model_ok() {
        let c = WasmConstraints::default();
        // Small INT8 model
        let result = c.validate_model("test", 1000, ModelPrecision::INT8);
        assert!(result.is_ok());
    }

    #[test]
    fn test_constraints_validate_model_too_large() {
        let c = WasmConstraints::default();
        // 10 MB model exceeds 3 MB limit
        let result = c.validate_model("test", 10 * 1024 * 1024, ModelPrecision::INT8);
        assert!(result.is_err());
    }

    #[test]
    fn test_constraints_validate_model_wrong_precision() {
        let c = WasmConstraints::default(); // requires INT8
        // FP32 model — 4 bytes/element > 1 byte/element
        let result = c.validate_model("test", 1000, ModelPrecision::FP32);
        assert!(result.is_err());
    }

    #[test]
    fn test_constraints_validate_total_ok() {
        let c = WasmConstraints::default();
        let models = vec![("a", 1_000_000u64), ("b", 2_000_000)];
        assert!(c.validate_total(&models).is_ok());
    }

    #[test]
    fn test_constraints_validate_total_exceeded() {
        let c = WasmConstraints::default();
        let models = vec![("a", 3_000_000u64), ("b", 3_000_000)];
        assert!(c.validate_total(&models).is_err());
    }

    #[test]
    fn test_constraints_serialization() {
        let c = WasmConstraints::with_simd();
        let json = serde_json::to_string(&c).unwrap();
        let back: WasmConstraints = serde_json::from_str(&json).unwrap();
        assert_eq!(back.simd_available, true);
        assert_eq!(back.required_precision, ModelPrecision::FP16);
    }

    // ── WASM readiness check ──

    #[test]
    fn test_wasm_readiness_all_ok() {
        let constraints = WasmConstraints::default();
        let models = vec![
            ("layout", 20_000u64, ModelPrecision::INT8),
            ("encoder", 2_000, ModelPrecision::INT8),
            ("decoder", 15_000, ModelPrecision::INT8),
        ];

        let report = check_wasm_readiness(&models, &constraints);
        assert!(report.ready);
        assert_eq!(report.model_status.len(), 3);
        assert!(report.model_status.iter().all(|s| s.compatible));
    }

    #[test]
    fn test_wasm_readiness_model_too_large() {
        let constraints = WasmConstraints::default();
        let models = vec![
            ("layout", 5_000_000u64, ModelPrecision::INT8), // exceeds 3 MB
        ];

        let report = check_wasm_readiness(&models, &constraints);
        assert!(!report.ready);
        assert!(!report.model_status[0].compatible);
        assert!(report.model_status[0].issue.is_some());
    }

    #[test]
    fn test_wasm_readiness_wrong_precision() {
        let constraints = WasmConstraints::default();
        let models = vec![
            ("layout", 1_000u64, ModelPrecision::FP32), // wrong precision
        ];

        let report = check_wasm_readiness(&models, &constraints);
        assert!(!report.ready);
    }

    #[test]
    fn test_wasm_readiness_with_real_models() {
        use std::path::PathBuf;
        let test_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-models");

        let mut models = Vec::new();
        for (name, file) in &[
            ("layout", "layout_gen_int8.onnx"),
            ("encoder", "style_encoder_int8.onnx"),
            ("decoder", "asset_decoder_int8.onnx"),
        ] {
            let path = test_dir.join(file);
            if path.exists() {
                let size = std::fs::metadata(&path).unwrap().len();
                models.push((*name, size, ModelPrecision::INT8));
            }
        }

        if !models.is_empty() {
            let constraints = WasmConstraints::default();
            let report = check_wasm_readiness(&models, &constraints);
            // Our test models are tiny (< 25 KB each), so should pass
            assert!(report.ready);
            assert!(report.total_size < constraints.max_total_size);
        }
    }

    #[test]
    fn test_wasm_readiness_report_serialization() {
        let report = WasmReadinessReport {
            platform: Platform::Wasm,
            constraints: WasmConstraints::default(),
            model_status: vec![WasmModelStatus {
                name: "test".into(),
                size_bytes: 1000,
                precision: ModelPrecision::INT8,
                compatible: true,
                issue: None,
            }],
            ready: true,
            total_size: 1000,
        };
        let json = serde_json::to_string(&report).unwrap();
        let back: WasmReadinessReport = serde_json::from_str(&json).unwrap();
        assert!(back.ready);
        assert_eq!(back.model_status.len(), 1);
    }
}
