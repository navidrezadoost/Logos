# G1-B5 — Transform and Renderer Conformance Findings

**Status:** investigation complete; transformed-parent compatibility remains blocked.  
**Baseline:** `884b2b447d030c4c4526ca2256df956980fe6173`  
**Scope:** read-only source audit, candidate geometry vectors, and actual-browser renderer observations. No production renderer or document-store changes.

## Executive finding

The repository does not currently have one executable editor-to-renderer transform contract to conform to. Rust has reusable affine math, and `render-wasm` has its own shape transform and hierarchy behavior. The React Canvas2D fallback ignores shape rotation/matrix, and its flat painter loop does not apply parent transforms or frame clipping. The React Canvas WASM path calls the legacy flat `syncScene`; it uploads parent IDs but does not upload the ordered child lists required by the renderer tree. A8's `projectAndSyncScene` is exercised in a separate support harness and is not called by `Canvas.tsx` at this baseline.

Therefore this milestone does **not** enable rotated, scaled, skewed, or deep transformed-parent conversion in the adapter. G1-B4's explicit rejection remains correct. Browser observations below identify a compatibility gap; they are not parity evidence. No unsupported path is represented as passing.

## Repository-grounded ownership and behavior

| Area | Current source of behavior | Finding |
| --- | --- | --- |
| React coordinates and creation | `logos-app/src/components/canvas/Canvas.tsx`, `logos-app/src/stores/documentStore.ts` | Pointer positions are converted from viewport to page/canvas space by undoing pan and zoom. Rectangle/frame creation stores those bounds and initializes identity transform and zero rotation. New shapes are roots. |
| React sizing and transforms | `Canvas.tsx`, `logos-app/src/types/shapes.ts`, inspector | The scale tool edits width/height bounds around the bounds center. Shape records carry both `rotation` and affine `transform`; the editor path does not establish that the two are a single authoritative transform. |
| React Canvas2D rendering | `logos-app/src/render-wasm/scene.ts::syncScene2D` | Validates shape types, clears the canvas, then paints the flat shape list in reverse order using `fillRect`/`ellipse`. It reads bounds, opacity, fill, and hidden only. Rotation, affine transform, parent hierarchy, and clipping are not applied. |
| React Canvas WASM upload | `Canvas.tsx` → `syncScene` | Calls legacy `syncScene`, not A8's `projectAndSyncScene`. The flat uploader writes the parent UUID in base properties but does not submit `children` lists. |
| A8 hierarchy test path | `scene-projection.ts`, `scene.ts::projectAndSyncScene`, A8 smoke harness | Validates a page graph, serializes deterministic child order, uploads child lists and renders the actual Rust/Skia WASM renderer in Chromium. This path is test support at this baseline, not the production Canvas call site. Existing A8 pixel assertions cover axis-aligned overlapping roots and nested frame descendants. |
| Rust renderer transform | `render-wasm/src/shapes.rs::Shape::get_matrix`, `calculate_bounds`, `calculate_extrect`; `render-wasm/src/render.rs::render_shape` | `rotation` participates in `get_matrix` (bounds origin followed by rotation about `center()`), but the paint path's affine transform is `shape.transform` conjugated about `center()`. This does not prove that the React `rotation` field is painted. `get_concatenated_matrix` walks parent matrices for consumers such as text layout; it is not a general child-paint composition contract. |
| Rust renderer hierarchy and clipping | `render-wasm/src/render.rs::get_simplified_children`, `NodeRenderState`; `render-wasm/src/state.rs::set_parent_for_current_shape` | Traversal consumes ordered child vectors. Parent assignment explicitly does not add child membership. Frame clipping is carried through render traversal and uses frame bounds/transform. Transforming a frame does not itself establish transformed child coordinates. |
| XD import | `logos-app/src/migration/xd/xd-shape-converter.ts` and `xd-format.ts` | XD supplies a six-component affine matrix. Current conversion sets bounds origin from `(tx,ty)` and also keeps the full matrix, including translation; pivot/origin and translation application need fixture-backed confirmation before treating these as one mapping. Rotation is separately extracted. |
| Sketch import | `logos-app/src/migration/sketch/sketch-shape-converter.ts` | Current conversion builds a rotation/flip affine matrix and also copies `layer.rotation` to the separate rotation field. Whether both are intended is unverified; the existing comment documents a direction sign change, not the pivot convention. |
| Rust candidate core | `rust/logos-types/src/document_geometry.rs` | Supplies matrix composition, inverse point mapping, rectangle corner mapping, and world-preserving reparent conversion. It is pure math and does not infer React/renderer pivots or import semantics. |

### Candidate geometry convention

For affine tuple `[a,b,c,d,e,f]`, the candidate core uses `x' = a*x + c*y + e`, `y' = b*x + d*y + f`. A point in nested local coordinates maps to page space as `M_root * M_child * ... * p`; inverse mapping uses the inverse of the target parent's composed matrix. A center-pivoted local transform is `T(p) * L * T(-p)`. Reparenting is `inverse(newParentWorld) * oldObjectWorld`.

