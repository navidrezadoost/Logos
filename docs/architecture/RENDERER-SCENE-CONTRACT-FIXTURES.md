# Renderer Scene Boundary and Contract Fixtures

**Baseline:** `fb0ea41d73a553b31d6dea0c7b1a341d68fdba7c` (`main`)
**Status:** compatibility evidence and a minimal fail-closed adapter check. This is not a production scene builder, a canonical document schema, or approval of a renderer/runtime route.

The fixture under `test/fixtures/renderer-scene-contract/ordered-hierarchy.json` mirrors the current React `Page`/`Shape` field names solely to make the required order and references executable. The test-only graph validator checks fixture invariants; production `scene.ts` still receives only `useCurrentPageShapes()` roots and does not upload nested children or build the nil-root structure expected by Rust.

## Field-level boundary matrix

| Contract | Required invariant | Current representation and code evidence | Verification / status |
|---|---|---|---|
| Shape identity | A UUID is preserved unchanged through projection and the base-props record. | React `Shape.id`; `scene.ts::writeUUID` writes four little-endian `u32`s at `[0..16)`. Rust `base_props.rs` reads those four words as `Shape.id`. | Fixture-driven adapter test compares the raw UUID words for two repeated uploads. **Verified for the present flat adapter path.** |
| Page roots | Page root membership is explicit and ordered; no root is inferred from hash-map order. | `Page.rootShapeIds` is authoritative for `useCurrentPageShapes`; React documents the list as top-first. Rust full-scene traversal starts from the nil UUID object's ordered `children`; Rust `children_ids_iter` reverses stored child order for traversal. Production `scene.ts` currently uploads the selected React root list but never creates the Rust nil root or calls a `set_children_*` export. | Fixture asserts stored root order; the fake-module test sees that same root upload order. The visual ordering conversion is not proven. Production root bridge remains **blocked** and is not wired here. |
| Parent/child links | Every child ID resolves; parent and child references agree; stored list order is stable and its mapping to visual stacking is explicit. | React `parentId` and ordered `children`; Rust `Shape.parent_id` and `children`. Rust `set_parent` assigns the parent but does not create the parent's child list. Rust `set_children_*` preserves the supplied sequence but does not validate references; Rust traversal reverses the stored list. | Test-only fixture validator covers reciprocal references, missing IDs, and stored order. Production upload has no hierarchy validation and currently does not upload nested nodes: **blocked**. |
| Shape type | A known discriminant maps to its documented Rust raw type. An unsupported discriminant is rejected before any live WASM mutation and is never converted to a rectangle. | Rust `RawShapeType` maps Frame/Group/Bool/Rect/Path/Text/Circle/SVGRaw to `0..7`. Previous TypeScript fallback `SHAPE_TYPE_MAP[shape.type] ?? 3` silently converted any missing type to Rect. | Rect is asserted as type `3`. The adapter now preflights the complete root upload list and throws `UnsupportedRendererShapeError(shapeId, discriminator)` before resize, pool init, or allocation. Tests cover `vector-network`, `component`, `instance`, and an unknown future discriminator. **Fail-closed for unsupported root shapes.** |
| Frontend-only discriminants | Existing content without a Rust discriminant must not be guessed into another type. | React `ShapeType` includes `vector-network`, `component`, and `instance`. `Canvas` creates vector networks through pen/boolean flows; Rust `RawShapeType` has no vector-network/component/instance variants. | Test verifies `vector-network` rejects before module calls. This exposes an existing product compatibility break rather than choosing an undocumented mapping; see ADR blockers. |
| Geometry | Bounds, transform, and rotation keep the declared coordinate convention. | React `bounds {x,y,w,h}`, six-number transform, and degree rotation map to Rust selrect `[x,y,x+w,y+h]`, 6 `f32`s, and rotation at the documented 104-byte offsets. | Fake-module test checks a deterministic rectangle's UUID, bounds, and transform record. **ABI projection verified; not a visual geometry parity claim.** |
| Fill state | A shape with no new fill cannot inherit a previous logical shape's fill. | `scene.ts` uploads only the first solid fill; it calls `applySolidFill` only when `fills.length > 0`. Rust `Shape::set_fills` replaces fills, but omission makes no call. `ShapesPoolImpl::initialize` previously reused `Shape` values and `add_shape` replaced only `id`. | Pool initialization now clears index-keyed derived state and each reused slot is replaced with `Shape::new(id)`. Rust regression models two pool/upload generations with a filled first shape and an unfilled second shape, then asserts no fill/children/flags/parent leakage and no inactive slot iteration. |
| Other omitted attributes | Optional text, stroke, blur, shadow, layout, SVG, and child state must not survive a new slot generation. | The base-props ABI writes type/flags/blend/constraints/opacity/rotation/transform/selrect/corners only; other fields are set by separate optional APIs or are absent from this adapter. | Reinitialization resets the complete `Shape` value and pool modifier/structure/scale caches. Test covers representative fill, child, type, parent, hidden, opacity, and derived-cache state. Full per-feature WASM ABI uploads remain unimplemented. |
| Repeated sync | Identical input produces identical semantic ABI records. | `syncScene` reuploads the root array and invokes `_render_sync`; the adapter does not build a graph. | Two successive fake-module uploads compare byte records and order. This is deterministic upload evidence, not a repeated browser-render pixel test. |
| Removal | Deleted objects do not remain in the resulting page-root projection. Subtree removal must have explicit semantics. | `documentStore.removeShape` removes a direct object's record and page-root entry, but explicitly does not recurse into children. `selectCurrentPageShapes` filters missing root IDs. | Test removes a direct root and checks the real selector output. Nested deletion can leave orphans and renderer cache/touched-ID reconciliation is unproven: **blocked**. |

