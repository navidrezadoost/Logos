use criterion::{criterion_group, criterion_main, Criterion};

fn sketch_import_benchmark(c: &mut Criterion) {
    use logos_import_sketch::archive::build_test_sketch;
    use logos_import_sketch::model::SketchLayer;

    let data = build_test_sketch(&[
        SketchLayer::rect("1", "BG", 0.0, 0.0, 375.0, 812.0),
        SketchLayer::text("2", "Title", 20.0, 40.0, 300.0, 30.0, "Hello"),
        SketchLayer::oval("3", "Dot", 100.0, 200.0, 50.0, 50.0),
    ]);

    c.bench_function("sketch_import", |b| {
        b.iter(|| logos_import_sketch::import_sketch(&data).unwrap())
    });
}

criterion_group!(benches, sketch_import_benchmark);
criterion_main!(benches);
