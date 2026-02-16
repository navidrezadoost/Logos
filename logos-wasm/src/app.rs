//! Main application facade — bridges JavaScript to the Logos engine.
//!
//! `LogosApp` owns the complete Document → Layout → Render pipeline
//! and exposes a flat, JS-friendly API via `#[wasm_bindgen]`.
//!
//! All engine optimizations carry over unchanged:
//! - 308 ns layout computation (subtree-only, FxHashMap)
//! - 131 ns partial GPU upload prep (dirty-slot tracking)
//! - 3 ns steady-state frame (retained instance buffer)

use wasm_bindgen::prelude::*;
use uuid::Uuid;

use logos_core::{Document, Layer, RectLayer};
use logos_render::bridge::collect_instances_fast;
use logos_render::frame_cache::FrameCache;
use logos_render::{GpuContext, Renderer, RectInstance};
use logos_layout::engine::LayoutEngine;

use crate::camera::Camera;
use crate::console_log;

// ── LogosApp ──────────────────────────────────────────────────────

/// The Logos design engine, fully self-contained for WebGPU rendering.
///
/// JavaScript creates an instance from a `<canvas>` element, then
/// calls methods on it each frame.
///
/// ```javascript
/// const app = await new LogosApp(canvas);
/// app.load_demo_scene(100);
/// function frame() { app.render_frame(); requestAnimationFrame(frame); }
/// requestAnimationFrame(frame);
/// ```
#[wasm_bindgen]
pub struct LogosApp {
    gpu: GpuContext,
    renderer: Renderer,
    document: Document,
    layout_engine: LayoutEngine,
    camera: Camera,
    #[allow(dead_code)]
    frame_cache: FrameCache,
    instances: Vec<RectInstance>,
    needs_redraw: bool,
    /// Parallel arrays: one UUID + one color per layer.
    layer_ids: Vec<Uuid>,
    layer_colors: Vec<[f32; 4]>,
}

// ── Constructor ───────────────────────────────────────────────────

#[wasm_bindgen]
impl LogosApp {
    /// Create a new Logos app attached to an HTML `<canvas>` element.
    ///
    /// This is `async` because WebGPU adapter/device requests are async.
    /// In JavaScript, use: `const app = await LogosApp.create(canvas);`
    pub async fn create(canvas: web_sys::HtmlCanvasElement) -> Result<LogosApp, JsValue> {
        let width = canvas.client_width().max(1) as u32;
        let height = canvas.client_height().max(1) as u32;

        console_log!("Logos: initializing WebGPU surface ({width}×{height})");

        let surface_target = wgpu::SurfaceTarget::Canvas(canvas);
        let gpu = GpuContext::new_with_surface(surface_target, width, height)
            .await
            .map_err(|e| JsValue::from_str(&format!("GPU init failed: {e:?}")))?;

        console_log!("Logos: WebGPU ready — format {:?}", gpu.surface_format);

        let renderer = Renderer::new(&gpu);
        let document = Document::new();
        let layout_engine = LayoutEngine::new();
        let camera = Camera::new(width as f32, height as f32);
        let frame_cache = FrameCache::new();

        Ok(LogosApp {
            gpu,
            renderer,
            document,
            layout_engine,
            camera,
            frame_cache,
            instances: Vec::new(),
            needs_redraw: true,
            layer_ids: Vec::new(),
            layer_colors: Vec::new(),
        })
    }
}

// ── Layer Operations ──────────────────────────────────────────────

#[wasm_bindgen]
impl LogosApp {
    /// Add a rectangle layer. Returns its UUID string.
    ///
    /// ```javascript
    /// const id = app.add_rect(0, 0, 100, 100, 0.94, 0.35, 0.35, 1.0);
    /// ```
    pub fn add_rect(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        r: f32,
        g: f32,
        b: f32,
        a: f32,
    ) -> Result<String, JsValue> {
        let layer = Layer::Rect(RectLayer::new(x, y, width, height));
        let id = layer.id();

        self.layout_engine
            .add_or_update_layer(&layer)
            .map_err(|e| JsValue::from_str(&format!("{e:?}")))?;
        self.document
            .add_layer(layer)
            .map_err(|e| JsValue::from_str(&e))?;
        self.layout_engine
            .compute_layout(id)
            .map_err(|e| JsValue::from_str(&format!("{e:?}")))?;

        self.layer_ids.push(id);
        self.layer_colors.push([r, g, b, a]);
        self.needs_redraw = true;

        Ok(id.to_string())
    }

