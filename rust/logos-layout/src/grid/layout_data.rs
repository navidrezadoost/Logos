//! Grid layout data — track resolution, auto-placement, and per-child sizing.
//!
//! Input:  GridContainer (track definitions, cells, gaps, padding) + available space
//! Output: ResolvedTracks + per-child placements + per-child sizes
//!
//! ## Algorithm (CSS Grid spec §11 / Logos grid-layout engine)
//!
//! ### Phase 1 — Track sizing
//! Initialize each track:
//!   - `Fixed(px)`:   base_size = max_size = px
//!   - `Percent(%)`:  base_size = max_size = container_dim × pct / 100
//!   - `Flex(fr)`:    base_size = 0 (resolved later), max_size = ∞
//!   - `Auto`:        base_size = 0 (content-driven), max_size = ∞
//!
//! Resolve `fr` units:
//!   free_space = container_dim − Σ(non-flex track sizes) − total_gap
//!   fr_value   = free_space / Σ(fr factors)
//!   Each flex track grows to `fr_factor × fr_value`, clamped to base_size.
//!
//! Distribute remaining free space to `Auto` tracks when `justify-content` or
//! `align-content` is `Stretch`.
//!
//! ### Phase 2 — Auto-placement
//! Children assigned to `Auto` cells get placed by scanning the grid in
//! row-major (or column-major, per `direction`) order and filling the first
//! empty cell that fits the child's span.
//!
//! ### Phase 3 — Child size computation
//! `width  = Σ column_tracks[col_start .. col_start + col_span] + gap × (col_span − 1)`
//! `height = Σ row_tracks[row_start .. row_start + row_span]    + gap × (row_span − 1)`
//!
//! Clamped to (min_width, max_width) and (min_height, max_height).

use std::collections::HashMap;

use super::params::{GridCell, GridContainer, GridDirection, GridTrackType, JustifyContent, AlignContent, Uuid};

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

/// Resolved pixel sizes for every row and column track.
///
/// These are the authoritative sizes used by `positions.rs` to compute
/// cell origins and child `(x, y)` coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedTracks {
    /// Resolved column widths in order (index 0 = column 1).
    pub columns: Vec<f64>,
    /// Resolved row heights in order (index 0 = row 1).
    pub rows: Vec<f64>,
}

/// Explicit (row, column) placement for a single child.
///
/// Produced by the auto-placement pass; children with an explicit cell
/// in `GridContainer::cells` use that cell's (row, column, spans) directly.
#[derive(Debug, Clone, PartialEq)]
pub struct GridPlacement {
    /// 1-based starting row.
    pub row_start: usize,
    /// How many rows this child spans.
    pub row_span: usize,
    /// 1-based starting column.
    pub col_start: usize,
    /// How many columns this child spans.
    pub col_span: usize,
}

/// Final computed layout data for a single child after track resolution.
#[derive(Debug, Clone, PartialEq)]
pub struct ChildGridLayout {
    /// Shape UUID.
    pub id: Uuid,
    /// Computed width (pixel-resolved, clamped to min/max).
    pub width: f64,
    /// Computed height (pixel-resolved, clamped to min/max).
    pub height: f64,
    /// 1-based row the child starts on.
    pub row: usize,
    /// 1-based column the child starts on.
    pub col: usize,
    /// Row span.
    pub row_span: usize,
    /// Column span.
    pub col_span: usize,
}

// ---------------------------------------------------------------------------
// Input helper: minimal child shape for layout_data
// ---------------------------------------------------------------------------

/// Minimal representation of a child shape for grid layout.
///
/// The layout pipeline only needs the child's UUID and its explicit size
/// constraints; rendering details live elsewhere.
#[derive(Debug, Clone)]
pub struct ChildShape {
    pub id: Uuid,
    /// Minimum width (default: 0.0).
    pub min_width: f64,
    /// Maximum width (default: `f64::INFINITY`).
    pub max_width: f64,
    /// Minimum height (default: 0.0).
    pub min_height: f64,
    /// Maximum height (default: `f64::INFINITY`).
    pub max_height: f64,
}

impl ChildShape {
    /// Construct a child with only explicit min/max bounds (no content sizing).
    pub fn new(id: Uuid) -> Self {
        ChildShape {
            id,
            min_width: 0.0,
            max_width: f64::INFINITY,
            min_height: 0.0,
            max_height: f64::INFINITY,
        }
    }

    /// Constrain width to [min, max].
    pub fn with_width_bounds(mut self, min: f64, max: f64) -> Self {
        self.min_width = min;
        self.max_width = max;
        self
    }

