//! # Preview Mode
//!
//! Runtime engine that drives interactive prototyping. A [`PreviewSession`]
//! takes a snapshot of the document, maintains per-container state machine
//! positions, processes user events, and advances animations.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::animate::{AnimationValue, PropertyAnimation};
use crate::flow::FlowGraph;
use crate::overlay::{ActiveOverlay, OverlayStack, DismissReason};
use crate::scroll::{ScrollConfig, ScrollState};
use crate::state_machine::{StateMachine, StateId};
use crate::timeline::Timeline;
use crate::trigger::{Action, DrawerTargetState, InteractionTarget, TriggerKind};

// ── Preview State ────────────────────────────────────────────────────

/// The runtime state of the preview session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PreviewState {
    /// Waiting to start.
    Idle,
    /// Actively running.
    Playing,
    /// Paused (animations frozen).
    Paused,
    /// Preview ended.
    Stopped,
}

impl Default for PreviewState {
    fn default() -> Self {
        Self::Idle
    }
}

// ── Active Animation ─────────────────────────────────────────────────

/// An animation currently in flight during preview.
#[derive(Debug, Clone)]
pub struct ActiveAnimation {
    pub layer_id: Uuid,
    pub animation: PropertyAnimation,
    /// When the animation started (ms since preview start).
    pub start_time_ms: u64,
}

impl ActiveAnimation {
    pub fn new(layer_id: Uuid, animation: PropertyAnimation, start_time_ms: u64) -> Self {
        Self {
            layer_id,
            animation,
            start_time_ms,
        }
    }

    /// Evaluate the animation at the given absolute preview time.
    pub fn evaluate(&self, current_time_ms: u64) -> Option<AnimationValue> {
        if current_time_ms < self.start_time_ms {
            return None;
        }
        let elapsed = current_time_ms - self.start_time_ms;
        self.animation.evaluate(elapsed)
    }

    /// Is this animation complete?
    pub fn is_complete(&self, current_time_ms: u64) -> bool {
        if current_time_ms < self.start_time_ms {
            return false;
        }
        self.animation.is_complete(current_time_ms - self.start_time_ms)
    }
}

// ── Navigation Stack ─────────────────────────────────────────────────

/// Entry in the navigation history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NavigationEntry {
    pub screen_id: Uuid,
    pub timestamp_ms: u64,
}

// ── Preview Event ────────────────────────────────────────────────────

/// Events that the preview session can emit to the host.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PreviewEvent {
    /// State machine transitioned.
    StateChanged {
        machine_owner: Uuid,
        from: StateId,
        to: StateId,
    },
    /// Navigated to a different screen.
    Navigated {
        from: Uuid,
        to: Uuid,
    },
    /// Animation started.
    AnimationStarted {
        layer_id: Uuid,
        property: String,
    },
    /// Animation completed.
    AnimationCompleted {
        layer_id: Uuid,
        property: String,
    },
    /// Drawer toggled.
    DrawerToggled {
        drawer_id: Uuid,
        new_state: DrawerTargetState,
    },
    /// Preview session state changed.
    SessionStateChanged(PreviewState),
    /// Interaction triggered.
    InteractionFired {
        layer_id: Uuid,
        trigger: TriggerKind,
    },
    /// Overlay shown.
    OverlayShown {
        overlay_id: Uuid,
        content_id: Uuid,
        kind: crate::overlay::OverlayKind,
    },
    /// Overlay dismissed.
    OverlayDismissed {
        overlay_id: Uuid,
        content_id: Uuid,
        reason: crate::overlay::DismissReason,
    },
    /// Scroll position changed.
    ScrollChanged {
        container_id: Uuid,
        offset_x: f64,
        offset_y: f64,
    },
    /// A generic string notification from a new action type.
    CustomNotification {
        message: String,
    },
}

// ── Preview Session ──────────────────────────────────────────────────

