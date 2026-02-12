//! Application state — owns the full document→layout→render pipeline.
//!
//! `AppState` is the single source of truth for the running application.
//! It holds the CRDT document, layout engine, spatial index (via engine),
//! GPU renderer, camera, text engine, and interaction state (selection/hover).

use logos_core::{Document, Layer, RectLayer};
use logos_layout::engine::LayoutEngine;
use logos_render::vertex::{CameraUniform, RectInstance, TextInstance};
use logos_render::renderer::{FrameStats, Renderer};
use logos_render::context::GpuContext;
use logos_text::{Atlas, TextEngine, TextStyle};
use uuid::Uuid;

/// Interactive state for selection / hover.
#[derive(Debug, Clone, Default)]
pub struct InteractionState {
    /// Currently selected layer, if any.
    pub selected: Option<Uuid>,
    /// Currently hovered layer (under cursor), if any.
    pub hovered: Option<Uuid>,
    /// Last known cursor position in world coordinates.
    pub cursor_world: [f32; 2],
}

/// Camera state — tracks pan and zoom for the viewport.
#[derive(Debug, Clone)]
pub struct Camera {
    pub pan_x: f32,
    pub pan_y: f32,
    pub zoom: f32,
    pub viewport_width: f32,
    pub viewport_height: f32,
}

impl Camera {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            pan_x: 0.0,
            pan_y: 0.0,
            zoom: 1.0,
            viewport_width: width,
            viewport_height: height,
        }
    }

    /// Convert screen pixel coordinates to world coordinates.
    pub fn screen_to_world(&self, screen_x: f32, screen_y: f32) -> (f32, f32) {
        let world_x = screen_x / self.zoom + self.pan_x;
        let world_y = screen_y / self.zoom + self.pan_y;
        (world_x, world_y)
    }

    /// Build the GPU camera uniform from current state.
    pub fn uniform(&self) -> CameraUniform {
        CameraUniform::orthographic(
            self.viewport_width,
            self.viewport_height,
            self.pan_x,
            self.pan_y,
            self.zoom,
        )
    }

    /// Pan by delta pixels (in screen space).
    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.pan_x -= dx / self.zoom;
        self.pan_y -= dy / self.zoom;
    }

    /// Zoom toward/away from screen point (sx, sy).
    pub fn zoom_at(&mut self, sx: f32, sy: f32, factor: f32) {
        // World point under cursor before zoom.
        let (wx, wy) = self.screen_to_world(sx, sy);

        self.zoom *= factor;
        self.zoom = self.zoom.clamp(0.1, 50.0);

        // Adjust pan so the same world point stays under cursor.
        self.pan_x = wx - sx / self.zoom;
        self.pan_y = wy - sy / self.zoom;
    }

    /// Update viewport dimensions (on resize).
    pub fn resize(&mut self, width: f32, height: f32) {
        self.viewport_width = width;
        self.viewport_height = height;
    }
}

// ── Selection / hover colors ────────────────────────────────────────
const SELECTION_COLOR: [f32; 4] = [0.26, 0.52, 0.96, 0.3]; // blue overlay
const HOVER_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 0.08]; // subtle white overlay
const SELECTION_BORDER_COLOR: [f32; 4] = [0.26, 0.52, 0.96, 1.0]; // solid blue border
const BORDER_WIDTH: f32 = 2.0;

/// Atlas texture size for glyph caching.
const ATLAS_SIZE: u32 = 1024;

/// Owns the entire application pipeline.
pub struct AppState {
    pub document: Document,
    pub layout_engine: LayoutEngine,
    pub renderer: Renderer,
    pub gpu: GpuContext,
    pub camera: Camera,
    pub interaction: InteractionState,
    /// Text engine for shaping and rasterization.
    pub text_engine: TextEngine,
    /// Glyph atlas for GPU texture upload.
    pub atlas: Atlas,
    /// Cached rect instance buffer, rebuilt each frame.
    instances: Vec<RectInstance>,
    /// Cached text instance buffer, rebuilt each frame.
    text_instances: Vec<TextInstance>,
    /// Dirty flag — set when layers/layout/selection change.
    needs_redraw: bool,
}

