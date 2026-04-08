// Phase 4 – Variant Thumbnails & Accessibility Tests (t416–t430)
//
// Tests for `VariantPanel::build_thumbnails`, `thumbnail_for_state`,
// `thumbnail_count`, `to_accessibility_nodes`, and
// `accessibility_label_for_state`.

use logos_core::WorkspaceMode;
use logos_core::container::{ComponentRef, ComponentVariant, VariantState};
use logos_desktop::variants::{VariantPanel, VariantThumbnail};
use uuid::Uuid;

// ── Helpers ────────────────────────────────────────────────────────────────────

fn panel_with_states(states: &[VariantState]) -> VariantPanel {
    let mut panel = VariantPanel::new(WorkspaceMode::Hybrid);
    let id = Uuid::new_v4();
    let mut cref = ComponentRef {
        component_id: Uuid::new_v4(),
        overrides: Vec::new(),
        variants: Vec::new(),
        current_state: VariantState::Default,
    };
    for &s in states {
        cref.add_variant(ComponentVariant::new(s));
    }
    panel.inspect(id, &cref);
    panel
}

// ── §1 thumbnail_count ────────────────────────────────────────────────────────

#[test]
fn t416_thumbnail_count_zero_when_no_variants() {
    let panel = VariantPanel::new(WorkspaceMode::Hybrid);
    assert_eq!(panel.thumbnail_count(), 0);
}

#[test]
fn t417_thumbnail_count_matches_available_states() {
    let panel = panel_with_states(&[VariantState::Hover, VariantState::Active, VariantState::Focus]);
    assert_eq!(panel.thumbnail_count(), 3);
}

// ── §2 build_thumbnails ───────────────────────────────────────────────────────

#[test]
fn t418_build_thumbnails_empty_for_no_variants() {
    let panel = VariantPanel::new(WorkspaceMode::Hybrid);
    assert!(panel.build_thumbnails().is_empty());
}

#[test]
fn t419_build_thumbnails_length_matches_states() {
    let panel = panel_with_states(&[VariantState::Hover, VariantState::Disabled]);
    let thumbnails = panel.build_thumbnails();
    assert_eq!(thumbnails.len(), 2);
}

#[test]
fn t420_thumbnail_active_flag_set_for_active_state() {
    let mut panel = panel_with_states(&[VariantState::Hover, VariantState::Active]);
    panel.set_state(VariantState::Hover);
    let thumbnails = panel.build_thumbnails();
    let hover_thumb = thumbnails.iter().find(|t| t.state == VariantState::Hover).unwrap();
    assert!(hover_thumb.is_active);
    let active_thumb = thumbnails.iter().find(|t| t.state == VariantState::Active).unwrap();
    assert!(!active_thumb.is_active);
}

#[test]
fn t421_thumbnail_is_display_state_follows_preview() {
    let mut panel = panel_with_states(&[VariantState::Hover, VariantState::Active]);
    // No preview yet — active state is Default implicitly, available states are Hover/Active.
    panel.set_state(VariantState::Hover);
    panel.begin_preview(VariantState::Active);
    let thumbnails = panel.build_thumbnails();
    let active_thumb = thumbnails.iter().find(|t| t.state == VariantState::Active).unwrap();
    assert!(active_thumb.is_display_state);
    let hover_thumb = thumbnails.iter().find(|t| t.state == VariantState::Hover).unwrap();
    assert!(!hover_thumb.is_display_state);
}

#[test]
fn t422_thumbnail_label_is_human_readable() {
    let panel = panel_with_states(&[VariantState::Hover]);
    let thumbs = panel.build_thumbnails();
    assert_eq!(thumbs[0].label, "Hover state");
}

#[test]
fn t423_thumbnail_base_color_starts_none() {
    let panel = panel_with_states(&[VariantState::Active]);
    let t = &panel.build_thumbnails()[0];
    assert!(t.base_color.is_none());
}

// ── §3 thumbnail_for_state ────────────────────────────────────────────────────

#[test]
fn t424_thumbnail_for_state_returns_correct_thumbnail() {
    let panel = panel_with_states(&[VariantState::Hover, VariantState::Focus]);
    let t = panel.thumbnail_for_state(VariantState::Focus).unwrap();
    assert_eq!(t.state, VariantState::Focus);
    assert_eq!(t.label, "Focus state");
}

#[test]
fn t425_thumbnail_for_state_missing_returns_none() {
    let panel = panel_with_states(&[VariantState::Hover]);
    assert!(panel.thumbnail_for_state(VariantState::Error).is_none());
}

// ── §4 accessibility_label_for_state ─────────────────────────────────────────

#[test]
fn t426_label_default_state() {
    let panel = VariantPanel::new(WorkspaceMode::Hybrid);
    assert_eq!(panel.accessibility_label_for_state(VariantState::Default), "Default state");
}

#[test]
fn t427_label_all_states_end_with_state() {
    let panel = VariantPanel::new(WorkspaceMode::Hybrid);
    let all = [
        VariantState::Default, VariantState::Hover, VariantState::Active,
        VariantState::Focus, VariantState::Disabled, VariantState::Error,
    ];
    for s in all {
        let label = panel.accessibility_label_for_state(s);
        assert!(label.ends_with(" state"), "label `{label}` should end with \" state\"");
    }
}

// ── §5 to_accessibility_nodes ─────────────────────────────────────────────────

#[test]
fn t428_accessibility_nodes_empty_panel_has_one_container() {
    let panel = VariantPanel::new(WorkspaceMode::Hybrid);
    let nodes = panel.to_accessibility_nodes();
    // Only the container List node (no items).
    assert_eq!(nodes.len(), 1);
}

#[test]
fn t429_accessibility_nodes_count_equals_states_plus_one_container() {
    let panel = panel_with_states(&[VariantState::Hover, VariantState::Active, VariantState::Focus]);
    let nodes = panel.to_accessibility_nodes();
    // 1 container + 3 items
    assert_eq!(nodes.len(), 4);
}

#[test]
fn t430_accessibility_active_node_is_selected() {
    let mut panel = panel_with_states(&[VariantState::Hover, VariantState::Active]);
    panel.set_state(VariantState::Hover);
    let nodes = panel.to_accessibility_nodes();
    // Container is first; items follow.
    let hover_item = nodes.iter().skip(1).find(|n| n.label == "Hover state").unwrap();
    assert!(hover_item.selected);
    let active_item = nodes.iter().skip(1).find(|n| n.label == "Active state").unwrap();
    assert!(!active_item.selected);
}