    /// Constrain height to [min, max].
    pub fn with_height_bounds(mut self, min: f64, max: f64) -> Self {
        self.min_height = min;
        self.max_height = max;
        self
    }
}

// ---------------------------------------------------------------------------
// Phase 1 — Track resolution
// ---------------------------------------------------------------------------

/// Resolve a list of track definitions to pixel sizes.
///
/// Handles `Fixed`, `Percent`, `Flex` (fr), and `Auto` tracks.
/// `Auto` tracks are sized to distribute any remaining space after
/// `Fixed`, `Percent`, and `Flex` tracks are resolved.
///
/// # Arguments
/// * `container`:      the grid container (for justify/align-content stretch)
/// * `container_size`: available space in this dimension (width for columns,
///                     height for rows)
/// * `gap`:            gap between tracks
/// * `tracks`:         ordered track definitions for this dimension
/// * `is_columns`:     true when resolving column tracks (controls which
///                     container-level alignment enum to consult for Stretch)
///
/// # Returns
/// Resolved pixel sizes in the same order as `tracks`.
///
/// # Example
/// ```rust
/// use logos_layout::grid::{GridContainer, GridTrack};
/// use logos_layout::grid::layout_data::resolve_tracks;
///
/// let container = GridContainer::default();
/// let tracks = vec![GridTrack::fixed(100.0), GridTrack::fixed(200.0)];
/// let sizes = resolve_tracks(&container, 500.0, 0.0, &tracks, true);
/// assert_eq!(sizes, vec![100.0, 200.0]);
/// ```
pub fn resolve_tracks(
    container: &GridContainer,
    container_size: f64,
    gap: f64,
    tracks: &[super::params::GridTrack],
    is_columns: bool,
) -> Vec<f64> {
    if tracks.is_empty() {
        return vec![];
    }

    let n = tracks.len();
    let total_gap = gap * (n.saturating_sub(1)) as f64;

    // --- Pass 1: resolve Fixed and Percent, record Flex/Auto indices ---
    let mut sizes: Vec<f64> = vec![0.0; n];
    let mut flex_total_fr: f64 = 0.0;
    let mut flex_indices: Vec<usize> = vec![];
    let mut auto_indices: Vec<usize> = vec![];
    let mut resolved_total: f64 = 0.0;

    for (i, track) in tracks.iter().enumerate() {
        match track.track_type {
            GridTrackType::Fixed => {
                let v = track.value.unwrap_or(0.0);
                sizes[i] = v;
                resolved_total += v;
            }
            GridTrackType::Percent => {
                let v = track.value.unwrap_or(0.0) / 100.0 * container_size;
                sizes[i] = v;
                resolved_total += v;
            }
            GridTrackType::Flex => {
                let fr = track.value.unwrap_or(1.0).max(0.0);
                flex_total_fr += fr;
                flex_indices.push(i);
                // base size stays 0 until fr is resolved
            }
            GridTrackType::Auto => {
                auto_indices.push(i);
                // base size stays 0 until auto is resolved
            }
            GridTrackType::Subgrid => {
                // Subgrid track: this item inherits its parent's track
                // definitions within the spanned area. At this level of the
                // algorithm the size comes from the parent's allocation, so we
                // treat it as Auto (content-sized) and let the parent pass the
                // concrete sizes down through the `GridContainer` it constructs
                // for each subgrid item.
                auto_indices.push(i);
            }
        }
    }

    // --- Pass 2: distribute free space to Flex (fr) tracks ---
    if !flex_indices.is_empty() {
        let free = (container_size - resolved_total - total_gap).max(0.0);
        let fr_value = if flex_total_fr > 0.0 { free / flex_total_fr } else { 0.0 };
        for &i in &flex_indices {
            let fr = tracks[i].value.unwrap_or(1.0).max(0.0);
            sizes[i] = (fr * fr_value).max(sizes[i]);
            resolved_total += sizes[i];
        }
    }

    // --- Pass 3: distribute remaining free space to Auto tracks ---
    if !auto_indices.is_empty() {
        // Check whether we should stretch auto tracks
        let stretch = if is_columns {
            container.justify_content == JustifyContent::Stretch
        } else {
            container.align_content == AlignContent::Stretch
        };

        let auto_free = (container_size - resolved_total - total_gap).max(0.0);
        let per_auto = if stretch && !auto_indices.is_empty() {
            auto_free / auto_indices.len() as f64
        } else {
            0.0
        };

        for &i in &auto_indices {
            sizes[i] = per_auto.max(sizes[i]);
        }
    }

    sizes
}

