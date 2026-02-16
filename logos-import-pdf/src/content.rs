//! PDF content stream element types.

/// An element extracted from a PDF content stream.
#[derive(Debug, Clone)]
pub enum PdfElement {
    /// A text element with position and font info.
    Text {
        content: String,
        x: f32,
        y: f32,
        font_size: f32,
    },
    /// A rectangle.
    Rect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
    /// A vector path.
    Path {
        commands: Vec<PathCmd>,
    },
    /// An image placeholder.
    Image {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
}

/// PDF path drawing commands.
#[derive(Debug, Clone)]
pub enum PathCmd {
    MoveTo(f32, f32),
    LineTo(f32, f32),
    CurveTo(f32, f32, f32, f32, f32, f32),
    Close,
}

/// A parsed PDF page with its elements and dimensions.
#[derive(Debug, Clone)]
pub struct PdfPage {
    pub width: f32,
    pub height: f32,
    pub elements: Vec<PdfElement>,
    pub page_number: usize,
}

/// A parsed PDF document.
#[derive(Debug, Clone)]
pub struct PdfDocument {
    pub pages: Vec<PdfPage>,
    pub version: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pdf_element_text() {
        let e = PdfElement::Text {
            content: "Hello".into(),
            x: 10.0,
            y: 20.0,
            font_size: 12.0,
        };
        match e {
            PdfElement::Text { content, .. } => assert_eq!(content, "Hello"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_pdf_element_rect() {
        let e = PdfElement::Rect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 50.0,
        };
        match e {
            PdfElement::Rect { width, height, .. } => {
                assert_eq!(width, 100.0);
                assert_eq!(height, 50.0);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_path_cmd() {
        let cmds = vec![
            PathCmd::MoveTo(0.0, 0.0),
            PathCmd::LineTo(100.0, 0.0),
            PathCmd::CurveTo(50.0, 50.0, 100.0, 100.0, 100.0, 100.0),
            PathCmd::Close,
        ];
        assert_eq!(cmds.len(), 4);
    }

    #[test]
    fn test_pdf_page() {
        let page = PdfPage {
            width: 612.0,
            height: 792.0,
            elements: vec![],
            page_number: 1,
        };
        assert_eq!(page.width, 612.0);
        assert_eq!(page.height, 792.0);
    }
}
