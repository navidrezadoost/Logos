# G1-B8 — Rust/React Operation Conformance

**Status:** Read-only candidate evaluation; React remains the only live editor commit owner.
**Baseline:** A10.1 merge `78e2964e95baf0987774089891b282a2a34b9e7e`.
**Fixture:** `logos-app/src/document-core/fixtures/g1-b8-operation-conformance-v1.json`.

## Authority and evaluation boundary

`evaluateHierarchyOperationConformance` first asks the existing React `planHierarchyOperation` for an immutable proposal. It then converts both the input and proposed React state through the existing G1-B3 adapter, maps the proposal to a typed Rust `DocumentOperation`, and evaluates that operation through the Rust/WASM ABI. The adapter projects the candidate back through the existing reverse mapping and compares the complete document/page/shape semantic projection. It reports a mismatch; it never normalizes one result to make the comparison pass.

The bridge does not call a Zustand setter, selection action, persistence action, or renderer upload. It evaluates one proposal once and returns both candidates. A production caller cannot treat the Rust candidate as committed state. Unsupported conversion stops before Rust evaluation, while Rust rejection returns typed validation diagnostics. There is no fallback after a mapped Rust operation is rejected.

## Operation correspondence matrix

| React path | Classification | Rust candidate operation | Qualification |
|---|---|---|---|
| `create-root` Rectangle / Frame | **ADAPTABLE** | `CreateShape` at root index 0 | Stable lower-case UUID, empty membership, A9-supported properties, and exact one-page adapter conversion required. |
| `insert-child` Rectangle | **ADAPTABLE** | `CreateShape` at the resulting child index | Shape and parent must be on the one mapped page. Parent-local bounds are derived from React page-space bounds only where the adapter has verified the translation-only coordinate contract. |
| `rename` | **ADAPTABLE** | `UpdateShape` name-only | React trimming, length/control-character checks, and lock checks run first. Rust receives the normalized name. |
| `reorder` root or sibling | **ADAPTABLE** | `ReorderChildren` with expected and proposed full order plus `mutationId` | Full membership precondition is checked in Rust. The moved child is identified for React lock policy; root-list replacement checks every root. |
| `reorder-root-list` | **ADAPTABLE** | `ReorderChildren` with `parentId: null` | Complete root membership and order are supplied; every root is checked against the React lock marker. |
| `delete` subtree | **ADAPTABLE** | `DeleteSubtree` | Requires same-page ownership, known subtree, supported conversion, and no locked target/ancestor/descendant or unsupported dependent relationship. Rust candidate validation may add stricter reference checks. |
| `reparent` preserving world position | **ADAPTABLE** | `Sequence(ReparentShape, UpdateShape)` | React page-space bounds are explicitly converted to the new parent-local coordinates after Rust's local-coordinate-preserving move. Same page, translation-only ancestry, supported graph, and A9 clipping constraints are required. Cross-page moves are excluded. |
| Structural cycles, invalid order, ID conflict, lock rejection | **EXACT semantics; mapped diagnostics** | Rust operation validation | Both sides reject without modifying their input. Diagnostic vocabulary/path differs; see mapping below. |
| `compat-add-shape(s)`, vector creation/replacement, component promotion, instance creation, compatibility deletion | **LEGACY** | None | Remain on the A10.1 compatibility planner. Renderer-limited records are retained there and are not coerced into Rust-supported shapes. |
| `updateShape` / `batchUpdate` value patches | **UNSUPPORTED for this bridge** | `UpdateShape` is narrower | These APIs include nonstructural fields outside this operation conformance contract; no generic patch delegation is exposed. |
| Page creation/switch and hydration/import | **UNSUPPORTED here** | No mapping | Hydration retains its own atomic load boundary. Multi-page documents, non-UUID/nanoid page IDs, and caller-ambiguous page ownership are rejected by adapter policy. |

Operation names alone are not evidence of equivalence. The adapter gate requires a one-page UUID document with lower-case canonical UUID strings, supported `rect`/`frame`/`group` records, supported fields/fills, validated graph membership, and coordinate transforms already qualified by A9/G1-B4/G1-B5. No ID is generated, case-folded, or migrated.

## Diagnostic mapping

The mapping is for comparison and reporting; neither side's original diagnostic is rewritten in the returned evidence.

