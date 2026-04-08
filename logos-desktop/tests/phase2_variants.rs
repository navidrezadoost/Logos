// SPDX-License-Identifier: MPL-2.0
// logos-desktop/tests/phase2_variants.rs — Phase 2 integration tests
//
//  Test numbering: t201–t250 (50 tests across 5 sections).
//
//  §1  VariantPanel construction + inspection   (t201–t210)
//  §2  State transitions                        (t211–t220)
//  §3  WorkspaceMode selector                   (t221–t230)
//  §4  Command round-trip                       (t231–t240)
//  §5  PanelManager integration                 (t241–t250)

use logos_core::WorkspaceMode;
use logos_core::container::{ComponentRef, ComponentVariant, PropertyOverride, VariantState};
use serde_json::json;
use logos_desktop::commands::{
    Command, CommandCategory, CommandHistory, CommandRegistry, PanelId,
};
use logos_desktop::panels::{DockSide, PanelManager};
use logos_desktop::variants::VariantPanel;
use uuid::Uuid;

// ── Helpers ─────────────────────────────────────────────────────────────────

fn make_component_ref(states: &[VariantState]) -> ComponentRef {
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

fn make_component_ref_with_base_override() -> ComponentRef {
    let mut cref = make_component_ref(&[VariantState::Hover]);
    cref.set_base_override("fill", json!("#FF0000"));
    cref
}

// ── §1: VariantPanel construction + inspection ───────────────────────────────

#[test]
fn t201_variant_panel_new_not_inspecting() {
    let panel = VariantPanel::new(WorkspaceMode::Hybrid);
    assert!(!panel.is_inspecting(), "new panel should not be inspecting");
}

#[test]
fn t202_variant_panel_new_default_state() {
    let panel = VariantPanel::new(WorkspaceMode::FlatPage);
    assert_eq!(panel.active_state, VariantState::Default);
}

#[test]
fn t203_variant_panel_new_mode_stored() {
    let panel = VariantPanel::new(WorkspaceMode::ArtboardSection);
    assert_eq!(panel.workspace_mode, WorkspaceMode::ArtboardSection);
}

#[test]
fn t204_variant_panel_new_no_overrides() {
    let panel = VariantPanel::new(WorkspaceMode::Hybrid);
    assert_eq!(panel.active_override_count(), 0);
}

#[test]
fn t205_inspect_sets_inspected_id() {
    let mut panel = VariantPanel::new(WorkspaceMode::Hybrid);
    let id = Uuid::new_v4();
    let cref = make_component_ref(&[]);
    panel.inspect(id, &cref);
    assert_eq!(panel.inspected_id, Some(id));
    assert!(panel.is_inspecting());
}

#[test]
fn t206_inspect_populates_available_states() {
    let mut panel = VariantPanel::new(WorkspaceMode::Hybrid);
    let cref = make_component_ref(&[VariantState::Hover, VariantState::Active, VariantState::Disabled]);
    panel.inspect(Uuid::new_v4(), &cref);
    assert_eq!(panel.available_states.len(), 3);
    assert!(panel.has_variant(VariantState::Hover));
    assert!(panel.has_variant(VariantState::Active));
    assert!(panel.has_variant(VariantState::Disabled));
}

#[test]
fn t207_inspect_loads_base_overrides() {
    let mut panel = VariantPanel::new(WorkspaceMode::Hybrid);
    let cref = make_component_ref_with_base_override();
    panel.inspect(Uuid::new_v4(), &cref);
    assert_eq!(panel.active_override_count(), 1, "base override should be loaded");
}

#[test]
fn t208_clear_resets_inspected_id() {
    let mut panel = VariantPanel::new(WorkspaceMode::Hybrid);
    panel.inspect(Uuid::new_v4(), &make_component_ref(&[VariantState::Focus]));
    panel.clear();
    assert!(!panel.is_inspecting());
    assert!(panel.inspected_id.is_none());
}

#[test]
fn t209_clear_resets_overrides() {
    let mut panel = VariantPanel::new(WorkspaceMode::Hybrid);
    panel.inspect(Uuid::new_v4(), &make_component_ref_with_base_override());
    panel.clear();
    assert_eq!(panel.active_override_count(), 0);
}

#[test]
fn t210_clear_resets_available_states() {
    let mut panel = VariantPanel::new(WorkspaceMode::Hybrid);
    panel.inspect(Uuid::new_v4(), &make_component_ref(&[VariantState::Error]));
    panel.clear();
    assert!(panel.available_states.is_empty());
}

// ── §2: State transitions ────────────────────────────────────────────────────

#[test]
fn t211_set_state_hover_returns_true() {
    let mut panel = VariantPanel::new(WorkspaceMode::Hybrid);
    assert!(panel.set_state(VariantState::Hover));
    assert_eq!(panel.active_state, VariantState::Hover);
}

#[test]
fn t212_set_state_active_returns_true() {
    let mut panel = VariantPanel::new(WorkspaceMode::Hybrid);
    assert!(panel.set_state(VariantState::Active));
}

#[test]
fn t213_set_state_disabled_returns_true() {
    let mut panel = VariantPanel::new(WorkspaceMode::Hybrid);
    assert!(panel.set_state(VariantState::Disabled));
}

#[test]
fn t214_set_state_focus_returns_true() {
    let mut panel = VariantPanel::new(WorkspaceMode::Hybrid);
    assert!(panel.set_state(VariantState::Focus));
}

#[test]
fn t215_set_state_error_returns_true() {
    let mut panel = VariantPanel::new(WorkspaceMode::Hybrid);
    assert!(panel.set_state(VariantState::Error));
}

#[test]
fn t216_set_state_default_returns_true_when_different() {
    let mut panel = VariantPanel::new(WorkspaceMode::Hybrid);
    panel.set_state(VariantState::Hover);
    assert!(panel.set_state(VariantState::Default));
}

#[test]
fn t217_set_same_state_returns_false() {
    let mut panel = VariantPanel::new(WorkspaceMode::Hybrid);
    panel.set_state(VariantState::Active);
    assert!(!panel.set_state(VariantState::Active), "double-set should be no-op");
}

#[test]
fn t218_default_set_same_returns_false() {
    let mut panel = VariantPanel::new(WorkspaceMode::Hybrid);
    // starts at Default
    assert!(!panel.set_state(VariantState::Default));
}

#[test]
fn t219_all_six_states_cycle() {
    let mut panel = VariantPanel::new(WorkspaceMode::Hybrid);
    let states = VariantState::all();
    assert_eq!(states.len(), 6, "expected exactly 6 variant states");
    for &s in states {
        panel.set_state(VariantState::Default); // reset
        assert!(panel.set_state(s) || s == VariantState::Default);
    }
}

#[test]
fn t220_has_variant_false_for_uninspected_state() {
    let mut panel = VariantPanel::new(WorkspaceMode::Hybrid);
    let cref = make_component_ref(&[VariantState::Hover]);
    panel.inspect(Uuid::new_v4(), &cref);
    assert!(!panel.has_variant(VariantState::Active));
    assert!(!panel.has_variant(VariantState::Disabled));
}

// ── §3: WorkspaceMode selector ───────────────────────────────────────────────

#[test]
fn t221_set_workspace_mode_flat_page() {
    let mut panel = VariantPanel::new(WorkspaceMode::Hybrid);
    panel.set_workspace_mode(WorkspaceMode::FlatPage);
    assert_eq!(panel.workspace_mode, WorkspaceMode::FlatPage);
}

#[test]
fn t222_set_workspace_mode_artboard_section() {
    let mut panel = VariantPanel::new(WorkspaceMode::FlatPage);
    panel.set_workspace_mode(WorkspaceMode::ArtboardSection);
    assert_eq!(panel.workspace_mode, WorkspaceMode::ArtboardSection);
}

#[test]
fn t223_set_workspace_mode_hybrid() {
    let mut panel = VariantPanel::new(WorkspaceMode::FlatPage);
    panel.set_workspace_mode(WorkspaceMode::Hybrid);
    assert_eq!(panel.workspace_mode, WorkspaceMode::Hybrid);
}

#[test]
fn t224_workspace_mode_hybrid_supports_artboards() {
    assert!(WorkspaceMode::Hybrid.supports_artboards());
}

#[test]
fn t225_workspace_mode_hybrid_supports_flat() {
    assert!(WorkspaceMode::Hybrid.supports_flat());
}

#[test]
fn t226_workspace_mode_flat_page_no_artboards() {
    assert!(!WorkspaceMode::FlatPage.supports_artboards());
    assert!(WorkspaceMode::FlatPage.supports_flat());
}

#[test]
fn t227_workspace_mode_artboard_no_flat() {
    assert!(WorkspaceMode::ArtboardSection.supports_artboards());
    assert!(!WorkspaceMode::ArtboardSection.supports_flat());
}

#[test]
fn t228_workspace_mode_flat_page_label() {
    assert_eq!(WorkspaceMode::FlatPage.label(), "Flat Page");
}

#[test]
fn t229_workspace_mode_artboard_label() {
    assert_eq!(WorkspaceMode::ArtboardSection.label(), "Artboard / Section");
}

#[test]
fn t230_workspace_mode_hybrid_label() {
    assert_eq!(WorkspaceMode::Hybrid.label(), "Hybrid");
}

// ── §4: Command round-trip ───────────────────────────────────────────────────

#[test]
fn t231_set_variant_state_command_in_history() {
    let mut history = CommandHistory::new(100);
    let id = Uuid::new_v4();
    history.push(Command::SetVariantState { id, state: VariantState::Hover });
    assert!(history.can_undo());
}

#[test]
fn t232_set_variant_state_command_undo() {
    let mut history = CommandHistory::new(100);
    let id = Uuid::new_v4();
    history.push(Command::SetVariantState { id, state: VariantState::Active });
    let undone = history.pop_undo().unwrap();
    assert!(matches!(undone.command, Command::SetVariantState { state: VariantState::Active, .. }));
}

#[test]
fn t233_set_workspace_mode_command_in_history() {
    let mut history = CommandHistory::new(100);
    history.push(Command::SetWorkspaceMode { mode: WorkspaceMode::FlatPage });
    assert!(history.can_undo());
}

#[test]
fn t234_set_workspace_mode_command_redo() {
    let mut history = CommandHistory::new(100);
    history.push(Command::SetWorkspaceMode { mode: WorkspaceMode::ArtboardSection });
    history.pop_undo();
    let redone = history.pop_redo().unwrap();
    assert!(matches!(redone.command, Command::SetWorkspaceMode { mode: WorkspaceMode::ArtboardSection }));
}

#[test]
fn t235_command_to_id_set_variant_state() {
    use logos_desktop::commands::command_to_id;
    let cmd = Command::SetVariantState { id: Uuid::new_v4(), state: VariantState::Hover };
    assert_eq!(command_to_id(&cmd), "variant.set-state");
}

#[test]
fn t236_command_to_id_set_workspace_mode() {
    use logos_desktop::commands::command_to_id;
    let cmd = Command::SetWorkspaceMode { mode: WorkspaceMode::Hybrid };
    assert_eq!(command_to_id(&cmd), "workspace.set-mode");
}

#[test]
fn t237_registry_contains_variant_set_state() {
    let reg = CommandRegistry::new();
    let info = reg.get("variant.set-state").unwrap();
    assert_eq!(info.category, CommandCategory::Layer);
    assert!(info.enabled);
}

#[test]
fn t238_registry_contains_workspace_set_mode() {
    let reg = CommandRegistry::new();
    let info = reg.get("workspace.set-mode").unwrap();
    assert_eq!(info.category, CommandCategory::View);
    assert!(info.enabled);
}

#[test]
fn t239_variant_state_commands_searchable() {
    let reg = CommandRegistry::new();
    let results = reg.search("variant");
    assert!(!results.is_empty(), "should find variant.set-state in registry");
}

#[test]
fn t240_workspace_mode_command_searchable() {
    let reg = CommandRegistry::new();
    let results = reg.search("workspace");
    assert!(!results.is_empty(), "should find workspace.set-mode in registry");
}

// ── §5: PanelManager integration ─────────────────────────────────────────────

#[test]
fn t241_panel_manager_has_8_panels() {
    let mgr = PanelManager::new();
    assert_eq!(mgr.panel_count(), 8, "expected 8 panels after adding Variants");
}

#[test]
fn t242_variants_panel_is_registered() {
    let mgr = PanelManager::new();
    assert!(mgr.state(PanelId::Variants).is_some());
}

#[test]
fn t243_variants_panel_on_right_dock() {
    let mgr = PanelManager::new();
    assert_eq!(
        mgr.state(PanelId::Variants).unwrap().dock,
        DockSide::Right,
    );
}

#[test]
fn t244_variants_panel_default_visible() {
    let mgr = PanelManager::new();
    assert!(mgr.is_visible(PanelId::Variants));
}

#[test]
fn t245_toggle_variants_panel() {
    let mut mgr = PanelManager::new();
    mgr.toggle(PanelId::Variants);
    assert!(!mgr.is_visible(PanelId::Variants));
    mgr.toggle(PanelId::Variants);
    assert!(mgr.is_visible(PanelId::Variants));
}

#[test]
fn t246_variants_panel_default_width() {
    let mgr = PanelManager::new();
    let state = mgr.state(PanelId::Variants).unwrap();
    assert!((state.width - 280.0).abs() < f32::EPSILON);
}

#[test]
fn t247_variants_panel_appears_in_visible_right() {
    let mgr = PanelManager::new();
    assert!(mgr.visible_right().contains(&PanelId::Variants));
}

#[test]
fn t248_hide_variants_removes_from_visible_right() {
    let mut mgr = PanelManager::new();
    mgr.hide(PanelId::Variants);
    assert!(!mgr.visible_right().contains(&PanelId::Variants));
}

#[test]
fn t249_canvas_width_accounts_for_variants_panel() {
    // Use a wide viewport so panel totals don't bottom-out at the 100px clamp.
    let vp = 3000.0_f32;
    let mgr = PanelManager::new();
    let canvas_with = mgr.canvas_width(vp);
    let mut mgr2 = PanelManager::new();
    mgr2.hide(PanelId::Variants);
    let canvas_without = mgr2.canvas_width(vp);
    // Hiding the 280-wide Variants panel should widen the canvas.
    assert!(
        canvas_without > canvas_with,
        "canvas should widen when Variants panel is hidden ({} > {})",
        canvas_without,
        canvas_with
    );
}

#[test]
fn t250_panel_id_variants_display() {
    let id = PanelId::Variants;
    assert_eq!(id.to_string(), "Variants");
}
