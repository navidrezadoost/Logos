//! # State Machine
//!
//! Each container (artboard, frame, drawer) can own a [`StateMachine`] that
//! tracks discrete visual states and the transitions between them.
//!
//! A [`State`] captures a set of property overrides that are applied when
//! the state is active. [`Transition`]s define how and when the machine
//! moves from one state to another.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::animate::PropertyAnimation;
use crate::trigger::TriggerKind;

// ── Identifiers ──────────────────────────────────────────────────────

/// Strongly-typed wrapper around a UUID identifying a state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StateId(pub Uuid);

impl StateId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for StateId {
    fn default() -> Self {
        Self::new()
    }
}

/// Strongly-typed wrapper around a UUID identifying a transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TransitionId(pub Uuid);

impl TransitionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for TransitionId {
    fn default() -> Self {
        Self::new()
    }
}

// ── Property Override ────────────────────────────────────────────────

/// A single property override applied when a state is active.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PropertyOverride {
    /// The target layer UUID inside the container.
    pub layer_id: Uuid,
    /// Dot-path of the property being overridden, e.g. `"fill.color"`.
    pub property: String,
    /// Serialised JSON value for the override.
    pub value: serde_json::Value,
}

// ── State ────────────────────────────────────────────────────────────

/// A discrete visual state within a state machine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct State {
    pub id: StateId,
    pub name: String,
    /// Property overrides that distinguish this state from the base design.
    pub overrides: Vec<PropertyOverride>,
    /// Whether this is the default (initial) state.
    pub is_default: bool,
}

impl State {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: StateId::new(),
            name: name.into(),
            overrides: Vec::new(),
            is_default: false,
        }
    }

    /// Create a state that is flagged as the default.
    pub fn new_default(name: impl Into<String>) -> Self {
        Self {
            id: StateId::new(),
            name: name.into(),
            overrides: Vec::new(),
            is_default: true,
        }
    }

    /// Add a property override.
    pub fn with_override(mut self, layer_id: Uuid, property: impl Into<String>, value: serde_json::Value) -> Self {
        self.overrides.push(PropertyOverride {
            layer_id,
            property: property.into(),
            value,
        });
        self
    }
}

// ── Transition ───────────────────────────────────────────────────────

/// Describes a transition between two states.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transition {
    pub id: TransitionId,
    /// Source state.
    pub from: StateId,
    /// Destination state.
    pub to: StateId,
    /// What triggers this transition.
    pub trigger: TriggerKind,
    /// Optional animation applied during the transition.
    pub animation: Option<PropertyAnimation>,
    /// Whether Smart Animate should auto-interpolate matching layers.
    pub smart_animate: bool,
}

impl Transition {
    pub fn new(from: StateId, to: StateId, trigger: TriggerKind) -> Self {
        Self {
            id: TransitionId::new(),
            from,
            to,
            trigger,
            animation: None,
            smart_animate: false,
        }
    }

    pub fn with_animation(mut self, animation: PropertyAnimation) -> Self {
        self.animation = Some(animation);
        self
    }

    pub fn with_smart_animate(mut self, enabled: bool) -> Self {
        self.smart_animate = enabled;
        self
    }
}

// ── State Machine ────────────────────────────────────────────────────

/// A finite state machine attached to a container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateMachine {
    /// The container (layer) UUID this machine belongs to.
    pub owner_id: Uuid,
    /// All registered states keyed by [`StateId`].
    pub states: HashMap<StateId, State>,
    /// All transitions keyed by [`TransitionId`].
    pub transitions: HashMap<TransitionId, Transition>,
    /// The currently active state (runtime-only, not persisted).
    #[serde(skip)]
    current_state: Option<StateId>,
}

impl StateMachine {
    /// Create a new, empty state machine for the given container.
    pub fn new(owner_id: Uuid) -> Self {
        Self {
            owner_id,
            states: HashMap::new(),
            transitions: HashMap::new(),
            current_state: None,
        }
    }

    // ── State management ─────────────────────────────────────────

    /// Add a state. If it's the default, it becomes the current state.
    pub fn add_state(&mut self, state: State) -> StateId {
        let id = state.id;
        if state.is_default {
            self.current_state = Some(id);
        }
        self.states.insert(id, state);
        id
    }

    /// Remove a state and all transitions referencing it.
    pub fn remove_state(&mut self, id: StateId) -> Option<State> {
        self.transitions.retain(|_, t| t.from != id && t.to != id);
        if self.current_state == Some(id) {
            self.current_state = None;
        }
        self.states.remove(&id)
    }

    pub fn get_state(&self, id: StateId) -> Option<&State> {
        self.states.get(&id)
    }

    pub fn state_count(&self) -> usize {
        self.states.len()
    }

    /// Return the default state, if any.
    pub fn default_state(&self) -> Option<&State> {
        self.states.values().find(|s| s.is_default)
    }

    // ── Transition management ────────────────────────────────────

