//! Interaction simulator — generate mock user input events for agent tests.
//!
//! `InteractionSimulator` builds a sequence of `InteractionEvent`s that mimic
//! real user gestures (clicks, typing, drag, scroll, keyboard shortcuts).
//! Events are replayed on a `SandboxEnv` canvas to produce a testable state.

use crate::sandbox::{CanvasState, SandboxResult};
use serde::{Deserialize, Serialize};

// ── Pointer event ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PointerEvent {
    pub x: f32,
    pub y: f32,
    pub button: u8, // 0 = left, 1 = middle, 2 = right
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

impl PointerEvent {
    pub fn left_click(x: f32, y: f32) -> Self {
        Self { x, y, button: 0, shift: false, ctrl: false, alt: false }
    }

    pub fn right_click(x: f32, y: f32) -> Self {
        Self { x, y, button: 2, shift: false, ctrl: false, alt: false }
    }

    pub fn shift_click(x: f32, y: f32) -> Self {
        Self { x, y, button: 0, shift: true, ctrl: false, alt: false }
    }

    pub fn ctrl_click(x: f32, y: f32) -> Self {
        Self { x, y, button: 0, shift: false, ctrl: true, alt: false }
    }
}

// ── Key event ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyEvent {
    pub key: String,
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

impl KeyEvent {
    pub fn key(key: impl Into<String>) -> Self {
        Self { key: key.into(), shift: false, ctrl: false, alt: false, meta: false }
    }

    pub fn ctrl_key(key: impl Into<String>) -> Self {
        Self { key: key.into(), shift: false, ctrl: true, alt: false, meta: false }
    }

    pub fn ctrl_shift_key(key: impl Into<String>) -> Self {
        Self { key: key.into(), shift: true, ctrl: true, alt: false, meta: false }
    }

    pub fn is_modifier_only(&self) -> bool {
        matches!(self.key.as_str(), "Control" | "Shift" | "Alt" | "Meta")
    }
}

// ── Drag event ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DragEvent {
    pub from_x: f32,
    pub from_y: f32,
    pub to_x: f32,
    pub to_y: f32,
    /// How many intermediate steps to simulate (0 = direct).
    pub steps: u32,
    pub shift: bool,
}

impl DragEvent {
    pub fn new(from_x: f32, from_y: f32, to_x: f32, to_y: f32) -> Self {
        Self { from_x, from_y, to_x, to_y, steps: 4, shift: false }
    }

    pub fn delta_x(&self) -> f32 { self.to_x - self.from_x }
    pub fn delta_y(&self) -> f32 { self.to_y - self.from_y }
    pub fn distance(&self) -> f32 { self.delta_x().hypot(self.delta_y()) }
}

// ── Scroll event ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScrollEvent {
    pub x: f32,
    pub y: f32,
    pub delta_x: f32,
    pub delta_y: f32,
    pub ctrl: bool, // ctrl+scroll = zoom
}

impl ScrollEvent {
    pub fn scroll_down(x: f32, y: f32, amount: f32) -> Self {
        Self { x, y, delta_x: 0.0, delta_y: amount, ctrl: false }
    }

    pub fn scroll_up(x: f32, y: f32, amount: f32) -> Self {
        Self { x, y, delta_x: 0.0, delta_y: -amount, ctrl: false }
    }

    pub fn zoom_in(x: f32, y: f32) -> Self {
        Self { x, y, delta_x: 0.0, delta_y: -1.0, ctrl: true }
    }

    pub fn zoom_out(x: f32, y: f32) -> Self {
        Self { x, y, delta_x: 0.0, delta_y: 1.0, ctrl: true }
    }
}

// ── Interaction event (union) ─────────────────────────────────────────────────

/// All event types that can be injected into a simulation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InteractionEvent {
    Click(PointerEvent),
    DoubleClick(PointerEvent),
    RightClick(PointerEvent),
    Type { text: String },
    Key(KeyEvent),
    Drag(DragEvent),
    Scroll(ScrollEvent),
    /// Pause between events (synthetic delay, in simulated ms).
    Delay { ms: u64 },
}

impl InteractionEvent {
    pub fn kind_label(&self) -> &str {
        match self {
            Self::Click(_)       => "click",
            Self::DoubleClick(_) => "double_click",
            Self::RightClick(_)  => "right_click",
            Self::Type { .. }    => "type",
            Self::Key(_)         => "key",
            Self::Drag(_)        => "drag",
            Self::Scroll(_)      => "scroll",
            Self::Delay { .. }   => "delay",
        }
    }
}

