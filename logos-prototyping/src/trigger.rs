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
    /// Mouse / pointer button pressed (not yet released).
    MouseDown,
    /// Mouse / pointer button released.
    MouseUp,
    /// Time-based trigger that fires after the layer is visible for `delay_ms`.
    ///
    /// Distinct from `OnDelay` to allow both an initial delay and a repeat delay
    /// on the same interaction target.
    AfterDelay {
        /// Milliseconds to wait before firing.
        delay_ms: u64,
    },
    /// Gamepad / controller button press.
    Gamepad {
        /// Platform-agnostic button label (e.g. `"A"`, `"B"`, `"DPad_Up"`).
        button: String,
    },

    // ── Focus / Keyboard ──────────────────────────────────────────────

    /// An element gains keyboard / programmatic focus.
    OnFocus,
    /// An element loses focus (blur).
    OnBlur,
    /// Right-click / long-press context menu.
    OnContextMenu,
    /// A key is released (complements OnKeyPress).
    OnKeyRelease {
        key: String,
    },
    /// A key is held without being released.
    OnKeyHold {
        key: String,
        /// How long the key must be held before firing (ms).
        hold_ms: u64,
    },

    // ── Pointer ──────────────────────────────────────────────────────

    /// Raw pointer enters bounds (fires before OnHoverEnter bubbling).
    OnPointerEnter,
    /// Raw pointer leaves bounds.
    OnPointerLeave,
    /// Pointer moves over the element.
    OnPointerMove,
    /// Pinch-zoom gesture.
    OnPinch {
        /// Scale factor threshold that fires the trigger (e.g. `1.5` = 150 %).
        scale_threshold: f64,
    },
    /// Rotate gesture (two-finger rotation).
    OnRotate {
        /// Minimum rotation in degrees.
        angle_deg: f64,
    },

    // ── Visibility / Lifecycle ────────────────────────────────────────

    /// The element enters or leaves the viewport (intersection observer).
    OnIntersection {
        /// Fraction of the element that must be visible [0.0, 1.0].
        threshold: f64,
        /// `true` = fires when entering, `false` = fires when leaving.
        entering: bool,
    },
    /// The browser tab / window changes visibility.
    OnVisibilityChange {
        visible: bool,
    },
    /// The browser window or canvas is resized.
    OnWindowResize,
    /// The canvas / page is scrolled.
    OnWindowScroll,
    /// The layer is first mounted / added to the canvas.
    OnMount,
    /// The layer is removed from the canvas.
    OnUnmount,

    // ── Media events ─────────────────────────────────────────────────

    /// An audio/video layer starts playing.
    OnMediaPlay,
    /// An audio/video layer is paused.
    OnMediaPause,
    /// An audio/video layer finishes playing.
    OnMediaEnded,
    /// An audio/video layer has finished loading and is ready.
    OnMediaLoad,
    /// An audio/video layer encounters a load / playback error.
    OnMediaError,
    /// Current playback time passes a marker.
    OnMediaTimeUpdate {
        /// Fire when playback position reaches or passes this time (seconds).
        at_seconds: f64,
    },

    // ── Form / Input events ───────────────────────────────────────────

    /// A form is submitted.
    OnFormSubmit,
    /// A form is reset.
    OnFormReset,
    /// An input value changes.
    OnInputChange,
    /// An input value is validated (custom validation scenario).
    OnInputValid,
    /// An input fails validation.
    OnInputInvalid,

    // ── Network / Data ────────────────────────────────────────────────

    /// A data-fetch completes successfully.
    OnDataLoaded {
        /// Logical data source identifier.
        source_id: String,
    },
    /// A data-fetch fails.
    OnDataError {
        source_id: String,
    },

    // ── Custom ────────────────────────────────────────────────────────

    /// A custom named event dispatched via `EmitCustomEvent`.
    OnCustomEvent {
        /// Event name (must match what `EmitCustomEvent` dispatches).
        name: String,
    },
}

/// Cardinal swipe direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SwipeDirection {
    Up,
    Down,
    Left,
    Right,
}

// ── Trigger Condition / Guard ─────────────────────────────────────────

