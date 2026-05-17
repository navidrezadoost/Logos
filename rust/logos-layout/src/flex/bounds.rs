//! Flex layout bounds — final container bounding rectangle.
//!
//! Input:  FlexContainer + Vec<ChildFinalPosition>
//! Output: FlexBounds — the container's new (x, y, width, height)
//!
//! Rules:
//! - If the container has an explicit size in a dimension (non-zero), use it.
//! - If the container is auto-sized, compute from the union of child bounding
//!   boxes plus padding.
//!
//! "Explicit size" is detected by comparing `available_width`/`available_height`
//! against zero (the sentinel for auto-sized containers). Callers that want
//! fully fixed containers pass non-zero values; callers that want auto-sized
//! containers pass `0.0`.
//!
//! Padding expands the container outward:
//!   auto_width  = max(child rights)  + padding_right  + padding_left
//!   auto_height = max(child bottoms) + padding_bottom + padding_top
//!
//! The origin `(x, y)` is always `(0.0, 0.0)` — coordinates are in the
//! container's local space (children have already been offset by padding
//! inside `compute_positions()`).

use super::params::FlexContainer;
use super::positions::{ChildFinalPosition, Uuid};

/// Bounding rectangle of the container after flex layout.
///
/// Coordinates are in the **container's local space** — `(0, 0)` is the
/// container origin.  The parent is responsible for translating this into
/// world space.
#[derive(Debug, Clone, PartialEq)]
pub struct FlexBounds {
    /// Origin X (always 0.0 in local space).
    pub x: f64,
    /// Origin Y (always 0.0 in local space).
    pub y: f64,
    /// Container width after layout.
    pub width: f64,
    /// Container height after layout.
    pub height: f64,
}

/// Compute the container's bounding rectangle after flex layout.
///
/// # Arguments
/// * `container`        — the flex container (provides padding and direction)
/// * `children`         — flat slice of final child positions (from all lines)
/// * `available_width`  — explicit container width, or `0.0` for auto-width
/// * `available_height` — explicit container height, or `0.0` for auto-height
///
/// # Auto-sizing
/// When `available_width == 0.0`, width = rightmost child edge + h-padding.
/// When `available_height == 0.0`, height = bottommost child edge + v-padding.
///
/// # Empty container
/// If `children` is empty, returns the explicit size.  If both dimensions
/// are auto (`0.0`), returns a zero-size bounds at the origin (degenerate
/// but well-defined).
///
/// # Example
/// ```rust
/// use logos_layout::flex::{
///     FlexContainer, compute_positions, compute_bounds,
///     ChildLayoutData, AlignSelf,
/// };
///
/// let container = FlexContainer::default();
///
/// let children: Vec<(u64, ChildLayoutData)> = vec![(1, ChildLayoutData {
///     main_min: 100.0,
///     main_max: 100.0,
///     main_fill: false,
///     main_auto: false,
///     cross_min: 50.0,
///     cross_max: 50.0,
///     cross_fill: false,
///     cross_auto: false,
///     width: Some(100.0),
///     height: Some(50.0),
///     flex_grow: 0.0,
///     flex_shrink: 1.0,
///     flex_basis: Some(100.0),
///     align_self: AlignSelf::Auto,
///     absolute: false,
/// })];
///
/// let lines = compute_positions(&container, &children, 400.0, 200.0);
/// let flat: Vec<_> = lines.iter().flat_map(|l| l.children.iter().cloned()).collect();
/// let bounds = compute_bounds(&container, &flat, 400.0, 200.0);
/// assert_eq!(bounds.width, 400.0);
/// assert_eq!(bounds.height, 200.0);
/// ```
pub fn compute_bounds(
    container: &FlexContainer,
    children: &[ChildFinalPosition],
    available_width: f64,
    available_height: f64,
) -> FlexBounds {
    let width = if available_width > 0.0 {
        available_width
    } else {
        auto_size_width(container, children)
    };

    let height = if available_height > 0.0 {
        available_height
    } else {
        auto_size_height(container, children)
    };

    FlexBounds {
        x: 0.0,
        y: 0.0,
        width,
        height,
    }
}

