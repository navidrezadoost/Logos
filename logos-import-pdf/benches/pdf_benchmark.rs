use criterion::{criterion_group, criterion_main, Criterion};

fn pdf_import_benchmark(c: &mut Criterion) {
    use logos_import_pdf::content::PdfElement;
    use logos_import_pdf::parser::build_test_pdf;

    let data = build_test_pdf(
        &[
            PdfElement::Rect { x: 10.0, y: 10.0, width: 100.0, height: 50.0 },
            PdfElement::Text { content: "Hello".into(), x: 72.0, y: 720.0, font_size: 12.0 },
            PdfElement::Rect { x: 200.0, y: 200.0, width: 150.0, height: 80.0 },
        ],
        612.0,
        792.0,
    );

    c.bench_function("pdf_import", |b| {
        b.iter(|| logos_import_pdf::import_pdf(&data).unwrap())
    });
}

criterion_group!(benches, pdf_import_benchmark);
criterion_main!(benches);
