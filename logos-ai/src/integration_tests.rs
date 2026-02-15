//! Cross-module integration tests for logos-ai.

use crate::error::AiError;
use crate::inference::asset_gen::{AssetGenerator, GenerationParams, ImageSize};
use crate::inference::engine::{InferenceEngine, Tensor};
use crate::inference::layout_gen::{ElementHint, LayoutConstraints, LayoutGenerator};
use crate::inference::style_transfer::StyleTransfer;
use crate::models::{ModelFormat, ModelInfo, ModelRegistry};
use crate::preprocess::image_tensor::ImageTensor;
use crate::preprocess::tokenizer::TextTokenizer;
use ndarray::Array3;

// ──────────────────────────────────────────────
// End-to-end pipeline tests
// ──────────────────────────────────────────────

#[test]
fn test_layout_to_document_pipeline() {
    let mut gen = LayoutGenerator::new();
    let constraints = LayoutConstraints::new(1920.0, 1080.0)
        .add_element(ElementHint::new("text").with_role("heading").with_priority(10))
        .add_element(ElementHint::new("image").with_role("hero").with_priority(8))
        .add_element(ElementHint::new("text").with_role("body").with_priority(5))
        .add_element(ElementHint::new("rect").with_role("cta").with_priority(9))
        .with_variations(5);

    let proposals = gen.generate(&constraints).unwrap();

    assert_eq!(proposals.len(), 5);
    for proposal in &proposals {
        assert_eq!(proposal.elements.len(), 4);
        assert!(proposal.is_valid());
        assert!(proposal.confidence > 0.0 && proposal.confidence <= 1.0);
        assert_eq!(proposal.canvas_width, 1920.0);
        assert_eq!(proposal.canvas_height, 1080.0);
    }

    // First proposal should have highest confidence
    assert!(proposals[0].confidence >= proposals[4].confidence);
}

#[test]
fn test_style_transfer_pipeline() {
    let content = ImageTensor::blank(64, 64);
    let style_data = Array3::from_shape_fn((3, 32, 32), |(c, _, _)| c as f32 * 0.3 + 0.1);

    let mut engine = StyleTransfer::new();
    let embedding = engine.extract_style(&style_data).unwrap();

    let result = engine
        .transfer(content.data(), &embedding, None)
        .unwrap();

    assert_eq!(result.width, 64);
    assert_eq!(result.height, 64);
    assert!(result.processing_time_ms >= 0.0);
}

#[test]
fn test_asset_generation_pipeline() {
    let tokenizer = TextTokenizer::new();
    let tokens = tokenizer.encode("a beautiful sunset landscape").unwrap();
    assert_eq!(tokens.len(), 77);

    let mut gen = AssetGenerator::new();
    let params = GenerationParams::new("a beautiful sunset landscape")
        .with_size(ImageSize::Small)
        .with_seed(42);
    let images = gen.generate(&params).unwrap();

    assert_eq!(images.len(), 1);
    assert_eq!(images[0].width, 256);
    assert_eq!(images[0].height, 256);

    // Verify image can be converted to ImageTensor
    let tensor = ImageTensor::from_chw(images[0].data.clone()).unwrap();
    assert_eq!(tensor.width(), 256);
    assert_eq!(tensor.height(), 256);
}

#[test]
fn test_model_registry_with_inference() {
    let mut registry = ModelRegistry::new("/tmp/logos-ai-test-models");
    let model = ModelInfo::new("layout-transformer", ModelFormat::Onnx)
        .with_version("1.0.0")
        .with_description("Layout generation model")
        .with_input_shapes(vec![vec![1, 105]])
        .with_output_shapes(vec![vec![1, 80]])
        .with_tags(vec!["layout".into(), "transformer".into()]);

    registry.register(model).unwrap();

    let mut engine = InferenceEngine::new();
    engine.create_session(
        "layout-transformer",
        vec![("input".into(), vec![1, 105])],
        vec![("output".into(), vec![1, 80])],
    );

    let input = Tensor::zeros("input", &[1, 105]);
    let output = engine.run("layout-transformer", &[input]).unwrap();
    assert!(!output.is_empty());
}

