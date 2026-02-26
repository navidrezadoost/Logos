//! Render backend adapter — bridges [`DrawBatch`] to GPU-ready instance data.
//!
//! This module provides:
//! - [`RectData`] / [`TextCommand`] — GPU-compatible output structs that mirror
//!   the `RectInstance` / `TextInstance` layouts in `logos-render` (without any
//!   GPU dependencies).
//! - [`ViewportCamera`] — orthographic projection parameters matching
//!   `CameraUniform` in `logos-render`.
//! - [`SpreadsheetFrame`] — the complete per-frame output ready for GPU upload.
//! - [`InstanceBridge`] — orchestrates the full conversion pipeline:
//!   `RenderFrame → DrawBatch → SpreadsheetFrame`.
//! - [`RenderBackend`] trait — abstract interface for any rendering backend.
//!
//! # Architecture
//!
//! ```text
//! RenderFrame ──► BatchConverter ──► DrawBatch ──► InstanceBridge ──► SpreadsheetFrame
//!                      │                                │                   │
//!               SpreadsheetTheme                   ViewportCamera     Vec<RectData>
//!                                                                    Vec<TextCommand>
//! ```

use super::batch::DrawBatch;
use super::converter::BatchConverter;
use super::primitives::{DrawRect, DrawText, TextAlign, TextVAlign};
use super::theme::SpreadsheetTheme;
use crate::ui::render_data::RenderFrame;

// ---------------------------------------------------------------------------
// RectData — mirrors logos-render RectInstance (48 bytes)
// ---------------------------------------------------------------------------

/// GPU-ready rectangle instance data.
///
/// Field layout intentionally matches `RectInstance` in `logos-render`:
/// `[position: 2, size: 2, color: 4, border_radius: 1, z_index: 1, _pad: 2]`
/// so that a thin unsafe transmute or field-by-field copy is trivial.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct RectData {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
    pub border_radius: f32,
    pub z_index: f32,
    pub _pad: [f32; 2],
}

impl RectData {
    /// Create from a [`DrawRect`].
    pub fn from_draw_rect(r: &DrawRect) -> Self {
        Self {
            position: [r.x, r.y],
            size: [r.width, r.height],
            color: r.color,
            border_radius: r.border_radius,
            z_index: r.z_index,
            _pad: [0.0; 2],
        }
    }

    /// Size in bytes (should be 48, matching RectInstance).
    pub const SIZE: usize = std::mem::size_of::<Self>();
}

// ---------------------------------------------------------------------------
// TextCommand — high-level text draw command
// ---------------------------------------------------------------------------

/// A text draw command ready for the text rendering subsystem.
///
/// Unlike `TextInstance` (which is per-glyph UV-mapped quads), this is a
/// high-level command: "draw this string at this position with these styles."
/// The rendering backend is responsible for glyph layout, atlas lookup, and
/// generating per-glyph `TextInstance` quads.
#[derive(Debug, Clone, PartialEq)]
pub struct TextCommand {
    pub x: f32,
    pub y: f32,
    pub max_width: f32,
    pub max_height: f32,
    pub text: String,
    pub font_size: f32,
    pub color: [f32; 4],
    pub align: TextAlign,
    pub v_align: TextVAlign,
    pub bold: bool,
    pub italic: bool,
    pub clip_rect: Option<[f32; 4]>,
}

impl TextCommand {
    /// Create from a [`DrawText`].
    pub fn from_draw_text(t: &DrawText) -> Self {
        Self {
            x: t.x,
            y: t.y,
            max_width: t.max_width,
            max_height: t.max_height,
            text: t.text.clone(),
            font_size: t.font_size,
            color: t.color,
            align: t.align,
            v_align: t.v_align,
            bold: t.bold,
            italic: t.italic,
            clip_rect: t.clip_rect,
        }
    }
}

// ---------------------------------------------------------------------------
// ViewportCamera — mirrors logos-render CameraUniform
// ---------------------------------------------------------------------------

/// Orthographic camera for the spreadsheet viewport.
///
/// Generates a 4×4 projection matrix matching `CameraUniform::orthographic()`
/// in `logos-render`: top-left origin, Y-down, NDC range `[-1, 1]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportCamera {
    pub width: f32,
    pub height: f32,
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub zoom: f32,
}

