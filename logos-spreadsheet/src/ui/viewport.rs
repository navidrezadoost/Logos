//! Viewport — scroll position, zoom level, and coordinate transforms.
//!
//! The viewport sits between sheet coordinates (logical pixels) and
//! screen coordinates (device pixels). It handles zooming, panning,
//! and the transforms needed to map between the two spaces.
//!
//! ```text
//!  Sheet coords          Viewport transform            Screen coords
//!  (col/row → px)  ──── zoom + scroll_offset ────►  (canvas px)
//! ```

use super::grid::{CellRect, COL_HEADER_HEIGHT, ROW_HEADER_WIDTH};

/// Zoom limits.
pub const MIN_ZOOM: f64 = 0.1;
pub const MAX_ZOOM: f64 = 4.0;
pub const DEFAULT_ZOOM: f64 = 1.0;

/// Scroll speed multiplier for smooth scrolling.
pub const SCROLL_LINE_HEIGHT: f64 = 40.0;

// ---------------------------------------------------------------------------
// Viewport
// ---------------------------------------------------------------------------

/// The visible window into the spreadsheet.
///
/// Tracks the scroll position (in sheet coordinates) and zoom level.
/// Provides methods to convert between sheet ↔ screen coordinates.
#[derive(Debug, Clone)]
pub struct Viewport {
    /// Width of the viewport in screen pixels.
    pub screen_width: f64,
    /// Height of the viewport in screen pixels.
    pub screen_height: f64,

    /// Horizontal scroll offset in sheet coordinates.
    scroll_x: f64,
    /// Vertical scroll offset in sheet coordinates.
    scroll_y: f64,

    /// Zoom factor (1.0 = 100%).
    zoom: f64,

    /// Whether to show row/column headers.
    show_headers: bool,
}

impl Viewport {
    pub fn new(screen_width: f64, screen_height: f64) -> Self {
        Self {
            screen_width,
            screen_height,
            scroll_x: 0.0,
            scroll_y: 0.0,
            zoom: DEFAULT_ZOOM,
            show_headers: true,
        }
    }

    // -----------------------------------------------------------------------
    // Accessors
    // -----------------------------------------------------------------------

    pub fn scroll_x(&self) -> f64 {
        self.scroll_x
    }

    pub fn scroll_y(&self) -> f64 {
        self.scroll_y
    }

    pub fn zoom(&self) -> f64 {
        self.zoom
    }

    pub fn show_headers(&self) -> bool {
        self.show_headers
    }

    pub fn set_show_headers(&mut self, show: bool) {
        self.show_headers = show;
    }

    /// The x offset where the cell area starts (after the row header).
    pub fn grid_origin_x(&self) -> f64 {
        if self.show_headers {
            ROW_HEADER_WIDTH
        } else {
            0.0
        }
    }

    /// The y offset where the cell area starts (after the column header).
    pub fn grid_origin_y(&self) -> f64 {
        if self.show_headers {
            COL_HEADER_HEIGHT
        } else {
            0.0
        }
    }

    /// The visible area width available for cells (screen minus headers).
    pub fn visible_width(&self) -> f64 {
        self.screen_width - self.grid_origin_x()
    }

    /// The visible area height available for cells.
    pub fn visible_height(&self) -> f64 {
        self.screen_height - self.grid_origin_y()
    }

    /// The visible area in sheet coordinates.
    pub fn visible_sheet_rect(&self) -> CellRect {
        CellRect {
            x: self.scroll_x,
            y: self.scroll_y,
            width: self.visible_width() / self.zoom,
            height: self.visible_height() / self.zoom,
        }
    }

    // -----------------------------------------------------------------------
    // Scrolling
    // -----------------------------------------------------------------------

    /// Set the scroll position, clamping to valid bounds.
    pub fn set_scroll(&mut self, x: f64, y: f64, max_x: f64, max_y: f64) {
        let max_scroll_x = (max_x - self.visible_width() / self.zoom).max(0.0);
        let max_scroll_y = (max_y - self.visible_height() / self.zoom).max(0.0);
        self.scroll_x = x.clamp(0.0, max_scroll_x);
        self.scroll_y = y.clamp(0.0, max_scroll_y);
    }

    /// Scroll by a delta (in screen pixels).
    pub fn scroll_by(&mut self, dx: f64, dy: f64, max_x: f64, max_y: f64) {
        let sheet_dx = dx / self.zoom;
        let sheet_dy = dy / self.zoom;
        self.set_scroll(
            self.scroll_x + sheet_dx,
            self.scroll_y + sheet_dy,
            max_x,
            max_y,
        );
    }

    /// Scroll to make a specific cell visible, with minimal movement.
    pub fn ensure_visible(
        &mut self,
        cell_rect: &CellRect,
        max_x: f64,
        max_y: f64,
    ) {
        let vis = self.visible_sheet_rect();

        let mut sx = self.scroll_x;
        let mut sy = self.scroll_y;

        // Horizontal
        if cell_rect.x < vis.x {
            sx = cell_rect.x;
        } else if cell_rect.right() > vis.right() {
            sx = cell_rect.right() - vis.width;
        }

        // Vertical
        if cell_rect.y < vis.y {
            sy = cell_rect.y;
        } else if cell_rect.bottom() > vis.bottom() {
            sy = cell_rect.bottom() - vis.height;
        }

        self.set_scroll(sx, sy, max_x, max_y);
    }

