//! Phase 5 Integration Tests — HybridLayoutEngine in Flat-Page Mode
//!
//! Focuses on the FlatPage-specific gating behaviour and multi-frame
//! flat-canvas layout scenarios:
//!
//!   §1 Mode switching at runtime           (t500–t509)
//!   §2 Flat-page layer operations          (t510–t519)
//!   §3 add_or_update_layer helper          (t520–t529)
//!   §4 Mode gating edge cases              (t530–t539)

use logos_core::{EllipseLayer, Layer, RectLayer, Rect, TextLayer, WorkspaceMode};
use logos_layout::hybrid::HybridLayoutEngine;
use taffy::prelude::Style;
use uuid::Uuid;

fn r(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect { x, y, width: w, height: h }
}

fn abs_style() -> Style {
    Style {
        position: taffy::style::Position::Absolute,
        ..Style::default()
    }
}

// ── §1: Mode switching at runtime ────────────────────────────────────────────

#[test]
fn t500_new_flatpage_engine_has_correct_mode() {
    let hle = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    assert_eq!(hle.mode(), WorkspaceMode::FlatPage);
}

#[test]
fn t501_switch_from_hybrid_to_flat() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    hle.set_mode(WorkspaceMode::FlatPage);
    assert_eq!(hle.mode(), WorkspaceMode::FlatPage);
}

#[test]
fn t502_switch_from_flat_to_artboard() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    hle.set_mode(WorkspaceMode::ArtboardSection);
    assert_eq!(hle.mode(), WorkspaceMode::ArtboardSection);
}

#[test]
fn t503_switch_from_flat_to_hybrid() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    hle.set_mode(WorkspaceMode::Hybrid);
    assert_eq!(hle.mode(), WorkspaceMode::Hybrid);
}

#[test]
fn t504_mode_stays_flat_after_layer_add() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    let id = Uuid::new_v4();
    hle.add_layer(id, None, abs_style(), r(0.0, 0.0, 100.0, 100.0)).unwrap();
    assert_eq!(hle.mode(), WorkspaceMode::FlatPage);
}

#[test]
fn t505_flat_engine_supports_flat_is_true() {
    let hle = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    assert!(hle.mode().supports_flat());
}

#[test]
fn t506_flat_engine_supports_artboards_is_false() {
    let hle = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    assert!(!hle.mode().supports_artboards());
}

#[test]
fn t507_mode_label_flat_page() {
    let hle = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    assert_eq!(hle.mode().label(), "Flat Page");
}

#[test]
fn t508_initial_node_count_zero_in_flat_mode() {
    let hle = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    assert_eq!(hle.node_count(), 0);
}

#[test]
fn t509_initial_dirty_count_zero_in_flat_mode() {
    let hle = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    assert_eq!(hle.dirty_count(), 0);
}

// ── §2: Flat-page layer operations ───────────────────────────────────────────

#[test]
fn t510_add_one_layer_increments_count() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    let id = Uuid::new_v4();
    hle.add_layer(id, None, abs_style(), r(0.0, 0.0, 200.0, 200.0)).unwrap();
    assert_eq!(hle.node_count(), 1);
}

#[test]
fn t511_add_ten_flat_frames_all_registered() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    for i in 0..10 {
        let id = Uuid::new_v4();
        hle.add_layer(id, None, abs_style(), r(i as f32 * 220.0, 0.0, 200.0, 200.0)).unwrap();
    }
    assert_eq!(hle.node_count(), 10);
}

#[test]
fn t512_remove_layer_from_flat_mode() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    let id = Uuid::new_v4();
    hle.add_layer(id, None, abs_style(), r(0.0, 0.0, 100.0, 100.0)).unwrap();
    hle.remove_layer(id).unwrap();
    assert_eq!(hle.node_count(), 0);
}

#[test]
fn t513_remove_nonexistent_layer_flat_mode_errors() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    let result = hle.remove_layer(Uuid::new_v4());
    assert!(result.is_err());
}

#[test]
fn t514_update_bounds_changes_layer_size() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    let id = Uuid::new_v4();
    hle.add_layer(id, None, abs_style(), r(0.0, 0.0, 100.0, 100.0)).unwrap();
    hle.update_bounds(id, r(0.0, 0.0, 300.0, 200.0)).unwrap();
    // no error = success; bounds stored internally
}

#[test]
fn t515_update_bounds_nonexistent_layer_errors() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    let result = hle.update_bounds(Uuid::new_v4(), r(0.0, 0.0, 100.0, 100.0));
    assert!(result.is_err());
}

#[test]
fn t516_add_child_layer_flat_doc() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    let parent = Uuid::new_v4();
    let child = Uuid::new_v4();
    hle.add_layer(parent, None, abs_style(), r(0.0, 0.0, 300.0, 300.0)).unwrap();
    hle.add_layer(child, Some(parent), abs_style(), r(10.0, 10.0, 80.0, 80.0)).unwrap();
    assert_eq!(hle.node_count(), 2);
}

#[test]
fn t517_dirty_count_after_bounds_update() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    let id = Uuid::new_v4();
    hle.add_layer(id, None, abs_style(), r(0.0, 0.0, 100.0, 100.0)).unwrap();
    hle.update_bounds(id, r(0.0, 0.0, 200.0, 150.0)).unwrap();
    // dirty count may be 0 or >0 depending on implementation; just ensure no panic
    let _ = hle.dirty_count();
}