impl ViewportCamera {
    /// Create a camera for the given viewport dimensions.
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            scroll_x: 0.0,
            scroll_y: 0.0,
            zoom: 1.0,
        }
    }

    /// Create from a `Viewport` (the UI module's viewport state).
    pub fn from_viewport(vp: &crate::ui::viewport::Viewport) -> Self {
        Self {
            width: vp.screen_width as f32,
            height: vp.screen_height as f32,
            scroll_x: vp.scroll_x() as f32,
            scroll_y: vp.scroll_y() as f32,
            zoom: vp.zoom() as f32,
        }
    }

    /// Set scroll offsets.
    pub fn with_scroll(mut self, x: f32, y: f32) -> Self {
        self.scroll_x = x;
        self.scroll_y = y;
        self
    }

    /// Set zoom level.
    pub fn with_zoom(mut self, zoom: f32) -> Self {
        self.zoom = zoom;
        self
    }

    /// Compute a 4×4 orthographic projection matrix.
    ///
    /// This matches `CameraUniform::orthographic()` in `logos-render`:
    /// - Top-left origin, Y increases downward
    /// - NDC output range: x ∈ [-1, 1], y ∈ [-1, 1]
    /// - Incorporates scroll and zoom
    ///
    /// The resulting matrix can be uploaded directly to a GPU uniform buffer.
    pub fn to_matrix(&self) -> [[f32; 4]; 4] {
        let z = self.zoom;
        let sx = 2.0 * z / self.width;
        let sy = -2.0 * z / self.height; // Y-down
        let tx = -1.0 - 2.0 * self.scroll_x * z / self.width;
        let ty = 1.0 + 2.0 * self.scroll_y * z / self.height;

        [
            [sx, 0.0, 0.0, 0.0],
            [0.0, sy, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [tx, ty, 0.0, 1.0],
        ]
    }

    /// Identity camera (no scroll, no zoom) for given dimensions.
    pub fn identity(width: f32, height: f32) -> Self {
        Self::new(width, height)
    }
}

impl Default for ViewportCamera {
    fn default() -> Self {
        Self::new(1920.0, 1080.0)
    }
}

// ---------------------------------------------------------------------------
// SpreadsheetFrame — complete per-frame output
// ---------------------------------------------------------------------------

/// A complete frame of spreadsheet draw data ready for GPU upload.
///
/// This is the final output of the rendering pipeline. A rendering backend
/// consumes this to issue draw calls:
///
/// 1. Upload `rects` to the rect instance buffer
/// 2. Process `texts` through glyph layout → generate `TextInstance` quads
/// 3. Upload `camera` to the camera uniform buffer
/// 4. Issue draw calls
#[derive(Debug, Clone)]
pub struct SpreadsheetFrame {
    /// Rectangle instances (cell backgrounds, grid lines, selection, headers).
    /// Sorted by z-index (ascending) for correct layering.
    pub rects: Vec<RectData>,

    /// High-level text commands. The backend must perform glyph layout
    /// and generate per-glyph `TextInstance` quads from these.
    pub texts: Vec<TextCommand>,

    /// Camera projection for this frame.
    pub camera: ViewportCamera,

    /// Frame statistics for performance monitoring.
    pub stats: FrameRenderStats,
}

impl SpreadsheetFrame {
    /// Total rect instances to upload.
    pub fn rect_count(&self) -> usize {
        self.rects.len()
    }

    /// Total text commands to process.
    pub fn text_count(&self) -> usize {
        self.texts.len()
    }

    /// Estimated GPU buffer size for rect instances (in bytes).
    pub fn rect_buffer_size(&self) -> usize {
        self.rects.len() * RectData::SIZE
    }

    /// Whether the frame has anything to draw.
    pub fn is_empty(&self) -> bool {
        self.rects.is_empty() && self.texts.is_empty()
    }
}

// ---------------------------------------------------------------------------
// FrameRenderStats
// ---------------------------------------------------------------------------

/// Performance statistics for a rendered frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameRenderStats {
    pub cell_count: usize,
    pub grid_line_count: usize,
    pub header_count: usize,
    pub rect_instances: usize,
    pub text_commands: usize,
    pub has_selection: bool,
    pub has_active_cell: bool,
}

impl FrameRenderStats {
    /// Total draw primitives.
    pub fn total_primitives(&self) -> usize {
        self.rect_instances + self.text_commands
    }
}

// ---------------------------------------------------------------------------
// RenderBackend trait
// ---------------------------------------------------------------------------

