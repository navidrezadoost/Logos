//! Chart render data — convert layout primitives to draw commands.
//!
//! Takes a [`ChartLayout`] and produces a [`ChartRenderData`] containing
//! vectors of `DrawRect`, `DrawLine`, and `DrawText` that the existing
//! render pipeline can consume directly.

use super::layout::{ChartLayout, LayoutSlice, LayoutText};
use super::types::LegendPosition;
use std::f64::consts::PI;

// ---------------------------------------------------------------------------
// Draw commands (chart-specific, lightweight)
// ---------------------------------------------------------------------------

/// A rectangle to draw.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub color: [f32; 4],
    pub border_radius: f32,
}

/// A line segment to draw.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartLine {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    pub color: [f32; 4],
    pub thickness: f32,
}

/// A circle / dot to draw.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartCircle {
    pub cx: f32,
    pub cy: f32,
    pub radius: f32,
    pub color: [f32; 4],
}

/// A text label to draw.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartText {
    pub x: f32,
    pub y: f32,
    pub text: String,
    pub font_size: f32,
    pub color: [f32; 4],
    pub bold: bool,
}

/// A triangle (used to approximate pie slices via tessellation).
#[derive(Debug, Clone, PartialEq)]
pub struct ChartTriangle {
    pub v0: (f32, f32),
    pub v1: (f32, f32),
    pub v2: (f32, f32),
    pub color: [f32; 4],
}

/// A filled polygon (used for area charts).
#[derive(Debug, Clone, PartialEq)]
pub struct ChartPolygon {
    pub vertices: Vec<(f32, f32)>,
    pub color: [f32; 4],
}

// ---------------------------------------------------------------------------
// Aggregate render data
// ---------------------------------------------------------------------------

/// Everything the GPU/render backend needs to draw one chart.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartRenderData {
    pub rects: Vec<ChartRect>,
    pub lines: Vec<ChartLine>,
    pub circles: Vec<ChartCircle>,
    pub texts: Vec<ChartText>,
    pub triangles: Vec<ChartTriangle>,
    pub polygons: Vec<ChartPolygon>,
}

impl ChartRenderData {
    pub fn new() -> Self {
        Self {
            rects: Vec::new(),
            lines: Vec::new(),
            circles: Vec::new(),
            texts: Vec::new(),
            triangles: Vec::new(),
            polygons: Vec::new(),
        }
    }

    /// Total number of draw commands.
    pub fn command_count(&self) -> usize {
        self.rects.len()
            + self.lines.len()
            + self.circles.len()
            + self.texts.len()
            + self.triangles.len()
            + self.polygons.len()
    }

    /// Whether there are any draw commands.
    pub fn is_empty(&self) -> bool {
        self.command_count() == 0
    }
}

