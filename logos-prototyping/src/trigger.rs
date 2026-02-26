//! # Interaction Triggers
//!
//! Defines the various user interactions that can fire state machine
//! transitions or standalone actions.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::animate::PropertyAnimation;
use crate::state_machine::StateId;

// ── Trigger Kind ─────────────────────────────────────────────────────

/// The type of user interaction that fires a transition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TriggerKind {
    /// Mouse / tap click.
    OnClick,
    /// Pointer enters the target bounds.
    OnHoverEnter,
    /// Pointer leaves the target bounds.
    OnHoverExit,
    /// Drag gesture on the target.
    OnDrag {
        /// Minimum drag distance in px before the trigger fires.
        threshold_px: f64,
    },
    /// Fires after a timed delay.
    OnDelay {
        /// Delay in milliseconds.
        delay_ms: u64,
    },
    /// Swipe gesture in a cardinal direction.
    OnSwipe {
        direction: SwipeDirection,
        /// Minimum swipe velocity (px/sec).
        min_velocity: f64,
    },
    /// Long press / touch-and-hold.
    OnLongPress {
        /// Duration required in ms.
        duration_ms: u64,
    },
    /// Keyboard shortcut.
    OnKeyPress {
        key: String,
    },
    /// Scroll / mouse-wheel delta.
    OnScroll,
    /// Double click / double tap.
    OnDoubleClick,
}

/// Cardinal swipe direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SwipeDirection {
    Up,
    Down,
    Left,
    Right,
}

// ── Action ───────────────────────────────────────────────────────────

/// What happens when a trigger fires.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Action {
    /// Navigate to a different artboard / frame.
    NavigateTo {
        target_id: Uuid,
        animation: Option<NavigationAnimation>,
    },
    /// Transition to a specific state in the owning state machine.
    SetState {
        state_id: StateId,
    },
    /// Toggle a drawer open/closed.
    ToggleDrawer {
        drawer_id: Uuid,
    },
    /// Set drawer to a specific state (open / closed / peeking).
    SetDrawerState {
        drawer_id: Uuid,
        state: DrawerTargetState,
    },
    /// Animate a property on a target layer.
    AnimateProperty {
        layer_id: Uuid,
        animation: PropertyAnimation,
    },
    /// Go back to the previous screen in the navigation stack.
    GoBack,
    /// Open an external URL.
    OpenUrl {
        url: String,
    },
    /// Execute multiple actions in sequence.
    Sequence(Vec<Action>),
}

/// Navigation animation preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NavigationAnimation {
    SlideLeft,
    SlideRight,
    SlideUp,
    SlideDown,
    Dissolve,
    Push,
    SmartAnimate,
    Instant,
}

/// Which drawer state to set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DrawerTargetState {
    Open,
    Closed,
    Peeking,
}

// ── Trigger (full) ───────────────────────────────────────────────────

/// A fully configured trigger: the event kind plus the resulting action.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trigger {
    pub id: Uuid,
    pub kind: TriggerKind,
    pub action: Action,
    /// Whether the trigger is active in preview mode.
    pub enabled: bool,
}

impl Trigger {
    pub fn new(kind: TriggerKind, action: Action) -> Self {
        Self {
            id: Uuid::new_v4(),
            kind,
            action,
            enabled: true,
        }
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

// ── Interaction Target ───────────────────────────────────────────────

/// Binds a specific layer to a set of triggers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InteractionTarget {
    /// The layer that responds to interactions.
    pub layer_id: Uuid,
    /// Triggers attached to this target.
    pub triggers: Vec<Trigger>,
}

impl InteractionTarget {
    pub fn new(layer_id: Uuid) -> Self {
        Self {
            layer_id,
            triggers: Vec::new(),
        }
    }

    pub fn with_trigger(mut self, trigger: Trigger) -> Self {
        self.triggers.push(trigger);
        self
    }

    pub fn add_trigger(&mut self, trigger: Trigger) {
        self.triggers.push(trigger);
    }

    pub fn remove_trigger(&mut self, id: Uuid) -> bool {
        let len = self.triggers.len();
        self.triggers.retain(|t| t.id != id);
        self.triggers.len() < len
    }

    /// Find triggers matching a given kind.
    pub fn matching_triggers(&self, kind: &TriggerKind) -> Vec<&Trigger> {
        self.triggers.iter().filter(|t| t.enabled && &t.kind == kind).collect()
    }

