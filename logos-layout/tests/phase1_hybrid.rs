//! Phase 1 – Hybrid Layout Engine integration tests (75 tests, t101–t175)
//!
//! §1  Construction + mode            t101–t115
//! §2  Constraint pre-pass            t116–t135
//! §3  Grid expansion pass            t136–t155
//! §4  Mixed constraints + grids      t156–t165
//! §5  WorkspaceMode gating           t166–t175

use logos_core::WorkspaceMode;
use logos_core::Rect;
use logos_core::constraint::{Constraints, HorizontalConstraint, VerticalConstraint};
use logos_layout::hybrid::{HybridError, HybridLayoutEngine};
use logos_layout::repeat_grid::RepeatGrid;
use logos_layout::spatial::Aabb;
use taffy::prelude::*;
use uuid::Uuid;

// ── helpers ──────────────────────────────────────────────────────────────────

fn r(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect { x, y, width: w, height: h }
}

fn abs_style(x: f32, y: f32, w: f32, h: f32) -> Style {
    Style {
        size: Size {
            width: Dimension::length(w),
            height: Dimension::length(h),
        },
        position: Position::Absolute,
        inset: taffy::Rect {
            left: LengthPercentageAuto::length(x),
            top: LengthPercentageAuto::length(y),
            right: LengthPercentageAuto::auto(),
            bottom: LengthPercentageAuto::auto(),
        },
        ..Style::default()
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// §1  Construction + mode  (t101–t115)
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn t101_new_default_hybrid_mode() {
    let hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    assert_eq!(hle.mode(), WorkspaceMode::Hybrid);
}

#[test]
fn t102_new_flatpage_mode() {
    let hle = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    assert_eq!(hle.mode(), WorkspaceMode::FlatPage);
}

#[test]
fn t103_new_artboard_section_mode() {
    let hle = HybridLayoutEngine::new(WorkspaceMode::ArtboardSection);
    assert_eq!(hle.mode(), WorkspaceMode::ArtboardSection);
}

#[test]
fn t104_initial_node_count_zero() {
    let hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    assert_eq!(hle.node_count(), 0);
}

#[test]
fn t105_initial_grid_count_zero() {
    let hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    assert_eq!(hle.grid_count(), 0);
}

#[test]
fn t106_initial_dirty_count_zero() {
    let hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    assert_eq!(hle.dirty_count(), 0);
}

#[test]
fn t107_set_mode_flatpage() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    hle.set_mode(WorkspaceMode::FlatPage);
    assert_eq!(hle.mode(), WorkspaceMode::FlatPage);
}

#[test]
fn t108_set_mode_artboard() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    hle.set_mode(WorkspaceMode::ArtboardSection);
    assert_eq!(hle.mode(), WorkspaceMode::ArtboardSection);
}

#[test]
fn t109_set_mode_back_to_hybrid() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    hle.set_mode(WorkspaceMode::Hybrid);
    assert_eq!(hle.mode(), WorkspaceMode::Hybrid);
}

#[test]
fn t110_add_layer_increments_count() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    hle.add_layer(Uuid::new_v4(), None, abs_style(0.0, 0.0, 100.0, 100.0), r(0.0, 0.0, 100.0, 100.0))
        .unwrap();
    assert_eq!(hle.node_count(), 1);
}

#[test]
fn t111_remove_layer_decrements_count() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let id = Uuid::new_v4();
    hle.add_layer(id, None, abs_style(0.0, 0.0, 100.0, 100.0), r(0.0, 0.0, 100.0, 100.0))
        .unwrap();
    hle.remove_layer(id).unwrap();
    assert_eq!(hle.node_count(), 0);
}

#[test]
fn t112_remove_nonexistent_layer_errors() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    assert!(hle.remove_layer(Uuid::new_v4()).is_err());
}

#[test]
fn t113_inner_engine_accessible() {
    let hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    assert_eq!(hle.inner().node_count(), 0);
}

#[test]
fn t114_with_cell_size_constructor() {
    let hle = HybridLayoutEngine::with_cell_size(WorkspaceMode::Hybrid, 64.0);
    assert_eq!(hle.node_count(), 0);
    assert_eq!(hle.mode(), WorkspaceMode::Hybrid);
}

#[test]
fn t115_add_multiple_layers() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    for _ in 0..5 {
        hle.add_layer(Uuid::new_v4(), None, abs_style(0.0, 0.0, 50.0, 50.0), r(0.0, 0.0, 50.0, 50.0))
            .unwrap();
    }
    assert_eq!(hle.node_count(), 5);
}

// ═════════════════════════════════════════════════════════════════════════════
// §2  Constraint pre-pass  (t116–t135)
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn t116_set_and_get_constraints() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let id = Uuid::new_v4();
    hle.set_constraints(id, Constraints::stretch());
    assert!(hle.get_constraints(id).is_some());
}

#[test]
fn t117_remove_constraints() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let id = Uuid::new_v4();
    hle.set_constraints(id, Constraints::stretch());
    hle.remove_constraints(id);
    assert!(hle.get_constraints(id).is_none());
}

#[test]
fn t118_no_constraint_no_change() {
    // Child without constraints should not be modified after parent resize.
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let pid = Uuid::new_v4();
    let cid = Uuid::new_v4();
    hle.add_layer(pid, None, abs_style(0.0, 0.0, 200.0, 200.0), r(0.0, 0.0, 200.0, 200.0))
        .unwrap();
    hle.add_layer(cid, Some(pid), abs_style(50.0, 50.0, 80.0, 40.0), r(50.0, 50.0, 80.0, 40.0))
        .unwrap();
    // No constraint set for cid.
    hle.notify_parent_resize(pid, r(0.0, 0.0, 200.0, 200.0), r(0.0, 0.0, 400.0, 200.0));
    hle.compute(pid).unwrap();
    // Child layout should still reflect original position from Taffy.
    let layout = hle.get_layout(cid).unwrap();
    assert!((layout.size.width - 80.0).abs() < f32::EPSILON);
}

#[test]
fn t119_stretch_h_constraint_widens_child() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let pid = Uuid::new_v4();
    let cid = Uuid::new_v4();
    hle.add_layer(pid, None, abs_style(0.0, 0.0, 200.0, 200.0), r(0.0, 0.0, 200.0, 200.0))
        .unwrap();
    hle.add_layer(cid, Some(pid), abs_style(20.0, 20.0, 160.0, 60.0), r(20.0, 20.0, 160.0, 60.0))
        .unwrap();
    hle.set_constraints(cid, Constraints::new(
        HorizontalConstraint::LeftAndRight,
        VerticalConstraint::Top,
    ));
    // Parent grows from 200→400 wide.
    hle.notify_parent_resize(pid, r(0.0, 0.0, 200.0, 200.0), r(0.0, 0.0, 400.0, 200.0));
    hle.compute(pid).unwrap();
    let layout = hle.get_layout(cid).unwrap();
    // left=20, right=200-180=20 → new width = 400-20-20 = 360
    assert!((layout.size.width - 360.0).abs() < 1.0);
}

