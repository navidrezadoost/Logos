# FG-C3.6.1 — Semantic Resource Bounding

**Scope:** isolated Rust/Go protocol spike only. The `validateComponentInstanceV1` capability remains unadvertised; this work does not grant production authority.

## Execution-path audit

The coordinator first bounds the UTF-8 request size, then the Rust strict JSON parser rejects duplicate decoded keys and enforces JSON nesting and value-count limits. The parsed document is treated as immutable input. The bounded validator charges an input-tree pass before cloning/normalizing and charges semantic operations as it runs.

| Work | Main cost shape | Guard |
| --- | --- | --- |
| Strict JSON parse and duplicate-key tracking | O(input bytes + JSON values), with bounded nesting | Request byte limit, parser depth, parser value count |
| Number normalization, structural cloning, and returned document copy | O(JSON values + bytes) | Precharged input traversal; parser input caps |
| Component, set, and instance enumeration | O(record entries), with deterministic JS-order key sorting | Weighted key-sort work and total semantic budget |
| Definition/slot containment, reciprocity, reachability | O(nodes + edges) for each graph walk; recursive depth bounded | Input-tree charge, graph traversal work charge, semantic depth 64 |
| Variant tuple validation and local-set resolution | O(members × properties) in the worst case | Product precharge before candidate scans |
| Nested component/slot expansion and cycle detection | O(expanded nodes/properties), potentially repeated by instances | Expansion-step budget 100,000 and depth 64, also charged to total work |
| Property, override, and slot-content checks | O(input entries), with repeated resolution and membership scans | Input-tree charge, key/list charges, diagnostic and total-work limits |
| Diagnostic construction and sorting | O(diagnostic bytes + d log d) | 512 diagnostics, 768 KiB, charged path/detail bytes and sorting work |
| Response encoding/framing | O(response bytes) | Existing bounded response limit and Go response validation |

The semantic work counter is deterministic accounting, not a CPU-time or memory quota. Sorting charges use `n × ceil(log₂(n+1))`; recognized member/property cross-products are precharged before scanning. The 5,000,000 work ceiling bounds the covered operations for an input admitted by parser limits and retains the previously qualified 100,000 expansion-step boundary. The Go coordinator's deadline remains the outer recovery mechanism for unexpectedly slow execution. No OS sandbox, RSS quota, or child-process CPU quota is established here.

## Failure behavior

- Semantic work exhaustion stops the current precharged operation and propagates as `ValidationLimit::SemanticWork`; the protocol emits one correlated `COMPONENT_RESOURCE_LIMIT` error, without partial diagnostics or a validation result.
- Expansion depth/step exhaustion uses the same resource error class. Expansion steps also consume total semantic work.
- Diagnostic count or byte exhaustion sets a sticky request flag. Budget-aware loops and helpers stop at their next checkpoint; the partial list is discarded and one correlated `COMPONENT_DIAGNOSTIC_LIMIT` error is returned.
- If semantic work and diagnostic limits are observed together, semantic resource exhaustion has deterministic priority. If parsing fails before semantic validation, parser/protocol errors retain priority.
- The request-local thread state is reset by a guard on normal return and panic unwinding. Tests verify a request after rejection is reusable and receives a fresh budget.
- Rust panic containment still belongs to the existing worker boundary. This change does not claim that an OS-level fault or process kill is recoverable in-process.

## Adversarial qualification

Rust tests cover a deliberately low total-work cap, high-cardinality member/property matching precharged before scanning, diagnostic overflow short-circuiting, stable simultaneous-limit priority, and same-process budget reset. Existing protocol tests retain depth and expansion boundaries, warning/rejection distinctions, exact diagnostic limits, and response-size behavior. Go FG-C3.6 tests cover process cancellation, timeout, reuse/restart, and framed transport behavior. The FG-C3.5 differential corpus and TypeScript v1 contract remain the semantic reference.

The validator uses owned JSON cloning, ordered key vectors, maps/sets, diagnostic strings, and temporary match collections. Input bytes/value count, diagnostic bytes/count, bounded graph recursion, and semantic/expansion work are capped. This milestone does not add a heap allocator budget or report per-request peak bytes; memory safety remains an explicit operational limitation.

## Local evidence and readiness

Final-head local verification:

| Check | Result |
| --- | --- |
| Rust planner crate tests | 18 passed, 0 failed |
| Rust planner crate format check | Passed |
| Go coordinator suite | Passed, `go test ./... -count=1` |
| Go race detector | Passed, `go test -race ./... -count=1` |
| TypeScript Component/Instance suite | 13 passed, 0 failed |
| FG-C3.5 protocol contract tests | 2 passed, 0 failed |
| FG-C3.5 differential corpus | 339 cases; 338 matched, 1 JSON-unsupported JavaScript `NaN`, 0 divergences |
| Native Rust/Node WASM parity | 4/4 matched |
| Hosted CI / production | Not run; not authorized |

Deterministic counters and local observations: the shared positive fixture consumed **2,634 semantic-work units**, completed in **409 μs** in one local debug test run, and passed at exactly 2,634 while 2,633 rejected. The 40-member × 40-property candidate-search fixture attempted 1,600 comparison units, was rejected before scanning at a configured cap of 500, and recorded exactly 500 units. A 1 ms `/proc` RSS sampler observed **5,008 KiB peak for the Rust test process** while running the positive-fixture case. That process high-water sample includes the test harness/runtime and is not a per-request heap measurement or a memory ceiling. No allocation-level or adversarial peak-memory bound was measured.

Hosted CI, production execution, and process sandbox qualification remain outside this milestone. Recommendation: **NO-GO for capability advertisement** until the deployment owner approves runtime isolation, observability, and operating-system resource controls. The protocol remains isolated, unadvertised, non-authoritative, and validation-only. G1-B9.6 memory qualification remains **NOT MEASURED**.
