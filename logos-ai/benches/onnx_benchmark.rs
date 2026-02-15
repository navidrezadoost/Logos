//! ONNX Runtime benchmarks — real model inference performance.
//!
//! Requires `onnx` feature: `cargo bench --bench onnx_benchmark --features onnx`
//!
//! ## Performance Targets
//!
//! | Benchmark                 | Target   |
//! |---------------------------|----------|
//! | `model_load/layout_gen`   | <100ms   |
//! | `model_load/style_encoder`| <100ms   |
//! | `model_load/asset_decoder`| <100ms   |
//! | `layout_gen/10_variations`| <50ms    |
//! | `style_encoder/64x64`    | <16ms    |
//! | `asset_decoder/32x32`    | <2s      |
//! | `pipeline/encode_decode`  | <2.1s    |

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use std::path::PathBuf;
use std::time::Duration;

use logos_ai::inference::engine::Tensor;
use logos_ai::inference::onnx_session::{
    InferenceBackendSession, OnnxSession, OnnxSessionConfig,
};

fn test_models_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-models")
}

// ──────────────────────────────────────────────
// Model loading benchmarks
// ──────────────────────────────────────────────

fn bench_model_load(c: &mut Criterion) {
    let mut group = c.benchmark_group("model_load");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));
    group.sample_size(20); // Model loading is heavy; fewer samples

    let models = vec![
        ("layout_gen", "layout_gen.onnx"),
        ("style_encoder", "style_encoder.onnx"),
        ("asset_decoder", "asset_decoder.onnx"),
    ];

    for (name, filename) in &models {
        let path = test_models_dir().join(filename);
        group.bench_with_input(BenchmarkId::new("from_file", name), &path, |b, path| {
            b.iter(|| {
                OnnxSession::from_file(
                    black_box(path),
                    OnnxSessionConfig::default()
                        .with_name(*name)
                        .with_threads(1)
                        .with_optimization(3),
                )
                .unwrap()
            });
        });
    }

    // Benchmark loading from bytes (in-memory)
    for (name, filename) in &models {
        let path = test_models_dir().join(filename);
        let bytes = std::fs::read(&path).unwrap();
        group.bench_with_input(
            BenchmarkId::new("from_bytes", name),
            &bytes,
            |b, bytes| {
                b.iter(|| {
                    OnnxSession::from_bytes(
                        black_box(bytes),
                        OnnxSessionConfig::default()
                            .with_name(*name)
                            .with_threads(1),
                    )
                    .unwrap()
                });
            },
        );
    }

    group.finish();
}

// ──────────────────────────────────────────────
// Layout generation ONNX inference
// ──────────────────────────────────────────────

fn bench_onnx_layout_gen(c: &mut Criterion) {
    let mut group = c.benchmark_group("onnx_layout_gen");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));

    let path = test_models_dir().join("layout_gen.onnx");
    let config = OnnxSessionConfig::default()
        .with_name("layout_gen")
        .with_threads(1)
        .with_optimization(3);
    let mut session = OnnxSession::from_file(&path, config).unwrap();

    // Single inference
    let input = Tensor::zeros("input", &[1, 105]);
    group.bench_function("single_inference", |b| {
        b.iter(|| session.run(&[black_box(input.clone())]).unwrap());
    });

    // Simulate 10 variations (10 sequential inferences)
    group.bench_function("10_variations", |b| {
        b.iter(|| {
            let mut results = Vec::with_capacity(10);
            for i in 0..10 {
                let input = Tensor::from_vec(
                    "input",
                    vec![i as f32 * 0.1; 105],
                    &[1, 105],
                )
                .unwrap();
                results.push(session.run(&[input]).unwrap());
            }
            black_box(results)
        });
    });

    // With profiling enabled
    group.bench_function("profiled_inference", |b| {
        let input = Tensor::zeros("input", &[1, 105]);
        b.iter(|| session.run_profiled(&[black_box(input.clone())]).unwrap());
    });

    group.finish();
}

// ──────────────────────────────────────────────
// Style encoder ONNX inference
// ──────────────────────────────────────────────

