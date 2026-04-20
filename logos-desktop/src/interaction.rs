// logos-desktop/src/interaction.rs
//
//! # Desktop Interaction Engine
//!
//! Bridges the `logos-prototyping` trigger/action system to the Logos
//! desktop runtime.  Every [`TriggerKind`] variant that can originate from
//! native OS input (mouse, keyboard, gamepad, resize, …) is translated here
//! into a [`DesktopInputEvent`] which the canvas/window layer can produce.
//! Every [`Action`] variant is executed by [`DesktopActionExecutor`].
//!
//! ## Architecture
//!
//! ```text
//!  OS event (winit / evdev)
//!       │
//!       ▼
//!  DesktopInputEvent              ← thin OS-agnostic wrapper
//!       │
//!       ▼
//!  InteractionDispatcher::dispatch()
//!       │  looks up InteractionTarget for the hit-tested layer
//!       │  evaluates TriggerCondition guard
//!       ▼
//!  DesktopActionExecutor::execute()   ← runs the Action
//!       │
//!       ├── Sound     → rodio (native) / no-op in headless tests
//!       ├── Animation → logos-prototyping PreviewSession
//!       ├── Media     → platform media player handle
//!       ├── Variable  → runtime VariableStore
//!       ├── Style     → queued LayerStyleUpdate (picked up by render loop)
//!       └── Navigate  → DesktopNavigationStack
//! ```

use std::collections::HashMap;
use uuid::Uuid;

use logos_prototyping::trigger::{
    Action, InteractionTarget, ScrollBehavior, TriggerCondition, TriggerKind,
};
use logos_prototyping::preview::{PreviewEvent, PreviewSession};

// ── OS-agnostic input event ───────────────────────────────────────────────────

/// A normalised desktop input event that the canvas window produces.
///
/// The dispatcher maps each variant to the matching [`TriggerKind`](s).
#[derive(Debug, Clone, PartialEq)]
pub enum DesktopInputEvent {
    // Pointer
    Click       { layer_id: Uuid },
    DoubleClick { layer_id: Uuid },
    RightClick  { layer_id: Uuid },
    MouseDown   { layer_id: Uuid },
    MouseUp     { layer_id: Uuid },
    HoverEnter  { layer_id: Uuid },
    HoverExit   { layer_id: Uuid },
    PointerEnter{ layer_id: Uuid },
    PointerLeave{ layer_id: Uuid },
    PointerMove { layer_id: Uuid },
    Drag        { layer_id: Uuid, distance_px: f64 },
    Swipe       { layer_id: Uuid, direction: logos_prototyping::trigger::SwipeDirection, velocity_px_s: f64 },
    LongPress   { layer_id: Uuid, held_ms: u64 },
    Pinch       { layer_id: Uuid, scale: f64 },
    Rotate      { layer_id: Uuid, angle_deg: f64 },

    // Keyboard
    KeyPress    { layer_id: Uuid, key: String },
    KeyRelease  { layer_id: Uuid, key: String },
    KeyHold     { layer_id: Uuid, key: String, held_ms: u64 },

    // Scroll / wheel
    Scroll      { layer_id: Uuid },
    WindowScroll,

    // Focus
    Focus  { layer_id: Uuid },
    Blur   { layer_id: Uuid },

    // Context menu
    ContextMenu { layer_id: Uuid },

    // Lifecycle
    Mount   { layer_id: Uuid },
    Unmount { layer_id: Uuid },

    // Viewport / visibility
    WindowResize,
    VisibilityChange { visible: bool },
    Intersection     { layer_id: Uuid, threshold: f64, entering: bool },

    // Media
    MediaPlay    { layer_id: Uuid },
    MediaPause   { layer_id: Uuid },
    MediaEnded   { layer_id: Uuid },
    MediaLoad    { layer_id: Uuid },
    MediaError   { layer_id: Uuid },
    MediaTimeUpdate { layer_id: Uuid, current_time_s: f64 },

    // Form / input
    FormSubmit    { layer_id: Uuid },
    FormReset     { layer_id: Uuid },
    InputChange   { layer_id: Uuid },
    InputValid    { layer_id: Uuid },
    InputInvalid  { layer_id: Uuid },

    // Network / data
    DataLoaded { layer_id: Uuid, source_id: String },
    DataError  { layer_id: Uuid, source_id: String },

    // Gamepad
    Gamepad { layer_id: Uuid, button: String },

    // Custom / synthetic
    CustomEvent { layer_id: Uuid, name: String },

    // Timed
    Delay      { layer_id: Uuid, elapsed_ms: u64 },
    AfterDelay { layer_id: Uuid, elapsed_ms: u64 },
}

// ── Pending action result ─────────────────────────────────────────────────────

/// What the executor wants the host UI to do after running an action.
#[derive(Debug, Clone, PartialEq)]
pub enum DesktopActionEffect {
    /// Navigate to a different artboard / frame.
    Navigate { target_id: Uuid },
    /// Go back in the navigation stack.
    GoBack,
    /// Open a URL in the system browser.
    OpenUrl { url: String },
    /// Show an overlay.
    ShowOverlay { content_id: Uuid },
    /// Dismiss an overlay.
    DismissOverlay { content_id: Uuid },
    /// Update a layer's visibility in the next render frame.
    SetLayerVisible { layer_id: Uuid, visible: bool },
    /// Update a layer's opacity in the next render frame.
    SetLayerOpacity { layer_id: Uuid, opacity: f64 },
    /// Add/remove/toggle a CSS-style class on an exported HTML layer.
    ModifyLayerClass { layer_id: Uuid, class_name: String, op: ClassOp },
    /// Set an inline style property on a layer.
    SetLayerStyle { layer_id: Uuid, property: String, value: String },
    /// Scroll the canvas to a layer.
    ScrollToLayer { layer_id: Uuid, behavior: ScrollBehavior },
    /// Set focus to a layer.
    FocusLayer { layer_id: Uuid },
    /// Play a named animation on a layer.
    AnimationPlay { layer_id: Uuid, name: String },
    /// Stop a named animation.
    AnimationStop { layer_id: Uuid, name: String },
    /// Pause a named animation.
    AnimationPause { layer_id: Uuid, name: String },
    /// Resume a named animation.
    AnimationResume { layer_id: Uuid, name: String },
    /// Seek a named animation to a time.
    AnimationSeek { layer_id: Uuid, name: String, time_ms: u64 },
    /// Start media playback on a layer.
    MediaPlay { layer_id: Uuid },
    /// Pause media.
    MediaPause { layer_id: Uuid },
    /// Stop media.
    MediaStop { layer_id: Uuid },
    /// Seek media to a time.
    MediaSeek { layer_id: Uuid, time_s: f64 },
    /// Mute/unmute a media layer.
    MediaMute { layer_id: Uuid, muted: bool },
    /// Play a sound asset (rodio on desktop).
    PlaySound { src: String, volume: f64, looping: bool },
    /// Pause a sound.
    PauseSound { src: String },
    /// Stop a sound.
    StopSound { src: String },
    /// Ramp sound volume.
    SetSoundVolume { src: String, volume: f64, ramp_ms: u64 },
    /// Trigger haptic / system vibration.
    Vibrate { pattern_ms: Vec<u64> },
    /// Copy text to clipboard.
    CopyToClipboard { text: String },
    /// Send analytics event to tracking backend.
    TrackEvent { category: String, action: String, label: Option<String>, value: Option<f64> },
    /// Emit a custom named event to be re-dispatched.
    EmitCustomEvent { name: String, payload: serde_json::Value },
    /// A runtime variable was updated (name → new value).
    VariableUpdated { name: String, value: serde_json::Value },
    /// No-op for actions that need no host notification.
    NoOp,
}