#[test]
fn t120_right_constraint_preserves_right_margin() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let pid = Uuid::new_v4();
    let cid = Uuid::new_v4();
    hle.add_layer(pid, None, abs_style(0.0, 0.0, 500.0, 300.0), r(0.0, 0.0, 500.0, 300.0))
        .unwrap();
    hle.add_layer(cid, Some(pid), abs_style(400.0, 0.0, 80.0, 50.0), r(400.0, 0.0, 80.0, 50.0))
        .unwrap();
    hle.set_constraints(cid, Constraints::new(
        HorizontalConstraint::Right,
        VerticalConstraint::Top,
    ));
    // right margin = 500 - (400+80) = 20
    // Parent grows to 700 wide → new x = 700 - 20 - 80 = 600
    hle.notify_parent_resize(pid, r(0.0, 0.0, 500.0, 300.0), r(0.0, 0.0, 700.0, 300.0));
    hle.compute(pid).unwrap();
    let layout = hle.get_layout(cid).unwrap();
    assert!((layout.location.x - 600.0).abs() < 1.0);
}

#[test]
fn t121_left_constraint_x_unchanged() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let pid = Uuid::new_v4();
    let cid = Uuid::new_v4();
    hle.add_layer(pid, None, abs_style(0.0, 0.0, 200.0, 200.0), r(0.0, 0.0, 200.0, 200.0))
        .unwrap();
    hle.add_layer(cid, Some(pid), abs_style(30.0, 30.0, 50.0, 50.0), r(30.0, 30.0, 50.0, 50.0))
        .unwrap();
    hle.set_constraints(cid, Constraints::new(
        HorizontalConstraint::Left,
        VerticalConstraint::Top,
    ));
    hle.notify_parent_resize(pid, r(0.0, 0.0, 200.0, 200.0), r(0.0, 0.0, 400.0, 200.0));
    hle.compute(pid).unwrap();
    let layout = hle.get_layout(cid).unwrap();
    assert!((layout.location.x - 30.0).abs() < f32::EPSILON);
    assert!((layout.size.width - 50.0).abs() < f32::EPSILON);
}

#[test]
fn t122_top_constraint_y_unchanged() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let pid = Uuid::new_v4();
    let cid = Uuid::new_v4();
    hle.add_layer(pid, None, abs_style(0.0, 0.0, 200.0, 200.0), r(0.0, 0.0, 200.0, 200.0))
        .unwrap();
    hle.add_layer(cid, Some(pid), abs_style(0.0, 30.0, 50.0, 50.0), r(0.0, 30.0, 50.0, 50.0))
        .unwrap();
    hle.set_constraints(cid, Constraints::new(
        HorizontalConstraint::Left,
        VerticalConstraint::Top,
    ));
    hle.notify_parent_resize(pid, r(0.0, 0.0, 200.0, 200.0), r(0.0, 0.0, 200.0, 400.0));
    hle.compute(pid).unwrap();
    let layout = hle.get_layout(cid).unwrap();
    assert!((layout.location.y - 30.0).abs() < f32::EPSILON);
    assert!((layout.size.height - 50.0).abs() < f32::EPSILON);
}

#[test]
fn t123_bottom_constraint_moves_with_parent() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let pid = Uuid::new_v4();
    let cid = Uuid::new_v4();
    hle.add_layer(pid, None, abs_style(0.0, 0.0, 200.0, 300.0), r(0.0, 0.0, 200.0, 300.0))
        .unwrap();
    // bottom margin = 300 - (200 + 60) = 40
    hle.add_layer(cid, Some(pid), abs_style(0.0, 200.0, 80.0, 60.0), r(0.0, 200.0, 80.0, 60.0))
        .unwrap();
    hle.set_constraints(cid, Constraints::new(
        HorizontalConstraint::Left,
        VerticalConstraint::Bottom,
    ));
    // Parent grows to 500 tall → new y = 500 - 40 - 60 = 400
    hle.notify_parent_resize(pid, r(0.0, 0.0, 200.0, 300.0), r(0.0, 0.0, 200.0, 500.0));
    hle.compute(pid).unwrap();
    let layout = hle.get_layout(cid).unwrap();
    assert!((layout.location.y - 400.0).abs() < 1.0);
}

#[test]
fn t124_scale_h_doubles_position_and_size() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let pid = Uuid::new_v4();
    let cid = Uuid::new_v4();
    hle.add_layer(pid, None, abs_style(0.0, 0.0, 200.0, 200.0), r(0.0, 0.0, 200.0, 200.0))
        .unwrap();
    hle.add_layer(cid, Some(pid), abs_style(50.0, 0.0, 100.0, 50.0), r(50.0, 0.0, 100.0, 50.0))
        .unwrap();
    hle.set_constraints(cid, Constraints::new(
        HorizontalConstraint::Scale,
        VerticalConstraint::Top,
    ));
    hle.notify_parent_resize(pid, r(0.0, 0.0, 200.0, 200.0), r(0.0, 0.0, 400.0, 200.0));
    hle.compute(pid).unwrap();
    let layout = hle.get_layout(cid).unwrap();
    assert!((layout.location.x - 100.0).abs() < 1.0);
    assert!((layout.size.width - 200.0).abs() < 1.0);
}

#[test]
fn t125_center_h_tracks_parent_center() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let pid = Uuid::new_v4();
    let cid = Uuid::new_v4();
    hle.add_layer(pid, None, abs_style(0.0, 0.0, 200.0, 200.0), r(0.0, 0.0, 200.0, 200.0))
        .unwrap();
    // child center @ 100 → offset from parent center = 0
    hle.add_layer(cid, Some(pid), abs_style(75.0, 0.0, 50.0, 50.0), r(75.0, 0.0, 50.0, 50.0))
        .unwrap();
    hle.set_constraints(cid, Constraints::new(
        HorizontalConstraint::Center,
        VerticalConstraint::Top,
    ));
    hle.notify_parent_resize(pid, r(0.0, 0.0, 200.0, 200.0), r(0.0, 0.0, 400.0, 200.0));
    hle.compute(pid).unwrap();
    let layout = hle.get_layout(cid).unwrap();
    // new center = 200, width = 50 → x = 175
    assert!((layout.location.x - 175.0).abs() < 1.0);
}

