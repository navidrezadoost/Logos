//! Phase 4 Integration Tests — Flat-Page (Canva) Mode Workflow
//!
//! Covers the full Document API when operating in `WorkspaceMode::FlatPage`:
//!   §1 DocumentMode in flat mode          (t400–t409)
//!   §2 Document::with_mode flat workflow  (t410–t419)
//!   §3 Layer management in flat doc       (t420–t429)
//!   §4 Selection management              (t430–t439)
//!   §5 Round-trip serialization          (t440–t449)

use logos_core::{
    Document, DocumentMode, EllipseLayer, FrameLayer, Layer, RectLayer, Rect,
    TextLayer, WorkspaceMode,
};
use uuid::Uuid;

// ── §1: DocumentMode in flat mode ────────────────────────────────────────────

#[test]
fn t400_flat_mode_constructor_has_flatpage_mode() {
    let dm = DocumentMode::flat();
    assert_eq!(dm.mode, WorkspaceMode::FlatPage);
}

#[test]
fn t401_flat_mode_supports_flat_returns_true() {
    let dm = DocumentMode::flat();
    assert!(dm.mode.supports_flat());
}

#[test]
fn t402_flat_mode_does_not_support_artboards() {
    let dm = DocumentMode::flat();
    assert!(!dm.mode.supports_artboards());
}

#[test]
fn t403_flat_mode_label_is_flat_page() {
    let dm = DocumentMode::flat();
    assert_eq!(dm.mode.label(), "Flat Page");
}

#[test]
fn t404_flat_mode_show_grid_default_false() {
    let dm = DocumentMode::flat();
    assert!(!dm.show_grid);
}

#[test]
fn t405_flat_mode_snap_to_objects_default_true() {
    let dm = DocumentMode::flat();
    assert!(dm.snap_to_objects);
}

#[test]
fn t406_new_from_mode_flatpage_matches_flat_constructor() {
    let dm1 = DocumentMode::flat();
    let dm2 = DocumentMode::new(WorkspaceMode::FlatPage);
    assert_eq!(dm1.mode, dm2.mode);
    assert_eq!(dm1.show_grid, dm2.show_grid);
    assert_eq!(dm1.snap_to_objects, dm2.snap_to_objects);
}

#[test]
fn t407_flatpage_mode_not_equal_to_hybrid() {
    assert_ne!(WorkspaceMode::FlatPage, WorkspaceMode::Hybrid);
}

#[test]
fn t408_flatpage_mode_not_equal_to_artboard_section() {
    assert_ne!(WorkspaceMode::FlatPage, WorkspaceMode::ArtboardSection);
}

#[test]
fn t409_workspace_mode_is_copy() {
    let m = WorkspaceMode::FlatPage;
    let m2 = m;
    assert_eq!(m, m2);
}

// ── §2: Document::with_mode flat workflow ────────────────────────────────────

#[test]
fn t410_document_with_mode_flat_sets_mode() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    assert_eq!(doc.doc_mode.mode, WorkspaceMode::FlatPage);
}

#[test]
fn t411_flat_doc_has_non_nil_id() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    assert_ne!(doc.id, Uuid::nil());
}

#[test]
fn t412_flat_doc_version_is_one() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    assert_eq!(doc.version, 1);
}

#[test]
fn t413_flat_doc_root_page_name_is_page_1() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    let page = doc.root.read().unwrap();
    assert_eq!(page.name, "Page 1");
}

#[test]
fn t414_flat_doc_root_starts_empty() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    let page = doc.root.read().unwrap();
    assert!(page.layers.is_empty());
}

#[test]
fn t415_flat_doc_supports_flat() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    assert!(doc.doc_mode.mode.supports_flat());
}

#[test]
fn t416_two_flat_docs_have_different_ids() {
    let d1 = Document::with_mode(WorkspaceMode::FlatPage);
    let d2 = Document::with_mode(WorkspaceMode::FlatPage);
    assert_ne!(d1.id, d2.id);
}

#[test]
fn t417_flat_doc_mode_clone_preserves_mode() {
    let dm = DocumentMode::flat();
    let cloned = dm.clone();
    assert_eq!(cloned.mode, WorkspaceMode::FlatPage);
}

#[test]
fn t418_document_page_has_non_nil_id() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    let page = doc.root.read().unwrap();
    assert_ne!(page.id, Uuid::nil());
}

#[test]
fn t419_doc_mode_debug_contains_flat() {
    let dm = DocumentMode::flat();
    let s = format!("{dm:?}");
    assert!(s.contains("FlatPage") || s.contains("Flat"), "debug: {s}");
}

// ── §3: Layer management in flat doc ─────────────────────────────────────────

#[test]
fn t420_add_rect_layer_to_flat_doc() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    let r = Layer::Rect(RectLayer::new(0.0, 0.0, 100.0, 100.0));
    doc.add_layer(r).unwrap();
    let page = doc.root.read().unwrap();
    assert_eq!(page.layers.len(), 1);
}

