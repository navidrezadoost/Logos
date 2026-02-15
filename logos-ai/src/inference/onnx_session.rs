//! ONNX Runtime session wrapper — real inference execution.
//!
//! Provides a clean abstraction over `ort::Session` for loading and running
//! ONNX models. Only compiled when the `onnx` feature is enabled.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────┐     ┌──────────────┐     ┌────────────────┐
//! │ OnnxRuntime  │────▶│ OnnxSession  │────▶│ ort::Session   │
//! │ (singleton)  │     │ (per model)  │     │ (C++ runtime)  │
//! └─────────────┘     └──────────────┘     └────────────────┘
//! ```

#[cfg(feature = "onnx")]
use ort::session::builder::GraphOptimizationLevel;
#[cfg(feature = "onnx")]
use ort::value::Tensor as OrtTensor;

use crate::error::{AiError, AiResult};
use crate::inference::engine::Tensor;
use ndarray::{Array, Dimension, IxDyn};
#[cfg(feature = "onnx")]
use ndarray::ArrayD;
#[cfg(feature = "onnx")]
use std::path::Path;
use std::time::Duration;
#[cfg(feature = "onnx")]
use std::time::Instant;

/// Configuration for an ONNX session.
#[derive(Clone, Debug)]
pub struct OnnxSessionConfig {
    /// Number of intra-op threads (0 = auto).
    pub num_threads: usize,
    /// Graph optimization level (0-3).
    pub optimization_level: u8,
    /// Enable profiling.
    pub enable_profiling: bool,
    /// Model name (for logging).
    pub model_name: String,
}

impl Default for OnnxSessionConfig {
    fn default() -> Self {
        Self {
            num_threads: 4,
            optimization_level: 3,
            enable_profiling: false,
            model_name: String::new(),
        }
    }
}

impl OnnxSessionConfig {
    /// Set thread count.
    pub fn with_threads(mut self, n: usize) -> Self {
        self.num_threads = n;
        self
    }

    /// Set optimization level (clamped 0-3).
    pub fn with_optimization(mut self, level: u8) -> Self {
        self.optimization_level = level.min(3);
        self
    }

    /// Set model name.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.model_name = name.into();
        self
    }

    /// Enable profiling.
    pub fn with_profiling(mut self, enabled: bool) -> Self {
        self.enable_profiling = enabled;
        self
    }
}

/// Metadata about a model's input/output tensors.
#[derive(Clone, Debug)]
pub struct TensorSpec {
    /// Tensor name.
    pub name: String,
    /// Expected shape (dimensions; -1 means dynamic).
    pub shape: Vec<i64>,
    /// Element type (usually f32).
    pub elem_type: String,
}

/// Profiling data from an ONNX inference run.
#[derive(Clone, Debug)]
pub struct OnnxInferenceProfile {
    /// Total time for the run() call.
    pub total_time: Duration,
    /// Time spent converting inputs.
    pub input_conversion_time: Duration,
    /// Time in the ONNX Runtime kernel.
    pub kernel_time: Duration,
    /// Time converting outputs.
    pub output_conversion_time: Duration,
    /// Model name.
    pub model_name: String,
}

/// Real ONNX Runtime session wrapping `ort::Session`.
///
/// This struct is the bridge between the Logos AI engine and the ONNX Runtime C++ library.
/// It handles:
/// - Model loading from file or bytes
/// - Tensor format conversion (ndarray ↔ ONNX)
/// - Inference execution with profiling
/// - Input/output spec introspection
#[cfg(feature = "onnx")]
pub struct OnnxSession {
    session: ort::session::Session,
    config: OnnxSessionConfig,
    input_specs: Vec<TensorSpec>,
    output_specs: Vec<TensorSpec>,
    run_count: u64,
}

