// logos-core/tests/doc_api.rs
//
// Documentation-style integration tests (t700–t724).
//
// Sections
// --------
//   §1  Basic workspace setup (t700–t704)
//   §2  Flat-page document lifecycle (t705–t709)
//   §3  Layer authoring patterns (t710–t714)
//   §4  Selection workflow (t715–t719)
//   §5  Serialisation / persistence (t720–t724)

use logos_core::{
    Document, DocumentMode, EllipseLayer, FrameLayer, Layer, Rect, RectLayer,
    TextLayer, WorkspaceMode,
};
use uuid::Uuid;

// §1 ──────────────────────────────────────────────────────────────────────────

#[test]
fn t700_doc_workspace_mode_selection() {
    let flat = WorkspaceMode::FlatPage;
    assert!(flat.supports_flat());
    assert!(!flat.supports_artboards());
    let artboard = WorkspaceMode::ArtboardSection;
    assert!(artboard.supports_artboards());
    assert!(!artboard.supports_flat());
    let hybrid = WorkspaceMode::Hybrid;
    assert!(hybrid.supports_flat());
    assert!(hybrid.supports_artboards());
}

#[test]
fn t701_doc_workspace_mode_labels() {
    assert_eq!(WorkspaceMode::FlatPage.label(), "Flat Page");
    assert_eq!(WorkspaceMode::ArtboardSection.label(), "Artboard / Section");
    assert_eq!(WorkspaceMode::Hybrid.label(), "Hybrid");
}

#[test]
fn t702_doc_document_mode_defaults() {
    let mode = DocumentMode::flat();
    assert!(!mode.show_grid);
    assert!(mode.snap_to_objects);
    assert_eq!(mode.mode, WorkspaceMode::FlatPage);
}

#[test]
fn t703_doc_document_mode_variants() {
    assert_eq!(DocumentMode::flat().mode, WorkspaceMode::FlatPage);
    assert_eq!(DocumentMode::artboard().mode, WorkspaceMode::ArtboardSection);
    assert_eq!(DocumentMode::hybrid().mode, WorkspaceMode::Hybrid);
}

#[test]
fn t704_doc_document_construction() {
    let doc = Document::new();
    assert_ne!(doc.id, Uuid::nil());
    assert!(doc.version >= 1);
}

// §2 ──────────────────────────────────────────────────────────────────────────

#[test]
fn t705_doc_enable_flat_page_mode() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    assert_eq!(doc.doc_mode.mode, WorkspaceMode::FlatPage);
}

#[test]
fn t706_doc_default_page_name() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    let page = doc.root.read().unwrap();
    assert_eq!(page.name, "Page 1");
}

#[test]
fn t707_doc_flat_page_starts_empty() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    let page = doc.root.read().unwrap();
    assert!(page.layers.is_empty());
}

#[test]
fn t708_doc_two_documents_have_distinct_ids() {
    let a = Document::with_mode(WorkspaceMode::FlatPage);
    let b = Document::with_mode(WorkspaceMode::FlatPage);
    assert_ne!(a.id, b.id);
}

#[test]
fn t709_doc_switch_to_artboard_mode() {
    let doc = Document::with_mode(WorkspaceMode::ArtboardSection);
    assert_eq!(doc.doc_mode.mode, WorkspaceMode::ArtboardSection);
    assert!(doc.doc_mode.mode.supports_artboards());
}

// §3 ──────────────────────────────────────────────────────────────────────────

#[test]
fn t710_doc_add_rect_layer() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    let rect = RectLayer::new(0.0, 0.0, 300.0, 200.0);
    let id = rect.id;
    doc.add_layer(Layer::Rect(rect)).unwrap();
    let layer = doc.find_layer_by_id(id).unwrap().unwrap();
    let b = layer.bounds();
    assert_eq!(b.width, 300.0);
    assert_eq!(b.height, 200.0);
}

#[test]
fn t711_doc_add_text_layer() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    let txt = TextLayer::new("Hello, Logos!", 20.0, 20.0, 400.0, 60.0);
    let id = txt.id;
    doc.add_layer(Layer::Text(txt)).unwrap();
    let layer = doc.find_layer_by_id(id).unwrap().unwrap();
    if let Layer::Text(ref t) = layer {
        assert_eq!(t.content, "Hello, Logos!");
    } else {
        panic!("expected Text layer");
    }
}

#[test]
fn t712_doc_add_ellipse_layer() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    let ellipse = EllipseLayer::new(50.0, 50.0, 150.0, 150.0);
    let id = ellipse.id;
    doc.add_layer(Layer::Ellipse(ellipse)).unwrap();
    assert!(doc.find_layer_by_id(id).unwrap().is_some());
}

