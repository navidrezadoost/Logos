# G1-B2 — Candidate Contract Hardening and Compatibility

**Status:** Candidate Rust contract only. No Go persistence, saved-file, production React, or Canvas path uses it.
**Baseline:** Chief Architect accepted PR #93 as an additive candidate core at `c1f8c155b326105459869cd8fd58e349f30f0636`.

## Architecture inventory

| Boundary | Current representation and ownership observed in this checkout | Compatibility constraint |
|---|---|---|
| React editor state | `logos-app/src/stores/documentStore.ts`: pages keyed by ID plus ordered `pageOrder`, `currentPageId`, shapes keyed by ID. `logos-app/src/types/shapes.ts`: shape `bounds`, matrix object, `parentId`, ordered `children`, direct fills, visibility/lock flags, text, vector-network, component/instance metadata, auto-layout. | This is a mutable editor projection with transient selection/current page and side stores. It is not the candidate's canonical model and has no generic unknown-field bag. |
| Rust domain DTO | `rust/logos-types::Shape`: UUID, `ShapeType`, `selrect`, transform and inverse, optional parent/frame, ordered children, direct fills/strokes/effects, constraints, component fields and plugin data. `document.rs` adds ordered pages, explicit shape `page-id`, extensions, graph validation and immutable in-memory operations. | Rust DTO serde alone may default or drop fields it does not know. Preservation is guaranteed only at candidate `ShapeRecord` extensions or the opaque `CompatibilityEnvelope`. |
| Go persistence | `backend-go/internal/filedata` handles legacy `file.data`; observed v67 uses `pages`, `pages-index`, Transit keyword markers and an object map with a NIL-UUID synthetic root. | Go snapshot/restore and persistence remain separate. Candidate JSON is not the existing saved-file format; no Go schema or write path changes here. |
| XD import | `logos-app/src/migration/xd/xd-importer.ts` unpacks `manifest.json` and `graphicContent.agx`, then delegates to token and shape converters. Current conversion creates generated IDs in shape conversion and exports tokens separately. | XD source identity, responsive constraints, and feature preservation need source archives and a deterministic adapter. This milestone does not alter the importer. |
| Rendering | React scene/renderer projection consumes editor state; Rust/WASM has renderer geometry and scene interfaces. | The document candidate does not own scene caches, selection, viewport, or renderer lifecycle. No Canvas integration is claimed. |

## Normative candidate semantics

| Topic | Candidate rule | Evidence / consequence |
|---|---|---|
| Containers | `ShapeType::is_container()` is the shared rule: `Frame`, `Group`, `Bool`, and `Component` own ordered children. `Shape::is_container()` delegates to it. | **Compatibility change:** existing helper behavior now treats `Component` as a container. Integration must check current component-definition tree assumptions before promotion. |
| Parent and frame | `parent-id` is the immediate structural parent. `frame-id` is the nearest ancestor whose type is `Frame`; a Frame itself inherits its outer Frame (if any), not itself. Page roots have no parent and no enclosing frame unless structurally nested elsewhere. | A nested Group under a Frame remains structurally parented to Group but points to that Frame for frame membership. |
| Geometry | `x/y` may be negative; width/height may be zero but not negative. All bounds and rotation values must be finite. Absolute coordinates, extents, and transform coefficients are bounded to `1e12`; bounds whose `x+width` or `y+height` exceed that bound fail. | This is a candidate safety envelope, not a claim that all legacy files obey it. Extreme real documents must be sampled before adoption. |
| Transform | All six affine coefficients and inverse coefficients must be finite and within `1e12`; transform must be invertible under `logos-layout::Matrix::inverse`; both multiplication orders must be identity within absolute tolerance `1e-8`. `UpdateShape` computes the inverse. | Documents with stale inverses or singular matrices reject. Existing DTO defaults do not prove historical values satisfy this rule. |
| Opacity | Finite and in `[0,1]`. | Fail closed on NaN/infinity/out-of-range values. |
| Cross-page reparent | Reparent moves the entire descendant closure, changes page ownership on every moved record, and recalculates nearest frame ancestry. Local coordinates are preserved; world-position adjustment is deliberately not guessed. | A move that leaves invalid external component references fails validation without changing the input. |
| Sibling insertion | Index is the insertion slot in the destination list after detaching the moved object. Valid slots are `0..=len_after_detach`. | Same-parent reorder has deterministic post-detach semantics; invalid indices reject with unchanged input. |
| Reorder | `expected_ids` must equal the current ordered list; `ordered_ids` must be a duplicate-free permutation. Arrays remain the order authority. | Prevents stale sibling-list replacement; it is not a concurrency/revision protocol. |
| Delete | Delete removes the requested shape and its child closure, then removes its one parent/root membership. Any remaining local references (for example, a component instance pointing at a deleted component) invalidate the clone and reject the operation. | No dependent object is silently deleted or retargeted; caller receives diagnostics and the source stays unchanged. |
| Import IDs | UUIDv5 name derivation over version-tagged, length-prefixed source format and source ID under a caller-owned namespace. Batch derivation rejects duplicate derived IDs. | Stable identity suggestion, not authorization or proof of source uniqueness. Source duplicate identity requires reject or stable source-path disambiguation before import. |
| Diagnostics | Stable code/path/IDs sorted by code, path, object ID, related ID; input values are not embedded. | Repeated validation yields identical diagnostics and avoids leaking source text into logs. |