/// The main preview session that orchestrates interactive prototyping.
#[derive(Debug)]
pub struct PreviewSession {
    /// Current state of the session.
    pub state: PreviewState,
    /// State machines keyed by their owner (container) id.
    pub state_machines: HashMap<Uuid, StateMachine>,
    /// Interaction targets keyed by layer id.
    pub interaction_targets: HashMap<Uuid, InteractionTarget>,
    /// Active timelines.
    pub timelines: Vec<Timeline>,
    /// Currently running animations.
    pub active_animations: Vec<ActiveAnimation>,
    /// The currently visible screen (artboard / frame).
    pub current_screen: Option<Uuid>,
    /// Navigation history stack.
    pub navigation_stack: Vec<NavigationEntry>,
    /// Drawer states (drawer_id → target state).
    pub drawer_states: HashMap<Uuid, DrawerTargetState>,
    /// Flow graph for navigation visualisation.
    pub flow_graph: Option<FlowGraph>,
    /// Scroll states keyed by container id.
    pub scroll_states: HashMap<Uuid, ScrollState>,
    /// Active overlay stack (z-ordered, last = topmost).
    pub overlay_stack: OverlayStack,
    /// Accumulated events since last flush.
    events: Vec<PreviewEvent>,
    /// Runtime variables (name → JSON value).
    pub variables: HashMap<String, serde_json::Value>,
    /// Current preview time in ms.
    pub time_ms: u64,
    /// Starting screen id.
    pub start_screen: Option<Uuid>,
}

impl PreviewSession {
    pub fn new() -> Self {
        Self {
            state: PreviewState::Idle,
            state_machines: HashMap::new(),
            interaction_targets: HashMap::new(),
            timelines: Vec::new(),
            active_animations: Vec::new(),
            current_screen: None,
            navigation_stack: Vec::new(),
            drawer_states: HashMap::new(),
            flow_graph: None,
            scroll_states: HashMap::new(),
            overlay_stack: OverlayStack::new(),
            events: Vec::new(),
            variables: HashMap::new(),
            time_ms: 0,
            start_screen: None,
        }
    }

    // ── Setup ────────────────────────────────────────────────────

    /// Register a state machine for a container.
    pub fn add_state_machine(&mut self, sm: StateMachine) {
        self.state_machines.insert(sm.owner_id, sm);
    }

    /// Register an interaction target.
    pub fn add_interaction_target(&mut self, target: InteractionTarget) {
        self.interaction_targets.insert(target.layer_id, target);
    }

    /// Add a timeline.
    pub fn add_timeline(&mut self, timeline: Timeline) {
        self.timelines.push(timeline);
    }

    /// Set the starting screen.
    pub fn set_start_screen(&mut self, screen_id: Uuid) {
        self.start_screen = Some(screen_id);
    }

    /// Set the flow graph.
    pub fn set_flow_graph(&mut self, graph: FlowGraph) {
        self.flow_graph = Some(graph);
    }

    // ── Lifecycle ────────────────────────────────────────────────

    /// Start the preview session.
    pub fn start(&mut self) {
        self.state = PreviewState::Playing;
        self.time_ms = 0;
        self.current_screen = self.start_screen;
        self.navigation_stack.clear();
        self.active_animations.clear();
        self.drawer_states.clear();
        self.overlay_stack.dismiss_all();
        // Reset scroll positions to zero.
        for state in self.scroll_states.values_mut() {
            state.offset_x = 0.0;
            state.offset_y = 0.0;
            state.velocity_x = 0.0;
            state.velocity_y = 0.0;
        }

        // Reset all state machines to their default.
        for sm in self.state_machines.values_mut() {
            sm.reset();
        }

        // Push starting screen to nav stack.
        if let Some(screen) = self.current_screen {
            self.navigation_stack.push(NavigationEntry {
                screen_id: screen,
                timestamp_ms: 0,
            });
        }

        // Start autoplay timelines.
        for tl in &self.timelines {
            if tl.autoplay {
                self.events.push(PreviewEvent::AnimationStarted {
                    layer_id: tl.target_layer_id,
                    property: tl.animated_properties().join(","),
                });
            }
        }

        self.events
            .push(PreviewEvent::SessionStateChanged(PreviewState::Playing));
    }

    /// Pause the preview.
    pub fn pause(&mut self) {
        if self.state == PreviewState::Playing {
            self.state = PreviewState::Paused;
            self.events
                .push(PreviewEvent::SessionStateChanged(PreviewState::Paused));
        }
    }

