//! Model registry — loading, caching, and managing ONNX models.

mod loader;
pub mod quantization;
pub mod embedding;
pub mod wasm;

pub use loader::{ModelLoader, ModelSource};
pub use quantization::{ModelPrecision, QuantizationManager, QuantizedModelInfo, SizeReport};
pub use embedding::{EmbeddedModel, EmbeddedModelRegistry, EmbeddedModelMeta};
pub use wasm::{Platform, WasmConstraints, WasmReadinessReport, check_wasm_readiness};

use crate::error::{AiError, AiResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use uuid::Uuid;

/// Supported model formats.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelFormat {
    /// ONNX Runtime format.
    Onnx,
    /// TensorFlow Lite (future).
    TfLite,
    /// Custom Logos model format (future).
    LogosModel,
}

impl ModelFormat {
    /// File extension for this format.
    pub fn extension(&self) -> &str {
        match self {
            ModelFormat::Onnx => "onnx",
            ModelFormat::TfLite => "tflite",
            ModelFormat::LogosModel => "lgm",
        }
    }

    /// Parse format from file extension.
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "onnx" => Some(ModelFormat::Onnx),
            "tflite" => Some(ModelFormat::TfLite),
            "lgm" => Some(ModelFormat::LogosModel),
            _ => None,
        }
    }
}

/// Current status of a model in the registry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelStatus {
    /// Model metadata registered but not loaded.
    Registered,
    /// Model is being downloaded or loaded.
    Loading,
    /// Model is loaded and ready for inference.
    Ready,
    /// Model failed to load.
    Failed(String),
    /// Model has been unloaded from memory.
    Unloaded,
}

/// Metadata about a registered model.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Unique identifier.
    pub id: Uuid,
    /// Human-readable name.
    pub name: String,
    /// Semantic version.
    pub version: String,
    /// Model format.
    pub format: ModelFormat,
    /// File size in bytes (0 if unknown).
    pub size_bytes: u64,
    /// Input tensor shapes (symbolic).
    pub input_shapes: Vec<Vec<i64>>,
    /// Output tensor shapes (symbolic).
    pub output_shapes: Vec<Vec<i64>>,
    /// Current status.
    pub status: ModelStatus,
    /// Path on disk (if local).
    pub path: Option<PathBuf>,
    /// Description of what this model does.
    pub description: String,
    /// Tags for classification.
    pub tags: Vec<String>,
}

impl ModelInfo {
    /// Create new model info with the given name and format.
    pub fn new(name: impl Into<String>, format: ModelFormat) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            version: "0.1.0".into(),
            format,
            size_bytes: 0,
            input_shapes: Vec::new(),
            output_shapes: Vec::new(),
            status: ModelStatus::Registered,
            path: None,
            description: String::new(),
            tags: Vec::new(),
        }
    }

    /// Builder: set version.
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    /// Builder: set description.
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Builder: set path.
    pub fn with_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.path = Some(path.into());
        self
    }

    /// Builder: set input shapes.
    pub fn with_input_shapes(mut self, shapes: Vec<Vec<i64>>) -> Self {
        self.input_shapes = shapes;
        self
    }

    /// Builder: set output shapes.
    pub fn with_output_shapes(mut self, shapes: Vec<Vec<i64>>) -> Self {
        self.output_shapes = shapes;
        self
    }

    /// Builder: add tags.
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Builder: set size.
    pub fn with_size(mut self, size: u64) -> Self {
        self.size_bytes = size;
        self
    }

    /// Check if model is ready for inference.
    pub fn is_ready(&self) -> bool {
        self.status == ModelStatus::Ready
    }
}

/// Central registry for AI models.
///
/// Manages model metadata, handles loading/unloading, and provides lookup.
pub struct ModelRegistry {
    /// Registered models keyed by ID.
    models: HashMap<Uuid, ModelInfo>,
    /// Name-to-ID index for fast lookup.
    name_index: HashMap<String, Uuid>,
    /// Base directory for local model storage.
    cache_dir: PathBuf,
    /// Maximum total memory budget (bytes).
    memory_budget: u64,
    /// Current estimated memory usage (bytes).
    memory_used: u64,
}