impl Default for ChartRenderData {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Converter
// ---------------------------------------------------------------------------

/// Convert a [`ChartLayout`] to [`ChartRenderData`].
pub fn render_chart(layout: &ChartLayout) -> ChartRenderData {
    let mut data = ChartRenderData::new();

    // 1. Background
    data.rects.push(ChartRect {
        x: layout.x as f32,
        y: layout.y as f32,
        width: layout.width as f32,
        height: layout.height as f32,
        color: layout.background.to_f32(),
        border_radius: 4.0,
    });

    // 2. Plot area background
    let (px, py, pw, ph) = layout.plot_rect;
    data.rects.push(ChartRect {
        x: (layout.x + px) as f32,
        y: (layout.y + py) as f32,
        width: pw as f32,
        height: ph as f32,
        color: layout.plot_background.to_f32(),
        border_radius: 0.0,
    });

    // 3. Grid lines
    for gl in &layout.grid_lines {
        data.lines.push(ChartLine {
            x1: (layout.x + gl.x1) as f32,
            y1: (layout.y + gl.y1) as f32,
            x2: (layout.x + gl.x2) as f32,
            y2: (layout.y + gl.y2) as f32,
            color: gl.color.to_f32(),
            thickness: gl.width,
        });
    }

    // 4. Bars
    for bar in &layout.bars {
        data.rects.push(ChartRect {
            x: (layout.x + bar.x) as f32,
            y: (layout.y + bar.y) as f32,
            width: bar.width as f32,
            height: bar.height as f32,
            color: bar.color.to_f32(),
            border_radius: 2.0,
        });
        if let Some(ref label) = bar.label {
            data.texts.push(ChartText {
                x: (layout.x + bar.x + bar.width / 2.0) as f32,
                y: (layout.y + bar.y - 4.0) as f32,
                text: label.clone(),
                font_size: 9.0,
                color: layout.background.to_f32(), // contrast
                bold: false,
            });
        }
    }

    // 5. Lines
    for line in &layout.lines {
        data.lines.push(ChartLine {
            x1: (layout.x + line.x1) as f32,
            y1: (layout.y + line.y1) as f32,
            x2: (layout.x + line.x2) as f32,
            y2: (layout.y + line.y2) as f32,
            color: line.color.to_f32(),
            thickness: line.width,
        });
    }

    // 6. Points / markers
    for pt in &layout.points {
        if pt.radius > 0.0 {
            data.circles.push(ChartCircle {
                cx: (layout.x + pt.x) as f32,
                cy: (layout.y + pt.y) as f32,
                radius: pt.radius,
                color: pt.color.to_f32(),
            });
        }
        if let Some(ref label) = pt.label {
            data.texts.push(ChartText {
                x: (layout.x + pt.x) as f32,
                y: (layout.y + pt.y - pt.radius as f64 - 4.0) as f32,
                text: label.clone(),
                font_size: 9.0,
                color: layout.background.to_f32(),
                bold: false,
            });
        }
    }

    // 7. Pie slices → triangles (fan tessellation)
    render_slices(&layout.slices, layout.x, layout.y, &mut data);

    // 8. Area fills → polygons
    for (vertices, color) in &layout.area_fills {
        data.polygons.push(ChartPolygon {
            vertices: vertices.iter().map(|(x, y)| ((layout.x + x) as f32, (layout.y + y) as f32)).collect(),
            color: color.to_f32(),
        });
    }

    // 9. Axis labels
    render_axis_labels(layout, &mut data);

    // 10. Title
    if let Some(ref title) = layout.title {
        render_text(title, layout.x, layout.y, &mut data);
    }

    // 11. Legend
    render_legend(layout, &mut data);

    data
}

fn render_slices(
    slices: &[LayoutSlice],
    chart_x: f64,
    chart_y: f64,
    data: &mut ChartRenderData,
) {
    const SEGMENTS_PER_SLICE: usize = 32;

    for slice in slices {
        let cx = (chart_x + slice.cx) as f32;
        let cy = (chart_y + slice.cy) as f32;
        let r = slice.radius as f32;
        let sweep = slice.end_angle - slice.start_angle;
        let steps = ((sweep.abs() / (2.0 * PI)) * SEGMENTS_PER_SLICE as f64).max(4.0) as usize;

        for i in 0..steps {
            let a0 = slice.start_angle + sweep * i as f64 / steps as f64;
            let a1 = slice.start_angle + sweep * (i + 1) as f64 / steps as f64;

            let v0_x = cx + r * a0.cos() as f32;
            let v0_y = cy + r * a0.sin() as f32;
            let v1_x = cx + r * a1.cos() as f32;
            let v1_y = cy + r * a1.sin() as f32;

            if slice.inner_radius > 0.0 {
                // Donut: two triangles forming a quad
                let ir = slice.inner_radius as f32;
                let i0_x = cx + ir * a0.cos() as f32;
                let i0_y = cy + ir * a0.sin() as f32;
                let i1_x = cx + ir * a1.cos() as f32;
                let i1_y = cy + ir * a1.sin() as f32;

                data.triangles.push(ChartTriangle {
                    v0: (v0_x, v0_y),
                    v1: (v1_x, v1_y),
                    v2: (i0_x, i0_y),
                    color: slice.color.to_f32(),
                });
                data.triangles.push(ChartTriangle {
                    v0: (v1_x, v1_y),
                    v1: (i1_x, i1_y),
                    v2: (i0_x, i0_y),
                    color: slice.color.to_f32(),
                });
            } else {
                // Solid pie: fan from centre
                data.triangles.push(ChartTriangle {
                    v0: (cx, cy),
                    v1: (v0_x, v0_y),
                    v2: (v1_x, v1_y),
                    color: slice.color.to_f32(),
                });
            }
        }

        // Slice label
        if let Some(ref label) = slice.label {
            let mid = slice.mid_angle();
            let label_r = slice.radius * 0.7;
            let lx = (chart_x + slice.cx + label_r * mid.cos()) as f32;
            let ly = (chart_y + slice.cy + label_r * mid.sin()) as f32;
            data.texts.push(ChartText {
                x: lx,
                y: ly,
                text: label.clone(),
                font_size: 9.0,
                color: [1.0, 1.0, 1.0, 1.0],
                bold: false,
            });
        }
    }
}

fn render_axis_labels(layout: &ChartLayout, data: &mut ChartRenderData) {
    let (px, py, _pw, ph) = layout.plot_rect;

    // Y-axis tick labels
    for tick in &layout.y_ticks {
        data.texts.push(ChartText {
            x: (layout.x + px - 8.0) as f32,
            y: (layout.y + py + tick.position) as f32,
            text: tick.label.clone(),
            font_size: 10.0,
            color: layout.plot_background.to_f32(), // muted
            bold: false,
        });
    }

    // X-axis tick labels
    for tick in &layout.x_ticks {
        data.texts.push(ChartText {
            x: (layout.x + px + tick.position) as f32,
            y: (layout.y + py + ph + 16.0) as f32,
            text: tick.label.clone(),
            font_size: 10.0,
            color: layout.plot_background.to_f32(),
            bold: false,
        });
    }
}

fn render_text(lt: &LayoutText, _chart_x: f64, _chart_y: f64, data: &mut ChartRenderData) {
    data.texts.push(ChartText {
        x: lt.x as f32,
        y: lt.y as f32,
        text: lt.text.clone(),
        font_size: lt.font_size,
        color: lt.color.to_f32(),
        bold: lt.bold,
    });
}

fn render_legend(layout: &ChartLayout, data: &mut ChartRenderData) {
    if layout.legend_position == LegendPosition::None || layout.legend_entries.is_empty() {
        return;
    }

    let swatch_size: f32 = 10.0;
    let spacing: f32 = 6.0;
    let entry_width: f32 = 80.0;
    let n = layout.legend_entries.len();

    // Bottom-centred legend
    let total_w = n as f32 * entry_width;
    let start_x = layout.x as f32 + (layout.width as f32 - total_w) / 2.0;
    let start_y = layout.y as f32 + layout.height as f32 - 16.0;

    for (i, entry) in layout.legend_entries.iter().enumerate() {
        let ex = start_x + i as f32 * entry_width;
        // Colour swatch
        data.rects.push(ChartRect {
            x: ex,
            y: start_y,
            width: swatch_size,
            height: swatch_size,
            color: entry.color.to_f32(),
            border_radius: 2.0,
        });
        // Label
        data.texts.push(ChartText {
            x: ex + swatch_size + spacing,
            y: start_y + 1.0,
            text: entry.label.clone(),
            font_size: 10.0,
            color: [0.2, 0.2, 0.2, 1.0],
            bold: false,
        });
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::data::{ResolvedChart, ResolvedSeries};
    use crate::chart::layout::compute_layout;
    use crate::chart::style::ChartStyle;
    use crate::chart::types::{ChartKind, StackMode};

    fn simple_bar_layout() -> ChartLayout {
        let chart = ResolvedChart {
            kind: ChartKind::Bar,
            title: Some("Sales".into()),
            series: vec![ResolvedSeries {
                label: "Q1".into(),
                values: vec![Some(10.0), Some(20.0), Some(30.0)],
            }],
            categories: vec!["A".into(), "B".into(), "C".into()],
            data_min: 0.0,
            data_max: 30.0,
            stack_mode: StackMode::None,
        };
        let style = ChartStyle::default();
        compute_layout(&chart, &style, 10.0, 20.0, 400.0, 300.0)
    }

    #[test]
    fn render_produces_rects() {
        let layout = simple_bar_layout();
        let rd = render_chart(&layout);
        // At least background + plot area + 3 bars = 5
        assert!(rd.rects.len() >= 5);
    }

    #[test]
    fn render_produces_title() {
        let layout = simple_bar_layout();
        let rd = render_chart(&layout);
        assert!(rd.texts.iter().any(|t| t.text == "Sales"));
    }

    #[test]
    fn render_produces_legend() {
        let layout = simple_bar_layout();
        let rd = render_chart(&layout);
        // Legend swatch rects (at least 1 for the single series)
        // Rects = background(1) + plot_area(1) + bars(3) + legend_swatch(1) = 6
        assert!(rd.rects.len() >= 6);
    }

    #[test]
    fn render_command_count() {
        let layout = simple_bar_layout();
        let rd = render_chart(&layout);
        assert!(rd.command_count() > 0);
        assert!(!rd.is_empty());
    }

    #[test]
    fn render_empty_chart() {
        let layout = ChartLayout::empty(0.0, 0.0, 400.0, 300.0, &ChartStyle::default());
        let rd = render_chart(&layout);
        // Only background + plot area rects
        assert_eq!(rd.rects.len(), 2);
        assert!(rd.lines.is_empty());
    }

    #[test]
    fn render_pie_produces_triangles() {
        let chart = ResolvedChart {
            kind: ChartKind::Pie,
            title: None,
            series: vec![ResolvedSeries {
                label: "Data".into(),
                values: vec![Some(50.0), Some(30.0), Some(20.0)],
            }],
            categories: vec!["A".into(), "B".into(), "C".into()],
            data_min: 0.0,
            data_max: 50.0,
            stack_mode: StackMode::None,
        };
        let style = ChartStyle::default();
        let layout = compute_layout(&chart, &style, 0.0, 0.0, 400.0, 300.0);
        let rd = render_chart(&layout);
        assert!(!rd.triangles.is_empty());
    }

    #[test]
    fn render_line_produces_lines() {
        let chart = ResolvedChart {
            kind: ChartKind::Line,
            title: None,
            series: vec![ResolvedSeries {
                label: "Trend".into(),
                values: vec![Some(10.0), Some(20.0), Some(15.0)],
            }],
            categories: vec!["1".into(), "2".into(), "3".into()],
            data_min: 0.0,
            data_max: 20.0,
            stack_mode: StackMode::None,
        };
        let style = ChartStyle::default();
        let layout = compute_layout(&chart, &style, 0.0, 0.0, 400.0, 300.0);
        let rd = render_chart(&layout);
        assert!(rd.lines.len() >= 2); // 2 line segments
        assert!(!rd.circles.is_empty()); // markers
    }

    #[test]
    fn render_area_produces_polygons() {
        let chart = ResolvedChart {
            kind: ChartKind::Area,
            title: None,
            series: vec![ResolvedSeries {
                label: "Fill".into(),
                values: vec![Some(5.0), Some(15.0), Some(10.0)],
            }],
            categories: vec!["1".into(), "2".into(), "3".into()],
            data_min: 0.0,
            data_max: 15.0,
            stack_mode: StackMode::None,
        };
        let style = ChartStyle::default();
        let layout = compute_layout(&chart, &style, 0.0, 0.0, 400.0, 300.0);
        let rd = render_chart(&layout);
        assert!(!rd.polygons.is_empty());
    }

    #[test]
    fn render_with_data_labels() {
        let chart = ResolvedChart {
            kind: ChartKind::Bar,
            title: None,
            series: vec![ResolvedSeries {
                label: "X".into(),
                values: vec![Some(42.0)],
            }],
            categories: vec!["A".into()],
            data_min: 0.0,
            data_max: 42.0,
            stack_mode: StackMode::None,
        };
        let style = ChartStyle::default().with_data_labels(true);
        let layout = compute_layout(&chart, &style, 0.0, 0.0, 400.0, 300.0);
        let rd = render_chart(&layout);
        assert!(rd.texts.iter().any(|t| t.text == "42"));
    }

    #[test]
    fn chart_render_data_default() {
        let rd = ChartRenderData::default();
        assert!(rd.is_empty());
        assert_eq!(rd.command_count(), 0);
    }
}
