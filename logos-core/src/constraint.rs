//! Independent constraint system for Logos.
//!
//! Constraints define how a layer repositions or resizes when its parent
//! frame is resized.  They are evaluated *before* Auto Layout runs so that
//! pinned elements are already in their final position when the flex pass
//! begins.
//!
//! Model mirrors Figma's constraint design:
//! - **Horizontal**: Left | Right | LeftAndRight (Stretch) | Center | Scale
//! - **Vertical**:   Top  | Bottom | TopAndBottom (Stretch) | Center | Scale

use serde::{Deserialize, Serialize};

use crate::Rect;

// ── Horizontal ────────────────────────────────────────────────────────────────

/// How a layer behaves horizontally when its parent frame is resized.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum HorizontalConstraint {
    /// Preserve the distance between the layer's left edge and the parent's
    /// left edge (default behaviour — like CSS `left: Xpx`).
    #[default]
    Left,
    /// Preserve the distance between the layer's right edge and the parent's
    /// right edge.
    Right,
    /// Pin to **both** edges: layer stretches horizontally with the parent
    /// while preserving margins on both sides (CSS `left + right`).
    LeftAndRight,
    /// Keep the layer's horizontal midpoint centred inside the parent.
    Center,
    /// Scale the layer's x-position and width proportionally with the parent.
    Scale,
}

// ── Vertical ─────────────────────────────────────────────────────────────────

/// How a layer behaves vertically when its parent frame is resized.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum VerticalConstraint {
    /// Preserve distance from the parent's top edge (default).
    #[default]
    Top,
    /// Preserve distance from the parent's bottom edge.
    Bottom,
    /// Pin to **both** edges: layer stretches vertically with the parent.
    TopAndBottom,
    /// Keep the layer's vertical midpoint centred inside the parent.
    Center,
    /// Scale the layer's y-position and height proportionally with the parent.
    Scale,
}

// ── Constraints struct ────────────────────────────────────────────────────────

/// A pair of horizontal + vertical constraints attached to a layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Constraints {
    pub horizontal: HorizontalConstraint,
    pub vertical: VerticalConstraint,
}

impl Constraints {
    /// Create with explicit axis values.
    pub fn new(horizontal: HorizontalConstraint, vertical: VerticalConstraint) -> Self {
        Self { horizontal, vertical }
    }

    /// Convenience: top-left pin (the default).
    pub fn top_left() -> Self {
        Self::default()
    }

    /// Convenience: scale on both axes.
    pub fn scale() -> Self {
        Self::new(HorizontalConstraint::Scale, VerticalConstraint::Scale)
    }

    /// Convenience: center on both axes.
    pub fn center() -> Self {
        Self::new(HorizontalConstraint::Center, VerticalConstraint::Center)
    }

    /// Convenience: stretch on both axes.
    pub fn stretch() -> Self {
        Self::new(
            HorizontalConstraint::LeftAndRight,
            VerticalConstraint::TopAndBottom,
        )
    }
}

// ── Resolver ─────────────────────────────────────────────────────────────────

/// Compute the new bounding rect for a child layer after its parent resizes.
///
/// - `parent_old` — the parent's rect *before* the resize.
/// - `parent_new` — the parent's rect *after* the resize.
/// - `child`      — the child's rect in parent-local coordinates (relative to
///                  the parent's origin).
/// - `constraints`— the constraint rules governing movement/resize.
///
/// Returns the updated child rect in parent-local coordinates.
///
/// # Panics
/// The function does not panic; it clamps dimensions to zero if a degenerate
/// parent produces a negative size.
pub fn resolve_constraints(
    parent_old: Rect,
    parent_new: Rect,
    child: Rect,
    constraints: &Constraints,
) -> Rect {
    let new_x = resolve_axis(
        parent_old.width,
        parent_new.width,
        child.x,
        child.width,
        axis_constraint_h(constraints.horizontal),
    );
    let new_y = resolve_axis(
        parent_old.height,
        parent_new.height,
        child.y,
        child.height,
        axis_constraint_v(constraints.vertical),
    );
    Rect {
        x: new_x.0,
        y: new_y.0,
        width: new_x.1.max(0.0),
        height: new_y.1.max(0.0),
    }
}

// ── Internal ──────────────────────────────────────────────────────────────────

