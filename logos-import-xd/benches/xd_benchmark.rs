use criterion::{criterion_group, criterion_main, Criterion};

fn xd_import_benchmark(c: &mut Criterion) {
    use logos_import_xd::archive::build_test_xd;
    use logos_import_xd::model::*;

    let data = build_test_xd(
        &[
            XdNode::rect("bg", 0.0, 0.0, 375.0, 812.0),
            XdNode::text("title", 20.0, 40.0, 300.0, 30.0, "Hello"),
            XdNode::ellipse("dot", 100.0, 200.0, 50.0, 50.0),
        ],
        "Bench Screen",
    );

    c.bench_function("xd_import", |b| {
        b.iter(|| logos_import_xd::import_xd(&data).unwrap())
    });
}

criterion_group!(benches, xd_import_benchmark);
criterion_main!(benches);
