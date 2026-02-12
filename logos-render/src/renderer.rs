//! High-level renderer that ties GPU context, pipelines, and document
//! data together into a single `render_frame()` call.

use thiserror::Error;
use wgpu::{
    Color, CommandEncoderDescriptor, LoadOp, Operations, RenderPassColorAttachment,
    RenderPassDescriptor, StoreOp, TextureViewDescriptor,
};

use crate::context::GpuContext;
use crate::pipelines::rect::RectPipeline;
use crate::vertex::{CameraUniform, RectInstance};

#[derive(Error, Debug)]
pub enum RenderError {
    #[error("Surface error: {0}")]
    Surface(#[from] wgpu::SurfaceError),
    #[error("No surface configured (headless mode)")]
    NoSurface,
}

/// Frame statistics returned after each render.
#[derive(Clone, Copy, Debug)]
pub struct FrameStats {
    /// Number of rect instances drawn.
    pub rect_count: u32,
    /// Number of draw calls.
    pub draw_calls: u32,
}

/// Main renderer for the Logos design tool.
///
/// Orchestrates GPU pipelines and manages per-frame uploads.
///
/// # Usage
///
/// ```ignore
/// let mut renderer = Renderer::new(&gpu);
/// renderer.prepare(&gpu, &instances, &camera);
/// let stats = renderer.render_to_surface(&gpu)?;
/// ```
pub struct Renderer {
    rect_pipeline: RectPipeline,
    clear_color: Color,
    quad_uploaded: bool,
}

impl Renderer {
    /// Create a new renderer for the given GPU context.
    pub fn new(gpu: &GpuContext) -> Self {
        let rect_pipeline = RectPipeline::new(&gpu.device, gpu.surface_format);

        Self {
            rect_pipeline,
            clear_color: Color {
                r: 0.12,
                g: 0.12,
                b: 0.13,
                a: 1.0,
            },
            quad_uploaded: false,
        }
    }

    /// Set the background clear color.
    pub fn set_clear_color(&mut self, r: f64, g: f64, b: f64, a: f64) {
        self.clear_color = Color { r, g, b, a };
    }

    /// Upload per-frame data (instances + camera) to the GPU.
    ///
    /// Call this once per frame before `render_to_surface()` or
    /// `render_to_texture()`.
    pub fn prepare(
        &mut self,
        gpu: &GpuContext,
        instances: &[RectInstance],
        camera: &CameraUniform,
    ) {
        // Upload static quad geometry on first frame.
        if !self.quad_uploaded {
            self.rect_pipeline.upload_quad(&gpu.queue);
            self.quad_uploaded = true;
        }

        self.rect_pipeline.upload_instances(&gpu.queue, instances);
        self.rect_pipeline.upload_camera(&gpu.queue, camera);
    }

    /// Render to the window surface.  Returns frame statistics.
    pub fn render_to_surface(&self, gpu: &GpuContext) -> Result<FrameStats, RenderError> {
        let surface = gpu.surface.as_ref().ok_or(RenderError::NoSurface)?;
        let output = surface.get_current_texture()?;
        let view = output.texture.create_view(&TextureViewDescriptor::default());

        let mut encoder = gpu.device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("logos_frame_encoder"),
        });

        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("logos_render_pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(self.clear_color),
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            self.rect_pipeline.draw(&mut pass);
        }

        gpu.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(FrameStats {
            rect_count: self.rect_pipeline.instance_count(),
            draw_calls: if self.rect_pipeline.instance_count() > 0 { 1 } else { 0 },
        })
    }

    /// Render to an off-screen texture (headless mode).
    ///
    /// Returns the frame stats. The rendered output is in `target_view`.
    pub fn render_to_texture(
        &self,
        gpu: &GpuContext,
        target_view: &wgpu::TextureView,
    ) -> FrameStats {
        let mut encoder = gpu.device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("logos_offscreen_encoder"),
        });

        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("logos_offscreen_pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: target_view,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(self.clear_color),
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            self.rect_pipeline.draw(&mut pass);
        }

        gpu.queue.submit(std::iter::once(encoder.finish()));

        FrameStats {
            rect_count: self.rect_pipeline.instance_count(),
            draw_calls: if self.rect_pipeline.instance_count() > 0 { 1 } else { 0 },
        }
    }

    /// Access the rect pipeline (for advanced usage).
    pub fn rect_pipeline(&self) -> &RectPipeline {
        &self.rect_pipeline
    }
}

// ===================================================================
// Tests
// ===================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_stats_default() {
        let stats = FrameStats {
            rect_count: 42,
            draw_calls: 1,
        };
        assert_eq!(stats.rect_count, 42);
        assert_eq!(stats.draw_calls, 1);
    }

    #[test]
    fn test_renderer_creation_headless() {
        // Attempt headless GPU init — may fail in CI without GPU
        let gpu = pollster::block_on(GpuContext::new_headless());
        if let Ok(gpu) = gpu {
            let renderer = Renderer::new(&gpu);
            assert_eq!(renderer.rect_pipeline.instance_count(), 0);
            assert!(!renderer.quad_uploaded);
        }
    }

    #[test]
    fn test_prepare_uploads_instances() {
        let gpu = pollster::block_on(GpuContext::new_headless());
        if let Ok(gpu) = gpu {
            let mut renderer = Renderer::new(&gpu);
            let instances = vec![
                RectInstance::new(0.0, 0.0, 100.0, 50.0, [1.0, 0.0, 0.0, 1.0]),
                RectInstance::new(200.0, 100.0, 80.0, 80.0, [0.0, 1.0, 0.0, 1.0]),
            ];
            let camera = CameraUniform::identity(800.0, 600.0);
            renderer.prepare(&gpu, &instances, &camera);

            assert_eq!(renderer.rect_pipeline.instance_count(), 2);
            assert!(renderer.quad_uploaded);
        }
    }
}