/// Unified axis resolver.  Returns `(new_origin, new_size)`.
///
/// Parameters (all on one axis):
/// - `old_parent_size` — parent size before resize
/// - `new_parent_size` — parent size after resize
/// - `child_origin`    — child origin relative to parent
/// - `child_size`      — child size
/// - `mode`            — unified constraint enum (0=Start, 1=End, 2=Both, 3=Center, 4=Scale)
fn resolve_axis(
    old_parent: f32,
    new_parent: f32,
    child_origin: f32,
    child_size: f32,
    mode: AxisMode,
) -> (f32, f32) {
    if old_parent == 0.0 {
        return (child_origin, child_size);
    }
    match mode {
        AxisMode::Start => (child_origin, child_size),
        AxisMode::End => {
            let dist_from_end = old_parent - (child_origin + child_size);
            let new_origin = new_parent - dist_from_end - child_size;
            (new_origin, child_size)
        }
        AxisMode::StartAndEnd => {
            let dist_start = child_origin;
            let dist_end = old_parent - (child_origin + child_size);
            let new_origin = dist_start;
            let new_size = (new_parent - dist_start - dist_end).max(0.0);
            (new_origin, new_size)
        }
        AxisMode::Center => {
            let old_center = child_origin + child_size / 2.0;
            let offset_from_center = old_center - old_parent / 2.0;
            let new_center = new_parent / 2.0 + offset_from_center;
            let new_origin = new_center - child_size / 2.0;
            (new_origin, child_size)
        }
        AxisMode::Scale => {
            let ratio = new_parent / old_parent;
            let new_origin = child_origin * ratio;
            let new_size = child_size * ratio;
            (new_origin, new_size)
        }
    }
}

#[derive(Clone, Copy)]
enum AxisMode {
    Start,
    End,
    StartAndEnd,
    Center,
    Scale,
}

fn axis_constraint_h(h: HorizontalConstraint) -> AxisMode {
    match h {
        HorizontalConstraint::Left => AxisMode::Start,
        HorizontalConstraint::Right => AxisMode::End,
        HorizontalConstraint::LeftAndRight => AxisMode::StartAndEnd,
        HorizontalConstraint::Center => AxisMode::Center,
        HorizontalConstraint::Scale => AxisMode::Scale,
    }
}

