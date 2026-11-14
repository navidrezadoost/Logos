//! Reproducible candidate-core benchmark.
//! Run: cargo bench --manifest-path rust/Cargo.toml -p logos-types --bench document_core
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use logos_types::{
    apply_operation, validate_document, Document, DocumentOperation, Page, Shape, ShapeRecord,
    ShapeType, Uuid, DOCUMENT_SCHEMA_VERSION,
};
use sha2::{Digest, Sha256};

struct CountingAllocator;
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static ALLOCATED_BYTES: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        ALLOCATED_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        System.alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }
    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        ALLOCATED_BYTES.fetch_add(new_size, Ordering::Relaxed);
        System.realloc(ptr, old, new_size)
    }
}
#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

fn id(n: u128) -> Uuid {
    Uuid::from_u128(n)
}
fn rec(shape: Shape, page_id: Uuid) -> ShapeRecord {
    ShapeRecord {
        page_id,
        shape,
        extensions: Default::default(),
    }
}
fn flat_document(size: usize) -> Document {
    let page_id = id(1);
    let mut roots = Vec::with_capacity(size + 2);
    let mut shapes = Vec::with_capacity(size + 2);
    for (frame_id, name) in [(id(2), "A"), (id(3), "B")] {
        roots.push(frame_id);
        shapes.push(rec(
            Shape::frame(frame_id, name, 0.0, 0.0, 100.0, 100.0),
            page_id,
        ));
    }
    for n in 0..size {
        let shape_id = id(10 + n as u128);
        roots.push(shape_id);
        shapes.push(rec(Shape::rect(shape_id, n as f64, 0.0, 1.0, 1.0), page_id));
    }
    Document {
        id: id(9),
        schema_version: DOCUMENT_SCHEMA_VERSION,
        pages: vec![Page {
            id: page_id,
            name: "Bench".into(),
            root_shape_ids: roots,
            extensions: Default::default(),
        }],
        shapes,
        extensions: Default::default(),
    }
}
fn deep_document(depth: usize) -> Document {
    let page_id = id(1);
    let root_id = id(2);
    let mut root = Shape::new(root_id, ShapeType::Component, "root");
    let mut shapes: Vec<ShapeRecord> = vec![];
    let mut parent_id = root_id;
    for n in 0..depth {
        let shape_id = id(10 + n as u128);
        let kind = if n + 1 == depth {
            ShapeType::Rect
        } else {
            ShapeType::Group
        };
        let mut shape = Shape::new(shape_id, kind, "node");
        shape.parent_id = Some(parent_id);
        if n == 0 {
            root.shapes.push(shape_id);
        }
        if let Some(parent) = shapes.last_mut() {
            parent.shape.shapes.push(shape_id);
        }
        shapes.push(rec(shape, page_id));
        parent_id = shape_id;
    }
    shapes.insert(0, rec(root, page_id));
    Document {
        id: id(9),
        schema_version: DOCUMENT_SCHEMA_VERSION,
        pages: vec![Page {
            id: page_id,
            name: "Deep".into(),
            root_shape_ids: vec![root_id],
            extensions: Default::default(),
        }],
        shapes,
        extensions: Default::default(),
    }
}
fn percentile(samples: &mut [u128], percentile: usize) -> u128 {
    samples.sort_unstable();
    samples[((samples.len() - 1) * percentile).div_ceil(100)]
}
fn bench(label: &str, objects: usize, mut f: impl FnMut()) {
    f();
    let mut nanos = Vec::with_capacity(31);
    for _ in 0..31 {
        let start = Instant::now();
        black_box(f());
        nanos.push(start.elapsed().as_nanos());
    }
    ALLOCATIONS.store(0, Ordering::SeqCst);
    ALLOCATED_BYTES.store(0, Ordering::SeqCst);
    f();
    let allocations = ALLOCATIONS.load(Ordering::SeqCst);
    let bytes = ALLOCATED_BYTES.load(Ordering::SeqCst);
    let p50 = percentile(&mut nanos, 50);
    let p95 = percentile(&mut nanos, 95);
    println!("{{\"case\":\"{label}\",\"objects\":{objects},\"samples\":31,\"p50_ns\":{p50},\"p95_ns\":{p95},\"requested_allocations_per_call\":{allocations},\"requested_bytes_per_call\":{bytes},\"retained_memory_measured\":false}}");
}
fn main() {
    for size in [100, 1_000, 10_000] {
        let doc = flat_document(size);
        let serialized = serde_json::to_vec(&doc).expect("fixture serializes");
        let digest = Sha256::digest(&serialized);
        println!(
            "{{\"fixture\":\"g1-b2-v1-flat-{size}\",\"sha256_serialized_json\":\"{}\",\"objects\":{}}}",
            hex(&digest),
            doc.shapes.len()
        );
        bench(&format!("validate-flat-{size}"), doc.shapes.len(), || {
            black_box(validate_document(black_box(&doc)));
        });
        let source = doc.clone();
        let move_to_frame = DocumentOperation::ReparentShape {
            shape_id: id(10),
            expected_parent_id: None,
            new_parent_id: Some(id(2)),
            new_page_id: id(1),
            index: 0,
        };
        bench(
            &format!("reparent-cycle-flat-{size}"),
            doc.shapes.len(),
            || {
                let moved = apply_operation(black_box(&source), black_box(&move_to_frame)).unwrap();
                let move_back = DocumentOperation::ReparentShape {
                    shape_id: id(10),
                    expected_parent_id: Some(id(2)),
                    new_parent_id: None,
                    new_page_id: id(1),
                    index: 2,
                };
                black_box(apply_operation(black_box(&moved), black_box(&move_back)).unwrap());
            },
        );
        let create = DocumentOperation::CreateShape {
            record: rec(Shape::rect(id(900_000), 1.0, 1.0, 2.0, 2.0), id(1)),
            parent_id: None,
            index: 2,
        };
        bench(
            &format!("create-delete-cycle-flat-{size}"),
            doc.shapes.len(),
            || {
                let created = apply_operation(black_box(&source), black_box(&create)).unwrap();
                let delete = DocumentOperation::DeleteSubtree {
                    shape_id: id(900_000),
                    expected_parent_id: None,
                };
                black_box(apply_operation(black_box(&created), black_box(&delete)).unwrap());
            },
        );
        let malformed = {
            let mut d = doc.clone();
            d.pages[0].root_shape_ids.push(id(999_999));
            d
        };
        bench(
            &format!("validate-malformed-flat-{size}"),
            malformed.shapes.len(),
            || {
                black_box(validate_document(black_box(&malformed)));
            },
        );
    }
    for depth in [512, 5_000] {
        let doc = deep_document(depth);
        let serialized = serde_json::to_vec(&doc).expect("fixture serializes");
        let digest = Sha256::digest(&serialized);
        println!(
            "{{\"fixture\":\"g1-b2-v1-deep-{depth}\",\"sha256_serialized_json\":\"{}\",\"objects\":{}}}",
            hex(&digest),
            doc.shapes.len()
        );
        bench(&format!("validate-deep-{depth}"), doc.shapes.len(), || {
            black_box(validate_document(black_box(&doc)));
        });
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