/// Abstract interface for rendering backends.
///
/// Implementations handle the actual GPU upload and draw calls.
/// This trait allows the spreadsheet to be rendered by different backends
/// (wgpu, Skia, Canvas2D, SVG, test stub) without any coupling.
pub trait RenderBackend {
    /// Submit a complete frame for rendering.
    fn submit_frame(&mut self, frame: &SpreadsheetFrame);

    /// Whether the backend supports partial updates (dirty-slot uploads).
    fn supports_partial_update(&self) -> bool {
        false
    }

    /// Submit only changed rect instances at specific buffer slots.
    /// Default is no-op; override for GPU backends that support partial upload.
    fn submit_partial_rects(&mut self, _updates: &[(usize, RectData)]) {
        // Default: no-op, backends override if they support partial updates
    }
}

// ---------------------------------------------------------------------------
// InstanceBridge — the full pipeline
// ---------------------------------------------------------------------------

/// Orchestrates the complete rendering pipeline:
/// `RenderFrame → DrawBatch → SpreadsheetFrame`.
///
/// Holds a [`BatchConverter`] (with theme) and produces GPU-ready output.
#[derive(Debug, Clone)]
pub struct InstanceBridge {
    converter: BatchConverter,
}

impl InstanceBridge {
    /// Create with the given theme.
    pub fn new(theme: SpreadsheetTheme) -> Self {
        Self {
            converter: BatchConverter::new(theme),
        }
    }

    /// Create with the default light theme.
    pub fn light() -> Self {
        Self::new(SpreadsheetTheme::light())
    }

    /// Create with the dark theme.
    pub fn dark() -> Self {
        Self::new(SpreadsheetTheme::dark())
    }

    /// Get a reference to the inner converter.
    pub fn converter(&self) -> &BatchConverter {
        &self.converter
    }

    /// Set a new theme.
    pub fn set_theme(&mut self, theme: SpreadsheetTheme) {
        self.converter.set_theme(theme);
    }

    /// Convert a `RenderFrame` into a GPU-ready `SpreadsheetFrame`.
    ///
    /// This is the main entry point — call once per frame.
    pub fn prepare_frame(
        &self,
        render_frame: &RenderFrame,
        camera: ViewportCamera,
    ) -> SpreadsheetFrame {
        // Step 1: Convert to draw primitives
        let batch = self.converter.convert(render_frame);

        // Step 2: Convert to GPU-ready instances
        self.batch_to_frame(&batch, camera)
    }

    /// Convert a pre-built `DrawBatch` into a `SpreadsheetFrame`.
    pub fn batch_to_frame(&self, batch: &DrawBatch, camera: ViewportCamera) -> SpreadsheetFrame {
        let stats_raw = batch.stats();

        // Collect all rects sorted by z-index
        let mut rects: Vec<RectData> = Vec::with_capacity(batch.rect_count());

        // Cell backgrounds
        for r in &batch.cell_backgrounds {
            rects.push(RectData::from_draw_rect(r));
        }

        // Grid lines (already converted to thin rects by the converter)
        for r in &batch.grid_lines {
            rects.push(RectData::from_draw_rect(r));
        }

        // Header backgrounds
        for r in &batch.header_backgrounds {
            rects.push(RectData::from_draw_rect(r));
        }

        // Selection fill
        if let Some(ref r) = batch.selection_fill {
            rects.push(RectData::from_draw_rect(r));
        }

        // Selection border → expand to 4 thin rects
        if let Some(ref border) = batch.selection_border {
            for r in border.to_rects() {
                rects.push(RectData::from_draw_rect(&r));
            }
        }

        // Active cell border → expand to 4 thin rects
        if let Some(ref border) = batch.active_cell_border {
            for r in border.to_rects() {
                rects.push(RectData::from_draw_rect(&r));
            }
        }

        // Corner rect
        if let Some(ref r) = batch.corner_rect {
            rects.push(RectData::from_draw_rect(r));
        }

        // Sort by z-index for correct draw order
        rects.sort_by(|a, b| a.z_index.partial_cmp(&b.z_index).unwrap_or(std::cmp::Ordering::Equal));

        // Collect all text commands
        let mut texts: Vec<TextCommand> = Vec::with_capacity(batch.text_count());

        for t in &batch.cell_texts {
            texts.push(TextCommand::from_draw_text(t));
        }
        for t in &batch.header_texts {
            texts.push(TextCommand::from_draw_text(t));
        }

        let stats = FrameRenderStats {
            cell_count: stats_raw.cell_bg_count,
            grid_line_count: stats_raw.grid_line_count,
            header_count: stats_raw.header_bg_count,
            rect_instances: rects.len(),
            text_commands: texts.len(),
            has_selection: stats_raw.has_selection,
            has_active_cell: stats_raw.has_active_cell,
        };

        SpreadsheetFrame {
            rects,
            texts,
            camera,
            stats,
        }
    }
}

