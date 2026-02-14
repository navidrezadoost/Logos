//! Event system for plugin callbacks.
//!
//! Provides an event bus that allows plugins to register JavaScript callbacks
//! for specific events and dispatches them with rate limiting and timeout
//! protection.
//!
//! ## Supported Events
//!
//! | Event | Description | Payload |
//! |-------|-------------|---------|
//! | `selectionChanged` | Selection modified | `{ ids: string[] }` |
//! | `layerAdded` | Layer created | `{ id: string, type: string }` |
//! | `layerRemoved` | Layer deleted | `{ id: string }` |
//! | `documentChanged` | Document modified | `{ version: number }` |
//!
//! ## Rate Limiting
//!
//! Events are throttled to a maximum dispatch rate (default 60fps / ~16ms)
//! to prevent plugins from being overwhelmed by rapid changes.
//!
//! ## Safety
//!
//! Callback execution is protected by the same deadline/timeout mechanism
//! used for normal plugin execution.

use boa_engine::{Context, JsObject, JsString, JsValue};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Recognized event names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventKind {
    SelectionChanged,
    LayerAdded,
    LayerRemoved,
    DocumentChanged,
}

impl EventKind {
    /// Parse from a JS event name string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "selectionChanged" => Some(Self::SelectionChanged),
            "layerAdded" => Some(Self::LayerAdded),
            "layerRemoved" => Some(Self::LayerRemoved),
            "documentChanged" => Some(Self::DocumentChanged),
            _ => None,
        }
    }

    /// The JS string name of this event.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SelectionChanged => "selectionChanged",
            Self::LayerAdded => "layerAdded",
            Self::LayerRemoved => "layerRemoved",
            Self::DocumentChanged => "documentChanged",
        }
    }
}

/// A registered callback for a specific event.
#[derive(Debug)]
struct EventCallback {
    /// The boa JsObject (must be callable).
    callback: JsObject,
}

/// An event that has been queued for dispatch.
#[derive(Debug, Clone)]
pub struct EventPayload {
    pub kind: EventKind,
    pub data: HashMap<String, EventData>,
}

/// Simple typed event data values.
#[derive(Debug, Clone)]
pub enum EventData {
    String(String),
    Number(f64),
    StringArray(Vec<String>),
}

impl EventData {
    /// Convert to a JsValue for passing to callbacks.
    pub fn to_js_value(&self, ctx: &mut Context) -> JsValue {
        match self {
            EventData::String(s) => JsValue::new(JsString::from(s.as_str())),
            EventData::Number(n) => JsValue::rational(*n),
            EventData::StringArray(arr) => {
                let js_arr = boa_engine::object::builtins::JsArray::new(ctx);
                for s in arr {
                    let _ = js_arr.push(JsValue::new(JsString::from(s.as_str())), ctx);
                }
                js_arr.into()
            }
        }
    }
}

/// The event bus manages callback registration and dispatch.
pub struct EventBus {
    /// Map from event kind → list of registered callbacks.
    listeners: HashMap<EventKind, Vec<EventCallback>>,
    /// Minimum interval between dispatches of the same event kind.
    min_interval: Duration,
    /// Last dispatch time per event kind.
    last_dispatch: HashMap<EventKind, Instant>,
    /// Maximum callback execution time.
    callback_timeout: Duration,
    /// Total events dispatched (for stats).
    dispatch_count: u64,
    /// Queued events waiting for dispatch.
    queue: Vec<EventPayload>,
}

impl EventBus {
    /// Create a new event bus with default rate limiting (60fps).
    pub fn new() -> Self {
        Self {
            listeners: HashMap::new(),
            min_interval: Duration::from_millis(16), // ~60fps
            last_dispatch: HashMap::new(),
            callback_timeout: Duration::from_millis(10),
            dispatch_count: 0,
            queue: Vec::new(),
        }
    }

    /// Create with custom rate limit interval.
    pub fn with_rate_limit(interval: Duration) -> Self {
        Self {
            min_interval: interval,
            ..Self::new()
        }
    }

    /// Register a callback for an event kind.
    pub fn on(&mut self, kind: EventKind, callback: JsObject) {
        self.listeners
            .entry(kind)
            .or_insert_with(Vec::new)
            .push(EventCallback { callback });
    }

    /// Remove all callbacks for an event kind.
    pub fn off(&mut self, kind: EventKind) {
        self.listeners.remove(&kind);
    }

    /// Remove all callbacks for all events.
    pub fn clear(&mut self) {
        self.listeners.clear();
        self.queue.clear();
    }

    /// Queue an event for dispatch.
    pub fn emit(&mut self, payload: EventPayload) {
        self.queue.push(payload);
    }

    /// Check if any listeners are registered for an event kind.
    pub fn has_listeners(&self, kind: EventKind) -> bool {
        self.listeners
            .get(&kind)
            .map_or(false, |v| !v.is_empty())
    }

