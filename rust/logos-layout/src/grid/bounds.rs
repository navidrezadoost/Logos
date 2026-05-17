// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) KALEIDOS INC
//
// Port of: common/src/app/common/geom/shapes/grid_layout/bounds.cljc
//
// Computes the outer bounding rectangle for a grid container from its
// resolved track sizes plus padding.  This mirrors `layout-content-bounds`
// from the Clojure source, simplified to the axis-aligned case.

use crate::grid::layout_data::ResolvedTracks;

// =============================================================================
// Types
// =============================================================================

/// Bounding rectangle for a grid container.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridBounds {
    /// Left edge (origin x).
    pub x: f64,
    /// Top edge (origin y).
    pub y: f64,
    /// Total width including padding.
    pub width: f64,
    /// Total height including padding.
    pub height: f64,
}

impl GridBounds {
    /// Right edge.
    #[inline]
    pub fn right(&self) -> f64 { self.x + self.width }
    /// Bottom edge.
    #[inline]
    pub fn bottom(&self) -> f64 { self.y + self.height }
}

// =============================================================================
// Bounds computation
// =============================================================================

/// Compute the total width occupied by `tracks` with `gap` between them.
///
/// Returns 0.0 for an empty track list.
///
/// # Example
///
/// ```
/// use logos_layout::grid::track_extent;
///
/// assert_eq!(track_extent(&[100.0, 200.0, 100.0], 10.0), 420.0);
/// assert_eq!(track_extent(&[300.0], 10.0), 300.0);
/// assert_eq!(track_extent(&[], 10.0), 0.0);
/// ```
pub fn track_extent(sizes: &[f64], gap: f64) -> f64 {
    if sizes.is_empty() {
        return 0.0;
    }
    let total: f64 = sizes.iter().sum();
    let gaps = gap * (sizes.len() as f64 - 1.0);
    total + gaps
}

/// Compute the bounding rectangle of a grid container.
///
/// `x` and `y` are the container's top-left origin in the parent coordinate
/// system.  `padding` is `(top, right, bottom, left)`.
///
/// For auto-sized containers the caller should set `x` / `y` to the known
/// container position and use the returned `width` / `height` as the new size.
///
/// # Example
///
/// ```
/// use logos_layout::grid::{compute_grid_bounds, ResolvedTracks};
///
/// let resolved = ResolvedTracks {
///     columns: vec![100.0, 200.0],
///     rows:    vec![80.0, 80.0],
/// };
/// // 10px padding on every side, no gap
/// let bounds = compute_grid_bounds(&resolved, 0.0, 0.0, (10.0, 10.0, 10.0, 10.0), 0.0, 0.0);
/// // width  = pad_left(10) + 100 + 200 + pad_right(10)  = 320
/// // height = pad_top(10)  +  80 +  80 + pad_bottom(10) = 180
/// assert_eq!(bounds.width,  320.0);
/// assert_eq!(bounds.height, 180.0);
/// ```
pub fn compute_grid_bounds(
    resolved: &ResolvedTracks,
    x: f64,
    y: f64,
    padding: (f64, f64, f64, f64),
    column_gap: f64,
    row_gap: f64,
) -> GridBounds {
    let (pad_top, pad_right, pad_bottom, pad_left) = padding;

    let content_width  = track_extent(&resolved.columns, column_gap);
    let content_height = track_extent(&resolved.rows, row_gap);

    GridBounds {
        x,
        y,
        width:  pad_left + content_width  + pad_right,
        height: pad_top  + content_height + pad_bottom,
    }
}

