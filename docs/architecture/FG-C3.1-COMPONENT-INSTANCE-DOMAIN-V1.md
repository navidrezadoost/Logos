# FG-C3.1 — Candidate Component/Instance Domain v1

**Status:** Executable candidate contract; not production-authoritative
**Base:** `f38e15bc04a6591a4e4d2a6fb0decd875d4a823d`
**Scope:** Pure input validation and diagnostics only. No Zustand integration, renderer resolver, Rust authority, or persistence changes.

## 1. Purpose and relationship to FG-C3.0

This module makes the FG-C3.0 proposal executable at the data-validation boundary. It accepts JSON-like `unknown`, validates a versioned candidate graph, and returns deterministic diagnostics without mutating the input. It does not resolve/render instances or establish the final canonical document schema.

Figma documents slot layer-count minima and maxima as guidance: going beyond a count produces warnings while editing may continue. Preferred instances are guidance unless the separate **Only allow preferred instances** option is enabled. This candidate keeps advisory `guidance` separate from `onlyAllowPreferredInstances`; it also has a separate `hardLimits` field reserved for explicit Logos-authored constraints, never inferred from advisory counts. The candidate's count basis is the number of direct ordered slot entries; this is a validator convention pending behavior fixtures, not a Figma parity claim. [Figma slots documentation](https://help.figma.com/hc/en-us/articles/38231200344599-Create-and-use-slots-to-build-flexible-components-in-Figma-Design)

## 2. Module and data boundary

Implementation: [`componentInstanceDomainV1.ts`](../../logos-app/src/document-core/componentInstanceDomainV1.ts). It is not imported by Zustand, Canvas, production React mutations, Rust/WASM, or persistence.

Top-level envelope:

```ts
interface ComponentInstanceDocumentV1 {
  version: 1;
  components: Record<string, ComponentDefinition>;
  componentSets: Record<string, ComponentSet>;
  instances: Record<string, ComponentInstance>;
}
```

The full TypeScript interfaces are the executable schema. The ID policy within this candidate is:

- Component, Component Set, instance, and definition-node IDs occupy one document-wide identity namespace and must be unique.
- Property IDs are scoped to their owning definition or set; record keys must equal their embedded IDs.
- Slot-content node IDs are scoped to one slot-content graph. They identify authored slot records only. They are not derived instance-child IDs, are not mapped to component definition children, and do not establish the eventual persisted identity strategy.
- All IDs are caller-supplied. The validator performs no ID issuance, regeneration, or repair.

Definition nodes form an ordered tree of `container`, `shape`, `text`, and `instance` records. Only containers own children. Definition node identity is local to the candidate document graph. A nested instance has an explicit source and variant selection. The model does not build an expanded instance subtree.

Component Sets own ordered `memberComponentIds` and variant properties. Every member points back to exactly one set and carries exactly one allowed value for each set property. Variant tuples must be unique; a set may omit combinations. A set reference plus selection must resolve to exactly one member. A direct component reference identifies that component and cannot also carry set selection.

Instances carry an explicit local-component, local-set, external, or unresolved source; set selections; typed property values; typed scalar overrides; and per-slot authored content graphs. External references require file ID, component ID, and nullable source revision. An unresolved reference retains its opaque JSON locator. Neither kind is resolved by display name.

## 3. Property and slot validation

The candidate covers all five current Figma property kinds:

| Kind | Candidate validation |
|---|---|
| `variant` | Set-owned property; unique allowed string values and a default included in its values. |
| `boolean` | Boolean default and instance value; target node must exist. |
| `text` | String default and instance value; target must be a text node. |
| `instance-swap` | Nested-instance target; unique allowed local component IDs and an allowed default/value. |
| `slot` | Container target; ordered content graph; optional advisory guidance, explicit preferred-instance allow-list setting, and separate explicit hard limits. |

Slot entries preserve caller order. Advisory `guidance.minimumEntries` and `guidance.maximumEntries` count direct roots in this candidate and emit `COMPONENT_SLOT_GUIDANCE_EXCEEDED` warnings only. They do not make validation fail. `guidance.preferredComponentIds` is advisory by itself. `onlyAllowPreferredInstances: true` is a separate hard rule; this candidate requires every direct slot entry to be an instance resolving to an allowed local component. `hardLimits` are separate explicit Logos restrictions and reject only when present and violated. Whether count rules, all layer types, nested preferences, and the allow-list behave exactly like Figma remains subject to direct observation.

Ordinary instance overrides use a typed address `{ targetNodeId, field }`, not arbitrary dot paths. The current candidate fields are name, text, fills, strokes, effects, layout guides, and export settings, with string values for name/text and JSON arrays for the collection fields. Addresses must point into the selected definition, values must match the field type, and duplicate addresses reject. This is deliberately a narrow contract; renderer property schemas and per-node applicability still require approval and qualification.

## 4. References, unknown data, and diagnostics

### Unknown-data policy

Component-domain v1 uses an explicit **reject** policy for unknown envelope, record, node, property, and reference fields. Unknown property kinds produce `COMPONENT_PROPERTY_KIND_UNSUPPORTED`. The validator returns `document: null` on any error, so an importer cannot treat a partial parse as accepted data. There is no silent field pruning, unknown-field dropping, ID rewriting, or hierarchy repair.

