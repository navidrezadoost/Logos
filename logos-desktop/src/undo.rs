// SPDX-License-Identifier: MPL-2.0
// logos-desktop/src/undo.rs — Desktop-level Undo / Redo stack
//
//  This module lives in *logos-desktop* (not logos-core) because the richer
//  `UndoAction` variants reference types from both `logos-core` and
//  `logos-layout`, and logos-layout already depends on logos-core.  Putting
//  new undo variants in logos-core would create a circular dependency.
//
//  The existing `logos_core::UndoStack` / `logos_core::UndoAction` are left
//  completely untouched; this is a separate, desktop-level stack that is
//  owned by `AppState`.

use uuid::Uuid;
use logos_core::container::VariantState;
use logos_layout::repeat_grid::DataSource;

// ═══════════════════════════════════════════════════════════════════════════
// UndoAction
// ═══════════════════════════════════════════════════════════════════════════

/// An atomic, reversible operation recorded by the desktop undo stack.
///
/// Each variant carries the information needed to both **undo** (revert to the
/// previous state) and **redo** (reapply the change).
#[derive(Clone, Debug, PartialEq)]
pub enum UndoAction {
    // ── Variant state change ───────────────────────────────────
    /// The user switched a component layer from one `VariantState` to another.
    SetVariantState {
        /// Layer whose component-ref was modified.
        layer_id: Uuid,
        /// The state before the change (undo restores this).
        old_state: VariantState,
        /// The state after the change (redo reapplies this).
        new_state: VariantState,
    },

    // ── Grid data override ─────────────────────────────────────
    /// A cell-level data override was set (or cleared) in a repeat grid.
    SetDataOverride {
        /// The repeat grid that was modified.
        grid_id: Uuid,
        /// Zero-based row index of the changed cell.
        row: u32,
        /// Zero-based column index of the changed cell.
        col: u32,
        /// Dot-separated property path (e.g. `"label.text"`).
        path: String,
        /// Value before the edit (`None` means the override did not exist).
        old_value: Option<serde_json::Value>,
        /// Value after the edit.
        new_value: serde_json::Value,
    },

    // ── Data source attach / detach ────────────────────────────
    /// A `DataSource` was attached to a repeat grid.
    AttachDataSource {
        /// The grid the source was attached to.
        grid_id: Uuid,
        /// The source that was added (undo removes it, redo re-adds it).
        source: DataSource,
    },
    /// A `DataSource` was detached from a repeat grid.
    DetachDataSource {
        /// The grid the source was detached from.
        grid_id: Uuid,
        /// The source that was removed (undo re-adds it, redo removes it).
        source: DataSource,
    },
}

