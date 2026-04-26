# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased] - 2026-04-26

### Changed – Pure Rust Frontend (Zero Java / Zero ClojureScript)

- **logos-wasm**: Completely replaced the ClojureScript/Java frontend with a pure Rust + egui design editor compiled to WebAssembly via `trunk`. No JVM, no Node.js runtime required.
- **logos-wasm/src/state.rs**: Full `EditorState` — pages, layers (Rect/Frame/Ellipse/Text/Path/Group), selection, viewport pan/zoom, drag-to-create, drag-to-move, snap-to-grid, undo/redo history, hit-test, demo scene.
- **logos-wasm/src/tools.rs**: `Tool` enum — Select (V), Frame (F), Rect (R), Ellipse (E), Text (T), Pen (P), Pan (H) — each with icon and keyboard shortcut.
- **logos-wasm/src/panels.rs**: All UI panels — top toolbar with tool buttons and zoom controls, left layers panel with page tabs and inline rename, right properties panel with transform/fill/stroke/opacity/text-content editors.
- **logos-wasm/src/editor.rs**: `LogosEditor` implementing `eframe::App` — canvas drawing with `egui::Painter`, selection handles, grid overlay, pan/zoom interactions, status bar, keyboard shortcuts.
- **logos-wasm/src/lib.rs**: New entry point — `run_app()` looks up `#logos-canvas` from DOM and hands off to `eframe::WebRunner`.
- **logos-wasm/index.html**: Minimal loader — shows splash screen while WASM initialises, calls `run_app()`, hides splash on success.
- **logos-wasm/src/app.rs**: Deleted — old WebGPU `LogosApp` replaced entirely.

### Fixed
- `camera.rs`: Removed dead `logos_render::CameraUniform` import and `uniform()` method (wgpu/logos-render no longer a direct dependency of logos-wasm).
- egui 0.29 API: `Frame::none()`, `epaint::EllipseShape`, `rect_stroke` 3-argument form, `WebRunner::start(HtmlCanvasElement, …)`.

### Infrastructure
- **logos-server**: Confirmed serving new WASM artifacts at `http://localhost:8080` with correct `application/wasm` MIME type.
- **trunk build**: `dist/` — WASM 5.4 MB (debug), JS glue 76 KB; `data-wasm-opt="0"` keeps Binaryen out of the build pipeline.

## [Unreleased] - 2026-04-18

### Added – Option C: AI Agent Track

- **logos-agent-versioning / `storage.rs`**: Content-addressed SHA-256 blob store with deduplication and quota enforcement (10 new tests; 70 total in crate).
- **logos-agent-tracing / `logging.rs`**: Structured JSON logging — level filtering, token-bucket rate limiter, trace-context correlation, runtime level changes (10 new tests; 116 total in crate).
- **logos-agent-marketplace / `submission.rs`**: Developer portal submission API — full lifecycle from Draft → InReview → ChangesRequested → Approved → Published, per-publisher filtering, metadata (10 new tests; 73 total in crate).

### Fixed
- **logos-desktop**: Added missing `AwarenessMessage::PageChange` and `AwarenessMessage::EditingUpdate` arms to `handle_presence_message` match (non-exhaustive pattern left from Phase 14).

## [Unreleased] - 2026-04-16

### Fixed
- **logos-collab**: Gated `rocksdb` behind optional `persistent-storage` feature — resolves `libclang`/bindgen build failure on systems without LLVM installed.
- **logos-collab**: Added `storage/memory.rs` — full in-memory `DocumentStore` drop-in (mirrors RocksDB API exactly; used when `persistent-storage` feature is absent).
- **logos-collab**: Fixed `TokenProvider` trait impl in `auth/token.rs` — corrected `issue`/`verify` method signatures to include `TokenKind` parameter, fixed `is_revoked` return type (`bool` not `Result<bool, _>`), removed invalid deref on `Uuid`.
- **logos-collab**: Fixed `save_snapshot` call sites missing the `version: u64` argument in `server.rs` and `tests/persistence_integration.rs`.
- **logos-collab**: Fixed non-exhaustive `AwarenessMessage` match arms — added `PageChange` and `EditingUpdate` handling in `server.rs` and `presence.rs`.
- **logos-collab**: Fixed missing `page_id`, `editing_state`, `idle_alpha` fields in `CursorRenderData` struct initializers in `tests/presence_integration.rs`.
- **logos-collab**: Gated crash-recovery integration tests behind `persistent-storage` feature (tests require cross-instance disk persistence).

### Tests
- **logos-collab**: 362 tests now passing (up from 0 due to rocksdb build failure).

## [0.2.0] - Week 2: Rendering Engine (Current)

### Added
- **Graphics Engine**: Integrated `wgpu` (v24).
- **Renderer**: Added `renderer.rs` with instance/device creation.
- **Architecture**: Split into `logos-core` and `logos-desktop`.

### Fixed
- **Dependencies**: Resolved `raw-window-handle` conflicts.

## [0.1.0] - Week 1: Inception

### Started
- **Rebranding**: Forked from Penpot, rebranded to Logos.
- **Strategy**: Published Executive Technical Roadmap.
- **Core**: Initialized Rust workspace and Object Model.
