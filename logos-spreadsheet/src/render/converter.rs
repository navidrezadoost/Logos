//! RenderFrame → DrawBatch converter.
//!
//! This is the core of the rendering pipeline: it takes a logical
//! [`RenderFrame`] (cell data, grid lines, headers, selection) and
//! produces a [`DrawBatch`] of GPU-ready draw primitives styled by
//! a [`SpreadsheetTheme`].
//!
//! The converter handles:
//! - Cell background fills (with selection/editing highlights)
//! - Grid lines → thin rects for instanced drawing
//! - Cell text positioning and alignment
//! - Column/row header rendering
//! - Selection overlay with semi-transparent fill + border
//! - Active cell (cursor) border
//! - Corner (select-all) button

use crate::ui::grid::{CellRect, COL_HEADER_HEIGHT, ROW_HEADER_WIDTH};
use crate::ui::render_data::{
    CellRenderData, Color, GridLine, HAlign, HeaderRenderData, RenderFrame, VAlign,
};

use super::batch::DrawBatch;
use super::primitives::{
    color_to_f32, DrawBorder, DrawLine, DrawRect, DrawText, TextAlign, TextVAlign,
};
use super::theme::SpreadsheetTheme;

// ---------------------------------------------------------------------------
// BatchConverter
// ---------------------------------------------------------------------------

/// Converts a [`RenderFrame`] into a [`DrawBatch`] using a theme.
///
/// The converter is stateless — call [`convert()`] each frame with
/// the latest `RenderFrame`.
#[derive(Debug, Clone)]
pub struct BatchConverter {
    theme: SpreadsheetTheme,
}

impl BatchConverter {
    /// Create a converter with the given theme.
    pub fn new(theme: SpreadsheetTheme) -> Self {
        Self { theme }
    }

    /// Create a converter with the default (light) theme.
    pub fn light() -> Self {
        Self::new(SpreadsheetTheme::light())
    }

    /// Create a converter with the dark theme.
    pub fn dark() -> Self {
        Self::new(SpreadsheetTheme::dark())
    }

    /// Get a reference to the current theme.
    pub fn theme(&self) -> &SpreadsheetTheme {
        &self.theme
    }

    /// Set a new theme.
    pub fn set_theme(&mut self, theme: SpreadsheetTheme) {
        self.theme = theme;
    }

    /// Convert a `RenderFrame` into a `DrawBatch`.
    ///
    /// This is the main entry point. Call once per frame.
    pub fn convert(&self, frame: &RenderFrame) -> DrawBatch {
        let cell_count = frame.cells.len();
        let line_count = frame.h_lines.len() + frame.v_lines.len();
        let header_count = frame.col_headers.len() + frame.row_headers.len();

        let mut batch = DrawBatch::with_capacity(cell_count, line_count, header_count);

        // Layer 0–2: Cells (background, then text)
        self.convert_cells(&frame.cells, &mut batch);

        // Layer 1: Grid lines
        self.convert_grid_lines(&frame.h_lines, &frame.v_lines, &mut batch);

        // Layer 3–4: Headers
        self.convert_headers(
            &frame.col_headers,
            &frame.row_headers,
            &mut batch,
        );

        // Layer 5: Selection overlay
        if let Some(sel_rect) = &frame.selection_rect {
            self.convert_selection(sel_rect, &mut batch);
        }

        // Layer 6: Active cell border
        if let Some(active_rect) = &frame.active_cell_rect {
            self.convert_active_cell(active_rect, &mut batch);
        }

        batch
    }

    // -----------------------------------------------------------------------
    // Cell conversion
    // -----------------------------------------------------------------------

