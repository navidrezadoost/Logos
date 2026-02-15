# Logos JavaScript API Reference

The `Logos` global object is available in all JavaScript plugins. It provides access to the document model, selection, undo/redo, events, UI, and logging.

---

## Document Operations

### `Logos.getDocumentInfo()`

Returns metadata about the current document.

**Permission:** `DocumentRead`

**Returns:** `Object`

| Field | Type | Description |
|-------|------|-------------|
| `id` | `string` | UUID of the document |
| `version` | `string` | Document version string |
| `pageName` | `string` | Name of the root page |
| `layerCount` | `number` | Total number of layers |

**Example:**
```javascript
const info = Logos.getDocumentInfo();
console.log(`Document: ${info.pageName} (${info.layerCount} layers)`);
```

**Performance:** ~3.8µs

---

### `Logos.getLayers()`

Returns all layers in the document as an array of layer objects.

**Permission:** `DocumentRead`

**Returns:** `Array<Layer>`

Each layer object has:

| Field | Type | Description |
|-------|------|-------------|
| `id` | `string` | UUID of the layer |
| `name` | `string` | Display name |
| `type` | `string` | One of: `Rectangle`, `Text`, `Frame`, `Component`, `Group` |
| `x` | `number` | X position |
| `y` | `number` | Y position |
| `width` | `number` | Width in pixels |
| `height` | `number` | Height in pixels |
| `rotation` | `number` | Rotation in degrees |

**Example:**
```javascript
const layers = Logos.getLayers();
layers.forEach(layer => {
  console.log(`${layer.name}: ${layer.type} at (${layer.x}, ${layer.y})`);
});
```

**Performance:** ~4.2µs (scales with layer count)

---

### `Logos.getLayerCount()`

Returns the number of layers in the document.

**Permission:** `DocumentRead`

**Returns:** `number`

**Example:**
```javascript
const count = Logos.getLayerCount();
Logos.log(`Document has ${count} layers`);
```

**Performance:** ~2.1µs

---

### `Logos.getLayer(id)`

Retrieves a single layer by UUID.

**Permission:** `DocumentRead`

**Parameters:**

| Name | Type | Description |
|------|------|-------------|
| `id` | `string` | UUID of the layer to retrieve |

**Returns:** `Layer | null`

**Example:**
```javascript
const layer = Logos.getLayer("550e8400-e29b-41d4-a716-446655440000");
if (layer) {
  console.log(`Found: ${layer.name} (${layer.width}x${layer.height})`);
}
```

**Performance:** ~2.4µs (O(1) hash map lookup)

---

### `Logos.createRect(x, y, width, height)`

Creates a new rectangle layer in the document.

**Permission:** `DocumentWrite`

**Parameters:**

| Name | Type | Description |
|------|------|-------------|
| `x` | `number` | X position |
| `y` | `number` | Y position |
| `width` | `number` | Width in pixels |
| `height` | `number` | Height in pixels |

**Returns:** `string` — UUID of the created layer

**Side Effects:**
- Pushes an undo action to the undo stack
- Emits `layerAdded` event with `{ layerId: string }`

**Example:**
```javascript
const rectId = Logos.createRect(100, 200, 300, 150);
Logos.log(`Created rectangle: ${rectId}`);

// Undo if needed
Logos.undo();
```

**Performance:** ~5.2µs

---

### `Logos.createPath(commands)`

Creates a complex path (bezier curves, lines, arcs) in the document.

**Permission:** `DocumentWrite`

**Parameters:**

| Name | Type | Description |
|------|------|-------------|
| `commands` | `Array<PathCommand>` | Array of path commands |

**Path Command Types:**

| Command | Fields | Description |
|---------|--------|-------------|
| `moveTo` | `x`, `y` | Move to point |
| `lineTo` | `x`, `y` | Line to point |
| `quadTo` | `cpx`, `cpy`, `x`, `y` | Quadratic bezier curve |
| `bezierTo` | `cp1x`, `cp1y`, `cp2x`, `cp2y`, `x`, `y` | Cubic bezier curve |
| `close` | *(none)* | Close the path |

**Returns:** `string` — UUID of the created path layer

**Side Effects:**
- Pushes an undo action
- Emits `layerAdded` event

**Example:**
```javascript
const pathId = Logos.createPath([
  { command: "moveTo", x: 10, y: 10 },
  { command: "bezierTo", 
    cp1x: 50, cp1y: 0, 
    cp2x: 150, cp2y: 200, 
    x: 200, y: 100 
  },
  { command: "lineTo", x: 200, y: 200 },
  { command: "close" }
]);
```

**Performance:** ~9.1µs (first call), ~2.1µs (subsequent)

---

### `Logos.deleteLayer(id)`

Deletes a layer from the document.

**Permission:** `DocumentWrite`

**Parameters:**

| Name | Type | Description |
|------|------|-------------|
| `id` | `string` | UUID of the layer to delete |

**Returns:** `boolean` — `true` if the layer was deleted

**Side Effects:**
- Pushes an undo action
- Emits `layerRemoved` event with `{ layerId: string }`

**Example:**
```javascript
const layers = Logos.getLayers();
if (layers.length > 0) {
  Logos.deleteLayer(layers[0].id);
}
```

**Performance:** ~3.8µs

---

## Selection

### `Logos.getSelection()`

Returns the currently selected layer UUIDs.

