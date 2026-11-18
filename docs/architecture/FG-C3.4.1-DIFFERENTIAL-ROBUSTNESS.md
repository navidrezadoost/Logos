# FG-C3.4.1 — Component/Instance Differential Robustness

**Status:** Completed local robustness qualification; Rust remains non-authoritative.
**Branch:** `core/g1-b9-6-memory-evidence-foundation`
**Starting commit:** `d8d5fd88f69a4aebf45a05ccede838981b673a02`
**Harness:** `test/figma-semantics/component-instance-differential.mjs`

## Corpus and result

The deterministic harness uses generator `fg-c3.4.1-seeded-v1`, seed `205259777` (`0x0c3c0401`), a xorshift32 PRNG, reference depth bounded to 8, and a fixed maximum of 48 generated samples. Each sample contributes one valid document and one controlled invalid reference mutation. Generated documents retain the representative fixture's component set, variant tuple, typed properties, overrides, slots, and external and unresolved references. Additional cases exercise bounded nested references, varying instance counts, malformed reference data, and larger documents.

| Corpus portion | Cases |
|---|---:|
| FG-C3.4 fixed corpus | 37 |
| Seeded valid documents | 48 |
| Seeded controlled-invalid mutations | 48 |
| Runtime and JSON boundary cases | 10 |
| **Total** | **143** |

Latest local differential run: **143 matched, 0 divergent, 0 unsupported**. Comparison includes acceptance, full diagnostic code/path/severity/detail and order, and returned semantic JSON. No diagnostic or result normalization is used by the comparator. TypeScript input-purity checks run for every parsed input. On any mismatch, the harness writes the complete original failing case and both results to `test/fixtures/figma-semantics/fg-c3-4-1-failures/`; for object inputs it also runs a deterministic greedy top-level record reducer and stores the reduced input. Any such generated failure fixture is retained for reproduction and must be reviewed before regeneration or deletion.

## Cross-runtime JSON compatibility findings

| Boundary | Qualified behavior |
|---|---|
| Unicode ordering | TypeScript string sort compares UTF-16 code units. Rust now uses the same ordering for validation traversal and final diagnostic sorting. The adversarial U+10000/U+E000 case confirms diagnostic parity. Unicode string values and keys are otherwise preserved without normalization. |
| Number precision | The wire contract is JSON parsed by each runtime, with JavaScript `Number` semantics. Rust converts integer JSON values outside ±(2^53−1) to their IEEE-754 representation before semantic validation/return, matching `JSON.parse` rounding. A `9007199254740993` wire integer therefore becomes `9007199254740992` in both semantic results. |
| Floating point | JSON `1.0`, negative zero, and finite exponent values are compared after runtime JSON parsing. Non-finite JavaScript numbers cannot be represented by valid JSON and are outside this shared text boundary. |
| Duplicate object member names | The JSON-text case confirms last-member-wins behavior for the tested duplicate `version` member in `JSON.parse` and `serde_json`. The parsed semantic object cannot retain evidence that the duplicate occurred. Inputs requiring duplicate-member rejection need a separate parser policy decision; current parity preserves runtime parser behavior. |
| Optional fields | Explicit `null` and absence are distinct. `setId: null` is accepted where the v1 schema permits it; an absent required `setId` rejects. |
| Empty/unusual keys | Empty IDs, `__proto__`, `constructor`, and Unicode keys are exercised. JSON object keys are treated as own string keys; the test constructs `__proto__` as an own property rather than mutating the JavaScript prototype. |
| Ordering | Object insertion order does not affect semantics or diagnostics. Authored arrays, including roots, children, set members, override lists, and slot roots, retain their semantic order. |
| Depth and envelope | Containment depth is bounded at 48; generated reference depth is at most 8; the largest explicit instance envelope adds 256 instances. This is a finite stress sample, not a resource-limit guarantee. |
| Malformed text | Malformed JSON text maps to `COMPONENT_JSON_INVALID`; valid JSON with invalid structure receives semantic v1 diagnostics. The APIs differ in their input types, so JavaScript-only values (cycles, `undefined`, non-plain objects, `NaN`, and infinities) are not JSON differential cases. |

The Rust change only adapts its isolated validator to the existing TypeScript v1 behavior. TypeScript semantics and production behavior were not changed. The Rust entrypoint returns JavaScript-compatible rounded numeric values for unsafe JSON integers; this is an explicit wire normalization rather than a claim that the original decimal spelling is preserved.

## Mismatch register and implementation changes

The earlier fixed corpus had no observed discrepancies. Boundary analysis identified two implementation gaps before the expanded run: Rust's Unicode scalar ordering could diverge from JavaScript UTF-16 ordering, and serde's exact integer representation could diverge from JavaScript number rounding. The Rust-only validator now applies UTF-16 ordering and unsafe-integer normalization. The complete 143-case run after these corrections has no remaining observed mismatch. This is corpus evidence, not exhaustive equivalence.

The mismatch register is therefore: **no unresolved observed divergences**. This does not establish parser equivalence for every duplicate-key arrangement, full Unicode/number domain equivalence, or protection from resource exhaustion at arbitrary document sizes.

## Isolation and readiness recommendation

No Go planner handshake, protocol capability, canonical ownership, persistence, saved-file format, React mutation, rendering, or production resolver was changed. Rust remains an isolated candidate and TypeScript remains the semantic reference.

**Recommendation: NO-GO for isolated protocol exposure at this stage.** The validator has exact agreement on this bounded corpus, but the planner protocol/capability handshake and deployed rejection behavior have not been qualified, and duplicate-key treatment remains parser-defined rather than an explicit cross-runtime rejection policy. Keep capability advertisement disabled pending a separate approved protocol-boundary qualification. Production authority and integration remain unauthorized.

## Reproduction

From the repository root:

```sh
cargo fmt --manifest-path spikes/fg-c3-2-1/rust-planner/Cargo.toml -- --check
cargo test --manifest-path spikes/fg-c3-2-1/rust-planner/Cargo.toml
node --test test/figma-semantics/component-instance-domain-v1.test.mjs
node test/figma-semantics/component-instance-differential.mjs
```

Results recorded for this work are local only. They are not hosted CI, Chromium, production-document, or performance evidence. G1-B9.6 retained memory remains **NOT MEASURED**; G1-B10 authority transfer remains **NO-GO**.
