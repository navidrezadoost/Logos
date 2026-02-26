//! Chart type definitions — the core vocabulary for describing charts.
//!
//! Every chart in Logos is described by a [`ChartSpec`] which captures:
//!
//! - **Kind** – bar, line, pie, area, scatter, or combo.
//! - **Data source** – one or more [`DataSeries`], each pointing at a cell
//!   range in the spreadsheet.
//! - **Configuration** – axis labels, legend position, title, stacking mode.
//!
//! The spec is *declarative*: it describes **what** to draw, not **how**.
//! The layout and render modules translate a spec + resolved data into
//! drawable primitives.

use std::fmt;

// ---------------------------------------------------------------------------
// Chart kind
// ---------------------------------------------------------------------------

/// Top-level chart type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChartKind {
    /// Vertical bars (columns).
    Bar,
    /// Horizontal bars.
    HorizontalBar,
    /// Connected line with optional markers.
    Line,
    /// Circular slices proportional to value.
    Pie,
    /// Filled area under a line.
    Area,
    /// Dots placed at (x, y) coordinates.
    Scatter,
}

impl fmt::Display for ChartKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChartKind::Bar => write!(f, "Bar"),
            ChartKind::HorizontalBar => write!(f, "Horizontal Bar"),
            ChartKind::Line => write!(f, "Line"),
            ChartKind::Pie => write!(f, "Pie"),
            ChartKind::Area => write!(f, "Area"),
            ChartKind::Scatter => write!(f, "Scatter"),
        }
    }
}

// ---------------------------------------------------------------------------
// Data series
// ---------------------------------------------------------------------------

/// A single data series inside a chart.
///
/// Each series points at a contiguous range of cells whose *computed values*
/// will be extracted at chart-build time.
#[derive(Debug, Clone, PartialEq)]
pub struct DataSeries {
    /// Human-readable label (used in legend).
    pub label: String,
    /// Column range: `(start_col, start_row, end_col, end_row)`.
    /// Inclusive on both ends.  Values are 0-based.
    pub range: (u32, u32, u32, u32),
    /// Optional override colour for this series.
    pub color: Option<SeriesColor>,
}

impl DataSeries {
    /// Create a new data series.
    pub fn new(label: impl Into<String>, range: (u32, u32, u32, u32)) -> Self {
        Self {
            label: label.into(),
            range,
            color: None,
        }
    }

    /// Builder: set an explicit colour.
    pub fn with_color(mut self, color: SeriesColor) -> Self {
        self.color = Some(color);
        self
    }

    /// Number of cells this series spans.
    pub fn cell_count(&self) -> usize {
        let cols = (self.range.2 as i64 - self.range.0 as i64).unsigned_abs() as usize + 1;
        let rows = (self.range.3 as i64 - self.range.1 as i64).unsigned_abs() as usize + 1;
        cols * rows
    }

    /// Iterate `(col, row)` pairs in reading order (row-major).
    pub fn cell_coords(&self) -> Vec<(u32, u32)> {
        let mut coords = Vec::with_capacity(self.cell_count());
        for row in self.range.1..=self.range.3 {
            for col in self.range.0..=self.range.2 {
                coords.push((col, row));
            }
        }
        coords
    }
}

impl fmt::Display for DataSeries {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "\"{}\" ({},{})→({},{})",
            self.label, self.range.0, self.range.1, self.range.2, self.range.3
        )
    }
}

// ---------------------------------------------------------------------------
// Category labels
// ---------------------------------------------------------------------------

/// Where the category (x-axis) labels come from.
#[derive(Debug, Clone, PartialEq)]
pub enum CategorySource {
    /// Auto-generate: "1", "2", "3", …
    Auto,
    /// Use values from a cell range as labels.
    Range(u32, u32, u32, u32),
    /// Explicit list of labels.
    Explicit(Vec<String>),
}

// ---------------------------------------------------------------------------
// Colours
// ---------------------------------------------------------------------------

