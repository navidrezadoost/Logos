# FG-C3.5 — Component/Instance Validation Protocol Proposal

**Proposal version:** `fg-c3.5-proposal-v1`  
**Status:** Design only; not authorized for implementation or capability advertisement  
**Baseline:** `e2734a0b96b31750920a75faf13393a60034ca6c`  
**Existing isolated planner protocol:** protocol 2, semantic 1, diagnostics 1  
**Existing transport limits:** 1 MiB request and 1 MiB response

## Proposed operation and wire shape

Propose an additive operation named `validateComponentInstanceV1` under planner protocol version 2. It is a validation-only request. It does not apply an operation, commit a document, authorize a user, migrate data, or make Rust authoritative. The current capability response must continue to say `componentInstanceDomain.status = not-supported-here` until a separate Chief Architect authorization and implementation qualification.

```json
{
  "protocolVersion": 2,
  "semanticVersion": 1,
  "diagnosticVersion": 1,
  "requestId": "caller-generated-id",
  "kind": "validateComponentInstanceV1",
  "componentSemanticVersion": 1,
  "documentJson": "{\"version\":1,\"components\":{},\"componentSets\":{},\"instances\":{}}"
}
```

`documentJson` is a UTF-8 JSON text string so the receiver can distinguish malformed component JSON and inspect duplicate members before an ordinary object parser discards them. This double encoding is a deliberate v1 transport choice; its escaping overhead counts toward the outer request limit. The document root must be an object. JSON object values accept null, booleans, strings, arrays, objects, and finite IEEE-754 binary64 numbers. `undefined`, non-finite values, cyclic objects, and non-JSON prototypes have no wire representation. Unicode keys and values are preserved as authored; no NFC/NFD normalization is performed. Diagnostic string sorting uses JavaScript UTF-16 code-unit order to remain compatible with the existing TypeScript v1 reference.

The response is a validation report only and never echoes the document:

```json
{
  "protocolVersion": 2,
  "semanticVersion": 1,
  "diagnosticVersion": 1,
  "componentSemanticVersion": 1,
  "requestId": "caller-generated-id",
  "operationKind": "validateComponentInstanceV1",
  "ok": true,
  "phase": "component-validation",
  "diagnostics": [
    {"code":"COMPONENT_SLOT_GUIDANCE_EXCEEDED","path":"$.instances.x.slotContent.y","severity":"warning","detail":"..."}
  ]
}
```

`ok: true` means the document has no error-severity diagnostics. It may contain warnings. `ok: false` means semantic validation rejected the document. `diagnostics` retains exact v1 order and the `code`, `path`, `severity`, and `detail` fields. The successful response deliberately omits the document: callers retain their own source bytes, response size is bounded, and validation cannot be mistaken for a canonicalized document transaction. The validator does not change source bytes or produce a canonical document.

### Version and correlation fields

| Field | Proposed value/policy |
|---|---|
| `protocolVersion` | `2`, matching the existing isolated planner envelope. A future incompatible framing change requires a new protocol version. |
| `semanticVersion` | Existing planner semantic version `1`; not sufficient on its own to select the component contract. |
| `componentSemanticVersion` | Required `1`, selecting Component/Instance v1 semantics. Other values receive a transport/envelope `COMPONENT_SEMANTIC_VERSION_UNSUPPORTED` error before domain validation. |
| `diagnosticVersion` | `1`, exact existing diagnostic shape/order. A changed diagnostic shape requires an explicit version increment. |
| `requestId` | Required non-empty UTF-8 string, at most 128 bytes. It is echoed byte-for-byte with `operationKind`. Caller discards a response whose correlation pair does not match the outstanding request. |

Transport validity and component semantic validity are separate. Invalid outer JSON, duplicate envelope members under the recommended policy, invalid envelope fields, or unsupported protocol/semantic/diagnostic versions use the protocol `error` response and do not call the component validator. Malformed `documentJson` produces a component-input parse failure (`COMPONENT_JSON_INVALID`); parseable but structurally invalid component JSON produces ordinary v1 component diagnostics. A semantic rejection is a completed, correlated request and must not poison the worker for a following request.

## JSON normalization and duplicate-key decision matrix

