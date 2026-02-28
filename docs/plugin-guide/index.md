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

### AI/ML

| Function | Permission | Description |
|----------|-----------|-------------|
| `ai_analyze_design(layers)` | `ai:design` | Run design suggestions analysis |
| `ai_check_accessibility(fg, bg)` | `ai:accessibility` | Check WCAG contrast ratio |
| `ai_generate_palette(base, scheme)` | `ai:color` | Generate color harmony palette |
| `ai_infer_constraints(layers)` | `ai:layout` | Detect grids, alignment rails, spacing |
| `ai_recommend_components(elements)` | `ai:components` | Find repeatable patterns for componentization |
| `ai_run_pipeline(pipeline)` | `ai:pipeline` | Run a custom AI workflow |

---

## AI APIs

Plugins can leverage Logos' AI engine for design analysis, accessibility checks, color harmony, and more.

### Design Suggestions

Analyze layouts for alignment, spacing, overlap, and hierarchy issues:

**WASM (Rust):**

```rust
use logos_plugin_sdk::prelude::*;

#[plugin_command("analyze_design")]
fn analyze_design(host: &dyn HostApi) {
    let layers = host.get_layers();
    let suggestions = host.ai_analyze_design(&layers, AnalyzerConfig::default());
    
    for suggestion in suggestions {
        host.show_notification(
            &format!("🔍 {}: {}", suggestion.kind, suggestion.message),
            NotificationType::Info
        );
    }
}
```

**JavaScript:**

```javascript
function analyzeDesign(host) {
    const layers = host.getLayers();
    const suggestions = host.aiAnalyzeDesign(layers, { 
        alignmentTolerance: 4.0,
        minConfidence: 0.3 
    });
    
    suggestions.forEach(s => {
        host.showNotification(`🔍 ${s.kind}: ${s.message}`, 'info');
    });
}
```

### Accessibility Checker

Validate WCAG contrast ratios:

**WASM (Rust):**

```rust
#[plugin_command("check_contrast")]
fn check_contrast(host: &dyn HostApi) {
    let selection = host.get_selection();
    
    for layer in selection {
        if let Some((fg, bg)) = extract_text_colors(&layer) {
            let result = host.ai_check_accessibility(fg, bg);
            
            if !result.passes_wcag_aa() {
                host.show_notification(
                    &format!("⚠️ {} contrast {:.2}:1 fails WCAG AA", 
                        layer.name, result.ratio),
                    NotificationType::Warning
                );
            }
        }
    }
}
```

**JavaScript:**

```javascript
function checkContrast(host) {
    const selection = host.getSelection();
    
    selection.forEach(layer => {
        const { foreground, background } = extractColors(layer);
        const result = host.aiCheckAccessibility(foreground, background);
        
        if (result.ratio < 4.5) {
            host.showNotification(
                `⚠️ ${layer.name} contrast ${result.ratio.toFixed(2)}:1 fails WCAG AA`,
                'warning'
            );
        }
    });
}
```

### Color Palette Generation

Generate harmonious color schemes:

**WASM (Rust):**

```rust
#[plugin_command("generate_palette")]
fn generate_palette(host: &dyn HostApi) {
    let base_color = host.get_selected_color();
    let palette = host.ai_generate_palette(base_color, HarmonyScheme::Triadic);
    
    // Add to document as color tokens
    for (i, color) in palette.colors.iter().enumerate() {
        host.add_color_token(&format!("Generated Color {}", i + 1), *color);
    }
    
    host.show_toast(&format!("Generated {} colors", palette.colors.len()));
}
```

**JavaScript:**

```javascript
function generatePalette(host) {
    const baseColor = host.getSelectedColor();
    const palette = host.aiGeneratePalette(baseColor, 'Triadic');
    
    palette.colors.forEach((color, i) => {
        host.addColorToken(`Generated Color ${i + 1}`, color);
    });
    
    host.showToast(`Generated ${palette.colors.length} colors`);
}
```

### Smart Constraints

Detect layout patterns automatically:

**WASM (Rust):**

```rust
#[plugin_command("detect_grid")]
fn detect_grid(host: &dyn HostApi) {
    let layers = host.get_selection();
    let constraints = host.ai_infer_constraints(&layers, InferrerConfig::default());
    
    for constraint in constraints {
        match constraint {
            InferredConstraint::GridDetected { rows, cols, .. } => {
                host.show_notification(
                    &format!("📐 Grid detected: {} rows × {} cols", rows, cols),
                    NotificationType::Info
                );
            }
            InferredConstraint::AlignmentRail { axis, value, indices } => {
                host.show_notification(
                    &format!("📏 Alignment rail: {:?} at {:.1}px ({} elements)", 
                        axis, value, indices.len()),
                    NotificationType::Info
                );
            }
            _ => {}
        }
    }
}
```

**JavaScript:**

```javascript
function detectGrid(host) {
    const layers = host.getSelection();
    const constraints = host.aiInferConstraints(layers, { 
        alignmentTolerance: 2.0 
    });
    
    constraints.forEach(c => {
        if (c.type === 'GridDetected') {
            host.showNotification(
                `📐 Grid detected: ${c.rows} rows × ${c.cols} cols`,
                'info'
            );
        }
    });
}
```

### Component Recommendations

Find repeated patterns to componentize:

**WASM (Rust):**

