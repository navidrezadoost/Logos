//! Grid container property parser — the input gate for grid layout.
//!
//! Input:  raw grid container options (track definitions, cell map, alignment)
//! Output: typed `GridContainer` struct ready for the layout pipeline
//!
//! ## CSS Grid concepts mapped here
//!
//! ### Tracks
//! Each row/column is a `GridTrack` with a `GridTrackType` and optional value:
//! - `Fixed(px)`: explicit pixel size
//! - `Percent(%)`: percent of container size
//! - `Flex(fr)`:   fraction of remaining free space (like CSS `fr` unit)
//! - `Auto`:       content-sized (min-content / max-content)
//!
//! ### Cells
//! A `GridCell` occupies a (row, column) position with optional span.
//! Each cell holds zero or one shape ID, a placement mode, and
//! per-cell alignment overrides (`align-self`, `justify-self`).
//!
//! ### Alignment
//! Container-level alignment mirrors CSS Grid alignment properties:
//! - `justify-content` / `align-content`: distribute free space between tracks
//! - `justify-items` / `align-items`:    default child alignment within cells

use std::collections::HashMap;

/// UUID placeholder (would be `uuid::Uuid` in production)
pub type Uuid = u64;

// ---------------------------------------------------------------------------
// Track type
// ---------------------------------------------------------------------------

/// The sizing algorithm for a single grid track (row or column).
///
/// Maps directly to CSS Grid track sizing keywords:
/// - `Fixed` → explicit pixel length (`100px`)
/// - `Percent` → fraction of container size (`50%`)
/// - `Flex` → fraction unit (`1fr`, `2fr`)
/// - `Auto` → content-sized (uses child min/max content)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GridTrackType {
    /// Fixed pixel size.
    Fixed,
    /// Percentage of container dimension.
    Percent,
    /// Fractional unit — share of remaining free space.
    Flex,
    /// Content-sized track (auto).
    Auto,
    /// Subgrid — inherit track definitions from the parent grid.
    /// The item itself does not define tracks; instead its children align to
    /// the parent grid's track lines within the item's occupied span.
    Subgrid,
}

impl GridTrackType {
    /// Parse from Clojure keyword string.
    ///
    /// ```rust
    /// use logos_layout::grid::GridTrackType;
    /// assert_eq!(GridTrackType::from_str("flex"),    GridTrackType::Flex);
    /// assert_eq!(GridTrackType::from_str("fixed"),   GridTrackType::Fixed);
    /// assert_eq!(GridTrackType::from_str("percent"), GridTrackType::Percent);
    /// assert_eq!(GridTrackType::from_str("auto"),    GridTrackType::Auto);
    /// assert_eq!(GridTrackType::from_str("unknown"), GridTrackType::Auto);
    /// ```
    pub fn from_str(s: &str) -> Self {
        match s {
            "fixed"   => GridTrackType::Fixed,
            "percent" => GridTrackType::Percent,
            "flex"    => GridTrackType::Flex,
            "auto"    => GridTrackType::Auto,
            "subgrid" => GridTrackType::Subgrid,
            _         => GridTrackType::Auto,
        }
    }

    /// Return string representation (round-trips with `from_str`).
    pub fn as_str(&self) -> &'static str {
        match self {
            GridTrackType::Fixed   => "fixed",
            GridTrackType::Percent => "percent",
            GridTrackType::Flex    => "flex",
            GridTrackType::Auto    => "auto",
            GridTrackType::Subgrid => "subgrid",
        }
    }

    /// Returns `true` if this track inherits definitions from the parent grid.
    pub fn is_subgrid(&self) -> bool {
        matches!(self, GridTrackType::Subgrid)
    }
}

impl Default for GridTrackType {
    fn default() -> Self {
        GridTrackType::Auto
    }
}

// ---------------------------------------------------------------------------
// GridTrack
// ---------------------------------------------------------------------------

