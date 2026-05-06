# Logos

<p align="center">
  <img src="https://img.shields.io/badge/Status-Active_Development-brightgreen.svg" alt="Status">
  <img src="https://img.shields.io/badge/Language-Rust_2021-orange.svg" alt="Rust">
  <img src="https://img.shields.io/badge/Renderer-WGPU_%2F_WebGPU-blue.svg" alt="WGPU">
  <img src="https://img.shields.io/badge/Collaboration-CRDT_%28yrs%29-purple.svg" alt="CRDT">
  <img src="https://img.shields.io/badge/License-MPL--2.0-lightgrey.svg" alt="License">
  <img src="https://img.shields.io/badge/Tests-676%20passing-success.svg" alt="Tests">
</p>

---

## What Is Logos?

**Logos** is a high-performance, open-source collaborative design tool built entirely in Rust. It is designed to compete with industry-standard tools such as Figma, Sketch, and Adobe XD by combining native GPU rendering, real-time multi-user collaboration, an offline-first architecture, and a sandboxed plugin system — all within a single, unified codebase.

The project targets both a **native desktop application** and a **WebAssembly / WebGPU web target**, sharing the same core engine across both platforms. It is structured as a Cargo workspace with clearly separated crates for rendering, layout, text shaping, collaboration, identity management, and plugin execution.

### Core Design Principles

| Principle | How Logos Achieves It |
|---|---|
| **Performance** | GPU-accelerated rendering via `wgpu`; instance batching; O(delta) incremental updates |
| **Real-Time Collaboration** | CRDT-based document model (`yrs`); WebSocket sync; offline edit queue with conflict resolution |
| **Extensibility** | Sandboxed WASM plugin runtime (Wasmtime); TOML manifest with permission model; plugin marketplace |
| **Privacy and Offline-First** | Local document storage; offline tracker queues edits while disconnected; sync-on-reconnect |
| **Cross-Platform** | Native desktop (winit + wgpu); WebAssembly target (`wasm32-unknown-unknown`); shared core |

---

## Project Architecture

```
Logos (Cargo Workspace)
├── logos-core            # Shared types, node model, traits, FFI safety layer
├── logos-render          # wgpu render pipeline — instance batching, atlas, GPU-driven draw
├── logos-layout          # Constraint-based layout engine with Taffy (Flexbox/Grid)
├── logos-text            # Text shaping via cosmic-text; glyph atlas; shaped-run cache
├── logos-collab          # Real-time collaboration — CRDT sessions, RBAC, REST API, conflict resolution
├── logos-desktop         # Desktop application — UI state machines, tool FSM, HTTP client
├── logos-wasm            # WebAssembly editor — egui/eframe UI, Figma-style canvas, trunk build
├── logos-plugin-system   # Plugin SDK, marketplace client, permission model
└── logos-plugins         # Built-in and example plugins
```

---

## Technology Stack

### Core Language and Runtime

| Technology | Version | Role |
|---|---|---|
| **Rust** | 2021 Edition (stable) | Primary language — memory safety, zero-cost abstractions, fearless concurrency |
| **Tokio** | 1.49 | Async runtime — powers collaboration server, HTTP client, background tasks |
| **Cargo Workspace** | — | Monorepo management; shared dependency resolution across 9+ crates |

### Rendering

| Technology | Role |
|---|---|
| **wgpu** | Cross-platform GPU abstraction (Vulkan, Metal, DX12, WebGPU) — render pipeline, shaders, instance batching |
| **WebGPU** | Web rendering backend via `wasm32-unknown-unknown` target |

### Collaboration and CRDTs

| Technology | Version | Role |
|---|---|---|
| **yrs (yjs-rs)** | 0.25.0 | CRDT-based document model; conflict-free merge; awareness/presence |
| **Axum** | 0.8 | Async HTTP/REST server — 20+ endpoints across auth, companies, projects, sessions, conflicts |
| **Tower / Tower-HTTP** | 0.5 / 0.6 | Middleware stack: CORS, request tracing, timeout layers |
| **tokio-tungstenite** | — | WebSocket transport for real-time CRDT sync |

### Networking and HTTP Client

| Technology | Version | Role |
|---|---|---|
| **reqwest** | 0.12 | Async HTTP client (rustls-tls); used by `logos-desktop` to communicate with the collab server |
| **serde / serde_json** | 1 | JSON serialization for all REST DTOs and plugin communication |
| **bincode** | — | Binary serialization for CRDT deltas and internal wire format |

