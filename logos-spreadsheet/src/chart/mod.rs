//! Charting module — turn spreadsheet data into visual charts.
//!
//! ## Architecture
//!
//! ```text
//! ChartSpec  ──▶  DataResolver  ──▶  ResolvedChart
//!                      │                    │
//!                      │                    ▼
//!                      │             compute_layout()
//!                      │                    │
//!                      │                    ▼
//!                      │              ChartLayout
//!                      │                    │
//!                      │                    ▼
//!                      │             render_chart()
//!                      │                    │
//!                      │                    ▼
//!                      │            ChartRenderData
//!                      │
//!                      ▼
//!                ChartEngine (orchestrator, caching, dirty-tracking)
//! ```
//!
//! ## Supported chart types
//!
//! | Kind | Module |
//! |------|--------|
//! | Bar (vertical & horizontal) | `layout::layout_bars` |
//! | Line | `layout::layout_lines` |
//! | Area | `layout::layout_lines` (fill mode) |
//! | Pie / Donut | `layout::layout_pie` |
//! | Scatter | `layout::layout_scatter` |

pub mod types;
pub mod data;
pub mod style;
pub mod layout;
pub mod render_data;
pub mod engine;

// Re-exports for convenience
pub use types::{
    AxisConfig, CategorySource, ChartId, ChartKind, ChartSpec, DataSeries,
    LegendPosition, SeriesColor, StackMode, DEFAULT_PALETTE, palette_color,
};

pub use data::{DataResolver, ResolvedChart, ResolvedSeries};

pub use style::{ChartFonts, ChartPadding, ChartStyle, ChartTheme};

pub use layout::{
    AxisTick, ChartLayout, LayoutLine, LayoutPoint, LayoutRect, LayoutSlice,
    LayoutText, LegendEntry, compute_layout,
};

pub use render_data::{
    ChartCircle, ChartLine as ChartRenderLine, ChartPolygon, ChartRect,
    ChartRenderData, ChartText, ChartTriangle, render_chart,
};

pub use engine::{ChartEngine, ChartHit};