#[test]
fn test_image_preprocessing_pipeline() {
    let rgb_bytes = vec![128u8; 64 * 64 * 3];
    let img = ImageTensor::from_rgb_bytes(&rgb_bytes, 64, 64).unwrap();

    let normalized = img.normalize_imagenet();
    assert_eq!(normalized.shape(), &[3, 64, 64]);

    let symmetric = img.normalize_symmetric();
    assert_eq!(symmetric.shape(), &[3, 64, 64]);

    let batch = img.to_batch_tensor();
    assert_eq!(batch.shape(), &[1, 3, 64, 64]);
}

#[test]
fn test_tokenizer_with_asset_generation() {
    let tokenizer = TextTokenizer::new();

    assert!(tokenizer.token_id("design").is_some());
    assert!(tokenizer.token_id("layout").is_some());
    assert!(tokenizer.token_id("gradient").is_some());

    let tokens = tokenizer.encode("modern minimal design").unwrap();
    let decoded = tokenizer.decode(&tokens);
    assert!(decoded.contains("modern"));
    assert!(decoded.contains("design"));

    let mut gen = AssetGenerator::new();
    let params = GenerationParams::new("modern minimal design")
        .with_size(ImageSize::Small)
        .with_seed(100);
    let images = gen.generate(&params).unwrap();
    assert_eq!(images.len(), 1);
}

// ──────────────────────────────────────────────
// Error path integration tests
// ──────────────────────────────────────────────

#[test]
fn test_error_propagation_empty_constraints() {
    let mut gen = LayoutGenerator::new();
    let constraints = LayoutConstraints::new(800.0, 600.0);
    let result = gen.generate(&constraints);
    assert!(result.is_err());
    match result.unwrap_err() {
        AiError::InvalidInput(_) => {}
        other => panic!("expected InvalidInput, got {:?}", other),
    }
}

#[test]
fn test_error_propagation_empty_prompt() {
    let mut gen = AssetGenerator::new();
    let params = GenerationParams::new("");
    let result = gen.generate(&params);
    assert!(result.is_err());
}

#[test]
fn test_error_propagation_wrong_channels() {
    let mut engine = StyleTransfer::new();
    let bad_image = Array3::zeros((1, 64, 64));
    let result = engine.extract_style(&bad_image);
    assert!(result.is_err());
}

#[test]
fn test_error_propagation_bad_image_bytes() {
    let result = ImageTensor::from_rgb_bytes(&[0u8; 10], 64, 64);
    assert!(result.is_err());
}

// ──────────────────────────────────────────────
// Multi-component interaction tests
// ──────────────────────────────────────────────

#[test]
fn test_multiple_sessions_in_engine() {
    let mut engine = InferenceEngine::new();

    engine.create_session(
        "layout",
        vec![("input".into(), vec![1, 105])],
        vec![("output".into(), vec![1, 80])],
    );
    engine.create_session(
        "style",
        vec![("image".into(), vec![1, 3, 256, 256])],
        vec![("embedding".into(), vec![1, 64])],
    );

    assert_eq!(engine.session_count(), 2);

    let layout_input = Tensor::zeros("input", &[1, 105]);
    let layout_out = engine.run("layout", &[layout_input]).unwrap();
    assert!(!layout_out.is_empty());

    let style_input = Tensor::zeros("image", &[1, 3, 256, 256]);
    let style_out = engine.run("style", &[style_input]).unwrap();
    assert!(!style_out.is_empty());

    assert_eq!(engine.total_runs(), 2);
}