/// CSS class operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClassOp { Add, Remove, Toggle }

// ── Condition evaluator ───────────────────────────────────────────────────────

/// Evaluates a [`TriggerCondition`] against the current runtime state.
pub struct ConditionEvaluator<'a> {
    pub variables: &'a HashMap<String, serde_json::Value>,
    pub viewport_width: u32,
    pub prefers_dark_mode: bool,
    pub prefers_reduced_motion: bool,
}

impl<'a> ConditionEvaluator<'a> {
    pub fn evaluate(&self, condition: &TriggerCondition) -> bool {
        match condition {
            TriggerCondition::Always => true,

            TriggerCondition::VariableEquals { variable, expected } => {
                self.variables.get(variable).map_or(false, |v| v == expected)
            }

            TriggerCondition::VariableTruthy { variable } => {
                match self.variables.get(variable) {
                    Some(serde_json::Value::Bool(b)) => *b,
                    Some(serde_json::Value::Number(n)) => n.as_f64().unwrap_or(0.0) != 0.0,
                    Some(serde_json::Value::String(s)) => !s.is_empty(),
                    Some(serde_json::Value::Null) | None => false,
                    Some(_) => true,
                }
            }

            TriggerCondition::Breakpoint { min_width, max_width } => {
                let w = self.viewport_width;
                min_width.map_or(true, |min| w >= min)
                    && max_width.map_or(true, |max| w <= max)
            }

            TriggerCondition::PrefersReducedMotion => self.prefers_reduced_motion,
            TriggerCondition::PrefersDarkMode       => self.prefers_dark_mode,

            TriggerCondition::Not(inner) => !self.evaluate(inner),
            TriggerCondition::All(list)  => list.iter().all(|c| self.evaluate(c)),
            TriggerCondition::Any(list)  => list.iter().any(|c| self.evaluate(c)),
        }
    }
}

// ── Desktop action executor ───────────────────────────────────────────────────

/// Translates a [`logos_prototyping`] [`Action`] into one or more
/// [`DesktopActionEffect`]s that the host window/render loop can apply.
pub struct DesktopActionExecutor {
    /// Runtime variables shared with [`PreviewSession`].
    pub variables: HashMap<String, serde_json::Value>,
}

impl Default for DesktopActionExecutor {
    fn default() -> Self {
        Self { variables: HashMap::new() }
    }
}

impl DesktopActionExecutor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Execute a single action, returning the host-facing effect(s).
    pub fn execute(&mut self, action: Action) -> Vec<DesktopActionEffect> {
        match action {
            // ── Navigation ────────────────────────────────────────────────────
            Action::NavigateTo { target_id, .. } => {
                vec![DesktopActionEffect::Navigate { target_id }]
            }
            Action::GoBack => vec![DesktopActionEffect::GoBack],
            Action::OpenUrl { url } => vec![DesktopActionEffect::OpenUrl { url }],

            // ── Drawer / Overlay ──────────────────────────────────────────────
            Action::ToggleDrawer { drawer_id } => {
                // Toggle is modelled as ShowOverlay / DismissTopOverlay at the
                // host level; the host tracks open state.
                vec![DesktopActionEffect::ShowOverlay { content_id: drawer_id }]
            }
            Action::SetDrawerState { drawer_id, state } => {
                use logos_prototyping::trigger::DrawerTargetState;
                match state {
                    DrawerTargetState::Open    => vec![DesktopActionEffect::ShowOverlay { content_id: drawer_id }],
                    DrawerTargetState::Closed  => vec![DesktopActionEffect::DismissOverlay { content_id: drawer_id }],
                    DrawerTargetState::Peeking => vec![DesktopActionEffect::ShowOverlay { content_id: drawer_id }],
                }
            }
            Action::ShowOverlay { overlay_config } => {
                vec![DesktopActionEffect::ShowOverlay { content_id: overlay_config.content_id }]
            }
            Action::DismissOverlay { content_id } => {
                vec![DesktopActionEffect::DismissOverlay { content_id }]
            }
            Action::DismissTopOverlay => {
                // No concrete id; the host pops the overlay stack.
                vec![DesktopActionEffect::NoOp]
            }

            // ── State machine ─────────────────────────────────────────────────
            Action::SetState { .. } | Action::AnimateProperty { .. } => {
                // Handled by the embedded PreviewSession; no extra host effect.
                vec![DesktopActionEffect::NoOp]
            }

            // ── Sequence ──────────────────────────────────────────────────────
            Action::Sequence(actions) => {
                actions.into_iter().flat_map(|a| self.execute(a)).collect()
            }

            // ── Sound ─────────────────────────────────────────────────────────
            Action::PlaySound { src, volume, looping } => {
                vec![DesktopActionEffect::PlaySound { src, volume, looping }]
            }
            Action::PauseSound { src } => {
                vec![DesktopActionEffect::PauseSound { src }]
            }
            Action::StopSound { src } => {
                vec![DesktopActionEffect::StopSound { src }]
            }
            Action::SetVolume { src, volume, ramp_ms } => {
                vec![DesktopActionEffect::SetSoundVolume { src, volume, ramp_ms }]
            }

            // ── Animation ─────────────────────────────────────────────────────
            Action::StartAnimation { layer_id, animation_name } => {
                vec![DesktopActionEffect::AnimationPlay { layer_id, name: animation_name }]
            }
            Action::StopAnimation { layer_id, animation_name } => {
                vec![DesktopActionEffect::AnimationStop { layer_id, name: animation_name }]
            }
            Action::PauseAnimation { layer_id, animation_name } => {
                vec![DesktopActionEffect::AnimationPause { layer_id, name: animation_name }]
            }
            Action::ResumeAnimation { layer_id, animation_name } => {
                vec![DesktopActionEffect::AnimationResume { layer_id, name: animation_name }]
            }
            Action::SeekAnimation { layer_id, animation_name, time_ms } => {
                vec![DesktopActionEffect::AnimationSeek { layer_id, name: animation_name, time_ms }]
            }

            // ── Layer / Style ─────────────────────────────────────────────────
            Action::SetVisibility { layer_id, visible } => {
                vec![DesktopActionEffect::SetLayerVisible { layer_id, visible }]
            }
            Action::SetOpacity { layer_id, opacity, .. } => {
                vec![DesktopActionEffect::SetLayerOpacity { layer_id, opacity }]
            }
            Action::AddCssClass { layer_id, class_name } => {
                vec![DesktopActionEffect::ModifyLayerClass { layer_id, class_name, op: ClassOp::Add }]
            }
            Action::RemoveCssClass { layer_id, class_name } => {
                vec![DesktopActionEffect::ModifyLayerClass { layer_id, class_name, op: ClassOp::Remove }]
            }
            Action::ToggleCssClass { layer_id, class_name } => {
                vec![DesktopActionEffect::ModifyLayerClass { layer_id, class_name, op: ClassOp::Toggle }]
            }
            Action::SetStyleProperty { layer_id, property, value, .. } => {
                vec![DesktopActionEffect::SetLayerStyle { layer_id, property, value }]
            }

            // ── Navigation / View ─────────────────────────────────────────────
            Action::ScrollTo { layer_id, behavior } => {
                vec![DesktopActionEffect::ScrollToLayer { layer_id, behavior }]
            }
            Action::SetFocus { layer_id } => {
                vec![DesktopActionEffect::FocusLayer { layer_id }]
            }

            // ── Variables ─────────────────────────────────────────────────────
            Action::UpdateVariable { name, value } => {
                self.variables.insert(name.clone(), value.clone());
                vec![DesktopActionEffect::VariableUpdated { name, value }]
            }
            Action::IncrementVariable { name, delta } => {
                let entry = self.variables
                    .entry(name.clone())
                    .or_insert(serde_json::Value::from(0.0));
                let new_val = if let Some(n) = entry.as_f64() {
                    serde_json::Value::from(n + delta)
                } else {
                    serde_json::Value::from(delta)
                };
                *entry = new_val.clone();
                vec![DesktopActionEffect::VariableUpdated { name, value: new_val }]
            }
            Action::ToggleVariable { name } => {
                let entry = self.variables
                    .entry(name.clone())
                    .or_insert(serde_json::Value::Bool(false));
                let new_val = match entry {
                    serde_json::Value::Bool(b) => serde_json::Value::Bool(!*b),
                    _ => serde_json::Value::Bool(true),
                };
                *entry = new_val.clone();
                vec![DesktopActionEffect::VariableUpdated { name, value: new_val }]
            }

            // ── Media ─────────────────────────────────────────────────────────
            Action::PlayMedia  { layer_id } => vec![DesktopActionEffect::MediaPlay  { layer_id }],
            Action::PauseMedia { layer_id } => vec![DesktopActionEffect::MediaPause { layer_id }],
            Action::StopMedia  { layer_id } => vec![DesktopActionEffect::MediaStop  { layer_id }],
            Action::SeekMedia  { layer_id, time_seconds } => {
                vec![DesktopActionEffect::MediaSeek { layer_id, time_s: time_seconds }]
            }
            Action::SetMute { layer_id, muted } => {
                vec![DesktopActionEffect::MediaMute { layer_id, muted }]
            }

            // ── Communication ─────────────────────────────────────────────────
            Action::EmitCustomEvent { name, payload } => {
                vec![DesktopActionEffect::EmitCustomEvent { name, payload }]
            }
            Action::CopyToClipboard { text } => {
                vec![DesktopActionEffect::CopyToClipboard { text }]
            }
            Action::Vibrate { pattern_ms } => {
                vec![DesktopActionEffect::Vibrate { pattern_ms }]
            }
            Action::TrackEvent { category, action, label, value } => {
                vec![DesktopActionEffect::TrackEvent { category, action, label, value }]
            }
        }
    }
}

