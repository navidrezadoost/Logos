# G0-B Source-of-Truth Ownership Map

**Baseline:** `41c88ca3217e9451883afeb50d93debbc3ded1ad` (`main`).
**Conclusion:** No single enforced, persisted document authority is evidenced end to end. Rust `logos-types` is a candidate type source, not proof that active React, backend, offline, renderer, and migration paths share it.

## Ownership by lifecycle stage

| Lifecycle stage | Current source / copy | Mutators | Serializer / persistence | Confidence and loss behavior |
|---|---|---|---|---|
| Authoring state | React Zustand `documentStore` (`logos-app/src/stores/documentStore.ts:24-43`) with `Page` and map of `Shape` in `logos-app/src/types/shapes.ts`. Selection, components, prototype, tokens live in separate stores. | Canvas, Inspector, layers, AI/template stores call direct store actions; `updateShape` shallow merges arbitrary patch; `addShape` inserts root whether or not parent metadata says otherwise. | `offline/persist.ts:178-200` snapshots only pages/order/current page/shapes to IndexedDB. | Actual experimental editor state. No central validated transaction. Store checks are mostly type-level, not runtime. |
| Shared Rust values | `rust/logos-types/src/{shape,fill,stroke,shadow,blur,typography,color,token}.rs`; geometry in `rust/logos-layout`. | Constructors/helpers and consumer-local mutation. No complete `Document` aggregate. | Serde derives; generated TS is emitted by custom `generate_types.rs`, checked by Makefile workflow. | Candidate authoritative DTOs. Struct fields differ from React/file wire; unknown fields are ignored and dropped in round trip. |
| Legacy database wire | Go `internal/filedata` creates root/page envelope; file handlers store arbitrary blob in `file.data`; `file_change.changes` hold operations. Historical schema remains Transit-shaped. | Go update handler applies/rebases changes under row lock and updates data/revn. Legacy Clojure backend remains a format origin/compatibility source where files are present. | JSON bytes and keyword marker normalization in Go; older Transit+zstd rows can remain in change history. | Server owns accepted writes/revisions operationally, but Go does not establish a canonical typed domain schema. Compatibility is partial. |
| Renderer | `scene-projection.ts` creates A8 ephemeral nodes/synthetic nil root; `scene.ts` uploads to Rust renderer. `render-wasm/src/shapes.rs` owns renderer-specific shape/cache state. | Full scene sync/project calls. A8 projection currently has isolated test/browser harness; mainline architecture note says production Canvas is not yet wired to it. | No document persistence; binary WASM ABI. | Derived output only, not source of truth. Validate before WASM mutation where A8 route is used. |
| Offline state | `offline/persist.ts` snapshot and generic `PendingOp.patch` types. | Zustand subscription / sync helpers. | IndexedDB stores `snapshots` and `pending-ops`. | Local recovery/cache mechanism; snapshot omits token/component/prototype stores and has no schema version or acknowledged server revision in the shown `DocumentSnapshot`. |
| Migration input | XD converter (`logos-app/src/migration/xd/*`); Sketch converter (`.../sketch/*`). | Importer produces new React `Shape[]`, token result, warnings. | No source-format-preserving persistent envelope shown. | Conversion is lossy in identified areas; XD IDs use random UUIDs, making outputs nondeterministic. |

## Object-level status

- **Document / Page — PARTIAL / UNVERIFIED:** React Page has ordered roots; Go has `pages` and `pages-index`; Rust has neither Document nor Page. Page IDs differ in generation (initial React page uses `nanoid`; shapes use UUID). No single document envelope/version or cross-page ownership validation exists at active boundary.
- **Shape / hierarchy — DUPLICATED:** Rust uses `selrect`, `parent_id`, `frame_id`, ordered `shapes`; React uses `bounds`, `parentId`, `children`, and extra UI types. Go stores generic maps in legacy keys. A8 defines a read-only renderer projection validator for page-scoped React records, not document-wide authority.
- **Frame/Artboard — SEMANTIC GAP:** Frame is a Shape discriminator in Rust and React. XD maps Artboard to Frame and preserves name/size, but no separate artboard semantics such as responsive constraints/start point are demonstrated. Do not add a new entity before semantic requirements are mapped.
- **Appearance — MULTIPLE CONTRACTS:** Rust direct fill/stroke/effects types, React fill/effect interfaces, legacy Transit data and the 104-byte renderer base-properties ABI are separate. Rust DTO serialization has no open extension bag beyond string `plugin_data`.
- **Components/instances — SPLIT:** Rust Shape has component IDs/flags; React shapes carry metadata and componentStore keeps copied defaults, variants and overrides. No durable adapter/atomic mutation is evidenced.
- **Auto Layout — MULTIPLE COMPUTATIONS:** Rust pure Flex/Grid crates and renderer reflow code coexist. Both compute geometry, but there is no shared input normalization or exact differential suite. React worker/WASM is another boundary.
- **Selection — UI TRANSIENT:** `selectionStore` is a view of selected IDs; correctly absent from persisted snapshot, but must be captured in proposed operation context, not persisted as domain state by default.
- **Prototype interactions — TRANSIENT:** per-source map in `prototypeStore`, explicitly resets on reload. Not in offline snapshot. XD importer doesn't consume interactions archive entry.
- **Tokens — MULTIPLE ISLANDS:** schema-versioned Rust `TokenRegistry` / Rust evaluator, legacy generated `TokensLib`, persisted browser import `tokenStore`, and transient DT-C3 workflow are distinct. PR #90 is not in baseline and its proposal revision/caller permission context are experimental only.
- **Operations/undo — SPLIT:** React offline patches are generic; Go uses Penpot-style operation names/change rows and OT; Rust has a rebase crate. No common typed operation identity/precondition/inverse receipt is enforced.
- **Renderer state — DERIVED:** Rust Skia scene pool and synthetic root are not persisted. Never serialize renderer IDs, caches, pixel buffers or GPU resources into the document contract.

## Required approved consolidation seam

Before a migration, the Chief Architect should choose or explicitly defer canonical serialized document ownership and approve a field-level mapping:

`persisted legacy file envelope ↔ versioned domain DTO ↔ React editing projection ↔ renderer plan`.

That mapping must include IDs/page ownership, field defaults, unknown extension policy, explicit loss diagnostics, migration versioning, and round-trip fixtures. Rust can own portable algorithms and value types without assuming it owns authoritative server persistence. Go can remain opaque transport without claiming validation. React can remain the current editor projection without becoming a second durable authority. No schema was changed in this audit.
