//! Hit testing — screen coordinate to cell/header mapping.
//!
//! Given a screen point (from a mouse click or touch event), determine
//! what the user clicked: a cell, a column header, a row header, a
//! column resize handle, or the select-all corner.

use super::grid::GridModel;
use super::viewport::Viewport;

/// The result of a hit test.
#[derive(Debug, Clone, PartialEq)]
pub enum HitTestResult {
    /// User clicked on a cell at (col, row).
    Cell { col: u32, row: u32 },

    /// User clicked on a column header.
    ColumnHeader { col: u32 },

    /// User clicked on a row header.
    RowHeader { row: u32 },

    /// User clicked on the resize handle at the right edge of a column header.
    ColumnResizeHandle { col: u32 },

    /// User clicked on the resize handle at the bottom edge of a row header.
    RowResizeHandle { row: u32 },

    /// User clicked on the top-left corner (select-all).
    Corner,

    /// Click was outside the grid area.
    None,
}

/// Width of the resize handle zone (in screen pixels).
const RESIZE_HANDLE_WIDTH: f64 = 5.0;

/// Perform a hit test: given a screen coordinate, determine what was clicked.
pub fn hit_test(
    screen_x: f64,
    screen_y: f64,
    grid: &GridModel,
    viewport: &Viewport,
) -> HitTestResult {
    // 1. Corner (select-all button)
    if viewport.is_in_corner(screen_x, screen_y) {
        return HitTestResult::Corner;
    }

    // 2. Column header
    if viewport.is_in_col_header(screen_x, screen_y) {
        let (sheet_x, _) = viewport.screen_to_sheet(screen_x, screen_y);
        if let Some(col) = grid.col_at_x(sheet_x) {
            // Check if near the right edge (resize handle)
            let col_right = grid.col_offset(col) + grid.col_width(col);
            let (right_screen_x, _) = viewport.sheet_to_screen(col_right, 0.0);
            if (screen_x - right_screen_x).abs() < RESIZE_HANDLE_WIDTH {
                return HitTestResult::ColumnResizeHandle { col };
            }
            return HitTestResult::ColumnHeader { col };
        }
        return HitTestResult::None;
    }

    // 3. Row header
    if viewport.is_in_row_header(screen_x, screen_y) {
        let (_, sheet_y) = viewport.screen_to_sheet(screen_x, screen_y);
        if let Some(row) = grid.row_at_y(sheet_y) {
            // Check if near the bottom edge (resize handle)
            let row_bottom = grid.row_offset(row) + grid.row_height(row);
            let (_, bottom_screen_y) = viewport.sheet_to_screen(0.0, row_bottom);
            if (screen_y - bottom_screen_y).abs() < RESIZE_HANDLE_WIDTH {
                return HitTestResult::RowResizeHandle { row };
            }
            return HitTestResult::RowHeader { row };
        }
        return HitTestResult::None;
    }

    // 4. Cell area
    let (sheet_x, sheet_y) = viewport.screen_to_sheet(screen_x, screen_y);
    if let (Some(col), Some(row)) = (grid.col_at_x(sheet_x), grid.row_at_y(sheet_y)) {
        return HitTestResult::Cell { col, row };
    }

    HitTestResult::None
}

