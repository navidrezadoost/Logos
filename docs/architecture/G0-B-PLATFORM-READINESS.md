# G0-B Platform Readiness Matrix

**Immediate target:** browser. **Long-term targets:** WebAssembly plus native macOS, Windows and Linux. This inventory reports crate/API evidence, not a successful matrix build.

| Concern | Browser/WASM evidence | Native evidence | Status / gap |
|---|---|---|---|
| Domain types | `logos-types` is `cdylib` + `rlib`, with optional `wasm-bindgen`; serde and UUID values are represented. | Standard Rust types and `rlib`; dependencies are mostly portable. | **PARTIAL / likely portable:** no canonical Document/Page aggregate; native build does not prove C ABI or Go integration. |
| Layout | `logos-layout` pure Rust math; `logos-layout-wasm` exposes JSON/wasm-bindgen boundary. | Rust crate has no browser imports and supports ordinary native target. | **IMPLEMENTED core, semantics PARTIAL:** Flex/Grid algorithms exist, but `render-wasm` duplicates computation and no parity suite. |
| Token resolver | `logos-tokens` pure Rust serde engine and tests; DT-C3 uses a Rust/WASM bridge on its experimental PR. | Standard Rust library dependencies; native Rust target is structurally viable. | **PARTIAL:** document persistence/authorization is not wired. DT-C3 remains integration candidate outside baseline. |
| Renderer / GPU | `render-wasm/Cargo.toml` uses `skia-safe` with `gl`, SVG and textlayout; current build is Emscripten/wasm. | Skia has platform/native builds in general, but this repo's native target, backend selection, lifecycle and ABI are not evidenced. | **UNVERIFIED:** GL/Skia handles, font managers, image decode and callbacks require platform adapter. |
| JavaScript/WASM ABI | JSON payload/wasm-bindgen in domain bridge; renderer uses Emscripten byte ABI (`BASE_PROPS_SIZE=104`) and Rust exported funcs. | No equivalent stable native boundary for editor document/renderer is shown. | **PARTIAL:** ABI versions, ownership, allocator pairing, error codes and memory growth policy need explicit contracts. |
| Fonts/text | Browser fetch/font registration in app; `render-wasm/src/shapes/text.rs` uses Skia Paragraph/TextLayout; bundling contains a font. | Native font discovery, licensing, fallback and user font import are OS-specific. | **UNVERIFIED:** same font bytes/version and shaping behavior are not guaranteed across platforms. Geometry may differ. |
| Filesystem/assets | Browser IndexedDB and fetch/asset paths. | Native file and asset access requires injected storage interface. | **PARTIAL:** domain algorithms should accept bytes/handles, not browser File/IDB APIs. Asset identity and missing-resource behavior need contract. |
| Workers/threads | App uses web workers; WASM threading depends on browser COOP/COEP, shared memory and Emscripten flags. | Native thread runtime is available, but no cross-target scheduler abstraction is proven. | **UNVERIFIED:** current build success does not establish threaded parity. Keep worker orchestration outside deterministic core. |
| Serialization | Rust Serde JSON plus Go Transit-compatible legacy wire transformations. | serde available natively. | **PARTIAL:** JSON/Transit naming and unknown-field preservation diverge; version envelope absent in shared doc. |
| Memory / allocation | WASM linear memory allocated/freed through Emscripten exports and wasm-bindgen; renderer has pools/caches. | Rust allocator differs by target; native Skia/GPU resources have separate owners. | **UNVERIFIED:** no heap/linear memory/GPU lifetime benchmark establishes leak freedom or cross-target boundedness. |
| Errors | Rust Result and bridge-specific string/JSON errors; renderer recovery returns typed TS diagnostics on A8 route. | Native panic/error ABI behavior not specified. | **PARTIAL:** define stable error codes, panic boundary, cleanup on cancellation and poisoned renderer recovery. |

## Browser dependencies leaking into domain boundaries

- React `Shape` and DOM `File`, `crypto.randomUUID`, IndexedDB and Web Worker APIs belong in adapters, not canonical Rust algorithms.
- `render-wasm`'s Skia GL types and Emscripten pool/memory exports are render/backend concerns. Keep them behind renderer interface; do not make document schema depend on `atlasSlot`, GPU resource handles or pixels.
- Token resolution and pure geometry can stay deterministic Rust functions accepting typed values; browser calls should marshal versioned inputs and outputs.
- Stable IDs should be supplied/generated at a boundary using platform-independent UUID semantics; do not make document identity depend on `nanoid`/browser crypto without fallback/format contract.

## Native readiness gates

1. Run CI builds/tests for `x86_64-apple-darwin`, `aarch64-apple-darwin`, `x86_64-pc-windows-msvc`, and `x86_64-unknown-linux-gnu` for domain crates.
2. Define a renderer backend trait (surface, font provider, image provider, clock, resource cleanup), then prove at least software-render path on each platform before GPU backend selection.
3. Specify Rust ↔ host ABI: protocol version, pointer ownership, allocator pairing, buffer free, panic conversion, cancellation, WASM memory growth and resource disposal.
4. Freeze font fixtures (font bytes + shaping library + expected glyph/run geometry) and numerical tolerances for cross-platform differences.
5. Add native/WASM golden contract tests using the same JSON fixtures; compare deterministic domain/layout outputs exactly or by explicitly approved tolerance.

No Electron/Tauri shell or platform-specific production implementation is proposed in this audit.