### Security and Identity

| Technology | Role |
|---|---|
| **argon2 (0.5.3)** | Argon2id password hashing with configurable memory/time cost |
| **hmac + sha2** | HMAC-SHA256 token issuance and verification |
| **Ed25519** | Asymmetric keypair generation; plugin signature verification; marketplace auth |
| **JWT** | Session tokens for marketplace API |

### Layout and Text

| Technology | Role |
|---|---|
| **Taffy** | Flexbox and CSS Grid layout engine — constraint solving, node tree diffing |
| **cosmic-text** | Unicode text shaping, font fallback, bidirectional text, glyph atlas management |

### Plugin System

| Technology | Role |
|---|---|
| **Wasmtime** | WASM plugin runtime — sandboxed execution with fuel limits and memory caps |
| **wasm-bindgen** | JS to Rust binding generation for the WebAssembly target |
| **QuickJS** | JavaScript runtime for lightweight JS plugins |

### Storage and Persistence

| Technology | Role |
|---|---|
| **RocksDB** | Optional persistent storage for collaboration sessions (feature-gated `persistent-storage`) |
| **PostgreSQL** | Marketplace database — 7 tables: publishers, plugins, versions, reviews, downloads, categories, audit_log |

### AI and ML

| Technology | Role |
|---|---|
| **ort (ONNX Runtime v2)** | AI inference — layout suggestion, style transfer; FP16 quantized models (6.48 MB to 1.63 MB, -75%) |
| **Criterion** | Benchmarking harness — CRDT hot path, text shaping, layout diffing, GPU pipeline throughput |

---

## Feature Flags

Logos uses Cargo feature flags to keep build times fast and binaries lean:

| Crate | Feature | What It Enables |
|---|---|---|
| `logos-collab` | `http-server` | Axum REST server, Tower middleware, all route handlers |
| `logos-collab` | `persistent-storage` | RocksDB-backed session persistence |
| `logos-collab` | `stress` | 50-user concurrent stress test suite |
| `logos-desktop` | `http-client` | reqwest-based API client, authentication flow |
| `logos-wasm` | *(default)* | wasm-bindgen exports, WebGPU canvas integration |

---

## Prerequisites

Before building Logos, ensure the following tools are installed.

### Required

**Rust (stable, 1.75 or later)**

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup update stable
```

**Git**

```bash
# Debian / Ubuntu
sudo apt install git

# Arch / Manjaro
sudo pacman -S git

# macOS
brew install git
```

### For WebAssembly Builds

**wasm-pack**

```bash
cargo install wasm-pack
rustup target add wasm32-unknown-unknown
```

### For Marketplace (PostgreSQL Backend)

```bash
# Debian / Ubuntu
sudo apt install postgresql postgresql-contrib

# Arch / Manjaro
sudo pacman -S postgresql

# macOS
brew install postgresql@14
```

### For Optional RocksDB Persistent Storage

```bash
# Debian / Ubuntu
sudo apt install librocksdb-dev clang

# Arch / Manjaro
sudo pacman -S rocksdb clang
```

---

## Installation

### 1. Clone the Repository

```bash
git clone https://github.com/navidrezadoost/Logos.git
cd Logos
```

### 2. Build the Core Library

```bash
cargo build -p logos-core
```

### 3. Build the Full Workspace

```bash
cargo build --workspace
```

The first build resolves and compiles all dependencies. Expect 3 to 10 minutes on first run.

### 4. Verify the Build

```bash
cargo check --workspace
```

---

## Running Tests

```bash
# All crates, no optional features (fastest)
cargo test --workspace --no-default-features

# Collaboration backend only
cargo test -p logos-collab --no-default-features

# Desktop client only
cargo test -p logos-desktop --no-default-features

# With HTTP server feature enabled
cargo test -p logos-collab --features http-server
```

Expected results:

```
test result: ok. 676 passed; 0 failed  (logos-collab)
test result: ok. 279 passed; 0 failed  (logos-desktop)
```

---

## Running the Collaboration Server

```bash
cargo run -p logos-collab --features http-server
```

The server starts at `http://0.0.0.0:8080`.

### REST API Reference

