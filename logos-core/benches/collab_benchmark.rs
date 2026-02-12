use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use logos_core::collab::CollaborationEngine;
use logos_core::{Document, Layer, RectLayer};

fn create_test_layer() -> Layer {
    Layer::Rect(RectLayer::new(10.0, 10.0, 50.0, 50.0))
}

fn bench_delta_generation(c: &mut Criterion) {
    let mut group = c.benchmark_group("CRDT Operations");
    group.throughput(Throughput::Elements(1));
    
    group.bench_function("add_layer_delta", |b| {
        let doc = Document::new();
        // Since CollaborationEngine holds explicit state, we need to create it inside iteration or reset it 
        // to avoid growing indefinitely. However, creating Doc is expensive.
        // We test "add one layer" on a growing doc.
        let mut engine = CollaborationEngine::new(&doc);
        let layer = create_test_layer();
        
        b.iter(|| {
            // This will measure adding a layer to an ever-growing document
            let delta = engine.add_layer_local(black_box(layer.clone())).unwrap();
            black_box(delta);
        })
    });
    
    group.finish();
}

fn bench_apply_remote(c: &mut Criterion) {
    let mut group = c.benchmark_group("CRDT Operations");
    group.throughput(Throughput::Elements(1));
    
    // Prepare a delta to apply
    let doc = Document::new();
    let mut engine_source = CollaborationEngine::new(&doc);
    let layer = create_test_layer();
    let delta = engine_source.add_layer_local(layer).unwrap();
    
    group.bench_function("apply_remote_delta", |b| {
        // We need a fresh engine each time or we are applying same update to same doc (which works, idempotent)
        let doc_dest = Document::new();
        let mut engine_dest = CollaborationEngine::new(&doc_dest);
        
        b.iter(|| {
            engine_dest.apply_remote_update(black_box(&delta)).unwrap();
        })
    });
    
    group.finish();
}

criterion_group!(benches, bench_delta_generation, bench_apply_remote);
criterion_main!(benches);