impl UndoAction {
    /// A short human-readable description for display in an "Edit » Undo …" menu.
    pub fn description(&self) -> &'static str {
        match self {
            Self::SetVariantState { .. }    => "Set Variant State",
            Self::SetDataOverride { .. }    => "Set Data Override",
            Self::AttachDataSource { .. }   => "Attach Data Source",
            Self::DetachDataSource { .. }   => "Detach Data Source",
        }
    }

    /// Build the **inverse** action so that applying the inverse is equivalent
    /// to undoing the original.
    ///
    /// For simple swaps (e.g. `SetVariantState`) this swaps old/new.
    /// For attach/detach this returns the mirror operation.
    /// For add/remove layer this returns the counterpart.
    pub fn inverse(&self) -> Self {
        match self {
            Self::SetVariantState { layer_id, old_state, new_state } => {
                Self::SetVariantState {
                    layer_id: *layer_id,
                    old_state: *new_state,
                    new_state: *old_state,
                }
            }
            Self::SetDataOverride { grid_id, row, col, path, old_value, new_value } => {
                Self::SetDataOverride {
                    grid_id: *grid_id,
                    row: *row,
                    col: *col,
                    path: path.clone(),
                    old_value: Some(new_value.clone()),
                    new_value: old_value.clone().unwrap_or(serde_json::Value::Null),
                }
            }
            Self::AttachDataSource { grid_id, source } => {
                Self::DetachDataSource { grid_id: *grid_id, source: source.clone() }
            }
            Self::DetachDataSource { grid_id, source } => {
                Self::AttachDataSource { grid_id: *grid_id, source: source.clone() }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// UndoStack
// ═══════════════════════════════════════════════════════════════════════════

/// A bounded undo / redo stack for the desktop application.
///
/// The stack separates the *undo history* (actions that can be undone) from
/// the *redo future* (actions that were undone and can be reapplied).
/// Pushing a new action while the redo stack is non-empty discards the
/// redo future (standard linear undo model).
#[derive(Debug, Clone)]
pub struct UndoStack {
    /// Actions that can still be undone, newest at the back.
    undo: Vec<UndoAction>,
    /// Actions that have been undone and can be redone, newest at the back.
    redo: Vec<UndoAction>,
    /// Maximum number of entries in the undo history.
    max_depth: usize,
}

impl UndoStack {
    /// Create a new stack with the given `max_depth` limit.
    ///
    /// When the undo history exceeds `max_depth` the oldest entry is dropped.
    pub fn new(max_depth: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            max_depth,
        }
    }

    /// Push `action` onto the undo history.
    ///
    /// Clears the redo future and enforces the depth limit.
    pub fn push(&mut self, action: UndoAction) {
        // A new action invalidates the redo future.
        self.redo.clear();

        self.undo.push(action);

        // Trim to capacity (drop the oldest entry from the front).
        if self.undo.len() > self.max_depth {
            self.undo.remove(0);
        }
    }

    /// Pop the most recent action from the undo stack and push it onto the
    /// redo stack (preserving the same action so `redo_label()` reflects the
    /// original operation name).
    ///
    /// Returns the popped `UndoAction` so the caller can apply its **inverse**
    /// to the data model.  Returns `None` when the stack is empty.
    pub fn undo(&mut self) -> Option<UndoAction> {
        let action = self.undo.pop()?;
        self.redo.push(action.clone());
        Some(action)
    }

    /// Pop the most recent redo action, push it back onto the undo stack, and
    /// return it so the caller can **re-apply** it to the data model.
    ///
    /// Returns `None` when there is nothing to redo.
    pub fn redo(&mut self) -> Option<UndoAction> {
        let action = self.redo.pop()?;
        self.undo.push(action.clone());
        Some(action)
    }

    /// `true` if there is at least one action that can be undone.
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// `true` if there is at least one action that can be redone.
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Number of entries in the undo history.
    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    /// Number of entries in the redo stack.
    pub fn redo_depth(&self) -> usize {
        self.redo.len()
    }

    /// Clear both the undo history and the redo future.
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    /// Return the description of the most-recent undo action (for "Undo X"
    /// menu text).  Returns `None` when there is nothing to undo.
    pub fn undo_label(&self) -> Option<&'static str> {
        self.undo.last().map(|a| a.description())
    }

    /// Return the description of the most-recent redo action (for "Redo X"
    /// menu text).  Returns `None` when there is nothing to redo.
    pub fn redo_label(&self) -> Option<&'static str> {
        self.redo.last().map(|a| a.description())
    }

    /// Maximum stack depth as configured at construction.
    pub fn max_depth(&self) -> usize {
        self.max_depth
    }
}

