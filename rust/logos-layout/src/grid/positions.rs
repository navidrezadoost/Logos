// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) KALEIDOS INC
//
// Port of: common/src/app/common/geom/shapes/grid_layout/positions.cljc
//
// This module converts resolved track sizes + child placements into final
// pixel positions.  The Clojure original uses a 2-D vector algebra layer
// (`gpt`, `gpo`) to support rotated containers; here we compute the
// axis-aligned case which is sufficient for the pure-data pipeline.

use crate::grid::layout_data::{ChildGridLayout, ResolvedTracks};
use crate::grid::params::{AlignItems, GridContainer, JustifyItems, TrackAlignSelf, TrackJustifySelf};

// =============================================================================
// Types
// =============================================================================

/// Margins around a child (CSS order: top / right / bottom / left).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChildMargins {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

impl Default for ChildMargins {
    fn default() -> Self {
        ChildMargins { top: 0.0, right: 0.0, bottom: 0.0, left: 0.0 }
    }
}

impl ChildMargins {
    pub fn new(top: f64, right: f64, bottom: f64, left: f64) -> Self {
        ChildMargins { top, right, bottom, left }
    }
    pub fn uniform(v: f64) -> Self {
        ChildMargins { top: v, right: v, bottom: v, left: v }
    }
}

/// Axis-aligned bounding box for one grid cell (or multi-cell span).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl CellBounds {
    /// Right edge.
    #[inline]
    pub fn right(&self) -> f64 { self.x + self.width }
    /// Bottom edge.
    #[inline]
    pub fn bottom(&self) -> f64 { self.y + self.height }
    /// Horizontal centre.
    #[inline]
    pub fn center_x(&self) -> f64 { self.x + self.width / 2.0 }
    /// Vertical centre.
    #[inline]
    pub fn center_y(&self) -> f64 { self.y + self.height / 2.0 }
}

/// Final positioned child (pixel rectangle).
#[derive(Debug, Clone, PartialEq)]
pub struct PositionedChild {
    pub id: u64,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// Grid column (1-based) this child starts at.
    pub col: usize,
    /// Grid row (1-based) this child starts at.
    pub row: usize,
}

// =============================================================================
// Track start positions
// =============================================================================

/// Converts a list of track pixel sizes into track *start* offsets.
///
/// `padding_start` is the container padding on the leading edge
/// (left for columns, top for rows).
///
/// # Example
///
/// ```
/// use logos_layout::grid::compute_track_starts;
///
/// let sizes  = vec![100.0, 200.0, 100.0];
/// let starts = compute_track_starts(&sizes, 10.0, 5.0);
/// // padding(5) | 100 | gap(10) | 200 | gap(10) | 100
/// assert_eq!(starts, vec![5.0, 115.0, 325.0]);
/// ```
pub fn compute_track_starts(sizes: &[f64], gap: f64, padding_start: f64) -> Vec<f64> {
    let mut starts = Vec::with_capacity(sizes.len());
    let mut cursor = padding_start;
    for (i, &size) in sizes.iter().enumerate() {
        starts.push(cursor);
        cursor += size;
        if i + 1 < sizes.len() {
            cursor += gap;
        }
    }
    starts
}

// =============================================================================
// Cell bounds
// =============================================================================

/// Computes the pixel bounding box for the cell(s) covered by `child`.
///
/// `col_starts` and `row_starts` are the leading-edge offsets produced by
/// [`compute_track_starts`].
///
/// Returns `None` when the placement is out of range for the resolved tracks.
pub fn cell_bounds_for_child(
    child: &ChildGridLayout,
    col_starts: &[f64],
    row_starts: &[f64],
    col_sizes: &[f64],
    row_sizes: &[f64],
    col_gap: f64,
    row_gap: f64,
) -> Option<CellBounds> {
    // child.row / child.col are 1-based
    let col0 = child.col.checked_sub(1)?;
    let row0 = child.row.checked_sub(1)?;

    if col0 >= col_starts.len() || row0 >= row_starts.len() {
        return None;
    }

    let x = col_starts[col0];
    let y = row_starts[row0];

    // Cell width = sum of all spanned column sizes + gaps between them.
    let col_end = (col0 + child.col_span).min(col_sizes.len());
    let width: f64 = col_sizes[col0..col_end].iter().sum::<f64>()
        + col_gap * (col_end.saturating_sub(col0).saturating_sub(1)) as f64;

    // Cell height = sum of all spanned row sizes + gaps between them.
    let row_end = (row0 + child.row_span).min(row_sizes.len());
    let height: f64 = row_sizes[row0..row_end].iter().sum::<f64>()
        + row_gap * (row_end.saturating_sub(row0).saturating_sub(1)) as f64;

    Some(CellBounds { x, y, width, height })
}

