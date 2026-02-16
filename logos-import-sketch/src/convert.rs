//! Sketch to logos-core Document conversion.

use logos_core::*;
use logos_import_common::{ImportOptions, ImportResult, ImportError};
use crate::model::{SketchDocument, SketchLayer};

/// Convert a parsed Sketch document to a logos-core Document.
pub fn convert_sketch(
    sketch: &SketchDocument,
    _options: &ImportOptions,
) -> ImportResult<Document> {
    let doc = Document::new();

    if let Some(first_page) = sketch.pages.first() {
        let mut page = doc.root.write().map_err(|e| {
            ImportError::ConversionError(e.to_string())
        })?;
        page.name = first_page.name.clone();

        for layer in &first_page.layers {
            if let Some(l) = convert_layer(layer) {
                page.layers.push(l);
            }
        }
    }

    Ok(doc)
}

fn convert_layer(sketch_layer: &SketchLayer) -> Option<Layer> {
    if !sketch_layer.isVisible {
        return None;
    }

    match sketch_layer.class.as_str() {
        "rectangle" | "shapePath" | "shapeGroup" => convert_rect(sketch_layer),
        "oval" => convert_oval(sketch_layer),
        "text" => convert_text(sketch_layer),
        "group" => convert_group(sketch_layer),
        "artboard" | "symbolMaster" => convert_artboard(sketch_layer),
        "bitmap" => convert_rect(sketch_layer), // treat images as rects
        "slice" => None,                        // skip slices
        _ => {
            // Unknown type: try as rect if it has frame
            if sketch_layer.frame.width > 0.0 && sketch_layer.frame.height > 0.0 {
                convert_rect(sketch_layer)
            } else {
                None
            }
        }
    }
}

fn convert_rect(layer: &SketchLayer) -> Option<Layer> {
    Some(Layer::Rect(RectLayer {
        id: uuid::Uuid::new_v4(),
        bounds: Rect {
            x: layer.frame.x as f32,
            y: layer.frame.y as f32,
            width: layer.frame.width as f32,
            height: layer.frame.height as f32,
        },
    }))
}

fn convert_oval(layer: &SketchLayer) -> Option<Layer> {
    Some(Layer::Ellipse(EllipseLayer {
        id: uuid::Uuid::new_v4(),
        bounds: Rect {
            x: layer.frame.x as f32,
            y: layer.frame.y as f32,
            width: layer.frame.width as f32,
            height: layer.frame.height as f32,
        },
    }))
}

fn convert_text(layer: &SketchLayer) -> Option<Layer> {
    let content = layer
        .attributed_string
        .as_ref()
        .map(|s| s.string.clone())
        .unwrap_or_default();

    Some(Layer::Text(TextLayer {
        id: uuid::Uuid::new_v4(),
        content,
        bounds: Rect {
            x: layer.frame.x as f32,
            y: layer.frame.y as f32,
            width: layer.frame.width as f32,
            height: layer.frame.height as f32,
        },
    }))
}

fn convert_group(layer: &SketchLayer) -> Option<Layer> {
    let mut children = Vec::new();
    for child in &layer.layers {
        if let Some(l) = convert_layer(child) {
            children.push(l);
        }
    }

    Some(Layer::Frame(FrameLayer {
        id: uuid::Uuid::new_v4(),
        children,
        bounds: Rect {
            x: layer.frame.x as f32,
            y: layer.frame.y as f32,
            width: layer.frame.width as f32,
            height: layer.frame.height as f32,
        },
    }))
}