impl Default for UndoStack {
    fn default() -> Self {
        Self::new(200)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sv_action(layer_id: Uuid, old: VariantState, new: VariantState) -> UndoAction {
        UndoAction::SetVariantState { layer_id, old_state: old, new_state: new }
    }

    #[test]
    fn test_new_stack_is_empty() {
        let s = UndoStack::new(10);
        assert!(!s.can_undo());
        assert!(!s.can_redo());
        assert_eq!(s.undo_depth(), 0);
        assert_eq!(s.redo_depth(), 0);
    }

    #[test]
    fn test_push_increases_depth() {
        let mut s = UndoStack::new(10);
        let id = Uuid::new_v4();
        s.push(sv_action(id, VariantState::Default, VariantState::Hover));
        assert_eq!(s.undo_depth(), 1);
        assert!(s.can_undo());
    }

    #[test]
    fn test_undo_returns_action() {
        let mut s = UndoStack::new(10);
        let id = Uuid::new_v4();
        let action = sv_action(id, VariantState::Default, VariantState::Hover);
        s.push(action.clone());
        let undone = s.undo().unwrap();
        assert_eq!(undone, action);
    }

    #[test]
    fn test_undo_pushes_original_to_redo() {
        let mut s = UndoStack::new(10);
        let id = Uuid::new_v4();
        let action = sv_action(id, VariantState::Default, VariantState::Hover);
        s.push(action.clone());
        s.undo();
        // The redo stack should hold the same action (not its inverse)
        // so that redo_label() returns the original description.
        assert!(s.can_redo());
        if let Some(UndoAction::SetVariantState { old_state, new_state, .. }) = s.redo.last() {
            assert_eq!(*old_state, VariantState::Default);
            assert_eq!(*new_state, VariantState::Hover);
        }
    }

    #[test]
    fn test_redo_restores_undo_entry() {
        let mut s = UndoStack::new(10);
        let id = Uuid::new_v4();
        let original = sv_action(id, VariantState::Default, VariantState::Active);
        s.push(original.clone());
        s.undo();
        s.redo();
        // After redo, undo stack should have the action back, redo should be empty.
        assert!(s.can_undo());
        assert!(!s.can_redo());
        assert_eq!(s.undo_label(), Some("Set Variant State"));
    }

    #[test]
    fn test_push_clears_redo() {
        let mut s = UndoStack::new(10);
        let id = Uuid::new_v4();
        s.push(sv_action(id, VariantState::Default, VariantState::Hover));
        s.undo(); // Moves to redo
        assert!(s.can_redo());
        s.push(sv_action(id, VariantState::Hover, VariantState::Active));
        assert!(!s.can_redo());
    }

    #[test]
    fn test_max_depth_trimming() {
        let mut s = UndoStack::new(3);
        let id = Uuid::new_v4();
        for i in 0u64..5 {
            let state = if i % 2 == 0 { VariantState::Default } else { VariantState::Hover };
            s.push(sv_action(id, state, state));
        }
        assert_eq!(s.undo_depth(), 3, "Stack should be capped at max_depth");
    }

    #[test]
    fn test_clear_empties_both_stacks() {
        let mut s = UndoStack::new(10);
        let id = Uuid::new_v4();
        s.push(sv_action(id, VariantState::Default, VariantState::Hover));
        s.undo();
        s.clear();
        assert!(!s.can_undo());
        assert!(!s.can_redo());
    }

    #[test]
    fn test_undo_empty_returns_none() {
        let mut s = UndoStack::new(10);
        assert!(s.undo().is_none());
    }

    #[test]
    fn test_redo_empty_returns_none() {
        let mut s = UndoStack::new(10);
        assert!(s.redo().is_none());
    }

    #[test]
    fn test_description_set_variant_state() {
        let id = Uuid::new_v4();
        let a = sv_action(id, VariantState::Default, VariantState::Hover);
        assert_eq!(a.description(), "Set Variant State");
    }

    #[test]
    fn test_description_data_source_actions() {
        let src = DataSource::new("s", "k", vec![json!(1)]);
        let attach = UndoAction::AttachDataSource { grid_id: Uuid::new_v4(), source: src.clone() };
        let detach = UndoAction::DetachDataSource { grid_id: Uuid::new_v4(), source: src };
        assert_eq!(attach.description(), "Attach Data Source");
        assert_eq!(detach.description(), "Detach Data Source");
    }

    #[test]
    fn test_attach_inverse_is_detach() {
        let src = DataSource::new("s", "k", vec![json!(1)]);
        let gid = Uuid::new_v4();
        let attach = UndoAction::AttachDataSource { grid_id: gid, source: src.clone() };
        if let UndoAction::DetachDataSource { grid_id, source } = attach.inverse() {
            assert_eq!(grid_id, gid);
            assert_eq!(source, src);
        } else {
            panic!("expected DetachDataSource");
        }
    }

    #[test]
    fn test_undo_label_shows_last_action() {
        let mut s = UndoStack::new(10);
        assert!(s.undo_label().is_none());
        let id = Uuid::new_v4();
        s.push(sv_action(id, VariantState::Default, VariantState::Hover));
        assert_eq!(s.undo_label(), Some("Set Variant State"));
    }

    #[test]
    fn test_redo_label_after_undo() {
        let mut s = UndoStack::new(10);
        let id = Uuid::new_v4();
        s.push(sv_action(id, VariantState::Default, VariantState::Hover));
        s.undo();
        assert_eq!(s.redo_label(), Some("Set Variant State"));
    }
}