// ---------------------------------------------------------------------------
// Auto-sizing helpers
// ---------------------------------------------------------------------------

/// Compute auto width: rightmost child edge + horizontal padding.
fn auto_size_width(container: &FlexContainer, children: &[ChildFinalPosition]) -> f64 {
    let max_right = children
        .iter()
        .map(|c| c.x + c.width)
        .fold(0.0_f64, f64::max);
    // padding: (top, right, bottom, left)
    max_right + container.padding.3 + container.padding.1
}

/// Compute auto height: bottommost child edge + vertical padding.
fn auto_size_height(container: &FlexContainer, children: &[ChildFinalPosition]) -> f64 {
    let max_bottom = children
        .iter()
        .map(|c| c.y + c.height)
        .fold(0.0_f64, f64::max);
    // padding: (top, right, bottom, left)
    max_bottom + container.padding.0 + container.padding.2
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flex::layout_data::{AlignSelf, ChildLayoutData};
    use crate::flex::params::{
        AlignContent, AlignItems, FlexContainer, FlexDirection, FlexWrap, JustifyContent,
    };
    use crate::flex::positions::compute_positions;

    fn make_container() -> FlexContainer {
        FlexContainer::default()
    }

    fn fixed_child(id: u64, main: f64, cross: f64) -> (u64, ChildLayoutData) {
        (
            id,
            ChildLayoutData {
                main_min: main,
                main_max: main,
                main_fill: false,
                main_auto: false,
                cross_min: cross,
                cross_max: cross,
                cross_fill: false,
                cross_auto: false,
                width: Some(main),
                height: Some(cross),
                flex_grow: 0.0,
                flex_shrink: 1.0,
                flex_basis: Some(main),
                align_self: AlignSelf::Auto,
                absolute: false,
            },
        )
    }

    fn positions_from(
        container: &FlexContainer,
        children: &[(u64, ChildLayoutData)],
        w: f64,
        h: f64,
    ) -> Vec<ChildFinalPosition> {
        let lines = compute_positions(container, children, w, h);
        lines
            .into_iter()
            .flat_map(|l| l.children.into_iter())
            .collect()
    }

    // ------------------------------------------------------------------
    // Explicit size
    // ------------------------------------------------------------------

    #[test]
    fn test_explicit_size_used_when_nonzero() {
        let c = make_container();
        let children_data = vec![fixed_child(1, 100.0, 50.0)];
        let flat = positions_from(&c, &children_data, 400.0, 200.0);
        let bounds = compute_bounds(&c, &flat, 400.0, 200.0);
        assert_eq!(bounds.width, 400.0);
        assert_eq!(bounds.height, 200.0);
    }

    #[test]
    fn test_bounds_origin_always_zero() {
        let c = make_container();
        let flat = positions_from(&c, &[fixed_child(1, 100.0, 50.0)], 400.0, 200.0);
        let bounds = compute_bounds(&c, &flat, 400.0, 200.0);
        assert_eq!(bounds.x, 0.0);
        assert_eq!(bounds.y, 0.0);
    }

    // ------------------------------------------------------------------
    // Auto-sizing (available == 0.0)
    // ------------------------------------------------------------------

    #[test]
    fn test_auto_width_from_children() {
        let c = make_container();
        // Two children 100px each → auto width should be 200px
        let children_data = vec![fixed_child(1, 100.0, 50.0), fixed_child(2, 100.0, 50.0)];
        let flat = positions_from(&c, &children_data, 400.0, 200.0);
        let bounds = compute_bounds(&c, &flat, 0.0, 200.0);
        // max right edge: child1 @ x=0 w=100 → 100; child2 @ x=100 w=100 → 200
        assert_eq!(bounds.width, 200.0);
        assert_eq!(bounds.height, 200.0); // explicit
    }

    #[test]
    fn test_auto_height_from_children() {
        let mut c = make_container();
        c.direction = FlexDirection::Column;
        // Column: main=height, cross=width → fixed_child(id, main=height, cross=width)
        let children_data = vec![fixed_child(1, 80.0, 60.0), fixed_child(2, 80.0, 60.0)];
        // positions: child1@(x=0,y=0) w=60,h=80; child2@(x=0,y=80) w=60,h=80
        let flat = positions_from(&c, &children_data, 200.0, 400.0);
        let bounds = compute_bounds(&c, &flat, 200.0, 0.0);
        // auto height: bottommost edge = 80 + 80 = 160
        assert_eq!(bounds.height, 160.0);
        assert_eq!(bounds.width, 200.0); // explicit
    }

    #[test]
    fn test_auto_both_dimensions() {
        let c = make_container();
        let children_data = vec![fixed_child(1, 120.0, 70.0)];
        let flat = positions_from(&c, &children_data, 400.0, 200.0);
        let bounds = compute_bounds(&c, &flat, 0.0, 0.0);
        assert_eq!(bounds.width, 120.0); // right edge = 0 + 120
        assert_eq!(bounds.height, 70.0); // bottom edge = 0 + 70
    }

    // ------------------------------------------------------------------
    // Padding
    // ------------------------------------------------------------------

    #[test]
    fn test_auto_width_includes_padding() {
        let mut c = make_container();
        // padding: (top, right, bottom, left)
        c.padding = (0.0, 15.0, 0.0, 10.0);
        let children_data = vec![fixed_child(1, 100.0, 50.0)];
        let flat = positions_from(&c, &children_data, 400.0, 200.0);
        let bounds = compute_bounds(&c, &flat, 0.0, 200.0);
        // right edge of child (positioned at x=0) = 100; + left(10) + right(15) = 125
        assert_eq!(bounds.width, 100.0 + 10.0 + 15.0);
    }

    #[test]
    fn test_auto_height_includes_padding() {
        let mut c = make_container();
        // padding: (top, right, bottom, left)
        c.padding = (8.0, 0.0, 12.0, 0.0);
        let children_data = vec![fixed_child(1, 100.0, 60.0)];
        let flat = positions_from(&c, &children_data, 400.0, 200.0);
        let bounds = compute_bounds(&c, &flat, 200.0, 0.0);
        assert_eq!(bounds.height, 60.0 + 8.0 + 12.0);
    }

    // ------------------------------------------------------------------
    // Edge cases
    // ------------------------------------------------------------------

    #[test]
    fn test_empty_children_explicit_size() {
        let c = make_container();
        let bounds = compute_bounds(&c, &[], 300.0, 150.0);
        assert_eq!(bounds.width, 300.0);
        assert_eq!(bounds.height, 150.0);
    }

    #[test]
    fn test_empty_children_auto_size_is_zero() {
        let c = make_container();
        let bounds = compute_bounds(&c, &[], 0.0, 0.0);
        assert_eq!(bounds.width, 0.0);
        assert_eq!(bounds.height, 0.0);
    }

    #[test]
    fn test_multi_line_auto_height() {
        // Wrapping container with AlignContent::Start so lines pack at top.
        // Default AlignContent is Stretch, which would spread lines across available_height.
        let mut c = make_container();
        c.wrap = FlexWrap::Wrap;
        c.align_content = AlignContent::Start;
        // 3 children × 150px in 300px container → 2 lines
        // Line 0: children 1+2  (y=0..50)
        // Line 1: child 3       (y=50..100)
        let children_data = vec![
            fixed_child(1, 150.0, 50.0),
            fixed_child(2, 150.0, 50.0),
            fixed_child(3, 150.0, 50.0),
        ];
        let flat = positions_from(&c, &children_data, 300.0, 400.0);
        let bounds = compute_bounds(&c, &flat, 300.0, 0.0);
        // Bottom of line 1 = 100
        assert_eq!(bounds.height, 100.0);
    }
}
