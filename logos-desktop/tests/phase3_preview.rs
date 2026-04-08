// Phase 3 – Live Variant Preview Tests (t321–t335)
//
// Tests for `VariantPanel`'s hover-preview API:
// begin_preview / end_preview / commit_preview / active_display_state /
// is_previewing.

use logos_core::WorkspaceMode;
use logos_core::container::{ComponentRef, ComponentVariant, VariantState};
use logos_desktop::variants::VariantPanel;
use uuid::Uuid;

// ── Helpers ────────────────────────────────────────────────────────────────────

fn panel() -> VariantPanel {
    VariantPanel::new(WorkspaceMode::Hybrid)
}

fn make_cref(states: &[VariantState]) -> ComponentRef {
    let mut cref = ComponentRef {
        component_id: Uuid::new_v4(),
        overrides: Vec::new(),
        variants: Vec::new(),
        current_state: VariantState::Default,
    };
    for &s in states {
        cref.add_variant(ComponentVariant::new(s));
    }
    cref
}

// ── §1 begin_preview / end_preview ────────────────────────────────────────────

#[test]
fn t321_begin_preview_sets_preview_state() {
    let mut p = panel();
    p.begin_preview(VariantState::Hover);
    assert_eq!(p.preview_state, Some(VariantState::Hover));
}

#[test]
fn t322_end_preview_clears_preview_state() {
    let mut p = panel();
    p.begin_preview(VariantState::Active);
    p.end_preview();
    assert_eq!(p.preview_state, None);
}

#[test]
fn t323_active_display_state_returns_preview_when_set() {
    let mut p = panel();
    p.begin_preview(VariantState::Focus);
    assert_eq!(p.active_display_state(), VariantState::Focus);
}

#[test]
fn t324_active_display_state_returns_active_when_no_preview() {
    let mut p = panel();
    p.active_state = VariantState::Hover;
    assert_eq!(p.preview_state, None);
    assert_eq!(p.active_display_state(), VariantState::Hover);
}

// ── §2 commit_preview ─────────────────────────────────────────────────────────

#[test]
fn t325_commit_preview_makes_preview_permanent_returns_true() {
    let mut p = panel();
    p.begin_preview(VariantState::Active);
    let changed = p.commit_preview();
    assert!(changed, "commit_preview should return true when state changed");
    assert_eq!(p.active_state, VariantState::Active);
    assert_eq!(p.preview_state, None);
}

#[test]
fn t326_commit_preview_with_no_preview_returns_false() {
    let mut p = panel();
    assert!(!p.commit_preview(), "commit with no preview should return false");
    assert_eq!(p.active_state, VariantState::Default);
}

#[test]
fn t327_double_commit_second_returns_false() {
    let mut p = panel();
    p.begin_preview(VariantState::Disabled);
    assert!(p.commit_preview());
    // After commit, preview_state is None → second commit is no-op.
    assert!(!p.commit_preview());
}

// ── §3 is_previewing ──────────────────────────────────────────────────────────

#[test]
fn t328_is_previewing_true_when_preview_is_set() {
    let mut p = panel();
    p.begin_preview(VariantState::Error);
    assert!(p.is_previewing());
}

#[test]
fn t329_is_previewing_false_when_no_preview() {
    let p = panel();
    assert!(!p.is_previewing());
}

// ── §4 Edge cases ──────────────────────────────────────────────────────────────

#[test]
fn t330_begin_preview_with_no_inspected_layer_is_valid() {
    let mut p = panel();
    assert!(!p.is_inspecting());
    // begin_preview is decoupled from inspected_id — it's valid regardless.
    p.begin_preview(VariantState::Hover);
    assert!(p.is_previewing());
}

#[test]
fn t331_all_six_states_can_be_previewed() {
    let states = [
        VariantState::Default,
        VariantState::Hover,
        VariantState::Active,
        VariantState::Disabled,
        VariantState::Focus,
        VariantState::Error,
    ];
    let mut p = panel();
    for state in states {
        p.begin_preview(state);
        assert_eq!(p.active_display_state(), state, "preview {state:?} failed");
        p.end_preview();
    }
}

#[test]
fn t332_cancel_preview_does_not_change_active_state() {
    let mut p = panel();
    p.active_state = VariantState::Active;
    p.begin_preview(VariantState::Hover);
    p.end_preview();  // cancel
    assert_eq!(p.active_state, VariantState::Active, "active_state should be unchanged");
    assert_eq!(p.preview_state, None);
}

#[test]
fn t333_begin_then_end_active_display_state_returns_active() {
    let mut p = panel();
    p.active_state = VariantState::Focus;
    p.begin_preview(VariantState::Error);
    p.end_preview();
    assert_eq!(p.active_display_state(), VariantState::Focus);
}

#[test]
fn t334_commit_preview_same_as_active_returns_false() {
    let mut p = panel();
    // active_state is already Default; preview the same state.
    p.begin_preview(VariantState::Default);
    let changed = p.commit_preview();
    assert!(!changed, "committing same state should return false");
    // active_state should still be Default.
    assert_eq!(p.active_state, VariantState::Default);
}

#[test]
fn t335_preview_state_is_none_after_new() {
    let p = panel();
    assert_eq!(p.preview_state, None);
    assert!(!p.is_previewing());
}
