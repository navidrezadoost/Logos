//! Benchmarks for logos-render instance-buffer generation and GPU uploads.

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use logos_render::vertex::{RectInstance, CameraUniform, TextInstance};
use logos_render::bridge::{
    collect_instances, collect_instances_direct, collect_instances_direct_into,
    collect_instances_into, collect_instances_fast, prepare_layer_data,
};
use logos_core::{Layer, RectLayer};
use logos_core::collab::CollabOp;
use logos_layout::engine::LayoutEngine;
use logos_layout::bridge::LayoutBridge;
use uuid::Uuid;

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

fn bench_collect_instances_reuse(c: &mut Criterion) {
    let mut group = c.benchmark_group("collect_instances_reuse");
    for &count in &[100, 1_000, 10_000] {
        let rects = make_rects(count);
        let mut buf = Vec::with_capacity(count);
        group.bench_with_input(
            BenchmarkId::from_parameter(count),
            &rects,
            |b, rects| {
                b.iter(|| {
                    collect_instances_direct_into(black_box(rects), &mut buf);
                    black_box(&buf);
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

fn bench_text_instance_creation(c: &mut Criterion) {
    c.bench_function("TextInstance::new", |b| {
        b.iter(|| {
            black_box(TextInstance::new(
                black_box(10.0),
                black_box(20.0),
                black_box(8.0),
                black_box(12.0),
                black_box([0.0, 0.0]),
                black_box([0.5, 0.5]),
                black_box([1.0, 1.0, 1.0, 1.0]),
            ));
        });
    });
}

fn bench_text_instance_batch(c: &mut Criterion) {
    c.bench_function("TextInstance_batch_1000", |b| {
        b.iter(|| {
            let mut v = Vec::with_capacity(1000);
            for i in 0..1000u32 {
                let fi = i as f32;
                v.push(TextInstance::new(
                    fi * 8.0, 100.0,
                    8.0, 12.0,
                    [fi * 0.001, 0.0], [fi * 0.001 + 0.01, 0.02],
                    [1.0, 1.0, 1.0, 1.0],
                ));
            }
            black_box(&v);
        });
    });
}

// ===================================================================
// End-to-end pipeline benchmarks (CPU-side: CRDT → Layout → Collect)
// ===================================================================

/// Helper: build a scene with N layers, compute layout, return (engine, layers).
fn setup_scene(n: usize) -> (LayoutEngine, Vec<(Uuid, Layer)>) {
    let mut engine = LayoutEngine::new();
    let layers: Vec<(Uuid, Layer)> = (0..n)
        .map(|i| {
            let fi = i as f32;
            let layer = Layer::Rect(RectLayer::new(
                (fi * 7.3) % 1920.0,
                (fi * 13.7) % 1080.0,
                50.0 + (fi * 3.1) % 200.0,
                30.0 + (fi * 5.7) % 150.0,
            ));
            let id = layer.id();
            engine.add_or_update_layer(&layer).unwrap();
            (id, layer)
        })
        .collect();
    // Compute layout for each root (they're independent absolute layers)
    for &(id, _) in &layers {
        engine.compute_layout(id).unwrap();
    }
    (engine, layers)
}

/// End-to-end: layout compute → collect_instances_into → bytemuck cast
/// This is the per-frame CPU path for a steady-state scene (no changes).
fn bench_pipeline_steady_state(c: &mut Criterion) {
    let mut group = c.benchmark_group("pipeline_steady_state");

    for &count in &[100, 1_000] {
        let (engine, layers) = setup_scene(count);
        let layer_refs: Vec<(Uuid, &Layer)> = layers.iter().map(|(id, l)| (*id, l)).collect();
        let mut buf: Vec<RectInstance> = Vec::with_capacity(count);

        group.bench_with_input(
            BenchmarkId::from_parameter(count),
            &count,
            |b, _| {
                b.iter(|| {
                    collect_instances_into(&engine, black_box(&layer_refs), &mut buf);
                    let bytes: &[u8] = bytemuck::cast_slice(&buf);
                    black_box(bytes.len());
                });
            },
        );
    }

    group.finish();
}

/// End-to-end: single layer CRDT add → bridge flush → layout compute → collect
/// This is the modification path (user adds a shape).
fn bench_pipeline_add_layer(c: &mut Criterion) {
    c.bench_function("pipeline_add_compute_collect", |b| {
        b.iter(|| {
            let mut bridge = LayoutBridge::new();
            let mut engine = LayoutEngine::new();

            let layer = Layer::Rect(RectLayer::new(10.0, 20.0, 100.0, 50.0));
            let id = layer.id();

            bridge.push(CollabOp::AddLayer {
                id,
                parent_id: Uuid::nil(),
                index: 0,
                layer: layer.clone(),
            });
            bridge.flush(&mut engine).unwrap();
            engine.compute_layout(id).unwrap();

            let layers = vec![(id, &layer)];
            let instances = collect_instances(&engine, black_box(&layers));
            let bytes: &[u8] = bytemuck::cast_slice(&instances);
            black_box(bytes.len());
        });
    });
}

/// End-to-end: modify property → recompute → collect (incremental path).
/// Scene has 100 layers, we modify one and re-collect all.
fn bench_pipeline_modify_recompute(c: &mut Criterion) {
    let mut group = c.benchmark_group("pipeline_modify_recompute");

    for &count in &[100, 1_000] {
        let (mut engine, layers) = setup_scene(count);
        let layer_refs: Vec<(Uuid, &Layer)> = layers.iter().map(|(id, l)| (*id, l)).collect();
        let mut buf: Vec<RectInstance> = Vec::with_capacity(count);
        // Pick a root to compute under — since all are independent absolute,
        // use the first one. But compute_layout needs dirty to work, so we
        // mark one dirty via update_dimension.
        let target_id = layers[0].0;

        group.bench_with_input(
            BenchmarkId::from_parameter(count),
            &count,
            |b, _| {
                b.iter(|| {
                    // 1. Modify one layer's width
                    engine
                        .update_dimension(
                            target_id,
                            logos_layout::bridge::DimAxis::Width,
                            black_box(120.0),
                        )
                        .unwrap();
                    // 2. Recompute layout (only dirty node recomputed by Taffy)
                    engine.compute_layout(target_id).unwrap();
                    // 3. Collect all instances
                    collect_instances_into(&engine, black_box(&layer_refs), &mut buf);
                    black_box(buf.len());
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: collect_instances from layout engine (not the direct variant)
fn bench_collect_instances_from_engine(c: &mut Criterion) {
    let mut group = c.benchmark_group("collect_from_engine");

    for &count in &[100, 1_000] {
        let (engine, layers) = setup_scene(count);
        let layer_refs: Vec<(Uuid, &Layer)> = layers.iter().map(|(id, l)| (*id, l)).collect();
        let mut buf: Vec<RectInstance> = Vec::with_capacity(count);

        group.bench_with_input(
            BenchmarkId::from_parameter(count),
            &count,
            |b, _| {
                b.iter(|| {
                    collect_instances_into(&engine, black_box(&layer_refs), &mut buf);
                    black_box(buf.len());
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: fast-path collect using pre-computed colors + batch layout lookup
fn bench_collect_instances_fast(c: &mut Criterion) {
    let mut group = c.benchmark_group("collect_fast");

    for &count in &[100, 1_000] {
        let (engine, layers) = setup_scene(count);
        let layer_refs: Vec<&Layer> = layers.iter().map(|(_, l)| l).collect();
        let (ids, colors) = prepare_layer_data(&layer_refs);
        let mut buf: Vec<RectInstance> = Vec::with_capacity(count);

        group.bench_with_input(
            BenchmarkId::from_parameter(count),
            &count,
            |b, _| {
                b.iter(|| {
                    collect_instances_fast(&engine, black_box(&ids), &colors, &mut buf);
                    black_box(buf.len());
                });
            },
        );
    }

    group.finish();
}

/// End-to-end: modify → recompute → collect_fast (optimized pipeline)
fn bench_pipeline_modify_recompute_fast(c: &mut Criterion) {
    let mut group = c.benchmark_group("pipeline_modify_fast");

    for &count in &[100, 1_000] {
        let (mut engine, layers) = setup_scene(count);
        let layer_refs: Vec<&Layer> = layers.iter().map(|(_, l)| l).collect();
        let (ids, colors) = prepare_layer_data(&layer_refs);
        let mut buf: Vec<RectInstance> = Vec::with_capacity(count);
        let target_id = layers[0].0;

        group.bench_with_input(
            BenchmarkId::from_parameter(count),
            &count,
            |b, _| {
                b.iter(|| {
                    engine
                        .update_dimension(
                            target_id,
                            logos_layout::bridge::DimAxis::Width,
                            black_box(120.0),
                        )
                        .unwrap();
                    engine.compute_layout(target_id).unwrap();
                    collect_instances_fast(&engine, black_box(&ids), &colors, &mut buf);
                    black_box(buf.len());
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_collect_instances,
    bench_collect_instances_reuse,
    bench_instance_creation,
    bench_instance_with_radius,
    bench_camera_orthographic,
    bench_bytemuck_cast,
    bench_text_instance_creation,
    bench_text_instance_batch,
    bench_pipeline_steady_state,
    bench_pipeline_add_layer,
    bench_pipeline_modify_recompute,
    bench_collect_instances_from_engine,
    bench_collect_instances_fast,
    bench_pipeline_modify_recompute_fast,
);
criterion_main!(benches);
