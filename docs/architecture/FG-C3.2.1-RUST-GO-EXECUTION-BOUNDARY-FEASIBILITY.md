# FG-C3.2.1 — Rust/Go Execution Boundary Feasibility

**Decision record:** `FG-C3.2.1-CA-001`
**Evaluation baseline:** `40220ab0341ff5dff79f5b82b7dbacb2032b748c`
**Evaluation branch:** `core/g1-b9-6-memory-evidence-foundation`
**Result:** **CONDITIONALLY FEASIBLE**
**Scope:** Isolated, non-production execution study. This record does not approve Alternative B for production or transfer editing, authorization, or persistence authority to Rust.

## Result

The existing `logos-types` Rust planner can be compiled into a native executable and a WASM module from the same isolated protocol wrapper. A Go fixture runner invoked that native planner, and Node-hosted WASM produced equivalent normalized responses for all 11 deterministic fixture requests. This is executable evidence for shared semantic code across these two entrypoints.

The study does not establish a production Go/Rust service boundary, a deployable Rust service, browser behavior, production authorization, transactional revision fencing, durable idempotency, or operational suitability under load. G1-B10 remains **NO-GO**. Production authority and persistence remain unchanged.

## What was executed

The disposable spike lives under `spikes/fg-c3-2-1/`. It uses the existing `rust/logos-types` crate through an isolated JSON wrapper and has no production handlers, credentials, network calls, editor mutations, or storage writes.

```text
React client (future command client)
  │ typed intent + operation key + document ID + expected revision
  ▼
Go transaction boundary (future; current persistence authority)
  ├─ authenticate and authorize
  ├─ load authoritative snapshot and revision
  ├─ invoke approved server semantic planner
  ├─ enforce revision fence and durable idempotency
  └─ atomically persist document revision + operation outcome
           ▲
           │ candidate plan only; never client-trusted
           ▼
        Rust planner
  (shared semantic implementation; no auth or persistence)

Browser WASM ── advisory preview only; its output is untrusted by Go
```

The executable spike implements only the bottom-level invocation and comparison: Go starts a local Rust process for each request; Node loads the same Rust wrapper compiled to WASM. The diagram's Go transaction boundary is a future specification, not implemented behavior.

## Boundary alternatives

| Option | Evidence and advantages | Costs and failure/operations profile | Finding |
| --- | --- | --- | --- |
| Native Rust library called from Go | Existing Rust semantics can be linked in-process in principle; avoids per-request process startup and a network hop. | The current crate has no stable C ABI, exported C interface, or generated Go bindings. Would require an FFI layer, ABI/version policy, memory ownership rules, panic containment, cross-language build packaging, and deployment qualification. A native crash or memory fault shares the Go process failure domain. | Not demonstrated; do not select on presumed performance. |
| Isolated Rust service/process called by Go | The spike proves Go can invoke the compiled planner process locally and the shared Rust helper also builds to WASM. Process boundary can isolate crashes and allows explicit protocol-version rejection. | Current spike is a one-shot subprocess, not a service. It has startup and serialization overhead, process supervision/upgrade/rollout requirements, bounded input but no bounded output, and no concurrency/load, health, network security, or deployment tests. Service RPC would add transport, timeout, authentication, and version-skew handling. | Preferred option to investigate next; conditional only. |
| Go-native semantics (Alternative C) | Fits current Go authorization and persistence deployment boundary and avoids cross-language runtime operations. | No comparative Go implementation was built. Porting semantics would duplicate or replace the Rust candidate and needs differential conformance to avoid two authorities drifting. Simpler deployment does not by itself prove semantic equivalence. | Fallback candidate; current evidence does not justify recommending it over Rust. |

Build reproducibility is limited to this checked-in spike layout and the tested local toolchain. The script pins dependencies through its generated `Cargo.lock` and uses Cargo offline mode, but it does not establish a hermetic CI build, supported deployment image, artifact signing, or a supported version matrix. The subprocess emits sanitized fixed failure text rather than planner stderr; diagnostics and protocol versions are structured responses. Rollback, production alerting, and operational maintenance are not tested.

## Trust and transaction protocol

The intended server path is:

1. **Receive:** React submits a typed intent, operation ID, document ID, and expected revision. The browser may use WASM for preview, but Go must not accept a client-supplied candidate document as proof of authorization or semantic validity.
2. **Authenticate/authorize:** Go authenticates the caller and checks access to the specific document and operation. Denial has no planner or persistence side effect.
3. **Read:** Go loads one authoritative document snapshot and revision. The request is bound to that exact revision.
4. **Plan:** The server invokes the approved Rust semantic engine against that snapshot and typed intent. The engine is deterministic and side-effect-free; its result is still a proposal until Go validates commit preconditions.
5. **Fence and deduplicate:** In the same transaction that will commit, Go verifies the expected revision and claims the operation key under a unique constraint. For a retry, Go first looks up the scoped operation key: an identical fingerprint returns its recorded outcome; a different fingerprint returns an idempotency conflict. If another transaction committed first, the loser reloads the record and applies the same rule.
6. **Commit:** Go atomically writes the new document revision and the operation record containing the request fingerprint, authoritative outcome/revision, and required replay data. No external side effect is included in this transaction. If the persistence layer cannot atomically write both, this protocol cannot promise retry-safe effects.
7. **Respond/reconcile:** Go returns the authoritative result and revision. React reconciles its local projection from that result. A lost response is recovered by retrying the same operation key.

**Key scope:** at minimum `(tenant/workspace, document ID, operation ID)`, with caller/authorization scope enforced so one principal cannot read another principal's replay result.
**Fingerprint:** canonical encoding of document ID, operation type and payload, expected revision, and any security-relevant actor/context. Canonicalization and inclusion rules require a versioned protocol.
**Retention:** operation records must live at least as long as the documented client retry/replay window; pruning behavior must not permit an old key to execute again unexpectedly. Exact retention is a product/storage decision not qualified here.

