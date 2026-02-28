//! UI Events — event bus for inter-component communication
//!
//! Decoupled publish/subscribe bus connecting the chat panel, command palette,
//! agent badges, and the editor canvas. Any UI component can publish an event
//! without knowing which other components are listening.

use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

// ── Panel events ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PanelEvent {
    ChatPanelOpened { session_id: String },
    ChatPanelClosed { session_id: String },
    MessageSent { session_id: String, text: String },
    MessageReceived { session_id: String, text: String, agent_session_id: String },
    HistoryCleared { session_id: String },
    StreamingStarted { session_id: String },
    StreamingChunk { session_id: String, chunk: String },
    StreamingEnded { session_id: String },
}

impl PanelEvent {
    pub fn session_id(&self) -> &str {
        match self {
            PanelEvent::ChatPanelOpened { session_id } => session_id,
            PanelEvent::ChatPanelClosed { session_id } => session_id,
            PanelEvent::MessageSent { session_id, .. } => session_id,
            PanelEvent::MessageReceived { session_id, .. } => session_id,
            PanelEvent::HistoryCleared { session_id } => session_id,
            PanelEvent::StreamingStarted { session_id } => session_id,
            PanelEvent::StreamingChunk { session_id, .. } => session_id,
            PanelEvent::StreamingEnded { session_id } => session_id,
        }
    }
}

// ── Palette events ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PaletteEvent {
    PaletteOpened,
    PaletteClosed,
    CommandSelected { command_id: String },
    QueryChanged { query: String },
    AgentTriggerDetected { trigger: String, input: String },
    CommandExecuted { command_id: String, success: bool },
}

impl PaletteEvent {
    pub fn kind_label(&self) -> &str {
        match self {
            PaletteEvent::PaletteOpened => "opened",
            PaletteEvent::PaletteClosed => "closed",
            PaletteEvent::CommandSelected { .. } => "selected",
            PaletteEvent::QueryChanged { .. } => "query",
            PaletteEvent::AgentTriggerDetected { .. } => "agent-trigger",
            PaletteEvent::CommandExecuted { .. } => "executed",
        }
    }
}

// ── Agent events ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentEvent {
    Connected { session_id: String, level: String, provider: String },
    Disconnected { session_id: String, reason: String },
    LevelChanged { session_id: String, old_level: String, new_level: String },
    TrainingStarted { session_id: String },
    Certified { session_id: String, level: String, score_pct: f32 },
    CertificationFailed { session_id: String, reason: String },
    RequestCompleted { session_id: String, latency_ms: u64 },
    ErrorOccurred { session_id: String, error: String },
}

impl AgentEvent {
    pub fn session_id(&self) -> &str {
        match self {
            AgentEvent::Connected { session_id, .. } => session_id,
            AgentEvent::Disconnected { session_id, .. } => session_id,
            AgentEvent::LevelChanged { session_id, .. } => session_id,
            AgentEvent::TrainingStarted { session_id } => session_id,
            AgentEvent::Certified { session_id, .. } => session_id,
            AgentEvent::CertificationFailed { session_id, .. } => session_id,
            AgentEvent::RequestCompleted { session_id, .. } => session_id,
            AgentEvent::ErrorOccurred { session_id, .. } => session_id,
        }
    }

    pub fn is_error(&self) -> bool {
        matches!(self, AgentEvent::ErrorOccurred { .. } | AgentEvent::CertificationFailed { .. })
    }
}

// ── Canvas events ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CanvasEvent {
    SelectionChanged { layer_ids: Vec<String> },
    PageChanged { page_id: String, page_name: String },
    ZoomChanged { zoom_pct: f32 },
    LayerCreated { layer_id: String, layer_type: String },
    LayerDeleted { layer_id: String },
    CommandApplied { command_name: String },
}

// ── UI event kind ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UiEventKind {
    Panel(PanelEvent),
    Palette(PaletteEvent),
    Agent(AgentEvent),
    Canvas(CanvasEvent),
    Custom { name: String, data: serde_json::Value },
}

impl UiEventKind {
    pub fn category(&self) -> &str {
        match self {
            UiEventKind::Panel(_) => "panel",
            UiEventKind::Palette(_) => "palette",
            UiEventKind::Agent(_) => "agent",
            UiEventKind::Canvas(_) => "canvas",
            UiEventKind::Custom { .. } => "custom",
        }
    }
}