/// RGBA colour for a series — stored as `u8` to match the UI `Color` type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeriesColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl SeriesColor {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// Convert to `[f32; 4]` for the render pipeline.
    pub fn to_f32(self) -> [f32; 4] {
        [
            self.r as f32 / 255.0,
            self.g as f32 / 255.0,
            self.b as f32 / 255.0,
            self.a as f32 / 255.0,
        ]
    }
}

impl fmt::Display for SeriesColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "rgba({},{},{},{})", self.r, self.g, self.b, self.a)
    }
}

/// Default palette — 8 distinguishable colours (Material-inspired).
pub const DEFAULT_PALETTE: [SeriesColor; 8] = [
    SeriesColor::rgb(66, 133, 244),   // blue
    SeriesColor::rgb(234, 67, 53),    // red
    SeriesColor::rgb(251, 188, 4),    // yellow
    SeriesColor::rgb(52, 168, 83),    // green
    SeriesColor::rgb(255, 109, 0),    // orange
    SeriesColor::rgb(171, 71, 188),   // purple
    SeriesColor::rgb(0, 172, 193),    // teal
    SeriesColor::rgb(117, 117, 117),  // grey
];

/// Pick a palette colour by index (wraps around).
pub fn palette_color(index: usize) -> SeriesColor {
    DEFAULT_PALETTE[index % DEFAULT_PALETTE.len()]
}

// ---------------------------------------------------------------------------
// Legend & axis
// ---------------------------------------------------------------------------

/// Where the legend should appear.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegendPosition {
    None,
    Top,
    Bottom,
    Left,
    Right,
}

impl Default for LegendPosition {
    fn default() -> Self {
        LegendPosition::Bottom
    }
}

/// Axis configuration (for bar / line / area / scatter).
#[derive(Debug, Clone, PartialEq)]
pub struct AxisConfig {
    /// Optional title displayed alongside the axis.
    pub title: Option<String>,
    /// Use an explicit min instead of auto-scaling.
    pub min: Option<f64>,
    /// Use an explicit max instead of auto-scaling.
    pub max: Option<f64>,
    /// How many grid lines / tick marks to show (0 = auto).
    pub tick_count: u32,
    /// Show grid lines behind the chart area.
    pub show_grid: bool,
}

impl Default for AxisConfig {
    fn default() -> Self {
        Self {
            title: None,
            min: None,
            max: None,
            tick_count: 0,
            show_grid: true,
        }
    }
}

impl AxisConfig {
    /// Builder: set axis title.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Builder: set explicit range.
    pub fn with_range(mut self, min: f64, max: f64) -> Self {
        self.min = Some(min);
        self.max = Some(max);
        self
    }
}

/// How multiple series are arranged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackMode {
    /// Each series stands alone.
    None,
    /// Series are stacked on top of each other.
    Stacked,
    /// Series are stacked and normalised to 100 %.
    PercentStacked,
}

impl Default for StackMode {
    fn default() -> Self {
        StackMode::None
    }
}

// ---------------------------------------------------------------------------
// ChartSpec — the full declarative description
// ---------------------------------------------------------------------------

/// Unique identity of a chart inside a spreadsheet.
pub type ChartId = u32;

/// Complete specification for one chart.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartSpec {
    pub id: ChartId,
    pub kind: ChartKind,
    pub title: Option<String>,
    pub series: Vec<DataSeries>,
    pub categories: CategorySource,
    pub x_axis: AxisConfig,
    pub y_axis: AxisConfig,
    pub legend: LegendPosition,
    pub stack_mode: StackMode,
    /// Position inside the spreadsheet viewport (pixels).
    pub position: (f64, f64),
    /// Size in pixels.
    pub size: (f64, f64),
}

impl ChartSpec {
    /// Create a minimal bar chart from a single range.
    pub fn bar(id: ChartId, label: impl Into<String>, range: (u32, u32, u32, u32)) -> Self {
        Self::new(id, ChartKind::Bar, label, range)
    }

    /// Create a minimal line chart from a single range.
    pub fn line(id: ChartId, label: impl Into<String>, range: (u32, u32, u32, u32)) -> Self {
        Self::new(id, ChartKind::Line, label, range)
    }

    /// Create a minimal pie chart from a single range.
    pub fn pie(id: ChartId, label: impl Into<String>, range: (u32, u32, u32, u32)) -> Self {
        Self::new(id, ChartKind::Pie, label, range)
    }