/// A single grid track definition (one row or one column).
///
/// Analogous to one entry in `layout-grid-rows` / `layout-grid-columns`.
///
/// ```rust
/// use logos_layout::grid::{GridTrack, GridTrackType};
///
/// let t = GridTrack::fixed(120.0);
/// assert_eq!(t.track_type, GridTrackType::Fixed);
/// assert_eq!(t.value, Some(120.0));
///
/// let fr = GridTrack::flex(1.0);
/// assert_eq!(fr.track_type, GridTrackType::Flex);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct GridTrack {
    /// Sizing algorithm for this track.
    pub track_type: GridTrackType,
    /// Numeric value (pixels for Fixed, fraction for Flex, percent for Percent).
    /// `None` for Auto tracks.
    pub value: Option<f64>,
}

impl GridTrack {
    /// Construct a fixed-size track.
    pub fn fixed(px: f64) -> Self {
        GridTrack { track_type: GridTrackType::Fixed, value: Some(px) }
    }

    /// Construct a flex (`fr`) track.
    pub fn flex(fr: f64) -> Self {
        GridTrack { track_type: GridTrackType::Flex, value: Some(fr) }
    }

    /// Construct a percent track.
    pub fn percent(pct: f64) -> Self {
        GridTrack { track_type: GridTrackType::Percent, value: Some(pct) }
    }

    /// Construct an auto-sized track.
    pub fn auto() -> Self {
        GridTrack { track_type: GridTrackType::Auto, value: None }
    }

    /// Parse from a (type_str, value) pair.
    pub fn from_parts(type_str: &str, value: Option<f64>) -> Self {
        GridTrack {
            track_type: GridTrackType::from_str(type_str),
            value,
        }
    }

    /// Resolve size in pixels given container dimension.
    ///
    /// Returns `None` for `Flex` and `Auto` tracks — these are resolved
    /// later in the positions pipeline once free space is known.
    ///
    /// ```rust
    /// use logos_layout::grid::{GridTrack};
    ///
    /// assert_eq!(GridTrack::fixed(80.0).resolve_px(400.0), Some(80.0));
    /// assert_eq!(GridTrack::percent(25.0).resolve_px(400.0), Some(100.0));
    /// assert_eq!(GridTrack::flex(1.0).resolve_px(400.0), None);
    /// assert_eq!(GridTrack::auto().resolve_px(400.0), None);
    /// ```
    pub fn resolve_px(&self, container_size: f64) -> Option<f64> {
        match self.track_type {
            GridTrackType::Fixed => self.value,
            GridTrackType::Percent => self.value.map(|pct| container_size * pct / 100.0),
            // Flex and Auto are resolved in subsequent passes.
            // Subgrid inherits track definitions from the parent; its own size
            // is determined by the parent's track allocation, not by a local px value.
            GridTrackType::Flex | GridTrackType::Auto | GridTrackType::Subgrid => None,
        }
    }
}

impl Default for GridTrack {
    fn default() -> Self {
        GridTrack::auto()
    }
}

// ---------------------------------------------------------------------------
// Cell placement & alignment
// ---------------------------------------------------------------------------

/// How a cell was placed in the grid.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum GridPosition {
    /// Cell placed automatically by the auto-placement algorithm.
    #[default]
    Auto,
    /// Cell placed explicitly by the user.
    Manual,
    /// Cell belongs to a named area.
    Area,
}

impl GridPosition {
    /// Parse from string.
    ///
    /// ```rust
    /// use logos_layout::grid::GridPosition;
    /// assert_eq!(GridPosition::from_str("auto"),   GridPosition::Auto);
    /// assert_eq!(GridPosition::from_str("manual"), GridPosition::Manual);
    /// assert_eq!(GridPosition::from_str("area"),   GridPosition::Area);
    /// ```
    pub fn from_str(s: &str) -> Self {
        match s {
            "manual" => GridPosition::Manual,
            "area"   => GridPosition::Area,
            _        => GridPosition::Auto,
        }
    }
}

/// Per-cell `align-self` override (block / column axis).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum TrackAlignSelf {
    /// Inherit from container `align-items`.
    #[default]
    Auto,
    Start,
    Center,
    End,
    Stretch,
}

impl TrackAlignSelf {
    pub fn from_str(s: &str) -> Self {
        match s {
            "start"   => TrackAlignSelf::Start,
            "center"  => TrackAlignSelf::Center,
            "end"     => TrackAlignSelf::End,
            "stretch" => TrackAlignSelf::Stretch,
            _         => TrackAlignSelf::Auto,
        }
    }
}

