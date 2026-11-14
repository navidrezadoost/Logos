# G1-B2 — Validation Report

**Baseline:** `c1f8c155b326105459869cd8fd58e349f30f0636`
**Source SHA:** Recorded in the Draft PR description; this report applies to that head.
**Status:** Candidate contract hardening complete; hosted checks pass. Historical compatibility remains unverified.

## Local checks

| Check | Result |
|---|---|
| Rust domain workspace | `cargo test --manifest-path rust/Cargo.toml --workspace`: 458 passed, 0 failed. |
| `logos-types` focused suite | 53 unit tests and 2 serialization characterization tests passed; WASM integration test is target-gated and did not run natively. |
| WASM target compile | `cargo check --manifest-path rust/Cargo.toml -p logos-types --target wasm32-unknown-unknown --features wasm`: passed. |
| Generated TypeScript drift | `make generate-rust-types CHECK=1`: passed; all nine generated files matched. |
| Candidate benchmark | See [G1-B2-BENCHMARK-RESULTS.md](G1-B2-BENCHMARK-RESULTS.md); release benchmark completed on x86_64 AMD Ryzen 9 7845HX. |
| Hosted Rust workflow | Passed on source head `1e54f185eca48449c81a87a276046db2281605b0`, including root Rust tests, Rust domain workspace, Node/WASM candidate conformance (3 tests), renderer WASM build, and renderer browser smoke. [Run #333](https://github.com/navidrezadoost/Logos/actions/runs/43571833299). |
| Hosted generated-types and TypeScript | Passed on source head `1e54f185eca48449c81a87a276046db2281605b0`: generated type drift and app TypeScript checks. [Run #30](https://github.com/navidrezadoost/Logos/actions/runs/43571833315). |
| Other PR workflows | Design token contract and Ubuntu runner qualification passed: [contract run #20](https://github.com/navidrezadoost/Logos/actions/runs/43571833325), [runner qualification #31](https://github.com/navidrezadoost/Logos/actions/runs/43571833301). |
| Local WASM runtime | `wasm-pack test --node rust/logos-types --features wasm` could not start locally because the offline Cargo cache lacks `console_error_panic_hook 0.1.7` (and related `wasm-bindgen-test` dependencies). Hosted Node/WASM tests passed. |
| Historical compatibility | UNVERIFIED: no real saved-file corpus or real XD archive was available. |
| Memory leak / retained heap | NOT MEASURED. |

Local Rust tests used a temporary Cargo patch to the upstream `sha1_smol 1.0.1` source in `/tmp` because this machine lacks its registry archive. That patch is outside the repository; the committed lockfile uses the crates.io source and checksum. Hosted CI will resolve the locked crate normally.

## Remaining evidence gates

1. The listed hosted checks passed on code head `1e54f185eca48449c81a87a276046db2281605b0`; the later evidence-only report update does not change tested code.
2. Candidate semantics do not establish production compatibility, document migration safety, renderer fidelity, authorization, revisions, or durable persistence.
3. Historical compatibility remains unverified until authorized representative file archives across versions, real component-rich samples, and XD archives are available.

No Go persistence, existing saved-file format, or production React mutation was changed. No production adoption is implied by this report.