/// Compute pixel start positions of each track *including* a terminating
/// end-sentinel.  Useful for rendering grid lines.
///
/// Returns `sizes.len() + 1` entries:
/// `[pad, pad+s0, pad+s0+gap+s1, …, pad + total_extent]`
pub fn track_line_positions(sizes: &[f64], gap: f64, padding_start: f64) -> Vec<f64> {
    let mut positions = Vec::with_capacity(sizes.len() + 1);
    let mut cursor = padding_start;
    for (i, &size) in sizes.iter().enumerate() {
        positions.push(cursor);
        cursor += size;
        if i + 1 < sizes.len() {
            cursor += gap;
        }
    }
    positions.push(cursor); // end sentinel
    positions
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::layout_data::ResolvedTracks;

    fn res(cols: Vec<f64>, rows: Vec<f64>) -> ResolvedTracks {
        ResolvedTracks { columns: cols, rows }
    }

    // -----------------------------------------------------------------------
    // track_extent
    // -----------------------------------------------------------------------

    #[test]
    fn extent_single_track() {
        assert_eq!(track_extent(&[200.0], 10.0), 200.0);
    }

    #[test]
    fn extent_three_equal_tracks_with_gap() {
        // 3 × 100 + 2 × 10 = 320
        assert_eq!(track_extent(&[100.0, 100.0, 100.0], 10.0), 320.0);
    }

    #[test]
    fn extent_empty() {
        assert_eq!(track_extent(&[], 10.0), 0.0);
    }

    #[test]
    fn extent_no_gap() {
        assert_eq!(track_extent(&[50.0, 150.0, 200.0], 0.0), 400.0);
    }

    // -----------------------------------------------------------------------
    // compute_grid_bounds
    // -----------------------------------------------------------------------

    #[test]
    fn bounds_no_padding_no_gap() {
        let r = res(vec![100.0, 200.0, 100.0], vec![80.0, 80.0]);
        let b = compute_grid_bounds(&r, 0.0, 0.0, (0.0, 0.0, 0.0, 0.0), 0.0, 0.0);
        assert_eq!(b.width, 400.0);
        assert_eq!(b.height, 160.0);
    }

    #[test]
    fn bounds_with_uniform_padding() {
        let r = res(vec![100.0, 200.0], vec![80.0, 80.0]);
        let b = compute_grid_bounds(&r, 0.0, 0.0, (10.0, 10.0, 10.0, 10.0), 0.0, 0.0);
        // 10 + 300 + 10 = 320
        assert_eq!(b.width, 320.0);
        // 10 + 160 + 10 = 180
        assert_eq!(b.height, 180.0);
    }

    #[test]
    fn bounds_with_gap() {
        let r = res(vec![100.0, 100.0, 100.0], vec![80.0, 80.0]);
        // col extent = 300 + 2*10 = 320; row extent = 160 + 8 = 168
        let b = compute_grid_bounds(&r, 0.0, 0.0, (0.0, 0.0, 0.0, 0.0), 10.0, 8.0);
        assert_eq!(b.width, 320.0);
        assert_eq!(b.height, 168.0);
    }

    #[test]
    fn bounds_with_gap_and_padding() {
        let r = res(vec![100.0, 100.0], vec![80.0]);
        // col = 200 + 10 + padding 5+5 = 220;  row = 80 + padding 4+4 = 88
        let b = compute_grid_bounds(&r, 0.0, 0.0, (4.0, 5.0, 4.0, 5.0), 10.0, 0.0);
        assert_eq!(b.width, 220.0);
        assert_eq!(b.height, 88.0);
    }

    #[test]
    fn bounds_origin_preserved() {
        let r = res(vec![100.0], vec![100.0]);
        let b = compute_grid_bounds(&r, 50.0, 30.0, (0.0, 0.0, 0.0, 0.0), 0.0, 0.0);
        assert_eq!(b.x, 50.0);
        assert_eq!(b.y, 30.0);
        assert_eq!(b.right(), 150.0);
        assert_eq!(b.bottom(), 130.0);
    }

    #[test]
    fn bounds_empty_tracks() {
        let r = res(vec![], vec![]);
        let b = compute_grid_bounds(&r, 0.0, 0.0, (5.0, 5.0, 5.0, 5.0), 10.0, 10.0);
        assert_eq!(b.width, 10.0);
        assert_eq!(b.height, 10.0);
    }

    // -----------------------------------------------------------------------
    // track_line_positions
    // -----------------------------------------------------------------------

    #[test]
    fn line_positions_basic() {
        // 3 tracks of 100px each, gap 10, no padding
        let pos = track_line_positions(&[100.0, 100.0, 100.0], 10.0, 0.0);
        assert_eq!(pos, vec![0.0, 110.0, 220.0, 320.0]);
    }

    #[test]
    fn line_positions_with_padding() {
        let pos = track_line_positions(&[100.0, 200.0], 0.0, 20.0);
        assert_eq!(pos, vec![20.0, 120.0, 320.0]);
    }

    #[test]
    fn line_positions_single_track() {
        let pos = track_line_positions(&[150.0], 10.0, 5.0);
        assert_eq!(pos, vec![5.0, 155.0]);
    }

    #[test]
    fn line_positions_empty() {
        let pos = track_line_positions(&[], 10.0, 0.0);
        assert_eq!(pos, vec![0.0]);
    }
}
