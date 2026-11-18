# FG-C2.1 — Section Geometry and Behavioral Qualification

**Status:** Source-level qualification complete; spatial parity remains unqualified
**Branch baseline:** `69ee6058c76ce1e5c77c17bb6412c204cdeeecd2`
**Scope:** Isolated TypeScript candidate only. No Figma Design browser session was available, so no behavior below is labeled directly observed. The generated fixture is explicitly a Logos test fixture, not a Figma export.

## 1. Evidence rules

- **Officially documented** means stated by the Figma Design Help Center, linked below. This is evidence of documented workflow, not a complete geometric algorithm.
- **Directly observed** requires a controlled interaction in Figma Design with before/after hierarchy and geometry captures. None was performed for this milestone.
- **Logos candidate choice** identifies behavior of the isolated planner. It is not a claim of Figma parity.
- **Unresolved** means do not implement or advertise the behavior as Figma-compatible until a Figma Design observation resolves it.

The principal source is [Figma Design: Organize your canvas with sections](https://help.figma.com/hc/en-us/articles/9771500257687-Organize-your-canvas-with-sections). It says Sections are top-level by default; may contain all layer types including Sections; may not be children of Frames or Groups; can be created by dragging an area or wrapping a selection; objects can be added by moving/resizing a Section over them or moving an object into a Section; and a Section can be deleted with or without its contents. It does not specify coordinate or overlap algorithms.

No FigJam behavior is used as Figma Design evidence. The [FigJam SectionNode API](https://developers.figma.com/docs/plugins/api/SectionNode/) is not generalized to Design.

## 2. Behavioral reference matrix

| Interaction | Officially documented Figma Design facts | Directly observed here | Logos isolated candidate choice | Unresolved / adoption gate |
|---|---|---|---|---|
| Create an empty Section | Choose Section tool and drag the canvas area. | None. | `create-section` stores the caller-supplied axis-aligned bounds and inserts an empty stable-ID node. | Coordinate origin, snapping, zero-size behavior and rendering are not qualified. |
| Create around existing shapes | Select objects and choose **Wrap in new section**; alternatively draw/drag a Section over objects. | None. | No atomic wrap-selection operation. Explicit `reparent` can express membership separately and preserves candidate geometry values. | Whether wrapping translates objects, changes local coordinates, changes their transforms, or computes bounds from selection. |
| Move a Section containing shapes | Documentation says a Section can be moved over an object to add it. It does not describe moving an already populated Section. | None. | No Section-move operation. | Whether existing contents move with the Section; whether membership is reevaluated; whether nested descendants move. |
| Resize a Section | Documentation says resizing a Section over an object can add that object. | None. | No Section-resize operation. | Whether children scale, translate, remain fixed, or affect bounds; exact membership rules for full/partial overlap. |
| Move shapes into/out of Sections | Moving an object into a Section is documented. A distinct “move out” gesture is not specified in the article. | None. | Same-page explicit `reparent` changes graph membership while retaining identity and stored geometry values. | Whether source/target coordinates are converted, whether dragging across a border has special semantics, and how a move out is exposed. |
| Reparent nested Sections | Sections may contain Sections. | None. | `reparent` validates graph/page invariants and preserves the candidate Section bounds. | Whether nesting affects coordinate spaces, bounds, stacking, clipping, or child motion. |
| Delete Section, preserve contents | The article documents a keyboard command to delete the Section without deleting contents. | None. | `delete-section(contents: "preserve")` removes one Section and splices its direct ordered children into the prior sibling slot. | Whether the actual Design action unwraps only one level or treats nested Sections/descendants differently; all geometry effects. |
| Delete Section with contents | The article documents Delete/Backspace to remove a Section and its contents. | None. | `delete-section(contents: "delete")` removes the entire candidate descendant subtree atomically. | Exact treatment of nested Sections, references, and special layer types requires a real Design fixture. |

**Behavioral conclusion:** documented workflows establish that Section bounds and membership interact, but not the state transition that relates them. There is not enough evidence to qualify Section translation, resizing, overlap thresholds, or nested coordinate composition.

## 3. Coordinate-space contract

### Candidate fields

- A Section's `bounds` is an independently stored axis-aligned rectangle supplied to `create-section`. The planner does not calculate it from descendants.
- Optional non-Section `worldBounds` exists only as test evidence in the candidate model. It represents a fixture's explicit page/world rectangle so tests can detect unintended coordinate changes. It is not a new React/Rust persistence field.
- The candidate performs no local-to-world or world-to-local conversion, transform composition, scaling, clipping, or overlap-based membership inference.
- Existing structural `reparent`, reorder, and preserve-delete operations preserve the bounds values they receive. This is a Logos candidate safety choice, not a verified statement about Figma Design.

### Existing integration representations

- React `Shape.bounds` is a `{x, y, w, h}` rectangle, accompanied by a six-value affine `transform` and `rotation`. Its type comment calls the bounds canvas-local.
- `react-adapter.ts` currently documents child bounds as page coordinates and subtracts the immediate parent's `x/y` to produce parent-local bounds. The same adapter reports `UNVERIFIED_COORDINATE_SPACE` when a parent transform or rotation is non-identity. These comments and the `Shape.bounds` type comment do not establish one unambiguous imported coordinate contract for Section adoption.
- Rust generated geometry exposes `{x, y, width, height}` and a six-value matrix. The existing A9 renderer capability gate rejects non-identity transforms and nonzero rotation as unverified.

These representations are not interchangeable by field renaming. The candidate uses no conversion until React's coordinate convention, nested parent conversion, transform origin/order, and Rust scene expectations are proven with fixtures. **Do not compose a Section transform with child transforms.** A Section transform model does not exist in the candidate.

### Geometry operation policy

There are intentionally no `move-section` or `resize-section` operations. Runtime calls with those discriminators reject as unsupported and leave the snapshot reference unchanged. A future movement contract must say whether it changes Section bounds, child world positions, child local positions, transforms, and membership. A future resize contract must separately define edge/corner behavior, child scaling/translation, and whether content changes Section bounds. No implicit policy is inferred from Frame or Group behavior.

## 4. Pure candidate evidence

[`sectionDomainV1.ts`](../../logos-app/src/document-core/sectionDomainV1.ts) remains a pure domain planner. FG-C2.1 adds no production document or rendering types. The new [`section-geometry-candidate-v1.json`](../../test/fixtures/figma-semantics/section-geometry-candidate-v1.json) is marked `generated-logos-candidate-fixture` and covers:

- Positive and negative page coordinates.
- Nested Sections, Frame/Group descendants, and mixed-size leaves.
- Overlapping Section bounds with explicit membership independent of overlap.
- An empty Section and a zero-area rectangle.

Tests verify that validated create/reparent/delete-preserve candidate operations retain stable IDs, explicit ordered membership, exact stored bounds, and exact fixture page/world rectangles. Delete-with-contents removes the exact tested subtree. Malformed/unsupported geometry operations reject immutably. These are deterministic Logos planner properties; none is labeled as Figma-observed behavior. The tests do not claim a coordinate conversion is supported: no conversion is performed.

Run from `logos-app`:

```sh
npm run test:figma-semantics
```

This source-level suite is not Figma Design interaction evidence, Chromium renderer evidence, persistence compatibility, or performance evidence.

## 5. Adoption decision matrix

| Capability | Decision | Evidence needed to advance |
|---|---|---|
| Keep Section bounds separate from descendant geometry in the isolated candidate | **Ready as candidate-only data handling** | No production adoption implied. |
| Create an empty Section with explicit bounds in the isolated planner | **Ready as candidate-only operation** | Figma-created file/export and actual Design interaction to qualify coordinate values. |
| Structural membership, stable identity, and atomic delete choices | **Source-level candidate only** | Real Design fixtures for nested structures and both deletion choices. |
| Move populated Section | **Blocked / unqualified** | Before/after Design captures of Section, descendants, parentage, and transforms. |
| Resize populated Section and crossing/overlap membership | **Blocked / unqualified** | Edge and corner resize matrix; fully-inside, partial-overlap, and outside objects of different sizes. |
| Shape move into and out of Section | **Blocked for geometry parity** | Before/after coordinates, transforms, membership, and selection in Design. |
| Nested Section movement, resizing, and preserve-delete | **Blocked / unqualified** | Nested Design fixtures with positive/negative coordinates and captured parent/child geometry. |
| React/Rust/renderer adapter | **Not authorized / not ready** | Reconcile React coordinate documentation, prove conversion round trips, and qualify renderer behavior in Chromium. |
| Saved-file/persistence changes | **Not authorized / not ready** | Canonical schema and compatibility decision plus representative real-format files. |

### Smallest safe future adapter boundary

1. Keep the candidate Section DTO isolated from `Shape` and Rust document records.
2. Add an observation importer only after authentic Figma Design fixtures exist; capture source/export provenance, IDs, parent order, Section bounds, each descendant's bounds/transforms before and after each interaction.
3. Implement a typed, versioned adapter that either round-trips the observed subset without loss or returns explicit issues. Do not flatten Sections, synthesize IDs, or attach world bounds to production records to make a sample pass.
4. Keep scene projection disabled until the renderer explicitly supports Section annotations, stacking, hit testing, selection and stale-pixel clearing, with actual Chromium evidence.

## 6. Status

- Official Figma Design workflow facts: **DOCUMENTED**
- Direct Figma Design behavior observation: **NOT PERFORMED**
- Generated geometry fixtures and pure planner tests: **IMPLEMENTED / LOGOS-GENERATED ONLY**
- Section move/resize/overlap semantics: **NOT QUALIFIED**
- Nested coordinate composition: **NOT QUALIFIED**
- Production Section editing or rendering: **NOT AUTHORIZED**
- Persistence migration: **NOT AUTHORIZED**
- Browser/Figma parity: **NOT VERIFIED**
