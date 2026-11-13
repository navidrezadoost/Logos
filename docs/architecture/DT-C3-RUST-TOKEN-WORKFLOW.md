# DT-C3 — Rust-backed, reversible token workflow

**Status:** experimental, in-memory workflow; no durable token changes
**Baseline:** `a154363e9b757d3946a21616b562d7ce9e3c5e48`

## User flow

The existing Fill section keeps its normal color input and opacity control.
An optional “Save as token” control proposes a named color token for the
selected shape's first fill when that fill is solid. It creates a typed v0.1 plan containing a
`createToken` and `setProperty` operation. The selected fill and token value
are evaluated by the existing `logos-tokens` Rust resolver compiled to
`logos_tokens_wasm.wasm`; the React code does not resolve aliases or token
values itself.

“Preview value change” submits a `setTokenValue` plan through the same Rust
validator and displays its resolved property preview. Resolution source and
diagnostics are shown so a fallback-rendered value is distinguishable from a
healthy token result. “Revert preview” removes the transient canvas projection.
Direct fill edits remain ordinary string-color edits and do not need token
metadata, a collection, a group, a theme, or agent configuration.

## Runtime and persistence boundary

- `logos-app/src/tokens/contract.ts` defines the typed TypeScript wire DTOs for
  snapshots, resolve requests/results, plans, validation results, and impact.
- `logos-app/src/tokens/rustTokenEngine.ts` owns JSON serialization, the
  linear-memory ABI, and calls the Rust `logos-tokens` resolver and validator.
  The small `logos-tokens-wasm` crate is only a bridge; it contains no second
  evaluator.
- The create/bind plan is a proposal against the selected shape's current
  direct fill. The preview is applied only to a separate Zustand render
  projection consumed by Canvas. The persisted React shape remains unchanged.
- The legacy imported-token store and its `logos-token-store` local-storage
  key are not read or modified by this workflow. No token registry, document
  schema, Go persistence, collaboration replay, undo log, or transaction path
  is added.
- The selected binding exists only in the Rust validation result during this
  experiment. `Shape.fills` still stores direct colors. Reloading, changing
  selection, or reverting discards the proposal.
- The demo's revision is an ephemeral proposal revision, not the server's
  document `revn`. Its `allowedNamespaces` and `proposeChanges` fields are
  validator inputs, not authenticated authorization. Neither can authorize a
  durable write.

## Test evidence

The browser smoke uses the actual Inspector, Canvas 2D fallback, and compiled
Rust WASM artifact. It checks direct editing, Rust direct resolution,
create-and-bind validation, serialized camelCase results, missing-reference
diagnostics, stale revision rejection, unchanged document and local-storage
state, visible token-value preview, and revert. It records the sampled canvas
pixel values in the `dt-c3-token-workflow-browser-evidence` CI artifact.

## Compatibility and migration impact

There is no persisted-format migration in this milestone. Existing files
continue storing solid fill hex strings. A future durable binding needs an
approved canonical document model, a schema version and migration strategy,
reader behavior for unknown or missing token references, and a rule for
preserving current visual appearance. This PR does not define or write that
model. The UUID v4 feature was moved to test-only dependencies in `logos-types`
and `logos-tokens`; production resolver code only parses and serializes IDs.

## Go file-transaction decision evidence

Repository inspection shows `backend-go/internal/handler/files_update.go`
already checks file edit permission, locks the file row, checks `vern` and
`revn`, rebases supported change sets, applies them to file data, writes a
`file_change` row and increments `revn` in one database transaction. That is
useful evidence for a possible durable authority, but it does not yet prove
that token bindings can safely use it. In particular, current updates mint a
new change UUID server-side and the inspected request contract has no explicit
idempotency key or typed token operation. The snapshot handler documents that
Go snapshots may lack a `file_data` copy when CRDT state is unavailable.

Before selecting it for token writes, verify with focused contract and
integration evidence that:

1. The proposed token operation is accepted in the canonical file format and
   preserves older readers' visual output.
2. The caller's expected document revision and token/property pre-state are
   checked atomically under the same lock as the write.
3. Retry identity and duplicate submission behavior are defined, including
   concurrent edits and rebase outcomes.
4. The committed file data, change history, broadcast/replay path, snapshots,
   and read-after-write response agree on one resulting revision.
5. Authorization covers both the file and every token namespace in the plan.
6. Revert/undo has a durable inverse or an explicit compensating operation.

No conclusion is made here that Go is the canonical document owner or that its
current operation surface meets these requirements. That decision remains
with the Chief Architect.
