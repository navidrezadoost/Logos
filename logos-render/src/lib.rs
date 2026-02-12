//! # logos-render
//!
//! GPU rendering backend for Logos, built on `wgpu`.
//!
//! ## Architecture
//!
//! ```text
//!  Document (logos-core)
//!       │
//!       ▼
//!  LayoutEngine (logos-layout)
//!       │
//!       ▼
//!  bridge::collect_instances()      ◀─── converts Layout → RectInstance
//!       │
//!       ▼
//!  Renderer.prepare(instances)      ◀─── uploads to GPU
//!       │
//!       ▼
//!  Renderer.render_to_surface()     ◀─── single draw call
//! ```
//!
//! ## Crate modules
//!
//! - [`context`] — GPU device/queue/surface initialisation
//! - [`vertex`] — vertex, instance, and camera data types
//! - [`pipelines`] — wgpu render pipelines (rect, …)
//! - [`renderer`] — high-level frame orchestration
//! - [`bridge`] — document → GPU instance conversion

pub mod context;
pub mod vertex;
pub mod pipelines;
pub mod renderer;
pub mod bridge;

// Re-exports for convenience
pub use context::GpuContext;
pub use vertex::{RectInstance, CameraUniform, TextInstance};
pub use renderer::{Renderer, FrameStats};
pub use bridge::{collect_instances, collect_instances_direct};