// ---------------------------------------------------------------------------
// Phase 2 — Auto-placement
// ---------------------------------------------------------------------------

/// Place every child in the grid, using explicit cell assignments where
/// available and auto-placement for the rest.
///
/// # Arguments
/// * `children`: children to place (must include ALL children in layout order)
/// * `container`: grid container providing cells, track counts, and direction
///
/// # Returns
/// A `HashMap<Uuid, GridPlacement>` — one entry per child.
/// Children with an explicit cell in `container.cells` get that cell's
/// (row, column, row_span, col_span). Children without a cell (or in a cell
/// with `GridPosition::Auto`) are placed in the next available slot.
///
/// # Auto-placement algorithm
/// The grid is scanned in the order determined by `container.direction`:
/// - `Row`:    row by row, column by column within each row.
/// - `Column`: column by column, row by row within each column.
///
/// An "occupied" position is any (row, col) already claimed by another
/// child's span.
pub fn auto_place_children(
    children: &[ChildShape],
    container: &GridContainer,
) -> HashMap<Uuid, GridPlacement> {
    let num_cols = container.num_columns().max(1);
    let num_rows = container.num_rows().max(1);

    // Build shape→cell lookup for explicitly placed children.
    let mut shape_to_cell: HashMap<Uuid, &GridCell> = HashMap::new();
    for cell in container.cells.values() {
        for &shape_id in &cell.shapes {
            shape_to_cell.insert(shape_id, cell);
        }
    }

    let mut placements: HashMap<Uuid, GridPlacement> = HashMap::new();

    // Track which (row, col) positions are occupied (by 1-based indices).
    // Format: key = (row_1based, col_1based).
    let mut occupied: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();

    // --- First pass: place children with explicit cells ---
    for child in children {
        if let Some(cell) = shape_to_cell.get(&child.id) {
            let placement = GridPlacement {
                row_start: cell.row,
                row_span: cell.row_span.max(1),
                col_start: cell.column,
                col_span: cell.column_span.max(1),
            };
            // Mark all cells spanned by this child as occupied
            for r in placement.row_start..placement.row_start + placement.row_span {
                for c in placement.col_start..placement.col_start + placement.col_span {
                    occupied.insert((r, c));
                }
            }
            placements.insert(child.id, placement);
        }
    }

    // --- Second pass: auto-place remaining children ---
    // Iterate children in order, skipping those already placed.
    let mut cursor_row = 1usize;
    let mut cursor_col = 1usize;

    for child in children {
        if placements.contains_key(&child.id) {
            continue;
        }

        // Default span = 1×1
        let row_span = 1usize;
        let col_span = 1usize;

        // Find next available slot in direction order
        let placement = match container.direction {
            GridDirection::Row => {
                find_next_slot_row_major(
                    &occupied, cursor_row, cursor_col,
                    num_rows, num_cols, row_span, col_span,
                )
            }
            GridDirection::Column => {
                find_next_slot_col_major(
                    &occupied, cursor_row, cursor_col,
                    num_rows, num_cols, row_span, col_span,
                )
            }
        };

        // Advance cursor past this placement for next auto child
        match container.direction {
            GridDirection::Row => {
                cursor_col = placement.col_start + placement.col_span;
                cursor_row = placement.row_start;
                if cursor_col > num_cols {
                    cursor_col = 1;
                    cursor_row += 1;
                }
            }
            GridDirection::Column => {
                cursor_row = placement.row_start + placement.row_span;
                cursor_col = placement.col_start;
                if cursor_row > num_rows {
                    cursor_row = 1;
                    cursor_col += 1;
                }
            }
        }

        // Mark occupied
        for r in placement.row_start..placement.row_start + placement.row_span {
            for c in placement.col_start..placement.col_start + placement.col_span {
                occupied.insert((r, c));
            }
        }

        placements.insert(child.id, placement);
    }

    placements
}

/// Find the next available slot scanning row-by-row (row-major order).
fn find_next_slot_row_major(
    occupied: &std::collections::HashSet<(usize, usize)>,
    start_row: usize,
    start_col: usize,
    num_rows: usize,
    num_cols: usize,
    row_span: usize,
    col_span: usize,
) -> GridPlacement {
    let mut r = start_row;
    let mut c = start_col;

    loop {
        if r + row_span - 1 > num_rows {
            // Exhausted grid — append a new row
            return GridPlacement { row_start: r, row_span, col_start: 1, col_span };
        }
        if c + col_span - 1 <= num_cols && fits(occupied, r, c, row_span, col_span) {
            return GridPlacement { row_start: r, row_span, col_start: c, col_span };
        }
        // Advance
        c += 1;
        if c > num_cols {
            c = 1;
            r += 1;
        }
    }
}