// ── Simulator config ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulatorConfig {
    /// Simulate typing at this delay per character (synthetic ms).
    pub typing_delay_ms: u64,
    /// Record all events for replay / inspection.
    pub record_events: bool,
}

impl Default for SimulatorConfig {
    fn default() -> Self {
        Self { typing_delay_ms: 10, record_events: true }
    }
}

// ── Interaction simulator ─────────────────────────────────────────────────────

/// Builds and replays synthetic user interaction sequences on a [`CanvasState`].
pub struct InteractionSimulator {
    pub config: SimulatorConfig,
    events: Vec<InteractionEvent>,
}

impl InteractionSimulator {
    pub fn new() -> Self {
        Self::with_config(SimulatorConfig::default())
    }

    pub fn with_config(config: SimulatorConfig) -> Self {
        Self { config, events: Vec::new() }
    }

    // ── Event builders ────────────────────────────────────────────────────────

    pub fn click(&mut self, x: f32, y: f32) -> &mut Self {
        self.push(InteractionEvent::Click(PointerEvent::left_click(x, y)))
    }

    pub fn double_click(&mut self, x: f32, y: f32) -> &mut Self {
        self.push(InteractionEvent::DoubleClick(PointerEvent::left_click(x, y)))
    }

    pub fn right_click(&mut self, x: f32, y: f32) -> &mut Self {
        self.push(InteractionEvent::RightClick(PointerEvent::right_click(x, y)))
    }

    pub fn shift_click(&mut self, x: f32, y: f32) -> &mut Self {
        self.push(InteractionEvent::Click(PointerEvent::shift_click(x, y)))
    }

    pub fn ctrl_click(&mut self, x: f32, y: f32) -> &mut Self {
        self.push(InteractionEvent::Click(PointerEvent::ctrl_click(x, y)))
    }

    pub fn type_text(&mut self, text: impl Into<String>) -> &mut Self {
        self.push(InteractionEvent::Type { text: text.into() })
    }

    pub fn press_key(&mut self, key: impl Into<String>) -> &mut Self {
        self.push(InteractionEvent::Key(KeyEvent::key(key)))
    }

    pub fn press_ctrl(&mut self, key: impl Into<String>) -> &mut Self {
        self.push(InteractionEvent::Key(KeyEvent::ctrl_key(key)))
    }

    pub fn press_ctrl_shift(&mut self, key: impl Into<String>) -> &mut Self {
        self.push(InteractionEvent::Key(KeyEvent::ctrl_shift_key(key)))
    }

    pub fn drag(&mut self, from_x: f32, from_y: f32, to_x: f32, to_y: f32) -> &mut Self {
        self.push(InteractionEvent::Drag(DragEvent::new(from_x, from_y, to_x, to_y)))
    }

    pub fn scroll_down(&mut self, x: f32, y: f32, amount: f32) -> &mut Self {
        self.push(InteractionEvent::Scroll(ScrollEvent::scroll_down(x, y, amount)))
    }

    pub fn scroll_up(&mut self, x: f32, y: f32, amount: f32) -> &mut Self {
        self.push(InteractionEvent::Scroll(ScrollEvent::scroll_up(x, y, amount)))
    }

    pub fn zoom_in(&mut self, x: f32, y: f32) -> &mut Self {
        self.push(InteractionEvent::Scroll(ScrollEvent::zoom_in(x, y)))
    }

    pub fn zoom_out(&mut self, x: f32, y: f32) -> &mut Self {
        self.push(InteractionEvent::Scroll(ScrollEvent::zoom_out(x, y)))
    }

    pub fn delay(&mut self, ms: u64) -> &mut Self {
        self.push(InteractionEvent::Delay { ms })
    }

    fn push(&mut self, event: InteractionEvent) -> &mut Self {
        if self.config.record_events {
            self.events.push(event);
        }
        self
    }

    // ── Replay ────────────────────────────────────────────────────────────────

    /// Replay all recorded events against a canvas, returning the list of
    /// applied effect descriptions.
    pub fn replay(&self, canvas: &mut CanvasState) -> SandboxResult<Vec<String>> {
        let mut effects = Vec::new();
        for event in &self.events {
            let effect = self.apply_event(event, canvas)?;
            effects.push(effect);
        }
        Ok(effects)
    }

