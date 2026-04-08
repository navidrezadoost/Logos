//! Repeat-Grid layout primitive.
//!
//! A `RepeatGrid` arranges copies of a set of *template layers* in a
//! regular rows × columns grid with configurable gutters.  Each cell can
//! receive typed *data overrides* that update specific properties (text,
//! fill colour, image URL …) without touching the shared template structure.
//!
//! This mirrors the Adobe XD / Canva *Repeat Grid* feature.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Data override ─────────────────────────────────────────────────────────────

/// A per-cell property override for one layer inside the grid cell.
///
/// `layer_path` is a dot-separated path from the cell root, e.g.
/// `"background.fill"` or `"label.text"`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DataOverride {
    /// Zero-based row index.
    pub row: u32,
    /// Zero-based column index.
    pub col: u32,
    /// Dot-separated property path inside the template hierarchy.
    pub layer_path: String,
    /// The replacement value (text, colour, image URI, …).
    pub value: serde_json::Value,
}

impl DataOverride {
    /// Create a new data override.
    pub fn new(
        row: u32,
        col: u32,
        layer_path: impl Into<String>,
        value: serde_json::Value,
    ) -> Self {
        Self {
            row,
            col,
            layer_path: layer_path.into(),
            value,
        }
    }

    /// Linear cell index = `row * columns + col` (caller must supply `columns`).
    pub fn cell_index(&self, columns: u32) -> usize {
        (self.row * columns + self.col) as usize
    }
}

// ── DataSource ──────────────────────────────────────────────────────────────────

/// A named data source that can be attached to a [`RepeatGrid`] and used to
/// automatically populate cell overrides via [`RepeatGrid::auto_fill`].
///
/// `column` identifies which layer-path property this source binds to (e.g.
/// `"label.text"` or `"background.fill"`).  Values cycle through `values`
/// if the grid has more cells than values in the list.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DataSource {
    /// Unique human-readable name (used by [`RepeatGrid::detach_data_source`]).
    pub name: String,
    /// Dot-separated layer-path this source binds to (e.g. `"label.text"`).
    pub column: String,
    /// Ordered list of values to distribute across cells.
    pub values: Vec<serde_json::Value>,
}

impl DataSource {
    /// Create a new data source.
    pub fn new(
        name: impl Into<String>,
        column: impl Into<String>,
        values: Vec<serde_json::Value>,
    ) -> Self {
        Self {
            name: name.into(),
            column: column.into(),
            values,
        }
    }

    /// Return the value for the given linear cell index, cycling if needed.
    ///
    /// Returns `serde_json::Value::Null` if `values` is empty.
    pub fn value_for_index(&self, idx: usize) -> &serde_json::Value {
        if self.values.is_empty() {
            return &serde_json::Value::Null;
        }
        &self.values[idx % self.values.len()]
    }

    /// Number of values in this source.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// `true` if the source has no values.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

// ── RepeatGrid ────────────────────────────────────────────────────────────────

/// A grid that repeats a set of template layers in `rows × columns` cells.
///
/// **Coordinates** — the grid's origin is at `(origin_x, origin_y)` in the
/// parent frame's local space.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RepeatGrid {
    /// Unique identifier for this grid instance.
    pub id: Uuid,
    /// Number of rows.
    pub rows: u32,
    /// Number of columns.
    pub columns: u32,
    /// Vertical gap between rows (px).
    pub row_gap: f32,
    /// Horizontal gap between columns (px).
    pub col_gap: f32,
    /// (width, height) of each individual cell.
    pub cell_size: (f32, f32),
    /// IDs of the template layers (from the host page/frame) that are
    /// tiled into every cell.
    pub template_layer_ids: Vec<Uuid>,
    /// Per-cell property overrides applied on top of the template.
    pub data_overrides: Vec<DataOverride>,
    /// Position of the grid's top-left corner in parent-local coordinates.
    pub origin_x: f32,
    /// Position of the grid's top-left corner in parent-local coordinates.
    pub origin_y: f32,
    /// Named data sources attached to this grid.
    pub data_sources: Vec<DataSource>,
}

