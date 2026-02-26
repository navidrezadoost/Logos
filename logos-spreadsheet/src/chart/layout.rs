//! Chart layout — transform resolved data into positioned geometry.
//!
//! The layout engine takes a [`ResolvedChart`] + [`ChartStyle`] and
//! computes **layout primitives** — rectangles for bars, (x, y) points
//! for lines, arc angles for pie slices, axis tick positions, etc.
//!
//! These layout primitives carry *only geometry and colour* — they are
//! independent of any specific rendering backend. The render module then
//! converts them into `DrawRect` / `DrawLine` / `DrawText` primitives.

use super::data::ResolvedChart;
use super::style::ChartStyle;
use super::types::{ChartKind, LegendPosition, SeriesColor, StackMode};

use std::f64::consts::PI;

// ---------------------------------------------------------------------------
// Layout primitives
// ---------------------------------------------------------------------------

/// A positioned rectangle (used for bars, backgrounds, legend swatches).
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub color: SeriesColor,
    /// Optional text label to render on/near the bar.
    pub label: Option<String>,
}

/// A positioned point (used for line vertices, scatter dots).
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutPoint {
    pub x: f64,
    pub y: f64,
    pub color: SeriesColor,
    pub radius: f32,
    pub label: Option<String>,
}

/// A line segment connecting two points.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutLine {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    pub color: SeriesColor,
    pub width: f32,
}

/// A pie/donut slice.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutSlice {
    /// Centre of the pie.
    pub cx: f64,
    pub cy: f64,
    /// Outer radius.
    pub radius: f64,
    /// Inner radius (0 = pie, >0 = donut).
    pub inner_radius: f64,
    /// Start angle in radians (0 = east, counter-clockwise).
    pub start_angle: f64,
    /// End angle in radians.
    pub end_angle: f64,
    pub color: SeriesColor,
    pub label: Option<String>,
    /// Percentage this slice occupies.
    pub percent: f64,
}

impl LayoutSlice {
    /// Midpoint angle (useful for label placement).
    pub fn mid_angle(&self) -> f64 {
        (self.start_angle + self.end_angle) / 2.0
    }
}

/// An axis tick mark with position and label.
#[derive(Debug, Clone, PartialEq)]
pub struct AxisTick {
    /// Position along the axis (in plot-area-local coordinates).
    pub position: f64,
    pub label: String,
}

/// A text label at a specific position.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutText {
    pub x: f64,
    pub y: f64,
    pub text: String,
    pub font_size: f32,
    pub color: SeriesColor,
    pub bold: bool,
}

/// A legend entry (swatch colour + label).
#[derive(Debug, Clone, PartialEq)]
pub struct LegendEntry {
    pub color: SeriesColor,
    pub label: String,
}

// ---------------------------------------------------------------------------
// Complete layout result
// ---------------------------------------------------------------------------

/// Everything the renderer needs to draw a chart.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartLayout {
    /// Overall chart position and size (absolute, in viewport pixels).
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,

    /// Background fill for the entire chart area.
    pub background: SeriesColor,
    /// Background fill for the plot area only.
    pub plot_background: SeriesColor,
    /// Plot area within the chart (relative to chart origin).
    pub plot_rect: (f64, f64, f64, f64),

    // Geometry — only the relevant vec is populated per chart kind.
    pub bars: Vec<LayoutRect>,
    pub lines: Vec<LayoutLine>,
    pub points: Vec<LayoutPoint>,
    pub slices: Vec<LayoutSlice>,
    /// Area fill polygons: each inner vec is a sequence of (x, y) vertices.
    pub area_fills: Vec<(Vec<(f64, f64)>, SeriesColor)>,

    // Axes
    pub x_ticks: Vec<AxisTick>,
    pub y_ticks: Vec<AxisTick>,
    pub grid_lines: Vec<LayoutLine>,
    pub x_axis_title: Option<LayoutText>,
    pub y_axis_title: Option<LayoutText>,

    // Decorations
    pub title: Option<LayoutText>,
    pub legend_entries: Vec<LegendEntry>,
    pub legend_position: LegendPosition,
    pub data_labels: Vec<LayoutText>,
}

