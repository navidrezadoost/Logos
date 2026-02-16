//! Quantization benchmarks — compare FP32 vs FP16 vs INT8 model performance.
//!
//! Requires `onnx` feature: `cargo bench --bench quantization_benchmark --features onnx`
//!
//! ## Performance Targets
//!
//! | Metric             | FP32    | FP16     | INT8     |
//! |---------------------|---------|----------|----------|
//! | Model load time     | <1ms    | <1ms     | <1ms     |
//! | Inference time      | baseline| ≤ FP32   | ≤ FP32   |
//! | Size reduction      | 1.0×    | ~2.0×    | ~4.0×    |

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use std::path::PathBuf;
use std::time::Duration;

use logos_ai::inference::engine::Tensor;
use logos_ai::inference::onnx_session::{OnnxSession, OnnxSessionConfig};
use logos_ai::models::quantization::{ModelPrecision, QuantizationManager};
use logos_ai::models::embedding::{EmbeddedModel, EmbeddedModelRegistry};

fn test_models_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-models")
}

// ──────────────────────────────────────────────
// Model loading: FP32 vs FP16 vs INT8
// ──────────────────────────────────────────────

fn bench_quantized_model_loading(c: &mut Criterion) {
    let mut group = c.benchmark_group("quantized_load");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));
    group.sample_size(20);

    let dir = test_models_dir();

    let variants = vec![
        ("layout_gen", "FP32", "layout_gen.onnx"),
        ("layout_gen", "FP16", "layout_gen_fp16.onnx"),
        ("layout_gen", "INT8", "layout_gen_int8.onnx"),
        ("style_encoder", "FP32", "style_encoder.onnx"),
        ("style_encoder", "FP16", "style_encoder_fp16.onnx"),
        ("style_encoder", "INT8", "style_encoder_int8.onnx"),
        ("asset_decoder", "FP32", "asset_decoder.onnx"),
        ("asset_decoder", "FP16", "asset_decoder_fp16.onnx"),
        ("asset_decoder", "INT8", "asset_decoder_int8.onnx"),
    ];

    for (model, precision, filename) in &variants {
        let path = dir.join(filename);
        if !path.exists() {
            continue;
        }

        let label = format!("{model}/{precision}");
        let config = OnnxSessionConfig::default()
            .with_name(*model)
            .with_threads(1)
            .with_optimization(3);

        // Validate model loads before benchmarking
        if OnnxSession::from_file(&path, config).is_err() {
            continue;
        }

        group.bench_with_input(
            BenchmarkId::new("from_file", &label),
            &path,
            |b, path| {
                b.iter(|| {
                    OnnxSession::from_file(
                        black_box(path),
                        OnnxSessionConfig::default()
                            .with_name(*model)
                            .with_threads(1)
                            .with_optimization(3),
                    )
                    .unwrap()
                });
            },
        );
    }

    group.finish();
}

// ──────────────────────────────────────────────
// Inference: FP32 vs FP16 vs INT8 (layout_gen)
// ──────────────────────────────────────────────

fn bench_quantized_layout_inference(c: &mut Criterion) {
    let mut group = c.benchmark_group("quantized_layout_inference");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));

    let dir = test_models_dir();

    for (precision, filename) in &[
        ("FP32", "layout_gen.onnx"),
        ("FP16", "layout_gen_fp16.onnx"),
        ("INT8", "layout_gen_int8.onnx"),
    ] {
        let path = dir.join(filename);
        if !path.exists() {
            continue;
        }

        let config = OnnxSessionConfig::default()
            .with_name("layout_gen")
            .with_threads(1)
            .with_optimization(3);

        if let Ok(mut session) = OnnxSession::from_file(&path, config) {
            let input = Tensor::zeros("input", &[1, 105]);
            // Validate inference works (FP16 models may reject f32 inputs)
            if session.run(&[input.clone()]).is_err() {
                continue;
            }
            group.bench_function(
                BenchmarkId::new("single", *precision),
                |b| {
                    b.iter(|| session.run(&[black_box(input.clone())]).unwrap());
                },
            );
        }
    }

    group.finish();
}