// ── Interaction dispatcher ────────────────────────────────────────────────────

/// Central dispatcher: maps [`DesktopInputEvent`]s to trigger kinds, finds
/// matching [`InteractionTarget`]s, evaluates guards, then runs actions.
pub struct InteractionDispatcher {
    /// All interaction targets keyed by their layer id.
    pub targets: HashMap<Uuid, InteractionTarget>,
    /// Action executor (also owns the variable store).
    pub executor: DesktopActionExecutor,
    /// Accumulated effects since last [`drain_effects`].
    effects: Vec<DesktopActionEffect>,
    /// Current viewport width (px) for breakpoint conditions.
    pub viewport_width: u32,
    /// OS-level dark mode preference.
    pub prefers_dark_mode: bool,
    /// OS-level reduced-motion preference.
    pub prefers_reduced_motion: bool,
}

impl Default for InteractionDispatcher {
    fn default() -> Self {
        Self {
            targets: HashMap::new(),
            executor: DesktopActionExecutor::new(),
            effects: Vec::new(),
            viewport_width: 1280,
            prefers_dark_mode: false,
            prefers_reduced_motion: false,
        }
    }
}

impl InteractionDispatcher {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an interaction target (replaces any existing one for that layer).
    pub fn register(&mut self, target: InteractionTarget) {
        self.targets.insert(target.layer_id, target);
    }

    /// Remove a target.
    pub fn unregister(&mut self, layer_id: Uuid) {
        self.targets.remove(&layer_id);
    }

    /// Drain all accumulated effects since the last call.
    pub fn drain_effects(&mut self) -> Vec<DesktopActionEffect> {
        std::mem::take(&mut self.effects)
    }

    /// Process a [`DesktopInputEvent`], fire matching triggers, return effects.
    pub fn dispatch(&mut self, event: &DesktopInputEvent) -> Vec<DesktopActionEffect> {
        let (layer_id_opt, trigger_kinds) = self.event_to_triggers(event);

        let evaluator = ConditionEvaluator {
            variables: &self.executor.variables,
            viewport_width: self.viewport_width,
            prefers_dark_mode: self.prefers_dark_mode,
            prefers_reduced_motion: self.prefers_reduced_motion,
        };

        let mut fired: Vec<Action> = Vec::new();

        for kind in &trigger_kinds {
            // Window-level events (no specific layer_id)
            if layer_id_opt.is_none() {
                for target in self.targets.values() {
                    for trigger in target.matching_triggers(kind) {
                        if evaluator.evaluate(&trigger.condition) {
                            fired.push(trigger.action.clone());
                        }
                    }
                }
                continue;
            }

            let layer_id = layer_id_opt.unwrap();
            if let Some(target) = self.targets.get(&layer_id) {
                for trigger in target.matching_triggers(kind) {
                    if evaluator.evaluate(&trigger.condition) {
                        fired.push(trigger.action.clone());
                    }
                }
            }
        }

        let mut frame_effects: Vec<DesktopActionEffect> = Vec::new();
        for action in fired {
            let mut fx = self.executor.execute(action);
            frame_effects.append(&mut fx);
        }
        self.effects.extend(frame_effects.clone());
        frame_effects
    }