impl AppState {
    /// Build a new AppState after GPU context has been created.
    pub fn new(gpu: GpuContext, width: u32, height: u32) -> Self {
        let renderer = Renderer::with_atlas_size(&gpu, ATLAS_SIZE);
        let document = Document::new();
        let layout_engine = LayoutEngine::new();
        let camera = Camera::new(width as f32, height as f32);
        let text_engine = TextEngine::new();
        let atlas = Atlas::new(ATLAS_SIZE);

        Self {
            document,
            layout_engine,
            renderer,
            gpu,
            camera,
            interaction: InteractionState::default(),
            text_engine,
            atlas,
            instances: Vec::new(),
            text_instances: Vec::new(),
            needs_redraw: true,
        }
    }

    /// Populate the document with a demo scene for initial bring-up.
    pub fn load_demo_scene(&mut self) {
        let layers = vec![
            // Background card
            Layer::Rect(RectLayer::new(60.0, 40.0, 680.0, 480.0)),
            // Header bar
            Layer::Rect(RectLayer::new(60.0, 40.0, 680.0, 56.0)),
            // Sidebar
            Layer::Rect(RectLayer::new(60.0, 96.0, 200.0, 424.0)),
            // Content area cards
            Layer::Rect(RectLayer::new(280.0, 116.0, 200.0, 160.0)),
            Layer::Rect(RectLayer::new(500.0, 116.0, 220.0, 160.0)),
            Layer::Rect(RectLayer::new(280.0, 296.0, 440.0, 100.0)),
            // Floating action button
            Layer::Rect(RectLayer::new(660.0, 460.0, 56.0, 56.0)),
            // Small badges
            Layer::Rect(RectLayer::new(80.0, 120.0, 160.0, 32.0)),
            Layer::Rect(RectLayer::new(80.0, 164.0, 160.0, 32.0)),
            Layer::Rect(RectLayer::new(80.0, 208.0, 160.0, 32.0)),
            Layer::Rect(RectLayer::new(80.0, 252.0, 160.0, 32.0)),
            Layer::Rect(RectLayer::new(80.0, 296.0, 160.0, 32.0)),
        ];

        for layer in &layers {
            self.document.add_layer(layer.clone()).unwrap();
            self.layout_engine.add_or_update_layer(layer).unwrap();
        }

        // Compute layout for each root-level layer.
        for layer in &layers {
            let _ = self.layout_engine.compute_layout(layer.id());
        }

        self.needs_redraw = true;
    }

