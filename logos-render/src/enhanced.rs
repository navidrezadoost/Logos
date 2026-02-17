//! Enhanced render pipeline — MSAA, gradients, shadows.
//!
//! References:
//! - Akenine-Möller, *Real-Time Rendering*, Ch. 5 (MSAA)
//! - Akenine-Möller, *Real-Time Rendering*, Ch. 9 (shadows)
//!
//! This module provides `MsaaConfig` for 4× multisampling and
//! `EnhancedRectInstance` with gradient + shadow per-instance data.

use bytemuck::{Pod, Zeroable};
use wgpu::{
    BufferAddress, TextureFormat, VertexAttribute, VertexBufferLayout,
    VertexFormat, VertexStepMode,
};

// ═══════════════════════════════════════════════════════════════════
// MSAA Configuration
// ═══════════════════════════════════════════════════════════════════

/// MSAA configuration for the render pipeline.
///
/// sample_count must be 1 (off) or 4 (4× MSAA).
/// wgpu on most backends supports 1 and 4; 8 and 16 are rare.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MsaaConfig {
    /// Number of samples per pixel (1 or 4).
    pub sample_count: u32,
}

impl MsaaConfig {
    pub const OFF: Self = Self { sample_count: 1 };
    pub const X4: Self = Self { sample_count: 4 };

    /// Create a new MSAA config, clamping to valid values.
    pub fn new(sample_count: u32) -> Self {
        let count = match sample_count {
            0 | 1 => 1,
            2..=4 => 4,
            _ => 4,
        };
        Self {
            sample_count: count,
        }
    }

    /// Whether MSAA is enabled (sample_count > 1).
    pub fn enabled(&self) -> bool {
        self.sample_count > 1
    }
}

impl Default for MsaaConfig {
    fn default() -> Self {
        Self::X4 // 4× MSAA by default
    }
}

// ═══════════════════════════════════════════════════════════════════
// MSAA Resolve Texture
// ═══════════════════════════════════════════════════════════════════

/// Manages the multisample texture used as the color attachment
/// when MSAA is enabled.  The final image is resolved onto the
/// single-sample surface/texture.
pub struct MsaaTarget {
    /// The multisample texture (sample_count > 1).
    pub texture: Option<wgpu::Texture>,
    /// View into the multisample texture.  
    pub view: Option<wgpu::TextureView>,
    /// Current dimensions.
    pub width: u32,
    pub height: u32,
    pub sample_count: u32,
    pub format: TextureFormat,
}

impl MsaaTarget {
    /// Create a new MSAA render target.
    ///
    /// If `config.sample_count == 1`, no texture is allocated.
    pub fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: TextureFormat,
        config: MsaaConfig,
    ) -> Self {
        if !config.enabled() || width == 0 || height == 0 {
            return Self {
                texture: None,
                view: None,
                width,
                height,
                sample_count: config.sample_count,
                format,
            };
        }

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("msaa_target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: config.sample_count,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        Self {
            texture: Some(texture),
            view: Some(view),
            width,
            height,
            sample_count: config.sample_count,
            format,
        }
    }

    /// Resize the MSAA target (recreate if dimensions changed).
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if width == self.width && height == self.height {
            return;
        }
        if self.sample_count <= 1 || width == 0 || height == 0 {
            self.width = width;
            self.height = height;
            return;
        }
        *self = Self::new(
            device,
            width,
            height,
            self.format,
            MsaaConfig::new(self.sample_count),
        );
    }

    /// Returns true if MSAA is active and the target is allocated.
    pub fn is_active(&self) -> bool {
        self.view.is_some()
    }
}

// ═══════════════════════════════════════════════════════════════════
// Enhanced Rect Instance (gradient + shadow)
// ═══════════════════════════════════════════════════════════════════

