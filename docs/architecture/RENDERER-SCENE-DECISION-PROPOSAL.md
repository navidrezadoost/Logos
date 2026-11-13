# Renderer Scene-Graph and Compatibility Decision Proposal

**Status:** proposal for architecture review; not an approved schema, runtime, route, or production implementation.
**Evidence baseline:** `main` at PR #78 merge `558d1290f71ffc686436a7d92f1557874348df47`.

This proposal specifies a validation boundary for a future React-to-Rust projection. It does not wire a production scene builder. The existing test fixture at `test/fixtures/renderer-scene-contract/ordered-hierarchy.json` is an example of current React field names, not a second canonical model.

## 1. Root, parent, child, and order contract

### Required input invariants

For one page projection:

1. Every shape ID is a valid, unique UUID and equals the key under which the shape is stored.
2. The nil UUID is reserved for the renderer's synthetic root and cannot identify a document shape.
3. `Page.rootShapeIds` is an ordered list of existing nodes whose `parentId` is `null`.
4. Every non-root node appears exactly once in one parent's ordered `children`; its `parentId` names that parent.
5. Every listed child resolves to a node, and the child points back to that parent. No duplicate child IDs, self-links, cycles, or unreachable page nodes are accepted.
6. Validation covers the complete candidate page before any mutation of the live WASM pool. A violation returns a diagnostic identifying the source shape and missing/conflicting reference; it does not upload a partial graph.

### Concrete order fixture

The checked-in fixture declares page roots in this order:

```json
{
  "rootShapeIds": ["00000000-0000-4000-8000-000000000020", "00000000-0000-4000-8000-000000000030"],
  "shapes": {
    "00000000-0000-4000-8000-000000000020": {
      "parentId": null,
      "children": ["00000000-0000-4000-8000-000000000022", "00000000-0000-4000-8000-000000000021"]
    },
    "00000000-0000-4000-8000-000000000022": { "parentId": "00000000-0000-4000-8000-000000000020", "children": [] },
    "00000000-0000-4000-8000-000000000021": { "parentId": "00000000-0000-4000-8000-000000000020", "children": [] },
    "00000000-0000-4000-8000-000000000030": { "parentId": null, "children": [] }
  }
}
```

The React page and child arrays are the stored order; the React adapter describes roots as top-first. Rust's `children_ids_iter` reads stored child lists in reverse, and its render traversal uses a stack. The proposal is therefore to serialize the source order unchanged and define visible stacking with an overlapping-color browser fixture. Do **not** reverse arrays ad hoc in TypeScript; verify the full traversal with the fixture before choosing a conversion rule.

### Candidate projection pseudocode (not production code)

```text
plan = validate_page(page, shape_map)       // pure; no live WASM writes
pool_init(plan.nodes.length + 1)
upload_renderer_root(NIL_UUID, approved_root_container_kind)
for node in plan.preorder:
    upload_base_props(node.id, node.parentId ?? NIL_UUID, node.geometry, ...)
for (parent_id, ordered_child_ids) in plan.ordered_child_lists:
    upload_children(parent_id, ordered_child_ids) // preserve stored sequence
render_sync()
```

The Rust full-scene entry point requires a node with `Uuid::nil()` and reads that node's children. The PR #64 test adapter used a `Frame` as this synthetic root, but that test-only choice is not an approved production contract. Decide the root kind and nil-ID reservation explicitly. Persisted React page roots remain `Page.rootShapeIds`; do not add a serialized nil-root object or a duplicate durable graph without a reviewed contract.

### Missing references and deletion

- Missing root, parent, or child references; duplicate membership; cycles; and parent/child disagreement reject the entire plan before pool initialization. No reference is silently omitted.
- Deleting a container removes the subtree from the page projection and removes its root/sibling link. Reparenting first removes the node from the old parent's children and attaches it exactly once to the new parent. The ownership layer must decide whether undo keeps a tombstone; this proposal does not create one.
- Existing `documentStore.removeShape` removes the direct record and root/parent link but explicitly does not recurse. That is insufficient for nested deletion. Rust `set_children_set` can observe removed child IDs and mark them touched, but the React adapter currently does not call it.