fn convert_artboard(layer: &SketchLayer) -> Option<Layer> {
    let mut children = Vec::new();
    for child in &layer.layers {
        if let Some(l) = convert_layer(child) {
            children.push(l);
        }
    }

    Some(Layer::Frame(FrameLayer {
        id: uuid::Uuid::new_v4(),
        children,
        bounds: Rect {
            x: layer.frame.x as f32,
            y: layer.frame.y as f32,
            width: layer.frame.width as f32,
            height: layer.frame.height as f32,
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    fn make_doc(layers: Vec<SketchLayer>) -> SketchDocument {
        SketchDocument {
            id: "doc-1".into(),
            class: "document".into(),
            pages: vec![SketchPage {
                id: "page-1".into(),
                name: "Test Page".into(),
                class: "page".into(),
                layers,
                frame: SketchFrame::default(),
            }],
        }
    }

    #[test]
    fn test_convert_rect() {
        let doc = make_doc(vec![SketchLayer::rect("1", "R", 5.0, 10.0, 100.0, 50.0)]);
        let result = convert_sketch(&doc, &ImportOptions::default()).unwrap();
        let page = result.root.read().unwrap();
        assert_eq!(page.layers.len(), 1);
        match &page.layers[0] {
            Layer::Rect(r) => {
                assert_eq!(r.bounds.x, 5.0);
                assert_eq!(r.bounds.width, 100.0);
            }
            _ => panic!("expected rect"),
        }
    }

    #[test]
    fn test_convert_oval() {
        let doc = make_doc(vec![SketchLayer::oval("1", "O", 0.0, 0.0, 80.0, 80.0)]);
        let result = convert_sketch(&doc, &ImportOptions::default()).unwrap();
        let page = result.root.read().unwrap();
        match &page.layers[0] {
            Layer::Ellipse(e) => assert_eq!(e.bounds.width, 80.0),
            _ => panic!("expected ellipse"),
        }
    }

    #[test]
    fn test_convert_text() {
        let doc = make_doc(vec![SketchLayer::text("1", "T", 0.0, 0.0, 100.0, 20.0, "Hi")]);
        let result = convert_sketch(&doc, &ImportOptions::default()).unwrap();
        let page = result.root.read().unwrap();
        match &page.layers[0] {
            Layer::Text(t) => assert_eq!(t.content, "Hi"),
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn test_convert_group() {
        let doc = make_doc(vec![SketchLayer::group("1", "G", vec![
            SketchLayer::rect("2", "R1", 0.0, 0.0, 50.0, 50.0),
            SketchLayer::rect("3", "R2", 60.0, 0.0, 50.0, 50.0),
        ])]);
        let result = convert_sketch(&doc, &ImportOptions::default()).unwrap();
        let page = result.root.read().unwrap();
        match &page.layers[0] {
            Layer::Frame(f) => assert_eq!(f.children.len(), 2),
            _ => panic!("expected frame"),
        }
    }

    #[test]
    fn test_convert_artboard() {
        let doc = make_doc(vec![SketchLayer::artboard(
            "1",
            "Artboard",
            0.0,
            0.0,
            375.0,
            812.0,
            vec![SketchLayer::rect("2", "BG", 0.0, 0.0, 375.0, 812.0)],
        )]);
        let result = convert_sketch(&doc, &ImportOptions::default()).unwrap();
        let page = result.root.read().unwrap();
        match &page.layers[0] {
            Layer::Frame(f) => {
                assert_eq!(f.bounds.width, 375.0);
                assert_eq!(f.children.len(), 1);
            }
            _ => panic!("expected frame"),
        }
    }

    #[test]
    fn test_convert_invisible_skipped() {
        let mut layer = SketchLayer::rect("1", "R", 0.0, 0.0, 100.0, 100.0);
        layer.isVisible = false;
        let doc = make_doc(vec![layer]);
        let result = convert_sketch(&doc, &ImportOptions::default()).unwrap();
        let page = result.root.read().unwrap();
        assert_eq!(page.layers.len(), 0);
    }

    #[test]
    fn test_convert_empty_doc() {
        let doc = SketchDocument {
            id: "1".into(),
            class: "document".into(),
            pages: vec![],
        };
        let result = convert_sketch(&doc, &ImportOptions::default()).unwrap();
        let page = result.root.read().unwrap();
        assert_eq!(page.layers.len(), 0);
    }

    #[test]
    fn test_page_name() {
        let doc = make_doc(vec![]);
        let result = convert_sketch(&doc, &ImportOptions::default()).unwrap();
        let page = result.root.read().unwrap();
        assert_eq!(page.name, "Test Page");
    }
}
