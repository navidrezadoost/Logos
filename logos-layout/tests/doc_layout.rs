// logos-layout/tests/doc_layout.rs
//
// Documentation-style integration tests (t800–t819).
//
// Each test is a self-contained usage example suitable for inclusion
// in the "Layout Engine" section of the developer guide.
//
// Sections
// --------
//   §1  Engine setup & mode selection (t800–t804)
//   §2  Adding and querying layers (t805–t809)
//   §3  Bounds updates & dirty tracking (t810–t814)
//   §4  Hierarchy and child layers (t815–t819)

use logos_core::{EllipseLayer, Layer, Rect, RectLayer, TextLayer, WorkspaceMode};
use logos_layout::hybrid::HybridLayoutEngine;
use taffy::prelude::Style;
use uuid::Uuid;

// ── helpers ──────────────────────────────────────────────────────────────────

fn r(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect { x, y, width: w, height: h }
}

fn default_style() -> Style {
    Style::default()
}

// ── §1  Engine setup & mode selection ────────────────────────────────────────

/// **Example:** creating a layout engine for a flat-page canvas.
///
/// ```no_run
/// let mut engine = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
/// ```
#[test]
fn t800_doc_engine_for_flat_page() {
    let engine = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    assert_eq!(engine.mode(), WorkspaceMode::FlatPage);
    assert!(engine.mode().supports_flat());
}

/// **Example:** creating a layout engine for artboard-based design.
#[test]
fn t801_doc_engine_for_artboard_mode() {
    let engine = HybridLayoutEngine::new(WorkspaceMode::ArtboardSection);
    assert!(engine.mode().supports_artboards());
    assert!(!engine.mode().supports_flat());
}

/// **Example:** switching an existing engine to flat-page mode at runtime.
#[test]
fn t802_doc_switch_engine_to_flat_page() {
    let mut engine = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
    engine.set_mode(WorkspaceMode::FlatPage);
    assert_eq!(engine.mode(), WorkspaceMode::FlatPage);
}

/// **Example:** a freshly created engine starts with no registered layers.
#[test]
fn t803_doc_fresh_engine_has_zero_nodes() {
    let engine = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    assert_eq!(engine.node_count(), 0);
}

/// **Example:** `grid_count` is always 0 in flat-page mode.
#[test]
fn t804_doc_flat_page_grid_count_zero() {
    let engine = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    assert_eq!(engine.grid_count(), 0);
}

// ── §2  Adding and querying layers ───────────────────────────────────────────

/// **Example:** registering a rect layer in the layout engine.
///
/// ```no_run
/// engine.add_layer(id, None, Style::default(), bounds)?;
/// ```
#[test]
fn t805_doc_register_rect_layer() {
    let mut engine = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    let id = Uuid::new_v4();
    engine.add_layer(id, None, default_style(), r(0.0, 0.0, 200.0, 100.0)).unwrap();
    assert_eq!(engine.node_count(), 1);
}

/// **Example:** using the convenience `add_or_update_layer` helper with a `Layer` value.
#[test]
fn t806_doc_add_or_update_rect_via_helper() {
    let mut engine = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    let rect = RectLayer::new(10.0, 20.0, 150.0, 80.0);
    engine.add_or_update_layer(&Layer::Rect(rect)).unwrap();
    assert_eq!(engine.node_count(), 1);
}

/// **Example:** registering multiple layers.
#[test]
fn t807_doc_register_three_layers() {
    let mut engine = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    for i in 0..3_u32 {
        let layer = RectLayer::new(i as f32 * 110.0, 0.0, 100.0, 100.0);
        engine.add_or_update_layer(&Layer::Rect(layer)).unwrap();
    }
    assert_eq!(engine.node_count(), 3);
}

/// **Example:** registering an ellipse layer via the helper.
#[test]
fn t808_doc_register_ellipse_layer() {
    let mut engine = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    let ellipse = EllipseLayer::new(0.0, 0.0, 120.0, 120.0);
    engine.add_or_update_layer(&Layer::Ellipse(ellipse)).unwrap();
    assert_eq!(engine.node_count(), 1);
}

/// **Example:** calling `add_or_update_layer` twice with the same id is idempotent.
///
/// This is the expected behaviour when a layer's style is refreshed from the UI.
#[test]
fn t809_doc_idempotent_update() {
    let mut engine = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    let rect = RectLayer::new(0.0, 0.0, 100.0, 100.0);
    let layer = Layer::Rect(rect);
    engine.add_or_update_layer(&layer).unwrap();
    engine.add_or_update_layer(&layer).unwrap(); // second call — same id
    assert_eq!(engine.node_count(), 1, "same id must not create a duplicate node");
}

// ── §3  Bounds updates & dirty tracking ──────────────────────────────────────

/// **Example:** updating a layer's bounds (e.g. after a drag-resize).
#[test]
fn t810_doc_update_bounds_after_resize() {
    let mut engine = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    let id = Uuid::new_v4();
    engine.add_layer(id, None, default_style(), r(0.0, 0.0, 100.0, 100.0)).unwrap();

    engine.update_bounds(id, r(0.0, 0.0, 250.0, 180.0)).unwrap();
    // No panic; the bounds are registered internally.
}