## Compatibility adapter matrix

Classification describes an adapter that does not yet exist. `PRESERVED_AS_EXTENSION` means raw source data is retained in a candidate extension or `CompatibilityEnvelope`, not that current production code maps it.

| Field/feature | React document state | Rust Shape DTO | Legacy Go file-data | Candidate Document | XD importer |
|---|---|---|---|---|---|
| Document identity | `UNKNOWN` (document store shown has no durable envelope ID) | `UNSUPPORTED_REJECTED` | `REQUIRES_TRANSFORMATION` (file UUID) | `MAPPED_LOSSLESSLY` as UUID | `UNKNOWN` (sample archive absent) |
| Page identity and order | `MAPPED_LOSSLESSLY` from `pages` + `pageOrder` | `UNSUPPORTED_REJECTED` (Shape has no pages) | `REQUIRES_TRANSFORMATION` (`pages` vector plus `pages-index`) | `MAPPED_LOSSLESSLY` (`pages` order) | `REQUIRES_TRANSFORMATION` (Artboard order) |
| Root membership | `MAPPED_LOSSLESSLY` from `rootShapeIds` | `REQUIRES_TRANSFORMATION` (parent `None`, ordered child IDs) | `REQUIRES_TRANSFORMATION` (remove/interpret NIL root) | `MAPPED_LOSSLESSLY` (`root-shape-ids`) | `REQUIRES_TRANSFORMATION` (Artboard nodes) |
| Object identity | `MAPPED_LOSSLESSLY` for existing UUID strings | `MAPPED_LOSSLESSLY` as UUID | `MAPPED_LOSSLESSLY` after Transit/key normalization | `MAPPED_LOSSLESSLY` as UUID | `UNKNOWN`; importer currently generates IDs |
| Parent/children/order | `MAPPED_LOSSLESSLY` (`parentId`, ordered `children`) | `MAPPED_LOSSLESSLY` (`parent-id`, `shapes`) | `REQUIRES_TRANSFORMATION` (object-map and synthetic root) | `MAPPED_LOSSLESSLY` as graph plus ordered vectors | `REQUIRES_TRANSFORMATION` from nested XD tree |
| Page ownership | `REQUIRES_TRANSFORMATION` (shape map plus page roots) | `UNSUPPORTED_REJECTED` as standalone Shape | `REQUIRES_TRANSFORMATION` from page's object map | `MAPPED_LOSSLESSLY` explicit `page-id` | `REQUIRES_TRANSFORMATION` from artboard assignment |
| Bounds / transforms | `REQUIRES_TRANSFORMATION` (`bounds`, object matrix) | `REQUIRES_TRANSFORMATION` to `selrect`, matrix + inverse | `REQUIRES_TRANSFORMATION` from duplicated legacy fields / Transit | `MAPPED_LOSSLESSLY` under candidate typed values | `REQUIRES_TRANSFORMATION`; source units/features need fixtures |
| Direct solid fills | `REQUIRES_TRANSFORMATION` (`color`/opacity object) | `REQUIRES_TRANSFORMATION` to Rust `Fill` DTO | `REQUIRES_TRANSFORMATION` (`fill-color`, opacity, keyword forms) | `MAPPED_LOSSLESSLY` once in typed `fills` | `REQUIRES_TRANSFORMATION` |
| Gradients | `MAPPED_LOSSLESSLY` in React shape | `MAPPED_LOSSLESSLY` where Rust Fill supports same variants | `UNKNOWN` for representative persisted corpus | `MAPPED_LOSSLESSLY` in typed `fills` | `UNKNOWN` (no real archive fixture) |
| Strokes and effects | `UNKNOWN` in current React `Shape` declaration (separate renderer paths may exist) | `MAPPED_LOSSLESSLY` by `strokes`, `shadow`, `blur` DTOs | `UNKNOWN` across saved-file corpus | `MAPPED_LOSSLESSLY` if represented by Rust DTO | `UNKNOWN` |
| Text content and typography | `MAPPED_LOSSLESSLY` for declared text/font properties | `REQUIRES_TRANSFORMATION` (separate typography type; not all fields represented on Shape) | `UNKNOWN` | `PRESERVED_AS_EXTENSION` until complete typed mapping | `UNKNOWN` |
| Vector networks | `MAPPED_LOSSLESSLY` for declared anchor/segment/region fields | `UNSUPPORTED_REJECTED` (no equivalent shape payload in current DTO) | `UNKNOWN` | `PRESERVED_AS_EXTENSION` or reject before promotion | `UNKNOWN` |
| Components / instances | `REQUIRES_TRANSFORMATION` (`componentMeta`, `instanceMeta`, instance shell) | `REQUIRES_TRANSFORMATION` (Component definition and component references) | `UNKNOWN` for representative corpus | `MAPPED_LOSSLESSLY` for locally resolvable references; external refs retained, not resolved | `UNKNOWN` pending archive fixture |
| Layout and constraints | `MAPPED_LOSSLESSLY` for declared auto-layout fields | `REQUIRES_TRANSFORMATION` (constraints exist; React flex model differs) | `UNKNOWN`; historical mock advertises Transit flex data only | `PRESERVED_AS_EXTENSION` until converter semantics are proven | `UNKNOWN`; converter evidence and archives absent |
| Tokens / variables | `PRESERVED_AS_EXTENSION` outside shown document state; token store is separate | `REQUIRES_TRANSFORMATION` to token types/registry, not Shape | `UNKNOWN` | `PRESERVED_AS_EXTENSION` in this candidate milestone | `REQUIRES_TRANSFORMATION`; XD tokens returned separately |
| Prototyping / interactions | `UNKNOWN` in inspected document-store/Shape definitions | `UNSUPPORTED_REJECTED` | `UNKNOWN` | `PRESERVED_AS_EXTENSION` or reject until typed model exists | `UNKNOWN` |
| Visibility / lock / editor-only state | `MAPPED_LOSSLESSLY` for shape flags; selection/current page are transient | `MAPPED_LOSSLESSLY` for hidden; no lock equivalent observed | `UNKNOWN` | `PRESERVED_AS_EXTENSION` if not typed; transient UI state excluded from durable document | `UNKNOWN` |
| Plugin / arbitrary extensions | `UNKNOWN`; current React Shape has no open extension bag | `REQUIRES_TRANSFORMATION` (plugin data is string map) | `REQUIRES_TRANSFORMATION` through source envelope | `PRESERVED_AS_EXTENSION` with collision checks | `PRESERVED_AS_EXTENSION` only while raw source is retained |
| Shape type and name | `MAPPED_LOSSLESSLY` only for shared enum values / exact name | `MAPPED_LOSSLESSLY` | `REQUIRES_TRANSFORMATION` from Transit keyword/object row | `MAPPED_LOSSLESSLY` for candidate `type` + `name` | `REQUIRES_TRANSFORMATION`; XD node types must map through explicit table |
| Rotation and transforms | `MAPPED_LOSSLESSLY` for six matrix numbers and degrees if units match | `MAPPED_LOSSLESSLY` for `transform`, derived inverse, and degree rotation | `UNKNOWN` until an actual persisted shape record is available | `MAPPED_LOSSLESSLY` under stated numeric rules | `UNKNOWN` without real archive evidence |
| Opacity / hidden / locked | `MAPPED_LOSSLESSLY` for opacity, hidden, locked | `MAPPED_LOSSLESSLY` for opacity and hidden; locked is not represented | `UNKNOWN` | `PRESERVED_AS_EXTENSION` for lock until a typed field exists; opacity/hidden typed | `UNKNOWN` |
| Boolean-operation metadata | `UNKNOWN` in inspected React shape declaration | `MAPPED_LOSSLESSLY` where `bool-type` is present | `UNKNOWN` | `MAPPED_LOSSLESSLY` in Rust `bool-type` | `UNKNOWN` |
| Resize constraints | `UNKNOWN` in React `Shape` declaration | `MAPPED_LOSSLESSLY` for `constraints-h/v`, fixed-scroll | `UNKNOWN` in available fixture records | `MAPPED_LOSSLESSLY` in Rust DTO; React layout semantics still separate | `UNKNOWN` |

