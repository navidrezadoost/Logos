# FG-C2.2 — Section Observation Protocol and Coordinate Decision

**Status:** Protocol prepared; execution **BLOCKED — Figma Design observation unavailable**
**Starting baseline:** `3a53c3d848046173c3cba82f4228cbfc1e631c48`
**Authority:** No production document, renderer, persistence, or Rust authority changes are authorized.

## 1. Evidence availability

This environment has no supplied Figma Design file/session or interactive Figma Design control. Official Help Center pages are available and cited below, but they do not provide enough detail to infer coordinate transitions. Therefore:

- No row in the scenario matrix below is marked directly observed.
- No generated Logos fixture is evidence of Figma behavior.
- No movement, resize, overlap, or nested-coordinate rule is promoted from an inference.
- Do not implement spatial semantics from this protocol until an observation bundle has been reviewed.

The Figma Design Help Center documents that Sections can be moved/resized over objects to add them, that objects can be moved into a Section, and that Sections can be deleted with or without contents. It does not say what happens to the existing contents' coordinates when the Section moves or resizes, nor the exact geometric rule for membership. [Organize your canvas with sections](https://help.figma.com/hc/en-us/articles/9771500257687-Organize-your-canvas-with-sections)

## 2. Reproducible observation setup

Use a new disposable Figma **Design** file owned by an authorized observer. Do not use a team production library, customer design, or an existing shared file. Do not publish components or libraries. Keep each scenario isolated in a separate duplicated page/file so the starting graph is not affected by a previous run.

For each scenario:

1. Record Figma Design surface (web or desktop), visible app/version information if available, browser/OS, observation date/time with timezone, and observer.
2. Create a minimal page with uniquely named test layers: one Section, nested Section where needed, and shapes with different widths/heights. Use both positive and negative canvas coordinates. Names aid screenshots only; they are not stable identity evidence.
3. Capture the initial canvas, Layers panel, selected-node Inspector values, and any supported document/plugin/API data available for IDs, parentage, order, bounds, transforms, and selection. If a field is not exposed by a supported surface, record `UNAVAILABLE`; do not infer it from a layer name or pixels.
4. Perform exactly one described interaction. Capture the same views/data immediately afterward. Record any selection changes, warnings, or confirmation dialogs.
5. Undo to the exact initial state and repeat three times. If results differ, preserve all runs and classify the behavior unresolved.
6. Export or save the disposable fixture and evidence bundle only through supported Figma Design controls. Keep content sanitized. Include screenshots, before/after records, a short action log, and checksums for the captured artifacts.

### Capture record template

```yaml
scenario_id: FG-C2.2-<scenario>
product_surface: Figma Design web | Figma Design desktop
app_version: <visible value or UNAVAILABLE>
platform: <browser and version / OS>
observed_at: <ISO-8601 timestamp with timezone>
observer: <name or approved internal identifier>
file_fixture: disposable synthetic file; no customer content
run: 1-of-3
source_class: DIRECTLY_OBSERVED | OFFICIALLY_DOCUMENTED | INFERRED
initial:
  screenshot: <artifact path + SHA-256>
  nodes: <IDs/types/parent IDs/order/bounds/transforms/selection or UNAVAILABLE>
action: <exact selection, pointer/key action, and release point>
final:
  screenshot: <artifact path + SHA-256>
  nodes: <same fields as initial or UNAVAILABLE>
observed_delta: <facts only>
limitations: <missing fields, ambiguous UI, timing, or none>
review_status: UNREVIEWED | REPRODUCED | DISPUTED
```

Do not promote an observation to `REPRODUCED` unless independent reruns begin from equivalent clean fixtures and show the same material state transition. Distinguish facts visible in captures from interpretations.

## 3. Scenario matrix

Run every scenario three times from its own clean starting fixture. Use a known delta for pointer movement and known edge/corner resize targets. Record sibling order and selection before/after. For nested scenarios, record each ancestor and descendant independently.

