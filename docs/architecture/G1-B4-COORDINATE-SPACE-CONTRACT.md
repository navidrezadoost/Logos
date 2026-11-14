# G1-B4 — Coordinate-Space Contract

**Status:** Executable candidate math; editor transform ownership partly unresolved

**Numerical comparison tolerance:** `1e-6` document units (f64); calculations retain full f64 precision and apply no rounding. Renderer transport later narrows to f32, so pixel comparisons must use separate raster tolerance.

**Baseline:** `02318503907f3ed9b2787a0fdde9fc862bbae5d0`

## Coordinate spaces found in the repository

| Space | Evidence and current meaning |
| --- | --- |
| Page/canvas | React `Canvas.toCanvas` computes `((clientX - canvasLeft - panX) / zoom, (clientY - canvasTop - panY) / zoom)`. Created shape bounds and hit tests use these coordinates. Document geometry therefore excludes viewport pan and zoom. |
| Viewport/screen | Canvas rendering applies pan and zoom after document geometry: `screen = page * zoom + pan`. Inverse pointer mapping subtracts pan then divides by zoom. This transform is not serialized. |
| Parent-local | Rust candidate `Shape.selrect` is explicitly documented as parent-local. `Matrix` uses column-vector `[a,b,c,d,e,f]`: `x'=a*x+c*y+e`, `y'=b*x+d*y+f`. |
| Page/world | The checked-in A8 Chromium fixture demonstrates that nested React shape bounds are currently page/canvas-positioned: outer frame starts at `(20,20)`, nested child at `(30,30)`, and Chromium observes that child at the page position around `(30,30)`, not `(50,50)`. The page hierarchy controls traversal/order and parent links; the renderer does not apply each ancestor's bounds translation as an implicit offset to child bounds. |
| Import source | XD converter reads `tx`/`ty` into `bounds.x/y` and also carries the source affine matrix; Sketch copies layer frame positions and a separately built transform. This duplicates position information in two fields for some imports. Importer conversion currently has no shared coordinate-space version or proof that source matrices use the same origin/pivot semantics as React. |

## Established math convention

Use `logos-layout::Matrix` in column-vector convention. A node matrix `local_to_parent` maps coordinates from the node's local space into its immediate parent's space. Parent chains are composed in root-to-leaf order:

```text
node_to_world = root_to_world * ... * parent_to_world * node_to_parent
parent_local_point = inverse(parent_to_world) * world_point
new_parent_local = inverse(new_parent_to_world) * old_parent_to_world * object_to_old_parent
```

Transform a rectangle by mapping all four corners and recomputing the enclosing axis-aligned bounds. Mapping only origin/width/height is incorrect under rotation or skew. Singular or non-finite matrices reject. No implicit rounding occurs; tolerance is `1e-6`.

The executable functions are in `logos-types::document_geometry`; they reuse `logos-layout::{Matrix, Point, Rect}` and do not own shape/document graph validation.

## Rotation, pivot, and multiplication order

`Matrix::rotate_matrix(90)` maps `(1,0)` to `(0,1)` (counter-clockwise in the algebraic matrix; on the editor's downward-positive canvas this appears visually clockwise). `Matrix::Mul` composes right-hand transforms first for a point: `(A * B) * p == A * (B * p)`. Pivoted rotation is `T(pivot) * R * T(-pivot)`. The current React `Shape` separately stores `bounds`, six-number `transform`, and `rotation`; renderer code applies affine transform around the shape center, while Rust candidate geometry stores `selrect`, `transform`, `transform-inverse`, and `rotation`. The exact normalization between the React matrix and Rust's full affine matrix remains unproven for imported/nonidentity transforms.

## Hierarchy and reparenting

Rust candidate parent/child IDs encode structure; a local-to-parent matrix supplies the geometric relation. The current A8 React fixture uses page-positioned child bounds. A future adapter can convert those coordinates only after it can derive the exact source parent-to-page matrix for every supported shape and prove the source object's own matrix/pivot is not double-applied. When reparenting with world-position preservation is explicitly requested, compute `inverse(newParentToWorld) * objectToWorld`; without that option, retain the source's local values and accept the world-space change. The current production Canvas has no reparent operation that demonstrates either behavior, so this contract is for pure candidate operations only.

## Coordinate integration gate

The updated test-only adapter accepts translated ancestry when all parent transforms and rotations are identity, converting page-space bounds to immediate-parent coordinates and restoring them on projection. It rejects rotated/scaled/skewed parent ancestry. This milestone's helper math proves general affine conversion, but does not yet promote converted React fixtures to lossless adapter inputs for transformed parents. Required evidence before that acceptance:

- derive and freeze parent-to-page matrices from actual editor behavior, including transform and rotation pivots;
- prove child visual behavior under parent translation, rotation, and scale with existing renderer pixels;
- demonstrate the same source operation through React geometry and candidate Rust geometry, including source matrix translation cases from XD and Sketch;
- confirm frame clipping is independent from geometric coordinate conversion;
- round-trip source bounds and transforms without changing semantic world-space corners.

Until those checks pass, translated/transformed real editor snapshots remain fail-closed and no production Canvas or persistence behavior changes.