    /// Rebuild the instance buffer from document + layout + interaction state.
    pub fn rebuild_instances(&mut self) {
        self.instances.clear();
        self.text_instances.clear();

        // Palette of distinct colors for visual variety.
        let palette: &[[f32; 4]] = &[
            [0.15, 0.15, 0.18, 1.0],  // dark card
            [0.26, 0.52, 0.96, 1.0],  // blue header
            [0.18, 0.18, 0.22, 1.0],  // sidebar
            [0.22, 0.22, 0.28, 1.0],  // content card 1
            [0.24, 0.24, 0.30, 1.0],  // content card 2
            [0.20, 0.20, 0.26, 1.0],  // wide card
            [0.96, 0.26, 0.42, 1.0],  // FAB (red)
            [0.22, 0.30, 0.38, 1.0],  // badge 1
            [0.22, 0.28, 0.36, 1.0],  // badge 2
            [0.22, 0.26, 0.34, 1.0],  // badge 3
            [0.22, 0.24, 0.32, 1.0],  // badge 4
            [0.22, 0.22, 0.30, 1.0],  // badge 5
        ];

        let page = self.document.root.read().unwrap();
        for (i, layer) in page.layers.iter().enumerate() {
            let id = layer.id();
            if let Some(layout) = self.layout_engine.get_layout(id) {
                let color = palette[i % palette.len()];
                let radius = match i {
                    6 => 28.0,           // FAB is circular
                    1 => 0.0,            // header sharp
                    _ if i >= 7 => 6.0,  // badges rounded
                    _ => 8.0,            // default rounded
                };

                let inst = RectInstance::new(
                    layout.location.x,
                    layout.location.y,
                    layout.size.width,
                    layout.size.height,
                    color,
                )
                .with_radius(radius)
                .with_z(i as f32);

                self.instances.push(inst);
            }
        }
        drop(page); // Release RwLock before mutable self borrow below.

        // Add hover overlay (if any).
        if let Some(hover_id) = self.interaction.hovered {
            if self.interaction.selected != Some(hover_id) {
                if let Some(layout) = self.layout_engine.get_layout(hover_id) {
                    let inst = RectInstance::new(
                        layout.location.x,
                        layout.location.y,
                        layout.size.width,
                        layout.size.height,
                        HOVER_COLOR,
                    )
                    .with_z(100.0);
                    self.instances.push(inst);
                }
            }
        }

        // Add selection overlay + border (if any).
        if let Some(sel_id) = self.interaction.selected {
            if let Some(layout) = self.layout_engine.get_layout(sel_id) {
                let x = layout.location.x;
                let y = layout.location.y;
                let w = layout.size.width;
                let h = layout.size.height;

                // Semi-transparent fill overlay.
                self.instances.push(
                    RectInstance::new(x, y, w, h, SELECTION_COLOR).with_z(101.0),
                );

                // Selection border (4 thin rects).
                let b = BORDER_WIDTH;
                // Top
                self.instances.push(
                    RectInstance::new(x - b, y - b, w + 2.0 * b, b, SELECTION_BORDER_COLOR)
                        .with_z(102.0),
                );
                // Bottom
                self.instances.push(
                    RectInstance::new(x - b, y + h, w + 2.0 * b, b, SELECTION_BORDER_COLOR)
                        .with_z(102.0),
                );
                // Left
                self.instances.push(
                    RectInstance::new(x - b, y, b, h, SELECTION_BORDER_COLOR)
                        .with_z(102.0),
                );
                // Right
                self.instances.push(
                    RectInstance::new(x + w, y, b, h, SELECTION_BORDER_COLOR)
                        .with_z(102.0),
                );
            }
        }

        // ── Text: "Hello Logos" centered on the canvas ──────────
        self.rebuild_text_instances();
    }

    /// Shape and generate text glyph instances.
    fn rebuild_text_instances(&mut self) {
        let style = TextStyle {
            font_size: 32.0,
            line_height: 38.0,
            color: [1.0, 1.0, 1.0, 1.0],
            family: "sans-serif".into(),
            weight: 700,
            italic: false,
        };

        let shaped = self.text_engine.shape_text(
            "Hello, Logos!",
            &style,
            f32::INFINITY,
            &mut self.atlas,
        );

        // Center the text on the canvas (below the demo scene).
        let center_x = 400.0 - shaped.width / 2.0;
        let center_y = 550.0;

        for glyph in &shaped.glyphs {
            self.text_instances.push(TextInstance::new(
                center_x + glyph.x,
                center_y + glyph.y,
                glyph.width,
                glyph.height,
                [glyph.atlas_region.u_min, glyph.atlas_region.v_min],
                [glyph.atlas_region.u_max, glyph.atlas_region.v_max],
                glyph.color,
            ));
        }

        // Second line: subtitle in lighter weight.
        let subtitle_style = TextStyle {
            font_size: 18.0,
            line_height: 22.0,
            color: [0.6, 0.6, 0.65, 1.0],
            family: "sans-serif".into(),
            weight: 400,
            italic: false,
        };

        let subtitle = self.text_engine.shape_text(
            "Design at the speed of thought",
            &subtitle_style,
            f32::INFINITY,
            &mut self.atlas,
        );

        let sub_x = 400.0 - subtitle.width / 2.0;
        let sub_y = center_y + 44.0;

        for glyph in &subtitle.glyphs {
            self.text_instances.push(TextInstance::new(
                sub_x + glyph.x,
                sub_y + glyph.y,
                glyph.width,
                glyph.height,
                [glyph.atlas_region.u_min, glyph.atlas_region.v_min],
                [glyph.atlas_region.u_max, glyph.atlas_region.v_max],
                glyph.color,
            ));
        }
    }

