//! Flex layout output layer — geometry modifier emission.
//!
//! Input:  FlexContainer + Vec<FlexLine> (from positions.rs)
//! Output: Vec<FlexModifier> — one record per positioned child
//!
//! Each `FlexModifier` is an instruction to the downstream geometry engine:
//! "apply (x, y, width, height) to this shape, with optional flip flags."
//!
//! The only logic here is reverse-direction flags:
//! - `RowReverse`    → `flip_x = true`
//! - `ColumnReverse` → `flip_y = true`
//!
//! Actual transform application lives in the geometry engine (downstream).
//! This module just emits the instruction record.

use super::params::{FlexContainer, FlexDirection};
use super::positions::{ChildFinalPosition, FlexLine, Uuid};

/// Geometry modifier instruction for a single child shape.
///
/// Fields map directly to CSS/SVG transform concepts:
/// - `(x, y)` — absolute position in the container's coordinate space
/// - `(width, height)` — computed size after flex grow/shrink/stretch
/// - `rotation` — always `0.0` here; provided for downstream uniformity
/// - `flip_x` — mirror on X axis (row-reverse direction)
/// - `flip_y` — mirror on Y axis (column-reverse direction)
#[derive(Debug, Clone, PartialEq)]
pub struct FlexModifier {
    /// Shape this modifier targets.
    pub shape_id: Uuid,
    /// Absolute X position within the container.
    pub x: f64,
    /// Absolute Y position within the container.
    pub y: f64,
    /// Final width after flex sizing.
    pub width: f64,
    /// Final height after flex sizing.
    pub height: f64,
    /// Rotation in radians — always 0.0 for flex-placed children.
    pub rotation: f64,
    /// Mirror on X axis — true when direction is `RowReverse`.
    pub flip_x: bool,
    /// Mirror on Y axis — true when direction is `ColumnReverse`.
    pub flip_y: bool,
}

impl FlexModifier {
    /// Construct from a `ChildFinalPosition` and directional flip flags.
    fn from_position(pos: &ChildFinalPosition, flip_x: bool, flip_y: bool) -> Self {
        Self {
            shape_id: pos.id,
            x: pos.x,
            y: pos.y,
            width: pos.width,
            height: pos.height,
            rotation: 0.0,
            flip_x,
            flip_y,
        }
    }
}

