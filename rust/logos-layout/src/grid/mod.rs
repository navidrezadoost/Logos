//! Grid layout implementation for Logos.
//!
//! Grid layout pipeline:
//! - `params`:      Parse grid container properties (tracks, cells, alignment)
//! - `layout_data`: Per-child cell assignment and sizing constraints
//! - `areas`:       Named area resolution and span expansion
//! - `positions`:   Track sizing, fr unit resolution, cell positioning
//! - `bounds`:      Compute final container bounding rectangle

pub mod params;

pub use params::{
    AlignContent, AlignItems, GridCell, GridContainer, GridDirection, GridPosition, GridTrack,
    GridTrackType, JustifyContent, JustifyItems, TrackAlignSelf, TrackJustifySelf,
};