**Permission:** `DocumentRead`

**Returns:** `Array<string>` — Array of selected layer UUIDs

**Example:**
```javascript
const selected = Logos.getSelection();
console.log(`${selected.length} layers selected`);
```

**Performance:** ~1.8µs

---

### `Logos.setSelection(ids)`

Sets the current selection to the specified layer UUIDs.

**Permission:** `DocumentWrite`

**Parameters:**

| Name | Type | Description |
|------|------|-------------|
| `ids` | `Array<string>` | Array of layer UUIDs to select |

**Side Effects:**
- Emits `selectionChanged` event with `{ layerIds: Array<string> }`

**Example:**
```javascript
const layers = Logos.getLayers();
const rects = layers.filter(l => l.type === "Rectangle");
Logos.setSelection(rects.map(r => r.id));
```

**Performance:** ~2.1µs

---

### `Logos.clearSelection()`

Clears the current selection.

**Permission:** `DocumentWrite`

**Side Effects:**
- Emits `selectionChanged` event with `{ layerIds: [] }`

**Example:**
```javascript
Logos.clearSelection();
```

**Performance:** ~1.5µs

---

## Undo / Redo

### `Logos.undo()`

Undoes the last document-modifying action performed by this plugin.

**Permission:** `DocumentWrite`

**Returns:** `boolean` — `true` if an action was undone

**Example:**
```javascript
Logos.createRect(0, 0, 100, 100);
Logos.undo(); // removes the rectangle
```

---

### `Logos.redo()`

Redoes the last undone action.

**Permission:** `DocumentWrite`

**Returns:** `boolean` — `true` if an action was redone

---

## Events

### `Logos.on(event, callback)`

Registers a callback for a document event.

**Permission:** None required

**Parameters:**

| Name | Type | Description |
|------|------|-------------|
| `event` | `string` | Event name |
| `callback` | `function` | Callback function receiving event data |

**Available Events:**

| Event | Payload | Trigger |
|-------|---------|---------|
| `selectionChanged` | `{ layerIds: string[] }` | Selection modified |
| `layerAdded` | `{ layerId: string }` | Layer created |
| `layerRemoved` | `{ layerId: string }` | Layer deleted |
| `documentChanged` | `{}` | Any document modification |

**Example:**
```javascript
Logos.on("selectionChanged", (data) => {
  console.log(`Selection: ${data.layerIds.join(", ")}`);
});

Logos.on("layerAdded", (data) => {
  const layer = Logos.getLayer(data.layerId);
  console.log(`New layer: ${layer.name}`);
});
```

**Note:** Events are rate-limited to ~60fps (16ms minimum interval between dispatches) to prevent performance degradation.

---

## Logging

### `Logos.log(message)`

Logs a message to the host console.

**Permission:** None required

**Parameters:**

| Name | Type | Description |
|------|------|-------------|
| `message` | `string` | Message to log |

**Example:**
```javascript
Logos.log("Plugin initialized successfully");
```

---

## Timeout Check

### `Logos.checkTimeout()`

Checks if the plugin has exceeded its execution time limit. Throws an error if the deadline has passed.

**Permission:** None required

**Example:**
```javascript
// In long-running loops, periodically check timeout
for (let i = 0; i < 10000; i++) {
  if (i % 100 === 0) Logos.checkTimeout();
  // ... work ...
}
```

---

## UI API

See [UI Components](ui-components.md) for the complete `Logos.ui.*` reference.

### `Logos.ui.createPanel(title, dock, options?)`

Creates a floating or docked UI panel.

**Permission:** `UI:Render`

**Parameters:**

| Name | Type | Description |
|------|------|-------------|
| `title` | `string` | Panel title |
| `dock` | `string` | Dock position: `"left"`, `"right"`, `"bottom"`, `"float"` |
| `options` | `object?` | Optional: `{ components, size, data }` |

**Returns:** `string` — Panel ID

**Example:**
```javascript
const panelId = Logos.ui.createPanel("My Plugin", "right", {
  components: [
    { type: "label", text: "Hello from my plugin!" },
    { type: "button", label: "Click Me", action: "btn_click" },
    { type: "numberInput", label: "Opacity", key: "opacity", 
      value: 100, min: 0, max: 100, step: 1 },
    { type: "separator" },
    { type: "colorPicker", label: "Fill", key: "fill", 
      value: { r: 255, g: 0, b: 0, a: 1.0 } }
  ]
});
```

**Performance:** ~191ns

---

### `Logos.ui.closePanel(panelId)`

Closes and removes a panel.

**Permission:** `UI:Render`

---

### `Logos.ui.sendMessage(panelId, message)`

Sends a typed message to a panel.

**Permission:** `UI:Render`

**Message Types:**

| Type | Fields | Description |
|------|--------|-------------|
| `setComponents` | `components: Array` | Replace component tree |
| `updateValue` | `key: string, value: any` | Update single value |
| `showNotification` | `text: string` | Show toast notification |
| `setTitle` | `title: string` | Change panel title |
| `custom` | `type: string, data: any` | Plugin-defined message |

---

### `Logos.ui.updatePanel(panelId, components)`

Replaces the component tree of a panel.

**Permission:** `UI:Render`

---

### `Logos.ui.getPanels()`

Returns a list of panels owned by the current plugin.

**Permission:** `UI:Render`

**Returns:** `Array<{ id, title, dock, state }>`
