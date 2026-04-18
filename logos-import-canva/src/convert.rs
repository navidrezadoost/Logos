//! Canva to logos-core Document conversion.

use logos_core::*;
use logos_import_common::error::{ImportError, ImportResult};
use logos_import_common::options::ImportOptions;

use crate::model::{CanvaDocument, CanvaElement};

/// Convert a parsed Canva document into a logos-core [`Document`].
pub fn convert_canva(canva: &CanvaDocument, _options: &ImportOptions) -> ImportResult<Document> {
    let doc = Document::new();

    {
        let mut page = doc.root.write().map_err(|e| {
            ImportError::ConversionError(e.to_string())
        })?;

        page.name = canva.name.clone();

        for element in &canva.elements {
            if let Some(layer) = convert_element(element) {
                page.layers.push(layer);
            }
        }
    }

    Ok(doc)
}

fn convert_element(element: &CanvaElement) -> Option<Layer> {
    if !element.visible {
        return None;
    }

    match element.element_type.as_str() {
        "rect" | "rectangle" => convert_rect(element),
        "ellipse" | "circle" => convert_ellipse(element),
        "text" => convert_text(element),
        "image" | "photo" => convert_image(element),
        "group" | "frame" => convert_group(element),
        "line" => convert_line(element),
        _ => {
            // Unknown type: render as rect if it has dimensions
            if element.width > 0.0 && element.height > 0.0 {
                convert_rect(element)
            } else {
                None
            }
        }
    }
}

fn convert_rect(element: &CanvaElement) -> Option<Layer> {
    Some(Layer::Rect(RectLayer {
        id: uuid::Uuid::new_v4(),
        bounds: Rect {
            x: element.x as f32,
            y: element.y as f32,
            width: element.width as f32,
            height: element.height as f32,
        },
        corner_radius: 0.0,
        corner_smoothing: 0.0,
    }))
}

fn convert_ellipse(element: &CanvaElement) -> Option<Layer> {
    Some(Layer::Ellipse(EllipseLayer {
        id: uuid::Uuid::new_v4(),
        bounds: Rect {
            x: element.x as f32,
            y: element.y as f32,
            width: element.width as f32,
            height: element.height as f32,
        },
    }))
}

fn convert_text(element: &CanvaElement) -> Option<Layer> {
    let content = if element.text.is_empty() {
        element.name.clone()
    } else {
        element.text.clone()
    };

    Some(Layer::Text(TextLayer {
        id: uuid::Uuid::new_v4(),
        content,
        bounds: Rect {
            x: element.x as f32,
            y: element.y as f32,
            width: element.width as f32,
            height: element.height as f32,
        },
    }))
}

fn convert_image(element: &CanvaElement) -> Option<Layer> {
    // Images are represented as placeholder rects
    Some(Layer::Rect(RectLayer {
        id: uuid::Uuid::new_v4(),
        bounds: Rect {
            x: element.x as f32,
            y: element.y as f32,
            width: element.width as f32,
            height: element.height as f32,
        },
        corner_radius: 0.0,
        corner_smoothing: 0.0,
    }))
}

fn convert_group(element: &CanvaElement) -> Option<Layer> {
    let mut children = Vec::new();
    for child in &element.children {
        if let Some(layer) = convert_element(child) {
            children.push(layer);
        }
    }

    Some(Layer::Frame(FrameLayer {
        id: uuid::Uuid::new_v4(),
        children,
        bounds: Rect {
            x: element.x as f32,
            y: element.y as f32,
            width: element.width as f32,
            height: element.height as f32,
        },
    }))
}

fn convert_line(element: &CanvaElement) -> Option<Layer> {
    let x = element.x as f32;
    let y = element.y as f32;
    let commands = vec![
        PathCommand::MoveTo(Point::new(x, y)),
        PathCommand::LineTo(Point::new(x + element.width as f32, y + element.height as f32)),
    ];
    Some(Layer::Path(PathLayer::new(commands)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    fn make_doc(elements: Vec<CanvaElement>) -> CanvaDocument {
        CanvaDocument::new("Test", 800.0, 600.0, elements)
    }

    #[test]
    fn test_convert_rect() {
        let doc = make_doc(vec![CanvaElement::rect("bg", 0.0, 0.0, 800.0, 600.0)]);
        let result = convert_canva(&doc, &ImportOptions::default()).unwrap();
        let page = result.root.read().unwrap();
        assert_eq!(page.layers.len(), 1);
        match &page.layers[0] {
            Layer::Rect(r) => {
                assert_eq!(r.bounds.width, 800.0);
                assert_eq!(r.bounds.height, 600.0);
            }
            _ => panic!("expected rect"),
        }
    }

    #[test]
    fn test_convert_ellipse() {
        let doc = make_doc(vec![CanvaElement::ellipse("e1", 10.0, 10.0, 100.0, 80.0)]);
        let result = convert_canva(&doc, &ImportOptions::default()).unwrap();
        let page = result.root.read().unwrap();
        match &page.layers[0] {
            Layer::Ellipse(e) => assert_eq!(e.bounds.width, 100.0),
            _ => panic!("expected ellipse"),
        }
    }

    #[test]
    fn test_convert_text() {
        let doc = make_doc(vec![CanvaElement::text("t1", 10.0, 20.0, 200.0, 30.0, "Hi")]);
        let result = convert_canva(&doc, &ImportOptions::default()).unwrap();
        let page = result.root.read().unwrap();
        match &page.layers[0] {
            Layer::Text(t) => assert_eq!(t.content, "Hi"),
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn test_convert_group() {
        let doc = make_doc(vec![CanvaElement::group("grp", vec![
            CanvaElement::rect("a", 0.0, 0.0, 50.0, 50.0),
            CanvaElement::rect("b", 60.0, 0.0, 50.0, 50.0),
        ])]);
        let result = convert_canva(&doc, &ImportOptions::default()).unwrap();
        let page = result.root.read().unwrap();
        match &page.layers[0] {
            Layer::Frame(f) => assert_eq!(f.children.len(), 2),
            _ => panic!("expected frame"),
        }
    }

    #[test]
    fn test_convert_invisible_skipped() {
        let mut e = CanvaElement::rect("hidden", 0.0, 0.0, 100.0, 100.0);
        e.visible = false;
        let doc = make_doc(vec![e]);
        let result = convert_canva(&doc, &ImportOptions::default()).unwrap();
        let page = result.root.read().unwrap();
        assert_eq!(page.layers.len(), 0);
    }

    #[test]
    fn test_convert_image() {
        let doc = make_doc(vec![CanvaElement::image("photo", 50.0, 50.0, 300.0, 200.0)]);
        let result = convert_canva(&doc, &ImportOptions::default()).unwrap();
        let page = result.root.read().unwrap();
        match &page.layers[0] {
            Layer::Rect(r) => {
                assert_eq!(r.bounds.width, 300.0);
            }
            _ => panic!("expected rect for image"),
        }
    }

    #[test]
    fn test_convert_empty() {
        let doc = make_doc(vec![]);
        let result = convert_canva(&doc, &ImportOptions::default()).unwrap();
        let page = result.root.read().unwrap();
        assert!(page.layers.is_empty());
    }
}