// ──────────────────────────────────────────────
// Inference: FP32 vs FP16 vs INT8 (style_encoder)
// ──────────────────────────────────────────────

fn bench_quantized_encoder_inference(c: &mut Criterion) {
    let mut group = c.benchmark_group("quantized_encoder_inference");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));

    let dir = test_models_dir();

    for (precision, filename) in &[
        ("FP32", "style_encoder.onnx"),
        ("FP16", "style_encoder_fp16.onnx"),
        ("INT8", "style_encoder_int8.onnx"),
    ] {
        let path = dir.join(filename);
        if !path.exists() {
            continue;
        }

        let config = OnnxSessionConfig::default()
            .with_name("style_encoder")
            .with_threads(1)
            .with_optimization(3);

        if let Ok(mut session) = OnnxSession::from_file(&path, config) {
            let input = Tensor::zeros("input", &[1, 3, 64, 64]);
            if session.run(&[input.clone()]).is_err() {
                continue;
            }
            group.bench_function(
                BenchmarkId::new("encode", *precision),
                |b| {
                    b.iter(|| session.run(&[black_box(input.clone())]).unwrap());
                },
            );
        }
    }

    group.finish();
}

// ──────────────────────────────────────────────
// Inference: FP32 vs FP16 vs INT8 (asset_decoder)
// ──────────────────────────────────────────────

fn bench_quantized_decoder_inference(c: &mut Criterion) {
    let mut group = c.benchmark_group("quantized_decoder_inference");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));

    let dir = test_models_dir();

    for (precision, filename) in &[
        ("FP32", "asset_decoder.onnx"),
        ("FP16", "asset_decoder_fp16.onnx"),
        ("INT8", "asset_decoder_int8.onnx"),
    ] {
        let path = dir.join(filename);
        if !path.exists() {
            continue;
        }

        let config = OnnxSessionConfig::default()
            .with_name("asset_decoder")
            .with_threads(1)
            .with_optimization(3);

        if let Ok(mut session) = OnnxSession::from_file(&path, config) {
            let input = Tensor::zeros("input", &[1, 64]);
            if session.run(&[input.clone()]).is_err() {
                continue;
            }
            group.bench_function(
                BenchmarkId::new("decode", *precision),
                |b| {
                    b.iter(|| session.run(&[black_box(input.clone())]).unwrap());
                },
            );
        }
    }

    group.finish();
}

// ──────────────────────────────────────────────
// QuantizationManager scan + select benchmarks
// ──────────────────────────────────────────────

fn bench_quantization_manager(c: &mut Criterion) {
    let mut group = c.benchmark_group("quantization_manager");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(3));

    let dir = test_models_dir();

    // Scan all variants
    group.bench_function("scan_all_models", |b| {
        b.iter(|| {
            let mut mgr = QuantizationManager::new(black_box(&dir));
            mgr.scan_variants("layout_gen").unwrap();
            mgr.scan_variants("style_encoder").unwrap();
            mgr.scan_variants("asset_decoder").unwrap();
            black_box(mgr.variant_count())
        });
    });

    // Best variant selection
    let mut mgr = QuantizationManager::new(&dir);
    mgr.scan_variants("layout_gen").unwrap();
    mgr.scan_variants("style_encoder").unwrap();
    mgr.scan_variants("asset_decoder").unwrap();

    group.bench_function("best_variant_lookup", |b| {
        b.iter(|| {
            let _ = black_box(mgr.best_variant("layout_gen"));
            let _ = black_box(mgr.best_variant("style_encoder"));
            let _ = black_box(mgr.best_variant("asset_decoder"));
        });
    });

    // Size report generation
    group.bench_function("size_report", |b| {
        b.iter(|| {
            let r1 = mgr.size_report("layout_gen");
            let r2 = mgr.size_report("style_encoder");
            let r3 = mgr.size_report("asset_decoder");
            black_box((r1, r2, r3))
        });
    });

    group.finish();
}

// ──────────────────────────────────────────────
// Embedded model registry benchmarks
// ──────────────────────────────────────────────

