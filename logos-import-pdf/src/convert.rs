//! Convert parsed PDF elements to logos-core document model.

use logos_core::*;
use logos_import_common::error::{ImportError, ImportResult};
use logos_import_common::options::ImportOptions;

use crate::content::{PathCmd, PdfDocument, PdfElement};

/// Convert a parsed PDF document into a logos-core [`Document`].
///
/// Uses the first page's elements as the root page. Multi-page PDFs
/// have subsequent pages merged into the root as additional layers.
pub fn convert_pdf(pdf: &PdfDocument, options: &ImportOptions) -> ImportResult<Document> {
    let doc = Document::new();

    {
        let mut page = doc.root.write().map_err(|e| {
            ImportError::ConversionError(e.to_string())
        })?;

        page.name = "PDF Document".into();

        // Collect elements from all pages, tracking page height for Y-flip
        for pdf_page in &pdf.pages {
            let mut count = 0;
            for element in &pdf_page.elements {
                if options.max_elements > 0 && count >= options.max_elements {
                    break;
                }
                if let Some(layer) = convert_element(element, pdf_page.height) {
                    page.layers.push(layer);
                    count += 1;
                }
            }
        }
    }

    Ok(doc)
}

/// Convert a single PDF element to a logos-core layer.
///
/// PDF uses bottom-left origin; we flip y-coordinates to top-left.
fn convert_element(element: &PdfElement, page_height: f32) -> Option<Layer> {
    match element {
        PdfElement::Text {
            content,
            x,
            y,
            font_size,
        } => {
            // Flip Y: PDF origin is bottom-left
            let flipped_y = page_height - y;
            let est_width = content.len() as f32 * font_size * 0.6;

            Some(Layer::Text(TextLayer {
                id: uuid::Uuid::new_v4(),
                content: content.clone(),
                bounds: Rect {
                    x: *x,
                    y: flipped_y,
                    width: est_width,
                    height: *font_size * 1.2,
                },
            }))
        }
        PdfElement::Rect {
            x,
            y,
            width,
            height,
        } => {
            let flipped_y = page_height - y - height;
            Some(Layer::Rect(RectLayer {
                id: uuid::Uuid::new_v4(),
                bounds: Rect {
                    x: *x,
                    y: flipped_y,
                    width: *width,
                    height: *height,
                },
            }))
        }
        PdfElement::Path { commands } => {
            if commands.is_empty() {
                return None;
            }
            let core_commands = convert_path_commands(commands, page_height);
            if core_commands.is_empty() {
                return None;
            }

            Some(Layer::Path(PathLayer::new(core_commands)))
        }
        PdfElement::Image {
            x,
            y,
            width,
            height,
        } => {
            let flipped_y = page_height - y - height;
            Some(Layer::Rect(RectLayer {
                id: uuid::Uuid::new_v4(),
                bounds: Rect {
                    x: *x,
                    y: flipped_y,
                    width: *width,
                    height: *height,
                },
            }))
        }
    }
}

