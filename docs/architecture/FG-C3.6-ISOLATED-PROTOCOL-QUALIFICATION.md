# FG-C3.6 — Isolated Component/Instance Protocol Qualification

**Status:** Implemented and locally qualified in the non-production Rust/Go planner spike. Capability advertisement remains disabled.
**Branch:** `core/g1-b9-6-memory-evidence-foundation`
**Baseline:** `6737c4cdaf2adf77c9208d11f7f6ae67184b9190`
**Operation:** `validateComponentInstanceV1`, planner protocol v2
**Authority:** Validation status and diagnostics only; no document output, mutation, persistence, or commit instruction.

## Contract decisions

- Diagnostic arrays are exact and ordered. Warning-only fixture expectations include every diagnostic with its path, severity, and detail. Tests do not treat the listed codes as a required subset.
- Transport JSON text strictly rejects duplicate **decoded** member names at both envelope and embedded-document levels. For example, `"property"` and `"pro\\u0070erty"` collide. This is an explicit raw transport rule and does not change TypeScript's object-level v1 semantics or the earlier last-member-wins differential cases.
- A completed semantic rejection is a correlated response with `ok:false`, phase `component-validation`, and one or more error diagnostics (warnings may accompany them). It does not poison or restart the process.
- Protocol/parser/resource failures use the `error` envelope and a stable code. A timeout/caller cancellation has no partial response; Go kills and reaps the child, invalidates the session, and the next call starts a new process.
- Limits are inclusive: exactly the stated size/count/depth/work budget is accepted by that guard; the next unit is rejected. JSON nesting counts all object and array containers, including the outer envelope. Diagnostic bytes are the exact serialized UTF-8 JSON array, including brackets, separators, field names, and escaping.

## Error taxonomy

| Condition | Outcome |
|---|---|
| Malformed JSON, unknown/missing operation fields | `REQUEST_INVALID` |
| Invalid frame UTF-8 | `REQUEST_INVALID_UTF8` |
| Missing, empty, or overlong correlation ID | `REQUEST_ID_INVALID` |
| Unsupported protocol / planner semantic / diagnostic version | `PROTOCOL_VERSION_UNSUPPORTED` / `SEMANTIC_VERSION_UNSUPPORTED` / `DIAGNOSTIC_VERSION_UNSUPPORTED` |
| Unsupported component semantic version | `COMPONENT_SEMANTIC_VERSION_UNSUPPORTED` |
| Duplicate decoded key in outer or embedded JSON | `DUPLICATE_JSON_MEMBER` |
| Outer request/frame > 1 MiB | `REQUEST_TOO_LARGE` |
| Embedded document > 768 KiB UTF-8 | `COMPONENT_INPUT_TOO_LARGE` |
| Outer parser depth > 64 containers or > 100,000 values | `REQUEST_RESOURCE_LIMIT` |
| Embedded parser depth > 64 containers or > 100,000 values; semantic expansion depth/work exhaustion | `COMPONENT_RESOURCE_LIMIT` |
| Embedded text is syntactically invalid JSON but within limits | Completed semantic validation with `COMPONENT_JSON_INVALID` error diagnostic |
| More than 512 diagnostics or serialized diagnostic array > 768 KiB | `COMPONENT_DIAGNOSTIC_LIMIT`; no partial success diagnostics |
| Serialized response > 1 MiB | `RESPONSE_TOO_LARGE`, replacing the original result |
| Truncated frame header/body | `FRAME_TRUNCATED`; process exits and must be restarted |
| Go caller context/deadline cancellation | Go returns context error without a response, kills/reaps process, and restarts lazily on next call |

A semantic document rejected by v1 is distinct from a protocol error: it is a valid completed operation and leaves the worker usable. An invalid or stale response is a Go protocol-validation failure and invalidates the process session.

## Enforcement order and budgets