    /// Create a minimal area chart.
    pub fn area(id: ChartId, label: impl Into<String>, range: (u32, u32, u32, u32)) -> Self {
        Self::new(id, ChartKind::Area, label, range)
    }

    /// Create a minimal scatter chart.
    pub fn scatter(id: ChartId, label: impl Into<String>, range: (u32, u32, u32, u32)) -> Self {
        Self::new(id, ChartKind::Scatter, label, range)
    }

    fn new(
        id: ChartId,
        kind: ChartKind,
        label: impl Into<String>,
        range: (u32, u32, u32, u32),
    ) -> Self {
        Self {
            id,
            kind,
            title: None,
            series: vec![DataSeries::new(label, range)],
            categories: CategorySource::Auto,
            x_axis: AxisConfig::default(),
            y_axis: AxisConfig::default(),
            legend: LegendPosition::default(),
            stack_mode: StackMode::default(),
            position: (0.0, 0.0),
            size: (400.0, 300.0),
        }
    }

    /// Builder: set chart title.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Builder: add a data series.
    pub fn with_series(mut self, series: DataSeries) -> Self {
        self.series.push(series);
        self
    }

    /// Builder: set position.
    pub fn with_position(mut self, x: f64, y: f64) -> Self {
        self.position = (x, y);
        self
    }

    /// Builder: set size.
    pub fn with_size(mut self, w: f64, h: f64) -> Self {
        self.size = (w, h);
        self
    }

    /// Builder: set legend position.
    pub fn with_legend(mut self, pos: LegendPosition) -> Self {
        self.legend = pos;
        self
    }

    /// Builder: set category source.
    pub fn with_categories(mut self, source: CategorySource) -> Self {
        self.categories = source;
        self
    }

    /// Builder: set stack mode.
    pub fn with_stack(mut self, mode: StackMode) -> Self {
        self.stack_mode = mode;
        self
    }

    /// Builder: set x-axis config.
    pub fn with_x_axis(mut self, cfg: AxisConfig) -> Self {
        self.x_axis = cfg;
        self
    }

    /// Builder: set y-axis config.
    pub fn with_y_axis(mut self, cfg: AxisConfig) -> Self {
        self.y_axis = cfg;
        self
    }

    /// Total number of data series.
    pub fn series_count(&self) -> usize {
        self.series.len()
    }

    /// Whether this is a radial chart (pie).
    pub fn is_radial(&self) -> bool {
        matches!(self.kind, ChartKind::Pie)
    }

    /// Whether this is a Cartesian chart (bar, line, area, scatter).
    pub fn is_cartesian(&self) -> bool {
        !self.is_radial()
    }
}