/// Per-cell `justify-self` override (inline / row axis).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum TrackJustifySelf {
    /// Inherit from container `justify-items`.
    #[default]
    Auto,
    Start,
    Center,
    End,
    Stretch,
}

impl TrackJustifySelf {
    pub fn from_str(s: &str) -> Self {
        match s {
            "start"   => TrackJustifySelf::Start,
            "center"  => TrackJustifySelf::Center,
            "end"     => TrackJustifySelf::End,
            "stretch" => TrackJustifySelf::Stretch,
            _         => TrackJustifySelf::Auto,
        }
    }
}

// ---------------------------------------------------------------------------
// GridCell
// ---------------------------------------------------------------------------

/// A single cell in the grid, occupying (row, column) with optional span.
///
/// Holds the shape assigned to this cell (zero or one), its placement mode,
/// and per-cell alignment overrides.
///
/// Analogous to one entry in `layout-grid-cells`.
#[derive(Debug, Clone, PartialEq)]
pub struct GridCell {
    /// Cell UUID (stable identifier).
    pub id: Uuid,
    /// 1-based row index.
    pub row: usize,
    /// 1-based column index.
    pub column: usize,
    /// Number of rows this cell spans (1 = normal).
    pub row_span: usize,
    /// Number of columns this cell spans (1 = normal).
    pub column_span: usize,
    /// Named area this cell belongs to (if any).
    pub area_name: Option<String>,
    /// Placement mode.
    pub position: GridPosition,
    /// Per-cell align override.
    pub align_self: TrackAlignSelf,
    /// Per-cell justify override.
    pub justify_self: TrackJustifySelf,
    /// Shape(s) assigned to this cell (0 or 1 in practice).
    pub shapes: Vec<Uuid>,
}

impl GridCell {
    /// Construct a minimal cell at (row, column) with no span and no shape.
    pub fn new(id: Uuid, row: usize, column: usize) -> Self {
        GridCell {
            id,
            row,
            column,
            row_span: 1,
            column_span: 1,
            area_name: None,
            position: GridPosition::Auto,
            align_self: TrackAlignSelf::Auto,
            justify_self: TrackJustifySelf::Auto,
            shapes: vec![],
        }
    }

    /// Assign a shape to this cell.
    pub fn with_shape(mut self, shape_id: Uuid) -> Self {
        self.shapes = vec![shape_id];
        self
    }

    /// Set span values.
    pub fn with_span(mut self, row_span: usize, column_span: usize) -> Self {
        self.row_span = row_span;
        self.column_span = column_span;
        self
    }
}

// ---------------------------------------------------------------------------
// Container-level alignment enums
// ---------------------------------------------------------------------------

/// `justify-content` — distribute free space between columns.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum JustifyContent {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
    Stretch,
}

impl JustifyContent {
    /// Parse from string.
    ///
    /// ```rust
    /// use logos_layout::grid::JustifyContent;
    /// assert_eq!(JustifyContent::from_str("start"),        JustifyContent::Start);
    /// assert_eq!(JustifyContent::from_str("center"),       JustifyContent::Center);
    /// assert_eq!(JustifyContent::from_str("end"),          JustifyContent::End);
    /// assert_eq!(JustifyContent::from_str("space-between"),JustifyContent::SpaceBetween);
    /// assert_eq!(JustifyContent::from_str("space-around"), JustifyContent::SpaceAround);
    /// assert_eq!(JustifyContent::from_str("space-evenly"), JustifyContent::SpaceEvenly);
    /// assert_eq!(JustifyContent::from_str("stretch"),      JustifyContent::Stretch);
    /// ```
    pub fn from_str(s: &str) -> Self {
        match s {
            "center"        => JustifyContent::Center,
            "end"           => JustifyContent::End,
            "space-between" => JustifyContent::SpaceBetween,
            "space-around"  => JustifyContent::SpaceAround,
            "space-evenly"  => JustifyContent::SpaceEvenly,
            "stretch"       => JustifyContent::Stretch,
            _               => JustifyContent::Start,
        }
    }
}

/// `align-content` — distribute free space between rows.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum AlignContent {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
    Stretch,
}