No adapter may report complete conversion based on this table. A field classified `UNKNOWN` blocks any lossless compatibility claim until a representative fixture and executable assertion exist.

### Core field transforms required of a future adapter

| React source | Rust DTO / candidate target | Rule and loss condition |
|---|---|---|
| `Page.id`, `Page.name`, `Page.rootShapeIds[]`, `pageOrder[]` | `Page.id`, `Page.name`, `Page.rootShapeIds[]`, ordered `Document.pages[]` | Carry page identity and both page/root order exactly. Shape-to-page ownership must be inferred from membership only after confirming each shape appears once. |
| `Shape.id`, `type`, `name` | `Shape.id`, `type`, `name` | Parse UUID and map only declared common variants. Unknown/React-only variant rejects the conversion unless the entire source shape is retained as an extension and excluded from editable projection. |
| `Shape.bounds{x,y,width,height}` | `Shape.selrect{x,y,width,height}` | Values are copied only if units and coordinate-space semantics match. Candidate requires nonnegative width/height and finite bounded numbers. |
| `Shape.transform{a,b,c,d,e,f}` | `Shape.transform: Matrix`; candidate `transform` | Copy six coefficients. Rust inverse is computed and stored; React rotation remains separately mapped only after determining whether it duplicates matrix rotation. |
| `Shape.parentId`, ordered `Shape.children[]` | `parent-id`, ordered `shapes[]` | Preserve immediate parent and child order; assert backlinks agree. A malformed pair rejects; adapter must not choose a side silently. |
| `Shape.fills[]` | Rust `fills[]` | Map each supported solid/gradient variant and opacity/stops exactly. Unsupported gradient encoding rejects or retains the complete source fill in extension data before excluding it from rendering. |
| React `currentPageId`, selection, hover, viewport, renderer caches | No durable candidate field | Keep as transient UI/runtime state outside the document. These values are not silently discarded from an attempted durable snapshot; the adapter input schema explicitly separates them from document data. |
| Go file UUID, `pages`, `pages-index`, object map, file version | Candidate UUID, ordered pages/roots/flat records/schema version | Decode Transit keywords and NIL renderer root only in a versioned Go-specific adapter. Retain original file-data bytes/envelope until output acceptance. No Go code is changed here. |
| XD `manifest.json`, `graphicContent.agx`, artboard/node IDs and tree | Candidate metadata/pages/shapes/extensions | Preserve original archive and source node IDs. Generated/random IDs are not stable import identities; deterministic UUIDv5 suggestions require stable source identity plus collision rejection. |