/// Convert a screen point to a cell coordinate (ignoring headers).
/// Returns `None` if the point is outside the grid.
pub fn screen_to_cell(
    screen_x: f64,
    screen_y: f64,
    grid: &GridModel,
    viewport: &Viewport,
) -> Option<(u32, u32)> {
    let (sheet_x, sheet_y) = viewport.screen_to_sheet(screen_x, screen_y);
    let col = grid.col_at_x(sheet_x)?;
    let row = grid.row_at_y(sheet_y)?;
    Some((col, row))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::grid::{COL_HEADER_HEIGHT, DEFAULT_COL_WIDTH, DEFAULT_ROW_HEIGHT, ROW_HEADER_WIDTH};

    fn setup() -> (GridModel, Viewport) {
        let grid = GridModel::new(26, 100);
        let viewport = Viewport::new(800.0, 600.0);
        (grid, viewport)
    }

    #[test]
    fn hit_corner() {
        let (grid, vp) = setup();
        assert_eq!(
            hit_test(10.0, 10.0, &grid, &vp),
            HitTestResult::Corner
        );
    }

    #[test]
    fn hit_col_header() {
        let (grid, vp) = setup();
        // Click in col header area, at x = ROW_HEADER_WIDTH + 50 (col 0)
        let x = ROW_HEADER_WIDTH + 50.0;
        let y = COL_HEADER_HEIGHT / 2.0;
        assert_eq!(
            hit_test(x, y, &grid, &vp),
            HitTestResult::ColumnHeader { col: 0 }
        );
    }

    #[test]
    fn hit_col_header_second() {
        let (grid, vp) = setup();
        let x = ROW_HEADER_WIDTH + DEFAULT_COL_WIDTH + 30.0; // col 1
        let y = COL_HEADER_HEIGHT / 2.0;
        assert_eq!(
            hit_test(x, y, &grid, &vp),
            HitTestResult::ColumnHeader { col: 1 }
        );
    }

    #[test]
    fn hit_row_header() {
        let (grid, vp) = setup();
        let x = ROW_HEADER_WIDTH / 2.0;
        let y = COL_HEADER_HEIGHT + 5.0; // row 0
        assert_eq!(
            hit_test(x, y, &grid, &vp),
            HitTestResult::RowHeader { row: 0 }
        );
    }

    #[test]
    fn hit_cell() {
        let (grid, vp) = setup();
        // Cell (0,0) starts at screen (ROW_HEADER_WIDTH, COL_HEADER_HEIGHT)
        let x = ROW_HEADER_WIDTH + 10.0;
        let y = COL_HEADER_HEIGHT + 5.0;
        assert_eq!(
            hit_test(x, y, &grid, &vp),
            HitTestResult::Cell { col: 0, row: 0 }
        );
    }

    #[test]
    fn hit_cell_second_row() {
        let (grid, vp) = setup();
        let x = ROW_HEADER_WIDTH + 10.0;
        let y = COL_HEADER_HEIGHT + DEFAULT_ROW_HEIGHT + 5.0;
        assert_eq!(
            hit_test(x, y, &grid, &vp),
            HitTestResult::Cell { col: 0, row: 1 }
        );
    }

    #[test]
    fn hit_cell_scrolled() {
        let (grid, mut vp) = setup();
        vp.scroll_by(200.0, 48.0, 10000.0, 10000.0);
        // After scrolling, the first visible cell is (2, 2)
        let x = ROW_HEADER_WIDTH + 10.0;
        let y = COL_HEADER_HEIGHT + 5.0;
        assert_eq!(
            hit_test(x, y, &grid, &vp),
            HitTestResult::Cell { col: 2, row: 2 }
        );
    }

    #[test]
    fn hit_col_resize_handle() {
        let (grid, vp) = setup();
        // Right edge of col 0 is at sheet x=100, screen x = 50+100 = 150
        let x = ROW_HEADER_WIDTH + DEFAULT_COL_WIDTH - 2.0; // near right edge
        let y = COL_HEADER_HEIGHT / 2.0;
        assert_eq!(
            hit_test(x, y, &grid, &vp),
            HitTestResult::ColumnResizeHandle { col: 0 }
        );
    }

    #[test]
    fn hit_row_resize_handle() {
        let (grid, vp) = setup();
        let x = ROW_HEADER_WIDTH / 2.0;
        let y = COL_HEADER_HEIGHT + DEFAULT_ROW_HEIGHT - 2.0;
        assert_eq!(
            hit_test(x, y, &grid, &vp),
            HitTestResult::RowResizeHandle { row: 0 }
        );
    }

    #[test]
    fn screen_to_cell_basic() {
        let (grid, vp) = setup();
        let x = ROW_HEADER_WIDTH + DEFAULT_COL_WIDTH * 2.5;
        let y = COL_HEADER_HEIGHT + DEFAULT_ROW_HEIGHT * 3.5;
        assert_eq!(screen_to_cell(x, y, &grid, &vp), Some((2, 3)));
    }

    #[test]
    fn screen_to_cell_out_of_bounds() {
        let (grid, vp) = setup();
        assert_eq!(screen_to_cell(5000.0, 5000.0, &grid, &vp), None);
    }

    #[test]
    fn hit_zoomed() {
        let (grid, mut vp) = setup();
        vp.set_zoom(2.0);
        // At 2x zoom, cell(0,0) occupies screen [50, 250) x [24, 72)
        let x = ROW_HEADER_WIDTH + 10.0;
        let y = COL_HEADER_HEIGHT + 10.0;
        assert_eq!(
            hit_test(x, y, &grid, &vp),
            HitTestResult::Cell { col: 0, row: 0 }
        );
    }
}
