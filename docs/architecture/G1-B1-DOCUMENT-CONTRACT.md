# G1-B1 — Candidate Document Contract

**Status:** Executable proposal only. Not connected to production persistence or editor state.

## Shape of the contract

The Rust `logos-types::document` module defines a version-1 candidate envelope:

- `Document`: stable UUID, `schema-version`, ordered pages, flat shape records, and unknown top-level extension values.
- `Page`: stable UUID, name, ordered `root-shape-ids`, and unknown page extensions.
- `ShapeRecord`: explicit `page-id`, existing Rust `Shape` fields flattened at the record level, and unknown shape extensions.
- `Shape`: existing Rust domain DTO with UUID, `ShapeType`, local geometry, transform and inverse, optional parent/frame links, and ordered child UUIDs. Direct style values remain first-class `fills`, `strokes`, `shadow`, and `blur`; tokens are not required.

Flat records avoid recursive deserialization. Parent/child and page links are graph edges, while page root and child arrays encode deterministic sibling order. The NIL UUID is reserved for renderer synthetic roots and cannot occur in persisted candidate IDs.

## Decode and preservation policy

`decode_and_validate_json` parses an untrusted JSON value, reports malformed IDs at stable paths, deserializes only the candidate schema, then validates the graph. Unknown object fields on Document, Page, and ShapeRecord round-trip through the extension maps. A future extension key colliding with a typed field is an error in the in-memory validation API. Rust Shape by itself currently drops unknown fields; therefore it must be placed in `ShapeRecord` for the proposed preservation behavior.

Legacy payloads without an approved adapter are represented as `CompatibilityEnvelope { source-format, source-schema, payload }`. This keeps source JSON intact but does not make it an editable Document. Conversion must either preserve every unsupported source field in extensions/envelope or reject before mutation; no guessed parent, child, page, style, layout, component, or text data is synthesized.

## Graph rules

1. Document, page, and shape IDs are non-NIL and globally unique.
2. Every shape has one existing page. Every page root names one existing same-page shape with no parent; a shape has one root membership or one parent membership.
3. Every parent-child edge is bidirectional, same-page, unique, and points from an allowed container (`Frame`, `Group`, `Bool`, or candidate `Component`) to its child.
4. Every shape is reachable from exactly one page root. Parent links are acyclic.
5. `frame-id`, when present, references a same-page Frame. A local `component-id` (no `component-file`) references a Component; external-file references are retained but cannot be resolved offline.
6. Geometry values are finite with nonnegative width/height; transform and inverse members are finite; opacity is finite in `[0,1]`.
7. Ordered arrays are authoritative for sibling order. Duplicate child/root entries are invalid; the validator never sorts document arrays.
8. Unknown extensions are retained. Malformed typed fields and unsupported schema versions are rejected with stable diagnostics.

Container membership currently includes `Component` for candidate nested component definitions. Existing `Shape::is_container()` does not include Component, so this validator has an explicit candidate policy and does not claim parity with that helper. The policy requires architecture approval before integration.

## Typed operation candidate

`apply_operation` first validates the immutable input, clones it, applies one typed operation, and validates the result. Errors discard the clone; input is never changed. There is no generic patch escape hatch.

| Operation | Preconditions and effects |
|---|---|
| `CreateShape` | New non-colliding ID, existing page, valid parent or root slot, insertion index in range. Adds shape and one ordered parent/root membership; candidate validation checks its other references/children. |
| `UpdateShape` | Existing shape and expected parent match. Only name, local bounds, transform, opacity can be changed by this initial candidate. Transform inverse is recomputed; singular transforms fail. Other field updates need explicit typed operations. |
| `DeleteSubtree` | Existing shape and expected parent match. Removes the full descendants closure and unlinks its root/parent membership. External links left dangling (such as local component/frame references) make validation fail; no collateral deletion is inferred. |
| `ReparentShape` | Existing shape, expected old parent, existing destination page/container, valid insertion index, and no cycle. Moves its descendant closure to the destination page and updates edge/order metadata. Local coordinates are preserved; world-position preservation is not inferred. Existing frame references must remain valid or the candidate is rejected. |
| `ReorderChildren` | Expected current ordered IDs exactly match target parent/root list; requested IDs are a duplicate-free permutation. Replaces only sibling order. |

These are pure models for review, not Zustand actions, WASM commands, RPCs, or durable writes.

## Cross-representation compatibility map

| Source | Current evidence | Candidate behavior |
|---|---|---|
| React `logos-app/src/types/shapes.ts` + page/store projection | `bounds`, `children`, `parentId`, no explicit `page-id` on shape, plus vector/text/layout/component/UI fields | Direct decode is rejected. An adapter must map geometry/hierarchy and explicitly carry or reject every field; source fixture is preserved in a compatibility envelope. |
| Rust `logos-types::Shape` DTO | `selrect`, `shapes`, `parent-id`, optional `frame-id`, direct appearance | Supported as the typed shape payload when accompanied by explicit page membership. Record extensions preserve unknown shape fields. Existing direct Shape serialization alone drops unknown fields. |
| Go legacy file-data v67 | Transit keyword strings, `pages` plus `pages-index`, object map keyed by UUID, NIL root frame | Not directly a candidate Document. Retain full source payload until a version-aware adapter maps order, root conventions and keyword values; never persist this candidate automatically. |
| XD importer Artboard/Frame hierarchy | Importer output includes generated IDs; current importer does not map all source metadata/responsive constraints | Preserve as source envelope. Proposed deterministic source-ID mapping is namespaced and collision-checked; missing parent links or unsupported metadata cause explicit reject/loss report, never guessed structure. |
| Nested components and groups | Rust Shape supports `Component` and `Group`; React uses separate component metadata/store and `instance` shell concepts | Candidate accepts nested Group/Component graph only when local references validate. React instance payload remains unsupported and must be retained/rejected pending an approved adapter. |

Golden samples under `rust/logos-types/tests/fixtures/g1-b1/` are synthetic contract evidence. No historical saved-file corpus was available for this milestone; historical compatibility is **unverified**.

## Versioning and migrations

The envelope has an explicit integer schema version. Unknown additive fields are retained within open extension maps. Unknown schema versions are rejected rather than interpreted under v1. Any rename, type change, graph-rule change, enum semantic change, or required-field change needs a new version or explicit migration. Migration must be pure and loss-accounted, must preserve source payload until acceptance, and must be tested against real historical files before a production dual-read or write rollout. There is no automatic migration in G1-B1.
