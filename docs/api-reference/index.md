---
title: API Reference
desc: Complete API reference for all 19 Logos workspace crates.
eleventyNavigation:
  key: API Reference
  order: 3
---

# API Reference

Logos is a Rust workspace with **19 crates** organized into 7 layers. Each crate has full `rustdoc` documentation generated from inline `///` comments.

## Generate Local API Docs

```bash
cargo doc --workspace --no-deps --open
```

This opens the full `rustdoc` output in your browser at `target/doc/logos_core/index.html`.

---

## Core Engine

The foundation layer — document model, layout, rendering, and text.

### logos-core

**CRDT-based document model** with operational transform for real-time collaboration.

| Module | Description |
|--------|-------------|
| `Document` | Top-level document container with page management |
| `Layer` | Design element (frame, rectangle, ellipse, text, path, group) |
| `Transaction` | Batch mutation API — 50% faster at N=10 |
| `CrdtState` | CRDT state with vector clocks and delta encoding |
| `LayerId` / `PageId` | Type-safe identifiers |

**Key types:**

```rust
use logos_core::{Document, Layer, LayerKind, Transaction};

let mut doc = Document::new();
let mut tx = Transaction::new();
tx.add_layer(Layer::new(LayerKind::Rectangle));
tx.add_layer(Layer::new(LayerKind::Ellipse));
doc.commit(tx); // Batch commit — O(1) amortized
```

**Performance:** `add_layer_delta` at 291ns (24% improvement via deferred delta encoding).

**Crate docs:** `cargo doc -p logos-core --open`

---

### logos-layout

**Constraint-based layout engine** with Taffy integration and spatial indexing.

| Module | Description |
|--------|-------------|
| `LayoutEngine` | Taffy-backed constraint solver with subtree invalidation |
| `SpatialHash` | Inline-AABB grid for O(1) hit testing (13.6ns) |
| `LayoutDiff` | Incremental layout diffing with FxHashMap |

**Key types:**

```rust
use logos_layout::{LayoutEngine, SpatialHash};

let mut engine = LayoutEngine::new();
let mut spatial = SpatialHash::new(64.0); // 64px cell size
spatial.insert(layer_id, aabb);
let hits = spatial.query_point(mouse_x, mouse_y); // 13.6ns
```

**Performance:** Layout recompute at 308ns (99.9% reduction via subtree walk).

**Crate docs:** `cargo doc -p logos-layout --open`

---

### logos-render

**GPU rendering pipeline** built on wgpu 24 with instance batching and frame coherence.

| Module | Description |
|--------|-------------|
| `Renderer` | Main render loop with draw indirect |
| `GpuContext` | wgpu device, queue, and surface management |
| `FrameCache` | Retained instance buffer with O(Δ) incremental updates |
| `RectPipeline` | Rectangle/ellipse instanced rendering |
| `Bridge` | Core → Render data bridge with deferred delta encoding |

**Key types:**

```rust
use logos_render::{Renderer, GpuContext};

let gpu = GpuContext::new_with_surface(window, width, height).await;
let mut renderer = Renderer::new(&gpu);
renderer.prepare(&document, &layout);
renderer.render(&gpu); // GPU-driven with partial buffer uploads
```

**Performance:** Frame updates at 3.02ns (99.9% via retained instance buffer), GPU uploads reduced by 99.9% via dirty-slot tracking.

**Crate docs:** `cargo doc -p logos-render --open`

---

### logos-text

**Text shaping and rendering** with cosmic-text and glyph atlas management.

| Module | Description |
|--------|-------------|
| `TextEngine` | cosmic-text shaping with shaped-run cache |
| `Atlas` | O(1) flat-array glyph atlas lookup (86% faster) |
| `FontRegistry` | System font enumeration and loading |

**Key types:**

```rust
use logos_text::{TextEngine, Atlas};

let mut engine = TextEngine::new();
let shaped = engine.shape("Hello, Logos!", &font, 16.0); // 102ns cached
let mut atlas = Atlas::new(1024, 1024);
atlas.insert_glyph(glyph_id, &bitmap); // O(1) flat-array lookup
```

**Performance:** Text shaping at 102ns cached (97.5% improvement via shaped-run cache).

**Crate docs:** `cargo doc -p logos-text --open`

---

## Collaboration

### logos-collab

**Real-time collaboration server** with WebSocket transport, CRDT sync, and presence.

| Module | Description |
|--------|-------------|
| `WebSocketServer` | tokio-tungstenite transport with room management |
| `CrdtSync` | State-based CRDT merge with vector clocks |
| `Presence` | Cursor position and selection broadcasting |
| `Storage` | RocksDB persistence with WAL |
| `Auth` | JWT token validation and session management |

**Crate docs:** `cargo doc -p logos-collab --open`

---

## Platform Targets

### logos-desktop

**Native desktop application** using winit 0.30 for windowing and wgpu 24 for rendering.

| Module | Description |
|--------|-------------|
| `commands` | 60+ command variants with registry and undo history |
| `shortcuts` | Figma-compatible keyboard shortcuts (V/R/O/T/P/H/Z/F/L/I) |
| `toolbar` | 3 preset toolbars with hit-testing |
| `panels` | 7 dockable panels (Layers, Properties, Library, History, Color, Typography, Export) |
| `palette` | Fuzzy-search command palette with MRU tracking |
| `tabs` | Multi-document tab bar with dirty indicators and pinning |
| `presence` | Multiplayer cursor rendering |
| `marketplace` | Plugin gallery and publisher tools |