    /// Remove a layer by UUID string.
    pub fn remove_layer(&mut self, id_str: &str) -> Result<(), JsValue> {
        let id = parse_uuid(id_str)?;

        self.document
            .remove_layer(id)
            .map_err(|e| JsValue::from_str(&e))?;
        let _ = self.layout_engine.remove_layer(id);

        if let Some(pos) = self.layer_ids.iter().position(|&lid| lid == id) {
            self.layer_ids.remove(pos);
            self.layer_colors.remove(pos);
        }
        self.needs_redraw = true;

        Ok(())
    }

    /// Move a layer to a new position (world-space).
    pub fn move_layer(&mut self, id_str: &str, x: f32, y: f32) -> Result<(), JsValue> {
        let id = parse_uuid(id_str)?;

        self.layout_engine
            .update_position(id, logos_layout::bridge::PosAxis::Left, x)
            .map_err(|e| JsValue::from_str(&format!("{e:?}")))?;
        self.layout_engine
            .update_position(id, logos_layout::bridge::PosAxis::Top, y)
            .map_err(|e| JsValue::from_str(&format!("{e:?}")))?;
        self.layout_engine
            .compute_layout(id)
            .map_err(|e| JsValue::from_str(&format!("{e:?}")))?;

        self.needs_redraw = true;
        Ok(())
    }

    /// Resize a layer (world-space dimensions).
    pub fn resize_layer(
        &mut self,
        id_str: &str,
        width: f32,
        height: f32,
    ) -> Result<(), JsValue> {
        let id = parse_uuid(id_str)?;

        self.layout_engine
            .update_dimension(id, logos_layout::bridge::DimAxis::Width, width)
            .map_err(|e| JsValue::from_str(&format!("{e:?}")))?;
        self.layout_engine
            .update_dimension(id, logos_layout::bridge::DimAxis::Height, height)
            .map_err(|e| JsValue::from_str(&format!("{e:?}")))?;
        self.layout_engine
            .compute_layout(id)
            .map_err(|e| JsValue::from_str(&format!("{e:?}")))?;

        self.needs_redraw = true;
        Ok(())
    }

    /// Get the number of layers in the scene.
    pub fn layer_count(&self) -> usize {
        self.layer_ids.len()
    }

    /// Load a demo scene with N rectangles arranged in a grid.
    pub fn load_demo_scene(&mut self, count: u32) -> Result<(), JsValue> {
        let cols = (count as f32).sqrt().ceil() as u32;
        let spacing = 120.0_f32;
        let size = 100.0_f32;

        let palette: [[f32; 4]; 6] = [
            [0.94, 0.35, 0.35, 1.0], // red
            [0.35, 0.67, 0.94, 1.0], // blue
            [0.47, 0.87, 0.47, 1.0], // green
            [0.95, 0.77, 0.32, 1.0], // yellow
            [0.73, 0.47, 0.95, 1.0], // purple
            [0.95, 0.58, 0.32, 1.0], // orange
        ];

        for i in 0..count {
            let col = i % cols;
            let row = i / cols;
            let x = col as f32 * spacing;
            let y = row as f32 * spacing;
            let c = palette[(i as usize) % palette.len()];
            self.add_rect(x, y, size, size, c[0], c[1], c[2], c[3])?;
        }

        console_log!("Logos: loaded demo scene — {count} rectangles");
        Ok(())
    }
}

// ── Interaction ───────────────────────────────────────────────────

#[wasm_bindgen]
impl LogosApp {
    /// Hit-test at screen coordinates. Returns UUID string or `undefined`.
    pub fn hit_test(&self, screen_x: f32, screen_y: f32) -> Option<String> {
        let (wx, wy) = self.camera.screen_to_world(screen_x, screen_y);
        self.layout_engine.hit_test(wx, wy).map(|id| id.to_string())
    }

    /// Select the layer at screen coordinates.
    /// Returns UUID string of selected layer, or `undefined`.
    pub fn select_at(&mut self, screen_x: f32, screen_y: f32) -> Option<String> {
        let (wx, wy) = self.camera.screen_to_world(screen_x, screen_y);
        if let Some(id) = self.layout_engine.hit_test(wx, wy) {
            let _ = self.document.set_selection(vec![id]);
            Some(id.to_string())
        } else {
            let _ = self.document.clear_selection();
            None
        }
    }

