//! Model embedding — bundle ONNX models directly into the binary.
//!
//! Eliminates network dependencies at runtime by embedding model bytes
//! into the compiled binary using `include_bytes!`. Supports versioned
//! model registry with automatic fallback.
//!
//! # Usage
//!
//! ```rust,no_run
//! use logos_ai::models::embedding::{EmbeddedModel, EmbeddedModelRegistry};
//!
//! let mut registry = EmbeddedModelRegistry::new();
//! registry.register(EmbeddedModel::new(
//!     "layout_gen",
//!     "1.0.0",
//!     include_bytes!("../../test-models/layout_gen.onnx"),
//! ));
//!
//! let model = registry.get("layout_gen").unwrap();
//! assert!(model.size_bytes() > 0);
//! ```

use crate::error::{AiError, AiResult};
use crate::models::quantization::ModelPrecision;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// An ONNX model embedded as static bytes in the binary.
#[derive(Clone, Debug)]
pub struct EmbeddedModel {
    /// Model name.
    name: String,
    /// Semantic version.
    version: String,
    /// Model bytes (static or owned).
    bytes: ModelBytes,
    /// Weight precision.
    precision: ModelPrecision,
    /// Description.
    description: String,
}

/// How model bytes are stored.
#[derive(Clone, Debug)]
enum ModelBytes {
    /// Static bytes from `include_bytes!`.
    Static(&'static [u8]),
    /// Owned bytes loaded at runtime.
    Owned(Vec<u8>),
}

impl ModelBytes {
    fn as_slice(&self) -> &[u8] {
        match self {
            ModelBytes::Static(b) => b,
            ModelBytes::Owned(b) => b,
        }
    }

    fn len(&self) -> usize {
        match self {
            ModelBytes::Static(b) => b.len(),
            ModelBytes::Owned(b) => b.len(),
        }
    }
}

impl EmbeddedModel {
    /// Create from static bytes (e.g. `include_bytes!`).
    pub fn new(name: impl Into<String>, version: impl Into<String>, bytes: &'static [u8]) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            bytes: ModelBytes::Static(bytes),
            precision: ModelPrecision::FP32,
            description: String::new(),
        }
    }

    /// Create from owned bytes (loaded at runtime).
    pub fn from_owned(
        name: impl Into<String>,
        version: impl Into<String>,
        bytes: Vec<u8>,
    ) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            bytes: ModelBytes::Owned(bytes),
            precision: ModelPrecision::FP32,
            description: String::new(),
        }
    }

    /// Set precision level.
    pub fn with_precision(mut self, precision: ModelPrecision) -> Self {
        self.precision = precision;
        self
    }

    /// Set description.
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Model name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Model version.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Model bytes.
    pub fn bytes(&self) -> &[u8] {
        self.bytes.as_slice()
    }

    /// Size in bytes.
    pub fn size_bytes(&self) -> usize {
        self.bytes.len()
    }

    /// Weight precision.
    pub fn precision(&self) -> ModelPrecision {
        self.precision
    }

    /// Description.
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Registry key: "name@version".
    pub fn key(&self) -> String {
        format!("{}@{}", self.name, self.version)
    }
}

/// Metadata about an embedded model (serializable, without the bytes).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EmbeddedModelMeta {
    /// Model name.
    pub name: String,
    /// Semantic version.
    pub version: String,
    /// Size in bytes.
    pub size_bytes: usize,
    /// Weight precision.
    pub precision: ModelPrecision,
    /// Description.
    pub description: String,
}

impl From<&EmbeddedModel> for EmbeddedModelMeta {
    fn from(m: &EmbeddedModel) -> Self {
        Self {
            name: m.name.clone(),
            version: m.version.clone(),
            size_bytes: m.size_bytes(),
            precision: m.precision,
            description: m.description.clone(),
        }
    }
}

/// Registry of embedded models with versioning and fallback.
///
/// Supports multiple versions of the same model, with automatic
/// fallback to the latest available version.
pub struct EmbeddedModelRegistry {
    /// Models keyed by "name@version".
    models: HashMap<String, EmbeddedModel>,
    /// Latest version index: name → version.
    latest: HashMap<String, String>,
}

