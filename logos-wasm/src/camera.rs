//! Viewport camera with pan, zoom, and coordinate conversion.
//!
//! Mirrors the desktop Camera from `logos-desktop` but without
//! any platform-specific dependencies. Generates `CameraUniform`
//! for the GPU orthographic projection.

use logos_render::CameraUniform;

/// Viewport camera state.
///
/// Manages pan/zoom Navigation and provides coordinate conversion
/// between screen-space (pixels) and world-space (design units).
pub struct Camera {
    pub pan_x: f32,
    pub pan_y: f32,
    pub zoom: f32,
    pub viewport_width: f32,
    pub viewport_height: f32,
}

impl Camera {
    /// Create a new camera centered on the origin.
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            pan_x: 0.0,
            pan_y: 0.0,
            zoom: 1.0,
            viewport_width: width,
            viewport_height: height,
        }
    }

    /// Convert screen-space coordinates to world-space.
    pub fn screen_to_world(&self, screen_x: f32, screen_y: f32) -> (f32, f32) {
        let world_x = (screen_x - self.viewport_width / 2.0) / self.zoom + self.pan_x;
        let world_y = (screen_y - self.viewport_height / 2.0) / self.zoom + self.pan_y;
        (world_x, world_y)
    }

    /// Convert world-space coordinates to screen-space.
    pub fn world_to_screen(&self, world_x: f32, world_y: f32) -> (f32, f32) {
        let screen_x = (world_x - self.pan_x) * self.zoom + self.viewport_width / 2.0;
        let screen_y = (world_y - self.pan_y) * self.zoom + self.viewport_height / 2.0;
        (screen_x, screen_y)
    }

    /// Generate the GPU camera uniform (4×4 orthographic matrix).
    pub fn uniform(&self) -> CameraUniform {
        CameraUniform::orthographic(
            self.viewport_width,
            self.viewport_height,
            self.pan_x,
            self.pan_y,
            self.zoom,
        )
    }

    /// Pan the camera by screen-space delta.
    /// The delta is scaled by the current zoom level so panning
    /// feels consistent regardless of zoom.
    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.pan_x -= dx / self.zoom;
        self.pan_y -= dy / self.zoom;
    }

    /// Zoom at a screen-space point, keeping the world point under
    /// the cursor stationary (focal zoom).
    pub fn zoom_at(&mut self, screen_x: f32, screen_y: f32, factor: f32) {
        // Remember world point under cursor
        let (world_x, world_y) = self.screen_to_world(screen_x, screen_y);

        // Apply zoom with clamping
        self.zoom *= factor;
        self.zoom = self.zoom.clamp(0.1, 50.0);

        // Adjust pan so the same world point stays under cursor
        let (new_world_x, new_world_y) = self.screen_to_world(screen_x, screen_y);
        self.pan_x -= new_world_x - world_x;
        self.pan_y -= new_world_y - world_y;
    }

    /// Resize the viewport (e.g., on window resize).
    pub fn resize(&mut self, width: f32, height: f32) {
        self.viewport_width = width;
        self.viewport_height = height;
    }

    /// Get viewport dimensions.
    pub fn viewport_size(&self) -> (f32, f32) {
        (self.viewport_width, self.viewport_height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_default_state() {
        let cam = Camera::new(800.0, 600.0);
        assert_eq!(cam.pan_x, 0.0);
        assert_eq!(cam.pan_y, 0.0);
        assert_eq!(cam.zoom, 1.0);
        assert_eq!(cam.viewport_width, 800.0);
        assert_eq!(cam.viewport_height, 600.0);
    }

    #[test]
    fn test_screen_to_world_center() {
        let cam = Camera::new(800.0, 600.0);
        // Center of screen maps to world origin
        let (wx, wy) = cam.screen_to_world(400.0, 300.0);
        assert!((wx).abs() < 1e-5);
        assert!((wy).abs() < 1e-5);
    }

    #[test]
    fn test_screen_to_world_corners() {
        let cam = Camera::new(800.0, 600.0);
        // Top-left corner
        let (wx, wy) = cam.screen_to_world(0.0, 0.0);
        assert!((wx - (-400.0)).abs() < 1e-5);
        assert!((wy - (-300.0)).abs() < 1e-5);
    }

    #[test]
    fn test_screen_to_world_with_pan() {
        let mut cam = Camera::new(800.0, 600.0);
        cam.pan_x = 100.0;
        cam.pan_y = 50.0;
        let (wx, wy) = cam.screen_to_world(400.0, 300.0);
        assert!((wx - 100.0).abs() < 1e-5);
        assert!((wy - 50.0).abs() < 1e-5);
    }

    #[test]
    fn test_screen_to_world_with_zoom() {
        let mut cam = Camera::new(800.0, 600.0);
        cam.zoom = 2.0;
        // At 2× zoom, screen center still maps to origin
        let (wx, wy) = cam.screen_to_world(400.0, 300.0);
        assert!((wx).abs() < 1e-5);
        assert!((wy).abs() < 1e-5);
        // But corners map to half the distance
        let (wx, wy) = cam.screen_to_world(0.0, 0.0);
        assert!((wx - (-200.0)).abs() < 1e-5);
        assert!((wy - (-150.0)).abs() < 1e-5);
    }

    #[test]
    fn test_world_to_screen_roundtrip() {
        let mut cam = Camera::new(1024.0, 768.0);
        cam.pan_x = 50.0;
        cam.pan_y = -30.0;
        cam.zoom = 1.5;

        let (wx, wy) = (123.0_f32, -456.0_f32);
        let (sx, sy) = cam.world_to_screen(wx, wy);
        let (wx2, wy2) = cam.screen_to_world(sx, sy);
        assert!((wx2 - wx).abs() < 1e-3);
        assert!((wy2 - wy).abs() < 1e-3);
    }

    #[test]
    fn test_pan_updates_position() {
        let mut cam = Camera::new(800.0, 600.0);
        cam.pan(100.0, 50.0);
        // pan() subtracts dx/zoom from pan_x (screen drag → world shift)
        assert!((cam.pan_x - (-100.0)).abs() < 1e-5);
        assert!((cam.pan_y - (-50.0)).abs() < 1e-5);
    }

    #[test]
    fn test_pan_accumulates() {
        let mut cam = Camera::new(800.0, 600.0);
        cam.pan(10.0, 20.0);
        cam.pan(30.0, 40.0);
        assert!((cam.pan_x - (-40.0)).abs() < 1e-5);
        assert!((cam.pan_y - (-60.0)).abs() < 1e-5);
    }

    #[test]
    fn test_pan_scales_with_zoom() {
        let mut cam = Camera::new(800.0, 600.0);
        cam.zoom = 2.0;
        cam.pan(100.0, 0.0);
        // At 2× zoom, 100px screen drag = 50 world units
        assert!((cam.pan_x - (-50.0)).abs() < 1e-5);
    }

    #[test]
    fn test_zoom_at_updates_level() {
        let mut cam = Camera::new(800.0, 600.0);
        cam.zoom_at(400.0, 300.0, 2.0);
        assert!((cam.zoom - 2.0).abs() < 1e-5);
    }

    #[test]
    fn test_zoom_clamp_min() {
        let mut cam = Camera::new(800.0, 600.0);
        cam.zoom_at(400.0, 300.0, 0.001);
        assert!((cam.zoom - 0.1).abs() < 1e-5);
    }

    #[test]
    fn test_zoom_clamp_max() {
        let mut cam = Camera::new(800.0, 600.0);
        cam.zoom = 49.0;
        cam.zoom_at(400.0, 300.0, 2.0);
        assert!((cam.zoom - 50.0).abs() < 1e-5);
    }

    #[test]
    fn test_zoom_at_preserves_world_point() {
        let mut cam = Camera::new(800.0, 600.0);
        let (wx_before, wy_before) = cam.screen_to_world(200.0, 150.0);
        cam.zoom_at(200.0, 150.0, 2.0);
        let (wx_after, wy_after) = cam.screen_to_world(200.0, 150.0);
        // The world point under cursor should stay the same after zoom
        assert!((wx_after - wx_before).abs() < 1e-3);
        assert!((wy_after - wy_before).abs() < 1e-3);
    }

    #[test]
    fn test_resize() {
        let mut cam = Camera::new(800.0, 600.0);
        cam.resize(1920.0, 1080.0);
        assert_eq!(cam.viewport_width, 1920.0);
        assert_eq!(cam.viewport_height, 1080.0);
    }

    #[test]
    fn test_viewport_size() {
        let cam = Camera::new(1024.0, 768.0);
        assert_eq!(cam.viewport_size(), (1024.0, 768.0));
    }

    #[test]
    fn test_uniform_generation() {
        let cam = Camera::new(800.0, 600.0);
        let uniform = cam.uniform();
        // CameraUniform is 64 bytes (4×4 f32 matrix)
        assert_eq!(std::mem::size_of_val(&uniform), 64);
    }

    #[test]
    fn test_zoom_at_center_no_pan_shift() {
        let mut cam = Camera::new(800.0, 600.0);
        // Zooming at the center should not change pan
        cam.zoom_at(400.0, 300.0, 3.0);
        assert!((cam.pan_x).abs() < 1e-3);
        assert!((cam.pan_y).abs() < 1e-3);
    }

    #[test]
    fn test_screen_to_world_combined() {
        let mut cam = Camera::new(800.0, 600.0);
        cam.pan_x = 100.0;
        cam.pan_y = -200.0;
        cam.zoom = 0.5;
        let (wx, wy) = cam.screen_to_world(400.0, 300.0);
        // Center of screen at zoom 0.5 maps to pan position
        assert!((wx - 100.0).abs() < 1e-3);
        assert!((wy - (-200.0)).abs() < 1e-3);
    }
}
