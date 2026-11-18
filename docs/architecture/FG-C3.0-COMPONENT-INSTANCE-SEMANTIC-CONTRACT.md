# FG-C3.0 — Component and Instance Semantic Contract Proposal

**Status:** Source-backed contract proposal; runtime parity and canonical ownership are unqualified
**Repository baseline:** `3a53c3d848046173c3cba82f4228cbfc1e631c48`
**Scope:** Domain boundaries and acceptance contract only. No runtime model, persistence format, or editor behavior is changed here.

## 1. Purpose and evidence classes

This proposal describes the minimum semantics Logos should make explicit before implementing a complete component/instance system. It distinguishes:

- **Officially documented:** stated by current Figma Design help pages. This is not direct observation of a Figma file or proof of undocumented transition details.
- **Repository fact:** behavior or shape of current Logos source at the baseline above.
- **Proposal:** a safe Logos contract for future approval and implementation.
- **Unresolved:** requires a real Figma Design observation, a canonical-document ownership decision, or representative file fixtures.

No direct Figma Design file observation was available for this milestone. The proposal therefore does not claim complete behavioral parity.

## 2. Reference behavior documented by Figma

Figma describes a main component as defining component properties and an instance as a linked reusable copy that receives main-component updates. Instances may have property changes, including text, fill, stroke, effects, nested instance swaps, layout guides, export settings, and layer names. The documented ordinary override model does not permit arbitrary changes to internal layer ordering, position, constraints, or text bounds. Variant changes and instance swaps attempt to preserve edits according to layer-name and property-baseline rules; text preservation also considers hierarchy. These preservation rules are more specific than a generic deep merge. [Guide to components](https://help.figma.com/hc/en-us/articles/360038662654-Guide-to-components-in-Figma), [Apply changes to instances](https://help.figma.com/hc/en-us/articles/360039150733-Apply-changes-to-instances)

Variants group similar components in a component set. Each variant has a unique combination of set properties; the docs say a set need not contain every possible combination. Component properties include variant, boolean visibility, text, instance swap, and slot. A slot is explicitly structural: it permits adding, resizing, and arranging content inside an instance without detaching. Figma documents minimum/maximum layer counts as guidance that warns but does not prevent continuing; preferred-instance lists are guidance unless the separate “Only allow preferred instances” option is enabled. A boolean property currently controls layer visibility. [Variants](https://help.figma.com/hc/en-us/articles/360056440594-Create-and-use-variants), [Component properties](https://help.figma.com/hc/en-us/articles/5579474826519-Explore-component-properties), [Slots](https://help.figma.com/hc/en-us/articles/38231200344599-Create-and-use-slots-to-build-flexible-components-in-Figma-Design)

Detaching converts an instance to a regular frame, keeps its current layers and properties, removes the source link, and stops future source updates. [Detach an instance](https://help.figma.com/hc/en-us/articles/360038665754-Detach-an-instance-from-the-component)

These docs establish user-facing concepts, not the complete serialization, identity, conflict-resolution, or geometry algorithm. In particular, Logos must not claim Figma's variant/swap preservation algorithm until it has direct fixtures that test it.

## 3. Current Logos architecture inventory

| Area | Current source-backed state | Consequence |
|---|---|---|
| React shape vocabulary | `ShapeType` includes `component` and `instance`. `ComponentMeta` has property definitions; `InstanceMeta` has `componentId`, string-valued `variantProperties`, and arbitrary `Record<string, unknown>` overrides keyed by dot paths. Supported kinds are only `variant`, `boolean`, and `text`. | No explicit component-set entity, instance-swap/slot kinds, or typed override path/value contract. |
| Separate component state | `componentStore.ts` stores `ComponentRecord` snapshots (`defaultShapes`, ordered `defaultChildIds`, properties) and `InstanceRecord`s separately from the document shape map. | Component and instance information is duplicated/mirrored across stores; there is no demonstrated single atomic document transaction for both. |
| Override application | `applyOverrides` shallow-clones shapes, applies a dynamically keyed top-level field, and silently skips malformed paths, unknown shapes, or removed targets. It does not validate allowed fields or value types. | This is a permissive prototype helper, not a safe import or domain resolver. Unknown overrides can disappear silently. |
| Variant resolution | Setter checks membership only when a property is defined as a variant. `resolveInstance` applies overrides to the component's default shape snapshot and does not select a variant-specific definition. | Variant metadata is not a complete variant resolver; do not infer rendered variant behavior. |
| Canvas integration | Canvas creates an instance shell when a component is dragged from assets. There is no demonstrated Canvas path that renders the resolved child tree as the instance. | A created instance record is not evidence of visible or persistent linked-instance behavior. |
| Document and persistence | Component promotion and instance shell insertion are document-store actions, while component defaults and live instances live in `componentStore`. No cross-store serialized round trip is established by this inventory. | Compatibility, durability, and update propagation remain unverified. |
| Rust candidate | Rust `ShapeType` has `Component` but no `Instance` or `ComponentSet` kind. Component is a child-owning shape. No variant/override/instance-link contract appears in the inspected shape model. | Rust is not yet a complete component/instance authority. This proposal does not grant authority or select it as canonical owner. |

Relevant implementation sources: [`shapes.ts`](../../logos-app/src/types/shapes.ts), [`componentStore.ts`](../../logos-app/src/stores/componentStore.ts), [`documentStore.ts`](../../logos-app/src/stores/documentStore.ts), [`Canvas.tsx`](../../logos-app/src/components/canvas/Canvas.tsx), and [`shape.rs`](../../rust/logos-types/src/shape.rs).

## 4. Proposed domain contract

### 4.1 Core records and identity

The logical model should distinguish these entities, whether the eventual schema stores them inline or in normalized maps:

- **Component definition:** stable ID, name, root subtree, declared properties, and definition revision.
- **Component set:** stable ID and ordered membership of component definitions representing variants. A component may remain standalone; sets are optional organization, not a prerequisite for using components.
- **Instance:** stable scene-node ID, source component reference, optional source-file/library reference, selected variant values where applicable, and explicit instance-local changes.
- **Property definition:** stable property ID, display name, kind, default, and kind-specific constraints. Renaming a label must not implicitly change identity.
- **Slot definition/binding:** an explicitly structural extension point and its ordered instance content. Slot contents are not ordinary scalar overrides.

Local component references must resolve to exactly one definition. A cross-file reference must carry an explicit source identity (file/library plus component identity and, where available, revision); an unresolved external source is preserved as unresolved data or rejected by the importing operation, never guessed from a name. Stable scene node IDs and definition IDs must survive edits, save/load, and import/export.

### 4.2 Property types and validation

The domain should represent at least the documented kinds below. The first five align with Figma's current property vocabulary; accepting a kind does not itself claim interaction parity.

| Kind | Value contract | Structural effect |
|---|---|---|
| `variant` | Enumerated string value scoped to a component set; selected combination must identify one member definition. | Selects a definition; it is not a freeform style override. |
| `boolean` | Boolean. Initially visibility-only, matching Figma's documented current limitation. | Toggles the visibility of a declared target. |
| `text` | String plus declared target text node(s). | Changes text content only; text geometry remains governed by supported layout behavior. |
| `instance-swap` | Reference to an allowed component definition or allowed set, validated against declared options. | Replaces a nested instance reference while preserving outer instance identity. |
| `slot` | Ordered child content owned by a declared slot property. Minimum/maximum entry counts and preferred instances are advisory guidance; an explicit `onlyAllowPreferredInstances` setting is represented separately as a hard restriction. | Permits explicit structural insertion, removal, and reorder inside the slot; guidance violations warn and do not reject. A Logos-authored `hardLimits` field is also separate and is not inferred from Figma guidance. |

**FG-C3.1 clarification:** The current reference docs state that slot layer-count limits guide users and show warnings while allowing continued edits. Preferred instances are optional guidance; Figma also documents a separate “Only allow preferred instances” option. The candidate validator must therefore keep advisory counts and preferred lists separate from any explicit allow-list restriction. It must never reject content solely for violating advisory counts. The candidate's exact direct-entry counting basis and behavior for the allow-list option still need behavioral fixtures before a parity claim.

Property IDs, kind, default, options, and target bindings must validate as one definition. Wrong-kind values, missing targets, duplicate IDs, undeclared properties, and invalid variant combinations produce stable diagnostics and reject the proposed edit atomically. Unknown imported property kinds are preserved verbatim as opaque data or cause a typed unsupported-feature result; they are never silently coerced or discarded.

### 4.3 Instance override representation

Replace arbitrary string dot paths conceptually with a typed address:

```ts
interface OverrideAddress {
  targetNodeId: string;
  property: "name" | "text" | "fills" | "strokes" | "effects" | "layoutGuides" | "exportSettings";
}
interface InstanceOverride {
  address: OverrideAddress;
  value: unknown; // must be narrowed by property-specific schema before acceptance
  provenance?: { sourceDefinitionRevision?: string; sourceValueDigest?: string };
}
```

This sketch is illustrative, not a chosen serialized schema. Every accepted property needs a narrow value schema; `unknown` must not leak past validation. Addresses resolve against a stable definition-node identity, not a display name or an unchecked path. The resolver must distinguish:

1. valid address and value;
2. address to a deleted/renamed/replaced target;
3. unsupported property for this target;
4. invalid value;
5. unknown future payload.

For interactive writes, invalid or stale addresses reject before mutation. For import, preserve the original payload with a diagnostic or reject the whole conversion under an explicit lossless-or-reject policy. Do not repeat today's behavior of silently dropping orphaned overrides.

### 4.4 Resolution, propagation, and deterministic conflicts

Proposed resolution order:

1. Resolve the instance's local/external component reference and selected set member.
2. Materialize the selected definition's immutable subtree, preserving definition IDs as provenance while assigning stable instance-scoped scene identities according to an explicitly approved identity policy.
3. Apply validated scalar instance overrides to supported targets.
4. Apply validated nested instance swaps.
5. Materialize slot content at declared slot anchors and in stored order.
6. Produce a derived scene projection and diagnostics without mutating the definition, instance source records, or persistent scene input.

This order is a proposal; step 2's instance child identity mapping, update survival behavior, and collision policy remain unresolved and require a canonical ownership decision plus real-format evidence. Results must be deterministic for the same input and revision. The resolver must not mutate source definitions or persist derived output as an independent token/component registry.

Definition updates trigger re-resolution of dependent instances. Existing edits are retained only when an explicit compatibility rule can map their target and prove the property remains valid. Name-based matching or source-value comparison may be explored to match documented Figma behavior, but must remain an adapter policy backed by fixtures, not an implicit fallback. Ambiguous mappings yield a diagnostic and preserve recoverable source data; they do not silently migrate edits to a different node.

### 4.5 Swap, reset, detach, and deletion

- **Variant selection:** validate the complete property tuple and resolve one member. If it selects no member or multiple members, reject atomically. Partial combinations are allowed only if the chosen component-set contract explicitly defines a deterministic default/member rule.
- **Instance swap:** validate allowed targets, update only the nested source reference, and report each preserved, reset, or orphaned override. The outer instance ID remains stable.
- **Reset:** support reset of one addressed property and reset-all. Reset removes only the selected override data; it does not change the source component or instance identity.
- **Detach:** atomically replace the linked instance with a regular supported container/subtree preserving visible properties and stable scene-node identity where conversion is unambiguous. Remove the live source link. Preserve unsupported source metadata as an extension or reject detach if lossless conversion is impossible. Figma documents conversion to a regular frame and link removal, but Logos' exact node identity and geometry mapping remain to be qualified.
- **Definition or set deletion:** reject while referenced unless a separately approved operation resolves all dependents in one transaction. Never cascade-delete scene instances implicitly.
- **Broken external link:** retain the reference and last-known data only when the saved format explicitly supports that fallback; otherwise surface a blocking unresolved-reference diagnostic. Do not substitute a same-named local component.

## 5. Integration boundaries and ownership gate

This proposal does not choose React, Rust, or Go as canonical owner.

- **Document boundary:** component definitions, set membership, instance source references, overrides, and slots must enter one versioned document contract before durable adoption. They cannot remain dependent on an unpersisted second Zustand store.
- **Operation boundary:** promote, create instance, edit property, choose variant, swap, reset, detach, and delete must be immutable validated operations over one proposed graph snapshot. A rejection leaves definition, scene graph, ordering, selection, and renderer projection unchanged.
- **Resolution boundary:** resolve linked definitions and produce an ephemeral scene projection. Canvas must consume that projection; it must not invent a second JavaScript resolver alongside an eventual canonical engine.
- **Adapter boundary:** React/Rust/legacy converters must preserve IDs, unknown metadata, property kinds, source references, and slot structure, or reject conversion with a stable diagnostic. Conversion must not resolve external libraries by names.
- **Persistence boundary:** no saved-file schema or Go persistence changes are authorized by this proposal. Durable format work waits on canonical-document ownership, versioning, and migration approval.

The Chief Architect's canonical-document ownership decision gates record placement, ID issuance, operation authority, serialization, cross-language bindings, and whether Rust's candidate document module can own resolution. This contract is a domain proposal only.

## 6. Unresolved compatibility questions

Before calling the behavior Figma-compatible, collect direct Design fixtures for:

- Variant selection when an override target is renamed, removed, or has a different baseline value.
- Text preservation across different layer nesting and text bounds.
- Nested component swap with matching/mismatching layer names and property baselines.
- Slot insertion, reordering, nested slots, min/max constraints, and updates from the main definition.
- Definition update propagation while an instance has overrides.
- Detach identity, bounds, order, and behavior for nested instances and slot content.
- Local vs published-library references, missing library/file, and source revisions.
- Undo/redo and concurrent edits to definition and instance.

Each evidence record must state Figma surface/version, date, exact action, initial/final hierarchy and property values, screenshots or supported machine-readable data, and limitations. Mark direct observations separately from official documentation and inference.

## 7. Acceptance tests for a future implementation

No tests are added by this specification because this environment has no controlled Figma file observation and the current semantics do not yet have an approved canonical representation. The implementation phase should add focused tests in this order:

1. **Pure validation:** every property kind's valid/invalid value, definition target, variant tuple, stable diagnostic, and unknown-data preservation.
2. **Graph integrity:** local and external reference resolution, component-set membership, acyclic definition references or explicit cycle policy, slot containment/cardinality, reciprocal parent/child links, stable IDs and ordering.
3. **Immutable operations:** each accepted operation publishes exactly one valid snapshot; every rejected operation publishes none and preserves selection and renderer-visible state.
4. **Resolver determinism:** same definitions/revisions/instance input yields byte-equivalent semantic output and diagnostics; resolver never mutates source.
5. **Propagation and overrides:** typed override application, target removal/rename, component updates, variant changes, swaps, reset-one/reset-all, and explicit orphan handling.
6. **Detach and persistence:** stable identity and lossless-or-reject round trips through the approved document adapter, including unknown fields and legacy fixtures.
7. **Browser parity:** actual Canvas, Layers, and Inspector interactions show the resolved tree and property edits; pixel and selection evidence are tied to the loaded fixture and final artifact.

## 8. Proposed next implementation sequence

1. Obtain the canonical-document ownership decision and approve component identity/reference policy.
2. Create a representative component/instance fixture from a disposable Figma Design file and record the evidence inventory above.
3. Define a versioned typed domain schema and diagnostic catalog; include sets, five property kinds, overrides, slots, and external references.
4. Implement pure validation and deterministic resolution behind the selected authority boundary.
5. Add atomic editor operations and connect Canvas, Layers, and Inspector to one resolved projection.
6. Add lossless-or-reject import/export and migration fixtures only after format ownership and schema versioning are approved.
7. Qualify end-to-end behavior in a real browser; report local tests separately from hosted CI and direct Figma observation.

Until these gates pass, Logos component/instance support remains **PARTIALLY IMPLEMENTED — NOT Figma-QUALIFIED**; Section spatial parity remains **BLOCKED — OBSERVATION REQUIRED**; retained-memory measurement remains **NOT MEASURED**.