| Topic | TypeScript v1 baseline / evidence | Candidate transport contract | Compatibility consequence and recommendation |
|---|---|---|---|
| Number parsing | `JSON.parse` produces IEEE-754 `Number`; unsafe integer tokens round. FG-C3.4.1 tests `9007199254740993`, integer-valued float, negative zero, and finite exponent. | Parse JSON number tokens to binary64 with JavaScript-compatible rounding. Reject values whose parse would be non-finite; retain negative zero. | Preserve existing TypeScript semantics. Exact source decimal spelling is not preserved. This is now explicit for wire v1. |
| Diagnostic ordering | TypeScript sort is UTF-16 code-unit order; Rust candidate matches it on the tested corpus. | Sort diagnostics by path, then code, then severity using UTF-16 code units. | Preserves tested v1 ordering; Unicode key/value content itself is unchanged. |
| Null/absence | Distinct. Nullable `setId: null` is valid; absent required field rejects. | Preserve JSON null and omission as different states; do not default missing required fields. | Exact existing semantic contract. |
| Duplicate object member names — last wins | `JSON.parse` and current Rust JSON parsing both keep the last tested `version` occurrence. This is current parser behavior, not a duplicate-aware validator guarantee. | Accept duplicates and use last-member-wins at every envelope/document level. | Best direct parity with existing parsers, but retains parser differential/smuggling ambiguity and loses evidence of earlier members. Not recommended for a trusted service boundary. |
| Duplicate object member names — strict rejection | Not observable after TypeScript receives a JavaScript object; it changes no behavior for ordinary object inputs but rejects raw texts previously accepted by last-wins parsing. | Token-scan both outer envelope and `documentJson` before object materialization; reject any duplicate decoded member name with stable `DUPLICATE_JSON_MEMBER` transport error. | Recommended for the future trusted protocol boundary, provided the scanner is independently tested for escapes, Unicode-equivalent spellings, nested objects, and bounded depth. This is a transport rule, not a change to the TypeScript v1 object validator. Existing corpus last-wins case remains as parser compatibility evidence, not service acceptance evidence. |
| Malformed JSON | Rust text validator returns `COMPONENT_JSON_INVALID`; structured but invalid documents get semantic diagnostics. | Invalid outer JSON => `REQUEST_INVALID`; invalid `documentJson` => `COMPONENT_JSON_INVALID`; valid JSON with bad v1 shape => semantic diagnostics. | Keeps transport/parser failure distinct from semantic rejection. |

**Recommended policy:** strict duplicate-member rejection at the future raw-text service boundary; retain the TypeScript v1 semantic API unchanged. This policy is proposed only. It requires a duplicate-aware tokenizer before `serde_json::Value` construction; default `serde_json::Value` is not evidence that duplicates were absent. If exact TypeScript parser parity is selected instead, the protocol must explicitly say last-wins and accept that the service cannot report duplicate members after parsing.

## Proposed limits and failure behavior

These are proposed qualification limits, not current Component/Instance service guarantees.

| Resource | Proposed bound | Behavior at limit |
|---|---:|---|
| Outer framed request | Existing maximum 1 MiB | Reject with `REQUEST_TOO_LARGE` before allocation beyond limit. |
| `documentJson` UTF-8 bytes | 768 KiB | Reject before parse with stable `COMPONENT_INPUT_TOO_LARGE`. Outer JSON escaping still counts toward the 1 MiB frame. |
| JSON nesting | 64 containers | Reject before recursive semantic traversal with `COMPONENT_RESOURCE_LIMIT`; no stack overflow or panic. |
| Diagnostic count | 512 | Do not return a partial successful list. Return a correlated resource-limit error with no document payload. |
| Response | Existing maximum 1 MiB; proposed diagnostic payload ceiling 768 KiB | If serialization exceeds the response ceiling, use the protocol's bounded `RESPONSE_TOO_LARGE` response; never silently truncate diagnostics. |
| CPU/wall time | Initial supervisor deadline target 2 seconds per request | Target requires measurement. On deadline, terminate and recreate the isolated worker; report correlated timeout when possible. No partial state exists because this operation is pure. |
| Cancellation | Caller cancellation drops the pending correlation; supervisor must stop work at deadline or worker termination | Current synchronous planner has no demonstrated in-process cancellation. Qualification must prove kill/restart recovery before any exposure. |