/// Find the next available slot scanning column-by-column (column-major order).
fn find_next_slot_col_major(
    occupied: &std::collections::HashSet<(usize, usize)>,
    start_row: usize,
    start_col: usize,
    num_rows: usize,
    num_cols: usize,
    row_span: usize,
    col_span: usize,
) -> GridPlacement {
    let mut r = start_row;
    let mut c = start_col;

    loop {
        if c + col_span - 1 > num_cols {
            return GridPlacement { row_start: 1, row_span, col_start: c, col_span };
        }
        if r + row_span - 1 <= num_rows && fits(occupied, r, c, row_span, col_span) {
            return GridPlacement { row_start: r, row_span, col_start: c, col_span };
        }
        r += 1;
        if r > num_rows {
            r = 1;
            c += 1;
        }
    }
}

/// Return true if a rect [row_start, row_start+row_span) × [col_start, col_start+col_span)
/// has no overlap with the occupied set.
fn fits(
    occupied: &std::collections::HashSet<(usize, usize)>,
    row_start: usize,
    col_start: usize,
    row_span: usize,
    col_span: usize,
) -> bool {
    for r in row_start..row_start + row_span {
        for c in col_start..col_start + col_span {
            if occupied.contains(&(r, c)) {
                return false;
            }
        }
    }
    true
}

// ---------------------------------------------------------------------------
// Phase 3 — Child size computation
// ---------------------------------------------------------------------------

