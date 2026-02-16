use criterion::{criterion_group, criterion_main, Criterion};

fn canva_import_benchmark(c: &mut Criterion) {
    use logos_import_canva::model::*;

    let doc = CanvaDocument::new("Bench", 800.0, 600.0, vec![
        CanvaElement::rect("bg", 0.0, 0.0, 800.0, 600.0),
        CanvaElement::text("title", 100.0, 50.0, 400.0, 40.0, "Title"),
        CanvaElement::ellipse("dot", 350.0, 300.0, 100.0, 100.0),
    ]);
    let data = serde_json::to_vec(&doc).unwrap();

    c.bench_function("canva_import", |b| {
        b.iter(|| logos_import_canva::import_canva(&data).unwrap())
    });
}

criterion_group!(benches, canva_import_benchmark);
criterion_main!(benches);
