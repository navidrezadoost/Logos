//! GPU vertex and instance data types for the Logos renderer.
//!
//! All types derive `bytemuck::Pod` + `Zeroable` for zero-copy upload
//! to GPU buffers.

use bytemuck::{Pod, Zeroable};
use wgpu::{BufferAddress, VertexAttribute, VertexBufferLayout, VertexFormat, VertexStepMode};

// ───────────────────────────────────────────────────────────────────
// Vertex (unit quad)
// ───────────────────────────────────────────────────────────────────

/// A single vertex of the unit quad (0,0)→(1,1).
///
/// The quad is shared across ALL rect instances.  Per-instance data
/// (position, size, color) is provided via `RectInstance`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct QuadVertex {
    /// Position in [0, 1] space.
    pub position: [f32; 2],
}

impl QuadVertex {
    /// The 4 vertices of a unit quad.
    pub const VERTICES: [QuadVertex; 4] = [
        QuadVertex { position: [0.0, 0.0] }, // top-left
        QuadVertex { position: [1.0, 0.0] }, // top-right
        QuadVertex { position: [0.0, 1.0] }, // bottom-left
        QuadVertex { position: [1.0, 1.0] }, // bottom-right
    ];

    /// Triangle-strip indices for the unit quad.
    pub const INDICES: [u16; 6] = [0, 1, 2, 2, 1, 3];

    pub fn layout() -> VertexBufferLayout<'static> {
        static ATTRS: &[VertexAttribute] = &[
            // location(0) = position
            VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: VertexFormat::Float32x2,
            },
        ];
        VertexBufferLayout {
            array_stride: std::mem::size_of::<QuadVertex>() as BufferAddress,
            step_mode: VertexStepMode::Vertex,
            attributes: ATTRS,
        }
    }
}

// ───────────────────────────────────────────────────────────────────
// Instance data
// ───────────────────────────────────────────────────────────────────

/// Per-instance data for a single rectangle drawn via instanced rendering.
///
/// 48 bytes per instance — 10,000 instances = 480 KB of GPU memory.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct RectInstance {
    /// World-space position (top-left corner) in pixels.
    pub position: [f32; 2],
    /// Width and height in pixels.
    pub size: [f32; 2],
    /// RGBA color, each channel in [0.0, 1.0].
    pub color: [f32; 4],
    /// Border radius in pixels (uniform for all 4 corners).
    pub border_radius: f32,
    /// Z-order (0 = backmost, higher = frontmost).
    pub z_index: f32,
    /// Padding for 16-byte alignment.
    pub _pad: [f32; 2],
}

impl RectInstance {
    pub fn new(x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) -> Self {
        Self {
            position: [x, y],
            size: [w, h],
            color,
            border_radius: 0.0,
            z_index: 0.0,
            _pad: [0.0; 2],
        }
    }

    pub fn with_radius(mut self, r: f32) -> Self {
        self.border_radius = r;
        self
    }

    pub fn with_z(mut self, z: f32) -> Self {
        self.z_index = z;
        self
    }

    pub fn layout() -> VertexBufferLayout<'static> {
        static ATTRS: &[VertexAttribute] = &[
            // location(1) = position
            VertexAttribute {
                offset: 0,
                shader_location: 1,
                format: VertexFormat::Float32x2,
            },
            // location(2) = size
            VertexAttribute {
                offset: 8,
                shader_location: 2,
                format: VertexFormat::Float32x2,
            },
            // location(3) = color
            VertexAttribute {
                offset: 16,
                shader_location: 3,
                format: VertexFormat::Float32x4,
            },
            // location(4) = border_radius
            VertexAttribute {
                offset: 32,
                shader_location: 4,
                format: VertexFormat::Float32,
            },
            // location(5) = z_index
            VertexAttribute {
                offset: 36,
                shader_location: 5,
                format: VertexFormat::Float32,
            },
        ];
        VertexBufferLayout {
            array_stride: std::mem::size_of::<RectInstance>() as BufferAddress,
            step_mode: VertexStepMode::Instance,
            attributes: ATTRS,
        }
    }
}

