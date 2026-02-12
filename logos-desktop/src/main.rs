//! Logos Desktop — native design tool powered by wgpu.
//!
//! Uses `winit` 0.30 for windowing and input, `logos-render` for GPU
//! rendering, and the full `logos-core` → `logos-layout` → `logos-render`
//! pipeline for real-time design editing.

mod state;

use log::info;
use std::sync::Arc;
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalPosition},
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{Key, NamedKey},
    window::{Window, WindowAttributes, WindowId},
};

use logos_render::context::GpuContext;
use state::AppState;

/// Winit 0.30 application handler.
struct App {
    window: Option<Arc<Window>>,
    state: Option<AppState>,
    // Mouse tracking for pan gestures.
    mouse_pressed: bool,
    last_mouse: (f64, f64),
    frame_count: u64,
}

impl App {
    fn new() -> Self {
        Self {
            window: None,
            state: None,
            mouse_pressed: false,
            last_mouse: (0.0, 0.0),
            frame_count: 0,
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return; // Already initialized.
        }

        let attrs = WindowAttributes::default()
            .with_title("Logos — Design Tool")
            .with_inner_size(LogicalSize::new(1280, 800))
            .with_min_inner_size(LogicalSize::new(400, 300));

        let window = Arc::new(
            event_loop
                .create_window(attrs)
                .expect("Failed to create window"),
        );

        let size = window.inner_size();

        // Initialize GPU context with the window surface.
        let gpu = pollster::block_on(GpuContext::new_with_surface(
            window.clone(),
            size.width.max(1),
            size.height.max(1),
        ))
        .expect("Failed to initialize GPU");

        let mut app_state = AppState::new(gpu, size.width.max(1), size.height.max(1));
        app_state.load_demo_scene();

        info!(
            "Logos Desktop initialized: {}×{}, GPU: {:?}",
            size.width,
            size.height,
            app_state.gpu.adapter.get_info().name
        );

        self.state = Some(app_state);
        self.window = Some(window);

        // Request the first frame.
        self.window.as_ref().unwrap().request_redraw();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let (Some(window), Some(state)) = (self.window.as_ref(), self.state.as_mut()) else {
            return;
        };

        match event {
            // ── Close / Escape ──────────────────────────────────
            WindowEvent::CloseRequested => {
                info!("Window closed after {} frames", self.frame_count);
                event_loop.exit();
            }
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed =>
            {
                match event.logical_key {
                    Key::Named(NamedKey::Escape) => {
                        // Deselect on Escape.
                        state.interaction.selected = None;
                        state.request_redraw();
                        window.request_redraw();
                    }
                    _ => {}
                }
            }

            // ── Resize ──────────────────────────────────────────
            WindowEvent::Resized(new_size) => {
                state.resize(new_size.width, new_size.height);
                window.request_redraw();
            }

            // ── Mouse move → hover hit test ─────────────────────
            WindowEvent::CursorMoved {
                position: PhysicalPosition { x, y },
                ..
            } => {
                if self.mouse_pressed {
                    // Middle-click or right-click drag → pan.
                    let dx = x - self.last_mouse.0;
                    let dy = y - self.last_mouse.1;
                    state.pan(dx as f32, dy as f32);
                    window.request_redraw();
                } else {
                    state.update_hover(x as f32, y as f32);
                    if state.interaction.hovered.is_some() {
                        window.request_redraw();
                    }
                }
                self.last_mouse = (x, y);
            }

            // ── Mouse buttons ───────────────────────────────────
            WindowEvent::MouseInput { state: btn_state, button, .. } => {
                match (button, btn_state) {
                    (MouseButton::Left, ElementState::Pressed) => {
                        state.select_at(self.last_mouse.0 as f32, self.last_mouse.1 as f32);
                        window.request_redraw();
                    }
                    (MouseButton::Middle | MouseButton::Right, ElementState::Pressed) => {
                        self.mouse_pressed = true;
                    }
                    (MouseButton::Middle | MouseButton::Right, ElementState::Released) => {
                        self.mouse_pressed = false;
                    }
                    _ => {}
                }
            }

            // ── Scroll → zoom ───────────────────────────────────
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(pos) => pos.y as f32 / 50.0,
                };
                state.zoom_at(
                    self.last_mouse.0 as f32,
                    self.last_mouse.1 as f32,
                    dy,
                );
                window.request_redraw();
            }

            // ── Redraw ──────────────────────────────────────────
            WindowEvent::RedrawRequested => {
                match state.render_frame() {
                    Ok(stats) => {
                        self.frame_count += 1;
                        if self.frame_count % 300 == 0 {
                            info!(
                                "Frame {}: {} rects, {} glyphs, {} draw call(s)",
                                self.frame_count, stats.rect_count, stats.text_count, stats.draw_calls
                            );
                        }
                    }
                    Err(logos_render::renderer::RenderError::Surface(
                        wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated,
                    )) => {
                        // Reconfigure surface on lost/outdated.
                        let size = window.inner_size();
                        state.resize(size.width, size.height);
                        window.request_redraw();
                    }
                    Err(e) => {
                        log::error!("Render error: {e}");
                    }
                }
                // Request continuous redraws for animation / interaction.
                window.request_redraw();
            }

            _ => {}
        }
    }
}

fn main() {
    env_logger::init();

    info!("Starting Logos Desktop...");

    let event_loop = EventLoop::new().expect("Failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = App::new();
    event_loop.run_app(&mut app).expect("Event loop error");
}