    fn apply_event(&self, event: &InteractionEvent, canvas: &mut CanvasState) -> SandboxResult<String> {
        match event {
            InteractionEvent::Click(p) => {
                // Simulate selecting a layer at the clicked position
                let hit: Option<String> = canvas
                    .layers()
                    .iter()
                    .rev() // top-most first
                    .find(|l| {
                        p.x >= l.x && p.x <= l.x + l.width
                            && p.y >= l.y && p.y <= l.y + l.height
                    })
                    .map(|l| l.id.clone());
                if let Some(ref id) = hit {
                    if p.shift {
                        // Collect existing selection as owned Strings so the
                        // immutable borrow of `canvas` ends before the mutable
                        // `canvas.select()` call below.
                        let existing: Vec<String> =
                            canvas.selected_ids().iter().cloned().collect();
                        let mut ids: Vec<&str> =
                            existing.iter().map(|s| s.as_str()).collect();
                        let id_str = id.as_str();
                        if !ids.contains(&id_str) {
                            ids.push(id_str);
                        }
                        canvas.select(&ids);
                    } else {
                        canvas.select(&[id.as_str()]);
                    }
                    Ok(format!("click → selected {id}"))
                } else {
                    canvas.deselect_all();
                    Ok(format!("click ({},{}) → no hit, deselected", p.x, p.y))
                }
            }

            InteractionEvent::DoubleClick(p) => {
                Ok(format!("double_click ({},{})", p.x, p.y))
            }

            InteractionEvent::RightClick(p) => {
                Ok(format!("right_click ({},{})", p.x, p.y))
            }

            InteractionEvent::Type { text } => {
                Ok(format!("type \"{}\"", text))
            }

            InteractionEvent::Key(k) => {
                if k.ctrl && k.key == "a" {
                    // Select all
                    let ids: Vec<String> = canvas.layers().iter().map(|l| l.id.clone()).collect();
                    let id_refs: Vec<&str> = ids.iter().map(|s| s.as_str()).collect();
                    canvas.select(&id_refs);
                    Ok("Ctrl+A → select all".into())
                } else if k.ctrl && k.key == "z" {
                    Ok("Ctrl+Z → undo (mock)".into())
                } else {
                    Ok(format!("key {:?}", k.key))
                }
            }

            InteractionEvent::Drag(d) => {
                // Move selected layers by delta
                let selected: Vec<String> = canvas.selected_ids().to_vec();
                for id in &selected {
                    if let Some(layer) = canvas.find_layer_mut(id) {
                        layer.x += d.delta_x();
                        layer.y += d.delta_y();
                    }
                }
                Ok(format!(
                    "drag ({},{})→({},{}) Δ({},{})",
                    d.from_x, d.from_y, d.to_x, d.to_y, d.delta_x(), d.delta_y()
                ))
            }

            InteractionEvent::Scroll(s) => {
                if s.ctrl {
                    let factor = if s.delta_y < 0.0 { 1.1_f32 } else { 0.9_f32 };
                    let new_zoom = canvas.zoom() * factor;
                    canvas.set_zoom(new_zoom);
                    Ok(format!("zoom → {:.2}", canvas.zoom()))
                } else {
                    let (vx, vy) = canvas.viewport();
                    canvas.set_viewport(vx + s.delta_x, vy + s.delta_y);
                    Ok(format!("scroll Δ({},{})", s.delta_x, s.delta_y))
                }
            }

            InteractionEvent::Delay { ms } => {
                Ok(format!("delay {}ms", ms))
            }
        }
    }

    // ── Inspection ────────────────────────────────────────────────────────────

    pub fn event_count(&self) -> usize { self.events.len() }

    pub fn events(&self) -> &[InteractionEvent] { &self.events }

    pub fn clear(&mut self) { self.events.clear(); }

    pub fn event_kinds(&self) -> Vec<&str> {
        self.events.iter().map(|e| e.kind_label()).collect()
    }
}

impl Default for InteractionSimulator {
    fn default() -> Self { Self::new() }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sandbox::{CanvasLayer, CanvasState};

    fn canvas_with_layer() -> CanvasState {
        let mut c = CanvasState::new();
        c.add_layer(
            CanvasLayer::new("l1", "Box", "rectangle")
                .with_position(100.0, 100.0)
                .with_size(200.0, 150.0),
        );
        c
    }