## 2. Unsupported shape types and Canvas 2D fallback

Rust `RawShapeType` currently encodes Frame, Group, Bool, Rect, Path, Text, Circle, and SVGRaw. The React type union additionally contains `vector-network`, `component`, and `instance`.

- **`vector-network`:** `Canvas` creates these via pen and boolean flows. Rust has a `Path` type, but the adapter does not upload the anchors, segments, and regions needed to establish equivalent semantics. Do not guess that `vector-network` means Rust `Path`.
- **`component` / `instance`:** these are product-level master/instance records with metadata/overrides, not plain rectangles. No Rust raw discriminants or complete renderer payload mapping are present.
- **Canvas 2D is a separate compatibility issue:** `syncScene2D` has an explicit vector-network drawing branch, but its `default` case calls `fillRect` for unsupported cases including `component` and `instance` (as well as other unimplemented types). Therefore fail-closed validation in the Rust adapter alone does not close silent rectangle substitution in fallback mode.

**Behavior implemented in the A6 follow-up diff:** both adapters preflight the complete root-shape list before mutating their live target. `syncScene` rejects unsupported Rust discriminants before resizing the viewbox or initializing/uploading the pool. `syncScene2D` rejects types outside its explicit rect/frame/circle/ellipse/vector-network set before `clearRect` or drawing. The typed errors carry shape ID and discriminator; the Canvas caller restores its saved context in `finally`. The document/store is not changed. No rectangle substitute or new unsupported-type mapping is introduced. A product-visible compatibility diagnostic remains unresolved; the exception propagates from the Canvas effect, but its browser/UI presentation is not tested here.

**Documented fallback behavior change:** before this change, `path`, `text`, `group`, `bool`, `svg-raw`, `component`, `instance`, and future types fell through to `fillRect(bounds)`. They now raise `UnsupportedCanvas2DShapeError` before `clearRect` or any draw call; shape records are untouched and prior canvas pixels remain. The Canvas effect does not translate this exception into a dedicated in-product compatibility banner, and actual browser/UI presentation remains unverified. `ellipse` changes from its accidental rectangle approximation to an explicit ellipse draw. Existing documents containing the listed types may therefore fail to render through Canvas 2D fallback rather than display misleading rectangles; WASM fallback/scene data remains intact. This impact is documented, not hidden. `vector-network` retains its existing path branch and is regression-tested for non-rectangle calls, without claiming complete fidelity.

**Fallback regressions:** deterministic context-recorder tests assert `component`, `instance`, `path`, `text`, `group`, `bool`, `svg-raw`, and a future type produce a diagnostic without clearing or drawing; paired Canvas/Rust cases cover `component`, `instance`, and a future type after a valid shape to prove all-or-nothing rejection before either target mutates. Ellipse and vector-network use their explicit drawing paths rather than shape-bounds rectangles. A browser-level test should still verify the actual Canvas component/error presentation before enabling this behavior broadly. A persistent partial-render warning would require product approval.

## 3. Renderer cache and touched-ID reconciliation

### Validated in source, not end-to-end validated

- Rust `set_children_set` stores the new ordered child vector, computes removed child IDs, and calls `delete_shape_children`/`touch_shape` for removals.
- `delete_shape_children` finds a removed shape's old tile bounds, calls `remove_cached_tile`, and removes the shape from the tile index.
- `remove_cached_tile_surface` marks a tile invalid while keeping old pixels until the next render replaces them. `syncScene` still does not upload parent/children lists or invoke these operations, so this behavior is not established through production React scene sync.

### Proposed reconciliation transaction

For an update, deletion, or reparent:

1. Validate the proposed graph and retain the previous graph until validation succeeds.
2. Compute `affected = changed nodes ∪ old/new parents ∪ old/new ancestor chains ∪ deleted subtree`.
3. Capture old and new tile coverage for every affected node/ancestor. Remove old parent/root links and stale IDs; apply new links in source order.
4. Mark every affected ID touched; invalidate the union of old/new tile coverage and remove stale shape membership from each tile index. Rebuild ancestor/extents and the shallow tile index for structural changes.
5. Publish/render the new graph only after the cache/index plan is complete. Untouched tiles remain reusable; every affected tile is replaced once by the normal render path. No repeated redraw loop is permitted as a recovery mechanism.
6. Reapplying an identical validated snapshot is idempotent: identical ordered IDs/records/child lists and equivalent output pixels, with no duplicate nodes/children or prior-generation attributes.

