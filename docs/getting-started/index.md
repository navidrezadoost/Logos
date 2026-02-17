---
title: Getting Started
desc: Build Logos from source, run the test suite, and launch the desktop application.
eleventyNavigation:
  key: Getting Started
  order: 2
---

# Getting Started with Logos

Logos is a high-performance open-source design tool built entirely in Rust. This guide walks you through building from source, running tests, and launching the desktop application.

## Prerequisites

### System Requirements

| Requirement | Minimum | Recommended |
|-------------|---------|-------------|
| OS | Linux, macOS, Windows | Linux (Wayland/X11) |
| Rust | 1.75 stable | Latest stable via rustup |
| GPU | Vulkan/Metal/DX12 capable | Dedicated GPU |
| RAM | 4 GB | 8 GB+ |
| Disk | 2 GB (build artifacts) | SSD recommended |

### Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
rustup update stable
```

### System Dependencies

**Linux (Debian/Ubuntu):**

```bash
sudo apt-get update
sudo apt-get install -y libclang-dev libfontconfig1-dev \
  pkg-config libx11-dev libxkbcommon-dev libwayland-dev
```

**Linux (Arch/Manjaro):**

```bash
sudo pacman -S clang fontconfig pkgconf libx11 libxkbcommon wayland
```

**macOS:**

```bash
brew install llvm fontconfig
```

**Windows:**

Install [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) and [LLVM](https://releases.llvm.org/).

### WASM Target (Optional)

```bash
rustup target add wasm32-unknown-unknown
```

---

## Clone and Build

```bash
# Clone the repository
git clone https://github.com/navidrezadoost/Logos.git
cd Logos

# Build the entire workspace (19 crates)
cargo build --workspace

# Build in release mode (optimized)
cargo build --workspace --release
```

First build takes 2-5 minutes depending on your hardware (615 dependencies).

---

## Run Tests

```bash
# Run all 2,007 tests
cargo test --workspace

# Run tests for a specific crate
cargo test -p logos-core        # 47 tests
cargo test -p logos-layout      # 59 tests
cargo test -p logos-render      # 47 tests
cargo test -p logos-text        # 48 tests
cargo test -p logos-collab      # 213 tests
cargo test -p logos-plugins     # 596 tests
cargo test -p logos-desktop     # 212 tests
cargo test -p logos-ai          # 235 tests

# Run with output visible
cargo test -p logos-core -- --nocapture

# Run a specific test
cargo test -p logos-plugins -- wasm_runtime::tests::test_fuel_limit
```

### GPU Tests

Some tests require a GPU surface and will fail in headless environments. Skip them with:

```bash
cargo test --workspace -- \
  --skip "headless" --skip "surface" \
  --skip "prepare_uploads" --skip "atlas" \
  --skip "demo_scene_creates" --skip "font_registry" \
  --skip "hover_adds" --skip "selection_adds" \
  --skip "text_instances" --skip "typography_showcase"
```

---

## Launch the Desktop App

```bash
# Debug build
cargo run -p logos-desktop

# Release build (recommended for performance)
cargo run -p logos-desktop --release
```

The desktop app opens a 1280×800 window with the full design canvas, toolbars, panels, and command palette.

### Keyboard Shortcuts

| Key | Tool |
|-----|------|
| V | Select / Move |
| R | Rectangle |
| O | Ellipse |
| T | Text |
| P | Pen (path) |
| H | Hand (pan) |
| Z | Zoom |
| F | Frame |
| L | Line |
| I | Eyedropper |
| Ctrl+Shift+P | Command Palette |
| Ctrl+Z | Undo |
| Ctrl+Shift+Z | Redo |
| Ctrl+S | Save |
| Escape | Cancel / Close |

---

## Build WASM Web Target

```bash
# Build the WASM binary
cargo build --target wasm32-unknown-unknown -p logos-wasm --release

# The output is at:
# target/wasm32-unknown-unknown/release/logos_wasm.wasm
```

To run the web demo, copy the WASM file to `logos-wasm/web/` and open `index.html` in a WebGPU-capable browser (Chrome 113+, Edge 113+, Firefox Nightly).

---

## Run Benchmarks

```bash
# All benchmarks
cargo bench --workspace

# Specific crate benchmarks
cargo bench -p logos-core      # CRDT operations
cargo bench -p logos-layout    # Layout solver + spatial hash
cargo bench -p logos-render    # GPU pipeline + frame cache
cargo bench -p logos-text      # Text shaping + atlas
cargo bench -p logos-plugins   # Plugin runtime overhead
cargo bench -p logos-ai        # ONNX inference latency

# Results saved to target/criterion/
# Open target/criterion/report/index.html for interactive charts
```

### Performance Baselines (v2.0.0-rc.1)

| Operation | Latency | Notes |
|-----------|---------|-------|
| CRDT `add_layer_delta` | 291 ns | Deferred delta encoding |
| Batch commit (N=10) | 755 ns | 50% faster than sequential |
| Spatial hit test | 13.6 ns | Inline-AABB grid |
| Layout recompute | 308 ns | Subtree invalidation |
| Frame update | 3.02 ns | Retained instance buffer |
| Text shaping (cached) | 102 ns | Shaped-run cache |
| Atlas lookup | O(1) | Flat-array indexing |

---

## Project Structure

```
Logos/
├── logos-core/          # CRDT document model
├── logos-layout/        # Constraint layout + spatial hash
├── logos-render/        # wgpu GPU pipeline
├── logos-text/          # Text shaping + atlas
├── logos-collab/        # WebSocket collaboration
├── logos-desktop/       # Native desktop app (winit + wgpu)
├── logos-wasm/          # WebAssembly target
├── logos-plugins/       # WASM + JS plugin runtime
├── logos-ai/            # ONNX inference engine
├── logos-import-*/      # 7 format importers
├── logos-marketplace-*/ # 3 marketplace crates
├── docs/                # This documentation site
├── .github/             # CI workflows + issue templates
└── Cargo.toml           # Workspace definition
```

---

## What's Next?

- **[API Reference](/api-reference/)** — Full documentation for all 19 crates
- **[Plugin Developer Guide](/plugin-guide/)** — Build and publish plugins
- **[Architecture](/technical-guide/)** — CRDT engine, GPU pipeline, collaboration protocol
- **[Contributing](/contributing-guide/)** — PR workflow, code style, release process
