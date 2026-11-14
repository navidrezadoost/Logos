# G1-B2 — Validation Report

**Baseline:** `c1f8c155b326105459869cd8fd58e349f30f0636`
**Source SHA:** `c315df91ca5a1561c9a475ba47df6a24210cfd1b` (G1-B2.1 final evidence head; Draft PR #94).
**Status:** Candidate contract hardening complete; final-head hosted checks pass. Historical compatibility remains unverified.

## Local checks

| Check | Result |
|---|---|
| Rust domain workspace | Hosted final-head Rust workflow passed root and Rust domain workspaces; see run #43579297700 below. The earlier local count of 458 belongs to the original G1-B2 implementation head. |
| Shared native conformance vectors | `cargo test --manifest-path rust/Cargo.toml -p logos-types --test native_conformance --locked`: 1 passed. The one harness compares all shared vectors. |
| WASM target compile | `cargo check --manifest-path rust/Cargo.toml -p logos-types --target wasm32-unknown-unknown --features wasm`: passed. |
| Generated TypeScript drift | `make generate-rust-types CHECK=1`: passed; all nine generated files matched. |
| Candidate benchmark | See [G1-B2-BENCHMARK-RESULTS.md](G1-B2-BENCHMARK-RESULTS.md); release benchmark completed on x86_64 AMD Ryzen 9 7845HX. |
| Hosted Rust workflow | Passed on final source head `c315df91ca5a1561c9a475ba47df6a24210cfd1b`, including root Rust tests, Rust domain workspace, one Node/WASM test executing the full shared-vector harness, renderer WASM build, and Chromium renderer smokes. [Run #43579297700](https://github.com/navidrezadoost/Logos/actions/runs/43579297700). |
| Hosted generated-types and TypeScript | Passed on final source head: generated type drift and app TypeScript. [Run #43579297640](https://github.com/navidrezadoost/Logos/actions/runs/43579297640). |
| Other PR workflows | Design-token contract [run #43579297610](https://github.com/navidrezadoost/Logos/actions/runs/43579297610) and Ubuntu runner qualification [run #43579297923](https://github.com/navidrezadoost/Logos/actions/runs/43579297923) passed. |
| Local WASM runtime | `wasm-pack test --node rust/logos-types --features wasm`: passed after the test-harness version alignment below; 1 registered test ran the shared vectors. |
| Historical compatibility | UNVERIFIED: no real saved-file corpus or real XD archive was available. |
| Exact-head benchmark | Locked replay and machine-readable raw artifact passed for final source head; benchmark provenance and artifact link are in the Draft PR description. |
| Memory leak / retained heap | NOT MEASURED. |

Local Rust tests used a temporary Cargo patch to the upstream `sha1_smol 1.0.1` source in `/tmp` because this machine lacks its registry archive. That patch is outside the repository; the committed lockfile uses the crates.io source and checksum. Hosted CI will resolve the locked crate normally.

## G1-B2.1 shared conformance vectors

Native and Node/WASM wrappers invoke the same immutable vector suite. It compares complete normalized JSON for valid nested documents and cross-page reparent results; complete serialized diagnostics for invalid graphs, malformed/unsupported schema input, and transform/inverse mismatch; fixed UUIDv5 output; deep Frame/Group membership; and rejection of a delete with an external dependent component reference while preserving the input. Both final-head native and hosted Node/WASM runs passed.

The initial hosted command returned success while discovering zero tests. The cause was a version mismatch: `wasm-bindgen-test 0.3.45` had been locked alongside `wasm-bindgen 0.2.126`. Pinning the harness to upstream `0.3.76` aligned it with `0.2.126`; the hosted workflow's captured output showed `wasm_shared_golden_vectors ... ok`, and CI now requires a positive test count. The earlier zero-test runs are not counted as WASM evidence.

Readiness is separated as follows: Rust domain correctness is an executable candidate; cross-platform conformance is established only for vectors and targets actually executed; production document compatibility remains **BLOCKED** pending executable adapters and real historical fixtures. An opaque `CompatibilityEnvelope` is not an editable document.

## Remaining evidence gates

1. Candidate semantics do not establish production compatibility, document migration safety, renderer fidelity, authorization, revisions, or durable persistence.
2. Historical compatibility remains unverified until authorized representative file archives across versions, real component-rich samples, and XD archives are available.

No Go persistence, existing saved-file format, or production React mutation was changed. No production adoption is implied by this report.