| Method | Path | Description |
|---|---|---|
| `POST` | `/api/auth/register` | Create a new user account |
| `POST` | `/api/auth/login` | Authenticate and receive a bearer token |
| `POST` | `/api/auth/logout` | Revoke the current session token |
| `GET` | `/api/companies` | List companies for the authenticated user |
| `POST` | `/api/companies` | Create a new company workspace |
| `GET` | `/api/companies/:id` | Get company details |
| `PATCH` | `/api/companies/:id` | Update company metadata |
| `POST` | `/api/companies/:id/members` | Add a member (`Admin / Editor / Viewer`) |
| `GET` | `/api/projects` | List projects |
| `POST` | `/api/projects` | Create a project |
| `GET` | `/api/projects/:id` | Get project details |
| `GET` | `/api/projects/:id/sessions` | List active collaboration sessions |
| `GET` | `/api/sessions/:id` | Get session state |
| `POST` | `/api/sessions/:id/join` | Join a collaboration session |
| `POST` | `/api/sessions/:id/leave` | Leave a session |
| `GET` | `/api/projects/:id/conflicts` | List pending conflicts |
| `POST` | `/api/projects/:id/conflicts` | Report a new conflict |
| `GET` | `/api/conflicts/:id` | Get conflict details |
| `POST` | `/api/conflicts/:id/review` | Assign conflict for review |
| `POST` | `/api/conflicts/:id/resolve` | Resolve with a chosen strategy |
| `POST` | `/api/conflicts/:id/reject` | Reject all conflicting versions |
| `GET` | `/api/projects/:id/sync-status` | Get per-element sync states |

All protected endpoints require:

```
Authorization: Bearer <token>
```

---

## Offline Conflict Resolution

Logos handles divergent edits made while a user is disconnected through a structured conflict resolution pipeline.

### Workflow

```
1. User goes offline
2. Local edits queued in OfflineTracker (Create / Update / Delete)
3. User reconnects — queued edits pushed to the server
4. Server detects divergent versions for the same element
5. ConflictRecord created with status: Pending
6. Reviewer assigned — status: UnderReview
7. Resolution strategy chosen:
     AcceptLocal   — local version wins
     AcceptRemote  — remote version wins
     AcceptBoth    — both versions kept as branches
     RejectAll     — all conflicting edits discarded
8. SyncStatusStore updated — element status: Synced
9. All connected clients notified
```

### Example API Usage

```bash
# List conflicts in a project
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/api/projects/$PROJECT_ID/conflicts

# Assign for review
curl -X POST -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/api/conflicts/$CONFLICT_ID/review

# Resolve a conflict
curl -X POST \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"strategy": "AcceptRemote", "reviewer_id": "'$USER_ID'"}' \
  http://localhost:8080/api/conflicts/$CONFLICT_ID/resolve

# Check sync status
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/api/projects/$PROJECT_ID/sync-status
```

---

## Building the WebAssembly Editor

The `logos-wasm` crate is the browser-based design editor, built with **egui 0.29 / eframe 0.29** and compiled to WebAssembly via **Trunk**.

```bash
# Install Trunk (once)
cargo install trunk

# Development build & dev server (hot-reload)
cd logos-wasm
trunk serve

# Production build
trunk build --release
# Output in logos-wasm/dist/
```

Serve the `dist/` directory with any static file server or use `logos-server` which serves it at `http://localhost:8080`.

### Editor Keyboard Shortcuts

| Key | Tool / Action |
|-----|---------------|
| `V` | Move tool — select and drag layers |
| `K` | Scale tool — select with proportional scale intent |
| `H` | Hand tool — pan canvas (no Space key needed) |
| `F` | Frame tool |
| `R` | Rectangle tool |
| `E` | Ellipse tool |
| `N` | Polygon tool |
| `T` | Text tool |
| `P` | Pen tool |
| `G` | Toggle grid |
| `Space + drag` | Temporary pan (any tool) |
| `Ctrl+Z / Ctrl+Shift+Z` | Undo / Redo |
| `Ctrl+C / Ctrl+X / Ctrl+V` | Copy / Cut / Paste |
| `Ctrl+D` | Duplicate selection |
| `Ctrl+A` | Select all |
| `Delete / Backspace` | Delete selected layers |
| `Shift+click` | Toggle multi-select |
| `Escape` | Clear selection / cancel tool |

### Toolbar Layout

The toolbar floats as a **dark pill at the bottom-centre** of the canvas (Figma-style):

- **Move-mode dropdown** — shows the active move tool (Move / Scale / Hand); click to open a popup and switch modes.
- **Shape-tool dropdown** — shows the last-used shape tool; click to open a popup listing Frame, Rect, Ellipse, Polygon, Text, Pen.
- **Zoom controls** — `−` / zoom% / `+`; click the percentage to reset to 100%.
- **Grid toggle** — `#` / `.`
- **Fit button** — `[ ]` — fits the canvas to the viewport.

