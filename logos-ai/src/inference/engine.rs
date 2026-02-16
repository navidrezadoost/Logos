//! Core inference engine — manages sessions, backends, and execution.

use crate::error::{AiError, AiResult};
use ndarray::{Array, IxDyn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Available inference backends.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InferenceBackend {
    /// CPU with optimized math kernels.
    Cpu,
    /// GPU acceleration (CUDA/Metal/Vulkan).
    Gpu,
    /// WebAssembly SIMD backend (browser).
    WasmSimd,
}

/// Configuration for an inference session.
#[derive(Clone, Debug)]
pub struct SessionConfig {
    /// Preferred backend.
    pub backend: InferenceBackend,
    /// Number of threads for CPU inference.
    pub num_threads: usize,
    /// Maximum execution time per inference call.
    pub timeout: Duration,
    /// Enable profiling.
    pub enable_profiling: bool,
    /// Optimization level (0-3).
    pub optimization_level: u8,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            backend: InferenceBackend::Cpu,
            num_threads: 4,
            timeout: Duration::from_secs(10),
            enable_profiling: false,
            optimization_level: 2,
        }
    }
}

impl SessionConfig {
    /// Set the backend.
    pub fn with_backend(mut self, backend: InferenceBackend) -> Self {
        self.backend = backend;
        self
    }

    /// Set number of threads.
    pub fn with_threads(mut self, n: usize) -> Self {
        self.num_threads = n;
        self
    }

    /// Set timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Enable profiling.
    pub fn with_profiling(mut self, enable: bool) -> Self {
        self.enable_profiling = enable;
        self
    }

    /// Set optimization level.
    pub fn with_optimization(mut self, level: u8) -> Self {
        self.optimization_level = level.min(3);
        self
    }
}

/// A named tensor (input or output).
#[derive(Clone, Debug)]
pub struct Tensor {
    /// Tensor name.
    pub name: String,
    /// Tensor data as a dynamic-dimensional ndarray.
    pub data: Array<f32, IxDyn>,
}

impl Tensor {
    /// Create a new named tensor.
    pub fn new(name: impl Into<String>, data: Array<f32, IxDyn>) -> Self {
        Self {
            name: name.into(),
            data,
        }
    }

    /// Total number of elements.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Whether the tensor is empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Shape of the tensor.
    pub fn shape(&self) -> &[usize] {
        self.data.shape()
    }

    /// Create a zeros tensor with given shape.
    pub fn zeros(name: impl Into<String>, shape: &[usize]) -> Self {
        Self {
            name: name.into(),
            data: Array::zeros(IxDyn(shape)),
        }
    }

    /// Create a ones tensor with given shape.
    pub fn ones(name: impl Into<String>, shape: &[usize]) -> Self {
        Self {
            name: name.into(),
            data: Array::ones(IxDyn(shape)),
        }
    }

    /// Create a tensor from a flat vector and shape.
    pub fn from_vec(name: impl Into<String>, data: Vec<f32>, shape: &[usize]) -> AiResult<Self> {
        let expected: usize = shape.iter().product();
        if data.len() != expected {
            return Err(AiError::InvalidInput(format!(
                "data length {} doesn't match shape {:?} (expected {})",
                data.len(),
                shape,
                expected
            )));
        }
        Ok(Self {
            name: name.into(),
            data: Array::from_shape_vec(IxDyn(shape), data)
                .map_err(|e| AiError::InvalidInput(e.to_string()))?,
        })
    }
}

/// Profiling info for an inference run.
#[derive(Clone, Debug)]
pub struct InferenceProfile {
    /// Total wall-clock time.
    pub total_time: Duration,
    /// Time spent on preprocessing.
    pub preprocess_time: Duration,
    /// Time spent on model execution.
    pub execution_time: Duration,
    /// Time spent on postprocessing.
    pub postprocess_time: Duration,
}

impl InferenceProfile {
    /// Create an empty profile.
    pub fn new() -> Self {
        Self {
            total_time: Duration::ZERO,
            preprocess_time: Duration::ZERO,
            execution_time: Duration::ZERO,
            postprocess_time: Duration::ZERO,
        }
    }
}

impl Default for InferenceProfile {
    fn default() -> Self {
        Self::new()
    }
}