impl AlignContent {
    /// Parse from string.
    ///
    /// ```rust
    /// use logos_layout::grid::AlignContent;
    /// assert_eq!(AlignContent::from_str("start"),   AlignContent::Start);
    /// assert_eq!(AlignContent::from_str("stretch"), AlignContent::Stretch);
    /// assert_eq!(AlignContent::from_str("center"),  AlignContent::Center);
    /// ```
    pub fn from_str(s: &str) -> Self {
        match s {
            "center"        => AlignContent::Center,
            "end"           => AlignContent::End,
            "space-between" => AlignContent::SpaceBetween,
            "space-around"  => AlignContent::SpaceAround,
            "space-evenly"  => AlignContent::SpaceEvenly,
            "stretch"       => AlignContent::Stretch,
            _               => AlignContent::Start,
        }
    }
}

/// `align-items` — default child alignment in the block (row) axis within cells.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum AlignItems {
    #[default]
    Start,
    Center,
    End,
    Stretch,
}

impl AlignItems {
    pub fn from_str(s: &str) -> Self {
        match s {
            "center"  => AlignItems::Center,
            "end"     => AlignItems::End,
            "stretch" => AlignItems::Stretch,
            _         => AlignItems::Start,
        }
    }
}

/// `justify-items` — default child alignment in the inline (column) axis within cells.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum JustifyItems {
    #[default]
    Start,
    Center,
    End,
    Stretch,
}

impl JustifyItems {
    pub fn from_str(s: &str) -> Self {
        match s {
            "center"  => JustifyItems::Center,
            "end"     => JustifyItems::End,
            "stretch" => JustifyItems::Stretch,
            _         => JustifyItems::Start,
        }
    }
}

// ---------------------------------------------------------------------------
// Grid direction (auto-placement flow)
// ---------------------------------------------------------------------------

/// Auto-placement flow direction.
///
/// - `Row`: fill rows first (default CSS `grid-auto-flow: row`)
/// - `Column`: fill columns first (`grid-auto-flow: column`)
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum GridDirection {
    #[default]
    Row,
    Column,
}

impl GridDirection {
    pub fn from_str(s: &str) -> Self {
        match s {
            "column" => GridDirection::Column,
            _        => GridDirection::Row,
        }
    }
}

// ---------------------------------------------------------------------------
// GridContainer — the top-level parsed struct
// ---------------------------------------------------------------------------

/// Parsed grid container properties.
///
/// Holds everything the layout pipeline needs to compute track sizes, assign
/// children to cells, and position each child.
///
/// # Example
/// ```rust
/// use logos_layout::grid::{GridContainer, GridTrack, GridCell};
///
/// let mut container = GridContainer::default();
/// container.columns = vec![GridTrack::fixed(100.0), GridTrack::flex(1.0)];
/// container.rows    = vec![GridTrack::fixed(80.0),  GridTrack::fixed(80.0)];
/// container.row_gap = 8.0;
/// container.column_gap = 8.0;
///
/// assert_eq!(container.columns.len(), 2);
/// assert_eq!(container.rows.len(), 2);
/// assert_eq!(container.column_gap, 8.0);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct GridContainer {
    // ------------------------------------------------------------------
    // Track definitions
    // ------------------------------------------------------------------
    /// Column track definitions (left to right).
    pub columns: Vec<GridTrack>,
    /// Row track definitions (top to bottom).
    pub rows: Vec<GridTrack>,

    // ------------------------------------------------------------------
    // Gaps
    // ------------------------------------------------------------------
    /// Gap between rows (row-gap).
    pub row_gap: f64,
    /// Gap between columns (column-gap).
    pub column_gap: f64,

    // ------------------------------------------------------------------
    // Padding (top, right, bottom, left)
    // ------------------------------------------------------------------
    pub padding: (f64, f64, f64, f64),

    // ------------------------------------------------------------------
    // Container-level alignment
    // ------------------------------------------------------------------
    pub justify_content: JustifyContent,
    pub align_content: AlignContent,
    pub justify_items: JustifyItems,
    pub align_items: AlignItems,

    // ------------------------------------------------------------------
    // Auto-placement
    // ------------------------------------------------------------------
    pub direction: GridDirection,

    // ------------------------------------------------------------------
    // Cell map
    // ------------------------------------------------------------------
    /// All cells: cell_id → GridCell
    pub cells: HashMap<Uuid, GridCell>,
}