```rust
#[plugin_command("find_components")]
fn find_components(host: &dyn HostApi) {
    let layers = host.get_layers();
    let elements = layers.iter().map(|l| DesignElement {
        index: l.index,
        label: l.name.clone(),
        width: l.bounds.width,
        height: l.bounds.height,
        style_hash: compute_style_hash(l),
        group: l.group_name.clone(),
    }).collect();
    
    let summary = host.ai_recommend_components(&elements, RecommenderConfig::default());
    
    for rec in summary.recommendations {
        host.show_notification(
            &format!("🔄 Component '{}': {} instances, saves {} nodes",
                rec.name, rec.instances.len(), rec.node_savings),
            NotificationType::Info
        );
    }
}
```

**JavaScript:**

```javascript
function findComponents(host) {
    const layers = host.getLayers();
    const elements = layers.map(l => ({
        index: l.index,
        label: l.name,
        width: l.bounds.width,
        height: l.bounds.height,
        styleHash: computeStyleHash(l),
        group: l.groupName
    }));
    
    const summary = host.aiRecommendComponents(elements, { minOccurrences: 2 });
    
    summary.recommendations.forEach(rec => {
        host.showNotification(
            `🔄 Component '${rec.name}': ${rec.instances.length} instances, saves ${rec.nodeSavings} nodes`,
            'info'
        );
    });
}
```

### Pipeline Orchestration

Run complex AI workflows:

**WASM (Rust):**

```rust
#[plugin_command("run_design_review")]
fn run_design_review(host: &dyn HostApi) {
    let pipeline = Pipeline::new("Design Review")
        .add_step(PipelineStep::new("analyze", StepKind::DesignAnalysis))
        .add_step(PipelineStep::new("accessibility", StepKind::AccessibilityAudit))
        .add_step(PipelineStep::new("constraints", StepKind::SmartConstraints))
        .with_timeout(5000);
    
    let result = host.ai_run_pipeline(&pipeline);
    
    if result.success {
        host.show_notification(
            &format!("✅ Design review complete in {}ms", result.total_duration),
            NotificationType::Success
        );
        
        for finding in result.all_findings() {
            host.show_toast(finding);
        }
    } else {
        host.show_notification("❌ Design review failed", NotificationType::Error);
    }
}
```

**JavaScript:**

```javascript
function runDesignReview(host) {
    const pipeline = {
        name: 'Design Review',
        steps: [
            { id: 'analyze', kind: 'DesignAnalysis' },
            { id: 'accessibility', kind: 'AccessibilityAudit' },
            { id: 'constraints', kind: 'SmartConstraints' }
        ],
        timeout: 5000
    };
    
    const result = host.aiRunPipeline(pipeline);
    
    if (result.success) {
        host.showNotification(
            `✅ Design review complete in ${result.totalDuration}ms`,
            'success'
        );
        result.allFindings.forEach(finding => host.showToast(finding));
    }
}
```

### Permissions

To use AI APIs, declare the appropriate permissions:

```toml
[permissions]
document = ["read"]
ai = ["design", "accessibility", "color", "layout", "components", "pipeline"]
```

Individual AI scopes:

- `ai:design` — Design suggestions
- `ai:accessibility` — WCAG checks, color blindness simulation
- `ai:color` — Color harmony, palette generation
- `ai:layout` — Smart constraints, grid detection
- `ai:components` — Component recommendations
- `ai:pipeline` — Pipeline orchestration

Or use `ai = ["all"]` for full access.

### Complete Example: AI-Powered Plugin

**plugin.toml:**

```toml
[plugin]
name = "design-assistant"
version = "1.0.0"
description = "AI-powered design review and optimization"
author = "Your Name"

[runtime]
type = "wasm"
entry = "design_assistant.wasm"

[permissions]
document = ["read", "write"]
selection = ["read"]
ai = ["all"]
ui = ["notifications", "dialogs"]
```

**src/lib.rs:**

```rust
use logos_plugin_sdk::prelude::*;

#[plugin_init]
fn init(host: &dyn HostApi) {
    host.show_notification("Design Assistant AI loaded ✨", NotificationType::Info);
}

#[plugin_command("full_analysis")]
fn full_analysis(host: &dyn HostApi) {
    let layers = host.get_layers();
    
    // 1. Design suggestions
    let design_suggestions = host.ai_analyze_design(&layers, AnalyzerConfig::default());
    let design_issues = design_suggestions.len();
    
    // 2. Accessibility audit
    let mut a11y_issues = 0;
    for layer in &layers {
        if let Some((fg, bg)) = extract_text_colors(layer) {
            let result = host.ai_check_accessibility(fg, bg);
            if !result.passes_wcag_aa() {
                a11y_issues += 1;
            }
        }
    }
    
    // 3. Component opportunities
    let elements = layers.iter().map(to_design_element).collect();
    let components = host.ai_recommend_components(&elements, RecommenderConfig::default());
    let component_count = components.recommendations.len();
    
    // 4. Show summary dialog
    let summary = format!(
        "Design Analysis Complete\n\n\
         ⚠️ Design Issues: {}\n\
         ♿ Accessibility Issues: {}\n\
         🔄 Component Opportunities: {}\n\n\
         Total Node Savings: {}",
        design_issues,
        a11y_issues,
        component_count,
        components.total_savings
    );
    
    host.show_dialog("Analysis Report", &summary);
}
```

See the [API Reference — logos-ai](/api-reference/logos-ai/) for full API documentation.

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
