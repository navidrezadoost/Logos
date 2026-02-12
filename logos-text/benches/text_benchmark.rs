use criterion::{black_box, criterion_group, criterion_main, Criterion};
use logos_text::{Atlas, FontDescriptor, FontRegistry, FontStyle, TextEngine, TextStyle};

fn bench_shape_short_text(c: &mut Criterion) {
    let mut engine = TextEngine::new();
    let mut atlas = Atlas::new(1024);
    let style = TextStyle {
        font_size: 16.0,
        line_height: 20.0,
        ..Default::default()
    };

    c.bench_function("shape_short_text", |b| {
        b.iter(|| {
            engine.shape_text(
                black_box("Hello, Logos!"),
                black_box(&style),
                f32::INFINITY,
                &mut atlas,
            )
        });
    });
}

fn bench_shape_paragraph(c: &mut Criterion) {
    let mut engine = TextEngine::new();
    let mut atlas = Atlas::new(2048);
    let style = TextStyle {
        font_size: 14.0,
        line_height: 18.0,
        ..Default::default()
    };

    let paragraph = "The quick brown fox jumps over the lazy dog. \
        Lorem ipsum dolor sit amet, consectetur adipiscing elit. \
        Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.";

    c.bench_function("shape_paragraph", |b| {
        b.iter(|| {
            engine.shape_text(
                black_box(paragraph),
                black_box(&style),
                400.0,
                &mut atlas,
            )
        });
    });
}

fn bench_atlas_insert(c: &mut Criterion) {
    let bitmap = vec![200u8; 16 * 16]; // alpha-only 16x16 glyph

    c.bench_function("atlas_insert_16x16", |b| {
        let mut atlas = Atlas::new(1024);
        let mut id = 0u16;
        b.iter(|| {
            id = id.wrapping_add(1);
            atlas.insert(black_box(id), 16, 16, black_box(&bitmap));
        });
    });
}

fn bench_atlas_lookup(c: &mut Criterion) {
    let mut atlas = Atlas::new(1024);
    // Pre-populate atlas.
    for id in 0..100u16 {
        let bitmap = vec![128u8; 12 * 12];
        atlas.insert(id, 12, 12, &bitmap);
    }

    c.bench_function("atlas_lookup", |b| {
        let mut id = 0u16;
        b.iter(|| {
            id = (id + 1) % 100;
            atlas.get(black_box(id));
        });
    });
}

fn bench_font_match(c: &mut Criterion) {
    let registry = FontRegistry::discover();
    let desc = FontDescriptor::from_css("Arial, Helvetica, sans-serif", 400, FontStyle::Normal);

    c.bench_function("font_match", |b| {
        b.iter(|| {
            registry.match_font(black_box(&desc))
        });
    });
}

fn bench_font_match_bold_italic(c: &mut Criterion) {
    let registry = FontRegistry::discover();
    let desc = FontDescriptor::from_css("sans-serif", 700, FontStyle::Italic);

    c.bench_function("font_match_bold_italic", |b| {
        b.iter(|| {
            registry.match_font(black_box(&desc))
        });
    });
}

fn bench_font_match_fallback(c: &mut Criterion) {
    let registry = FontRegistry::discover();
    // Non-existent font → walks entire fallback chain.
    let desc = FontDescriptor::from_css("NonExistent, AlsoMissing, sans-serif", 400, FontStyle::Normal);

    c.bench_function("font_match_fallback_chain", |b| {
        b.iter(|| {
            registry.match_font(black_box(&desc))
        });
    });
}

fn bench_style_to_descriptor(c: &mut Criterion) {
    let style = TextStyle {
        family: "Arial, Helvetica, sans-serif".into(),
        weight: 700,
        italic: true,
        ..Default::default()
    };

    c.bench_function("style_to_descriptor", |b| {
        b.iter(|| {
            black_box(&style).to_descriptor()
        });
    });
}

fn bench_shape_serif(c: &mut Criterion) {
    let mut engine = TextEngine::new();
    let mut atlas = Atlas::new(1024);
    let style = TextStyle {
        font_size: 20.0,
        line_height: 24.0,
        family: "serif".into(),
        ..Default::default()
    };

    c.bench_function("shape_serif_text", |b| {
        b.iter(|| {
            engine.shape_text(
                black_box("Serif text"),
                black_box(&style),
                f32::INFINITY,
                &mut atlas,
            )
        });
    });
}

fn bench_shape_bold_italic(c: &mut Criterion) {
    let mut engine = TextEngine::new();
    let mut atlas = Atlas::new(1024);
    let style = TextStyle {
        font_size: 20.0,
        line_height: 24.0,
        weight: 700,
        italic: true,
        ..Default::default()
    };

    c.bench_function("shape_bold_italic", |b| {
        b.iter(|| {
            engine.shape_text(
                black_box("Bold Italic"),
                black_box(&style),
                f32::INFINITY,
                &mut atlas,
            )
        });
    });
}

criterion_group!(
    benches,
    bench_shape_short_text,
    bench_shape_paragraph,
    bench_atlas_insert,
    bench_atlas_lookup,
    bench_font_match,
    bench_font_match_bold_italic,
    bench_font_match_fallback,
    bench_style_to_descriptor,
    bench_shape_serif,
    bench_shape_bold_italic,
);
criterion_main!(benches);