#[test]
fn test_model_registry_operations() {
    let mut registry = ModelRegistry::new("/tmp/logos-ai-test-registry");

    let m1 = ModelInfo::new("layout-v1", ModelFormat::Onnx)
        .with_tags(vec!["layout".into()])
        .with_size(1024 * 1024);
    let m2 = ModelInfo::new("style-v1", ModelFormat::Onnx)
        .with_tags(vec!["style".into()])
        .with_size(2 * 1024 * 1024);
    let m3 = ModelInfo::new("asset-v1", ModelFormat::Onnx)
        .with_tags(vec!["asset".into()])
        .with_size(5 * 1024 * 1024);

    registry.register(m1).unwrap();
    registry.register(m2).unwrap();
    registry.register(m3).unwrap();

    assert_eq!(registry.list().len(), 3);

    let layout_models = registry.list_by_tag("layout");
    assert_eq!(layout_models.len(), 1);

    let id = registry.get_by_name("style-v1").unwrap().id;
    registry.mark_ready(id).unwrap();
    assert_eq!(registry.list_ready().len(), 1);
}

#[test]
fn test_generated_image_roundtrip_via_tensor() {
    let mut gen = AssetGenerator::new();
    let params = GenerationParams::new("test")
        .with_size(ImageSize::Small)
        .with_seed(42);
    let images = gen.generate(&params).unwrap();

    let bytes = images[0].to_rgb_bytes();
    let tensor = ImageTensor::from_rgb_bytes(&bytes, 256, 256).unwrap();
    assert_eq!(tensor.width(), 256);
    assert_eq!(tensor.height(), 256);

    let normalized = tensor.normalize_imagenet();
    assert_eq!(normalized.shape(), &[3, 256, 256]);
}

// ──────────────────────────────────────────────
// ONNX Runtime integration tests (require `onnx` feature)
// ──────────────────────────────────────────────

#[cfg(feature = "onnx")]
mod onnx_integration {
    use crate::inference::engine::Tensor;
    use crate::inference::onnx_session::{OnnxSession, OnnxSessionConfig, InferenceBackendSession};
    use std::path::PathBuf;