    /// Prepare and render a frame. Returns FrameStats.
    pub fn render_frame(&mut self) -> Result<FrameStats, logos_render::renderer::RenderError> {
        if self.needs_redraw {
            self.rebuild_instances();
            self.needs_redraw = false;
        }

        // Upload atlas texture if glyphs changed.
        if self.atlas.dirty {
            self.renderer.upload_atlas(&self.gpu, &self.atlas.data, self.atlas.size);
            self.atlas.dirty = false;
        }

        let camera = self.camera.uniform();
        self.renderer.prepare(&self.gpu, &self.instances, &camera);
        self.renderer.prepare_text(&self.gpu, &self.text_instances);
        self.renderer.render_to_surface(&self.gpu)
    }

    /// Handle window resize.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.gpu.resize(width, height);
        self.camera.resize(width as f32, height as f32);
        self.needs_redraw = true;
    }

    /// Request a redraw on next frame.
    pub fn request_redraw(&mut self) {
        self.needs_redraw = true;
    }

    /// Perform hit test at world coordinates and update hover state.
    pub fn update_hover(&mut self, screen_x: f32, screen_y: f32) {
        let (wx, wy) = self.camera.screen_to_world(screen_x, screen_y);
        self.interaction.cursor_world = [wx, wy];

        let hit = self.layout_engine.hit_test(wx, wy);
        if hit != self.interaction.hovered {
            self.interaction.hovered = hit;
            self.needs_redraw = true;
        }
    }

    /// Select whatever is under the cursor.
    pub fn select_at(&mut self, screen_x: f32, screen_y: f32) {
        let (wx, wy) = self.camera.screen_to_world(screen_x, screen_y);
        let hit = self.layout_engine.hit_test(wx, wy);

        if hit != self.interaction.selected {
            self.interaction.selected = hit;
            self.needs_redraw = true;
        }
    }

    /// Pan the camera by screen-space delta.
    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.camera.pan(dx, dy);
        self.needs_redraw = true;
    }

    /// Zoom toward screen point.
    pub fn zoom_at(&mut self, screen_x: f32, screen_y: f32, delta: f32) {
        let factor = if delta > 0.0 { 1.1 } else { 1.0 / 1.1 };
        self.camera.zoom_at(screen_x, screen_y, factor);
        self.needs_redraw = true;
    }
}

