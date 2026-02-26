//! Grid geometry — column widths, row heights, and cell bounding rects.
//!
//! The [`GridModel`] stores per-column widths and per-row heights and
//! provides O(1) cell-rect lookups via prefix-sum arrays that are rebuilt
//! on any size change.

/// Default column width in logical pixels.
pub const DEFAULT_COL_WIDTH: f64 = 100.0;

/// Default row height in logical pixels.
pub const DEFAULT_ROW_HEIGHT: f64 = 24.0;

/// Minimum allowed column width.
pub const MIN_COL_WIDTH: f64 = 20.0;

/// Minimum allowed row height.
pub const MIN_ROW_HEIGHT: f64 = 12.0;

/// Maximum allowed column width.
pub const MAX_COL_WIDTH: f64 = 2000.0;

/// Maximum allowed row height.
pub const MAX_ROW_HEIGHT: f64 = 1000.0;

/// Header width for row numbers (left gutter).
pub const ROW_HEADER_WIDTH: f64 = 50.0;

/// Header height for column letters (top gutter).
pub const COL_HEADER_HEIGHT: f64 = 24.0;

// ---------------------------------------------------------------------------
// GridModel
// ---------------------------------------------------------------------------

/// A rectangle in sheet coordinates (logical pixels, before zoom/scroll).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl CellRect {
    pub fn right(&self) -> f64 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f64 {
        self.y + self.height
    }

    pub fn contains(&self, px: f64, py: f64) -> bool {
        px >= self.x && px < self.right() && py >= self.y && py < self.bottom()
    }
}

/// The grid geometry model.
///
/// Stores per-column widths and per-row heights (with defaults for
/// un-customised columns/rows) and provides fast lookups for:
///
/// - Cell bounding rect from (col, row)
/// - X/Y offset of any column/row
/// - Total grid dimensions
///
/// Uses prefix-sum arrays for O(1) offset lookups, rebuilt lazily when
/// a column width or row height changes.
#[derive(Debug, Clone)]
pub struct GridModel {
    num_cols: u32,
    num_rows: u32,

    /// Custom column widths (sparse — only non-default entries).
    col_widths: Vec<f64>,

    /// Custom row heights (sparse — only non-default entries).
    row_heights: Vec<f64>,

    /// Prefix sums: `col_offsets[i]` = x offset of column i.
    /// Length = num_cols + 1.
    col_offsets: Vec<f64>,

    /// Prefix sums: `row_offsets[i]` = y offset of row i.
    /// Length = num_rows + 1.
    row_offsets: Vec<f64>,
}

impl GridModel {
    /// Create a new grid with the given dimensions and default cell sizes.
    pub fn new(num_cols: u32, num_rows: u32) -> Self {
        let col_widths = vec![DEFAULT_COL_WIDTH; num_cols as usize];
        let row_heights = vec![DEFAULT_ROW_HEIGHT; num_rows as usize];

        let mut model = Self {
            num_cols,
            num_rows,
            col_widths,
            row_heights,
            col_offsets: Vec::new(),
            row_offsets: Vec::new(),
        };
        model.rebuild_offsets();
        model
    }

    // -----------------------------------------------------------------------
    // Dimensions
    // -----------------------------------------------------------------------

    pub fn num_cols(&self) -> u32 {
        self.num_cols
    }

    pub fn num_rows(&self) -> u32 {
        self.num_rows
    }

    /// Total width of the grid (excluding row header gutter).
    pub fn total_width(&self) -> f64 {
        *self.col_offsets.last().unwrap_or(&0.0)
    }

    /// Total height of the grid (excluding column header gutter).
    pub fn total_height(&self) -> f64 {
        *self.row_offsets.last().unwrap_or(&0.0)
    }

    // -----------------------------------------------------------------------
    // Column / row sizes
    // -----------------------------------------------------------------------

    /// Get the width of a column.
    pub fn col_width(&self, col: u32) -> f64 {
        self.col_widths
            .get(col as usize)
            .copied()
            .unwrap_or(DEFAULT_COL_WIDTH)
    }

