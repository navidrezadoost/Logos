//! Selection model — cursor position, ranges, keyboard navigation.
//!
//! Tracks the active cell, optional selection range, and provides
//! movement commands (arrows, Tab, Enter, Home, End, Page Up/Down).

use crate::deps::CellCoord;

// ---------------------------------------------------------------------------
// SelectionRange
// ---------------------------------------------------------------------------

/// A rectangular range of selected cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionRange {
    /// The anchor cell (where the selection started).
    pub anchor_col: u32,
    pub anchor_row: u32,

    /// The moving end of the selection (where the cursor is now).
    pub cursor_col: u32,
    pub cursor_row: u32,
}

impl SelectionRange {
    pub fn single(col: u32, row: u32) -> Self {
        Self {
            anchor_col: col,
            anchor_row: row,
            cursor_col: col,
            cursor_row: row,
        }
    }

    /// Top-left corner of the range.
    pub fn min_col(&self) -> u32 {
        self.anchor_col.min(self.cursor_col)
    }
    pub fn min_row(&self) -> u32 {
        self.anchor_row.min(self.cursor_row)
    }

    /// Bottom-right corner of the range.
    pub fn max_col(&self) -> u32 {
        self.anchor_col.max(self.cursor_col)
    }
    pub fn max_row(&self) -> u32 {
        self.anchor_row.max(self.cursor_row)
    }

    /// Width in columns.
    pub fn width(&self) -> u32 {
        self.max_col() - self.min_col() + 1
    }

    /// Height in rows.
    pub fn height(&self) -> u32 {
        self.max_row() - self.min_row() + 1
    }

    /// Number of cells in the range.
    pub fn cell_count(&self) -> u32 {
        self.width() * self.height()
    }

    /// Check if a cell is within this range.
    pub fn contains(&self, col: u32, row: u32) -> bool {
        col >= self.min_col()
            && col <= self.max_col()
            && row >= self.min_row()
            && row <= self.max_row()
    }

    /// Is this a single-cell selection?
    pub fn is_single(&self) -> bool {
        self.anchor_col == self.cursor_col && self.anchor_row == self.cursor_row
    }

    /// Iterate over all cells in the range (row-major order).
    pub fn cells(&self) -> Vec<CellCoord> {
        let mut result = Vec::with_capacity(self.cell_count() as usize);
        for r in self.min_row()..=self.max_row() {
            for c in self.min_col()..=self.max_col() {
                result.push((c, r));
            }
        }
        result
    }
}

// ---------------------------------------------------------------------------
// Selection
// ---------------------------------------------------------------------------

/// Keyboard movement direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

/// The spreadsheet selection state.
///
/// Manages the active cell (cursor), optional range selection,
/// and movement commands.
#[derive(Debug, Clone)]
pub struct Selection {
    /// Current cursor position (the "active cell").
    cursor_col: u32,
    cursor_row: u32,

    /// If the user is selecting a range, this holds the anchor.
    /// When `None`, only the cursor cell is selected.
    anchor: Option<(u32, u32)>,

    /// Grid bounds for clamping.
    max_col: u32,
    max_row: u32,

    /// Whether the cell is being edited.
    editing: bool,
}

impl Selection {
    pub fn new(max_col: u32, max_row: u32) -> Self {
        Self {
            cursor_col: 0,
            cursor_row: 0,
            anchor: None,
            max_col: max_col.saturating_sub(1),
            max_row: max_row.saturating_sub(1),
            editing: false,
        }
    }

    // -----------------------------------------------------------------------
    // Accessors
    // -----------------------------------------------------------------------

    pub fn cursor(&self) -> (u32, u32) {
        (self.cursor_col, self.cursor_row)
    }

    pub fn cursor_col(&self) -> u32 {
        self.cursor_col
    }

    pub fn cursor_row(&self) -> u32 {
        self.cursor_row
    }

    pub fn is_editing(&self) -> bool {
        self.editing
    }

    pub fn set_editing(&mut self, editing: bool) {
        self.editing = editing;
    }