impl fmt::Display for ChartSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let title = self.title.as_deref().unwrap_or("(untitled)");
        write!(
            f,
            "Chart#{} {} \"{}\" ({} series)",
            self.id,
            self.kind,
            title,
            self.series.len()
        )
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chart_kind_display() {
        assert_eq!(ChartKind::Bar.to_string(), "Bar");
        assert_eq!(ChartKind::HorizontalBar.to_string(), "Horizontal Bar");
        assert_eq!(ChartKind::Line.to_string(), "Line");
        assert_eq!(ChartKind::Pie.to_string(), "Pie");
        assert_eq!(ChartKind::Area.to_string(), "Area");
        assert_eq!(ChartKind::Scatter.to_string(), "Scatter");
    }

    #[test]
    fn data_series_cell_count() {
        let s = DataSeries::new("Sales", (0, 0, 0, 4)); // A1:A5 → 5 cells
        assert_eq!(s.cell_count(), 5);
    }

    #[test]
    fn data_series_cell_coords() {
        let s = DataSeries::new("X", (1, 0, 2, 1)); // B1:C2
        let coords = s.cell_coords();
        assert_eq!(coords, vec![(1, 0), (2, 0), (1, 1), (2, 1)]);
    }

    #[test]
    fn data_series_with_color() {
        let s = DataSeries::new("Q1", (0, 0, 0, 3))
            .with_color(SeriesColor::rgb(255, 0, 0));
        assert_eq!(s.color.unwrap().r, 255);
    }

    #[test]
    fn series_color_to_f32() {
        let c = SeriesColor::new(128, 0, 255, 128);
        let f = c.to_f32();
        assert!((f[0] - 128.0 / 255.0).abs() < 0.001);
        assert!((f[2] - 1.0).abs() < 0.001);
        assert!((f[3] - 128.0 / 255.0).abs() < 0.001);
    }

    #[test]
    fn palette_wraps() {
        let a = palette_color(0);
        let b = palette_color(8);
        assert_eq!(a, b);
    }

    #[test]
    fn chart_spec_bar_factory() {
        let spec = ChartSpec::bar(1, "Revenue", (0, 0, 0, 5));
        assert_eq!(spec.kind, ChartKind::Bar);
        assert_eq!(spec.series.len(), 1);
        assert_eq!(spec.series[0].label, "Revenue");
    }

    #[test]
    fn chart_spec_display() {
        let spec = ChartSpec::bar(1, "Rev", (0, 0, 0, 5))
            .with_title("Sales Chart");
        assert_eq!(
            spec.to_string(),
            "Chart#1 Bar \"Sales Chart\" (1 series)"
        );
    }

    #[test]
    fn chart_spec_builders() {
        let spec = ChartSpec::line(2, "Trend", (0, 0, 0, 9))
            .with_title("Trend Line")
            .with_position(100.0, 200.0)
            .with_size(500.0, 400.0)
            .with_legend(LegendPosition::Right)
            .with_stack(StackMode::Stacked)
            .with_series(DataSeries::new("Extra", (1, 0, 1, 9)));

        assert_eq!(spec.title.as_deref(), Some("Trend Line"));
        assert_eq!(spec.position, (100.0, 200.0));
        assert_eq!(spec.size, (500.0, 400.0));
        assert_eq!(spec.legend, LegendPosition::Right);
        assert_eq!(spec.stack_mode, StackMode::Stacked);
        assert_eq!(spec.series.len(), 2);
    }

    #[test]
    fn is_radial() {
        assert!(ChartSpec::pie(1, "X", (0, 0, 0, 3)).is_radial());
        assert!(!ChartSpec::bar(1, "X", (0, 0, 0, 3)).is_radial());
    }

    #[test]
    fn is_cartesian() {
        assert!(ChartSpec::bar(1, "X", (0, 0, 0, 3)).is_cartesian());
        assert!(ChartSpec::scatter(1, "X", (0, 0, 0, 3)).is_cartesian());
        assert!(!ChartSpec::pie(1, "X", (0, 0, 0, 3)).is_cartesian());
    }

    #[test]
    fn axis_config_builder() {
        let ax = AxisConfig::default()
            .with_title("Revenue ($)")
            .with_range(0.0, 1000.0);
        assert_eq!(ax.title.as_deref(), Some("Revenue ($)"));
        assert_eq!(ax.min, Some(0.0));
        assert_eq!(ax.max, Some(1000.0));
    }

    #[test]
    fn category_source_auto() {
        let spec = ChartSpec::bar(1, "X", (0, 0, 0, 3));
        assert_eq!(spec.categories, CategorySource::Auto);
    }

    #[test]
    fn category_source_explicit() {
        let spec = ChartSpec::bar(1, "X", (0, 0, 0, 3))
            .with_categories(CategorySource::Explicit(vec![
                "Q1".into(),
                "Q2".into(),
                "Q3".into(),
                "Q4".into(),
            ]));
        if let CategorySource::Explicit(labels) = &spec.categories {
            assert_eq!(labels.len(), 4);
        } else {
            panic!("Expected Explicit");
        }
    }

    #[test]
    fn data_series_display() {
        let s = DataSeries::new("Sales", (0, 0, 2, 5));
        assert_eq!(s.to_string(), "\"Sales\" (0,0)→(2,5)");
    }

    #[test]
    fn series_color_display() {
        let c = SeriesColor::new(100, 200, 50, 128);
        assert_eq!(c.to_string(), "rgba(100,200,50,128)");
    }
}
