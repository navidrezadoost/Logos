# Changelog

All notable changes to the Logos project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [v2.0.0-rc.1] - 2026-02-16

Release candidate integrating **25 commits** across **16 feature branches** and **8 workstreams**.
All 19 workspace crates compile. 2,007 tests pass with 0 failures.

### Performance

- **CRDT hot path** — 24% faster `add_layer_delta` via deferred delta encoding (`d30e153`)
- **Batch transaction API** — 50% faster at N=10 with batch commit (`858d046`)
- **Atlas lookup** — 86% faster with O(1) flat-array indexing (`c9f250a`)
- **Spatial hash** — bitflag permissions + inline-AABB for 88% faster hit testing (`afc5a4a`)
- **Text shaping** — shaped-run cache for 97.5% reduction in shaping latency (`17fe0ac`)
- **Layout diffing** — FxHashMap + reusable buffers for layout engine (`008acc1`)
- **Pipeline** — subtree walk + fast collect + pipeline benchmarks (`d47893a`)
- **Frame coherence** — retained instance buffer with O(Δ) incremental updates (`c12ecf0`)
- **GPU-driven rendering** — partial buffer uploads + draw indirect + dirty-slot tracking (`2260724`)
- **Delta encoding** — deferred delta encode + reusable instance buffer (`cecfc0c`)

### Added — Web Platform

- **WASM + WebGPU target** — full engine pipeline compiled to `wasm32-unknown-unknown` (`058dbf4`)
- 23 JS-exported methods via `wasm-bindgen`
- Demo page at `logos-wasm/web/index.html`
- Camera module with pan, zoom, and screen-to-world coordinate mapping

### Added — Plugin System

- **Wasmtime WASM runtime backend** — sandboxed execution with fuel and memory limits (`a79a5a9`)
- **21 host functions** across 6 categories: document, selection, viewport, UI, lifecycle, state (`cd208ec`)
- **TOML manifest system** — structured plugin metadata with permission declarations (`7d2dfbf`)
- **Ed25519 signature verification** — cryptographic plugin signing and validation
- **Marketplace HTTP client** — search, download, publish, rate, review with caching
- **Permission prompt system** — user-facing approval for sensitive operations
- 3 example plugins: hello-world, shape-generator, color-palette

### Added — Desktop UI

- **Command system** — 60+ command variants with `CommandRegistry` and `CommandHistory` (`80194ef`)
- **Shortcut registry** — Figma-compatible tool shortcuts (V/R/O/T/P/H/Z/F/L/I), platform-aware modifiers
- **Toolbar** — 3 preset toolbars with layout-computed hit testing
- **Panel manager** — 7 dockable panels (Layers, Properties, Library, History, Color, Typography, Export)
- **Command palette** — fuzzy-search with MRU tracking and category filtering
- **Tab bar** — multi-document tabs with dirty indicators, pinning, and reorder

### Added — File Format Importers

- **Figma** (.fig) — binary parser supporting 20 node types (`16983d8`)
- **SVG** — dependency-free XML parser with full path data support (`b00f858`)
- **Sketch** — ZIP extraction + JSON model mapping
- **PDF** — content stream tokenizer + page extraction
- **Adobe XD** — ZIP/AGC extraction + artboard mapping
- **Canva** — JSON template parser + element conversion
- **Common** — shared `ImportDocument` trait and `ImportResult` types

### Added — Marketplace

- **`logos-marketplace-auth`** — Ed25519 keypair generation, JWT sessions, permission scoping (`b4795cb`)
- **`logos-marketplace-db`** — PostgreSQL schema with 7 tables (publishers, plugins, versions, reviews, downloads, categories, audit_log)
- **`logos-marketplace-api`** — REST server with 18+ routes for publishing, search, review, and admin
- **Marketplace UI** — publisher onboarding (6-step flow), plugin submission, gallery, analytics, moderation (`9bce46a`)

### Added — AI Engine

- **ONNX Runtime integration** — real inference via `ort` v2 crate (`4c57e96`)
- **Criterion benchmarks** — layout generation (30.9µs/10 variations), style transfer (32.2µs), asset decoding (8.6µs) (`9b57791`)
- **Model quantization** — FP32 → FP16 compression (6.48MB → 1.63MB, -75%) (`1b015bf`)
- **Embedding pipeline** — style extraction and layout suggestion via quantized models
- **WASM portability** — conditional compilation for web and native targets

### Changed

- **CI workflow** — triggers on `v*` tags, tests all 19 crates with updated thresholds, automated GitHub Release with pre-release flag for RC tags (`3b2225c`)
- **Cargo.lock** — regenerated for merged dependency tree (`9563c80`)

## [v1.1.0] - 2026-02-16

### Added

- **AI engine scaffolding** — `logos-ai` crate with simulated ONNX inference and model management (`e6c499b`)
- **ONNX Runtime** — real inference backend via `ort` v2 (`8957548`)
- **AI benchmarks** — criterion benchmarks for inference latency (`569cedd`)
- **Phase 5 roadmap** — ecosystem and intelligence development plan (`0c76634`)

### Changed

- Merged `release/v1.0.0-rc.1` into main (`24319e5`)

## [v1.0.0-rc.1] - 2026-02-10

### Added

- **Core engine** — CRDT-based document model with operational transform
- **Layout engine** — constraint-based layout with Taffy integration
- **Render pipeline** — wgpu-based GPU rendering with instance batching
- **Text engine** — cosmic-text shaping with glyph atlas
- **Collaboration** — WebSocket server with RocksDB persistence and presence tracking
- **Plugin system** — dual JS (QuickJS) and WASM runtime with permission model
- **Desktop shell** — winit 0.30 window with mouse/keyboard input and GPU surface
- **WASM target** — `logos-wasm` crate for WebAssembly compilation
- **CI pipeline** — GitHub Actions workflow with build, test, and WASM verification
- **Documentation** — plugin system API reference, SDK guides, architecture docs

[v2.0.0-rc.1]: https://github.com/navidrezadoost/Logos/compare/v1.1.0...v2.0.0-rc.1
[v1.1.0]: https://github.com/navidrezadoost/Logos/compare/v1.0.0-rc.1...v1.1.0
[v1.0.0-rc.1]: https://github.com/navidrezadoost/Logos/releases/tag/v1.0.0-rc.1