    /// Resume from pause.
    pub fn resume(&mut self) {
        if self.state == PreviewState::Paused {
            self.state = PreviewState::Playing;
            self.events
                .push(PreviewEvent::SessionStateChanged(PreviewState::Playing));
        }
    }

    /// Stop the preview session.
    pub fn stop(&mut self) {
        self.state = PreviewState::Stopped;
        self.active_animations.clear();
        self.events
            .push(PreviewEvent::SessionStateChanged(PreviewState::Stopped));
    }

    /// Advance the preview clock by `delta_ms`.
    pub fn tick(&mut self, delta_ms: u64) {
        if self.state != PreviewState::Playing {
            return;
        }
        self.time_ms += delta_ms;

        // Remove completed animations.
        let time = self.time_ms;
        let completed: Vec<(Uuid, String)> = self
            .active_animations
            .iter()
            .filter(|a| a.is_complete(time))
            .map(|a| (a.layer_id, a.animation.property.clone()))
            .collect();

        for (layer_id, property) in &completed {
            self.events.push(PreviewEvent::AnimationCompleted {
                layer_id: *layer_id,
                property: property.clone(),
            });
        }

        self.active_animations.retain(|a| !a.is_complete(time));
    }

    // ── Interaction dispatch ─────────────────────────────────────

    /// Fire an interaction trigger on a specific layer.
    pub fn fire_trigger(&mut self, layer_id: Uuid, kind: &TriggerKind) -> Vec<PreviewEvent> {
        if self.state != PreviewState::Playing {
            return Vec::new();
        }

        self.events.push(PreviewEvent::InteractionFired {
            layer_id,
            trigger: kind.clone(),
        });

        // Check interaction targets.
        if let Some(target) = self.interaction_targets.get(&layer_id) {
            let actions: Vec<Action> = target
                .matching_triggers(kind)
                .iter()
                .map(|t| t.action.clone())
                .collect();
            for action in actions {
                self.execute_action(action);
            }
        }

        // Check state machines for this layer.
        if let Some(sm) = self.state_machines.get_mut(&layer_id) {
            let old_state = sm.current_state();
            if let Some(transition) = sm.fire(kind) {
                let new_state = transition.to;
                if let Some(old) = old_state {
                    self.events.push(PreviewEvent::StateChanged {
                        machine_owner: layer_id,
                        from: old,
                        to: new_state,
                    });
                }
            }
        }

        self.drain_events()
    }