/// Per-instance data for enhanced rendering: gradient fill + drop shadow.
///
/// 128 bytes per instance (80 base + 48 for gradient/shadow).
/// At 65,536 instances this is 8 MB GPU memory.
///
/// Fields match the `rect_enhanced.wgsl` shader vertex input.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct EnhancedRectInstance {
    // ── Base (48 bytes, same layout as RectInstance) ──
    /// World-space position (top-left corner) in pixels.
    pub position: [f32; 2],
    /// Width and height in pixels.
    pub size: [f32; 2],
    /// RGBA fill color (or gradient start color).
    pub color: [f32; 4],
    /// Border radius in pixels (uniform corners).
    pub border_radius: f32,
    /// Z-order (higher = frontmost).
    pub z_index: f32,
    pub _pad0: [f32; 2],

    // ── Gradient (32 bytes) ──
    /// Second gradient color (end stop).
    pub grad_color: [f32; 4],
    /// Gradient parameters: (angle_rad, type, _, _)
    ///   type: 0 = solid, 1 = linear, 2 = radial
    pub grad_params: [f32; 4],

    // ── Shadow (32 bytes) ──
    /// Shadow RGBA color.
    pub shadow_color: [f32; 4],
    /// Shadow parameters: (offset_x, offset_y, blur_radius, spread)
    pub shadow_params: [f32; 4],
}

impl EnhancedRectInstance {
    /// Create a solid-color rectangle with no gradient or shadow.
    pub fn solid(x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) -> Self {
        Self {
            position: [x, y],
            size: [w, h],
            color,
            border_radius: 0.0,
            z_index: 0.0,
            _pad0: [0.0; 2],
            grad_color: [0.0; 4],
            grad_params: [0.0; 4], // type = 0 → solid
            shadow_color: [0.0; 4],
            shadow_params: [0.0; 4],
        }
    }

    /// Set border radius.
    pub fn with_radius(mut self, r: f32) -> Self {
        self.border_radius = r;
        self
    }

    /// Set z-index.
    pub fn with_z(mut self, z: f32) -> Self {
        self.z_index = z;
        self
    }

    /// Set a linear gradient fill.
    pub fn with_linear_gradient(mut self, angle_deg: f32, end_color: [f32; 4]) -> Self {
        self.grad_color = end_color;
        self.grad_params = [angle_deg.to_radians(), 1.0, 0.0, 0.0];
        self
    }

    /// Set a radial gradient fill.
    pub fn with_radial_gradient(mut self, end_color: [f32; 4]) -> Self {
        self.grad_color = end_color;
        self.grad_params = [0.0, 2.0, 0.0, 0.0];
        self
    }

    /// Set a drop shadow.
    pub fn with_shadow(
        mut self,
        color: [f32; 4],
        offset_x: f32,
        offset_y: f32,
        blur_radius: f32,
        spread: f32,
    ) -> Self {
        self.shadow_color = color;
        self.shadow_params = [offset_x, offset_y, blur_radius, spread];
        self
    }