    /// Get the current selection range. Always returns a range
    /// (single cell if no range selection is active).
    pub fn range(&self) -> SelectionRange {
        match self.anchor {
            Some((ac, ar)) => SelectionRange {
                anchor_col: ac,
                anchor_row: ar,
                cursor_col: self.cursor_col,
                cursor_row: self.cursor_row,
            },
            None => SelectionRange::single(self.cursor_col, self.cursor_row),
        }
    }

    /// Is a multi-cell range selected?
    pub fn has_range(&self) -> bool {
        self.anchor.is_some() && !self.range().is_single()
    }

    // -----------------------------------------------------------------------
    // Movement
    // -----------------------------------------------------------------------

    /// Move the cursor to a specific cell. Clears any range selection.
    pub fn go_to(&mut self, col: u32, row: u32) {
        self.cursor_col = col.min(self.max_col);
        self.cursor_row = row.min(self.max_row);
        self.anchor = None;
        self.editing = false;
    }

    /// Move the cursor in a direction. If `extend`, the range grows.
    pub fn move_cursor(&mut self, dir: Direction, extend: bool) {
        if extend && self.anchor.is_none() {
            self.anchor = Some((self.cursor_col, self.cursor_row));
        }

        match dir {
            Direction::Up => {
                self.cursor_row = self.cursor_row.saturating_sub(1);
            }
            Direction::Down => {
                self.cursor_row = (self.cursor_row + 1).min(self.max_row);
            }
            Direction::Left => {
                self.cursor_col = self.cursor_col.saturating_sub(1);
            }
            Direction::Right => {
                self.cursor_col = (self.cursor_col + 1).min(self.max_col);
            }
        }

        if !extend {
            self.anchor = None;
        }
        self.editing = false;
    }

    /// Move to the start of the row.
    pub fn home(&mut self) {
        self.cursor_col = 0;
        self.anchor = None;
    }

    /// Move to the end of the row.
    pub fn end(&mut self) {
        self.cursor_col = self.max_col;
        self.anchor = None;
    }

    /// Move to A1.
    pub fn ctrl_home(&mut self) {
        self.cursor_col = 0;
        self.cursor_row = 0;
        self.anchor = None;
    }

    /// Move to the last cell.
    pub fn ctrl_end(&mut self) {
        self.cursor_col = self.max_col;
        self.cursor_row = self.max_row;
        self.anchor = None;
    }

    /// Page down (move by `page_rows` rows).
    pub fn page_down(&mut self, page_rows: u32) {
        self.cursor_row = (self.cursor_row + page_rows).min(self.max_row);
        self.anchor = None;
    }

    /// Page up (move by `page_rows` rows).
    pub fn page_up(&mut self, page_rows: u32) {
        self.cursor_row = self.cursor_row.saturating_sub(page_rows);
        self.anchor = None;
    }

    /// Tab moves to the next cell in the row.
    pub fn tab(&mut self) {
        if self.cursor_col < self.max_col {
            self.cursor_col += 1;
        } else {
            // Wrap to next row
            self.cursor_col = 0;
            self.cursor_row = (self.cursor_row + 1).min(self.max_row);
        }
        self.anchor = None;
        self.editing = false;
    }

