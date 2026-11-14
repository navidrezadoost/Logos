# G0-B Core Correctness and Performance Baseline

**Status:** No memory or editor-runtime benchmark was run by this audit. **Performance: NOT MEASURED.** CI build/test success is not a memory result.

## What the current infrastructure establishes

- Rust unit tests exist in `logos-types`, `logos-layout`, `logos-tokens`, `logos-vector`, and WASM bridge crates. `logos-types` has a P0 shape fixture test; `logos-tokens` has contract fixtures and alias/mode/plan tests.
- `logos-layout` has unit tests for Flex sizing/wrap and Grid track/position behavior. They are not a shared exact fixture suite across pure layout and `render-wasm`.
- A8 has invalid graph/recovery and real Chromium pixel tests for an isolated projection API. A8 architecture documentation explicitly says production Canvas uses the legacy route, so this does not characterize full app documents.
- Go has unit and DB integration tests for file updates, malformed history, snapshot, compatibility and persistence paths. They do not establish safe behavior for all historical rows or all race/retry interleavings.
- Rust generated TS drift workflow checks emitted files. It does not check semantic parity with handwritten React projection or legacy Transit shape types.
- Existing P0.5 readiness/operational workflows are infrastructure checks; not benchmark execution.

G0-B adds three focused characterizations: React shape JSON is rejected by the Rust Shape DTO; unknown JSON fields are silently dropped through Rust deserialize/serialize; identical Flex input returns identical exact geometry. Added fixture JSON lists invalid graph cases but is explicitly not a validator test. The existing A8 projection tests remain the executable graph-boundary evidence.

## Correctness gaps to cover next

| Domain | Current evidence | Required next executable cases |
|---|---|---|
| Serialization | Rust color/token and P0 shape tests; Go legacy fixtures. | Full representative document round trip through each adapter; direct Rust↔React↔legacy fixture comparison; unknown field preserve/reject policy; version upgrade/downgrade; defaults and explicit nulls. |
| Graph | A8 projection rejects many malformed page graphs. | Canonical domain validator: duplicate/case-colliding IDs, page ownership, dangling parent/child, duplicate membership, cycle, orphan, root rules, reparent/delete closure, unchanged snapshot on rejection. |
| Geometry | Point/Rect/Matrix unit tests and renderer tests. | NaN/infinity/singular transform, inverse consistency, extreme coordinate, negative/zero dimension policy, vector segment references, tolerance and roundoff. |
| Layout | Pure Flex/Grid tests; renderer implementations separate. | Golden fixtures for row/column, wrap, fixed/fill/hug, min/max, ordering, gaps/padding, align/justify, nested, absolute, text metrics and invalidation. Differential comparison between pure Rust and actual renderer paths. |
| Tokens/references | Rust resolver and plan validation fixtures; DT-C3 browser workflow on non-main PR. | Cross-mode, unresolved/cyclic alias, fallback, binding type, delete/rename, migration and document serialization; Rust/WASM ABI vectors. |
| Operations/replay | Go rebase tests and integration tests; Rust rebase crate. | Cross-language operation vectors, stable order/idempotency, gap/malformed history, snapshot↔replay hash, undo inverse, duplicate retry, concurrency schedules. |
| Adversarial/import | A8 graph diagnostics, XD typed converter. | Huge/deep trees, duplicate source IDs, unsupported paths/effects, malformed archives, deterministic outputs, warning/loss completeness and fuzz/property cases. |

## Measurement plan (future approved job)

Measure separately and label each resource domain:

1. **Rust heap allocations:** allocator instrumentation around deserialize, graph validation, operation apply, layout and token resolve; report allocations/bytes/op and peak live bytes.
2. **WASM linear memory:** `memory.buffer.byteLength`, heap high-water mark, free-list/allocator state and memory growth; distinguish reserved pages from live Rust objects.
3. **JavaScript heap:** browser heap snapshot before/after warm-up and deterministic edit/revert/undo loops; report retained-object delta after GC opportunity and repeat.
4. **Renderer/GPU resources:** pool generations, image/font references, texture/surface counts and explicit destruction events; do not infer from JS heap.
5. **Serialization and projection:** bytes, elapsed distribution and peak memory for versioned fixture sizes, including nested graph and missing references.
6. **Layout:** cold/warm full compute and incremental invalidation cost on fixed fixtures; same outputs must be checked before timing.

Use fixed fixture files, browser/version/OS/CPU metadata, warm-up count, iteration count, timeout/failure policy, immutable commit SHA, artifact hashes, raw snapshots and machine-readable results. A failed/missing/unsupported run is **NOT MEASURED**, never pass. Do not publish invented numbers or “zero leaks” from tests/compilation.

## Measured status

No G0-B memory number, latency number or leak-free conclusion exists. **NOT MEASURED** remains the correct status.
