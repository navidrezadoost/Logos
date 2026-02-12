//! # logos-text
//!
//! Text engine for the Logos design tool. Provides text shaping, glyph
//! rasterization, and texture atlas management via `cosmic-text`.
//!
//! ## Architecture
//!
//! ```text
//! TextEngine (cosmic-text FontSystem + SwashCache)
//!     │
//!     ▼
//! shape_text(str, style) ──► ShapedText { Vec<GlyphQuad> }
//!     │                            │
//!     ▼                            ▼
//!   Atlas ◄── glyph bitmaps ── GPU upload (texture)
//! ```
//!
//! - **`engine`** — Text shaping, font resolution, glyph rasterization.
//! - **`atlas`** — CPU-side glyph texture atlas with shelf packing.

pub mod atlas;
pub mod engine;

// Re-exports for ergonomic use.
pub use atlas::{Atlas, AtlasRegion};
pub use engine::{GlyphQuad, ShapedText, TextEngine, TextStyle};