    /// Map an input event to (optional layer_id, list of TriggerKind to check).
    fn event_to_triggers(&self, event: &DesktopInputEvent) -> (Option<Uuid>, Vec<TriggerKind>) {
        use logos_prototyping::trigger::SwipeDirection;
        match event {
            // ── Pointer ───────────────────────────────────────────────────────
            DesktopInputEvent::Click       { layer_id } => (Some(*layer_id), vec![TriggerKind::OnClick]),
            DesktopInputEvent::DoubleClick { layer_id } => (Some(*layer_id), vec![TriggerKind::OnDoubleClick]),
            DesktopInputEvent::RightClick  { layer_id } => (Some(*layer_id), vec![TriggerKind::OnContextMenu]),
            DesktopInputEvent::MouseDown   { layer_id } => (Some(*layer_id), vec![TriggerKind::MouseDown]),
            DesktopInputEvent::MouseUp     { layer_id } => (Some(*layer_id), vec![TriggerKind::MouseUp]),
            DesktopInputEvent::HoverEnter  { layer_id } => (Some(*layer_id), vec![TriggerKind::OnHoverEnter]),
            DesktopInputEvent::HoverExit   { layer_id } => (Some(*layer_id), vec![TriggerKind::OnHoverExit]),
            DesktopInputEvent::PointerEnter{ layer_id } => (Some(*layer_id), vec![TriggerKind::OnPointerEnter]),
            DesktopInputEvent::PointerLeave{ layer_id } => (Some(*layer_id), vec![TriggerKind::OnPointerLeave]),
            DesktopInputEvent::PointerMove { layer_id } => (Some(*layer_id), vec![TriggerKind::OnPointerMove]),
            DesktopInputEvent::ContextMenu { layer_id } => (Some(*layer_id), vec![TriggerKind::OnContextMenu]),

            DesktopInputEvent::Drag { layer_id, distance_px } => (
                Some(*layer_id),
                vec![TriggerKind::OnDrag { threshold_px: *distance_px }],
            ),
            DesktopInputEvent::Swipe { layer_id, direction, velocity_px_s } => (
                Some(*layer_id),
                vec![TriggerKind::OnSwipe { direction: *direction, min_velocity: *velocity_px_s }],
            ),
            DesktopInputEvent::LongPress { layer_id, held_ms } => (
                Some(*layer_id),
                vec![TriggerKind::OnLongPress { duration_ms: *held_ms }],
            ),
            DesktopInputEvent::Pinch { layer_id, scale } => (
                Some(*layer_id),
                vec![TriggerKind::OnPinch { scale_threshold: *scale }],
            ),
            DesktopInputEvent::Rotate { layer_id, angle_deg } => (
                Some(*layer_id),
                vec![TriggerKind::OnRotate { angle_deg: *angle_deg }],
            ),

            // ── Keyboard ──────────────────────────────────────────────────────
            DesktopInputEvent::KeyPress { layer_id, key } => (
                Some(*layer_id),
                vec![TriggerKind::OnKeyPress { key: key.clone() }],
            ),
            DesktopInputEvent::KeyRelease { layer_id, key } => (
                Some(*layer_id),
                vec![TriggerKind::OnKeyRelease { key: key.clone() }],
            ),
            DesktopInputEvent::KeyHold { layer_id, key, held_ms } => (
                Some(*layer_id),
                vec![TriggerKind::OnKeyHold { key: key.clone(), hold_ms: *held_ms }],
            ),

            // ── Scroll ────────────────────────────────────────────────────────
            DesktopInputEvent::Scroll { layer_id } => {
                (Some(*layer_id), vec![TriggerKind::OnScroll])
            }
            DesktopInputEvent::WindowScroll => (None, vec![TriggerKind::OnWindowScroll]),

            // ── Focus ─────────────────────────────────────────────────────────
            DesktopInputEvent::Focus { layer_id } => (Some(*layer_id), vec![TriggerKind::OnFocus]),
            DesktopInputEvent::Blur  { layer_id } => (Some(*layer_id), vec![TriggerKind::OnBlur]),

            // ── Lifecycle ─────────────────────────────────────────────────────
            DesktopInputEvent::Mount   { layer_id } => (Some(*layer_id), vec![TriggerKind::OnMount]),
            DesktopInputEvent::Unmount { layer_id } => (Some(*layer_id), vec![TriggerKind::OnUnmount]),

            // ── Viewport ──────────────────────────────────────────────────────
            DesktopInputEvent::WindowResize => (None, vec![TriggerKind::OnWindowResize]),
            DesktopInputEvent::VisibilityChange { visible } => (
                None,
                vec![TriggerKind::OnVisibilityChange { visible: *visible }],
            ),
            DesktopInputEvent::Intersection { layer_id, threshold, entering } => (
                Some(*layer_id),
                vec![TriggerKind::OnIntersection { threshold: *threshold, entering: *entering }],
            ),

            // ── Media ─────────────────────────────────────────────────────────
            DesktopInputEvent::MediaPlay    { layer_id } => (Some(*layer_id), vec![TriggerKind::OnMediaPlay]),
            DesktopInputEvent::MediaPause   { layer_id } => (Some(*layer_id), vec![TriggerKind::OnMediaPause]),
            DesktopInputEvent::MediaEnded   { layer_id } => (Some(*layer_id), vec![TriggerKind::OnMediaEnded]),
            DesktopInputEvent::MediaLoad    { layer_id } => (Some(*layer_id), vec![TriggerKind::OnMediaLoad]),
            DesktopInputEvent::MediaError   { layer_id } => (Some(*layer_id), vec![TriggerKind::OnMediaError]),
            DesktopInputEvent::MediaTimeUpdate { layer_id, current_time_s } => (
                Some(*layer_id),
                vec![TriggerKind::OnMediaTimeUpdate { at_seconds: *current_time_s }],
            ),

            // ── Form / Input ──────────────────────────────────────────────────
            DesktopInputEvent::FormSubmit   { layer_id } => (Some(*layer_id), vec![TriggerKind::OnFormSubmit]),
            DesktopInputEvent::FormReset    { layer_id } => (Some(*layer_id), vec![TriggerKind::OnFormReset]),
            DesktopInputEvent::InputChange  { layer_id } => (Some(*layer_id), vec![TriggerKind::OnInputChange]),
            DesktopInputEvent::InputValid   { layer_id } => (Some(*layer_id), vec![TriggerKind::OnInputValid]),
            DesktopInputEvent::InputInvalid { layer_id } => (Some(*layer_id), vec![TriggerKind::OnInputInvalid]),

            // ── Network / data ────────────────────────────────────────────────
            DesktopInputEvent::DataLoaded { layer_id, source_id } => (
                Some(*layer_id),
                vec![TriggerKind::OnDataLoaded { source_id: source_id.clone() }],
            ),
            DesktopInputEvent::DataError { layer_id, source_id } => (
                Some(*layer_id),
                vec![TriggerKind::OnDataError { source_id: source_id.clone() }],
            ),

            // ── Gamepad ───────────────────────────────────────────────────────
            DesktopInputEvent::Gamepad { layer_id, button } => (
                Some(*layer_id),
                vec![TriggerKind::Gamepad { button: button.clone() }],
            ),

            // ── Custom ────────────────────────────────────────────────────────
            DesktopInputEvent::CustomEvent { layer_id, name } => (
                Some(*layer_id),
                vec![TriggerKind::OnCustomEvent { name: name.clone() }],
            ),

            // ── Timed ─────────────────────────────────────────────────────────
            DesktopInputEvent::Delay { layer_id, elapsed_ms } => (
                Some(*layer_id),
                vec![TriggerKind::OnDelay { delay_ms: *elapsed_ms }],
            ),
            DesktopInputEvent::AfterDelay { layer_id, elapsed_ms } => (
                Some(*layer_id),
                vec![TriggerKind::AfterDelay { delay_ms: *elapsed_ms }],
            ),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use logos_prototyping::trigger::{Action, Trigger, TriggerKind, TriggerCondition};

    fn make_dispatcher() -> InteractionDispatcher {
        InteractionDispatcher::new()
    }

    // ── Registration ─────────────────────────────────────────────────

    // DI-01: Registering a target makes it retrievable.
    #[test]
    fn di_01_register_target() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        let target = InteractionTarget::new(layer)
            .with_trigger(Trigger::new(TriggerKind::OnClick, Action::GoBack));
        d.register(target);
        assert!(d.targets.contains_key(&layer));
    }

    // DI-02: Unregistering removes the target.
    #[test]
    fn di_02_unregister_target() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer));
        d.unregister(layer);
        assert!(!d.targets.contains_key(&layer));
    }

    // DI-03: No targets → dispatch returns empty effects.
    #[test]
    fn di_03_no_targets_empty_effects() {
        let mut d = make_dispatcher();
        let effects = d.dispatch(&DesktopInputEvent::Click { layer_id: Uuid::new_v4() });
        assert!(effects.is_empty());
    }

    // ── Click / navigation ────────────────────────────────────────────

    // DI-04: Click on layer with OnClick → GoBack → GoBack effect.
    #[test]
    fn di_04_click_go_back() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer)
            .with_trigger(Trigger::new(TriggerKind::OnClick, Action::GoBack)));
        let effects = d.dispatch(&DesktopInputEvent::Click { layer_id: layer });
        assert_eq!(effects, vec![DesktopActionEffect::GoBack]);
    }

    // DI-05: Click on wrong layer fires nothing.
    #[test]
    fn di_05_click_wrong_layer_fires_nothing() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer)
            .with_trigger(Trigger::new(TriggerKind::OnClick, Action::GoBack)));
        let effects = d.dispatch(&DesktopInputEvent::Click { layer_id: Uuid::new_v4() });
        assert!(effects.is_empty());
    }

    // DI-06: Click → NavigateTo produces Navigate effect.
    #[test]
    fn di_06_click_navigate() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        let dest = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnClick,
            Action::NavigateTo { target_id: dest, animation: None },
        )));
        let effects = d.dispatch(&DesktopInputEvent::Click { layer_id: layer });
        assert_eq!(effects, vec![DesktopActionEffect::Navigate { target_id: dest }]);
    }

    // ── Hover events ──────────────────────────────────────────────────

    // DI-07: HoverEnter fires OnHoverEnter.
    #[test]
    fn di_07_hover_enter() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnHoverEnter,
            Action::SetVisibility { layer_id: layer, visible: false },
        )));
        let effects = d.dispatch(&DesktopInputEvent::HoverEnter { layer_id: layer });
        assert_eq!(effects, vec![DesktopActionEffect::SetLayerVisible { layer_id: layer, visible: false }]);
    }

    // DI-08: HoverExit fires OnHoverExit, not OnHoverEnter.
    #[test]
    fn di_08_hover_exit_not_enter() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnHoverEnter,
            Action::GoBack,
        )));
        let effects = d.dispatch(&DesktopInputEvent::HoverExit { layer_id: layer });
        assert!(effects.is_empty());
    }

    // ── Mouse down / up ───────────────────────────────────────────────

    // DI-09: MouseDown fires MouseDown trigger.
    #[test]
    fn di_09_mouse_down() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::MouseDown,
            Action::GoBack,
        )));
        let effects = d.dispatch(&DesktopInputEvent::MouseDown { layer_id: layer });
        assert_eq!(effects, vec![DesktopActionEffect::GoBack]);
    }

    // DI-10: MouseUp fires MouseUp trigger.
    #[test]
    fn di_10_mouse_up() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::MouseUp,
            Action::GoBack,
        )));
        let effects = d.dispatch(&DesktopInputEvent::MouseUp { layer_id: layer });
        assert_eq!(effects, vec![DesktopActionEffect::GoBack]);
    }

    // ── Pointer ───────────────────────────────────────────────────────

    // DI-11: PointerEnter fires OnPointerEnter.
    #[test]
    fn di_11_pointer_enter() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnPointerEnter, Action::GoBack,
        )));
        let e = d.dispatch(&DesktopInputEvent::PointerEnter { layer_id: layer });
        assert_eq!(e.len(), 1);
    }

    // DI-12: PointerLeave fires OnPointerLeave.
    #[test]
    fn di_12_pointer_leave() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnPointerLeave, Action::GoBack,
        )));
        let e = d.dispatch(&DesktopInputEvent::PointerLeave { layer_id: layer });
        assert_eq!(e.len(), 1);
    }

    // DI-13: PointerMove fires OnPointerMove.
    #[test]
    fn di_13_pointer_move() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnPointerMove, Action::GoBack,
        )));
        let e = d.dispatch(&DesktopInputEvent::PointerMove { layer_id: layer });
        assert_eq!(e.len(), 1);
    }

    // ── Keyboard ──────────────────────────────────────────────────────

    // DI-14: KeyPress fires OnKeyPress with matching key.
    #[test]
    fn di_14_key_press() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnKeyPress { key: "Enter".into() }, Action::GoBack,
        )));
        let e = d.dispatch(&DesktopInputEvent::KeyPress { layer_id: layer, key: "Enter".into() });
        assert_eq!(e.len(), 1);
    }

    // DI-15: KeyRelease fires OnKeyRelease.
    #[test]
    fn di_15_key_release() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnKeyRelease { key: "Escape".into() }, Action::GoBack,
        )));
        let e = d.dispatch(&DesktopInputEvent::KeyRelease { layer_id: layer, key: "Escape".into() });
        assert_eq!(e.len(), 1);
    }

    // DI-16: KeyHold fires OnKeyHold.
    #[test]
    fn di_16_key_hold() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnKeyHold { key: "Shift".into(), hold_ms: 1000 }, Action::GoBack,
        )));
        let e = d.dispatch(&DesktopInputEvent::KeyHold { layer_id: layer, key: "Shift".into(), held_ms: 1000 });
        assert_eq!(e.len(), 1);
    }

    // ── Focus / blur ──────────────────────────────────────────────────

    // DI-17: Focus event fires OnFocus.
    #[test]
    fn di_17_focus() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnFocus, Action::GoBack,
        )));
        let e = d.dispatch(&DesktopInputEvent::Focus { layer_id: layer });
        assert_eq!(e.len(), 1);
    }

    // DI-18: Blur event fires OnBlur.
    #[test]
    fn di_18_blur() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnBlur, Action::GoBack,
        )));
        let e = d.dispatch(&DesktopInputEvent::Blur { layer_id: layer });
        assert_eq!(e.len(), 1);
    }

    // ── Lifecycle ─────────────────────────────────────────────────────

    // DI-19: Mount event fires OnMount.
    #[test]
    fn di_19_mount() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnMount, Action::GoBack,
        )));
        let e = d.dispatch(&DesktopInputEvent::Mount { layer_id: layer });
        assert_eq!(e.len(), 1);
    }

    // DI-20: Unmount event fires OnUnmount.
    #[test]
    fn di_20_unmount() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnUnmount, Action::GoBack,
        )));
        let e = d.dispatch(&DesktopInputEvent::Unmount { layer_id: layer });
        assert_eq!(e.len(), 1);
    }

    // ── Window / viewport ─────────────────────────────────────────────

    // DI-21: WindowResize fires OnWindowResize on all registered targets.
    #[test]
    fn di_21_window_resize_fires_all_targets() {
        let mut d = make_dispatcher();
        let l1 = Uuid::new_v4();
        let l2 = Uuid::new_v4();
        d.register(InteractionTarget::new(l1).with_trigger(Trigger::new(TriggerKind::OnWindowResize, Action::GoBack)));
        d.register(InteractionTarget::new(l2).with_trigger(Trigger::new(TriggerKind::OnWindowResize, Action::GoBack)));
        let e = d.dispatch(&DesktopInputEvent::WindowResize);
        assert_eq!(e.len(), 2);
    }

    // DI-22: WindowScroll fires OnWindowScroll.
    #[test]
    fn di_22_window_scroll() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnWindowScroll, Action::GoBack,
        )));
        let e = d.dispatch(&DesktopInputEvent::WindowScroll);
        assert_eq!(e.len(), 1);
    }

    // DI-23: VisibilityChange hidden fires OnVisibilityChange { visible:false }.
    #[test]
    fn di_23_visibility_change_hidden() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnVisibilityChange { visible: false }, Action::GoBack,
        )));
        let e = d.dispatch(&DesktopInputEvent::VisibilityChange { visible: false });
        assert_eq!(e.len(), 1);
    }

    // DI-24: Intersection (entering) fires OnIntersection.
    #[test]
    fn di_24_intersection_entering() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnIntersection { threshold: 0.5, entering: true }, Action::GoBack,
        )));
        let e = d.dispatch(&DesktopInputEvent::Intersection { layer_id: layer, threshold: 0.5, entering: true });
        assert_eq!(e.len(), 1);
    }

    // ── Media ─────────────────────────────────────────────────────────

    // DI-25: MediaPlay fires OnMediaPlay.
    #[test]
    fn di_25_media_play() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnMediaPlay, Action::GoBack,
        )));
        let e = d.dispatch(&DesktopInputEvent::MediaPlay { layer_id: layer });
        assert_eq!(e.len(), 1);
    }

    // DI-26: MediaEnded → PlaySound effect.
    #[test]
    fn di_26_media_ended_play_sound() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnMediaEnded,
            Action::PlaySound { src: "end.mp3".into(), volume: 1.0, looping: false },
        )));
        let e = d.dispatch(&DesktopInputEvent::MediaEnded { layer_id: layer });
        assert_eq!(e, vec![DesktopActionEffect::PlaySound { src: "end.mp3".into(), volume: 1.0, looping: false }]);
    }

    // DI-27: MediaTimeUpdate fires OnMediaTimeUpdate.
    #[test]
    fn di_27_media_time_update() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnMediaTimeUpdate { at_seconds: 5.0 }, Action::GoBack,
        )));
        let e = d.dispatch(&DesktopInputEvent::MediaTimeUpdate { layer_id: layer, current_time_s: 5.0 });
        assert_eq!(e.len(), 1);
    }

    // ── Form / input ──────────────────────────────────────────────────

    // DI-28: FormSubmit fires OnFormSubmit.
    #[test]
    fn di_28_form_submit() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnFormSubmit, Action::GoBack,
        )));
        let e = d.dispatch(&DesktopInputEvent::FormSubmit { layer_id: layer });
        assert_eq!(e.len(), 1);
    }

    // DI-29: InputChange fires OnInputChange.
    #[test]
    fn di_29_input_change() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnInputChange, Action::GoBack,
        )));
        let e = d.dispatch(&DesktopInputEvent::InputChange { layer_id: layer });
        assert_eq!(e.len(), 1);
    }

    // DI-30: InputInvalid fires OnInputInvalid.
    #[test]
    fn di_30_input_invalid() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnInputInvalid, Action::GoBack,
        )));
        let e = d.dispatch(&DesktopInputEvent::InputInvalid { layer_id: layer });
        assert_eq!(e.len(), 1);
    }

    // ── Network / data ────────────────────────────────────────────────

    // DI-31: DataLoaded fires OnDataLoaded.
    #[test]
    fn di_31_data_loaded() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnDataLoaded { source_id: "api".into() }, Action::GoBack,
        )));
        let e = d.dispatch(&DesktopInputEvent::DataLoaded { layer_id: layer, source_id: "api".into() });
        assert_eq!(e.len(), 1);
    }

    // DI-32: DataError fires OnDataError.
    #[test]
    fn di_32_data_error() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnDataError { source_id: "api".into() }, Action::GoBack,
        )));
        let e = d.dispatch(&DesktopInputEvent::DataError { layer_id: layer, source_id: "api".into() });
        assert_eq!(e.len(), 1);
    }

    // ── Gamepad ───────────────────────────────────────────────────────

    // DI-33: Gamepad button A fires Gamepad { button: "A" } trigger.
    #[test]
    fn di_33_gamepad_button() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::Gamepad { button: "A".into() }, Action::GoBack,
        )));
        let e = d.dispatch(&DesktopInputEvent::Gamepad { layer_id: layer, button: "A".into() });
        assert_eq!(e.len(), 1);
    }

    // ── Custom event ──────────────────────────────────────────────────

    // DI-34: CustomEvent fires OnCustomEvent.
    #[test]
    fn di_34_custom_event() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnCustomEvent { name: "my-event".into() },
            Action::GoBack,
        )));
        let e = d.dispatch(&DesktopInputEvent::CustomEvent { layer_id: layer, name: "my-event".into() });
        assert_eq!(e.len(), 1);
    }

    // ── Guard conditions ──────────────────────────────────────────────

    // DI-35: Always condition passes — action fires.
    #[test]
    fn di_35_condition_always_fires() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(
            Trigger::new(TriggerKind::OnClick, Action::GoBack)
                .with_condition(TriggerCondition::Always),
        ));
        let e = d.dispatch(&DesktopInputEvent::Click { layer_id: layer });
        assert_eq!(e.len(), 1);
    }

    // DI-36: VariableTruthy condition blocks action when variable is false.
    #[test]
    fn di_36_condition_variable_truthy_blocks_when_false() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(
            Trigger::new(TriggerKind::OnClick, Action::GoBack)
                .with_condition(TriggerCondition::VariableTruthy { variable: "enabled".into() }),
        ));
        // variable not set → falsy
        let e = d.dispatch(&DesktopInputEvent::Click { layer_id: layer });
        assert!(e.is_empty());
    }

    // DI-37: VariableTruthy passes when variable is set to true.
    #[test]
    fn di_37_condition_variable_truthy_passes_when_true() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.executor.variables.insert("enabled".into(), serde_json::Value::Bool(true));
        d.register(InteractionTarget::new(layer).with_trigger(
            Trigger::new(TriggerKind::OnClick, Action::GoBack)
                .with_condition(TriggerCondition::VariableTruthy { variable: "enabled".into() }),
        ));
        let e = d.dispatch(&DesktopInputEvent::Click { layer_id: layer });
        assert_eq!(e.len(), 1);
    }

    // DI-38: PrefersReducedMotion condition blocks when flag is false.
    #[test]
    fn di_38_prefers_reduced_motion_blocks() {
        let mut d = make_dispatcher();
        d.prefers_reduced_motion = false;
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(
            Trigger::new(TriggerKind::OnClick, Action::GoBack)
                .with_condition(TriggerCondition::PrefersReducedMotion),
        ));
        let e = d.dispatch(&DesktopInputEvent::Click { layer_id: layer });
        assert!(e.is_empty());
    }

    // DI-39: PrefersDarkMode condition passes when flag is true.
    #[test]
    fn di_39_prefers_dark_mode_passes() {
        let mut d = make_dispatcher();
        d.prefers_dark_mode = true;
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(
            Trigger::new(TriggerKind::OnClick, Action::GoBack)
                .with_condition(TriggerCondition::PrefersDarkMode),
        ));
        let e = d.dispatch(&DesktopInputEvent::Click { layer_id: layer });
        assert_eq!(e.len(), 1);
    }

    // DI-40: Breakpoint condition respects viewport_width.
    #[test]
    fn di_40_breakpoint_condition() {
        let mut d = make_dispatcher();
        d.viewport_width = 1024;
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(
            Trigger::new(TriggerKind::OnClick, Action::GoBack)
                .with_condition(TriggerCondition::Breakpoint { min_width: Some(768), max_width: Some(1280) }),
        ));
        let e = d.dispatch(&DesktopInputEvent::Click { layer_id: layer });
        assert_eq!(e.len(), 1);
    }

    // DI-41: Breakpoint condition blocks outside viewport width.
    #[test]
    fn di_41_breakpoint_condition_blocks() {
        let mut d = make_dispatcher();
        d.viewport_width = 400;
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(
            Trigger::new(TriggerKind::OnClick, Action::GoBack)
                .with_condition(TriggerCondition::Breakpoint { min_width: Some(768), max_width: None }),
        ));
        let e = d.dispatch(&DesktopInputEvent::Click { layer_id: layer });
        assert!(e.is_empty());
    }

    // DI-42: Not condition inverts guard.
    #[test]
    fn di_42_not_condition_inverts() {
        let mut d = make_dispatcher();
        d.prefers_dark_mode = false;
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(
            Trigger::new(TriggerKind::OnClick, Action::GoBack)
                .with_condition(TriggerCondition::Not(Box::new(TriggerCondition::PrefersDarkMode))),
        ));
        let e = d.dispatch(&DesktopInputEvent::Click { layer_id: layer });
        assert_eq!(e.len(), 1);
    }

    // ── Action effects ────────────────────────────────────────────────

    // DI-43: PlaySound action produces PlaySound effect.
    #[test]
    fn di_43_play_sound_effect() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnClick,
            Action::PlaySound { src: "click.mp3".into(), volume: 0.8, looping: false },
        )));
        let e = d.dispatch(&DesktopInputEvent::Click { layer_id: layer });
        assert_eq!(e, vec![DesktopActionEffect::PlaySound { src: "click.mp3".into(), volume: 0.8, looping: false }]);
    }

    // DI-44: SetVisibility action produces SetLayerVisible effect.
    #[test]
    fn di_44_set_visibility_effect() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        let target_layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnClick,
            Action::SetVisibility { layer_id: target_layer, visible: true },
        )));
        let e = d.dispatch(&DesktopInputEvent::Click { layer_id: layer });
        assert_eq!(e, vec![DesktopActionEffect::SetLayerVisible { layer_id: target_layer, visible: true }]);
    }

    // DI-45: AddCssClass action produces ModifyLayerClass(Add) effect.
    #[test]
    fn di_45_add_css_class_effect() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnClick,
            Action::AddCssClass { layer_id: layer, class_name: "active".into() },
        )));
        let e = d.dispatch(&DesktopInputEvent::Click { layer_id: layer });
        assert_eq!(e, vec![DesktopActionEffect::ModifyLayerClass { layer_id: layer, class_name: "active".into(), op: ClassOp::Add }]);
    }

    // DI-46: ToggleCssClass action produces ModifyLayerClass(Toggle) effect.
    #[test]
    fn di_46_toggle_css_class_effect() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnClick,
            Action::ToggleCssClass { layer_id: layer, class_name: "open".into() },
        )));
        let e = d.dispatch(&DesktopInputEvent::Click { layer_id: layer });
        assert_eq!(e, vec![DesktopActionEffect::ModifyLayerClass { layer_id: layer, class_name: "open".into(), op: ClassOp::Toggle }]);
    }

    // DI-47: UpdateVariable stores the value and produces VariableUpdated.
    #[test]
    fn di_47_update_variable_stores_and_emits() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnClick,
            Action::UpdateVariable { name: "count".into(), value: serde_json::json!(5) },
        )));
        let e = d.dispatch(&DesktopInputEvent::Click { layer_id: layer });
        assert_eq!(e, vec![DesktopActionEffect::VariableUpdated { name: "count".into(), value: serde_json::json!(5) }]);
        assert_eq!(d.executor.variables.get("count"), Some(&serde_json::json!(5)));
    }

    // DI-48: IncrementVariable increments from zero and emits new value.
    #[test]
    fn di_48_increment_variable() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnClick,
            Action::IncrementVariable { name: "hits".into(), delta: 1.0 },
        )));
        d.dispatch(&DesktopInputEvent::Click { layer_id: layer });
        d.dispatch(&DesktopInputEvent::Click { layer_id: layer });
        let val = d.executor.variables.get("hits").and_then(|v| v.as_f64());
        assert_eq!(val, Some(2.0));
    }

    // DI-49: ToggleVariable flips boolean after each click.
    #[test]
    fn di_49_toggle_variable() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnClick,
            Action::ToggleVariable { name: "open".into() },
        )));
        d.dispatch(&DesktopInputEvent::Click { layer_id: layer });
        assert_eq!(d.executor.variables.get("open"), Some(&serde_json::Value::Bool(true)));
        d.dispatch(&DesktopInputEvent::Click { layer_id: layer });
        assert_eq!(d.executor.variables.get("open"), Some(&serde_json::Value::Bool(false)));
    }

    // DI-50: EmitCustomEvent produces EmitCustomEvent effect.
    #[test]
    fn di_50_emit_custom_event_effect() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnClick,
            Action::EmitCustomEvent { name: "done".into(), payload: serde_json::Value::Null },
        )));
        let e = d.dispatch(&DesktopInputEvent::Click { layer_id: layer });
        assert_eq!(e, vec![DesktopActionEffect::EmitCustomEvent { name: "done".into(), payload: serde_json::Value::Null }]);
    }

    // DI-51: Sequence action fires all sub-actions in order.
    #[test]
    fn di_51_sequence_action() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        let media_layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnClick,
            Action::Sequence(vec![
                Action::PlaySound { src: "ding.mp3".into(), volume: 1.0, looping: false },
                Action::PlayMedia { layer_id: media_layer },
            ]),
        )));
        let e = d.dispatch(&DesktopInputEvent::Click { layer_id: layer });
        assert_eq!(e.len(), 2);
        assert_eq!(e[0], DesktopActionEffect::PlaySound { src: "ding.mp3".into(), volume: 1.0, looping: false });
        assert_eq!(e[1], DesktopActionEffect::MediaPlay { layer_id: media_layer });
    }

    // DI-52: drain_effects clears the internal buffer.
    #[test]
    fn di_52_drain_effects_clears_buffer() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnClick, Action::GoBack,
        )));
        d.dispatch(&DesktopInputEvent::Click { layer_id: layer });
        let first  = d.drain_effects();
        let second = d.drain_effects();
        assert_eq!(first.len(),  1);
        assert_eq!(second.len(), 0);
    }

    // DI-53: Disabled trigger is not fired.
    #[test]
    fn di_53_disabled_trigger_not_fired() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(
            Trigger::new(TriggerKind::OnClick, Action::GoBack).disabled(),
        ));
        let e = d.dispatch(&DesktopInputEvent::Click { layer_id: layer });
        assert!(e.is_empty());
    }

    // DI-54: StartAnimation produces AnimationPlay effect.
    #[test]
    fn di_54_start_animation_effect() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        let anim_layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnClick,
            Action::StartAnimation { layer_id: anim_layer, animation_name: "pulse".into() },
        )));
        let e = d.dispatch(&DesktopInputEvent::Click { layer_id: layer });
        assert_eq!(e, vec![DesktopActionEffect::AnimationPlay { layer_id: anim_layer, name: "pulse".into() }]);
    }

    // DI-55: SeekMedia produces MediaSeek effect.
    #[test]
    fn di_55_seek_media_effect() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        let media = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnClick,
            Action::SeekMedia { layer_id: media, time_seconds: 30.0 },
        )));
        let e = d.dispatch(&DesktopInputEvent::Click { layer_id: layer });
        assert_eq!(e, vec![DesktopActionEffect::MediaSeek { layer_id: media, time_s: 30.0 }]);
    }

    // DI-56: SetMute produces MediaMute effect.
    #[test]
    fn di_56_set_mute_effect() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        let media = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnClick,
            Action::SetMute { layer_id: media, muted: true },
        )));
        let e = d.dispatch(&DesktopInputEvent::Click { layer_id: layer });
        assert_eq!(e, vec![DesktopActionEffect::MediaMute { layer_id: media, muted: true }]);
    }

    // DI-57: CopyToClipboard produces CopyToClipboard effect.
    #[test]
    fn di_57_copy_to_clipboard_effect() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnClick,
            Action::CopyToClipboard { text: "Hello".into() },
        )));
        let e = d.dispatch(&DesktopInputEvent::Click { layer_id: layer });
        assert_eq!(e, vec![DesktopActionEffect::CopyToClipboard { text: "Hello".into() }]);
    }

    // DI-58: Vibrate produces Vibrate effect.
    #[test]
    fn di_58_vibrate_effect() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnClick,
            Action::Vibrate { pattern_ms: vec![100, 50, 100] },
        )));
        let e = d.dispatch(&DesktopInputEvent::Click { layer_id: layer });
        assert_eq!(e, vec![DesktopActionEffect::Vibrate { pattern_ms: vec![100, 50, 100] }]);
    }

    // DI-59: TrackEvent produces TrackEvent effect.
    #[test]
    fn di_59_track_event_effect() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnClick,
            Action::TrackEvent { category: "ui".into(), action: "click".into(), label: None, value: None },
        )));
        let e = d.dispatch(&DesktopInputEvent::Click { layer_id: layer });
        assert_eq!(e, vec![DesktopActionEffect::TrackEvent { category: "ui".into(), action: "click".into(), label: None, value: None }]);
    }

    // DI-60: ScrollTo produces ScrollToLayer effect.
    #[test]
    fn di_60_scroll_to_effect() {
        let mut d = make_dispatcher();
        let layer = Uuid::new_v4();
        let target = Uuid::new_v4();
        d.register(InteractionTarget::new(layer).with_trigger(Trigger::new(
            TriggerKind::OnClick,
            Action::ScrollTo { layer_id: target, behavior: ScrollBehavior::Smooth },
        )));
        let e = d.dispatch(&DesktopInputEvent::Click { layer_id: layer });
        assert_eq!(e, vec![DesktopActionEffect::ScrollToLayer { layer_id: target, behavior: ScrollBehavior::Smooth }]);
    }

    // ─── ConditionEvaluator standalone ───────────────────────────────

    // DI-61: VariableEquals passes when equal.
    #[test]
    fn di_61_condition_variable_equals_passes() {
        let mut vars = HashMap::new();
        vars.insert("role".to_string(), serde_json::json!("admin"));
        let eval = ConditionEvaluator { variables: &vars, viewport_width: 1280, prefers_dark_mode: false, prefers_reduced_motion: false };
        let c = TriggerCondition::VariableEquals { variable: "role".into(), expected: serde_json::json!("admin") };
        assert!(eval.evaluate(&c));
    }

    // DI-62: VariableEquals fails when not equal.
    #[test]
    fn di_62_condition_variable_equals_fails() {
        let vars = HashMap::new();
        let eval = ConditionEvaluator { variables: &vars, viewport_width: 1280, prefers_dark_mode: false, prefers_reduced_motion: false };
        let c = TriggerCondition::VariableEquals { variable: "role".into(), expected: serde_json::json!("admin") };
        assert!(!eval.evaluate(&c));
    }

    // DI-63: All condition requires every inner to pass.
    #[test]
    fn di_63_all_condition() {
        let vars = HashMap::new();
        let eval = ConditionEvaluator { variables: &vars, viewport_width: 1280, prefers_dark_mode: true, prefers_reduced_motion: false };
        let c = TriggerCondition::All(vec![TriggerCondition::PrefersDarkMode, TriggerCondition::Always]);
        assert!(eval.evaluate(&c));
    }

    // DI-64: Any condition passes if any inner passes.
    #[test]
    fn di_64_any_condition() {
        let vars = HashMap::new();
        let eval = ConditionEvaluator { variables: &vars, viewport_width: 1280, prefers_dark_mode: false, prefers_reduced_motion: false };
        let c = TriggerCondition::Any(vec![TriggerCondition::PrefersDarkMode, TriggerCondition::Always]);
        assert!(eval.evaluate(&c));
    }

    // DI-65: Nested Not(All([PrefersDarkMode, Always])) blocks when dark mode off.
    #[test]
    fn di_65_nested_not_all() {
        let vars = HashMap::new();
        let eval = ConditionEvaluator { variables: &vars, viewport_width: 1280, prefers_dark_mode: false, prefers_reduced_motion: false };
        let c = TriggerCondition::Not(Box::new(
            TriggerCondition::All(vec![TriggerCondition::PrefersDarkMode, TriggerCondition::Always])
        ));
        assert!(eval.evaluate(&c)); // Not(All([false, true])) = Not(false) = true
    }
}