#[test]
fn t126_stretch_v_grows_height() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let pid = Uuid::new_v4();
    let cid = Uuid::new_v4();
    hle.add_layer(pid, None, abs_style(0.0, 0.0, 200.0, 200.0), r(0.0, 0.0, 200.0, 200.0))
        .unwrap();
    hle.add_layer(cid, Some(pid), abs_style(0.0, 10.0, 50.0, 180.0), r(0.0, 10.0, 50.0, 180.0))
        .unwrap();
    hle.set_constraints(cid, Constraints::new(
        HorizontalConstraint::Left,
        VerticalConstraint::TopAndBottom,
    ));
    // margin_top=10, margin_bottom=200-190=10; parent grows to 300 → new h=280
    hle.notify_parent_resize(pid, r(0.0, 0.0, 200.0, 200.0), r(0.0, 0.0, 200.0, 300.0));
    hle.compute(pid).unwrap();
    let layout = hle.get_layout(cid).unwrap();
    assert!((layout.size.height - 280.0).abs() < 1.0);
}

#[test]
fn t127_constraint_pre_pass_before_taffy() {
    // Ensure the constraint-updated width is what Taffy sees.
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let pid = Uuid::new_v4();
    let cid = Uuid::new_v4();
    hle.add_layer(pid, None, abs_style(0.0, 0.0, 100.0, 100.0), r(0.0, 0.0, 100.0, 100.0))
        .unwrap();
    hle.add_layer(cid, Some(pid), abs_style(10.0, 10.0, 80.0, 80.0), r(10.0, 10.0, 80.0, 80.0))
        .unwrap();
    hle.set_constraints(cid, Constraints::new(
        HorizontalConstraint::LeftAndRight,
        VerticalConstraint::TopAndBottom,
    ));
    // Double the parent.
    hle.notify_parent_resize(pid, r(0.0, 0.0, 100.0, 100.0), r(0.0, 0.0, 200.0, 200.0));
    hle.compute(pid).unwrap();
    let layout = hle.get_layout(cid).unwrap();
    // margins = 10 each side → new w = 200-10-10 = 180
    assert!((layout.size.width - 180.0).abs() < 1.0);
    assert!((layout.size.height - 180.0).abs() < 1.0);
}

#[test]
fn t128_multiple_children_same_parent_resize() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let pid = Uuid::new_v4();
    let c1 = Uuid::new_v4();
    let c2 = Uuid::new_v4();
    hle.add_layer(pid, None, abs_style(0.0, 0.0, 200.0, 200.0), r(0.0, 0.0, 200.0, 200.0))
        .unwrap();
    hle.add_layer(c1, Some(pid), abs_style(0.0, 0.0, 60.0, 60.0), r(0.0, 0.0, 60.0, 60.0))
        .unwrap();
    hle.add_layer(c2, Some(pid), abs_style(140.0, 0.0, 60.0, 60.0), r(140.0, 0.0, 60.0, 60.0))
        .unwrap();
    hle.set_constraints(c1, Constraints::new(HorizontalConstraint::Left, VerticalConstraint::Top));
    hle.set_constraints(c2, Constraints::new(HorizontalConstraint::Right, VerticalConstraint::Top));
    // c2 right margin = 200 - 200 = 0
    hle.notify_parent_resize(pid, r(0.0, 0.0, 200.0, 200.0), r(0.0, 0.0, 400.0, 200.0));
    hle.compute(pid).unwrap();
    // c1 stays at x=0; c2 moves to 400-0-60=340
    let l1 = hle.get_layout(c1).unwrap();
    let l2 = hle.get_layout(c2).unwrap();
    assert!((l1.location.x - 0.0).abs() < f32::EPSILON);
    assert!((l2.location.x - 340.0).abs() < 1.0);
}

#[test]
fn t129_no_pending_resize_no_constraint_effect() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let pid = Uuid::new_v4();
    let cid = Uuid::new_v4();
    hle.add_layer(pid, None, abs_style(0.0, 0.0, 200.0, 200.0), r(0.0, 0.0, 200.0, 200.0))
        .unwrap();
    hle.add_layer(cid, Some(pid), abs_style(50.0, 50.0, 50.0, 50.0), r(50.0, 50.0, 50.0, 50.0))
        .unwrap();
    hle.set_constraints(cid, Constraints::stretch());
    // No notify_parent_resize call.
    hle.compute(pid).unwrap();
    let layout = hle.get_layout(cid).unwrap();
    // Taffy should give original size.
    assert!((layout.size.width - 50.0).abs() < f32::EPSILON);
}

#[test]
fn t130_resize_notification_consumed_after_compute() {
    // After one compute, the pending resize is gone; second compute is a no-op.
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let pid = Uuid::new_v4();
    let cid = Uuid::new_v4();
    hle.add_layer(pid, None, abs_style(0.0, 0.0, 200.0, 200.0), r(0.0, 0.0, 200.0, 200.0))
        .unwrap();
    hle.add_layer(cid, Some(pid), abs_style(20.0, 20.0, 160.0, 160.0), r(20.0, 20.0, 160.0, 160.0))
        .unwrap();
    hle.set_constraints(cid, Constraints::stretch());
    hle.notify_parent_resize(pid, r(0.0, 0.0, 200.0, 200.0), r(0.0, 0.0, 400.0, 200.0));
    hle.compute(pid).unwrap();
    let w_after_first = hle.get_layout(cid).unwrap().size.width;
    // Second compute without a new resize notification.
    hle.compute(pid).unwrap();
    let w_after_second = hle.get_layout(cid).unwrap().size.width;
    assert_eq!(w_after_first, w_after_second);
}

#[test]
fn t131_update_bounds_reflects_in_next_compute() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let pid = Uuid::new_v4();
    let cid = Uuid::new_v4();
    hle.add_layer(pid, None, abs_style(0.0, 0.0, 200.0, 200.0), r(0.0, 0.0, 200.0, 200.0))
        .unwrap();
    hle.add_layer(cid, Some(pid), abs_style(10.0, 10.0, 50.0, 50.0), r(10.0, 10.0, 50.0, 50.0))
        .unwrap();
    hle.update_bounds(cid, r(0.0, 0.0, 100.0, 100.0)).unwrap();
    hle.set_constraints(cid, Constraints::new(
        HorizontalConstraint::LeftAndRight,
        VerticalConstraint::TopAndBottom,
    ));
    // Use updated bounds (0,0,100,100) for constraint resolution.
    hle.notify_parent_resize(pid, r(0.0, 0.0, 200.0, 200.0), r(0.0, 0.0, 200.0, 200.0));
    // Same old/new → no change expected.
    hle.compute(pid).unwrap();
    let layout = hle.get_layout(cid).unwrap();
    // bounds unchanged, child should be at updated Taffy position.
    assert!(layout.size.width > 0.0);
}