| ID | Scenario and exact action | Required before/after capture | Official source status | Direct observation status |
|---|---|---|---|---|
| S1 | Create an empty Section by dragging the Section tool across a blank canvas region. | Section bounds, page parent, root order, selection. | Documented creation flow. | **BLOCKED — OBSERVATION REQUIRED** |
| S2 | Move an empty Section by a known canvas delta, without touching any other object. | Section bounds, order, selected ID, any new membership. | General Section movement is not geometrically specified. | **BLOCKED — OBSERVATION REQUIRED** |
| S3 | Move a populated Section by a known delta. Include one direct child and one grandchild under a Frame/Group; do not cross another object's bounds. | Section and every descendant bounds/transforms, hierarchy, order, selection. | Not specified. | **BLOCKED — OBSERVATION REQUIRED** |
| S4 | Move a nested Section within its parent and then outside its parent's visible bounds. | Both Section bounds, nested descendants, parentage, sibling order, selection. | Nested Sections are allowed; movement geometry is not specified. | **BLOCKED — OBSERVATION REQUIRED** |
| S5 | Resize a populated Section edge/corner without initially touching another object. Repeat with an object fully outside, fully inside, and straddling the resized edge. | Section and child bounds/transforms, membership/order, selection for each geometry case. | Resizing over an object is documented as an add-to-Section method; exact transition is not. | **BLOCKED — OBSERVATION REQUIRED** |
| S6 | Create/resize a Section so a shape is fully inside; in a separate run make only partial overlap. | Bounds, parent IDs, order, selection, any changed visibility/clipping. | Adding objects by moving/resizing a Section over them is documented. | **BLOCKED — OBSERVATION REQUIRED** |
| S7 | Drag one shape into a Section, then drag a direct child out; repeat using keyboard movement if supported. | Shape bounds/transforms, old/new parent and order, selection. | Moving an object into is documented; move-out gesture/geometry is not fully described. | **BLOCKED — OBSERVATION REQUIRED** |
| S8 | Select multiple existing objects with mixed sizes and use **Wrap in new section**. | New Section bounds, selected object IDs, each object's bounds/transforms/order, resulting hierarchy. | Wrap-in-new-Section workflow is documented. | **BLOCKED — OBSERVATION REQUIRED** |
| S9 | Delete a nested Section with the documented preserve-contents shortcut. | Deleted identity, parent/child order, all descendant bounds/transforms, selection. | Preserve-contents deletion is documented; nested result details are not. | **BLOCKED — OBSERVATION REQUIRED** |

Official source: [Figma Design — Organize your canvas with sections](https://help.figma.com/hc/en-us/articles/9771500257687-Organize-your-canvas-with-sections). Do not substitute the FigJam Section API or FigJam UI for these Design runs.

## 4. Coordinate-space decision matrix

| Layer | Repository contract today | Compatibility question | Disposition pending evidence |
|---|---|---|---|
| FG-C2 Section candidate | Section `bounds` are stored independently as authored AABBs. Optional non-Section `worldBounds` is fixture-only. No spatial operation or conversion exists. | Are Figma Section bounds page/canvas coordinates? How does Section hierarchy alter them? | Keep isolated. Existing `reparent` preservation is only a Logos candidate choice. |
| React `Shape` | `bounds: {x,y,w,h}` comment says canvas-local; shape also stores affine `transform`, `rotation`, `parentId`, and `children`. | Are child bounds local or page-space in the actual current document data? | Resolve the type comment versus live creation/hydration data before adapter changes. |
| React-to-Rust adapter | Adapter comment says React child bounds are in page coordinates and subtracts immediate parent `x/y` to produce parent-local `selrect`. It rejects ancestry with non-identity transform/rotation as `UNVERIFIED_COORDINATE_SPACE`. | Which coordinate convention is authoritative for all nested shapes and what is the transform origin/order? | Adapter-level evidence needed; do not change conversion yet. |
| Rust candidate document | Rust shape records carry `selrect` bounds, affine transforms, and parent/ordered-child relations. Component references are validated separately. | Do Section geometry fields map to existing shape fields or require a distinct node type? | Document-model decision required; no schema change. |
| WASM scene projection | Current A9 gate rejects non-identity affine transforms and nonzero rotation as unverified; Section is not a supported renderer kind. | Can Section annotations coexist with renderer scene nodes without changing child paint coordinates? | Renderer remains unqualified; no Section projection. |

### Resolution rule

After observations exist, classify each discrepancy at the narrowest responsible layer:

- **Operation planner:** only if the observed edit changes semantic bounds or membership before adaptation.
- **Geometry adapter:** only if the source and target coordinate meanings are proven and a lossless mapping can be tested both ways.
- **Renderer:** only if the stored scene is correct but projection, paint, hit-testing, or pixel output differs.
- **Document model:** only if stable identity, ownership, or geometry information cannot be represented without loss in the approved model.

Do not move a discrepancy into another layer to avoid changing a contract. Require a before/after fixture, exact expected IDs and order, coordinate round-trip assertions, and browser pixel evidence for any proposed implementation.

## 5. Adoption decision

| Behavior | Status | Reason / next evidence |
|---|---|---|
| Empty Section creation flow and existence of authored bounds | **PARTIALLY QUALIFIED** | Official workflow exists; coordinate values and editor/runtime result need direct observation. |
| Section containment hierarchy and nested Section possibility | **PARTIALLY QUALIFIED** | Official docs specify high-level containment, not complete order/serialization mapping. |
| Add membership by move/resize Section over object | **PARTIALLY QUALIFIED** | Workflow is documented; overlap threshold and side effects remain unknown. |
| Move populated Section, resize effects, automatic containment, nested coordinate composition | **BLOCKED — OBSERVATION REQUIRED** | No direct Design observation and no complete official transition contract. |
| Preserve-content delete for nested Section | **PARTIALLY QUALIFIED** | Choice is documented; nested child ordering/geometry outcome is not directly observed. |
| Current React/Rust adapter for Sections | **BLOCKED — NOT IMPLEMENTED** | React coordinate comments disagree; Rust/renderer have no qualified Section mapping. |

**Smallest safe production change:** none until required observations and coordinate reconciliation are complete. Section spatial parity remains **NOT VERIFIED** and production Section support remains unauthorized.
