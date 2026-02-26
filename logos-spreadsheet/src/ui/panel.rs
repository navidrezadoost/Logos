//! Spreadsheet panel — the top-level orchestrator.
//!
//! Ties together [`RecalcEngine`], [`GridModel`], [`Viewport`], [`Selection`],
//! and produces [`RenderFrame`]s that a renderer can draw.

use crate::recalc::RecalcEngine;
use crate::types::Value;

use super::formula_bar::FormulaBarState;
use super::grid::{CellRect, GridModel, COL_HEADER_HEIGHT, ROW_HEADER_WIDTH};
use super::hit_test::{self, HitTestResult};
use super::render_data::*;
use super::selection::{Direction, Selection};
use super::viewport::Viewport;

// ---------------------------------------------------------------------------
// Value formatting
// ---------------------------------------------------------------------------

/// Format a cell value for display.
fn format_value(value: &Value) -> String {
    match value {
        Value::Number(n) => {
            if *n == (*n as i64) as f64 && n.abs() < 1e15 {
                format!("{}", *n as i64)
            } else {
                format!("{}", n)
            }
        }
        Value::Text(s) => s.clone(),
        Value::Boolean(b) => {
            if *b {
                "TRUE".to_string()
            } else {
                "FALSE".to_string()
            }
        }
        Value::Error(e) => format!("{}", e),
        Value::Empty => String::new(),
        Value::Array(rows) => {
            // Show first element or {array}
            if let Some(first_row) = rows.first() {
                if let Some(first) = first_row.first() {
                    return format_value(first);
                }
            }
            "{array}".to_string()
        }
        Value::DesignRef(r) => format!("[{}]", r),
    }
}

/// Determine text alignment based on value type.
fn align_for_value(value: &Value) -> HAlign {
    match value {
        Value::Number(_) | Value::Boolean(_) => HAlign::Right,
        Value::Error(_) => HAlign::Center,
        _ => HAlign::Left,
    }
}

/// Determine text color based on value type.
fn color_for_value(value: &Value) -> Color {
    match value {
        Value::Error(_) => Color::RED,
        _ => Color::BLACK,
    }
}

// ---------------------------------------------------------------------------
// Column letter helpers
// ---------------------------------------------------------------------------

fn col_to_letter(col: u32) -> String {
    let mut result = String::new();
    let mut c = col;
    loop {
        result.insert(0, (b'A' + (c % 26) as u8) as char);
        if c < 26 {
            break;
        }
        c = c / 26 - 1;
    }
    result
}

// ---------------------------------------------------------------------------
// SpreadsheetPanel
// ---------------------------------------------------------------------------

/// The top-level spreadsheet panel, combining engine, layout, and interaction.
#[derive(Debug, Clone)]
pub struct SpreadsheetPanel {
    engine: RecalcEngine,
    grid: GridModel,
    viewport: Viewport,
    selection: Selection,
    formula_bar: FormulaBarState,
}

impl SpreadsheetPanel {
    /// Create a new panel with the given dimensions and screen size.
    pub fn new(
        num_cols: u32,
        num_rows: u32,
        screen_width: f64,
        screen_height: f64,
    ) -> Self {
        Self {
            engine: RecalcEngine::new(num_cols, num_rows),
            grid: GridModel::new(num_cols, num_rows),
            viewport: Viewport::new(screen_width, screen_height),
            selection: Selection::new(num_cols, num_rows),
            formula_bar: FormulaBarState::new(),
        }
    }

    // -----------------------------------------------------------------------
    // Component access
    // -----------------------------------------------------------------------

    pub fn engine(&self) -> &RecalcEngine {
        &self.engine
    }

    pub fn engine_mut(&mut self) -> &mut RecalcEngine {
        &mut self.engine
    }

    pub fn grid(&self) -> &GridModel {
        &self.grid
    }

    pub fn grid_mut(&mut self) -> &mut GridModel {
        &mut self.grid
    }

    pub fn viewport(&self) -> &Viewport {
        &self.viewport
    }

