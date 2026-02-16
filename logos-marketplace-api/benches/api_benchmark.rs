use criterion::{criterion_group, criterion_main, Criterion};

fn api_benchmark(c: &mut Criterion) {
    c.bench_function("marketplace_server_new", |b| {
        b.iter(|| {
            let _server = logos_marketplace_api::MarketplaceServer::new();
        });
    });
}

criterion_group!(benches, api_benchmark);
criterion_main!(benches);
