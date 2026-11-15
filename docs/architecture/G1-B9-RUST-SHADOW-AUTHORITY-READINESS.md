# G1-B9 — Rust Shadow Evaluation and Authority Readiness

**Status:** Read-only shadow integration; React remains the only production mutation authority.  
**Baseline:** `ee54ec404e308b68480f04a42a0c780e570a6afd`  
**Readiness:** Rust production authority transfer is **NOT AUTHORIZED**.  
**Performance:** Shadow-stage timings are evidence only; no editor performance claim or retained-memory claim is made.

## Ownership and identity audit

| Boundary | Repository evidence | Current consequence / policy |
|---|---|---|
| Go file creation | `backend-go/internal/handler/files_create.go` uses request `id` when supplied and otherwise `newUUID()`; `files_create_internal.go` also fills an empty ID. PostgreSQL stores the file identity. | Treat the persisted server file UUID as the document identity when a document is loaded from the service. The caller-supplied creation ID path means server issuance is not universal in source; confirm the actual client call contract before making it authoritative. |
| Initial Go page | `backend-go/internal/filedata/filedata.go` calls `NewPageID()` when constructing empty file data. | New Go-created pages are UUID v4 identities. Preserve the returned identity. |
| React page creation | `logos-app/src/stores/documentStore.ts` creates the initial page and `addPage` identities with `nanoid()`. | New React-only pages are currently outside the Rust adapter's UUID contract. Do not rewrite those identities in shadow mode. A future page-creation change needs an approved issuance decision and compatibility plan. |
| Offline persistence | `logos-app/src/offline/persist.ts` keys IndexedDB snapshots and pending operations by a caller-supplied `documentId`; `WorkspacePage.tsx` supplies the file route ID. The Zustand document state itself has no document ID. | An offline snapshot inherits the caller's ID. The repository does not establish a standalone offline-document UUID issuance flow. The shadow runtime must receive an explicit, already-owned document identity; it cannot generate one. |
| Existing shape IDs | React shape creation generally uses `crypto.randomUUID()` (`documentStore.ts`, Canvas and Layers). Hydration copies the supplied document without remapping IDs. | Keep existing shape IDs through all edits. Adapter identity failure is a rejection, never an invitation to regenerate. |
| Shape duplication | `templateStore.ts` creates new UUIDs and a local source-to-copy map for a copied template tree. | Duplication is a new identity event: allocate fresh IDs once and remap only internal references. Preserve the source/template's existing IDs. Confirm any additional duplication paths before delegation. |
| XD / Sketch imports | `migration/xd/xd-shape-converter.ts` and `migration/sketch/sketch-shape-converter.ts` map source IDs to generated UUIDs in an in-memory `idMap`. That map is not a durable source-alias registry. Some unsupported source layers may be skipped with warnings. | Existing imported Logos IDs remain stable after import. Do not re-import or re-key as a side effect of shadow evaluation. Reproducible source aliases and unsupported-source preservation require a separately approved import contract. |
| Rust candidate core | G1-B8's React adapter requires a canonical UUID document/page/shape subset and one page; the Rust `Document` includes a document UUID. | A missing or incompatible identity is `ADAPTER_UNSUPPORTED`. No synthesized identity, case folding, or conversion repair is permitted. |

### Versioned ownership and compatibility policy (`logos-identity-shadow/v1`)

1. A document loaded from Go uses its existing persisted file UUID. A future server-created document must return the server-owned UUID to the editor before the editor treats it as canonical. The current caller-supplied `create-file.id` behavior must be reconciled by the Chief Architect before any authority transfer.
2. A separately created offline document may use a UUID only when the offline creation flow explicitly allocates and persists it as document identity. No such independent flow is established by the inspected repository. Until then, an offline snapshot uses the exact supplied document ID or remains ineligible.
3. Newly created pages should receive UUIDs only after the page-ID authority is approved. Existing nanoid page IDs are historical data and remain unchanged; those documents stay on React compatibility behavior until an explicit migration decision exists.
4. Import source identities are aliases, not replacement canonical IDs. The current XD/Sketch maps are conversion-local; a future lossless import path must preserve original source identity/format/schema alongside generated imported IDs if cross-import identity is required.
5. Editing preserves a shape's ID. Duplication creates fresh IDs and remaps references wholly inside the copied subtree. External references are preserved or explicitly rejected; never silently point them at another object.
6. Hydration, imports, and historical IDs are not ordinary edit operations. They remain atomic load boundaries. Shadow conversion rejects unsupported data and leaves the loaded React document untouched.

