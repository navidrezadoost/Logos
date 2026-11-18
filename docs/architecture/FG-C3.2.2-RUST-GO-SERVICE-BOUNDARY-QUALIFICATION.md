# FG-C3.2.2 — Rust/Go Service Boundary and Protocol Qualification

**Baseline:** `a9f6572ac6631c00d06b7f6cf15818ba6a3b7b89`
**Branch:** `core/g1-b9-6-memory-evidence-foundation`
**Result:** **CONDITIONALLY FEASIBLE**
**Scope:** isolated local-process qualification only; no production integration, writes, authority transfer, or schema change.

## Outcome

The one-shot Rust executable from FG-C3.2.1 has been advanced to a persistent, length-prefixed child process. The Go coordinator starts one Rust planner on demand, sends repeated requests over framed stdin/stdout, validates correlated response IDs, serializes exchanges on the single process, and restarts it after failure or cancellation. The native Rust process and Node-hosted WASM entrypoint remain backed by the same Rust wrapper and matched **12/12** complete normalized fixture responses.

This advances Alternative B as an operational candidate. It does not qualify a network service, sandbox, production deployment, parallel Rust execution, storage transaction, or Component/Instance semantics in Rust. Production editing remains React/Zustand; Go remains the server persistence authority. G1-B10 is still **NO-GO**.

## Protocol v2

### Framing and envelopes

The local process transport uses a 4-byte unsigned big-endian payload length followed by exactly that many UTF-8 JSON bytes. One frame is one request; one response frame is returned for each complete request. The maximum request and response frame payload is **1 MiB**. The Rust `capabilities` response reports these limits and the framing name.

Every valid request contains:

| Field | Contract |
| --- | --- |
| `protocolVersion` | `2` |
| `semanticVersion` | `1` for this planner operation set |
| `diagnosticVersion` | `1` for this error/diagnostic contract |
| `requestId` | Non-empty string up to 128 UTF-8 bytes, echoed in the response |
| `kind` | `capabilities`, `evaluate`, `deriveIds`, or `checkDetachIds` |

Responses include protocol, semantic, and diagnostic versions, `requestId`, and either a result/diagnostic set or a structured `error`. JSON is serialized by `serde_json` with the crate's default deterministic map ordering. The parity harness compares normalized JSON values, while repeated Rust diagnostic responses are also compared byte-for-byte. This is not a separate canonical-JSON signing format.

Supported capabilities are:

- Document operations, semantic version 1: `applyOperation` and document validation through the existing `logos-types` decode/validate path.
- Projection identity experiment, version 1: `deriveIds` and caller-supplied detach-ID collision checking.
- Diagnostic version 1.
- **Component/Instance v1 domain validation: unsupported in this Rust boundary.** The capability response identifies `TypeScript componentInstanceDomainV1` as its current owner. Rust parity for that domain is not claimed.

### Failure and resource behavior

| Condition | Response/behavior | Process lifecycle |
| --- | --- | --- |
| Malformed complete JSON or invalid request envelope | `REQUEST_INVALID` | Frame consumed; process continues. Correlation ID is null if it cannot be safely read. |
| Invalid UTF-8 frame | `REQUEST_INVALID_UTF8` | Frame consumed; process continues. |
| Request exceeds 1 MiB | `REQUEST_TOO_LARGE` | Rust replies and closes, avoiding an unbounded drain. Go rejects oversized payloads before writing them to the child. |
| Response exceeds configured response cap | `RESPONSE_TOO_LARGE` | Bounded structured response; process continues. A lower response cap exists only for local negative-path testing. |
| Partial header or body | `FRAME_TRUNCATED` | Structured response where output is available, then controlled exit because the stream cannot be resynchronized. |
| Unsupported protocol, semantic, or diagnostic version | `PROTOCOL_VERSION_UNSUPPORTED`, `SEMANTIC_VERSION_UNSUPPORTED`, or `DIAGNOSTIC_VERSION_UNSUPPORTED` | Correlated error; process continues. |
| Rust panic during request handling | `PLANNER_INTERNAL_ERROR` via `catch_unwind` | Process continues if unwinding succeeds. OOM, abort, or native process termination is not recoverable in-process. |
| Caller cancellation / timeout | Go kills the active child and returns cancellation/deadline error | Next request lazily starts a new child. The CLI coordinator uses a 20-second request deadline; the library caller can pass a shorter context. |
| Child exits, is unavailable, or returns a malformed/oversized response frame | Go fails the call closed and drops the child | A later call attempts a fresh start. No candidate is treated as committed. |

