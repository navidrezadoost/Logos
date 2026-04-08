// Phase 4 – Undo/Redo Stack Tests (t401–t415)
//
// Integration tests for `logos_desktop::undo::{UndoStack, UndoAction}`.
// Covers the public API from outside the module.

use logos_core::container::VariantState;
use logos_desktop::undo::{UndoAction, UndoStack};
use logos_layout::repeat_grid::DataSource;
use serde_json::json;
use uuid::Uuid;

// ── Helpers ────────────────────────────────────────────────────────────────────

fn set_variant(
    layer_id: Uuid,
    old: VariantState,
    new: VariantState,
) -> UndoAction {
    UndoAction::SetVariantState { layer_id, old_state: old, new_state: new }
}

fn data_source(name: &str) -> DataSource {
    DataSource::new(name, "label.text", vec![json!("A"), json!("B")])
}

// ── §1 Basic push / undo / redo ────────────────────────────────────────────────

#[test]
fn t401_empty_stack_cannot_undo_or_redo() {
    let s = UndoStack::new(50);
    assert!(!s.can_undo());
    assert!(!s.can_redo());
}

#[test]
fn t402_single_push_enables_undo() {
    let mut s = UndoStack::new(50);
    let id = Uuid::new_v4();
    s.push(set_variant(id, VariantState::Default, VariantState::Hover));
    assert!(s.can_undo());
    assert_eq!(s.undo_depth(), 1);
}

#[test]
fn t403_undo_decrements_undo_depth() {
    let mut s = UndoStack::new(50);
    let id = Uuid::new_v4();
    s.push(set_variant(id, VariantState::Default, VariantState::Hover));
    s.undo();
    assert_eq!(s.undo_depth(), 0);
}

#[test]
fn t404_undo_increments_redo_depth() {
    let mut s = UndoStack::new(50);
    let id = Uuid::new_v4();
    s.push(set_variant(id, VariantState::Default, VariantState::Hover));
    s.undo();
    assert_eq!(s.redo_depth(), 1);
    assert!(s.can_redo());
}

#[test]
fn t405_redo_after_undo_restores_undo_depth() {
    let mut s = UndoStack::new(50);
    let id = Uuid::new_v4();
    s.push(set_variant(id, VariantState::Default, VariantState::Active));
    s.undo();
    s.redo();
    assert_eq!(s.undo_depth(), 1);
    assert_eq!(s.redo_depth(), 0);
}

#[test]
fn t406_interleaved_undo_redo_sequence() {
    let mut s = UndoStack::new(50);
    let id = Uuid::new_v4();
    // Push 3 actions
    s.push(set_variant(id, VariantState::Default, VariantState::Hover));
    s.push(set_variant(id, VariantState::Hover, VariantState::Active));
    s.push(set_variant(id, VariantState::Active, VariantState::Focus));
    assert_eq!(s.undo_depth(), 3);
    // Undo twice
    s.undo();
    s.undo();
    assert_eq!(s.undo_depth(), 1);
    assert_eq!(s.redo_depth(), 2);
    // Redo once
    s.redo();
    assert_eq!(s.undo_depth(), 2);
    assert_eq!(s.redo_depth(), 1);
}

// ── §2 Depth limit ─────────────────────────────────────────────────────────────

#[test]
fn t407_depth_limit_enforced_at_push() {
    let mut s = UndoStack::new(5);
    let id = Uuid::new_v4();
    for _ in 0..10 {
        s.push(set_variant(id, VariantState::Default, VariantState::Hover));
    }
    assert_eq!(s.undo_depth(), 5);
}

#[test]
fn t408_max_depth_getter_returns_configured_value() {
    let s = UndoStack::new(42);
    assert_eq!(s.max_depth(), 42);
}

// ── §3 Clear ───────────────────────────────────────────────────────────────────

#[test]
fn t409_clear_resets_both_stacks() {
    let mut s = UndoStack::new(20);
    let id = Uuid::new_v4();
    s.push(set_variant(id, VariantState::Default, VariantState::Hover));
    s.undo();
    s.clear();
    assert_eq!(s.undo_depth(), 0);
    assert_eq!(s.redo_depth(), 0);
}

// ── §4 Action types ────────────────────────────────────────────────────────────

#[test]
fn t410_set_data_override_action_description() {
    let action = UndoAction::SetDataOverride {
        grid_id: Uuid::new_v4(),
        row: 0,
        col: 1,
        path: "label.text".into(),
        old_value: None,
        new_value: json!("Hello"),
    };
    assert_eq!(action.description(), "Set Data Override");
}

#[test]
fn t411_attach_data_source_roundtrip_via_inverse() {
    let gid = Uuid::new_v4();
    let src = data_source("cols");
    let attach = UndoAction::AttachDataSource { grid_id: gid, source: src.clone() };
    let detach = attach.inverse();
    let re_attach = detach.inverse();
    // Re-attaching should be equivalent to the original attach
    if let UndoAction::AttachDataSource { grid_id, source } = re_attach {
        assert_eq!(grid_id, gid);
        assert_eq!(source, src);
    } else {
        panic!("expected AttachDataSource after double inverse");
    }
}

#[test]
fn t412_detach_inverse_is_attach() {
    let gid = Uuid::new_v4();
    let src = data_source("rows");
    let detach = UndoAction::DetachDataSource { grid_id: gid, source: src.clone() };
    if let UndoAction::AttachDataSource { grid_id, source } = detach.inverse() {
        assert_eq!(grid_id, gid);
        assert_eq!(source, src);
    } else {
        panic!("expected AttachDataSource");
    }
}

#[test]
fn t413_set_data_override_inverse_swaps_old_new() {
    let action = UndoAction::SetDataOverride {
        grid_id: Uuid::new_v4(),
        row: 2,
        col: 3,
        path: "bg.fill".into(),
        old_value: Some(json!("red")),
        new_value: json!("blue"),
    };
    if let UndoAction::SetDataOverride { old_value, new_value, .. } = action.inverse() {
        assert_eq!(new_value, json!("red"));
        assert_eq!(old_value, Some(json!("blue")));
    } else {
        panic!("expected SetDataOverride");
    }
}

// ── §5 Label helpers ──────────────────────────────────────────────────────────

#[test]
fn t414_undo_label_none_when_empty() {
    let s = UndoStack::new(10);
    assert!(s.undo_label().is_none());
}

#[test]
fn t415_redo_label_reflects_action_after_undo() {
    let mut s = UndoStack::new(10);
    let id = Uuid::new_v4();
    s.push(UndoAction::AttachDataSource {
        grid_id: id,
        source: data_source("x"),
    });
    s.undo();
    assert_eq!(s.redo_label(), Some("Attach Data Source"));
}
