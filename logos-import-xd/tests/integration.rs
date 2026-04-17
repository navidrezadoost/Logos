// logos-import-xd integration tests (external)

use logos_import_xd::archive::build_test_xd;
use logos_import_xd::model::XdNode;
use logos_import_xd::{import_xd, import_xd_with_options, XdImporter};
use logos_import_common::{ImportOptions, Importer};

#[test]
fn import_empty_artboard_sets_page_name() {
    let data = build_test_xd(&[], "Landing");
    let doc = import_xd(&data).unwrap();
    let page = doc.root.read().unwrap();
    assert_eq!(page.name, "Landing");
}

#[test]
fn import_empty_artboard_has_no_layers() {
    let data = build_test_xd(&[], "Main");
    let doc = import_xd(&data).unwrap();
    let page = doc.root.read().unwrap();
    assert_eq!(page.layers.len(), 0);
}

#[test]
fn import_rect_creates_rect_layer() {
    let nodes = [XdNode::rect("bg", 0.0, 0.0, 375.0, 812.0)];
    let data = build_test_xd(&nodes, "Art");
    let doc = import_xd(&data).unwrap();
    let page = doc.root.read().unwrap();
    assert_eq!(page.layers.len(), 1);
    match &page.layers[0] {
        logos_core::Layer::Rect(r) => {
            assert!((r.bounds.width - 375.0).abs() < 1e-3);
            assert!((r.bounds.height - 812.0).abs() < 1e-3);
        }
        _ => panic!("expected Rect layer"),
    }
}

#[test]
fn import_ellipse_creates_ellipse_layer() {
    let nodes = [XdNode::ellipse("oval", 10.0, 10.0, 100.0, 80.0)];
    let data = build_test_xd(&nodes, "Art");
    let doc = import_xd(&data).unwrap();
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
    let nodes = [XdNode::text("label", 0.0, 0.0, 200.0, 30.0, "Hello XD")];
    let data = build_test_xd(&nodes, "Art");
    let doc = import_xd(&data).unwrap();
    let page = doc.root.read().unwrap();
    match &page.layers[0] {
        logos_core::Layer::Text(t) => assert_eq!(t.content, "Hello XD"),
        _ => panic!("expected Text layer"),
    }
}

#[test]
fn import_group_creates_frame_with_children() {
    let nodes = [XdNode::group("grp", vec![
        XdNode::rect("r1", 0.0, 0.0, 50.0, 50.0),
        XdNode::rect("r2", 60.0, 0.0, 50.0, 50.0),
    ])];
    let data = build_test_xd(&nodes, "Art");
    let doc = import_xd(&data).unwrap();
    let page = doc.root.read().unwrap();
    match &page.layers[0] {
        logos_core::Layer::Frame(f) => assert_eq!(f.children.len(), 2),
        _ => panic!("expected Frame layer"),
    }
}

#[test]
fn import_line_creates_layer() {
    let nodes = [XdNode::line("ln", 0.0, 0.0, 100.0, 100.0)];
    let data = build_test_xd(&nodes, "Art");
    let doc = import_xd(&data).unwrap();
    let page = doc.root.read().unwrap();
    assert_eq!(page.layers.len(), 1);
}

#[test]
fn import_multiple_nodes_all_converted() {
    let nodes = [
        XdNode::rect("r1", 0.0, 0.0, 100.0, 100.0),
        XdNode::ellipse("e1", 110.0, 0.0, 80.0, 80.0),
        XdNode::text("t1", 0.0, 110.0, 200.0, 20.0, "Hi"),
    ];
    let data = build_test_xd(&nodes, "Art");
    let doc = import_xd(&data).unwrap();
    let page = doc.root.read().unwrap();
    assert_eq!(page.layers.len(), 3);
}

#[test]
fn import_invalid_bytes_returns_error() {
    assert!(import_xd(b"not a zip file").is_err());
}

#[test]
fn import_with_default_options_succeeds() {
    let data = build_test_xd(&[XdNode::rect("r", 0.0, 0.0, 100.0, 100.0)], "A");
    assert!(import_xd_with_options(&data, &ImportOptions::default()).is_ok());
}

#[test]
fn import_with_fast_options_succeeds() {
    let data = build_test_xd(&[XdNode::rect("r", 0.0, 0.0, 100.0, 100.0)], "A");
    assert!(import_xd_with_options(&data, &ImportOptions::fast()).is_ok());
}

#[test]
fn import_with_preview_options_succeeds() {
    let data = build_test_xd(&[XdNode::rect("r", 0.0, 0.0, 100.0, 100.0)], "A");
    assert!(import_xd_with_options(&data, &ImportOptions::preview()).is_ok());
}

#[test]
fn xd_importer_name_is_xd() {
    assert_eq!(XdImporter.name(), "xd");
}

#[test]
fn xd_importer_handles_xd_extension() {
    assert!(XdImporter.extensions().contains(&"xd"));
}

#[test]
fn xd_importer_can_handle_xd() {
    assert!(XdImporter.can_handle("xd"));
}

#[test]
fn xd_importer_cannot_handle_sketch() {
    assert!(!XdImporter.can_handle("sketch"));
}

#[test]
fn xd_importer_import_method_works() {
    let data = build_test_xd(&[], "Art");
    assert!(XdImporter.import(&data, &ImportOptions::default()).is_ok());
}

#[test]
fn build_test_xd_produces_non_empty_bytes() {
    assert!(!build_test_xd(&[], "A").is_empty());
}
