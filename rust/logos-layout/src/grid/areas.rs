// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) KALEIDOS INC
//
// Port of: common/src/app/common/geom/shapes/grid_layout/areas.cljc
// Based on algorithm from:
// https://en.wikibooks.org/wiki/Algorithm_Implementation/Geometry/Rectangle_difference

/// A grid-coordinate rectangle described as (col, row, col_span, row_span).
///
/// Columns and rows are 1-based. `col_span` and `row_span` are always ≥ 1.
///
/// # Example
///
/// ```
/// use logos_layout::grid::GridArea;
///
/// let a = GridArea::new(1, 1, 4, 4);
/// let b = GridArea::new(2, 2, 2, 2);
/// assert!(a.contains(&b));
/// assert!(b.intersects(&a));
/// assert!(!b.contains(&a));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridArea {
    /// First column occupied (1-based).
    pub col: usize,
    /// First row occupied (1-based).
    pub row: usize,
    /// Number of columns spanned (≥ 1).
    pub col_span: usize,
    /// Number of rows spanned (≥ 1).
    pub row_span: usize,
}

impl GridArea {
    /// Create a new [`GridArea`].
    pub fn new(col: usize, row: usize, col_span: usize, row_span: usize) -> Self {
        GridArea { col, row, col_span, row_span }
    }

    // -------------------------------------------------------------------------
    // Internal helpers – mirror Clojure destructuring [a-x a-y a-width a-height]
    // -------------------------------------------------------------------------

    #[inline]
    fn end_col(self) -> usize { self.col + self.col_span }

    #[inline]
    fn end_row(self) -> usize { self.row + self.row_span }

    // -------------------------------------------------------------------------
    // Public predicates
    // -------------------------------------------------------------------------

    /// Returns `true` if `other` is entirely contained within `self`.
    ///
    /// # Example
    ///
    /// ```
    /// use logos_layout::grid::GridArea;
    /// let outer = GridArea::new(1, 1, 6, 6);
    /// let inner = GridArea::new(2, 2, 3, 3);
    /// assert!(outer.contains(&inner));
    /// assert!(!inner.contains(&outer));
    /// ```
    pub fn contains(&self, other: &GridArea) -> bool {
        other.col >= self.col
            && other.row >= self.row
            && other.end_col() <= self.end_col()
            && other.end_row() <= self.end_row()
    }

    /// Returns `true` if `self` and `other` share at least one grid cell.
    ///
    /// # Example
    ///
    /// ```
    /// use logos_layout::grid::GridArea;
    /// let a = GridArea::new(1, 1, 3, 3);
    /// let b = GridArea::new(2, 2, 3, 3);
    /// assert!(a.intersects(&b));
    /// let c = GridArea::new(5, 5, 2, 2);
    /// assert!(!a.intersects(&c));
    /// ```
    pub fn intersects(&self, other: &GridArea) -> bool {
        !(other.end_col() <= self.col
            || other.end_row() <= self.row
            || other.col >= self.end_col()
            || other.row >= self.end_row())
    }

    // -------------------------------------------------------------------------
    // Rectangle-difference fragments (four cardinal sub-rects)
    // -------------------------------------------------------------------------

    /// Rows of `self` strictly above `other`.
    pub fn top_rect(&self, other: &GridArea) -> Option<GridArea> {
        if other.row > self.row {
            Some(GridArea::new(self.col, self.row, self.col_span, other.row - self.row))
        } else {
            None
        }
    }

    /// Rows of `self` strictly below `other`.
    pub fn bottom_rect(&self, other: &GridArea) -> Option<GridArea> {
        let b_end = other.end_row();
        let a_end = self.end_row();
        if b_end < a_end {
            let height = a_end - b_end;
            Some(GridArea::new(self.col, b_end, self.col_span, height))
        } else {
            None
        }
    }

    /// Columns of `self` strictly to the left of `other`, within the
    /// row-overlap band.
    pub fn left_rect(&self, other: &GridArea) -> Option<GridArea> {
        let y1 = self.row.max(other.row);
        let y2 = self.end_row().min(other.end_row());
        let height = if y2 > y1 { y2 - y1 } else { 0 };
        let width = if other.col > self.col { other.col - self.col } else { 0 };
        if width > 0 && height > 0 {
            Some(GridArea::new(self.col, y1, width, height))
        } else {
            None
        }
    }

