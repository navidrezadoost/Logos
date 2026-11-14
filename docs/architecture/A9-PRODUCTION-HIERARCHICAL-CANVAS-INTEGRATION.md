# A9 — Production Hierarchical Canvas Integration

**Status:** Draft implementation; production rollout is not authorized.
**Baseline:** `ec60301e506e9bf5a224f3c326d7440bde622f37`
**Ownership:** React `Canvas` owns one upload call to the existing A8 Rust/WASM scene projection.

## Baseline audit

At the approved baseline, `Canvas.tsx` called `syncScene(mod, shapes, width, height)`. `useCurrentPageShapes()` returned only the ordered top-level IDs in `Page.rootShapeIds`; the production component did not send nested child membership to the Rust renderer. A8's `projectAndSyncScene()` was exercised by a separate renderer smoke harness, not production Canvas.

The document store keeps all page shapes in one `shapes` dictionary. A `Page` identifies ordered roots; a `Shape` identifies its `parentId` and ordered `children`, but does not carry a `pageId`. The production selector now follows child links from roots in either direction to include malformed one-sided memberships for validation. Disconnected parentless records cannot be assigned to a page from this model, so they cannot be diagnosed as page orphans without adding ownership information to the document contract. A9 does not change that contract.

Existing live page IDs may be `nanoid()` values while shape IDs are UUIDs. A8 requires UUIDs for renderer shape identities. A9 passes existing values through and does not regenerate IDs or migrate files; a page with a non-UUID root shape is rejected by A8 diagnostics.

Draft PR #90 was inspected. It is still a separate, unmerged token-preview branch based on an older Canvas revision; its scene preview and resize-status changes are not part of the approved `main` baseline. A9 does not cherry-pick or overwrite that work. Any future PR #90 reconciliation must compare its Canvas changes against this production hierarchy path.

## Production flow and ownership

1. Zustand `Page.rootShapeIds` supplies the document's ordered top-level roots.
2. `selectCurrentPageScene` collects the reachable records while preserving root and child membership. The immutable `Shape` records remain the only React document state.
3. `Canvas.tsx` calls `projectAndSyncScene()` once for the WASM backend. A8 validates IDs, parent/child symmetry, duplicates, cycles, root membership, reachability within the selected graph, and deterministic order before native mutation.
4. A9 capability validation rejects unsupported geometry before upload. The A8 adapter supplies a renderer-owned synthetic root and sends every child list through the established Rust ABI.
5. The renderer's committed projection is cached by WASM module. If upload is interrupted, A8 re-uploads the last committed page, dimensions, and view. A failed recovery poisons that module and is reported as unverified output.
6. The React view (`zoom`, `panX`, `panY`) is sent to WASM on every successful candidate upload. Page changes, immutable shape changes, fill/visibility changes, and root order changes rerun the same path.
7. Deleting a row in Layers removes its whole subtree and unlinks the removed membership. Layers displays nested descendants and can select their stable shape IDs.

The Canvas2D fallback remains a flat compatibility path. It accepts only flat, untransformed shapes with at most one solid fill and rejects nested hierarchy, group, transforms, and unsupported fill forms. It is not represented as equivalent to Rust rendering.

## Capability matrix

| Behavior | A9 status | Contract |
|---|---|---|
| Rectangle, Frame, Group | Supported | Existing WASM type mapping and A8 upload ABI |
| Nested Frame/Group/Rectangle | Supported | Child bounds are stored in page coordinates; hierarchy determines order and ownership, not implicit ancestor translation |
| Root and sibling order | Supported | Page and child arrays are retained; projection serializes the renderer's established painter order |
| Zero or one solid fill | Supported | Any gradients or multiple fills fail closed |
| Visibility and opacity | Supported | Existing `hidden` flag and opacity fields flow through base properties |
| Frame clipping | Qualified | Frame clipping is enabled in the Rust upload. Since React has no clipping switch and transformed-parent parity is unresolved, every descendant must be fully contained by each ancestor Frame; out-of-bounds content is rejected |
| Identity transforms and zero rotation | Supported | Geometry is page/canvas positioned and axis aligned |
| Non-identity affine matrices or rotation | Unsupported | Rejected before WASM mutation; no coordinate reinterpretation |
| Other shape kinds, gradients, multiple fills | Unsupported | Rejected with a scene diagnostic; data remains in Zustand |
| Canvas2D fallback hierarchy/transforms/clipping | Unsupported | No independent transform engine is added |
| Disconnected parentless shape/page ownership | Unresolved | Existing store has no shape-to-page ownership field; no migration is authorized |

## Browser and performance evidence

`test-harness/a9-production-canvas-smoke.html` mounts the production `Canvas` and `LayersPanel`, loads the actual Emscripten renderer, and reads the Rust renderer framebuffer. Playwright drives the runner-installed Chromium on its real-time event loop (without a virtual-time budget), saves the browser DOM report, and fails if the frame clock never advances. It distinguishes fixture/store-injected mutations from implemented pointer gestures. The report includes the browser user agent, WebGL renderer/vendor, logical CPU count, viewport, device pixel ratio, WASM buffer size, observed Chromium heap when available, and per-render timing samples.

The timing fields are:

- `projectionValidationMs`: A8 graph projection plus A9 capability validation.
- `rendererWidth` / `rendererHeight`: backing dimensions used by that successful upload.
- `wasmUploadMs`: synchronous view setup, shape-property upload, and child-list upload.
- `nativeRendererMs`: wall time inside `_render_sync()` (the renderer entry is synchronous).
- `synchronousMainThreadMs`: the full projection/validation/upload/render call duration.
- `browserFrameLatencyMs`: time from that call returning until the next animation frame.
- `wasmReservedBytes`: current WebAssembly memory buffer size.
- `observedJsHeapBytes`: Chromium's point-in-time `performance.memory.usedJSHeapSize`, when exposed.

These observations are evidence for this CI browser/runner only. They are not a workload benchmark, retained-growth study, memory-leak proof, production latency claim, or user-device estimate. Hosted CI uploads the raw DOM report and browser logs under `a9-production-canvas-browser-evidence`.

## Required regression coverage

The A9 Chromium harness checks initial nested rendering, root overlap and paint order, nested Layers selection, viewport pan/zoom, fill and visibility invalidation, fixture-driven reparenting, child insertion/deletion, subtree deletion and stale-pixel clearing, page switching, malformed graph rejection before upload, transformed-parent and unqualified clipping rejection, injected native upload interruption and successful compensation, failed compensation reporting, and repeated Canvas mount/unmount. Pixel checks sample actual Rust renderer output. Existing A8 browser coverage remains in place.

No durable document schema, saved file format, Rust canonical document model, Go persistence, production configuration, or renderer engine is changed by A9.

## Remaining blockers and limits

- Parent-local coordinate conversion and transformed ancestry remain unverified; A9 rejects these cases.
- Imported XD/Sketch matrices and rotation/pivot semantics remain unresolved.
- The flat Canvas2D renderer cannot safely show nested/clip-sensitive content and displays an explicit compatibility error instead.
- Current React state has no page ownership for disconnected records; malformed reachable membership is detected, while disconnected parentless ownership is not inferable.
- A9 produces no authorization to deploy, persist hierarchy changes, migrate historical files, or merge this PR.
