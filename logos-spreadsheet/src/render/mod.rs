//! Cell Rendering Pipeline — renderer-agnostic draw primitives.
//!
//! This module converts a logical [`RenderFrame`](crate::ui::render_data::RenderFrame)
//! into a [`DrawBatch`] of flat draw primitives ([`DrawRect`], [`DrawText`],
//! [`DrawLine`], [`DrawBorder`]) organized by rendering layer.
//!
//! The output is intentionally **GPU-agnostic** — it carries no `wgpu` or
//! other backend dependency. A thin adapter (in `logos-render` or whichever
//! backend is active) can map these 1:1 to instanced draw calls.
//!
//! # Architecture
//!
//! ```text
//! RenderFrame ──► BatchConverter ──► DrawBatch ──► InstanceBridge ──► SpreadsheetFrame
//!                     │                   │              │                   │
//!              SpreadsheetTheme     DrawRect / ...  ViewportCamera    Vec<RectData>
//!                                                                    Vec<TextCommand>
//!                                                       │
//!                                                  DirtyTracker ──► FrameUpdate
//! ```
//!
//! # Quick Start
//!
//! ```rust,ignore
//! use logos_spreadsheet::render::{BatchConverter, DrawBatch};
//!
//! let converter = BatchConverter::light();
//! let batch: DrawBatch = converter.convert(&render_frame);
//!
//! // Feed to a GPU backend, canvas, SVG exporter, etc.
//! for rect in batch.all_rects() {
//!     backend.draw_rect(rect);
//! }
//! ```

pub mod primitives;
pub mod theme;
pub mod batch;
pub mod converter;
pub mod adapter;
pub mod dirty;

// Re-exports for convenience
pub use primitives::{DrawRect, DrawLine, DrawText, DrawBorder, TextAlign, TextVAlign, color_to_f32};
pub use theme::SpreadsheetTheme;
pub use batch::{DrawBatch, DrawLayer, BatchStats};
pub use converter::BatchConverter;
pub use adapter::{
    RectData, TextCommand, ViewportCamera, SpreadsheetFrame,
    FrameRenderStats, InstanceBridge, RenderBackend,
};
pub use dirty::{DirtyTracker, FrameUpdate};