    /// Get the number of registered listeners for an event kind.
    pub fn listener_count(&self, kind: EventKind) -> usize {
        self.listeners.get(&kind).map_or(0, |v| v.len())
    }

    /// Get total dispatch count.
    pub fn dispatch_count(&self) -> u64 {
        self.dispatch_count
    }

    /// Get the number of queued events.
    pub fn queue_len(&self) -> usize {
        self.queue.len()
    }

    /// Drain the event queue and dispatch to registered callbacks.
    ///
    /// Returns the number of callbacks invoked.
    /// Rate-limited events are dropped (not re-queued).
    pub fn flush(&mut self, ctx: &mut Context) -> u64 {
        let events: Vec<EventPayload> = self.queue.drain(..).collect();
        let mut invoked = 0u64;

        for event in events {
            // Rate limit check
            if let Some(last) = self.last_dispatch.get(&event.kind) {
                if last.elapsed() < self.min_interval {
                    continue; // Skip — too soon
                }
            }

            if let Some(callbacks) = self.listeners.get(&event.kind) {
                // Build the event data JS object
                let event_obj = self.build_event_object(&event, ctx);
                let event_val = JsValue::from(event_obj);

                let deadline = Instant::now() + self.callback_timeout;

                for cb in callbacks {
                    if Instant::now() > deadline {
                        log::warn!(
                            "event callback timeout for {:?}, skipping remaining",
                            event.kind
                        );
                        break;
                    }
                    let _ = cb
                        .callback
                        .call(&JsValue::undefined(), &[event_val.clone()], ctx);
                    invoked += 1;
                }

                self.last_dispatch.insert(event.kind, Instant::now());
                self.dispatch_count += invoked;
            }
        }

        invoked
    }

