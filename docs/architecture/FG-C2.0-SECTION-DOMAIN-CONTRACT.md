# FG-C2.0 — Section Domain Contract v1

**Status:** Versioned, pure-domain candidate; not production-enabled
**Evidence:** Source-backed Figma rules; deterministic candidate tests; no browser/runtime parity
**Authority:** This proposal does not change the production React document, renderer, Rust authority, Go persistence, or saved-file formats.

## 1. Reference evidence and limits

Figma Design documents Sections as top-level canvas elements by default. They may contain other Sections and layer types, and may not be contained in Frames or Groups. Objects can be added by moving or resizing a Section over them, or by moving objects into it. Figma documents two deletion choices: delete the Section with its contents, or delete the Section without deleting its contents. [Figma: Organize your canvas with sections](https://help.figma.com/hc/en-us/articles/9771500257687-Organize-your-canvas-with-sections)

Sections have authored canvas bounds: Figma's documented creation flow drags an area, and the edit flow resizes a Section. The public help page also says adding a frame to a Section can resize the Section to accommodate it. This supports independently stored bounds, but does not establish the complete resize/move/reparent coordinate algorithm. [Figma: Frames in Figma Design](https://help.figma.com/hc/en-us/articles/360041539473-Frames-in-Figma-Design)

The public SectionNode API describes **FigJam** Sections. Its explicit statement that resizing does not resize children is scoped to FigJam and is not used here to assert Figma Design parity. [Figma SectionNode API](https://developers.figma.com/docs/plugins/api/SectionNode/)

The Section API's component behavior, exact z-order mapping, lock propagation, visibility inheritance, transformed child behavior, automatic bounds adjustment, and deletion of nested Section contents are not fully established by these sources. Candidate behavior below is therefore labeled where it is a Logos contract choice rather than a verified Figma rule.

## 2. Identity, ownership and hierarchy

The candidate uses one typed node graph for each document snapshot:

- Page owns an ordered list of root nodes.
- Each node has one stable UUID, one page ID, zero or one parent ID, and an ordered child list.
- A Section is a distinct `section` kind with authored bounds, `visible`, and `locked` fields. It is not represented as a generic Frame or Group.
- The candidate may carry non-Section `worldBounds` solely as geometry evidence in fixtures. This field is not a proposed replacement for the existing React/Rust geometry contract.
- Section children may be nested Sections, Frames, Groups, or the supported leaf kinds below.
- Frames and Groups may contain the supported ordinary Frame/Group/leaf subset, but never a Section.
- Every membership entry and parent ID must agree. Every node must be reachable from exactly one page root. Duplicate ownership, dangling references, cross-page membership and cycles reject the complete candidate operation.
- IDs are supplied by the caller and preserved. The planner validates UUID form; it never creates, repairs, or replaces identities.

This model deliberately keeps the candidate's common ownership edge explicit. It does not attempt to map a Section member into the production `Shape.parentId` graph or assert that doing so is lossless for legacy files.

## 3. Version 1 containment matrix

| Parent | Section | Frame | Group | Supported leaf | Component / Component Set / Instance | Boolean |
|---|---:|---:|---:|---:|---:|---:|
| Page | Allowed | Allowed | Allowed | Allowed | Unqualified | Unqualified |
| Section | Allowed | Allowed | Allowed | Allowed | Unqualified | Unqualified |
| Frame | Rejected | Allowed | Allowed | Allowed | Unqualified | Unqualified |
| Group | Rejected | Allowed | Allowed | Allowed | Unqualified | Unqualified |
| Leaf | Rejected | Rejected | Rejected | Rejected | Unqualified | Unqualified |

Supported leaf kinds are `rect`, `circle`, `ellipse`, `path`, `text`, `svg-raw`, and `vector-network`. “Allowed” means only that this candidate containment relation is accepted; it does not imply renderer support or full property compatibility. Figma's help says Sections can contain all layer types, but Logos keeps Component, Component Set, Instance, and Boolean relations **unqualified** until definition links, overrides, Boolean operands, serialization, and deletion consequences have their own contract. Unqualified means reject in this v1 planner, not silently accept.

All relations must also satisfy graph invariants. A move that would make a node its own ancestor, give it two owners, or associate it with a different Page rejects before publication.

## 4. Candidate operation boundary

[`sectionDomainV1.ts`](../../logos-app/src/document-core/sectionDomainV1.ts) is pure and deterministic. `planSectionOperation(snapshot, operation)` validates the source graph, constructs a deep immutable candidate, validates the full result, and returns either the new snapshot plus changed IDs or a stable diagnostic with the exact original snapshot reference and no changed IDs. It uses caller-provided UUIDs. It has no Zustand, renderer, network, Rust, or persistence imports.

| Operation | Candidate semantics |
|---|---|
| `create-section` | Insert an empty Section at a page or Section child index with authored bounds. Parent must be Page or Section. |
| `insert-node` | Insert a new, unattached, empty node under a supported Section, Frame, or Group after containment and page checks. Children can then be inserted through additional validated operations. |
| `reorder` | Reorder one node within its current ordered sibling list. |
| `reparent` | Move one node within the same Page to a Section, Frame, Group, or Page root where the matrix permits. Identity and all bounds/child geometry are preserved. Cross-page reparent is rejected. |
| `delete-section(contents: "delete")` | Delete the Section and its full descendant subtree as one operation. This is the documented Figma Design destructive choice. |
| `delete-section(contents: "preserve")` | Remove only the selected Section and splice its immediate ordered children into the former parent's slot; preserve every descendant identity and nested relationship. This is a candidate interpretation of Figma's “without deleting its contents” choice; nested behavior remains unqualified. |

Mutations inside a locked Section or node, or below a locked ancestor, reject with `SECTION_LOCKED_ANCESTOR`. Deleting or unwrapping a Section also rejects if its affected subtree contains any locked node. The candidate lock rule is a safety rule, not a claim of complete Figma selection/lock parity. Visibility is carried as data and is not used to decide structural validity. Rejected operations preserve the entire input snapshot.

Stable diagnostics distinguish malformed runtime records, unsupported node kinds, invalid flags/order, missing ownership, duplicate ownership, cycles, cross-Page requests, forbidden containment, unqualified relations, lock rejection and invalid Section bounds. The validator applies containment checks to both Page-root edges and node-child edges. It validates external JSON at runtime rather than trusting TypeScript types.

Diagnostic precedence is fixed: (1) snapshot envelope/version and record structure, kind, flags, bounds and child-list shape; (2) page ordering and IDs, missing Page/parent/child references, local duplicate membership and containment; (3) cycles in all node components, including disconnected components; (4) ownership duplicated across distinct parents; (5) unreachable nodes, parent reciprocity, then Page ownership. Thus cycle errors take precedence over the duplicate-owner or unreachable symptoms that the same cycle can create. If an input has independent defects, the earliest phase above determines the returned stable code.

Operation input has its own fail-closed boundary. The planner validates the snapshot first; an invalid snapshot is returned unchanged with its snapshot diagnostic before operation processing begins. With a valid snapshot, the planner requires a plain operation record and an own, recognized `kind`; unknown discriminators return `SECTION_OPERATION_KIND_UNSUPPORTED`, malformed/missing operation structure returns `SECTION_OPERATION_MALFORMED`, and an exhaustive `switch` dispatches the five declared operations. Every operation validates required fields, UUIDs, indices and optional flags before lookup or mutation. Inserted nodes are checked at runtime for kind, required identity/parent/list/flag fields, bounds, and an empty child list. Delete requires an own `contents` field equal to exactly `delete` or `preserve`; otherwise it returns `SECTION_DELETE_CONTENT_POLICY_UNQUALIFIED`. Valid but missing Page/parent/target references and out-of-range indices are rejected during operation-specific validation. Every rejection returns the exact input snapshot reference, an empty `changedIds` list and no mutation.

## 5. Geometry contract and explicit unknowns

Section `bounds` are explicitly authored in the candidate and are not calculated from descendants. Reparent, reorder, and preserve-delete operations do not rewrite a child node's bounds. The candidate contains no move or resize operation, so it cannot accidentally claim a world-space transform rule.

Figma Design source confirms Sections can be resized and that adding content can affect Section extent, but this source set does not define whether every drag, resize, reparent, or nested Section change translates children, changes membership based on overlap, adjusts the Section bounds, or changes world-space positions. Those behaviors remain **NOT QUALIFIED**. Do not connect this candidate to production editing until representative Figma-created fixtures and browser observations establish the required geometry rules. The FigJam API behavior is not substituted for Design behavior.

## 6. Executable evidence

The candidate is exercised by [`section-domain-v1.test.mjs`](../../test/figma-semantics/section-domain-v1.test.mjs) against [`section-domain-v1.json`](../../test/fixtures/figma-semantics/section-domain-v1.json). Tests cover:

- The complete matrix for candidate node kinds, including explicit unqualified classes.
- Page-root and nested Section creation, stable IDs, ordered insertion and reordering.
- Frame, Group and supported leaf containment; Section-under-Frame/Group rejection.
- A matrix-driven check of every Page and node parent/child kind against both the validator and planner, including root-level unqualified relationships.
- Runtime rejection of unknown kinds, malformed records, flags, child lists, bounds and references.
- Runtime rejection of unknown operation discriminators, absent/malformed payloads and deletion policies, with exact snapshot identity and immutability checks.
- Reparenting, root order, self/multi-node/disconnected cycle detection, duplicate ownership, and page-ownership validation.
- Delete subtree and preserve-content unwrap, including IDs and sibling order.
- Lock rejection, malformed identity/bounds, duplicate ownership, and exact input immutability on rejection.
- Child geometry and Section bounds remain unchanged under candidate hierarchy edits.

Run from `logos-app` with `npm run test:figma-semantics`. This is pure-domain/source-level evidence. It is not Chromium, renderer, persistence, Figma browser parity, or performance evidence.

## 7. Production adoption proposal — gated and not implemented

No adoption work is included in FG-C2.0. Once geometry and migration fixtures are accepted, a separately authorized implementation would need:

1. **React Shape types/document store:** add a distinct Section record or typed Section node, preserve UUID, page ownership, order, bounds, title/appearance fields and unknown payloads. Define a versioned load boundary and explicit preserve-or-reject conversion. Do not overload Frame/Group.
2. **Hierarchy planner:** add Section-specific create/insert/reorder/reparent/delete operations and whole-page validation. Keep parent-child reciprocity, stable identity, same-page policy, lock handling and both delete choices atomic. Rust delegation would be a later, separate canonical-authority decision.
3. **Scene projection/renderer:** define whether Section renders as canvas annotation, container, or both; define bounds, stacking, hit testing, clipping, hidden behavior and fallback diagnostics. Renderer support must be verified before production editability is advertised.
4. **Layers and Inspector:** display Sections distinctly; support nesting, ordering, rename, lock/visibility, bounds editing, selection and both deletion actions without bypassing the operation boundary.
5. **React/Rust conversion:** add explicit versioned mappings for Section identity, ownership, order, geometry, visibility/lock and unsupported fields. Preserve or reject; do not synthesize IDs or silently flatten Sections. This depends on the Chief Architect's canonical-document ownership decision and Rust schema/API approval.
6. **Offline and Go persistence:** first inventory the actual formats and representative files. Add versioned adapters and round-trip fixtures only after authorization; do not change Go persistence or saved-file formats as part of the candidate contract.

Acceptance should require generated valid/invalid fixtures, exact IDs/order round trips, immutable negative cases, real Figma Design samples for nested/deletion/geometry semantics, Chromium Canvas/Layers/Inspector interactions, renderer pixel checks, and migration evidence. No browser evidence currently exists for this candidate.

## 8. Current status

- Section domain contract v1: **PROPOSED / TESTED LOCALLY**
- React/Zustand production support: **NOT IMPLEMENTED**
- Rust canonical authority: **NOT AUTHORIZED**
- Renderer and Chromium parity: **NOT VERIFIED**
- Figma geometry/deletion parity: **PARTIALLY SOURCED; REMAINS UNQUALIFIED**
- Persistence or migration: **NOT IMPLEMENTED**

### Reference sources

- [Figma Design — Organize your canvas with sections](https://help.figma.com/hc/en-us/articles/9771500257687-Organize-your-canvas-with-sections)
- [Figma Design — Frames in Figma Design](https://help.figma.com/hc/en-us/articles/360041539473-Frames-in-Figma-Design)
- [Figma Design — The difference between frames and groups](https://help.figma.com/hc/en-us/articles/360039832054-The-difference-between-frames-and-groups)
- [Figma Plugin API — SectionNode (FigJam API; not generalized to Design)](https://developers.figma.com/docs/plugins/api/SectionNode/)