// =============================================================================
// Child (x, y) within its cell
// =============================================================================

/// Resolves the horizontal `x` for a child inside `cell` given a justify mode
/// and optional per-cell override.
fn child_x(
    cell: &CellBounds,
    child_width: f64,
    justify_items: &JustifyItems,
    justify_self: &TrackJustifySelf,
    margins: &ChildMargins,
) -> f64 {
    // Per-cell override takes precedence when it is not Auto.
    let effective = match justify_self {
        TrackJustifySelf::Start   => JustifyItems::Start,
        TrackJustifySelf::Center  => JustifyItems::Center,
        TrackJustifySelf::End     => JustifyItems::End,
        TrackJustifySelf::Stretch => JustifyItems::Stretch,
        TrackJustifySelf::Auto    => *justify_items,
    };

    match effective {
        JustifyItems::End => {
            // Right-align: right edge of child touches right edge of cell minus right margin
            cell.right() - child_width - margins.right
        }
        JustifyItems::Center => {
            // Centre: midpoint of child = midpoint of cell, adjusted by margin imbalance
            cell.center_x() - child_width / 2.0 + (margins.left - margins.right) / 2.0
        }
        // Start | Stretch (stretch sizing is handled upstream; here we just position)
        _ => cell.x + margins.left,
    }
}

/// Resolves the vertical `y` for a child inside `cell` given an align mode
/// and optional per-cell override.
fn child_y(
    cell: &CellBounds,
    child_height: f64,
    align_items: &AlignItems,
    align_self: &TrackAlignSelf,
    margins: &ChildMargins,
) -> f64 {
    let effective = match align_self {
        TrackAlignSelf::Start   => AlignItems::Start,
        TrackAlignSelf::Center  => AlignItems::Center,
        TrackAlignSelf::End     => AlignItems::End,
        TrackAlignSelf::Stretch => AlignItems::Stretch,
        TrackAlignSelf::Auto    => *align_items,
    };

    match effective {
        AlignItems::End => cell.bottom() - child_height - margins.bottom,
        AlignItems::Center => {
            cell.center_y() - child_height / 2.0 + (margins.top - margins.bottom) / 2.0
        }
        _ => cell.y + margins.top,
    }
}

// =============================================================================
// High-level entry point
// =============================================================================

/// Compute final pixel `(x, y)` for every child.
///
/// `children` must be the `Vec<ChildGridLayout>` produced by
/// `calc_grid_layout_data` (they carry width/height already clamped to
/// min/max).  `margins` is a parallel slice — pass `&[]` (or a shorter
/// slice) to use zero margins for all children.
///
/// Returns a [`PositionedChild`] for each input child.
pub fn compute_positions(
    container: &GridContainer,
    resolved: &ResolvedTracks,
    children: &[ChildGridLayout],
    margins: &[ChildMargins],
) -> Vec<PositionedChild> {
    let (pad_top, _pad_right, _pad_bottom, pad_left) = container.padding;

    let col_starts = compute_track_starts(&resolved.columns, container.column_gap, pad_left);
    let row_starts = compute_track_starts(&resolved.rows, container.row_gap, pad_top);

    let default_margins = ChildMargins::default();

    children
        .iter()
        .enumerate()
        .map(|(i, child)| {
            let m = margins.get(i).unwrap_or(&default_margins);

            // compute cell bounds from starts (child.width/height already set)
            let cell = cell_bounds_for_child(
                child,
                &col_starts,
                &row_starts,
                &resolved.columns,
                &resolved.rows,
                container.column_gap,
                container.row_gap,
            );

            let (x, y) = if let Some(cb) = cell {
                // Look up per-cell alignment overrides from the container's cells map.
                let (align_self, justify_self) = container
                    .cells
                    .get(&child.id)
                    .map(|c| (c.align_self, c.justify_self))
                    .unwrap_or((TrackAlignSelf::Auto, TrackJustifySelf::Auto));

                let cx = child_x(&cb, child.width, &container.justify_items, &justify_self, m);
                let cy = child_y(&cb, child.height, &container.align_items, &align_self, m);
                (cx, cy)
            } else {
                (0.0, 0.0)
            };

            PositionedChild {
                id: child.id,
                x,
                y,
                width: child.width,
                height: child.height,
                col: child.col,
                row: child.row,
            }
        })
        .collect()
}