    /// Add a transition. Returns its ID.
    pub fn add_transition(&mut self, transition: Transition) -> TransitionId {
        let id = transition.id;
        self.transitions.insert(id, transition);
        id
    }

    /// Remove a transition by ID.
    pub fn remove_transition(&mut self, id: TransitionId) -> Option<Transition> {
        self.transitions.remove(&id)
    }

    /// Find all transitions originating from the given state.
    pub fn transitions_from(&self, state: StateId) -> Vec<&Transition> {
        self.transitions.values().filter(|t| t.from == state).collect()
    }

    /// Find all transitions arriving at the given state.
    pub fn transitions_to(&self, state: StateId) -> Vec<&Transition> {
        self.transitions.values().filter(|t| t.to == state).collect()
    }

    /// Find transitions from `state` that match a given trigger kind.
    pub fn matching_transitions(&self, state: StateId, trigger: &TriggerKind) -> Vec<&Transition> {
        self.transitions
            .values()
            .filter(|t| t.from == state && &t.trigger == trigger)
            .collect()
    }

    // ── Runtime ──────────────────────────────────────────────────

    /// Get the currently active state id.
    pub fn current_state(&self) -> Option<StateId> {
        self.current_state
    }

    /// Reset the machine to the default state.
    pub fn reset(&mut self) {
        self.current_state = self.states.values().find(|s| s.is_default).map(|s| s.id);
    }

    /// Attempt to fire a trigger. If a matching transition exists from the
    /// current state, advance to the target state and return the transition.
    pub fn fire(&mut self, trigger: &TriggerKind) -> Option<&Transition> {
        let current = self.current_state?;
        let tid = self
            .transitions
            .values()
            .find(|t| t.from == current && &t.trigger == trigger)
            .map(|t| t.id)?;

        let transition = self.transitions.get(&tid)?;
        self.current_state = Some(transition.to);

        // Return the transition (need to re-borrow to satisfy borrow checker)
        self.transitions.get(&tid)
    }

    /// Force-set the current state (no transition fired).
    pub fn set_current_state(&mut self, id: StateId) -> bool {
        if self.states.contains_key(&id) {
            self.current_state = Some(id);
            true
        } else {
            false
        }
    }

    /// Get the property overrides for the current state.
    pub fn current_overrides(&self) -> Vec<&PropertyOverride> {
        self.current_state
            .and_then(|id| self.states.get(&id))
            .map(|s| s.overrides.iter().collect())
            .unwrap_or_default()
    }

    /// Validate the machine: ensure all transitions reference valid states.
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        for (tid, t) in &self.transitions {
            if !self.states.contains_key(&t.from) {
                errors.push(format!("Transition {:?} references unknown source state {:?}", tid, t.from));
            }
            if !self.states.contains_key(&t.to) {
                errors.push(format!("Transition {:?} references unknown target state {:?}", tid, t.to));
            }
        }
        if self.states.values().filter(|s| s.is_default).count() > 1 {
            errors.push("Multiple default states defined".into());
        }
        errors
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trigger::TriggerKind;

    fn make_machine() -> StateMachine {
        let owner = Uuid::new_v4();
        let mut sm = StateMachine::new(owner);

        let s1 = State::new_default("Idle");
        let s2 = State::new("Hovered");
        let s3 = State::new("Pressed");

        let id1 = sm.add_state(s1);
        let id2 = sm.add_state(s2);
        let id3 = sm.add_state(s3);

        sm.add_transition(Transition::new(id1, id2, TriggerKind::OnHoverEnter));
        sm.add_transition(Transition::new(id2, id1, TriggerKind::OnHoverExit));
        sm.add_transition(Transition::new(id2, id3, TriggerKind::OnClick));
        sm.add_transition(Transition::new(id3, id1, TriggerKind::OnClick));

        sm
    }

    #[test]
    fn test_state_machine_creation() {
        let sm = make_machine();
        assert_eq!(sm.state_count(), 3);
        assert!(sm.default_state().is_some());
        assert_eq!(sm.default_state().unwrap().name, "Idle");
    }

    #[test]
    fn test_current_state_starts_at_default() {
        let sm = make_machine();
        let default_id = sm.default_state().unwrap().id;
        assert_eq!(sm.current_state(), Some(default_id));
    }

    #[test]
    fn test_fire_hover_transition() {
        let mut sm = make_machine();
        let t = sm.fire(&TriggerKind::OnHoverEnter);
        assert!(t.is_some());
        let current = sm.current_state().unwrap();
        let state = sm.get_state(current).unwrap();
        assert_eq!(state.name, "Hovered");
    }

    #[test]
    fn test_fire_click_from_hovered() {
        let mut sm = make_machine();
        sm.fire(&TriggerKind::OnHoverEnter);
        sm.fire(&TriggerKind::OnClick);
        let current = sm.current_state().unwrap();
        let state = sm.get_state(current).unwrap();
        assert_eq!(state.name, "Pressed");
    }