Both Rust and Go enforce the 1 MiB frame limits. The Rust request is bounded before allocation; response JSON is checked before framing, and Go independently bounds the response length. Go's stderr capture is capped at 4 KiB. These are application-level bounds, not an OS sandbox or memory/cpu quota. The spike does not qualify protection against kernel-level memory pressure or a malicious executable at the configured path.

### Trust boundary

```text
Untrusted React / browser WASM preview
        │ typed intent + operation key + doc ID + expected revision
        ▼
Future Go transaction coordinator — not implemented here
  authenticate/authorize → load authoritative snapshot/revision
        │ framed typed intent against that snapshot
        ▼
Persistent local Rust semantic planner — implemented as isolated spike
        │ validated candidate + deterministic diagnostics
        ▼
Go must still fence revision + operation key and atomically persist
document revision + replay outcome before returning an authoritative result
```

Browser/WASM output remains advisory and untrusted. A future Go adapter must receive a typed operation and authoritative snapshot/revision, call the approved semantic planner, validate the response envelope, and treat the result as a candidate only. Go must authenticate and authorize before invocation, enforce revision fencing and durable operation-key semantics, and commit the candidate together with the operation outcome in one database transaction. No client-supplied candidate bypasses server-side planning.

For durable retries, scope the unique operation key at least by workspace/tenant, document, and operation ID. Fingerprint a versioned canonical request including document ID, operation kind/payload, expected revision, and any security-relevant caller context. The same key and fingerprint replays the stored result; the same key with a different fingerprint conflicts. The operation ledger and document revision must commit atomically. Planner timeout/crash before commit has no effect; a lost response after commit must replay the committed outcome. External effects are excluded. This remains a specification: no database, revision fence, operation ledger, or crash-after-commit behavior was implemented. Therefore no exactly-once claim is made and G1-B10 remains **NO-GO**.

## Operational comparison

The spike compares a persistent local process with the prior one-shot subprocess shape, both running the same release Rust binary and the same 5,938-byte `reorder.json` fixture. Each sample includes Go request/response handling. The first persistent sample includes process startup; warm samples reuse one planner. One-shot samples start a new Rust process per request. Eight concurrent Go callers are queued by a single-flight gate and receive correlated results; the Rust process itself evaluates one frame at a time. Concurrent measurements therefore include queue time and are not parallel Rust throughput.

**Observed locally on 2026-11-18 12:59 UTC:** Manjaro Linux, kernel `6.12.112-1-MANJARO`, x86_64; AMD Ryzen 9 7845HX; Go `1.27.1-X:nodwarf5`; Rust `1.93.0`; Node `26.10.0`; wasm-pack `0.13.1`. 200 sequential samples per mode; concurrent mode used eight callers and 200 total requests.

| Measurement | Persistent process | One-shot subprocess |
| --- | ---: | ---: |
| Cold first request, wall time | 1.419 ms | Included in each one-shot sample |
| Warm/sequential median | 0.157 ms | 0.752 ms |
| Warm/sequential p95 / p99 | 0.208 / 0.307 ms | 0.872 / 0.981 ms |
| Sequential throughput | 5,316.2 requests/s | 1,291.6 requests/s |
| Eight-caller throughput | 5,531.0 requests/s | Not measured |
| Eight-caller median / p95 / p99 | 1.381 / 1.715 / 1.924 ms | Not measured |
| Planner resident memory | 3,428 KiB sampled from `/proc` during persistent run | 13,716 KiB `wait4` max-RSS for one process |