1. Go rejects an outgoing request above 1 MiB before starting the worker. The Rust frame loop checks an announced frame length before allocating/reading the body.
2. The frame loop reads exactly the declared body, rejects truncated header/body, then checks UTF-8.
3. The outer JSON parser rejects duplicate decoded keys and enforces 64 containers and 100,000 JSON values while parsing, before materializing unbounded nested values. Outer bytes are also bounded at 1 MiB.
4. The protocol validates fields and protocol/planner/diagnostic versions, then checks the component semantic version.
5. The embedded `documentJson` byte length is checked before its parser allocates the document value.
6. The embedded parser rejects duplicate decoded keys, depth above 64, and values above 100,000 before semantic traversal.
7. Semantic traversal has independent expansion-work (100,000 steps) and expansion-depth (64) limits. Diagnostic accumulation stops at 512 entries or 768 KiB of exact serialized diagnostic-array bytes. If either guard trips, the partial semantic list is discarded and a bounded protocol error is returned.
8. The complete serialized response is checked against the 1 MiB response limit. Go independently bounds the response frame and validates the complete result shape, versions, request correlation, operation kind, exact diagnostic fields, UTF-16 path/code/severity order, warning/error consistency, and absence of `document` or commit fields.
9. `Planner.ValidateComponentInstanceV1` applies an initial 2-second request deadline. An earlier parent context deadline wins. Go cancellation terminates and waits for the child before allowing a later request to restart it.

These limits are initial qualification targets. They are not operating-system memory/CPU isolation or a deployment service-level guarantee.

## Executable adversarial matrix

The twelve FG-C3.5 scenarios are covered by `go-coordinator/fg_c3_6_test.go` using the framed Rust process, except stale-response injection, which uses a test helper child that speaks the same framed protocol:

| Scenario | Executable assertion |
|---|---|
| `valid-clean` | Exact validation-only success with zero diagnostics and correlated request ID |
| `valid-warning-only` | Exact three diagnostic objects, preserving source ordering and warning-only success |
| `rejected-component-graph` | Exact five diagnostic objects and error-bearing semantic rejection |
| `unsupported-component-semantic-version` | Correlated `COMPONENT_SEMANTIC_VERSION_UNSUPPORTED` protocol error |
| `malformed-request-envelope` | `REQUEST_INVALID`; following request still succeeds |
| `duplicate-json-member-policy` | Duplicate outer escaped key rejected with null correlation; nested escaped duplicate in `documentJson` rejected with the valid correlation ID |
| `oversized-document-payload` | 768 KiB accepted by the size guard and reaches JSON validation; 768 KiB + 1 returns `COMPONENT_INPUT_TOO_LARGE` |
| `oversized-request-frame` | 1 MiB exact framed request accepted; 1 MiB + 1 announcement returns `REQUEST_TOO_LARGE` before body allocation |
| `excessive-diagnostic-output` | Exactly 512 diagnostics returned as a complete semantic rejection; 513 returns `COMPONENT_DIAGNOSTIC_LIMIT`; same worker then accepts a valid request |
| `stale-response-correlation` | Stale ID rejected by Go; worker session invalidated; next valid call succeeds using a new Rust child |
| `truncated-input-frame` | Declared 20-byte body with 7 bytes returns `FRAME_TRUNCATED`; process exits |
| `restart-after-semantic-rejection` | Invalid semantic result followed by valid result remains correlated on the same process; no partial state crosses calls |

Additional executable coverage checks outer and embedded parser depth/work boundaries, semantic expansion depth separate from JSON nesting, malformed component-response shapes, warning/error contradictions, diagnostic serialized-byte limits, cancellation/kill/reap/restart, and the unchanged `not-supported-here` capabilities handshake.

## Differential and regression evidence

Recorded after completing the local run:

| Check | Result |
|---|---|
| Rust planner workspace unit tests | **15 passed, 0 failed**; binary/main/doc test targets contain 0 tests |
| Rust planner crate formatting | **Passed** with package-scoped `cargo fmt --manifest-path ... -- --check` |
| Go coordinator full suite | **52 test/subtest outcomes passed, 0 failed** |
| FG-C3.6 Go outcomes | **20 test/subtest outcomes passed, 0 failed**; all 12 named protocol scenarios are accounted for |
| Go race detector | **Passed**, `go test -race ./... -count=1` |
| TypeScript Component/Instance tests | **13 passed, 0 failed**; test runner printed a non-fatal Vite WebSocket port-in-use warning |
| FG-C3.5 fixture/capability contract tests | **2 passed, 0 failed** |
| FG-C3.5 differential corpus | **339 total; 338 compared and matched; 1 JavaScript-only input unsupported; 0 divergences, runner failures, or parser failures** |
| Rust native vs Node-hosted WASM parity | **4/4 matched**: clean success, warning-only success, escaped duplicate rejection, and depth-limit rejection |
| Hosted CI | **Not executed**; local results are not hosted CI |
| Remote deployment / production Component/Instance validation | **Not executed and not authorized** |

The 338 shared-wire parity claim belongs to the pre-existing FG-C3.5 semantic corpus; strict duplicate rejection is separately tested as a transport-only policy difference. No production corpus, production endpoint, browser rendering, retained-memory measurement, durable transaction, or authority transfer was exercised.

## Readiness verdict and limitations

**Isolated protocol qualification: source and local execution evidence prepared for review.** The capability handshake deliberately continues to say `componentInstanceDomain.status = "not-supported-here"`; no advertised capability is added.

**Capability exposure: NOT AUTHORIZED. Production readiness and deployment qualification: NOT ESTABLISHED. Durable transaction safety: NOT ESTABLISHED.** The child is a local process without deployment packaging, OS-level resource controls, authenticated caller context, cross-process transaction semantics, or production observability. Rust remains a candidate validator; TypeScript v1 semantics and all production mutation/persistence authorities remain unchanged. Hosted CI is unavailable under the billing restriction and is not represented by local test results.

G1-B9.6 retained memory remains **NOT MEASURED**. G1-B10 remains **NO-GO**.

## Reproduction

From the repository root:

```sh
cargo fmt --manifest-path spikes/fg-c3-2-1/rust-planner/Cargo.toml -- --check
cargo test --manifest-path spikes/fg-c3-2-1/rust-planner/Cargo.toml --workspace
node --test test/figma-semantics/component-instance-domain-v1.test.mjs
node test/figma-semantics/component-instance-differential.mjs
cargo build --manifest-path spikes/fg-c3-2-1/rust-planner/Cargo.toml
FG_C3_2_2_RUST_PLANNER="$PWD/spikes/fg-c3-2-1/rust-planner/target/debug/fg-c3-2-1-rust-planner-spike" go -C spikes/fg-c3-2-1/go-coordinator test ./... -count=1
FG_C3_2_2_RUST_PLANNER="$PWD/spikes/fg-c3-2-1/rust-planner/target/debug/fg-c3-2-1-rust-planner-spike" go -C spikes/fg-c3-2-1/go-coordinator test -race ./... -count=1
cargo build --manifest-path spikes/fg-c3-2-1/rust-planner/Cargo.toml --target wasm32-unknown-unknown --features wasm
wasm-bindgen spikes/fg-c3-2-1/rust-planner/target/wasm32-unknown-unknown/debug/fg_c3_2_1_rust_planner_spike.wasm --target nodejs --out-dir /tmp/fg-c3-6-wasm-node
FG_C3_6_WASM_NODE_MODULE=/tmp/fg-c3-6-wasm-node/fg_c3_2_1_rust_planner_spike.js FG_C3_6_RUST_PLANNER="$PWD/spikes/fg-c3-2-1/rust-planner/target/debug/fg-c3-2-1-rust-planner-spike" node test/figma-semantics/component-instance-protocol-wasm-parity.mjs
```

All results are local and must not be described as hosted CI or production qualification. A repository-wide `cargo fmt --all -- --check` also reports existing formatting differences in unrelated Rust packages; the isolated planner crate's targeted format check passes, and no unrelated files were changed.