    #[test]
    fn click_selects_layer_under_cursor() {
        let mut sim = InteractionSimulator::new();
        sim.click(150.0, 150.0);
        let mut canvas = canvas_with_layer();
        let effects = sim.replay(&mut canvas).unwrap();
        assert!(effects[0].contains("selected l1"));
        assert_eq!(canvas.selected_ids(), &["l1".to_string()]);
    }

    #[test]
    fn click_outside_deselects() {
        let mut sim = InteractionSimulator::new();
        sim.click(10.0, 10.0);
        let mut canvas = canvas_with_layer();
        canvas.select(&["l1"]);
        sim.replay(&mut canvas).unwrap();
        assert!(canvas.selected_ids().is_empty());
    }

    #[test]
    fn drag_moves_selected_layer() {
        let mut sim = InteractionSimulator::new();
        sim.click(150.0, 150.0).drag(150.0, 150.0, 200.0, 200.0);
        let mut canvas = canvas_with_layer();
        sim.replay(&mut canvas).unwrap();
        let layer = canvas.find_layer("l1").unwrap();
        // After drag Δ(50,50) applied
        assert!((layer.x - 150.0).abs() < 0.1);
        assert!((layer.y - 150.0).abs() < 0.1);
    }

    #[test]
    fn scroll_zoom_in_increases_zoom() {
        let mut sim = InteractionSimulator::new();
        sim.zoom_in(400.0, 300.0);
        let mut canvas = CanvasState::new();
        sim.replay(&mut canvas).unwrap();
        assert!(canvas.zoom() > 1.0);
    }

    #[test]
    fn scroll_zoom_out_decreases_zoom() {
        let mut sim = InteractionSimulator::new();
        sim.zoom_out(400.0, 300.0);
        let mut canvas = CanvasState::new();
        sim.replay(&mut canvas).unwrap();
        assert!(canvas.zoom() < 1.0);
    }

    #[test]
    fn ctrl_a_selects_all() {
        let mut sim = InteractionSimulator::new();
        sim.press_ctrl("a");
        let mut canvas = CanvasState::new();
        canvas.add_layer(CanvasLayer::new("l1", "A", "r"));
        canvas.add_layer(CanvasLayer::new("l2", "B", "r"));
        sim.replay(&mut canvas).unwrap();
        assert_eq!(canvas.selected_ids().len(), 2);
    }

    #[test]
    fn type_text_event_recorded() {
        let mut sim = InteractionSimulator::new();
        sim.type_text("Hello");
        assert_eq!(sim.event_count(), 1);
        assert_eq!(sim.event_kinds(), vec!["type"]);
    }

    #[test]
    fn delay_event_recorded() {
        let mut sim = InteractionSimulator::new();
        sim.delay(100);
        assert!(matches!(sim.events()[0], InteractionEvent::Delay { ms: 100 }));
    }

    #[test]
    fn key_event_labels() {
        let k = KeyEvent::ctrl_key("z");
        assert!(!k.is_modifier_only());
        assert!(k.ctrl);
        let m = KeyEvent::key("Shift");
        assert!(m.is_modifier_only());
    }

    #[test]
    fn drag_event_distance() {
        let d = DragEvent::new(0.0, 0.0, 3.0, 4.0);
        assert!((d.distance() - 5.0).abs() < 0.001);
    }

    #[test]
    fn simulator_clear() {
        let mut sim = InteractionSimulator::new();
        sim.click(0.0, 0.0).drag(0.0, 0.0, 10.0, 10.0);
        assert_eq!(sim.event_count(), 2);
        sim.clear();
        assert_eq!(sim.event_count(), 0);
    }

    #[test]
    fn shift_click_adds_to_selection() {
        let mut canvas = CanvasState::new();
        canvas.add_layer(CanvasLayer::new("l1", "A", "r").with_position(0.0, 0.0).with_size(50.0, 50.0));
        canvas.add_layer(CanvasLayer::new("l2", "B", "r").with_position(100.0, 0.0).with_size(50.0, 50.0));
        canvas.select(&["l1"]);

        let mut sim = InteractionSimulator::new();
        sim.shift_click(120.0, 20.0); // hits l2
        sim.replay(&mut canvas).unwrap();
        assert!(canvas.selected_ids().contains(&"l1".to_string()));
        assert!(canvas.selected_ids().contains(&"l2".to_string()));
    }
}