    /// Clear the current selection.
    pub fn clear_selection(&mut self) {
        let _ = self.document.clear_selection();
    }

    /// Get the current selection as a JavaScript array of UUID strings.
    pub fn selection(&self) -> Result<JsValue, JsValue> {
        let sel = self.document.get_selection().map_err(|e| JsValue::from_str(&e))?;
        let arr = js_sys::Array::new();
        for id in &sel {
            arr.push(&JsValue::from_str(&id.to_string()));
        }
        Ok(arr.into())
    }
}

// ── Camera / Viewport ─────────────────────────────────────────────

#[wasm_bindgen]
impl LogosApp {
    /// Pan the camera by screen-space pixels.
    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.camera.pan(dx, dy);
        self.needs_redraw = true;
    }

    /// Zoom at a screen-space point.
    /// `factor > 1.0` zooms in, `factor < 1.0` zooms out.
    pub fn zoom_at(&mut self, screen_x: f32, screen_y: f32, factor: f32) {
        self.camera.zoom_at(screen_x, screen_y, factor);
        self.needs_redraw = true;
    }

    /// Resize the viewport (call on window resize).
    pub fn resize(&mut self, width: u32, height: u32) {
        let w = width.max(1);
        let h = height.max(1);
        self.gpu.resize(w, h);
        self.camera.resize(w as f32, h as f32);
        self.needs_redraw = true;
    }

    /// Get the current zoom level.
    pub fn zoom(&self) -> f32 {
        self.camera.zoom
    }

    /// Get viewport width.
    pub fn viewport_width(&self) -> f32 {
        self.camera.viewport_width
    }

    /// Get viewport height.
    pub fn viewport_height(&self) -> f32 {
        self.camera.viewport_height
    }

    /// Convert screen coordinates to world coordinates.
    /// Returns `[world_x, world_y]` as a `Float32Array`.
    pub fn screen_to_world(&self, screen_x: f32, screen_y: f32) -> Vec<f32> {
        let (wx, wy) = self.camera.screen_to_world(screen_x, screen_y);
        vec![wx, wy]
    }
}

// ── Rendering ─────────────────────────────────────────────────────

#[wasm_bindgen]
impl LogosApp {
    /// Set the background clear color (0.0–1.0 per channel).
    pub fn set_clear_color(&mut self, r: f64, g: f64, b: f64, a: f64) {
        self.renderer.set_clear_color(r, g, b, a);
        self.needs_redraw = true;
    }

    /// Render one frame. Call this from `requestAnimationFrame`.
    ///
    /// Returns the number of draw calls issued.
    ///
    /// ```javascript
    /// function frame() {
    ///     const draws = app.render_frame();
    ///     requestAnimationFrame(frame);
    /// }
    /// ```
    pub fn render_frame(&mut self) -> Result<u32, JsValue> {
        if self.needs_redraw {
            self.rebuild_instances();
            self.needs_redraw = false;
        }

        let camera = self.camera.uniform();
        self.renderer.prepare(&self.gpu, &self.instances, &camera);

        let stats = self
            .renderer
            .render_to_surface(&self.gpu)
            .map_err(|e| JsValue::from_str(&format!("Render error: {e:?}")))?;

        Ok(stats.draw_calls)
    }

    /// Force a full redraw on the next frame.
    pub fn invalidate(&mut self) {
        self.needs_redraw = true;
    }

    /// Check if a redraw is pending.
    pub fn needs_redraw(&self) -> bool {
        self.needs_redraw
    }
}

// ── Private helpers ───────────────────────────────────────────────

impl LogosApp {
    /// Rebuild the flat instance buffer from layout engine output.
    fn rebuild_instances(&mut self) {
        self.instances.clear();
        collect_instances_fast(
            &self.layout_engine,
            &self.layer_ids,
            &self.layer_colors,
            &mut self.instances,
        );
    }
}

/// Parse a UUID string from JavaScript, returning a clean error message.
fn parse_uuid(s: &str) -> Result<Uuid, JsValue> {
    Uuid::parse_str(s).map_err(|e| JsValue::from_str(&format!("Invalid UUID '{s}': {e}")))
}
