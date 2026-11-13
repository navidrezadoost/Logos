# DT-C2 — Rust resolver compatibility and semantic decisions

**Baseline:** `3858ed87eb29aa041994d6ff9e2d0013f7840a2a`  
**Status:** implementation proposal; canonical document ownership remains undecided  
**Scope:** pure in-memory resolution and plan validation; no persistence

The proposed contract implementation is in `rust/logos-tokens/src/contract.rs`,
`contract_resolver.rs`, and `plan_validation.rs`. The compatibility table below
describes the pre-existing `logos_types::TokenRegistry` resolver, not the new
contract entry point. Both remain separate until the Chief Architect decides
document ownership and an integration boundary.

## Existing Rust resolver versus proposed v0.1 contract

The inspected implementation is `rust/logos-tokens/src/variable_resolver.rs`.
It consumes `logos_types::TokenRegistry`, which is UUID keyed and file-local.
It remains useful for its existing callers. The new versioned contract entry
point is added in this crate so it can represent the richer v0.1 request without
changing the persisted registry or creating a JavaScript runtime.

| Capability | Status | Evidence and gap |
|---|---|---|
| Stable-ID aliases | PARTIAL | `resolve_variable` follows UUID aliases, detects cycles, and records a UUID chain. It has no namespace component and does not validate alias target type. |
| Mode selection | PARTIAL | Explicit mode, nearest-first parent contexts, and collection default are supported. Arbitrary context values and mode dimensions are not. |
| Set precedence | MISSING | Neither `TokenRegistry` nor `VariableResolver` has active sets or ordered override layers. |
| Contextual values | PARTIAL | Values are selected by token/mode UUID only. Context maps, specificity, and ambiguity errors are absent. |
| Property bindings | PARTIAL | Scope and optional fallback are supported. No local override, stable namespaced reference, or property type validation exists. |
| Type validation | MISSING | `SemanticToken.token_type` is not compared with `TokenValueKind` or the bound property type. |
| Expression evaluation | MISSING | `TokenValueKind` supports primitive values and aliases only; no AST or arithmetic exists. |
| Error diagnostics | PARTIAL | Errors are typed Rust variants, but there are no versioned diagnostic codes, severity, or stable messages. |
| Provenance | PARTIAL | `alias_chain` and a coarse source are returned. Selected context, set, mode path, and diagnostic token chain are not. |
| Evaluation budgets | PARTIAL | Alias depth is capped at 32. Expression node/depth, context-axis, preview-context, and numeric limits are absent. Parent context recursion is not bounded. |
| Duplicate/malformed registry handling | MISSING | Hash maps hide duplicate entity IDs at construction; `token_values` can be deserialized with repeated token/mode pairs and the current lookup returns the first. |
| Pure/no-mutation behavior | IMPLEMENTED | Current resolution methods take immutable references and do not write to the registry. |

## Proposed v0.1 arithmetic decision

These rules amend the **proposed**, not production-approved, v0.1 semantics.
They are added to the shared input/output fixture set and to the Rust and
JavaScript test oracles in this PR.

- Values are finite IEEE-754 binary64 numbers with absolute magnitude at most
  `1_000_000_000_000` after normalization and after every operation. A result
  outside the bound returns `NUMERIC_RANGE`; it is never clamped.
- `number × number` returns `number`.
- `dimension × number` and `number × dimension` return a dimension. Two
  dimensions cannot be multiplied.
- `dimension ÷ number` returns a dimension. `dimension ÷ compatible dimension`
  returns a dimensionless `number`; for example `12px ÷ 3px = 4`.
- `number ÷ number` returns a number. `number ÷ dimension` is unsupported and
  returns `TYPE_MISMATCH` because reciprocal dimensions are not represented in
  v0.1.
- `px` is its own dimension. `ms` and `s` are compatible time units and are
  normalized to milliseconds for arithmetic (`1s + 500ms = 1500ms`). A
  dimension result involving time uses `ms`; direct literal values retain
  their input unit. Compatible time-dimension division returns a number.
- Add, subtract, min, max and clamp require compatible operand types and
  dimensions. Subtract and divide take two operands; clamp takes three; the
  remaining operators take at least two.
- Unit mismatch returns `TYPE_MISMATCH`, division by zero returns
  `DIVISION_BY_ZERO`, and non-finite or out-of-range values return
  `NUMERIC_RANGE`.

## Scope and remaining decisions

The Rust contract evaluator and validator are pure proposals over typed DTOs.
They do not load or store `TokenRegistry`, mutate a document, emit Go file
changes, or choose the canonical document owner. Invalid snapshots and plans
produce complete rejection results; the plan validator previews against cloned
in-memory values only. Production integration, document operation ownership,
external library authorization, collaboration replay, and persistent migration
remain out of scope.

Plan validation is a non-committing foundation: it validates protocol and
revision, proposal permission, namespace/scope, operation order and expected
pre-state, token references and cycles, expression/type evaluation, and
preview contexts. It returns a stable digest and impact summary. It does not
apply property operations to document state, issue a transaction receipt, or
provide durable atomicity/undo; `READY` means preview validation only. The
`DocumentContext` type is supplied by a caller and is not connected to a
canonical document model. Property validation can infer the type only from the
included property literal/reference and is rejected as unsupported otherwise.

The shared full-input/full-output conformance file is
`contracts/design-token/v0.1/fixtures/resolution-vectors.json`. Both the Rust
crate test and JavaScript reference oracle consume it. It covers the bounded
alias chain, a 64-leaf dependency graph, and context/mode/set cases. These are
correctness and resource-bound tests, not a performance benchmark; no runtime
or memory result is claimed. CI test duration is not a memory measurement.