| Failure point | Required behavior | Evidence status |
| --- | --- | --- |
| Authentication/authorization denied | Reject before planner and storage mutation; do not reveal stored operation outcomes across principals. | Specified; not implemented/tested. |
| Snapshot missing or expected revision already stale | Return not-found/conflict; no commit. Any re-plan requires a new explicit revision/intent. | Specified; not implemented/tested. |
| Planner rejects input | Return stable typed diagnostics; no commit or operation success record. | Stable deterministic rejection evidenced by cycle fixture; server transaction behavior specified only. |
| Planner timeout/crash/malformed response/version mismatch | Fail closed; no commit. Record sanitized operational diagnostic. | Unsupported protocol rejection and local invocation evidenced; timeout/crash/service mismatch not tested. |
| Revision changes after planning | Transactional revision fence fails; no candidate commit. | Specified; not implemented/tested. |
| Same operation key and same fingerprint | Return stored result/revision without applying again. | Specified; no durable store evidence. |
| Same operation key and different fingerprint | Reject as idempotency conflict. | Specified; not implemented/tested. |
| Process stops before atomic commit | No mutation; request may be retried. | Specified; not crash-injection tested. |
| Commit succeeds, response is lost | Retry resolves to the stored operation result; no second mutation. | Specified; not storage-qualified. |
| Concurrent duplicate requests | Unique key claim/transaction serialization yields one committed result; the other request replays or conflicts. | Specified; no concurrency database test. |
| Go/Rust protocol version mismatch | Reject before commit; deploy compatible versions or roll back. | Version 2 rejected in conformance fixture; rolling deployment behavior not tested. |

These requirements provide an at-most-one committed mutation per retained operation key only if the database transaction and unique constraint behave as specified. They do **not** establish general exactly-once execution, especially for external effects. G1-B10 remains **NO-GO** pending storage-backed implementation, crash/concurrency qualification, and operational evidence.

## Projection identity findings

The spike derives UUIDv5 projection keys from a document namespace and a typed address containing the scene instance ID, component-definition route (including the instance node at each route step), definition node ID, slot route/authored node IDs, and optional external source revision. This is an experiment, not a finalized persisted identity policy. Derived keys must not be used as canonical authored IDs.

| Fixture relation | Expected and observed relationship | Interpretation/limit |
| --- | --- | --- |
| Reordered surviving definition child | Same key | Sibling order is not part of the address. Fixture repeats an address; graph enumeration must still reject duplicate addresses. |
| Nested component path | Different key | Route contributes to projection identity. |
| Same definition in two scene instances | Different key | Scene instance scopes projections. |
| Definition node replaced | Different key | Stale projected node reference does not silently point at replacement. |
| External source revision changes | Different key | Revision-pinned external content changes its projection key; policy for local/unpinned sources is not settled. |
| Slot-authored node changes | Different key | Authored slot child identity contributes to the address. |
| Detach supplied IDs | Unique, valid UUIDs accepted; existing/duplicate collision rejected | This validates caller-supplied candidate IDs only. It does not allocate IDs for a real detach operation or prove global concurrent allocation safety. |

Identity allocation for detach must eventually be server-authoritative or transactionally collision-checked against the canonical authored-ID namespace. Selection, undo/redo, override migration, and external-library upgrade behavior remain open compatibility requirements.

## Conformance evidence

Run from repository root:

```sh
./spikes/fg-c3-2-1/run.sh
```

The script uses `CARGO_NET_OFFLINE=true`, runs Rust formatting and unit tests, builds the native Rust executable, runs Go tests and builds the Go fixture runner, compiles the same wrapper with `wasm-pack --target nodejs`, then compares normalized complete JSON responses from Go→native-Rust and Node-hosted WASM.

**Observed on this worktree:**

- Rust spike library unit tests: **2 passed, 0 failed**.
- Rust spike main/doc tests: **0 tests**.
- Go: **5 top-level tests, including 6 identity subtests; 10 test cases passed, 0 failed**.
- Cross-entrypoint parity: **11/11 requests equivalent** (root reorder, cycle rejection, detach validation/collision, six identity relations, unsupported protocol version).
- `wasm-pack` compiled and optimized the WASM package successfully. This is Node-hosted WASM execution, not a real browser test.
- The build emitted two existing unused-import warnings from `rust/logos-layout`; no unrelated source was changed.
- The repository-wide Rust workspace, production Go service, hosted CI, production deployment, load/concurrency, database, and real browser were not tested as part of this feasibility spike.

## Recommendation and gates

**Recommendation: CONDITIONALLY FEASIBLE.** Continue investigating a server-side Rust semantic planner with Go as the authorization, revision-fencing, idempotency, and persistence coordinator. The isolated executable evidence answers only whether the same candidate Rust code can be reached through a local native process and a Node-hosted WASM module for the fixtures above.

Before production selection, require:

1. A deployment-shaped service or library comparison, including failure, concurrency, observability, build and rollback evidence.
2. A versioned typed command/response protocol with bounded input/output and explicit diagnostic compatibility.
3. Storage-backed atomic revision + operation-ledger implementation, conflict semantics, crash recovery, and concurrent retry tests (G1-B10).
4. Actual legacy/current component documents, unknown-field preservation, lossless-or-reject round trips, and migration policy.
5. Canonical identity decisions for authored, projected, detached, selected, overridden, and externally sourced nodes.
6. A later explicit Chief Architect decision before any production authority or schema change.

No production integration, Go write-path change, schema migration, production Rust authority, PR, or merge is part of this work.