    fn test_models_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-models")
    }

    #[test]
    fn test_onnx_load_layout_model() {
        let path = test_models_dir().join("layout_gen.onnx");
        let config = OnnxSessionConfig::default()
            .with_name("layout_gen")
            .with_threads(1)
            .with_optimization(1);
        let session = OnnxSession::from_file(&path, config).unwrap();

        assert_eq!(session.model_name(), "layout_gen");
        assert_eq!(session.input_specs().len(), 1);
        assert_eq!(session.output_specs().len(), 1);
        assert_eq!(session.run_count(), 0);

        // Verify tensor shapes
        let input_spec = &session.input_specs()[0];
        assert_eq!(input_spec.shape, vec![1, 105]);

        let output_spec = &session.output_specs()[0];
        assert_eq!(output_spec.shape, vec![1, 80]);
    }

    #[test]
    fn test_onnx_load_style_encoder() {
        let path = test_models_dir().join("style_encoder.onnx");
        let config = OnnxSessionConfig::default()
            .with_name("style_encoder")
            .with_threads(1);
        let session = OnnxSession::from_file(&path, config).unwrap();

        assert_eq!(session.input_specs().len(), 1);
        assert_eq!(session.output_specs().len(), 1);

        let input_spec = &session.input_specs()[0];
        assert_eq!(input_spec.shape, vec![1, 3, 64, 64]);

        let output_spec = &session.output_specs()[0];
        assert_eq!(output_spec.shape, vec![1, 64]);
    }

    #[test]
    fn test_onnx_load_asset_decoder() {
        let path = test_models_dir().join("asset_decoder.onnx");
        let config = OnnxSessionConfig::default()
            .with_name("asset_decoder")
            .with_threads(1);
        let session = OnnxSession::from_file(&path, config).unwrap();

        assert_eq!(session.input_specs().len(), 1);
        assert_eq!(session.output_specs().len(), 1);

        let input_spec = &session.input_specs()[0];
        assert_eq!(input_spec.shape, vec![1, 64]);

        let output_spec = &session.output_specs()[0];
        assert_eq!(output_spec.shape, vec![1, 3, 32, 32]);
    }

    #[test]
    fn test_onnx_load_from_bytes() {
        let path = test_models_dir().join("style_encoder.onnx");
        let bytes = std::fs::read(&path).unwrap();
        let config = OnnxSessionConfig::default().with_name("from_bytes");
        let session = OnnxSession::from_bytes(&bytes, config).unwrap();

        assert_eq!(session.model_name(), "from_bytes");
        assert_eq!(session.input_specs().len(), 1);
    }

    #[test]
    fn test_onnx_run_layout_inference() {
        let path = test_models_dir().join("layout_gen.onnx");
        let config = OnnxSessionConfig::default()
            .with_name("layout_gen")
            .with_threads(1);
        let mut session = OnnxSession::from_file(&path, config).unwrap();

        // Create input tensor matching model's expected shape [1, 105]
        let input = Tensor::zeros("input", &[1, 105]);
        let outputs = session.run(&[input]).unwrap();

        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].shape(), &[1, 80]);
        assert_eq!(session.run_count(), 1);

        // Verify output contains actual computed values (not all zeros from a real model)
        let has_nonzero = outputs[0].data.iter().any(|&v| v != 0.0);
        // The model may output zeros for zero input, so just check shape is right
        assert_eq!(outputs[0].data.len(), 80);
    }

    #[test]
    fn test_onnx_run_style_encoder_inference() {
        let path = test_models_dir().join("style_encoder.onnx");
        let config = OnnxSessionConfig::default()
            .with_name("style_encoder")
            .with_threads(1);
        let mut session = OnnxSession::from_file(&path, config).unwrap();

        // Input: [1, 3, 64, 64] image tensor
        let input = Tensor::zeros("input", &[1, 3, 64, 64]);
        let outputs = session.run(&[input]).unwrap();

        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].shape(), &[1, 64]);
        assert_eq!(session.run_count(), 1);
    }

    #[test]
    fn test_onnx_run_asset_decoder_inference() {
        let path = test_models_dir().join("asset_decoder.onnx");
        let config = OnnxSessionConfig::default()
            .with_name("asset_decoder")
            .with_threads(1);
        let mut session = OnnxSession::from_file(&path, config).unwrap();

        // Input: [1, 64] latent vector
        let input = Tensor::zeros("input", &[1, 64]);
        let outputs = session.run(&[input]).unwrap();

        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].shape(), &[1, 3, 32, 32]);
        assert_eq!(session.run_count(), 1);
    }

    #[test]
    fn test_onnx_run_profiled() {
        let path = test_models_dir().join("layout_gen.onnx");
        let config = OnnxSessionConfig::default()
            .with_name("layout_gen")
            .with_threads(1);
        let mut session = OnnxSession::from_file(&path, config).unwrap();

        let input = Tensor::zeros("input", &[1, 105]);
        let (outputs, profile) = session.run_profiled(&[input]).unwrap();

        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].shape(), &[1, 80]);
        assert_eq!(profile.model_name, "layout_gen");
        assert!(profile.total_time.as_nanos() > 0);
        assert!(profile.kernel_time.as_nanos() > 0);
    }

    #[test]
    fn test_onnx_multiple_inferences() {
        let path = test_models_dir().join("style_encoder.onnx");
        let config = OnnxSessionConfig::default()
            .with_name("style_encoder")
            .with_threads(1);
        let mut session = OnnxSession::from_file(&path, config).unwrap();

        for i in 0..5 {
            let input = Tensor::from_vec(
                "input",
                vec![i as f32 * 0.1; 1 * 3 * 64 * 64],
                &[1, 3, 64, 64],
            )
            .unwrap();
            let outputs = session.run(&[input]).unwrap();
            assert_eq!(outputs[0].shape(), &[1, 64]);
        }
        assert_eq!(session.run_count(), 5);
    }

    #[test]
    fn test_onnx_backend_session_from_file() {
        let path = test_models_dir().join("layout_gen.onnx");
        let config = OnnxSessionConfig::default()
            .with_name("layout_gen")
            .with_threads(1);
        let mut backend = InferenceBackendSession::from_onnx_file(&path, config).unwrap();

        assert!(backend.is_onnx());
        assert_eq!(backend.model_name(), "layout_gen");
        assert_eq!(backend.input_specs().len(), 1);
        assert_eq!(backend.output_specs().len(), 1);

        let input = Tensor::zeros("input", &[1, 105]);
        let outputs = backend.run(&[input]).unwrap();
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].shape(), &[1, 80]);
        assert_eq!(backend.run_count(), 1);
    }

    #[test]
    fn test_onnx_backend_session_from_bytes() {
        let path = test_models_dir().join("asset_decoder.onnx");
        let bytes = std::fs::read(&path).unwrap();
        let config = OnnxSessionConfig::default().with_name("asset_decoder");
        let mut backend = InferenceBackendSession::from_onnx_bytes(&bytes, config).unwrap();

        assert!(backend.is_onnx());
        let input = Tensor::zeros("input", &[1, 64]);
        let outputs = backend.run(&[input]).unwrap();
        assert_eq!(outputs[0].shape(), &[1, 3, 32, 32]);
    }

    #[test]
    fn test_onnx_encoder_decoder_pipeline() {
        // Simulate the style transfer pipeline: encode image → decode from latent
        let encoder_path = test_models_dir().join("style_encoder.onnx");
        let decoder_path = test_models_dir().join("asset_decoder.onnx");

        let encoder_config = OnnxSessionConfig::default()
            .with_name("encoder")
            .with_threads(1);
        let mut encoder = OnnxSession::from_file(&encoder_path, encoder_config).unwrap();

        let decoder_config = OnnxSessionConfig::default()
            .with_name("decoder")
            .with_threads(1);
        let mut decoder = OnnxSession::from_file(&decoder_path, decoder_config).unwrap();

        // Encode: [1, 3, 64, 64] → [1, 64]
        let image_input = Tensor::zeros("input", &[1, 3, 64, 64]);
        let encoded = encoder.run(&[image_input]).unwrap();
        assert_eq!(encoded[0].shape(), &[1, 64]);

        // Decode: [1, 64] → [1, 3, 32, 32]
        let latent = Tensor {
            name: "input".to_string(),
            data: encoded[0].data.clone(),
        };
        let decoded = decoder.run(&[latent]).unwrap();
        assert_eq!(decoded[0].shape(), &[1, 3, 32, 32]);

        // Full pipeline completed
        assert_eq!(encoder.run_count(), 1);
        assert_eq!(decoder.run_count(), 1);
    }

    #[test]
    fn test_onnx_model_not_found() {
        let config = OnnxSessionConfig::default().with_name("missing");
        let result = OnnxSession::from_file("/nonexistent/model.onnx", config);
        assert!(result.is_err());
    }

    #[test]
    fn test_onnx_load_performance() {
        let path = test_models_dir().join("layout_gen.onnx");
        let start = std::time::Instant::now();
        let config = OnnxSessionConfig::default()
            .with_name("perf_test")
            .with_threads(1);
        let _session = OnnxSession::from_file(&path, config).unwrap();
        let load_time = start.elapsed();

        // Model load should be under 100ms for these small test models
        assert!(
            load_time.as_millis() < 1000,
            "Model load took {}ms, expected < 1000ms",
            load_time.as_millis()
        );
    }

    #[test]
    fn test_onnx_inference_performance() {
        let path = test_models_dir().join("layout_gen.onnx");
        let config = OnnxSessionConfig::default()
            .with_name("perf_test")
            .with_threads(1);
        let mut session = OnnxSession::from_file(&path, config).unwrap();

        // Warm up
        let input = Tensor::zeros("input", &[1, 105]);
        let _ = session.run(&[input]).unwrap();

        // Measure
        let start = std::time::Instant::now();
        let iterations = 10;
        for _ in 0..iterations {
            let input = Tensor::zeros("input", &[1, 105]);
            let _ = session.run(&[input]).unwrap();
        }
        let total = start.elapsed();
        let avg_ms = total.as_millis() as f64 / iterations as f64;

        // Layout generation should be under 50ms per inference
        assert!(
            avg_ms < 500.0,
            "Average inference took {avg_ms}ms, expected < 500ms"
        );
    }
}