    /// Execute an action.
    fn execute_action(&mut self, action: Action) {
        match action {
            Action::NavigateTo {
                target_id,
                animation: _,
            } => {
                let from = self.current_screen.unwrap_or(Uuid::nil());
                self.current_screen = Some(target_id);
                self.navigation_stack.push(NavigationEntry {
                    screen_id: target_id,
                    timestamp_ms: self.time_ms,
                });
                self.events.push(PreviewEvent::Navigated {
                    from,
                    to: target_id,
                });
            }
            Action::GoBack => {
                if self.navigation_stack.len() > 1 {
                    let from_entry = self.navigation_stack.pop().unwrap();
                    let to = self
                        .navigation_stack
                        .last()
                        .map(|e| e.screen_id)
                        .unwrap_or(Uuid::nil());
                    self.current_screen = Some(to);
                    self.events.push(PreviewEvent::Navigated {
                        from: from_entry.screen_id,
                        to,
                    });
                }
            }
            Action::SetState { state_id } => {
                // Find the state machine that owns this state and force-set.
                for (owner, sm) in &mut self.state_machines {
                    let old = sm.current_state();
                    if sm.set_current_state(state_id) {
                        if let Some(old_id) = old {
                            self.events.push(PreviewEvent::StateChanged {
                                machine_owner: *owner,
                                from: old_id,
                                to: state_id,
                            });
                        }
                        break;
                    }
                }
            }
            Action::ToggleDrawer { drawer_id } => {
                let current = self
                    .drawer_states
                    .get(&drawer_id)
                    .copied()
                    .unwrap_or(DrawerTargetState::Closed);
                let new = match current {
                    DrawerTargetState::Closed => DrawerTargetState::Open,
                    DrawerTargetState::Open => DrawerTargetState::Closed,
                    DrawerTargetState::Peeking => DrawerTargetState::Open,
                };
                self.drawer_states.insert(drawer_id, new);
                self.events.push(PreviewEvent::DrawerToggled {
                    drawer_id,
                    new_state: new,
                });
            }
            Action::SetDrawerState { drawer_id, state } => {
                self.drawer_states.insert(drawer_id, state);
                self.events.push(PreviewEvent::DrawerToggled {
                    drawer_id,
                    new_state: state,
                });
            }
            Action::AnimateProperty {
                layer_id,
                animation,
            } => {
                self.events.push(PreviewEvent::AnimationStarted {
                    layer_id,
                    property: animation.property.clone(),
                });
                self.active_animations
                    .push(ActiveAnimation::new(layer_id, animation, self.time_ms));
            }
            Action::OpenUrl { .. } => {
                // No-op in preview; the host should handle this.
            }
            Action::ShowOverlay { overlay_config } => {
                let content_id = overlay_config.content_id;
                let kind = overlay_config.kind;
                let overlay = ActiveOverlay::new(overlay_config, self.time_ms);
                let oid = overlay.id;
                self.overlay_stack.push(overlay);
                self.events.push(PreviewEvent::OverlayShown {
                    overlay_id: oid,
                    content_id,
                    kind,
                });
            }
            Action::DismissOverlay { content_id } => {
                let dismissed = self.overlay_stack.dismiss_by_content(content_id);
                for o in dismissed {
                    self.events.push(PreviewEvent::OverlayDismissed {
                        overlay_id: o.id,
                        content_id: o.config.content_id,
                        reason: DismissReason::ActionDismiss,
                    });
                }
            }
            Action::DismissTopOverlay => {
                if let Some(o) = self.overlay_stack.pop() {
                    self.events.push(PreviewEvent::OverlayDismissed {
                        overlay_id: o.id,
                        content_id: o.config.content_id,
                        reason: DismissReason::ActionDismiss,
                    });
                }
            }
            Action::Sequence(actions) => {
                for a in actions {
                    self.execute_action(a);
                }
            }

            // ── Sound (host handles actual playback) ──────────────────────────
            Action::PlaySound { .. }
            | Action::PauseSound { .. }
            | Action::StopSound { .. }
            | Action::SetVolume { .. } => {
                // Preview engine delegates sound to the host; no-op here.
            }

            // ── Animation ─────────────────────────────────────────────────────
            Action::StartAnimation { layer_id, animation_name } => {
                self.events.push(PreviewEvent::CustomNotification {
                    message: format!("start_animation:{}:{}", layer_id, animation_name),
                });
            }
            Action::StopAnimation { layer_id, animation_name } => {
                self.events.push(PreviewEvent::CustomNotification {
                    message: format!("stop_animation:{}:{}", layer_id, animation_name),
                });
            }
            Action::PauseAnimation { layer_id, animation_name } => {
                self.events.push(PreviewEvent::CustomNotification {
                    message: format!("pause_animation:{}:{}", layer_id, animation_name),
                });
            }
            Action::ResumeAnimation { layer_id, animation_name } => {
                self.events.push(PreviewEvent::CustomNotification {
                    message: format!("resume_animation:{}:{}", layer_id, animation_name),
                });
            }
            Action::SeekAnimation { layer_id, animation_name, time_ms } => {
                self.events.push(PreviewEvent::CustomNotification {
                    message: format!("seek_animation:{}:{}:{}", layer_id, animation_name, time_ms),
                });
            }

            // ── Layer / Style ─────────────────────────────────────────────────
            Action::SetVisibility { layer_id, visible } => {
                self.events.push(PreviewEvent::CustomNotification {
                    message: format!("set_visibility:{}:{}", layer_id, visible),
                });
            }
            Action::SetOpacity { layer_id, opacity, duration_ms: _ } => {
                self.events.push(PreviewEvent::CustomNotification {
                    message: format!("set_opacity:{}:{}", layer_id, opacity),
                });
            }
            Action::AddCssClass { layer_id, class_name } => {
                self.events.push(PreviewEvent::CustomNotification {
                    message: format!("add_class:{}:{}", layer_id, class_name),
                });
            }
            Action::RemoveCssClass { layer_id, class_name } => {
                self.events.push(PreviewEvent::CustomNotification {
                    message: format!("remove_class:{}:{}", layer_id, class_name),
                });
            }
            Action::ToggleCssClass { layer_id, class_name } => {
                self.events.push(PreviewEvent::CustomNotification {
                    message: format!("toggle_class:{}:{}", layer_id, class_name),
                });
            }
            Action::SetStyleProperty { layer_id, property, value, transition_ms: _ } => {
                self.events.push(PreviewEvent::CustomNotification {
                    message: format!("set_style:{}:{}:{}", layer_id, property, value),
                });
            }

            // ── Navigation / View ─────────────────────────────────────────────
            Action::ScrollTo { layer_id, behavior: _ } => {
                self.events.push(PreviewEvent::CustomNotification {
                    message: format!("scroll_to:{}", layer_id),
                });
            }
            Action::SetFocus { layer_id } => {
                self.events.push(PreviewEvent::CustomNotification {
                    message: format!("set_focus:{}", layer_id),
                });
            }

            // ── Variable ─────────────────────────────────────────────────────
            Action::UpdateVariable { name, value } => {
                self.variables.insert(name, value);
            }
            Action::IncrementVariable { name, delta } => {
                let entry = self.variables.entry(name).or_insert(serde_json::Value::from(0.0));
                if let Some(n) = entry.as_f64() {
                    *entry = serde_json::Value::from(n + delta);
                }
            }
            Action::ToggleVariable { name } => {
                let entry = self.variables.entry(name).or_insert(serde_json::Value::Bool(false));
                if let serde_json::Value::Bool(b) = entry {
                    *b = !*b;
                }
            }

            // ── Media ─────────────────────────────────────────────────────────
            Action::PlayMedia { layer_id } => {
                self.events.push(PreviewEvent::CustomNotification {
                    message: format!("play_media:{}", layer_id),
                });
            }
            Action::PauseMedia { layer_id } => {
                self.events.push(PreviewEvent::CustomNotification {
                    message: format!("pause_media:{}", layer_id),
                });
            }
            Action::StopMedia { layer_id } => {
                self.events.push(PreviewEvent::CustomNotification {
                    message: format!("stop_media:{}", layer_id),
                });
            }
            Action::SeekMedia { layer_id, time_seconds } => {
                self.events.push(PreviewEvent::CustomNotification {
                    message: format!("seek_media:{}:{}", layer_id, time_seconds),
                });
            }
            Action::SetMute { layer_id, muted } => {
                self.events.push(PreviewEvent::CustomNotification {
                    message: format!("set_mute:{}:{}", layer_id, muted),
                });
            }

            // ── Communication ────────────────────────────────────────────────
            Action::EmitCustomEvent { name, payload: _ } => {
                self.events.push(PreviewEvent::CustomNotification {
                    message: format!("custom_event:{}", name),
                });
            }
            Action::CopyToClipboard { text } => {
                self.events.push(PreviewEvent::CustomNotification {
                    message: format!("copy:{}", text),
                });
            }
            Action::Vibrate { pattern_ms } => {
                self.events.push(PreviewEvent::CustomNotification {
                    message: format!("vibrate:{:?}", pattern_ms),
                });
            }
            Action::TrackEvent { category, action, label: _, value: _ } => {
                self.events.push(PreviewEvent::CustomNotification {
                    message: format!("track:{}:{}", category, action),
                });
            }
        }
    }