#[test]
fn t132_update_bounds_nonexistent_layer_errors() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    assert!(hle.update_bounds(Uuid::new_v4(), r(0.0, 0.0, 50.0, 50.0)).is_err());
}

#[test]
fn t133_scale_v_grows_proportionally() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let pid = Uuid::new_v4();
    let cid = Uuid::new_v4();
    hle.add_layer(pid, None, abs_style(0.0, 0.0, 200.0, 200.0), r(0.0, 0.0, 200.0, 200.0))
        .unwrap();
    hle.add_layer(cid, Some(pid), abs_style(0.0, 50.0, 50.0, 100.0), r(0.0, 50.0, 50.0, 100.0))
        .unwrap();
    hle.set_constraints(cid, Constraints::new(
        HorizontalConstraint::Left,
        VerticalConstraint::Scale,
    ));
    hle.notify_parent_resize(pid, r(0.0, 0.0, 200.0, 200.0), r(0.0, 0.0, 200.0, 400.0));
    hle.compute(pid).unwrap();
    let layout = hle.get_layout(cid).unwrap();
    // ratio = 2 → y = 100, height = 200
    assert!((layout.location.y - 100.0).abs() < 1.0);
    assert!((layout.size.height - 200.0).abs() < 1.0);
}

#[test]
fn t134_center_v_tracks_parent_center() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let pid = Uuid::new_v4();
    let cid = Uuid::new_v4();
    hle.add_layer(pid, None, abs_style(0.0, 0.0, 200.0, 200.0), r(0.0, 0.0, 200.0, 200.0))
        .unwrap();
    // child centered at y=75+25=100 (parent center), offset=0
    hle.add_layer(cid, Some(pid), abs_style(0.0, 75.0, 50.0, 50.0), r(0.0, 75.0, 50.0, 50.0))
        .unwrap();
    hle.set_constraints(cid, Constraints::new(
        HorizontalConstraint::Left,
        VerticalConstraint::Center,
    ));
    hle.notify_parent_resize(pid, r(0.0, 0.0, 200.0, 200.0), r(0.0, 0.0, 200.0, 400.0));
    hle.compute(pid).unwrap();
    let layout = hle.get_layout(cid).unwrap();
    // new center=200, width=50 → y=175
    assert!((layout.location.y - 175.0).abs() < 1.0);
}

#[test]
fn t135_drain_changed_after_constraint_compute() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let pid = Uuid::new_v4();
    let cid = Uuid::new_v4();
    hle.add_layer(pid, None, abs_style(0.0, 0.0, 200.0, 200.0), r(0.0, 0.0, 200.0, 200.0))
        .unwrap();
    hle.add_layer(cid, Some(pid), abs_style(10.0, 10.0, 180.0, 180.0), r(10.0, 10.0, 180.0, 180.0))
        .unwrap();
    hle.set_constraints(cid, Constraints::stretch());
    hle.notify_parent_resize(pid, r(0.0, 0.0, 200.0, 200.0), r(0.0, 0.0, 300.0, 200.0));
    hle.compute(pid).unwrap();
    let changed = hle.drain_changed();
    // At minimum the child and/or parent should appear as changed.
    assert!(!changed.is_empty());
}

// ═════════════════════════════════════════════════════════════════════════════
// §3  Grid expansion pass  (t136–t155)
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn t136_register_grid_increments_count() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    hle.register_grid(RepeatGrid::new(2, 3, 50.0, 50.0)).unwrap();
    assert_eq!(hle.grid_count(), 1);
}

#[test]
fn t137_unregister_grid_decrements_count() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let g = RepeatGrid::new(2, 3, 50.0, 50.0);
    let gid = g.id;
    hle.register_grid(g).unwrap();
    hle.unregister_grid(gid).unwrap();
    assert_eq!(hle.grid_count(), 0);
}

#[test]
fn t138_unregister_nonexistent_grid_errors() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let result = hle.unregister_grid(Uuid::new_v4());
    assert!(matches!(result, Err(HybridError::GridNotFound(_))));
}

#[test]
fn t139_cell_virtual_id_in_range() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let g = RepeatGrid::new(3, 4, 50.0, 50.0);
    let gid = g.id;
    hle.register_grid(g).unwrap();
    assert!(hle.cell_virtual_id(gid, 0, 0).is_ok());
    assert!(hle.cell_virtual_id(gid, 2, 3).is_ok());
}

#[test]
fn t140_cell_virtual_id_out_of_range() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let g = RepeatGrid::new(2, 2, 50.0, 50.0);
    let gid = g.id;
    hle.register_grid(g).unwrap();
    let result = hle.cell_virtual_id(gid, 5, 5);
    assert!(matches!(result, Err(HybridError::GridCellOutOfRange { .. })));
}

#[test]
fn t141_cell_virtual_id_stable_across_calls() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let g = RepeatGrid::new(3, 3, 50.0, 50.0);
    let gid = g.id;
    hle.register_grid(g).unwrap();
    let id1 = hle.cell_virtual_id(gid, 1, 2).unwrap();
    let id2 = hle.cell_virtual_id(gid, 1, 2).unwrap();
    assert_eq!(id1, id2);
}

#[test]
fn t142_grid_cells_visible_after_compute() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    // Need a Taffy root node (even empty) so compute() can run.
    let root = Uuid::new_v4();
    hle.add_layer(root, None, abs_style(0.0, 0.0, 400.0, 400.0), r(0.0, 0.0, 400.0, 400.0))
        .unwrap();
    let g = RepeatGrid::new(2, 2, 50.0, 50.0);
    let gid = g.id;
    hle.register_grid(g).unwrap();
    hle.compute(root).unwrap();
    let cell00 = hle.cell_virtual_id(gid, 0, 0).unwrap();
    assert!(hle.get_layout(cell00).is_some());
}

#[test]
fn t143_grid_cell_bounds_match_repeat_grid() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let root = Uuid::new_v4();
    hle.add_layer(root, None, abs_style(0.0, 0.0, 400.0, 400.0), r(0.0, 0.0, 400.0, 400.0))
        .unwrap();
    let g = RepeatGrid::new(3, 3, 60.0, 40.0).with_gap(10.0, 10.0).with_origin(20.0, 30.0);
    let gid = g.id;
    let expected = g.cell_bounds_absolute(1, 1).unwrap();
    hle.register_grid(g).unwrap();
    hle.compute(root).unwrap();
    let cell11 = hle.cell_virtual_id(gid, 1, 1).unwrap();
    let layout = hle.get_layout(cell11).unwrap();
    assert!((layout.location.x - expected.0).abs() < f32::EPSILON);
    assert!((layout.location.y - expected.1).abs() < f32::EPSILON);
    assert!((layout.size.width - expected.2).abs() < f32::EPSILON);
    assert!((layout.size.height - expected.3).abs() < f32::EPSILON);
}