The browser harness uses fixed UUIDs as explicit test-fixture identities before mounting the editor. They are test data only and are not proposed as identities for user documents.

## Shadow integration boundary

`documentStore.applyHierarchyOperationForPage` remains the only strict React hierarchy plan/commit boundary. It now:

1. Captures the current immutable store references and plans the React operation once.
2. Commits the accepted React result once, including the existing selection reconciliation.
3. Schedules the optional comparison for a later event-loop turn, passing a detached snapshot and the already-computed React plan to the G1-B8 evaluator. It does not execute the user command a second time in React.
4. Records only operation kind, classification, stable diagnostic codes/paths and timing/memory observations. It never writes candidate output to Zustand, selection, offline persistence, or renderer APIs.

The integration is compiled only by the dedicated A10/G1-B9 smoke configuration and also requires explicit `__LOGOS_G1_B9_SHADOW_OPT_IN__ = true` plus an installed evaluator. An ordinary Vite build has no enabled shadow build constant; ordinary editor usage is off by default. Compatibility operations are labelled `LEGACY_COMPATIBILITY` and are not converted or sent to Rust. React rejects are reported before scheduling Rust.

Before a scheduled comparison runs, it verifies the post-React `pages` and `shapes` references and current page. A later edit, page switch, or disposed editor drops the comparison and records a discard in the harness; it cannot affect the document. These in-memory references are an ephemeral snapshot identity, not a durable revision number. There is no extra command queue or worker mutation engine.

## Classification and equivalence gate

| Category | Condition |
|---|---|
| `ELIGIBLE_EQUIVALENT` | Input and proposed React state both convert; Rust accepts; reverse projection succeeds; the complete canonical semantic projections match. |
| `ELIGIBLE_MISMATCH` | Both conversions and Rust evaluation succeed, but complete projected states differ. This is never an approval. |
| `RUST_REJECTED` | React planned/committed, but Rust rejects its mapped candidate. Original Rust diagnostic codes and paths are retained. |
| `REACT_REJECTED` | The established React planner rejects before commit. Rust is not invoked. |
| `ADAPTER_UNSUPPORTED` | Input or proposed output cannot be converted losslessly. Adapter issue codes and paths are retained; React remains unchanged or owns its already-accepted result. |
| `LEGACY_COMPATIBILITY` | Existing compatibility operation, including Vector Network/component/instance paths. Payload remains on the React compatibility path. |
| `SHADOW_EVALUATION_FAILED` | Evaluator missing, invalid response/projection, snapshot capture failure, or unexpected exception. The React result remains the only result. |

Successful Rust return alone never qualifies an operation. The bridge compares the whole React projection; it does not normalize IDs, fields, order, transforms, fill data, or unknown properties. Any mapped difference remains a mismatch.

## Operation and persisted-population readiness

The technically eligible subset is defined by the G1-B8 adapter contract plus observed Chromium evidence. It is limited to a **single-page** document with canonical UUID document/page/shape IDs, valid symmetric hierarchy, supported `rect`/`frame`/`group` records and fields, supported solid fills, and the verified axis-aligned/translation coordinate contract.

Within that subset, the Rust candidate operations are root Rectangle/Frame creation, child Rectangle insertion, rename, root/sibling reorder, subtree deletion, and same-page world-position-preserving reparent. Locked objects, cycles, malformed membership, invalid order, ID conflicts, and incompatible page ownership remain React rejection cases; the Rust result is diagnostic evidence only. Eligibility is per actual document and operation, not a blanket promise for all files.