    /// Build a JS object from an EventPayload's data map.
    fn build_event_object(&self, event: &EventPayload, ctx: &mut Context) -> JsObject {
        // Pre-convert all values to avoid double borrow of ctx
        let mut js_values: Vec<(String, JsValue)> = Vec::with_capacity(event.data.len() + 1);
        js_values.push((
            "event".to_string(),
            JsValue::new(JsString::from(event.kind.as_str())),
        ));
        for (key, val) in &event.data {
            js_values.push((key.clone(), val.to_js_value(ctx)));
        }

        let mut builder = boa_engine::object::ObjectInitializer::new(ctx);
        for (key, js_val) in js_values {
            builder.property(
                JsString::from(key.as_str()),
                js_val,
                boa_engine::property::Attribute::all(),
            );
        }
        builder.build()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_kind_from_str() {
        assert_eq!(EventKind::from_str("selectionChanged"), Some(EventKind::SelectionChanged));
        assert_eq!(EventKind::from_str("layerAdded"), Some(EventKind::LayerAdded));
        assert_eq!(EventKind::from_str("layerRemoved"), Some(EventKind::LayerRemoved));
        assert_eq!(EventKind::from_str("documentChanged"), Some(EventKind::DocumentChanged));
        assert_eq!(EventKind::from_str("unknown"), None);
    }

    #[test]
    fn test_event_kind_roundtrip() {
        for kind in [
            EventKind::SelectionChanged,
            EventKind::LayerAdded,
            EventKind::LayerRemoved,
            EventKind::DocumentChanged,
        ] {
            assert_eq!(EventKind::from_str(kind.as_str()), Some(kind));
        }
    }

    #[test]
    fn test_event_bus_new() {
        let bus = EventBus::new();
        assert_eq!(bus.dispatch_count(), 0);
        assert_eq!(bus.queue_len(), 0);
        assert!(!bus.has_listeners(EventKind::SelectionChanged));
    }

    #[test]
    fn test_event_bus_emit_and_queue() {
        let mut bus = EventBus::new();
        let payload = EventPayload {
            kind: EventKind::LayerAdded,
            data: {
                let mut m = HashMap::new();
                m.insert("id".to_string(), EventData::String("abc".to_string()));
                m
            },
        };
        bus.emit(payload);
        assert_eq!(bus.queue_len(), 1);
    }

    #[test]
    fn test_event_bus_clear() {
        let mut bus = EventBus::new();
        let payload = EventPayload {
            kind: EventKind::LayerAdded,
            data: HashMap::new(),
        };
        bus.emit(payload);
        bus.clear();
        assert_eq!(bus.queue_len(), 0);
        assert!(!bus.has_listeners(EventKind::LayerAdded));
    }

    #[test]
    fn test_event_bus_off() {
        let mut bus = EventBus::new();
        // We can't easily create a JsObject without a Context here,
        // but we can test that off() removes the entry
        bus.off(EventKind::SelectionChanged);
        assert!(!bus.has_listeners(EventKind::SelectionChanged));
    }

    #[test]
    fn test_event_bus_listener_count() {
        let bus = EventBus::new();
        assert_eq!(bus.listener_count(EventKind::SelectionChanged), 0);
    }

    #[test]
    fn test_event_bus_flush_empty() {
        let mut bus = EventBus::new();
        let mut ctx = Context::default();
        assert_eq!(bus.flush(&mut ctx), 0);
    }

    #[test]
    fn test_event_bus_flush_no_listeners() {
        let mut bus = EventBus::new();
        let mut ctx = Context::default();
        bus.emit(EventPayload {
            kind: EventKind::DocumentChanged,
            data: HashMap::new(),
        });
        // No listeners registered → 0 callbacks invoked
        assert_eq!(bus.flush(&mut ctx), 0);
        assert_eq!(bus.queue_len(), 0);
    }

    #[test]
    fn test_event_bus_with_callback() {
        let mut bus = EventBus::new();
        let mut ctx = Context::default();

        // Create a simple callback function
        let code = "function handler(e) { globalThis.__lastEvent = e.event; }; handler";
        let result = ctx
            .eval(boa_engine::Source::from_bytes(code))
            .expect("eval failed");
        let cb = result.as_object().expect("should be object").clone();

        bus.on(EventKind::SelectionChanged, cb);
        assert!(bus.has_listeners(EventKind::SelectionChanged));
        assert_eq!(bus.listener_count(EventKind::SelectionChanged), 1);

        bus.emit(EventPayload {
            kind: EventKind::SelectionChanged,
            data: {
                let mut m = HashMap::new();
                m.insert(
                    "ids".to_string(),
                    EventData::StringArray(vec!["id1".to_string()]),
                );
                m
            },
        });

        let invoked = bus.flush(&mut ctx);
        assert_eq!(invoked, 1);

        // Verify the callback was actually called
        let check = ctx
            .eval(boa_engine::Source::from_bytes("globalThis.__lastEvent"))
            .unwrap();
        assert_eq!(
            check.as_string().map(|s| s.to_std_string_escaped()),
            Some("selectionChanged".to_string())
        );
    }

    #[test]
    fn test_event_bus_multiple_callbacks() {
        let mut bus = EventBus::new();
        let mut ctx = Context::default();

        let code1 = "function h1(e) { globalThis.__count = (globalThis.__count || 0) + 1; }; h1";
        let code2 = "function h2(e) { globalThis.__count = (globalThis.__count || 0) + 1; }; h2";

        let cb1 = ctx
            .eval(boa_engine::Source::from_bytes(code1))
            .unwrap()
            .as_object()
            .unwrap()
            .clone();
        let cb2 = ctx
            .eval(boa_engine::Source::from_bytes(code2))
            .unwrap()
            .as_object()
            .unwrap()
            .clone();

        bus.on(EventKind::LayerAdded, cb1);
        bus.on(EventKind::LayerAdded, cb2);
        assert_eq!(bus.listener_count(EventKind::LayerAdded), 2);

        bus.emit(EventPayload {
            kind: EventKind::LayerAdded,
            data: HashMap::new(),
        });

        let invoked = bus.flush(&mut ctx);
        assert_eq!(invoked, 2);

        let count = ctx
            .eval(boa_engine::Source::from_bytes("globalThis.__count"))
            .unwrap();
        assert_eq!(count.as_number(), Some(2.0));
    }

    #[test]
    fn test_event_bus_rate_limiting() {
        let mut bus = EventBus::with_rate_limit(Duration::from_secs(10)); // 10s — won't pass
        let mut ctx = Context::default();

        let code = "function rl(e) { globalThis.__rl = (globalThis.__rl || 0) + 1; }; rl";
        let cb = ctx
            .eval(boa_engine::Source::from_bytes(code))
            .unwrap()
            .as_object()
            .unwrap()
            .clone();

        bus.on(EventKind::DocumentChanged, cb);

        // First dispatch should go through
        bus.emit(EventPayload {
            kind: EventKind::DocumentChanged,
            data: HashMap::new(),
        });
        assert_eq!(bus.flush(&mut ctx), 1);

        // Second dispatch within rate limit should be dropped
        bus.emit(EventPayload {
            kind: EventKind::DocumentChanged,
            data: HashMap::new(),
        });
        assert_eq!(bus.flush(&mut ctx), 0);
    }

    #[test]
    fn test_event_data_to_js_string() {
        let mut ctx = Context::default();
        let data = EventData::String("hello".to_string());
        let val = data.to_js_value(&mut ctx);
        assert_eq!(
            val.as_string().map(|s| s.to_std_string_escaped()),
            Some("hello".to_string())
        );
    }

    #[test]
    fn test_event_data_to_js_number() {
        let mut ctx = Context::default();
        let data = EventData::Number(42.0);
        let val = data.to_js_value(&mut ctx);
        assert_eq!(val.as_number(), Some(42.0));
    }

    #[test]
    fn test_event_data_to_js_string_array() {
        let mut ctx = Context::default();
        let data = EventData::StringArray(vec!["a".to_string(), "b".to_string()]);
        let val = data.to_js_value(&mut ctx);
        assert!(val.is_object());
    }
}
