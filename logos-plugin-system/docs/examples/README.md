# Example Plugins

Complete, working Logos plugin examples covering beginner to advanced use cases.

---

## Examples

| # | Example | Complexity | Key Concepts |
|---|---------|------------|-------------|
| 1 | [Hello World](01-hello-world/) | Beginner | Panel, buttons, notifications, document read |
| 2 | [Layer Counter](02-layer-counter/) | Beginner | Live stats, events, type filtering, groups |
| 3 | [Color Palette](03-color-palette/) | Intermediate | createRect, color picker, undo, grid generation |
| 4 | [Export Helper](04-export-helper/) | Intermediate | Document traversal, JSON/CSV/text export, select |
| 5 | [Animation Tool](05-animation-tool/) | Advanced | createPath, bezier curves, math, spiral patterns |

---

## Running an Example

Each example has the same structure:

```
example-name/
├── README.md       — Description and screenshot
├── manifest.json   — Plugin manifest
└── plugin.js       — Plugin code
```

### To run an example:

1. **Package it:**
```rust
let manifest = serde_json::from_str::<PluginManifest>(
    &std::fs::read_to_string("manifest.json").unwrap()
).unwrap();
let code = std::fs::read("plugin.js").unwrap();

let package = PackageBuilder::new()
    .manifest(manifest)
    .code(code)
    .build()
    .unwrap();
```

2. **Load it:**
```rust
let mut manager = PluginManager::new(document);
let id = manager.load(manifest).unwrap();
manager.execute(&id, &code_string).unwrap();
```

---

## What Each Example Teaches

### 1. Hello World
Your first plugin. Learn the absolute basics:
- How to create a `manifest.json`
- How to use `Logos.getDocumentInfo()` to read doc metadata
- How to create a UI panel with `Logos.ui.createPanel()`
- How to listen for events with `Logos.on()`

### 2. Layer Counter
Build a live dashboard:
- Iterate all layers with `Logos.getLayers()`
- Group components to organize UI
- Update UI dynamically when the document changes
- Filter layers by type

### 3. Color Palette
Create shapes programmatically:
- Use `Logos.createRect()` to add shapes
- Color picker and number input components
- Undo integration with `Logos.undo()`
- Batch shape creation (grid pattern)

### 4. Export Helper
Analyze and export document data:
- Traverse the full layer tree
- Generate JSON, CSV, and plain text reports
- Select dropdown for format choice
- Toggle components for options

### 5. Animation Tool
Advanced generative design:
- Use `Logos.createPath()` for bezier curves
- Mathematical transformations (sin, cos, spiral)
- Circle pattern approximation with bezier curves
- Batch undo for pattern removal
- Complex multi-group UI layout

---

## Building Your Own

Start with the [Getting Started Guide](../guides/getting-started.md), then use these examples as reference for specific features.

**Key resources:**
- [JavaScript API Reference](../api/javascript-api.md)
- [UI Components Reference](../api/ui-components.md)
- [Permissions Reference](../api/permissions.md)
- [Events Reference](../api/events.md)
