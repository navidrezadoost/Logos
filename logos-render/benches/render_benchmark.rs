//! Benchmarks for logos-render instance-buffer generation and GPU uploads.

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use logos_render::vertex::{RectInstance, CameraUniform};
use logos_render::bridge::collect_instances_direct;

/// Generate `n` random-ish rect descriptors.
fn make_rects(n: usize) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
    (0..n)
        .map(|i| {
            let fi = i as f32;
            (
                (fi * 7.3) % 1920.0,
                (fi * 13.7) % 1080.0,
                50.0 + (fi * 3.1) % 200.0,
                30.0 + (fi * 5.7) % 150.0,
                [
                    (fi * 0.17) % 1.0,
                    (fi * 0.31) % 1.0,
                    (fi * 0.53) % 1.0,
                    1.0,
                ],
            )
        })
        .collect()
}

fn bench_collect_instances(c: &mut Criterion) {
    let mut group = c.benchmark_group("collect_instances");
    for &count in &[100, 1_000, 10_000] {
        let rects = make_rects(count);
        group.bench_with_input(
            BenchmarkId::from_parameter(count),
            &rects,
            |b, rects| {
                b.iter(|| {
                    black_box(collect_instances_direct(black_box(rects)));
                });
            },
        );
    }
    group.finish();
}

fn bench_instance_creation(c: &mut Criterion) {
    c.bench_function("RectInstance::new", |b| {
        b.iter(|| {
            black_box(RectInstance::new(
                black_box(100.0),
                black_box(200.0),
                black_box(300.0),
                black_box(150.0),
                black_box([1.0, 0.0, 0.0, 1.0]),
            ));
        });
    });
}

fn bench_instance_with_radius(c: &mut Criterion) {
    c.bench_function("RectInstance::with_radius", |b| {
        let inst = RectInstance::new(0.0, 0.0, 100.0, 50.0, [1.0; 4]);
        b.iter(|| {
            black_box(black_box(inst).with_radius(black_box(8.0)));
        });
    });
}

fn bench_camera_orthographic(c: &mut Criterion) {
    c.bench_function("CameraUniform::orthographic", |b| {
        b.iter(|| {
            black_box(CameraUniform::orthographic(
                black_box(1920.0),
                black_box(1080.0),
                black_box(100.0),
                black_box(50.0),
                black_box(1.5),
            ));
        });
    });
}

fn bench_bytemuck_cast(c: &mut Criterion) {
    let instances: Vec<RectInstance> = make_rects(1_000)
        .iter()
        .map(|&(x, y, w, h, c)| RectInstance::new(x, y, w, h, c))
        .collect();

    c.bench_function("bytemuck_cast_1k_instances", |b| {
        b.iter(|| {
            let bytes: &[u8] = bytemuck::cast_slice(black_box(&instances));
            black_box(bytes.len());
        });
    });
}

criterion_group!(
    benches,
    bench_collect_instances,
    bench_instance_creation,
    bench_instance_with_radius,
    bench_camera_orthographic,
    bench_bytemuck_cast,
);
criterion_main!(benches);
