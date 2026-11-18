# FG-C3.3 — Component/Instance Typed Schema and Conversion Boundary

**Status:** Non-production schema qualification; TypeScript remains the v1 validator reference.  
**Rust implementation:** `spikes/fg-c3-2-1/rust-planner/src/component_instance_v1.rs`  
**Shared fixtures:** `test/fixtures/figma-semantics/component-instance-domain-v1.json` and `component-instance-v1-invalid-cases.json`

This milestone introduces a strict Rust JSON representation only. It does not add the component domain to `logos_types::Document`, persist a registry, resolve components in production, or transfer semantic authority. The DTO proves that the valid generated v1 fixture can cross JSON → typed Rust → JSON while preserving supported values and authored ordering. The existing TypeScript validator remains the only executable semantic validator in this milestone.

## Field coverage matrix

“Exact representation” describes JSON shape and value preservation only. It does not claim graph, membership, property-target, or expansion validity. Rust's `BTreeMap` sorts object keys on output; arrays retain order. JSON object key order is outside semantic equality.

| TypeScript v1 field | Rust representation | Coverage / limitation |
|---|---|---|
| Document `version` | `u32` with v1 gate | Exact numeric representation; other versions rejected. |
| `components`, `componentSets`, `instances` | `BTreeMap<String, …>` | Exact JSON records; output key order is canonicalized lexically. |
| Component `id`, `name`, `setId` | strings and required nullable string | Exact, including explicit null for `setId`; semantic ID/key and referential checks remain TS-only. |
| `rootNodeIds` | `Vec<String>` | Exact ordered roots. |
| `nodes` | ordered record of `DefinitionNode` tagged union | Explicit `container`, `shape`, `text`, `instance`; unknown kinds/fields reject. |
| Definition node `id`, `parentId`, `childIds` | string, required nullable string, `Vec<String>` | Exact, including authored membership order; graph symmetry/cycles are not checked in Rust. |
| Instance-node `source`, `variantSelection` | `ComponentReference`, ordered string record | Exact supported references and selections; local resolution is not performed. |
| Component `properties` | tagged `ComponentProperty` union | Boolean/text/instance-swap/slot fields exact; unknown variants reject. |
| Property `guidance`, `onlyAllowPreferredInstances`, `hardLimits` | optional typed structs/boolean | Omission is preserved; explicit null and unknown nested fields reject. Numeric counts are unsigned integers; negative/fractional counts reject as incompatible. |
| `variantValues` | ordered string record | Exact values; set tuple semantics remain TS-only. |
| Component set `id`, `name` | strings | Exact. |
| `memberComponentIds` | `Vec<String>` | Exact semantic order; membership reciprocity and uniqueness remain TS-only. |
| Set `properties` and variant property `id`, `name`, `kind`, `values`, `defaultValue` | ordered record, explicit `variant` kind, ordered vector | Exact; unknown property kind rejects. |
| Instance `id`, `source` | string and explicit `ComponentReference` union | Exact local-component, local-set, external and unresolved forms. External `revision` must be present and string-or-null. |
| Instance `variantSelection` | ordered string record | Exact; allowed tuple validation remains TS-only. |
| `propertyValues` | ordered record of `serde_json::Value` | Exact JSON values without coercion. Contextual boolean/text/swap constraints remain TS-only; the TS contract intentionally stores `JsonValue`. |
| `overrides` | ordered vector of typed address plus `JsonValue` | Exact order, target identity, and one of the seven declared fields; target existence/value-field compatibility remains TS-only. |
| `slotContent` | ordered slot record; ordered roots; tagged node map | Exact authored slot IDs and order; IDs are not converted to definition-node IDs. Graph validity and contextual slot constraints remain TS-only. |
| Slot-content node source and selection | same reference union and ordered string map | Exact, no external/unresolved resolution or expansion. |
| External file/component identity and nullable revision | `External { file_id, component_id, revision }` | Exact payload representation. Revision pinning policy and remote resolution are unresolved. |
| Unresolved `locator` | arbitrary `serde_json::Value` | Exact JSON locator preservation; no interpretation. |
| Diagnostic `code`, `path`, `severity`, `detail` | closed diagnostic-code enum, string, error/warning enum, string | Field/value shape exact for current code set. Code enumeration is not a Rust validator and must be extended deliberately when TS adds codes. |

