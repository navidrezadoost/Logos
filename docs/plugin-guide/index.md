---
title: Plugin Developer Guide
desc: Build plugins for Logos using WASM or JavaScript — from hello world to marketplace publishing.
eleventyNavigation:
  key: Plugin Guide
  order: 4
---

# Plugin Developer Guide

Logos supports two plugin runtimes — **WASM** (via Wasmtime) and **JavaScript** (via QuickJS). Both run in a sandboxed environment with configurable permissions, fuel limits, and memory caps.

## Quick Start — Hello World Plugin

### 1. Create the Plugin Manifest

Every plugin needs a `plugin.toml` manifest:

```toml
[plugin]
name = "hello-world"
version = "1.0.0"
description = "A simple hello world plugin for Logos"
author = "Your Name"
license = "MIT"
min_logos_version = "2.0.0"

[runtime]
type = "wasm"  # or "javascript"
entry = "hello.wasm"
fuel_limit = 1_000_000     # Max operations (default: 1M)
memory_limit = 52_428_800  # Max memory in bytes (default: 50MB)

[permissions]
document = ["read"]
ui = ["notifications"]
```

### 2. Write the Plugin (Rust → WASM)

```rust
// src/lib.rs
use logos_plugin_sdk::prelude::*;

#[plugin_init]
fn init(host: &dyn HostApi) {
    host.show_notification("Hello from my plugin!", NotificationType::Info);
    
    let doc = host.get_document();
    let layer_count = doc.layers.len();
    host.show_notification(
        &format!("Document has {} layers", layer_count),
        NotificationType::Info,
    );
}

#[plugin_event("selection_changed")]
fn on_selection(host: &dyn HostApi) {
    let selection = host.get_selection();
    if let Some(layer) = selection.first() {
        host.show_toast(&format!("Selected: {}", layer.name));
    }
}
```

### 3. Build

```bash
cargo build --target wasm32-unknown-unknown --release
cp target/wasm32-unknown-unknown/release/hello_world.wasm ./hello.wasm
```

### 4. Install Locally

```bash
# Copy to Logos plugins directory
mkdir -p ~/.logos/plugins/hello-world/
cp plugin.toml hello.wasm ~/.logos/plugins/hello-world/
```

### 5. Load in Logos

Open Logos Desktop → Command Palette (Ctrl+Shift+P) → "Load Plugin" → select `hello-world`.

---

## JavaScript Plugins

For simpler plugins, use the JavaScript runtime:

### plugin.toml

```toml
[plugin]
name = "color-randomizer"
version = "1.0.0"
description = "Randomizes colors of selected layers"

[runtime]
type = "javascript"
entry = "main.js"

[permissions]
document = ["read", "write"]
selection = ["read"]
ui = ["notifications"]
```

### main.js

```javascript
function activate(host) {
    host.showNotification("Color Randomizer loaded!", "info");
}

function onSelectionChanged(host) {
    const selection = host.getSelection();
    selection.forEach(layer => {
        const color = '#' + Math.floor(Math.random() * 16777215).toString(16);
        host.updateLayer(layer.id, { fill: color });
    });
    host.showToast(`Randomized ${selection.length} layers`);
}
```

---

## Host API Reference

Plugins interact with Logos through **21 host functions** organized into 6 categories.

### Document Operations

| Function | Permission | Description |
|----------|-----------|-------------|
| `get_document()` | `document:read` | Get the current document state |
| `get_layers()` | `document:read` | Get all layers in the active page |
| `add_layer(layer)` | `document:write` | Add a new layer to the document |
| `remove_layer(id)` | `document:write` | Remove a layer by ID |
| `update_layer(id, props)` | `document:write` | Update layer properties |

### Selection

| Function | Permission | Description |
|----------|-----------|-------------|
| `get_selection()` | `selection:read` | Get currently selected layers |
| `set_selection(ids)` | `selection:write` | Set the selection to specific layers |
| `clear_selection()` | `selection:write` | Deselect all layers |

### Viewport

| Function | Permission | Description |
|----------|-----------|-------------|
| `get_viewport()` | `viewport:read` | Get camera position, zoom, and bounds |
| `set_viewport(x, y, zoom)` | `viewport:write` | Set camera position and zoom |
| `zoom_to_fit()` | `viewport:write` | Zoom to fit all content |

### UI

| Function | Permission | Description |
|----------|-----------|-------------|
| `show_notification(msg, type)` | `ui:notifications` | Show a notification banner |
| `show_dialog(title, body)` | `ui:dialogs` | Show a modal dialog |
| `show_toast(msg)` | `ui:notifications` | Show a brief toast message |

### Lifecycle