    /// Vertex buffer layout for the enhanced instance.
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
            // location(6) = grad_color
            VertexAttribute {
                offset: 48,
                shader_location: 6,
                format: VertexFormat::Float32x4,
            },
            // location(7) = grad_params
            VertexAttribute {
                offset: 64,
                shader_location: 7,
                format: VertexFormat::Float32x4,
            },
            // location(8) = shadow_color
            VertexAttribute {
                offset: 80,
                shader_location: 8,
                format: VertexFormat::Float32x4,
            },
            // location(9) = shadow_params
            VertexAttribute {
                offset: 96,
                shader_location: 9,
                format: VertexFormat::Float32x4,
            },
        ];
        VertexBufferLayout {
            array_stride: std::mem::size_of::<EnhancedRectInstance>() as BufferAddress,
            step_mode: VertexStepMode::Instance,
            attributes: ATTRS,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ─── MSAA Config ─────────────────────────────────────────

    #[test]
    fn test_msaa_config_default() {
        let config = MsaaConfig::default();
        assert_eq!(config.sample_count, 4);
        assert!(config.enabled());
    }

    #[test]
    fn test_msaa_config_off() {
        let config = MsaaConfig::OFF;
        assert_eq!(config.sample_count, 1);
        assert!(!config.enabled());
    }

    #[test]
    fn test_msaa_config_clamping() {
        assert_eq!(MsaaConfig::new(0).sample_count, 1);
        assert_eq!(MsaaConfig::new(1).sample_count, 1);
        assert_eq!(MsaaConfig::new(2).sample_count, 4);
        assert_eq!(MsaaConfig::new(3).sample_count, 4);
        assert_eq!(MsaaConfig::new(4).sample_count, 4);
        assert_eq!(MsaaConfig::new(8).sample_count, 4);
    }

    #[test]
    fn test_msaa_config_equality() {
        assert_eq!(MsaaConfig::X4, MsaaConfig::new(4));
        assert_ne!(MsaaConfig::OFF, MsaaConfig::X4);
    }

    // ─── Enhanced Rect Instance ──────────────────────────────

    #[test]
    fn test_enhanced_instance_size() {
        assert_eq!(std::mem::size_of::<EnhancedRectInstance>(), 112);
    }

    #[test]
    fn test_enhanced_instance_solid() {
        let inst = EnhancedRectInstance::solid(10.0, 20.0, 100.0, 50.0, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(inst.position, [10.0, 20.0]);
        assert_eq!(inst.size, [100.0, 50.0]);
        assert_eq!(inst.grad_params[1], 0.0); // type = solid
        assert_eq!(inst.shadow_color[3], 0.0); // no shadow
    }

    #[test]
    fn test_enhanced_instance_gradient() {
        let inst = EnhancedRectInstance::solid(0.0, 0.0, 200.0, 100.0, [1.0, 0.0, 0.0, 1.0])
            .with_linear_gradient(90.0, [0.0, 0.0, 1.0, 1.0]);
        assert!((inst.grad_params[0] - std::f32::consts::FRAC_PI_2).abs() < 0.01);
        assert_eq!(inst.grad_params[1], 1.0); // linear
        assert_eq!(inst.grad_color, [0.0, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn test_enhanced_instance_radial_gradient() {
        let inst = EnhancedRectInstance::solid(0.0, 0.0, 100.0, 100.0, [1.0, 1.0, 1.0, 1.0])
            .with_radial_gradient([0.0, 0.0, 0.0, 1.0]);
        assert_eq!(inst.grad_params[1], 2.0); // radial
    }

    #[test]
    fn test_enhanced_instance_shadow() {
        let inst = EnhancedRectInstance::solid(0.0, 0.0, 100.0, 50.0, [1.0; 4])
            .with_shadow([0.0, 0.0, 0.0, 0.5], 4.0, 4.0, 8.0, 2.0);
        assert_eq!(inst.shadow_params, [4.0, 4.0, 8.0, 2.0]);
        assert_eq!(inst.shadow_color[3], 0.5);
    }

    #[test]
    fn test_enhanced_instance_bytemuck() {
        let inst = EnhancedRectInstance::solid(1.0, 2.0, 3.0, 4.0, [0.5; 4])
            .with_radius(8.0)
            .with_z(3.0)
            .with_linear_gradient(45.0, [1.0; 4])
            .with_shadow([0.0, 0.0, 0.0, 0.3], 2.0, 2.0, 4.0, 0.0);

        let bytes = bytemuck::bytes_of(&inst);
        assert_eq!(bytes.len(), 112);

        let back: &EnhancedRectInstance = bytemuck::from_bytes(bytes);
        assert_eq!(back.position, inst.position);
        assert_eq!(back.size, inst.size);
        assert_eq!(back.border_radius, inst.border_radius);
    }

    #[test]
    fn test_enhanced_instance_layout() {
        let layout = EnhancedRectInstance::layout();
        assert_eq!(layout.attributes.len(), 9);
        assert_eq!(layout.step_mode, VertexStepMode::Instance);
        assert_eq!(
            layout.array_stride as usize,
            std::mem::size_of::<EnhancedRectInstance>()
        );
        // Check shader locations are sequential
        for (i, attr) in layout.attributes.iter().enumerate() {
            assert_eq!(attr.shader_location, (i + 1) as u32);
        }
    }

    #[test]
    fn test_enhanced_instance_builder_chain() {
        let inst = EnhancedRectInstance::solid(0.0, 0.0, 50.0, 50.0, [1.0, 0.0, 0.0, 1.0])
            .with_radius(10.0)
            .with_z(5.0)
            .with_linear_gradient(180.0, [0.0, 1.0, 0.0, 1.0])
            .with_shadow([0.0, 0.0, 0.0, 0.5], 0.0, 4.0, 12.0, 0.0);

        assert_eq!(inst.border_radius, 10.0);
        assert_eq!(inst.z_index, 5.0);
        assert_eq!(inst.shadow_params[2], 12.0); // blur_radius
    }
}