impl ModelRegistry {
    /// Create a new registry with the given cache directory.
    pub fn new(cache_dir: impl Into<PathBuf>) -> Self {
        Self {
            models: HashMap::new(),
            name_index: HashMap::new(),
            cache_dir: cache_dir.into(),
            memory_budget: 512 * 1024 * 1024, // 512 MB default
            memory_used: 0,
        }
    }

    /// Set the memory budget in bytes.
    pub fn with_memory_budget(mut self, budget: u64) -> Self {
        self.memory_budget = budget;
        self
    }

    /// Register a model in the registry (does not load it).
    pub fn register(&mut self, info: ModelInfo) -> AiResult<Uuid> {
        if self.name_index.contains_key(&info.name) {
            return Err(AiError::InvalidInput(format!(
                "model '{}' already registered",
                info.name
            )));
        }
        let id = info.id;
        self.name_index.insert(info.name.clone(), id);
        self.models.insert(id, info);
        Ok(id)
    }

    /// Unregister a model, removing it from the registry.
    pub fn unregister(&mut self, id: Uuid) -> AiResult<ModelInfo> {
        let info = self
            .models
            .remove(&id)
            .ok_or_else(|| AiError::ModelNotFound(id.to_string()))?;
        self.name_index.remove(&info.name);
        if info.is_ready() {
            self.memory_used = self.memory_used.saturating_sub(info.size_bytes);
        }
        Ok(info)
    }

    /// Look up a model by name.
    pub fn get_by_name(&self, name: &str) -> Option<&ModelInfo> {
        let id = self.name_index.get(name)?;
        self.models.get(id)
    }

    /// Look up a model by ID.
    pub fn get(&self, id: Uuid) -> Option<&ModelInfo> {
        self.models.get(&id)
    }

    /// Get mutable reference to model by ID.
    pub fn get_mut(&mut self, id: Uuid) -> Option<&mut ModelInfo> {
        self.models.get_mut(&id)
    }

    /// List all registered models.
    pub fn list(&self) -> Vec<&ModelInfo> {
        self.models.values().collect()
    }

    /// List models matching a tag.
    pub fn list_by_tag(&self, tag: &str) -> Vec<&ModelInfo> {
        self.models
            .values()
            .filter(|m| m.tags.iter().any(|t| t == tag))
            .collect()
    }

    /// List models that are ready for inference.
    pub fn list_ready(&self) -> Vec<&ModelInfo> {
        self.models.values().filter(|m| m.is_ready()).collect()
    }

    /// Mark a model as ready (simulates successful load).
    pub fn mark_ready(&mut self, id: Uuid) -> AiResult<()> {
        let info = self
            .models
            .get_mut(&id)
            .ok_or_else(|| AiError::ModelNotFound(id.to_string()))?;

        let needed = info.size_bytes;
        if self.memory_used + needed > self.memory_budget {
            return Err(AiError::ResourceLimit(format!(
                "loading {} ({} bytes) would exceed budget ({}/{} bytes)",
                info.name, needed, self.memory_used, self.memory_budget
            )));
        }

        info.status = ModelStatus::Ready;
        self.memory_used += needed;
        Ok(())
    }

    /// Mark a model as unloaded.
    pub fn mark_unloaded(&mut self, id: Uuid) -> AiResult<()> {
        let info = self
            .models
            .get_mut(&id)
            .ok_or_else(|| AiError::ModelNotFound(id.to_string()))?;

        if info.is_ready() {
            self.memory_used = self.memory_used.saturating_sub(info.size_bytes);
        }
        info.status = ModelStatus::Unloaded;
        Ok(())
    }

    /// Mark a model as failed.
    pub fn mark_failed(&mut self, id: Uuid, reason: impl Into<String>) -> AiResult<()> {
        let info = self
            .models
            .get_mut(&id)
            .ok_or_else(|| AiError::ModelNotFound(id.to_string()))?;

        if info.is_ready() {
            self.memory_used = self.memory_used.saturating_sub(info.size_bytes);
        }
        info.status = ModelStatus::Failed(reason.into());
        Ok(())
    }

    /// Number of registered models.
    pub fn count(&self) -> usize {
        self.models.len()
    }

