// SPDX-License-Identifier: MPL-2.0
// logos-desktop/src/variants.rs — Variant panel state model
//
//  Holds the data model for the Variants panel: which component is being
//  inspected, which state is active, and the merged property overrides for
//  that state.  Also serves as the single source of truth for the current
//  WorkspaceMode selector that lives inside the same panel.

use uuid::Uuid;
use logos_core::WorkspaceMode;
use logos_core::container::{ComponentRef, PropertyOverride, VariantState};
#[cfg(test)] use serde_json::json;

// ── VariantPanel ────────────────────────────────────────────────────────────

/// State for the Variants panel.
///
/// Mirrors the selection-driven pattern used by [`crate::panels::PropertiesPanel`]:
/// inspect a component → show its states and live-merged overrides → user
/// switches state → overrides update.  A `WorkspaceMode` selector is embedded
/// here so that the same right-dock panel can surface both concerns without
/// needing a separate panel for mode switching.
#[derive(Debug, Clone)]
pub struct VariantPanel {
    /// The layer whose `ComponentRef` we are currently displaying.
    pub inspected_id: Option<Uuid>,

    /// All `VariantState`s that have explicit `ComponentVariant` entries on the
    /// inspected component (populated from `ComponentRef::variants`).
    pub available_states: Vec<VariantState>,

    /// The state currently selected for preview.
    pub active_state: VariantState,

    /// A temporary preview state shown on hover (hover-preview pattern).
    /// `None` when no in-flight preview is active.
    pub preview_state: Option<VariantState>,

    /// Merged property overrides for the `active_state` (base + state-specific).
    pub overrides: Vec<PropertyOverride>,

    /// The current workspace/document layout mode, mirrored here so the
    /// mode-selector widget has a single place to read from.
    pub workspace_mode: WorkspaceMode,

    /// When `true` all available states are shown; when `false` only the active
    /// state row is displayed (collapsed view).
    pub show_all_states: bool,
}

impl VariantPanel {
    /// Create a new `VariantPanel` with the given workspace mode and no
    /// inspected component.
    pub fn new(mode: WorkspaceMode) -> Self {
        Self {
            inspected_id: None,
            available_states: Vec::new(),
            active_state: VariantState::Default,
            preview_state: None,
            overrides: Vec::new(),
            workspace_mode: mode,
            show_all_states: true,
        }
    }

    /// Load the given component reference into the panel.
    ///
    /// Populates `available_states` from the component's variants and
    /// immediately computes `overrides` for the component's `current_state`.
    pub fn inspect(&mut self, id: Uuid, component_ref: &ComponentRef) {
        self.inspected_id = Some(id);
        self.active_state = component_ref.current_state;
        self.available_states = component_ref
            .variants
            .iter()
            .map(|v| v.state)
            .collect();
        self.overrides = component_ref.get_active_overrides();
    }

    /// Clear the inspection (deselects any component).
    pub fn clear(&mut self) {
        self.inspected_id = None;
        self.available_states.clear();
        self.active_state = VariantState::Default;
        self.overrides.clear();
    }

    /// Switch the previewed state. Returns `true` if the state actually
    /// changed (can be used as a "needs-redraw" signal).
    ///
    /// This method only updates the *panel* state; the caller is responsible
    /// for propagating the change back to the `ComponentRef` in the document.
    pub fn set_state(&mut self, state: VariantState) -> bool {
        if self.active_state == state {
            return false;
        }
        self.active_state = state;
        true
    }

    /// Update the workspace mode shown in the mode selector widget.
    pub fn set_workspace_mode(&mut self, mode: WorkspaceMode) {
        self.workspace_mode = mode;
    }

    /// `true` if a component is currently being inspected.
    pub fn is_inspecting(&self) -> bool {
        self.inspected_id.is_some()
    }

    /// Number of active (merged) overrides for the current state.
    pub fn active_override_count(&self) -> usize {
        self.overrides.len()
    }

    /// `true` if the given state has an explicit variant entry on the
    /// inspected component.
    pub fn has_variant(&self, state: VariantState) -> bool {
        self.available_states.contains(&state)
    }

    /// Toggle the expanded / collapsed state list view.
    pub fn toggle_show_all(&mut self) {
        self.show_all_states = !self.show_all_states;
    }

    // ── Live preview ──────────────────────────────────────────────────

    /// Begin a hover-preview for `state` without committing the active state.
    ///
    /// Callers should call [`end_preview`] when the hover leaves, or
    /// [`commit_preview`] when the user clicks to confirm.
    pub fn begin_preview(&mut self, state: VariantState) {
        self.preview_state = Some(state);
    }