// ── UI event ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiEvent {
    pub id: String,
    pub kind: UiEventKind,
    pub timestamp_secs: u64,
    pub source_component: Option<String>,
}

impl UiEvent {
    pub fn new(kind: UiEventKind, ts: u64) -> Self {
        UiEvent {
            id: uuid::Uuid::new_v4().to_string(),
            kind,
            timestamp_secs: ts,
            source_component: None,
        }
    }

    pub fn from_source(kind: UiEventKind, ts: u64, source: impl Into<String>) -> Self {
        let mut e = Self::new(kind, ts);
        e.source_component = Some(source.into());
        e
    }

    pub fn category(&self) -> &str {
        self.kind.category()
    }

    /// Convenience: is this an agent certification event?
    pub fn is_certification(&self) -> bool {
        matches!(&self.kind, UiEventKind::Agent(AgentEvent::Certified { .. }))
    }
}

// ── Event subscriber trait ────────────────────────────────────────────────────

pub trait EventSubscriber: Send + Sync {
    fn on_event(&self, event: &UiEvent);
    fn subscriber_name(&self) -> &str { "subscriber" }
    fn event_filter(&self, _event: &UiEvent) -> bool { true }
}

// ── Event handler ─────────────────────────────────────────────────────────────

/// A subscriber backed by a closure stored in a Mutex.
pub struct EventHandler {
    pub name: String,
    pub category_filter: Option<String>,
    received: Arc<Mutex<Vec<UiEvent>>>,
}

