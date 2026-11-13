# DT-C1 — Proposed executable token and agent contract v0.1

**Status:** Proposed contract; not approved for production persistence
**Purpose:** Contract schemas, reference semantics, and conformance vectors for
Chief Architect review
**Schema:** [`contracts/design-token/v0.1/schema.json`](../../contracts/design-token/v0.1/schema.json)
**Fixtures:** [`contracts/design-token/v0.1/fixtures/golden.json`](../../contracts/design-token/v0.1/fixtures/golden.json)
**Tests:** [`test/design-token-contract/`](../../test/design-token-contract/)

The CTO has provisionally accepted Option C as the contract direction. This
document turns that direction into an executable proposal. Canonical document
ownership, the production evaluator, and persistence/runtime decisions remain
pending Chief Architect approval. A JSON Schema validates message shape;
golden vectors exercise proposed semantics through a test-only reference
oracle. Passing these tests
means the proposal and its oracle agree. It does not mean that an application
runtime, canonical document model, or persistence format has been approved or
implemented.

Every fixture is explicitly marked `PROPOSED`. Where choices remain open, the
fixtures preserve candidate semantics for architecture review; they are not
normative pass specifications. The tests report their proposed status rather
than presenting them as accepted production conformance.

## 1. Contract package

The schema defines versioned shapes for:

- `TokenSnapshot`, token definitions/references, typed values, collections,
  modes, sets, context dimensions and evaluation requests/results;
- `UserIntent`, `DocumentContext`, `ProposedPlan`, `ImpactSummary`,
  `ValidationResult`, `CommitRequest` and `TransactionReceipt`.

The supported v0.1 literal types are color (sRGB hex), number, dimension (`px`,
`ms`, `s`), string, boolean, opacity, font family and font weight. Other types
such as typography composites, gradients, shadows, blur and easing require an
explicitly versioned extension or a later contract version; they must not be
silently coerced into a supported type.

Token identity is `(namespace, UUID)`. Names and optional `groupPath` are for
display and import/export; renaming or regrouping does not change identity.
Collections/modes, sets, groups and context dimensions are optional. A
document with only direct values and no token metadata is valid.

## 2. Proposed resolution semantics

These rules are encoded in the reference vectors but remain subject to Chief
Architect approval:

1. Direct property literals resolve directly and require no token lookup.
2. For a binding, `localOverride` wins first. Otherwise resolve its stable
   `(namespace, tokenId)` under the selected context. If resolution fails and a
   type-compatible fallback exists, return the fallback with a diagnostic;
   otherwise return `ERROR` with no value.
3. `explicit` binding mode selects that mode in the target token's collection.
   `inherit` selects the first available value by context order: property
   explicit context, nearest ancestor to farthest, page, document, then that
   collection's default mode. Context dimensions combine independently.
4. Candidate set values must be active and have matching `when` constraints.
   `activeSetIds` is ordered low to high precedence; the last matching set
   definition for a stable token ID wins. A base definition is the initial
   value. Same-ID set definitions must have compatible token types.
5. Within a token definition, choose the matching value with the greatest
   number of context constraints. Equal-specificity ties are
   `CONTEXT_CONFLICT`. `groupPath` does not affect lookup or precedence.
6. Aliases resolve by namespace and stable UUID, never by display name. Alias
   output type must equal the source token and destination property types.
7. Expressions are declarative AST nodes only; no JavaScript, shell, dynamic
   property access, model-generated code or network lookups are evaluated.
   v0.1 proposes `add`, `subtract`, `multiply`, `divide`, `min`, `max`, and
   three-argument `clamp`. `subtract` and `divide` take exactly two arguments;
   `clamp` takes exactly three; the other operators take at least two. Addition,
   subtraction, min, max and clamp require compatible types and units.
   Multiplication permits at most one dimension and scalar operands. Division
   requires compatible dimensional units. Division by zero, invalid units,
   non-finite values and type mismatch fail.
8. Proposed hard limits per resolution: 32 alias hops, 32 expression levels,
   256 expression nodes, 8 context dimensions, and 128 preview contexts. Limits
   are versioned contract constants, not document-controlled values.