    pub fn viewport_mut(&mut self) -> &mut Viewport {
        &mut self.viewport
    }

    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    pub fn selection_mut(&mut self) -> &mut Selection {
        &mut self.selection
    }

    pub fn formula_bar(&self) -> &FormulaBarState {
        &self.formula_bar
    }

    pub fn formula_bar_mut(&mut self) -> &mut FormulaBarState {
        &mut self.formula_bar
    }

    // -----------------------------------------------------------------------
    // Cell editing
    // -----------------------------------------------------------------------

    /// Set a cell's formula (with "=" prefix) or raw value.
    pub fn set_cell_input(&mut self, col: u32, row: u32, input: &str) {
        if input.starts_with('=') {
            self.engine.set_formula(col, row, input);
        } else if let Ok(n) = input.parse::<f64>() {
            self.engine.set_value(col, row, Value::Number(n));
        } else if input.eq_ignore_ascii_case("true") {
            self.engine.set_value(col, row, Value::Boolean(true));
        } else if input.eq_ignore_ascii_case("false") {
            self.engine.set_value(col, row, Value::Boolean(false));
        } else if input.is_empty() {
            self.engine.clear_cell(col, row);
        } else {
            self.engine.set_value(col, row, Value::Text(input.to_string()));
        }
    }

    /// Get the display text for a cell.
    pub fn cell_display_text(&self, col: u32, row: u32) -> String {
        format_value(&self.engine.get_value(col, row))
    }

    /// Get the edit text for a cell (formula source if it has one, otherwise display text).
    pub fn cell_edit_text(&self, col: u32, row: u32) -> String {
        if let Some(formula) = self.engine.get_formula(col, row) {
            formula.to_string()
        } else {
            self.cell_display_text(col, row)
        }
    }

    // -----------------------------------------------------------------------
    // Mouse interaction
    // -----------------------------------------------------------------------

    /// Handle a mouse click at screen coordinates.
    pub fn mouse_down(&mut self, screen_x: f64, screen_y: f64, shift: bool) {
        let result = hit_test::hit_test(screen_x, screen_y, &self.grid, &self.viewport);
        match result {
            HitTestResult::Cell { col, row } => {
                self.selection.start_select(col, row, shift);
            }
            HitTestResult::ColumnHeader { col } => {
                self.selection.select_col(col);
            }
            HitTestResult::RowHeader { row } => {
                self.selection.select_row(row);
            }
            HitTestResult::Corner => {
                self.selection.select_all();
            }
            _ => {}
        }
    }

    /// Handle mouse drag at screen coordinates.
    pub fn mouse_move(&mut self, screen_x: f64, screen_y: f64) {
        if let Some((col, row)) =
            hit_test::screen_to_cell(screen_x, screen_y, &self.grid, &self.viewport)
        {
            self.selection.extend_select(col, row);
        }
    }

    /// Handle mouse release.
    pub fn mouse_up(&mut self) {
        self.selection.end_select();
    }

    // -----------------------------------------------------------------------
    // Keyboard interaction
    // -----------------------------------------------------------------------

    /// Handle arrow key press.
    pub fn arrow_key(&mut self, dir: Direction, shift: bool) {
        self.selection.move_cursor(dir, shift);
        self.ensure_cursor_visible();
    }

    /// Handle Enter key.
    pub fn enter_key(&mut self) {
        self.selection.enter();
        self.ensure_cursor_visible();
    }

    /// Handle Tab key.
    pub fn tab_key(&mut self, shift: bool) {
        if shift {
            self.selection.shift_tab();
        } else {
            self.selection.tab();
        }
        self.ensure_cursor_visible();
    }

    /// Handle Home key.
    pub fn home_key(&mut self, ctrl: bool) {
        if ctrl {
            self.selection.ctrl_home();
        } else {
            self.selection.home();
        }
        self.ensure_cursor_visible();
    }

    /// Handle End key.
    pub fn end_key(&mut self, ctrl: bool) {
        if ctrl {
            self.selection.ctrl_end();
        } else {
            self.selection.end();
        }
        self.ensure_cursor_visible();
    }