#[cfg(feature = "onnx")]
impl OnnxSession {
    /// Load an ONNX model from a file path.
    pub fn from_file(path: impl AsRef<Path>, config: OnnxSessionConfig) -> AiResult<Self> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(AiError::ModelNotFound(path.display().to_string()));
        }

        let opt_level = match config.optimization_level {
            0 => GraphOptimizationLevel::Disable,
            1 => GraphOptimizationLevel::Level1,
            2 => GraphOptimizationLevel::Level2,
            _ => GraphOptimizationLevel::Level3,
        };

        let session = ort::session::Session::builder()
            .map_err(|e| AiError::ModelLoadFailed(format!("session builder: {e}")))?
            .with_optimization_level(opt_level)
            .map_err(|e| AiError::ModelLoadFailed(format!("optimization level: {e}")))?
            .with_intra_threads(config.num_threads)
            .map_err(|e| AiError::ModelLoadFailed(format!("thread config: {e}")))?
            .commit_from_file(path)
            .map_err(|e| AiError::ModelLoadFailed(format!("load model: {e}")))?;

        Self::from_session(session, config)
    }

    /// Load an ONNX model from in-memory bytes.
    pub fn from_bytes(bytes: &[u8], config: OnnxSessionConfig) -> AiResult<Self> {
        let opt_level = match config.optimization_level {
            0 => GraphOptimizationLevel::Disable,
            1 => GraphOptimizationLevel::Level1,
            2 => GraphOptimizationLevel::Level2,
            _ => GraphOptimizationLevel::Level3,
        };

        let session = ort::session::Session::builder()
            .map_err(|e| AiError::ModelLoadFailed(format!("session builder: {e}")))?
            .with_optimization_level(opt_level)
            .map_err(|e| AiError::ModelLoadFailed(format!("optimization level: {e}")))?
            .with_intra_threads(config.num_threads)
            .map_err(|e| AiError::ModelLoadFailed(format!("thread config: {e}")))?
            .commit_from_memory(bytes)
            .map_err(|e| AiError::ModelLoadFailed(format!("load model from bytes: {e}")))?;

        Self::from_session(session, config)
    }

    /// Create from an already-built ort session.
    fn from_session(session: ort::session::Session, config: OnnxSessionConfig) -> AiResult<Self> {
        let input_specs = session
            .inputs()
            .iter()
            .map(|input| {
                let shape: Vec<i64> = input
                    .dtype()
                    .tensor_shape()
                    .map(|s| s.to_vec())
                    .unwrap_or_default();
                TensorSpec {
                    name: input.name().to_string(),
                    shape,
                    elem_type: "f32".to_string(),
                }
            })
            .collect();

        let output_specs = session
            .outputs()
            .iter()
            .map(|output| {
                let shape: Vec<i64> = output
                    .dtype()
                    .tensor_shape()
                    .map(|s| s.to_vec())
                    .unwrap_or_default();
                TensorSpec {
                    name: output.name().to_string(),
                    shape,
                    elem_type: "f32".to_string(),
                }
            })
            .collect();

        Ok(Self {
            session,
            config,
            input_specs,
            output_specs,
            run_count: 0,
        })
    }

    /// Get input tensor specs.
    pub fn input_specs(&self) -> &[TensorSpec] {
        &self.input_specs
    }

    /// Get output tensor specs.
    pub fn output_specs(&self) -> &[TensorSpec] {
        &self.output_specs
    }

    /// Number of successful inferences.
    pub fn run_count(&self) -> u64 {
        self.run_count
    }

    /// Model name.
    pub fn model_name(&self) -> &str {
        &self.config.model_name
    }

    /// Run inference with the given input tensors.
    ///
    /// Input tensors must match the model's expected input names and shapes.
    /// Returns output tensors as `Vec<Tensor>`.
    pub fn run(&mut self, inputs: &[Tensor]) -> AiResult<Vec<Tensor>> {
        let total_start = Instant::now();

        // Convert ndarray inputs to ort values
        let input_start = Instant::now();
        let mut ort_inputs: Vec<(String, ort::session::SessionInputValue<'_>)> =
            Vec::with_capacity(inputs.len());

        for tensor in inputs {
            let value = OrtTensor::from_array(tensor.data.clone())
                .map_err(|e| AiError::InferenceFailed(format!("input conversion '{}': {e}", tensor.name)))?;
            ort_inputs.push((tensor.name.clone(), value.into()));
        }
        let input_time = input_start.elapsed();

        // Run inference
        let kernel_start = Instant::now();
        let outputs = self
            .session
            .run(ort_inputs)
            .map_err(|e| AiError::InferenceFailed(format!("runtime error: {e}")))?;
        let kernel_time = kernel_start.elapsed();

        // Convert outputs back to Tensor
        let output_start = Instant::now();
        let mut result_tensors = Vec::with_capacity(self.output_specs.len());

        for (i, spec) in self.output_specs.iter().enumerate() {
            let extracted: ArrayD<f32> = outputs[i]
                .try_extract_array::<f32>()
                .map_err(|e| AiError::InferenceFailed(format!("output extraction '{}': {e}", spec.name)))?
                .into_owned();

            result_tensors.push(Tensor {
                name: spec.name.clone(),
                data: extracted,
            });
        }
        let output_time = output_start.elapsed();

        self.run_count += 1;

        let total_time = total_start.elapsed();
        log::debug!(
            "ONNX inference '{}' #{}: total={:?} (input={:?}, kernel={:?}, output={:?})",
            self.config.model_name,
            self.run_count,
            total_time,
            input_time,
            kernel_time,
            output_time,
        );

        Ok(result_tensors)
    }

    /// Run inference and return profiling data.
    pub fn run_profiled(&mut self, inputs: &[Tensor]) -> AiResult<(Vec<Tensor>, OnnxInferenceProfile)> {
        let total_start = Instant::now();

        let input_start = Instant::now();
        let mut ort_inputs: Vec<(String, ort::session::SessionInputValue<'_>)> =
            Vec::with_capacity(inputs.len());
        for tensor in inputs {
            let value = OrtTensor::from_array(tensor.data.clone())
                .map_err(|e| AiError::InferenceFailed(format!("input conversion '{}': {e}", tensor.name)))?;
            ort_inputs.push((tensor.name.clone(), value.into()));
        }
        let input_conversion_time = input_start.elapsed();

        let kernel_start = Instant::now();
        let outputs = self
            .session
            .run(ort_inputs)
            .map_err(|e| AiError::InferenceFailed(format!("runtime error: {e}")))?;
        let kernel_time = kernel_start.elapsed();

        let output_start = Instant::now();
        let mut result_tensors = Vec::with_capacity(self.output_specs.len());
        for (i, spec) in self.output_specs.iter().enumerate() {
            let extracted: ArrayD<f32> = outputs[i]
                .try_extract_array::<f32>()
                .map_err(|e| AiError::InferenceFailed(format!("output extraction '{}': {e}", spec.name)))?
                .into_owned();
            result_tensors.push(Tensor {
                name: spec.name.clone(),
                data: extracted,
            });
        }
        let output_conversion_time = output_start.elapsed();

        self.run_count += 1;

        let profile = OnnxInferenceProfile {
            total_time: total_start.elapsed(),
            input_conversion_time,
            kernel_time,
            output_conversion_time,
            model_name: self.config.model_name.clone(),
        };

        Ok((result_tensors, profile))
    }
}

/// Simulated ONNX session for use without the `onnx` feature.
///
/// Returns deterministic output tensors based on input data,
/// mimicking real model behavior for testing and development.
pub struct SimulatedOnnxSession {
    config: OnnxSessionConfig,
    input_specs: Vec<TensorSpec>,
    output_specs: Vec<TensorSpec>,
    run_count: u64,
    /// Simulation mode controlling output behavior.
    pub mode: SimulationMode,
}

/// How the simulated session generates output.
#[derive(Clone, Debug, PartialEq)]
pub enum SimulationMode {
    /// Output all zeros.
    Zeros,
    /// Output all ones.
    Ones,
    /// Output is a function of input (deterministic hash).
    DeterministicHash,
    /// Output echoes a downsampled version of input.
    Echo,
}

impl Default for SimulationMode {
    fn default() -> Self {
        SimulationMode::DeterministicHash
    }
}

impl SimulatedOnnxSession {
    /// Create a new simulated session.
    pub fn new(
        config: OnnxSessionConfig,
        input_specs: Vec<TensorSpec>,
        output_specs: Vec<TensorSpec>,
    ) -> Self {
        Self {
            config,
            input_specs,
            output_specs,
            run_count: 0,
            mode: SimulationMode::default(),
        }
    }

    /// Input specs.
    pub fn input_specs(&self) -> &[TensorSpec] {
        &self.input_specs
    }

    /// Output specs.
    pub fn output_specs(&self) -> &[TensorSpec] {
        &self.output_specs
    }

    /// Run count.
    pub fn run_count(&self) -> u64 {
        self.run_count
    }

    /// Model name.
    pub fn model_name(&self) -> &str {
        &self.config.model_name
    }

    /// Set simulation mode.
    pub fn with_mode(mut self, mode: SimulationMode) -> Self {
        self.mode = mode;
        self
    }

    /// Run simulated inference.
    pub fn run(&mut self, inputs: &[Tensor]) -> AiResult<Vec<Tensor>> {
        // Validate input count
        if inputs.len() != self.input_specs.len() {
            return Err(AiError::InferenceFailed(format!(
                "expected {} inputs, got {}",
                self.input_specs.len(),
                inputs.len()
            )));
        }

        // Generate outputs based on simulation mode
        let outputs = self
            .output_specs
            .iter()
            .map(|spec| {
                let shape: Vec<usize> = spec
                    .shape
                    .iter()
                    .map(|&d| if d < 0 { 1usize } else { d as usize })
                    .collect();

                let data = match self.mode {
                    SimulationMode::Zeros => Array::zeros(IxDyn(&shape)),
                    SimulationMode::Ones => Array::ones(IxDyn(&shape)),
                    SimulationMode::DeterministicHash => {
                        // Generate deterministic output from input stats
                        let input_sum: f32 = inputs.iter().flat_map(|t| t.data.iter()).sum();
                        let hash_base = (input_sum.abs() + 1.0).ln() / 10.0;
                        Array::from_shape_fn(IxDyn(&shape), |idx| {
                            let dim_view = idx.as_array_view();
                            let i: usize = dim_view.iter().copied().sum();
                            let v = ((i as f32 + hash_base) * 0.1).sin().abs();
                            v.clamp(0.0, 1.0)
                        })
                    }
                    SimulationMode::Echo => {
                        // Echo first input, resized to output shape
                        if let Some(first_input) = inputs.first() {
                            let input_len = first_input.data.len();
                            Array::from_shape_fn(IxDyn(&shape), |idx| {
                                let dim_view = idx.as_array_view();
                                let flat_idx: usize = dim_view.iter().copied().sum();
                                let src_idx = flat_idx % input_len;
                                first_input.data.as_slice().map(|s| s[src_idx]).unwrap_or(0.0)
                            })
                        } else {
                            Array::zeros(IxDyn(&shape))
                        }
                    }
                };

                Tensor {
                    name: spec.name.clone(),
                    data,
                }
            })
            .collect();

        self.run_count += 1;
        Ok(outputs)
    }
}

/// Unified inference backend that works with or without ONNX Runtime.
///
/// When the `onnx` feature is enabled, can load real ONNX models.
/// Otherwise, uses simulated inference.
pub enum InferenceBackendSession {
    /// Simulated backend (always available).
    Simulated(SimulatedOnnxSession),
    /// Real ONNX Runtime backend (requires `onnx` feature).
    #[cfg(feature = "onnx")]
    Onnx(OnnxSession),
}

impl InferenceBackendSession {
    /// Create a simulated session.
    pub fn simulated(
        config: OnnxSessionConfig,
        input_specs: Vec<TensorSpec>,
        output_specs: Vec<TensorSpec>,
    ) -> Self {
        Self::Simulated(SimulatedOnnxSession::new(config, input_specs, output_specs))
    }

    /// Load a real ONNX model from file (requires `onnx` feature).
    #[cfg(feature = "onnx")]
    pub fn from_onnx_file(path: impl AsRef<Path>, config: OnnxSessionConfig) -> AiResult<Self> {
        Ok(Self::Onnx(OnnxSession::from_file(path, config)?))
    }

    /// Load a real ONNX model from bytes (requires `onnx` feature).
    #[cfg(feature = "onnx")]
    pub fn from_onnx_bytes(bytes: &[u8], config: OnnxSessionConfig) -> AiResult<Self> {
        Ok(Self::Onnx(OnnxSession::from_bytes(bytes, config)?))
    }

    /// Run inference.
    pub fn run(&mut self, inputs: &[Tensor]) -> AiResult<Vec<Tensor>> {
        match self {
            Self::Simulated(session) => session.run(inputs),
            #[cfg(feature = "onnx")]
            Self::Onnx(session) => session.run(inputs),
        }
    }

    /// Get input specs.
    pub fn input_specs(&self) -> &[TensorSpec] {
        match self {
            Self::Simulated(s) => s.input_specs(),
            #[cfg(feature = "onnx")]
            Self::Onnx(s) => s.input_specs(),
        }
    }

    /// Get output specs.
    pub fn output_specs(&self) -> &[TensorSpec] {
        match self {
            Self::Simulated(s) => s.output_specs(),
            #[cfg(feature = "onnx")]
            Self::Onnx(s) => s.output_specs(),
        }
    }

    /// Run count.
    pub fn run_count(&self) -> u64 {
        match self {
            Self::Simulated(s) => s.run_count(),
            #[cfg(feature = "onnx")]
            Self::Onnx(s) => s.run_count(),
        }
    }

    /// Model name.
    pub fn model_name(&self) -> &str {
        match self {
            Self::Simulated(s) => s.model_name(),
            #[cfg(feature = "onnx")]
            Self::Onnx(s) => s.model_name(),
        }
    }

    /// Whether this is a real ONNX session.
    pub fn is_onnx(&self) -> bool {
        match self {
            Self::Simulated(_) => false,
            #[cfg(feature = "onnx")]
            Self::Onnx(_) => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_onnx_session_config_default() {
        let cfg = OnnxSessionConfig::default();
        assert_eq!(cfg.num_threads, 4);
        assert_eq!(cfg.optimization_level, 3);
        assert!(!cfg.enable_profiling);
        assert!(cfg.model_name.is_empty());
    }

    #[test]
    fn test_onnx_session_config_builder() {
        let cfg = OnnxSessionConfig::default()
            .with_threads(8)
            .with_optimization(2)
            .with_name("test-model")
            .with_profiling(true);
        assert_eq!(cfg.num_threads, 8);
        assert_eq!(cfg.optimization_level, 2);
        assert_eq!(cfg.model_name, "test-model");
        assert!(cfg.enable_profiling);
    }

    #[test]
    fn test_onnx_session_config_optimization_clamp() {
        let cfg = OnnxSessionConfig::default().with_optimization(99);
        assert_eq!(cfg.optimization_level, 3);
    }

    #[test]
    fn test_tensor_spec() {
        let spec = TensorSpec {
            name: "input".to_string(),
            shape: vec![1, 3, 224, 224],
            elem_type: "f32".to_string(),
        };
        assert_eq!(spec.name, "input");
        assert_eq!(spec.shape, vec![1, 3, 224, 224]);
    }

    #[test]
    fn test_simulated_session_zeros() {
        let mut session = SimulatedOnnxSession::new(
            OnnxSessionConfig::default().with_name("test"),
            vec![TensorSpec {
                name: "input".into(),
                shape: vec![1, 10],
                elem_type: "f32".into(),
            }],
            vec![TensorSpec {
                name: "output".into(),
                shape: vec![1, 5],
                elem_type: "f32".into(),
            }],
        )
        .with_mode(SimulationMode::Zeros);

        let input = Tensor::zeros("input", &[1, 10]);
        let outputs = session.run(&[input]).unwrap();
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].shape(), &[1, 5]);
        assert!(outputs[0].data.iter().all(|&v| v == 0.0));
        assert_eq!(session.run_count(), 1);
    }

    #[test]
    fn test_simulated_session_ones() {
        let mut session = SimulatedOnnxSession::new(
            OnnxSessionConfig::default(),
            vec![TensorSpec {
                name: "x".into(),
                shape: vec![1, 3],
                elem_type: "f32".into(),
            }],
            vec![TensorSpec {
                name: "y".into(),
                shape: vec![1, 2],
                elem_type: "f32".into(),
            }],
        )
        .with_mode(SimulationMode::Ones);

        let input = Tensor::zeros("x", &[1, 3]);
        let outputs = session.run(&[input]).unwrap();
        assert!(outputs[0].data.iter().all(|&v| v == 1.0));
    }

    #[test]
    fn test_simulated_session_deterministic() {
        let mut session = SimulatedOnnxSession::new(
            OnnxSessionConfig::default(),
            vec![TensorSpec {
                name: "x".into(),
                shape: vec![1, 4],
                elem_type: "f32".into(),
            }],
            vec![TensorSpec {
                name: "y".into(),
                shape: vec![1, 3],
                elem_type: "f32".into(),
            }],
        )
        .with_mode(SimulationMode::DeterministicHash);

        let input = Tensor::from_vec("x", vec![1.0, 2.0, 3.0, 4.0], &[1, 4]).unwrap();
        let out1 = session.run(&[input.clone()]).unwrap();
        let out2 = session.run(&[input]).unwrap();
        // Deterministic: same input → same output
        assert_eq!(out1[0].data, out2[0].data);
    }

    #[test]
    fn test_simulated_session_wrong_input_count() {
        let mut session = SimulatedOnnxSession::new(
            OnnxSessionConfig::default(),
            vec![
                TensorSpec { name: "a".into(), shape: vec![1], elem_type: "f32".into() },
                TensorSpec { name: "b".into(), shape: vec![1], elem_type: "f32".into() },
            ],
            vec![TensorSpec { name: "out".into(), shape: vec![1], elem_type: "f32".into() }],
        );

        let result = session.run(&[Tensor::zeros("a", &[1])]);
        assert!(result.is_err());
    }

    #[test]
    fn test_simulated_session_echo_mode() {
        let mut session = SimulatedOnnxSession::new(
            OnnxSessionConfig::default(),
            vec![TensorSpec {
                name: "x".into(),
                shape: vec![4],
                elem_type: "f32".into(),
            }],
            vec![TensorSpec {
                name: "y".into(),
                shape: vec![2],
                elem_type: "f32".into(),
            }],
        )
        .with_mode(SimulationMode::Echo);

        let input = Tensor::from_vec("x", vec![0.1, 0.2, 0.3, 0.4], &[4]).unwrap();
        let outputs = session.run(&[input]).unwrap();
        assert_eq!(outputs[0].shape(), &[2]);
    }

    #[test]
    fn test_inference_backend_session_simulated() {
        let mut backend = InferenceBackendSession::simulated(
            OnnxSessionConfig::default().with_name("test"),
            vec![TensorSpec { name: "x".into(), shape: vec![1, 5], elem_type: "f32".into() }],
            vec![TensorSpec { name: "y".into(), shape: vec![1, 3], elem_type: "f32".into() }],
        );

        assert!(!backend.is_onnx());
        assert_eq!(backend.model_name(), "test");
        assert_eq!(backend.input_specs().len(), 1);
        assert_eq!(backend.output_specs().len(), 1);

        let input = Tensor::zeros("x", &[1, 5]);
        let outputs = backend.run(&[input]).unwrap();
        assert_eq!(outputs.len(), 1);
        assert_eq!(backend.run_count(), 1);
    }

    #[test]
    fn test_simulation_mode_default() {
        let mode = SimulationMode::default();
        assert_eq!(mode, SimulationMode::DeterministicHash);
    }

    #[test]
    fn test_onnx_inference_profile() {
        let profile = OnnxInferenceProfile {
            total_time: Duration::from_millis(10),
            input_conversion_time: Duration::from_millis(1),
            kernel_time: Duration::from_millis(8),
            output_conversion_time: Duration::from_millis(1),
            model_name: "test".to_string(),
        };
        assert_eq!(profile.total_time.as_millis(), 10);
        assert_eq!(profile.model_name, "test");
    }

    #[test]
    fn test_dynamic_shape_handling() {
        let mut session = SimulatedOnnxSession::new(
            OnnxSessionConfig::default(),
            vec![TensorSpec {
                name: "x".into(),
                shape: vec![-1, 3],  // dynamic batch
                elem_type: "f32".into(),
            }],
            vec![TensorSpec {
                name: "y".into(),
                shape: vec![-1, 2],  // dynamic batch
                elem_type: "f32".into(),
            }],
        )
        .with_mode(SimulationMode::Zeros);

        let input = Tensor::zeros("x", &[4, 3]);
        let outputs = session.run(&[input]).unwrap();
        // Dynamic dims default to 1 in simulation
        assert_eq!(outputs[0].shape(), &[1, 2]);
    }

    #[test]
    fn test_multiple_outputs() {
        let mut session = SimulatedOnnxSession::new(
            OnnxSessionConfig::default(),
            vec![TensorSpec {
                name: "x".into(),
                shape: vec![1, 10],
                elem_type: "f32".into(),
            }],
            vec![
                TensorSpec { name: "logits".into(), shape: vec![1, 5], elem_type: "f32".into() },
                TensorSpec { name: "embeddings".into(), shape: vec![1, 64], elem_type: "f32".into() },
                TensorSpec { name: "attention".into(), shape: vec![1, 8, 10], elem_type: "f32".into() },
            ],
        )
        .with_mode(SimulationMode::Ones);

        let input = Tensor::zeros("x", &[1, 10]);
        let outputs = session.run(&[input]).unwrap();
        assert_eq!(outputs.len(), 3);
        assert_eq!(outputs[0].name, "logits");
        assert_eq!(outputs[1].name, "embeddings");
        assert_eq!(outputs[2].name, "attention");
        assert_eq!(outputs[0].shape(), &[1, 5]);
        assert_eq!(outputs[1].shape(), &[1, 64]);
        assert_eq!(outputs[2].shape(), &[1, 8, 10]);
    }
}