The following populations remain unsupported or unproven: existing nanoid-page documents; multiple pages; non-UUID or mixed-case IDs; unknown document/page/shape fields; Vector Networks; components/instances; token or plugin payloads; non-solid/multiple fills; imported transform/pivot semantics beyond the verified contract; and any source content currently skipped or lossy in import. All are preserved on their current React/legacy path. The current store does not have an authoritative document ID, so the harness's explicit fixture UUID does not prove production eligibility.

## Browser and failure evidence

The existing A10 Chromium workflow uses production Canvas, Layers and Inspector components with the actual `logos-types` WASM candidate. Required evidence on the PR head is:

- Real Canvas creation and click selection; Layers insertion, rename, root reorder, reparent, and subtree delete.
- At least one equivalent result for each exercised strict operation kind, plus React rejection and explicit legacy compatibility classification.
- Injected unavailable evaluator, Rust rejection, malformed response, thrown exception and semantic mismatch. Each case must retain the single React commit and must not commit the Rust candidate.
- Rapid repeated edits and page switch during the queued comparison; stale results are discarded. Editor unmount invalidates its pending comparison.
- Existing A8/A9/A10 browser assertions, including pixel and renderer-recovery checks, remain active.

The harness samples the current JS heap and WASM linear-memory reservation and records the G1-B8 stage timings. A10's existing interaction-to-ready-frame observations remain separate. These are disposable Chromium observations, not representative performance results. No leak or retained-memory conclusion follows from a point sample or linear-memory reservation.

The exact final-head CI run and artifact values must be attached to the PR before acceptance. Until those artifacts are available, test/evidence status is **PENDING CI**.

## Authority decision matrix

| Gate | Current evidence | Readiness |
|---|---|---|
| Identity ownership | Go file/page UUID generation exists; React initial/additional pages use nanoid; Zustand lacks document ID; import aliases are ephemeral. | **BLOCKED** pending canonical issuance and legacy policy. |
| Lossless conversion | Restricted one-page adapter rejects unsupported fields/content. | **NARROW SUBSET ONLY**; no general document adoption. |
| Operation semantics | G1-B8 fixtures plus real-editor shadow cases compare proposals; React remains commit owner. | **PENDING final-head Chromium CI**. |
| Atomicity | One React planner/commit; Rust runs read-only after commit. Candidate never reaches store or renderer. | **SHADOW CONTRACT ONLY**; production authority unchanged. |
| Renderer | A8/A9 consume committed React state; Rust shadow result is never uploaded. | **React-owned**. |
| Observability | Sanitized per-operation categories, codes/paths, stage timings and harness memory samples. | **Test evidence only**; production telemetry not authorized or implemented. |
| Performance | CI timings are to be reported per stage and separated from interaction-to-frame measurements. | **NOT A PERFORMANCE RESULT** until real final-head run; no invented numbers. |
| Rollback | Build/runtime gate off by default; React path is unchanged. | **Disable shadow flag/runtime**; no schema or persisted data rollback required. |
| Controlled pilot | Requires identity decision, broader lossless corpus, policy/diagnostic alignment, production-like latency and retention evidence, and rollback approval. | **NOT AUTHORIZED**. |

## Next evidence before any pilot proposal

1. Chief Architect decision on canonical document ID issuance and how `create-file.id`, offline creation, and React page UUID issuance align.
2. Representative sanitized persisted-document corpus with population counts and adapter classification, including historical nanoid pages and XD/Sketch imports.
3. Explicit compatibility extensions or lossless preservation contracts for components, instances, Vector Networks, fills, tokens, plugins and unknown fields.
4. Final-head Chromium/CI artifact evidence for all operation and failure cases, with exact commit SHA and timings; repeatability across supported browsers.
5. Operation policy reconciliation for locks, diagnostics, page ownership and imported coordinates.
6. Controlled memory-retention study if memory growth or leak safety is to be claimed.
7. Separate Chief Architect authorization for a bounded, observable Rust-authoritative pilot and its rollback plan.

No production authority transfer, migration, persistence change, renderer change, or deployment is authorized by this report.