The measured one-shot RSS is the kernel maximum for a single process; persistent RSS is sampled from `/proc` and may miss short peaks. The methods are not directly equivalent. Latency and throughput are local loopback-pipe observations for one small fixture, without network transport, real documents, production concurrency, CPU contention controls, warm-up study, or confidence intervals. They must not be used as production capacity or end-user latency claims. The persistent local process outperformed the one-shot process on this fixture, but adds lifecycle supervision and protocol maintenance.

### Alternative comparison

| Alternative | Evidence in this milestone | Operational trade-off and remaining gate |
| --- | --- | --- |
| Persistent Rust child process (evaluated) | Sequential request reuse, concurrent caller correlation, crash/timeout restart, framing limits, and local latency/resource observations are executable. | Current transport is local stdin/stdout and single-flight. No network service, pool, OS isolation, health endpoint, deployment package, service rollout, or production observability. Preferred next evaluation, not approved architecture. |
| Native Rust library / Go FFI | No implementation or benchmark. | Avoids IPC/startup but needs a stable C ABI, generated bindings, ownership/panic rules, per-target packaging, and process-shared failure analysis. Not justified by this evidence. |
| Go-native semantic ownership | No Go port/comparison exists. | Easier deployment with current Go transaction service, but risks duplicated semantic rules unless Rust is replaced as owner and conformance covers the full domain. Not currently recommended over the candidate. |

## Qualification evidence

Reproduce from the repository root:

```sh
./spikes/fg-c3-2-1/run.sh
```

The script builds offline from the spike `Cargo.lock`, formats and runs Rust tests, runs Go tests, compiles the same wrapper for Node-hosted WASM, compares the full fixture responses through one persistent Go→Rust process and WASM, and reports a 200-request local benchmark. Optional benchmark controls are `FG_C3_2_2_BENCH_ITERATIONS` (minimum 20) and `FG_C3_2_2_BENCH_CONCURRENCY`. A Go race-detector run is:

```sh
FG_C3_2_2_RUST_PLANNER="$(realpath ../../../target/fg-c3-2-1/cargo/release/fg-c3-2-1-rust-planner-spike)" \
  go test -race ./...
```

Run the race command from `spikes/fg-c3-2-1/go-coordinator/`.

Observed results:

- Rust unit tests: **5 passed, 0 failed**.
- Go tests: **11 top-level tests, including 6 identity subtests; 17 total test cases passed, 0 failed**.
- Go race detector: **passed**.
- Native persistent Go→Rust vs Node-hosted WASM parity: **12/12 normalized fixture requests equivalent**.
- Negative paths exercised: invalid JSON/envelope/UTF-8, empty/invalid/oversized correlation ID, oversized request/response, truncated header/body, unsupported protocol/semantic/diagnostic versions, planner unavailable/early exit/crash, timeout and subsequent restart, deterministic cycle diagnostics after restart.
- Existing `logos-layout` compilation emitted two unused-import warnings (`Uuid`, `FlexDirection`); they predate this spike and were not changed.
- No hosted CI or real-browser run was performed. Node-hosted WASM is not browser evidence. Full Rust workspace, production Go service, production data, network transport, load test, and storage-backed retry behavior remain unverified.

## Decision and remaining gates

**FG-C3.2.2 result: CONDITIONALLY FEASIBLE.** The persistent local-process boundary is a better candidate than one-shot spawning for further evaluation on this fixture. The single-flight design proves safe serialized reuse and concurrent caller correlation, not parallel serving. Before deployment selection, require a deployment-shaped service/process supervisor and rollout test, resource quotas, production-shaped document corpus, parallelism/pool evaluation, transport/security review, and broader semantic conformance.

The Rust capability response explicitly separates the supported `logos-types` operations from the TypeScript Component/Instance v1 validator. Rust Component/Instance parity remains **NOT VERIFIED**. Durable revision fencing and idempotency remain unimplemented; G1-B10 remains **NO-GO**. No production persistence, authority, schema, React, Go write path, PR, or merge change is authorized by this milestone.
