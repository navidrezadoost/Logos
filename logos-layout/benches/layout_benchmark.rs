use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
use logos_layout::engine::LayoutEngine;
use logos_layout::bridge::LayoutBridge;
use logos_core::{Layer, RectLayer};
use logos_core::collab::CollabOp;
use taffy::prelude::*;
use uuid::Uuid;

/// Benchmark: add a single leaf node to a fresh engine
fn bench_add_layer(c: &mut Criterion) {
    c.bench_function("add_single_layer", |b| {
        b.iter(|| {
            let mut engine = LayoutEngine::new();
            let layer = Layer::Rect(RectLayer::new(0.0, 0.0, 100.0, 100.0));
            engine.add_or_update_layer(&layer).unwrap();
        })
    });
}

/// Benchmark: build N nodes from scratch (add_or_update_layer)
fn bench_build_tree(c: &mut Criterion) {
    let mut group = c.benchmark_group("build_tree");

    for count in [100, 1_000] {
        group.bench_with_input(
            BenchmarkId::from_parameter(count),
            &count,
            |b, &n| {
                b.iter(|| {
                    let mut engine = LayoutEngine::new();
                    for _ in 0..n {
                        let layer = Layer::Rect(RectLayer::new(0.0, 0.0, 100.0, 50.0));
                        engine.add_or_update_layer(&layer).unwrap();
                    }
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: compute layout for a flex container with N children
/// CTO target: 1000 nodes < 2ms
fn bench_compute_flex_tree(c: &mut Criterion) {
    let mut group = c.benchmark_group("compute_layout_flex_tree");

    for count in [100, 1_000] {
        group.bench_with_input(
            BenchmarkId::from_parameter(count),
            &count,
            |b, &n| {
                let mut engine = LayoutEngine::new();
                let root_id = Uuid::new_v4();
                engine
                    .add_layer(
                        root_id,
                        None,
                        Style {
                            display: Display::Flex,
                            flex_direction: FlexDirection::Column,
                            size: Size {
                                width: Dimension::length(800.0),
                                height: Dimension::auto(),
                            },
                            ..Style::default()
                        },
                    )
                    .unwrap();

                let mut child_ids = Vec::with_capacity(n);
                for _ in 0..n {
                    let child_id = Uuid::new_v4();
                    engine
                        .add_layer(
                            child_id,
                            Some(root_id),
                            LayoutEngine::create_rect_style(100.0, 30.0),
                        )
                        .unwrap();
                    child_ids.push(child_id);
                }

                b.iter(|| {
                    // Re-dirty root to force full recomputation
                    engine
                        .add_layer(
                            root_id,
                            None,
                            Style {
                                display: Display::Flex,
                                flex_direction: FlexDirection::Column,
                                size: Size {
                                    width: Dimension::length(800.0),
                                    height: Dimension::auto(),
                                },
                                ..Style::default()
                            },
                        )
                        .unwrap();
                    engine.compute_layout(root_id).unwrap();
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: nested flex hierarchy (frames containing frames containing rects)
/// Simulates realistic design document: 1 root -> 10 groups -> 100 leaves = 1011 nodes
fn bench_compute_nested_hierarchy(c: &mut Criterion) {
    c.bench_function("compute_nested_hierarchy_1000", |b| {
        let mut engine = LayoutEngine::new();

        let root_id = Uuid::new_v4();
        engine
            .add_layer(
                root_id,
                None,
                LayoutEngine::create_flex_style(FlexDirection::Column, 8.0),
            )
            .unwrap();

        for _ in 0..10 {
            let group_id = Uuid::new_v4();
            engine
                .add_layer(
                    group_id,
                    Some(root_id),
                    LayoutEngine::create_flex_style(FlexDirection::Row, 4.0),
                )
                .unwrap();

            for _ in 0..100 {
                let leaf_id = Uuid::new_v4();
                engine
                    .add_layer(
                        leaf_id,
                        Some(group_id),
                        LayoutEngine::create_rect_style(50.0, 30.0),
                    )
                    .unwrap();
            }
        }

        b.iter(|| {
            // Re-dirty root to force full tree recomputation
            engine
                .add_layer(
                    root_id,
                    None,
                    LayoutEngine::create_flex_style(FlexDirection::Column, 8.0),
                )
                .unwrap();
            engine.compute_layout(root_id).unwrap();
        });
    });
}

/// Benchmark: get_layout after computation (cache read)
fn bench_get_layout_cached(c: &mut Criterion) {
    c.bench_function("get_layout_cached", |b| {
        let mut engine = LayoutEngine::new();
        let layer = Layer::Rect(RectLayer::new(0.0, 0.0, 100.0, 100.0));
        let id = layer.id();
        engine.add_or_update_layer(&layer).unwrap();
        engine.compute_layout(id).unwrap();

        b.iter(|| {
            engine.get_layout(id).unwrap();
        });
    });
}

// ===================================================================
// Bridge benchmarks
// ===================================================================

/// Benchmark: bridge push + flush of a single AddLayer op
fn bench_bridge_single_op(c: &mut Criterion) {
    c.bench_function("bridge_single_add_op", |b| {
        b.iter(|| {
            let mut bridge = LayoutBridge::new();
            let mut engine = LayoutEngine::new();
            let layer = Layer::Rect(RectLayer::new(0.0, 0.0, 100.0, 50.0));
            bridge.push(CollabOp::AddLayer {
                id: layer.id(),
                parent_id: Uuid::nil(),
                index: 0,
                layer,
            });
            bridge.flush(&mut engine).unwrap();
        });
    });
}

/// Benchmark: bridge batch flush of N AddLayer ops
fn bench_bridge_batch_flush(c: &mut Criterion) {
    let mut group = c.benchmark_group("bridge_batch_flush");

    for count in [10, 100, 1_000] {
        group.bench_with_input(
            BenchmarkId::from_parameter(count),
            &count,
            |b, &n| {
                b.iter(|| {
                    let mut bridge = LayoutBridge::new();
                    let mut engine = LayoutEngine::new();
                    let ops: Vec<CollabOp> = (0..n)
                        .map(|_| {
                            let layer = Layer::Rect(RectLayer::new(0.0, 0.0, 50.0, 30.0));
                            CollabOp::AddLayer {
                                id: layer.id(),
                                parent_id: Uuid::nil(),
                                index: 0,
                                layer,
                            }
                        })
                        .collect();
                    bridge.push_batch(ops);
                    bridge.flush(&mut engine).unwrap();
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: end-to-end CollabOp → Layout (add + compute + read)
fn bench_bridge_end_to_end(c: &mut Criterion) {
    c.bench_function("bridge_end_to_end_add_compute_read", |b| {
        b.iter(|| {
            let mut bridge = LayoutBridge::new();
            let mut engine = LayoutEngine::new();

            let layer = Layer::Rect(RectLayer::new(10.0, 20.0, 100.0, 50.0));
            let id = layer.id();
            bridge.push(CollabOp::AddLayer {
                id,
                parent_id: Uuid::nil(),
                index: 0,
                layer,
            });
            bridge.flush(&mut engine).unwrap();
            engine.compute_layout(id).unwrap();
            engine.get_layout(id).unwrap();
        });
    });
}

/// Benchmark: bridge overhead for ModifyProperty (layout-relevant)
fn bench_bridge_modify_property(c: &mut Criterion) {
    c.bench_function("bridge_modify_width", |b| {
        let mut bridge = LayoutBridge::new();
        let mut engine = LayoutEngine::new();

        let layer = Layer::Rect(RectLayer::new(0.0, 0.0, 100.0, 50.0));
        let id = layer.id();
        bridge.push(CollabOp::AddLayer {
            id,
            parent_id: Uuid::nil(),
            index: 0,
            layer,
        });
        bridge.flush(&mut engine).unwrap();

        b.iter(|| {
            bridge.push(CollabOp::ModifyProperty {
                id,
                property: "width".to_string(),
                value: serde_json::json!(200.0),
            });
            bridge.flush(&mut engine).unwrap();
        });
    });
}

criterion_group!(
    benches,
    bench_add_layer,
    bench_build_tree,
    bench_compute_flex_tree,
    bench_compute_nested_hierarchy,
    bench_get_layout_cached,
    bench_bridge_single_op,
    bench_bridge_batch_flush,
    bench_bridge_end_to_end,
    bench_bridge_modify_property,
);
criterion_main!(benches);