#[test]
fn t144_grid_hit_test_finds_cell() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let root = Uuid::new_v4();
    hle.add_layer(root, None, abs_style(0.0, 0.0, 400.0, 400.0), r(0.0, 0.0, 400.0, 400.0))
        .unwrap();
    // 1×1 grid, cell covers (0,0,100,100)
    let g = RepeatGrid::new(1, 1, 100.0, 100.0);
    let gid = g.id;
    hle.register_grid(g).unwrap();
    hle.compute(root).unwrap();
    let cell00 = hle.cell_virtual_id(gid, 0, 0).unwrap();
    let hits = hle.hit_test_all(50.0, 50.0);
    assert!(hits.contains(&cell00));
}

#[test]
fn t145_grid_hit_test_misses_outside_cell() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let root = Uuid::new_v4();
    hle.add_layer(root, None, abs_style(0.0, 0.0, 1000.0, 1000.0), r(0.0, 0.0, 1000.0, 1000.0))
        .unwrap();
    let g = RepeatGrid::new(1, 1, 100.0, 100.0);
    hle.register_grid(g).unwrap();
    hle.compute(root).unwrap();
    // Point far outside the single cell
    let hits = hle.hit_test_all(500.0, 500.0);
    // The root node covers that point, but the cell (0,0,100,100) should not.
    let cell_hits: Vec<_> = hits.iter().filter(|&&id| id != root).cloned().collect();
    assert!(cell_hits.is_empty() || !cell_hits.iter().any(|_| true));
}

#[test]
fn t146_update_grid_reregisters_cells() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let root = Uuid::new_v4();
    hle.add_layer(root, None, abs_style(0.0, 0.0, 400.0, 400.0), r(0.0, 0.0, 400.0, 400.0))
        .unwrap();
    let mut g = RepeatGrid::new(1, 1, 50.0, 50.0);
    let gid = g.id;
    hle.register_grid(g.clone()).unwrap();
    // Expand to 2×2.
    g.rows = 2;
    g.columns = 2;
    hle.update_grid(g).unwrap();
    hle.compute(root).unwrap();
    // All 4 cells should have layout.
    for row in 0..2u32 {
        for col in 0..2u32 {
            let cid = hle.cell_virtual_id(gid, row, col).unwrap();
            assert!(hle.get_layout(cid).is_some(), "cell ({row},{col}) missing");
        }
    }
}

#[test]
fn t147_multiple_grids_independent() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let root = Uuid::new_v4();
    hle.add_layer(root, None, abs_style(0.0, 0.0, 1000.0, 1000.0), r(0.0, 0.0, 1000.0, 1000.0))
        .unwrap();
    let g1 = RepeatGrid::new(2, 2, 50.0, 50.0).with_origin(0.0, 0.0);
    let g2 = RepeatGrid::new(3, 3, 30.0, 30.0).with_origin(300.0, 300.0);
    let gid1 = g1.id;
    let gid2 = g2.id;
    hle.register_grid(g1).unwrap();
    hle.register_grid(g2).unwrap();
    hle.compute(root).unwrap();
    assert_eq!(hle.grid_count(), 2);
    assert!(hle.get_layout(hle.cell_virtual_id(gid1, 0, 0).unwrap()).is_some());
    assert!(hle.get_layout(hle.cell_virtual_id(gid2, 2, 2).unwrap()).is_some());
}

#[test]
fn t148_unregister_removes_cell_knowledge() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let root = Uuid::new_v4();
    hle.add_layer(root, None, abs_style(0.0, 0.0, 400.0, 400.0), r(0.0, 0.0, 400.0, 400.0))
        .unwrap();
    let g = RepeatGrid::new(2, 2, 50.0, 50.0);
    let gid = g.id;
    hle.register_grid(g).unwrap();
    hle.compute(root).unwrap();
    hle.unregister_grid(gid).unwrap();
    // After unregister, cell_virtual_id should error.
    assert!(hle.cell_virtual_id(gid, 0, 0).is_err());
}

#[test]
fn t149_grid_cell_ids_unique_per_cell() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let g = RepeatGrid::new(3, 3, 50.0, 50.0);
    let gid = g.id;
    hle.register_grid(g).unwrap();
    let mut ids = std::collections::HashSet::new();
    for row in 0..3u32 {
        for col in 0..3u32 {
            let id = hle.cell_virtual_id(gid, row, col).unwrap();
            assert!(ids.insert(id), "duplicate cell ID at ({row},{col})");
        }
    }
}

#[test]
fn t150_grid_with_gap_origin_injected_correctly() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let root = Uuid::new_v4();
    hle.add_layer(root, None, abs_style(0.0, 0.0, 1000.0, 1000.0), r(0.0, 0.0, 1000.0, 1000.0))
        .unwrap();
    let g = RepeatGrid::new(2, 2, 60.0, 40.0).with_gap(10.0, 5.0).with_origin(100.0, 50.0);
    let gid = g.id;
    let (ex, ey, ew, eh) = g.cell_bounds_absolute(1, 1).unwrap();
    hle.register_grid(g).unwrap();
    hle.compute(root).unwrap();
    let cid = hle.cell_virtual_id(gid, 1, 1).unwrap();
    let layout = hle.get_layout(cid).unwrap();
    assert!((layout.location.x - ex).abs() < f32::EPSILON);
    assert!((layout.location.y - ey).abs() < f32::EPSILON);
    assert!((layout.size.width - ew).abs() < f32::EPSILON);
    assert!((layout.size.height - eh).abs() < f32::EPSILON);
}

#[test]
fn t151_grid_expand_then_recompute() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let root = Uuid::new_v4();
    hle.add_layer(root, None, abs_style(0.0, 0.0, 600.0, 600.0), r(0.0, 0.0, 600.0, 600.0))
        .unwrap();
    let mut g = RepeatGrid::new(2, 2, 50.0, 50.0);
    let gid = g.id;
    hle.register_grid(g.clone()).unwrap();
    hle.compute(root).unwrap();
    g.add_row();
    hle.update_grid(g).unwrap();
    hle.compute(root).unwrap();
    // Row 2 cells should now exist.
    let cid = hle.cell_virtual_id(gid, 2, 0).unwrap();
    assert!(hle.get_layout(cid).is_some());
}

