use criterion::{criterion_group, criterion_main, Criterion};

fn svg_import_benchmark(c: &mut Criterion) {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">
        <rect x="10" y="10" width="80" height="60"/>
        <circle cx="150" cy="50" r="30"/>
        <text x="50" y="150">Hello</text>
    </svg>"#;
    let data = svg.as_bytes();

    c.bench_function("svg_import", |b| {
        b.iter(|| logos_import_svg::import_svg(data).unwrap())
    });
}

criterion_group!(benches, svg_import_benchmark);
criterion_main!(benches);