Limits must be checked before unbounded recursion/allocation. They are part of the proposed operation contract, not semantic rules in the TypeScript v1 validator. Resource errors are protocol outcomes and must not be reported as successful validation.

## Adversarial transport fixture plan

`test/fixtures/figma-semantics/fg-c3-5-protocol-adversarial-v1.json` defines twelve deterministic scenarios. At FG-C3.5 these specified future behavior only. FG-C3.6 converted them into local framed Rust/Go qualification tests; the fixture now records exact semantic diagnostics and the status of each case. See [FG-C3.6 isolated protocol qualification](FG-C3.6-ISOLATED-PROTOCOL-QUALIFICATION.md) for implementation and run evidence.

Coverage includes clean success; warning-only success; invalid graph; unsupported component semantic version; malformed envelope; strict duplicate-key rejection; document-payload and outer-frame size limits; excessive diagnostics; stale response correlation; truncated frame; and process restart after validation rejection. Size-heavy inputs use deterministic generation directives rather than committing multi-megabyte blobs. No fixture asks the planner to persist, render, authorize, or migrate a document.

## FG-C3.5 differential evidence

The retained FG-C3.4.1 seed and its generated documents/case names remain unchanged in identity. This revision adds two fixed seeds, so the matrix is `[0x0c3c0401, 0x5eedc301, 0x20261118]` (decimal `[205259777, 1592640257, 539365656]`); each runs 48 valid and 48 controlled-invalid samples with graph depth capped at 8. Three additional deterministic combined mutations cover variant selection, instance-swap allow-lists, and slot topology. Numeric and Unicode boundary cases remain in the fixed corpus. Case names include the seed and sample; mismatch artifacts preserve the original input and both outcomes, with deterministic top-level record reduction on JSON object cases.

Every case is classified as `shared-wire` or `typescript-only`; unclassified cases fail the run. Rust process/spawn/count faults are runner failures; invalid JSON output lines are parser failures; result differences after valid responses are semantic divergences. A JavaScript-only NaN case is explicitly unsupported because no JSON text can represent it. It is still checked for TypeScript input purity, but excluded from shared parity counts. The report separately lists validator outcomes carrying `COMPONENT_JSON_INVALID` (the non-object and malformed-text cases); these are semantic API results, distinct from a harness response-parser failure.

Latest local run: **339 total cases; 338 compared; 338 matched; 1 explicitly unsupported; 0 runner failures; 0 parser failures; 0 semantic divergences.** The cases are 37 fixed cases, 144 seeded valid documents, 144 controlled-invalid mutations, 3 combined variant/swap/slot mutations, 10 wire/runtime boundaries, and one JavaScript-only NaN input. The two fixture-contract tests pass **2/2**. These are local results, not hosted CI, a production corpus, or performance qualification. FG-C3.6 transport evidence is recorded separately below.

## Readiness decision

**FG-C3.5 historical verdict: NOT READY.** At the time of this proposal the operation was not implemented and transport controls were unqualified. FG-C3.6 has since implemented and locally qualified the isolated protocol operation, strict parser, resource limits, framed adversarial behavior, response validation, timeout and process recovery. This follow-up does not authorize capability advertisement, production deployment, Rust authority transfer, durable transaction safety, or canonical-document ownership. Those remain separate decisions.

The historical implementation sequence is complete through local qualification; the remaining adoption gate is a separate Chief Architect decision to advertise the isolated capability. Production authority, persistence, rendering, deployment isolation, and migration require separate decisions.

## Reproduction

From the repository root:

```sh
cargo fmt --manifest-path spikes/fg-c3-2-1/rust-planner/Cargo.toml -- --check
cargo test --manifest-path spikes/fg-c3-2-1/rust-planner/Cargo.toml
node --test test/figma-semantics/component-instance-domain-v1.test.mjs
node test/figma-semantics/component-instance-differential.mjs
node --test test/figma-semantics/fg-c3-5-protocol-contract.test.mjs
```