// ===================================================================
// Tests
// ===================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_camera_screen_to_world_identity() {
        let cam = Camera::new(800.0, 600.0);
        let (wx, wy) = cam.screen_to_world(400.0, 300.0);
        assert!((wx - 400.0).abs() < f32::EPSILON);
        assert!((wy - 300.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_camera_screen_to_world_zoomed() {
        let mut cam = Camera::new(800.0, 600.0);
        cam.zoom = 2.0;
        let (wx, wy) = cam.screen_to_world(400.0, 300.0);
        assert!((wx - 200.0).abs() < f32::EPSILON);
        assert!((wy - 150.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_camera_screen_to_world_panned() {
        let mut cam = Camera::new(800.0, 600.0);
        cam.pan_x = 100.0;
        cam.pan_y = 50.0;
        let (wx, wy) = cam.screen_to_world(0.0, 0.0);
        assert!((wx - 100.0).abs() < f32::EPSILON);
        assert!((wy - 50.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_camera_zoom_clamp() {
        let mut cam = Camera::new(800.0, 600.0);
        cam.zoom_at(400.0, 300.0, -100.0); // zoom out a lot
        assert!(cam.zoom >= 0.1);
        cam.zoom = 1.0;
        for _ in 0..200 {
            cam.zoom_at(400.0, 300.0, 1.0); // zoom in a lot
        }
        assert!(cam.zoom <= 50.0);
    }

    #[test]
    fn test_camera_pan() {
        let mut cam = Camera::new(800.0, 600.0);
        cam.pan(10.0, 20.0);
        assert!((cam.pan_x - (-10.0)).abs() < f32::EPSILON);
        assert!((cam.pan_y - (-20.0)).abs() < f32::EPSILON);
    }

    #[test]
    fn test_camera_resize() {
        let mut cam = Camera::new(800.0, 600.0);
        cam.resize(1920.0, 1080.0);
        assert!((cam.viewport_width - 1920.0).abs() < f32::EPSILON);
        assert!((cam.viewport_height - 1080.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_interaction_state_default() {
        let state = InteractionState::default();
        assert!(state.selected.is_none());
        assert!(state.hovered.is_none());
        assert_eq!(state.cursor_world, [0.0, 0.0]);
    }

    #[test]
    fn test_demo_scene_creates_instances() {
        let gpu = pollster::block_on(GpuContext::new_headless());
        if let Ok(gpu) = gpu {
            let mut app = AppState::new(gpu, 800, 600);
            app.load_demo_scene();
            app.rebuild_instances();
            assert_eq!(app.instances.len(), 12); // 12 rects in demo
        }
    }

    #[test]
    fn test_selection_adds_overlay_instances() {
        let gpu = pollster::block_on(GpuContext::new_headless());
        if let Ok(gpu) = gpu {
            let mut app = AppState::new(gpu, 800, 600);
            app.load_demo_scene();

            // Select first layer
            let page = app.document.root.read().unwrap();
            let first_id = page.layers[0].id();
            drop(page);

            app.interaction.selected = Some(first_id);
            app.rebuild_instances();
            // 12 scene + 1 fill overlay + 4 border rects = 17
            assert_eq!(app.instances.len(), 17);
        }
    }

    #[test]
    fn test_hover_adds_overlay() {
        let gpu = pollster::block_on(GpuContext::new_headless());
        if let Ok(gpu) = gpu {
            let mut app = AppState::new(gpu, 800, 600);
            app.load_demo_scene();

            let page = app.document.root.read().unwrap();
            let second_id = page.layers[1].id();
            drop(page);

            app.interaction.hovered = Some(second_id);
            app.rebuild_instances();
            // 12 scene + 1 hover overlay = 13
            assert_eq!(app.instances.len(), 13);
        }
    }

    #[test]
    fn test_text_instances_generated() {
        let gpu = pollster::block_on(GpuContext::new_headless());
        if let Ok(gpu) = gpu {
            let mut app = AppState::new(gpu, 800, 600);
            app.load_demo_scene();
            app.rebuild_instances();
            // Should have text glyphs from "Hello, Logos!" + subtitle.
            assert!(!app.text_instances.is_empty(), "Text instances should be generated");
            // "Hello, Logos!" has visible glyphs (space/comma may not produce quads).
            assert!(app.text_instances.len() >= 5, "Expected at least 5 glyph quads, got {}", app.text_instances.len());
        }
    }

    #[test]
    fn test_text_instances_have_valid_uvs() {
        let gpu = pollster::block_on(GpuContext::new_headless());
        if let Ok(gpu) = gpu {
            let mut app = AppState::new(gpu, 800, 600);
            app.load_demo_scene();
            app.rebuild_instances();
            for inst in &app.text_instances {
                assert!(inst.uv_min[0] >= 0.0 && inst.uv_min[0] <= 1.0, "u_min out of range");
                assert!(inst.uv_min[1] >= 0.0 && inst.uv_min[1] <= 1.0, "v_min out of range");
                assert!(inst.uv_max[0] > inst.uv_min[0], "u_max should be > u_min");
                assert!(inst.uv_max[1] > inst.uv_min[1], "v_max should be > v_min");
                assert!(inst.size[0] > 0.0, "glyph width should be > 0");
                assert!(inst.size[1] > 0.0, "glyph height should be > 0");
            }
        }
    }

    #[test]
    fn test_atlas_populated_after_rebuild() {
        let gpu = pollster::block_on(GpuContext::new_headless());
        if let Ok(gpu) = gpu {
            let mut app = AppState::new(gpu, 800, 600);
            app.load_demo_scene();
            app.rebuild_instances();
            assert!(app.atlas.glyph_count() > 0, "Atlas should have glyphs");
            assert!(app.atlas.dirty, "Atlas should be dirty after new glyphs");
        }
    }
}
