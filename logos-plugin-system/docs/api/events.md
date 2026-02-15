# Event System Reference

The Logos event system enables plugins to react to document changes in real-time. Events are rate-limited and dispatched through a centralized event bus.

---

## Available Events

| Event Name | Payload | Trigger |
|-----------|---------|---------|
| `selectionChanged` | `{ layerIds: string[] }` | Selection is modified |
| `layerAdded` | `{ layerId: string }` | A new layer is created |
| `layerRemoved` | `{ layerId: string }` | A layer is deleted |
| `documentChanged` | `{}` | Any document modification |

---

## Registering Listeners

```javascript
// Listen for selection changes
Logos.on("selectionChanged", (data) => {
  console.log(`Selected: ${data.layerIds.length} layers`);
  data.layerIds.forEach(id => {
    const layer = Logos.getLayer(id);
    if (layer) {
      console.log(`  - ${layer.name} (${layer.type})`);
    }
  });
});

// Listen for new layers
Logos.on("layerAdded", (data) => {
  const layer = Logos.getLayer(data.layerId);
  console.log(`New layer: ${layer.name}`);
});

// Listen for layer deletion
Logos.on("layerRemoved", (data) => {
  console.log(`Layer removed: ${data.layerId}`);
});

// Listen for any document change
Logos.on("documentChanged", () => {
  console.log("Document was modified");
});
```

---

## Event Flow

```
Document Action (e.g., createRect)
        │
        ▼
   Event Emitted ──► Event Queue
                          │
                          ▼
                    Rate Limiter (16ms)
                          │
                          ▼
                  Dispatch to Callbacks
                          │
                    ┌─────┴─────┐
                    ▼           ▼
              Plugin A     Plugin B
              callback     callback
```

---

## Rate Limiting

Events are rate-limited to prevent performance degradation:

- **Default interval:** 16ms (~60fps)
- **Behavior:** Events emitted within the rate limit window are queued
- **Dispatch:** Queued events are dispatched in order during `flush()`

```rust
// Custom rate limit
let bus = EventBus::with_rate_limit(Duration::from_millis(32)); // ~30fps
```

This ensures that rapid document modifications (e.g., dragging a layer) don't flood plugins with thousands of events per second.

---

## Event Data Types

### `EventData` Enum

| Variant | JavaScript Type | Description |
|---------|----------------|-------------|
| `String(String)` | `string` | Text data |
| `Number(f64)` | `number` | Numeric data |
| `StringArray(Vec<String>)` | `string[]` | Array of strings |

---

## Rust API

### EventBus

```rust
use logos_plugins::EventBus;
use logos_plugins::engine::events::{EventKind, EventPayload, EventData};

// Create event bus
let mut bus = EventBus::new();

// Register a JavaScript callback
bus.on(EventKind::SelectionChanged, js_callback_object);

// Emit an event
let mut payload = EventPayload::new(EventKind::LayerAdded);
payload.set("layerId", EventData::String(uuid.to_string()));
bus.emit(payload);

// Dispatch queued events (call from main loop)
let dispatched = bus.flush(&mut js_context);

// Query
assert!(bus.has_listeners(EventKind::SelectionChanged));
assert_eq!(bus.listener_count(EventKind::SelectionChanged), 1);
```

### EventKind Parsing

```rust
let kind = EventKind::from_str("selectionChanged");
assert_eq!(kind, Some(EventKind::SelectionChanged));

assert_eq!(EventKind::LayerAdded.as_str(), "layerAdded");
```

---

## Performance

| Operation | Latency |
|-----------|---------|
| Event emit (queue) | ~50ns |
| Event dispatch (per callback) | ~200ns |
| Rate limit check | ~10ns |
| Flush (10 queued events) | ~2µs |

---

## Best Practices

1. **Keep callbacks fast** — Event callbacks count against the plugin's execution time limit
2. **Use `checkTimeout()`** — In long event handlers, periodically check the timeout
3. **Batch updates** — If your callback modifies the UI, batch updates to avoid thrashing
4. **Unsubscribe when done** — Remove listeners when your plugin no longer needs them

```javascript
// Good: Fast callback
Logos.on("selectionChanged", (data) => {
  updateLayerCount(data.layerIds.length);
});

// Bad: Expensive callback
Logos.on("selectionChanged", (data) => {
  // Don't iterate over ALL layers on every selection change
  const allLayers = Logos.getLayers();
  processAllLayers(allLayers); // Too expensive!
});
```
