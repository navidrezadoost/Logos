//! Spreadsheet visual theme — colors, sizes, and styling constants.
//!
//! The theme controls the appearance of every visual element: grid lines,
//! cell backgrounds, headers, selection overlays, and text. All colors
//! are stored as GPU-ready `[f32; 4]` RGBA values.
//!
//! Two built-in themes are provided: [`SpreadsheetTheme::light()`] (Google
//! Sheets-like) and [`SpreadsheetTheme::dark()`] (dark mode).

use super::primitives::color_to_f32;

// ---------------------------------------------------------------------------
// SpreadsheetTheme
// ---------------------------------------------------------------------------

/// Visual theme for spreadsheet rendering.
///
/// All colors are `[r, g, b, a]` in `[0.0, 1.0]`.
#[derive(Debug, Clone, PartialEq)]
pub struct SpreadsheetTheme {
    // --- Grid ---
    /// Color of grid lines.
    pub grid_line_color: [f32; 4],
    /// Thickness of grid lines in pixels.
    pub grid_line_thickness: f32,

    // --- Cells ---
    /// Default cell background color.
    pub cell_bg: [f32; 4],
    /// Default cell text color.
    pub cell_text_color: [f32; 4],
    /// Cell text font size in pixels.
    pub cell_font_size: f32,
    /// Horizontal padding inside cells (each side).
    pub cell_padding_h: f32,
    /// Vertical padding inside cells (each side).
    pub cell_padding_v: f32,

    // --- Error cells ---
    /// Text color for error values (#REF!, #VALUE!, etc.).
    pub error_text_color: [f32; 4],

    // --- Headers ---
    /// Header background color.
    pub header_bg: [f32; 4],
    /// Header background when the column/row is selected.
    pub header_selected_bg: [f32; 4],
    /// Header text color.
    pub header_text_color: [f32; 4],
    /// Header font size in pixels.
    pub header_font_size: f32,
    /// Header separator line color.
    pub header_border_color: [f32; 4],

    // --- Selection ---
    /// Selection range fill color (semi-transparent).
    pub selection_fill: [f32; 4],
    /// Selection range border color.
    pub selection_border_color: [f32; 4],
    /// Selection border thickness.
    pub selection_border_thickness: f32,

    // --- Active cell ---
    /// Active cell border color (the cursor cell).
    pub active_cell_border_color: [f32; 4],
    /// Active cell border thickness.
    pub active_cell_border_thickness: f32,

    // --- Editing ---
    /// Background color for the cell being edited.
    pub editing_cell_bg: [f32; 4],

    // --- Corner (select-all) ---
    /// Background color of the top-left corner button.
    pub corner_bg: [f32; 4],

    // --- Z-ordering ---
    /// Z-index for cell backgrounds.
    pub z_cell_bg: f32,
    /// Z-index for grid lines.
    pub z_grid_lines: f32,
    /// Z-index for cell text.
    pub z_cell_text: f32,
    /// Z-index for headers.
    pub z_headers: f32,
    /// Z-index for selection overlay.
    pub z_selection: f32,
    /// Z-index for active cell border.
    pub z_active_cell: f32,
}

impl SpreadsheetTheme {
    /// Light theme inspired by Google Sheets.
    pub fn light() -> Self {
        Self {
            // Grid
            grid_line_color: color_to_f32(218, 220, 224, 255),
            grid_line_thickness: 1.0,

            // Cells
            cell_bg: [1.0, 1.0, 1.0, 1.0],
            cell_text_color: [0.0, 0.0, 0.0, 1.0],
            cell_font_size: 13.0,
            cell_padding_h: 4.0,
            cell_padding_v: 2.0,

            // Errors
            error_text_color: color_to_f32(220, 53, 69, 255),

            // Headers
            header_bg: color_to_f32(242, 243, 244, 255),
            header_selected_bg: color_to_f32(210, 227, 252, 255),
            header_text_color: color_to_f32(95, 99, 104, 255),
            header_font_size: 12.0,
            header_border_color: color_to_f32(196, 199, 204, 255),

            // Selection
            selection_fill: color_to_f32(66, 133, 244, 40),
            selection_border_color: color_to_f32(66, 133, 244, 255),
            selection_border_thickness: 1.0,

            // Active cell
            active_cell_border_color: color_to_f32(66, 133, 244, 255),
            active_cell_border_thickness: 2.0,

            // Editing
            editing_cell_bg: [1.0, 1.0, 1.0, 1.0],

            // Corner
            corner_bg: color_to_f32(242, 243, 244, 255),

            // Z-ordering (back to front)
            z_cell_bg: 0.0,
            z_grid_lines: 1.0,
            z_cell_text: 2.0,
            z_headers: 3.0,
            z_selection: 4.0,
            z_active_cell: 5.0,
        }
    }