/// Convert PDF path commands to logos-core PathCommands, flipping Y.
fn convert_path_commands(commands: &[PathCmd], page_height: f32) -> Vec<PathCommand> {
    let mut out = Vec::new();

    for cmd in commands {
        match cmd {
            PathCmd::MoveTo(x, y) => {
                out.push(PathCommand::MoveTo(Point::new(*x, page_height - y)));
            }
            PathCmd::LineTo(x, y) => {
                out.push(PathCommand::LineTo(Point::new(*x, page_height - y)));
            }
            PathCmd::CurveTo(x1, y1, x2, y2, x3, y3) => {
                out.push(PathCommand::BezierTo {
                    cp1: Point::new(*x1, page_height - y1),
                    cp2: Point::new(*x2, page_height - y2),
                    end: Point::new(*x3, page_height - y3),
                });
            }
            PathCmd::Close => {
                out.push(PathCommand::Close);
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{PdfDocument, PdfElement, PdfPage, PathCmd};

    fn make_doc(elements: Vec<PdfElement>) -> PdfDocument {
        PdfDocument {
            pages: vec![PdfPage {
                width: 612.0,
                height: 792.0,
                elements,
                page_number: 1,
            }],
            version: "1.4".into(),
        }
    }

    #[test]
    fn test_convert_empty() {
        let doc = make_doc(vec![]);
        let result = convert_pdf(&doc, &ImportOptions::full()).unwrap();
        let page = result.root.read().unwrap();
        assert!(page.layers.is_empty());
    }

    #[test]
    fn test_convert_text() {
        let doc = make_doc(vec![PdfElement::Text {
            content: "Hello".into(),
            x: 72.0,
            y: 720.0,
            font_size: 12.0,
        }]);
        let result = convert_pdf(&doc, &ImportOptions::full()).unwrap();
        let page = result.root.read().unwrap();
        assert_eq!(page.layers.len(), 1);
        match &page.layers[0] {
            Layer::Text(t) => {
                assert_eq!(t.content, "Hello");
                assert_eq!(t.bounds.x, 72.0);
                // Y flipped: 792 - 720 = 72
                assert!((t.bounds.y - 72.0).abs() < 0.01);
            }
            _ => panic!("Expected Text layer"),
        }
    }

    #[test]
    fn test_convert_rect() {
        let doc = make_doc(vec![PdfElement::Rect {
            x: 10.0,
            y: 10.0,
            width: 100.0,
            height: 50.0,
        }]);
        let result = convert_pdf(&doc, &ImportOptions::full()).unwrap();
        let page = result.root.read().unwrap();
        assert_eq!(page.layers.len(), 1);
        match &page.layers[0] {
            Layer::Rect(r) => {
                assert_eq!(r.bounds.width, 100.0);
                assert_eq!(r.bounds.height, 50.0);
            }
            _ => panic!("Expected Rect layer"),
        }
    }

    #[test]
    fn test_convert_path() {
        let doc = make_doc(vec![PdfElement::Path {
            commands: vec![
                PathCmd::MoveTo(0.0, 0.0),
                PathCmd::LineTo(100.0, 0.0),
                PathCmd::LineTo(100.0, 100.0),
                PathCmd::Close,
            ],
        }]);
        let result = convert_pdf(&doc, &ImportOptions::full()).unwrap();
        let page = result.root.read().unwrap();
        assert_eq!(page.layers.len(), 1);
        match &page.layers[0] {
            Layer::Path(p) => {
                assert_eq!(p.commands.len(), 4);
                assert!(p.closed);
            }
            _ => panic!("Expected Path layer"),
        }
    }

    #[test]
    fn test_max_elements_limit() {
        let elements: Vec<PdfElement> = (0..100)
            .map(|i| PdfElement::Rect {
                x: i as f32,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            })
            .collect();
        let doc = make_doc(elements);
        let mut opts = ImportOptions::full();
        opts.max_elements = 5;
        let result = convert_pdf(&doc, &opts).unwrap();
        let page = result.root.read().unwrap();
        assert_eq!(page.layers.len(), 5);
    }

    #[test]
    fn test_convert_image_placeholder() {
        let doc = make_doc(vec![PdfElement::Image {
            x: 50.0,
            y: 50.0,
            width: 200.0,
            height: 150.0,
        }]);
        let result = convert_pdf(&doc, &ImportOptions::full()).unwrap();
        let page = result.root.read().unwrap();
        assert_eq!(page.layers.len(), 1);
        match &page.layers[0] {
            Layer::Rect(r) => {
                assert_eq!(r.bounds.width, 200.0);
                assert_eq!(r.bounds.height, 150.0);
            }
            _ => panic!("Expected Rect layer for image"),
        }
    }

    #[test]
    fn test_multi_page_elements_merged() {
        let doc = PdfDocument {
            pages: vec![
                PdfPage {
                    width: 612.0,
                    height: 792.0,
                    elements: vec![PdfElement::Text {
                        content: "Page 1".into(),
                        x: 72.0,
                        y: 700.0,
                        font_size: 12.0,
                    }],
                    page_number: 1,
                },
                PdfPage {
                    width: 612.0,
                    height: 792.0,
                    elements: vec![PdfElement::Text {
                        content: "Page 2".into(),
                        x: 72.0,
                        y: 700.0,
                        font_size: 12.0,
                    }],
                    page_number: 2,
                },
            ],
            version: "1.4".into(),
        };
        let result = convert_pdf(&doc, &ImportOptions::full()).unwrap();
        let page = result.root.read().unwrap();
        // Both pages' elements merged into single root page
        assert_eq!(page.layers.len(), 2);
    }
}