    /// Current memory usage.
    pub fn memory_used(&self) -> u64 {
        self.memory_used
    }

    /// Memory budget.
    pub fn memory_budget(&self) -> u64 {
        self.memory_budget
    }

    /// Cache directory path.
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_registry() -> ModelRegistry {
        ModelRegistry::new("/tmp/logos-ai-test")
            .with_memory_budget(100 * 1024 * 1024) // 100MB
    }

    fn test_model(name: &str) -> ModelInfo {
        ModelInfo::new(name, ModelFormat::Onnx)
            .with_version("1.0.0")
            .with_description("Test model")
            .with_size(10 * 1024 * 1024) // 10MB
            .with_tags(vec!["test".into()])
    }

    #[test]
    fn test_model_info_new() {
        let info = ModelInfo::new("layout-v1", ModelFormat::Onnx);
        assert_eq!(info.name, "layout-v1");
        assert_eq!(info.format, ModelFormat::Onnx);
        assert_eq!(info.status, ModelStatus::Registered);
        assert!(!info.is_ready());
    }

    #[test]
    fn test_model_info_builder() {
        let info = ModelInfo::new("style-v2", ModelFormat::Onnx)
            .with_version("2.0.0")
            .with_description("Style transfer")
            .with_path("/models/style.onnx")
            .with_size(50_000_000)
            .with_input_shapes(vec![vec![1, 3, 224, 224]])
            .with_output_shapes(vec![vec![1, 3, 224, 224]])
            .with_tags(vec!["style".into(), "cnn".into()]);

        assert_eq!(info.version, "2.0.0");
        assert_eq!(info.description, "Style transfer");
        assert_eq!(info.size_bytes, 50_000_000);
        assert_eq!(info.input_shapes.len(), 1);
        assert_eq!(info.output_shapes.len(), 1);
        assert_eq!(info.tags.len(), 2);
        assert_eq!(info.path, Some(PathBuf::from("/models/style.onnx")));
    }

    #[test]
    fn test_model_format_extension() {
        assert_eq!(ModelFormat::Onnx.extension(), "onnx");
        assert_eq!(ModelFormat::TfLite.extension(), "tflite");
        assert_eq!(ModelFormat::LogosModel.extension(), "lgm");
    }

    #[test]
    fn test_model_format_from_extension() {
        assert_eq!(ModelFormat::from_extension("onnx"), Some(ModelFormat::Onnx));
        assert_eq!(ModelFormat::from_extension("ONNX"), Some(ModelFormat::Onnx));
        assert_eq!(ModelFormat::from_extension("tflite"), Some(ModelFormat::TfLite));
        assert_eq!(ModelFormat::from_extension("lgm"), Some(ModelFormat::LogosModel));
        assert_eq!(ModelFormat::from_extension("xyz"), None);
    }

    #[test]
    fn test_registry_new() {
        let reg = test_registry();
        assert_eq!(reg.count(), 0);
        assert_eq!(reg.memory_used(), 0);
        assert_eq!(reg.memory_budget(), 100 * 1024 * 1024);
    }

    #[test]
    fn test_registry_register() {
        let mut reg = test_registry();
        let model = test_model("layout-v1");
        let id = reg.register(model).unwrap();
        assert_eq!(reg.count(), 1);
        assert!(reg.get(id).is_some());
    }

    #[test]
    fn test_registry_register_duplicate_name() {
        let mut reg = test_registry();
        reg.register(test_model("layout-v1")).unwrap();
        let result = reg.register(test_model("layout-v1"));
        assert!(result.is_err());
    }

    #[test]
    fn test_registry_get_by_name() {
        let mut reg = test_registry();
        reg.register(test_model("layout-v1")).unwrap();
        let info = reg.get_by_name("layout-v1");
        assert!(info.is_some());
        assert_eq!(info.unwrap().name, "layout-v1");
    }

    #[test]
    fn test_registry_get_by_name_missing() {
        let reg = test_registry();
        assert!(reg.get_by_name("nonexistent").is_none());
    }