    // -----------------------------------------------------------------------
    // Zoom
    // -----------------------------------------------------------------------

    /// Set the zoom level, clamped to valid range.
    pub fn set_zoom(&mut self, zoom: f64) {
        self.zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
    }

    /// Zoom in/out, keeping the given screen point fixed.
    /// `pivot_x`, `pivot_y` are in screen coordinates.
    pub fn zoom_at(
        &mut self,
        new_zoom: f64,
        pivot_x: f64,
        pivot_y: f64,
        max_x: f64,
        max_y: f64,
    ) {
        let new_zoom = new_zoom.clamp(MIN_ZOOM, MAX_ZOOM);

        // Convert pivot from screen to sheet coords (before zoom change)
        let sheet_px = (pivot_x - self.grid_origin_x()) / self.zoom + self.scroll_x;
        let sheet_py = (pivot_y - self.grid_origin_y()) / self.zoom + self.scroll_y;

        self.zoom = new_zoom;

        // Adjust scroll so the pivot still maps to the same screen point
        let new_sx = sheet_px - (pivot_x - self.grid_origin_x()) / self.zoom;
        let new_sy = sheet_py - (pivot_y - self.grid_origin_y()) / self.zoom;

        self.set_scroll(new_sx, new_sy, max_x, max_y);
    }

    // -----------------------------------------------------------------------
    // Coordinate transforms
    // -----------------------------------------------------------------------

    /// Convert a screen point to sheet coordinates.
    pub fn screen_to_sheet(&self, screen_x: f64, screen_y: f64) -> (f64, f64) {
        let sheet_x = (screen_x - self.grid_origin_x()) / self.zoom + self.scroll_x;
        let sheet_y = (screen_y - self.grid_origin_y()) / self.zoom + self.scroll_y;
        (sheet_x, sheet_y)
    }

    /// Convert a sheet point to screen coordinates.
    pub fn sheet_to_screen(&self, sheet_x: f64, sheet_y: f64) -> (f64, f64) {
        let screen_x = (sheet_x - self.scroll_x) * self.zoom + self.grid_origin_x();
        let screen_y = (sheet_y - self.scroll_y) * self.zoom + self.grid_origin_y();
        (screen_x, screen_y)
    }

    /// Convert a CellRect from sheet coordinates to screen coordinates.
    pub fn sheet_rect_to_screen(&self, r: &CellRect) -> CellRect {
        let (sx, sy) = self.sheet_to_screen(r.x, r.y);
        CellRect {
            x: sx,
            y: sy,
            width: r.width * self.zoom,
            height: r.height * self.zoom,
        }
    }

    /// Check if a screen point is in the row header area.
    pub fn is_in_row_header(&self, screen_x: f64, screen_y: f64) -> bool {
        self.show_headers && screen_x < ROW_HEADER_WIDTH && screen_y >= COL_HEADER_HEIGHT
    }

    /// Check if a screen point is in the column header area.
    pub fn is_in_col_header(&self, screen_x: f64, screen_y: f64) -> bool {
        self.show_headers && screen_y < COL_HEADER_HEIGHT && screen_x >= ROW_HEADER_WIDTH
    }

    /// Check if a screen point is in the top-left corner (select-all button).
    pub fn is_in_corner(&self, screen_x: f64, screen_y: f64) -> bool {
        self.show_headers && screen_x < ROW_HEADER_WIDTH && screen_y < COL_HEADER_HEIGHT
    }

