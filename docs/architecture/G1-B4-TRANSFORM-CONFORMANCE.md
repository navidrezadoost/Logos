# G1-B4 — Transform Conformance and Integration Report

**Status:** Pure affine helpers and shared native/WASM vectors implemented; editor adapter promotion remains blocked on source transform evidence.

**Tolerance:** f64 geometry `1e-6`; no rounding; raster comparisons remain an independent renderer-specific gate.

## Shared golden cases

`rust/logos-types/tests/fixtures/g1-b4-coordinate-golden-v1.json` is consumed by native Rust and browser-target WASM tests. It includes the required named cases:

1. Root rectangle.
2. Rectangle under translated Frame.
3. Nested Frame in Frame.
4. Group under translated Frame.
5. Rotated parent.
6. Scaled parent.
7. Combined rotation and translation.
8. Negative coordinates.
9. Reparent while preserving world transform.
10. Singular transform rejection.
11. XD artboard-to-Frame coordinate conversion.
12. Deep transform composition.

Each non-singular case records its local-to-parent chain, source point, expected world point, composed matrix, and expected inverse. Reparent fixtures additionally record destination-parent and expected new-local matrices. Singular input records an explicit failure. Tests assert f64 results against the fixture tolerance.

## Implemented helpers

`logos-types::document_geometry` composes root-to-leaf `logos-layout::Matrix` values; maps points both directions; transforms all four rectangle corners; converts object transforms to a new parent; and performs explicit world-preserving reparent conversion. Non-finite values and singular inverses fail closed. The helper module does not independently validate document graph structure.

## Adapter and product boundaries

The test-only read-only adapter now converts page-positioned React bounds into parent-local bounds for a strictly translated hierarchy. It subtracts the immediate parent's page-space origin and reconstructs the original page position after Rust validation. The Chromium harness proves exact round-trip for one nested Frame → Group → Rectangle fixture with identity parent matrices. Parent rotation, scale, skew, or any non-identity affine parent matrix remains rejected as `UNVERIFIED_COORDINATE_SPACE`; React importer transform arrays may independently carry translation (XD and Sketch), so composing them without source-specific pivot evidence can double-apply translation. The A8 renderer fixture confirms current nested page-position behavior for its concrete untransformed fixture; it does not cover transformed ancestry, importer matrices, frame clipping, or React-vs-Rust pixel equality for those cases.

The adapter remains unconnected to production mutations or persistence. Missing or mixed coordinate provenance remains a stable fail-closed diagnostic.

## Performance and lifecycle

Local browser measurement (2026-11-14) used HeadlessChrome 156.0.0.0 on Linux x86_64, 1280×720, DPR 1. Each record count had one warmup and five measured runs. Fixtures at 100/1,000/10,000 are flat root rectangles; the separate translated nested fixture contains four shapes. Timing values are p50/p95 milliseconds. “WASM validation boundary” measures the synchronous JSON-in/JSON-out call, including Rust parse/validation/serialization and JS/WASM boundary overhead; the Rust core phases are not separately observable through this ABI. For this local browser run, the harness used the previously generated G1-B3 validator binding because the installed `wasm-bindgen` CLI is 0.2.108 while the Rust binding is 0.2.126; the current-source WASM conformance suite passed separately, and hosted exact-head CI must regenerate the browser binding.

| Records | Mapping | Geometry conversion | JSON serialization | WASM validation boundary | Response parse | Result materialization | Total |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 4 | 0.00 / 0.10 | 0.00 / 0.00 | 0.00 / 0.00 | 0.20 / 0.20 | 0.00 / 0.00 | 0.00 / 0.00 | 0.20 / 0.30 |
| 100 | 0.90 / 2.00 | 0.00 / 0.00 | 0.10 / 0.40 | 1.50 / 2.30 | 0.10 / 0.50 | 0.10 / 0.20 | 3.10 / 4.10 |
| 1,000 | 2.70 / 2.90 | 0.10 / 0.20 | 0.60 / 0.70 | 10.10 / 11.30 | 1.20 / 1.60 | 0.40 / 0.60 | 15.40 / 16.60 |
| 10,000 | 17.20 / 36.30 | 1.50 / 1.80 | 9.60 / 12.90 | 135.50 / 140.80 | 10.50 / 14.30 | 3.10 / 4.00 | 189.60 / 196.40 |

The flat-root size run does not measure deep-ancestry scaling. Parent-space qualification now memoizes the ancestry result per shape, and result projection caches each accumulated parent origin; the 4-shape nested browser fixture verifies correctness but is too small for scaling conclusions. Five rejected Rust backlink validations took p50 0.20 ms / p95 3.50 ms.

WASM linear memory reserved after the 10,000-record sample was 61,407,232 bytes. Observed JS heap after that sample was 81,566,982 bytes. Five React mount/unmount validation cycles observed 1,976,841 bytes before and 2,102,213 bytes after, without forced GC. Five rejected backlink validations had p50 0.20 ms / p95 3.30 ms. These are observations, not retained-resource/leak evidence. A controlled repeated-cycle retention study remains outstanding.

## Validation on this branch

- `cargo test --manifest-path rust/Cargo.toml -p logos-types --locked`: 64 tests passed, 0 failed, including the 12 shared native geometry vectors.
- `wasm-pack test --node rust/logos-types --features wasm`: 4 tests executed and passed, including the same 12 shared vectors and Rust document ABI cases.
- React adapter TypeScript check and isolated Vite smoke build passed.
- Headless Chromium harness passed the supported round trip, translated nested conversion (`parent local 5,5` ↔ page `125,75`), rotated-parent fail-closed diagnostic, invalid Rust graph diagnostic, no Zustand/storage writes, and five lifecycle cycles.
- The local Chromium harness used the previously generated G1-B3 validator binding because the local `wasm-bindgen` CLI is 0.2.108 while the project binding is 0.2.126 and network access to install 0.2.126 timed out. The Rust source's new WASM conformance tests were separately executed against current source with `wasm-pack test --node`. Hosted exact-head CI must regenerate and execute the browser artifact before the PR is considered qualified.

## Unresolved acceptance risks

- Which creation/import boundary issues a stable offline document UUID and replaces ephemeral initial page identity is an ownership decision; no production migration is included.
- React's shape `transform` plus separate `rotation` representation does not yet have complete round-trip/pivot fixtures for React-created, XD-imported, and Sketch-imported nested shapes.
- Production Canvas does not expose a demonstrated reparent operation or an explicit “preserve world position” choice.
- Existing saved-file historical identity and geometry compatibility remain unverified; no file was rewritten.
