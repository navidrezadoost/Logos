# Building Your First Logos Plugin

This guide walks you through creating, packaging, and installing a Logos plugin from scratch.

---

## Prerequisites

- Logos v1.0.0 or later
- A text editor
- Basic JavaScript knowledge

---

## Step 1: Create Your Plugin Directory

```bash
mkdir hello-world-plugin
cd hello-world-plugin
```

---

## Step 2: Create the Manifest

Create `manifest.json`:

```json
{
  "name": "Hello World",
  "version": "1.0.0",
  "author": "Your Name",
  "description": "A simple Hello World plugin for Logos.",
  "entry_point": "plugin.js",
  "category": "DevTools",
  "license": "MIT",
  "permissions": {
    "document": ["read"],
    "ui": ["panel"]
  },
  "hooks": ["onLoad"],
  "commands": [
    {
      "id": "hello-greet",
      "label": "Say Hello",
      "shortcut": "Ctrl+Shift+H"
    }
  ],
  "tags": ["hello", "example", "beginner"]
}
```

This manifest declares:
- **Document read** permission to inspect layers
- **UI panel** permission to show a panel
- **onLoad** hook to run code when the plugin loads
- A keyboard shortcut **Ctrl+Shift+H** to trigger the greeting

---

## Step 3: Write the Plugin Code

Create `plugin.js`:

```javascript
// Hello World Plugin for Logos
// Demonstrates: panels, document access, events, and commands

// Get document info on load
const info = Logos.getDocumentInfo();
Logos.log(`Hello World loaded! Document: ${info.pageName}`);

// Create a UI panel
const panelId = Logos.ui.createPanel("Hello World", "right", {
  components: [
    { type: "label", text: "👋 Hello from Logos!" },
    { type: "separator" },
    { type: "label", text: `Document: ${info.pageName}` },
    { type: "label", text: `Layers: ${info.layerCount}` },
    { type: "separator" },
    { type: "button", label: "Count Layers", action: "count" },
    { type: "button", label: "Create Rectangle", action: "create_rect" },
    { type: "button", label: "Show Selection", action: "show_selection" }
  ]
});

// Listen for selection changes
Logos.on("selectionChanged", (data) => {
  const count = data.layerIds.length;
  Logos.ui.sendMessage(panelId, {
    type: "showNotification",
    text: `${count} layer${count !== 1 ? "s" : ""} selected`
  });
});

// Listen for new layers
Logos.on("layerAdded", (data) => {
  // Update the layer count display
  const newCount = Logos.getLayerCount();
  Logos.log(`Layer added! Total: ${newCount}`);
});
```

---

## Step 4: Package Your Plugin

### Using Rust (programmatic)

```rust
use logos_plugins::*;

fn main() {
    let manifest = PluginManifest::new("Hello World")
        .with_version(1, 0, 0)
        .with_author("Your Name")
        .with_entry_point("plugin.js")
        .with_description("A simple Hello World plugin")
        .with_category(PluginCategory::DevTools);

    let code = std::fs::read("plugin.js").unwrap();

    let package = PackageBuilder::new()
        .manifest(manifest)
        .code(code)
        .build()
        .unwrap();

    let bytes = package.to_bytes();
    std::fs::write("hello-world.logos-plugin", &bytes).unwrap();
    println!("Package created: {} bytes", bytes.len());
}
```

### Signed Package

For marketplace distribution, sign your package:

```rust
let signing = SigningContext::new();
println!("Your publisher key: {}", signing.public_key().to_hex());

let package = PackageBuilder::new()
    .manifest(manifest)
    .code(code)
    .sign(&signing)
    .build()
    .unwrap();
```

---

## Step 5: Install the Plugin

### Programmatic Installation

```rust
use logos_plugins::*;

// Read the package
let bytes = std::fs::read("hello-world.logos-plugin").unwrap();
let package = PluginPackage::from_bytes(&bytes).unwrap();

// Verify integrity
package.verify_integrity().unwrap();

// Install to registry
let mut registry = PluginRegistry::new();
registry.install(
    package.manifest.clone(),
    package.content_hash.clone(),
    RegistrySource::Local,
    package.is_signed(),
    package.signature.as_ref().map(|s| s.signer_public_key()),
).unwrap();

println!("Installed: {}", package.name());
```

---

## Step 6: Load and Run

```rust
use logos_plugins::*;
use logos_core::Document;
use std::sync::{Arc, RwLock};

// Create or open a document
let document = Arc::new(RwLock::new(Document::new()));

// Create the plugin manager
let mut manager = PluginManager::new(document);

// Load the plugin
let manifest = PluginManifest::new("Hello World")
    .with_version(1, 0, 0)
    .with_entry_point("plugin.js")
    .with_permissions(PermissionSet::read_only());

let plugin_id = manager.load(manifest).unwrap();

// Execute the plugin code
let code = std::fs::read_to_string("plugin.js").unwrap();
manager.execute(&plugin_id, &code).unwrap();

// Check state
assert_eq!(manager.state(&plugin_id), Some(&PluginState::Running));
```

---

## What's Next?

Now that you've built your first plugin, explore these topics:

| Topic | Guide |
|-------|-------|
| **UI Components** | [UI Components Reference](../api/ui-components.md) — Build rich panel interfaces |
| **Document Access** | [JavaScript API](../api/javascript-api.md) — Full document manipulation API |
| **Permissions** | [Permissions Reference](../api/permissions.md) — Security model deep dive |
| **Events** | [Events Reference](../api/events.md) — React to document changes |
| **Marketplace** | [Publishing Guide](publishing-guide.md) — Share your plugin with the world |
| **Examples** | [Example Plugins](../examples/) — Complete working examples |

---

## Troubleshooting

### "PermissionDenied" error
Make sure your manifest declares all required permissions. If your plugin calls `Logos.createRect()`, you need `"document": ["read", "write"]`.

### "TimeLimitExceeded" error
Your plugin code exceeds the default 10ms execution limit. Break work into smaller steps, or use `Logos.checkTimeout()` in loops to fail gracefully.

### Plugin doesn't load
Check that `entry_point` in your manifest points to a valid JavaScript file. The file extension must be `.js` for the JavaScript engine to be used.

### Events not firing
Events are rate-limited to ~60fps. Very rapid changes may be coalesced. Use `Logos.on()` to register listeners — they will be called during the next `flush()` cycle.