    /// Shift+Tab moves to the previous cell in the row.
    pub fn shift_tab(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        } else if self.cursor_row > 0 {
            // Wrap to end of previous row
            self.cursor_col = self.max_col;
            self.cursor_row -= 1;
        }
        self.anchor = None;
        self.editing = false;
    }

    /// Enter moves down (or to next row in a range).
    pub fn enter(&mut self) {
        self.move_cursor(Direction::Down, false);
        self.editing = false;
    }

    // -----------------------------------------------------------------------
    // Range selection via mouse
    // -----------------------------------------------------------------------

    /// Start a selection at a cell (mouse down).
    pub fn start_select(&mut self, col: u32, row: u32, extend: bool) {
        let col = col.min(self.max_col);
        let row = row.min(self.max_row);

        if extend {
            if self.anchor.is_none() {
                self.anchor = Some((self.cursor_col, self.cursor_row));
            }
        } else {
            self.anchor = Some((col, row));
        }
        self.cursor_col = col;
        self.cursor_row = row;
        self.editing = false;
    }

    /// Extend the selection to a cell (mouse drag).
    pub fn extend_select(&mut self, col: u32, row: u32) {
        self.cursor_col = col.min(self.max_col);
        self.cursor_row = row.min(self.max_row);
    }

    /// End the selection (mouse up). If the range is a single cell,
    /// convert to a simple cursor position.
    pub fn end_select(&mut self) {
        if let Some((ac, ar)) = self.anchor {
            if ac == self.cursor_col && ar == self.cursor_row {
                self.anchor = None;
            }
        }
    }

    /// Select the entire grid.
    pub fn select_all(&mut self) {
        self.anchor = Some((0, 0));
        self.cursor_col = self.max_col;
        self.cursor_row = self.max_row;
    }

    /// Select an entire column.
    pub fn select_col(&mut self, col: u32) {
        self.anchor = Some((col.min(self.max_col), 0));
        self.cursor_col = col.min(self.max_col);
        self.cursor_row = self.max_row;
    }

    /// Select an entire row.
    pub fn select_row(&mut self, row: u32) {
        self.anchor = Some((0, row.min(self.max_row)));
        self.cursor_col = self.max_col;
        self.cursor_row = row.min(self.max_row);
    }

    /// Clear the range (keep only the cursor).
    pub fn clear_range(&mut self) {
        self.anchor = None;
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sel() -> Selection {
        Selection::new(26, 100)
    }

    #[test]
    fn initial_state() {
        let s = sel();
        assert_eq!(s.cursor(), (0, 0));
        assert!(!s.has_range());
        assert!(s.range().is_single());
    }

    #[test]
    fn move_down() {
        let mut s = sel();
        s.move_cursor(Direction::Down, false);
        assert_eq!(s.cursor(), (0, 1));
    }

    #[test]
    fn move_right() {
        let mut s = sel();
        s.move_cursor(Direction::Right, false);
        assert_eq!(s.cursor(), (1, 0));
    }

    #[test]
    fn move_up_clamped() {
        let mut s = sel();
        s.move_cursor(Direction::Up, false);
        assert_eq!(s.cursor(), (0, 0));
    }

    #[test]
    fn move_left_clamped() {
        let mut s = sel();
        s.move_cursor(Direction::Left, false);
        assert_eq!(s.cursor(), (0, 0));
    }

    #[test]
    fn move_down_clamped() {
        let mut s = sel();
        for _ in 0..200 {
            s.move_cursor(Direction::Down, false);
        }
        assert_eq!(s.cursor_row(), 99); // max_row = 100-1
    }

    #[test]
    fn move_right_clamped() {
        let mut s = sel();
        for _ in 0..50 {
            s.move_cursor(Direction::Right, false);
        }
        assert_eq!(s.cursor_col(), 25); // max_col = 26-1
    }

    #[test]
    fn go_to() {
        let mut s = sel();
        s.go_to(5, 10);
        assert_eq!(s.cursor(), (5, 10));
    }

    #[test]
    fn go_to_clamped() {
        let mut s = sel();
        s.go_to(100, 200);
        assert_eq!(s.cursor(), (25, 99));
    }

    #[test]
    fn extend_selection() {
        let mut s = sel();
        s.go_to(2, 3);
        s.move_cursor(Direction::Right, true);
        s.move_cursor(Direction::Down, true);
        assert!(s.has_range());
        let r = s.range();
        assert_eq!(r.min_col(), 2);
        assert_eq!(r.min_row(), 3);
        assert_eq!(r.max_col(), 3);
        assert_eq!(r.max_row(), 4);
        assert_eq!(r.cell_count(), 4);
    }

    #[test]
    fn extend_then_move_clears_range() {
        let mut s = sel();
        s.move_cursor(Direction::Right, true);
        assert!(s.has_range());
        s.move_cursor(Direction::Down, false);
        assert!(!s.has_range());
    }

    #[test]
    fn tab_wraps() {
        let mut s = Selection::new(3, 3);
        s.go_to(2, 0);
        s.tab();
        assert_eq!(s.cursor(), (0, 1));
    }

    #[test]
    fn shift_tab_wraps() {
        let mut s = Selection::new(3, 3);
        s.go_to(0, 1);
        s.shift_tab();
        assert_eq!(s.cursor(), (2, 0));
    }

    #[test]
    fn home_and_end() {
        let mut s = sel();
        s.go_to(10, 5);
        s.home();
        assert_eq!(s.cursor_col(), 0);
        s.end();
        assert_eq!(s.cursor_col(), 25);
    }

    #[test]
    fn ctrl_home_end() {
        let mut s = sel();
        s.go_to(10, 50);
        s.ctrl_home();
        assert_eq!(s.cursor(), (0, 0));
        s.ctrl_end();
        assert_eq!(s.cursor(), (25, 99));
    }

    #[test]
    fn page_up_down() {
        let mut s = sel();
        s.page_down(25);
        assert_eq!(s.cursor_row(), 25);
        s.page_up(10);
        assert_eq!(s.cursor_row(), 15);
    }

    #[test]
    fn mouse_select() {
        let mut s = sel();
        s.start_select(2, 3, false);
        s.extend_select(5, 7);
        s.end_select();
        let r = s.range();
        assert_eq!(r.min_col(), 2);
        assert_eq!(r.min_row(), 3);
        assert_eq!(r.max_col(), 5);
        assert_eq!(r.max_row(), 7);
    }

    #[test]
    fn mouse_click_single() {
        let mut s = sel();
        s.start_select(3, 4, false);
        s.end_select();
        // Single cell click → no range
        assert!(!s.has_range());
        assert_eq!(s.cursor(), (3, 4));
    }

    #[test]
    fn select_all() {
        let mut s = sel();
        s.select_all();
        let r = s.range();
        assert_eq!(r.min_col(), 0);
        assert_eq!(r.min_row(), 0);
        assert_eq!(r.max_col(), 25);
        assert_eq!(r.max_row(), 99);
    }

    #[test]
    fn select_col() {
        let mut s = sel();
        s.select_col(3);
        let r = s.range();
        assert_eq!(r.min_col(), 3);
        assert_eq!(r.max_col(), 3);
        assert_eq!(r.min_row(), 0);
        assert_eq!(r.max_row(), 99);
    }

    #[test]
    fn select_row() {
        let mut s = sel();
        s.select_row(5);
        let r = s.range();
        assert_eq!(r.min_col(), 0);
        assert_eq!(r.max_col(), 25);
        assert_eq!(r.min_row(), 5);
        assert_eq!(r.max_row(), 5);
    }

    #[test]
    fn selection_range_contains() {
        let r = SelectionRange {
            anchor_col: 2,
            anchor_row: 3,
            cursor_col: 5,
            cursor_row: 7,
        };
        assert!(r.contains(3, 5));
        assert!(r.contains(2, 3));
        assert!(r.contains(5, 7));
        assert!(!r.contains(1, 3));
        assert!(!r.contains(3, 8));
    }

    #[test]
    fn selection_range_cells() {
        let r = SelectionRange {
            anchor_col: 1,
            anchor_row: 0,
            cursor_col: 2,
            cursor_row: 1,
        };
        let cells = r.cells();
        assert_eq!(cells, vec![(1, 0), (2, 0), (1, 1), (2, 1)]);
    }

    #[test]
    fn editing_flag() {
        let mut s = sel();
        assert!(!s.is_editing());
        s.set_editing(true);
        assert!(s.is_editing());
        s.move_cursor(Direction::Down, false);
        assert!(!s.is_editing()); // movement clears editing
    }

    #[test]
    fn enter_moves_down() {
        let mut s = sel();
        s.go_to(3, 5);
        s.enter();
        assert_eq!(s.cursor(), (3, 6));
    }
}