    /// Handle Page Down.
    pub fn page_down(&mut self) {
        let visible = self.viewport.visible_sheet_rect();
        let page_rows = (visible.height / self.grid.row_height(0)) as u32;
        self.selection.page_down(page_rows.max(1));
        self.ensure_cursor_visible();
    }

    /// Handle Page Up.
    pub fn page_up(&mut self) {
        let visible = self.viewport.visible_sheet_rect();
        let page_rows = (visible.height / self.grid.row_height(0)) as u32;
        self.selection.page_up(page_rows.max(1));
        self.ensure_cursor_visible();
    }

    // -----------------------------------------------------------------------
    // Scrolling
    // -----------------------------------------------------------------------

    /// Scroll the viewport.
    pub fn scroll(&mut self, dx: f64, dy: f64) {
        self.viewport.scroll_by(
            dx,
            dy,
            self.grid.total_width(),
            self.grid.total_height(),
        );
    }

    /// Zoom at a pivot point (e.g., mouse position).
    pub fn zoom_at(&mut self, new_zoom: f64, pivot_x: f64, pivot_y: f64) {
        self.viewport.zoom_at(
            new_zoom,
            pivot_x,
            pivot_y,
            self.grid.total_width(),
            self.grid.total_height(),
        );
    }

    /// Resize the viewport.
    pub fn resize(&mut self, width: f64, height: f64) {
        self.viewport.resize(width, height);
    }

    // -----------------------------------------------------------------------
    // Render frame generation
    // -----------------------------------------------------------------------