    fn convert_cells(&self, cells: &[CellRenderData], batch: &mut DrawBatch) {
        let t = &self.theme;

        for cell in cells {
            // Cell background
            let bg_color = if cell.flags.editing {
                t.editing_cell_bg
            } else if let Some(custom_bg) = &cell.bg_color {
                color_from_ui(*custom_bg)
            } else {
                t.cell_bg
            };

            batch.cell_backgrounds.push(
                DrawRect::new(
                    cell.screen_rect.x as f32,
                    cell.screen_rect.y as f32,
                    cell.screen_rect.width as f32,
                    cell.screen_rect.height as f32,
                    bg_color,
                )
                .with_z(t.z_cell_bg),
            );

            // Cell text (skip empty)
            if !cell.text.is_empty() {
                let text_color = if cell.flags.has_error {
                    t.error_text_color
                } else {
                    color_from_ui(cell.text_color)
                };

                let align = match cell.h_align {
                    HAlign::Left => TextAlign::Left,
                    HAlign::Center => TextAlign::Center,
                    HAlign::Right => TextAlign::Right,
                };

                let v_align = match cell.v_align {
                    VAlign::Top => TextVAlign::Top,
                    VAlign::Middle => TextVAlign::Middle,
                    VAlign::Bottom => TextVAlign::Bottom,
                };

                let padded_x = cell.screen_rect.x as f32 + t.cell_padding_h;
                let padded_y = cell.screen_rect.y as f32 + t.cell_padding_v;
                let padded_w = (cell.screen_rect.width as f32 - 2.0 * t.cell_padding_h).max(0.0);
                let padded_h =
                    (cell.screen_rect.height as f32 - 2.0 * t.cell_padding_v).max(0.0);

                batch.cell_texts.push(
                    DrawText::new(padded_x, padded_y, &cell.text, text_color)
                        .with_size(t.cell_font_size)
                        .with_align(align)
                        .with_v_align(v_align)
                        .with_bounds(padded_w, padded_h)
                        .with_bold(cell.flags.has_error) // errors in bold
                        .with_clip(
                            cell.screen_rect.x as f32,
                            cell.screen_rect.y as f32,
                            cell.screen_rect.width as f32,
                            cell.screen_rect.height as f32,
                        ),
                );
            }
        }
    }

    // -----------------------------------------------------------------------
    // Grid lines
    // -----------------------------------------------------------------------

    fn convert_grid_lines(
        &self,
        h_lines: &[GridLine],
        v_lines: &[GridLine],
        batch: &mut DrawBatch,
    ) {
        let t = &self.theme;

        for line in h_lines {
            let dl = DrawLine::new(
                line.x1 as f32,
                line.y1 as f32,
                line.x2 as f32,
                line.y2 as f32,
                t.grid_line_color,
            )
            .with_thickness(t.grid_line_thickness);

            batch.grid_lines.push(dl.to_rect().with_z(t.z_grid_lines));
        }

        for line in v_lines {
            let dl = DrawLine::new(
                line.x1 as f32,
                line.y1 as f32,
                line.x2 as f32,
                line.y2 as f32,
                t.grid_line_color,
            )
            .with_thickness(t.grid_line_thickness);

            batch.grid_lines.push(dl.to_rect().with_z(t.z_grid_lines));
        }
    }

    // -----------------------------------------------------------------------
    // Headers
    // -----------------------------------------------------------------------

