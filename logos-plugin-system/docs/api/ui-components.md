# UI Components Reference

Logos plugins use a **declarative component model** to create panels. Instead of raw HTML/CSS, plugins describe their UI as a tree of typed components.

---

## Panel System

### Creating Panels

```javascript
const panelId = Logos.ui.createPanel("My Panel", "right", {
  components: [
    { type: "label", text: "Welcome!" }
  ],
  size: {
    preferredWidth: 280,
    preferredHeight: 400,
    minWidth: 200,
    minHeight: 150,
    maxWidth: 600,
    maxHeight: 800
  }
});
```

### Dock Positions

| Position | Description |
|----------|-------------|
| `"left"` | Docked to the left sidebar |
| `"right"` | Docked to the right sidebar |
| `"bottom"` | Docked to the bottom panel |
| `"float"` | Floating window |

### Panel States

| State | Description |
|-------|-------------|
| `Created` | Panel initialized but not yet shown |
| `Active` | Panel is visible and interactive |
| `Hidden` | Panel exists but is not visible |
| `Closed` | Panel has been closed |

---

## Component Types

### Label

Static text display.

```javascript
{ type: "label", text: "Hello, World!" }
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `text` | `string` | yes | Text to display |

---

### Button

Clickable action button. When clicked, sends a `ButtonClicked` message with the `action` string.

```javascript
{ type: "button", label: "Apply Changes", action: "apply" }
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `label` | `string` | yes | Button text |
| `action` | `string` | yes | Action identifier sent on click |

---

### NumberInput

Numeric input with optional range constraints. Supports drag-to-adjust.

```javascript
{ 
  type: "numberInput", 
  label: "Opacity", 
  key: "opacity",
  value: 100,
  min: 0,
  max: 100,
  step: 1 
}
```

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `label` | `string` | yes | — | Display label |
| `key` | `string` | yes | — | Unique identifier for value changes |
| `value` | `number` | yes | — | Current value |
| `min` | `number` | no | `0.0` | Minimum value |
| `max` | `number` | no | `100.0` | Maximum value |
| `step` | `number` | no | `1.0` | Step increment |

---

### TextInput

Single-line text input field.

```javascript
{
  type: "textInput",
  label: "Layer Name",
  key: "layer_name",
  value: "Rectangle 1",
  placeholder: "Enter name..."
}
```

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `label` | `string` | yes | — | Display label |
| `key` | `string` | yes | — | Unique identifier |
| `value` | `string` | no | `""` | Current text |
| `placeholder` | `string` | no | `""` | Placeholder text |

---

### ColorPicker

RGBA color picker component.