impl EmbeddedModelRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            models: HashMap::new(),
            latest: HashMap::new(),
        }
    }

    /// Register an embedded model.
    ///
    /// If a model with the same name exists, keeps the one with the higher version
    /// as the "latest."
    pub fn register(&mut self, model: EmbeddedModel) {
        let name = model.name.clone();
        let version = model.version.clone();
        let key = model.key();

        // Update latest version tracking
        let is_newer = match self.latest.get(&name) {
            None => true,
            Some(existing) => version_cmp(&version, existing) == std::cmp::Ordering::Greater,
        };
        if is_newer {
            self.latest.insert(name, version);
        }

        self.models.insert(key, model);
    }

    /// Get a model by name (returns latest version).
    pub fn get(&self, name: &str) -> Option<&EmbeddedModel> {
        let version = self.latest.get(name)?;
        let key = format!("{name}@{version}");
        self.models.get(&key)
    }

    /// Get a specific version of a model.
    pub fn get_version(&self, name: &str, version: &str) -> Option<&EmbeddedModel> {
        let key = format!("{name}@{version}");
        self.models.get(&key)
    }

    /// Get the bytes of the latest version of a model.
    pub fn get_bytes(&self, name: &str) -> AiResult<&[u8]> {
        self.get(name)
            .map(|m| m.bytes())
            .ok_or_else(|| AiError::ModelNotFound(format!("embedded model '{name}' not found")))
    }

    /// List all registered model metadata (without bytes).
    pub fn list(&self) -> Vec<EmbeddedModelMeta> {
        self.models.values().map(EmbeddedModelMeta::from).collect()
    }

    /// List all versions of a specific model.
    pub fn versions(&self, name: &str) -> Vec<&str> {
        self.models
            .values()
            .filter(|m| m.name == name)
            .map(|m| m.version.as_str())
            .collect()
    }

    /// Total number of registered models (including all versions).
    pub fn count(&self) -> usize {
        self.models.len()
    }

    /// Total size of all embedded models in bytes.
    pub fn total_size_bytes(&self) -> usize {
        self.models.values().map(|m| m.size_bytes()).sum()
    }

    /// Check if a model is registered.
    pub fn contains(&self, name: &str) -> bool {
        self.latest.contains_key(name)
    }

    /// Get the latest version string for a model.
    pub fn latest_version(&self, name: &str) -> Option<&str> {
        self.latest.get(name).map(|s| s.as_str())
    }
}

