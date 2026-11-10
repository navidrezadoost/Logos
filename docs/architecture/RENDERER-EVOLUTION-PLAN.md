# Renderer and Layout Evolution Plan — P0 Baseline

**Baseline:** `964676b90398f4458a72e3d41fd87e0dc91f51f9`
**Policy:** preserve the verified Skia/WASM path; do not replace or optimize a backend without integration and benchmark evidence.

## Current implementations

| Path | Source and current responsibility | Evidence / status |
|---|---|---|
| Skia/WebGL renderer | `render-wasm/` Rust crate; Emscripten JS/WASM served under `/js`; React adapter in `logos-app/src/render-wasm/{module,scene}.ts`. | Main workflow builds Emscripten artifacts, checks JS/WASM/worker/shared assets and WASM magic, then runs a browser scene smoke. Hosted main Run [#236](https://github.com/navidrezadoost/Logos/actions/runs/42846256424) passed with blue interior and transparent outside pixel assertions. **Verified for the isolated renderer test only.** |
| Canvas 2D fallback | `scene.ts::syncScene2D` and `Canvas.tsx`. | Source covers a limited shape fallback; no parity for text/effects/layout is shown. **Implemented, not parity-verified.** |
| WebGPU tile path | TypeScript `render-webgpu/` with tile, composite, text, hit-test, snap, and layout pipelines; `TileRenderer` requests a `webgpu` canvas context. | Substantial source exists, but the React `Canvas` passes its exact canvas to both `useWebGPURenderer` and the Skia loader. The hook requests `getContext("webgpu")`; `initCanvasContext` requests `getContext("webgl2")`. A canvas cannot provide both context types. `Canvas.tsx` still uses the Skia/2D `mode` to draw and does not use the returned `gpuRenderer` handle for backend selection. **Integration ownership conflict; not browser-verified.** |
| Rust layout + WASM ABI | `rust/logos-layout/`, `rust/logos-layout-wasm/`; separate layout code in the WebGPU tree. | Local locked domain tests passed (including flex/grid and 12 layout-WASM tests). `logos-app/src/worker/layout.worker.ts` states its WASM bootstrap is not wired and currently uses a pure-TS fallback for flex row/column; grid returns no layout. **Domain verified, React-to-Rust integration not verified.** |
| Workers | `logos-app/src/worker/{index,layout.worker,snap.worker,serialize.worker,vector-network.worker}.ts`. | Request/reply pool and termination rejection exist in source; no app test suite exercises worker failure, queue limits, timeout, cancellation, or context loss. |

## Ownership decision

For the P0 baseline, **Skia/WASM is the only renderer path with hosted browser pixel evidence**. This does not yet establish it as a complete editor renderer. WebGPU remains an experimental/parallel source path until an explicit selector gives one backend sole ownership of each canvas, defines initialization/cleanup/fallback, and removes the competing context initialization. Do not infer production support from shader files or WebGPU API stubs.

## ABI and scene boundary

- React `scene.ts` emits 104-byte shape base records and separate fill buffers. `module.ts` documents the 160-byte Rust `RawFillData` enum record. The fill ABI and a deterministic blue-rectangle pixel are covered by PR #51’s Rust/browser regression checks.
- The hosted renderer smoke directly initializes Emscripten and creates the nil-UUID root/child scene. It is not the same startup route as React `Canvas`, nor does it establish root/page synchronization for actual Go documents.
- Current `syncScene()` reuploads the full shape list. Dirty-region updates and incremental GPU-resource ownership have no before/after benchmark in this repository baseline.

## Required benchmark plan (proposed, no performance claim)

First freeze a deterministic fixture corpus and hardware/browser matrix. Separate measurements for: time-to-first-pixel; scene upload; CPU layout; GPU frame render; input-to-visible latency for select/move/resize; text shaping; memory and GPU allocations; context-loss recovery; and reload/reinitialize cost. Include 100/1,000/10,000 shapes only as test sizes, not targets or claims. Record p50/p95/p99, frame-time distribution, memory, artifact hashes, browser/GPU/driver, and cold/warm caches. Compare Skia to WebGPU on identical documents and interactions.

## Dependency-ordered work

1. **G1/G3 ownership:** decide one canvas backend per editor instance; test missing WebGPU → Skia, WebGL unavailable, resize, repeated mount/unmount, context loss, and document preservation.
2. **G2/G3 contract:** reconcile React shapes, Rust layout types, stored document model, and renderer ABI; retain the tested 160-byte fill contract.
3. **G3 integration:** wire layout worker to the selected Rust/WASM implementation or explicitly document a tested TS implementation; compare flex/grid outputs with Rust goldens.
4. **G3 correctness:** browser golden pixels for rectangles, ellipses, vectors, text, gradients, nested ordering, clipping, and selection; execute via the actual React adapter.
5. **G6 performance:** only after correctness, collect benchmark baselines and then propose incremental updates, tile culling/cache reuse, or other optimizations.

**P0 status:** Skia build and isolated pixels **Verified**; React renderer ownership and app parity **Blocked**; performance targets **Proposed**.
