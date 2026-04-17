// logos-import-sketch integration tests (external)

use logos_import_sketch::archive::build_test_sketch;
use logos_import_sketch::model::SketchLayer;
use logos_import_sketch::{import_sketch, import_sketch_with_options, SketchImporter};
use logos_import_common::{ImportOptions, Importer};

#[test]
fn import_empty_document_has_no_layers() {
    let data = build_test_sketch(&[]);
    let doc = import_sketch(&data).unwrap();
    let page = doc.root.read().unwrap();
    assert_eq!(page.layers.len(), 0);
}

#[test]
fn import_rect_creates_rect_layer() {
    let layers = [SketchLayer::rect("id-1", "bg", 0.0, 0.0, 800.0, 600.0)];
    let data = build_test_sketch(&layers);
    let doc = import_sketch(&data).unwrap();
    let page = doc.root.read().unwrap();
    assert_eq!(page.layers.len(), 1);
    match &page.layers[0] {
        logos_core::Layer::Rect(r) => {
            assert!((r.bounds.width - 800.0).abs() < 1e-3);
            assert!((r.bounds.height - 600.0).abs() < 1e-3);
        }
        _ => panic!("expected Rect layer"),
    }
}

#[test]
fn import_oval_creates_ellipse_layer() {
    let layers = [SketchLayer::oval("id-2", "oval", 10.0, 10.0, 100.0, 100.0)];
    let data = build_test_sketch(&layers);
    let doc = import_sketch(&data).unwrap();
    let page = doc.root.read().unwrap();
    match &page.layers[0] {
        logos_core::Layer::Ellipse(e) => {
            assert!((e.bounds.width - 100.0).abs() < 1e-3);
        }
        _ => panic!("expected Ellipse layer"),
    }
}

#[test]
fn import_text_preserves_content() {
    let layers = [SketchLayer::text("id-3", "label", 0.0, 0.0, 200.0, 30.0, "Sketch!")];
    let data = build_test_sketch(&layers);
    let doc = import_sketch(&data).unwrap();
    let page = doc.root.read().unwrap();
    match &page.layers[0] {
        logos_core::Layer::Text(t) => assert_eq!(t.content, "Sketch!"),
        _ => panic!("expected Text layer"),
    }
}

#[test]
fn import_group_creates_frame_with_children() {
    let layers = [SketchLayer::group("g1", "group", vec![
        SketchLayer::rect("c1", "r1", 0.0, 0.0, 50.0, 50.0),
        SketchLayer::rect("c2", "r2", 60.0, 0.0, 50.0, 50.0),
    ])];
    let data = build_test_sketch(&layers);
    let doc = import_sketch(&data).unwrap();
    let page = doc.root.read().unwrap();
    match &page.layers[0] {
        logos_core::Layer::Frame(f) => assert_eq!(f.children.len(), 2),
        _ => panic!("expected Frame layer"),
    }
}

#[test]
fn import_artboard_creates_layer() {
    let layers = [SketchLayer::artboard("ab1", "Screen 1", 0.0, 0.0, 375.0, 812.0, vec![])];
    let data = build_test_sketch(&layers);
    let doc = import_sketch(&data).unwrap();
    let page = doc.root.read().unwrap();
    assert_eq!(page.layers.len(), 1);
}

#[test]
fn import_multiple_layers_all_converted() {
    let layers = [
        SketchLayer::rect("r1", "Rect 1", 0.0, 0.0, 100.0, 50.0),
        SketchLayer::oval("o1", "Oval 1", 110.0, 0.0, 50.0, 50.0),
        SketchLayer::text("t1", "Label", 0.0, 60.0, 150.0, 25.0, "Hello"),
    ];
    let data = build_test_sketch(&layers);
    let doc = import_sketch(&data).unwrap();
    let page = doc.root.read().unwrap();
    assert_eq!(page.layers.len(), 3);
}

#[test]
fn import_invalid_bytes_returns_error() {
    assert!(import_sketch(b"\x00\x01garbage").is_err());
}

#[test]
fn import_with_default_options_succeeds() {
    let layers = [SketchLayer::rect("r1", "Rect", 0.0, 0.0, 100.0, 100.0)];
    let data = build_test_sketch(&layers);
    assert!(import_sketch_with_options(&data, &ImportOptions::default()).is_ok());
}

#[test]
fn import_with_fast_options_succeeds() {
    let layers = [SketchLayer::rect("r1", "Rect", 0.0, 0.0, 100.0, 100.0)];
    let data = build_test_sketch(&layers);
    assert!(import_sketch_with_options(&data, &ImportOptions::fast()).is_ok());
}

#[test]
fn import_with_preview_options_succeeds() {
    let layers = [SketchLayer::rect("r1", "Rect", 0.0, 0.0, 100.0, 100.0)];
    let data = build_test_sketch(&layers);
    assert!(import_sketch_with_options(&data, &ImportOptions::preview()).is_ok());
}

#[test]
fn sketch_importer_name_is_sketch() {
    assert_eq!(SketchImporter.name(), "sketch");
}

#[test]
fn sketch_importer_handles_sketch_extension() {
    assert!(SketchImporter.extensions().contains(&"sketch"));
}

#[test]
fn sketch_importer_can_handle_sketch() {
    assert!(SketchImporter.can_handle("sketch"));
}

#[test]
fn sketch_importer_cannot_handle_xd() {
    assert!(!SketchImporter.can_handle("xd"));
}

#[test]
fn sketch_importer_import_method_works() {
    let data = build_test_sketch(&[]);
    assert!(SketchImporter.import(&data, &ImportOptions::default()).is_ok());
}

#[test]
fn import_rect_position_preserved() {
    let layers = [SketchLayer::rect("r1", "R", 15.0, 25.0, 80.0, 60.0)];
    let data = build_test_sketch(&layers);
    let doc = import_sketch(&data).unwrap();
    let page = doc.root.read().unwrap();
    match &page.layers[0] {
        logos_core::Layer::Rect(r) => {
            assert!((r.bounds.x - 15.0).abs() < 1e-3);
            assert!((r.bounds.y - 25.0).abs() < 1e-3);
        }
        _ => panic!("expected Rect"),
    }
}

#[test]
fn build_test_sketch_produces_non_empty_bytes() {
    assert!(!build_test_sketch(&[]).is_empty());
}