    // ── Event handling ───────────────────────────────────────────

    /// Drain accumulated events.
    pub fn drain_events(&mut self) -> Vec<PreviewEvent> {
        std::mem::take(&mut self.events)
    }

    /// Get the number of pending events.
    pub fn pending_event_count(&self) -> usize {
        self.events.len()
    }

    // ── Queries ──────────────────────────────────────────────────

    /// Get the current screen.
    pub fn current_screen_id(&self) -> Option<Uuid> {
        self.current_screen
    }

    /// Get the navigation stack depth.
    pub fn navigation_depth(&self) -> usize {
        self.navigation_stack.len()
    }

    /// Get drawer state.
    pub fn drawer_state(&self, drawer_id: Uuid) -> DrawerTargetState {
        self.drawer_states
            .get(&drawer_id)
            .copied()
            .unwrap_or(DrawerTargetState::Closed)
    }

    /// Get the number of active (in-flight) animations.
    pub fn active_animation_count(&self) -> usize {
        self.active_animations.len()
    }

    /// Evaluate all active animations at the current time.
    pub fn evaluate_animations(&self) -> Vec<(Uuid, String, AnimationValue)> {
        self.active_animations
            .iter()
            .filter_map(|a| {
                a.evaluate(self.time_ms)
                    .map(|v| (a.layer_id, a.animation.property.clone(), v))
            })
            .collect()
    }