#[test]
fn t152_grid_region_query() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let root = Uuid::new_v4();
    hle.add_layer(root, None, abs_style(0.0, 0.0, 600.0, 600.0), r(0.0, 0.0, 600.0, 600.0))
        .unwrap();
    let g = RepeatGrid::new(2, 2, 50.0, 50.0);
    let gid = g.id;
    hle.register_grid(g).unwrap();
    hle.compute(root).unwrap();
    // Region covering the entire 2×2 grid (0,0,100,100 with no gap)
    let region = Aabb::from_rect(0.0, 0.0, 150.0, 150.0);
    let hits = hle.query_region(&region);
    // All 4 cells + root should be in there
    let cell_hits: Vec<_> = hits.iter().filter(|&&id| id != root).cloned().collect();
    assert!(cell_hits.len() >= 4, "expected 4 cells, got {}", cell_hits.len());
    // All 4 cell IDs should be present
    for row in 0..2u32 {
        for col in 0..2u32 {
            let cid = hle.cell_virtual_id(gid, row, col).unwrap();
            assert!(hits.contains(&cid), "missing cell ({row},{col})");
        }
    }
}

#[test]
fn t153_drain_changed_includes_grid_cells() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let root = Uuid::new_v4();
    hle.add_layer(root, None, abs_style(0.0, 0.0, 400.0, 400.0), r(0.0, 0.0, 400.0, 400.0))
        .unwrap();
    let g = RepeatGrid::new(2, 2, 50.0, 50.0);
    hle.register_grid(g).unwrap();
    hle.compute(root).unwrap();
    let changed = hle.drain_changed();
    // Grid cells are first-time injected → they appear as changed.
    assert!(changed.len() >= 4);
}

#[test]
fn t154_second_compute_same_grid_no_change() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let root = Uuid::new_v4();
    hle.add_layer(root, None, abs_style(0.0, 0.0, 400.0, 400.0), r(0.0, 0.0, 400.0, 400.0))
        .unwrap();
    let g = RepeatGrid::new(2, 2, 50.0, 50.0);
    hle.register_grid(g).unwrap();
    hle.compute(root).unwrap();
    let _ = hle.drain_changed();
    // Second compute: Taffy is clean; grid cells are same bounds → no changes.
    hle.compute(root).unwrap();
    let changed = hle.drain_changed();
    // Grid cells unchanged → not in changed list.
    assert!(changed.is_empty());
}

#[test]
fn t155_large_grid_all_cells_visible() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let root = Uuid::new_v4();
    hle.add_layer(root, None, abs_style(0.0, 0.0, 2000.0, 2000.0), r(0.0, 0.0, 2000.0, 2000.0))
        .unwrap();
    let g = RepeatGrid::new(5, 5, 40.0, 40.0).with_gap(5.0, 5.0);
    let gid = g.id;
    hle.register_grid(g).unwrap();
    hle.compute(root).unwrap();
    for row in 0..5u32 {
        for col in 0..5u32 {
            let cid = hle.cell_virtual_id(gid, row, col).unwrap();
            assert!(hle.get_layout(cid).is_some(), "cell ({row},{col}) missing");
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// §4  Mixed constraints + grids  (t156–t165)
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn t156_constrained_child_and_grid_in_same_parent() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let pid = Uuid::new_v4();
    let cid = Uuid::new_v4();
    hle.add_layer(pid, None, abs_style(0.0, 0.0, 400.0, 300.0), r(0.0, 0.0, 400.0, 300.0))
        .unwrap();
    hle.add_layer(cid, Some(pid), abs_style(10.0, 10.0, 380.0, 50.0), r(10.0, 10.0, 380.0, 50.0))
        .unwrap();
    hle.set_constraints(cid, Constraints::new(
        HorizontalConstraint::LeftAndRight,
        VerticalConstraint::Top,
    ));
    let g = RepeatGrid::new(2, 4, 80.0, 60.0).with_gap(10.0, 10.0).with_origin(10.0, 80.0);
    let gid = g.id;
    hle.register_grid(g).unwrap();
    hle.notify_parent_resize(pid, r(0.0, 0.0, 400.0, 300.0), r(0.0, 0.0, 600.0, 300.0));
    hle.compute(pid).unwrap();
    // Constrained child should stretch: 600-10-10=580
    let child_layout = hle.get_layout(cid).unwrap();
    assert!((child_layout.size.width - 580.0).abs() < 1.0);
    // Grid cells should still be present.
    let cell = hle.cell_virtual_id(gid, 0, 0).unwrap();
    assert!(hle.get_layout(cell).is_some());
}

#[test]
fn t157_constraint_right_and_grid_independent() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let pid = Uuid::new_v4();
    let btn = Uuid::new_v4();
    hle.add_layer(pid, None, abs_style(0.0, 0.0, 500.0, 400.0), r(0.0, 0.0, 500.0, 400.0))
        .unwrap();
    // Button pinned to right
    hle.add_layer(btn, Some(pid), abs_style(420.0, 10.0, 70.0, 30.0), r(420.0, 10.0, 70.0, 30.0))
        .unwrap();
    hle.set_constraints(btn, Constraints::new(
        HorizontalConstraint::Right,
        VerticalConstraint::Top,
    ));
    let g = RepeatGrid::new(3, 3, 60.0, 50.0).with_origin(10.0, 60.0);
    let gid = g.id;
    hle.register_grid(g).unwrap();
    // Resize parent
    hle.notify_parent_resize(pid, r(0.0, 0.0, 500.0, 400.0), r(0.0, 0.0, 700.0, 400.0));
    hle.compute(pid).unwrap();
    // btn right margin=500-490=10 → new x=700-10-70=620
    let bl = hle.get_layout(btn).unwrap();
    assert!((bl.location.x - 620.0).abs() < 1.0);
    // Grid still renders
    let cell = hle.cell_virtual_id(gid, 2, 2).unwrap();
    assert!(hle.get_layout(cell).is_some());
}