impl ChartLayout {
    pub(crate) fn empty(x: f64, y: f64, w: f64, h: f64, style: &ChartStyle) -> Self {
        let plot_rect = style.plot_rect(w, h);
        Self {
            x,
            y,
            width: w,
            height: h,
            background: style.background,
            plot_background: style.plot_background,
            plot_rect,
            bars: Vec::new(),
            lines: Vec::new(),
            points: Vec::new(),
            slices: Vec::new(),
            area_fills: Vec::new(),
            x_ticks: Vec::new(),
            y_ticks: Vec::new(),
            grid_lines: Vec::new(),
            x_axis_title: None,
            y_axis_title: None,
            title: None,
            legend_entries: Vec::new(),
            legend_position: LegendPosition::None,
            data_labels: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Layout engine
// ---------------------------------------------------------------------------

/// Compute the full layout for a resolved chart + style.
pub fn compute_layout(
    chart: &ResolvedChart,
    style: &ChartStyle,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> ChartLayout {
    let mut layout = ChartLayout::empty(x, y, width, height, style);

    // Series colours
    let overrides: Vec<Option<SeriesColor>> = Vec::new(); // TODO: pass from spec
    let colors = style.series_colors(chart.series.len(), &overrides);

    // Title
    if let Some(ref title_text) = chart.title {
        layout.title = Some(LayoutText {
            x: x + width / 2.0,
            y: y + 10.0,
            text: title_text.clone(),
            font_size: style.fonts.title_size,
            color: style.text_color,
            bold: true,
        });
    }

    // Legend
    layout.legend_position = LegendPosition::Bottom; // default
    layout.legend_entries = chart
        .series
        .iter()
        .enumerate()
        .map(|(i, s)| LegendEntry {
            color: colors.get(i).copied().unwrap_or(SeriesColor::rgb(128, 128, 128)),
            label: s.label.clone(),
        })
        .collect();

    match chart.kind {
        ChartKind::Bar => layout_bars(chart, style, &colors, &mut layout, false),
        ChartKind::HorizontalBar => layout_bars(chart, style, &colors, &mut layout, true),
        ChartKind::Line => layout_lines(chart, style, &colors, &mut layout, false),
        ChartKind::Area => layout_lines(chart, style, &colors, &mut layout, true),
        ChartKind::Scatter => layout_scatter(chart, style, &colors, &mut layout),
        ChartKind::Pie => layout_pie(chart, style, &colors, &mut layout),
    }

    layout
}

// ---------------------------------------------------------------------------
// Bar layout
// ---------------------------------------------------------------------------

fn layout_bars(
    chart: &ResolvedChart,
    style: &ChartStyle,
    colors: &[SeriesColor],
    layout: &mut ChartLayout,
    horizontal: bool,
) {
    let (px, py, pw, ph) = layout.plot_rect;
    let n_cats = chart.point_count();
    let n_series = chart.series.len();
    if n_cats == 0 || n_series == 0 {
        return;
    }

    let value_range = chart.data_max - chart.data_min;
    if value_range.abs() < f64::EPSILON {
        return;
    }

    // Value → pixel mapping
    let _val_len = if horizontal { pw } else { ph };
    let cat_len = if horizontal { ph } else { pw };

    let gap_frac = style.bar_gap;
    let group_gap_frac = style.bar_group_gap;

    let cat_width = cat_len / n_cats as f64;
    let group_gap = cat_width * group_gap_frac;
    let usable_cat = cat_width - group_gap;

    let (bar_w, bar_gap_px) = if chart.stack_mode == StackMode::None && n_series > 1 {
        let total_gap = usable_cat * gap_frac * (n_series - 1) as f64 / n_series as f64;
        let w = (usable_cat - total_gap) / n_series as f64;
        (w, total_gap / (n_series - 1).max(1) as f64)
    } else {
        (usable_cat, 0.0)
    };

    // Y-axis ticks
    let tick_count = if style.y_axis.tick_count > 0 {
        style.y_axis.tick_count as usize
    } else {
        5
    };
    compute_value_ticks(
        chart.data_min,
        chart.data_max,
        tick_count,
        style,
        layout,
        px,
        py,
        pw,
        ph,
        horizontal,
    );

    // X-axis (category) ticks
    for (i, cat) in chart.categories.iter().enumerate() {
        let pos = cat_width * i as f64 + cat_width / 2.0;
        layout.x_ticks.push(AxisTick {
            position: pos,
            label: cat.clone(),
        });
    }

    // Bars
    let mut stack_accum: Vec<f64> = vec![0.0; n_cats];

    for (si, series) in chart.series.iter().enumerate() {
        let color = colors.get(si).copied().unwrap_or(SeriesColor::rgb(128, 128, 128));

        for (ci, val_opt) in series.values.iter().enumerate() {
            if ci >= n_cats {
                break;
            }
            let raw_val = val_opt.unwrap_or(0.0);

            let val = match chart.stack_mode {
                StackMode::PercentStacked => {
                    let total = chart.stacked_totals()[ci];
                    if total.abs() < f64::EPSILON {
                        0.0
                    } else {
                        raw_val / total * 100.0
                    }
                }
                _ => raw_val,
            };

            let base_offset = match chart.stack_mode {
                StackMode::None => 0.0,
                _ => stack_accum[ci],
            };

            let val_frac = (val - chart.data_min) / value_range;
            let base_frac = (base_offset + chart.data_min.max(0.0) - chart.data_min) / value_range;

            if horizontal {
                let cat_start = py + cat_width * ci as f64 + group_gap / 2.0;
                let bar_y = if chart.stack_mode == StackMode::None {
                    cat_start + si as f64 * (bar_w + bar_gap_px)
                } else {
                    cat_start
                };
                let bar_x = px + base_frac * pw;
                let bar_len = val_frac * pw;

                layout.bars.push(LayoutRect {
                    x: bar_x,
                    y: bar_y,
                    width: bar_len,
                    height: bar_w,
                    color,
                    label: if style.show_data_labels {
                        Some(format_value(raw_val))
                    } else {
                        None
                    },
                });
            } else {
                let cat_start = px + cat_width * ci as f64 + group_gap / 2.0;
                let bar_x = if chart.stack_mode == StackMode::None {
                    cat_start + si as f64 * (bar_w + bar_gap_px)
                } else {
                    cat_start
                };
                // Bars grow upward from bottom
                let bar_height = val_frac * ph;
                let bar_y = py + ph - base_frac * ph - bar_height;

                layout.bars.push(LayoutRect {
                    x: bar_x,
                    y: bar_y,
                    width: bar_w,
                    height: bar_height,
                    color,
                    label: if style.show_data_labels {
                        Some(format_value(raw_val))
                    } else {
                        None
                    },
                });
            }

            if chart.stack_mode != StackMode::None {
                stack_accum[ci] += val;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Line / Area layout
// ---------------------------------------------------------------------------

fn layout_lines(
    chart: &ResolvedChart,
    style: &ChartStyle,
    colors: &[SeriesColor],
    layout: &mut ChartLayout,
    fill_area: bool,
) {
    let (px, py, pw, ph) = layout.plot_rect;
    let n_cats = chart.point_count();
    if n_cats == 0 {
        return;
    }

    let value_range = chart.data_max - chart.data_min;
    if value_range.abs() < f64::EPSILON {
        return;
    }

    // Ticks
    let tick_count = if style.y_axis.tick_count > 0 {
        style.y_axis.tick_count as usize
    } else {
        5
    };
    compute_value_ticks(
        chart.data_min,
        chart.data_max,
        tick_count,
        style,
        layout,
        px,
        py,
        pw,
        ph,
        false,
    );
    for (i, cat) in chart.categories.iter().enumerate() {
        let pos = if n_cats == 1 {
            pw / 2.0
        } else {
            pw * i as f64 / (n_cats - 1) as f64
        };
        layout.x_ticks.push(AxisTick {
            position: pos,
            label: cat.clone(),
        });
    }

    for (si, series) in chart.series.iter().enumerate() {
        let color = colors.get(si).copied().unwrap_or(SeriesColor::rgb(128, 128, 128));

        let mut pts: Vec<(f64, f64)> = Vec::new();

        for (ci, val_opt) in series.values.iter().enumerate() {
            if ci >= n_cats {
                break;
            }

            let x_pos = if n_cats == 1 {
                px + pw / 2.0
            } else {
                px + pw * ci as f64 / (n_cats - 1) as f64
            };

            if let Some(val) = val_opt {
                let val_frac = (val - chart.data_min) / value_range;
                let y_pos = py + ph - val_frac * ph;
                pts.push((x_pos, y_pos));

                // Marker point
                layout.points.push(LayoutPoint {
                    x: x_pos,
                    y: y_pos,
                    color,
                    radius: style.marker_radius,
                    label: if style.show_data_labels {
                        Some(format_value(*val))
                    } else {
                        None
                    },
                });
            }
        }

        // Connect consecutive points with lines
        for pair in pts.windows(2) {
            layout.lines.push(LayoutLine {
                x1: pair[0].0,
                y1: pair[0].1,
                x2: pair[1].0,
                y2: pair[1].1,
                color,
                width: style.line_width,
            });
        }

        // Fill area below line
        if fill_area && pts.len() >= 2 {
            let mut polygon = pts.clone();
            let baseline_y = py + ph; // bottom of plot
            polygon.push((pts.last().unwrap().0, baseline_y));
            polygon.push((pts.first().unwrap().0, baseline_y));
            let fill_color = SeriesColor::new(color.r, color.g, color.b, 80);
            layout.area_fills.push((polygon, fill_color));
        }
    }
}

// ---------------------------------------------------------------------------
// Scatter layout
// ---------------------------------------------------------------------------

fn layout_scatter(
    chart: &ResolvedChart,
    style: &ChartStyle,
    colors: &[SeriesColor],
    layout: &mut ChartLayout,
) {
    let (px, py, pw, ph) = layout.plot_rect;
    let n_cats = chart.point_count();
    if n_cats == 0 {
        return;
    }

    let value_range = chart.data_max - chart.data_min;
    if value_range.abs() < f64::EPSILON {
        return;
    }

    let tick_count = if style.y_axis.tick_count > 0 {
        style.y_axis.tick_count as usize
    } else {
        5
    };
    compute_value_ticks(
        chart.data_min,
        chart.data_max,
        tick_count,
        style,
        layout,
        px,
        py,
        pw,
        ph,
        false,
    );

    for (si, series) in chart.series.iter().enumerate() {
        let color = colors.get(si).copied().unwrap_or(SeriesColor::rgb(128, 128, 128));

        for (ci, val_opt) in series.values.iter().enumerate() {
            if ci >= n_cats {
                break;
            }
            if let Some(val) = val_opt {
                let x_pos = if n_cats == 1 {
                    px + pw / 2.0
                } else {
                    px + pw * ci as f64 / (n_cats - 1) as f64
                };
                let val_frac = (val - chart.data_min) / value_range;
                let y_pos = py + ph - val_frac * ph;

                layout.points.push(LayoutPoint {
                    x: x_pos,
                    y: y_pos,
                    color,
                    radius: style.marker_radius,
                    label: if style.show_data_labels {
                        Some(format_value(*val))
                    } else {
                        None
                    },
                });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Pie layout
// ---------------------------------------------------------------------------

fn layout_pie(
    chart: &ResolvedChart,
    style: &ChartStyle,
    colors: &[SeriesColor],
    layout: &mut ChartLayout,
) {
    let (px, py, pw, ph) = layout.plot_rect;

    // Use first series only for pie
    let series = match chart.series.first() {
        Some(s) => s,
        None => return,
    };

    let total: f64 = series.values.iter().filter_map(|v| *v).sum();
    if total.abs() < f64::EPSILON {
        return;
    }

    let cx = px + pw / 2.0;
    let cy = py + ph / 2.0;
    let radius = pw.min(ph) / 2.0 * 0.85; // leave room for labels
    let inner_radius = radius * style.donut_hole;

    let start_deg = style.pie_start_angle;
    let mut current_angle = start_deg * PI / 180.0;

    for (i, val_opt) in series.values.iter().enumerate() {
        let val = val_opt.unwrap_or(0.0);
        if val.abs() < f64::EPSILON {
            continue;
        }

        let fraction = val / total;
        let sweep = fraction * 2.0 * PI;
        let end_angle = current_angle + sweep;
        let percent = fraction * 100.0;

        let color = if i < colors.len() {
            colors[i]
        } else {
            colors[i % colors.len().max(1)]
        };

        let label = if style.show_data_labels {
            let cat = chart.categories.get(i).cloned().unwrap_or_default();
            if cat.is_empty() {
                Some(format!("{:.1}%", percent))
            } else {
                Some(format!("{}: {:.1}%", cat, percent))
            }
        } else {
            None
        };

        layout.slices.push(LayoutSlice {
            cx,
            cy,
            radius,
            inner_radius,
            start_angle: current_angle,
            end_angle,
            color,
            label,
            percent,
        });

        current_angle = end_angle;
    }

    // For pie charts, legend entries come from categories, not series
    layout.legend_entries = chart
        .categories
        .iter()
        .enumerate()
        .map(|(i, cat)| LegendEntry {
            color: colors.get(i).copied().unwrap_or(SeriesColor::rgb(128, 128, 128)),
            label: cat.clone(),
        })
        .collect();
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn compute_value_ticks(
    data_min: f64,
    data_max: f64,
    count: usize,
    style: &ChartStyle,
    layout: &mut ChartLayout,
    px: f64,
    py: f64,
    pw: f64,
    ph: f64,
    horizontal: bool,
) {
    let range = data_max - data_min;
    if range.abs() < f64::EPSILON || count == 0 {
        return;
    }

    for i in 0..=count {
        let t = i as f64 / count as f64;
        let val = data_min + t * range;
        let label = format_value(val);

        if horizontal {
            let pos = t * pw;
            layout.y_ticks.push(AxisTick {
                position: pos,
                label,
            });
        } else {
            let pos = ph - t * ph; // invert — larger values at top
            layout.y_ticks.push(AxisTick {
                position: pos,
                label,
            });
        }

        // Grid line
        if style.y_axis.show_grid && i > 0 && i < count {
            if horizontal {
                let gx = px + t * pw;
                layout.grid_lines.push(LayoutLine {
                    x1: gx,
                    y1: py,
                    x2: gx,
                    y2: py + ph,
                    color: style.grid_color,
                    width: 1.0,
                });
            } else {
                let gy = py + ph - t * ph;
                layout.grid_lines.push(LayoutLine {
                    x1: px,
                    y1: gy,
                    x2: px + pw,
                    y2: gy,
                    color: style.grid_color,
                    width: 1.0,
                });
            }
        }
    }
}

fn format_value(v: f64) -> String {
    if v == v.floor() && v.abs() < 1e9 {
        format!("{}", v as i64)
    } else {
        format!("{:.1}", v)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::data::{ResolvedChart, ResolvedSeries};
    use crate::chart::types::{ChartKind, StackMode};

    fn simple_resolved(kind: ChartKind, values: Vec<f64>) -> ResolvedChart {
        let n = values.len();
        ResolvedChart {
            kind,
            title: Some("Test Chart".into()),
            series: vec![ResolvedSeries {
                label: "S1".into(),
                values: values.into_iter().map(Some).collect(),
            }],
            categories: (1..=n).map(|i| i.to_string()).collect(),
            data_min: 0.0,
            data_max: 100.0,
            stack_mode: StackMode::None,
        }
    }

    #[test]
    fn bar_layout_generates_bars() {
        let data = simple_resolved(ChartKind::Bar, vec![25.0, 50.0, 75.0]);
        let style = ChartStyle::default();
        let layout = compute_layout(&data, &style, 0.0, 0.0, 400.0, 300.0);
        assert_eq!(layout.bars.len(), 3);
    }

    #[test]
    fn bar_heights_proportional() {
        let data = simple_resolved(ChartKind::Bar, vec![50.0, 100.0]);
        let style = ChartStyle::default();
        let layout = compute_layout(&data, &style, 0.0, 0.0, 400.0, 300.0);
        // Second bar should be taller (larger height)
        assert!(layout.bars[1].height > layout.bars[0].height);
    }

    #[test]
    fn horizontal_bar_layout() {
        let data = simple_resolved(ChartKind::HorizontalBar, vec![25.0, 50.0]);
        // Manually set kind since helper sets it
        let mut data = data;
        data.kind = ChartKind::HorizontalBar;
        let style = ChartStyle::default();
        let layout = compute_layout(&data, &style, 0.0, 0.0, 400.0, 300.0);
        assert_eq!(layout.bars.len(), 2);
        // Horizontal: width is proportional to value
        assert!(layout.bars[1].width > layout.bars[0].width);
    }

    #[test]
    fn line_layout_points_and_segments() {
        let data = simple_resolved(ChartKind::Line, vec![10.0, 40.0, 30.0]);
        let style = ChartStyle::default();
        let layout = compute_layout(&data, &style, 0.0, 0.0, 400.0, 300.0);
        assert_eq!(layout.points.len(), 3);
        assert_eq!(layout.lines.len(), 2); // 3 points → 2 segments
    }

    #[test]
    fn area_layout_fills() {
        let data = simple_resolved(ChartKind::Area, vec![10.0, 40.0, 30.0]);
        let style = ChartStyle::default();
        let layout = compute_layout(&data, &style, 0.0, 0.0, 400.0, 300.0);
        assert_eq!(layout.area_fills.len(), 1);
        // Polygon: 3 data points + 2 baseline corners = 5 vertices
        assert_eq!(layout.area_fills[0].0.len(), 5);
    }

    #[test]
    fn scatter_layout_points() {
        let data = simple_resolved(ChartKind::Scatter, vec![10.0, 20.0, 30.0]);
        let style = ChartStyle::default();
        let layout = compute_layout(&data, &style, 0.0, 0.0, 400.0, 300.0);
        assert_eq!(layout.points.len(), 3);
        assert!(layout.lines.is_empty()); // scatter has no lines
    }

    #[test]
    fn pie_layout_slices() {
        let data = simple_resolved(ChartKind::Pie, vec![25.0, 50.0, 25.0]);
        let style = ChartStyle::default();
        let layout = compute_layout(&data, &style, 0.0, 0.0, 400.0, 300.0);
        assert_eq!(layout.slices.len(), 3);
        // Check percentages
        assert!((layout.slices[0].percent - 25.0).abs() < 0.1);
        assert!((layout.slices[1].percent - 50.0).abs() < 0.1);
    }

    #[test]
    fn pie_donut_hole() {
        let data = simple_resolved(ChartKind::Pie, vec![50.0, 50.0]);
        let style = ChartStyle::default().with_donut_hole(0.5);
        let layout = compute_layout(&data, &style, 0.0, 0.0, 400.0, 300.0);
        assert!(layout.slices[0].inner_radius > 0.0);
        assert!(layout.slices[0].inner_radius < layout.slices[0].radius);
    }

    #[test]
    fn title_present() {
        let data = simple_resolved(ChartKind::Bar, vec![10.0]);
        let style = ChartStyle::default();
        let layout = compute_layout(&data, &style, 0.0, 0.0, 400.0, 300.0);
        assert!(layout.title.is_some());
        assert_eq!(layout.title.unwrap().text, "Test Chart");
    }

    #[test]
    fn legend_entries_match_series() {
        let data = {
            let mut d = simple_resolved(ChartKind::Bar, vec![10.0, 20.0]);
            d.series.push(ResolvedSeries {
                label: "S2".into(),
                values: vec![Some(30.0), Some(40.0)],
            });
            d
        };
        let style = ChartStyle::default();
        let layout = compute_layout(&data, &style, 0.0, 0.0, 400.0, 300.0);
        assert_eq!(layout.legend_entries.len(), 2);
        assert_eq!(layout.legend_entries[0].label, "S1");
        assert_eq!(layout.legend_entries[1].label, "S2");
    }

    #[test]
    fn y_ticks_generated() {
        let data = simple_resolved(ChartKind::Bar, vec![0.0, 50.0, 100.0]);
        let style = ChartStyle::default();
        let layout = compute_layout(&data, &style, 0.0, 0.0, 400.0, 300.0);
        assert!(!layout.y_ticks.is_empty());
        // Default 5 ticks → 6 marks (0 through 5 inclusive)
        assert_eq!(layout.y_ticks.len(), 6);
    }

    #[test]
    fn grid_lines_generated() {
        let data = simple_resolved(ChartKind::Bar, vec![0.0, 50.0, 100.0]);
        let style = ChartStyle::default();
        let layout = compute_layout(&data, &style, 0.0, 0.0, 400.0, 300.0);
        // Grid lines between ticks (not at 0% or 100%)
        assert!(!layout.grid_lines.is_empty());
    }

    #[test]
    fn data_labels_off_by_default() {
        let data = simple_resolved(ChartKind::Bar, vec![10.0, 20.0]);
        let style = ChartStyle::default();
        let layout = compute_layout(&data, &style, 0.0, 0.0, 400.0, 300.0);
        assert!(layout.bars.iter().all(|b| b.label.is_none()));
    }

    #[test]
    fn data_labels_on() {
        let data = simple_resolved(ChartKind::Bar, vec![10.0, 20.0]);
        let style = ChartStyle::default().with_data_labels(true);
        let layout = compute_layout(&data, &style, 0.0, 0.0, 400.0, 300.0);
        assert!(layout.bars.iter().all(|b| b.label.is_some()));
    }

    #[test]
    fn empty_chart_layout() {
        let data = ResolvedChart {
            kind: ChartKind::Bar,
            title: None,
            series: vec![],
            categories: vec![],
            data_min: 0.0,
            data_max: 1.0,
            stack_mode: StackMode::None,
        };
        let style = ChartStyle::default();
        let layout = compute_layout(&data, &style, 0.0, 0.0, 400.0, 300.0);
        assert!(layout.bars.is_empty());
        assert!(layout.title.is_none());
    }

    #[test]
    fn pie_legend_from_categories() {
        let data = simple_resolved(ChartKind::Pie, vec![30.0, 70.0]);
        let style = ChartStyle::default();
        let layout = compute_layout(&data, &style, 0.0, 0.0, 400.0, 300.0);
        // Pie legend uses categories, not series labels
        assert_eq!(layout.legend_entries.len(), 2);
        assert_eq!(layout.legend_entries[0].label, "1");
        assert_eq!(layout.legend_entries[1].label, "2");
    }

    #[test]
    fn slice_mid_angle() {
        let s = LayoutSlice {
            cx: 0.0,
            cy: 0.0,
            radius: 100.0,
            inner_radius: 0.0,
            start_angle: 0.0,
            end_angle: PI,
            color: SeriesColor::rgb(0, 0, 0),
            label: None,
            percent: 50.0,
        };
        assert!((s.mid_angle() - PI / 2.0).abs() < 0.001);
    }

    #[test]
    fn format_value_integer() {
        assert_eq!(format_value(42.0), "42");
        assert_eq!(format_value(0.0), "0");
    }

    #[test]
    fn format_value_decimal() {
        assert_eq!(format_value(3.14), "3.1");
    }

    #[test]
    fn multi_series_bar_grouped() {
        let data = {
            let mut d = simple_resolved(ChartKind::Bar, vec![20.0, 40.0]);
            d.series.push(ResolvedSeries {
                label: "S2".into(),
                values: vec![Some(30.0), Some(50.0)],
            });
            d
        };
        let style = ChartStyle::default();
        let layout = compute_layout(&data, &style, 0.0, 0.0, 400.0, 300.0);
        // 2 series × 2 categories = 4 bars
        assert_eq!(layout.bars.len(), 4);
    }

    #[test]
    fn chart_position_offset() {
        let data = simple_resolved(ChartKind::Bar, vec![50.0]);
        let style = ChartStyle::default();
        let layout = compute_layout(&data, &style, 100.0, 200.0, 400.0, 300.0);
        assert_eq!(layout.x, 100.0);
        assert_eq!(layout.y, 200.0);
    }
}
