//! # Prototype State Sync
//!
//! Real-time synchronization of prototype execution state between
//! multiple viewers. When a user triggers a prototype interaction
//! (e.g. click, hover), the action is broadcast so all viewers
//! see the same prototype state.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use logos_components::VariantKey;

// ── Identifiers ──────────────────────────────────────────────────────

/// Unique identifier for a prototype viewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ViewerId(pub Uuid);

impl ViewerId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ViewerId {
    fn default() -> Self {
        Self::new()
    }
}

// ── Prototype Actions ────────────────────────────────────────────────

/// An action in the prototype preview.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PrototypeAction {
    /// Navigate to a different frame/artboard.
    Navigate {
        from_frame: Uuid,
        to_frame: Uuid,
    },
    /// Swap a component instance to a different variant.
    SwapVariant {
        instance_id: Uuid,
        new_key: VariantKey,
    },
    /// Toggle an interactive state (e.g. checkbox, toggle).
    ToggleState {
        target_id: Uuid,
        state_name: String,
        active: bool,
    },
    /// Scroll to a position.
    Scroll {
        container_id: Uuid,
        x: f64,
        y: f64,
    },
    /// Hover enter/leave.
    Hover {
        target_id: Uuid,
        entered: bool,
    },
    /// Focus/blur an input.
    Focus {
        target_id: Uuid,
        focused: bool,
    },
    /// Set a text input value.
    TextInput {
        target_id: Uuid,
        value: String,
    },
    /// Trigger a custom action.
    Custom {
        action_name: String,
        payload: serde_json::Value,
    },
    /// Reset prototype to initial state.
    Reset,
}

impl PrototypeAction {
    /// Human-readable summary of this action.
    pub fn summary(&self) -> String {
        match self {
            Self::Navigate { to_frame, .. } => {
                format!("Navigate to {}", &to_frame.to_string()[..8])
            }
            Self::SwapVariant { .. } => "Swap variant".into(),
            Self::ToggleState {
                state_name, active, ..
            } => {
                format!("Toggle '{}' → {}", state_name, active)
            }
            Self::Scroll { .. } => "Scroll".into(),
            Self::Hover { entered, .. } => {
                if *entered {
                    "Hover enter".into()
                } else {
                    "Hover leave".into()
                }
            }
            Self::Focus { focused, .. } => {
                if *focused {
                    "Focus".into()
                } else {
                    "Blur".into()
                }
            }
            Self::TextInput { value, .. } => {
                format!("Text input: '{}'", &value[..value.len().min(20)])
            }
            Self::Custom { action_name, .. } => {
                format!("Custom: {}", action_name)
            }
            Self::Reset => "Reset prototype".into(),
        }
    }
}

// ── Sync Messages ────────────────────────────────────────────────────

/// A message sent between prototype viewers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrototypeSyncMessage {
    pub viewer_id: ViewerId,
    pub action: PrototypeAction,
    pub timestamp: u64,
    /// Logical clock for ordering.
    pub sequence: u64,
}

impl PrototypeSyncMessage {
    pub fn new(
        viewer_id: ViewerId,
        action: PrototypeAction,
        timestamp: u64,
        sequence: u64,
    ) -> Self {
        Self {
            viewer_id,
            action,
            timestamp,
            sequence,
        }
    }
}

// ── Viewer State ─────────────────────────────────────────────────────

/// The current state of a prototype viewer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViewerState {
    pub viewer_id: ViewerId,
    pub user_id: Uuid,
    pub user_name: String,
    pub current_frame: Uuid,
    pub variant_states: HashMap<Uuid, VariantKey>,
    pub toggle_states: HashMap<String, bool>,
    pub scroll_positions: HashMap<Uuid, (f64, f64)>,
    pub last_action: Option<PrototypeAction>,
    pub joined_at: u64,
    pub last_active: u64,
    pub is_controlling: bool,
}