impl Default for InstanceBridge {
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
    use crate::ui::grid::CellRect;
    use crate::ui::render_data::*;

    // -- Helpers --

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

    fn make_header(text: &str, x: f64, y: f64, w: f64, h: f64) -> HeaderRenderData {
        HeaderRenderData {
            screen_rect: CellRect { x, y, width: w, height: h },
            text: text.to_string(),
            selected: false,
        }
    }

    fn make_line(x1: f64, y1: f64, x2: f64, y2: f64) -> GridLine {
        GridLine { x1, y1, x2, y2 }
    }

    fn sample_frame() -> RenderFrame {
        RenderFrame {
            cells: vec![make_cell(0, 0, "Hello"), make_cell(1, 0, "42")],
            h_lines: vec![make_line(50.0, 24.0, 800.0, 24.0)],
            v_lines: vec![make_line(50.0, 24.0, 50.0, 600.0)],
            col_headers: vec![make_header("A", 50.0, 0.0, 100.0, 24.0)],
            row_headers: vec![make_header("1", 0.0, 24.0, 50.0, 24.0)],
            selection_rect: Some(CellRect {
                x: 50.0, y: 24.0, width: 200.0, height: 24.0,
            }),
            active_cell_rect: Some(CellRect {
                x: 50.0, y: 24.0, width: 100.0, height: 24.0,
            }),
            formula_bar: None,
        }
    }

    // -- RectData --

    #[test]
    fn rect_data_size_is_48_bytes() {
        assert_eq!(RectData::SIZE, 48);
    }

    #[test]
    fn rect_data_from_draw_rect() {
        let dr = DrawRect::new(10.0, 20.0, 100.0, 24.0, [1.0, 0.0, 0.0, 1.0])
            .with_radius(4.0)
            .with_z(2.5);
        let rd = RectData::from_draw_rect(&dr);

        assert_eq!(rd.position, [10.0, 20.0]);
        assert_eq!(rd.size, [100.0, 24.0]);
        assert_eq!(rd.color, [1.0, 0.0, 0.0, 1.0]);
        assert!((rd.border_radius - 4.0).abs() < f32::EPSILON);
        assert!((rd.z_index - 2.5).abs() < f32::EPSILON);
        assert_eq!(rd._pad, [0.0; 2]);
    }

    // -- TextCommand --

    #[test]
    fn text_command_from_draw_text() {
        let dt = DrawText::new(5.0, 10.0, "Hello", [0.0, 0.0, 0.0, 1.0])
            .with_size(14.0)
            .with_align(TextAlign::Right)
            .with_v_align(TextVAlign::Bottom)
            .with_bold(true)
            .with_bounds(90.0, 20.0);

        let tc = TextCommand::from_draw_text(&dt);

        assert_eq!(tc.text, "Hello");
        assert!((tc.font_size - 14.0).abs() < f32::EPSILON);
        assert_eq!(tc.align, TextAlign::Right);
        assert_eq!(tc.v_align, TextVAlign::Bottom);
        assert!(tc.bold);
        assert!(!tc.italic);
        assert!((tc.max_width - 90.0).abs() < f32::EPSILON);
    }

    // -- ViewportCamera --