/// A boolean guard that must pass before an action fires.
///
/// Conditions are evaluated at trigger time; if the condition returns `false`
/// the action is skipped (the trigger is still considered "matched").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TriggerCondition {
    /// Always passes (no guard).
    Always,
    /// Passes when the named boolean variable equals `expected`.
    VariableEquals {
        variable: String,
        expected: serde_json::Value,
    },
    /// Passes when the named boolean variable is truthy.
    VariableTruthy { variable: String },
    /// Passes when the screen/artboard width matches the breakpoint.
    Breakpoint {
        /// Minimum width in px (inclusive).
        min_width: Option<u32>,
        /// Maximum width in px (inclusive).
        max_width: Option<u32>,
    },
    /// Passes when the OS prefers reduced motion.
    PrefersReducedMotion,
    /// Passes when the OS is in dark mode.
    PrefersDarkMode,
    /// Logical NOT of the inner condition.
    Not(Box<TriggerCondition>),
    /// All inner conditions must pass.
    All(Vec<TriggerCondition>),
    /// At least one inner condition must pass.
    Any(Vec<TriggerCondition>),
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
    /// Show an overlay (modal, tooltip, dropdown, etc.).
    ShowOverlay {
        overlay_config: crate::overlay::OverlayConfig,
    },
    /// Dismiss an active overlay by its content id.
    DismissOverlay {
        content_id: Uuid,
    },
    /// Dismiss the topmost overlay.
    DismissTopOverlay,
    /// Execute multiple actions in sequence.
    Sequence(Vec<Action>),

    // ── Sound ─────────────────────────────────────────────────────────

    /// Play a sound asset.
    PlaySound {
        /// Asset path or URL.
        src: String,
        /// Volume [0.0, 1.0]. Defaults to `1.0`.
        volume: f64,
        /// Whether to loop the sound indefinitely.
        looping: bool,
    },
    /// Pause a currently playing sound.
    PauseSound {
        src: String,
    },
    /// Stop (and reset position of) a sound.
    StopSound {
        src: String,
    },
    /// Smoothly change playback volume of a sound.
    SetVolume {
        src: String,
        /// Target volume [0.0, 1.0].
        volume: f64,
        /// Ramp duration in ms.
        ramp_ms: u64,
    },

    // ── Animation ─────────────────────────────────────────────────────

    /// Start a named animation on a layer.
    StartAnimation {
        layer_id: Uuid,
        animation_name: String,
    },
    /// Stop a named animation.
    StopAnimation {
        layer_id: Uuid,
        animation_name: String,
    },
    /// Pause a named animation.
    PauseAnimation {
        layer_id: Uuid,
        animation_name: String,
    },
    /// Resume a paused animation.
    ResumeAnimation {
        layer_id: Uuid,
        animation_name: String,
    },
    /// Seek a named animation to a specific time.
    SeekAnimation {
        layer_id: Uuid,
        animation_name: String,
        /// Position in milliseconds.
        time_ms: u64,
    },

    // ── Layer / Style ─────────────────────────────────────────────────

    /// Show or hide a layer.
    SetVisibility {
        layer_id: Uuid,
        visible: bool,
    },
    /// Animate a layer's opacity.
    SetOpacity {
        layer_id: Uuid,
        /// Target opacity [0.0, 1.0].
        opacity: f64,
        /// Transition duration ms.
        duration_ms: u64,
    },
    /// Add a CSS class to the exported HTML element for this layer.
    AddCssClass {
        layer_id: Uuid,
        class_name: String,
    },
    /// Remove a CSS class.
    RemoveCssClass {
        layer_id: Uuid,
        class_name: String,
    },
    /// Toggle a CSS class on/off.
    ToggleCssClass {
        layer_id: Uuid,
        class_name: String,
    },
    /// Apply an inline style property to a layer (exported as CSS variable).
    SetStyleProperty {
        layer_id: Uuid,
        property: String,
        value: String,
        /// Transition duration ms (0 = instant).
        transition_ms: u64,
    },

    // ── Navigation / View ─────────────────────────────────────────────

    /// Smooth-scroll the canvas to a target layer.
    ScrollTo {
        layer_id: Uuid,
        behavior: ScrollBehavior,
    },
    /// Bring a layer into focus programmatically.
    SetFocus {
        layer_id: Uuid,
    },

    // ── Variable ─────────────────────────────────────────────────────

    /// Create or update a named runtime variable.
    UpdateVariable {
        name: String,
        value: serde_json::Value,
    },
    /// Increment a numeric variable by `delta`.
    IncrementVariable {
        name: String,
        delta: f64,
    },
    /// Toggle a boolean variable.
    ToggleVariable {
        name: String,
    },

    // ── Media ─────────────────────────────────────────────────────────

    /// Start playback on a media layer.
    PlayMedia { layer_id: Uuid },
    /// Pause media playback.
    PauseMedia { layer_id: Uuid },
    /// Stop media and reset to beginning.
    StopMedia { layer_id: Uuid },
    /// Seek a media layer to a position.
    SeekMedia {
        layer_id: Uuid,
        time_seconds: f64,
    },
    /// Mute or unmute a media layer.
    SetMute {
        layer_id: Uuid,
        muted: bool,
    },

    // ── Communication ────────────────────────────────────────────────

    /// Dispatch a custom named event (pairs with OnCustomEvent trigger).
    EmitCustomEvent {
        name: String,
        payload: serde_json::Value,
    },
    /// Copy a string to the system clipboard.
    CopyToClipboard {
        text: String,
    },
    /// Trigger device haptic feedback.
    Vibrate {
        /// Pattern in ms: [on, off, on, …].
        pattern_ms: Vec<u64>,
    },
    /// Send an analytics event.
    TrackEvent {
        category: String,
        action: String,
        label: Option<String>,
        value: Option<f64>,
    },
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

/// Scroll animation behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScrollBehavior {
    /// Instantly jump to the target.
    Instant,
    /// Smooth CSS scroll.
    Smooth,
    /// Follow easing curve defined in the layer's animation settings.
    Auto,
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
    /// Optional guard condition evaluated before the action fires.
    pub condition: TriggerCondition,
}

impl Trigger {
    pub fn new(kind: TriggerKind, action: Action) -> Self {
        Self {
            id: Uuid::new_v4(),
            kind,
            action,
            enabled: true,
            condition: TriggerCondition::Always,
        }
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    /// Attach a guard condition to this trigger.
    pub fn with_condition(mut self, condition: TriggerCondition) -> Self {
        self.condition = condition;
        self
    }

    /// Returns `true` if the trigger should fire for the given kind.
    pub fn matches(&self, kind: &TriggerKind) -> bool {
        self.enabled && &self.kind == kind
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

    // ─── New trigger variants (Phase 1) ─────────────────────────────

    // TRIG-01: MouseDown trigger can be created and matched.
    #[test]
    fn trig_01_mouse_down_creation() {
        let t = Trigger::new(TriggerKind::MouseDown, Action::GoBack);
        assert_eq!(t.kind, TriggerKind::MouseDown);
        assert!(t.enabled);
    }

    // TRIG-02: MouseUp trigger can be created and matched.
    #[test]
    fn trig_02_mouse_up_creation() {
        let t = Trigger::new(TriggerKind::MouseUp, Action::GoBack);
        assert_eq!(t.kind, TriggerKind::MouseUp);
        assert!(t.enabled);
    }

    // TRIG-03: AfterDelay stores delay_ms correctly.
    #[test]
    fn trig_03_after_delay_stores_delay() {
        let t = Trigger::new(TriggerKind::AfterDelay { delay_ms: 2000 }, Action::GoBack);
        if let TriggerKind::AfterDelay { delay_ms } = t.kind {
            assert_eq!(delay_ms, 2000);
        } else {
            panic!("wrong variant");
        }
    }

    // TRIG-04: Gamepad stores button label correctly.
    #[test]
    fn trig_04_gamepad_stores_button() {
        let t = Trigger::new(TriggerKind::Gamepad { button: "A".into() }, Action::GoBack);
        if let TriggerKind::Gamepad { button } = &t.kind {
            assert_eq!(button, "A");
        } else {
            panic!("wrong variant");
        }
    }

    // TRIG-05: MouseDown can be matched on an InteractionTarget.
    #[test]
    fn trig_05_mouse_down_matching() {
        let layer = Uuid::new_v4();
        let target = InteractionTarget::new(layer)
            .with_trigger(Trigger::new(TriggerKind::MouseDown, Action::GoBack));
        assert_eq!(target.matching_triggers(&TriggerKind::MouseDown).len(), 1);
        assert_eq!(target.matching_triggers(&TriggerKind::MouseUp).len(), 0);
    }

    // TRIG-06: MouseUp can be matched on an InteractionTarget.
    #[test]
    fn trig_06_mouse_up_matching() {
        let layer = Uuid::new_v4();
        let target = InteractionTarget::new(layer)
            .with_trigger(Trigger::new(TriggerKind::MouseUp, Action::GoBack));
        assert_eq!(target.matching_triggers(&TriggerKind::MouseUp).len(), 1);
    }

    // TRIG-07: AfterDelay with different delay_ms values are distinct.
    #[test]
    fn trig_07_after_delay_distinct_values() {
        let k1 = TriggerKind::AfterDelay { delay_ms: 500 };
        let k2 = TriggerKind::AfterDelay { delay_ms: 1000 };
        assert_ne!(k1, k2);
    }

    // TRIG-08: Gamepad with different buttons are distinct.
    #[test]
    fn trig_08_gamepad_distinct_buttons() {
        let ka = TriggerKind::Gamepad { button: "A".into() };
        let kb = TriggerKind::Gamepad { button: "B".into() };
        assert_ne!(ka, kb);
    }

    // TRIG-09: MouseDown and MouseUp are distinct kinds.
    #[test]
    fn trig_09_mouse_down_up_distinct() {
        assert_ne!(TriggerKind::MouseDown, TriggerKind::MouseUp);
    }

    // TRIG-10: AfterDelay serde roundtrip.
    #[test]
    fn trig_10_after_delay_serde_roundtrip() {
        let k = TriggerKind::AfterDelay { delay_ms: 750 };
        let json = serde_json::to_string(&k).unwrap();
        let back: TriggerKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, k);
    }

    // TRIG-11: Gamepad serde roundtrip.
    #[test]
    fn trig_11_gamepad_serde_roundtrip() {
        let k = TriggerKind::Gamepad { button: "DPad_Up".into() };
        let json = serde_json::to_string(&k).unwrap();
        let back: TriggerKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, k);
    }

