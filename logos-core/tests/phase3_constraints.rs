// Phase 3 – Constraint Visualisation Model Tests (t341–t350)
//
// Tests for `logos_core::constraint::{PinAnchor, Axis, AnchorKind,
// ConstraintOverlay, compute_overlay}`.

use logos_core::constraint::{
    AnchorKind, Axis, ConstraintOverlay, Constraints, HorizontalConstraint,
    PinAnchor, VerticalConstraint, compute_overlay,
};
use logos_core::Rect;
use uuid::Uuid;

// ── Helpers ────────────────────────────────────────────────────────────────────

fn r(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect { x, y, width: w, height: h }
}

fn parent() -> Rect {
    r(0.0, 0.0, 400.0, 300.0)
}

fn child() -> Rect {
    r(50.0, 30.0, 100.0, 60.0)
}

// ── §1 Anchor counts by constraint preset ─────────────────────────────────────

#[test]
fn t341_top_left_produces_two_pin_anchors() {
    let id = Uuid::new_v4();
    let overlay = compute_overlay(id, &Constraints::top_left(), child(), parent());
    assert_eq!(overlay.anchors.len(), 2, "top_left should produce exactly 2 anchors");
    assert!(overlay.anchors.iter().all(|a| a.kind == AnchorKind::Pin),
        "all anchors should be Pin for top_left");
}

#[test]
fn t342_scale_produces_two_scale_anchors() {
    let id = Uuid::new_v4();
    let overlay = compute_overlay(id, &Constraints::scale(), child(), parent());
    assert_eq!(overlay.anchors.len(), 2, "scale should produce exactly 2 anchors");
    assert!(overlay.anchors.iter().all(|a| a.kind == AnchorKind::Scale),
        "all anchors should be Scale for scale");
}

#[test]
fn t343_center_produces_two_center_anchors() {
    let id = Uuid::new_v4();
    let overlay = compute_overlay(id, &Constraints::center(), child(), parent());
    assert_eq!(overlay.anchors.len(), 2, "center should produce exactly 2 anchors");
    assert!(overlay.anchors.iter().all(|a| a.kind == AnchorKind::Center),
        "all anchors should be Center for center");
}

#[test]
fn t344_stretch_produces_four_stretch_anchors() {
    let id = Uuid::new_v4();
    let overlay = compute_overlay(id, &Constraints::stretch(), child(), parent());
    // LeftAndRight → 2 Stretch (H), TopAndBottom → 2 Stretch (V) = 4 total.
    assert_eq!(overlay.anchors.len(), 4, "stretch should produce 4 anchors (2H + 2V)");
    assert!(overlay.anchors.iter().all(|a| a.kind == AnchorKind::Stretch),
        "all anchors should be Stretch for stretch");
}

#[test]
fn t345_mixed_left_right_and_top_produces_three_anchors() {
    let id = Uuid::new_v4();
    let c = Constraints::new(HorizontalConstraint::LeftAndRight, VerticalConstraint::Top);
    let overlay = compute_overlay(id, &c, child(), parent());
    // LeftAndRight → 2 Stretch (H), Top → 1 Pin (V) = 3 total.
    assert_eq!(overlay.anchors.len(), 3,
        "LeftAndRight + Top should produce 3 anchors, got {}", overlay.anchors.len());
}

// ── §2 Anchor axis assignment ──────────────────────────────────────────────────

#[test]
fn t346_pin_anchor_position_within_parent_bounds() {
    let id = Uuid::new_v4();
    let bounds = r(20.0, 10.0, 80.0, 50.0);
    let par = r(0.0, 0.0, 200.0, 150.0);
    let overlay = compute_overlay(id, &Constraints::top_left(), bounds, par);

    // Left pin: position == bounds.x == 20.0
    let h_anchor = overlay.anchors.iter().find(|a| a.axis == Axis::Horizontal).unwrap();
    assert!((h_anchor.position - 20.0).abs() < 0.01, "horizontal pin at left margin");

    // Top pin: position == bounds.y == 10.0
    let v_anchor = overlay.anchors.iter().find(|a| a.axis == Axis::Vertical).unwrap();
    assert!((v_anchor.position - 10.0).abs() < 0.01, "vertical pin at top margin");
}

// ── §3 ConstraintOverlay serde ────────────────────────────────────────────────

#[test]
fn t347_constraint_overlay_serde_roundtrip() {
    let id = Uuid::new_v4();
    let overlay = compute_overlay(id, &Constraints::scale(), child(), parent());
    let json = serde_json::to_string(&overlay).unwrap();
    let back: ConstraintOverlay = serde_json::from_str(&json).unwrap();
    assert_eq!(back.layer_id, id);
    assert_eq!(back.anchors.len(), overlay.anchors.len());
}

// ── §4 Determinism + independence ─────────────────────────────────────────────

#[test]
fn t348_anchor_count_matches_constraint_complexity() {
    let id = Uuid::new_v4();
    // Pure Left+Top → 2
    let o1 = compute_overlay(id, &Constraints::top_left(), child(), parent());
    assert_eq!(o1.anchors.len(), 2);

    // LeftAndRight+Top → 3
    let o2 = compute_overlay(
        id,
        &Constraints::new(HorizontalConstraint::LeftAndRight, VerticalConstraint::Top),
        child(),
        parent(),
    );
    assert_eq!(o2.anchors.len(), 3);

    // LeftAndRight+TopAndBottom → 4
    let o3 = compute_overlay(id, &Constraints::stretch(), child(), parent());
    assert_eq!(o3.anchors.len(), 4);
}

#[test]
fn t349_two_layers_produce_independent_overlays() {
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let bounds1 = r(10.0, 10.0, 50.0, 30.0);
    let bounds2 = r(200.0, 150.0, 100.0, 80.0);
    let par = r(0.0, 0.0, 400.0, 300.0);

    let o1 = compute_overlay(id1, &Constraints::top_left(), bounds1, par);
    let o2 = compute_overlay(id2, &Constraints::top_left(), bounds2, par);

    assert_eq!(o1.layer_id, id1);
    assert_eq!(o2.layer_id, id2);
    // Positions differ since bounds differ.
    let o1_h = o1.anchors.iter().find(|a| a.axis == Axis::Horizontal).unwrap();
    let o2_h = o2.anchors.iter().find(|a| a.axis == Axis::Horizontal).unwrap();
    assert!((o1_h.position - o2_h.position).abs() > 0.1);
}

#[test]
fn t350_compute_overlay_is_deterministic() {
    let id = Uuid::new_v4();
    let c = Constraints::center();
    let bounds = child();
    let par = parent();

    let o1 = compute_overlay(id, &c, bounds, par);
    let o2 = compute_overlay(id, &c, bounds, par);

    assert_eq!(o1.anchors.len(), o2.anchors.len());
    for (a, b) in o1.anchors.iter().zip(o2.anchors.iter()) {
        assert_eq!(a.kind, b.kind);
        assert_eq!(a.axis, b.axis);
        assert!((a.position - b.position).abs() < f32::EPSILON);
    }
}