    fn convert_headers(
        &self,
        col_headers: &[HeaderRenderData],
        row_headers: &[HeaderRenderData],
        batch: &mut DrawBatch,
    ) {
        let t = &self.theme;

        // Column headers (top bar)
        for header in col_headers {
            let bg = if header.selected {
                t.header_selected_bg
            } else {
                t.header_bg
            };

            batch.header_backgrounds.push(
                DrawRect::new(
                    header.screen_rect.x as f32,
                    header.screen_rect.y as f32,
                    header.screen_rect.width as f32,
                    header.screen_rect.height as f32,
                    bg,
                )
                .with_z(t.z_headers),
            );

            batch.header_texts.push(
                DrawText::new(
                    header.screen_rect.x as f32,
                    header.screen_rect.y as f32,
                    &header.text,
                    t.header_text_color,
                )
                .with_size(t.header_font_size)
                .with_align(TextAlign::Center)
                .with_v_align(TextVAlign::Middle)
                .with_bounds(
                    header.screen_rect.width as f32,
                    header.screen_rect.height as f32,
                ),
            );
        }

        // Row headers (left bar)
        for header in row_headers {
            let bg = if header.selected {
                t.header_selected_bg
            } else {
                t.header_bg
            };

            batch.header_backgrounds.push(
                DrawRect::new(
                    header.screen_rect.x as f32,
                    header.screen_rect.y as f32,
                    header.screen_rect.width as f32,
                    header.screen_rect.height as f32,
                    bg,
                )
                .with_z(t.z_headers),
            );

            batch.header_texts.push(
                DrawText::new(
                    header.screen_rect.x as f32,
                    header.screen_rect.y as f32,
                    &header.text,
                    t.header_text_color,
                )
                .with_size(t.header_font_size)
                .with_align(TextAlign::Center)
                .with_v_align(TextVAlign::Middle)
                .with_bounds(
                    header.screen_rect.width as f32,
                    header.screen_rect.height as f32,
                ),
            );
        }

        // Corner button (top-left)
        if !col_headers.is_empty() || !row_headers.is_empty() {
            batch.corner_rect = Some(
                DrawRect::new(
                    0.0,
                    0.0,
                    ROW_HEADER_WIDTH as f32,
                    COL_HEADER_HEIGHT as f32,
                    t.corner_bg,
                )
                .with_z(t.z_headers),
            );
        }
    }

    // -----------------------------------------------------------------------
    // Selection overlay
    // -----------------------------------------------------------------------

    fn convert_selection(&self, sel_rect: &CellRect, batch: &mut DrawBatch) {
        let t = &self.theme;

        // Semi-transparent fill
        batch.selection_fill = Some(
            DrawRect::new(
                sel_rect.x as f32,
                sel_rect.y as f32,
                sel_rect.width as f32,
                sel_rect.height as f32,
                t.selection_fill,
            )
            .with_z(t.z_selection),
        );

        // Solid border
        batch.selection_border = Some(
            DrawBorder::new(
                sel_rect.x as f32,
                sel_rect.y as f32,
                sel_rect.width as f32,
                sel_rect.height as f32,
                t.selection_border_color,
            )
            .with_thickness(t.selection_border_thickness)
            .with_z(t.z_selection),
        );
    }

    // -----------------------------------------------------------------------
    // Active cell border
    // -----------------------------------------------------------------------

    fn convert_active_cell(&self, active_rect: &CellRect, batch: &mut DrawBatch) {
        let t = &self.theme;

        batch.active_cell_border = Some(
            DrawBorder::new(
                active_rect.x as f32,
                active_rect.y as f32,
                active_rect.width as f32,
                active_rect.height as f32,
                t.active_cell_border_color,
            )
            .with_thickness(t.active_cell_border_thickness)
            .with_z(t.z_active_cell),
        );
    }
}

impl Default for BatchConverter {
    fn default() -> Self {
        Self::light()
    }
}

// ---------------------------------------------------------------------------
// Color helpers
// ---------------------------------------------------------------------------