#[test]
fn t713_doc_add_frame_layer() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    let frame = FrameLayer {
        id: Uuid::new_v4(),
        children: vec![],
        bounds: Rect { x: 0.0, y: 0.0, width: 1920.0, height: 1080.0 },
    };
    let id = frame.id;
    doc.add_layer(Layer::Frame(frame)).unwrap();
    let layer = doc.find_layer_by_id(id).unwrap().unwrap();
    assert_eq!(layer.children().map(|c| c.len()).unwrap_or(0), 0);
}

#[test]
fn t714_doc_remove_layer() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    let rect = RectLayer::new(10.0, 10.0, 100.0, 50.0);
    let id = rect.id;
    doc.add_layer(Layer::Rect(rect)).unwrap();
    doc.remove_layer(id).unwrap();
    assert!(doc.find_layer_by_id(id).unwrap().is_none());
}

// §4 ──────────────────────────────────────────────────────────────────────────

#[test]
fn t715_doc_initial_selection_empty() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    assert!(doc.get_selection().unwrap().is_empty());
}

#[test]
fn t716_doc_select_layer_on_click() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    let id = Uuid::new_v4();
    doc.set_selection(vec![id]).unwrap();
    assert_eq!(doc.get_selection().unwrap(), vec![id]);
}

#[test]
fn t717_doc_multi_select() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    let ids: Vec<Uuid> = (0..3_u32).map(|i| {
        let r = RectLayer::new(i as f32 * 120.0, 0.0, 100.0, 100.0);
        let id = r.id;
        doc.add_layer(Layer::Rect(r)).unwrap();
        id
    }).collect();
    doc.set_selection(ids.clone()).unwrap();
    assert_eq!(doc.get_selection().unwrap().len(), 3);
}

#[test]
fn t718_doc_clear_selection() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    doc.set_selection(vec![Uuid::new_v4()]).unwrap();
    doc.clear_selection().unwrap();
    assert!(doc.get_selection().unwrap().is_empty());
}

#[test]
fn t719_doc_replace_selection() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    let id_a = Uuid::new_v4();
    let id_b = Uuid::new_v4();
    doc.set_selection(vec![id_a]).unwrap();
    assert_eq!(doc.get_selection().unwrap(), vec![id_a]);
    doc.set_selection(vec![id_b]).unwrap();
    assert_eq!(doc.get_selection().unwrap(), vec![id_b]);
}

// §5 ──────────────────────────────────────────────────────────────────────────

#[test]
fn t720_doc_serialize_to_json() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    doc.add_layer(Layer::Rect(RectLayer::new(0.0, 0.0, 100.0, 100.0))).unwrap();
    let json = serde_json::to_string(&doc).unwrap();
    assert!(json.contains("FlatPage"));
    assert!(json.contains("Page 1"));
}

#[test]
fn t721_doc_document_mode_round_trip() {
    let original = DocumentMode::flat();
    let json = serde_json::to_string(&original).unwrap();
    let restored: DocumentMode = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.mode, WorkspaceMode::FlatPage);
}

#[test]
fn t722_doc_workspace_mode_round_trip() {
    for mode in [WorkspaceMode::FlatPage, WorkspaceMode::ArtboardSection, WorkspaceMode::Hybrid] {
        let json = serde_json::to_string(&mode).unwrap();
        let restored: WorkspaceMode = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, mode);
    }
}

#[test]
fn t723_doc_layer_bounds_preserved_after_serde() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    let rect = RectLayer::new(42.0, 84.0, 128.0, 64.0);
    let id = rect.id;
    doc.add_layer(Layer::Rect(rect)).unwrap();
    let json = serde_json::to_string(&doc).unwrap();
    let restored: Document = serde_json::from_str(&json).unwrap();
    let layer = restored.find_layer_by_id(id).unwrap().unwrap();
    let b = layer.bounds();
    assert_eq!(b.x, 42.0);
    assert_eq!(b.y, 84.0);
    assert_eq!(b.width, 128.0);
    assert_eq!(b.height, 64.0);
}

#[test]
fn t724_doc_text_content_preserved_after_serde() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    let txt = TextLayer::new("Logos Design", 0.0, 0.0, 300.0, 50.0);
    let id = txt.id;
    doc.add_layer(Layer::Text(txt)).unwrap();
    let json = serde_json::to_string(&doc).unwrap();
    let restored: Document = serde_json::from_str(&json).unwrap();
    if let Layer::Text(ref t) = restored.find_layer_by_id(id).unwrap().unwrap() {
        assert_eq!(t.content, "Logos Design");
    } else {
        panic!("expected Text layer after round-trip");
    }
}
