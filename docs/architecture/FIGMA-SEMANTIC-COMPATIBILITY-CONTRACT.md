# Figma Semantic Compatibility Contract

**Status:** Behavioral reference and source-backed compatibility inventory  
**Scope:** Current React/Zustand editor projection, candidate Rust document contract, renderer and related component/layout/token models  
**Authority:** This contract sets a target for observable editor behavior. It does not make the Rust candidate production-authoritative or change any saved-file format.

Logos uses Figma Design's observable document hierarchy and editing semantics as the compatibility reference. This means future work must match user-visible structure and behavior where the relevant capability is in scope. Figma's private implementation is not treated as known or copied. This document classifies the current implementation; it does not claim parity based on names or appearance.

## 1. Reference model

The behavioral reference is a design file/document containing ordered pages; each page owns an ordered scene-node tree. Workspace, team, and folder/project organization sit outside the scene tree. Pages contain scene nodes, and nodes have a single parent (or are page roots), stable identity, and ordered children where the node kind permits containment. Frames, groups, sections, components, component sets, instances, and leaf shapes have distinct semantics; they are not interchangeable generic containers.

The user-visible reference rules used here are:

- A document has pages; a page's children are layers in painter order. Figma documents page children as ordered back-to-front (index 0 is bottommost). [Figma Document API](https://developers.figma.com/docs/plugins/accessing-document/), [PageNode API](https://developers.figma.com/docs/plugins/api/PageNode/)
- Frames are explicit containers. They may nest, clip content, and provide frame-specific constraints or auto layout. Groups are distinct: their bounds follow their contents and they do not behave like frames for constraints. [Frames](https://help.figma.com/hc/en-us/articles/360041539473-Frames-in-Figma-Design), [Groups vs. frames](https://help.figma.com/hc/en-us/articles/360039832054-The-difference-between-frames-and-groups), [Constraints](https://help.figma.com/hc/en-us/articles/360039957734-Apply-constraints-to-define-how-layers-resize)
- Sections organize canvas content and may contain nested sections and other layer types; sections cannot be children of frames or groups. The candidate v1 deliberately leaves Component, Component Set, Instance and Boolean relations unqualified pending their separate domain semantics. [Sections](https://help.figma.com/hc/en-us/articles/9771500257687-Organize-your-canvas-with-sections)
- A component set contains component variants. An instance refers to a main component, can receive supported property overrides and variant swaps, and remains linked until detached. [Component sets and variants](https://help.figma.com/hc/en-us/articles/360056440594-Create-and-use-variants), [InstanceNode API](https://developers.figma.com/docs/plugins/api/InstanceNode/), [Instance changes](https://help.figma.com/hc/en-us/articles/360039150733-Apply-changes-to-instances), [Detach an instance](https://help.figma.com/hc/en-us/articles/360038665754-Detach-an-instance-from-the-component)
- Visibility and lock state are node properties. Reparenting changes hierarchy and stacking order; an accepted operation must preserve identity and either preserve world geometry or explicitly define the changed geometry. [Figma REST node types](https://developers.figma.com/docs/rest-api/file-node-types/)

The containment matrix below is a **target rule matrix**, not a statement that Logos implements all target node kinds. For components and instances, the tree refers to the exposed scene hierarchy; definition links and overrides remain separate semantic relations.

| Parent | Allowed direct children in the reference model | Current Logos disposition |
|---|---|---|
| Document | Pages | **PARTIALLY COMPATIBLE** — React has `pageOrder` and `pages`; there is no active document envelope in that store. Rust `Document` is an additive candidate only. |
| Page | Section, Frame, Group, Component, Component Set, Instance, leaf shape | **PARTIALLY COMPATIBLE** — `rootShapeIds` is ordered and strict create supports only Frame/Rectangle. Compatibility additions can preserve other legacy node records, with renderer diagnostics for unsupported types. |
| Section | Section, Frame, Group, leaf shape; component-related and Boolean relations require separate qualification | **NOT IMPLEMENTED in production** — no Section shape kind or section-specific nesting rule. A separately versioned, pure candidate is described below. |
| Frame | Frame, Group, Component, Instance, leaf shape | **PARTIALLY COMPATIBLE** — strict child insertion currently accepts Rectangle only; compatibility paths and hydration can retain broader existing trees. Frame-specific clipping/layout parity is not established. |
| Group | Group, Frame, Component, Instance, leaf shape | **PARTIALLY COMPATIBLE** — strict child insertion currently accepts Rectangle only; group-specific bounds behavior is not established. |
| Component (main definition) | Frame, Group, Component, Instance, leaf shape | **PARTIALLY COMPATIBLE** — component promotion/metadata exist in React; the component registry/default tree is separately stored and is not one atomic document model. |
| Component Set | Component variants only | **NOT IMPLEMENTED** — no component-set node or enforced variant membership. |
| Instance | Exposed instance subtree/overrides as defined by its main component | **PARTIALLY COMPATIBLE** — an `instance` shape and override metadata exist, but resolution is separate, shallow, and not demonstrated as a live linked scene subtree. |
| Leaf shape | None | **PARTIALLY COMPATIBLE** — primitive shapes exist; the shared scene validator checks graph reciprocity but the React shape type set is broader than the renderer-supported set. |

### Current validated containment subset

These are the only new containment relationships this contract treats as accepted by the current strict hierarchy-operation boundary. The fixture-backed test is [`figma-semantic-compatibility.test.mjs`](../../test/figma-semantics/figma-semantic-compatibility.test.mjs), using [`supported-containment.json`](../../test/fixtures/figma-semantics/supported-containment.json).

| Current operation | Logos parent → child | Status | Acceptance evidence |
|---|---|---|---|
| `create-root` | Page → Rectangle | **PARTIALLY COMPATIBLE** | `page-roots-and-ordered-containment` |
| `create-root` | Page → Frame | **PARTIALLY COMPATIBLE** | `page-roots-and-ordered-containment` |
| `insert-child` | Frame → Rectangle | **PARTIALLY COMPATIBLE** | `frame-and-group-rectangle-children` |
| `insert-child` | Group → Rectangle | **PARTIALLY COMPATIBLE** | `frame-and-group-rectangle-children` |

“Partially” reflects that these relationships match the reference's broad hierarchy but the complete Figma behavior (all child node kinds, order convention mapping, frame/group geometry, and persistence round-trip) is not established. Existing compatibility/hydration paths are characterized separately; they do not expand this strict supported subset.

### Versioned strict contract v1

[`figmaHierarchyContract.ts`](../../logos-app/src/document-core/figmaHierarchyContract.ts) defines the current strict subset as a renderer-independent test oracle. Version 1 declares only existing Logos kinds (`frame`, `group`, `rect`), page roots (`frame`, `rect`), and direct children (`frame → rect`, `group → rect`). A Rectangle is a leaf in this subset. The contract module does not create a second store, mutate documents, or participate in production authorization. The existing hierarchy planner and the contract are tested against one another. Renderer capability validation remains a separate boundary and may reject an otherwise semantically valid record.

Sections, Component Sets, and instance semantics are explicitly outside the production strict hierarchy contract v1. Their names in the reference matrix do not create placeholder production Logos nodes or broaden the runtime union. The isolated [Section domain contract v1](FG-C2.0-SECTION-DOMAIN-CONTRACT.md) is a separate candidate oracle; it is not imported by the React production planner or store. Existing React compatibility records of other types remain governed by their current preservation/rendering diagnostics, outside this strict contract.

### Canonical ordering contract for current React representation

This section makes the current editor projection's order explicit; it does not silently change existing saved arrays or claim that the representation is already a canonical persisted schema.

| Order | Current sequence owner | Meaning in the React/editor representation | Adapter/display boundary |
|---|---|---|---|
| Document pages | `pageOrder` | Ordered page sequence; page creation appends, hydration preserves the supplied sequence, and identity association checks it exactly. Its relation to every historical saved-file page order remains **NOT VERIFIED**. | Do not sort, regenerate, or infer this order during hydration/import. |
| Page roots | `Page.rootShapeIds` | Front-to-back; index 0 is topmost. Root creation inserts at index 0. | Layers presentation walks roots front-to-back. |
| Container children | `Shape.children` | Front-to-back; index 0 is topmost. Child insertion/reorder indices use this array order. | Layers walks sibling arrays front-to-back and shows the parent row before descendants. |
| Canvas hit-test | `canvasHitOrder` | Front-to-back candidate order; descendants are visited before their parent so selectable child content is considered over its owner. | This is selection/hit order; it is not itself a persisted sibling array. |
| Canvas painter / renderer | projected root and child ID arrays | Renderer-facing sibling arrays are back-to-front. | `buildSceneProjection` reverses each root/child sibling array exactly once when creating the renderer plan. This adapter mapping is not written back to document state. |

This convention gives Logos a deterministic current mapping to Figma's documented back-to-front painter order. It does **not** prove equivalence across the production React path, Canvas2D fallback, renderer save/load, or historical Go file formats. The contract characterizes Layers and hit-test traversal by source inspection and tests source arrays plus scene-projection reversal; browser pixel ordering remains renderer qualification evidence.

## 2. Current architecture inventory and status

Evidence tiers are independent from compatibility status: **CHARACTERIZED** means source behavior was inspected; **TESTED** means deterministic automated assertions ran; **RUNTIME-VERIFIED** means actual editor/browser behavior was observed. None of these labels alone means complete Figma parity.

| Area | Current implementation and evidence | Compatibility status | Evidence tier |
|---|---|---|---|
| Workspace/team/folder/project | Workspace and project resources are outside `documentStore`; no source-backed contract connects them to page/node ownership. | **NOT VERIFIED** | **CHARACTERIZED** |
| Document and pages | React stores `pages`, explicit `pageOrder`, `currentPageId`, and global shape records. Rust's versioned `Document` is not wired to React, Go, persistence, or saved-document reads/writes. | **PARTIALLY COMPATIBLE** | **TESTED** for current React hydration/JSON state; historical persistence **NOT VERIFIED** |
| Scene graph reciprocity | React roots, parent IDs and ordered children; hydration and scene projection validate reciprocity, uniqueness, cycles and reachability. Rust candidate has related graph invariants. | **PARTIALLY COMPATIBLE** | **TESTED** for fixture subset and A10 operations; no browser parity |
| Root and sibling ordering | React arrays are front-to-back; projection reverses renderer child arrays. No historical save/load parity establishes one shared convention. | **PARTIALLY COMPATIBLE** | **TESTED** in state/scene-plan fixtures; pixel order not runtime-verified |
| Frames | React frame membership/layout fields; strict creation creates page roots and strict insertion accepts Rectangle children. Full frame constraints, clipping and transformed nesting remain unestablished. | **PARTIALLY COMPATIBLE** | **TESTED** for identity-transform rectangle subset; renderer/browser parity **NOT VERIFIED** |
| Groups | React groups support strict Rectangle child insertion; dynamic group bounds and full group transform semantics are not established. | **PARTIALLY COMPATIBLE** | **TESTED** for fixture containment; visual parity not runtime-verified |
| Sections | No React/Rust document scene-node kind or section containment validation in audited paths. | **NOT IMPLEMENTED** | **CHARACTERIZED** |
| Components | React component shape and promotion carry metadata; `componentStore` separately snapshots defaults and properties, without a proven atomic document transaction. | **PARTIALLY COMPATIBLE** | **CHARACTERIZED**; full semantic behavior not tested/runtime-verified |
| Component sets/variants | Component properties and variant values exist, but no `ComponentSet` node or enforced component-only set membership exists. | **NOT IMPLEMENTED** | **CHARACTERIZED** |
| Instances and overrides | React `instance` metadata and separate Zustand records; overrides shallow-spread fields and drop missing shape paths. Link propagation, override policy, swap, detach and persistence are unestablished. | **PARTIALLY COMPATIBLE** | **CHARACTERIZED** |
| Primitive/leaf type system | React and Rust type sets/fields differ; renderer supports a narrower capability set. | **PARTIALLY COMPATIBLE** | **CHARACTERIZED**; Rectangle strict subset tested |
| Visibility | React `Shape.hidden` exists and is carried in scene data; nested hidden selection/render behavior is not covered across all paths. | **PARTIALLY COMPATIBLE** | **CHARACTERIZED**; full runtime policy **NOT VERIFIED** |
| Locking | React lock rejects structural edits; Rust candidate treats React lock as an extension; selection/property behavior is not end-to-end matched. | **PARTIALLY COMPATIBLE** | **TESTED** for strict mutation rejection; browser semantics not runtime-verified |
| Selection | Separate React store; page switching and structural deletion reconcile selection. Hidden-node policy and full Canvas/Layers/Inspector parity remain unverified. | **PARTIALLY COMPATIBLE** | **TESTED** for page/delete reconciliation; browser parity not runtime-verified |
| Transforms | React stores bounds, transform and rotation; Rust stores selrect, affine matrix/inverse, rotation and optional points. Composition/pivot and parent/Canvas parity remain unresolved. | **INCOMPATIBLE** for general transformed-node parity | **CHARACTERIZED** at renderer capability boundary; missing world-preservation precondition is tested; transformed rendering not runtime-verified |
| Clipping | Rust `masked_group` exists but React has no corresponding canonical field; some renderer frame clipping exists without complete transformed nesting evidence. | **PARTIALLY COMPATIBLE** | **CHARACTERIZED**; nested pixel behavior **NOT VERIFIED** |
| Reparenting | Same-page operation requires `preserveWorldPosition`, with cycle/lock/order/graph checks; broad transform support is gated. | **PARTIALLY COMPATIBLE** | **TESTED** for supported identity-transform subset; browser parity not runtime-verified |
| Auto Layout/constraints | React has partial flex fields; Rust layout and renderer modifiers are distinct calculation paths without shared parity. | **PARTIALLY COMPATIBLE** | **CHARACTERIZED** |
| Variables/tokens | Separate token/theme runtime and transient preview; document-scoped variable bindings are not in the React Shape/persistence contract. | **NOT IMPLEMENTED** for document-scoped Figma variable semantics | **CHARACTERIZED** |
| Identity | React uses string IDs and operations preserve existing node IDs; component data has a separate lifecycle. | **PARTIALLY COMPATIBLE** | **TESTED** for fixture and strict-operation IDs; cross-format identity not runtime-verified |
| Hydration/import and saved files | React hydration is atomic for its current projection and retains unknown shape fields; no lossless React/Rust/Go existing-format adapter is proven. | **NOT VERIFIED** for historical round trips | **TESTED** for JSON-compatible React state only; saved-file runtime compatibility not verified |

### Source inventory

- React page/document projection and hydration: [`documentStore.ts`](../../logos-app/src/stores/documentStore.ts), [`shapes.ts`](../../logos-app/src/types/shapes.ts), [`hierarchyOperations.ts`](../../logos-app/src/stores/hierarchyOperations.ts).
- Scene graph checks and renderer order conversion: [`scene-projection.ts`](../../logos-app/src/render-wasm/scene-projection.ts), [`scene.ts`](../../logos-app/src/render-wasm/scene.ts).
- Candidate Rust envelope and graph semantics: [`document.rs`](../../rust/logos-types/src/document.rs), [`shape.rs`](../../rust/logos-types/src/shape.rs).
- Isolated Section domain proposal and pure planner: [`FG-C2.0 Section Domain Contract v1`](FG-C2.0-SECTION-DOMAIN-CONTRACT.md), [`sectionDomainV1.ts`](../../logos-app/src/document-core/sectionDomainV1.ts). This does not alter production types or operations.
- Component definitions, variants and resolution: [`componentStore.ts`](../../logos-app/src/stores/componentStore.ts).
- Token collections and themes in the current runtime: [`tokenStore.ts`](../../logos-app/src/stores/tokenStore.ts), [`tokenWorkflowPreviewStore.ts`](../../logos-app/src/stores/tokenWorkflowPreviewStore.ts), [`contract.ts`](../../logos-app/src/tokens/contract.ts).
- Existing renderer transform/clip limitations: [`G1-B5 transform and renderer findings`](G1-B5-TRANSFORM-RENDERER-CONFORMANCE.md).
- Candidate ownership and boundaries: [`G1-B1 canonical document ADR`](G1-B1-CANONICAL-DOCUMENT-ADR.md); this contract does not amend that proposal or its production-adoption gate.

## 3. Normative behavior for future Logos work

1. **One document graph:** pages own roots; each node has zero or one parent; every parent child-list entry reciprocates the child's parent link; every node is reachable from exactly one page root. No implicit repair, reparenting, ID regeneration, or silent data loss is permitted during load.
2. **Explicit node-kind rules:** a node kind declares whether it is a container and which kinds it may contain. Section restrictions, ComponentSet membership, and leaf-ness are semantic validation, not UI-only conventions.
3. **Stable identity:** page, node, component, component-set, and variable identities survive valid edits, save/load, and import/export. Derived renderer IDs and previews are never persisted as user identities.
4. **Single ordering convention:** persisted sibling lists have a documented order. Adapters map that order explicitly to Figma's back-to-front painter order; a renderer's reversal is not a document mutation.
5. **Visibility and lock are distinct:** hidden affects rendering/hit testing according to a declared policy; locked remains selectable if the product policy allows it but rejects prohibited mutation. Ancestor effects are explicit and tested.
6. **Geometry contract:** local bounds, transform matrix, rotation, pivot, coordinate space and composition order have one normative definition. Reparenting either preserves world geometry within a stated tolerance or is rejected before mutation. Unsupported shear, perspective, or ambiguous imported transforms are preserved-and-rejected, not approximated silently.
7. **Frame and group distinction:** frame clipping, constraints and layout belong to Frames; group bounds and transforms follow group semantics. No generic `isContainer` flag alone can stand in for all behavior.
8. **Components and instances are graph-linked domain entities:** component definition, set membership, instance main-component identity, variant selection, override paths, detach and deletion consequences are validated together with the scene operation. Overrides identify supported target properties; unknown paths are diagnosed and preserved/quarantined rather than silently applied or discarded.
9. **Variables are document-scoped semantic data:** collections, modes, aliases, typed values, bindings, and contextual resolution require a versioned document boundary and durable identity. A separate runtime store or visual preview is not evidence of persisted compatibility.
10. **Atomic operations:** build an immutable proposed change, validate the complete affected graph and operation invariants, then commit once. Rejection leaves document, identity, ordering, selection and renderer-visible state unchanged.

### Executable operation coverage

The hierarchy compatibility tests exercise all seven current strict operation kinds and the v1 containment policy:

| Operation/invariant | Executable evidence |
|---|---|
| `create-root` Rectangle and Frame | `page-roots-and-ordered-containment` |
| `insert-child` Rectangle under Frame and Group | `frame-and-group-rectangle-children` |
| `rename` | `strict-rename-and-invalid-containment-leaves-all-state-unchanged`; publication count in `all-seven-strict-operations-publish-once-and-retain-a-valid-ordered-graph` |
| `reorder` sibling membership/order | `all-seven-strict-operations-publish-once-and-retain-a-valid-ordered-graph` |
| `reorder-root-list` exact root membership/order | `all-seven-strict-operations-publish-once-and-retain-a-valid-ordered-graph` |
| `reparent` parent/child reciprocity, stable ID, sibling order and root order | `all-seven-strict-operations-publish-once-and-retain-a-valid-ordered-graph` |
| `delete` root subtree, root order and descendant selection reconciliation | `all-seven-strict-operations-publish-once-and-retain-a-valid-ordered-graph`; child deletion is also covered by the existing A10 suite |
| Cycle, lock, malformed membership, ambiguous page ownership and rejected-operation immutability | Contract-to-planner rejected-pair and zero-publication tests plus unchanged [`A10 hierarchy operation tests`](../../test/a10/hierarchy-operations.test.mjs) |
| Hydration identity/membership vs scene projection order | `page-roots-and-ordered-containment`, JSON-compatible state round trip, and existing A10 hydration tests |

The all-seven-operation test subscribes to the document store and asserts exactly one page/shape publication per accepted operation. Rejected containment and cycle attempts assert zero publications, unchanged document references and unchanged selection. Existing A10 tests continue to cover locking, malformed membership, and ambiguous cross-page ownership; this milestone does not replace or weaken them.

## 4. Gap report and smallest safe correction boundaries

No gap below authorizes a renderer, document-store, or persistence rewrite. Each item requires its own acceptance evidence before a compatibility status can be raised.

| Gap / violated invariant | Affected source modules | Smallest safe correction boundary | Required regression evidence | Dependency |
|---|---|---|---|---|
| Production Sections remain absent; section nesting restrictions are not wired to production operations. | `logos-app/src/types/shapes.ts`, `hierarchyOperations.ts`, `scene-projection.ts`; Rust `logos-types::ShapeType` and `document.rs` | The isolated FG-C2.0 candidate now defines a versioned matrix and stable diagnostics. Before production implementation, qualify geometry/deletion details against real files, then integrate through an authorized Section-specific boundary. | Candidate matrix tests; reject Section under Frame/Group; retain unknown legacy payload; no rendering or migration claim without browser/fixture evidence. | FG-C2.0 candidate contract; geometry and real-format evidence before production implementation. |
| Component Sets absent; component-only variant membership not enforced. | `componentStore.ts`, `shapes.ts`, `hierarchyOperations.ts`; candidate Rust `shape.rs` | Define IDs and membership as domain relationships before adding UI; keep existing component metadata intact. | Valid/invalid set membership, variants, identity, swap, detach and deletion tests on immutable snapshots. | Section/node vocabulary; component-domain decision. |
| Frame/Group allowed child kinds and behavior differ from the full reference. | `hierarchyOperations.ts`, `scene-projection.ts`, `scene.ts`, `Canvas.tsx`, `shapes.ts` | Expand only the strict operation's accepted matrix after renderer and property compatibility are separately proven. Do not broaden compatibility/hydration implicitly. | Every new pair has positive operation test, reciprocal graph checks and actual Chromium rendering evidence; rejected pair preserves state. | Node-kind contract, then renderer evidence. |
| Figma's back-to-front order differs from React's front-to-back sibling arrays. | `documentStore.ts`, `hierarchyOperations.ts`, `LayersPanel.tsx`, `Canvas.tsx`, `scene-projection.ts`, offline persistence adapters | Preserve current arrays and make adapters declare direction. A future saved-format change requires a lossless versioned adapter, not an in-place reverse. | Create, reorder, reparent, render, Layers presentation, hydration and representative save/load fixtures agree on stable IDs and visual stacking. | Ordering contract v1; historical file corpus before persistence work. |
| General transform, pivot, parent-local coordinate and clip semantics are not equivalent. | `shapes.ts`, `scene.ts`, `scene-projection.ts`, `render-wasm/src/render.rs`, Rust `document_geometry.rs`, import converters | Fix at the narrowest conversion/renderer boundary after selecting one coordinate/pivot convention using real editor-produced examples. Until then reject unsupported cases before mutation. | Rotation/scale/skew, nested frame clip, all world corners, reparent invariance and pixels from actual Chromium. | Ordering and node-kind contract; geometry decision. |
| `component`, `instance`, defaults and overrides are split across document and component stores. | `documentStore.ts`, `componentStore.ts`, `shapes.ts`, persistence adapters | Add typed, atomic operation/adaptation boundary; do not copy data into a parallel canonical registry until ownership is decided. | Reference integrity, supported override value/path validation, propagation, unknown path handling, detach and round-trip. | Canonical document ownership and component-set semantics. |
| Document-scoped variables/modes/bindings are not represented by the active React document serialization boundary. | `tokenStore.ts`, `tokenWorkflowPreviewStore.ts`, `tokens/contract.ts`, persistence adapters | Decide versioned document resource and binding boundary; keep current preview transient and token runtime separate until approved. | Collection/mode/alias/binding identity; save/load round-trip; renderer property result; no preview persistence. | Canonical document ownership plus stable ordering/identity. |
| Lossless cross-format conversion and historical saved-file compatibility remain unverified. | React adapters, Rust `document.rs`/`shape.rs`, Go `filedata` and offline persistence | Build format-specific adapters with preserve-or-reject behavior; do not rewrite legacy payloads or promote Rust production authority in this milestone. | Representative historical and current fixtures, unknown-field preservation, stable IDs/order, repeated round-trip equivalence and rejection atomicity. | Real corpus authorization and canonical ownership decision. |

## 5. Existing-document compatibility and migration policy

- Current React/Zustand and saved-file formats remain untouched by this milestone. No schema version, Go persistence boundary, canonical owner, or Rust production authority is changed.
- Existing data is interpreted only through an explicit adapter. Unknown fields and IDs must be preserved or the adapter rejects/quarantines the source with a stable diagnostic. “Best effort” conversion must not silently drop unsupported shapes, transforms, component data, variable bindings, or hierarchy membership.
- A fixture generated from the current React shape projection is a characterization fixture, not proof of an existing `.penpot`, `.logos`, server, or legacy file round trip. Real representative documents are still required before migration acceptance.
- If a valid older document encodes semantics not supported by the active renderer, retain its source data and report the rendering limitation. Do not normalize it into a lossy alternative tree.
- No compatibility adapter may infer canonical page ownership from a shape's parent alone where a shape can be shared or referenced. Cross-page moves require explicit source and destination ownership plus one atomic operation.

## 6. Minimum compatibility work, in dependency order

1. **Freeze observable vocabulary and order:** use contract v1 to characterize the current strict node-kind/containment subset and array-direction adapters. Keep future node-kind extensions and persisted-order changes behind a separately versioned, lossless-or-reject adapter.
2. **Close graph semantics:** add explicit Section and ComponentSet kinds/rules, validate container-child kinds, and test root/child membership, cycles, reciprocal edges, visibility, locking and deletion effects. Do not retrofit a generic container rule where semantics differ.
3. **Prove geometry semantics:** choose local-coordinate and pivot rules using real editor-produced fixtures; reconcile `bounds`/`selrect`, `rotation`/matrix, frame clipping and painter ordering. Only then widen reparent support.
4. **Unify component domain operations:** model definition/set/instance references, supported overrides, variant swap, detach and deletion transactionally; preserve IDs and unknown payloads.
5. **Reconcile layout and variables:** choose explicit parity fixtures across current layout paths; define document-scoped collections/modes/bindings and versioned import/export before connecting them to shape properties.
6. **Prove adapters and persistence:** create representative real-format fixtures and lossless-or-reject conversion tests across React, Rust and Go boundaries. Do not change durable formats or promote Rust authority until the canonical ownership decision and migration plan authorize it.
7. **Qualify actual editor behavior:** Chromium tests must exercise production Canvas, Layers and Inspector; verify pixels, interaction results, selection, and rejected-operation immutability. Report local and hosted evidence separately.

These steps are proposals for future milestones. They do not authorize subsystem rewrites. A rewrite is justified only by a concrete invariant defect that cannot be corrected at a bounded adapter or operation boundary.

## 7. Focused regression fixture and evidence

The deterministic JSON fixture [`supported-containment.json`](../../test/fixtures/figma-semantics/supported-containment.json) defines page roots, a Frame and Group with Rectangle children, IDs, bounds and sibling order. [`figma-semantic-compatibility.test.mjs`](../../test/figma-semantics/figma-semantic-compatibility.test.mjs), run with `npm run test:figma-semantics` from `logos-app`, hydrates that graph, checks the JSON-compatible React state round trip, compares the hydrated IDs/order with scene projection, exercises all seven strict operations, and verifies reciprocal edges, order, stable IDs, lock/cycle/containment rejection and atomicity. It is a characterization of the current React representation and adapter, not an existing `.logos`, Go, or legacy persistence round trip. It does not imply production Section, ComponentSet, instance, transformed-parent, layout, or historical saved-file parity. The separately versioned Section candidate has its own fixture and tests in [`section-domain-v1.json`](../../test/fixtures/figma-semantics/section-domain-v1.json) and [`section-domain-v1.test.mjs`](../../test/figma-semantics/section-domain-v1.test.mjs); those establish source-level candidate semantics only, not production wiring or runtime parity.

The existing [`A10 hierarchy operation tests`](../../test/a10/hierarchy-operations.test.mjs) additionally cover stable IDs, create-root, reparent, cycle rejection, subtree deletion, lock rejection, hydration, and unsupported-rendering diagnostics. These tests are functional contract evidence, not browser pixel or historical-file evidence.
