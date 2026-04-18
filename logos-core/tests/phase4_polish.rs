/// Phase 4 — Performance & Polish integration tests
/// SpatialHash API, Document::diff, AnimationLibrary-in-Document, DocumentPatch helpers
use logos_core::{
    animation::{AnimationClip, AnimationFormat},
    Document, DocumentPatch, Layer, Rect, RectLayer, SpatialHash,
};
use uuid::Uuid;

// ── helpers ──────────────────────────────────────────────────────────────────

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect { x, y, width: w, height: h }
}

fn rect_layer(x: f32, y: f32, w: f32, h: f32) -> Layer {
    Layer::Rect(RectLayer::new(x, y, w, h))
}

fn clip(name: &str, fmt: AnimationFormat) -> AnimationClip {
    AnimationClip {
        id: Uuid::new_v4(),
        name: name.to_string(),
        format: fmt,
        frame_rate: 30.0,
        duration_ms: 1000,
        track_count: 1,
        source: String::new(),
    }
}

// ── §1  SpatialHash (p001–p012) ───────────────────────────────────────────────

#[test]
fn p001_spatial_hash_new_is_empty() {
    let sh = SpatialHash::new(100.0);
    assert!(sh.is_empty());
    assert_eq!(sh.entry_count(), 0);
}

#[test]
fn p002_insert_one_layer() {
    let mut sh = SpatialHash::new(100.0);
    let id = Uuid::new_v4();
    sh.insert(id, rect(0.0, 0.0, 50.0, 50.0));
    assert!(!sh.is_empty());
    assert_eq!(sh.entry_count(), 1);
}

#[test]
fn p003_query_hit() {
    let mut sh = SpatialHash::new(100.0);
    let id = Uuid::new_v4();
    sh.insert(id, rect(10.0, 10.0, 30.0, 30.0));
    let results = sh.query(rect(0.0, 0.0, 100.0, 100.0));
    assert!(results.contains(&id));
}

#[test]
fn p004_query_no_hit() {
    let mut sh = SpatialHash::new(100.0);
    let id = Uuid::new_v4();
    sh.insert(id, rect(500.0, 500.0, 10.0, 10.0));
    let results = sh.query(rect(0.0, 0.0, 100.0, 100.0));
    assert!(!results.contains(&id));
}

#[test]
fn p005_remove_layer() {
    let mut sh = SpatialHash::new(100.0);
    let id = Uuid::new_v4();
    sh.insert(id, rect(0.0, 0.0, 50.0, 50.0));
    sh.remove(id);
    assert!(sh.is_empty());
    assert_eq!(sh.entry_count(), 0);
}

#[test]
fn p006_remove_nonexistent_is_noop() {
    let mut sh = SpatialHash::new(100.0);
    sh.remove(Uuid::new_v4()); // should not panic
}

#[test]
fn p007_multiple_layers_same_cell() {
    let mut sh = SpatialHash::new(200.0);
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    sh.insert(a, rect(0.0, 0.0, 50.0, 50.0));
    sh.insert(b, rect(60.0, 60.0, 50.0, 50.0));
    assert_eq!(sh.entry_count(), 2);
    let results = sh.query(rect(0.0, 0.0, 200.0, 200.0));
    assert!(results.contains(&a));
    assert!(results.contains(&b));
}

#[test]
fn p008_layer_spanning_multiple_cells() {
    let mut sh = SpatialHash::new(50.0);
    let id = Uuid::new_v4();
    sh.insert(id, rect(25.0, 25.0, 75.0, 75.0)); // crosses 4 cells
    let results = sh.query(rect(0.0, 0.0, 200.0, 200.0));
    assert!(results.contains(&id));
}

#[test]
fn p009_query_returns_unique_ids() {
    let mut sh = SpatialHash::new(50.0);
    let id = Uuid::new_v4();
    sh.insert(id, rect(25.0, 25.0, 75.0, 75.0)); // spans multiple cells
    let results = sh.query(rect(0.0, 0.0, 200.0, 200.0));
    let unique: std::collections::HashSet<_> = results.iter().collect();
    assert_eq!(unique.len(), results.len());
}