    #[test]
    fn test_fire_nonmatching_trigger() {
        let mut sm = make_machine();
        // Idle has no OnClick transition
        let t = sm.fire(&TriggerKind::OnClick);
        assert!(t.is_none());
        // Still at idle
        let state = sm.get_state(sm.current_state().unwrap()).unwrap();
        assert_eq!(state.name, "Idle");
    }

    #[test]
    fn test_reset() {
        let mut sm = make_machine();
        sm.fire(&TriggerKind::OnHoverEnter);
        sm.reset();
        let state = sm.get_state(sm.current_state().unwrap()).unwrap();
        assert_eq!(state.name, "Idle");
    }

    #[test]
    fn test_remove_state_cleans_transitions() {
        let mut sm = make_machine();
        let hovered_id = sm
            .states
            .values()
            .find(|s| s.name == "Hovered")
            .unwrap()
            .id;
        let count_before = sm.transitions.len();
        sm.remove_state(hovered_id);
        // Transitions referencing Hovered should be gone
        assert!(sm.transitions.len() < count_before);
        assert!(sm.transitions_from(hovered_id).is_empty());
        assert!(sm.transitions_to(hovered_id).is_empty());
    }

    #[test]
    fn test_set_current_state_valid() {
        let mut sm = make_machine();
        let pressed_id = sm.states.values().find(|s| s.name == "Pressed").unwrap().id;
        assert!(sm.set_current_state(pressed_id));
        assert_eq!(sm.current_state(), Some(pressed_id));
    }

    #[test]
    fn test_set_current_state_invalid() {
        let mut sm = make_machine();
        let fake = StateId(Uuid::new_v4());
        assert!(!sm.set_current_state(fake));
    }

    #[test]
    fn test_validate_ok() {
        let sm = make_machine();
        assert!(sm.validate().is_empty());
    }

    #[test]
    fn test_validate_dangling_transition() {
        let mut sm = make_machine();
        let bogus_from = StateId(Uuid::new_v4());
        let bogus_to = StateId(Uuid::new_v4());
        sm.add_transition(Transition::new(bogus_from, bogus_to, TriggerKind::OnClick));
        let errors = sm.validate();
        assert_eq!(errors.len(), 2); // both from and to are unknown
    }

    #[test]
    fn test_property_override_in_state() {
        let layer = Uuid::new_v4();
        let state = State::new("Active")
            .with_override(layer, "fill.color", serde_json::json!("#FF0000"))
            .with_override(layer, "opacity", serde_json::json!(0.5));
        assert_eq!(state.overrides.len(), 2);
        assert_eq!(state.overrides[0].property, "fill.color");
    }

    #[test]
    fn test_current_overrides() {
        let owner = Uuid::new_v4();
        let layer = Uuid::new_v4();
        let mut sm = StateMachine::new(owner);
        let s = State::new_default("Active")
            .with_override(layer, "opacity", serde_json::json!(0.5));
        sm.add_state(s);
        let overrides = sm.current_overrides();
        assert_eq!(overrides.len(), 1);
        assert_eq!(overrides[0].property, "opacity");
    }

    #[test]
    fn test_transitions_from() {
        let sm = make_machine();
        let idle_id = sm.default_state().unwrap().id;
        let from_idle = sm.transitions_from(idle_id);
        assert_eq!(from_idle.len(), 1); // Idle -> Hovered
    }

    #[test]
    fn test_matching_transitions() {
        let sm = make_machine();
        let hovered_id = sm.states.values().find(|s| s.name == "Hovered").unwrap().id;
        let clicks = sm.matching_transitions(hovered_id, &TriggerKind::OnClick);
        assert_eq!(clicks.len(), 1);
    }

    #[test]
    fn test_transition_with_smart_animate() {
        let from = StateId::new();
        let to = StateId::new();
        let t = Transition::new(from, to, TriggerKind::OnClick).with_smart_animate(true);
        assert!(t.smart_animate);
    }

    #[test]
    fn test_serde_roundtrip_state() {
        let layer = Uuid::new_v4();
        let state = State::new("Test")
            .with_override(layer, "x", serde_json::json!(100.0));
        let json = serde_json::to_string(&state).unwrap();
        let back: State = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "Test");
        assert_eq!(back.overrides.len(), 1);
    }

    #[test]
    fn test_serde_roundtrip_machine() {
        let sm = make_machine();
        let json = serde_json::to_string(&sm).unwrap();
        let back: StateMachine = serde_json::from_str(&json).unwrap();
        assert_eq!(back.state_count(), 3);
        assert_eq!(back.transitions.len(), 4);
        // current_state is skipped in serde
        assert!(back.current_state().is_none());
    }

    #[test]
    fn test_state_id_default() {
        let a = StateId::default();
        let b = StateId::default();
        assert_ne!(a, b); // UUIDs differ
    }

    #[test]
    fn test_empty_machine_fire() {
        let mut sm = StateMachine::new(Uuid::new_v4());
        assert!(sm.fire(&TriggerKind::OnClick).is_none());
    }
}