**Crate docs:** `cargo doc -p logos-desktop --open`

---

### logos-wasm

**WebAssembly target** for running Logos in the browser via WebGPU.

| Export | Description |
|--------|-------------|
| `LogosApp::new()` | Initialize the WASM engine |
| `create_document()` | Create a new document |
| `add_rectangle()` | Add a rectangle to the canvas |
| `set_viewport()` | Set camera position and zoom |
| `render()` | Render the current frame |
| 18 more... | Full list in rustdoc |

**Usage:**

```javascript
import init, { LogosApp } from './logos_wasm.js';

await init();
const app = LogosApp.new(canvas);
app.create_document("My Design");
app.add_rectangle(100, 100, 200, 150, "#3b82f6");
app.render();
```

**Build:**

```bash
cargo build --target wasm32-unknown-unknown -p logos-wasm --release
```

**Crate docs:** `cargo doc -p logos-wasm --open`

---

## Extensibility

### logos-plugins

**Plugin runtime** supporting both JavaScript (QuickJS) and WASM (Wasmtime) sandboxed execution.

| Module | Description |
|--------|-------------|
| `JsEngine` | QuickJS-based JavaScript runtime |
| `WasmRuntime` | Wasmtime sandbox with fuel (1M ops) and memory limits (50MB) |
| `Manifest` | TOML plugin manifest parser and validator |
| `Signing` | Ed25519 signature creation and verification |
| `PluginManager` | Lifecycle management (load, start, stop, unload) |
| `MarketplaceHttp` | Search, download, publish, rate, review |
| `PermissionPrompt` | User-facing permission approval |

**21 Host Functions:**

| Category | Functions |
|----------|-----------|
| Document | `get_document`, `get_layers`, `add_layer`, `remove_layer`, `update_layer` |
| Selection | `get_selection`, `set_selection`, `clear_selection` |
| Viewport | `get_viewport`, `set_viewport`, `zoom_to_fit` |
| UI | `show_notification`, `show_dialog`, `show_toast` |
| Lifecycle | `on_activate`, `on_deactivate`, `on_event` |
| State | `get_state`, `set_state`, `get_preferences` |

See the [Plugin Developer Guide](/plugin-guide/) for building your own plugins.

**Crate docs:** `cargo doc -p logos-plugins --open`

---

## AI Engine

### logos-ai

**AI-powered design assistance** with ONNX Runtime inference.

| Module | Description |
|--------|-------------|
| `OnnxInference` | Real inference via `ort` v2 with session pooling |
| `ModelManager` | Model loading, caching, and version management |
| `Quantization` | FP32 → FP16 compression (75% size reduction) |
| `Embeddings` | Style extraction and layout suggestion pipelines |

**Model Performance:**

| Model | Latency | Size (FP16) |
|-------|---------|-------------|
| Layout Generator | 30.9µs / 10 variations | 0.41 MB |
| Style Encoder | 32.2µs | 0.41 MB |
| Asset Decoder | 8.6µs | 0.41 MB |

**Crate docs:** `cargo doc -p logos-ai --open`

---

## File Import

7 crates for importing designs from other tools.

| Crate | Format | Key Feature |
|-------|--------|-------------|
| `logos-import-common` | — | Shared `ImportDocument` trait |
| `logos-import-figma` | .fig | Binary parser, 20 node types |
| `logos-import-svg` | .svg | Dependency-free XML + path parser |
| `logos-import-sketch` | .sketch | ZIP + JSON model mapping |
| `logos-import-pdf` | .pdf | Content stream tokenizer |
| `logos-import-xd` | .xd | ZIP/AGC extraction |
| `logos-import-canva` | .json | JSON template parser |

**Usage pattern:**

```rust
use logos_import_figma::FigmaImporter;
use logos_import_common::Importer;

let importer = FigmaImporter::new();
let doc = importer.import(&file_bytes)?;
// doc is now a logos_core::Document
```

**Crate docs:** `cargo doc -p logos-import-figma --open`

---

## Marketplace

3 crates powering the plugin marketplace.

| Crate | Purpose | Key Feature |
|-------|---------|-------------|
| `logos-marketplace-auth` | Authentication | Ed25519 keypairs, JWT sessions |
| `logos-marketplace-db` | Database | PostgreSQL, 7 tables |
| `logos-marketplace-api` | REST Server | 18+ routes |

**API Endpoints (selection):**

```
GET    /api/v1/plugins              # Search plugins
GET    /api/v1/plugins/:id          # Get plugin details
POST   /api/v1/plugins              # Publish plugin
PUT    /api/v1/plugins/:id/versions # Upload new version
GET    /api/v1/plugins/:id/reviews  # Get reviews
POST   /api/v1/plugins/:id/reviews  # Submit review
POST   /api/v1/publishers/register  # Register as publisher
GET    /api/v1/publishers/:id/stats # Publisher analytics
POST   /api/v1/admin/plugins/:id/approve  # Approve plugin
```

**Crate docs:** `cargo doc -p logos-marketplace-api --open`
