//! SVG to logos-core Document conversion.

use logos_core::*;
use logos_import_common::{ImportOptions, ImportResult};
use crate::parser::{SvgNode, parse_color, parse_points};
use crate::path_data::parse_path_data;

/// Convert a parsed SVG tree to a logos-core Document.
pub fn convert_svg(root: &SvgNode, _options: &ImportOptions) -> ImportResult<Document> {
    let doc = Document::new();

    {
        let mut page = doc.root.write().map_err(|e| {
            logos_import_common::ImportError::ConversionError(e.to_string())
        })?;

        page.name = root
            .attr("id")
            .unwrap_or("SVG Document")
            .to_string();

        for child in &root.children {
            if let Some(layer) = convert_element(child) {
                page.layers.push(layer);
            }
        }
    }

    Ok(doc)
}

fn convert_element(node: &SvgNode) -> Option<Layer> {
    match node.tag.as_str() {
        "rect" => convert_rect(node),
        "circle" => convert_circle(node),
        "ellipse" => convert_ellipse(node),
        "line" => convert_line(node),
        "polyline" => convert_polyline(node),
        "polygon" => convert_polygon(node),
        "path" => convert_path(node),
        "text" => convert_text(node),
        "g" | "svg" => convert_group(node),
        "defs" | "style" | "title" | "desc" | "metadata" |
        "linearGradient" | "radialGradient" | "clipPath" | "mask" |
        "filter" | "symbol" | "use" | "marker" | "pattern" => None,
        _ => {
            // Unknown element — try to convert children as a group
            if !node.children.is_empty() {
                convert_group(node)
            } else {
                None
            }
        }
    }
}

fn convert_rect(node: &SvgNode) -> Option<Layer> {
    let x = node.attr_f32("x");
    let y = node.attr_f32("y");
    let w = node.attr_f32("width");
    let h = node.attr_f32("height");

    if w <= 0.0 || h <= 0.0 {
        return None;
    }

    Some(Layer::Rect(RectLayer {
        id: uuid::Uuid::new_v4(),
        bounds: Rect {
            x,
            y,
            width: w,
            height: h,
        },
        corner_radius: 0.0,
        corner_smoothing: 0.0,
    }))
}

fn convert_circle(node: &SvgNode) -> Option<Layer> {
    let cx = node.attr_f32("cx");
    let cy = node.attr_f32("cy");
    let r = node.attr_f32("r");

    if r <= 0.0 {
        return None;
    }

    Some(Layer::Ellipse(EllipseLayer {
        id: uuid::Uuid::new_v4(),
        bounds: Rect {
            x: cx - r,
            y: cy - r,
            width: r * 2.0,
            height: r * 2.0,
        },
    }))
}

fn convert_ellipse(node: &SvgNode) -> Option<Layer> {
    let cx = node.attr_f32("cx");
    let cy = node.attr_f32("cy");
    let rx = node.attr_f32("rx");
    let ry = node.attr_f32("ry");

    if rx <= 0.0 || ry <= 0.0 {
        return None;
    }

    Some(Layer::Ellipse(EllipseLayer {
        id: uuid::Uuid::new_v4(),
        bounds: Rect {
            x: cx - rx,
            y: cy - ry,
            width: rx * 2.0,
            height: ry * 2.0,
        },
    }))
}

fn convert_line(node: &SvgNode) -> Option<Layer> {
    let x1 = node.attr_f32("x1");
    let y1 = node.attr_f32("y1");
    let x2 = node.attr_f32("x2");
    let y2 = node.attr_f32("y2");

    let commands = vec![
        PathCommand::MoveTo(Point::new(x1, y1)),
        PathCommand::LineTo(Point::new(x2, y2)),
    ];

    Some(Layer::Path(PathLayer::new(commands)))
}

fn convert_polyline(node: &SvgNode) -> Option<Layer> {
    let points_str = node.attr("points")?;
    let points = parse_points(points_str);
    if points.is_empty() {
        return None;
    }

    let mut commands = Vec::with_capacity(points.len());
    for (i, (x, y)) in points.iter().enumerate() {
        if i == 0 {
            commands.push(PathCommand::MoveTo(Point::new(*x, *y)));
        } else {
            commands.push(PathCommand::LineTo(Point::new(*x, *y)));
        }
    }

    Some(Layer::Path(PathLayer::new(commands)))
}

