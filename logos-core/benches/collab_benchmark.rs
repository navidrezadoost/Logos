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
        let mut engine = CollaborationEngine::new(&doc);
        let layer = create_test_layer();
        
        b.iter(|| {
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
        let doc_dest = Document::new();
        let mut engine_dest = CollaborationEngine::new(&doc_dest);
        
        b.iter(|| {
            engine_dest.apply_remote_update(black_box(&delta)).unwrap();
        })
    });
    
    group.finish();
}

fn bench_serialization_only(c: &mut Criterion) {
    let mut group = c.benchmark_group("Serialization");
    group.throughput(Throughput::Elements(1));
    
    // Measure bincode serialization in isolation
    group.bench_function("bincode_layer_serialize", |b| {
        let layer = create_test_layer();
        let mut buf = Vec::with_capacity(256);
        
        b.iter(|| {
            buf.clear();
            bincode::serialize_into(&mut buf, black_box(&layer)).unwrap();
            black_box(&buf);
        })
    });
    
    // Measure JSON serialization for comparison
    group.bench_function("json_layer_serialize", |b| {
        let layer = create_test_layer();
        
        b.iter(|| {
            let json = serde_json::to_string(black_box(&layer)).unwrap();
            black_box(json);
        })
    });
    
    // Measure UUID to_string in isolation
    group.bench_function("uuid_to_string", |b| {
        let id = uuid::Uuid::new_v4();
        
        b.iter(|| {
            let s = black_box(id).to_string();
            black_box(s);
        })
    });
    
    // Measure stack-allocated UUID formatting (zero-alloc)
    group.bench_function("uuid_stack_format", |b| {
        let id = uuid::Uuid::new_v4();
        
        b.iter(|| {
            let mut buf = [0u8; uuid::fmt::Hyphenated::LENGTH];
            let s = black_box(id).hyphenated().encode_lower(&mut buf);
            black_box(s);
        })
    });
    
    group.finish();
}

fn bench_large_document(c: &mut Criterion) {
    let mut group = c.benchmark_group("Large Document");
    group.throughput(Throughput::Elements(1));
    
    // Pre-populate document with 1000 layers, then measure add cost
    group.bench_function("add_layer_1k_existing", |b| {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        
        // Add 1000 layers
        for _ in 0..1000 {
            let layer = create_test_layer();
            engine.add_layer_local(layer).unwrap();
        }
        
        let layer = create_test_layer();
        b.iter(|| {
            let delta = engine.add_layer_local(black_box(layer.clone())).unwrap();
            black_box(delta);
        })
    });
    
    group.finish();
}

criterion_group!(benches, bench_delta_generation, bench_apply_remote, bench_serialization_only, bench_large_document);
criterion_main!(benches);