#[test]
fn t421_add_multiple_layers_to_flat_doc() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    for _ in 0..5 {
        doc.add_layer(Layer::Rect(RectLayer::new(0.0, 0.0, 50.0, 50.0))).unwrap();
    }
    let page = doc.root.read().unwrap();
    assert_eq!(page.layers.len(), 5);
}

#[test]
fn t422_find_layer_by_id_in_flat_doc() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    let rect = RectLayer::new(10.0, 20.0, 80.0, 40.0);
    let id = rect.id;
    doc.add_layer(Layer::Rect(rect)).unwrap();
    let found = doc.find_layer_by_id(id).unwrap();
    assert!(found.is_some());
}

#[test]
fn t423_find_layer_by_id_wrong_id_returns_none() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    doc.add_layer(Layer::Rect(RectLayer::new(0.0, 0.0, 50.0, 50.0))).unwrap();
    let result = doc.find_layer_by_id(Uuid::new_v4()).unwrap();
    assert!(result.is_none());
}

#[test]
fn t424_remove_layer_from_flat_doc() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    let rect = RectLayer::new(0.0, 0.0, 50.0, 50.0);
    let id = rect.id;
    doc.add_layer(Layer::Rect(rect)).unwrap();
    doc.remove_layer(id).unwrap();
    let page = doc.root.read().unwrap();
    assert!(page.layers.is_empty());
}

#[test]
fn t425_remove_nonexistent_layer_returns_err() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    let result = doc.remove_layer(Uuid::new_v4());
    assert!(result.is_err());
}

#[test]
fn t426_add_text_layer_to_flat_doc() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    let t = Layer::Text(TextLayer::new("Hello flat page", 0.0, 0.0, 200.0, 30.0));
    doc.add_layer(t).unwrap();
    let page = doc.root.read().unwrap();
    match &page.layers[0] {
        Layer::Text(t) => assert_eq!(t.content, "Hello flat page"),
        _ => panic!("expected text layer"),
    }
}

#[test]
fn t427_add_frame_directly_on_flat_page() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    let frame = Layer::Frame(FrameLayer {
        id: Uuid::new_v4(),
        children: vec![
            Layer::Rect(RectLayer::new(0.0, 0.0, 50.0, 50.0)),
        ],
        bounds: Rect { x: 0.0, y: 0.0, width: 200.0, height: 200.0 },
    });
    doc.add_layer(frame).unwrap();
    let page = doc.root.read().unwrap();
    assert_eq!(page.layers.len(), 1);
    match &page.layers[0] {
        Layer::Frame(f) => assert_eq!(f.children.len(), 1),
        _ => panic!("expected frame"),
    }
}

#[test]
fn t428_add_ellipse_to_flat_doc() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    doc.add_layer(Layer::Ellipse(EllipseLayer::new(0.0, 0.0, 100.0, 100.0))).unwrap();
    let page = doc.root.read().unwrap();
    matches!(&page.layers[0], Layer::Ellipse(_));
}

#[test]
fn t429_layers_preserve_insertion_order() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    let r1 = RectLayer::new(0.0, 0.0, 10.0, 10.0);
    let r2 = RectLayer::new(20.0, 0.0, 10.0, 10.0);
    let id1 = r1.id;
    let id2 = r2.id;
    doc.add_layer(Layer::Rect(r1)).unwrap();
    doc.add_layer(Layer::Rect(r2)).unwrap();
    let page = doc.root.read().unwrap();
    assert_eq!(page.layers[0].id(), id1);
    assert_eq!(page.layers[1].id(), id2);
}

// ── §4: Selection management ──────────────────────────────────────────────────

#[test]
fn t430_initial_selection_is_empty() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    let sel = doc.get_selection().unwrap();
    assert!(sel.is_empty());
}

#[test]
fn t431_set_selection_to_one_layer() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    let id = Uuid::new_v4();
    doc.set_selection(vec![id]).unwrap();
    assert_eq!(doc.get_selection().unwrap(), vec![id]);
}

#[test]
fn t432_set_selection_to_multiple_layers() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    let ids = vec![Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
    doc.set_selection(ids.clone()).unwrap();
    assert_eq!(doc.get_selection().unwrap(), ids);
}

#[test]
fn t433_clear_selection_empties_list() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    doc.set_selection(vec![Uuid::new_v4()]).unwrap();
    doc.clear_selection().unwrap();
    assert!(doc.get_selection().unwrap().is_empty());
}

#[test]
fn t434_set_selection_replaces_previous() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    doc.set_selection(vec![id1]).unwrap();
    doc.set_selection(vec![id2]).unwrap();
    let sel = doc.get_selection().unwrap();
    assert_eq!(sel, vec![id2]);
}

#[test]
fn t435_selection_of_real_layer_ids() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    let rect = RectLayer::new(0.0, 0.0, 50.0, 50.0);
    let id = rect.id;
    doc.add_layer(Layer::Rect(rect)).unwrap();
    doc.set_selection(vec![id]).unwrap();
    let sel = doc.get_selection().unwrap();
    assert_eq!(sel.len(), 1);
    assert_eq!(sel[0], id);
}

