# G1-B1 — Graph Validation and Evidence

**Status:** Candidate Rust validation boundary; production adoption pending.

## Implementation

`rust/logos-types/src/document.rs` provides a pure immutable validator and candidate typed operations. Diagnostics carry stable enum codes, JSON-pointer-like paths, and optional affected IDs, are sorted deterministically, and do not include source values. JSON is decoded through `decode_and_validate_json`; malformed IDs are detected before typed deserialization. The validator checks uniqueness, reserved NIL IDs, page/root/shape existence, page ownership, parent-child backlinks, duplicate membership, allowed containers, cycles, reachability, local frame/component references, finite geometry/transforms, opacity bounds, and schema version.

The candidate currently defines ordered vectors as the sole sibling-order source. It rejects duplicate entries and operation reorder requests that do not exactly match the expected current list. No runtime numeric benchmark was run; performance and memory are **NOT MEASURED**.

## Conformance evidence

Unit tests cover valid nested graph round-trips, unknown extension preservation, malformed IDs, duplicate child membership, cycles, missing shapes, invalid geometry, non-finite transform/opacity, renderer NIL collision, deterministic diagnostic order, immutable input snapshots, successful and failed typed operations, subtree deletion, stable importer identity, a 2,000-root flat graph, and a 512-level hierarchy. Golden synthetic source fixtures cover current React projection, Rust contract DTO, Go file-data v67, XD Artboard hierarchy, nested Component/Group, valid/invalid graphs, and unknown extensions. Unsupported React/Go/XD payloads are retained in `CompatibilityEnvelope` and rejected as canonical Documents. No historical component fixture is available, so adapter adoption still needs real source data.

## Known limits

- Rust `Shape` presently deserializes many omitted fields with defaults and drops unknown fields when used alone. The candidate `ShapeRecord` extension envelope addresses only its own boundary; it does not retrofit existing DTO readers.
- Current Rust and React Shape types differ substantially. The generated `rust-generated` TypeScript shape is not proof of production React store compatibility.
- Go file data, Transit keyword encoding, file version 67, and historical files have not been round-tripped through this candidate. Synthetic fixture coverage is not historical compatibility proof.
- XD import identity uses a namespaced deterministic, non-cryptographic 128-bit suggestion. Importers must collision-check; source IDs must be stable and duplicate source IDs must be rejected or disambiguated by a documented source path.
- The `Component` container policy differs from the existing Rust helper. External component references cannot be validated without their source document.
- Transform/inverse consistency is maintained by the update operation but the generic validator only checks finiteness, since legacy transforms may not be invertible and the existing DTO has no declared singular-transform rule.
- No React, Go, renderer, storage, or saved-format integration has been made. Go snapshot/restore integrity remains a separate deferred task.

## Test execution

Final local native command: `cargo test --manifest-path rust/Cargo.toml --workspace --offline` — **all workspace test suites passed**. `logos-types` ran **45 unit tests passed; 0 failed** and 2 serialization characterization tests passed. Existing warnings are in `logos-layout` and `logos-vector-ops`; no test failures occurred. The earlier online attempt to fetch `sha1_smol` timed out; that unnecessary UUID-v5 dependency was removed before final tests.

WASM-compatible build: `cargo check --manifest-path rust/Cargo.toml -p logos-types --target wasm32-unknown-unknown --features wasm --offline` — passed. UUID's JS randomness backend was enabled so the existing UUID-v4 API compiles for this target. Hosted CI evidence must still be updated on the final PR head; no performance result is claimed.