The representation is deliberately lossless-or-reject for the declared v1 payload shape. It does not round-trip unknown fields, future union variants, missing required nullable fields, or values whose JSON type differs from the declared v1 field. No field is silently dropped. It is not byte preserving: whitespace and object-key order are normalized. Floating point JSON values inside `JsonValue` retain JSON numeric semantics subject to `serde_json`; the fixture comparison is semantic JSON equality, not arbitrary numeric lexical preservation.

## Diagnostic coverage boundary

The TS fixture suite executes the current validator against the shared positive and negative cases. Rust currently represents the diagnostic envelope and its current code vocabulary only; it does not generate equivalent validation diagnostics. The following table separates data representability from semantic implementation.

| TypeScript diagnostic family / codes | Relevant input representable? | Rust equivalent diagnostic implemented? |
|---|---:|---:|
| Document/version/JSON: `COMPONENT_DOCUMENT_MALFORMED`, `COMPONENT_VERSION_UNSUPPORTED`, `COMPONENT_JSON_INVALID` | Yes for valid JSON; unsupported structural values fail typed decoding | No; decode errors are not mapped to TS codes. |
| Unknown field / malformed records / ID mismatch: `COMPONENT_UNKNOWN_FIELD`, `COMPONENT_RECORD_MALFORMED`, `COMPONENT_ID_MISMATCH` | Unknown fields reject at decode; remaining cases are typed when shape permits | No. |
| Duplicate global identity: `COMPONENT_DUPLICATE_ID` | Yes | No. |
| Property/node kind or constraints: `COMPONENT_PROPERTY_KIND_UNSUPPORTED`, `COMPONENT_PROPERTY_INVALID`, `COMPONENT_NODE_KIND_UNSUPPORTED`, `COMPONENT_NODE_GRAPH_INVALID` | Declared kinds/fields represent; unknown kinds/fields reject before a Rust value exists | No graph or target validation. |
| References: `COMPONENT_REFERENCE_INVALID`, `COMPONENT_REFERENCE_MISSING`, `COMPONENT_REFERENCE_UNRESOLVED`, `COMPONENT_REFERENCE_CYCLE`, `COMPONENT_AMBIGUOUS_SOURCE` | All four source variants and locator payload representable | No resolution, warning generation, ambiguity check or cycle expansion. |
| Set and variant rules: `COMPONENT_SET_MEMBERSHIP_INVALID`, `COMPONENT_VARIANT_INVALID`, `COMPONENT_VARIANT_DUPLICATE`, `COMPONENT_VARIANT_SELECTION_INVALID` | IDs, ordered members, values and selections representable | No reciprocal membership, tuple uniqueness or selection validation. |
| Values and overrides: `COMPONENT_PROPERTY_VALUE_INVALID`, `COMPONENT_OVERRIDE_INVALID`, `COMPONENT_OVERRIDE_TARGET_MISSING`, `COMPONENT_OVERRIDE_FIELD_INVALID` | JSON values and typed addresses representable; unknown override field rejects | No contextual property or target validation. |
| Slots: `COMPONENT_SLOT_INVALID`, `COMPONENT_SLOT_GUIDANCE_EXCEEDED`, `COMPONENT_SLOT_HARD_LIMIT_EXCEEDED`, `COMPONENT_SLOT_CONTENT_NOT_ALLOWED` | Guidance, hard limits, authored graph and node sources representable | No slot graph/limit/allow-list validation. |

