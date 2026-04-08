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
use crate::accessibility::{AccessibilityNode, AriaRole};
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

    // ── Thumbnails ────────────────────────────────────────────────────

    /// Build a [`VariantThumbnail`] representation for every available state.
    ///
    /// Each thumbnail captures a snapshot of the state's label, override
    /// count, whether it is the currently active state, and whether it is
    /// the display state (preview or active).
    pub fn build_thumbnails(&self) -> Vec<VariantThumbnail> {
        self.available_states
            .iter()
            .map(|&state| {
                let label = self.accessibility_label_for_state(state);
                let override_count = self.overrides.len(); // per current panel overrides
                let is_active = self.active_state == state;
                let is_display_state = self.active_display_state() == state;
                VariantThumbnail {
                    state,
                    label,
                    override_count,
                    base_color: None,
                    is_active,
                    is_display_state,
                }
            })
            .collect()
    }

    /// Return the thumbnail for a specific `state`, or `None` if that state
    /// has no explicit variant entry on the inspected component.
    pub fn thumbnail_for_state(&self, state: VariantState) -> Option<VariantThumbnail> {
        self.build_thumbnails().into_iter().find(|t| t.state == state)
    }

    /// Number of thumbnails (= number of available states).
    pub fn thumbnail_count(&self) -> usize {
        self.available_states.len()
    }

    // ── Accessibility ────────────────────────────────────────────────

    /// Build an ordered list of [`AccessibilityNode`]s for the variants panel.
    ///
    /// One `List` node acts as the container; one `ListItem` child is created
    /// per available state.  The active state is marked as `selected`.
    pub fn to_accessibility_nodes(&self) -> Vec<AccessibilityNode> {
        let container_label = match self.inspected_id {
            Some(id) => format!("Variants for layer {}", id),
            None => "Variants panel".to_string(),
        };

        let mut container = AccessibilityNode::new(AriaRole::List, container_label);
        let mut nodes: Vec<AccessibilityNode> = Vec::new();

        for &state in &self.available_states {
            let label = self.accessibility_label_for_state(state);
            let mut item = AccessibilityNode::new(AriaRole::ListItem, label);
            item.selected = self.active_state == state;
            item.parent = Some(container.id);
            container.children.push(item.id);
            nodes.push(item);
        }

        let mut result = vec![container];
        result.extend(nodes);
        result
    }

    /// Human-readable accessibility label for `state`.
    ///
    /// Returns a sentence like `"Default state"`, `"Hover state"`, etc.
    pub fn accessibility_label_for_state(&self, state: VariantState) -> String {
        let name = match state {
            VariantState::Default  => "Default",
            VariantState::Hover    => "Hover",
            VariantState::Active   => "Active",
            VariantState::Focus    => "Focus",
            VariantState::Disabled => "Disabled",
            VariantState::Error    => "Error",
        };
        format!("{} state", name)
    }
}

// ── VariantThumbnail ─────────────────────────────────────────────────────────

/// A lightweight descriptor used to render a thumbnail row in the Variants
/// panel for one component state.
///
/// Built from [`VariantPanel::build_thumbnails`]; does not own a GPU texture
/// — rendering is delegated to the caller which can map `state` to a cached
/// bitmap.
#[derive(Debug, Clone, PartialEq)]
pub struct VariantThumbnail {
    /// The variant state this thumbnail represents.
    pub state: VariantState,
    /// Human-readable label (e.g. `"Hover state"`).
    pub label: String,
    /// Number of overrides active for the current panel state (informational).
    pub override_count: usize,
    /// Optional representative color sampled from the variant (CSS hex string).
    /// `None` when no color has been calculated yet.
    pub base_color: Option<String>,
    /// `true` when this state is the currently committed active state.
    pub is_active: bool,
    /// `true` when this state is the display state (preview or active).
    pub is_display_state: bool,
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