fn convert_polygon(node: &SvgNode) -> Option<Layer> {
    let points_str = node.attr("points")?;
    let points = parse_points(points_str);
    if points.is_empty() {
        return None;
    }

    let mut commands = Vec::with_capacity(points.len() + 1);
    for (i, (x, y)) in points.iter().enumerate() {
        if i == 0 {
            commands.push(PathCommand::MoveTo(Point::new(*x, *y)));
        } else {
            commands.push(PathCommand::LineTo(Point::new(*x, *y)));
        }
    }
    commands.push(PathCommand::Close);

    Some(Layer::Path(PathLayer::new(commands)))
}

fn convert_path(node: &SvgNode) -> Option<Layer> {
    let d = node.attr("d")?;
    let commands = parse_path_data(d).ok()?;
    if commands.is_empty() {
        return None;
    }

    Some(Layer::Path(PathLayer::new(commands)))
}

fn convert_text(node: &SvgNode) -> Option<Layer> {
    let x = node.attr_f32("x");
    let y = node.attr_f32("y");

    let content = if !node.text.is_empty() {
        node.text.clone()
    } else {
        // Collect text from child <tspan> elements
        node.children
            .iter()
            .filter(|c| c.tag == "tspan")
            .map(|c| c.text.clone())
            .collect::<Vec<_>>()
            .join(" ")
    };

    if content.is_empty() {
        return None;
    }

    // Estimate text bounds from font-size if available
    let font_size = node.attr_f32_opt("font-size").unwrap_or(16.0);
    let est_width = content.len() as f32 * font_size * 0.6;

    Some(Layer::Text(TextLayer {
        id: uuid::Uuid::new_v4(),
        content,
        bounds: Rect {
            x,
            y: y - font_size, // SVG text y is baseline
            width: est_width,
            height: font_size * 1.2,
        },
    }))
}

fn convert_group(node: &SvgNode) -> Option<Layer> {
    let mut children = Vec::new();
    for child in &node.children {
        if let Some(layer) = convert_element(child) {
            children.push(layer);
        }
    }

    if children.is_empty() {
        return None;
    }

    // Compute bounds from children
    let bounds = compute_children_bounds(&children);

    Some(Layer::Frame(FrameLayer {
        id: uuid::Uuid::new_v4(),
        children,
        bounds,
    }))
}

fn compute_children_bounds(layers: &[Layer]) -> Rect {
    if layers.is_empty() {
        return Rect::default();
    }

    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;

    for layer in layers {
        let b = layer_bounds(layer);
        min_x = min_x.min(b.x);
        min_y = min_y.min(b.y);
        max_x = max_x.max(b.x + b.width);
        max_y = max_y.max(b.y + b.height);
    }

    if min_x == f32::MAX {
        return Rect::default();
    }

    Rect {
        x: min_x,
        y: min_y,
        width: max_x - min_x,
        height: max_y - min_y,
    }
}

fn layer_bounds(layer: &Layer) -> &Rect {
    match layer {
        Layer::Rect(l) => &l.bounds,
        Layer::Ellipse(l) => &l.bounds,
        Layer::Text(l) => &l.bounds,
        Layer::Frame(l) => &l.bounds,
        Layer::Path(l) => &l.bounds,
        Layer::Artboard(a) => &a.bounds,
        Layer::Drawer(d) => &d.bounds,
        // Sections have no intrinsic bounds; fall back to zero-rect.
        Layer::Section(_) => &ZERO_RECT,
        Layer::Line(_) => &ZERO_RECT,
        Layer::Polygon(l) => &l.bounds,
        Layer::Star(l) => &l.bounds,
        Layer::BooleanGroup(l) => &l.bounds,
        Layer::VectorNetwork(l) => &l.bounds,        Layer::Image(l) => &l.bounds,
        Layer::Audio(l) => &l.bounds,
        Layer::Video(l) => &l.bounds,    }
}