    #[test]
    fn camera_identity() {
        let cam = ViewportCamera::identity(800.0, 600.0);
        assert!((cam.width - 800.0).abs() < f32::EPSILON);
        assert!((cam.scroll_x).abs() < f32::EPSILON);
        assert!((cam.zoom - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn camera_with_scroll_and_zoom() {
        let cam = ViewportCamera::new(1920.0, 1080.0)
            .with_scroll(100.0, 200.0)
            .with_zoom(1.5);

        assert!((cam.scroll_x - 100.0).abs() < f32::EPSILON);
        assert!((cam.scroll_y - 200.0).abs() < f32::EPSILON);
        assert!((cam.zoom - 1.5).abs() < f32::EPSILON);
    }

    #[test]
    fn camera_matrix_identity_corners() {
        // With no scroll and zoom=1, top-left (0,0) → NDC (-1, 1)
        let cam = ViewportCamera::identity(800.0, 600.0);
        let m = cam.to_matrix();

        // Transform (0, 0)
        let x = m[0][0] * 0.0 + m[3][0]; // sx*0 + tx = -1
        let y = m[1][1] * 0.0 + m[3][1]; // sy*0 + ty = +1

        assert!((x - (-1.0)).abs() < 1e-5, "top-left x should be -1, got {x}");
        assert!((y - 1.0).abs() < 1e-5, "top-left y should be +1, got {y}");

        // Transform (800, 600)
        let x2 = m[0][0] * 800.0 + m[3][0]; // sx*800 + tx = +1
        let y2 = m[1][1] * 600.0 + m[3][1]; // sy*600 + ty = -1

        assert!((x2 - 1.0).abs() < 1e-5, "bottom-right x should be +1, got {x2}");
        assert!((y2 - (-1.0)).abs() < 1e-5, "bottom-right y should be -1, got {y2}");
    }

    #[test]
    fn camera_matrix_with_zoom() {
        let cam = ViewportCamera::new(800.0, 600.0).with_zoom(2.0);
        let m = cam.to_matrix();

        // With zoom=2, (0,0) still maps to (-1, 1) since scroll is 0
        let x = m[0][0] * 0.0 + m[3][0];
        let y = m[1][1] * 0.0 + m[3][1];
        assert!((x - (-1.0)).abs() < 1e-5);
        assert!((y - 1.0).abs() < 1e-5);

        // (400, 300) should map to (1, -1) with zoom 2 (half viewport shows)
        let x2 = m[0][0] * 400.0 + m[3][0];
        let y2 = m[1][1] * 300.0 + m[3][1];
        assert!((x2 - 1.0).abs() < 1e-5, "half-viewport x should be +1 at zoom 2, got {x2}");
        assert!((y2 - (-1.0)).abs() < 1e-5, "half-viewport y should be -1 at zoom 2, got {y2}");
    }

    #[test]
    fn camera_matrix_with_scroll() {
        let cam = ViewportCamera::new(800.0, 600.0).with_scroll(100.0, 50.0);
        let m = cam.to_matrix();

        // (100, 50) — the scroll offset — should map to (-1, 1)
        let x = m[0][0] * 100.0 + m[3][0];
        let y = m[1][1] * 50.0 + m[3][1];
        assert!((x - (-1.0)).abs() < 1e-5);
        assert!((y - 1.0).abs() < 1e-5);
    }

    // -- InstanceBridge --

    #[test]
    fn bridge_empty_frame() {
        let bridge = InstanceBridge::light();
        let frame = bridge.prepare_frame(&RenderFrame::empty(), ViewportCamera::default());
        assert!(frame.is_empty());
        assert_eq!(frame.stats.rect_instances, 0);
        assert_eq!(frame.stats.text_commands, 0);
    }

    #[test]
    fn bridge_sample_frame_rect_count() {
        let bridge = InstanceBridge::light();
        let rf = sample_frame();
        let frame = bridge.prepare_frame(&rf, ViewportCamera::new(1920.0, 1080.0));

        // Expected rects:
        // 2 cell bg + 2 grid lines + 2 header bg + 1 corner
        // + 1 selection fill + 4 selection border edges
        // + 4 active cell border edges
        // = 2 + 2 + 2 + 1 + 1 + 4 + 4 = 16
        assert_eq!(frame.stats.cell_count, 2);
        assert_eq!(frame.stats.grid_line_count, 2);
        assert_eq!(frame.stats.header_count, 2);
        assert!(frame.stats.has_selection);
        assert!(frame.stats.has_active_cell);

        // Rect count includes all the above
        assert!(frame.rect_count() > 0);
        assert!(!frame.is_empty());
    }

    #[test]
    fn bridge_sample_frame_text_count() {
        let bridge = InstanceBridge::light();
        let rf = sample_frame();
        let frame = bridge.prepare_frame(&rf, ViewportCamera::new(1920.0, 1080.0));

        // 2 cell texts + 2 header texts = 4
        assert_eq!(frame.text_count(), 4);
    }

    #[test]
    fn bridge_rects_sorted_by_z() {
        let bridge = InstanceBridge::light();
        let rf = sample_frame();
        let frame = bridge.prepare_frame(&rf, ViewportCamera::new(1920.0, 1080.0));

        // Verify z-index is non-decreasing
        for w in frame.rects.windows(2) {
            assert!(
                w[0].z_index <= w[1].z_index,
                "Rects should be sorted by z_index: {} <= {}",
                w[0].z_index,
                w[1].z_index,
            );
        }
    }

    #[test]
    fn bridge_rect_buffer_size() {
        let bridge = InstanceBridge::light();
        let rf = sample_frame();
        let frame = bridge.prepare_frame(&rf, ViewportCamera::new(1920.0, 1080.0));

        assert_eq!(frame.rect_buffer_size(), frame.rect_count() * 48);
    }

    #[test]
    fn bridge_dark_theme() {
        let bridge = InstanceBridge::dark();
        let rf = RenderFrame {
            cells: vec![make_cell(0, 0, "Dark")],
            ..RenderFrame::empty()
        };
        let frame = bridge.prepare_frame(&rf, ViewportCamera::default());

        // Cell background should be dark
        let bg = &frame.rects[0];
        assert!(bg.color[0] < 0.2, "dark theme bg should be dark");
    }

    #[test]
    fn bridge_theme_switch() {
        let mut bridge = InstanceBridge::light();
        let rf = RenderFrame {
            cells: vec![make_cell(0, 0, "Test")],
            ..RenderFrame::empty()
        };

        let light_frame = bridge.prepare_frame(&rf, ViewportCamera::default());
        let light_bg = light_frame.rects[0].color;

        bridge.set_theme(SpreadsheetTheme::dark());
        let dark_frame = bridge.prepare_frame(&rf, ViewportCamera::default());
        let dark_bg = dark_frame.rects[0].color;

        // Light and dark backgrounds should differ
        assert_ne!(light_bg, dark_bg);
    }

    // -- FrameRenderStats --

    #[test]
    fn stats_total_primitives() {
        let stats = FrameRenderStats {
            cell_count: 10,
            grid_line_count: 20,
            header_count: 5,
            rect_instances: 35,
            text_commands: 15,
            has_selection: true,
            has_active_cell: true,
        };
        assert_eq!(stats.total_primitives(), 50);
    }

    // -- RenderBackend trait --

    struct MockBackend {
        frames_submitted: usize,
        last_rect_count: usize,
        last_text_count: usize,
    }

    impl MockBackend {
        fn new() -> Self {
            Self {
                frames_submitted: 0,
                last_rect_count: 0,
                last_text_count: 0,
            }
        }
    }

    impl RenderBackend for MockBackend {
        fn submit_frame(&mut self, frame: &SpreadsheetFrame) {
            self.frames_submitted += 1;
            self.last_rect_count = frame.rect_count();
            self.last_text_count = frame.text_count();
        }
    }

    #[test]
    fn mock_backend_receives_frame() {
        let bridge = InstanceBridge::light();
        let rf = sample_frame();
        let frame = bridge.prepare_frame(&rf, ViewportCamera::default());

        let mut backend = MockBackend::new();
        backend.submit_frame(&frame);

        assert_eq!(backend.frames_submitted, 1);
        assert!(backend.last_rect_count > 0);
        assert!(backend.last_text_count > 0);
    }

    // -- Edge cases --

    #[test]
    fn border_expands_to_4_rects() {
        let bridge = InstanceBridge::light();
        let rf = RenderFrame {
            cells: vec![],
            h_lines: vec![],
            v_lines: vec![],
            col_headers: vec![],
            row_headers: vec![],
            selection_rect: None,
            active_cell_rect: Some(CellRect {
                x: 50.0, y: 24.0, width: 100.0, height: 24.0,
            }),
            formula_bar: None,
        };

        let frame = bridge.prepare_frame(&rf, ViewportCamera::default());
        // Active cell border: 4 thin rects
        assert_eq!(frame.rect_count(), 4);
    }

    #[test]
    fn selection_produces_5_rects() {
        let bridge = InstanceBridge::light();
        let rf = RenderFrame {
            cells: vec![],
            h_lines: vec![],
            v_lines: vec![],
            col_headers: vec![],
            row_headers: vec![],
            selection_rect: Some(CellRect {
                x: 50.0, y: 24.0, width: 200.0, height: 48.0,
            }),
            active_cell_rect: None,
            formula_bar: None,
        };

        let frame = bridge.prepare_frame(&rf, ViewportCamera::default());
        // Selection: 1 fill + 4 border rects = 5
        assert_eq!(frame.rect_count(), 5);
    }
}