9. Resolution and validation are pure. They return typed values, effective
   context, source (`DIRECT`, `TOKEN`, `LOCAL_OVERRIDE`, `FALLBACK`), stable
   diagnostics and the full dependency trace. They do not write, normalize or
   delete document data.

Import/export adapters are outside this v0.1 execution contract. A later
adapter must declare format/version support, preserve opaque extensions, and
report information loss; it must not guess stable identity from a display
name or silently coerce unsupported token types.

Set precedence and context ordering deliberately appear in proposed fixtures.
The implementation gate should resolve any alternatives before those fixtures
are promoted from `PROPOSED`.

## 3. Binding edits and compatibility

- Binding stores a stable reference and mode strategy, with an optional
  last-known typed fallback. `localOverride` is a separate optional literal;
  it is not the fallback. A binding without a fallback resolves to a typed
  error when its reference cannot be resolved.
- **Override here** sets/replaces `localOverride` and keeps the reference.
  **Revert override** removes it and resolves the reference again. **Detach**
  stores the current resolved value (or compatible fallback if resolution is
  broken) as a direct literal and removes the binding. Detach fails if no safe
  value exists. Each is one reversible property operation.
- Existing direct-value documents remain unchanged. An adapter may stage a
  proposal but cannot persist it during open, import preview, validation or
  resolution. Unknown vendor data is stored under `extensions` and preserved
  opaquely. Unknown keys on a known v0.1 object are rejected; an unsupported
  major version is retained as raw input and is not editable.
- Token creation plus binding is proposed as one operation transaction. The
  authoritative operation layer captures pre-state and derives inverse actions;
  agents cannot supply executable inverse code.

## 4. Agent proposal protocol

The protocol follows this data flow:

```text
UserIntent + DocumentContext + TokenSnapshot
        → ProposedPlan (declarative operations, assumptions, preview contexts)
        → deterministic validation and impact analysis
        → ValidationResult (READY / REJECTED / NEEDS_CONFIRMATION)
        → explicit user-approved, expected-revision CommitRequest
        → future authoritative operation layer
        → TransactionReceipt (revision, operations, undo/replay reference)
```

The plan schema requires `requiresUserApproval: true`, a base revision, bounded
operation count and a closed operation vocabulary. Operations include expected
pre-state so validation can detect stale or conflicting edits and the future
operation owner can derive undo. Schemas reject arbitrary `script`/`code`
fields. A plan only proposes changes; validation and preview do not mutate a
snapshot. Commit must recheck permissions, revision, plan digest and approval
at the authoritative operation boundary. The test suite validates protocol
messages and the pure preview gate; it does not implement a commit service or
claim that a TransactionReceipt is currently emitted by Logos.

Validation must check:

- document/revision and permission scope for all target namespaces;
- identity/name collisions, references, aliases, cycles, types, property scope,
  context/set precedence and expression budgets;
- expected pre-state for each property/token operation;
- impact paths and contexts, including missing/unavailable features; and
- complete atomic rollback on any failed validation or commit.

External libraries must be permission-checked and revision-pinned by the
future authoritative layer. The current v0.1 fixture uses document-local
namespaces and makes no claim of a secure shared-library implementation.

## 5. Golden fixture coverage

`golden.json` contains proposed cases for:

- no-token direct editing; create-token-and-bind proposal; override, revert and
  detach;
- multi-hop aliases, missing target, alias cycle and ID-stable rename;
- explicit/inherited/default mode, multiple context dimensions, set ordering,
  and fallback;
- arithmetic results, unit/type errors, division by zero and budget exhaustion;
- unknown-extension preservation and repeated deterministic results; and
- valid/invalid agent plans, stale revisions, denied namespace access, failed
  validation with unchanged input, and explicit-approval shape.

The test-only JavaScript oracle computes only these candidate vectors. It is
not linked to the editor and is not a second production resolver. A future Rust
resolver must match the promoted vectors before it is designated as the
evaluator; React consumes the contract for authoring and presentation.