    // ── Scroll ───────────────────────────────────────────────────

    /// Register a scroll area for a container.
    pub fn add_scroll_area(&mut self, container_id: Uuid, config: ScrollConfig) {
        self.scroll_states
            .insert(container_id, ScrollState::new(container_id, config));
    }

    /// Get the scroll state for a container.
    pub fn scroll_state(&self, container_id: Uuid) -> Option<&ScrollState> {
        self.scroll_states.get(&container_id)
    }

    /// Apply a scroll delta to a container.
    pub fn scroll_by(&mut self, container_id: Uuid, dx: f64, dy: f64) {
        if let Some(state) = self.scroll_states.get_mut(&container_id) {
            state.scroll_by(dx, dy);
            self.events.push(PreviewEvent::ScrollChanged {
                container_id,
                offset_x: state.offset_x,
                offset_y: state.offset_y,
            });
        }
    }

    /// Get the number of active scroll areas.
    pub fn scroll_area_count(&self) -> usize {
        self.scroll_states.len()
    }

    // ── Overlays ─────────────────────────────────────────────────

    /// Get the number of active overlays.
    pub fn overlay_count(&self) -> usize {
        self.overlay_stack.len()
    }

    /// Whether any blocking overlay is active.
    pub fn has_blocking_overlay(&self) -> bool {
        self.overlay_stack.has_blocking_overlay()
    }
}

