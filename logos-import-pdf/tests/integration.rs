// logos-import-pdf integration tests (external)

use logos_import_pdf::content::PdfElement;
use logos_import_pdf::parser::build_test_pdf;
use logos_import_pdf::{import_pdf, import_pdf_with_options, PdfImporter};
use logos_import_common::{ImportOptions, Importer};

#[test]
fn import_empty_pdf_has_known_page_name() {
    let data = build_test_pdf(&[], 595.0, 842.0);
    let doc = import_pdf(&data).unwrap();
    let page = doc.root.read().unwrap();
    assert!(!page.name.is_empty());
}

#[test]
fn import_empty_pdf_has_no_layers() {
    let data = build_test_pdf(&[], 595.0, 842.0);
    let doc = import_pdf(&data).unwrap();
    let page = doc.root.read().unwrap();
    assert_eq!(page.layers.len(), 0);
}

#[test]
fn import_rect_creates_rect_layer() {
    let elems = [PdfElement::Rect { x: 0.0, y: 0.0, width: 200.0, height: 100.0 }];
    let data = build_test_pdf(&elems, 595.0, 842.0);
    let doc = import_pdf(&data).unwrap();
    let page = doc.root.read().unwrap();
    assert_eq!(page.layers.len(), 1);
    match &page.layers[0] {
        logos_core::Layer::Rect(r) => {
            assert!((r.bounds.width - 200.0).abs() < 1e-3);
            assert!((r.bounds.height - 100.0).abs() < 1e-3);
        }
        _ => panic!("expected Rect layer"),
    }
}

#[test]
fn import_text_creates_text_layer() {
    let elems = [PdfElement::Text {
        content: "PDF Text".to_string(),
        x: 50.0,
        y: 100.0,
        font_size: 12.0,
    }];
    let data = build_test_pdf(&elems, 595.0, 842.0);
    let doc = import_pdf(&data).unwrap();
    let page = doc.root.read().unwrap();
    match &page.layers[0] {
        logos_core::Layer::Text(t) => assert_eq!(t.content, "PDF Text"),
        _ => panic!("expected Text layer"),
    }
}

#[test]
fn import_mixed_elements_all_converted() {
    let elems = [
        PdfElement::Rect { x: 0.0, y: 0.0, width: 100.0, height: 50.0 },
        PdfElement::Text { content: "Hi".to_string(), x: 0.0, y: 60.0, font_size: 12.0 },
        PdfElement::Rect { x: 110.0, y: 0.0, width: 80.0, height: 80.0 },
    ];
    let data = build_test_pdf(&elems, 595.0, 842.0);
    let doc = import_pdf(&data).unwrap();
    let page = doc.root.read().unwrap();
    assert_eq!(page.layers.len(), 3);
}

#[test]
fn import_rect_position_preserved() {
    let elems = [PdfElement::Rect { x: 30.0, y: 40.0, width: 100.0, height: 60.0 }];
    let data = build_test_pdf(&elems, 595.0, 842.0);
    let doc = import_pdf(&data).unwrap();
    let page = doc.root.read().unwrap();
    match &page.layers[0] {
        logos_core::Layer::Rect(r) => {
            assert!((r.bounds.x - 30.0).abs() < 1e-3);
            
        }
        _ => panic!("expected Rect"),
    }
}

#[test]
fn import_invalid_bytes_returns_error() {
    assert!(import_pdf(b"not a pdf").is_err());
}

#[test]
fn import_with_default_options_succeeds() {
    let elems = [PdfElement::Rect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 }];
    let data = build_test_pdf(&elems, 595.0, 842.0);
    assert!(import_pdf_with_options(&data, &ImportOptions::default()).is_ok());
}

#[test]
fn import_with_fast_options_succeeds() {
    let elems = [PdfElement::Rect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 }];
    let data = build_test_pdf(&elems, 595.0, 842.0);
    assert!(import_pdf_with_options(&data, &ImportOptions::fast()).is_ok());
}

#[test]
fn import_with_preview_options_succeeds() {
    let elems = [PdfElement::Rect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 }];
    let data = build_test_pdf(&elems, 595.0, 842.0);
    assert!(import_pdf_with_options(&data, &ImportOptions::preview()).is_ok());
}

#[test]
fn pdf_importer_name_is_pdf() {
    assert_eq!(PdfImporter.name(), "pdf");
}

#[test]
fn pdf_importer_handles_pdf_extension() {
    assert!(PdfImporter.extensions().contains(&"pdf"));
}

#[test]
fn pdf_importer_can_handle_pdf() {
    assert!(PdfImporter.can_handle("pdf"));
}

#[test]
fn pdf_importer_cannot_handle_svg() {
    assert!(!PdfImporter.can_handle("svg"));
}

#[test]
fn pdf_importer_import_method_works() {
    let data = build_test_pdf(&[], 595.0, 842.0);
    assert!(PdfImporter.import(&data, &ImportOptions::default()).is_ok());
}

#[test]
fn build_test_pdf_produces_non_empty_bytes() {
    assert!(!build_test_pdf(&[], 100.0, 100.0).is_empty());
}

#[test]
fn import_many_rects_all_converted() {
    let elems: Vec<PdfElement> = (0..10).map(|i| PdfElement::Rect {
        x: (i as f32) * 60.0,
        y: 0.0,
        width: 50.0,
        height: 50.0,
    }).collect();
    let data = build_test_pdf(&elems, 800.0, 600.0);
    let doc = import_pdf(&data).unwrap();
    let page = doc.root.read().unwrap();
    assert_eq!(page.layers.len(), 10);
}
