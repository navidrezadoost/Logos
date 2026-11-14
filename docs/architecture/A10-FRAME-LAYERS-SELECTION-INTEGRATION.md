# A10 — Frame, Layers and Selection Integration

**Status:** Candidate integration slice; Draft PR only
**Baseline:** `81201fac76be85fecee9223233e946c3b94ff4b2`
**Scope:** React editor interactions and the existing Rust/WASM renderer boundary

## Architecture boundary

The React document store remains the editor shell's current in-memory document projection. `Page.rootShapeIds` preserves root order; each shape's `parentId` and ordered `children` preserve hierarchy membership. The Canvas and Layers panel derive their scene from that store. The Rust/WASM scene graph remains the renderer's sole rendering engine. A10 adds no persistence format, durable Go model, geometry engine, or second document store.

`applyHierarchyOperation` is the one React compatibility boundary for supported hierarchy changes. It plans against an immutable snapshot, validates the existing page graph with the A8 scene projection validator, validates renderable capabilities with the A9 capability gate, then makes one Zustand `set({ pages, shapes })` call. Failure returns a stable diagnostic and does not set either document field. `updateShape` and `batchUpdate` reject `parentId` or `children` patches so callers cannot bypass this boundary. Non-hierarchy visual property edits continue through those existing APIs.

The React checks are transitional boundary validation, not a competing canonical engine. Once the approved Rust document contract and canonical ownership are adopted by the editor runtime, the store action should submit typed operations to Rust, consume its validated immutable candidate and diagnostics, and only then publish the React projection. Until then, operations outside this supported A10 subset fail closed.

## Supported operations

| Operation | Accepted behavior | Rejection conditions |
| --- | --- | --- |
| Create root | Frame or Rectangle; stable supplied ID; inserted at front of ordered page roots | ID collision, non-empty child list, parent assignment, unsupported type, invalid graph or A9 capability |
| Insert child | Rectangle into Frame or Group at a specified sibling index | Unknown/other-page parent, unsupported parent/type, locked owner, ID conflict, invalid index, candidate graph/capability failure |
| Rename | Trimmed non-empty name up to 128 characters; preserves ID and order | Missing shape, locked shape, empty/overlong/control-character name |
| Reorder sibling | Move an existing shape to a valid index within its current parent/root list | Missing ownership, locked shape or parent, invalid index, malformed current graph |
| Reorder roots | Supply a permutation with exactly the existing root membership | Membership/duplicate mismatch, locked root, malformed candidate graph |
| Delete subtree | Remove root or child and all descendants; unlink from parent/page | Unknown or ambiguous page ownership, malformed graph, any locked shape in subtree or locked owner |
| Reparent | Change root/parent membership while preserving the shape's bounds (world-position representation) | Unsupported/non-container target, locked shape/source/target parent, cycle, cross-page/unknown owner, invalid index, non-identity transform, clipping ambiguity, invalid candidate graph/capability |

Only axis-aligned identity transforms are accepted. Bounds are retained unchanged during reparent; the target graph must pass the A8 clipping/containment contract. The current contract rejects reparenting when containment or clipping semantics cannot be established. It does not synthesize transforms or silently normalize coordinates.

The production Layers panel exposes child insertion, rename, sibling/root ordering, reparent-to-root/container, and subtree deletion from the layer action menu. Rectangles and Frames are created by the existing Toolbar tools and Canvas drag. There is no production Group creation action in A10; Groups can be addressed only when already present in a valid document. Layer actions are disabled for locked objects. Parent/child mutations also reject when a locked parent record would change.

## Selection contract

`selectionStore` is the shared selection authority consumed by Canvas, Layers, and Inspector.

- Canvas selection follows the stored top-first root/sibling order, tests descendants before their containing frame, and selects the topmost visible, unlocked shape with axis-aligned bounds. Empty Canvas clicks clear selection. Ctrl/Cmd toggles; Shift selects the interval in that hit order.
- Hidden and locked objects are not Canvas hit targets for point, marquee, scale, or prototype-source selection. Layers can select either one for inspection. Locked objects cannot use the A10 hierarchy actions.
- Switching to a known page clears selection. A document subscription removes selections whose IDs are no longer in the active page scene after deletion or page membership changes.
- Selection IDs remain stable across supported mutations; a rejected operation leaves both document references and selection unchanged.

The hit test is intentionally limited to axis-aligned bounding boxes. It is not pixel-perfect geometry selection and does not qualify rotated/transformed shapes.

## Renderer lifecycle

The A8 scene upload boundary already records a poisoned module when upload and compensation both fail. `projectAndSyncScene` rejects all subsequent writes to that module. A10 adds an explicit `Recreate renderer` action only for poison/recovery-failed diagnostics:

1. Mark the output `RECOVERING` and unverified.
2. Abandon the poisoned instance without calling its WASM cleanup or GL table methods.
3. Lose the old canvas context, discard the loader singleton, and replace the canvas element.
4. Load and initialize a fresh WASM module; the editor returns to `READY` only after a new projected scene frame succeeds.

Ordinary render failures retain the existing retry behavior. The automated browser harness injects a deterministic upload failure after mutation, confirms compensation failure and the poisoned diagnostic, clicks the production recovery action, then verifies a new module instance, a ready frame, and retained-document pixels. The fault injection is test-only.

## Verification and evidence

`test/a10/hierarchy-operations.test.mjs` exercises atomic acceptance/rejection, IDs and ordered membership, reparent bounds, cycle/index/name errors, bypass guards, subtree selection reconciliation, page switching, and locked shapes.

`test/benchmarks/a10-editor-interactions-smoke.mjs` drives the production Toolbar, Canvas, Layers and Inspector in Chromium with actual keyboard/pointer and layer-menu input. It verifies a rendered child pixel and stale-pixel clearing, invalid mutation immutability, hidden/locked selection behavior, rename, root ordering, reparenting, deletion, page switching, and renderer recreation. The evidence records Chromium version/user agent and platform, logical processors, viewport/DPR, WASM heap reservation, renderer instance count, pixel samples, browser errors, and action-to-READY-frame timings. These timings include editor scheduling and renderer work; they are interaction observations, not isolated renderer benchmarks or performance claims.

Run the operation and existing scene contract tests:

```sh
node --test test/a10/hierarchy-operations.test.mjs
node --experimental-strip-types --test test/p0.5/renderer-scene-contract/scene-boundary.test.mjs
```

The hosted `Rust workspaces` workflow builds the actual Emscripten renderer and runs the A10 Chromium harness in a disposable local preview. CI artifact output is the operational browser evidence. No production service, account, or credentials are used.

## Known limits

- No durable document persistence, Go change, legacy ID migration, or canonical document adoption is included.
- The operation planner supports current React shape/page structures and renderer-qualified identity transforms; it does not cover arbitrary transforms, unsupported shape types, disconnected records, cross-page moves, or legacy repair.
- Frame selection uses bounds, not exact painted geometry. Layers remains the selection path for hidden, locked, or overlapping objects.
- Canvas 2D fallback does not support this hierarchical mutation subset; A10 browser acceptance requires the Rust/WASM renderer.
- Local Chromium is not installed in the current development environment. The hosted Chromium workflow is required for end-to-end pixels and renderer-recreation qualification.