fn axis_constraint_v(v: VerticalConstraint) -> AxisMode {
    match v {
        VerticalConstraint::Top => AxisMode::Start,
        VerticalConstraint::Bottom => AxisMode::End,
        VerticalConstraint::TopAndBottom => AxisMode::StartAndEnd,
        VerticalConstraint::Center => AxisMode::Center,
        VerticalConstraint::Scale => AxisMode::Scale,
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, width: w, height: h }
    }

    #[test]
    fn default_is_top_left() {
        let c = Constraints::default();
        assert_eq!(c.horizontal, HorizontalConstraint::Left);
        assert_eq!(c.vertical, VerticalConstraint::Top);
    }

    #[test]
    fn pin_left_does_not_move() {
        let parent_old = rect(0.0, 0.0, 400.0, 300.0);
        let parent_new = rect(0.0, 0.0, 800.0, 300.0);
        let child = rect(20.0, 0.0, 100.0, 50.0);
        let c = Constraints::new(HorizontalConstraint::Left, VerticalConstraint::Top);
        let r = resolve_constraints(parent_old, parent_new, child, &c);
        assert_eq!(r.x, 20.0);
        assert_eq!(r.width, 100.0);
    }

    #[test]
    fn pin_right_preserves_end_margin() {
        let parent_old = rect(0.0, 0.0, 400.0, 300.0);
        let parent_new = rect(0.0, 0.0, 600.0, 300.0);
        // child ends 50px from right edge: origin=100, size=250 → end=350, margin=50
        let child = rect(100.0, 0.0, 250.0, 50.0);
        let c = Constraints::new(HorizontalConstraint::Right, VerticalConstraint::Top);
        let r = resolve_constraints(parent_old, parent_new, child, &c);
        assert_eq!(r.x, 300.0); // 600 - 50 - 250
        assert_eq!(r.width, 250.0);
    }

    #[test]
    fn stretch_fills_between_margins() {
        let parent_old = rect(0.0, 0.0, 400.0, 300.0);
        let parent_new = rect(0.0, 0.0, 600.0, 300.0);
        // child at x=20, width=360, margins: left=20, right=20
        let child = rect(20.0, 0.0, 360.0, 50.0);
        let c = Constraints::stretch();
        let r = resolve_constraints(parent_old, parent_new, child, &c);
        assert_eq!(r.x, 20.0);
        assert_eq!(r.width, 560.0); // 600 - 20 - 20
    }

    #[test]
    fn center_horizontal_stays_centred() {
        let parent_old = rect(0.0, 0.0, 400.0, 300.0);
        let parent_new = rect(0.0, 0.0, 600.0, 300.0);
        // child centered: x=150, w=100 → center=200 (== parent center)
        let child = rect(150.0, 0.0, 100.0, 50.0);
        let c = Constraints::new(HorizontalConstraint::Center, VerticalConstraint::Top);
        let r = resolve_constraints(parent_old, parent_new, child, &c);
        assert_eq!(r.x, 250.0); // new center=300, so 300 - 50

        assert_eq!(r.width, 100.0); // size unchanged
    }

    #[test]
    fn scale_horizontal_proportional() {
        let parent_old = rect(0.0, 0.0, 400.0, 300.0);
        let parent_new = rect(0.0, 0.0, 800.0, 300.0);
        let child = rect(100.0, 0.0, 200.0, 50.0);
        let c = Constraints::scale();
        let r = resolve_constraints(parent_old, parent_new, child, &c);
        assert_eq!(r.x, 200.0); // scaled x
        assert_eq!(r.width, 400.0); // scaled width
    }

    #[test]
    fn pin_top_does_not_move_vertically() {
        let parent_old = rect(0.0, 0.0, 400.0, 300.0);
        let parent_new = rect(0.0, 0.0, 400.0, 600.0);
        let child = rect(0.0, 30.0, 100.0, 50.0);
        let c = Constraints::new(HorizontalConstraint::Left, VerticalConstraint::Top);
        let r = resolve_constraints(parent_old, parent_new, child, &c);
        assert_eq!(r.y, 30.0);
        assert_eq!(r.height, 50.0);
    }

    #[test]
    fn pin_bottom_preserves_bottom_margin() {
        let parent_old = rect(0.0, 0.0, 400.0, 300.0);
        let parent_new = rect(0.0, 0.0, 400.0, 500.0);
        // child ends 40px from bottom: y=200, h=60 → end=260, margin=40
        let child = rect(0.0, 200.0, 100.0, 60.0);
        let c = Constraints::new(HorizontalConstraint::Left, VerticalConstraint::Bottom);
        let r = resolve_constraints(parent_old, parent_new, child, &c);
        assert_eq!(r.y, 400.0); // 500 - 40 - 60
        assert_eq!(r.height, 60.0);
    }

    #[test]
    fn stretch_vertical_fills_margins() {
        let parent_old = rect(0.0, 0.0, 400.0, 300.0);
        let parent_new = rect(0.0, 0.0, 400.0, 500.0);
        let child = rect(0.0, 20.0, 100.0, 260.0); // margins top=20, bottom=20
        let c = Constraints::new(HorizontalConstraint::Left, VerticalConstraint::TopAndBottom);
        let r = resolve_constraints(parent_old, parent_new, child, &c);
        assert_eq!(r.y, 20.0);
        assert_eq!(r.height, 460.0); // 500 - 20 - 20
    }

    #[test]
    fn center_vertical_stays_centred() {
        let parent_old = rect(0.0, 0.0, 400.0, 300.0);
        let parent_new = rect(0.0, 0.0, 400.0, 500.0);
        // child centered vertically: y=125, h=50 → center=150 == 300/2
        let child = rect(0.0, 125.0, 100.0, 50.0);
        let c = Constraints::new(HorizontalConstraint::Left, VerticalConstraint::Center);
        let r = resolve_constraints(parent_old, parent_new, child, &c);
        assert_eq!(r.y, 225.0); // new center=250, so 250 - 25
        assert_eq!(r.height, 50.0);
    }

    #[test]
    fn scale_vertical_proportional() {
        let parent_old = rect(0.0, 0.0, 400.0, 200.0);
        let parent_new = rect(0.0, 0.0, 400.0, 400.0);
        let child = rect(0.0, 40.0, 100.0, 80.0);
        let c = Constraints::scale();
        let r = resolve_constraints(parent_old, parent_new, child, &c);
        assert_eq!(r.y, 80.0);
        assert_eq!(r.height, 160.0);
    }

    #[test]
    fn no_parent_resize_identity() {
        let parent = rect(0.0, 0.0, 400.0, 300.0);
        let child = rect(50.0, 50.0, 100.0, 80.0);
        for h in [
            HorizontalConstraint::Left,
            HorizontalConstraint::Right,
            HorizontalConstraint::LeftAndRight,
            HorizontalConstraint::Center,
            HorizontalConstraint::Scale,
        ] {
            for v in [
                VerticalConstraint::Top,
                VerticalConstraint::Bottom,
                VerticalConstraint::TopAndBottom,
                VerticalConstraint::Center,
                VerticalConstraint::Scale,
            ] {
                let r = resolve_constraints(parent, parent, child, &Constraints::new(h, v));
                // When parent doesn't resize, child stays identical (modulo
                // floating-point scale rounding which is exact for these values)
                assert!((r.width - child.width).abs() < 0.01, "{h:?}/{v:?}");
                assert!((r.height - child.height).abs() < 0.01, "{h:?}/{v:?}");
            }
        }
    }

    #[test]
    fn stretch_clamps_to_zero_on_collapse() {
        let parent_old = rect(0.0, 0.0, 400.0, 300.0);
        let parent_new = rect(0.0, 0.0, 0.0, 0.0); // parent collapsed
        let child = rect(20.0, 20.0, 360.0, 260.0);
        let c = Constraints::stretch();
        let r = resolve_constraints(parent_old, parent_new, child, &c);
        assert_eq!(r.width, 0.0);
        assert_eq!(r.height, 0.0);
    }

    #[test]
    fn constraints_serde_roundtrip() {
        let c = Constraints::new(HorizontalConstraint::LeftAndRight, VerticalConstraint::Center);
        let json = serde_json::to_string(&c).unwrap();
        let back: Constraints = serde_json::from_str(&json).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn convenience_scale() {
        let c = Constraints::scale();
        assert_eq!(c.horizontal, HorizontalConstraint::Scale);
        assert_eq!(c.vertical, VerticalConstraint::Scale);
    }

    #[test]
    fn convenience_center() {
        let c = Constraints::center();
        assert_eq!(c.horizontal, HorizontalConstraint::Center);
        assert_eq!(c.vertical, VerticalConstraint::Center);
    }

    #[test]
    fn convenience_stretch() {
        let c = Constraints::stretch();
        assert_eq!(c.horizontal, HorizontalConstraint::LeftAndRight);
        assert_eq!(c.vertical, VerticalConstraint::TopAndBottom);
    }

    #[test]
    fn pin_right_shrink_parent() {
        let parent_old = rect(0.0, 0.0, 400.0, 300.0);
        let parent_new = rect(0.0, 0.0, 200.0, 300.0);
        // child 50px margin from right, width=100
        let child = rect(250.0, 0.0, 100.0, 50.0);
        let c = Constraints::new(HorizontalConstraint::Right, VerticalConstraint::Top);
        let r = resolve_constraints(parent_old, parent_new, child, &c);
        assert_eq!(r.x, 50.0); // 200 - 50 - 100
    }

    #[test]
    fn center_offset_from_center() {
        // Child is 20px right of center
        let parent_old = rect(0.0, 0.0, 200.0, 200.0);
        let parent_new = rect(0.0, 0.0, 400.0, 200.0);
        // old center=100, child center=120 (+20), child x=70, w=100
        let child = rect(70.0, 0.0, 100.0, 50.0);
        let c = Constraints::new(HorizontalConstraint::Center, VerticalConstraint::Top);
        let r = resolve_constraints(parent_old, parent_new, child, &c);
        // new center=200+20=220, x=220-50=170
        assert_eq!(r.x, 170.0);
    }

    #[test]
    fn scale_both_axes_doubles() {
        let parent_old = rect(0.0, 0.0, 100.0, 100.0);
        let parent_new = rect(0.0, 0.0, 200.0, 200.0);
        let child = rect(10.0, 10.0, 80.0, 80.0);
        let c = Constraints::scale();
        let r = resolve_constraints(parent_old, parent_new, child, &c);
        assert_eq!(r.x, 20.0);
        assert_eq!(r.y, 20.0);
        assert_eq!(r.width, 160.0);
        assert_eq!(r.height, 160.0);
    }
}