    /// Dark theme for dark-mode editors.
    pub fn dark() -> Self {
        Self {
            // Grid
            grid_line_color: color_to_f32(60, 64, 67, 255),
            grid_line_thickness: 1.0,

            // Cells
            cell_bg: color_to_f32(32, 33, 36, 255),
            cell_text_color: color_to_f32(232, 234, 237, 255),
            cell_font_size: 13.0,
            cell_padding_h: 4.0,
            cell_padding_v: 2.0,

            // Errors
            error_text_color: color_to_f32(242, 139, 130, 255),

            // Headers
            header_bg: color_to_f32(41, 42, 45, 255),
            header_selected_bg: color_to_f32(44, 57, 82, 255),
            header_text_color: color_to_f32(154, 160, 166, 255),
            header_font_size: 12.0,
            header_border_color: color_to_f32(60, 64, 67, 255),

            // Selection
            selection_fill: color_to_f32(66, 133, 244, 50),
            selection_border_color: color_to_f32(138, 180, 248, 255),
            selection_border_thickness: 1.0,

            // Active cell
            active_cell_border_color: color_to_f32(138, 180, 248, 255),
            active_cell_border_thickness: 2.0,

            // Editing
            editing_cell_bg: color_to_f32(48, 49, 52, 255),

            // Corner
            corner_bg: color_to_f32(41, 42, 45, 255),

            // Z-ordering (same as light)
            z_cell_bg: 0.0,
            z_grid_lines: 1.0,
            z_cell_text: 2.0,
            z_headers: 3.0,
            z_selection: 4.0,
            z_active_cell: 5.0,
        }
    }
}

impl Default for SpreadsheetTheme {
    fn default() -> Self {
        Self::light()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_theme_defaults() {
        let t = SpreadsheetTheme::light();
        assert!((t.cell_font_size - 13.0).abs() < f32::EPSILON);
        assert!((t.grid_line_thickness - 1.0).abs() < f32::EPSILON);
        assert!((t.cell_bg[0] - 1.0).abs() < f32::EPSILON); // white
        assert!((t.cell_text_color[0]).abs() < f32::EPSILON); // black
    }

    #[test]
    fn dark_theme_defaults() {
        let t = SpreadsheetTheme::dark();
        assert!((t.cell_font_size - 13.0).abs() < f32::EPSILON);
        assert!(t.cell_bg[0] < 0.2); // dark background
        assert!(t.cell_text_color[0] > 0.8); // light text
    }

    #[test]
    fn default_is_light() {
        let default = SpreadsheetTheme::default();
        let light = SpreadsheetTheme::light();
        assert_eq!(default, light);
    }

    #[test]
    fn z_ordering() {
        let t = SpreadsheetTheme::light();
        assert!(t.z_cell_bg < t.z_grid_lines);
        assert!(t.z_grid_lines < t.z_cell_text);
        assert!(t.z_cell_text < t.z_headers);
        assert!(t.z_headers < t.z_selection);
        assert!(t.z_selection < t.z_active_cell);
    }

    #[test]
    fn selection_is_semi_transparent() {
        let t = SpreadsheetTheme::light();
        assert!(t.selection_fill[3] < 0.5); // alpha < 50%
    }

    #[test]
    fn active_cell_border_is_opaque() {
        let t = SpreadsheetTheme::light();
        assert!((t.active_cell_border_color[3] - 1.0).abs() < 0.01);
    }

    #[test]
    fn dark_error_color_is_distinct() {
        let t = SpreadsheetTheme::dark();
        // Error text should be reddish (r > g, r > b)
        assert!(t.error_text_color[0] > t.error_text_color[1]);
        assert!(t.error_text_color[0] > t.error_text_color[2]);
    }
}
