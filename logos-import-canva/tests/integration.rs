// logos-import-canva integration tests (external)
//
// Uses only the public API of logos-import-canva.

use logos_import_canva::model::{CanvaDocument, CanvaElement};
use logos_import_canva::{import_canva, import_canva_with_options, CanvaImporter};
use logos_import_common::{ImportOptions, Importer};

fn canva_bytes(doc: &CanvaDocument) -> Vec<u8> {
    serde_json::to_vec(doc).unwrap()
}

#[test]
fn import_empty_document_sets_page_name() {
    let doc = CanvaDocument::new("My Page", 800.0, 600.0, vec![]);
    let result = import_canva(&canva_bytes(&doc)).unwrap();
    let page = result.root.read().unwrap();
    assert_eq!(page.name, "My Page");
}

#[test]
fn import_empty_document_has_no_layers() {
    let doc = CanvaDocument::new("Empty", 1920.0, 1080.0, vec![]);
    let result = import_canva(&canva_bytes(&doc)).unwrap();
    let page = result.root.read().unwrap();
    assert_eq!(page.layers.len(), 0);
}

#[test]
fn import_rect_creates_rect_layer() {
    let doc = CanvaDocument::new("D", 800.0, 600.0, vec![
        CanvaElement::rect("bg", 0.0, 0.0, 800.0, 600.0),
    ]);
    let result = import_canva(&canva_bytes(&doc)).unwrap();
    let page = result.root.read().unwrap();
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
fn import_ellipse_creates_ellipse_layer() {
    let doc = CanvaDocument::new("D", 400.0, 400.0, vec![
        CanvaElement::ellipse("circle", 50.0, 50.0, 300.0, 300.0),
    ]);
    let result = import_canva(&canva_bytes(&doc)).unwrap();
    let page = result.root.read().unwrap();
    match &page.layers[0] {
        logos_core::Layer::Ellipse(e) => {
            assert!((e.bounds.width - 300.0).abs() < 1e-3);
        }
        _ => panic!("expected Ellipse layer"),
    }
}

#[test]
fn import_text_preserves_content() {
    let doc = CanvaDocument::new("D", 800.0, 600.0, vec![
        CanvaElement::text("title", 100.0, 50.0, 300.0, 40.0, "Hello Canva"),
    ]);
    let result = import_canva(&canva_bytes(&doc)).unwrap();
    let page = result.root.read().unwrap();
    match &page.layers[0] {
        logos_core::Layer::Text(t) => assert_eq!(t.content, "Hello Canva"),
        _ => panic!("expected Text layer"),
    }
}

#[test]
fn import_image_creates_layer() {
    let doc = CanvaDocument::new("D", 800.0, 600.0, vec![
        CanvaElement::image("photo", 0.0, 0.0, 400.0, 300.0),
    ]);
    let result = import_canva(&canva_bytes(&doc)).unwrap();
    let page = result.root.read().unwrap();
    assert_eq!(page.layers.len(), 1);
}

#[test]
fn import_group_creates_frame_with_two_children() {
    let doc = CanvaDocument::new("D", 800.0, 600.0, vec![
        CanvaElement::group("grp", vec![
            CanvaElement::rect("r1", 0.0, 0.0, 100.0, 100.0),
            CanvaElement::rect("r2", 110.0, 0.0, 100.0, 100.0),
        ]),
    ]);
    let result = import_canva(&canva_bytes(&doc)).unwrap();
    let page = result.root.read().unwrap();
    match &page.layers[0] {
        logos_core::Layer::Frame(f) => assert_eq!(f.children.len(), 2),
        _ => panic!("expected Frame layer"),
    }
}

#[test]
fn import_multiple_elements_all_converted() {
    let doc = CanvaDocument::new("D", 800.0, 600.0, vec![
        CanvaElement::rect("r1", 0.0, 0.0, 100.0, 50.0),
        CanvaElement::rect("r2", 110.0, 0.0, 100.0, 50.0),
        CanvaElement::text("t1", 0.0, 60.0, 200.0, 30.0, "Hi"),
        CanvaElement::ellipse("e1", 220.0, 0.0, 80.0, 80.0),
    ]);
    let result = import_canva(&canva_bytes(&doc)).unwrap();
    let page = result.root.read().unwrap();
    assert_eq!(page.layers.len(), 4);
}

#[test]
fn import_invalid_json_returns_error() {
    assert!(import_canva(b"not json at all").is_err());
}

#[test]
fn import_empty_bytes_returns_error() {
    assert!(import_canva(b"").is_err());
}

#[test]
fn import_with_default_options_succeeds() {
    let doc = CanvaDocument::new("D", 400.0, 400.0, vec![
        CanvaElement::rect("r", 0.0, 0.0, 400.0, 400.0),
    ]);
    assert!(import_canva_with_options(&canva_bytes(&doc), &ImportOptions::default()).is_ok());
}

#[test]
fn import_with_fast_options_succeeds() {
    let doc = CanvaDocument::new("D", 400.0, 400.0, vec![
        CanvaElement::rect("r", 0.0, 0.0, 400.0, 400.0),
    ]);
    assert!(import_canva_with_options(&canva_bytes(&doc), &ImportOptions::fast()).is_ok());
}

#[test]
fn import_with_preview_options_succeeds() {
    let doc = CanvaDocument::new("D", 400.0, 400.0, vec![
        CanvaElement::rect("r", 0.0, 0.0, 400.0, 400.0),
    ]);
    assert!(import_canva_with_options(&canva_bytes(&doc), &ImportOptions::preview()).is_ok());
}

#[test]
fn canva_importer_name_is_canva() {
    assert_eq!(CanvaImporter.name(), "canva");
}

#[test]
fn canva_importer_handles_canva_extension() {
    assert!(CanvaImporter.extensions().contains(&"canva"));
}

#[test]
fn canva_importer_import_method_works() {
    let doc = CanvaDocument::new("D", 100.0, 100.0, vec![]);
    let result = CanvaImporter.import(&canva_bytes(&doc), &ImportOptions::default());
    assert!(result.is_ok());
}