    /// Cancel the in-flight preview and revert to `active_state`.
    pub fn end_preview(&mut self) {
        self.preview_state = None;
    }

    /// Commit the current preview as the new active state.
    ///
    /// Returns `true` if the active state actually changed (i.e. a preview
    /// was set and it differed from the current `active_state`).
    pub fn commit_preview(&mut self) -> bool {
        if let Some(preview) = self.preview_state.take() {
            if preview != self.active_state {
                self.active_state = preview;
                return true;
            }
        }
        false
    }

    /// The state to use for rendering: `preview_state` if in-flight, otherwise
    /// `active_state`.
    pub fn active_display_state(&self) -> VariantState {
        self.preview_state.unwrap_or(self.active_state)
    }

    /// `true` if a hover-preview is currently in-flight.
    pub fn is_previewing(&self) -> bool {
        self.preview_state.is_some()
    }
}

impl Default for VariantPanel {
    fn default() -> Self {
        Self::new(WorkspaceMode::Hybrid)
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use logos_core::container::{ComponentRef, ComponentVariant, PropertyOverride};

    fn make_ref_with_states(states: &[VariantState]) -> ComponentRef {
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

    #[test]
    fn test_new_default_state() {
        let panel = VariantPanel::new(WorkspaceMode::Hybrid);
        assert!(!panel.is_inspecting());
        assert_eq!(panel.active_state, VariantState::Default);
        assert_eq!(panel.workspace_mode, WorkspaceMode::Hybrid);
        assert!(panel.show_all_states);
    }

    #[test]
    fn test_inspect_populates_states() {
        let mut panel = VariantPanel::new(WorkspaceMode::Hybrid);
        let id = Uuid::new_v4();
        let cref = make_ref_with_states(&[VariantState::Hover, VariantState::Active]);
        panel.inspect(id, &cref);
        assert!(panel.is_inspecting());
        assert_eq!(panel.inspected_id, Some(id));
        assert_eq!(panel.available_states.len(), 2);
    }

    #[test]
    fn test_clear() {
        let mut panel = VariantPanel::new(WorkspaceMode::Hybrid);
        let cref = make_ref_with_states(&[VariantState::Hover]);
        panel.inspect(Uuid::new_v4(), &cref);
        panel.clear();
        assert!(!panel.is_inspecting());
        assert!(panel.available_states.is_empty());
        assert_eq!(panel.active_state, VariantState::Default);
    }

    #[test]
    fn test_set_state_returns_true_on_change() {
        let mut panel = VariantPanel::new(WorkspaceMode::Hybrid);
        assert!(panel.set_state(VariantState::Hover));
    }

    #[test]
    fn test_set_state_returns_false_on_same() {
        let mut panel = VariantPanel::new(WorkspaceMode::Hybrid);
        panel.set_state(VariantState::Active);
        assert!(!panel.set_state(VariantState::Active));
    }

    #[test]
    fn test_has_variant() {
        let mut panel = VariantPanel::new(WorkspaceMode::Hybrid);
        let cref = make_ref_with_states(&[VariantState::Disabled, VariantState::Focus]);
        panel.inspect(Uuid::new_v4(), &cref);
        assert!(panel.has_variant(VariantState::Disabled));
        assert!(panel.has_variant(VariantState::Focus));
        assert!(!panel.has_variant(VariantState::Error));
    }

    #[test]
    fn test_active_override_count() {
        let mut panel = VariantPanel::new(WorkspaceMode::Hybrid);
        let mut cref = make_ref_with_states(&[VariantState::Hover]);
        cref.set_base_override("fill", json!("red"));
        panel.inspect(Uuid::new_v4(), &cref);
        assert_eq!(panel.active_override_count(), 1);
    }

    #[test]
    fn test_set_workspace_mode() {
        let mut panel = VariantPanel::new(WorkspaceMode::FlatPage);
        panel.set_workspace_mode(WorkspaceMode::ArtboardSection);
        assert_eq!(panel.workspace_mode, WorkspaceMode::ArtboardSection);
    }

    #[test]
    fn test_toggle_show_all() {
        let mut panel = VariantPanel::new(WorkspaceMode::Hybrid);
        assert!(panel.show_all_states);
        panel.toggle_show_all();
        assert!(!panel.show_all_states);
        panel.toggle_show_all();
        assert!(panel.show_all_states);
    }
}
