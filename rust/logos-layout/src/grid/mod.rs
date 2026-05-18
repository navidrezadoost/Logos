//! Grid layout implementation for Logos.
//!
//! Grid layout pipeline:
//! - `params`:      Parse grid container properties (tracks, cells, alignment)
//! - `layout_data`: Per-child cell assignment and sizing constraints
//! - `areas`:       Named area resolution and span expansion
//! - `positions`:   Track sizing, fr unit resolution, cell positioning
//! - `bounds`:      Compute final container bounding rectangle

pub mod areas;
pub mod bounds;
pub mod layout_data;
pub mod params;
pub mod positions;

pub use areas::GridArea;
pub use bounds::{compute_grid_bounds, track_extent, track_line_positions, GridBounds};
pub use layout_data::{
    auto_place_children, calc_grid_layout_data, compute_child_sizes, resolve_tracks,
    ChildGridLayout, ChildShape as GridChildShape, GridPlacement, ResolvedTracks,
};
pub use params::{
    AlignContent, AlignItems, GridCell, GridContainer, GridDirection, GridPosition, GridTrack,
    GridTrackType, JustifyContent, JustifyItems, TrackAlignSelf, TrackJustifySelf,
};
pub use positions::{
    cell_bounds_for_child, compute_positions, compute_track_starts, get_cell_at_position,
    CellBounds, ChildMargins, PositionedChild,
};
