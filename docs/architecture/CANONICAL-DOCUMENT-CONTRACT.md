# Canonical Document Contract — P0 Findings

**Baseline:** `964676b90398f4458a72e3d41fd87e0dc91f51f9`
**Decision:** no replacement model proposed in P0. Identify owners and reconcile existing formats first.

## Executive finding

There is not yet one proven, end-to-end canonical document representation. The repository calls Rust `logos-types` canonical, but its own generated TypeScript compatibility layer says the Malli/Transit shape is still the database/CRDT wire format pending Go migration. React also has a richer editor shape type that extends that wire type. Go reads and writes legacy-shaped JSON file data and change rows. These representations are related, but the source does not show a single validated conversion boundary used by the active product route.

## Representation and ownership map

| Concern | Current representation / owner claim | Evidence and gap |
|---|---|---|
| Domain shapes | `rust/logos-types/src/shape.rs`: UUID identity, `selrect`, affine transforms, `parent_id`, ordered `shapes`, paint/effects, constraints, and component links. `rust/logos-types/src/lib.rs` describes Rust as the type source and Rust generates `logos-app/src/types/rust-generated/`. | Rust is a strong candidate canonical domain owner. It does not have the exact React `Shape` fields/variants or the stored Malli wire contract. |
| React document projection | `logos-app/src/types/shapes.ts` and Zustand `stores/documentStore.ts`: `bounds`, `children`, `parentId`, fills, vector-network geometry, and React-only component/instance extensions. | Store exposes direct mutations (`addShape`, `updateShape`, `removeShape`, reordering); no transaction/validation owner is present. `updateShape` spreads a missing shape, and `removeShape` explicitly does not recurse. |
| Generated/compatibility TypeScript | `logos-app/src/types/rust-generated/` plus `compat.ts`. | `compat.ts` says layouts/blend modes are absent from `logos-types` and that its `CanonicalShape` is the Malli wire shape persisted in the DB; it says Rust `Shape` is structurally different and the compatibility surface remains until Go/CRDT migration. Thus generated types are not equivalent to all app/wire contracts. |
| Persisted backend document | Go `internal/filedata` defines root UUID `00000000-0000-0000-0000-000000000000`, `FileVersion = 67`, ordered `pages`, `pages-index`, `objects`, Transit keyword encodings, and JSON encoding for Go writes. PostgreSQL `file.data` and `file_change` are used by file handlers. | Current Go backend is authoritative for its server writes/revisions, but supported serialized versions, full schema provenance, and conversion/round-trip policy are not fully defined. |
| Change/revision protocol | Go `internal/rebase` and `internal/changes`; Rust `logos-rebase`; historical Clojure/Transit rows can coexist. | Go `files_update.go` comments say undecodable legacy Transit+zstd rows are skipped as competing revisions. The path treats competing-load errors as non-fatal and continues without rebase. This is a compatibility/data-integrity risk; fail-closed behavior is not established. |
| Rendering ABI | `logos-app/src/render-wasm/{scene,module}.ts` converts React shapes into 104-byte base-property records and separate fill records; Rust renderer owns GPU/Skia scene state. | ABI is a rendering projection, not the saved-document model. PR #51’s 160-byte `RawFillData` record/pixel test is verified; it does not certify semantic shape round-tripping. |
| Page/document identity | Backend pages and shapes use UUID-like IDs; React initial page IDs use `nanoid`, while new shape IDs use `crypto.randomUUID()`. | Identity formats are not reconciled at the active editor boundary. Page ownership, selected-page loading, and cross-document references need contract tests. |
| Components, tokens, and interactions | Separate Zustand stores (`componentStore`, `tokenStore`, `prototypeStore`) plus Rust token registry/types. | The durable document snapshot only stores pages/order/current-page/shapes. It omits component registry, prototype interactions, and token state. Token store uses its own browser persistence key, not a document-scoped contract. |

## Candidate subsystem ownership (not an approved duplicate schema)

- **Rust:** canonical value types, geometry/layout/vector algorithms, token resolution, and rebase algorithms; publish explicit versioned serialized contracts.
- **Go/PostgreSQL:** authoritative file identity, authorization, revision/change history, accepted document bytes, and durable asset metadata. Go must validate the canonical contract rather than invent a parallel model.
- **React/Zustand:** transient UI state and a derived editor projection. Durable edits must flow through validated editor transactions, not arbitrary store mutation.
- **Renderer/WASM:** transient scene projection and binary ABI only; never document authority.
- **IndexedDB:** local journal/cache only, never server acknowledgement or authoritative revision.

## Required contract inventory before implementation

| Contract | Minimum fields/invariants to reconcile | Current owner/status |
|---|---|---|
| Document and page | document ID, schema version, ordered page IDs, page deletion/reorder semantics, active page, root-frame convention | Split between Go file data, React `Page`, and legacy data. **Blocked.** |
| Shape/scene graph | UUID format, discriminator mapping, local/page coordinates, affine transform, parent/frame IDs, ordered children, page-root membership, tombstones | Rust `Shape`, React `Shape`, and Go Malli-shaped objects differ. **Blocked.** |
| Appearance and typography | fill/stroke/effect units, color-space, text runs/font identity/variation, missing-resource fallback | Rust types, React compatibility types, legacy wire, and renderer ABI differ. **Blocked.** |
| Layout and constraints | flex/grid vocabulary, units, sizing/constraint semantics, deterministic solver version, derived versus persisted geometry | Rust layout, compatibility types, and worker subset diverge. **Blocked.** |
| Components/tokens/prototypes | durable IDs, library links, modes/overrides, interaction targets, cross-file references, delete behavior | Separate app stores; no complete persisted mapping. **Blocked.** |
| Assets | ownership, content hash, media IDs, font licensing/availability, authorization, import/export references | Go object/storage tables and shape metadata; schema provenance remains open. **Blocked.** |
| Revision/commands | versioned operation schema, base revision, idempotency key, author/session, validation outcome, inverse operation, conflict result | Go and Rust rebase sources exist; React default sync is a stub. **Blocked.** |

## Invariants to make executable

Reject or explicitly migrate: duplicate IDs; missing target/page/parent; cycles; duplicate child references; orphaned descendants; cross-file/page references; root-frame mutation; malformed geometry/transform; unsupported discriminants; invalid token/mode/component references; and stale revisions whose history cannot be decoded. Define how deletion handles an entire descendant subtree. Define whether derived caches such as points/bounds are recomputed or stored.

## Sequencing and acceptance

1. Schema owner identifies the actual deployed schema lineage and supported document versions (see `docs/r1/PKG-01-SCHEMA-PROVENANCE-FOLLOWUP.md`).
2. Domain owner publishes a field-by-field mapping: Rust domain ↔ persisted Malli/Transit ↔ React projection ↔ renderer records. Keep legacy fields and unknown extensions round-trippable.
3. Add golden serialization/deserialization and migration fixtures from historical Penpot/Logos files; compare IDs, tree order, geometry, style, components, tokens, interactions, and assets.
4. Introduce validated commands only after the mapping is approved. Test rejection and inverse/undo behavior against the same canonical fixtures.
5. Integrate server acknowledgement and recovery after G1/G2/G3 contracts are accepted; do not mark snapshots “synced.”

**P0 status:** ownership candidates documented; canonical contract and migration compatibility remain **Blocked**. No new document type or schema was introduced.