    /// Get the height of a row.
    pub fn row_height(&self, row: u32) -> f64 {
        self.row_heights
            .get(row as usize)
            .copied()
            .unwrap_or(DEFAULT_ROW_HEIGHT)
    }

    /// Set the width of a column (clamped to min/max).
    pub fn set_col_width(&mut self, col: u32, width: f64) {
        if (col as usize) < self.col_widths.len() {
            self.col_widths[col as usize] = width.clamp(MIN_COL_WIDTH, MAX_COL_WIDTH);
            self.rebuild_offsets();
        }
    }

    /// Set the height of a row (clamped to min/max).
    pub fn set_row_height(&mut self, row: u32, height: f64) {
        if (row as usize) < self.row_heights.len() {
            self.row_heights[row as usize] = height.clamp(MIN_ROW_HEIGHT, MAX_ROW_HEIGHT);
            self.rebuild_offsets();
        }
    }

    /// Reset a column to default width.
    pub fn reset_col_width(&mut self, col: u32) {
        self.set_col_width(col, DEFAULT_COL_WIDTH);
    }

    /// Reset a row to default height.
    pub fn reset_row_height(&mut self, row: u32) {
        self.set_row_height(row, DEFAULT_ROW_HEIGHT);
    }

    // -----------------------------------------------------------------------
    // Cell geometry lookups (O(1) via prefix sums)
    // -----------------------------------------------------------------------

    /// X offset of a column (left edge in sheet coordinates).
    pub fn col_offset(&self, col: u32) -> f64 {
        self.col_offsets
            .get(col as usize)
            .copied()
            .unwrap_or(self.total_width())
    }

    /// Y offset of a row (top edge in sheet coordinates).
    pub fn row_offset(&self, row: u32) -> f64 {
        self.row_offsets
            .get(row as usize)
            .copied()
            .unwrap_or(self.total_height())
    }

    /// Get the bounding rect of a cell in sheet coordinates.
    pub fn cell_rect(&self, col: u32, row: u32) -> CellRect {
        CellRect {
            x: self.col_offset(col),
            y: self.row_offset(row),
            width: self.col_width(col),
            height: self.row_height(row),
        }
    }

    /// Get the bounding rect of a range of cells (inclusive).
    pub fn range_rect(&self, col1: u32, row1: u32, col2: u32, row2: u32) -> CellRect {
        let c1 = col1.min(col2);
        let r1 = row1.min(row2);
        let c2 = col1.max(col2);
        let r2 = row1.max(row2);

        let x = self.col_offset(c1);
        let y = self.row_offset(r1);
        let right = self.col_offset(c2) + self.col_width(c2);
        let bottom = self.row_offset(r2) + self.row_height(r2);

        CellRect {
            x,
            y,
            width: right - x,
            height: bottom - y,
        }
    }

    // -----------------------------------------------------------------------
    // Column/row from position (binary search, O(log n))
    // -----------------------------------------------------------------------

    /// Find the column index at a given x position (sheet coordinates).
    /// Returns `None` if x is outside the grid.
    pub fn col_at_x(&self, x: f64) -> Option<u32> {
        if x < 0.0 || x >= self.total_width() {
            return None;
        }
        // Binary search in col_offsets for the largest offset <= x
        let idx = self
            .col_offsets
            .partition_point(|&off| off <= x)
            .saturating_sub(1);
        Some(idx as u32)
    }

    /// Find the row index at a given y position (sheet coordinates).
    /// Returns `None` if y is outside the grid.
    pub fn row_at_y(&self, y: f64) -> Option<u32> {
        if y < 0.0 || y >= self.total_height() {
            return None;
        }
        let idx = self
            .row_offsets
            .partition_point(|&off| off <= y)
            .saturating_sub(1);
        Some(idx as u32)
    }

    // -----------------------------------------------------------------------
    // Visible range
    // -----------------------------------------------------------------------

    /// Determine which columns are visible in a given x-range.
    /// Returns `(first_col, last_col)` inclusive, or `None` if no columns
    /// are visible.
    pub fn visible_cols(&self, x_min: f64, x_max: f64) -> Option<(u32, u32)> {
        let first = self.col_at_x(x_min.max(0.0))?;
        let last = self
            .col_at_x((x_max - 0.001).min(self.total_width() - 0.001))
            .unwrap_or(self.num_cols - 1);
        Some((first, last))
    }