impl ViewerState {
    pub fn new(
        viewer_id: ViewerId,
        user_id: Uuid,
        user_name: impl Into<String>,
        initial_frame: Uuid,
        timestamp: u64,
    ) -> Self {
        Self {
            viewer_id,
            user_id,
            user_name: user_name.into(),
            current_frame: initial_frame,
            variant_states: HashMap::new(),
            toggle_states: HashMap::new(),
            scroll_positions: HashMap::new(),
            last_action: None,
            joined_at: timestamp,
            last_active: timestamp,
            is_controlling: false,
        }
    }

    /// Apply an action to this viewer's state.
    pub fn apply_action(&mut self, action: &PrototypeAction, timestamp: u64) {
        self.last_active = timestamp;
        self.last_action = Some(action.clone());

        match action {
            PrototypeAction::Navigate { to_frame, .. } => {
                self.current_frame = *to_frame;
            }
            PrototypeAction::SwapVariant {
                instance_id,
                new_key,
            } => {
                self.variant_states
                    .insert(*instance_id, new_key.clone());
            }
            PrototypeAction::ToggleState {
                state_name, active, ..
            } => {
                self.toggle_states
                    .insert(state_name.clone(), *active);
            }
            PrototypeAction::Scroll { container_id, x, y } => {
                self.scroll_positions.insert(*container_id, (*x, *y));
            }
            PrototypeAction::Reset => {
                self.variant_states.clear();
                self.toggle_states.clear();
                self.scroll_positions.clear();
            }
            _ => {}
        }
    }

    pub fn idle_duration(&self, now: u64) -> u64 {
        now.saturating_sub(self.last_active)
    }
}

// ── Prototype Sync Room ──────────────────────────────────────────────

/// Sync mode for prototype preview rooms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncMode {
    /// All viewers see the same state (presenter-controlled).
    Follow,
    /// Each viewer can interact independently.
    Independent,
}

impl Default for SyncMode {
    fn default() -> Self {
        Self::Follow
    }
}

/// A room for synchronizing prototype state between multiple viewers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrototypeSyncRoom {
    pub document_id: Uuid,
    pub prototype_id: Uuid,
    pub sync_mode: SyncMode,
    pub viewers: HashMap<ViewerId, ViewerState>,
    pub controller: Option<ViewerId>,
    pub action_log: Vec<PrototypeSyncMessage>,
    pub next_sequence: u64,
    pub created_at: u64,
    /// Maximum action log size before truncation.
    pub max_log_size: usize,
}

impl PrototypeSyncRoom {
    pub fn new(document_id: Uuid, prototype_id: Uuid, timestamp: u64) -> Self {
        Self {
            document_id,
            prototype_id,
            sync_mode: SyncMode::Follow,
            viewers: HashMap::new(),
            controller: None,
            action_log: Vec::new(),
            next_sequence: 1,
            created_at: timestamp,
            max_log_size: 1000,
        }
    }

    /// Add a viewer to the room.
    pub fn add_viewer(
        &mut self,
        viewer_id: ViewerId,
        user_id: Uuid,
        user_name: impl Into<String>,
        initial_frame: Uuid,
        timestamp: u64,
    ) {
        let state =
            ViewerState::new(viewer_id, user_id, user_name, initial_frame, timestamp);
        self.viewers.insert(viewer_id, state);

        // First viewer becomes controller in Follow mode
        if self.controller.is_none() && self.sync_mode == SyncMode::Follow {
            self.controller = Some(viewer_id);
            if let Some(v) = self.viewers.get_mut(&viewer_id) {
                v.is_controlling = true;
            }
        }
    }

    /// Remove a viewer from the room.
    pub fn remove_viewer(&mut self, viewer_id: ViewerId) -> Option<ViewerState> {
        let state = self.viewers.remove(&viewer_id);
        // If controller left, hand off
        if self.controller == Some(viewer_id) {
            self.controller = self.viewers.keys().next().copied();
            if let Some(new_ctrl) = self.controller {
                if let Some(v) = self.viewers.get_mut(&new_ctrl) {
                    v.is_controlling = true;
                }
            }
        }
        state
    }

