use criterion::{black_box, criterion_group, criterion_main, Criterion};

use logos_ai::inference::asset_gen::{AssetGenerator, GenerationParams, ImageSize};
use logos_ai::inference::engine::{InferenceEngine, Tensor};
use logos_ai::inference::layout_gen::{ElementHint, LayoutConstraints, LayoutGenerator};
use logos_ai::inference::style_transfer::StyleTransfer;
use logos_ai::models::{ModelFormat, ModelInfo, ModelRegistry};
use logos_ai::preprocess::image_tensor::ImageTensor;
use logos_ai::preprocess::tokenizer::TextTokenizer;
use ndarray::Array3;

fn bench_layout_generation(c: &mut Criterion) {
    let mut gen = LayoutGenerator::new();
    let constraints = LayoutConstraints::new(1920.0, 1080.0)
        .add_element(ElementHint::new("text").with_role("heading"))
        .add_element(ElementHint::new("image").with_role("hero"))
        .add_element(ElementHint::new("text").with_role("body"))
        .add_element(ElementHint::new("rect").with_role("cta"))
        .with_variations(10);

    c.bench_function("layout_gen_10_variations", |b| {
        b.iter(|| gen.generate(black_box(&constraints)).unwrap())
    });
}

fn bench_style_transfer(c: &mut Criterion) {
    let mut engine = StyleTransfer::new();
    let content = Array3::from_shape_fn((3, 256, 256), |(c, y, x)| {
        ((c * 100 + y + x) as f32 / 1000.0).min(1.0)
    });
    let style = Array3::from_shape_fn((3, 128, 128), |(c, y, x)| {
        ((c * 50 + y + x) as f32 / 500.0).min(1.0)
    });
    let embedding = engine.extract_style(&style).unwrap();

    c.bench_function("style_transfer_256x256", |b| {
        b.iter(|| engine.transfer(black_box(&content), &embedding, None).unwrap())
    });
}

fn bench_asset_generation(c: &mut Criterion) {
    let mut gen = AssetGenerator::new();
    let params = GenerationParams::new("a beautiful sunset")
        .with_size(ImageSize::Small)
        .with_seed(42);

    c.bench_function("asset_gen_256x256", |b| {
        b.iter(|| gen.generate(black_box(&params)).unwrap())
    });
}

fn bench_inference_engine(c: &mut Criterion) {
    let mut engine = InferenceEngine::new();
    engine.create_session(
        "bench-session",
        vec![("input".into(), vec![1, 105])],
        vec![("output".into(), vec![1, 80])],
    );

    let input = Tensor::zeros("input", &[1, 105]);

    c.bench_function("inference_run_105_to_80", |b| {
        b.iter(|| engine.run("bench-session", &[black_box(input.clone())]).unwrap())
    });
}

fn bench_model_registry(c: &mut Criterion) {
    c.bench_function("model_registry_register_100", |b| {
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
}

fn bench_image_preprocessing(c: &mut Criterion) {
    let bytes = vec![128u8; 256 * 256 * 3];

    c.bench_function("image_from_rgb_256x256", |b| {
        b.iter(|| ImageTensor::from_rgb_bytes(black_box(&bytes), 256, 256).unwrap())
    });

    let img = ImageTensor::from_rgb_bytes(&bytes, 256, 256).unwrap();

    c.bench_function("image_normalize_imagenet_256x256", |b| {
        b.iter(|| black_box(img.normalize_imagenet()))
    });

    c.bench_function("image_to_batch_tensor_256x256", |b| {
        b.iter(|| black_box(img.to_batch_tensor()))
    });
}

fn bench_tokenizer(c: &mut Criterion) {
    let tokenizer = TextTokenizer::new();

    c.bench_function("tokenize_short_prompt", |b| {
        b.iter(|| tokenizer.encode(black_box("a beautiful sunset")).unwrap())
    });

    let long_prompt = "a beautiful modern minimalist design with gradient background and clean typography featuring bold heading text centered layout with warm color palette and subtle shadow effects on a light background";

    c.bench_function("tokenize_long_prompt", |b| {
        b.iter(|| tokenizer.encode(black_box(long_prompt)).unwrap())
    });
}

criterion_group!(
    benches,
    bench_layout_generation,
    bench_style_transfer,
    bench_asset_generation,
    bench_inference_engine,
    bench_model_registry,
    bench_image_preprocessing,
    bench_tokenizer,
);
criterion_main!(benches);