    #[test]
    fn test_registry_unregister() {
        let mut reg = test_registry();
        let id = reg.register(test_model("layout-v1")).unwrap();
        let removed = reg.unregister(id).unwrap();
        assert_eq!(removed.name, "layout-v1");
        assert_eq!(reg.count(), 0);
        assert!(reg.get_by_name("layout-v1").is_none());
    }

    #[test]
    fn test_registry_unregister_not_found() {
        let mut reg = test_registry();
        let result = reg.unregister(Uuid::new_v4());
        assert!(result.is_err());
    }

    #[test]
    fn test_registry_mark_ready() {
        let mut reg = test_registry();
        let id = reg.register(test_model("layout-v1")).unwrap();
        reg.mark_ready(id).unwrap();
        assert!(reg.get(id).unwrap().is_ready());
        assert_eq!(reg.memory_used(), 10 * 1024 * 1024);
    }

    #[test]
    fn test_registry_mark_unloaded() {
        let mut reg = test_registry();
        let id = reg.register(test_model("layout-v1")).unwrap();
        reg.mark_ready(id).unwrap();
        reg.mark_unloaded(id).unwrap();
        assert_eq!(reg.get(id).unwrap().status, ModelStatus::Unloaded);
        assert_eq!(reg.memory_used(), 0);
    }

    #[test]
    fn test_registry_mark_failed() {
        let mut reg = test_registry();
        let id = reg.register(test_model("layout-v1")).unwrap();
        reg.mark_failed(id, "corrupt file").unwrap();
        match &reg.get(id).unwrap().status {
            ModelStatus::Failed(reason) => assert_eq!(reason, "corrupt file"),
            _ => panic!("expected Failed status"),
        }
    }

    #[test]
    fn test_registry_memory_budget_enforcement() {
        let mut reg = ModelRegistry::new("/tmp/test")
            .with_memory_budget(15 * 1024 * 1024); // 15MB budget

        let id1 = reg.register(test_model("model-a")).unwrap(); // 10MB
        reg.mark_ready(id1).unwrap();

        let id2 = reg.register(test_model("model-b")).unwrap(); // 10MB — would exceed
        let result = reg.mark_ready(id2);
        assert!(result.is_err());
    }

    #[test]
    fn test_registry_list() {
        let mut reg = test_registry();
        reg.register(test_model("a")).unwrap();
        reg.register(test_model("b")).unwrap();
        reg.register(test_model("c")).unwrap();
        assert_eq!(reg.list().len(), 3);
    }

    #[test]
    fn test_registry_list_by_tag() {
        let mut reg = test_registry();
        reg.register(
            ModelInfo::new("style", ModelFormat::Onnx)
                .with_tags(vec!["style".into()])
        ).unwrap();
        reg.register(
            ModelInfo::new("layout", ModelFormat::Onnx)
                .with_tags(vec!["layout".into()])
        ).unwrap();
        let styles = reg.list_by_tag("style");
        assert_eq!(styles.len(), 1);
        assert_eq!(styles[0].name, "style");
    }

    #[test]
    fn test_registry_list_ready() {
        let mut reg = test_registry();
        let id1 = reg.register(test_model("a")).unwrap();
        let _id2 = reg.register(test_model("b")).unwrap();
        reg.mark_ready(id1).unwrap();
        let ready = reg.list_ready();
        assert_eq!(ready.len(), 1);
    }

    #[test]
    fn test_registry_unregister_ready_model_frees_memory() {
        let mut reg = test_registry();
        let id = reg.register(test_model("a")).unwrap();
        reg.mark_ready(id).unwrap();
        assert_eq!(reg.memory_used(), 10 * 1024 * 1024);
        reg.unregister(id).unwrap();
        assert_eq!(reg.memory_used(), 0);
    }

    #[test]
    fn test_model_status_serialization() {
        let status = ModelStatus::Ready;
        let json = serde_json::to_string(&status).unwrap();
        let back: ModelStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ModelStatus::Ready);
    }

    #[test]
    fn test_model_info_serialization() {
        let info = test_model("test-ser");
        let json = serde_json::to_string(&info).unwrap();
        let back: ModelInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "test-ser");
        assert_eq!(back.format, ModelFormat::Onnx);
    }
}