    /// Process an action from a viewer.
    ///
    /// In Follow mode, only the controller's actions are broadcast.
    /// In Independent mode, actions only affect the originating viewer.
    pub fn process_action(
        &mut self,
        viewer_id: ViewerId,
        action: PrototypeAction,
        timestamp: u64,
    ) -> Option<PrototypeSyncMessage> {
        let msg = PrototypeSyncMessage::new(
            viewer_id,
            action.clone(),
            timestamp,
            self.next_sequence,
        );
        self.next_sequence += 1;

        match self.sync_mode {
            SyncMode::Follow => {
                if self.controller == Some(viewer_id) {
                    // Apply to all viewers
                    let viewer_ids: Vec<_> = self.viewers.keys().copied().collect();
                    for vid in viewer_ids {
                        if let Some(v) = self.viewers.get_mut(&vid) {
                            v.apply_action(&action, timestamp);
                        }
                    }
                    self.log_action(msg.clone());
                    Some(msg)
                } else {
                    None // Non-controller action ignored in Follow mode
                }
            }
            SyncMode::Independent => {
                // Apply only to the originating viewer
                if let Some(v) = self.viewers.get_mut(&viewer_id) {
                    v.apply_action(&action, timestamp);
                }
                self.log_action(msg.clone());
                Some(msg)
            }
        }
    }

    /// Transfer control to another viewer.
    pub fn set_controller(&mut self, viewer_id: ViewerId) -> bool {
        if !self.viewers.contains_key(&viewer_id) {
            return false;
        }
        // Remove old controller flag
        if let Some(old) = self.controller {
            if let Some(v) = self.viewers.get_mut(&old) {
                v.is_controlling = false;
            }
        }
        self.controller = Some(viewer_id);
        if let Some(v) = self.viewers.get_mut(&viewer_id) {
            v.is_controlling = true;
        }
        true
    }

    /// Switch sync mode.
    pub fn set_sync_mode(&mut self, mode: SyncMode) {
        self.sync_mode = mode;
        if mode == SyncMode::Independent {
            // Clear controller in independent mode
            if let Some(old) = self.controller.take() {
                if let Some(v) = self.viewers.get_mut(&old) {
                    v.is_controlling = false;
                }
            }
        }
    }

    /// Get a viewer's state.
    pub fn get_viewer(&self, id: ViewerId) -> Option<&ViewerState> {
        self.viewers.get(&id)
    }

    pub fn viewer_count(&self) -> usize {
        self.viewers.len()
    }

    pub fn action_count(&self) -> usize {
        self.action_log.len()
    }

    /// Get actions since a given sequence.
    pub fn actions_since(&self, sequence: u64) -> Vec<&PrototypeSyncMessage> {
        self.action_log
            .iter()
            .filter(|m| m.sequence > sequence)
            .collect()
    }

