//! GPU context — owns `wgpu::Device`, `Queue`, and optional `Surface`.
//!
//! Two construction paths:
//!
//! 1. **Headless** (`GpuContext::new_headless`) — no window, no surface.
//!    Used for tests, benchmarks, and server-side rendering.
//!
//! 2. **Windowed** (`GpuContext::new_with_surface`) — requires a
//!    `raw_window_handle`-compatible window.  Used by `logos-desktop`.

use thiserror::Error;
use wgpu::{
    Adapter, Device, DeviceDescriptor, Instance, InstanceDescriptor, Queue,
    RequestAdapterOptions, Surface, SurfaceConfiguration, TextureFormat,
    TextureUsages,
};

#[derive(Error, Debug)]
pub enum GpuError {
    #[error("No suitable GPU adapter found")]
    NoAdapter,
    #[error("Failed to request device: {0}")]
    RequestDevice(#[from] wgpu::RequestDeviceError),
    #[error("Surface error: {0}")]
    Surface(String),
}

/// Core GPU state shared by all rendering subsystems.
pub struct GpuContext {
    pub device: Device,
    pub queue: Queue,
    pub adapter: Adapter,
    /// Present only when rendering to a window.
    pub surface: Option<Surface<'static>>,
    pub surface_config: Option<SurfaceConfiguration>,
    pub surface_format: TextureFormat,
}

impl GpuContext {
    /// Create a headless context (no window, no surface).
    ///
    /// Useful for off-screen rendering, tests, and CI pipelines.
    pub async fn new_headless() -> Result<Self, GpuError> {
        let instance = Instance::new(&InstanceDescriptor::default());

        let adapter = instance
            .request_adapter(&RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .ok_or(GpuError::NoAdapter)?;

        let (device, queue) = adapter
            .request_device(&DeviceDescriptor {
                label: Some("logos-headless"),
                ..Default::default()
            }, None)
            .await?;

        Ok(Self {
            device,
            queue,
            adapter,
            surface: None,
            surface_config: None,
            // Bgra8UnormSrgb is the most universally supported format.
            surface_format: TextureFormat::Bgra8UnormSrgb,
        })
    }

    /// Create a context with a surface attached to `window`.
    ///
    /// The caller must ensure `window` outlives the returned `GpuContext`.
    ///
    /// # Safety
    ///
    /// `window` must implement `raw_window_handle::HasWindowHandle` and
    /// `raw_window_handle::HasDisplayHandle`.  The handles must remain
    /// valid for the lifetime of the returned context.
    pub async fn new_with_surface<W>(window: W, width: u32, height: u32) -> Result<Self, GpuError>
    where
        W: wgpu::WasmNotSendSync + Into<wgpu::SurfaceTarget<'static>>,
    {
        let instance = Instance::new(&InstanceDescriptor::default());

        let surface = instance
            .create_surface(window)
            .map_err(|e| GpuError::Surface(e.to_string()))?;

        let adapter = instance
            .request_adapter(&RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or(GpuError::NoAdapter)?;

        let (device, queue) = adapter
            .request_device(&DeviceDescriptor {
                label: Some("logos-windowed"),
                ..Default::default()
            }, None)
            .await?;

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(caps.formats[0]);

        let config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode: wgpu::PresentMode::Fifo, // VSync
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        Ok(Self {
            device,
            queue,
            adapter,
            surface: Some(surface),
            surface_config: Some(config),
            surface_format: format,
        })
    }

    /// Resize the surface.  No-op if headless.
    pub fn resize(&mut self, width: u32, height: u32) {
        if let Some(config) = &mut self.surface_config {
            if width == 0 || height == 0 {
                return;
            }
            config.width = width;
            config.height = height;
            if let Some(surface) = &self.surface {
                surface.configure(&self.device, config);
            }
        }
    }

    /// Current surface dimensions, or `(0, 0)` if headless.
    pub fn surface_size(&self) -> (u32, u32) {
        self.surface_config
            .as_ref()
            .map(|c| (c.width, c.height))
            .unwrap_or((0, 0))
    }
}

// ===================================================================
// Tests
// ===================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_surface_size_headless() {
        let ctx = pollster::block_on(GpuContext::new_headless());
        // May fail in CI without GPU — that's OK, skip gracefully.
        if let Ok(ctx) = ctx {
            assert_eq!(ctx.surface_size(), (0, 0));
            assert!(ctx.surface.is_none());
            assert!(ctx.surface_config.is_none());
        }
    }
}