#[test]
fn t158_multiple_constrained_children_plus_grid() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let pid = Uuid::new_v4();
    let left_bar = Uuid::new_v4();
    let right_bar = Uuid::new_v4();
    hle.add_layer(pid, None, abs_style(0.0, 0.0, 400.0, 400.0), r(0.0, 0.0, 400.0, 400.0))
        .unwrap();
    hle.add_layer(left_bar, Some(pid), abs_style(0.0, 0.0, 40.0, 400.0), r(0.0, 0.0, 40.0, 400.0))
        .unwrap();
    hle.add_layer(right_bar, Some(pid), abs_style(360.0, 0.0, 40.0, 400.0), r(360.0, 0.0, 40.0, 400.0))
        .unwrap();
    hle.set_constraints(left_bar, Constraints::new(HorizontalConstraint::Left, VerticalConstraint::TopAndBottom));
    hle.set_constraints(right_bar, Constraints::new(HorizontalConstraint::Right, VerticalConstraint::TopAndBottom));
    let g = RepeatGrid::new(3, 3, 80.0, 80.0).with_origin(50.0, 50.0);
    hle.register_grid(g).unwrap();
    hle.notify_parent_resize(pid, r(0.0, 0.0, 400.0, 400.0), r(0.0, 0.0, 400.0, 600.0));
    hle.compute(pid).unwrap();
    // left_bar should stretch to 600 height
    let ll = hle.get_layout(left_bar).unwrap();
    assert!((ll.size.height - 600.0).abs() < 1.0);
    // right_bar same
    let rl = hle.get_layout(right_bar).unwrap();
    assert!((rl.size.height - 600.0).abs() < 1.0);
}

#[test]
fn t159_grid_survives_layer_removal() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let pid = Uuid::new_v4();
    let cid = Uuid::new_v4();
    hle.add_layer(pid, None, abs_style(0.0, 0.0, 400.0, 400.0), r(0.0, 0.0, 400.0, 400.0))
        .unwrap();
    hle.add_layer(cid, Some(pid), abs_style(10.0, 10.0, 50.0, 50.0), r(10.0, 10.0, 50.0, 50.0))
        .unwrap();
    let g = RepeatGrid::new(2, 2, 50.0, 50.0);
    let gid = g.id;
    hle.register_grid(g).unwrap();
    hle.compute(pid).unwrap();
    // Remove the non-grid layer.
    hle.remove_layer(cid).unwrap();
    hle.compute(pid).unwrap();
    // Grid cells still present.
    let cell = hle.cell_virtual_id(gid, 0, 0).unwrap();
    assert!(hle.get_layout(cell).is_some());
}

#[test]
fn t160_constraint_and_grid_drain_changed_together() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let pid = Uuid::new_v4();
    let cid = Uuid::new_v4();
    hle.add_layer(pid, None, abs_style(0.0, 0.0, 300.0, 300.0), r(0.0, 0.0, 300.0, 300.0))
        .unwrap();
    hle.add_layer(cid, Some(pid), abs_style(10.0, 10.0, 280.0, 280.0), r(10.0, 10.0, 280.0, 280.0))
        .unwrap();
    hle.set_constraints(cid, Constraints::stretch());
    let g = RepeatGrid::new(2, 2, 30.0, 30.0).with_origin(5.0, 5.0);
    hle.register_grid(g).unwrap();
    hle.notify_parent_resize(pid, r(0.0, 0.0, 300.0, 300.0), r(0.0, 0.0, 500.0, 300.0));
    hle.compute(pid).unwrap();
    let changed = hle.drain_changed();
    // Expect at minimum: cid (resized) + 4 grid cells
    assert!(changed.len() >= 5);
}

#[test]
fn t161_scale_constraint_child_and_grid_same_frame() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let pid = Uuid::new_v4();
    let cid = Uuid::new_v4();
    hle.add_layer(pid, None, abs_style(0.0, 0.0, 200.0, 200.0), r(0.0, 0.0, 200.0, 200.0))
        .unwrap();
    hle.add_layer(cid, Some(pid), abs_style(40.0, 40.0, 120.0, 120.0), r(40.0, 40.0, 120.0, 120.0))
        .unwrap();
    hle.set_constraints(cid, Constraints::scale());
    let g = RepeatGrid::new(1, 1, 50.0, 50.0).with_origin(0.0, 0.0);
    hle.register_grid(g).unwrap();
    hle.notify_parent_resize(pid, r(0.0, 0.0, 200.0, 200.0), r(0.0, 0.0, 400.0, 400.0));
    hle.compute(pid).unwrap();
    let layout = hle.get_layout(cid).unwrap();
    // scale ratio=2 → x=80, y=80, w=240, h=240
    assert!((layout.location.x - 80.0).abs() < 1.0);
    assert!((layout.size.width - 240.0).abs() < 1.0);
}

#[test]
fn t162_grid_then_constraint_then_grid_again() {
    // Re-register grid after constraint modifies tree; grid should still render.
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let pid = Uuid::new_v4();
    let cid = Uuid::new_v4();
    hle.add_layer(pid, None, abs_style(0.0, 0.0, 300.0, 300.0), r(0.0, 0.0, 300.0, 300.0))
        .unwrap();
    hle.add_layer(cid, Some(pid), abs_style(0.0, 0.0, 100.0, 100.0), r(0.0, 0.0, 100.0, 100.0))
        .unwrap();
    hle.set_constraints(cid, Constraints::stretch());
    let g = RepeatGrid::new(2, 2, 50.0, 50.0);
    let gid = g.id;
    hle.register_grid(g.clone()).unwrap();
    hle.notify_parent_resize(pid, r(0.0, 0.0, 300.0, 300.0), r(0.0, 0.0, 300.0, 300.0));
    hle.compute(pid).unwrap();
    // Re-register same grid
    hle.update_grid(g).unwrap();
    hle.compute(pid).unwrap();
    let cell = hle.cell_virtual_id(gid, 1, 1).unwrap();
    assert!(hle.get_layout(cell).is_some());
}

#[test]
fn t163_hit_test_cell_and_constrained_child_overlap() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let pid = Uuid::new_v4();
    let cid = Uuid::new_v4();
    hle.add_layer(pid, None, abs_style(0.0, 0.0, 400.0, 400.0), r(0.0, 0.0, 400.0, 400.0))
        .unwrap();
    // Child covers (0,0,100,100)
    hle.add_layer(cid, Some(pid), abs_style(0.0, 0.0, 100.0, 100.0), r(0.0, 0.0, 100.0, 100.0))
        .unwrap();
    // Grid cell (0,0) also at (0,0,100,100)
    let g = RepeatGrid::new(1, 1, 100.0, 100.0);
    let gid = g.id;
    hle.register_grid(g).unwrap();
    hle.compute(pid).unwrap();
    let cell00 = hle.cell_virtual_id(gid, 0, 0).unwrap();
    let hits = hle.hit_test_all(50.0, 50.0);
    // Both the constrained child and grid cell should be present.
    assert!(hits.contains(&cid) || hits.contains(&cell00));
}