impl Default for GridContainer {
    fn default() -> Self {
        GridContainer {
            columns: vec![],
            rows: vec![],
            row_gap: 0.0,
            column_gap: 0.0,
            padding: (0.0, 0.0, 0.0, 0.0),
            justify_content: JustifyContent::default(),
            align_content: AlignContent::default(),
            justify_items: JustifyItems::default(),
            align_items: AlignItems::default(),
            direction: GridDirection::default(),
            cells: HashMap::new(),
        }
    }
}

impl GridContainer {
    /// Construct from raw option values (analogous to `FlexContainer::from_options`).
    ///
    /// All parameters are optional; missing values use defaults.
    #[allow(clippy::too_many_arguments)]
    pub fn from_options(
        direction: Option<&str>,
        justify_content: Option<&str>,
        align_content: Option<&str>,
        justify_items: Option<&str>,
        align_items: Option<&str>,
        row_gap: Option<f64>,
        column_gap: Option<f64>,
        padding_top: Option<f64>,
        padding_right: Option<f64>,
        padding_bottom: Option<f64>,
        padding_left: Option<f64>,
        columns: Vec<GridTrack>,
        rows: Vec<GridTrack>,
        cells: HashMap<Uuid, GridCell>,
    ) -> Self {
        GridContainer {
            direction: direction.map(GridDirection::from_str).unwrap_or_default(),
            justify_content: justify_content.map(JustifyContent::from_str).unwrap_or_default(),
            align_content: align_content.map(AlignContent::from_str).unwrap_or_default(),
            justify_items: justify_items.map(JustifyItems::from_str).unwrap_or_default(),
            align_items: align_items.map(AlignItems::from_str).unwrap_or_default(),
            row_gap: row_gap.unwrap_or(0.0),
            column_gap: column_gap.unwrap_or(0.0),
            padding: (
                padding_top.unwrap_or(0.0),
                padding_right.unwrap_or(0.0),
                padding_bottom.unwrap_or(0.0),
                padding_left.unwrap_or(0.0),
            ),
            columns,
            rows,
            cells,
        }
    }

    /// Total number of columns.
    pub fn num_columns(&self) -> usize {
        self.columns.len()
    }

    /// Total number of rows.
    pub fn num_rows(&self) -> usize {
        self.rows.len()
    }

    /// Find the cell at (row, column) if it exists.
    pub fn cell_at(&self, row: usize, column: usize) -> Option<&GridCell> {
        self.cells.values().find(|c| c.row == row && c.column == column)
    }

    /// Iterate cells in row-major order (row 1 col 1, row 1 col 2, ...).
    pub fn cells_row_major(&self) -> Vec<&GridCell> {
        let mut cells: Vec<&GridCell> = self.cells.values().collect();
        cells.sort_by_key(|c| (c.row, c.column));
        cells
    }