    /// Columns of `self` strictly to the right of `other`, within the
    /// row-overlap band.
    pub fn right_rect(&self, other: &GridArea) -> Option<GridArea> {
        let y1 = self.row.max(other.row);
        let y2 = self.end_row().min(other.end_row());
        let height = if y2 > y1 { y2 - y1 } else { 0 };
        let b_end_col = other.end_col();
        let width = if self.end_col() > b_end_col { self.end_col() - b_end_col } else { 0 };
        if width > 0 && height > 0 {
            Some(GridArea::new(b_end_col, y1, width, height))
        } else {
            None
        }
    }

    // -------------------------------------------------------------------------
    // Set difference
    // -------------------------------------------------------------------------

    /// Returns up to four sub-areas of `self` that are not covered by `other`.
    ///
    /// Mirrors Clojure `difference`: if `other` is nil, non-intersecting, or
    /// fully contains `self` the result is the empty vec / empty vec
    /// respectively.
    ///
    /// # Example
    ///
    /// ```
    /// use logos_layout::grid::GridArea;
    ///
    /// // Punching a 2×2 hole in the centre of a 4×4 square.
    /// let outer = GridArea::new(1, 1, 4, 4);
    /// let hole  = GridArea::new(2, 2, 2, 2);
    /// let diff  = outer.difference(&hole);
    /// // top row, bottom row, left col, right col
    /// assert_eq!(diff.len(), 4);
    /// ```
    pub fn difference(&self, other: &GridArea) -> Vec<GridArea> {
        if !self.intersects(other) || other.contains(self) {
            return vec![];
        }

        let mut result = Vec::with_capacity(4);
        if let Some(r) = self.top_rect(other)    { result.push(r); }
        if let Some(r) = self.left_rect(other)   { result.push(r); }
        if let Some(r) = self.right_rect(other)  { result.push(r); }
        if let Some(r) = self.bottom_rect(other) { result.push(r); }
        result
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // contains
    // -------------------------------------------------------------------------

    #[test]
    fn contains_self() {
        let a = GridArea::new(1, 1, 4, 4);
        assert!(a.contains(&a));
    }

    #[test]
    fn contains_inner() {
        let outer = GridArea::new(1, 1, 6, 6);
        let inner = GridArea::new(2, 3, 2, 2);
        assert!(outer.contains(&inner));
        assert!(!inner.contains(&outer));
    }

    #[test]
    fn contains_adjacent_does_not_contain() {
        let a = GridArea::new(1, 1, 3, 3);
        let b = GridArea::new(4, 1, 2, 2);
        assert!(!a.contains(&b));
    }

    #[test]
    fn contains_partial_overlap() {
        let a = GridArea::new(1, 1, 4, 4);
        let b = GridArea::new(3, 3, 4, 4); // overlaps but extends beyond a
        assert!(!a.contains(&b));
    }

    // -------------------------------------------------------------------------
    // intersects
    // -------------------------------------------------------------------------

    #[test]
    fn intersects_overlapping() {
        let a = GridArea::new(1, 1, 3, 3);
        let b = GridArea::new(2, 2, 3, 3);
        assert!(a.intersects(&b));
        assert!(b.intersects(&a));
    }

    #[test]
    fn intersects_touching_edge_not_intersecting() {
        // b starts exactly where a ends → no shared cell
        let a = GridArea::new(1, 1, 3, 3);
        let b = GridArea::new(4, 1, 2, 2); // col 4 = a.end_col
        assert!(!a.intersects(&b));
    }

    #[test]
    fn intersects_disjoint() {
        let a = GridArea::new(1, 1, 2, 2);
        let b = GridArea::new(5, 5, 2, 2);
        assert!(!a.intersects(&b));
    }

    #[test]
    fn intersects_contained() {
        let outer = GridArea::new(1, 1, 5, 5);
        let inner = GridArea::new(2, 2, 2, 2);
        assert!(outer.intersects(&inner));
    }

    // -------------------------------------------------------------------------
    // top_rect / bottom_rect / left_rect / right_rect
    // -------------------------------------------------------------------------

    #[test]
    fn top_rect_basic() {
        let a = GridArea::new(1, 1, 4, 4); // rows 1-4
        let b = GridArea::new(1, 3, 4, 2); // starts at row 3
        let top = a.top_rect(&b).unwrap();
        // rows 1..2 (height 2), full width
        assert_eq!(top, GridArea::new(1, 1, 4, 2));
    }

    #[test]
    fn top_rect_none_when_b_at_top() {
        let a = GridArea::new(1, 1, 4, 4);
        let b = GridArea::new(1, 1, 2, 2);
        assert!(a.top_rect(&b).is_none());
    }

    #[test]
    fn bottom_rect_basic() {
        let a = GridArea::new(1, 1, 4, 6); // rows 1-6
        let b = GridArea::new(2, 2, 2, 2); // rows 2-3 → b_end = 4
        let bot = a.bottom_rect(&b).unwrap();
        // rows 4-6 → start=4, height=3
        assert_eq!(bot, GridArea::new(1, 4, 4, 3));
    }

    #[test]
    fn bottom_rect_none_when_b_extends_to_bottom() {
        let a = GridArea::new(1, 1, 4, 4);
        let b = GridArea::new(1, 2, 2, 3); // b_end = 5 ≥ a_end = 5
        assert!(a.bottom_rect(&b).is_none());
    }

    #[test]
    fn left_rect_basic() {
        let a = GridArea::new(1, 1, 6, 4); // cols 1-6
        let b = GridArea::new(3, 1, 3, 4); // starts at col 3
        let left = a.left_rect(&b).unwrap();
        // cols 1-2 (width 2), full row overlap
        assert_eq!(left, GridArea::new(1, 1, 2, 4));
    }

    #[test]
    fn left_rect_none_when_b_at_left() {
        let a = GridArea::new(1, 1, 4, 4);
        let b = GridArea::new(1, 1, 2, 2);
        assert!(a.left_rect(&b).is_none());
    }

    #[test]
    fn right_rect_basic() {
        let a = GridArea::new(1, 1, 6, 4); // cols 1-6 → end_col=7
        let b = GridArea::new(1, 1, 4, 4); // cols 1-4 → b_end_col=5
        let right = a.right_rect(&b).unwrap();
        // cols 5-6 (width 2), rows 1-4
        assert_eq!(right, GridArea::new(5, 1, 2, 4));
    }

    #[test]
    fn right_rect_none_when_b_covers_width() {
        let a = GridArea::new(1, 1, 4, 4);
        let b = GridArea::new(1, 1, 4, 2);
        assert!(a.right_rect(&b).is_none());
    }

    // -------------------------------------------------------------------------
    // difference
    // -------------------------------------------------------------------------

    #[test]
    fn difference_non_intersecting_is_empty() {
        let a = GridArea::new(1, 1, 3, 3);
        let b = GridArea::new(5, 5, 2, 2);
        assert!(a.difference(&b).is_empty());
    }

    #[test]
    fn difference_b_contains_a_is_empty() {
        let a = GridArea::new(2, 2, 2, 2);
        let b = GridArea::new(1, 1, 6, 6);
        assert!(a.difference(&b).is_empty());
    }

    #[test]
    fn difference_centre_punch_gives_four_rects() {
        // 4×4 outer minus 2×2 centre
        let outer = GridArea::new(1, 1, 4, 4);
        let hole  = GridArea::new(2, 2, 2, 2);
        let diff  = outer.difference(&hole);
        assert_eq!(diff.len(), 4, "expected top/left/right/bottom: {diff:?}");
    }

    #[test]
    fn difference_b_covers_full_row_gives_top_and_bottom() {
        // a = rows 1-4, b = row 2 full width → top (row 1) + bottom (rows 3-4)
        let a = GridArea::new(1, 1, 4, 4);
        let b = GridArea::new(1, 2, 4, 1);
        let diff = a.difference(&b);
        // top_rect: (1,1,4,1), bottom_rect: (1,3,4,2); no left/right
        assert_eq!(diff.len(), 2);
        assert!(diff.contains(&GridArea::new(1, 1, 4, 1)));
        assert!(diff.contains(&GridArea::new(1, 3, 4, 2)));
    }

    #[test]
    fn difference_corner_overlap() {
        // Overlap at top-left corner: a=(1,1,4,4), b=(1,1,2,2)
        // b covers top-left → remaining: right strip + bottom strip
        let a = GridArea::new(1, 1, 4, 4);
        let b = GridArea::new(1, 1, 2, 2);
        let diff = a.difference(&b);
        // top_rect: none (b.row == a.row)
        // left_rect: none (b.col == a.col)
        // right_rect: cols 3-4, rows 1-2 → (3,1,2,2)
        // bottom_rect: rows 3-4 → (1,3,4,2)
        assert_eq!(diff.len(), 2);
        assert!(diff.contains(&GridArea::new(3, 1, 2, 2)));
        assert!(diff.contains(&GridArea::new(1, 3, 4, 2)));
    }
}
