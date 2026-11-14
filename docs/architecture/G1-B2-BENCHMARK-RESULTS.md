# G1-B2 — Candidate Graph Benchmark Results

**Status:** Local microbenchmark; runtime/retained-memory measurements are not implied.
**Commit:** Run before final commit; final head SHA will be recorded in the PR description and CI evidence.

## Environment

| Field | Value |
|---|---|
| Command | `cargo run --manifest-path rust/Cargo.toml -p logos-types --release --example document_core_bench` |
| Compiler | `rustc 1.93.0 (254b59607 2026-01-19)` |
| Profile | Cargo `release` (`opt-level=3`, fat LTO, one codegen unit, panic abort) |
| Target | `x86_64-unknown-linux-gnu` |
| CPU | AMD Ryzen 9 7845HX with Radeon Graphics |
| Sampling | 31 timed post-warmup calls per case; reported median and nearest-rank p95 |
| Allocation accounting | Global counting allocator; one additional untimed call, counting requested allocation count and bytes |

The local Cargo cache lacked the `sha1_smol` crate archive required by UUIDv5. Local execution used the upstream `sha1_smol` 1.0.1 source in `/tmp` via a temporary Cargo patch; this patch is not part of the branch. `Cargo.lock` records the registry source and checksum. The output below is from the same locked crate version, but a clean local replay still requires the crate archive or network access.

## Fixture hashes

SHA-256 is over the deterministic Rust `Debug` representation of the generated fixture. All objects are built in ordered vectors, and generated shape plugin maps and extension maps are empty.

| Fixture | Objects | SHA-256 |
|---|---:|---|
| Flat 100 | 102 | `2801b9a5e70a454905c1488c4eff5b8fccdd773ca1dd828750dd5c04ec6553ee` |
| Flat 1,000 | 1,002 | `ebf098bfb97fcb57cc4da6ae4e9920893345a1b944b3f164c3eeaab462ae3dfe` |
| Flat 10,000 | 10,002 | `339928b61602398261c20673f75b58077e643b49f0e41be6d09042588426b1f9` |
| Deep 512 | 513 | `71ee0c4930ac7eaa9442d54ea6145afda0da5aa6ce57d19b6d370d87e00fc3b1` |
| Deep 5,000 | 5,001 | `754e917e42e7e857f86660f63e9935c69742c5cc4d36654e60aaad6832dec9df` |

## Results

Times are milliseconds. Allocation counts and requested bytes are for one validator invocation or one full two-operation cycle as labeled.

| Case | p50 | p95 | Allocations | Allocated bytes |
|---|---:|---:|---:|---:|
| Validate flat 100 | 0.040 | 0.045 | 1,058 | 83,865 |
| Reparent cycle flat 100 | 0.194 | 0.213 | 4,468 | 507,734 |
| Create/delete cycle flat 100 | 0.174 | 0.187 | 4,471 | 629,694 |
| Malformed validation flat 100 | 0.039 | 0.043 | 1,060 | 84,167 |
| Validate flat 1,000 | 0.420 | 0.428 | 10,080 | 1,069,115 |
| Reparent cycle flat 1,000 | 1.904 | 1.972 | 42,356 | 6,096,934 |
| Create/delete cycle flat 1,000 | 1.847 | 1.869 | 42,359 | 7,169,364 |
| Malformed validation flat 1,000 | 0.418 | 0.427 | 10,082 | 1,069,417 |
| Validate flat 10,000 | 4.498 | 4.572 | 100,095 | 9,609,409 |
| Reparent cycle flat 10,000 | 22.398 | 23.211 | 420,416 | 55,888,910 |
| Create/delete cycle flat 10,000 | 21.400 | 23.683 | 420,419 | 67,194,946 |
| Malformed validation flat 10,000 | 4.528 | 4.651 | 100,097 | 9,609,711 |
| Validate deep 512 | 0.231 | 0.236 | 6,202 | 542,168 |
| Validate deep 5,000 | 2.416 | 2.461 | 60,070 | 4,891,595 |

## Interpretation and limits

The initial indexed-map implementation still recomputed each shape's nearest frame by walking its full ancestor chain. The first measured candidate took 5.25 ms at 513 deep objects and 543.17 ms at 5,001. The memoized iterative frame-membership pass reduced those to 0.231 ms and 2.416 ms, respectively, on the same fixture hashes and machine. Flat validation increased slightly because the memoization map allocates per shape; latency remains approximately proportional to shape count in this fixture. The before/after comparison is diagnostic evidence from this host, not a cross-machine performance guarantee.

The repeated operations clone the complete candidate and validate both the source and result; their costs are therefore higher than graph traversal alone. Allocation counts are requested allocation volume, not retained heap, peak RSS, leak detection, or browser/WASM memory. **Memory leak status: NOT MEASURED.**
