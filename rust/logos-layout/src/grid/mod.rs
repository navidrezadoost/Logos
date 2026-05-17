//! Grid layout implementation for Logos.
//!
//! Grid layout pipeline:
//! - `params`:      Parse grid container properties (tracks, cells, alignment)
//! - `layout_data`: Per-child cell assignment and sizing constraints
//! - `areas`:       Named area resolution and span expansion
//! - `positions`:   Track sizing, fr unit resolution, cell positioning
//! - `bounds`:      Compute final container bounding rectangle

pub mod layout_data;
pub mod params;

pub use layout_data::{
    calc_grid_layout_data, auto_place_children, compute_child_sizes, resolve_tracks,
    ChildGridLayout, ChildShape as GridChildShape, GridPlacement, ResolvedTracks,
};
pub use params::{
    AlignContent, AlignItems, GridCell, GridContainer, GridDirection, GridPosition, GridTrack,
    GridTrackType, JustifyContent, JustifyItems, TrackAlignSelf, TrackJustifySelf,
};