| Function | Permission | Description |
|----------|-----------|-------------|
| `on_activate()` | — | Called when plugin is loaded |
| `on_deactivate()` | — | Called when plugin is unloaded |
| `on_event(name, handler)` | — | Register an event handler |

### State

| Function | Permission | Description |
|----------|-----------|-------------|
| `get_state(key)` | `state:read` | Get persistent plugin state |
| `set_state(key, value)` | `state:write` | Save persistent plugin state |
| `get_preferences()` | `state:read` | Get plugin preferences |

---

## Permissions

Plugins must declare permissions in `plugin.toml`. Users are prompted to approve permissions when a plugin is first loaded.

### Permission Categories

| Category | Values | Description |
|----------|--------|-------------|
| `document` | `read`, `write` | Access to document layers and properties |
| `selection` | `read`, `write` | Access to current selection |
| `viewport` | `read`, `write` | Camera position and zoom control |
| `ui` | `notifications`, `dialogs` | Display notifications and dialogs |
| `state` | `read`, `write` | Persistent key-value storage |
| `network` | `http` | Make HTTP requests (future) |

### Principle of Least Privilege

Only declare the permissions your plugin actually needs. Plugins with fewer permissions are more likely to be approved on the marketplace and trusted by users.

```toml
# ✅ Good — minimal permissions
[permissions]
document = ["read"]
ui = ["notifications"]

# ❌ Avoid — requesting everything
[permissions]
document = ["read", "write"]
selection = ["read", "write"]
viewport = ["read", "write"]
ui = ["notifications", "dialogs"]
state = ["read", "write"]
```

---

## Security Model

### Sandboxing

| Runtime | Sandbox | Fuel Limit | Memory Limit |
|---------|---------|-----------|--------------|
| WASM (Wasmtime) | Process isolation, no filesystem/network | 1M operations | 50 MB |
| JavaScript (QuickJS) | No `eval`, no globals, restricted stdlib | 10M operations | 25 MB |

### Signing

All marketplace plugins must be signed with an Ed25519 key:

```bash
# Generate a keypair
logos-cli keygen --output publisher.key

# Sign your plugin
logos-cli sign --key publisher.key --plugin ./hello.wasm --manifest ./plugin.toml

# Verify (optional)
logos-cli verify --key publisher.pub --plugin ./hello.wasm --manifest ./plugin.toml
```

The signature is embedded in the manifest and verified on download from the marketplace.

---

## Publishing to the Marketplace

### 1. Register as a Publisher

Create an account on the Logos Marketplace and generate Ed25519 keypair.

### 2. Prepare Your Plugin

```bash
# Build release binary
cargo build --target wasm32-unknown-unknown --release

# Sign
logos-cli sign --key publisher.key --plugin ./target/.../plugin.wasm --manifest ./plugin.toml

# Package
tar czf my-plugin-1.0.0.tar.gz plugin.toml plugin.wasm
```

### 3. Submit

Use the Logos Desktop marketplace UI or the API:

```bash
curl -X POST https://marketplace.logos.dev/api/v1/plugins \
  -H "Authorization: Bearer $TOKEN" \
  -F "package=@my-plugin-1.0.0.tar.gz"
```

### 4. Review Process

- Automated checks: manifest validation, signature verification, permission audit
- Manual review for marketplace listing
- Typical turnaround: 1-3 business days

### 5. Updates

Submit new versions via the publisher dashboard or API:

```bash
curl -X PUT https://marketplace.logos.dev/api/v1/plugins/$PLUGIN_ID/versions \
  -H "Authorization: Bearer $TOKEN" \
  -F "package=@my-plugin-1.1.0.tar.gz"
```

---

## Example Plugins

The repository includes 3 example plugins in `logos-plugins/src/examples.rs`:

| Plugin | Description | Runtime |
|--------|-------------|---------|
| `hello-world` | Shows a notification on load | WASM |
| `shape-generator` | Creates random shapes on the canvas | WASM |
| `color-palette` | Extracts and displays document colors | WASM |

Study these examples as templates for your own plugins.

---

## Troubleshooting

### Plugin won't load

1. Check manifest: `logos-cli validate-manifest plugin.toml`
2. Verify WASM target: `file plugin.wasm` should show `WebAssembly`
3. Check permissions match host function calls
4. Look at Logos console output for error messages

### Fuel limit exceeded

Your plugin is doing too much computation in a single invocation. Break work into smaller batches or increase `fuel_limit` in manifest (max: 100M).

### Memory limit exceeded

Reduce data structures in memory. The 50MB default is sufficient for most plugins. Contact marketplace support if you need more.

### Permission denied

Ensure the permission is declared in `plugin.toml` AND the user has approved it. Undeclared permissions will always fail silently.