fn bench_onnx_style_encoder(c: &mut Criterion) {
    let mut group = c.benchmark_group("onnx_style_encoder");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));

    let path = test_models_dir().join("style_encoder.onnx");
    let config = OnnxSessionConfig::default()
        .with_name("style_encoder")
        .with_threads(1)
        .with_optimization(3);
    let mut session = OnnxSession::from_file(&path, config).unwrap();

    // 64x64 image encoding
    let input = Tensor::zeros("input", &[1, 3, 64, 64]);
    group.bench_function("encode_64x64", |b| {
        b.iter(|| session.run(&[black_box(input.clone())]).unwrap());
    });

    // With profiling
    group.bench_function("encode_64x64_profiled", |b| {
        b.iter(|| session.run_profiled(&[black_box(input.clone())]).unwrap());
    });

    // Multiple sequential encodings (simulate batch style extraction)
    group.bench_function("encode_5_images", |b| {
        b.iter(|| {
            let mut embeddings = Vec::with_capacity(5);
            for i in 0..5 {
                let input = Tensor::from_vec(
                    "input",
                    vec![i as f32 * 0.05; 1 * 3 * 64 * 64],
                    &[1, 3, 64, 64],
                )
                .unwrap();
                embeddings.push(session.run(&[input]).unwrap());
            }
            black_box(embeddings)
        });
    });

    group.finish();
}

// ──────────────────────────────────────────────
// Asset decoder ONNX inference
// ──────────────────────────────────────────────

fn bench_onnx_asset_decoder(c: &mut Criterion) {
    let mut group = c.benchmark_group("onnx_asset_decoder");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));

    let path = test_models_dir().join("asset_decoder.onnx");
    let config = OnnxSessionConfig::default()
        .with_name("asset_decoder")
        .with_threads(1)
        .with_optimization(3);
    let mut session = OnnxSession::from_file(&path, config).unwrap();

    // Single decode
    let input = Tensor::zeros("input", &[1, 64]);
    group.bench_function("decode_32x32", |b| {
        b.iter(|| session.run(&[black_box(input.clone())]).unwrap());
    });

    // Different latent vectors
    group.bench_function("decode_varied_latents", |b| {
        b.iter(|| {
            let mut images = Vec::with_capacity(4);
            for i in 0..4 {
                let input = Tensor::from_vec(
                    "input",
                    vec![(i as f32 * 0.25).sin(); 64],
                    &[1, 64],
                )
                .unwrap();
                images.push(session.run(&[input]).unwrap());
            }
            black_box(images)
        });
    });

    group.finish();
}

// ──────────────────────────────────────────────
// End-to-end pipeline benchmark (real ONNX)
// ──────────────────────────────────────────────

fn bench_onnx_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("onnx_pipeline");
    group.warm_up_time(Duration::from_millis(1000));
    group.measurement_time(Duration::from_secs(10));

    let encoder_path = test_models_dir().join("style_encoder.onnx");
    let decoder_path = test_models_dir().join("asset_decoder.onnx");

    let encoder_config = OnnxSessionConfig::default()
        .with_name("encoder")
        .with_threads(1)
        .with_optimization(3);
    let mut encoder = OnnxSession::from_file(&encoder_path, encoder_config).unwrap();

    let decoder_config = OnnxSessionConfig::default()
        .with_name("decoder")
        .with_threads(1)
        .with_optimization(3);
    let mut decoder = OnnxSession::from_file(&decoder_path, decoder_config).unwrap();

    // Encoder → Decoder pipeline
    group.bench_function("encode_decode", |b| {
        b.iter(|| {
            // Encode: [1, 3, 64, 64] → [1, 64]
            let image_input = Tensor::zeros("input", &[1, 3, 64, 64]);
            let encoded = encoder.run(&[image_input]).unwrap();

            // Decode: [1, 64] → [1, 3, 32, 32]
            let latent = Tensor {
                name: "input".to_string(),
                data: encoded[0].data.clone(),
            };
            let decoded = decoder.run(&[latent]).unwrap();
            black_box(decoded)
        });
    });

    // Full pipeline with layout gen
    let layout_path = test_models_dir().join("layout_gen.onnx");
    let layout_config = OnnxSessionConfig::default()
        .with_name("layout")
        .with_threads(1)
        .with_optimization(3);
    let mut layout_session = OnnxSession::from_file(&layout_path, layout_config).unwrap();

    group.bench_function("full_pipeline_3_models", |b| {
        b.iter(|| {
            // 1. Layout generation
            let layout_input = Tensor::zeros("input", &[1, 105]);
            let layout_output = layout_session.run(&[layout_input]).unwrap();

            // 2. Style encoding
            let image_input = Tensor::zeros("input", &[1, 3, 64, 64]);
            let style_embedding = encoder.run(&[image_input]).unwrap();

            // 3. Asset decoding
            let latent = Tensor {
                name: "input".to_string(),
                data: style_embedding[0].data.clone(),
            };
            let generated = decoder.run(&[latent]).unwrap();

            black_box((layout_output, generated))
        });
    });

    group.finish();
}