static ZERO_RECT: Rect = Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 };

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_svg;

    #[test]
    fn test_convert_rect() {
        let svg = parse_svg(b"<svg><rect x=\"5\" y=\"10\" width=\"100\" height=\"50\"/></svg>").unwrap();
        let doc = convert_svg(&svg, &ImportOptions::default()).unwrap();
        let page = doc.root.read().unwrap();
        assert_eq!(page.layers.len(), 1);
        match &page.layers[0] {
            Layer::Rect(r) => {
                assert_eq!(r.bounds.x, 5.0);
                assert_eq!(r.bounds.y, 10.0);
                assert_eq!(r.bounds.width, 100.0);
            }
            _ => panic!("expected rect"),
        }
    }

    #[test]
    fn test_convert_circle() {
        let svg = parse_svg(b"<svg><circle cx=\"50\" cy=\"50\" r=\"25\"/></svg>").unwrap();
        let doc = convert_svg(&svg, &ImportOptions::default()).unwrap();
        let page = doc.root.read().unwrap();
        assert_eq!(page.layers.len(), 1);
        match &page.layers[0] {
            Layer::Ellipse(e) => {
                assert!((e.bounds.width - 50.0).abs() < 0.01);
                assert!((e.bounds.height - 50.0).abs() < 0.01);
            }
            _ => panic!("expected ellipse"),
        }
    }

    #[test]
    fn test_convert_zero_size_rect_skipped() {
        let svg = parse_svg(b"<svg><rect x=\"0\" y=\"0\" width=\"0\" height=\"50\"/></svg>").unwrap();
        let doc = convert_svg(&svg, &ImportOptions::default()).unwrap();
        let page = doc.root.read().unwrap();
        assert_eq!(page.layers.len(), 0);
    }

    #[test]
    fn test_convert_group() {
        let svg = parse_svg(
            b"<svg><g><rect x=\"0\" y=\"0\" width=\"50\" height=\"50\"/><rect x=\"60\" y=\"0\" width=\"50\" height=\"50\"/></g></svg>",
        )
        .unwrap();
        let doc = convert_svg(&svg, &ImportOptions::default()).unwrap();
        let page = doc.root.read().unwrap();
        assert_eq!(page.layers.len(), 1);
        match &page.layers[0] {
            Layer::Frame(f) => assert_eq!(f.children.len(), 2),
            _ => panic!("expected frame"),
        }
    }

    #[test]
    fn test_convert_text() {
        let svg = parse_svg(b"<svg><text x=\"10\" y=\"30\">Greetings</text></svg>").unwrap();
        let doc = convert_svg(&svg, &ImportOptions::default()).unwrap();
        let page = doc.root.read().unwrap();
        match &page.layers[0] {
            Layer::Text(t) => assert_eq!(t.content, "Greetings"),
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn test_convert_path() {
        let svg = parse_svg(b"<svg><path d=\"M 0 0 L 100 0 L 100 100 Z\"/></svg>").unwrap();
        let doc = convert_svg(&svg, &ImportOptions::default()).unwrap();
        let page = doc.root.read().unwrap();
        match &page.layers[0] {
            Layer::Path(p) => {
                assert!(p.closed);
                assert!(p.commands.len() >= 3);
            }
            _ => panic!("expected path"),
        }
    }

    #[test]
    fn test_convert_defs_skipped() {
        let svg = parse_svg(
            b"<svg><defs><linearGradient id=\"g1\"/></defs><rect x=\"0\" y=\"0\" width=\"100\" height=\"100\"/></svg>",
        )
        .unwrap();
        let doc = convert_svg(&svg, &ImportOptions::default()).unwrap();
        let page = doc.root.read().unwrap();
        assert_eq!(page.layers.len(), 1);
    }

    #[test]
    fn test_convert_ellipse_element() {
        let svg = parse_svg(b"<svg><ellipse cx=\"100\" cy=\"50\" rx=\"80\" ry=\"40\"/></svg>").unwrap();
        let doc = convert_svg(&svg, &ImportOptions::default()).unwrap();
        let page = doc.root.read().unwrap();
        match &page.layers[0] {
            Layer::Ellipse(e) => {
                assert!((e.bounds.width - 160.0).abs() < 0.01);
                assert!((e.bounds.height - 80.0).abs() < 0.01);
            }
            _ => panic!("expected ellipse"),
        }
    }

    #[test]
    fn test_convert_line() {
        let svg = parse_svg(b"<svg><line x1=\"10\" y1=\"20\" x2=\"100\" y2=\"200\"/></svg>").unwrap();
        let doc = convert_svg(&svg, &ImportOptions::default()).unwrap();
        let page = doc.root.read().unwrap();
        assert_eq!(page.layers.len(), 1);
        match &page.layers[0] {
            Layer::Path(p) => assert_eq!(p.commands.len(), 2),
            _ => panic!("expected path"),
        }
    }
}
