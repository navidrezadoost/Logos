//! # logos-ai — AI-Native Design Engine
//!
//! On-device machine learning for intelligent design assistance.
//!
//! ## Modules
//!
//! - [`models`] — Model registry, loading, caching, and versioning
//! - [`inference`] — Inference engine with layout generation, style transfer, and asset generation
//! - [`preprocess`] — Input preprocessing (image normalization, tokenization)
//! - [`error`] — Error types
//!
//! ## Capabilities
//!
//! - **Layout generation** — Transformer-based layout proposals from constraints
//! - **Style transfer** — Real-time perceptual style application
//! - **Asset generation** — Text-to-image via optimized diffusion pipeline
//! - **Model registry** — Load, cache, and version-manage ONNX models
//!
//! All inference runs locally via ONNX Runtime with WASM SIMD and GPU backends.
//!
//! ## Performance Targets
//!
//! | Capability | Target |
//! |---|---|
//! | Model load | <100ms |
//! | Layout generation (10 options) | <50ms |
//! | Style transfer (1024×1024) | <16ms |
//! | Asset generation (512×512) | <2s |

pub mod error;
pub mod models;
pub mod inference;
pub mod preprocess;

// Error types
pub use error::{AiError, AiResult};

// Model management
pub use models::{ModelRegistry, ModelInfo, ModelFormat, ModelStatus};

// Layout generation
pub use inference::layout_gen::{LayoutGenerator, LayoutProposal, LayoutConstraints, ElementHint};

// Style transfer
pub use inference::style_transfer::{StyleTransfer, StyleOptions, StyleResult};

// Asset generation
pub use inference::asset_gen::{AssetGenerator, GenerationParams, GeneratedImage, ImageSize};

// Inference engine
pub use inference::engine::{InferenceEngine, InferenceBackend, InferenceSession};

// ONNX Runtime integration
pub use inference::onnx_session::{
    OnnxSessionConfig, TensorSpec, SimulatedOnnxSession, SimulationMode,
    InferenceBackendSession, OnnxInferenceProfile,
};
#[cfg(feature = "onnx")]
pub use inference::onnx_session::OnnxSession;

// Preprocessing
pub use preprocess::{ImageTensor, TextTokenizer, TokenizerConfig};

#[cfg(test)]
mod integration_tests;