## Fixture corpus and historical evidence

The repository currently supplies:

* `rust/logos-types/tests/fixtures/g1-b1/`: synthetic React projection, synthetic Go v67 file-data envelope, synthetic XD Artboard hierarchy, Rust candidate document with extension fields, valid nested Component/Group, and malformed graph. These support candidate decode/serialize and rejection tests, not cross-runtime conversion.
* `test/fixtures/p0-document-compat/`: a synthetic four-boundary rectangle vector plus the byte-preserved legacy render-WASM Playwright mock response. Its SHA-256 is `8af0a6ced3bab8f4fc4b5d738d457477591ad1f3291710bd1f7e4596eed20b12`; it is not a user-saved `.penpot` archive and Go deliberately retains its unsupported Transit envelope opaquely.

No independently verified user-saved historical archive, multiple persisted schema versions, real XD archive, or real component-rich document fixture was present. Consequently historical file compatibility remains **UNVERIFIED**. No fixtures are fabricated or re-labelled as real.

## Conformance and WASM boundary

Native executable tests cover candidate decode/serialize against the golden candidate JSON, field and identity assertions, unknown extensions, deterministic diagnostics, graph ownership, failed-operation immutability, cross-page reparent closure, same-parent insertion indices, subtree deletion/reference rejection, numeric boundaries, stable ID derivation/collision detection, and malformed/unsupported payload retention. The React, Go and XD examples are still opaque envelopes; there is no converter whose round-trip could truthfully be tested.

