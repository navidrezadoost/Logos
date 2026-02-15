use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use std::time::Duration;

use logos_ai::inference::asset_gen::{AssetGenerator, GenerationParams, ImageSize};
use logos_ai::inference::engine::{InferenceEngine, Tensor};
use logos_ai::inference::layout_gen::{ElementHint, LayoutConstraints, LayoutGenerator};
use logos_ai::inference::onnx_session::{
    InferenceBackendSession, OnnxSessionConfig, TensorSpec,
};
use logos_ai::inference::style_transfer::StyleTransfer;
use logos_ai::models::{ModelFormat, ModelInfo, ModelRegistry};
use logos_ai::preprocess::image_tensor::ImageTensor;
use logos_ai::preprocess::tokenizer::TextTokenizer;
use ndarray::Array3;

// ──────────────────────────────────────────────
// Layout generation benchmarks
// ──────────────────────────────────────────────

fn bench_layout_generation(c: &mut Criterion) {
    let mut group = c.benchmark_group("layout_gen");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(3));

    let mut gen = LayoutGenerator::new();
    for variations in [1, 5, 10] {
        let constraints = LayoutConstraints::new(1920.0, 1080.0)
            .add_element(ElementHint::new("text").with_role("heading"))
            .add_element(ElementHint::new("image").with_role("hero"))
            .add_element(ElementHint::new("text").with_role("body"))
            .add_element(ElementHint::new("rect").with_role("cta"))
            .with_variations(variations);

        group.bench_with_input(
            BenchmarkId::new("variations", variations),
            &constraints,
            |b, constraints| {
                b.iter(|| gen.generate(black_box(constraints)).unwrap());
            },
        );
    }
    group.finish();
}

fn bench_layout_gen_element_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("layout_gen_elements");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(3));

    let mut gen = LayoutGenerator::new();
    for elem_count in [2, 4, 8, 16] {
        let mut constraints = LayoutConstraints::new(1920.0, 1080.0).with_variations(5);
        for i in 0..elem_count {
            constraints = constraints.add_element(
                ElementHint::new(if i % 2 == 0 { "text" } else { "image" })
                    .with_role(&format!("elem_{}", i)),
            );
        }

        group.bench_with_input(
            BenchmarkId::new("elements", elem_count),
            &constraints,
            |b, constraints| {
                b.iter(|| gen.generate(black_box(constraints)).unwrap());
            },
        );
    }
    group.finish();
}

// ──────────────────────────────────────────────
// Style transfer benchmarks
// ──────────────────────────────────────────────

fn bench_style_transfer(c: &mut Criterion) {
    let mut group = c.benchmark_group("style_transfer");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(3));

    let mut engine = StyleTransfer::new();
    let style = Array3::from_shape_fn((3, 128, 128), |(c, y, x)| {
        ((c * 50 + y + x) as f32 / 500.0).min(1.0)
    });
    let embedding = engine.extract_style(&style).unwrap();

    for size in [128, 256, 512] {
        let content = Array3::from_shape_fn((3, size, size), |(c, y, x)| {
            ((c * 100 + y + x) as f32 / 1000.0).min(1.0)
        });

        group.bench_with_input(
            BenchmarkId::new("transfer", format!("{}x{}", size, size)),
            &content,
            |b, content| {
                b.iter(|| engine.transfer(black_box(content), &embedding, None).unwrap());
            },
        );
    }

    // Benchmark style extraction separately
    group.bench_function("extract_128x128", |b| {
        b.iter(|| engine.extract_style(black_box(&style)).unwrap());
    });

    group.finish();
}

// ──────────────────────────────────────────────
// Asset generation benchmarks
// ──────────────────────────────────────────────