    // TRIG-12: MouseDown serde roundtrip.
    #[test]
    fn trig_12_mouse_down_serde_roundtrip() {
        let k = TriggerKind::MouseDown;
        let json = serde_json::to_string(&k).unwrap();
        let back: TriggerKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, k);
    }

    // TRIG-13: Disabled AfterDelay is not matched.
    #[test]
    fn trig_13_disabled_after_delay_not_matched() {
        let layer = Uuid::new_v4();
        let target = InteractionTarget::new(layer)
            .with_trigger(
                Trigger::new(TriggerKind::AfterDelay { delay_ms: 500 }, Action::GoBack)
                    .disabled(),
            );
        assert_eq!(
            target.matching_triggers(&TriggerKind::AfterDelay { delay_ms: 500 }).len(),
            0
        );
    }

    // TRIG-14: All four new triggers can coexist on one InteractionTarget.
    #[test]
    fn trig_14_all_new_triggers_on_target() {
        let layer = Uuid::new_v4();
        let target = InteractionTarget::new(layer)
            .with_trigger(Trigger::new(TriggerKind::MouseDown, Action::GoBack))
            .with_trigger(Trigger::new(TriggerKind::MouseUp, Action::GoBack))
            .with_trigger(Trigger::new(TriggerKind::AfterDelay { delay_ms: 300 }, Action::GoBack))
            .with_trigger(Trigger::new(TriggerKind::Gamepad { button: "X".into() }, Action::GoBack));
        assert_eq!(target.active_trigger_count(), 4);
    }

    // TRIG-15: Trigger kind list includes all 14 variants.
    #[test]
    fn trig_15_total_trigger_kind_count() {
        let kinds: Vec<TriggerKind> = vec![
            TriggerKind::OnClick,
            TriggerKind::OnHoverEnter,
            TriggerKind::OnHoverExit,
            TriggerKind::OnDrag { threshold_px: 10.0 },
            TriggerKind::OnDelay { delay_ms: 500 },
            TriggerKind::OnSwipe { direction: SwipeDirection::Left, min_velocity: 100.0 },
            TriggerKind::OnLongPress { duration_ms: 800 },
            TriggerKind::OnKeyPress { key: "Space".into() },
            TriggerKind::OnScroll,
            TriggerKind::OnDoubleClick,
            TriggerKind::MouseDown,
            TriggerKind::MouseUp,
            TriggerKind::AfterDelay { delay_ms: 1000 },
            TriggerKind::Gamepad { button: "Start".into() },
        ];
        assert_eq!(kinds.len(), 14);
    }

    // ── Focus / Keyboard ─────────────────────────────────────────────

    // TRIG-16: OnFocus trigger creation.
    #[test]
    fn trig_16_on_focus_creation() {
        let t = Trigger::new(TriggerKind::OnFocus, Action::GoBack);
        assert_eq!(t.kind, TriggerKind::OnFocus);
        assert!(t.enabled);
    }

    // TRIG-17: OnBlur trigger creation.
    #[test]
    fn trig_17_on_blur_creation() {
        let t = Trigger::new(TriggerKind::OnBlur, Action::GoBack);
        assert_eq!(t.kind, TriggerKind::OnBlur);
    }

    // TRIG-18: OnContextMenu trigger creation.
    #[test]
    fn trig_18_on_context_menu_creation() {
        let t = Trigger::new(TriggerKind::OnContextMenu, Action::GoBack);
        assert_eq!(t.kind, TriggerKind::OnContextMenu);
    }

    // TRIG-19: OnKeyRelease stores key correctly.
    #[test]
    fn trig_19_key_release_stores_key() {
        let t = Trigger::new(TriggerKind::OnKeyRelease { key: "Enter".into() }, Action::GoBack);
        if let TriggerKind::OnKeyRelease { key } = &t.kind {
            assert_eq!(key, "Enter");
        } else {
            panic!("wrong variant");
        }
    }

    // TRIG-20: OnKeyHold stores key and hold_ms.
    #[test]
    fn trig_20_key_hold_stores_fields() {
        let t = Trigger::new(
            TriggerKind::OnKeyHold { key: "Shift".into(), hold_ms: 1000 },
            Action::GoBack,
        );
        if let TriggerKind::OnKeyHold { key, hold_ms } = &t.kind {
            assert_eq!(key, "Shift");
            assert_eq!(*hold_ms, 1000);
        } else {
            panic!("wrong variant");
        }
    }

    // TRIG-21: OnKeyRelease and OnKeyPress are distinct.
    #[test]
    fn trig_21_key_release_distinct_from_press() {
        let press = TriggerKind::OnKeyPress { key: "A".into() };
        let release = TriggerKind::OnKeyRelease { key: "A".into() };
        assert_ne!(press, release);
    }

    // TRIG-22: OnKeyRelease serde roundtrip.
    #[test]
    fn trig_22_key_release_serde_roundtrip() {
        let k = TriggerKind::OnKeyRelease { key: "Escape".into() };
        let json = serde_json::to_string(&k).unwrap();
        let back: TriggerKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, k);
    }

    // ── Pointer ──────────────────────────────────────────────────────

    // TRIG-23: OnPointerEnter creation.
    #[test]
    fn trig_23_pointer_enter_creation() {
        let t = Trigger::new(TriggerKind::OnPointerEnter, Action::GoBack);
        assert_eq!(t.kind, TriggerKind::OnPointerEnter);
    }

    // TRIG-24: OnPointerLeave creation.
    #[test]
    fn trig_24_pointer_leave_creation() {
        let t = Trigger::new(TriggerKind::OnPointerLeave, Action::GoBack);
        assert_eq!(t.kind, TriggerKind::OnPointerLeave);
    }

    // TRIG-25: OnPointerMove creation.
    #[test]
    fn trig_25_pointer_move_creation() {
        let t = Trigger::new(TriggerKind::OnPointerMove, Action::GoBack);
        assert_eq!(t.kind, TriggerKind::OnPointerMove);
    }

    // TRIG-26: OnPinch stores scale_threshold.
    #[test]
    fn trig_26_pinch_stores_threshold() {
        let t = Trigger::new(TriggerKind::OnPinch { scale_threshold: 1.5 }, Action::GoBack);
        if let TriggerKind::OnPinch { scale_threshold } = t.kind {
            assert!((scale_threshold - 1.5).abs() < f64::EPSILON);
        } else {
            panic!("wrong variant");
        }
    }

    // TRIG-27: OnRotate stores angle_deg.
    #[test]
    fn trig_27_rotate_stores_angle() {
        let t = Trigger::new(TriggerKind::OnRotate { angle_deg: 45.0 }, Action::GoBack);
        if let TriggerKind::OnRotate { angle_deg } = t.kind {
            assert!((angle_deg - 45.0).abs() < f64::EPSILON);
        } else {
            panic!("wrong variant");
        }
    }

    // TRIG-28: OnPinch serde roundtrip.
    #[test]
    fn trig_28_pinch_serde_roundtrip() {
        let k = TriggerKind::OnPinch { scale_threshold: 2.0 };
        let json = serde_json::to_string(&k).unwrap();
        let back: TriggerKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, k);
    }

    // ── Visibility / Lifecycle ────────────────────────────────────────

    // TRIG-29: OnIntersection entering=true.
    #[test]
    fn trig_29_intersection_entering() {
        let t = Trigger::new(
            TriggerKind::OnIntersection { threshold: 0.5, entering: true },
            Action::GoBack,
        );
        if let TriggerKind::OnIntersection { entering, .. } = t.kind {
            assert!(entering);
        } else {
            panic!("wrong variant");
        }
    }

    // TRIG-30: OnIntersection entering vs leaving are distinct.
    #[test]
    fn trig_30_intersection_entering_leaving_distinct() {
        let k_enter = TriggerKind::OnIntersection { threshold: 0.5, entering: true };
        let k_leave = TriggerKind::OnIntersection { threshold: 0.5, entering: false };
        assert_ne!(k_enter, k_leave);
    }

    // TRIG-31: OnVisibilityChange hidden and visible are distinct.
    #[test]
    fn trig_31_visibility_change_distinct() {
        let show = TriggerKind::OnVisibilityChange { visible: true };
        let hide = TriggerKind::OnVisibilityChange { visible: false };
        assert_ne!(show, hide);
    }

    // TRIG-32: OnWindowResize creation.
    #[test]
    fn trig_32_window_resize_creation() {
        let t = Trigger::new(TriggerKind::OnWindowResize, Action::GoBack);
        assert_eq!(t.kind, TriggerKind::OnWindowResize);
    }

    // TRIG-33: OnMount and OnUnmount are distinct.
    #[test]
    fn trig_33_mount_unmount_distinct() {
        assert_ne!(TriggerKind::OnMount, TriggerKind::OnUnmount);
    }

    // TRIG-34: OnWindowResize serde roundtrip.
    #[test]
    fn trig_34_window_resize_serde_roundtrip() {
        let k = TriggerKind::OnWindowResize;
        let json = serde_json::to_string(&k).unwrap();
        let back: TriggerKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, k);
    }

    // ── Media events ─────────────────────────────────────────────────

    // TRIG-35: OnMediaPlay creation.
    #[test]
    fn trig_35_media_play_creation() {
        let t = Trigger::new(TriggerKind::OnMediaPlay, Action::GoBack);
        assert_eq!(t.kind, TriggerKind::OnMediaPlay);
    }

    // TRIG-36: OnMediaPause creation.
    #[test]
    fn trig_36_media_pause_creation() {
        let t = Trigger::new(TriggerKind::OnMediaPause, Action::GoBack);
        assert_eq!(t.kind, TriggerKind::OnMediaPause);
    }

    // TRIG-37: OnMediaEnded creation.
    #[test]
    fn trig_37_media_ended_creation() {
        let t = Trigger::new(TriggerKind::OnMediaEnded, Action::GoBack);
        assert_eq!(t.kind, TriggerKind::OnMediaEnded);
    }

    // TRIG-38: OnMediaLoad and OnMediaError are distinct.
    #[test]
    fn trig_38_media_load_error_distinct() {
        assert_ne!(TriggerKind::OnMediaLoad, TriggerKind::OnMediaError);
    }

    // TRIG-39: OnMediaTimeUpdate stores at_seconds.
    #[test]
    fn trig_39_media_time_update_stores_seconds() {
        let t = Trigger::new(
            TriggerKind::OnMediaTimeUpdate { at_seconds: 3.5 },
            Action::GoBack,
        );
        if let TriggerKind::OnMediaTimeUpdate { at_seconds } = t.kind {
            assert!((at_seconds - 3.5).abs() < f64::EPSILON);
        } else {
            panic!("wrong variant");
        }
    }

    // TRIG-40: OnMediaTimeUpdate serde roundtrip.
    #[test]
    fn trig_40_media_time_serde_roundtrip() {
        let k = TriggerKind::OnMediaTimeUpdate { at_seconds: 10.0 };
        let json = serde_json::to_string(&k).unwrap();
        let back: TriggerKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, k);
    }

    // TRIG-41: All 5 basic media trigger kinds are distinct.
    #[test]
    fn trig_41_media_kinds_all_distinct() {
        let kinds = vec![
            TriggerKind::OnMediaPlay,
            TriggerKind::OnMediaPause,
            TriggerKind::OnMediaEnded,
            TriggerKind::OnMediaLoad,
            TriggerKind::OnMediaError,
        ];
        for i in 0..kinds.len() {
            for j in 0..kinds.len() {
                if i != j {
                    assert_ne!(kinds[i], kinds[j]);
                }
            }
        }
    }

    // ── Form / Input ──────────────────────────────────────────────────

    // TRIG-42: OnFormSubmit creation.
    #[test]
    fn trig_42_form_submit_creation() {
        let t = Trigger::new(TriggerKind::OnFormSubmit, Action::GoBack);
        assert_eq!(t.kind, TriggerKind::OnFormSubmit);
    }

    // TRIG-43: OnFormReset creation.
    #[test]
    fn trig_43_form_reset_creation() {
        let t = Trigger::new(TriggerKind::OnFormReset, Action::GoBack);
        assert_eq!(t.kind, TriggerKind::OnFormReset);
    }

    // TRIG-44: OnInputChange creation.
    #[test]
    fn trig_44_input_change_creation() {
        let t = Trigger::new(TriggerKind::OnInputChange, Action::GoBack);
        assert_eq!(t.kind, TriggerKind::OnInputChange);
    }

    // TRIG-45: OnInputValid and OnInputInvalid are distinct.
    #[test]
    fn trig_45_input_valid_invalid_distinct() {
        assert_ne!(TriggerKind::OnInputValid, TriggerKind::OnInputInvalid);
    }

    // TRIG-46: OnFormSubmit serde roundtrip.
    #[test]
    fn trig_46_form_submit_serde_roundtrip() {
        let k = TriggerKind::OnFormSubmit;
        let json = serde_json::to_string(&k).unwrap();
        let back: TriggerKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, k);
    }

    // ── Network / Data ────────────────────────────────────────────────

    // TRIG-47: OnDataLoaded stores source_id.
    #[test]
    fn trig_47_data_loaded_stores_id() {
        let t = Trigger::new(
            TriggerKind::OnDataLoaded { source_id: "users-api".into() },
            Action::GoBack,
        );
        if let TriggerKind::OnDataLoaded { source_id } = &t.kind {
            assert_eq!(source_id, "users-api");
        } else {
            panic!("wrong variant");
        }
    }

    // TRIG-48: OnDataError stores source_id.
    #[test]
    fn trig_48_data_error_stores_id() {
        let t = Trigger::new(
            TriggerKind::OnDataError { source_id: "users-api".into() },
            Action::GoBack,
        );
        if let TriggerKind::OnDataError { source_id } = &t.kind {
            assert_eq!(source_id, "users-api");
        } else {
            panic!("wrong variant");
        }
    }

    // TRIG-49: OnDataLoaded and OnDataError with same source_id are distinct.
    #[test]
    fn trig_49_data_loaded_error_distinct() {
        let loaded = TriggerKind::OnDataLoaded { source_id: "s".into() };
        let error  = TriggerKind::OnDataError  { source_id: "s".into() };
        assert_ne!(loaded, error);
    }

    // TRIG-50: OnDataLoaded serde roundtrip.
    #[test]
    fn trig_50_data_loaded_serde_roundtrip() {
        let k = TriggerKind::OnDataLoaded { source_id: "products".into() };
        let json = serde_json::to_string(&k).unwrap();
        let back: TriggerKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, k);
    }

    // ── Custom event ─────────────────────────────────────────────────

    // TRIG-51: OnCustomEvent stores name.
    #[test]
    fn trig_51_custom_event_stores_name() {
        let t = Trigger::new(
            TriggerKind::OnCustomEvent { name: "hero:clicked".into() },
            Action::GoBack,
        );
        if let TriggerKind::OnCustomEvent { name } = &t.kind {
            assert_eq!(name, "hero:clicked");
        } else {
            panic!("wrong variant");
        }
    }

    // TRIG-52: Two OnCustomEvent with different names are distinct.
    #[test]
    fn trig_52_custom_event_distinct_names() {
        let a = TriggerKind::OnCustomEvent { name: "foo".into() };
        let b = TriggerKind::OnCustomEvent { name: "bar".into() };
        assert_ne!(a, b);
    }

    // TRIG-53: OnCustomEvent serde roundtrip.
    #[test]
    fn trig_53_custom_event_serde_roundtrip() {
        let k = TriggerKind::OnCustomEvent { name: "my-event".into() };
        let json = serde_json::to_string(&k).unwrap();
        let back: TriggerKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, k);
    }

    // ── TriggerCondition ─────────────────────────────────────────────

    // TRIG-54: Always condition serde roundtrip.
    #[test]
    fn trig_54_condition_always_serde() {
        let c = TriggerCondition::Always;
        let json = serde_json::to_string(&c).unwrap();
        let back: TriggerCondition = serde_json::from_str(&json).unwrap();
        assert_eq!(back, c);
    }

    // TRIG-55: VariableEquals condition stores fields correctly.
    #[test]
    fn trig_55_condition_variable_equals() {
        let c = TriggerCondition::VariableEquals {
            variable: "isLoggedIn".into(),
            expected: serde_json::Value::Bool(true),
        };
        if let TriggerCondition::VariableEquals { variable, expected } = &c {
            assert_eq!(variable, "isLoggedIn");
            assert_eq!(*expected, serde_json::Value::Bool(true));
        } else {
            panic!("wrong variant");
        }
    }

    // TRIG-56: Breakpoint condition stores min/max width.
    #[test]
    fn trig_56_condition_breakpoint() {
        let c = TriggerCondition::Breakpoint { min_width: Some(768), max_width: None };
        if let TriggerCondition::Breakpoint { min_width, max_width } = c {
            assert_eq!(min_width, Some(768));
            assert!(max_width.is_none());
        } else {
            panic!("wrong variant");
        }
    }

    // TRIG-57: Not(Always) condition serde roundtrip.
    #[test]
    fn trig_57_condition_not_serde() {
        let c = TriggerCondition::Not(Box::new(TriggerCondition::Always));
        let json = serde_json::to_string(&c).unwrap();
        let back: TriggerCondition = serde_json::from_str(&json).unwrap();
        assert_eq!(back, c);
    }

    // TRIG-58: All condition holds multiple inner conditions.
    #[test]
    fn trig_58_condition_all() {
        let c = TriggerCondition::All(vec![
            TriggerCondition::PrefersReducedMotion,
            TriggerCondition::PrefersDarkMode,
        ]);
        if let TriggerCondition::All(inner) = &c {
            assert_eq!(inner.len(), 2);
        } else {
            panic!("wrong variant");
        }
    }

    // TRIG-59: Any condition holds one inner condition.
    #[test]
    fn trig_59_condition_any() {
        let c = TriggerCondition::Any(vec![TriggerCondition::PrefersDarkMode]);
        if let TriggerCondition::Any(inner) = &c {
            assert_eq!(inner.len(), 1);
        } else {
            panic!("wrong variant");
        }
    }

    // TRIG-60: Trigger with attached condition stores it correctly.
    #[test]
    fn trig_60_trigger_with_condition() {
        let t = Trigger::new(TriggerKind::OnClick, Action::GoBack)
            .with_condition(TriggerCondition::PrefersDarkMode);
        assert_eq!(t.condition, TriggerCondition::PrefersDarkMode);
    }

    // TRIG-61: Default trigger has Always condition.
    #[test]
    fn trig_61_default_trigger_condition_always() {
        let t = Trigger::new(TriggerKind::OnClick, Action::GoBack);
        assert_eq!(t.condition, TriggerCondition::Always);
    }

    // TRIG-62: Trigger with condition serde roundtrip.
    #[test]
    fn trig_62_trigger_condition_serde_roundtrip() {
        let t = Trigger::new(TriggerKind::OnFocus, Action::GoBack)
            .with_condition(TriggerCondition::VariableTruthy { variable: "ready".into() });
        let json = serde_json::to_string(&t).unwrap();
        let back: Trigger = serde_json::from_str(&json).unwrap();
        assert_eq!(back.condition, t.condition);
    }

    // ── New Action variants ───────────────────────────────────────────

    // TRIG-63: PlaySound action stores all fields.
    #[test]
    fn trig_63_play_sound_action_fields() {
        let a = Action::PlaySound { src: "click.mp3".into(), volume: 0.8, looping: false };
        if let Action::PlaySound { src, volume, looping } = &a {
            assert_eq!(src, "click.mp3");
            assert!((volume - 0.8).abs() < f64::EPSILON);
            assert!(!looping);
        } else {
            panic!("wrong variant");
        }
    }

    // TRIG-64: PlaySound serde roundtrip.
    #[test]
    fn trig_64_play_sound_serde_roundtrip() {
        let a = Action::PlaySound { src: "bg.ogg".into(), volume: 1.0, looping: true };
        let json = serde_json::to_string(&a).unwrap();
        let back: Action = serde_json::from_str(&json).unwrap();
        assert_eq!(back, a);
    }

    // TRIG-65: SetVolume serde roundtrip.
    #[test]
    fn trig_65_set_volume_serde_roundtrip() {
        let a = Action::SetVolume { src: "bg.ogg".into(), volume: 0.5, ramp_ms: 200 };
        let json = serde_json::to_string(&a).unwrap();
        let back: Action = serde_json::from_str(&json).unwrap();
        assert_eq!(back, a);
    }

    // TRIG-66: StartAnimation stores layer_id and animation_name.
    #[test]
    fn trig_66_start_animation_action() {
        let lid = Uuid::new_v4();
        let a = Action::StartAnimation { layer_id: lid, animation_name: "pulse".into() };
        if let Action::StartAnimation { layer_id, animation_name } = &a {
            assert_eq!(*layer_id, lid);
            assert_eq!(animation_name, "pulse");
        } else {
            panic!("wrong variant");
        }
    }

    // TRIG-67: SetVisibility stores visible=false.
    #[test]
    fn trig_67_set_visibility_false() {
        let lid = Uuid::new_v4();
        let a = Action::SetVisibility { layer_id: lid, visible: false };
        if let Action::SetVisibility { visible, .. } = a {
            assert!(!visible);
        } else {
            panic!("wrong variant");
        }
    }

    // TRIG-68: AddCssClass stores class_name.
    #[test]
    fn trig_68_add_css_class_name() {
        let lid = Uuid::new_v4();
        let a = Action::AddCssClass { layer_id: lid, class_name: "highlighted".into() };
        if let Action::AddCssClass { class_name, .. } = &a {
            assert_eq!(class_name, "highlighted");
        } else {
            panic!("wrong variant");
        }
    }

    // TRIG-69: UpdateVariable stores name and value.
    #[test]
    fn trig_69_update_variable_stores_fields() {
        let a = Action::UpdateVariable { name: "score".into(), value: serde_json::json!(42) };
        if let Action::UpdateVariable { name, value } = &a {
            assert_eq!(name, "score");
            assert_eq!(*value, serde_json::json!(42));
        } else {
            panic!("wrong variant");
        }
    }

    // TRIG-70: IncrementVariable stores delta.
    #[test]
    fn trig_70_increment_variable_delta() {
        let a = Action::IncrementVariable { name: "counter".into(), delta: 1.0 };
        if let Action::IncrementVariable { delta, .. } = a {
            assert!((delta - 1.0).abs() < f64::EPSILON);
        } else {
            panic!("wrong variant");
        }
    }

    // TRIG-71: ToggleVariable stores name.
    #[test]
    fn trig_71_toggle_variable_name() {
        let a = Action::ToggleVariable { name: "menu_open".into() };
        if let Action::ToggleVariable { name } = &a {
            assert_eq!(name, "menu_open");
        } else {
            panic!("wrong variant");
        }
    }

    // TRIG-72: PlayMedia stores layer_id.
    #[test]
    fn trig_72_play_media_layer_id() {
        let lid = Uuid::new_v4();
        let a = Action::PlayMedia { layer_id: lid };
        if let Action::PlayMedia { layer_id } = a {
            assert_eq!(layer_id, lid);
        } else {
            panic!("wrong variant");
        }
    }

    // TRIG-73: SeekMedia stores time_seconds.
    #[test]
    fn trig_73_seek_media_time() {
        let lid = Uuid::new_v4();
        let a = Action::SeekMedia { layer_id: lid, time_seconds: 12.5 };
        if let Action::SeekMedia { time_seconds, .. } = a {
            assert!((time_seconds - 12.5).abs() < f64::EPSILON);
        } else {
            panic!("wrong variant");
        }
    }

    // TRIG-74: SetMute with muted=true.
    #[test]
    fn trig_74_set_mute_true() {
        let lid = Uuid::new_v4();
        let a = Action::SetMute { layer_id: lid, muted: true };
        if let Action::SetMute { muted, .. } = a {
            assert!(muted);
        } else {
            panic!("wrong variant");
        }
    }

    // TRIG-75: EmitCustomEvent serde roundtrip.
    #[test]
    fn trig_75_emit_custom_event_serde() {
        let a = Action::EmitCustomEvent {
            name: "hero:clicked".into(),
            payload: serde_json::json!({ "x": 10, "y": 20 }),
        };
        let json = serde_json::to_string(&a).unwrap();
        let back: Action = serde_json::from_str(&json).unwrap();
        assert_eq!(back, a);
    }

    // TRIG-76: CopyToClipboard stores text.
    #[test]
    fn trig_76_copy_to_clipboard_text() {
        let a = Action::CopyToClipboard { text: "Hello, World!".into() };
        if let Action::CopyToClipboard { text } = &a {
            assert_eq!(text, "Hello, World!");
        } else {
            panic!("wrong variant");
        }
    }

    // TRIG-77: Vibrate stores pattern.
    #[test]
    fn trig_77_vibrate_pattern() {
        let a = Action::Vibrate { pattern_ms: vec![100, 50, 100] };
        if let Action::Vibrate { pattern_ms } = &a {
            assert_eq!(*pattern_ms, vec![100u64, 50, 100]);
        } else {
            panic!("wrong variant");
        }
    }

    // TRIG-78: TrackEvent stores all fields.
    #[test]
    fn trig_78_track_event_fields() {
        let a = Action::TrackEvent {
            category: "ui".into(),
            action: "click".into(),
            label: Some("hero-btn".into()),
            value: None,
        };
        if let Action::TrackEvent { category, action, label, value } = &a {
            assert_eq!(category, "ui");
            assert_eq!(action, "click");
            assert_eq!(label.as_deref(), Some("hero-btn"));
            assert!(value.is_none());
        } else {
            panic!("wrong variant");
        }
    }

    // TRIG-79: ScrollTo stores layer_id and Smooth behavior.
    #[test]
    fn trig_79_scroll_to_smooth() {
        let lid = Uuid::new_v4();
        let a = Action::ScrollTo { layer_id: lid, behavior: ScrollBehavior::Smooth };
        if let Action::ScrollTo { layer_id, behavior } = a {
            assert_eq!(layer_id, lid);
            assert_eq!(behavior, ScrollBehavior::Smooth);
        } else {
            panic!("wrong variant");
        }
    }

    // TRIG-80: ScrollBehavior serde roundtrip.
    #[test]
    fn trig_80_scroll_behavior_serde() {
        let b = ScrollBehavior::Auto;
        let json = serde_json::to_string(&b).unwrap();
        let back: ScrollBehavior = serde_json::from_str(&json).unwrap();
        assert_eq!(back, b);
    }

    // ── Scenario / Integration ────────────────────────────────────────

    // TRIG-81: OnMediaPlay trigger fires PlaySound action — matched by kind.
    #[test]
    fn trig_81_media_play_fires_sound() {
        let layer = Uuid::new_v4();
        let target = InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnMediaPlay,
            Action::PlaySound { src: "start.mp3".into(), volume: 1.0, looping: false },
        ));
        let matches = target.matching_triggers(&TriggerKind::OnMediaPlay);
        assert_eq!(matches.len(), 1);
        if let Action::PlaySound { src, .. } = &matches[0].action {
            assert_eq!(src, "start.mp3");
        } else {
            panic!("expected PlaySound");
        }
    }

    // TRIG-82: OnFormSubmit triggers NavigateTo.
    #[test]
    fn trig_82_form_submit_navigates() {
        let layer = Uuid::new_v4();
        let dest = Uuid::new_v4();
        let target = InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnFormSubmit,
            Action::NavigateTo { target_id: dest, animation: None },
        ));
        let matches = target.matching_triggers(&TriggerKind::OnFormSubmit);
        assert_eq!(matches.len(), 1);
        if let Action::NavigateTo { target_id, .. } = matches[0].action {
            assert_eq!(target_id, dest);
        } else {
            panic!("expected NavigateTo");
        }
    }

    // TRIG-83: OnIntersection (entering) triggers EmitCustomEvent.
    #[test]
    fn trig_83_intersection_emits_event() {
        let layer = Uuid::new_v4();
        let target = InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnIntersection { threshold: 0.75, entering: true },
            Action::EmitCustomEvent { name: "visible".into(), payload: serde_json::Value::Null },
        ));
        let k = TriggerKind::OnIntersection { threshold: 0.75, entering: true };
        assert_eq!(target.matching_triggers(&k).len(), 1);
    }

    // TRIG-84: VariableTruthy condition attaches to trigger correctly.
    #[test]
    fn trig_84_variable_truthy_guard() {
        let t = Trigger::new(TriggerKind::OnClick, Action::GoBack)
            .with_condition(TriggerCondition::VariableTruthy {
                variable: "featureEnabled".into(),
            });
        if let TriggerCondition::VariableTruthy { variable } = &t.condition {
            assert_eq!(variable, "featureEnabled");
        } else {
            panic!("wrong condition variant");
        }
    }

    // TRIG-85: Nested All(Any) condition serde roundtrip.
    #[test]
    fn trig_85_nested_all_any_serde() {
        let c = TriggerCondition::All(vec![
            TriggerCondition::Any(vec![
                TriggerCondition::PrefersDarkMode,
                TriggerCondition::PrefersReducedMotion,
            ]),
            TriggerCondition::VariableTruthy { variable: "active".into() },
        ]);
        let json = serde_json::to_string(&c).unwrap();
        let back: TriggerCondition = serde_json::from_str(&json).unwrap();
        assert_eq!(back, c);
    }

    // TRIG-86: Disabled OnMediaEnded trigger is not matched.
    #[test]
    fn trig_86_disabled_media_trigger() {
        let layer = Uuid::new_v4();
        let target = InteractionTarget::new(layer).with_trigger(
            Trigger::new(TriggerKind::OnMediaEnded, Action::GoBack).disabled(),
        );
        assert_eq!(target.matching_triggers(&TriggerKind::OnMediaEnded).len(), 0);
    }

    // TRIG-87: OnDataLoaded fires UpdateVariable — matched and action stored.
    #[test]
    fn trig_87_data_loaded_updates_variable() {
        let layer = Uuid::new_v4();
        let target = InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnDataLoaded { source_id: "cart".into() },
            Action::UpdateVariable {
                name: "cartLoaded".into(),
                value: serde_json::json!(true),
            },
        ));
        let k = TriggerKind::OnDataLoaded { source_id: "cart".into() };
        let matches = target.matching_triggers(&k);
        assert_eq!(matches.len(), 1);
        if let Action::UpdateVariable { name, .. } = &matches[0].action {
            assert_eq!(name, "cartLoaded");
        } else {
            panic!("expected UpdateVariable");
        }
    }

    // TRIG-88: OnCustomEvent fires Vibrate action.
    #[test]
    fn trig_88_custom_event_vibrates() {
        let layer = Uuid::new_v4();
        let target = InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnCustomEvent { name: "success".into() },
            Action::Vibrate { pattern_ms: vec![50, 30, 50] },
        ));
        let k = TriggerKind::OnCustomEvent { name: "success".into() };
        assert_eq!(target.matching_triggers(&k).len(), 1);
    }

    // TRIG-89: SetStyleProperty serde roundtrip.
    #[test]
    fn trig_89_set_style_property_serde() {
        let lid = Uuid::new_v4();
        let a = Action::SetStyleProperty {
            layer_id: lid,
            property: "color".into(),
            value: "#ff0000".into(),
            transition_ms: 300,
        };
        let json = serde_json::to_string(&a).unwrap();
        let back: Action = serde_json::from_str(&json).unwrap();
        assert_eq!(back, a);
    }

    // TRIG-90: SeekAnimation serde roundtrip.
    #[test]
    fn trig_90_seek_animation_serde() {
        let lid = Uuid::new_v4();
        let a = Action::SeekAnimation {
            layer_id: lid,
            animation_name: "fade".into(),
            time_ms: 500,
        };
        let json = serde_json::to_string(&a).unwrap();
        let back: Action = serde_json::from_str(&json).unwrap();
        assert_eq!(back, a);
    }

    // TRIG-91: StopAnimation and PauseAnimation are distinct.
    #[test]
    fn trig_91_stop_vs_pause_animation() {
        let lid = Uuid::new_v4();
        let stop  = Action::StopAnimation  { layer_id: lid, animation_name: "x".into() };
        let pause = Action::PauseAnimation { layer_id: lid, animation_name: "x".into() };
        assert_ne!(stop, pause);
    }

    // TRIG-92: SetOpacity stores opacity and duration_ms.
    #[test]
    fn trig_92_set_opacity_fields() {
        let lid = Uuid::new_v4();
        let a = Action::SetOpacity { layer_id: lid, opacity: 0.5, duration_ms: 400 };
        if let Action::SetOpacity { opacity, duration_ms, .. } = a {
            assert!((opacity - 0.5).abs() < f64::EPSILON);
            assert_eq!(duration_ms, 400);
        } else {
            panic!("wrong variant");
        }
    }

    // TRIG-93: RemoveCssClass and ToggleCssClass are distinct.
    #[test]
    fn trig_93_remove_vs_toggle_class() {
        let lid = Uuid::new_v4();
        let remove = Action::RemoveCssClass { layer_id: lid, class_name: "active".into() };
        let toggle = Action::ToggleCssClass { layer_id: lid, class_name: "active".into() };
        assert_ne!(remove, toggle);
    }

    // TRIG-94: Sequence wrapping new action types holds correct count.
    #[test]
    fn trig_94_sequence_new_actions_count() {
        let lid = Uuid::new_v4();
        let seq = Action::Sequence(vec![
            Action::PlaySound { src: "success.mp3".into(), volume: 1.0, looping: false },
            Action::SetVisibility { layer_id: lid, visible: true },
            Action::EmitCustomEvent { name: "done".into(), payload: serde_json::Value::Null },
        ]);
        if let Action::Sequence(actions) = &seq {
            assert_eq!(actions.len(), 3);
        } else {
            panic!("expected Sequence");
        }
    }

    // TRIG-95: Sequence with new action types serde roundtrip.
    #[test]
    fn trig_95_sequence_serde_roundtrip_new_actions() {
        let lid = Uuid::new_v4();
        let seq = Action::Sequence(vec![
            Action::PlayMedia { layer_id: lid },
            Action::TrackEvent {
                category: "proto".into(),
                action: "play".into(),
                label: None,
                value: Some(1.0),
            },
        ]);
        let json = serde_json::to_string(&seq).unwrap();
        let back: Action = serde_json::from_str(&json).unwrap();
        assert_eq!(back, seq);
    }

    // TRIG-96: All 5 animation actions are pairwise distinct.
    #[test]
    fn trig_96_animation_actions_pairwise_distinct() {
        let lid = Uuid::new_v4();
        let name = "fade".to_string();
        let actions = vec![
            Action::StartAnimation  { layer_id: lid, animation_name: name.clone() },
            Action::StopAnimation   { layer_id: lid, animation_name: name.clone() },
            Action::PauseAnimation  { layer_id: lid, animation_name: name.clone() },
            Action::ResumeAnimation { layer_id: lid, animation_name: name.clone() },
            Action::SeekAnimation   { layer_id: lid, animation_name: name.clone(), time_ms: 0 },
        ];
        for i in 0..actions.len() {
            for j in 0..actions.len() {
                if i != j {
                    assert_ne!(actions[i], actions[j]);
                }
            }
        }
    }

    // TRIG-97: OnPointerEnter/Leave/Move are each matched independently.
    #[test]
    fn trig_97_pointer_triggers_independent() {
        let layer = Uuid::new_v4();
        let target = InteractionTarget::new(layer)
            .with_trigger(Trigger::new(TriggerKind::OnPointerEnter, Action::GoBack))
            .with_trigger(Trigger::new(TriggerKind::OnPointerLeave, Action::GoBack))
            .with_trigger(Trigger::new(TriggerKind::OnPointerMove,  Action::GoBack));
        assert_eq!(target.matching_triggers(&TriggerKind::OnPointerEnter).len(), 1);
        assert_eq!(target.matching_triggers(&TriggerKind::OnPointerLeave).len(), 1);
        assert_eq!(target.matching_triggers(&TriggerKind::OnPointerMove).len(),  1);
        assert_eq!(target.active_trigger_count(), 3);
    }

    // TRIG-98: OnMount trigger is not matched by OnUnmount.
    #[test]
    fn trig_98_mount_not_matched_by_unmount() {
        let layer = Uuid::new_v4();
        let target = InteractionTarget::new(layer)
            .with_trigger(Trigger::new(TriggerKind::OnMount, Action::GoBack));
        assert_eq!(target.matching_triggers(&TriggerKind::OnMount).len(),   1);
        assert_eq!(target.matching_triggers(&TriggerKind::OnUnmount).len(), 0);
    }

    // TRIG-99: OnWindowScroll fires UpdateVariable — matched correctly.
    #[test]
    fn trig_99_scroll_fires_update_variable() {
        let layer = Uuid::new_v4();
        let target = InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnWindowScroll,
            Action::UpdateVariable { name: "scrolled".into(), value: serde_json::json!(true) },
        ));
        assert_eq!(target.matching_triggers(&TriggerKind::OnWindowScroll).len(), 1);
    }

    // TRIG-100: All 21 new trigger kinds can coexist on one InteractionTarget.
    #[test]
    fn trig_100_all_new_triggers_coexist() {
        let layer = Uuid::new_v4();
        let target = InteractionTarget::new(layer)
            .with_trigger(Trigger::new(TriggerKind::OnFocus,        Action::GoBack))
            .with_trigger(Trigger::new(TriggerKind::OnBlur,         Action::GoBack))
            .with_trigger(Trigger::new(TriggerKind::OnContextMenu,  Action::GoBack))
            .with_trigger(Trigger::new(TriggerKind::OnPointerEnter, Action::GoBack))
            .with_trigger(Trigger::new(TriggerKind::OnPointerLeave, Action::GoBack))
            .with_trigger(Trigger::new(TriggerKind::OnPointerMove,  Action::GoBack))
            .with_trigger(Trigger::new(TriggerKind::OnMount,        Action::GoBack))
            .with_trigger(Trigger::new(TriggerKind::OnUnmount,      Action::GoBack))
            .with_trigger(Trigger::new(TriggerKind::OnMediaPlay,    Action::GoBack))
            .with_trigger(Trigger::new(TriggerKind::OnMediaPause,   Action::GoBack))
            .with_trigger(Trigger::new(TriggerKind::OnMediaEnded,   Action::GoBack))
            .with_trigger(Trigger::new(TriggerKind::OnMediaLoad,    Action::GoBack))
            .with_trigger(Trigger::new(TriggerKind::OnMediaError,   Action::GoBack))
            .with_trigger(Trigger::new(TriggerKind::OnFormSubmit,   Action::GoBack))
            .with_trigger(Trigger::new(TriggerKind::OnFormReset,    Action::GoBack))
            .with_trigger(Trigger::new(TriggerKind::OnInputChange,  Action::GoBack))
            .with_trigger(Trigger::new(TriggerKind::OnInputValid,   Action::GoBack))
            .with_trigger(Trigger::new(TriggerKind::OnInputInvalid, Action::GoBack))
            .with_trigger(Trigger::new(TriggerKind::OnWindowResize, Action::GoBack))
            .with_trigger(Trigger::new(TriggerKind::OnWindowScroll, Action::GoBack))
            .with_trigger(Trigger::new(
                TriggerKind::OnCustomEvent { name: "x".into() },
                Action::GoBack,
            ));
        assert_eq!(target.active_trigger_count(), 21);
    }
}