/// Convert a UI `Color` (u8 RGBA) to [f32; 4].
fn color_from_ui(c: Color) -> [f32; 4] {
    color_to_f32(c.r, c.g, c.b, c.a)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::grid::CellRect;
    use crate::ui::render_data::*;

    fn make_cell(col: u32, row: u32, text: &str) -> CellRenderData {
        CellRenderData {
            col,
            row,
            screen_rect: CellRect {
                x: col as f64 * 100.0 + 50.0,
                y: row as f64 * 24.0 + 24.0,
                width: 100.0,
                height: 24.0,
            },
            text: text.to_string(),
            h_align: HAlign::Left,
            v_align: VAlign::Middle,
            text_color: Color::BLACK,
            bg_color: None,
            flags: CellFlags::default(),
        }
    }

    fn make_header(text: &str, x: f64, y: f64, w: f64, h: f64, selected: bool) -> HeaderRenderData {
        HeaderRenderData {
            screen_rect: CellRect {
                x,
                y,
                width: w,
                height: h,
            },
            text: text.to_string(),
            selected,
        }
    }

    fn make_grid_line(x1: f64, y1: f64, x2: f64, y2: f64) -> GridLine {
        GridLine { x1, y1, x2, y2 }
    }

    // --- Basic conversion ---

    #[test]
    fn empty_frame_produces_empty_batch() {
        let conv = BatchConverter::light();
        let frame = RenderFrame::empty();
        let batch = conv.convert(&frame);
        assert!(batch.is_empty());
    }

    #[test]
    fn cells_produce_backgrounds_and_text() {
        let conv = BatchConverter::light();
        let frame = RenderFrame {
            cells: vec![
                make_cell(0, 0, "Hello"),
                make_cell(1, 0, "42"),
            ],
            h_lines: vec![],
            v_lines: vec![],
            col_headers: vec![],
            row_headers: vec![],
            selection_rect: None,
            active_cell_rect: None,
            formula_bar: None,
        };

        let batch = conv.convert(&frame);
        assert_eq!(batch.cell_backgrounds.len(), 2);
        assert_eq!(batch.cell_texts.len(), 2);
    }

    #[test]
    fn empty_cell_text_skipped() {
        let conv = BatchConverter::light();
        let frame = RenderFrame {
            cells: vec![make_cell(0, 0, "")],
            h_lines: vec![],
            v_lines: vec![],
            col_headers: vec![],
            row_headers: vec![],
            selection_rect: None,
            active_cell_rect: None,
            formula_bar: None,
        };

        let batch = conv.convert(&frame);
        assert_eq!(batch.cell_backgrounds.len(), 1);
        assert_eq!(batch.cell_texts.len(), 0); // empty text skipped
    }

    // --- Grid lines ---

    #[test]
    fn grid_lines_converted_to_rects() {
        let conv = BatchConverter::light();
        let frame = RenderFrame {
            cells: vec![],
            h_lines: vec![
                make_grid_line(50.0, 24.0, 800.0, 24.0),
                make_grid_line(50.0, 48.0, 800.0, 48.0),
            ],
            v_lines: vec![
                make_grid_line(50.0, 24.0, 50.0, 600.0),
                make_grid_line(150.0, 24.0, 150.0, 600.0),
            ],
            col_headers: vec![],
            row_headers: vec![],
            selection_rect: None,
            active_cell_rect: None,
            formula_bar: None,
        };

        let batch = conv.convert(&frame);
        assert_eq!(batch.grid_lines.len(), 4); // 2 h + 2 v
    }

    // --- Headers ---

    #[test]
    fn headers_produce_bg_and_text() {
        let conv = BatchConverter::light();
        let frame = RenderFrame {
            cells: vec![],
            h_lines: vec![],
            v_lines: vec![],
            col_headers: vec![
                make_header("A", 50.0, 0.0, 100.0, 24.0, false),
                make_header("B", 150.0, 0.0, 100.0, 24.0, true),
            ],
            row_headers: vec![
                make_header("1", 0.0, 24.0, 50.0, 24.0, false),
            ],
            selection_rect: None,
            active_cell_rect: None,
            formula_bar: None,
        };

        let batch = conv.convert(&frame);
        assert_eq!(batch.header_backgrounds.len(), 3); // 2 col + 1 row
        assert_eq!(batch.header_texts.len(), 3);
        assert!(batch.corner_rect.is_some()); // corner button
    }

    #[test]
    fn selected_header_uses_different_bg() {
        let conv = BatchConverter::light();
        let frame = RenderFrame {
            cells: vec![],
            h_lines: vec![],
            v_lines: vec![],
            col_headers: vec![
                make_header("A", 50.0, 0.0, 100.0, 24.0, false),
                make_header("B", 150.0, 0.0, 100.0, 24.0, true),
            ],
            row_headers: vec![],
            selection_rect: None,
            active_cell_rect: None,
            formula_bar: None,
        };

        let batch = conv.convert(&frame);
        let t = conv.theme();

        // First header (not selected) → normal bg
        assert_eq!(batch.header_backgrounds[0].color, t.header_bg);
        // Second header (selected) → selected bg
        assert_eq!(batch.header_backgrounds[1].color, t.header_selected_bg);
    }

    // --- Selection ---

    #[test]
    fn selection_produces_fill_and_border() {
        let conv = BatchConverter::light();
        let frame = RenderFrame {
            cells: vec![],
            h_lines: vec![],
            v_lines: vec![],
            col_headers: vec![],
            row_headers: vec![],
            selection_rect: Some(CellRect {
                x: 50.0,
                y: 24.0,
                width: 300.0,
                height: 72.0,
            }),
            active_cell_rect: None,
            formula_bar: None,
        };

        let batch = conv.convert(&frame);
        assert!(batch.selection_fill.is_some());
        assert!(batch.selection_border.is_some());

        let fill = batch.selection_fill.unwrap();
        assert!((fill.x - 50.0).abs() < f32::EPSILON);
        assert!((fill.width - 300.0).abs() < f32::EPSILON);
    }

    // --- Active cell ---

    #[test]
    fn active_cell_produces_border() {
        let conv = BatchConverter::light();
        let frame = RenderFrame {
            cells: vec![],
            h_lines: vec![],
            v_lines: vec![],
            col_headers: vec![],
            row_headers: vec![],
            selection_rect: None,
            active_cell_rect: Some(CellRect {
                x: 50.0,
                y: 24.0,
                width: 100.0,
                height: 24.0,
            }),
            formula_bar: None,
        };

        let batch = conv.convert(&frame);
        assert!(batch.active_cell_border.is_some());

        let border = batch.active_cell_border.unwrap();
        assert!((border.x - 50.0).abs() < f32::EPSILON);
        assert!((border.width - 100.0).abs() < f32::EPSILON);
        assert!((border.thickness - 2.0).abs() < f32::EPSILON); // default theme
    }

    // --- Theme switching ---

    #[test]
    fn dark_theme_converter() {
        let conv = BatchConverter::dark();
        let frame = RenderFrame {
            cells: vec![make_cell(0, 0, "Test")],
            h_lines: vec![],
            v_lines: vec![],
            col_headers: vec![],
            row_headers: vec![],
            selection_rect: None,
            active_cell_rect: None,
            formula_bar: None,
        };

        let batch = conv.convert(&frame);
        // Dark theme: cell bg should be dark
        assert!(batch.cell_backgrounds[0].color[0] < 0.2);
    }

    // --- Error cell styling ---

    #[test]
    fn error_cell_uses_error_color() {
        let conv = BatchConverter::light();
        let mut cell = make_cell(0, 0, "#VALUE!");
        cell.flags.has_error = true;
        cell.text_color = Color::RED;

        let frame = RenderFrame {
            cells: vec![cell],
            h_lines: vec![],
            v_lines: vec![],
            col_headers: vec![],
            row_headers: vec![],
            selection_rect: None,
            active_cell_rect: None,
            formula_bar: None,
        };

        let batch = conv.convert(&frame);
        let text = &batch.cell_texts[0];
        assert_eq!(text.color, conv.theme().error_text_color);
        assert!(text.bold); // errors render bold
    }

    // --- Editing cell ---

    #[test]
    fn editing_cell_uses_editing_bg() {
        let conv = BatchConverter::light();
        let mut cell = make_cell(0, 0, "editing...");
        cell.flags.editing = true;

        let frame = RenderFrame {
            cells: vec![cell],
            h_lines: vec![],
            v_lines: vec![],
            col_headers: vec![],
            row_headers: vec![],
            selection_rect: None,
            active_cell_rect: None,
            formula_bar: None,
        };

        let batch = conv.convert(&frame);
        assert_eq!(batch.cell_backgrounds[0].color, conv.theme().editing_cell_bg);
    }

    // --- Custom cell background ---

    #[test]
    fn custom_cell_bg_used() {
        let conv = BatchConverter::light();
        let mut cell = make_cell(0, 0, "Hi");
        cell.bg_color = Some(Color {
            r: 255,
            g: 200,
            b: 0,
            a: 255,
        });

        let frame = RenderFrame {
            cells: vec![cell],
            h_lines: vec![],
            v_lines: vec![],
            col_headers: vec![],
            row_headers: vec![],
            selection_rect: None,
            active_cell_rect: None,
            formula_bar: None,
        };

        let batch = conv.convert(&frame);
        let bg = batch.cell_backgrounds[0].color;
        assert!((bg[0] - 1.0).abs() < 0.01); // full red
        assert!((bg[1] - 200.0 / 255.0).abs() < 0.01); // green
        assert!((bg[2]).abs() < 0.01); // no blue
    }

    // --- Text alignment ---

    #[test]
    fn text_alignment_converted() {
        let conv = BatchConverter::light();
        let mut left_cell = make_cell(0, 0, "left");
        left_cell.h_align = HAlign::Left;

        let mut right_cell = make_cell(1, 0, "right");
        right_cell.h_align = HAlign::Right;

        let mut center_cell = make_cell(2, 0, "center");
        center_cell.h_align = HAlign::Center;

        let frame = RenderFrame {
            cells: vec![left_cell, right_cell, center_cell],
            h_lines: vec![],
            v_lines: vec![],
            col_headers: vec![],
            row_headers: vec![],
            selection_rect: None,
            active_cell_rect: None,
            formula_bar: None,
        };

        let batch = conv.convert(&frame);
        assert_eq!(batch.cell_texts[0].align, TextAlign::Left);
        assert_eq!(batch.cell_texts[1].align, TextAlign::Right);
        assert_eq!(batch.cell_texts[2].align, TextAlign::Center);
    }

    // --- Z-ordering ---

    #[test]
    fn z_ordering_correct() {
        let conv = BatchConverter::light();
        let frame = RenderFrame {
            cells: vec![make_cell(0, 0, "Test")],
            h_lines: vec![make_grid_line(50.0, 24.0, 800.0, 24.0)],
            v_lines: vec![],
            col_headers: vec![make_header("A", 50.0, 0.0, 100.0, 24.0, false)],
            row_headers: vec![],
            selection_rect: Some(CellRect {
                x: 50.0,
                y: 24.0,
                width: 100.0,
                height: 24.0,
            }),
            active_cell_rect: Some(CellRect {
                x: 50.0,
                y: 24.0,
                width: 100.0,
                height: 24.0,
            }),
            formula_bar: None,
        };

        let batch = conv.convert(&frame);

        // Cell bg < grid lines
        assert!(batch.cell_backgrounds[0].z_index < batch.grid_lines[0].z_index);
        // Grid lines < headers
        assert!(batch.grid_lines[0].z_index < batch.header_backgrounds[0].z_index);
        // Headers < selection
        assert!(batch.header_backgrounds[0].z_index < batch.selection_fill.unwrap().z_index);
        // Selection < active cell
        assert!(
            batch.selection_fill.unwrap().z_index
                < batch.active_cell_border.unwrap().z_index
        );
    }

    // --- Full integration ---

    #[test]
    fn full_frame_conversion() {
        let conv = BatchConverter::light();

        let mut error_cell = make_cell(2, 0, "#DIV/0!");
        error_cell.flags.has_error = true;

        let frame = RenderFrame {
            cells: vec![
                make_cell(0, 0, "Hello"),
                make_cell(1, 0, "42"),
                error_cell,
                make_cell(0, 1, ""),
            ],
            h_lines: vec![
                make_grid_line(50.0, 24.0, 800.0, 24.0),
                make_grid_line(50.0, 48.0, 800.0, 48.0),
                make_grid_line(50.0, 72.0, 800.0, 72.0),
            ],
            v_lines: vec![
                make_grid_line(50.0, 24.0, 50.0, 600.0),
                make_grid_line(150.0, 24.0, 150.0, 600.0),
                make_grid_line(250.0, 24.0, 250.0, 600.0),
            ],
            col_headers: vec![
                make_header("A", 50.0, 0.0, 100.0, 24.0, true),
                make_header("B", 150.0, 0.0, 100.0, 24.0, false),
                make_header("C", 250.0, 0.0, 100.0, 24.0, false),
            ],
            row_headers: vec![
                make_header("1", 0.0, 24.0, 50.0, 24.0, true),
                make_header("2", 0.0, 48.0, 50.0, 24.0, false),
            ],
            selection_rect: Some(CellRect {
                x: 50.0,
                y: 24.0,
                width: 200.0,
                height: 48.0,
            }),
            active_cell_rect: Some(CellRect {
                x: 50.0,
                y: 24.0,
                width: 100.0,
                height: 24.0,
            }),
            formula_bar: None,
        };

        let batch = conv.convert(&frame);
        let stats = batch.stats();

        assert_eq!(stats.cell_bg_count, 4); // 4 cells (including empty)
        assert_eq!(stats.grid_line_count, 6); // 3h + 3v
        assert_eq!(stats.cell_text_count, 3); // 3 non-empty cells
        assert_eq!(stats.header_bg_count, 5); // 3 col + 2 row
        assert_eq!(stats.header_text_count, 5);
        assert!(stats.has_selection);
        assert!(stats.has_active_cell);
        assert!(stats.draw_calls >= 5);
    }

    // --- Cell padding ---

    #[test]
    fn cell_text_has_padding() {
        let conv = BatchConverter::light();
        let frame = RenderFrame {
            cells: vec![make_cell(0, 0, "Test")],
            h_lines: vec![],
            v_lines: vec![],
            col_headers: vec![],
            row_headers: vec![],
            selection_rect: None,
            active_cell_rect: None,
            formula_bar: None,
        };

        let batch = conv.convert(&frame);
        let text = &batch.cell_texts[0];
        let bg = &batch.cell_backgrounds[0];
        let t = conv.theme();

        // Text should be offset by padding from the background
        assert!((text.x - bg.x - t.cell_padding_h).abs() < f32::EPSILON);
        assert!((text.y - bg.y - t.cell_padding_v).abs() < f32::EPSILON);
        assert!(
            (text.max_width - (bg.width - 2.0 * t.cell_padding_h)).abs() < f32::EPSILON
        );
    }

    // --- Clip rect ---

    #[test]
    fn cell_text_clipped_to_cell() {
        let conv = BatchConverter::light();
        let frame = RenderFrame {
            cells: vec![make_cell(0, 0, "Long text that might overflow")],
            h_lines: vec![],
            v_lines: vec![],
            col_headers: vec![],
            row_headers: vec![],
            selection_rect: None,
            active_cell_rect: None,
            formula_bar: None,
        };

        let batch = conv.convert(&frame);
        let text = &batch.cell_texts[0];
        assert!(text.clip_rect.is_some());

        let clip = text.clip_rect.unwrap();
        // Clip should match cell bounds
        assert!((clip[0] - 50.0).abs() < f32::EPSILON); // x = col*100 + 50
        assert!((clip[1] - 24.0).abs() < f32::EPSILON); // y = row*24 + 24
        assert!((clip[2] - 100.0).abs() < f32::EPSILON); // width
        assert!((clip[3] - 24.0).abs() < f32::EPSILON); // height
    }
}
