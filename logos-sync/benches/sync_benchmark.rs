use criterion::{criterion_group, criterion_main, Criterion};

fn sync_benchmarks(_c: &mut Criterion) {
    // TODO: add benchmarks
}

criterion_group!(benches, sync_benchmarks);
criterion_main!(benches);
