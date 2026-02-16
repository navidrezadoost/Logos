use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use logos_import_figma::{
    FigmaParser, FigmaConverter,
    fixtures::{TestFixture, generate_test_fig},
};

fn bench_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("figma_parse");

    let fixtures = [
        ("minimal", TestFixture::Minimal),
        ("single_rect", TestFixture::SingleRectangle),
        ("basic_shapes", TestFixture::BasicShapes),
        ("nested_frames", TestFixture::NestedFrames),
        ("mobile_app", TestFixture::MobileAppScreen),
        ("styled_shapes", TestFixture::StyledShapes),
        ("components", TestFixture::ComponentsAndInstances),
        ("large_doc_100", TestFixture::LargeDocument),
    ];

    for (name, fixture) in &fixtures {
        let data = generate_test_fig(*fixture);
        group.bench_with_input(
            BenchmarkId::new("parse", name),
            &data,
            |b, data| {
                b.iter(|| {
                    let mut parser = FigmaParser::new();
                    black_box(parser.parse(data).unwrap())
                })
            },
        );
    }

    group.finish();
}

fn bench_convert(c: &mut Criterion) {
    let mut group = c.benchmark_group("figma_convert");

    let fixtures = [
        ("basic_shapes", TestFixture::BasicShapes),
        ("mobile_app", TestFixture::MobileAppScreen),
        ("styled_shapes", TestFixture::StyledShapes),
        ("large_doc_100", TestFixture::LargeDocument),
    ];

    for (name, fixture) in &fixtures {
        let data = generate_test_fig(*fixture);
        let mut parser = FigmaParser::new();
        let node = parser.parse(&data).unwrap();

        group.bench_with_input(
            BenchmarkId::new("convert", name),
            &node,
            |b, node| {
                b.iter(|| {
                    let converter = FigmaConverter::new();
                    black_box(converter.convert(node).unwrap())
                })
            },
        );
    }

    group.finish();
}

fn bench_end_to_end(c: &mut Criterion) {
    let mut group = c.benchmark_group("figma_e2e");

    let fixtures = [
        ("basic_shapes", TestFixture::BasicShapes),
        ("mobile_app", TestFixture::MobileAppScreen),
        ("large_doc_100", TestFixture::LargeDocument),
    ];

    for (name, fixture) in &fixtures {
        let data = generate_test_fig(*fixture);
        group.bench_with_input(
            BenchmarkId::new("import", name),
            &data,
            |b, data| {
                b.iter(|| {
                    black_box(logos_import_figma::import_figma(data).unwrap())
                })
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_parse, bench_convert, bench_end_to_end);
criterion_main!(benches);
