# A8 Hierarchical Scene Projection Foundation

**Status:** Draft foundation for renderer review. The adapter is not connected to the production Canvas.

**Baseline:** `main` at PR #86 merge `50df48b8608280907d9b014b32087690662dfa30`.

## Pure input and ownership

`logos-app/src/render-wasm/scene-projection.ts` accepts a page identity, the ordered `Page.rootShapeIds`, and a **page-scoped** shape-record set. It returns a typed diagnostic or a deterministic projection plan and does not mutate its inputs, Zustand, persisted document data, or WASM state. A renderer-only nil-UUID root exists only in this ephemeral plan; no second persisted graph is introduced.

The current `documentStore.shapes` map contains records for all pages, while `useCurrentPageShapes()` returns only roots. The builder therefore requires a page-scoped record set so it can identify unreachable/orphan nodes. The production store does not currently provide that complete scope (especially for disconnected nodes). The new builder/uploader is deliberately **not** wired into `Canvas`; this ownership/projection gap is explicit and remains an integration gate.

## Validation contract

The builder fails before upload for invalid UUIDs, key/ID mismatches, duplicate IDs/roots/child entries/membership (including UUID case-fold collisions), nil-root collisions, mismatched ordered roots, missing root/parent/child references, parent-child backlink disagreement, cycles, orphaned/unreachable page records, and unsupported Rust shape discriminants—including nested descendants in the supplied page set. Diagnostics use stable `LOGOS_SCENE_*` codes and carry the page and offending/reference IDs where relevant.

The input arrays are source order, top-first. The Rust renderer reverses stored child IDs and traverses a stack; the plan stores reversed root and child vectors so paint order is deterministically back-to-front. This policy is covered by exact plan assertions and overlapping-color Chromium pixels. Rust layout/z-index reordering and the full layout payload are not represented by the current base-props ABI; this order proof does not claim layout/z-index parity.

## Synthetic root and serialization

The renderer plan creates a nil-UUID `Frame` with zero bounds, no fills, no layout payload, and default non-clipping behavior. Its ordered children are the current page roots in renderer storage order. It is uploaded first, followed by reachable nodes in deterministic source-order preorder; all parent records exist before ordered child lists are installed. The plan is recreated on each full projection and never serialized to the document.

`projectAndSyncScene` is the integration boundary: it builds and validates first, then calls `begin_scene_upload`, uploads base properties, installs child vectors, and only then calls `_render_sync`. The direct upload routine is private so external callers cannot bypass projection validation. Invalid input returns without any WASM mutator. Production `Canvas` still uses the legacy root-only `syncScene`; the new API is exercised by unit and browser harnesses only.

## Interrupted-upload recovery

The existing renderer is a live singleton pool, not a staged transaction. `begin_scene_upload` resets the prior pool generation and renderer-derived tile/cache/touched state. If an upload or render call throws, the adapter starts another clean generation and restores the last committed plan before returning `LOGOS_SCENE_UPLOAD_INTERRUPTED` with `recovered=true`. If compensation fails, it returns `LOGOS_SCENE_RECOVERY_FAILED`, poisons the module, and blocks further adapter writes until the owner recreates it. It does not render the failed candidate.

This is compensating recovery, **not atomic commit/rollback**: transient partial WASM state can exist during the synchronous call, and a fatal runtime abort may prevent compensation. Fault-injection tests cover resize, pool reset, allocation, base props, fills, shape selection, child upload, and render boundaries; one-shot failures restore the last committed fake-module frame, and unrecoverable failures poison the module. The real-browser test injects a failure after a native base-props mutation and compares restored full-frame pixels. No claim is made for process loss or every possible WebAssembly abort.

## Actual renderer browser evidence

`test/fixtures/renderer-scene-projection/nested-overlap.json` defines overlapping page roots, ordered siblings, a nested frame, and a nested descendant. `logos-app/test-harness/a8-hierarchical-renderer-smoke.html` loads the real Rust/WASM renderer in headless Chromium and records expected/observed pixels for root/sibling stacking and nested rendering. It also verifies invalid-scene rejection without WASM mutation, interrupted-upload recovery, subtree removal clearing old pixels, a reparented child’s parent UUID and new position, and byte-identical framebuffer output on repeated sync. The Node fault-injection suite covers each upload mutation/render boundary and the poisoned-module path when compensation fails.

The browser test exercises `projectAndSyncScene` directly, not the production `Canvas` route. The payload remains limited to the fields currently encoded by the adapter (base properties and its existing first-solid-fill helper); full text, gradients, strokes, effects, layout, component/instance, and vector-network parity are not established.

## Separate product workstreams

This projection consumes concrete resolved appearance values. It does not implement or own design-token resolution, authoring, aliases, themes, or agent decisions. It changes no canonical schema, document persistence, Go/backend code, or production Canvas wiring.