// ──────────────────────────────────────────────
// Thread scaling benchmarks
// ──────────────────────────────────────────────

fn bench_thread_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("thread_scaling");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));
    group.sample_size(20);

    let path = test_models_dir().join("layout_gen.onnx");

    for threads in [1, 2, 4] {
        let config = OnnxSessionConfig::default()
            .with_name("layout_gen")
            .with_threads(threads)
            .with_optimization(3);
        let mut session = OnnxSession::from_file(&path, config).unwrap();
        let input = Tensor::zeros("input", &[1, 105]);

        group.bench_with_input(
            BenchmarkId::new("layout_gen_threads", threads),
            &input,
            |b, input| {
                b.iter(|| session.run(&[black_box(input.clone())]).unwrap());
            },
        );
    }

    group.finish();
}

// ──────────────────────────────────────────────
// Optimization level benchmarks
// ──────────────────────────────────────────────

fn bench_optimization_levels(c: &mut Criterion) {
    let mut group = c.benchmark_group("optimization_level");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));
    group.sample_size(20);

    let path = test_models_dir().join("layout_gen.onnx");

    for opt_level in [0, 1, 2, 3] {
        let config = OnnxSessionConfig::default()
            .with_name("layout_gen")
            .with_threads(1)
            .with_optimization(opt_level);
        let mut session = OnnxSession::from_file(&path, config).unwrap();
        let input = Tensor::zeros("input", &[1, 105]);

        group.bench_with_input(
            BenchmarkId::new("layout_gen_opt", opt_level),
            &input,
            |b, input| {
                b.iter(|| session.run(&[black_box(input.clone())]).unwrap());
            },
        );
    }

    group.finish();
}

// ──────────────────────────────────────────────
// InferenceBackendSession benchmarks (real ONNX backend)
// ──────────────────────────────────────────────

fn bench_backend_session(c: &mut Criterion) {
    let mut group = c.benchmark_group("backend_session");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));

    // Real ONNX backend
    let path = test_models_dir().join("layout_gen.onnx");
    let config = OnnxSessionConfig::default()
        .with_name("layout_gen")
        .with_threads(1)
        .with_optimization(3);
    let mut onnx_backend = InferenceBackendSession::from_onnx_file(&path, config).unwrap();
    let input = Tensor::zeros("input", &[1, 105]);

    group.bench_function("onnx_backend_run", |b| {
        b.iter(|| onnx_backend.run(&[black_box(input.clone())]).unwrap());
    });

    // Simulated backend (for comparison)
    use logos_ai::inference::onnx_session::TensorSpec;
    let mut sim_backend = InferenceBackendSession::simulated(
        OnnxSessionConfig::default().with_name("sim_layout"),
        vec![TensorSpec {
            name: "input".into(),
            shape: vec![1, 105],
            elem_type: "f32".into(),
        }],
        vec![TensorSpec {
            name: "output".into(),
            shape: vec![1, 80],
            elem_type: "f32".into(),
        }],
    );

    group.bench_function("simulated_backend_run", |b| {
        b.iter(|| sim_backend.run(&[black_box(input.clone())]).unwrap());
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_model_load,
    bench_onnx_layout_gen,
    bench_onnx_style_encoder,
    bench_onnx_asset_decoder,
    bench_onnx_pipeline,
    bench_thread_scaling,
    bench_optimization_levels,
    bench_backend_session,
);
criterion_main!(benches);