impl EventHandler {
    pub fn new(name: impl Into<String>) -> Self {
        EventHandler {
            name: name.into(),
            category_filter: None,
            received: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn with_category_filter(mut self, category: impl Into<String>) -> Self {
        self.category_filter = Some(category.into());
        self
    }

    pub fn received_events(&self) -> Vec<UiEvent> {
        self.received.lock().unwrap().clone()
    }

    pub fn received_count(&self) -> usize {
        self.received.lock().unwrap().len()
    }

    pub fn last_event(&self) -> Option<UiEvent> {
        self.received.lock().unwrap().last().cloned()
    }

    pub fn events_of_category(&self, cat: &str) -> Vec<UiEvent> {
        self.received.lock().unwrap()
            .iter()
            .filter(|e| e.category() == cat)
            .cloned()
            .collect()
    }
}

impl EventSubscriber for EventHandler {
    fn on_event(&self, event: &UiEvent) {
        if let Some(cat) = &self.category_filter {
            if event.category() != cat { return; }
        }
        self.received.lock().unwrap().push(event.clone());
    }

    fn subscriber_name(&self) -> &str { &self.name }

    fn event_filter(&self, event: &UiEvent) -> bool {
        if let Some(cat) = &self.category_filter {
            return event.category() == cat;
        }
        true
    }
}

// ── Event bus ─────────────────────────────────────────────────────────────────

pub struct EventBus {
    subscribers: Vec<Arc<dyn EventSubscriber>>,
    event_log: Vec<UiEvent>,
    log_capacity: usize,
}

impl EventBus {
    pub fn new() -> Self {
        EventBus { subscribers: Vec::new(), event_log: Vec::new(), log_capacity: 1000 }
    }

    pub fn with_capacity(mut self, cap: usize) -> Self {
        self.log_capacity = cap;
        self
    }

    pub fn subscribe(&mut self, subscriber: Arc<dyn EventSubscriber>) {
        self.subscribers.push(subscriber);
    }

    pub fn publish(&mut self, event: UiEvent) {
        for sub in &self.subscribers {
            if sub.event_filter(&event) {
                sub.on_event(&event);
            }
        }
        if self.event_log.len() >= self.log_capacity {
            self.event_log.remove(0);
        }
        self.event_log.push(event);
    }

    pub fn subscriber_count(&self) -> usize { self.subscribers.len() }
    pub fn event_count(&self) -> usize { self.event_log.len() }

    pub fn events_of_category(&self, cat: &str) -> Vec<&UiEvent> {
        self.event_log.iter().filter(|e| e.category() == cat).collect()
    }

    pub fn last_event(&self) -> Option<&UiEvent> { self.event_log.last() }
    pub fn clear_log(&mut self) { self.event_log.clear(); }
}

impl Default for EventBus {
    fn default() -> Self { Self::new() }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn panel_event() -> UiEvent {
        UiEvent::new(
            UiEventKind::Panel(PanelEvent::MessageSent {
                session_id: "sess-1".into(),
                text: "Hello agent!".into(),
            }),
            100,
        )
    }

    fn agent_event() -> UiEvent {
        UiEvent::new(
            UiEventKind::Agent(AgentEvent::Certified {
                session_id: "sess-1".into(),
                level: "Senior".into(),
                score_pct: 91.5,
            }),
            200,
        )
    }

    #[test]
    fn event_bus_starts_empty() {
        let bus = EventBus::new();
        assert_eq!(bus.subscriber_count(), 0);
        assert_eq!(bus.event_count(), 0);
    }

    #[test]
    fn subscribe_and_receive_event() {
        let mut bus = EventBus::new();
        let handler = Arc::new(EventHandler::new("test-handler"));
        bus.subscribe(handler.clone());
        bus.publish(panel_event());
        assert_eq!(handler.received_count(), 1);
    }

    #[test]
    fn multiple_subscribers_receive_event() {
        let mut bus = EventBus::new();
        let h1 = Arc::new(EventHandler::new("h1"));
        let h2 = Arc::new(EventHandler::new("h2"));
        bus.subscribe(h1.clone());
        bus.subscribe(h2.clone());
        bus.publish(panel_event());
        assert_eq!(h1.received_count(), 1);
        assert_eq!(h2.received_count(), 1);
    }

    #[test]
    fn category_filter_only_receives_matching() {
        let mut bus = EventBus::new();
        let agent_handler = Arc::new(EventHandler::new("agent-only").with_category_filter("agent"));
        bus.subscribe(agent_handler.clone());
        bus.publish(panel_event());       // should NOT reach handler
        bus.publish(agent_event());       // SHOULD reach handler
        assert_eq!(agent_handler.received_count(), 1);
        assert_eq!(agent_handler.events_of_category("agent").len(), 1);
    }

    #[test]
    fn event_bus_logs_events() {
        let mut bus = EventBus::new();
        bus.publish(panel_event());
        bus.publish(agent_event());
        assert_eq!(bus.event_count(), 2);
    }

    #[test]
    fn event_bus_category_filter() {
        let mut bus = EventBus::new();
        bus.publish(panel_event());
        bus.publish(agent_event());
        assert_eq!(bus.events_of_category("agent").len(), 1);
        assert_eq!(bus.events_of_category("panel").len(), 1);
    }

    #[test]
    fn event_is_certification() {
        let e = agent_event();
        assert!(e.is_certification());
        assert!(!panel_event().is_certification());
    }

    #[test]
    fn event_has_correct_category() {
        assert_eq!(panel_event().category(), "panel");
        assert_eq!(agent_event().category(), "agent");
    }

    #[test]
    fn handler_last_event() {
        let mut bus = EventBus::new();
        let h = Arc::new(EventHandler::new("h"));
        bus.subscribe(h.clone());
        bus.publish(panel_event());
        bus.publish(agent_event());
        let last = h.last_event().unwrap();
        assert_eq!(last.category(), "agent");
    }

    #[test]
    fn bus_log_capacity_respected() {
        let mut bus = EventBus::new().with_capacity(3);
        for i in 0..5u64 {
            bus.publish(UiEvent::new(
                UiEventKind::Custom { name: format!("evt-{}", i), data: serde_json::Value::Null },
                i * 10,
            ));
        }
        // Should have trimmed oldest
        assert!(bus.event_count() <= 3, "Count: {}", bus.event_count());
    }

    #[test]
    fn bus_clear_log() {
        let mut bus = EventBus::new();
        bus.publish(panel_event());
        bus.clear_log();
        assert_eq!(bus.event_count(), 0);
    }

    #[test]
    fn agent_event_is_error_check() {
        let err_event = UiEvent::new(
            UiEventKind::Agent(AgentEvent::ErrorOccurred {
                session_id: "s".into(),
                error: "timeout".into(),
            }),
            0,
        );
        assert!(matches!(
            &err_event.kind,
            UiEventKind::Agent(ae) if ae.is_error()
        ));
    }

    #[test]
    fn event_source_component() {
        let e = UiEvent::from_source(
            UiEventKind::Palette(PaletteEvent::PaletteOpened),
            100, "toolbar"
        );
        assert_eq!(e.source_component.as_deref(), Some("toolbar"));
    }

    #[test]
    fn panel_event_session_id() {
        let e = PanelEvent::MessageSent { session_id: "sess-99".into(), text: "hi".into() };
        assert_eq!(e.session_id(), "sess-99");
    }
}