impl Default for EmbeddedModelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple semver comparison (major.minor.patch).
fn version_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |v: &str| -> Vec<u64> {
        v.split('.')
            .filter_map(|s| s.parse::<u64>().ok())
            .collect()
    };
    let va = parse(a);
    let vb = parse(b);
    va.cmp(&vb)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_model(name: &str, version: &str) -> EmbeddedModel {
        // Create a small owned model for testing
        EmbeddedModel::from_owned(name, version, vec![0x08, 0x07, 0x12, 0x03])
    }

    // ── EmbeddedModel ──

    #[test]
    fn test_embedded_model_new() {
        let bytes: &'static [u8] = b"test model bytes";
        let model = EmbeddedModel::new("layout_gen", "1.0.0", bytes);
        assert_eq!(model.name(), "layout_gen");
        assert_eq!(model.version(), "1.0.0");
        assert_eq!(model.size_bytes(), 16);
        assert_eq!(model.precision(), ModelPrecision::FP32);
        assert_eq!(model.key(), "layout_gen@1.0.0");
    }

    #[test]
    fn test_embedded_model_from_owned() {
        let model = EmbeddedModel::from_owned("test", "0.1.0", vec![1, 2, 3, 4]);
        assert_eq!(model.name(), "test");
        assert_eq!(model.size_bytes(), 4);
        assert_eq!(model.bytes(), &[1, 2, 3, 4]);
    }

    #[test]
    fn test_embedded_model_with_precision() {
        let model = EmbeddedModel::from_owned("test", "1.0.0", vec![1, 2])
            .with_precision(ModelPrecision::FP16);
        assert_eq!(model.precision(), ModelPrecision::FP16);
    }

    #[test]
    fn test_embedded_model_with_description() {
        let model = EmbeddedModel::from_owned("test", "1.0.0", vec![1])
            .with_description("A test model");
        assert_eq!(model.description(), "A test model");
    }

    #[test]
    fn test_embedded_model_meta() {
        let model = EmbeddedModel::from_owned("layout", "2.0.0", vec![0; 100])
            .with_precision(ModelPrecision::INT8)
            .with_description("Layout model");
        let meta = EmbeddedModelMeta::from(&model);
        assert_eq!(meta.name, "layout");
        assert_eq!(meta.version, "2.0.0");
        assert_eq!(meta.size_bytes, 100);
        assert_eq!(meta.precision, ModelPrecision::INT8);
    }

    #[test]
    fn test_embedded_model_meta_serialization() {
        let meta = EmbeddedModelMeta {
            name: "test".into(),
            version: "1.0.0".into(),
            size_bytes: 500,
            precision: ModelPrecision::FP16,
            description: "desc".into(),
        };
        let json = serde_json::to_string(&meta).unwrap();
        let back: EmbeddedModelMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "test");
        assert_eq!(back.precision, ModelPrecision::FP16);
    }

    // ── EmbeddedModelRegistry ──

    #[test]
    fn test_registry_new() {
        let reg = EmbeddedModelRegistry::new();
        assert_eq!(reg.count(), 0);
        assert_eq!(reg.total_size_bytes(), 0);
    }

    #[test]
    fn test_registry_register_and_get() {
        let mut reg = EmbeddedModelRegistry::new();
        reg.register(sample_model("layout", "1.0.0"));

        assert_eq!(reg.count(), 1);
        assert!(reg.contains("layout"));

        let model = reg.get("layout").unwrap();
        assert_eq!(model.name(), "layout");
        assert_eq!(model.version(), "1.0.0");
    }

    #[test]
    fn test_registry_get_missing() {
        let reg = EmbeddedModelRegistry::new();
        assert!(reg.get("nonexistent").is_none());
    }

    #[test]
    fn test_registry_get_bytes() {
        let mut reg = EmbeddedModelRegistry::new();
        reg.register(sample_model("layout", "1.0.0"));

        let bytes = reg.get_bytes("layout").unwrap();
        assert!(!bytes.is_empty());

        let err = reg.get_bytes("nonexistent");
        assert!(err.is_err());
    }

    #[test]
    fn test_registry_multiple_versions() {
        let mut reg = EmbeddedModelRegistry::new();
        reg.register(sample_model("layout", "1.0.0"));
        reg.register(sample_model("layout", "2.0.0"));
        reg.register(sample_model("layout", "1.5.0"));

        assert_eq!(reg.count(), 3);

        // Latest should be 2.0.0
        let latest = reg.get("layout").unwrap();
        assert_eq!(latest.version(), "2.0.0");

        // Can still get specific versions
        let v1 = reg.get_version("layout", "1.0.0").unwrap();
        assert_eq!(v1.version(), "1.0.0");

        let v15 = reg.get_version("layout", "1.5.0").unwrap();
        assert_eq!(v15.version(), "1.5.0");
    }

    #[test]
    fn test_registry_latest_version() {
        let mut reg = EmbeddedModelRegistry::new();
        reg.register(sample_model("layout", "1.0.0"));
        reg.register(sample_model("layout", "3.0.0"));
        reg.register(sample_model("layout", "2.0.0")); // Registered after but lower version

        assert_eq!(reg.latest_version("layout"), Some("3.0.0"));
    }

    #[test]
    fn test_registry_versions() {
        let mut reg = EmbeddedModelRegistry::new();
        reg.register(sample_model("layout", "1.0.0"));
        reg.register(sample_model("layout", "2.0.0"));

        let versions = reg.versions("layout");
        assert_eq!(versions.len(), 2);
        assert!(versions.contains(&"1.0.0"));
        assert!(versions.contains(&"2.0.0"));
    }

    #[test]
    fn test_registry_multiple_models() {
        let mut reg = EmbeddedModelRegistry::new();
        reg.register(sample_model("layout", "1.0.0"));
        reg.register(sample_model("encoder", "1.0.0"));
        reg.register(sample_model("decoder", "1.0.0"));

        assert_eq!(reg.count(), 3);
        assert!(reg.contains("layout"));
        assert!(reg.contains("encoder"));
        assert!(reg.contains("decoder"));
    }

    #[test]
    fn test_registry_total_size() {
        let mut reg = EmbeddedModelRegistry::new();
        reg.register(EmbeddedModel::from_owned("a", "1.0.0", vec![0; 100]));
        reg.register(EmbeddedModel::from_owned("b", "1.0.0", vec![0; 200]));

        assert_eq!(reg.total_size_bytes(), 300);
    }

    #[test]
    fn test_registry_list() {
        let mut reg = EmbeddedModelRegistry::new();
        reg.register(
            sample_model("layout", "1.0.0").with_precision(ModelPrecision::FP16),
        );
        reg.register(sample_model("encoder", "2.0.0"));

        let list = reg.list();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_registry_default() {
        let reg = EmbeddedModelRegistry::default();
        assert_eq!(reg.count(), 0);
    }

    // ── Version comparison ──

    #[test]
    fn test_version_cmp() {
        use std::cmp::Ordering;
        assert_eq!(version_cmp("1.0.0", "2.0.0"), Ordering::Less);
        assert_eq!(version_cmp("2.0.0", "1.0.0"), Ordering::Greater);
        assert_eq!(version_cmp("1.0.0", "1.0.0"), Ordering::Equal);
        assert_eq!(version_cmp("1.2.0", "1.1.0"), Ordering::Greater);
        assert_eq!(version_cmp("1.0.1", "1.0.0"), Ordering::Greater);
        assert_eq!(version_cmp("1.10.0", "1.9.0"), Ordering::Greater);
    }

    // ── Real model embedding test ──

    #[test]
    fn test_embed_real_test_model() {
        let test_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("test-models");

        let layout_path = test_dir.join("layout_gen.onnx");
        if layout_path.exists() {
            let bytes = std::fs::read(&layout_path).unwrap();
            let model = EmbeddedModel::from_owned("layout_gen", "1.0.0", bytes)
                .with_description("Layout generator");
            assert!(model.size_bytes() > 1000);

            let mut reg = EmbeddedModelRegistry::new();
            reg.register(model);
            assert!(reg.contains("layout_gen"));
            assert!(reg.get_bytes("layout_gen").unwrap().len() > 1000);
        }
    }

    #[test]
    fn test_embed_quantized_models() {
        let test_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("test-models");

        let mut reg = EmbeddedModelRegistry::new();

        // Load all precision variants
        for (suffix, precision) in &[
            ("", ModelPrecision::FP32),
            ("_fp16", ModelPrecision::FP16),
            ("_int8", ModelPrecision::INT8),
        ] {
            let path = test_dir.join(format!("layout_gen{suffix}.onnx"));
            if path.exists() {
                let bytes = std::fs::read(&path).unwrap();
                let version = match precision {
                    ModelPrecision::FP32 => "1.0.0",
                    ModelPrecision::FP16 => "1.0.0-fp16",
                    ModelPrecision::INT8 => "1.0.0-int8",
                    _ => "1.0.0",
                };
                let model = EmbeddedModel::from_owned("layout_gen", version, bytes)
                    .with_precision(*precision);
                reg.register(model);
            }
        }

        assert!(reg.count() >= 1);
        let list = reg.list();
        for meta in &list {
            assert!(meta.size_bytes > 0);
        }
    }
}