The same `logos-types::document` Rust source compiles for `wasm32-unknown-unknown` with the `wasm` feature. This milestone's automated evidence is native execution plus WASM target compilation; no WASM runtime test harness is installed in the repository, so runtime native/WASM result parity remains **NOT VERIFIED**. Target compilation must not be described as runtime parity.

## Benchmark protocol

`rust/logos-types/examples/document_core_bench.rs` emits JSONL with SHA-256 of the deterministic `Debug` representation of each generated candidate fixture, object count, p50/p95 latency across 31 post-warmup samples, allocation count and requested bytes from a counting allocator. Cases cover valid/malformed flat graphs of 100, 1,000 and 10,000 shapes; validation of nested graphs at 512 and 5,000 levels; and repeated create/delete and reparent cycles for the three flat sizes. Run in release mode with:

```bash
rustc -Vv
grep -m1 'model name' /proc/cpuinfo
cargo run --manifest-path rust/Cargo.toml -p logos-types --release --example document_core_bench
```

Record the exact command output, CPU model, OS/kernel, target triple, rustc output, Cargo profile, fixture hashes, and commit SHA in `G1-B2-BENCHMARK-RESULTS.md`. Allocations are per benchmark closure (two operations for a cycle); they measure requested allocations, not retained heap. No memory-leak claim follows. This document intentionally contains no fabricated benchmark numbers.

## Minimal future OpenCode contract

The proposed editor adapter is a typed boundary, not another store:

```ts
type CandidateRequest = {
  schemaVersion: 1;
  document: CandidateDocument;
  operation:
    | { type: "create-shape"; record: ShapeRecord; parentId: UUID | null; index: number }
    | { type: "delete-subtree"; shapeId: UUID; expectedParentId: UUID | null }
    | { type: "reparent-shape"; shapeId: UUID; expectedParentId: UUID | null;
        newParentId: UUID | null; newPageId: UUID; index: number }
    | { type: "reorder-children"; pageId: UUID; parentId: UUID | null;
        expectedIds: UUID[]; orderedIds: UUID[] };
};
type CandidateResult =
  | { ok: true; document: CandidateDocument; rendererProjection: RendererProjection }
  | { ok: false; diagnostics: ValidationDiagnostic[] };
```

Flow: snapshot React state into a versioned request → Rust decode/validate/apply → return the full candidate plus a derived renderer projection → React updates its existing store only after acceptance. Selection, viewport, hover, and renderer caches remain transient. Request IDs, server revisions, authentication, authorization, persistence, and collaboration are not defined by this in-memory contract and must be added at an approved Go/Rust integration boundary; preconditions here are not concurrency control.

## Promotion prerequisites

Before a production adapter or migration is approved: obtain owner-authorized representative saved files across versions and actual XD archives; build privacy-safe, provenance-documented fixtures; implement source-specific decode/normalize/encode adapters with loss reports and byte-preserving rejection; prove imported identities against source identity rules; exercise text, gradients, components, layout, tokens, prototypes, vectors, and extensions; run WASM runtime parity; record benchmark output on named hardware and commit; add renderer integration tests; and separately close Go snapshot/restore correctness. Until those steps pass, the Rust document remains an additive candidate core.