    /// Build a complete render frame for the current viewport.
    pub fn render_frame(&self) -> RenderFrame {
        let vis = self.viewport.visible_sheet_rect();
        let sel_range = self.selection.range();
        let cursor = self.selection.cursor();

        // Determine visible cell range
        let (first_col, last_col) = match self.grid.visible_cols(vis.x, vis.right()) {
            Some(r) => r,
            None => return RenderFrame::empty(),
        };
        let (first_row, last_row) = match self.grid.visible_rows(vis.y, vis.bottom()) {
            Some(r) => r,
            None => return RenderFrame::empty(),
        };

        // Build cell render data
        let mut cells = Vec::new();
        for row in first_row..=last_row {
            for col in first_col..=last_col {
                let value = self.engine.get_value(col, row);
                let sheet_rect = self.grid.cell_rect(col, row);
                let screen_rect = self.viewport.sheet_rect_to_screen(&sheet_rect);
                let text = format_value(&value);

                let flags = CellFlags {
                    selected: sel_range.contains(col, row),
                    active: col == cursor.0 && row == cursor.1,
                    editing: self.selection.is_editing()
                        && col == cursor.0
                        && row == cursor.1,
                    has_error: value.is_error(),
                    has_formula: self.engine.get_formula(col, row).is_some(),
                };

                cells.push(CellRenderData {
                    col,
                    row,
                    screen_rect,
                    text,
                    h_align: align_for_value(&value),
                    v_align: VAlign::Middle,
                    text_color: color_for_value(&value),
                    bg_color: None,
                    flags,
                });
            }
        }

        // Build grid lines
        let mut h_lines = Vec::new();
        let mut v_lines = Vec::new();

        let grid_left = self.viewport.grid_origin_x();
        let grid_top = self.viewport.grid_origin_y();

        // Horizontal lines (row separators)
        for row in first_row..=last_row + 1 {
            let sheet_y = self.grid.row_offset(row);
            let (_, screen_y) = self.viewport.sheet_to_screen(0.0, sheet_y);
            h_lines.push(GridLine {
                x1: grid_left,
                y1: screen_y,
                x2: self.viewport.screen_width,
                y2: screen_y,
            });
        }

        // Vertical lines (column separators)
        for col in first_col..=last_col + 1 {
            let sheet_x = self.grid.col_offset(col);
            let (screen_x, _) = self.viewport.sheet_to_screen(sheet_x, 0.0);
            v_lines.push(GridLine {
                x1: screen_x,
                y1: grid_top,
                x2: screen_x,
                y2: self.viewport.screen_height,
            });
        }

        // Column headers
        let mut col_headers = Vec::new();
        if self.viewport.show_headers() {
            for col in first_col..=last_col {
                let sheet_x = self.grid.col_offset(col);
                let (screen_x, _) = self.viewport.sheet_to_screen(sheet_x, 0.0);
                col_headers.push(HeaderRenderData {
                    screen_rect: CellRect {
                        x: screen_x,
                        y: 0.0,
                        width: self.grid.col_width(col) * self.viewport.zoom(),
                        height: COL_HEADER_HEIGHT,
                    },
                    text: col_to_letter(col),
                    selected: sel_range.contains(col, sel_range.min_row()),
                });
            }
        }

        // Row headers
        let mut row_headers = Vec::new();
        if self.viewport.show_headers() {
            for row in first_row..=last_row {
                let sheet_y = self.grid.row_offset(row);
                let (_, screen_y) = self.viewport.sheet_to_screen(0.0, sheet_y);
                row_headers.push(HeaderRenderData {
                    screen_rect: CellRect {
                        x: 0.0,
                        y: screen_y,
                        width: ROW_HEADER_WIDTH,
                        height: self.grid.row_height(row) * self.viewport.zoom(),
                    },
                    text: format!("{}", row + 1),
                    selected: sel_range.contains(sel_range.min_col(), row),
                });
            }
        }

        // Selection overlay
        let selection_rect = if self.selection.has_range() {
            let sheet_r = self.grid.range_rect(
                sel_range.min_col(),
                sel_range.min_row(),
                sel_range.max_col(),
                sel_range.max_row(),
            );
            Some(self.viewport.sheet_rect_to_screen(&sheet_r))
        } else {
            None
        };

        // Active cell border
        let active_sheet_r = self.grid.cell_rect(cursor.0, cursor.1);
        let active_cell_rect = Some(self.viewport.sheet_rect_to_screen(&active_sheet_r));

        // Formula bar render data
        let formula_bar_data = self.formula_bar.render_data(&[], &[]);

        RenderFrame {
            cells,
            h_lines,
            v_lines,
            col_headers,
            row_headers,
            selection_rect,
            active_cell_rect,
            formula_bar: Some(formula_bar_data),
        }
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    fn ensure_cursor_visible(&mut self) {
        let (col, row) = self.selection.cursor();
        let cell_rect = self.grid.cell_rect(col, row);
        self.viewport.ensure_visible(
            &cell_rect,
            self.grid.total_width(),
            self.grid.total_height(),
        );
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::SpreadsheetError;

    fn panel() -> SpreadsheetPanel {
        SpreadsheetPanel::new(26, 100, 800.0, 600.0)
    }

    // --- Basic setup ---

    #[test]
    fn creates_successfully() {
        let p = panel();
        assert_eq!(p.grid().num_cols(), 26);
        assert_eq!(p.grid().num_rows(), 100);
    }

    // --- Cell input ---

    #[test]
    fn set_cell_number() {
        let mut p = panel();
        p.set_cell_input(0, 0, "42");
        assert_eq!(p.engine().get_value(0, 0), Value::Number(42.0));
        assert_eq!(p.cell_display_text(0, 0), "42");
    }

    #[test]
    fn set_cell_text() {
        let mut p = panel();
        p.set_cell_input(0, 0, "Hello");
        assert_eq!(p.engine().get_value(0, 0), Value::Text("Hello".into()));
    }

    #[test]
    fn set_cell_boolean() {
        let mut p = panel();
        p.set_cell_input(0, 0, "true");
        assert_eq!(p.engine().get_value(0, 0), Value::Boolean(true));
    }

    #[test]
    fn set_cell_formula() {
        let mut p = panel();
        p.set_cell_input(0, 0, "10");
        p.set_cell_input(1, 0, "=A1 * 2");
        assert_eq!(p.engine().get_value(1, 0), Value::Number(20.0));
        assert_eq!(p.cell_edit_text(1, 0), "=A1 * 2");
        assert_eq!(p.cell_display_text(1, 0), "20");
    }

    #[test]
    fn set_cell_empty() {
        let mut p = panel();
        p.set_cell_input(0, 0, "42");
        p.set_cell_input(0, 0, "");
        assert_eq!(p.cell_display_text(0, 0), "");
    }

    // --- Mouse interaction ---

    #[test]
    fn click_selects_cell() {
        let mut p = panel();
        // Click on cell (1, 2): screen pos = (50+100+50, 24+24*2+12) = (200, 84)
        let x = ROW_HEADER_WIDTH + 100.0 + 50.0;
        let y = COL_HEADER_HEIGHT + 24.0 * 2.0 + 12.0;
        p.mouse_down(x, y, false);
        p.mouse_up();
        assert_eq!(p.selection().cursor(), (1, 2));
    }

    #[test]
    fn drag_selects_range() {
        let mut p = panel();
        let x1 = ROW_HEADER_WIDTH + 10.0;
        let y1 = COL_HEADER_HEIGHT + 5.0;
        p.mouse_down(x1, y1, false); // cell (0, 0)

        let x2 = ROW_HEADER_WIDTH + 250.0;
        let y2 = COL_HEADER_HEIGHT + 50.0;
        p.mouse_move(x2, y2); // cell (2, 2)
        p.mouse_up();

        assert!(p.selection().has_range());
        let r = p.selection().range();
        assert_eq!(r.min_col(), 0);
        assert_eq!(r.min_row(), 0);
        assert_eq!(r.max_col(), 2);
        assert_eq!(r.max_row(), 2);
    }

    // --- Keyboard navigation ---

    #[test]
    fn arrow_keys() {
        let mut p = panel();
        p.arrow_key(Direction::Right, false);
        p.arrow_key(Direction::Down, false);
        assert_eq!(p.selection().cursor(), (1, 1));
    }

    #[test]
    fn shift_arrow_extends_selection() {
        let mut p = panel();
        p.arrow_key(Direction::Right, true);
        p.arrow_key(Direction::Down, true);
        assert!(p.selection().has_range());
        let r = p.selection().range();
        assert_eq!(r.cell_count(), 4);
    }

    #[test]
    fn tab_moves_right() {
        let mut p = panel();
        p.tab_key(false);
        assert_eq!(p.selection().cursor(), (1, 0));
    }

    #[test]
    fn enter_moves_down() {
        let mut p = panel();
        p.enter_key();
        assert_eq!(p.selection().cursor(), (0, 1));
    }

    // --- Render frame ---

    #[test]
    fn render_frame_has_cells() {
        let mut p = panel();
        p.set_cell_input(0, 0, "Hello");
        p.set_cell_input(1, 0, "42");

        let frame = p.render_frame();
        assert!(!frame.cells.is_empty());

        // Find A1 in the render data
        let a1 = frame.cells.iter().find(|c| c.col == 0 && c.row == 0).unwrap();
        assert_eq!(a1.text, "Hello");
        assert_eq!(a1.h_align, HAlign::Left);

        // Find B1
        let b1 = frame.cells.iter().find(|c| c.col == 1 && c.row == 0).unwrap();
        assert_eq!(b1.text, "42");
        assert_eq!(b1.h_align, HAlign::Right);
    }

    #[test]
    fn render_frame_has_grid_lines() {
        let p = panel();
        let frame = p.render_frame();
        assert!(!frame.h_lines.is_empty());
        assert!(!frame.v_lines.is_empty());
    }

    #[test]
    fn render_frame_has_headers() {
        let p = panel();
        let frame = p.render_frame();
        assert!(!frame.col_headers.is_empty());
        assert!(!frame.row_headers.is_empty());

        // First col header should be "A"
        assert_eq!(frame.col_headers[0].text, "A");
        // First row header should be "1"
        assert_eq!(frame.row_headers[0].text, "1");
    }

    #[test]
    fn render_frame_active_cell() {
        let p = panel();
        let frame = p.render_frame();
        assert!(frame.active_cell_rect.is_some());

        // Active cell should be (0,0)
        let active = frame.cells.iter().find(|c| c.flags.active).unwrap();
        assert_eq!(active.col, 0);
        assert_eq!(active.row, 0);
    }

    #[test]
    fn render_frame_selection_overlay() {
        let mut p = panel();
        p.selection_mut().start_select(1, 1, false);
        p.selection_mut().extend_select(3, 3);
        p.selection_mut().end_select();

        let frame = p.render_frame();
        assert!(frame.selection_rect.is_some());
    }

    #[test]
    fn render_frame_error_display() {
        let mut p = panel();
        p.engine_mut()
            .set_value(0, 0, Value::Error(SpreadsheetError::Value));

        let frame = p.render_frame();
        let a1 = frame.cells.iter().find(|c| c.col == 0 && c.row == 0).unwrap();
        assert_eq!(a1.text, "#VALUE!");
        assert!(a1.flags.has_error);
        assert_eq!(a1.text_color, Color::RED);
    }

    #[test]
    fn render_frame_formula_flag() {
        let mut p = panel();
        p.set_cell_input(0, 0, "10");
        p.set_cell_input(1, 0, "=A1 + 5");

        let frame = p.render_frame();
        let b1 = frame.cells.iter().find(|c| c.col == 1 && c.row == 0).unwrap();
        assert!(b1.flags.has_formula);
        assert_eq!(b1.text, "15");
    }

    // --- Scrolling ---

    #[test]
    fn scroll_changes_visible_cells() {
        let mut p = panel();
        p.set_cell_input(20, 50, "far away");

        // Scroll to make cell (20, 50) visible
        p.scroll(20.0 * 100.0, 50.0 * 24.0);

        let frame = p.render_frame();
        let cell = frame.cells.iter().find(|c| c.col == 20 && c.row == 50);
        assert!(cell.is_some());
        assert_eq!(cell.unwrap().text, "far away");
    }

    // --- Column letter conversion ---

    #[test]
    fn col_letters() {
        assert_eq!(col_to_letter(0), "A");
        assert_eq!(col_to_letter(1), "B");
        assert_eq!(col_to_letter(25), "Z");
        assert_eq!(col_to_letter(26), "AA");
        assert_eq!(col_to_letter(27), "AB");
        assert_eq!(col_to_letter(51), "AZ");
        assert_eq!(col_to_letter(52), "BA");
    }

    // --- Format value ---

    #[test]
    fn format_integer() {
        assert_eq!(format_value(&Value::Number(42.0)), "42");
    }

    #[test]
    fn format_decimal() {
        assert_eq!(format_value(&Value::Number(3.14)), "3.14");
    }

    #[test]
    fn format_bool() {
        assert_eq!(format_value(&Value::Boolean(true)), "TRUE");
        assert_eq!(format_value(&Value::Boolean(false)), "FALSE");
    }

    #[test]
    fn format_error() {
        assert_eq!(
            format_value(&Value::Error(SpreadsheetError::DivZero)),
            "#DIV/0!"
        );
    }

    #[test]
    fn format_empty() {
        assert_eq!(format_value(&Value::Empty), "");
    }

    // --- Integration: edit & render cycle ---

    #[test]
    fn full_edit_render_cycle() {
        let mut p = panel();

        // Enter data
        p.set_cell_input(0, 0, "100");
        p.set_cell_input(0, 1, "200");
        p.set_cell_input(0, 2, "=SUM(A1:A2)");

        // Render
        let frame = p.render_frame();
        let a3 = frame.cells.iter().find(|c| c.col == 0 && c.row == 2).unwrap();
        assert_eq!(a3.text, "300");
        assert!(a3.flags.has_formula);

        // Update A1, recalc happens automatically
        p.set_cell_input(0, 0, "500");
        let frame2 = p.render_frame();
        let a3_2 = frame2.cells.iter().find(|c| c.col == 0 && c.row == 2).unwrap();
        assert_eq!(a3_2.text, "700");
    }
}