## Validation record

The Node fixture tests run through Vite's SSR transform because the production TypeScript adapter uses bundler-style extensionless imports. `vite.renderer-scene-smoke.config.ts` provides a standalone scene-adapter bundle check. It deliberately does not claim browser/WebGL pixel execution. PR #64's browser harness is now in the remotely advanced `main`, but is absent from this branch's explicitly requested `fb0ea41d` baseline; it was not copied from that later head.

## Unsupported-shape compatibility finding

Fail-closed behavior intentionally replaces silent rectangle substitution. `vector-network` is not hypothetical: the React canvas creates these shapes, while the Rust shape enum currently has only `Path` as a possible vector-like raw discriminant. The active adapter does not upload vector-network anchors/segments/regions or establish that `Path` has equivalent semantics. Mapping it to Rust `Path`, `Rect`, or another discriminator would guess and could lose content. The explicit rejection therefore makes this incompatibility visible; it does not claim vector-network support or migration safety.

The adapter still supports the listed known raw types at the discriminant level. That does not prove their complete geometry, fills, text, layouts, or extensions are uploaded correctly; in particular, React's `vector-network` is a separate runtime feature.

## Production integration blockers requiring architecture review

1. Define who projects the durable page/root/parent/child graph into the Rust scene, including nil-root creation, validation, order, nested descendants, and missing-reference handling. Do not wire a replacement builder until this contract is reviewed.
2. Decide how frontend `vector-network` content is represented and preserved at the renderer boundary. Do not map it to `Path` without a field-level geometry/semantics proof.
3. Define deletion/subtree/tombstone behavior and invalidate Rust touched IDs, tile caches, and parent links consistently.
4. Extend separate-property ABI coverage for non-solid fills, strokes, text, SVG, effects, layout, and unknown extension handling before claiming full shape parity.
5. Validate actual repeated React→WASM render state once a reviewed hierarchy bridge exists; the current fake-module and pool tests do not substitute for an end-to-end scene test.

No runtime, route, schema, canonical document owner, or parallel durable model is selected by this work.

## Validation record

Passed locally:

```text
node --experimental-strip-types --test test/p0.5/renderer-scene-contract/scene-boundary.test.mjs  # 5 passed
logos-app: ./node_modules/.bin/tsc --noEmit                                                          # exit 0
logos-app: ./node_modules/.bin/vite build --config vite.renderer-scene-smoke.config.ts               # exit 0; 4.25 kB ES bundle
cargo fmt --manifest-path render-wasm/Cargo.toml -- --check                                         # exit 0
git diff --check                                                                                     # exit 0
```

`cargo test --manifest-path render-wasm/Cargo.toml --locked` was attempted but could not begin compilation: crates.io downloads timed out for pinned dependencies including `skia-safe 0.87.0`, `glam 0.24.2`, and `khronos_api`. This is **Blocked / Not Run**, not a passing Rust regression. The separate `rust/Cargo.toml` workspace suite passed locally, but it does not contain the modified `render-wasm` crate. The renderer-pool regression therefore still needs hosted Rust execution before it can be considered verified.