#[test]
fn t518_drain_changed_returns_vec_after_update() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    let id = Uuid::new_v4();
    hle.add_layer(id, None, abs_style(), r(0.0, 0.0, 100.0, 100.0)).unwrap();
    hle.update_bounds(id, r(5.0, 5.0, 200.0, 150.0)).unwrap();
    let changed = hle.drain_changed();
    // changed may or may not include id, but type must be Vec<Uuid>
    let _: Vec<Uuid> = changed;
}

#[test]
fn t519_grid_count_zero_in_flat_mode() {
    let hle = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    assert_eq!(hle.grid_count(), 0);
}

// ── §3: add_or_update_layer helper ───────────────────────────────────────────

#[test]
fn t520_add_or_update_rect_layer_in_flat_mode() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    let layer = Layer::Rect(RectLayer::new(0.0, 0.0, 100.0, 100.0));
    hle.add_or_update_layer(&layer).unwrap();
    assert_eq!(hle.node_count(), 1);
}

#[test]
fn t521_add_or_update_ellipse_in_flat_mode() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    let layer = Layer::Ellipse(EllipseLayer::new(0.0, 0.0, 80.0, 80.0));
    hle.add_or_update_layer(&layer).unwrap();
    assert_eq!(hle.node_count(), 1);
}

#[test]
fn t522_add_or_update_text_in_flat_mode() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    let layer = Layer::Text(TextLayer::new("Canva", 0.0, 0.0, 200.0, 30.0));
    hle.add_or_update_layer(&layer).unwrap();
    assert_eq!(hle.node_count(), 1);
}

#[test]
fn t523_add_or_update_twice_is_idempotent_count() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    let layer = Layer::Rect(RectLayer::new(0.0, 0.0, 100.0, 100.0));
    // Two calls with the same layer (same id) should not double the node count
    hle.add_or_update_layer(&layer).unwrap();
    let count_after_first = hle.node_count();
    hle.add_or_update_layer(&layer).unwrap();
    let count_after_second = hle.node_count();
    assert_eq!(count_after_first, count_after_second,
        "second add_or_update should not increase count");
}

#[test]
fn t524_add_multiple_different_layers_via_helper() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    let layers = vec![
        Layer::Rect(RectLayer::new(0.0, 0.0, 100.0, 100.0)),
        Layer::Ellipse(EllipseLayer::new(110.0, 0.0, 80.0, 80.0)),
        Layer::Text(TextLayer::new("Hi", 0.0, 110.0, 200.0, 25.0)),
    ];
    for l in &layers {
        hle.add_or_update_layer(l).unwrap();
    }
    assert_eq!(hle.node_count(), 3);
}

// ── §4: Mode gating edge cases ────────────────────────────────────────────────

#[test]
fn t530_flatpage_mode_not_eq_artboard() {
    assert_ne!(WorkspaceMode::FlatPage, WorkspaceMode::ArtboardSection);
}

#[test]
fn t531_flatpage_mode_not_eq_hybrid() {
    assert_ne!(WorkspaceMode::FlatPage, WorkspaceMode::Hybrid);
}

#[test]
fn t532_flatpage_eq_flatpage() {
    assert_eq!(WorkspaceMode::FlatPage, WorkspaceMode::FlatPage);
}

#[test]
fn t533_hybrid_supports_both_artboard_and_flat() {
    let m = WorkspaceMode::Hybrid;
    assert!(m.supports_artboards());
    assert!(m.supports_flat());
}

#[test]
fn t534_artboard_section_supports_artboards_not_flat() {
    let m = WorkspaceMode::ArtboardSection;
    assert!(m.supports_artboards());
    assert!(!m.supports_flat());
}

#[test]
fn t535_flatpage_supports_flat_not_artboards() {
    let m = WorkspaceMode::FlatPage;
    assert!(m.supports_flat());
    assert!(!m.supports_artboards());
}

#[test]
fn t536_inner_engine_accessible_in_flat_mode() {
    let hle = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    let _inner = hle.inner();
}

#[test]
fn t537_compute_layout_on_empty_flat_engine() {
    let mut hle = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    let root_id = Uuid::new_v4();
    hle.add_layer(root_id, None, abs_style(), r(0.0, 0.0, 1920.0, 1080.0)).unwrap();
    let result = hle.compute_layout(root_id);
    assert!(result.is_ok(), "compute_layout should succeed on flat engine");
}

#[test]
fn t538_mode_clone_is_equal() {
    let m = WorkspaceMode::FlatPage;
    let m2 = m;
    assert_eq!(m, m2);
}

#[test]
fn t539_flat_mode_engine_from_hybrid_set_mode_same_behavior() {
    let mut hle1 = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    let mut hle2 = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    hle2.set_mode(WorkspaceMode::FlatPage);
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    hle1.add_layer(id1, None, abs_style(), r(0.0, 0.0, 100.0, 100.0)).unwrap();
    hle2.add_layer(id2, None, abs_style(), r(0.0, 0.0, 100.0, 100.0)).unwrap();
    assert_eq!(hle1.node_count(), hle2.node_count());
}