/// An inference session that can run model computations.
///
/// This is a lightweight simulation of ONNX Runtime sessions.
/// In production, this wraps [`ort::Session`].
pub struct InferenceSession {
    /// Session configuration.
    config: SessionConfig,
    /// Model name this session is bound to.
    model_name: String,
    /// Input tensor names and expected shapes.
    input_specs: Vec<(String, Vec<usize>)>,
    /// Output tensor names and expected shapes.
    output_specs: Vec<(String, Vec<usize>)>,
    /// Number of runs completed.
    run_count: u64,
}

impl InferenceSession {
    /// Create a new inference session for the named model.
    pub fn new(
        model_name: impl Into<String>,
        config: SessionConfig,
        input_specs: Vec<(String, Vec<usize>)>,
        output_specs: Vec<(String, Vec<usize>)>,
    ) -> Self {
        Self {
            config,
            model_name: model_name.into(),
            input_specs,
            output_specs,
            run_count: 0,
        }
    }

    /// Run inference with the given input tensors.
    ///
    /// Validates inputs, simulates computation, and returns output tensors.
    pub fn run(&mut self, inputs: &[Tensor]) -> AiResult<Vec<Tensor>> {
        let start = Instant::now();

        // Validate input count
        if inputs.len() != self.input_specs.len() {
            return Err(AiError::InvalidInput(format!(
                "expected {} inputs, got {}",
                self.input_specs.len(),
                inputs.len()
            )));
        }

        // Validate input shapes
        for (input, (_expected_name, expected_shape)) in inputs.iter().zip(&self.input_specs) {
            if input.shape() != expected_shape.as_slice() {
                return Err(AiError::InvalidInput(format!(
                    "input '{}' has shape {:?}, expected {:?}",
                    input.name,
                    input.shape(),
                    expected_shape
                )));
            }
        }

        // Check timeout
        if start.elapsed() > self.config.timeout {
            return Err(AiError::Timeout(self.config.timeout.as_millis() as u64));
        }

        // Generate output tensors (simulated inference — zeros for now)
        let outputs: Vec<Tensor> = self
            .output_specs
            .iter()
            .map(|(name, shape)| Tensor::zeros(name.clone(), shape))
            .collect();

        self.run_count += 1;

        Ok(outputs)
    }

    /// Model name.
    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    /// Session config.
    pub fn config(&self) -> &SessionConfig {
        &self.config
    }

    /// Number of runs completed.
    pub fn run_count(&self) -> u64 {
        self.run_count
    }

    /// Input specifications.
    pub fn input_specs(&self) -> &[(String, Vec<usize>)] {
        &self.input_specs
    }

    /// Output specifications.
    pub fn output_specs(&self) -> &[(String, Vec<usize>)] {
        &self.output_specs
    }
}

/// Top-level inference engine managing multiple sessions.
pub struct InferenceEngine {
    /// Active sessions keyed by model name.
    sessions: HashMap<String, InferenceSession>,
    /// Default configuration for new sessions.
    default_config: SessionConfig,
    /// Total inference runs across all sessions.
    total_runs: u64,
}