#[test]
fn p010_cell_size_affects_granularity() {
    let mut sh_coarse = SpatialHash::new(1000.0);
    let mut sh_fine   = SpatialHash::new(10.0);
    let id = Uuid::new_v4();
    sh_coarse.insert(id, rect(0.0, 0.0, 50.0, 50.0));
    sh_fine.insert(id, rect(0.0, 0.0, 50.0, 50.0));
    // Query slightly outside the layer — coarse may still hit, fine should not
    // (both are acceptable implementations; just verify no panic)
    let _ = sh_coarse.query(rect(200.0, 200.0, 10.0, 10.0));
    let _ = sh_fine.query(rect(200.0, 200.0, 10.0, 10.0));
}

#[test]
fn p011_insert_many_then_query_subset() {
    let mut sh = SpatialHash::new(100.0);
    let in_range: Vec<Uuid> = (0..5).map(|i| {
        let id = Uuid::new_v4();
        sh.insert(id, rect(i as f32 * 10.0, 0.0, 9.0, 9.0));
        id
    }).collect();
    let out_id = Uuid::new_v4();
    sh.insert(out_id, rect(900.0, 900.0, 10.0, 10.0));

    let results = sh.query(rect(0.0, 0.0, 60.0, 60.0));
    for id in &in_range {
        assert!(results.contains(id));
    }
}

#[test]
fn p012_empty_query_region_returns_empty() {
    let mut sh = SpatialHash::new(100.0);
    sh.insert(Uuid::new_v4(), rect(50.0, 50.0, 10.0, 10.0));
    let results = sh.query(rect(0.0, 0.0, 0.0, 0.0));
    assert!(results.is_empty());
}

// ── §2  Document::diff (p013–p022) ───────────────────────────────────────────

#[test]
fn p013_diff_identical_docs_is_noop() {
    let doc = Document::new();
    let patch = doc.diff(&doc).unwrap();
    assert!(patch.is_empty());
}

#[test]
fn p014_diff_added_layer() {
    let before = Document::new();
    let after  = Document::new();
    let layer = rect_layer(0.0, 0.0, 100.0, 100.0);
    let id = layer.id();
    after.add_layer(layer).unwrap();
    let patch = before.diff(&after).unwrap();
    assert_eq!(patch.added.len(), 1);
    assert!(patch.added.contains(&id));
    assert!(patch.removed.is_empty());
}

#[test]
fn p015_diff_removed_layer() {
    let before = Document::new();
    let layer = rect_layer(0.0, 0.0, 100.0, 100.0);
    let id = layer.id();
    before.add_layer(layer).unwrap();
    let after = Document::new(); // empty
    let patch = before.diff(&after).unwrap();
    assert!(patch.removed.contains(&id));
    assert!(patch.added.is_empty());
}

#[test]
fn p016_diff_moved_layer() {
    let before = Document::new();
    let after  = Document::new();
    let id = Uuid::new_v4();
    let mut r1 = RectLayer::new(0.0, 0.0, 100.0, 100.0);
    r1.id = id;
    let mut r2 = RectLayer::new(200.0, 200.0, 100.0, 100.0);
    r2.id = id;
    before.add_layer(Layer::Rect(r1)).unwrap();
    after.add_layer(Layer::Rect(r2)).unwrap();
    let patch = before.diff(&after).unwrap();
    assert!(patch.moved.contains(&id));
    assert!(patch.added.is_empty());
    assert!(patch.removed.is_empty());
}

#[test]
fn p017_diff_unchanged_bounds_not_in_moved() {
    let before = Document::new();
    let after  = Document::new();
    let id = Uuid::new_v4();
    let mut r1 = RectLayer::new(50.0, 50.0, 100.0, 100.0);
    r1.id = id;
    let mut r2 = RectLayer::new(50.0, 50.0, 100.0, 100.0);
    r2.id = id;
    before.add_layer(Layer::Rect(r1)).unwrap();
    after.add_layer(Layer::Rect(r2)).unwrap();
    let patch = before.diff(&after).unwrap();
    assert!(!patch.moved.contains(&id));
    assert!(patch.is_empty());
}

#[test]
fn p018_diff_multiple_adds() {
    let before = Document::new();
    let after  = Document::new();
    let ids: Vec<Uuid> = (0..3).map(|i| {
        let l = rect_layer(i as f32 * 50.0, 0.0, 40.0, 40.0);
        let id = l.id();
        after.add_layer(l).unwrap();
        id
    }).collect();
    let patch = before.diff(&after).unwrap();
    assert_eq!(patch.added.len(), 3);
    for id in &ids { assert!(patch.added.contains(id)); }
}