// ───────────────────────────────────────────────────────────────────
// Camera uniform
// ───────────────────────────────────────────────────────────────────

/// Per-instance data for a single glyph quad drawn via instanced rendering.
///
/// 48 bytes per instance — 10,000 glyphs = 480 KB of GPU memory.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct TextInstance {
    /// World-space position of the glyph quad top-left.
    pub position: [f32; 2],
    /// Width and height of the glyph in pixels.
    pub size: [f32; 2],
    /// Atlas UV top-left.
    pub uv_min: [f32; 2],
    /// Atlas UV bottom-right.
    pub uv_max: [f32; 2],
    /// RGBA text color, each channel in [0.0, 1.0].
    pub color: [f32; 4],
}

impl TextInstance {
    pub fn new(
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        uv_min: [f32; 2],
        uv_max: [f32; 2],
        color: [f32; 4],
    ) -> Self {
        Self {
            position: [x, y],
            size: [w, h],
            uv_min,
            uv_max,
            color,
        }
    }

    pub fn layout() -> VertexBufferLayout<'static> {
        static ATTRS: &[VertexAttribute] = &[
            // location(1) = position
            VertexAttribute {
                offset: 0,
                shader_location: 1,
                format: VertexFormat::Float32x2,
            },
            // location(2) = size
            VertexAttribute {
                offset: 8,
                shader_location: 2,
                format: VertexFormat::Float32x2,
            },
            // location(3) = uv_min
            VertexAttribute {
                offset: 16,
                shader_location: 3,
                format: VertexFormat::Float32x2,
            },
            // location(4) = uv_max
            VertexAttribute {
                offset: 24,
                shader_location: 4,
                format: VertexFormat::Float32x2,
            },
            // location(5) = color
            VertexAttribute {
                offset: 32,
                shader_location: 5,
                format: VertexFormat::Float32x4,
            },
        ];
        VertexBufferLayout {
            array_stride: std::mem::size_of::<TextInstance>() as BufferAddress,
            step_mode: VertexStepMode::Instance,
            attributes: ATTRS,
        }
    }
}

// ───────────────────────────────────────────────────────────────────
// Camera uniform
// ───────────────────────────────────────────────────────────────────

/// Camera/viewport uniform sent to the GPU once per frame.
///
/// 80 bytes — fits in a single uniform buffer.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct CameraUniform {
    /// 4×4 orthographic projection matrix (column-major).
    pub view_proj: [[f32; 4]; 4],
}