fn bench_asset_generation(c: &mut Criterion) {
    let mut group = c.benchmark_group("asset_gen");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(3));

    let mut gen = AssetGenerator::new();

    for size in [ImageSize::Small, ImageSize::Medium, ImageSize::Large] {
        let label = match size {
            ImageSize::Small => "256x256",
            ImageSize::Medium => "512x512",
            ImageSize::Large => "1024x1024",
            _ => "unknown",
        };
        let params = GenerationParams::new("a beautiful sunset over mountains")
            .with_size(size)
            .with_seed(42);

        group.bench_with_input(BenchmarkId::new("generate", label), &params, |b, params| {
            b.iter(|| gen.generate(black_box(params)).unwrap());
        });
    }

    // Batch generation
    let params = GenerationParams::new("a beautiful sunset")
        .with_size(ImageSize::Small)
        .with_seed(42)
        .with_count(4);
    group.bench_function("batch_4_256x256", |b| {
        b.iter(|| gen.generate(black_box(&params)).unwrap());
    });

    group.finish();
}

// ──────────────────────────────────────────────
// Simulated inference backend benchmarks
// ──────────────────────────────────────────────

fn bench_simulated_backend(c: &mut Criterion) {
    let mut group = c.benchmark_group("simulated_backend");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(3));

    // Layout-sized model
    let mut layout_backend = InferenceBackendSession::simulated(
        OnnxSessionConfig::default().with_name("layout_sim"),
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
    let layout_input = Tensor::zeros("input", &[1, 105]);

    group.bench_function("layout_105_to_80", |b| {
        b.iter(|| layout_backend.run(&[black_box(layout_input.clone())]).unwrap());
    });

    // Style encoder model
    let mut encoder_backend = InferenceBackendSession::simulated(
        OnnxSessionConfig::default().with_name("encoder_sim"),
        vec![TensorSpec {
            name: "input".into(),
            shape: vec![1, 3, 64, 64],
            elem_type: "f32".into(),
        }],
        vec![TensorSpec {
            name: "output".into(),
            shape: vec![1, 64],
            elem_type: "f32".into(),
        }],
    );
    let encoder_input = Tensor::zeros("input", &[1, 3, 64, 64]);

    group.bench_function("encoder_3x64x64_to_64", |b| {
        b.iter(|| {
            encoder_backend
                .run(&[black_box(encoder_input.clone())])
                .unwrap()
        });
    });

    // Asset decoder model
    let mut decoder_backend = InferenceBackendSession::simulated(
        OnnxSessionConfig::default().with_name("decoder_sim"),
        vec![TensorSpec {
            name: "input".into(),
            shape: vec![1, 64],
            elem_type: "f32".into(),
        }],
        vec![TensorSpec {
            name: "output".into(),
            shape: vec![1, 3, 32, 32],
            elem_type: "f32".into(),
        }],
    );
    let decoder_input = Tensor::zeros("input", &[1, 64]);

    group.bench_function("decoder_64_to_3x32x32", |b| {
        b.iter(|| {
            decoder_backend
                .run(&[black_box(decoder_input.clone())])
                .unwrap()
        });
    });

    group.finish();
}

// ──────────────────────────────────────────────
// Inference engine benchmarks
// ──────────────────────────────────────────────

fn bench_inference_engine(c: &mut Criterion) {
    let mut group = c.benchmark_group("inference_engine");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(3));

    let mut engine = InferenceEngine::new();
    engine.create_session(
        "bench-session",
        vec![("input".into(), vec![1, 105])],
        vec![("output".into(), vec![1, 80])],
    );
    let input = Tensor::zeros("input", &[1, 105]);

    group.bench_function("run_105_to_80", |b| {
        b.iter(|| {
            engine
                .run("bench-session", &[black_box(input.clone())])
                .unwrap()
        });
    });

    group.finish();
}

// ──────────────────────────────────────────────
// Model registry benchmarks
// ──────────────────────────────────────────────