    fn log_action(&mut self, msg: PrototypeSyncMessage) {
        self.action_log.push(msg);
        // Truncate if over limit
        if self.action_log.len() > self.max_log_size {
            let excess = self.action_log.len() - self.max_log_size;
            self.action_log.drain(..excess);
        }
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn alice_viewer() -> ViewerId {
        ViewerId(Uuid::from_bytes([1; 16]))
    }

    fn bob_viewer() -> ViewerId {
        ViewerId(Uuid::from_bytes([2; 16]))
    }

    fn alice_user() -> Uuid {
        Uuid::from_bytes([1; 16])
    }

    fn bob_user() -> Uuid {
        Uuid::from_bytes([2; 16])
    }

    fn doc_id() -> Uuid {
        Uuid::from_bytes([10; 16])
    }

    fn proto_id() -> Uuid {
        Uuid::from_bytes([11; 16])
    }

    fn frame1() -> Uuid {
        Uuid::from_bytes([20; 16])
    }

    fn frame2() -> Uuid {
        Uuid::from_bytes([21; 16])
    }

    #[test]
    fn test_viewer_state_creation() {
        let vs = ViewerState::new(alice_viewer(), alice_user(), "Alice", frame1(), 1000);
        assert_eq!(vs.current_frame, frame1());
        assert!(!vs.is_controlling);
    }

    #[test]
    fn test_viewer_apply_navigate() {
        let mut vs = ViewerState::new(alice_viewer(), alice_user(), "Alice", frame1(), 1000);
        vs.apply_action(
            &PrototypeAction::Navigate {
                from_frame: frame1(),
                to_frame: frame2(),
            },
            1001,
        );
        assert_eq!(vs.current_frame, frame2());
    }

    #[test]
    fn test_viewer_apply_toggle() {
        let mut vs = ViewerState::new(alice_viewer(), alice_user(), "Alice", frame1(), 1000);
        vs.apply_action(
            &PrototypeAction::ToggleState {
                target_id: Uuid::new_v4(),
                state_name: "expanded".into(),
                active: true,
            },
            1001,
        );
        assert_eq!(vs.toggle_states.get("expanded"), Some(&true));
    }

    #[test]
    fn test_viewer_apply_reset() {
        let mut vs = ViewerState::new(alice_viewer(), alice_user(), "Alice", frame1(), 1000);
        vs.apply_action(
            &PrototypeAction::ToggleState {
                target_id: Uuid::new_v4(),
                state_name: "x".into(),
                active: true,
            },
            1001,
        );
        vs.apply_action(&PrototypeAction::Reset, 1002);
        assert!(vs.toggle_states.is_empty());
        assert!(vs.variant_states.is_empty());
    }

    #[test]
    fn test_room_creation() {
        let room = PrototypeSyncRoom::new(doc_id(), proto_id(), 1000);
        assert_eq!(room.viewer_count(), 0);
        assert_eq!(room.sync_mode, SyncMode::Follow);
    }

    #[test]
    fn test_room_add_viewer_first_becomes_controller() {
        let mut room = PrototypeSyncRoom::new(doc_id(), proto_id(), 1000);
        room.add_viewer(alice_viewer(), alice_user(), "Alice", frame1(), 1000);
        assert_eq!(room.controller, Some(alice_viewer()));
        assert!(room.get_viewer(alice_viewer()).unwrap().is_controlling);
    }

    #[test]
    fn test_room_remove_viewer_hands_off_control() {
        let mut room = PrototypeSyncRoom::new(doc_id(), proto_id(), 1000);
        room.add_viewer(alice_viewer(), alice_user(), "Alice", frame1(), 1000);
        room.add_viewer(bob_viewer(), bob_user(), "Bob", frame1(), 1001);
        room.remove_viewer(alice_viewer());
        assert_eq!(room.controller, Some(bob_viewer()));
    }

    #[test]
    fn test_follow_mode_only_controller_acts() {
        let mut room = PrototypeSyncRoom::new(doc_id(), proto_id(), 1000);
        room.add_viewer(alice_viewer(), alice_user(), "Alice", frame1(), 1000);
        room.add_viewer(bob_viewer(), bob_user(), "Bob", frame1(), 1001);

        // Alice is controller
        let action = PrototypeAction::Navigate {
            from_frame: frame1(),
            to_frame: frame2(),
        };
        let msg = room.process_action(alice_viewer(), action, 1002);
        assert!(msg.is_some());
        // Both viewers should have navigated
        assert_eq!(
            room.get_viewer(alice_viewer()).unwrap().current_frame,
            frame2()
        );
        assert_eq!(
            room.get_viewer(bob_viewer()).unwrap().current_frame,
            frame2()
        );

        // Bob's action ignored
        let msg2 = room.process_action(
            bob_viewer(),
            PrototypeAction::Reset,
            1003,
        );
        assert!(msg2.is_none());
    }

    #[test]
    fn test_independent_mode() {
        let mut room = PrototypeSyncRoom::new(doc_id(), proto_id(), 1000);
        room.set_sync_mode(SyncMode::Independent);
        room.add_viewer(alice_viewer(), alice_user(), "Alice", frame1(), 1000);
        room.add_viewer(bob_viewer(), bob_user(), "Bob", frame1(), 1001);

        let msg = room.process_action(
            bob_viewer(),
            PrototypeAction::Navigate {
                from_frame: frame1(),
                to_frame: frame2(),
            },
            1002,
        );
        assert!(msg.is_some());
        // Only Bob navigated
        assert_eq!(
            room.get_viewer(bob_viewer()).unwrap().current_frame,
            frame2()
        );
        assert_eq!(
            room.get_viewer(alice_viewer()).unwrap().current_frame,
            frame1()
        );
    }

    #[test]
    fn test_set_controller() {
        let mut room = PrototypeSyncRoom::new(doc_id(), proto_id(), 1000);
        room.add_viewer(alice_viewer(), alice_user(), "Alice", frame1(), 1000);
        room.add_viewer(bob_viewer(), bob_user(), "Bob", frame1(), 1001);
        assert!(room.set_controller(bob_viewer()));
        assert_eq!(room.controller, Some(bob_viewer()));
        assert!(!room.get_viewer(alice_viewer()).unwrap().is_controlling);
        assert!(room.get_viewer(bob_viewer()).unwrap().is_controlling);
    }

    #[test]
    fn test_set_controller_invalid_viewer() {
        let mut room = PrototypeSyncRoom::new(doc_id(), proto_id(), 1000);
        assert!(!room.set_controller(ViewerId::new()));
    }

    #[test]
    fn test_action_log() {
        let mut room = PrototypeSyncRoom::new(doc_id(), proto_id(), 1000);
        room.add_viewer(alice_viewer(), alice_user(), "Alice", frame1(), 1000);
        room.process_action(
            alice_viewer(),
            PrototypeAction::Navigate {
                from_frame: frame1(),
                to_frame: frame2(),
            },
            1001,
        );
        assert_eq!(room.action_count(), 1);
    }

    #[test]
    fn test_action_log_truncation() {
        let mut room = PrototypeSyncRoom::new(doc_id(), proto_id(), 1000);
        room.max_log_size = 5;
        room.add_viewer(alice_viewer(), alice_user(), "Alice", frame1(), 1000);
        for i in 0..10 {
            room.process_action(
                alice_viewer(),
                PrototypeAction::ToggleState {
                    target_id: Uuid::new_v4(),
                    state_name: format!("s{}", i),
                    active: true,
                },
                1001 + i,
            );
        }
        assert_eq!(room.action_count(), 5);
    }

    #[test]
    fn test_actions_since() {
        let mut room = PrototypeSyncRoom::new(doc_id(), proto_id(), 1000);
        room.add_viewer(alice_viewer(), alice_user(), "Alice", frame1(), 1000);
        for _ in 0..5 {
            room.process_action(
                alice_viewer(),
                PrototypeAction::Reset,
                1001,
            );
        }
        assert_eq!(room.actions_since(3).len(), 2); // sequences 4 and 5
    }

    #[test]
    fn test_prototype_action_summary() {
        assert!(PrototypeAction::Reset.summary().contains("Reset"));
        assert!(PrototypeAction::Hover {
            target_id: Uuid::new_v4(),
            entered: true
        }
        .summary()
        .contains("enter"));
    }

    #[test]
    fn test_sync_message_serde() {
        let msg = PrototypeSyncMessage::new(
            alice_viewer(),
            PrototypeAction::Reset,
            1000,
            1,
        );
        let json = serde_json::to_string(&msg).unwrap();
        let back: PrototypeSyncMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(back.sequence, 1);
    }

    #[test]
    fn test_viewer_idle_duration() {
        let vs = ViewerState::new(alice_viewer(), alice_user(), "Alice", frame1(), 1000);
        assert_eq!(vs.idle_duration(1050), 50);
    }
}