fn bench_embedded_registry(c: &mut Criterion) {
    let mut group = c.benchmark_group("embedded_registry");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(3));

    let dir = test_models_dir();

    // Registry creation + population
    group.bench_function("create_and_populate", |b| {
        b.iter(|| {
            let mut reg = EmbeddedModelRegistry::new();
            for (name, file) in &[
                ("layout_gen", "layout_gen.onnx"),
                ("style_encoder", "style_encoder.onnx"),
                ("asset_decoder", "asset_decoder.onnx"),
            ] {
                let path = dir.join(file);
                if path.exists() {
                    let bytes = std::fs::read(&path).unwrap();
                    reg.register(EmbeddedModel::from_owned(*name, "1.0.0", bytes));
                }
            }
            black_box(reg.count())
        });
    });

    // Pre-populated registry lookups
    let mut reg = EmbeddedModelRegistry::new();
    for (name, file) in &[
        ("layout_gen", "layout_gen.onnx"),
        ("style_encoder", "style_encoder.onnx"),
        ("asset_decoder", "asset_decoder.onnx"),
    ] {
        let path = dir.join(file);
        if path.exists() {
            let bytes = std::fs::read(&path).unwrap();
            reg.register(EmbeddedModel::from_owned(*name, "1.0.0", bytes));
        }
    }

    group.bench_function("get_model", |b| {
        b.iter(|| {
            let _ = black_box(reg.get("layout_gen"));
            let _ = black_box(reg.get("style_encoder"));
            let _ = black_box(reg.get("asset_decoder"));
        });
    });

    group.bench_function("get_bytes", |b| {
        b.iter(|| {
            let bytes = reg.get_bytes("layout_gen").unwrap();
            black_box(bytes.len())
        });
    });

    // Load from embedded bytes → ONNX session
    group.bench_function("bytes_to_session", |b| {
        let bytes = reg.get_bytes("layout_gen").unwrap();
        b.iter(|| {
            OnnxSession::from_bytes(
                black_box(bytes),
                OnnxSessionConfig::default()
                    .with_name("layout_gen")
                    .with_threads(1)
                    .with_optimization(3),
            )
            .unwrap()
        });
    });

    group.finish();
}

// ──────────────────────────────────────────────
// Size comparison: FP32 vs FP16 vs INT8 (reporting)
// ──────────────────────────────────────────────

fn bench_size_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("size_comparison");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(3));

    let dir = test_models_dir();
    let mut mgr = QuantizationManager::new(&dir);
    mgr.scan_variants("layout_gen").unwrap();
    mgr.scan_variants("style_encoder").unwrap();
    mgr.scan_variants("asset_decoder").unwrap();

    // Benchmark loading each precision variant from disk
    for (base, input_name, input_shape) in &[
        ("layout_gen", "input", vec![1usize, 105]),
        ("style_encoder", "input", vec![1, 3, 64, 64]),
        ("asset_decoder", "input", vec![1, 64]),
    ] {
        for precision in &[ModelPrecision::FP32, ModelPrecision::FP16, ModelPrecision::INT8] {
            if let Some(variant) = mgr.get_variant(base, *precision) {
                let path = variant.path.clone();
                let _size = variant.size_bytes;
                let label = format!("{base}/{}", precision.label());

                group.bench_with_input(
                    BenchmarkId::new("load_and_infer", &label),
                    &(),
                    |b, _| {
                        b.iter(|| {
                            let config = OnnxSessionConfig::default()
                                .with_name(*base)
                                .with_threads(1)
                                .with_optimization(3);
                            if let Ok(mut session) = OnnxSession::from_file(black_box(&path), config) {
                                let input = Tensor::zeros(*input_name, input_shape);
                                let _ = session.run(&[input]);
                            }
                        });
                    },
                );
            }
        }
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_quantized_model_loading,
    bench_quantized_layout_inference,
    bench_quantized_encoder_inference,
    bench_quantized_decoder_inference,
    bench_quantization_manager,
    bench_embedded_registry,
    bench_size_comparison,
);
criterion_main!(benches);