    /// Get the number of active (enabled) triggers.
    pub fn active_trigger_count(&self) -> usize {
        self.triggers.iter().filter(|t| t.enabled).count()
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trigger_kind_variants() {
        let kinds = vec![
            TriggerKind::OnClick,
            TriggerKind::OnHoverEnter,
            TriggerKind::OnHoverExit,
            TriggerKind::OnDrag { threshold_px: 10.0 },
            TriggerKind::OnDelay { delay_ms: 500 },
            TriggerKind::OnSwipe {
                direction: SwipeDirection::Left,
                min_velocity: 200.0,
            },
            TriggerKind::OnLongPress { duration_ms: 800 },
            TriggerKind::OnKeyPress { key: "Enter".into() },
            TriggerKind::OnScroll,
            TriggerKind::OnDoubleClick,
        ];
        assert_eq!(kinds.len(), 10);
    }

    #[test]
    fn test_action_navigate() {
        let action = Action::NavigateTo {
            target_id: Uuid::new_v4(),
            animation: Some(NavigationAnimation::SlideLeft),
        };
        match &action {
            Action::NavigateTo { animation, .. } => {
                assert_eq!(*animation, Some(NavigationAnimation::SlideLeft));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_action_sequence() {
        let action = Action::Sequence(vec![
            Action::GoBack,
            Action::ToggleDrawer { drawer_id: Uuid::new_v4() },
        ]);
        if let Action::Sequence(actions) = &action {
            assert_eq!(actions.len(), 2);
        }
    }

    #[test]
    fn test_trigger_creation() {
        let t = Trigger::new(TriggerKind::OnClick, Action::GoBack);
        assert!(t.enabled);
    }

    #[test]
    fn test_trigger_disabled() {
        let t = Trigger::new(TriggerKind::OnClick, Action::GoBack).disabled();
        assert!(!t.enabled);
    }

    #[test]
    fn test_interaction_target_with_triggers() {
        let layer = Uuid::new_v4();
        let target = InteractionTarget::new(layer)
            .with_trigger(Trigger::new(TriggerKind::OnClick, Action::GoBack))
            .with_trigger(Trigger::new(TriggerKind::OnHoverEnter, Action::GoBack));
        assert_eq!(target.triggers.len(), 2);
        assert_eq!(target.active_trigger_count(), 2);
    }

    #[test]
    fn test_interaction_target_matching() {
        let layer = Uuid::new_v4();
        let target = InteractionTarget::new(layer)
            .with_trigger(Trigger::new(TriggerKind::OnClick, Action::GoBack))
            .with_trigger(Trigger::new(TriggerKind::OnClick, Action::GoBack))
            .with_trigger(Trigger::new(TriggerKind::OnHoverEnter, Action::GoBack));
        assert_eq!(target.matching_triggers(&TriggerKind::OnClick).len(), 2);
        assert_eq!(target.matching_triggers(&TriggerKind::OnHoverEnter).len(), 1);
        assert_eq!(target.matching_triggers(&TriggerKind::OnScroll).len(), 0);
    }

    #[test]
    fn test_interaction_target_remove_trigger() {
        let layer = Uuid::new_v4();
        let mut target = InteractionTarget::new(layer);
        let t = Trigger::new(TriggerKind::OnClick, Action::GoBack);
        let tid = t.id;
        target.add_trigger(t);
        assert_eq!(target.triggers.len(), 1);
        assert!(target.remove_trigger(tid));
        assert!(target.triggers.is_empty());
    }

    #[test]
    fn test_disabled_trigger_not_matched() {
        let layer = Uuid::new_v4();
        let target = InteractionTarget::new(layer)
            .with_trigger(Trigger::new(TriggerKind::OnClick, Action::GoBack).disabled());
        assert_eq!(target.matching_triggers(&TriggerKind::OnClick).len(), 0);
        assert_eq!(target.active_trigger_count(), 0);
    }

    #[test]
    fn test_swipe_directions() {
        for dir in [SwipeDirection::Up, SwipeDirection::Down, SwipeDirection::Left, SwipeDirection::Right] {
            let kind = TriggerKind::OnSwipe {
                direction: dir,
                min_velocity: 100.0,
            };
            if let TriggerKind::OnSwipe { direction, .. } = kind {
                assert_eq!(direction, dir);
            }
        }
    }

    #[test]
    fn test_drawer_target_states() {
        let states = [DrawerTargetState::Open, DrawerTargetState::Closed, DrawerTargetState::Peeking];
        assert_eq!(states.len(), 3);
    }

    #[test]
    fn test_navigation_animations() {
        let anims = [
            NavigationAnimation::SlideLeft,
            NavigationAnimation::SlideRight,
            NavigationAnimation::SlideUp,
            NavigationAnimation::SlideDown,
            NavigationAnimation::Dissolve,
            NavigationAnimation::Push,
            NavigationAnimation::SmartAnimate,
            NavigationAnimation::Instant,
        ];
        assert_eq!(anims.len(), 8);
    }

    #[test]
    fn test_serde_roundtrip_trigger_kind() {
        let kind = TriggerKind::OnSwipe {
            direction: SwipeDirection::Right,
            min_velocity: 300.0,
        };
        let json = serde_json::to_string(&kind).unwrap();
        let back: TriggerKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, kind);
    }

    #[test]
    fn test_serde_roundtrip_action() {
        let action = Action::SetDrawerState {
            drawer_id: Uuid::new_v4(),
            state: DrawerTargetState::Peeking,
        };
        let json = serde_json::to_string(&action).unwrap();
        let back: Action = serde_json::from_str(&json).unwrap();
        assert_eq!(back, action);
    }

    #[test]
    fn test_serde_roundtrip_interaction_target() {
        let layer = Uuid::new_v4();
        let target = InteractionTarget::new(layer)
            .with_trigger(Trigger::new(TriggerKind::OnClick, Action::GoBack));
        let json = serde_json::to_string(&target).unwrap();
        let back: InteractionTarget = serde_json::from_str(&json).unwrap();
        assert_eq!(back.layer_id, layer);
        assert_eq!(back.triggers.len(), 1);
    }

    #[test]
    fn test_open_url_action() {
        let action = Action::OpenUrl { url: "https://example.com".into() };
        if let Action::OpenUrl { url } = &action {
            assert_eq!(url, "https://example.com");
        } else {
            panic!("wrong variant");
        }
    }
}