The current Rust ABI writes directly into the live singleton pool; it has no staging/commit/rollback API. A TypeScript preflight prevents known invalid graphs from starting, but it cannot make a later WASM allocation/trap atomic. The proposed contract is:

1. Encode and validate the complete plan and all ABI payloads before `_init_shapes_pool` or any other live mutator.
2. If an allocation/setter fails after the first mutation, do not call `_render_sync`; mark the renderer instance unusable and discard/recreate it from the last committed snapshot before rendering again. Do not expose a partially uploaded scene.
3. A stronger future option is an inactive scene pool plus atomic swap after successful upload. That requires an explicit ownership/cost decision; it is not implemented here and does not justify a new record format by itself.

Required tests are old/new-parent reparenting, nested deletion, moved geometry crossing tile boundaries, adjacent unaffected cached tiles, invalid references with no live mutation, injected failures at each upload stage, recovery from the last committed snapshot, and repeated identical sync. These tests must use full framebuffer comparisons and confirm that the correct old pixels clear while unrelated tiles remain byte-identical.

## A6.2 implementation and validation record

The current follow-up adds Canvas 2D preflight before `clearRect`, an explicit ellipse path, a typed unsupported-shape error, and `try/finally` restoration of the caller's transformed Canvas context. Context-recorder tests cover eight unsupported known/future types, no clear/draw calls on rejection, and explicit ellipse/vector-network paths. This is not a browser pixel/e2e test of the actual Canvas component or an in-product compatibility banner.

Local follow-up checks passed: the Node fixture suite (8 tests), TypeScript project check, isolated Vite adapter bundle, full Vite app bundle, `cargo fmt`, and `git diff --check`. Hosted checks passed on PR81's previous head `2ccbf118` (Rust workspace and React); this follow-up requires fresh checks on its own head before review.

## Architecture decision alternatives (proposed)

| Decision | Alternatives and compatibility/risk | Required evidence | Recommendation for review (not approval) |
|---|---|---|---|
| Synthetic nil root | **A.** Renderer-only virtual root using existing Frame ABI: least ABI churn, but risks inheriting frame clip/layout semantics. **B.** Add a dedicated Rust root sentinel: clear semantics, but changes ABI and all traversal entry points. **C.** Pass ordered roots out-of-band to render entry points: avoids fake Shape, but broadens render APIs and cache ownership. | Nil UUID collision rejection; empty and multiple roots; nested frame; clip/layout neutrality; overlapping-color pixels proving painter order. | Prefer a renderer-owned virtual root sentinel with no persisted/document identity; choose whether it is a dedicated kind or an existing neutral container only after the above tests and an ADR. Do not persist the synthetic node. |
| Parent/child validation owner | **A.** React adapter validates before upload: protects current WASM from partial invalid graphs, but only validates its projection. **B.** Rust validates as well: strongest defense at traversal boundary, but needs error-returning ABI instead of silent/missing-node traversal. **C.** Durable document layer owns all validation: desirable once a canonical contract exists, but that contract is currently blocked and cannot be presumed. | Duplicate IDs, missing parent/child, wrong parent backlink, duplicate child, cycle, orphan, nil-ID misuse; assert no live WASM mutator is called on rejection. | Validate in the renderer adapter before mutation and add Rust defensive checks at the ABI boundary. This does not select the durable document/schema owner. |
| Unsupported type behavior | **A.** Current generic rectangle approximation: keeps something visible but misrepresents geometry and semantics. **B.** Typed fail-closed diagnostic: preserves document and avoids false geometry, but some existing fallback users will see an error. **C.** Labeled placeholder: can preserve layout affordance, but risks being mistaken for actual content unless clearly non-exportable/non-editable. | Backend capability matrix; unsupported IDs/types; no canvas clear/draw; data remains unchanged; 2D vector-network geometry tests; browser-visible diagnostics. | Prefer typed fail-closed behavior now. Consider a labeled placeholder only after product design defines accessibility, export, and editing semantics. No Path↔vector-network guess. |
| Cache/deletion ownership | **A.** Renderer owns cache/tile index and consumes a validated old/new graph diff: coherent with tile implementation, requires diff/invalidation contract. **B.** React computes tile invalidations: duplicates renderer geometry and can miss transformed bounds. **C.** Clear/rebuild all cache on every update: simplest correctness baseline but costs performance and is not a substitute for measured behavior. | Delete/reparent/subtree tests; stale edge clearing; untouched-neighbor byte equality; old/new tile coverage; repeated sync; context-loss/recovery. | Prefer renderer-owned invalidation from a validated transaction/diff. Until that exists, use one bounded full invalidation for structural mutations, not redraw loops; benchmark only after correctness. |