    /// Resize the viewport.
    pub fn resize(&mut self, width: f64, height: f64) {
        self.screen_width = width;
        self.screen_height = height;
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn vp() -> Viewport {
        Viewport::new(800.0, 600.0)
    }

    #[test]
    fn default_state() {
        let v = vp();
        assert_eq!(v.zoom(), DEFAULT_ZOOM);
        assert_eq!(v.scroll_x(), 0.0);
        assert_eq!(v.scroll_y(), 0.0);
    }

    #[test]
    fn visible_sheet_rect() {
        let v = vp();
        let r = v.visible_sheet_rect();
        // screen_width=800, headers=50, visible=750
        assert_eq!(r.x, 0.0);
        assert_eq!(r.y, 0.0);
        assert_eq!(r.width, 750.0);
        assert_eq!(r.height, 576.0);
    }

    #[test]
    fn visible_sheet_rect_zoomed() {
        let mut v = vp();
        v.set_zoom(2.0);
        let r = v.visible_sheet_rect();
        // visible_width=750 / 2.0 = 375
        assert_eq!(r.width, 375.0);
        assert_eq!(r.height, 288.0);
    }

    #[test]
    fn screen_to_sheet_identity() {
        let v = vp();
        // At zoom=1, scroll=0, the mapping is just offset by header
        let (sx, sy) = v.screen_to_sheet(ROW_HEADER_WIDTH, COL_HEADER_HEIGHT);
        assert_eq!(sx, 0.0);
        assert_eq!(sy, 0.0);
    }

    #[test]
    fn screen_to_sheet_scrolled() {
        let mut v = vp();
        v.set_scroll(100.0, 50.0, 10000.0, 10000.0);
        let (sx, sy) = v.screen_to_sheet(ROW_HEADER_WIDTH, COL_HEADER_HEIGHT);
        assert_eq!(sx, 100.0);
        assert_eq!(sy, 50.0);
    }

    #[test]
    fn sheet_to_screen_roundtrip() {
        let mut v = vp();
        v.set_scroll(200.0, 100.0, 10000.0, 10000.0);
        v.set_zoom(1.5);

        let sheet_pt = (350.0, 200.0);
        let screen_pt = v.sheet_to_screen(sheet_pt.0, sheet_pt.1);
        let back = v.screen_to_sheet(screen_pt.0, screen_pt.1);

        assert!((back.0 - sheet_pt.0).abs() < 1e-10);
        assert!((back.1 - sheet_pt.1).abs() < 1e-10);
    }

    #[test]
    fn scroll_clamped() {
        let mut v = vp();
        v.set_scroll(-100.0, -50.0, 1000.0, 500.0);
        assert_eq!(v.scroll_x(), 0.0);
        assert_eq!(v.scroll_y(), 0.0);
    }

    #[test]
    fn scroll_clamped_max() {
        let mut v = vp();
        // max_x=1000, visible_width=750/1.0=750, max_scroll_x=250
        v.set_scroll(9999.0, 9999.0, 1000.0, 800.0);
        assert_eq!(v.scroll_x(), 250.0);
        assert_eq!(v.scroll_y(), 224.0); // 800-576
    }

    #[test]
    fn scroll_by() {
        let mut v = vp();
        v.scroll_by(50.0, 30.0, 10000.0, 10000.0);
        assert_eq!(v.scroll_x(), 50.0);
        assert_eq!(v.scroll_y(), 30.0);
    }

    #[test]
    fn zoom_clamp() {
        let mut v = vp();
        v.set_zoom(0.01);
        assert_eq!(v.zoom(), MIN_ZOOM);
        v.set_zoom(10.0);
        assert_eq!(v.zoom(), MAX_ZOOM);
    }

    #[test]
    fn is_in_header_areas() {
        let v = vp();
        assert!(v.is_in_row_header(25.0, 30.0));       // in row header
        assert!(!v.is_in_row_header(55.0, 30.0));      // past row header
        assert!(v.is_in_col_header(100.0, 10.0));      // in col header
        assert!(!v.is_in_col_header(100.0, 30.0));     // below col header
        assert!(v.is_in_corner(25.0, 10.0));            // corner
        assert!(!v.is_in_corner(55.0, 10.0));           // past corner
    }

    #[test]
    fn ensure_visible_scrolls_right() {
        let mut v = vp();
        // Cell at sheet x=800 (beyond visible 750)
        let cell = CellRect {
            x: 800.0,
            y: 0.0,
            width: 100.0,
            height: 24.0,
        };
        v.ensure_visible(&cell, 10000.0, 10000.0);
        // Should scroll right so cell's right edge (900) aligns with
        // the visible right edge: scroll_x = 900 - 750 = 150
        assert_eq!(v.scroll_x(), 150.0);
    }

    #[test]
    fn ensure_visible_scrolls_left() {
        let mut v = vp();
        v.set_scroll(500.0, 0.0, 10000.0, 10000.0);
        let cell = CellRect {
            x: 200.0,
            y: 0.0,
            width: 100.0,
            height: 24.0,
        };
        v.ensure_visible(&cell, 10000.0, 10000.0);
        assert_eq!(v.scroll_x(), 200.0);
    }

    #[test]
    fn headers_disabled() {
        let mut v = vp();
        v.set_show_headers(false);
        assert_eq!(v.grid_origin_x(), 0.0);
        assert_eq!(v.grid_origin_y(), 0.0);
        assert_eq!(v.visible_width(), 800.0);
        assert_eq!(v.visible_height(), 600.0);
        assert!(!v.is_in_row_header(25.0, 30.0));
    }

    #[test]
    fn resize() {
        let mut v = vp();
        v.resize(1024.0, 768.0);
        assert_eq!(v.screen_width, 1024.0);
        assert_eq!(v.screen_height, 768.0);
    }

    #[test]
    fn sheet_rect_to_screen() {
        let mut v = vp();
        v.set_zoom(2.0);
        v.set_scroll(100.0, 50.0, 10000.0, 10000.0);
        let sheet_r = CellRect {
            x: 100.0,
            y: 50.0,
            width: 100.0,
            height: 24.0,
        };
        let screen_r = v.sheet_rect_to_screen(&sheet_r);
        // (100-100)*2+50=50, (50-50)*2+24=24
        assert_eq!(screen_r.x, ROW_HEADER_WIDTH);
        assert_eq!(screen_r.y, COL_HEADER_HEIGHT);
        assert_eq!(screen_r.width, 200.0);
        assert_eq!(screen_r.height, 48.0);
    }
}