| React diagnostic | Rust diagnostic | Meaning |
|---|---|---|
| `LOGOS_OPERATION_CYCLE` | `CYCLE` | Reparent would create an ancestor cycle. |
| `LOGOS_OPERATION_ROOT_ORDER_MEMBERSHIP_MISMATCH` | `INVALID_SIBLING_ORDER` | Proposed order is not an exact permutation of current membership. |
| `LOGOS_OPERATION_ID_CONFLICT` | `OPERATION_PRECONDITION_FAILED` | New ID already exists. |
| `LOGOS_OPERATION_LOCKED_SHAPE` | `OPERATION_PRECONDITION_FAILED` at `/operation/locked` | React `react-locked` compatibility marker blocks mutation. |
| `LOGOS_OPERATION_PAGE_OWNERSHIP_UNKNOWN` / `...AMBIGUOUS` | `MISSING_PAGE`, `MISSING_SHAPE`, or adapter rejection | Rust carries explicit page ownership; React page selection is inferred from roots. Ambiguous ownership is never guessed. |
| `LOGOS_SCENE_UNSUPPORTED_SHAPE` / `LOGOS_SCENE_UNSUPPORTED_CAPABILITY` | Adapter `UNSUPPORTED_SHAPE` / `UNSUPPORTED_PROPERTY` or candidate validation diagnostic | Unsupported data is rejected before candidate evaluation and remains unchanged in React. |

Rust diagnostics include stable codes and JSON-pointer-like paths. The native/WASM vectors assert exact Rust code/path/IDs; browser vectors assert the exact React or adapter diagnostic at the boundary that rejected the proposal.

## Losslessness and extensions

The current React adapter supports only its explicit one-page field mapping. It rejects unrecognized document/page/shape properties, unsupported fills and shape kinds, multi-page input, non-canonical identities, and unresolved coordinates before Rust evaluation. That rejection leaves the input React state untouched; it does not claim the extension was converted.

The Rust document core itself preserves unknown top-level, page, and shape-record fields in flattened extension maps across typed operations. Native operation vectors and a WASM ABI test exercise this preservation. This Rust preservation is not enough to delegate a React edit: the React-to-Rust-to-React adapter must also prove a lossless mapping for that field first.

The `react-locked` marker is an adapter compatibility extension, not a promotion of lock into the canonical Rust schema. Candidate operation evaluation honors it for operation-level lock checks so it cannot approve a mutation rejected by the established React policy. The field remains an explicit schema-ownership decision for a future milestone.

## Shared fixtures and test boundaries

The shared fixture contains paired React and Rust candidate baselines plus case names and classifications. Native Rust tests call the operation-vector function for create-root rectangle/frame, child insertion, rename, root/sibling reorder, subtree deletion, translation-preserving reparent sequence, cycle, invalid membership, duplicate ID, locked mutation, repeated execution, input immutability, and extension preservation. The `wasm32` test target separately exercises rename, extension preservation, deterministic cycle rejection, and input immutability. The Chromium adapter harness consumes the same fixture, invokes the production React planner and actual WASM `evaluate_document_operation_json` for every eligible proposal, and compares every field of successful projected states.

Negative cases cover React planner rejection of unsupported Vector Network geometry, plus adapter rejection of cross-page documents, unknown fields, nanoid page identity, and noncanonical UUID casing. The browser harness asserts the stable rejection phase/code, unchanged Zustand document and selection, and exactly one WASM evaluation for each eligible case. It also plans a legacy Vector Network compatibility add and verifies the full geometry payload remains unchanged with the renderer limitation attached; this does not call Rust or commit the proposal.

A8/A9 rendering remains driven only by committed React state. Because this milestone never commits a candidate, renderer synchronization is intentionally not triggered; the A8/A9/A10 workflows remain the independent evidence that committed state continues through the renderer path. This milestone does not claim Rust-owned mutation or production renderer integration.

## Performance observations and limits

The browser conformance harness records React planning, React-to-Rust candidate conversion, the Rust/WASM operation boundary (decode, validation, application, serialization and JS response parse), Rust-to-React projection, and total elapsed time separately for eligible operations. These are small deterministic conformance fixtures, not representative editor performance claims. Renderer synchronization is `NOT RUN` because no candidate commits; it must be measured in a later approved production-delegation experiment.

The existing native candidate-core benchmark reports validation and operation p50/p95, requested allocation counts/bytes, and explicitly reports `retained_memory_measured: false`. It is separate from browser stage timings. Neither linear-memory reservation nor requested allocations establish retained-memory safety.

## Blockers to Rust production authority

- The React adapter remains one-page and requires lower-case UUID document/page/shape identities; current nanoid pages remain unsupported and unchanged.
- React has no persistent document ID today; the candidate document identity must be caller-supplied.
- Components, instances, tokens, Vector Networks, arbitrary extensions, non-solid/multiple fills, and other legacy shape properties do not have a proven lossless React adapter mapping.
- Rotated/scaled/skewed ancestry and imported XD/Sketch coordinate/pivot semantics remain outside the verified subset.
- Rust and React have different operation APIs, preconditions, lock representation, and diagnostic vocabularies. The bridge maps a narrow subset; it does not make Rust the authority.
- No renderer synchronization, production persistence, multi-page operation ownership, or production commit has been delegated.

G1-B9 or any production-authority transfer requires a separate Chief Architect decision after this conformance evidence is reviewed.