These recommendations identify decision owners and evidence; they do not authorize implementation or establish a second graph model.

## A7 renderer compatibility UX and validation boundary

### Typed renderer status

`logos-app/src/render-wasm/renderer-status.ts` defines the editor-facing `RendererStatus` union: `READY`, `RENDERING`, `UNSUPPORTED_SHAPE`, `RENDER_FAILED`, and `RECOVERING`. Unsupported-shape failures retain `shapeId`, `shapeType`, backend (`canvas2d` or `wasm`), and stable diagnostic code `LOGOS_RENDER_UNSUPPORTED_SHAPE`. Other renderer exceptions map to `LOGOS_RENDER_FAILED`; raw exception messages/stack traces are not included in the UI status.

The frame disposition distinguishes `none`, `previous`, and `unverified`. Typed unsupported errors are raised during adapter preflight and leave the target unchanged; when dimensions/backend match the last successful frame, the editor identifies that frame as previous. Generic errors may happen after writes, so any retained output is labelled unverified rather than claimed to be an intact prior frame.

### Compatibility presentation and recovery

`Canvas` contains the diagnostic overlay within the canvas region so its scrim blocks interaction with stale pixels but does not cover neighboring Layers, Inspector, or document-navigation UI. It names the unsupported type, shape ID, backend, and diagnostic code; explains that renderer failure does not delete document data; and labels previous/unverified canvas output as not current. Unsupported content offers **Select in Layers**; a generic renderer failure offers an explicit user-triggered retry, never an automatic retry loop. The persistent inline alert is not a toast and remains a single notice across repeated failed render attempts. A successful compatible render returns status to `READY` and removes the notice automatically. No alternative renderer is invoked on unsupported content because capability for that content has not been proven.

### Nested-shape limitation (explicit)

This change validates only the shapes supplied to the adapters. `Canvas` obtains that array from `useCurrentPageShapes()`, which currently selects `Page.rootShapeIds`; `syncScene2D` likewise preflights only its `shapes` argument. The production path does **not** project or recursively validate nested descendants here. An unsupported child beneath a supported root can therefore bypass this diagnostic boundary, and nested visual parity is not established. This task does not add recursive scene projection, a production hierarchy bridge, or canonical graph ownership.

### Browser evidence

`logos-app/test-harness/a7-renderer-compatibility-smoke.html` mounts the real Canvas component in Chromium with the normal WASM loader directed to a deliberately absent artifact, exercising the actual Canvas 2D fallback. The browser proof checks a supported pixel, an accessible unsupported-shape diagnostic, shape ID/type/backend/code, dimmed previous pixels, unchanged document state after failure, the real LayersPanel and Inspector alongside document navigation outside the Canvas overlay, context restoration, non-duplicating repeated failures, and automatic recovery after a compatible shape update. It also confirms a Canvas resize no longer labels a cleared backing store as a visible previous frame. It is separate from browser-persistence evidence and makes no persistence claim.

## Decision gates

Before production integration, architecture review must approve: the synthetic nil-root contract; source-versus-renderer order and stacking; behavior/capability of vector-network, component, and instance across both renderers; subtree delete/undo semantics; and cache invalidation ownership. No runtime, route, schema, canonical document owner, or migration contract is selected here.