/// Given a pixel `(px, py)` coordinate relative to the container origin,
/// return the `(row, col)` (both 1-based) of the nearest containing cell.
///
/// Mirrors Clojure `get-position-grid-coord`.  When the point falls outside
/// the grid it is snapped to the closest track boundary.
pub fn get_cell_at_position(
    resolved: &ResolvedTracks,
    column_gap: f64,
    row_gap: f64,
    padding_left: f64,
    padding_top: f64,
    px: f64,
    py: f64,
) -> Option<(usize, usize)> {
    if resolved.columns.is_empty() || resolved.rows.is_empty() {
        return None;
    }

    let col_starts = compute_track_starts(&resolved.columns, column_gap, padding_left);
    let row_starts = compute_track_starts(&resolved.rows, row_gap, padding_top);

    let find_track = |starts: &[f64], sizes: &[f64], coord: f64| -> usize {
        // Find the track that contains coord, or the closest one.
        let mut best_idx = 0usize;
        let mut best_dist = f64::INFINITY;

        for (i, (&start, &size)) in starts.iter().zip(sizes.iter()).enumerate() {
            let end = start + size;
            if coord >= start && coord <= end {
                return i;
            }
            let dist = (coord - start).abs().min((coord - end).abs());
            if dist < best_dist {
                best_dist = dist;
                best_idx = i;
            }
        }
        best_idx
    };

    let col_idx = find_track(&col_starts, &resolved.columns, px);
    let row_idx = find_track(&row_starts, &resolved.rows, py);

    Some((row_idx + 1, col_idx + 1))
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::layout_data::{ChildGridLayout, ResolvedTracks};
    use crate::grid::params::{AlignItems, GridContainer, JustifyItems};

    fn simple_container(cols: Vec<f64>, rows: Vec<f64>) -> GridContainer {
        use crate::grid::params::{GridDirection, AlignContent, JustifyContent};
        GridContainer {
            columns: vec![],
            rows: vec![],
            column_gap: 0.0,
            row_gap: 0.0,
            padding: (0.0, 0.0, 0.0, 0.0),
            justify_items: JustifyItems::Start,
            align_items: AlignItems::Start,
            justify_content: JustifyContent::Start,
            align_content: AlignContent::Start,
            direction: GridDirection::Row,
            cells: std::collections::HashMap::new(),
        }
    }

    fn resolved(cols: Vec<f64>, rows: Vec<f64>) -> ResolvedTracks {
        ResolvedTracks { columns: cols, rows }
    }

    fn child(id: u64, col: usize, row: usize, w: f64, h: f64) -> ChildGridLayout {
        ChildGridLayout { id, width: w, height: h, row, col, row_span: 1, col_span: 1 }
    }

    // -----------------------------------------------------------------------
    // compute_track_starts
    // -----------------------------------------------------------------------

    #[test]
    fn track_starts_no_gap() {
        let starts = compute_track_starts(&[100.0, 200.0, 150.0], 0.0, 0.0);
        assert_eq!(starts, vec![0.0, 100.0, 300.0]);
    }

    #[test]
    fn track_starts_with_gap() {
        let starts = compute_track_starts(&[100.0, 200.0, 100.0], 10.0, 0.0);
        assert_eq!(starts, vec![0.0, 110.0, 320.0]);
    }

    #[test]
    fn track_starts_with_padding() {
        let starts = compute_track_starts(&[100.0, 200.0], 10.0, 20.0);
        // 20, 20+100+10=130
        assert_eq!(starts, vec![20.0, 130.0]);
    }

    #[test]
    fn track_starts_single_track() {
        let starts = compute_track_starts(&[300.0], 10.0, 5.0);
        assert_eq!(starts, vec![5.0]);
    }

    #[test]
    fn track_starts_empty() {
        let starts = compute_track_starts(&[], 10.0, 5.0);
        assert!(starts.is_empty());
    }

    // -----------------------------------------------------------------------
    // compute_positions — basic alignment
    // -----------------------------------------------------------------------

    #[test]
    fn positions_start_align_no_margins() {
        let container = simple_container(vec![], vec![]);
        let res = resolved(vec![200.0, 200.0], vec![100.0, 100.0]);
        let children = vec![
            child(1, 1, 1, 50.0, 40.0),
            child(2, 2, 2, 80.0, 60.0),
        ];
        let positioned = compute_positions(&container, &res, &children, &[]);

        // child 1: col_start=0→x=0, row_start=0→y=0
        assert_eq!(positioned[0].x, 0.0);
        assert_eq!(positioned[0].y, 0.0);

        // child 2: col_start[1]=200, row_start[1]=100
        assert_eq!(positioned[1].x, 200.0);
        assert_eq!(positioned[1].y, 100.0);
    }

    #[test]
    fn positions_with_gaps_and_padding() {
        use crate::grid::params::{AlignContent, GridDirection, JustifyContent};
        let container = GridContainer {
            columns: vec![],
            rows: vec![],
            column_gap: 10.0,
            row_gap: 8.0,
            padding: (5.0, 5.0, 5.0, 5.0),
            justify_items: JustifyItems::Start,
            align_items: AlignItems::Start,
            justify_content: JustifyContent::Start,
            align_content: AlignContent::Start,
            direction: GridDirection::Row,
            cells: std::collections::HashMap::new(),
        };
        let res = resolved(vec![100.0, 100.0], vec![80.0, 80.0]);
        let children = vec![child(1, 2, 2, 40.0, 30.0)];
        let positioned = compute_positions(&container, &res, &children, &[]);

        // col_starts: 5 | 100 | gap(10) → starts[1] = 5 + 100 + 10 = 115
        // row_starts: 5 | 80 | gap(8)  → starts[1] = 5 + 80 + 8  = 93
        assert_eq!(positioned[0].x, 115.0);
        assert_eq!(positioned[0].y, 93.0);
    }

    #[test]
    fn positions_center_justify() {
        use crate::grid::params::{AlignContent, GridDirection, JustifyContent};
        let container = GridContainer {
            columns: vec![],
            rows: vec![],
            column_gap: 0.0,
            row_gap: 0.0,
            padding: (0.0, 0.0, 0.0, 0.0),
            justify_items: JustifyItems::Center,
            align_items: AlignItems::Start,
            justify_content: JustifyContent::Start,
            align_content: AlignContent::Start,
            direction: GridDirection::Row,
            cells: std::collections::HashMap::new(),
        };
        let res = resolved(vec![200.0], vec![100.0]);
        // child 60px wide in 200px column → x = (200 - 60) / 2 = 70
        let children = vec![child(1, 1, 1, 60.0, 40.0)];
        let positioned = compute_positions(&container, &res, &children, &[]);
        assert_eq!(positioned[0].x, 70.0);
    }

    #[test]
    fn positions_end_align() {
        use crate::grid::params::{AlignContent, GridDirection, JustifyContent};
        let container = GridContainer {
            columns: vec![],
            rows: vec![],
            column_gap: 0.0,
            row_gap: 0.0,
            padding: (0.0, 0.0, 0.0, 0.0),
            justify_items: JustifyItems::End,
            align_items: AlignItems::End,
            justify_content: JustifyContent::Start,
            align_content: AlignContent::Start,
            direction: GridDirection::Row,
            cells: std::collections::HashMap::new(),
        };
        let res = resolved(vec![200.0], vec![100.0]);
        let children = vec![child(1, 1, 1, 60.0, 40.0)];
        let positioned = compute_positions(&container, &res, &children, &[]);
        // x = cell.right(200) - child_width(60) - margin_right(0) = 140
        // y = cell.bottom(100) - child_height(40) - margin_bottom(0) = 60
        assert_eq!(positioned[0].x, 140.0);
        assert_eq!(positioned[0].y, 60.0);
    }

    #[test]
    fn positions_with_margins() {
        let container = simple_container(vec![], vec![]);
        let res = resolved(vec![200.0], vec![100.0]);
        let children = vec![child(1, 1, 1, 60.0, 40.0)];
        let margins = vec![ChildMargins::new(5.0, 10.0, 5.0, 10.0)];
        let positioned = compute_positions(&container, &res, &children, &margins);
        // start justify: x = cell.x(0) + margin_left(10) = 10
        // start align:   y = cell.y(0) + margin_top(5)   = 5
        assert_eq!(positioned[0].x, 10.0);
        assert_eq!(positioned[0].y, 5.0);
    }

    #[test]
    fn positions_center_with_margins() {
        use crate::grid::params::{AlignContent, GridDirection, JustifyContent};
        let container = GridContainer {
            columns: vec![],
            rows: vec![],
            column_gap: 0.0,
            row_gap: 0.0,
            padding: (0.0, 0.0, 0.0, 0.0),
            justify_items: JustifyItems::Center,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Start,
            align_content: AlignContent::Start,
            direction: GridDirection::Row,
            cells: std::collections::HashMap::new(),
        };
        let res = resolved(vec![200.0], vec![100.0]);
        let children = vec![child(1, 1, 1, 60.0, 40.0)];
        // left=8, right=2 → margin imbalance 6 → x = 100 - 30 + 3 = 73
        let margins = vec![ChildMargins::new(10.0, 2.0, 0.0, 8.0)];
        let positioned = compute_positions(&container, &res, &children, &margins);
        // center_x = 100, x = 100 - 30 + (8-2)/2 = 73
        assert_eq!(positioned[0].x, 73.0);
        // center_y = 50, y = 50 - 20 + (10-0)/2 = 35
        assert_eq!(positioned[0].y, 35.0);
    }

    // -----------------------------------------------------------------------
    // get_cell_at_position
    // -----------------------------------------------------------------------

    #[test]
    fn cell_at_position_inside_first_cell() {
        let res = resolved(vec![100.0, 200.0], vec![80.0, 80.0]);
        let result = get_cell_at_position(&res, 0.0, 0.0, 0.0, 0.0, 50.0, 40.0);
        assert_eq!(result, Some((1, 1)));
    }

    #[test]
    fn cell_at_position_inside_second_column() {
        let res = resolved(vec![100.0, 200.0], vec![80.0, 80.0]);
        let result = get_cell_at_position(&res, 0.0, 0.0, 0.0, 0.0, 150.0, 40.0);
        assert_eq!(result, Some((1, 2)));
    }

    #[test]
    fn cell_at_position_with_gap() {
        let res = resolved(vec![100.0, 100.0], vec![100.0]);
        // col_starts: [0, 110]  (gap=10)
        // point at x=50 → col 1, x=120 → col 2
        let r1 = get_cell_at_position(&res, 10.0, 0.0, 0.0, 0.0, 50.0, 50.0);
        let r2 = get_cell_at_position(&res, 10.0, 0.0, 0.0, 0.0, 120.0, 50.0);
        assert_eq!(r1, Some((1, 1)));
        assert_eq!(r2, Some((1, 2)));
    }

    #[test]
    fn cell_at_position_clamps_outside() {
        let res = resolved(vec![100.0, 100.0], vec![100.0, 100.0]);
        // x=500 is far outside → should snap to col 2 (rightmost)
        let result = get_cell_at_position(&res, 0.0, 0.0, 0.0, 0.0, 500.0, 50.0);
        assert_eq!(result, Some((1, 2)));
    }

    #[test]
    fn cell_at_position_empty_grid() {
        let res = resolved(vec![], vec![]);
        let result = get_cell_at_position(&res, 0.0, 0.0, 0.0, 0.0, 50.0, 50.0);
        assert!(result.is_none());
    }
}