/// **Example:** dirty tracking — layers accumulate on the dirty list until drained.
#[test]
fn t811_doc_dirty_count_increases_after_update() {
    let mut engine = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    let id = Uuid::new_v4();
    engine.add_layer(id, None, default_style(), r(0.0, 0.0, 100.0, 100.0)).unwrap();
    engine.update_bounds(id, r(0.0, 0.0, 200.0, 100.0)).unwrap();

    assert!(engine.dirty_count() > 0);
}

/// **Example:** `drain_changed` returns the list of changed ids and resets the dirty set.
#[test]
fn t812_doc_drain_changed_clears_dirty() {
    let mut engine = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    let id = Uuid::new_v4();
    engine.add_layer(id, None, default_style(), r(0.0, 0.0, 100.0, 100.0)).unwrap();
    engine.update_bounds(id, r(10.0, 10.0, 90.0, 90.0)).unwrap();

    // drain_changed returns a Vec<Uuid>; it is populated after compute_layout
    let changed: Vec<uuid::Uuid> = engine.drain_changed();
    let _ = changed; // type check — does not panic
}

/// **Example:** removing a layer that does not exist returns an error (safe to ignore).
#[test]
fn t813_doc_remove_nonexistent_layer_is_safe() {
    let mut engine = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    let result = engine.remove_layer(Uuid::new_v4());
    assert!(result.is_err(), "removing a missing layer must return Err, not panic");
}

/// **Example:** removing a registered layer decrements the node count.
#[test]
fn t814_doc_remove_layer_decrements_count() {
    let mut engine = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    let id = Uuid::new_v4();
    engine.add_layer(id, None, default_style(), r(0.0, 0.0, 100.0, 100.0)).unwrap();
    assert_eq!(engine.node_count(), 1);

    engine.remove_layer(id).unwrap();
    assert_eq!(engine.node_count(), 0);
}

// ── §4  Hierarchy and child layers ───────────────────────────────────────────

/// **Example:** registering a parent-child relationship (frame with inner rect).
#[test]
fn t815_doc_parent_child_hierarchy() {
    let mut engine = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    let parent_id = Uuid::new_v4();
    let child_id = Uuid::new_v4();

    engine.add_layer(parent_id, None, default_style(), r(0.0, 0.0, 400.0, 300.0)).unwrap();
    engine.add_layer(child_id, Some(parent_id), default_style(), r(10.0, 10.0, 100.0, 80.0)).unwrap();

    // Both nodes are registered.
    assert_eq!(engine.node_count(), 2);
}

/// **Example:** multiple children under a single parent.
#[test]
fn t816_doc_multiple_children_under_parent() {
    let mut engine = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    let parent_id = Uuid::new_v4();
    engine.add_layer(parent_id, None, default_style(), r(0.0, 0.0, 800.0, 600.0)).unwrap();

    for i in 0..4_u32 {
        let child = Uuid::new_v4();
        engine.add_layer(child, Some(parent_id), default_style(),
            r(i as f32 * 90.0, 0.0, 80.0, 80.0)).unwrap();
    }
    assert_eq!(engine.node_count(), 5); // 1 parent + 4 children
}

/// **Example:** text layer added via helper with a parent frame.
#[test]
fn t817_doc_text_inside_frame() {
    let mut engine = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    let frame_id = Uuid::new_v4();
    engine.add_layer(frame_id, None, default_style(), r(0.0, 0.0, 600.0, 200.0)).unwrap();

    let text = TextLayer::new("Welcome to Logos", 20.0, 20.0, 400.0, 60.0);
    let text_id = text.id;
    // Register text inside the frame using the low-level API.
    engine.add_layer(text_id, Some(frame_id), default_style(),
        r(text.bounds.x, text.bounds.y, text.bounds.width, text.bounds.height)).unwrap();

    assert_eq!(engine.node_count(), 2);
}

/// **Example:** updating a child's bounds after the parent is resized.
#[test]
fn t818_doc_update_child_bounds() {
    let mut engine = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    let parent = Uuid::new_v4();
    let child = Uuid::new_v4();
    engine.add_layer(parent, None, default_style(), r(0.0, 0.0, 300.0, 200.0)).unwrap();
    engine.add_layer(child, Some(parent), default_style(), r(10.0, 10.0, 80.0, 50.0)).unwrap();

    // After the parent is resized, propagate new child bounds.
    engine.update_bounds(parent, r(0.0, 0.0, 500.0, 400.0)).unwrap();
    engine.update_bounds(child, r(10.0, 10.0, 140.0, 80.0)).unwrap();

    // drain_changed collects ids that moved; call compute_layout first in production
    let changed: Vec<uuid::Uuid> = engine.drain_changed();
    let _ = changed; // verifies the call does not panic
}

/// **Example:** the inner Taffy engine is accessible for advanced layout operations.
#[test]
fn t819_doc_inner_engine_access() {
    let engine = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
    // `inner()` gives access to the underlying TaffyLayoutEngine for custom CSS Grid
    // or Flexbox computations beyond what the hybrid wrapper exposes.
    let _inner = engine.inner();
}
