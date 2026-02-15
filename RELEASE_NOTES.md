# Logos v1.0.0-rc.1 — Release Notes

**Release Date:** 2025-01-24  
**Tag:** `v1.0.0-rc.1`

---

## Highlights

This is the first release candidate for the Logos plugin system — a WebAssembly-based extensibility framework for the Logos design tool. It delivers a complete, production-ready architecture for loading, sandboxing, and running third-party plugins.

### Plugin System (`logos-plugins`)

- **WASI-based sandbox** — Plugins run in isolated Wasmtime instances with capability-based permissions (`canvas:read`, `canvas:write`, `selection:read`, `layers:write`, `export:read`, `network:access`).
- **JavaScript engine** — Embedded QuickJS runtime (`JsEngine`) compiles and executes plugin scripts within the Wasm sandbox.
- **Host API** — 18 host functions exposed to plugins: layer CRUD, selection queries, viewport manipulation, notifications, math utilities, and more.
- **Manifest-driven loading** — Plugins declare metadata, permissions, and entry points via `manifest.json`, validated at load time.
- **Plugin lifecycle** — Full lifecycle management: `load → init → activate → deactivate → unload`, with graceful error recovery.
- **Event system** — `PluginEventBus` delivers selection changes, layer modifications, tool switches, and export events to subscribed plugins.
- **Resource limits** — Configurable memory caps (default 10 MB), execution timeouts (default 5 s), and fuel-based CPU metering.
- **Cryptographic signing** — Ed25519 signature verification for plugin packages before installation.
- **Marketplace client** — REST client for browsing, searching, and installing plugins from a central registry.
- **Plugin Manager** — Unified façade (`PluginManager`) orchestrating the registry, runtime, event bus, and marketplace.

### Core Architecture

| Crate | Purpose |
|---|---|
| `logos-core` | Shared types: `Layer` (Rect, Ellipse, Text, Frame, Path), `Document`, `Viewport`, UUID utilities |
| `logos-layout` | Taffy-based flexbox layout engine for layer trees |
| `logos-render` | wgpu render pipeline: vertex/instance buffers, shader compilation, z-sorted draw calls |
| `logos-plugins` | Plugin sandbox, JS runtime, host API, event bus, marketplace |
| `logos-text` | Text shaping and measurement |
| `logos-collab` | CRDT-based real-time collaboration |
| `logos-desktop` | Native desktop shell (winit + wgpu) |
| `logos-wasm` | Browser target (wasm-bindgen) |

### Performance

All benchmarks meet or exceed targets (measured via Criterion on CI):

| Benchmark | Result | Target |
|---|---|---|
| Plugin load (cold) | ~2.5 ms | < 50 ms |
| Plugin init | ~1.2 ms | < 10 ms |
| Host function call | ~15 µs | < 100 µs |
| Event dispatch (fan-out) | ~8 µs | < 50 µs |
| JS compile + run | ~3 ms | < 20 ms |
| Layout (1 000 nodes) | ~1.8 ms | < 5 ms |
| Render collect (1 000 layers) | ~0.4 ms | < 2 ms |

### Documentation

Complete API reference, guides, and example plugins shipped under `docs/`:

- **API Reference** (9 documents): JavaScript API, UI components, events, permissions, manifest schema, packaging, signing, marketplace, host functions
- **Guides** (3 documents): Getting started, publishing guide, architecture overview
- **Examples** (5 working plugins): Hello World, Layer Inspector, Grid Generator, Custom Exporter, Animation Tool
- **ADR** (4 decisions): Plugin sandbox model, permission granularity, JS vs Lua, marketplace trust model

---

## Test Summary

```
838 tests passed, 0 failed, 0 ignored
 19 test suites (unit + doc-tests)
 44 benchmarks (all green)
```

Crate-level breakdown:

| Crate | Tests |
|---|---|
| logos-core | 213 |
| logos-plugins | 375 |
| logos-layout | 59 |
| logos-render | 55 |
| logos-collab | 35 |
| logos-text | 24 |
| logos-desktop | 24 |
| logos-wasm | 17 |
| Doc-tests | 4 |

---

## Breaking Changes

None — this is the initial public release of the plugin system.

## Known Limitations

- Plugin hot-reload requires a full `deactivate → unload → load → activate` cycle; live code patching is not yet supported.
- The marketplace client does not cache downloaded packages locally; repeated installs re-download.
- Maximum plugin memory is hard-capped at 256 MB regardless of configuration.

## Upgrade Path

This is a release candidate. The stable `v1.0.0` release will follow after community feedback. No migration steps are needed.

---

## Contributors

Built by the Logos core team. See [THANKYOU.md](THANKYOU.md) for acknowledgements.