/// Compute the final (width, height) for every child from its placement and
/// the resolved track sizes.
///
/// `width  = Σ column_sizes[col_start-1 .. col_start-1+col_span] + column_gap × (col_span - 1)`
/// `height = Σ row_sizes[row_start-1 .. row_start-1+row_span]    + row_gap    × (row_span - 1)`
///
/// Results are clamped to the child's `(min_width, max_width)` and
/// `(min_height, max_height)`.
///
/// # Arguments
/// * `placements`:  output of `auto_place_children()`
/// * `tracks`:      output of `resolve_tracks()` for both axes
/// * `children`:    slice of `ChildShape` for min/max constraints
/// * `column_gap`:  gap between column tracks
/// * `row_gap`:     gap between row tracks
///
/// # Returns
/// One `ChildGridLayout` per child, in the same order as `children`.
///
/// # Example
/// ```rust
/// use logos_layout::grid::{GridContainer, GridTrack, GridCell};
/// use logos_layout::grid::layout_data::{
///     ChildShape, GridPlacement, ResolvedTracks,
///     resolve_tracks, compute_child_sizes,
/// };
/// use std::collections::HashMap;
///
/// let mut container = GridContainer::default();
/// container.columns = vec![GridTrack::fixed(100.0), GridTrack::fixed(200.0)];
/// container.rows    = vec![GridTrack::fixed(80.0)];
/// container.column_gap = 10.0;
///
/// // Child spans columns 1-2
/// let mut placements = HashMap::new();
/// placements.insert(1u64, GridPlacement { row_start: 1, row_span: 1, col_start: 1, col_span: 2 });
///
/// let tracks = ResolvedTracks {
///     columns: vec![100.0, 200.0],
///     rows:    vec![80.0],
/// };
///
/// let children = vec![ChildShape::new(1)];
/// let layouts = compute_child_sizes(&placements, &tracks, &children, 10.0, 0.0);
/// assert_eq!(layouts[0].width,  310.0);  // 100 + 10 + 200
/// assert_eq!(layouts[0].height, 80.0);
/// ```
pub fn compute_child_sizes(
    placements: &HashMap<Uuid, GridPlacement>,
    tracks: &ResolvedTracks,
    children: &[ChildShape],
    column_gap: f64,
    row_gap: f64,
) -> Vec<ChildGridLayout> {
    children
        .iter()
        .filter_map(|child| {
            let placement = placements.get(&child.id)?;

            let col_start = (placement.col_start.saturating_sub(1)).min(tracks.columns.len().saturating_sub(1));
            let col_end   = (col_start + placement.col_span).min(tracks.columns.len());
            let row_start = (placement.row_start.saturating_sub(1)).min(tracks.rows.len().saturating_sub(1));
            let row_end   = (row_start + placement.row_span).min(tracks.rows.len());

            let col_span_actual = col_end.saturating_sub(col_start);
            let row_span_actual = row_end.saturating_sub(row_start);

            let raw_w: f64 = tracks.columns[col_start..col_end].iter().sum::<f64>()
                + column_gap * col_span_actual.saturating_sub(1) as f64;
            let raw_h: f64 = tracks.rows[row_start..row_end].iter().sum::<f64>()
                + row_gap * row_span_actual.saturating_sub(1) as f64;

            let width  = raw_w.max(child.min_width).min(child.max_width);
            let height = raw_h.max(child.min_height).min(child.max_height);

            Some(ChildGridLayout {
                id: child.id,
                width,
                height,
                row: placement.row_start,
                col: placement.col_start,
                row_span: placement.row_span,
                col_span: placement.col_span,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// High-level entry point
// ---------------------------------------------------------------------------

/// Run the full grid layout data pipeline for a container.
///
/// Combines `resolve_tracks` + `auto_place_children` + `compute_child_sizes`
/// into a single call.
///
/// # Returns
/// `(ResolvedTracks, Vec<ChildGridLayout>)` — the resolved track sizes and
/// the per-child layout data.
pub fn calc_grid_layout_data(
    container: &GridContainer,
    children: &[ChildShape],
    available_width: f64,
    available_height: f64,
) -> (ResolvedTracks, Vec<ChildGridLayout>) {
    let column_sizes = resolve_tracks(
        container,
        available_width,
        container.column_gap,
        &container.columns,
        true,
    );
    let row_sizes = resolve_tracks(
        container,
        available_height,
        container.row_gap,
        &container.rows,
        false,
    );

    let tracks = ResolvedTracks {
        columns: column_sizes,
        rows: row_sizes,
    };

    let placements = auto_place_children(children, container);
    let layouts = compute_child_sizes(&placements, &tracks, children, container.column_gap, container.row_gap);

    (tracks, layouts)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::params::{
        AlignContent, GridCell, GridContainer, GridDirection, GridPosition, GridTrack,
        JustifyContent,
    };

    fn container_with_tracks(cols: Vec<GridTrack>, rows: Vec<GridTrack>) -> GridContainer {
        let mut c = GridContainer::default();
        c.columns = cols;
        c.rows = rows;
        c
    }

    // ------------------------------------------------------------------
    // resolve_tracks — fixed
    // ------------------------------------------------------------------

    #[test]
    fn test_resolve_all_fixed() {
        let c = GridContainer::default();
        let tracks = vec![GridTrack::fixed(100.0), GridTrack::fixed(200.0)];
        let sizes = resolve_tracks(&c, 500.0, 0.0, &tracks, true);
        assert_eq!(sizes, vec![100.0, 200.0]);
    }

    #[test]
    fn test_resolve_fixed_gap_is_not_subtracted() {
        // Gap does not change individual track sizes for fixed tracks
        let c = GridContainer::default();
        let tracks = vec![GridTrack::fixed(100.0), GridTrack::fixed(100.0)];
        let sizes = resolve_tracks(&c, 300.0, 20.0, &tracks, true);
        assert_eq!(sizes, vec![100.0, 100.0]);
    }

    // ------------------------------------------------------------------
    // resolve_tracks — flex / fr
    // ------------------------------------------------------------------

    #[test]
    fn test_resolve_all_flex_equal_fr() {
        // container 900, gap 0, three 1fr tracks → each 300
        let c = GridContainer::default();
        let tracks = vec![GridTrack::flex(1.0), GridTrack::flex(1.0), GridTrack::flex(1.0)];
        let sizes = resolve_tracks(&c, 900.0, 0.0, &tracks, true);
        assert!((sizes[0] - 300.0).abs() < 1e-9);
        assert!((sizes[1] - 300.0).abs() < 1e-9);
        assert!((sizes[2] - 300.0).abs() < 1e-9);
    }

    #[test]
    fn test_resolve_flex_unequal_fr() {
        // container 800, gap 0 → [1fr, 2fr, 1fr] → [200, 400, 200]
        let c = GridContainer::default();
        let tracks = vec![GridTrack::flex(1.0), GridTrack::flex(2.0), GridTrack::flex(1.0)];
        let sizes = resolve_tracks(&c, 800.0, 0.0, &tracks, true);
        assert!((sizes[0] - 200.0).abs() < 1e-9, "expected 200, got {}", sizes[0]);
        assert!((sizes[1] - 400.0).abs() < 1e-9, "expected 400, got {}", sizes[1]);
        assert!((sizes[2] - 200.0).abs() < 1e-9, "expected 200, got {}", sizes[2]);
    }

    #[test]
    fn test_resolve_mixed_fixed_flex() {
        // container 700, gap 0 → [100px, 1fr, 2fr]
        // free = 700 - 100 = 600; fr_value = 600/3 = 200 → [100, 200, 400]
        let c = GridContainer::default();
        let tracks = vec![
            GridTrack::fixed(100.0),
            GridTrack::flex(1.0),
            GridTrack::flex(2.0),
        ];
        let sizes = resolve_tracks(&c, 700.0, 0.0, &tracks, true);
        assert!((sizes[0] - 100.0).abs() < 1e-9);
        assert!((sizes[1] - 200.0).abs() < 1e-9, "expected 200, got {}", sizes[1]);
        assert!((sizes[2] - 400.0).abs() < 1e-9, "expected 400, got {}", sizes[2]);
    }

    // ------------------------------------------------------------------
    // resolve_tracks — percent
    // ------------------------------------------------------------------

    #[test]
    fn test_resolve_percent() {
        let c = GridContainer::default();
        let tracks = vec![GridTrack::percent(50.0)];
        let sizes = resolve_tracks(&c, 800.0, 0.0, &tracks, true);
        assert!((sizes[0] - 400.0).abs() < 1e-9);
    }

    #[test]
    fn test_resolve_percent_two_tracks() {
        let c = GridContainer::default();
        let tracks = vec![GridTrack::percent(25.0), GridTrack::percent(75.0)];
        let sizes = resolve_tracks(&c, 400.0, 0.0, &tracks, true);
        assert!((sizes[0] - 100.0).abs() < 1e-9);
        assert!((sizes[1] - 300.0).abs() < 1e-9);
    }

    // ------------------------------------------------------------------
    // resolve_tracks — auto stretch
    // ------------------------------------------------------------------

    #[test]
    fn test_resolve_auto_stretch_columns() {
        // justify-content: stretch → distribute free space to auto tracks
        let mut c = GridContainer::default();
        c.justify_content = JustifyContent::Stretch;
        let tracks = vec![GridTrack::fixed(100.0), GridTrack::auto()];
        // container 300, gap 0 → fixed=100, free=200 → auto=200
        let sizes = resolve_tracks(&c, 300.0, 0.0, &tracks, true);
        assert!((sizes[0] - 100.0).abs() < 1e-9);
        assert!((sizes[1] - 200.0).abs() < 1e-9, "expected 200, got {}", sizes[1]);
    }

    #[test]
    fn test_resolve_auto_no_stretch_is_zero() {
        // Without stretch, auto tracks get 0 (content sizing deferred)
        let c = GridContainer::default(); // default justify_content = Start
        let tracks = vec![GridTrack::auto()];
        let sizes = resolve_tracks(&c, 300.0, 0.0, &tracks, true);
        assert_eq!(sizes[0], 0.0);
    }

    // ------------------------------------------------------------------
    // resolve_tracks — gaps affect fr free space
    // ------------------------------------------------------------------

    #[test]
    fn test_resolve_flex_accounts_for_gap() {
        // container 500, gap 20, two 1fr tracks
        // total_gap = 20 * (2-1) = 20
        // free = 500 - 0(fixed) - 20(gap) = 480 → each fr = 240
        let c = GridContainer::default();
        let tracks = vec![GridTrack::flex(1.0), GridTrack::flex(1.0)];
        let sizes = resolve_tracks(&c, 500.0, 20.0, &tracks, true);
        assert!((sizes[0] - 240.0).abs() < 1e-9, "expected 240, got {}", sizes[0]);
        assert!((sizes[1] - 240.0).abs() < 1e-9);
    }

    // ------------------------------------------------------------------
    // auto_place_children — explicit cells
    // ------------------------------------------------------------------

    #[test]
    fn test_auto_place_explicit_cell() {
        // Child has an explicit cell at (2, 3)
        let mut container = container_with_tracks(
            vec![GridTrack::fixed(100.0); 3],
            vec![GridTrack::fixed(80.0); 3],
        );
        let cell = GridCell::new(10, 2, 3).with_shape(42);
        container.cells.insert(10, cell);

        let children = vec![ChildShape::new(42)];
        let placements = auto_place_children(&children, &container);

        let p = placements.get(&42).unwrap();
        assert_eq!(p.row_start, 2);
        assert_eq!(p.col_start, 3);
        assert_eq!(p.row_span, 1);
        assert_eq!(p.col_span, 1);
    }

    // ------------------------------------------------------------------
    // auto_place_children — sequential fill
    // ------------------------------------------------------------------

    #[test]
    fn test_auto_place_sequential_2x2() {
        // 4 auto children in a 2×2 grid → each gets a unique cell
        let container = container_with_tracks(
            vec![GridTrack::fixed(100.0), GridTrack::fixed(100.0)],
            vec![GridTrack::fixed(80.0), GridTrack::fixed(80.0)],
        );
        let children = vec![
            ChildShape::new(1),
            ChildShape::new(2),
            ChildShape::new(3),
            ChildShape::new(4),
        ];
        let placements = auto_place_children(&children, &container);

        // Collect (row, col) pairs, must be 4 distinct positions
        let mut positions: Vec<(usize, usize)> = placements
            .values()
            .map(|p| (p.row_start, p.col_start))
            .collect();
        positions.sort();
        assert_eq!(
            positions,
            vec![(1, 1), (1, 2), (2, 1), (2, 2)],
            "all 4 cells should be occupied"
        );
    }

    #[test]
    fn test_auto_place_row_major_order() {
        // 3 auto children in 2×3 grid → fills row 1 first
        let container = container_with_tracks(
            vec![GridTrack::fixed(100.0); 3],
            vec![GridTrack::fixed(80.0); 2],
        );
        let children: Vec<ChildShape> = (1u64..=3).map(ChildShape::new).collect();
        let placements = auto_place_children(&children, &container);

        let p1 = placements.get(&1).unwrap();
        let p2 = placements.get(&2).unwrap();
        let p3 = placements.get(&3).unwrap();

        // All three should land in row 1 (columns 1, 2, 3)
        assert_eq!(p1.row_start, 1);
        assert_eq!(p2.row_start, 1);
        assert_eq!(p3.row_start, 1);
        assert_eq!(p1.col_start, 1);
        assert_eq!(p2.col_start, 2);
        assert_eq!(p3.col_start, 3);
    }

    #[test]
    fn test_auto_place_column_major_order() {
        // Column direction: fills col 1 first (row 1, row 2), then col 2
        let mut container = container_with_tracks(
            vec![GridTrack::fixed(100.0); 2],
            vec![GridTrack::fixed(80.0); 2],
        );
        container.direction = GridDirection::Column;

        let children: Vec<ChildShape> = (1u64..=3).map(ChildShape::new).collect();
        let placements = auto_place_children(&children, &container);

        let p1 = placements.get(&1).unwrap();
        let p2 = placements.get(&2).unwrap();
        let p3 = placements.get(&3).unwrap();

        // Column-major: child1 → (1,1), child2 → (2,1), child3 → (1,2)
        assert_eq!((p1.row_start, p1.col_start), (1, 1));
        assert_eq!((p2.row_start, p2.col_start), (2, 1));
        assert_eq!((p3.row_start, p3.col_start), (1, 2));
    }

    // ------------------------------------------------------------------
    // auto_place_children — explicit cells block auto slots
    // ------------------------------------------------------------------

    #[test]
    fn test_auto_place_skips_occupied_cells() {
        // Explicitly place one child at (1,1); auto child should go to (1,2)
        let mut container = container_with_tracks(
            vec![GridTrack::fixed(100.0); 2],
            vec![GridTrack::fixed(80.0)],
        );
        let cell = GridCell::new(99, 1, 1).with_shape(1);
        container.cells.insert(99, cell);

        let children = vec![ChildShape::new(1), ChildShape::new(2)];
        let placements = auto_place_children(&children, &container);

        let p2 = placements.get(&2).unwrap();
        assert_eq!(p2.col_start, 2, "auto child should skip occupied (1,1)");
    }

    // ------------------------------------------------------------------
    // compute_child_sizes — single-track
    // ------------------------------------------------------------------

    #[test]
    fn test_child_size_single_cell() {
        let tracks = ResolvedTracks {
            columns: vec![120.0],
            rows: vec![80.0],
        };
        let mut placements = HashMap::new();
        placements.insert(1u64, GridPlacement { row_start: 1, row_span: 1, col_start: 1, col_span: 1 });

        let children = vec![ChildShape::new(1)];
        let layouts = compute_child_sizes(&placements, &tracks, &children, 0.0, 0.0);

        assert_eq!(layouts[0].width,  120.0);
        assert_eq!(layouts[0].height, 80.0);
    }

    // ------------------------------------------------------------------
    // compute_child_sizes — span-2 columns with gap
    // ------------------------------------------------------------------

    #[test]
    fn test_child_size_span2_columns_with_gap() {
        // tracks: [100, 200], gap 10 → span-2 width = 100 + 10 + 200 = 310
        let tracks = ResolvedTracks {
            columns: vec![100.0, 200.0],
            rows: vec![80.0],
        };
        let mut placements = HashMap::new();
        placements.insert(1u64, GridPlacement { row_start: 1, row_span: 1, col_start: 1, col_span: 2 });

        let children = vec![ChildShape::new(1)];
        let layouts = compute_child_sizes(&placements, &tracks, &children, 10.0, 0.0);

        assert_eq!(layouts[0].width, 310.0);
    }

    #[test]
    fn test_child_size_span2_rows_with_gap() {
        let tracks = ResolvedTracks {
            columns: vec![100.0],
            rows: vec![80.0, 80.0],
        };
        let mut placements = HashMap::new();
        placements.insert(1u64, GridPlacement { row_start: 1, row_span: 2, col_start: 1, col_span: 1 });

        let children = vec![ChildShape::new(1)];
        let layouts = compute_child_sizes(&placements, &tracks, &children, 0.0, 16.0);

        // 80 + 16 + 80 = 176
        assert_eq!(layouts[0].height, 176.0);
    }

    // ------------------------------------------------------------------
    // compute_child_sizes — min/max clamping
    // ------------------------------------------------------------------

    #[test]
    fn test_child_size_clamped_by_max_width() {
        let tracks = ResolvedTracks {
            columns: vec![500.0],
            rows: vec![80.0],
        };
        let mut placements = HashMap::new();
        placements.insert(1u64, GridPlacement { row_start: 1, row_span: 1, col_start: 1, col_span: 1 });

        let children = vec![
            ChildShape::new(1).with_width_bounds(0.0, 300.0),
        ];
        let layouts = compute_child_sizes(&placements, &tracks, &children, 0.0, 0.0);

        assert_eq!(layouts[0].width, 300.0, "width should be clamped to max");
    }

    #[test]
    fn test_child_size_clamped_by_min_width() {
        let tracks = ResolvedTracks {
            columns: vec![50.0],
            rows: vec![80.0],
        };
        let mut placements = HashMap::new();
        placements.insert(1u64, GridPlacement { row_start: 1, row_span: 1, col_start: 1, col_span: 1 });

        let children = vec![
            ChildShape::new(1).with_width_bounds(100.0, f64::INFINITY),
        ];
        let layouts = compute_child_sizes(&placements, &tracks, &children, 0.0, 0.0);

        assert_eq!(layouts[0].width, 100.0, "width should be raised to min");
    }

    // ------------------------------------------------------------------
    // calc_grid_layout_data — integration
    // ------------------------------------------------------------------

    #[test]
    fn test_integration_2x2_fixed() {
        let mut container = container_with_tracks(
            vec![GridTrack::fixed(100.0), GridTrack::fixed(200.0)],
            vec![GridTrack::fixed(80.0), GridTrack::fixed(120.0)],
        );
        container.column_gap = 10.0;
        container.row_gap = 8.0;

        let children: Vec<ChildShape> = (1u64..=4).map(ChildShape::new).collect();
        let (tracks, layouts) = calc_grid_layout_data(&container, &children, 320.0, 216.0);

        assert_eq!(tracks.columns, vec![100.0, 200.0]);
        assert_eq!(tracks.rows, vec![80.0, 120.0]);
        assert_eq!(layouts.len(), 4);

        // Row 1 children → height 80; row 2 → height 120
        for l in &layouts {
            if l.row == 1 && l.col == 1 { assert_eq!(l.width, 100.0); assert_eq!(l.height, 80.0); }
            if l.row == 1 && l.col == 2 { assert_eq!(l.width, 200.0); assert_eq!(l.height, 80.0); }
            if l.row == 2 && l.col == 1 { assert_eq!(l.width, 100.0); assert_eq!(l.height, 120.0); }
            if l.row == 2 && l.col == 2 { assert_eq!(l.width, 200.0); assert_eq!(l.height, 120.0); }
        }
    }

    #[test]
    fn test_integration_fr_columns() {
        // container 800 wide, two 1fr columns — each should be 400
        let container = container_with_tracks(
            vec![GridTrack::flex(1.0), GridTrack::flex(1.0)],
            vec![GridTrack::fixed(100.0)],
        );

        let children: Vec<ChildShape> = (1u64..=2).map(ChildShape::new).collect();
        let (tracks, layouts) = calc_grid_layout_data(&container, &children, 800.0, 100.0);

        assert!((tracks.columns[0] - 400.0).abs() < 1e-9);
        assert!((tracks.columns[1] - 400.0).abs() < 1e-9);
        for l in &layouts {
            assert!((l.width - 400.0).abs() < 1e-9, "child {} width = {}", l.id, l.width);
        }
    }
}