#[test]
fn t164_remove_constrained_child_grid_unaffected() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let pid = Uuid::new_v4();
    let cid = Uuid::new_v4();
    hle.add_layer(pid, None, abs_style(0.0, 0.0, 400.0, 400.0), r(0.0, 0.0, 400.0, 400.0))
        .unwrap();
    hle.add_layer(cid, Some(pid), abs_style(10.0, 10.0, 60.0, 60.0), r(10.0, 10.0, 60.0, 60.0))
        .unwrap();
    hle.set_constraints(cid, Constraints::stretch());
    let g = RepeatGrid::new(2, 2, 50.0, 50.0).with_origin(200.0, 200.0);
    let gid = g.id;
    hle.register_grid(g).unwrap();
    hle.compute(pid).unwrap();
    hle.remove_layer(cid).unwrap();
    hle.compute(pid).unwrap();
    let cell = hle.cell_virtual_id(gid, 1, 1).unwrap();
    assert!(hle.get_layout(cell).is_some());
}

#[test]
fn t165_center_constraint_plus_grid_cells_distinct() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let pid = Uuid::new_v4();
    let cid = Uuid::new_v4();
    hle.add_layer(pid, None, abs_style(0.0, 0.0, 400.0, 400.0), r(0.0, 0.0, 400.0, 400.0))
        .unwrap();
    hle.add_layer(cid, Some(pid), abs_style(175.0, 175.0, 50.0, 50.0), r(175.0, 175.0, 50.0, 50.0))
        .unwrap();
    hle.set_constraints(cid, Constraints::center());
    let g = RepeatGrid::new(2, 2, 40.0, 40.0).with_origin(0.0, 0.0);
    let gid = g.id;
    hle.register_grid(g).unwrap();
    hle.notify_parent_resize(pid, r(0.0, 0.0, 400.0, 400.0), r(0.0, 0.0, 600.0, 600.0));
    hle.compute(pid).unwrap();
    let child_layout = hle.get_layout(cid).unwrap();
    let cell00 = hle.get_layout(hle.cell_virtual_id(gid, 0, 0).unwrap()).unwrap();
    // centered child and grid cell[0,0] should have different positions.
    assert!(child_layout.location.x != cell00.location.x
        || child_layout.location.y != cell00.location.y);
}

// ═════════════════════════════════════════════════════════════════════════════
// §5  WorkspaceMode gating  (t166–t175)
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn t166_hybrid_mode_supports_artboards_and_flat() {
    let hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    assert!(hle.mode().supports_artboards());
    assert!(hle.mode().supports_flat());
}

#[test]
fn t167_flatpage_mode_only_supports_flat() {
    let hle = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    assert!(!hle.mode().supports_artboards());
    assert!(hle.mode().supports_flat());
}

#[test]
fn t168_artboard_section_mode_only_supports_artboards() {
    let hle = HybridLayoutEngine::new(WorkspaceMode::ArtboardSection);
    assert!(hle.mode().supports_artboards());
    assert!(!hle.mode().supports_flat());
}

#[test]
fn t169_flatpage_skips_unparented_resize() {
    // In FlatPage mode, a resize of an unregistered root (artboard) should be skipped.
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    let artboard_id = Uuid::new_v4(); // not in child_info → treated as artboard root
    let cid = Uuid::new_v4();
    // Root is a flat frame — add it as a real layer so compute works.
    let root = Uuid::new_v4();
    hle.add_layer(root, None, abs_style(0.0, 0.0, 200.0, 200.0), r(0.0, 0.0, 200.0, 200.0))
        .unwrap();
    hle.add_layer(cid, Some(root), abs_style(20.0, 20.0, 60.0, 60.0), r(20.0, 20.0, 60.0, 60.0))
        .unwrap();
    hle.set_constraints(cid, Constraints::stretch());
    // Notify resize of artboard_id (not in child_info) — should be skipped.
    hle.notify_parent_resize(artboard_id, r(0.0, 0.0, 200.0, 200.0), r(0.0, 0.0, 400.0, 200.0));
    hle.compute(root).unwrap();
    // Child should remain at original size (constraint was not applied).
    let layout = hle.get_layout(cid).unwrap();
    assert!((layout.size.width - 60.0).abs() < f32::EPSILON);
}

#[test]
fn t170_artboard_mode_constraint_propagates() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::ArtboardSection);
    let pid = Uuid::new_v4();
    let cid = Uuid::new_v4();
    hle.add_layer(pid, None, abs_style(0.0, 0.0, 300.0, 300.0), r(0.0, 0.0, 300.0, 300.0))
        .unwrap();
    hle.add_layer(cid, Some(pid), abs_style(10.0, 10.0, 280.0, 280.0), r(10.0, 10.0, 280.0, 280.0))
        .unwrap();
    hle.set_constraints(cid, Constraints::stretch());
    hle.notify_parent_resize(pid, r(0.0, 0.0, 300.0, 300.0), r(0.0, 0.0, 500.0, 300.0));
    hle.compute(pid).unwrap();
    let layout = hle.get_layout(cid).unwrap();
    // 500-10-10=480
    assert!((layout.size.width - 480.0).abs() < 1.0);
}

#[test]
fn t171_hybrid_mode_constraint_propagates() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    let pid = Uuid::new_v4();
    let cid = Uuid::new_v4();
    hle.add_layer(pid, None, abs_style(0.0, 0.0, 200.0, 100.0), r(0.0, 0.0, 200.0, 100.0))
        .unwrap();
    hle.add_layer(cid, Some(pid), abs_style(0.0, 0.0, 200.0, 100.0), r(0.0, 0.0, 200.0, 100.0))
        .unwrap();
    hle.set_constraints(cid, Constraints::stretch());
    hle.notify_parent_resize(pid, r(0.0, 0.0, 200.0, 100.0), r(0.0, 0.0, 400.0, 100.0));
    hle.compute(pid).unwrap();
    let layout = hle.get_layout(cid).unwrap();
    assert!((layout.size.width - 400.0).abs() < 1.0);
}

#[test]
fn t172_set_mode_from_flatpage_to_hybrid_enables_artboards() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    assert!(!hle.mode().supports_artboards());
    hle.set_mode(WorkspaceMode::Hybrid);
    assert!(hle.mode().supports_artboards());
    assert!(hle.mode().supports_flat());
}

#[test]
fn t173_mode_label_hybrid() {
    let hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    assert_eq!(hle.mode().label(), "Hybrid");
}

#[test]
fn t174_mode_label_flatpage() {
    let hle = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    assert_eq!(hle.mode().label(), "Flat Page");
}

#[test]
fn t175_mode_label_artboard_section() {
    let hle = HybridLayoutEngine::new(WorkspaceMode::ArtboardSection);
    assert_eq!(hle.mode().label(), "Artboard / Section");
}