/// Convert flex layout results into geometry modifier records.
///
/// # Arguments
/// * `container`  — the flex container (used for direction flags only)
/// * `lines`      — positioned children produced by `compute_positions()`
///
/// # Returns
/// Flat `Vec<FlexModifier>` in line-major, child-minor order (same order
/// as the input `lines`).  The downstream geometry engine applies these
/// in any order — ordering here is for determinism in tests.
///
/// # Flip semantics
/// `RowReverse` / `ColumnReverse` set flip flags but do **not** rewrite
/// coordinates.  The geometry engine mirrors the shape around its own
/// centre after applying `(x, y, width, height)`.  This matches the
/// Clojure implementation's modifier map approach.
///
/// # Example
/// ```rust
/// use logos_layout::flex::{
///     FlexContainer, compute_positions, modifiers_from_positions,
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
/// let mods = modifiers_from_positions(&container, &lines);
/// assert_eq!(mods.len(), 1);
/// assert_eq!(mods[0].shape_id, 1);
/// assert!(!mods[0].flip_x);
/// assert!(!mods[0].flip_y);
/// ```
pub fn modifiers_from_positions(
    container: &FlexContainer,
    lines: &[FlexLine],
) -> Vec<FlexModifier> {
    let flip_x = container.direction == FlexDirection::RowReverse;
    let flip_y = container.direction == FlexDirection::ColumnReverse;

    lines
        .iter()
        .flat_map(|line| line.children.iter())
        .map(|pos| FlexModifier::from_position(pos, flip_x, flip_y))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flex::layout_data::{AlignSelf, ChildLayoutData};
    use crate::flex::params::{
        AlignContent, AlignItems, FlexContainer, FlexDirection, FlexWrap, JustifyContent,
    };
    use crate::flex::positions::compute_positions;

    fn make_container(direction: FlexDirection) -> FlexContainer {
        FlexContainer {
            direction,
            ..FlexContainer::default()
        }
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

    // ------------------------------------------------------------------
    // Basic modifier emission
    // ------------------------------------------------------------------

    #[test]
    fn test_modifier_count_matches_children() {
        let c = make_container(FlexDirection::Row);
        let children = vec![fixed_child(1, 100.0, 50.0), fixed_child(2, 100.0, 50.0)];
        let lines = compute_positions(&c, &children, 400.0, 100.0);
        let mods = modifiers_from_positions(&c, &lines);
        assert_eq!(mods.len(), 2);
    }

    #[test]
    fn test_modifier_shape_ids_preserved() {
        let c = make_container(FlexDirection::Row);
        let children = vec![fixed_child(42, 100.0, 50.0), fixed_child(99, 80.0, 50.0)];
        let lines = compute_positions(&c, &children, 400.0, 100.0);
        let mods = modifiers_from_positions(&c, &lines);
        assert_eq!(mods[0].shape_id, 42);
        assert_eq!(mods[1].shape_id, 99);
    }

    #[test]
    fn test_modifier_coordinates_match_positions() {
        let c = make_container(FlexDirection::Row);
        let children = vec![fixed_child(1, 100.0, 50.0)];
        let lines = compute_positions(&c, &children, 400.0, 200.0);
        let mods = modifiers_from_positions(&c, &lines);
        assert_eq!(mods[0].x, 0.0);
        assert_eq!(mods[0].y, 0.0);
        assert_eq!(mods[0].width, 100.0);
        assert_eq!(mods[0].height, 50.0);
    }

    #[test]
    fn test_rotation_always_zero() {
        let c = make_container(FlexDirection::Column);
        let children = vec![fixed_child(1, 80.0, 60.0)];
        let lines = compute_positions(&c, &children, 200.0, 400.0);
        let mods = modifiers_from_positions(&c, &lines);
        assert_eq!(mods[0].rotation, 0.0);
    }

    // ------------------------------------------------------------------
    // Flip flags
    // ------------------------------------------------------------------

    #[test]
    fn test_row_no_flip() {
        let c = make_container(FlexDirection::Row);
        let lines = compute_positions(&c, &[fixed_child(1, 50.0, 50.0)], 200.0, 100.0);
        let mods = modifiers_from_positions(&c, &lines);
        assert!(!mods[0].flip_x);
        assert!(!mods[0].flip_y);
    }

    #[test]
    fn test_column_no_flip() {
        let c = make_container(FlexDirection::Column);
        let lines = compute_positions(&c, &[fixed_child(1, 50.0, 50.0)], 100.0, 200.0);
        let mods = modifiers_from_positions(&c, &lines);
        assert!(!mods[0].flip_x);
        assert!(!mods[0].flip_y);
    }

    #[test]
    fn test_row_reverse_sets_flip_x() {
        let c = make_container(FlexDirection::RowReverse);
        let lines = compute_positions(&c, &[fixed_child(1, 50.0, 50.0)], 200.0, 100.0);
        let mods = modifiers_from_positions(&c, &lines);
        assert!(mods[0].flip_x, "RowReverse should set flip_x");
        assert!(!mods[0].flip_y);
    }

    #[test]
    fn test_column_reverse_sets_flip_y() {
        let c = make_container(FlexDirection::ColumnReverse);
        let lines = compute_positions(&c, &[fixed_child(1, 50.0, 50.0)], 100.0, 200.0);
        let mods = modifiers_from_positions(&c, &lines);
        assert!(!mods[0].flip_x);
        assert!(mods[0].flip_y, "ColumnReverse should set flip_y");
    }

    #[test]
    fn test_flip_applied_to_all_children() {
        let c = make_container(FlexDirection::RowReverse);
        let children = vec![
            fixed_child(1, 80.0, 50.0),
            fixed_child(2, 80.0, 50.0),
            fixed_child(3, 80.0, 50.0),
        ];
        let lines = compute_positions(&c, &children, 400.0, 100.0);
        let mods = modifiers_from_positions(&c, &lines);
        assert_eq!(mods.len(), 3);
        for m in &mods {
            assert!(m.flip_x, "all children should have flip_x");
        }
    }

    // ------------------------------------------------------------------
    // Multi-line ordering
    // ------------------------------------------------------------------

    #[test]
    fn test_multi_line_flat_order() {
        // Two lines of 2 children → 4 modifiers, line-major order.
        let mut c = make_container(FlexDirection::Row);
        c.wrap = FlexWrap::Wrap;
        let children = vec![
            fixed_child(1, 150.0, 50.0),
            fixed_child(2, 150.0, 50.0),
            fixed_child(3, 150.0, 50.0),
            fixed_child(4, 150.0, 50.0),
        ];
        // Container 300px wide → 2 children per line
        let lines = compute_positions(&c, &children, 300.0, 200.0);
        let mods = modifiers_from_positions(&c, &lines);
        assert_eq!(mods.len(), 4);
        // All children present (order determined by line-major traversal)
        let ids: Vec<u64> = mods.iter().map(|m| m.shape_id).collect();
        assert!(ids.contains(&1));
        assert!(ids.contains(&2));
        assert!(ids.contains(&3));
        assert!(ids.contains(&4));
    }
}