impl RepeatGrid {
    /// Create a new `RepeatGrid` with the given dimensions and cell size.
    pub fn new(
        rows: u32,
        columns: u32,
        cell_width: f32,
        cell_height: f32,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            rows,
            columns,
            row_gap: 0.0,
            col_gap: 0.0,
            cell_size: (cell_width, cell_height),
            template_layer_ids: Vec::new(),
            data_overrides: Vec::new(),
            origin_x: 0.0,
            origin_y: 0.0,
            data_sources: Vec::new(),
        }
    }

    /// Set horizontal and vertical gutters between cells.
    pub fn with_gap(mut self, col_gap: f32, row_gap: f32) -> Self {
        self.col_gap = col_gap;
        self.row_gap = row_gap;
        self
    }

    /// Set the grid's top-left origin.
    pub fn with_origin(mut self, x: f32, y: f32) -> Self {
        self.origin_x = x;
        self.origin_y = y;
        self
    }

    // ── Template management ────────────────────────────────────────────────

    /// Add a layer ID to the template.  No-op if already present.
    pub fn add_template_layer(&mut self, layer_id: Uuid) {
        if !self.template_layer_ids.contains(&layer_id) {
            self.template_layer_ids.push(layer_id);
        }
    }

    /// Remove a layer ID from the template.  Has no effect if not found.
    pub fn remove_template_layer(&mut self, layer_id: &Uuid) {
        self.template_layer_ids.retain(|id| id != layer_id);
    }

    /// Total number of template layer IDs.
    pub fn template_count(&self) -> usize {
        self.template_layer_ids.len()
    }

    // ── Override management ────────────────────────────────────────────────

    /// Insert or replace a data override for `(row, col, layer_path)`.
    ///
    /// If an override for the same `(row, col, layer_path)` already exists it
    /// is replaced; otherwise a new one is appended.
    pub fn set_data_override(
        &mut self,
        row: u32,
        col: u32,
        layer_path: impl Into<String>,
        value: serde_json::Value,
    ) {
        let path = layer_path.into();
        if let Some(existing) = self
            .data_overrides
            .iter_mut()
            .find(|o| o.row == row && o.col == col && o.layer_path == path)
        {
            existing.value = value;
        } else {
            self.data_overrides.push(DataOverride::new(row, col, path, value));
        }
    }

    /// Retrieve a data override for the given cell and path, if any.
    pub fn get_data_override(
        &self,
        row: u32,
        col: u32,
        layer_path: &str,
    ) -> Option<&DataOverride> {
        self.data_overrides
            .iter()
            .find(|o| o.row == row && o.col == col && o.layer_path == layer_path)
    }

    /// Remove a specific override.  Returns `true` if something was removed.
    pub fn remove_data_override(&mut self, row: u32, col: u32, layer_path: &str) -> bool {
        let before = self.data_overrides.len();
        self.data_overrides
            .retain(|o| !(o.row == row && o.col == col && o.layer_path == layer_path));
        self.data_overrides.len() < before
    }

    /// Remove all overrides for a specific cell.
    pub fn clear_cell_overrides(&mut self, row: u32, col: u32) {
        self.data_overrides.retain(|o| !(o.row == row && o.col == col));
    }

    /// Remove every data override from this grid.
    pub fn clear_all_overrides(&mut self) {
        self.data_overrides.clear();
    }

    /// All overrides that apply to a cell at `(row, col)`.
    pub fn overrides_for_cell(&self, row: u32, col: u32) -> Vec<&DataOverride> {
        self.data_overrides
            .iter()
            .filter(|o| o.row == row && o.col == col)
            .collect()
    }

    // ── Geometry ───────────────────────────────────────────────────────────

    /// Total number of cells in the grid (`rows × columns`).
    pub fn total_cells(&self) -> usize {
        (self.rows * self.columns) as usize
    }

    /// Bounding box (x, y, width, height) of the cell at `(row, col)` in
    /// grid-local coordinates (relative to `origin_x / origin_y`).
    ///
    /// Returns `None` if `(row, col)` is out of range.
    pub fn cell_bounds(&self, row: u32, col: u32) -> Option<(f32, f32, f32, f32)> {
        if row >= self.rows || col >= self.columns {
            return None;
        }
        let x = col as f32 * (self.cell_size.0 + self.col_gap);
        let y = row as f32 * (self.cell_size.1 + self.row_gap);
        Some((x, y, self.cell_size.0, self.cell_size.1))
    }

    /// Total width of the grid including all gaps.
    pub fn total_width(&self) -> f32 {
        if self.columns == 0 {
            return 0.0;
        }
        self.columns as f32 * self.cell_size.0
            + (self.columns.saturating_sub(1)) as f32 * self.col_gap
    }

    /// Total height of the grid including all gaps.
    pub fn total_height(&self) -> f32 {
        if self.rows == 0 {
            return 0.0;
        }
        self.rows as f32 * self.cell_size.1
            + (self.rows.saturating_sub(1)) as f32 * self.row_gap
    }

    /// Absolute (parent-frame) bounding box of the cell at `(row, col)`.
    ///
    /// Returns `None` if out of range.
    pub fn cell_bounds_absolute(&self, row: u32, col: u32) -> Option<(f32, f32, f32, f32)> {
        self.cell_bounds(row, col)
            .map(|(x, y, w, h)| (self.origin_x + x, self.origin_y + y, w, h))
    }

    /// Expand the grid by adding one row.
    pub fn add_row(&mut self) {
        self.rows += 1;
    }

    /// Expand the grid by adding one column.
    pub fn add_column(&mut self) {
        self.columns += 1;
    }

    /// Shrink: remove the last row (if rows > 1).
    pub fn remove_last_row(&mut self) {
        if self.rows > 1 {
            let last = self.rows - 1;
            self.data_overrides.retain(|o| o.row != last);
            self.rows -= 1;
        }
    }

    /// Shrink: remove the last column (if columns > 1).
    pub fn remove_last_column(&mut self) {
        if self.columns > 1 {
            let last = self.columns - 1;
            self.data_overrides.retain(|o| o.col != last);
            self.columns -= 1;
        }
    }

    // ── Data source binding ──────────────────────────────────────────────────

    /// Attach a [`DataSource`] to this grid.
    ///
    /// If a source with the same name already exists it is replaced.
    pub fn attach_data_source(&mut self, source: DataSource) {
        // Replace if duplicate name.
        for existing in &mut self.data_sources {
            if existing.name == source.name {
                *existing = source;
                return;
            }
        }
        self.data_sources.push(source);
    }

    /// Detach a source by name. Returns `true` if a source was removed.
    pub fn detach_data_source(&mut self, name: &str) -> bool {
        let before = self.data_sources.len();
        self.data_sources.retain(|s| s.name != name);
        self.data_sources.len() < before
    }

    /// Number of attached data sources.
    pub fn source_count(&self) -> usize {
        self.data_sources.len()
    }

    /// Remove all attached data sources.
    pub fn clear_data_sources(&mut self) {
        self.data_sources.clear();
    }

    /// Distribute attached data-source values into [`DataOverride`] entries.
    ///
    /// For each attached source, iterates every cell in row-major order and
    /// writes a `DataOverride` for `source.column`.  Existing overrides for
    /// the same path are replaced.  Sources with empty `values` produce no
    /// overrides.
    pub fn auto_fill(&mut self) {
        let rows = self.rows;
        let columns = self.columns;
        // Collect sources first to avoid borrow issues.
        let sources: Vec<DataSource> = self.data_sources.clone();
        for source in &sources {
            if source.is_empty() {
                continue;
            }
            let mut cell_idx = 0usize;
            for row in 0..rows {
                for col in 0..columns {
                    let value = source.value_for_index(cell_idx).clone();
                    self.set_data_override(row, col, &source.column, value);
                    cell_idx += 1;
                }
            }
        }
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn basic_grid() -> RepeatGrid {
        RepeatGrid::new(3, 4, 100.0, 80.0)
    }

    #[test]
    fn new_grid_has_correct_dimensions() {
        let g = basic_grid();
        assert_eq!(g.rows, 3);
        assert_eq!(g.columns, 4);
        assert_eq!(g.cell_size, (100.0, 80.0));
    }

    #[test]
    fn total_cells_is_rows_times_columns() {
        let g = basic_grid();
        assert_eq!(g.total_cells(), 12);
    }

    #[test]
    fn cell_bounds_first_cell() {
        let g = basic_grid();
        let b = g.cell_bounds(0, 0).unwrap();
        assert_eq!(b, (0.0, 0.0, 100.0, 80.0));
    }

    #[test]
    fn cell_bounds_with_gap() {
        let g = basic_grid().with_gap(10.0, 5.0);
        let b = g.cell_bounds(1, 2).unwrap();
        // x = 2 * (100 + 10) = 220, y = 1 * (80 + 5) = 85
        assert_eq!(b, (220.0, 85.0, 100.0, 80.0));
    }

    #[test]
    fn cell_bounds_out_of_range_returns_none() {
        let g = basic_grid();
        assert!(g.cell_bounds(3, 0).is_none()); // row 3 out of range (0..3)
        assert!(g.cell_bounds(0, 4).is_none()); // col 4 out of range
    }

    #[test]
    fn total_width_no_gap() {
        let g = basic_grid();
        assert_eq!(g.total_width(), 400.0);
    }

    #[test]
    fn total_height_with_gap() {
        let g = basic_grid().with_gap(0.0, 10.0);
        // 3 rows * 80 + 2 gaps * 10 = 260
        assert_eq!(g.total_height(), 260.0);
    }

    #[test]
    fn total_width_with_gap() {
        let g = basic_grid().with_gap(12.0, 0.0);
        // 4 * 100 + 3 * 12 = 436
        assert_eq!(g.total_width(), 436.0);
    }

    #[test]
    fn add_template_layer_no_duplicates() {
        let mut g = basic_grid();
        let id = Uuid::new_v4();
        g.add_template_layer(id);
        g.add_template_layer(id);
        assert_eq!(g.template_count(), 1);
    }

    #[test]
    fn remove_template_layer() {
        let mut g = basic_grid();
        let id = Uuid::new_v4();
        g.add_template_layer(id);
        g.remove_template_layer(&id);
        assert_eq!(g.template_count(), 0);
    }

    #[test]
    fn set_data_override_insert() {
        let mut g = basic_grid();
        g.set_data_override(0, 1, "label.text", serde_json::json!("Hello"));
        assert!(g.get_data_override(0, 1, "label.text").is_some());
    }

    #[test]
    fn set_data_override_replaces_existing() {
        let mut g = basic_grid();
        g.set_data_override(1, 2, "bg.color", serde_json::json!("#fff"));
        g.set_data_override(1, 2, "bg.color", serde_json::json!("#000"));
        let o = g.get_data_override(1, 2, "bg.color").unwrap();
        assert_eq!(o.value, serde_json::json!("#000"));
        assert_eq!(g.data_overrides.len(), 1); // only one entry
    }

    #[test]
    fn get_data_override_missing_returns_none() {
        let g = basic_grid();
        assert!(g.get_data_override(0, 0, "nope").is_none());
    }

    #[test]
    fn remove_data_override_returns_true_when_found() {
        let mut g = basic_grid();
        g.set_data_override(0, 0, "x", serde_json::json!(1));
        assert!(g.remove_data_override(0, 0, "x"));
        assert!(g.get_data_override(0, 0, "x").is_none());
    }

    #[test]
    fn remove_data_override_returns_false_when_not_found() {
        let mut g = basic_grid();
        assert!(!g.remove_data_override(0, 0, "ghost"));
    }

    #[test]
    fn clear_cell_overrides() {
        let mut g = basic_grid();
        g.set_data_override(0, 0, "a", serde_json::json!(1));
        g.set_data_override(0, 0, "b", serde_json::json!(2));
        g.set_data_override(1, 1, "c", serde_json::json!(3));
        g.clear_cell_overrides(0, 0);
        assert_eq!(g.data_overrides.len(), 1);
        assert_eq!(g.data_overrides[0].row, 1);
    }

    #[test]
    fn clear_all_overrides() {
        let mut g = basic_grid();
        for i in 0..5u32 {
            g.set_data_override(i % 3, i % 4, "v", serde_json::json!(i));
        }
        g.clear_all_overrides();
        assert!(g.data_overrides.is_empty());
    }

    #[test]
    fn overrides_for_cell_filter() {
        let mut g = basic_grid();
        g.set_data_override(1, 1, "x", serde_json::json!(1));
        g.set_data_override(1, 1, "y", serde_json::json!(2));
        g.set_data_override(2, 2, "x", serde_json::json!(3));
        assert_eq!(g.overrides_for_cell(1, 1).len(), 2);
        assert_eq!(g.overrides_for_cell(2, 2).len(), 1);
        assert_eq!(g.overrides_for_cell(0, 0).len(), 0);
    }

    #[test]
    fn add_row_increments() {
        let mut g = basic_grid();
        g.add_row();
        assert_eq!(g.rows, 4);
        assert_eq!(g.total_cells(), 16);
    }

    #[test]
    fn add_column_increments() {
        let mut g = basic_grid();
        g.add_column();
        assert_eq!(g.columns, 5);
        assert_eq!(g.total_cells(), 15);
    }

    #[test]
    fn remove_last_row_removes_overrides_in_that_row() {
        let mut g = basic_grid();
        g.set_data_override(2, 0, "v", serde_json::json!("gone"));
        g.set_data_override(0, 0, "v", serde_json::json!("stay"));
        g.remove_last_row();
        assert_eq!(g.rows, 2);
        assert_eq!(g.data_overrides.len(), 1);
    }

    #[test]
    fn remove_last_row_does_not_go_below_one() {
        let mut g = RepeatGrid::new(1, 2, 50.0, 50.0);
        g.remove_last_row();
        assert_eq!(g.rows, 1);
    }

    #[test]
    fn remove_last_column_removes_overrides_in_that_col() {
        let mut g = basic_grid();
        g.set_data_override(0, 3, "v", serde_json::json!("gone"));
        g.set_data_override(0, 0, "v", serde_json::json!("stay"));
        g.remove_last_column();
        assert_eq!(g.columns, 3);
        assert_eq!(g.data_overrides.len(), 1);
    }

    #[test]
    fn with_origin_sets_absolute_position() {
        let g = basic_grid().with_origin(50.0, 100.0);
        let abs = g.cell_bounds_absolute(0, 0).unwrap();
        assert_eq!(abs, (50.0, 100.0, 100.0, 80.0));
    }

    #[test]
    fn cell_bounds_absolute_second_cell() {
        let g = basic_grid().with_gap(10.0, 5.0).with_origin(20.0, 30.0);
        let abs = g.cell_bounds_absolute(1, 1).unwrap();
        // local: x=110, y=85; absolute: x=130, y=115
        assert_eq!(abs, (130.0, 115.0, 100.0, 80.0));
    }

    #[test]
    fn data_override_cell_index() {
        let o = DataOverride::new(2, 3, "k", serde_json::json!(0));
        assert_eq!(o.cell_index(4), 11); // 2*4 + 3
    }

    #[test]
    fn grid_serde_roundtrip() {
        let mut g = basic_grid().with_gap(8.0, 4.0);
        g.set_data_override(0, 0, "text", serde_json::json!("hi"));
        let json = serde_json::to_string(&g).unwrap();
        let back: RepeatGrid = serde_json::from_str(&json).unwrap();
        assert_eq!(back.rows, 3);
        assert_eq!(back.col_gap, 8.0);
        assert_eq!(back.data_overrides.len(), 1);
    }
}