impl InferenceEngine {
    /// Create a new inference engine with default config.
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            default_config: SessionConfig::default(),
            total_runs: 0,
        }
    }

    /// Create with custom default config.
    pub fn with_config(config: SessionConfig) -> Self {
        Self {
            sessions: HashMap::new(),
            default_config: config,
            total_runs: 0,
        }
    }

    /// Register a session for a model.
    pub fn create_session(
        &mut self,
        model_name: impl Into<String>,
        input_specs: Vec<(String, Vec<usize>)>,
        output_specs: Vec<(String, Vec<usize>)>,
    ) -> &mut InferenceSession {
        let name: String = model_name.into();
        let session = InferenceSession::new(
            name.clone(),
            self.default_config.clone(),
            input_specs,
            output_specs,
        );
        self.sessions.insert(name.clone(), session);
        self.sessions.get_mut(&name).unwrap()
    }

    /// Create a session with custom config.
    pub fn create_session_with_config(
        &mut self,
        model_name: impl Into<String>,
        config: SessionConfig,
        input_specs: Vec<(String, Vec<usize>)>,
        output_specs: Vec<(String, Vec<usize>)>,
    ) -> &mut InferenceSession {
        let name: String = model_name.into();
        let session = InferenceSession::new(name.clone(), config, input_specs, output_specs);
        self.sessions.insert(name.clone(), session);
        self.sessions.get_mut(&name).unwrap()
    }

    /// Get a mutable session by model name.
    pub fn session_mut(&mut self, model_name: &str) -> AiResult<&mut InferenceSession> {
        self.sessions
            .get_mut(model_name)
            .ok_or_else(|| AiError::ModelNotFound(model_name.into()))
    }

    /// Get a session by model name.
    pub fn session(&self, model_name: &str) -> AiResult<&InferenceSession> {
        self.sessions
            .get(model_name)
            .ok_or_else(|| AiError::ModelNotFound(model_name.into()))
    }

    /// Run inference on a named session.
    pub fn run(&mut self, model_name: &str, inputs: &[Tensor]) -> AiResult<Vec<Tensor>> {
        let session = self
            .sessions
            .get_mut(model_name)
            .ok_or_else(|| AiError::ModelNotFound(model_name.into()))?;
        let result = session.run(inputs)?;
        self.total_runs += 1;
        Ok(result)
    }

    /// Remove a session.
    pub fn remove_session(&mut self, model_name: &str) -> bool {
        self.sessions.remove(model_name).is_some()
    }

    /// Number of active sessions.
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Total inference runs across all sessions.
    pub fn total_runs(&self) -> u64 {
        self.total_runs
    }

    /// List active session names.
    pub fn session_names(&self) -> Vec<&str> {
        self.sessions.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for InferenceEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_config_default() {
        let cfg = SessionConfig::default();
        assert_eq!(cfg.backend, InferenceBackend::Cpu);
        assert_eq!(cfg.num_threads, 4);
        assert_eq!(cfg.optimization_level, 2);
        assert!(!cfg.enable_profiling);
    }

    #[test]
    fn test_session_config_builder() {
        let cfg = SessionConfig::default()
            .with_backend(InferenceBackend::Gpu)
            .with_threads(8)
            .with_timeout(Duration::from_secs(5))
            .with_profiling(true)
            .with_optimization(3);
        assert_eq!(cfg.backend, InferenceBackend::Gpu);
        assert_eq!(cfg.num_threads, 8);
        assert_eq!(cfg.timeout, Duration::from_secs(5));
        assert!(cfg.enable_profiling);
        assert_eq!(cfg.optimization_level, 3);
    }

    #[test]
    fn test_session_config_optimization_clamp() {
        let cfg = SessionConfig::default().with_optimization(10);
        assert_eq!(cfg.optimization_level, 3);
    }

    #[test]
    fn test_tensor_new() {
        let data = Array::zeros(IxDyn(&[1, 3, 224, 224]));
        let t = Tensor::new("input", data);
        assert_eq!(t.name, "input");
        assert_eq!(t.shape(), &[1, 3, 224, 224]);
        assert_eq!(t.len(), 1 * 3 * 224 * 224);
        assert!(!t.is_empty());
    }

    #[test]
    fn test_tensor_zeros() {
        let t = Tensor::zeros("x", &[2, 3]);
        assert_eq!(t.shape(), &[2, 3]);
        assert_eq!(t.len(), 6);
        assert!(t.data.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn test_tensor_ones() {
        let t = Tensor::ones("x", &[2, 3]);
        assert!(t.data.iter().all(|&v| v == 1.0));
    }

    #[test]
    fn test_tensor_from_vec() {
        let t = Tensor::from_vec("x", vec![1.0, 2.0, 3.0, 4.0], &[2, 2]).unwrap();
        assert_eq!(t.shape(), &[2, 2]);
        assert_eq!(t.data[[0, 0]], 1.0);
        assert_eq!(t.data[[1, 1]], 4.0);
    }

    #[test]
    fn test_tensor_from_vec_shape_mismatch() {
        let result = Tensor::from_vec("x", vec![1.0, 2.0, 3.0], &[2, 2]);
        assert!(result.is_err());
    }

    #[test]
    fn test_tensor_empty() {
        let t = Tensor::zeros("x", &[0]);
        assert!(t.is_empty());
        assert_eq!(t.len(), 0);
    }

    #[test]
    fn test_inference_profile() {
        let p = InferenceProfile::new();
        assert_eq!(p.total_time, Duration::ZERO);
        assert_eq!(p.preprocess_time, Duration::ZERO);
    }

    #[test]
    fn test_session_new() {
        let session = InferenceSession::new(
            "test-model",
            SessionConfig::default(),
            vec![("input".into(), vec![1, 3, 224, 224])],
            vec![("output".into(), vec![1, 10])],
        );
        assert_eq!(session.model_name(), "test-model");
        assert_eq!(session.run_count(), 0);
        assert_eq!(session.input_specs().len(), 1);
        assert_eq!(session.output_specs().len(), 1);
    }

    #[test]
    fn test_session_run_success() {
        let mut session = InferenceSession::new(
            "test",
            SessionConfig::default(),
            vec![("input".into(), vec![1, 3, 224, 224])],
            vec![("output".into(), vec![1, 10])],
        );
        let input = Tensor::zeros("input", &[1, 3, 224, 224]);
        let outputs = session.run(&[input]).unwrap();
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].shape(), &[1, 10]);
        assert_eq!(session.run_count(), 1);
    }

    #[test]
    fn test_session_run_wrong_input_count() {
        let mut session = InferenceSession::new(
            "test",
            SessionConfig::default(),
            vec![("a".into(), vec![1, 3]), ("b".into(), vec![1, 3])],
            vec![("out".into(), vec![1, 6])],
        );
        let input = Tensor::zeros("a", &[1, 3]);
        let result = session.run(&[input]);
        assert!(result.is_err());
    }

    #[test]
    fn test_session_run_wrong_shape() {
        let mut session = InferenceSession::new(
            "test",
            SessionConfig::default(),
            vec![("input".into(), vec![1, 3, 224, 224])],
            vec![("output".into(), vec![1, 10])],
        );
        let input = Tensor::zeros("input", &[1, 3, 128, 128]); // wrong shape
        let result = session.run(&[input]);
        assert!(result.is_err());
    }

    #[test]
    fn test_engine_new() {
        let engine = InferenceEngine::new();
        assert_eq!(engine.session_count(), 0);
        assert_eq!(engine.total_runs(), 0);
    }

    #[test]
    fn test_engine_create_session() {
        let mut engine = InferenceEngine::new();
        engine.create_session(
            "layout-gen",
            vec![("input".into(), vec![1, 512])],
            vec![("output".into(), vec![10, 4])],
        );
        assert_eq!(engine.session_count(), 1);
        assert!(engine.session_names().contains(&"layout-gen"));
    }

    #[test]
    fn test_engine_run() {
        let mut engine = InferenceEngine::new();
        engine.create_session(
            "classifier",
            vec![("features".into(), vec![1, 128])],
            vec![("logits".into(), vec![1, 10])],
        );
        let input = Tensor::zeros("features", &[1, 128]);
        let outputs = engine.run("classifier", &[input]).unwrap();
        assert_eq!(outputs.len(), 1);
        assert_eq!(engine.total_runs(), 1);
    }

    #[test]
    fn test_engine_run_missing_session() {
        let mut engine = InferenceEngine::new();
        let input = Tensor::zeros("x", &[1]);
        let result = engine.run("nonexistent", &[input]);
        assert!(result.is_err());
    }

    #[test]
    fn test_engine_remove_session() {
        let mut engine = InferenceEngine::new();
        engine.create_session(
            "test",
            vec![("x".into(), vec![1])],
            vec![("y".into(), vec![1])],
        );
        assert!(engine.remove_session("test"));
        assert_eq!(engine.session_count(), 0);
        assert!(!engine.remove_session("test"));
    }

    #[test]
    fn test_engine_multiple_sessions() {
        let mut engine = InferenceEngine::new();
        engine.create_session("a", vec![("x".into(), vec![1])], vec![("y".into(), vec![1])]);
        engine.create_session("b", vec![("x".into(), vec![2])], vec![("y".into(), vec![2])]);
        engine.create_session("c", vec![("x".into(), vec![3])], vec![("y".into(), vec![3])]);
        assert_eq!(engine.session_count(), 3);

        let input = Tensor::zeros("x", &[2]);
        engine.run("b", &[input]).unwrap();
        assert_eq!(engine.total_runs(), 1);
    }

    #[test]
    fn test_inference_backend_serialization() {
        let backend = InferenceBackend::Gpu;
        let json = serde_json::to_string(&backend).unwrap();
        let back: InferenceBackend = serde_json::from_str(&json).unwrap();
        assert_eq!(back, InferenceBackend::Gpu);
    }
}