Decode success is not validator parity. Warning/error distinction is represented by `DiagnosticSeverity`; this Rust DTO does not decide when to emit either. The existing TS suite contains the generated positive fixture and shared negative mutation cases and remains the current differential reference. No Rust differential result is claimed.

## Document placement options

| Option | Benefits | Costs / risks |
|---|---|---|
| **A — Document-owned component entities** | One document revision boundary; component definitions/sets and scene instance nodes can participate in one atomic operation; references can target canonical scene-node identities; serialization has one owner. | Requires a designed versioned Document extension and migration policy; current flat `Document.shapes` model does not express all component semantics; avoid duplicating instance identity in both metadata and scene nodes. |
| **B — Isolated semantic envelope** | Allows qualification and conversion experiments without changing current Document; clear gate around TS v1 compatibility; can reject unsupported inputs independently. | A separate durable envelope risks a second registry, duplicate identities/revisions, non-atomic scene/metadata edits, and later migration/adapter complexity. |

**Recommendation:** Keep the current qualification DTO isolated as option B, but target option A for canonical ownership if the Chief Architect confirms document-owned semantics. Component definitions and sets should then be owned within the versioned canonical Document, while scene instances refer to stable canonical scene-node identities and metadata is updated atomically with those nodes. Do not add component entities or persistence to `Document` in this milestone. The current candidate Document's `Shape` component-link fields are insufficient to establish a complete component contract and should not be treated as proof that canonical ownership has already been decided.

## Conversion, identity, and compatibility policy

- Semantic round-trip equivalence means parsing output as JSON and comparing JSON values, preserving array order and all field/value semantics. It ignores whitespace and object-key order. Deterministic Rust output sorts all record keys and preserves every vector's authored order.
- Byte-for-byte equivalence to source files is neither promised nor tested. No lexical numeric representation, whitespace, or source key order is retained.
- The TS validator declares components, component sets, instances, and definition-node IDs in a shared domain identity namespace. Slot-content IDs are scoped to each owning slot graph. Rust represents those strings but does not enforce global/scoped uniqueness.
- Whether canonical document scene-node IDs and component definition IDs share one global namespace is unresolved. A10/G1 document IDs must not be regenerated to fit this candidate schema.
- Component-root reuse and whether definition node IDs are globally unique across definitions remain decisions for the canonical contract; current TS validation's namespace rule is mirrored as documentation, not implemented in this DTO.
- External revision strings may be null. Whether external identities must pin an immutable revision, and how unavailable revisions are surfaced, remains unresolved.
- Unknown fields are rejected. There is no extension bag or preserve-unknown sidecar in v1; future extension requires a versioned contract change rather than silent dropping.
- Orphaned override targets are representable but rejected by the TS validator when their selected definition lacks the target. This Rust milestone does not classify them.
- Detach identity conversion has no v1 field-level contract here. A future operation must define identity issuance and projection/detach mappings without deriving or regenerating IDs during import.
- No automatic migration, normalization, repair, or identity issuance occurs in the Rust decoder.

## Evidence and remaining decisions

The Rust tests use the existing generated `component-instance-domain-v1.json` fixture and assert semantic equality after parse/serialize/reparse, deterministic output, ordered set membership, authored slot IDs, property values, override addresses, unresolved references, and strict rejection of unknown/incompatible fields. The TypeScript test suite uses the same positive fixture and `component-instance-v1-invalid-cases.json` to qualify current TS diagnostics. These are generated contract fixtures, not evidence of production file population or saved-file compatibility.

Not qualified in this milestone: Rust graph and property validation, contextual expansion cycles, Rust/TS diagnostic parity, Go persistence, saved-file migration, production resolution, byte-preserving serialization, and runtime/performance behavior. G1-B9.6 retained memory remains **NOT MEASURED**; G1-B10 authority transfer remains **NO-GO**.

Chief Architect decisions still needed before canonical adoption: confirm document-owned component placement; define the relationship between scene-node and definition identities; decide external revision pinning; and specify detach identity conversion. No production or persistence integration is authorized by this record.