### Alignment (Right Panel → Transform)

Select one or more layers and use the Align buttons:

| Action | Single selection | Multi-selection |
|--------|-----------------|-----------------|
| Align Left | Align to page left edge | Align all to the leftmost edge in the group |
| Center H | Center on page horizontal axis | Center all on group's horizontal midpoint |
| Align Right | Align to page right edge | Align all to the rightmost edge in the group |
| Align Top | Align to page top edge | Align all to the topmost edge in the group |
| Center V | Center on page vertical axis | Center all on group's vertical midpoint |
| Align Bottom | Align to page bottom edge | Align all to the bottommost edge in the group |

---

## Running Benchmarks

```bash
# CRDT and collaboration benchmarks
cargo bench -p logos-collab

# Render pipeline benchmarks
cargo bench -p logos-render

# Layout engine benchmarks
cargo bench -p logos-layout

# Text shaping benchmarks
cargo bench -p logos-text
```

HTML reports are generated in `target/criterion/report/index.html`.

### Known Benchmark Results

| Subsystem | Operation | Result |
|---|---|---|
| CRDT | `add_layer_delta` | 24% faster with deferred delta encoding |
| Batch API | `commit` at N=10 | 50% faster with batch transactions |
| Atlas | Lookup | 86% faster with O(1) flat-array indexing |
| Spatial hash | Hit testing | 88% faster with bitflag permissions |
| Text shaping | Shaping latency | 97.5% reduction via shaped-run cache |
| AI (ONNX) | Layout generation | 30.9 µs per 10 variations |
| AI (ONNX) | Style transfer | 32.2 µs per call |

---

## Stress Testing

```bash
cargo test -p logos-collab --no-default-features --features stress -- --nocapture
```

Simulates 50 concurrent users with randomized edits, network partition, and reconnect. An HTML report is generated at `target/stress-report.html`.

---

## Plugin Development

Plugins are compiled WASM modules loaded by the Wasmtime runtime with sandboxed execution.

### Create a Plugin

```bash
cargo new my-logos-plugin --lib
```

`Cargo.toml`:

```toml
[lib]
crate-type = ["cdylib"]

[target.'cfg(target_arch = "wasm32")'.dependencies]
logos-plugin-sdk = { path = "../../logos-plugin-system/sdk" }
```

`src/lib.rs`:

```rust
#[no_mangle]
pub extern "C" fn plugin_init() {
    logos_plugin_sdk::register_command("insert-rect", on_insert_rect);
}

fn on_insert_rect() {
    logos_plugin_sdk::insert_shape("rectangle", 0.0, 0.0, 200.0, 100.0);
}
```

`plugin.toml` manifest:

```toml
[plugin]
name    = "My Plugin"
version = "1.0.0"
author  = "Your Name"

[permissions]
document_write = true
selection_read = true
```

Build and install:

```bash
cargo build --target wasm32-wasi --release
cp target/wasm32-wasi/release/my_logos_plugin.wasm ~/.logos/plugins/
cp plugin.toml ~/.logos/plugins/
```

---

## Contributing

Please read [CONTRIBUTING.md](./CONTRIBUTING.md) and [CODE_OF_CONDUCT.md](./CODE_OF_CONDUCT.md) before opening a pull request.

All submissions must:
- Include unit tests for new functionality
- Pass `cargo fmt --all` and `cargo clippy --workspace -- -D warnings`
- Update the relevant section of `CHANGELOG.md`
- Use Conventional Commit messages: `feat:`, `fix:`, `docs:`, `refactor:`, `test:`

---

## License

Logos is licensed under the **Mozilla Public License 2.0 (MPL-2.0)**. See [LICENSE](./LICENSE) for the full text.

---

## Acknowledgements

- [Penpot](https://penpot.app/) — open-source design tool, architectural reference
- [yjs / yrs](https://github.com/y-crdt/y-crdt) — CRDT engine
- [wgpu](https://github.com/gfx-rs/wgpu) — cross-platform GPU abstraction
- [Taffy](https://github.com/nickel-lang/taffy) — layout engine
- [cosmic-text](https://github.com/pop-os/cosmic-text) — text shaping
- [Axum](https://github.com/tokio-rs/axum) — async web framework
