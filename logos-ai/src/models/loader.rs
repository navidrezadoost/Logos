//! Model loading from various sources.

use crate::error::{AiError, AiResult};
use crate::models::ModelFormat;
use std::path::{Path, PathBuf};

/// Where a model can be loaded from.
#[derive(Clone, Debug)]
pub enum ModelSource {
    /// Local filesystem path.
    File(PathBuf),
    /// HTTP/HTTPS URL (for future download support).
    Url(String),
    /// In-memory bytes.
    Memory(Vec<u8>),
}

/// Handles loading model files from various sources.
pub struct ModelLoader {
    cache_dir: PathBuf,
}

impl ModelLoader {
    /// Create a new loader with the given cache directory.
    pub fn new(cache_dir: impl Into<PathBuf>) -> Self {
        Self {
            cache_dir: cache_dir.into(),
        }
    }

    /// Validate that a model path exists and has a supported extension.
    pub fn validate_path(&self, path: &Path) -> AiResult<ModelFormat> {
        if !path.exists() {
            return Err(AiError::ModelNotFound(path.display().to_string()));
        }

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .ok_or_else(|| {
                AiError::UnsupportedFormat("no file extension".into())
            })?;

        ModelFormat::from_extension(ext).ok_or_else(|| {
            AiError::UnsupportedFormat(format!("unsupported extension: .{ext}"))
        })
    }

    /// Read raw bytes from a file path.
    pub fn read_bytes(&self, path: &Path) -> AiResult<Vec<u8>> {
        std::fs::read(path).map_err(AiError::Io)
    }

    /// Compute the cache path for a model.
    pub fn cache_path(&self, name: &str, format: &ModelFormat) -> PathBuf {
        self.cache_dir
            .join(format!("{}.{}", name, format.extension()))
    }

    /// Check if a model is cached locally.
    pub fn is_cached(&self, name: &str, format: &ModelFormat) -> bool {
        self.cache_path(name, format).exists()
    }

    /// Get file size in bytes (returns 0 if file doesn't exist).
    pub fn file_size(path: &Path) -> u64 {
        std::fs::metadata(path)
            .map(|m| m.len())
            .unwrap_or(0)
    }

    /// Cache directory path.
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_model_source_variants() {
        let _file = ModelSource::File(PathBuf::from("/tmp/model.onnx"));
        let _url = ModelSource::Url("https://models.example.com/v1.onnx".into());
        let _mem = ModelSource::Memory(vec![0u8; 100]);
    }

    #[test]
    fn test_loader_new() {
        let loader = ModelLoader::new("/tmp/cache");
        assert_eq!(loader.cache_dir(), Path::new("/tmp/cache"));
    }

    #[test]
    fn test_loader_cache_path() {
        let loader = ModelLoader::new("/tmp/cache");
        let path = loader.cache_path("layout-v1", &ModelFormat::Onnx);
        assert_eq!(path, PathBuf::from("/tmp/cache/layout-v1.onnx"));
    }

    #[test]
    fn test_loader_validate_path_not_found() {
        let loader = ModelLoader::new("/tmp/cache");
        let result = loader.validate_path(Path::new("/nonexistent/model.onnx"));
        assert!(result.is_err());
    }

    #[test]
    fn test_loader_is_cached_false() {
        let loader = ModelLoader::new("/tmp/logos-ai-test-nonexistent");
        assert!(!loader.is_cached("model-x", &ModelFormat::Onnx));
    }

    #[test]
    fn test_file_size_nonexistent() {
        assert_eq!(ModelLoader::file_size(Path::new("/nonexistent")), 0);
    }
}