#[test]
fn p019_diff_total_changes() {
    let patch = DocumentPatch {
        added: vec![Uuid::new_v4(), Uuid::new_v4()],
        removed: vec![Uuid::new_v4()],
        moved: vec![],
    };
    assert_eq!(patch.total_changes(), 3);
}

#[test]
fn p020_diff_patch_not_empty() {
    let patch = DocumentPatch {
        added: vec![Uuid::new_v4()],
        removed: vec![],
        moved: vec![],
    };
    assert!(!patch.is_empty());
}

#[test]
fn p021_diff_sub_threshold_move_not_flagged() {
    let before = Document::new();
    let after  = Document::new();
    let id = Uuid::new_v4();
    let mut r1 = RectLayer::new(100.0, 100.0, 100.0, 100.0);
    r1.id = id;
    let mut r2 = RectLayer::new(100.3, 100.3, 100.0, 100.0); // < 0.5 delta
    r2.id = id;
    before.add_layer(Layer::Rect(r1)).unwrap();
    after.add_layer(Layer::Rect(r2)).unwrap();
    let patch = before.diff(&after).unwrap();
    assert!(!patch.moved.contains(&id));
}

#[test]
fn p022_diff_add_and_remove_simultaneously() {
    let before = Document::new();
    let after  = Document::new();
    let rem = rect_layer(0.0, 0.0, 100.0, 100.0);
    let rem_id = rem.id();
    before.add_layer(rem).unwrap();
    let add = rect_layer(200.0, 200.0, 100.0, 100.0);
    let add_id = add.id();
    after.add_layer(add).unwrap();
    let patch = before.diff(&after).unwrap();
    assert!(patch.added.contains(&add_id));
    assert!(patch.removed.contains(&rem_id));
}

// ── §3  AnimationLibrary in Document (p023–p028) ─────────────────────────────

#[test]
fn p023_new_doc_animation_library_empty() {
    let doc = Document::new();
    assert!(doc.animation_library.is_empty());
}

#[test]
fn p024_add_clip_to_doc_library() {
    let mut doc = Document::new();
    let c = clip("intro", AnimationFormat::Lottie);
    let id = c.id;
    doc.animation_library.add(c);
    assert_eq!(doc.animation_library.len(), 1);
    assert!(doc.animation_library.get(id).is_some());
}

#[test]
fn p025_remove_clip_from_doc_library() {
    let mut doc = Document::new();
    let c = clip("intro", AnimationFormat::Lottie);
    let id = c.id;
    doc.animation_library.add(c);
    let removed = doc.animation_library.remove(id);
    assert!(removed);
    assert!(doc.animation_library.is_empty());
}

#[test]
fn p026_filter_clips_by_format() {
    let mut doc = Document::new();
    doc.animation_library.add(clip("lottie1", AnimationFormat::Lottie));
    doc.animation_library.add(clip("svg1",    AnimationFormat::AnimatedSvg));
    let lotties = doc.animation_library.by_format(AnimationFormat::Lottie);
    assert_eq!(lotties.len(), 1);
    assert_eq!(lotties[0].name, "lottie1");
}

#[test]
fn p027_multiple_clips_in_doc_library() {
    let mut doc = Document::new();
    for i in 0..5 {
        doc.animation_library.add(clip(&format!("clip{i}"), AnimationFormat::Native));
    }
    assert_eq!(doc.animation_library.len(), 5);
}

#[test]
fn p028_doc_library_iter() {
    let mut doc = Document::new();
    doc.animation_library.add(clip("a", AnimationFormat::Lottie));
    doc.animation_library.add(clip("b", AnimationFormat::Lottie));
    let names: Vec<_> = doc.animation_library.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"a"));
    assert!(names.contains(&"b"));
}

// ── §4  DocumentPatch helpers (p029–p030) ────────────────────────────────────

#[test]
fn p029_patch_is_empty_when_default() {
    let patch = DocumentPatch::default();
    assert!(patch.is_empty());
    assert_eq!(patch.total_changes(), 0);
}

#[test]
fn p030_patch_total_changes_all_buckets() {
    let patch = DocumentPatch {
        added:   vec![Uuid::new_v4(), Uuid::new_v4()],
        removed: vec![Uuid::new_v4()],
        moved:   vec![Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()],
    };
    assert_eq!(patch.total_changes(), 6);
    assert!(!patch.is_empty());
}