impl CameraUniform {
    /// Build an orthographic projection for a viewport of `width × height`
    /// pixels, with optional pan and zoom.
    ///
    /// Maps (0,0) to top-left, (width, height) to bottom-right.
    /// This matches the design-tool convention where Y grows downward.
    pub fn orthographic(width: f32, height: f32, pan_x: f32, pan_y: f32, zoom: f32) -> Self {
        // NDC: x ∈ [-1, 1], y ∈ [-1, 1]
        //
        // world_x_visible = [pan_x, pan_x + width/zoom]
        // world_y_visible = [pan_y, pan_y + height/zoom]
        //
        // ndc_x = (world_x - pan_x) * (2 * zoom / width) - 1
        // ndc_y = -((world_y - pan_y) * (2 * zoom / height) - 1)
        //       = 1 - (world_y - pan_y) * (2 * zoom / height)
        //
        // Column-major 4×4:
        let sx = 2.0 * zoom / width;
        let sy = -2.0 * zoom / height; // flip Y for top-left origin
        let tx = -pan_x * sx - 1.0;
        let ty = -pan_y * sy + 1.0;

        Self {
            view_proj: [
                [sx,  0.0, 0.0, 0.0],
                [0.0, sy,  0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [tx,  ty,  0.0, 1.0],
            ],
        }
    }

    /// Identity: 1px = 1 unit, no pan, no zoom.
    pub fn identity(width: f32, height: f32) -> Self {
        Self::orthographic(width, height, 0.0, 0.0, 1.0)
    }
}

// ===================================================================
// Tests
// ===================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quad_vertex_size() {
        assert_eq!(std::mem::size_of::<QuadVertex>(), 8);
    }

    #[test]
    fn test_rect_instance_size() {
        assert_eq!(std::mem::size_of::<RectInstance>(), 48);
    }

    #[test]
    fn test_camera_uniform_size() {
        assert_eq!(std::mem::size_of::<CameraUniform>(), 64);
    }

    #[test]
    fn test_rect_instance_builder() {
        let inst = RectInstance::new(10.0, 20.0, 100.0, 50.0, [1.0, 0.0, 0.0, 1.0])
            .with_radius(5.0)
            .with_z(3.0);
        assert_eq!(inst.position, [10.0, 20.0]);
        assert_eq!(inst.size, [100.0, 50.0]);
        assert_eq!(inst.color, [1.0, 0.0, 0.0, 1.0]);
        assert!((inst.border_radius - 5.0).abs() < f32::EPSILON);
        assert!((inst.z_index - 3.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_camera_identity_top_left() {
        let cam = CameraUniform::identity(800.0, 600.0);
        let vp = cam.view_proj;

        // Top-left (0,0) should map to NDC (-1, 1)
        let ndc_x = 0.0 * vp[0][0] + 0.0 * vp[1][0] + vp[3][0];
        let ndc_y = 0.0 * vp[0][1] + 0.0 * vp[1][1] + vp[3][1];
        assert!((ndc_x - (-1.0)).abs() < 1e-5, "top-left x should be -1, got {ndc_x}");
        assert!((ndc_y - 1.0).abs() < 1e-5, "top-left y should be 1, got {ndc_y}");
    }

    #[test]
    fn test_camera_identity_bottom_right() {
        let cam = CameraUniform::identity(800.0, 600.0);
        let vp = cam.view_proj;

        // Bottom-right (800, 600) should map to NDC (1, -1)
        let ndc_x = 800.0 * vp[0][0] + 600.0 * vp[1][0] + vp[3][0];
        let ndc_y = 800.0 * vp[0][1] + 600.0 * vp[1][1] + vp[3][1];
        assert!((ndc_x - 1.0).abs() < 1e-5, "bottom-right x should be 1, got {ndc_x}");
        assert!((ndc_y - (-1.0)).abs() < 1e-5, "bottom-right y should be -1, got {ndc_y}");
    }

    #[test]
    fn test_camera_identity_center() {
        let cam = CameraUniform::identity(800.0, 600.0);
        let vp = cam.view_proj;

        let ndc_x = 400.0 * vp[0][0] + 300.0 * vp[1][0] + vp[3][0];
        let ndc_y = 400.0 * vp[0][1] + 300.0 * vp[1][1] + vp[3][1];
        assert!((ndc_x).abs() < 1e-5, "center x should be 0, got {ndc_x}");
        assert!((ndc_y).abs() < 1e-5, "center y should be 0, got {ndc_y}");
    }

    #[test]
    fn test_camera_zoom() {
        let cam = CameraUniform::orthographic(800.0, 600.0, 0.0, 0.0, 2.0);
        let vp = cam.view_proj;

        // At 2× zoom, (400, 300) should map to NDC (1, -1) — only top-left quarter visible
        let ndc_x = 400.0 * vp[0][0] + 300.0 * vp[1][0] + vp[3][0];
        let ndc_y = 400.0 * vp[0][1] + 300.0 * vp[1][1] + vp[3][1];
        assert!((ndc_x - 1.0).abs() < 1e-5, "zoomed center x should be 1, got {ndc_x}");
        assert!((ndc_y - (-1.0)).abs() < 1e-5, "zoomed center y should be -1, got {ndc_y}");
    }

    #[test]
    fn test_camera_pan() {
        let cam = CameraUniform::orthographic(800.0, 600.0, 100.0, 50.0, 1.0);
        let vp = cam.view_proj;

        // World (100, 50) should map to NDC (-1, 1) = top-left of screen
        let ndc_x = 100.0 * vp[0][0] + 50.0 * vp[1][0] + vp[3][0];
        let ndc_y = 100.0 * vp[0][1] + 50.0 * vp[1][1] + vp[3][1];
        assert!((ndc_x - (-1.0)).abs() < 1e-5, "panned top-left x should be -1, got {ndc_x}");
        assert!((ndc_y - 1.0).abs() < 1e-5, "panned top-left y should be 1, got {ndc_y}");
    }

    #[test]
    fn test_quad_vertices_count() {
        assert_eq!(QuadVertex::VERTICES.len(), 4);
        assert_eq!(QuadVertex::INDICES.len(), 6);
    }

    #[test]
    fn test_vertex_layout_locations() {
        let layout = QuadVertex::layout();
        assert_eq!(layout.attributes.len(), 1);
        assert_eq!(layout.attributes[0].shader_location, 0);
        assert_eq!(layout.step_mode, VertexStepMode::Vertex);
    }

    #[test]
    fn test_instance_layout_locations() {
        let layout = RectInstance::layout();
        assert_eq!(layout.attributes.len(), 5);
        assert_eq!(layout.attributes[0].shader_location, 1); // position
        assert_eq!(layout.attributes[1].shader_location, 2); // size
        assert_eq!(layout.attributes[2].shader_location, 3); // color
        assert_eq!(layout.attributes[3].shader_location, 4); // border_radius
        assert_eq!(layout.attributes[4].shader_location, 5); // z_index
        assert_eq!(layout.step_mode, VertexStepMode::Instance);
    }

    #[test]
    fn test_text_instance_size() {
        assert_eq!(std::mem::size_of::<TextInstance>(), 48);
    }

    #[test]
    fn test_text_instance_builder() {
        let inst = TextInstance::new(
            10.0, 20.0, 8.0, 12.0,
            [0.0, 0.0], [0.5, 0.5],
            [1.0, 1.0, 1.0, 1.0],
        );
        assert_eq!(inst.position, [10.0, 20.0]);
        assert_eq!(inst.size, [8.0, 12.0]);
        assert_eq!(inst.uv_min, [0.0, 0.0]);
        assert_eq!(inst.uv_max, [0.5, 0.5]);
        assert_eq!(inst.color, [1.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn test_text_instance_layout_locations() {
        let layout = TextInstance::layout();
        assert_eq!(layout.attributes.len(), 5);
        assert_eq!(layout.attributes[0].shader_location, 1); // position
        assert_eq!(layout.attributes[1].shader_location, 2); // size
        assert_eq!(layout.attributes[2].shader_location, 3); // uv_min
        assert_eq!(layout.attributes[3].shader_location, 4); // uv_max
        assert_eq!(layout.attributes[4].shader_location, 5); // color
        assert_eq!(layout.step_mode, VertexStepMode::Instance);
    }

    #[test]
    fn test_text_instance_bytemuck_cast() {
        let inst = TextInstance::new(
            1.0, 2.0, 3.0, 4.0,
            [0.1, 0.2], [0.9, 0.8],
            [1.0, 0.0, 0.0, 1.0],
        );
        let bytes = bytemuck::bytes_of(&inst);
        assert_eq!(bytes.len(), 48);
        // Round-trip
        let back: &TextInstance = bytemuck::from_bytes(bytes);
        assert_eq!(back.position, inst.position);
        assert_eq!(back.uv_min, inst.uv_min);
    }
}