This convention is specified in the golden fixture. It is **not** asserted as the current React editor's meaning for all stored bounds, nor as equivalence to imported XD/Sketch records.

## Evidence and support matrix

| Case | Candidate core fixture | Actual renderer observation | React editor compatibility | Adapter action |
| --- | --- | --- | --- | --- |
| Identity rectangle | Golden vectors and native/WASM tests | Existing A8 Chromium harness covers axis-aligned pixels | Canvas2D path supports axis-aligned bounds | No change |
| Single transformed rectangle | Center-pivot matrix and corners in fixture | Rust WASM renderer consumes `transform`; Canvas2D ignores it. Separate `rotation` is not applied by `render_shape` paint code | No proven common `rotation`/matrix contract | Keep fail closed |
| Translated frame with nested rectangle | Existing G1-B4 plus B5 vectors | A8 harness verifies the actual Rust WASM scene upload and nested pixels for its test fixture | Canvas2D paints the flat list and has no frame clip/hierarchy transform; A8 projection path is not the `Canvas.tsx` call path | Keep only G1-B4 translated adapter behavior |
| Rotated/scaled/skewed frame and deep mixed hierarchy | Matrix/vector coverage | No actual editor-compatible pixel equivalence demonstrated | Parent pivots, stored bounds space, rotation/matrix relationship, and clipping order are unresolved | Reject with `UNVERIFIED_COORDINATE_SPACE` |
| Reparent preserving world | Rust helper math vector | No production reparent gesture/operation exercised by this test path | No source mutation contract to compare | Math-only |
| XD/Sketch transforms | Source representation recorded in fixture | No representative imported document fixtures with expected Canvas pixels were found in the audited paths | Translation/pivot, sign, and possible duplicate rotation are unresolved | Reject unsupported imported transform semantics |

### Renderer pixel observation

The existing A8 harness runs in headless Chromium against the built Rust/Skia WASM module and reads the renderer framebuffer. It proves ordered hierarchy upload, pixels for the supplied axis-aligned scene, rejection-before-mutation, recovery after injected upload interruption, deletion clearing, and repeatability. It does not test transforms. The current Canvas2D fallback code is an executable negative control: its draw routine has no calls to `rotate`, `transform`, parent traversal, or clip. Thus transformed-object parity is absent by inspection, not inferred from a synthetic renderer.

The expected visual contract for future support is: all four local corners mapped once by the composed object-to-world matrix; rotation and scale about the declared local pivot; clipping at each clipping Frame in its transformed world polygon; and no change to world corners after a valid world-preserving reparent. Pixel comparisons must allow a documented edge-only antialias tolerance while requiring interior and exterior sample pixels to match exactly.

## Identity and fixture provenance

Golden vectors use fixed synthetic UUID-free geometry inputs. They do not fabricate server-issued IDs. Fixture definitions preserve source and target representation as independent fields; source IDs from any future Go-backed or legacy document fixture must be copied exactly. A fixture containing an unsupported legacy page identity must preserve that source ID and be rejected by the adapter, per the approved G1-B4 policy.

The XD/Sketch entry records the available source representation and deliberately marks importer-to-renderer equivalence unsupported. No historical saved-file compatibility is claimed.

## Performance and resource lifecycle

G1-B4's local adapter timing evidence covers a read-only translation adapter on flat synthetic snapshots; it does not include transformed geometry, current production Canvas rendering, or retained-memory proof. A8's browser test is functional and pixel-oriented, not a performance benchmark. No G1-B5 memory-retention or full-editor responsiveness measurement was performed. A future measurement must report geometry, adapter mapping, WASM invocation, and actual renderer execution separately, repeat runs with browser/GPU/runtime metadata, exercise a representative nested fixture, and document cleanup/recreation; this report makes no latency or leak claim.

## Blockers and next integration prerequisites

1. OpenCode must identify and expose the production Canvas path that owns hierarchy projection and child membership, or explicitly preserve flat rendering as the supported boundary.
2. Define whether child bounds are page-space or parent-local at each creation/import boundary and add a minimal real editor fixture with source IDs.
3. Establish one pivot convention and whether `rotation` and `transform` are mutually exclusive or composed; add actual editor-produced fixtures for rectangle resize/rotation and parent rotation/scale/skew.
4. Wire any approved hierarchy scene upload through the production Canvas owner. This PR intentionally does not change that subsystem.
5. Add renderer fixtures that prove transformed frame clipping and nested Frame/Group pixel output in the real Chromium renderer, then compare the same fixture's corners to Rust Document Core at an agreed tolerance.
6. Resolve XD translation/pivot and Sketch sign/duplicate-rotation behavior with representative source files and observed outputs.
7. Only after these decisions, expand the read-only adapter's accepted cases. Until then, transformed parent cases remain rejected without mutation.

## Test and measurement status

The exact test commands and final-head CI evidence are recorded in the PR description after execution. Native and WASM golden tests validate candidate affine equations; A8 Chromium evidence validates the existing axis-aligned hierarchy path and the Canvas2D negative control. No production transformed-parent parity is claimed. Memory retention is **NOT MEASURED**.
