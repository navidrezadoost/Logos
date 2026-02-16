//! Error types for the AI engine.

use thiserror::Error;

/// Result alias for AI operations.
pub type AiResult<T> = Result<T, AiError>;

/// Errors produced by the AI engine.
#[derive(Error, Debug)]
pub enum AiError {
    #[error("model not found: {0}")]
    ModelNotFound(String),

    #[error("model load failed: {0}")]
    ModelLoadFailed(String),

    #[error("inference failed: {0}")]
    InferenceFailed(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("preprocessing failed: {0}")]
    PreprocessingFailed(String),

    #[error("tokenization failed: {0}")]
    TokenizationFailed(String),

    #[error("model format unsupported: {0}")]
    UnsupportedFormat(String),

    #[error("timeout after {0}ms")]
    Timeout(u64),

    #[error("resource limit exceeded: {0}")]
    ResourceLimit(String),

    #[error("backend unavailable: {0}")]
    BackendUnavailable(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_model_not_found() {
        let e = AiError::ModelNotFound("layout-v1.onnx".into());
        assert!(e.to_string().contains("layout-v1.onnx"));
    }

    #[test]
    fn test_error_display_inference_failed() {
        let e = AiError::InferenceFailed("dimension mismatch".into());
        assert!(e.to_string().contains("dimension mismatch"));
    }

    #[test]
    fn test_error_display_timeout() {
        let e = AiError::Timeout(5000);
        assert!(e.to_string().contains("5000"));
    }

    #[test]
    fn test_error_display_invalid_input() {
        let e = AiError::InvalidInput("empty prompt".into());
        assert!(e.to_string().contains("empty prompt"));
    }

    #[test]
    fn test_error_display_resource_limit() {
        let e = AiError::ResourceLimit("memory exceeded 256MB".into());
        assert!(e.to_string().contains("256MB"));
    }

    #[test]
    fn test_error_display_backend_unavailable() {
        let e = AiError::BackendUnavailable("CUDA".into());
        assert!(e.to_string().contains("CUDA"));
    }

    #[test]
    fn test_error_display_unsupported_format() {
        let e = AiError::UnsupportedFormat("tflite".into());
        assert!(e.to_string().contains("tflite"));
    }

    #[test]
    fn test_error_display_preprocessing() {
        let e = AiError::PreprocessingFailed("invalid image dimensions".into());
        assert!(e.to_string().contains("invalid image dimensions"));
    }

    #[test]
    fn test_error_display_tokenization() {
        let e = AiError::TokenizationFailed("unknown token".into());
        assert!(e.to_string().contains("unknown token"));
    }

    #[test]
    fn test_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let ai_err: AiError = io_err.into();
        assert!(ai_err.to_string().contains("file missing"));
    }
}