An explicit `external` or `unresolved` reference is different from an unknown field: its source identity/locator remains in the original document, the result may be structurally `ok`, and a `COMPONENT_REFERENCE_UNRESOLVED` warning states that the source is not locally resolved. Consumers must not treat this warning as successful instance resolution. Missing local IDs, malformed references, or ambiguous source selections are errors.

### Graph invariants

- Record keys agree with stable embedded IDs; document entity IDs do not collide.
- Definition and slot-content node graphs have ordered roots, reciprocal parent/child edges, unique ownership, no containment cycles, and no unreachable nodes.
- Component Set membership is reciprocal and ordered; each member belongs to one set and has one valid, unique variant tuple.
- Local references resolve by typed ID. Cycle checks run in each authored instance's concrete expansion context: the selected root component/variant, that instance's selected swaps, and its authored slot content. Instance selections from separate roots are never combined into one graph. External and unresolved references remain opaque and cannot provide local expansion edges.
- External and unresolved references are explicit opaque leaves; their remote graph cannot be cycle-checked without authorized resolution data.
- Slots are owned only by a declared slot property. Content nodes have one slot owner. Instance-swap and preferred IDs must be declared local components.

### Diagnostic behavior

Each diagnostic has a stable code, JSON-like path, severity, and detail. The complete list sorts by code-point path, code, and severity, independent of insertion order and host locale. Errors reject the candidate (`ok: false`, `document: null`); warnings are retained and do not reject. On valid input, `document` is the original reference. Validation never mutates either accepted or rejected input.

## 5. Executable evidence

Positive fixture: [`component-instance-domain-v1.json`](../../test/fixtures/figma-semantics/component-instance-domain-v1.json). It covers standalone definitions, a set with ordered variant members, direct and set-linked instances, nested references, all five property kinds, typed overrides, slot order, advisory warning, and an unresolved source locator.

Negative fixture cases: [`component-instance-v1-invalid-cases.json`](../../test/fixtures/figma-semantics/component-instance-v1-invalid-cases.json). The regression suite also checks malformed JSON/version, unknown kinds, active reference cycles, independent opposite swap selections, variant-dependent paths, slot-content references, deterministic ordering, no mutation, warning-only guidance, and explicit hard-limit rejection: [`component-instance-domain-v1.test.mjs`](../../test/figma-semantics/component-instance-domain-v1.test.mjs).

## FG-C3.1.1 cycle qualification boundary

The validator proves structural local-reference validity and rejects a cycle when it is reachable along an authored instance expansion path. It evaluates each top-level instance separately and applies only that instance's root-level property values and slot content. Nested definitions use their authored references, selected set variants, and declared swap defaults; unresolved or external leaves stay opaque. This prevents mutually opposite swaps on unrelated instances from creating a synthetic cycle.

This is graph validation, not full instance materialization. It does not produce an expanded tree, resolve renderable property values, issue derived child identities, or establish canonical persisted identity. In particular, nested instances do not carry their own persisted exposed-property values or slot-content selections in v1. Identity mapping, nested override propagation, and production resolution remain gated on the canonical identity/ownership decision.

Run from `logos-app`:

```sh
npm run test:component-instance-domain
```

The focused suite currently has eight Node tests, including fourteen table-driven negative fixture cases. This is generated Logos candidate evidence, not a Figma export or direct Figma observation. It does not verify Canvas rendering, user interactions, saved-file compatibility, or production resolution.

## 6. Adoption gate and known limits

| Capability | Current candidate status | Additional gate before adoption |
|---|---|---|
| Envelope, record shape, typed property values, set membership | **VALIDATED in isolated candidate** | Canonical-document ownership, schema review, cross-language contract. |
| Local component and set reference integrity | **VALIDATED for supplied graph** | Real document fixtures and approved nested-definition semantics. |
| External/unresolved source | **PRESERVED with resolution warning** | Approved library identity/revision policy and authorized resolver. |
| Variant tuple uniqueness and exact selection | **VALIDATED in candidate** | Direct Figma fixtures for variant changes and edit preservation. |
| Scalar override path/value shape | **VALIDATED in candidate subset** | Qualified property applicability, update propagation and reset behavior. |
| Slot authored graph/order and advisory warning | **VALIDATED in candidate** | Direct observations for count basis, preferred filtering, restrictions, update/reset, and all supported child types. |
| Slot-content IDs | **Scoped authored IDs only** | Chief Architect must decide canonical identity and any instance-child mapping. No such mapping is proposed here. |
| Instance materialization, Canvas/Layers/Inspector behavior | **NOT IMPLEMENTED** | Approved resolver owner, browser interaction and pixel qualification. |
| Durable persistence and migration | **NOT IMPLEMENTED** | Canonical-document ownership and lossless-or-reject fixture corpus. |
| Figma behavioral parity | **NOT QUALIFIED** | Controlled Figma Design observations; generated candidate fixtures are not evidence. |

Current statuses remain: component-domain candidate validation **IMPLEMENTED — NON-AUTHORITATIVE**; production Component/Instance resolution **NOT AUTHORIZED**; canonical ownership **UNDECIDED**; Section spatial observation **BLOCKED**; memory measurement **NOT MEASURED**; G1-B10 authority transfer **NO-GO**.