```javascript
{
  type: "colorPicker",
  label: "Fill Color",
  key: "fill",
  value: { r: 255, g: 128, b: 0, a: 1.0 }
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `label` | `string` | yes | Display label |
| `key` | `string` | yes | Unique identifier |
| `value` | `Color` | yes | `{ r, g, b, a }` with r/g/b: 0–255, a: 0.0–1.0 |

---

### Toggle

Boolean toggle / checkbox.

```javascript
{
  type: "toggle",
  label: "Show Grid",
  key: "show_grid",
  value: true
}
```

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `label` | `string` | yes | — | Display label |
| `key` | `string` | yes | — | Unique identifier |
| `value` | `boolean` | no | `false` | Current state |

---

### Select

Dropdown selection with predefined options.

```javascript
{
  type: "select",
  label: "Blend Mode",
  key: "blend_mode",
  value: "normal",
  options: ["normal", "multiply", "screen", "overlay"]
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `label` | `string` | yes | Display label |
| `key` | `string` | yes | Unique identifier |
| `value` | `string` | yes | Currently selected option |
| `options` | `Array<string>` | yes | Available choices |

---

### LayerList

Scrollable list of document layers with optional selection sync.

```javascript
{ type: "layerList", syncSelection: true }
```

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `syncSelection` | `boolean` | no | `false` | Sync with document selection |

When `syncSelection` is `true`, selecting a layer in the list also selects it in the document, and vice versa.

---

### PropertyEditor

Auto-generated property editor for the currently selected layer(s).

```javascript
{ type: "propertyEditor" }
```

Automatically displays editable fields for position, size, rotation, fill, stroke, and other properties of the selected layer.

---

### Separator

Visual horizontal divider between sections.

```javascript
{ type: "separator" }
```

---

### Group

Collapsible group container with child components.

```javascript
{
  type: "group",
  label: "Transform",
  collapsed: false,
  children: [
    { type: "numberInput", label: "X", key: "x", value: 0, min: -9999, max: 9999 },
    { type: "numberInput", label: "Y", key: "y", value: 0, min: -9999, max: 9999 },
    { type: "numberInput", label: "Width", key: "w", value: 100, min: 0, max: 9999 },
    { type: "numberInput", label: "Height", key: "h", value: 100, min: 0, max: 9999 }
  ]
}
```

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `label` | `string` | yes | — | Group heading |
| `collapsed` | `boolean` | no | `false` | Initial collapsed state |
| `children` | `Array<Component>` | yes | — | Child components |

---

## Message Protocol

### Plugin → Panel Messages

```javascript
// Replace entire component tree
Logos.ui.sendMessage(panelId, {
  type: "setComponents",
  components: [...]
});

// Update a single value by key
Logos.ui.sendMessage(panelId, {
  type: "updateValue",
  key: "opacity",
  value: 75
});

// Show a toast notification
Logos.ui.sendMessage(panelId, {
  type: "showNotification",
  text: "Changes saved!"
});

// Change panel title
Logos.ui.sendMessage(panelId, {
  type: "setTitle",
  title: "My Plugin — Editing"
});

// Custom plugin-defined message
Logos.ui.sendMessage(panelId, {
  type: "custom",
  customType: "preview_update",
  data: { url: "...", scale: 2 }
});

// Request/response pattern
Logos.ui.sendMessage(panelId, {
  type: "request",
  requestId: "req_001",
  method: "getFormData",
  data: {}
});
```

### Panel → Plugin Messages

These messages are received by the plugin via the event system:

| Message Type | Fields | Description |
|-------------|--------|-------------|
| `ButtonClicked` | `action: string` | User clicked a button |
| `ValueChanged` | `key: string, value: any` | User changed an input value |
| `LayerSelected` | `layerId: string` | User selected a layer in LayerList |
| `PanelEvent` | `event: string` | Panel lifecycle event (shown/hidden/resized) |
| `Custom` | `type: string, data: any` | Custom message from panel |
| `Response` | `requestId: string, data: any` | Response to a request |

---

## UI Permissions

Plugins must declare UI permissions in their manifest:

```json
{
  "permissions": {
    "ui": ["panel"]
  }
}
```

| Permission | Description |
|-----------|-------------|
| `Render` | Create panels and send messages |
| `ReadDocument` | Access document data from UI |
| `WriteDocument` | Modify document from UI |
| `Network` | Make network requests from UI |

---

## Complete Example

```javascript
// Create a comprehensive plugin panel
const panelId = Logos.ui.createPanel("Layer Inspector", "right", {
  components: [
    { type: "label", text: "Layer Inspector v1.0" },
    { type: "separator" },
    
    { type: "group", label: "Position", collapsed: false, children: [
      { type: "numberInput", label: "X", key: "pos_x", value: 0, min: -9999, max: 9999 },
      { type: "numberInput", label: "Y", key: "pos_y", value: 0, min: -9999, max: 9999 }
    ]},
    
    { type: "group", label: "Size", collapsed: false, children: [
      { type: "numberInput", label: "Width", key: "size_w", value: 100, min: 0, max: 9999 },
      { type: "numberInput", label: "Height", key: "size_h", value: 100, min: 0, max: 9999 }
    ]},
    
    { type: "separator" },
    { type: "colorPicker", label: "Fill", key: "fill", 
      value: { r: 200, g: 200, b: 200, a: 1.0 } },
    
    { type: "toggle", label: "Visible", key: "visible", value: true },
    
    { type: "select", label: "Blend Mode", key: "blend",
      value: "normal",
      options: ["normal", "multiply", "screen", "overlay", "darken", "lighten"] },
    
    { type: "separator" },
    { type: "button", label: "Duplicate Layer", action: "duplicate" },
    { type: "button", label: "Delete Layer", action: "delete" }
  ]
});

// Listen for selection changes to update the panel
Logos.on("selectionChanged", (data) => {
  if (data.layerIds.length === 1) {
    const layer = Logos.getLayer(data.layerIds[0]);
    if (layer) {
      Logos.ui.sendMessage(panelId, {
        type: "updateValue", key: "pos_x", value: layer.x
      });
      Logos.ui.sendMessage(panelId, {
        type: "updateValue", key: "pos_y", value: layer.y
      });
      Logos.ui.sendMessage(panelId, {
        type: "updateValue", key: "size_w", value: layer.width
      });
      Logos.ui.sendMessage(panelId, {
        type: "updateValue", key: "size_h", value: layer.height
      });
    }
  }
});
```

---

## Performance Notes

- Panel creation: **~191ns** — Panels are created as lightweight data structures
- Message roundtrip: **~382ns** — Messages are delivered synchronously within the process
- Rate limiting: Messages are coalesced at **~60fps** (16ms intervals) to prevent UI thrashing
- Max panels per plugin: **16** — Enforced by the UI bridge