#[test]
fn t436_set_empty_selection_works() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    doc.set_selection(vec![]).unwrap();
    assert!(doc.get_selection().unwrap().is_empty());
}

#[test]
fn t437_selection_preserved_across_layer_add() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    let id = Uuid::new_v4();
    doc.set_selection(vec![id]).unwrap();
    doc.add_layer(Layer::Rect(RectLayer::new(0.0, 0.0, 10.0, 10.0))).unwrap();
    assert_eq!(doc.get_selection().unwrap(), vec![id]);
}

#[test]
fn t438_ten_frame_flat_layout_all_stored() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    for i in 0..10 {
        let frame = Layer::Frame(FrameLayer {
            id: Uuid::new_v4(),
            children: vec![],
            bounds: Rect { x: (i as f32) * 220.0, y: 0.0, width: 200.0, height: 200.0 },
        });
        doc.add_layer(frame).unwrap();
    }
    let page = doc.root.read().unwrap();
    assert_eq!(page.layers.len(), 10);
}

#[test]
fn t439_layer_id_method_returns_correct_id() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    let rect = RectLayer::new(0.0, 0.0, 50.0, 50.0);
    let expected = rect.id;
    doc.add_layer(Layer::Rect(rect)).unwrap();
    let page = doc.root.read().unwrap();
    assert_eq!(page.layers[0].id(), expected);
}

// ── §5: Round-trip serialization ─────────────────────────────────────────────

#[test]
fn t440_flat_page_doc_serializes_to_json() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    doc.add_layer(Layer::Rect(RectLayer::new(0.0, 0.0, 100.0, 100.0))).unwrap();
    let json = serde_json::to_string(&doc).unwrap();
    assert!(json.contains("FlatPage"));
}

#[test]
fn t441_document_mode_flat_round_trips() {
    let dm = DocumentMode::flat();
    let json = serde_json::to_string(&dm).unwrap();
    let dm2: DocumentMode = serde_json::from_str(&json).unwrap();
    assert_eq!(dm2.mode, WorkspaceMode::FlatPage);
}

#[test]
fn t442_workspace_mode_flatpage_round_trips() {
    let mode = WorkspaceMode::FlatPage;
    let json = serde_json::to_string(&mode).unwrap();
    let mode2: WorkspaceMode = serde_json::from_str(&json).unwrap();
    assert_eq!(mode2, WorkspaceMode::FlatPage);
}

#[test]
fn t443_flat_doc_json_contains_page_name() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    let json = serde_json::to_string(&doc).unwrap();
    assert!(json.contains("Page 1"), "json: {json}");
}

#[test]
fn t444_layer_bounds_preserved_after_serde() {
    let rect = RectLayer::new(5.0, 10.0, 80.0, 60.0);
    let json = serde_json::to_string(&rect).unwrap();
    let rect2: RectLayer = serde_json::from_str(&json).unwrap();
    assert!((rect2.bounds.x - 5.0).abs() < 1e-5);
    assert!((rect2.bounds.width - 80.0).abs() < 1e-5);
}

#[test]
fn t445_layer_id_preserved_after_serde() {
    let rect = RectLayer::new(0.0, 0.0, 100.0, 100.0);
    let id = rect.id;
    let json = serde_json::to_string(&rect).unwrap();
    let rect2: RectLayer = serde_json::from_str(&json).unwrap();
    assert_eq!(rect2.id, id);
}

#[test]
fn t446_ellipse_layer_round_trips() {
    let e = EllipseLayer::new(1.0, 2.0, 50.0, 40.0);
    let json = serde_json::to_string(&e).unwrap();
    let e2: EllipseLayer = serde_json::from_str(&json).unwrap();
    assert!((e2.bounds.height - 40.0).abs() < 1e-5);
}

#[test]
fn t447_text_layer_content_preserved_in_serde() {
    let t = TextLayer::new("canvas text", 0.0, 0.0, 200.0, 30.0);
    let json = serde_json::to_string(&t).unwrap();
    let t2: TextLayer = serde_json::from_str(&json).unwrap();
    assert_eq!(t2.content, "canvas text");
}

#[test]
fn t448_layer_bounds_method_returns_correct_rect() {
    let rect = RectLayer::new(3.0, 7.0, 120.0, 90.0);
    let layer = Layer::Rect(rect);
    let b = layer.bounds();
    assert!((b.x - 3.0).abs() < 1e-5);
    assert!((b.y - 7.0).abs() < 1e-5);
    assert!((b.width - 120.0).abs() < 1e-5);
    assert!((b.height - 90.0).abs() < 1e-5);
}

#[test]
fn t449_frame_layer_children_returns_slice() {
    let child = Layer::Rect(RectLayer::new(0.0, 0.0, 50.0, 50.0));
    let frame = Layer::Frame(FrameLayer {
        id: Uuid::new_v4(),
        children: vec![child],
        bounds: Rect { x: 0.0, y: 0.0, width: 200.0, height: 200.0 },
    });
    assert_eq!(frame.children().unwrap().len(), 1);
}
