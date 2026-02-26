use criterion::{criterion_group, criterion_main, Criterion};

fn perf_benchmarks(_c: &mut Criterion) {
    // Benchmark stubs — will be populated with pool/buffer/cache benchmarks.
}

criterion_group!(benches, perf_benchmarks);
criterion_main!(benches);
