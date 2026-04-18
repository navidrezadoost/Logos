//! XD to logos-core Document conversion.

use logos_core::*;
use logos_import_common::error::{ImportError, ImportResult};
use logos_import_common::options::ImportOptions;

use crate::model::{XdDocument, XdNode};

/// Convert a parsed XD document into a logos-core [`Document`].
pub fn convert_xd(xd: &XdDocument, _options: &ImportOptions) -> ImportResult<Document> {
    let doc = Document::new();

    {
        let mut page = doc.root.write().map_err(|e| {
            ImportError::ConversionError(e.to_string())
        })?;

        if let Some(artboard) = xd.artboards.first() {
            page.name = artboard.name.clone();

            for node in &artboard.children {
                if let Some(layer) = convert_node(node) {
                    page.layers.push(layer);
                }
            }
        }
    }

    Ok(doc)
}

fn convert_node(node: &XdNode) -> Option<Layer> {
    if !node.visible {
        return None;
    }

    match node.node_type.as_str() {
        "shape" => convert_shape(node),
        "text" => convert_text(node),
        "group" | "artboard" => convert_group(node),
        _ => {
            // Unknown type: try as shape-like if it has bounds
            if node.bounds.width > 0.0 || node.bounds.height > 0.0 {
                convert_shape(node)
            } else if !node.children.is_empty() {
                convert_group(node)
            } else {
                None
            }
        }
    }
}

fn convert_shape(node: &XdNode) -> Option<Layer> {
    let bounds = Rect {
        x: node.bounds.x as f32,
        y: node.bounds.y as f32,
        width: node.bounds.width as f32,
        height: node.bounds.height as f32,
    };

    match node.shape_type.as_str() {
        "ellipse" | "circle" => {
            Some(Layer::Ellipse(EllipseLayer {
                id: uuid::Uuid::new_v4(),
                bounds,
            }))
        }
        "line" => {
            let commands = vec![
                PathCommand::MoveTo(Point::new(bounds.x, bounds.y)),
                PathCommand::LineTo(Point::new(
                    bounds.x + bounds.width,
                    bounds.y + bounds.height,
                )),
            ];
            Some(Layer::Path(PathLayer::new(commands)))
        }
        "path" => {
            // Without actual path data, approximate as a rectangle
            Some(Layer::Rect(RectLayer {
                id: uuid::Uuid::new_v4(),
                bounds,
                corner_radius: 0.0,
                corner_smoothing: 0.0,
            }))
        }
        _ => {
            // rect, or unknown → rectangle
            Some(Layer::Rect(RectLayer {
                id: uuid::Uuid::new_v4(),
                bounds,
                corner_radius: 0.0,
                corner_smoothing: 0.0,
            }))
        }
    }
}

fn convert_text(node: &XdNode) -> Option<Layer> {
    let content = if node.text_content.is_empty() {
        node.name.clone()
    } else {
        node.text_content.clone()
    };

    Some(Layer::Text(TextLayer {
        id: uuid::Uuid::new_v4(),
        content,
        bounds: Rect {
            x: node.bounds.x as f32,
            y: node.bounds.y as f32,
            width: node.bounds.width as f32,
            height: node.bounds.height as f32,
        },
    }))
}

fn convert_group(node: &XdNode) -> Option<Layer> {
    let mut children = Vec::new();
    for child in &node.children {
        if let Some(layer) = convert_node(child) {
            children.push(layer);
        }
    }

    Some(Layer::Frame(FrameLayer {
        id: uuid::Uuid::new_v4(),
        children,
        bounds: Rect {
            x: node.bounds.x as f32,
            y: node.bounds.y as f32,
            width: node.bounds.width as f32,
            height: node.bounds.height as f32,
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    fn make_doc(nodes: Vec<XdNode>) -> XdDocument {
        XdDocument {
            artboards: vec![XdArtboard {
                name: "Test".into(),
                width: 375.0,
                height: 812.0,
                children: nodes,
            }],
        }
    }

    #[test]
    fn test_convert_rect() {
        let doc = make_doc(vec![XdNode::rect("bg", 0.0, 0.0, 375.0, 812.0)]);
        let result = convert_xd(&doc, &ImportOptions::default()).unwrap();
        let page = result.root.read().unwrap();
        assert_eq!(page.layers.len(), 1);
        match &page.layers[0] {
            Layer::Rect(r) => {
                assert_eq!(r.bounds.width, 375.0);
                assert_eq!(r.bounds.height, 812.0);
            }
            _ => panic!("expected rect"),
        }
    }

    #[test]
    fn test_convert_ellipse() {
        let doc = make_doc(vec![XdNode::ellipse("e1", 10.0, 10.0, 100.0, 80.0)]);
        let result = convert_xd(&doc, &ImportOptions::default()).unwrap();
        let page = result.root.read().unwrap();
        match &page.layers[0] {
            Layer::Ellipse(e) => assert_eq!(e.bounds.width, 100.0),
            _ => panic!("expected ellipse"),
        }
    }

    #[test]
    fn test_convert_text() {
        let doc = make_doc(vec![XdNode::text("t1", 10.0, 20.0, 200.0, 30.0, "Hello XD")]);
        let result = convert_xd(&doc, &ImportOptions::default()).unwrap();
        let page = result.root.read().unwrap();
        match &page.layers[0] {
            Layer::Text(t) => assert_eq!(t.content, "Hello XD"),
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn test_convert_group() {
        let doc = make_doc(vec![XdNode::group("grp", vec![
            XdNode::rect("a", 0.0, 0.0, 50.0, 50.0),
            XdNode::rect("b", 60.0, 0.0, 50.0, 50.0),
        ])]);
        let result = convert_xd(&doc, &ImportOptions::default()).unwrap();
        let page = result.root.read().unwrap();
        match &page.layers[0] {
            Layer::Frame(f) => assert_eq!(f.children.len(), 2),
            _ => panic!("expected frame"),
        }
    }

    #[test]
    fn test_convert_invisible_skipped() {
        let mut node = XdNode::rect("hidden", 0.0, 0.0, 100.0, 100.0);
        node.visible = false;
        let doc = make_doc(vec![node]);
        let result = convert_xd(&doc, &ImportOptions::default()).unwrap();
        let page = result.root.read().unwrap();
        assert_eq!(page.layers.len(), 0);
    }

    #[test]
    fn test_convert_empty() {
        let doc = XdDocument { artboards: vec![] };
        let result = convert_xd(&doc, &ImportOptions::default()).unwrap();
        let page = result.root.read().unwrap();
        assert!(page.layers.is_empty());
    }

    #[test]
    fn test_convert_line() {
        let doc = make_doc(vec![XdNode::line("ln", 0.0, 0.0, 100.0, 50.0)]);
        let result = convert_xd(&doc, &ImportOptions::default()).unwrap();
        let page = result.root.read().unwrap();
        match &page.layers[0] {
            Layer::Path(p) => assert_eq!(p.commands.len(), 2),
            _ => panic!("expected path for line"),
        }
    }
}