    /// Return all `fr` column indices and their flex values.
    pub fn flex_columns(&self) -> Vec<(usize, f64)> {
        self.columns
            .iter()
            .enumerate()
            .filter_map(|(i, t)| {
                if t.track_type == GridTrackType::Flex {
                    Some((i, t.value.unwrap_or(1.0)))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Return all `fr` row indices and their flex values.
    pub fn flex_rows(&self) -> Vec<(usize, f64)> {
        self.rows
            .iter()
            .enumerate()
            .filter_map(|(i, t)| {
                if t.track_type == GridTrackType::Flex {
                    Some((i, t.value.unwrap_or(1.0)))
                } else {
                    None
                }
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // GridTrackType parsing
    // ------------------------------------------------------------------

    #[test]
    fn test_track_type_roundtrip() {
        for (s, expected) in [
            ("fixed",   GridTrackType::Fixed),
            ("percent", GridTrackType::Percent),
            ("flex",    GridTrackType::Flex),
            ("auto",    GridTrackType::Auto),
        ] {
            let parsed = GridTrackType::from_str(s);
            assert_eq!(parsed, expected, "from_str({s})");
            assert_eq!(parsed.as_str(), s, "as_str() roundtrip for {s}");
        }
    }

    #[test]
    fn test_track_type_unknown_defaults_auto() {
        assert_eq!(GridTrackType::from_str("bogus"), GridTrackType::Auto);
        assert_eq!(GridTrackType::from_str(""),      GridTrackType::Auto);
    }

    // ------------------------------------------------------------------
    // GridTrack constructors
    // ------------------------------------------------------------------

    #[test]
    fn test_track_fixed() {
        let t = GridTrack::fixed(120.0);
        assert_eq!(t.track_type, GridTrackType::Fixed);
        assert_eq!(t.value, Some(120.0));
    }

    #[test]
    fn test_track_flex() {
        let t = GridTrack::flex(2.0);
        assert_eq!(t.track_type, GridTrackType::Flex);
        assert_eq!(t.value, Some(2.0));
    }

    #[test]
    fn test_track_percent() {
        let t = GridTrack::percent(50.0);
        assert_eq!(t.track_type, GridTrackType::Percent);
        assert_eq!(t.value, Some(50.0));
    }

    #[test]
    fn test_track_auto() {
        let t = GridTrack::auto();
        assert_eq!(t.track_type, GridTrackType::Auto);
        assert_eq!(t.value, None);
    }

    // ------------------------------------------------------------------
    // GridTrack::resolve_px
    // ------------------------------------------------------------------

    #[test]
    fn test_resolve_px_fixed() {
        let t = GridTrack::fixed(80.0);
        assert_eq!(t.resolve_px(400.0), Some(80.0));
        // Fixed is independent of container size
        assert_eq!(t.resolve_px(1000.0), Some(80.0));
    }

    #[test]
    fn test_resolve_px_percent() {
        let t = GridTrack::percent(25.0);
        assert_eq!(t.resolve_px(400.0), Some(100.0));
        assert_eq!(t.resolve_px(200.0), Some(50.0));
    }

    #[test]
    fn test_resolve_px_flex_is_none() {
        let t = GridTrack::flex(1.0);
        assert_eq!(t.resolve_px(400.0), None);
    }

    #[test]
    fn test_resolve_px_auto_is_none() {
        let t = GridTrack::auto();
        assert_eq!(t.resolve_px(400.0), None);
    }

    // ------------------------------------------------------------------
    // GridPosition parsing
    // ------------------------------------------------------------------

    #[test]
    fn test_grid_position_from_str() {
        assert_eq!(GridPosition::from_str("auto"),   GridPosition::Auto);
        assert_eq!(GridPosition::from_str("manual"), GridPosition::Manual);
        assert_eq!(GridPosition::from_str("area"),   GridPosition::Area);
        assert_eq!(GridPosition::from_str("?"),      GridPosition::Auto);
    }

    // ------------------------------------------------------------------
    // Per-cell alignment parsing
    // ------------------------------------------------------------------

    #[test]
    fn test_track_align_self_from_str() {
        assert_eq!(TrackAlignSelf::from_str("auto"),    TrackAlignSelf::Auto);
        assert_eq!(TrackAlignSelf::from_str("start"),   TrackAlignSelf::Start);
        assert_eq!(TrackAlignSelf::from_str("center"),  TrackAlignSelf::Center);
        assert_eq!(TrackAlignSelf::from_str("end"),     TrackAlignSelf::End);
        assert_eq!(TrackAlignSelf::from_str("stretch"), TrackAlignSelf::Stretch);
    }

    #[test]
    fn test_track_justify_self_from_str() {
        assert_eq!(TrackJustifySelf::from_str("auto"),    TrackJustifySelf::Auto);
        assert_eq!(TrackJustifySelf::from_str("start"),   TrackJustifySelf::Start);
        assert_eq!(TrackJustifySelf::from_str("center"),  TrackJustifySelf::Center);
        assert_eq!(TrackJustifySelf::from_str("end"),     TrackJustifySelf::End);
        assert_eq!(TrackJustifySelf::from_str("stretch"), TrackJustifySelf::Stretch);
    }

    // ------------------------------------------------------------------
    // Container-level alignment parsing
    // ------------------------------------------------------------------

    #[test]
    fn test_justify_content_from_str() {
        let cases = [
            ("start",         JustifyContent::Start),
            ("center",        JustifyContent::Center),
            ("end",           JustifyContent::End),
            ("space-between", JustifyContent::SpaceBetween),
            ("space-around",  JustifyContent::SpaceAround),
            ("space-evenly",  JustifyContent::SpaceEvenly),
            ("stretch",       JustifyContent::Stretch),
        ];
        for (s, expected) in cases {
            assert_eq!(JustifyContent::from_str(s), expected, "JustifyContent::{s}");
        }
    }

    #[test]
    fn test_align_content_from_str() {
        let cases = [
            ("start",         AlignContent::Start),
            ("center",        AlignContent::Center),
            ("end",           AlignContent::End),
            ("space-between", AlignContent::SpaceBetween),
            ("space-around",  AlignContent::SpaceAround),
            ("space-evenly",  AlignContent::SpaceEvenly),
            ("stretch",       AlignContent::Stretch),
        ];
        for (s, expected) in cases {
            assert_eq!(AlignContent::from_str(s), expected, "AlignContent::{s}");
        }
    }

    #[test]
    fn test_align_items_from_str() {
        assert_eq!(AlignItems::from_str("start"),   AlignItems::Start);
        assert_eq!(AlignItems::from_str("center"),  AlignItems::Center);
        assert_eq!(AlignItems::from_str("end"),     AlignItems::End);
        assert_eq!(AlignItems::from_str("stretch"), AlignItems::Stretch);
        assert_eq!(AlignItems::from_str("?"),       AlignItems::Start);
    }

    #[test]
    fn test_justify_items_from_str() {
        assert_eq!(JustifyItems::from_str("start"),   JustifyItems::Start);
        assert_eq!(JustifyItems::from_str("center"),  JustifyItems::Center);
        assert_eq!(JustifyItems::from_str("end"),     JustifyItems::End);
        assert_eq!(JustifyItems::from_str("stretch"), JustifyItems::Stretch);
    }

    // ------------------------------------------------------------------
    // GridDirection
    // ------------------------------------------------------------------

    #[test]
    fn test_grid_direction_from_str() {
        assert_eq!(GridDirection::from_str("row"),    GridDirection::Row);
        assert_eq!(GridDirection::from_str("column"), GridDirection::Column);
        assert_eq!(GridDirection::from_str("?"),      GridDirection::Row);
    }

    // ------------------------------------------------------------------
    // GridCell
    // ------------------------------------------------------------------

    #[test]
    fn test_grid_cell_new() {
        let cell = GridCell::new(42, 2, 3);
        assert_eq!(cell.id, 42);
        assert_eq!(cell.row, 2);
        assert_eq!(cell.column, 3);
        assert_eq!(cell.row_span, 1);
        assert_eq!(cell.column_span, 1);
        assert!(cell.shapes.is_empty());
        assert_eq!(cell.position, GridPosition::Auto);
    }

    #[test]
    fn test_grid_cell_with_shape() {
        let cell = GridCell::new(1, 1, 1).with_shape(99);
        assert_eq!(cell.shapes, vec![99]);
    }

    #[test]
    fn test_grid_cell_with_span() {
        let cell = GridCell::new(1, 1, 1).with_span(2, 3);
        assert_eq!(cell.row_span, 2);
        assert_eq!(cell.column_span, 3);
    }

    // ------------------------------------------------------------------
    // GridContainer
    // ------------------------------------------------------------------

    #[test]
    fn test_container_default() {
        let c = GridContainer::default();
        assert!(c.columns.is_empty());
        assert!(c.rows.is_empty());
        assert_eq!(c.row_gap, 0.0);
        assert_eq!(c.column_gap, 0.0);
        assert_eq!(c.padding, (0.0, 0.0, 0.0, 0.0));
        assert_eq!(c.direction, GridDirection::Row);
    }

    #[test]
    fn test_container_num_columns_rows() {
        let mut c = GridContainer::default();
        c.columns = vec![GridTrack::fixed(100.0), GridTrack::flex(1.0)];
        c.rows    = vec![GridTrack::fixed(80.0)];
        assert_eq!(c.num_columns(), 2);
        assert_eq!(c.num_rows(), 1);
    }

    #[test]
    fn test_container_flex_columns() {
        let mut c = GridContainer::default();
        c.columns = vec![
            GridTrack::fixed(100.0),
            GridTrack::flex(1.0),
            GridTrack::flex(2.0),
        ];
        let flex = c.flex_columns();
        assert_eq!(flex.len(), 2);
        assert_eq!(flex[0], (1, 1.0));
        assert_eq!(flex[1], (2, 2.0));
    }

    #[test]
    fn test_container_flex_rows() {
        let mut c = GridContainer::default();
        c.rows = vec![
            GridTrack::auto(),
            GridTrack::flex(3.0),
        ];
        let flex = c.flex_rows();
        assert_eq!(flex.len(), 1);
        assert_eq!(flex[0], (1, 3.0));
    }

    #[test]
    fn test_container_cell_at() {
        let mut c = GridContainer::default();
        let cell = GridCell::new(1, 2, 3);
        c.cells.insert(1, cell);
        assert!(c.cell_at(2, 3).is_some());
        assert!(c.cell_at(1, 1).is_none());
    }

    #[test]
    fn test_container_cells_row_major() {
        let mut c = GridContainer::default();
        c.cells.insert(1, GridCell::new(1, 2, 1));
        c.cells.insert(2, GridCell::new(2, 1, 2));
        c.cells.insert(3, GridCell::new(3, 1, 1));
        let ordered = c.cells_row_major();
        assert_eq!(ordered[0].id, 3); // row 1, col 1
        assert_eq!(ordered[1].id, 2); // row 1, col 2
        assert_eq!(ordered[2].id, 1); // row 2, col 1
    }

    #[test]
    fn test_container_from_options() {
        let c = GridContainer::from_options(
            Some("column"),
            Some("center"),
            Some("end"),
            Some("stretch"),
            Some("start"),
            Some(10.0),
            Some(8.0),
            Some(4.0),
            Some(4.0),
            Some(4.0),
            Some(4.0),
            vec![GridTrack::fixed(100.0)],
            vec![GridTrack::flex(1.0)],
            HashMap::new(),
        );
        assert_eq!(c.direction, GridDirection::Column);
        assert_eq!(c.justify_content, JustifyContent::Center);
        assert_eq!(c.align_content, AlignContent::End);
        assert_eq!(c.justify_items, JustifyItems::Stretch);
        assert_eq!(c.align_items, AlignItems::Start);
        assert_eq!(c.row_gap, 10.0);
        assert_eq!(c.column_gap, 8.0);
        assert_eq!(c.padding, (4.0, 4.0, 4.0, 4.0));
        assert_eq!(c.columns.len(), 1);
        assert_eq!(c.rows.len(), 1);
    }

    // ------------------------------------------------------------------
    // GridTrack::from_parts
    // ------------------------------------------------------------------

    #[test]
    fn test_track_from_parts_fixed() {
        let t = GridTrack::from_parts("fixed", Some(120.0));
        assert_eq!(t.track_type, GridTrackType::Fixed);
        assert_eq!(t.value, Some(120.0));
    }

    #[test]
    fn test_track_from_parts_flex_no_value_defaults_1fr() {
        // When flex value is None, resolve should return None (unresolved)
        let t = GridTrack { track_type: GridTrackType::Flex, value: None };
        assert_eq!(t.resolve_px(400.0), None);
    }

    // ------------------------------------------------------------------
    // P4.3: Subgrid track type
    // ------------------------------------------------------------------

    /// `Subgrid` parses from "subgrid", reports is_subgrid(), and resolves
    /// to `None` (size comes from the parent grid, not a local px value).
    #[test]
    fn test_subgrid_track_type() {
        let t = GridTrackType::from_str("subgrid");
        assert_eq!(t, GridTrackType::Subgrid);
        assert!(t.is_subgrid());
        assert_eq!(t.as_str(), "subgrid");

        let track = GridTrack { track_type: GridTrackType::Subgrid, value: None };
        // resolve_px must return None for subgrid (inherited from parent).
        assert_eq!(
            track.resolve_px(500.0),
            None,
            "subgrid track size is determined by parent, not locally"
        );
    }

    /// `GridTrack::from_parts("subgrid", None)` produces a Subgrid track.
    #[test]
    fn test_track_from_parts_subgrid() {
        let t = GridTrack::from_parts("subgrid", None);
        assert_eq!(t.track_type, GridTrackType::Subgrid);
        assert!(t.track_type.is_subgrid());
    }
}
