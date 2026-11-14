# G0-B Domain and Scene-Graph Invariants

**Status:** Proposed invariants for a future canonical contract. A8 currently validates a renderer projection; the React store does not enforce this full list.

## Required graph invariants

1. IDs are stable, unique, canonical UUIDs within the document. No document object may use renderer-reserved synthetic IDs.
2. Every page has one ordered root list; every shape belongs to exactly one page, including nested shapes. Page-root and parent ownership are mutually exclusive.
3. Every non-root has exactly one extant parent. Each parent lists each child exactly once and each child's parent points back to it.
4. Parent-child edges are acyclic. Every page-scoped node is reachable from exactly one page root; no dangling parent, child, component, token, or prototype references.
5. Ordered roots and children are semantic paint/layer order. Sorting IDs or deriving order from map iteration is forbidden. Renderer conversion may transform order only through an explicit documented adapter.
6. Root kind and root deletion semantics are explicit; a renderer synthetic root is never a document object.
7. Numeric values are finite and within domain-specific bounds: IDs parse; bounds are finite and nonnegative as appropriate; transforms are finite and invertibility requirements explicit; opacity and percentages bounded; layout constraints internally consistent. Do not silently clamp invalid persisted values.
8. Visibility and locking do not alter graph reachability. Locked/hidden nodes remain structurally valid. Operation authorization may prohibit edit, but does not imply deletion or reparent behavior.
9. Renderer-only caches, transformed points, selection, GPU handles, and synthetic nodes are derived state with explicit invalidation, never durable document authority unless a future version explicitly adopts cache semantics.

## Operation semantics proposal

- **Create:** require new globally unique ID, valid page/root or parent, known type/version; validate full candidate graph before commit.
- **Update:** only allow typed fields; geometry and type changes validated together; mutation must not create an invalid transform or parent relation.
- **Delete subtree:** include the target and every descendant in the atomic closure; detach exactly once from root/parent list; explicitly handle inbound component/interaction/token refs by reject, rewrite or tombstone policy. Current `removeShape` deletes only one record.
- **Reparent:** reject self/descendant targets; require destination exists and is a container; preserve explicit sibling position; update old/new child lists and parent/frame/page ownership atomically. Coordinate transform preservation needs a separately tested policy.
- **Reorder:** require submitted list be an exact permutation of current siblings (or documented partial-order operation), reject duplicate/missing IDs, never introduce cross-parent roots.
- **Set style/layout/binding:** type and numeric validation plus reference checks; direct values remain supported. Binding a token cannot silently rewrite fallback values.
- **Prototype interaction:** source and destination exist in the same document or use a versioned external reference; define behavior when either is deleted.

Each command validates a candidate immutable snapshot, yields diagnostics without mutation on failure, and returns the new revision plus inverse/compensation metadata after success.

## Existing implementation observations

| Location | Repro/result | Expected | Severity, corruption risk, minimal repair | Migration / UI impact |
|---|---|---|---|---|
| `logos-app/src/stores/documentStore.ts:171-184` | `addShape(child)` always adds ID to page roots even if `parentId` is non-null. | Route to parent insertion or reject inconsistent root. | High; duplicate ownership/orphan renderer graph. Introduce validated command boundary. | No storage migration if corrected before durable save; layer tree UI may need command adapter. |
| `documentStore.ts:226-237` | `updateShape(missingId, patch)` spreads `undefined` into an object with no valid identity fields. | Reject unknown target and leave state unchanged. | Medium; malformed map entry. | No migration; no UI redesign. |
| `documentStore.ts:240-270` | Delete parent with child: child remains dangling (method docs state nonrecursive). | Explicit subtree semantics or explicit reject. | High; orphan data and stale refs. | No schema change; layer delete action affected. |
| `documentStore.ts:273-291` | `reorderPageShapes` accepts arbitrary `newOrder`; inspect exact source before using. | Exact valid roots permutation. | High; omitted roots disappear from visible order / cross-page references possible. | No schema change; layer reorder interaction affected. |
| `logos-app/src/render-wasm/scene-projection.ts:68-76,131-200` | A8 validates a supplied page-scoped record set for duplicates, missing parents/children, backlinks, cycles, orphans and root order. It cannot prove global document integrity if caller passes an incomplete scope; code documents this. | Production caller supplies complete page partition; document store validates before persistence. | Renderer boundary protection is implemented, document authority partial. | No schema change; adapter integration only. |
| `rust/logos-types/src/shape.rs:153-162` | Shape has optional parent/frame IDs and child list but no page ID or graph validator. | Invariants attach to a typed Document/Page aggregate. | High if assumed canonical; ID tree can be ambiguous cross pages. | Requires approved contract, likely schema migration. |

## Executable evidence and gaps

- A8 has detailed pure projection and Chromium recovery tests, but they validate an adapter plan, not production Canvas writes or persisted document graphs. See `docs/architecture/HIERARCHICAL-SCENE-PROJECTION-A8.md`.
- `test/fixtures/g0-b/graph-invalid-cases.json` defines duplicate, dangling, cycle and backlink cases for the next canonical validator. It is intentionally not asserted against a test-only fake validator.
- `rust/logos-layout/tests/g0b_layout_determinism.rs` adds a Rust deterministic geometry characterization. Existing Rust layout tests cover more Flex/Grid cases.
- `rust/logos-types/tests/g0b_serialization_characterization.rs` records that a React shape is not directly a Rust DTO and that unknown fields are dropped. This is evidence against a lossless shared contract claim, not a desired long-term policy.

## Numeric and reference validation scope

The validator should reject NaN/infinity before serde encoding, bounds with invalid dimensions, singular transforms where inverse is required, invalid vector-network segment indices, invalid opacity, inconsistent min/max, dangling mode/token/component IDs and invalid prototype endpoints. Error codes should be stable, machine-readable and include object/path references without mutating the candidate. Define tolerance for geometry comparison and canonical rounding before claiming deterministic cross-architecture output.