## 6. DT-C1.5 — canonical operation/state-owner decision input

The evidence supports keeping distinct authorities for durable commits,
domain evaluation and UI projection. The Chief Architect must still approve
the boundary.

| Boundary | Current repository evidence | Failure recovery | Migration risk / integration cost |
|---|---|---|---|
| React document state | `logos-app/src/stores/documentStore.ts` owns a pages/shapes Zustand projection and direct mutators; there is no token binding model or integrated undo stack in that store. | In-memory state alone does not establish revision-checked recovery; recovery would depend on IndexedDB/sync and backend acceptance. | High migration risk if React became a second persistent authority; high cost to add atomic multi-property operations, revisioning and replay here. Keep as editor projection. |
| Rust domain/resolver | `logos-types` has typed shapes and token candidates; `logos-tokens` has unit-tested alias/mode resolution. No document transaction/revision store connects it to the app. | Deterministic evaluation can fail without mutation, but Rust currently has no durable transaction/revision recovery boundary for document writes. | Moderate integration cost as the evaluator; high cost and duplication risk if it also acquired canonical document persistence. Best current evaluator candidate, not durable owner. |
| Go document persistence | `files_update.go` locks a file row, checks `revn`/`vern`, loads complete competing change history, rebases operations, applies them to `file.data`, inserts `file_change`, and updates data/revision in one database transaction. `get-file` otherwise transports generic JSON. | Existing transaction rollback, stale revision checks, and stored history are the strongest inspected recovery path. Token-aware inverse/replay behavior remains unverified. | Lowest incremental migration risk and integration cost if Go keeps opaque payloads and its existing transaction boundary. Do not add Go token semantics or token tables. |
| Offline IndexedDB | `offline/persist.ts` snapshots pages/shapes and queues generic patches. `offline/sync.ts` currently contains a lightweight rebase shim and describes the fuller Rust rebase path as future wiring. | Local snapshots and queued patches may aid offline recovery, but conflict replay and authoritative commit recovery are not established by this shim. | Moderate-to-high effort to make this the authority; it would duplicate server revision and durable history responsibilities. Keep as local optimistic cache/proposal queue. |
| Undo/redo and collaboration replay | `rust/logos-rebase` implements rebase primitives; the current React document store has no token-aware undo stack. Go stores change history and rebases stale operations, but that alone is not product-level undo/redo. | Go history is the closest recovery source; undo/redo semantics, inverse integrity, and collaboration replay have not been demonstrated end-to-end. | Define one reversible operation format and derive inverses at the authoritative transaction layer. Wiring this into multiple mutable stores would raise divergence and migration cost. |

**Recommendation to the Chief Architect:** make the existing Go file transaction
boundary the sole authority for durable document revisions, accepted
operations, and persisted history; keep its payload handling opaque. Keep React
as the authoring/optimistic projection and Rust as the leading deterministic
domain evaluator candidate, with both consuming the shared versioned
contract. This recommendation minimizes new persistence/migration work and
reuses the only inspected atomic revision/history boundary. It remains
conditional on verifying token-aware atomic operations, rollback, offline
replay and inverse handling. It is a recommendation, not an approved decision;
no implementation or serialization change is authorized by this proposal.

Before approval, validate the present Go commit/rebase path for token-aware
atomic changes, offline replay and failure recovery. If Go's opaque operation
path cannot safely preserve token semantics and inverses, return with evidence
and alternatives rather than adding a parallel registry.

## 7. CI and acceptance boundary

The focused Node test package uses a pinned JSON Schema validator and Node's
built-in test runner. Its workflow runs without credentials, services or
deployment access. Acceptance requires schema-valid fixtures, passing proposed
golden checks, deterministic repeat outputs, no mutation during preview, and
strict rejection of malformed or unauthorized plans. CI success validates
this proposed contract package only; it does not close the canonical owner
decision or claim editor integration.

**Not implemented:** persistent token storage, document schema changes,
Go token tables/validation, React UI, Rust production evaluator integration,
library authorization, transaction service, AI model/tool orchestration,
renderer integration or performance benchmarks.