    /// Determine which rows are visible in a given y-range.
    /// Returns `(first_row, last_row)` inclusive, or `None` if no rows
    /// are visible.
    pub fn visible_rows(&self, y_min: f64, y_max: f64) -> Option<(u32, u32)> {
        let first = self.row_at_y(y_min.max(0.0))?;
        let last = self
            .row_at_y((y_max - 0.001).min(self.total_height() - 0.001))
            .unwrap_or(self.num_rows - 1);
        Some((first, last))
    }

    // -----------------------------------------------------------------------
    // Internal
    // -----------------------------------------------------------------------

    fn rebuild_offsets(&mut self) {
        // Column prefix sums
        self.col_offsets = Vec::with_capacity(self.num_cols as usize + 1);
        let mut x = 0.0;
        self.col_offsets.push(x);
        for &w in &self.col_widths {
            x += w;
            self.col_offsets.push(x);
        }

        // Row prefix sums
        self.row_offsets = Vec::with_capacity(self.num_rows as usize + 1);
        let mut y = 0.0;
        self.row_offsets.push(y);
        for &h in &self.row_heights {
            y += h;
            self.row_offsets.push(y);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_dimensions() {
        let g = GridModel::new(26, 100);
        assert_eq!(g.num_cols(), 26);
        assert_eq!(g.num_rows(), 100);
        assert_eq!(g.col_width(0), DEFAULT_COL_WIDTH);
        assert_eq!(g.row_height(0), DEFAULT_ROW_HEIGHT);
    }

    #[test]
    fn total_size() {
        let g = GridModel::new(10, 20);
        assert_eq!(g.total_width(), 10.0 * DEFAULT_COL_WIDTH);
        assert_eq!(g.total_height(), 20.0 * DEFAULT_ROW_HEIGHT);
    }

    #[test]
    fn col_offset_and_row_offset() {
        let g = GridModel::new(5, 5);
        assert_eq!(g.col_offset(0), 0.0);
        assert_eq!(g.col_offset(1), DEFAULT_COL_WIDTH);
        assert_eq!(g.col_offset(3), 3.0 * DEFAULT_COL_WIDTH);
        assert_eq!(g.row_offset(2), 2.0 * DEFAULT_ROW_HEIGHT);
    }

    #[test]
    fn cell_rect_default() {
        let g = GridModel::new(5, 5);
        let r = g.cell_rect(2, 3);
        assert_eq!(r.x, 2.0 * DEFAULT_COL_WIDTH);
        assert_eq!(r.y, 3.0 * DEFAULT_ROW_HEIGHT);
        assert_eq!(r.width, DEFAULT_COL_WIDTH);
        assert_eq!(r.height, DEFAULT_ROW_HEIGHT);
    }

    #[test]
    fn custom_col_width() {
        let mut g = GridModel::new(5, 5);
        g.set_col_width(1, 200.0);
        assert_eq!(g.col_width(1), 200.0);
        assert_eq!(g.col_offset(2), DEFAULT_COL_WIDTH + 200.0);
        // Total width: 4 * 100 + 200 = 600
        assert_eq!(g.total_width(), 600.0);
    }

    #[test]
    fn custom_row_height() {
        let mut g = GridModel::new(5, 5);
        g.set_row_height(0, 48.0);
        assert_eq!(g.row_height(0), 48.0);
        assert_eq!(g.row_offset(1), 48.0);
        assert_eq!(g.total_height(), 48.0 + 4.0 * DEFAULT_ROW_HEIGHT);
    }

    #[test]
    fn clamp_col_width() {
        let mut g = GridModel::new(5, 5);
        g.set_col_width(0, 5.0); // below minimum
        assert_eq!(g.col_width(0), MIN_COL_WIDTH);
        g.set_col_width(0, 5000.0); // above maximum
        assert_eq!(g.col_width(0), MAX_COL_WIDTH);
    }

    #[test]
    fn clamp_row_height() {
        let mut g = GridModel::new(5, 5);
        g.set_row_height(0, 1.0);
        assert_eq!(g.row_height(0), MIN_ROW_HEIGHT);
        g.set_row_height(0, 5000.0);
        assert_eq!(g.row_height(0), MAX_ROW_HEIGHT);
    }

    #[test]
    fn col_at_x_default() {
        let g = GridModel::new(10, 10);
        assert_eq!(g.col_at_x(0.0), Some(0));
        assert_eq!(g.col_at_x(50.0), Some(0));
        assert_eq!(g.col_at_x(100.0), Some(1));
        assert_eq!(g.col_at_x(250.0), Some(2));
        assert_eq!(g.col_at_x(-1.0), None);
        assert_eq!(g.col_at_x(1000.0), None); // 10*100=1000 is out
    }

    #[test]
    fn row_at_y_default() {
        let g = GridModel::new(10, 10);
        assert_eq!(g.row_at_y(0.0), Some(0));
        assert_eq!(g.row_at_y(23.0), Some(0));
        assert_eq!(g.row_at_y(24.0), Some(1));
        assert_eq!(g.row_at_y(50.0), Some(2));
    }

    #[test]
    fn col_at_x_custom_width() {
        let mut g = GridModel::new(5, 5);
        g.set_col_width(0, 200.0);
        // col 0: [0, 200), col 1: [200, 300), col 2: [300, 400)
        assert_eq!(g.col_at_x(150.0), Some(0));
        assert_eq!(g.col_at_x(200.0), Some(1));
        assert_eq!(g.col_at_x(300.0), Some(2));
    }

    #[test]
    fn range_rect() {
        let g = GridModel::new(10, 10);
        let r = g.range_rect(1, 2, 3, 4);
        assert_eq!(r.x, 1.0 * DEFAULT_COL_WIDTH);
        assert_eq!(r.y, 2.0 * DEFAULT_ROW_HEIGHT);
        assert_eq!(r.width, 3.0 * DEFAULT_COL_WIDTH);
        assert_eq!(r.height, 3.0 * DEFAULT_ROW_HEIGHT);
    }

    #[test]
    fn range_rect_reversed() {
        let g = GridModel::new(10, 10);
        // (3,4) to (1,2) should be the same as (1,2) to (3,4)
        let r1 = g.range_rect(1, 2, 3, 4);
        let r2 = g.range_rect(3, 4, 1, 2);
        assert_eq!(r1, r2);
    }

    #[test]
    fn visible_cols_default() {
        let g = GridModel::new(26, 100);
        // Viewport from x=0 to x=500 should show cols 0-4
        let (first, last) = g.visible_cols(0.0, 500.0).unwrap();
        assert_eq!(first, 0);
        assert_eq!(last, 4);
    }

    #[test]
    fn visible_cols_scrolled() {
        let g = GridModel::new(26, 100);
        // Viewport from x=250 to x=750 → cols 2-7
        let (first, last) = g.visible_cols(250.0, 750.0).unwrap();
        assert_eq!(first, 2);
        assert_eq!(last, 7);
    }

    #[test]
    fn visible_rows_default() {
        let g = GridModel::new(26, 100);
        // Viewport from y=0 to y=120 → rows 0-4
        let (first, last) = g.visible_rows(0.0, 120.0).unwrap();
        assert_eq!(first, 0);
        assert_eq!(last, 4);
    }

    #[test]
    fn cell_rect_contains() {
        let r = CellRect {
            x: 100.0,
            y: 48.0,
            width: 100.0,
            height: 24.0,
        };
        assert!(r.contains(150.0, 60.0));       // center
        assert!(r.contains(100.0, 48.0));       // top-left
        assert!(!r.contains(200.0, 48.0));      // right edge (exclusive)
        assert!(!r.contains(100.0, 72.0));      // bottom edge (exclusive)
        assert!(!r.contains(99.0, 48.0));       // just outside left
    }

    #[test]
    fn reset_col_width() {
        let mut g = GridModel::new(5, 5);
        g.set_col_width(2, 250.0);
        assert_eq!(g.col_width(2), 250.0);
        g.reset_col_width(2);
        assert_eq!(g.col_width(2), DEFAULT_COL_WIDTH);
    }
}