fn bench_model_registry(c: &mut Criterion) {
    let mut group = c.benchmark_group("model_registry");
    group.warm_up_time(Duration::from_millis(500));

    group.bench_function("register_100", |b| {
        b.iter(|| {
            let mut registry = ModelRegistry::new("/tmp/logos-bench-models");
            for i in 0..100 {
                let model =
                    ModelInfo::new(format!("model-{}", i), ModelFormat::Onnx).with_size(1024);
                registry.register(model).unwrap();
            }
            black_box(&registry);
        })
    });

    group.finish();
}

// ──────────────────────────────────────────────
// Preprocessing benchmarks
// ──────────────────────────────────────────────

fn bench_image_preprocessing(c: &mut Criterion) {
    let mut group = c.benchmark_group("image_preprocess");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(3));

    for size in [128u32, 256, 512, 1024] {
        let bytes = vec![128u8; (size * size * 3) as usize];

        group.bench_with_input(
            BenchmarkId::new("from_rgb", format!("{}x{}", size, size)),
            &bytes,
            |b, bytes| {
                b.iter(|| ImageTensor::from_rgb_bytes(black_box(bytes), size, size).unwrap());
            },
        );
    }

    let bytes = vec![128u8; 256 * 256 * 3];
    let img = ImageTensor::from_rgb_bytes(&bytes, 256, 256).unwrap();

    group.bench_function("normalize_imagenet_256x256", |b| {
        b.iter(|| black_box(img.normalize_imagenet()));
    });

    group.bench_function("to_batch_tensor_256x256", |b| {
        b.iter(|| black_box(img.to_batch_tensor()));
    });

    group.finish();
}

fn bench_tokenizer(c: &mut Criterion) {
    let mut group = c.benchmark_group("tokenizer");
    group.warm_up_time(Duration::from_millis(500));

    let tokenizer = TextTokenizer::new();

    group.bench_function("short_prompt", |b| {
        b.iter(|| tokenizer.encode(black_box("a beautiful sunset")).unwrap());
    });

    let long_prompt = "a beautiful modern minimalist design with gradient background and \
        clean typography featuring bold heading text centered layout with warm color palette \
        and subtle shadow effects on a light background";

    group.bench_function("long_prompt", |b| {
        b.iter(|| tokenizer.encode(black_box(long_prompt)).unwrap());
    });

    group.finish();
}

// ──────────────────────────────────────────────
// End-to-end pipeline benchmark (simulated)
// ──────────────────────────────────────────────

fn bench_pipeline_e2e(c: &mut Criterion) {
    let mut group = c.benchmark_group("pipeline_e2e");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));

    let tokenizer = TextTokenizer::new();
    let mut layout_gen = LayoutGenerator::new();
    let mut style_transfer = StyleTransfer::new();
    let mut asset_gen = AssetGenerator::new();

    group.bench_function("full_pipeline_simulated", |b| {
        b.iter(|| {
            // 1. Tokenize
            let _tokens = tokenizer.encode("a beautiful sunset").unwrap();

            // 2. Generate layout
            let constraints = LayoutConstraints::new(1920.0, 1080.0)
                .add_element(ElementHint::new("text").with_role("heading"))
                .add_element(ElementHint::new("image").with_role("hero"))
                .with_variations(3);
            let _layouts = layout_gen.generate(&constraints).unwrap();

            // 3. Style transfer
            let content = Array3::zeros((3, 256, 256));
            let style = Array3::zeros((3, 128, 128));
            let embedding = style_transfer.extract_style(&style).unwrap();
            let _transferred = style_transfer.transfer(&content, &embedding, None).unwrap();

            // 4. Asset generation
            let params = GenerationParams::new("a beautiful sunset")
                .with_size(ImageSize::Small)
                .with_seed(42);
            let _images = asset_gen.generate(&params).unwrap();
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_layout_generation,
    bench_layout_gen_element_scaling,
    bench_style_transfer,
    bench_asset_generation,
    bench_simulated_backend,
    bench_inference_engine,
    bench_model_registry,
    bench_image_preprocessing,
    bench_tokenizer,
    bench_pipeline_e2e,
);
criterion_main!(benches);
