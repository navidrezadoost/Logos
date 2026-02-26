//! Render data — computed information for drawing visible cells.
//!
//! The renderer doesn't know about formulas or the dependency graph — it
//! receives a flat list of [`CellRenderData`] items, each describing one
//! visible cell's screen rect, text content, and visual styling.

use super::grid::CellRect;

/// Horizontal text alignment within a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HAlign {
    Left,
    Center,
    Right,
}

/// Vertical text alignment within a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VAlign {
    Top,
    Middle,
    Bottom,
}

/// Cell visual state flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellFlags {
    /// Cell is part of the current selection.
    pub selected: bool,
    /// Cell is the active (cursor) cell.
    pub active: bool,
    /// Cell is being edited.
    pub editing: bool,
    /// Cell has an error value.
    pub has_error: bool,
    /// Cell has a formula (show formula indicator).
    pub has_formula: bool,
}

impl Default for CellFlags {
    fn default() -> Self {
        Self {
            selected: false,
            active: false,
            editing: false,
            has_error: false,
            has_formula: false,
        }
    }
}

/// The color of a cell element (0-255 RGBA).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const BLACK: Color = Color { r: 0, g: 0, b: 0, a: 255 };
    pub const WHITE: Color = Color { r: 255, g: 255, b: 255, a: 255 };
    pub const RED: Color = Color { r: 220, g: 53, b: 69, a: 255 };
    pub const SELECTION_BG: Color = Color { r: 66, g: 133, b: 244, a: 40 };
    pub const ACTIVE_BORDER: Color = Color { r: 66, g: 133, b: 244, a: 255 };
    pub const GRID_LINE: Color = Color { r: 218, g: 220, b: 224, a: 255 };
    pub const HEADER_BG: Color = Color { r: 242, g: 243, b: 244, a: 255 };
    pub const HEADER_TEXT: Color = Color { r: 95, g: 99, b: 104, a: 255 };
}

/// Everything a renderer needs to draw one cell.
#[derive(Debug, Clone)]
pub struct CellRenderData {
    /// Cell position in grid coordinates.
    pub col: u32,
    pub row: u32,

    /// Bounding rect in screen coordinates.
    pub screen_rect: CellRect,

    /// The display text (formatted value).
    pub text: String,

    /// Text alignment.
    pub h_align: HAlign,
    pub v_align: VAlign,

    /// Text color.
    pub text_color: Color,

    /// Background color (None = transparent/default).
    pub bg_color: Option<Color>,

    /// Visual state.
    pub flags: CellFlags,
}

/// Grid line segment for rendering.
#[derive(Debug, Clone)]
pub struct GridLine {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}

/// Header cell (column letter or row number).
#[derive(Debug, Clone)]
pub struct HeaderRenderData {
    pub screen_rect: CellRect,
    pub text: String,
    pub selected: bool,
}

/// Complete frame of data for rendering one viewport.
#[derive(Debug, Clone)]
pub struct RenderFrame {
    /// Visible cells with their render data.
    pub cells: Vec<CellRenderData>,

    /// Horizontal grid lines (screen coords).
    pub h_lines: Vec<GridLine>,

    /// Vertical grid lines (screen coords).
    pub v_lines: Vec<GridLine>,

    /// Column headers.
    pub col_headers: Vec<HeaderRenderData>,

    /// Row headers.
    pub row_headers: Vec<HeaderRenderData>,

    /// Selection overlay rect (screen coords), if any.
    pub selection_rect: Option<CellRect>,

    /// Active cell border rect (screen coords).
    pub active_cell_rect: Option<CellRect>,

    /// Formula bar render data (syntax-highlighted, completions, etc.).
    pub formula_bar: Option<super::formula_bar::FormulaBarRenderData>,
}

impl RenderFrame {
    pub fn empty() -> Self {
        Self {
            cells: Vec::new(),
            h_lines: Vec::new(),
            v_lines: Vec::new(),
            col_headers: Vec::new(),
            row_headers: Vec::new(),
            selection_rect: None,
            active_cell_rect: None,
            formula_bar: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_flags_default() {
        let f = CellFlags::default();
        assert!(!f.selected);
        assert!(!f.active);
        assert!(!f.editing);
        assert!(!f.has_error);
        assert!(!f.has_formula);
    }

    #[test]
    fn render_frame_empty() {
        let f = RenderFrame::empty();
        assert!(f.cells.is_empty());
        assert!(f.h_lines.is_empty());
        assert!(f.v_lines.is_empty());
        assert!(f.col_headers.is_empty());
        assert!(f.row_headers.is_empty());
        assert!(f.selection_rect.is_none());
        assert!(f.active_cell_rect.is_none());
    }

    #[test]
    fn color_constants() {
        assert_eq!(Color::BLACK.r, 0);
        assert_eq!(Color::WHITE.r, 255);
        assert_eq!(Color::RED.r, 220);
    }
}