impl Default for PreviewSession {
    fn default() -> Self {
        Self::new()
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animate::{AnimationValue, EasingCurve, PropertyAnimation};
    use crate::state_machine::{State, StateMachine, Transition};
    use crate::trigger::{NavigationAnimation, Trigger, TriggerKind};

    fn setup_session() -> (PreviewSession, Uuid, Uuid) {
        let screen_a = Uuid::new_v4();
        let screen_b = Uuid::new_v4();
        let mut session = PreviewSession::new();
        session.set_start_screen(screen_a);

        // Set up a state machine on screen_a.
        let mut sm = StateMachine::new(screen_a);
        let s1 = State::new_default("Idle");
        let s2 = State::new("Active");
        let id1 = sm.add_state(s1);
        let id2 = sm.add_state(s2);
        sm.add_transition(Transition::new(id1, id2, TriggerKind::OnClick));
        sm.add_transition(Transition::new(id2, id1, TriggerKind::OnClick));
        session.add_state_machine(sm);

        // Add a navigation trigger on a button layer.
        let button = Uuid::new_v4();
        let target = InteractionTarget::new(button).with_trigger(Trigger::new(
            TriggerKind::OnClick,
            Action::NavigateTo {
                target_id: screen_b,
                animation: Some(NavigationAnimation::SlideLeft),
            },
        ));
        session.add_interaction_target(target);

        (session, screen_a, screen_b)
    }

    #[test]
    fn test_session_creation() {
        let session = PreviewSession::new();
        assert_eq!(session.state, PreviewState::Idle);
        assert!(session.current_screen.is_none());
    }

    #[test]
    fn test_session_start() {
        let (mut session, screen_a, _) = setup_session();
        session.start();
        assert_eq!(session.state, PreviewState::Playing);
        assert_eq!(session.current_screen, Some(screen_a));
        assert_eq!(session.navigation_depth(), 1);
    }

    #[test]
    fn test_session_pause_resume() {
        let (mut session, _, _) = setup_session();
        session.start();
        let _ = session.drain_events();
        session.pause();
        assert_eq!(session.state, PreviewState::Paused);
        session.resume();
        assert_eq!(session.state, PreviewState::Playing);
    }

    #[test]
    fn test_session_stop() {
        let (mut session, _, _) = setup_session();
        session.start();
        let _ = session.drain_events();
        session.stop();
        assert_eq!(session.state, PreviewState::Stopped);
    }

    #[test]
    fn test_tick_does_nothing_when_paused() {
        let (mut session, _, _) = setup_session();
        session.start();
        let _ = session.drain_events();
        session.pause();
        session.tick(100);
        assert_eq!(session.time_ms, 0); // didn't advance
    }

    #[test]
    fn test_tick_advances_time() {
        let (mut session, _, _) = setup_session();
        session.start();
        let _ = session.drain_events();
        session.tick(100);
        assert_eq!(session.time_ms, 100);
        session.tick(50);
        assert_eq!(session.time_ms, 150);
    }

    #[test]
    fn test_fire_state_machine_trigger() {
        let (mut session, screen_a, _) = setup_session();
        session.start();
        let _ = session.drain_events();

        let events = session.fire_trigger(screen_a, &TriggerKind::OnClick);
        let state_changed = events
            .iter()
            .any(|e| matches!(e, PreviewEvent::StateChanged { .. }));
        assert!(state_changed);
    }

    #[test]
    fn test_navigation_via_trigger() {
        let (mut session, _, screen_b) = setup_session();
        session.start();
        let _ = session.drain_events();

        // Find the button layer id.
        let button_id = *session.interaction_targets.keys().next().unwrap();
        let events = session.fire_trigger(button_id, &TriggerKind::OnClick);
        let navigated = events
            .iter()
            .any(|e| matches!(e, PreviewEvent::Navigated { to, .. } if *to == screen_b));
        assert!(navigated);
        assert_eq!(session.current_screen, Some(screen_b));
        assert_eq!(session.navigation_depth(), 2);
    }

    #[test]
    fn test_go_back() {
        let (mut session, screen_a, screen_b) = setup_session();
        session.start();
        let _ = session.drain_events();

        // Navigate to screen_b.
        session.execute_action(Action::NavigateTo {
            target_id: screen_b,
            animation: None,
        });
        let _ = session.drain_events();
        assert_eq!(session.current_screen, Some(screen_b));

        // Go back.
        session.execute_action(Action::GoBack);
        let events = session.drain_events();
        assert_eq!(session.current_screen, Some(screen_a));
        let nav = events
            .iter()
            .any(|e| matches!(e, PreviewEvent::Navigated { to, .. } if *to == screen_a));
        assert!(nav);
    }

    #[test]
    fn test_go_back_at_root_noop() {
        let (mut session, screen_a, _) = setup_session();
        session.start();
        let _ = session.drain_events();

        session.execute_action(Action::GoBack);
        assert_eq!(session.current_screen, Some(screen_a)); // unchanged
    }

    #[test]
    fn test_toggle_drawer() {
        let (mut session, _, _) = setup_session();
        session.start();
        let _ = session.drain_events();

        let drawer = Uuid::new_v4();
        assert_eq!(session.drawer_state(drawer), DrawerTargetState::Closed);

        session.execute_action(Action::ToggleDrawer { drawer_id: drawer });
        assert_eq!(session.drawer_state(drawer), DrawerTargetState::Open);

        session.execute_action(Action::ToggleDrawer { drawer_id: drawer });
        assert_eq!(session.drawer_state(drawer), DrawerTargetState::Closed);
    }

    #[test]
    fn test_set_drawer_state() {
        let (mut session, _, _) = setup_session();
        session.start();
        let _ = session.drain_events();

        let drawer = Uuid::new_v4();
        session.execute_action(Action::SetDrawerState {
            drawer_id: drawer,
            state: DrawerTargetState::Peeking,
        });
        assert_eq!(session.drawer_state(drawer), DrawerTargetState::Peeking);
    }

    #[test]
    fn test_animate_property_action() {
        let (mut session, _, _) = setup_session();
        session.start();
        let _ = session.drain_events();

        let layer = Uuid::new_v4();
        let anim = PropertyAnimation::new(
            "opacity",
            AnimationValue::Scalar(0.0),
            AnimationValue::Scalar(1.0),
            300,
        )
        .with_easing(EasingCurve::Linear);

        session.execute_action(Action::AnimateProperty {
            layer_id: layer,
            animation: anim,
        });
        let events = session.drain_events();
        assert!(events
            .iter()
            .any(|e| matches!(e, PreviewEvent::AnimationStarted { .. })));
        assert_eq!(session.active_animation_count(), 1);
    }

    #[test]
    fn test_active_animation_completes() {
        let (mut session, _, _) = setup_session();
        session.start();
        let _ = session.drain_events();

        let layer = Uuid::new_v4();
        let anim = PropertyAnimation::new(
            "opacity",
            AnimationValue::Scalar(0.0),
            AnimationValue::Scalar(1.0),
            200,
        );
        session.execute_action(Action::AnimateProperty {
            layer_id: layer,
            animation: anim,
        });
        let _ = session.drain_events();

        session.tick(250);
        let events = session.drain_events();
        assert!(events
            .iter()
            .any(|e| matches!(e, PreviewEvent::AnimationCompleted { .. })));
        assert_eq!(session.active_animation_count(), 0);
    }

    #[test]
    fn test_evaluate_animations() {
        let (mut session, _, _) = setup_session();
        session.start();
        let _ = session.drain_events();

        let layer = Uuid::new_v4();
        let anim = PropertyAnimation::new(
            "x",
            AnimationValue::Scalar(0.0),
            AnimationValue::Scalar(100.0),
            200,
        )
        .with_easing(EasingCurve::Linear);
        session.execute_action(Action::AnimateProperty {
            layer_id: layer,
            animation: anim,
        });
        let _ = session.drain_events();

        session.tick(100);
        let results = session.evaluate_animations();
        assert_eq!(results.len(), 1);
        let (_, _, val) = &results[0];
        assert_eq!(*val, AnimationValue::Scalar(50.0));
    }

    #[test]
    fn test_sequence_action() {
        let (mut session, _, _) = setup_session();
        session.start();
        let _ = session.drain_events();

        let drawer = Uuid::new_v4();
        session.execute_action(Action::Sequence(vec![
            Action::ToggleDrawer { drawer_id: drawer },
            Action::ToggleDrawer { drawer_id: drawer },
        ]));
        // Toggle twice: Closed → Open → Closed
        assert_eq!(session.drawer_state(drawer), DrawerTargetState::Closed);
    }

    #[test]
    fn test_fire_trigger_when_stopped() {
        let (mut session, screen_a, _) = setup_session();
        session.start();
        let _ = session.drain_events();
        session.stop();
        let _ = session.drain_events();

        let events = session.fire_trigger(screen_a, &TriggerKind::OnClick);
        assert!(events.is_empty());
    }

    #[test]
    fn test_preview_state_default() {
        assert_eq!(PreviewState::default(), PreviewState::Idle);
    }

    #[test]
    fn test_session_default() {
        let session = PreviewSession::default();
        assert_eq!(session.state, PreviewState::Idle);
    }

    #[test]
    fn test_active_animation_evaluate() {
        let anim = PropertyAnimation::new(
            "y",
            AnimationValue::Scalar(0.0),
            AnimationValue::Scalar(200.0),
            400,
        )
        .with_easing(EasingCurve::Linear);
        let active = ActiveAnimation::new(Uuid::new_v4(), anim, 100);
        // Before start
        assert!(active.evaluate(50).is_none());
        // At start
        let v = active.evaluate(100).unwrap();
        assert_eq!(v, AnimationValue::Scalar(0.0));
        // Midway
        let v = active.evaluate(300).unwrap();
        assert_eq!(v, AnimationValue::Scalar(100.0));
        // Complete
        assert!(active.is_complete(500));
    }

    #[test]
    fn test_serde_roundtrip_preview_state() {
        for state in [
            PreviewState::Idle,
            PreviewState::Playing,
            PreviewState::Paused,
            PreviewState::Stopped,
        ] {
            let json = serde_json::to_string(&state).unwrap();
            let back: PreviewState = serde_json::from_str(&json).unwrap();
            assert_eq!(back, state);
        }
    }
}
