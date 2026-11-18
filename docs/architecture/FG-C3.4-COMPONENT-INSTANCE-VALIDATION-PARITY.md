# FG-C3.4 — Component/Instance Structural Validation Parity

**Status:** Isolated candidate validator; TypeScript remains the semantic reference.
**Baseline:** `05eee774e603d849a274f60783af2be8af9691e6`
**Implementation:** `spikes/fg-c3-2-1/rust-planner/src/component_instance_validator.rs`
**Differential runner:** `test/figma-semantics/component-instance-differential.mjs`

## Contract and comparison policy

The source baseline is `logos-app/src/document-core/componentInstanceDomainV1.ts`. Both validators receive the same JSON values: the generated positive fixture, every case in `component-instance-v1-invalid-cases.json`, and focused adversarial mutations assembled by the differential runner.

The comparison is exact for acceptance, complete diagnostic arrays (code, path, severity, and detail), diagnostic order, and semantic JSON document value. Object key order is ignored in the returned document; array order and every value are compared. Rust emits its `BTreeMap`-based representation deterministically, while TypeScript returns its original input value. `detail` text is **not normalized**. There is no broad compatibility normalization.

On success, the original document value is returned unchanged, including warning-only documents. On any error, both validators return a null document; neither returns a partial accepted document. Rust takes an immutable JSON reference and tests verify the caller value is unchanged. Its text entrypoint maps malformed JSON syntax to `COMPONENT_JSON_INVALID`, distinct from the structured-record diagnostic for parseable but incomplete documents.

## Differential evidence

| Result | Count |
|---|---:|
| Shared inputs executed by both validators | **37** |
| Exact matches | **37** |
| Divergences | **0** |
| Unsupported cases in this JSON corpus | **0** |

The corpus covers the generated positive document; all 14 checked-in negative mutations; parse failure and non-object JSON; wrong version and malformed top-level/component records; multiple simultaneous errors; reversed object insertion order; duplicate component/definition-node identity; malformed reference discriminator and external reference; containment cycle; nonreciprocal set membership; invalid variant tuple and selection; duplicate override and invalid override value; invalid slot-node field and property target; independent opposite instance swaps; direct self-expansion and mutual component expansion; nested slot expansion cycle; advisory slot warning; hard slot rejection; and preferred-instance restrictions.

**Observed TypeScript diagnostic codes:**

`COMPONENT_AMBIGUOUS_SOURCE`, `COMPONENT_DOCUMENT_MALFORMED`, `COMPONENT_DUPLICATE_ID`, `COMPONENT_ID_MISMATCH`, `COMPONENT_JSON_INVALID`, `COMPONENT_NODE_GRAPH_INVALID`, `COMPONENT_OVERRIDE_FIELD_INVALID`, `COMPONENT_OVERRIDE_INVALID`, `COMPONENT_OVERRIDE_TARGET_MISSING`, `COMPONENT_PROPERTY_INVALID`, `COMPONENT_PROPERTY_KIND_UNSUPPORTED`, `COMPONENT_PROPERTY_VALUE_INVALID`, `COMPONENT_RECORD_MALFORMED`, `COMPONENT_REFERENCE_CYCLE`, `COMPONENT_REFERENCE_INVALID`, `COMPONENT_REFERENCE_MISSING`, `COMPONENT_REFERENCE_UNRESOLVED`, `COMPONENT_SET_MEMBERSHIP_INVALID`, `COMPONENT_SLOT_CONTENT_NOT_ALLOWED`, `COMPONENT_SLOT_GUIDANCE_EXCEEDED`, `COMPONENT_SLOT_HARD_LIMIT_EXCEEDED`, `COMPONENT_SLOT_INVALID`, `COMPONENT_UNKNOWN_FIELD`, `COMPONENT_VARIANT_DUPLICATE`, `COMPONENT_VARIANT_INVALID`, `COMPONENT_VARIANT_SELECTION_INVALID`, and `COMPONENT_VERSION_UNSUPPORTED`.

All observed codes matched exactly in the corpus. The TypeScript diagnostic union also declares `COMPONENT_NODE_KIND_UNSUPPORTED`, but the current TypeScript source has no emission site for that code; it is therefore not an executable parity target. It is retained as an explicit source-contract gap, not counted as a Rust/TypeScript mismatch.

**Mismatch register:** none for the 37 executed JSON cases. No cases were suppressed or normalized to obtain this result.

## Implementation boundary

The Rust validator consumes raw `serde_json::Value` rather than deserializing the strict FG-C3.3 DTO first. This allows unknown fields and malformed records to receive semantic diagnostics instead of being converted into generic serde failures. The FG-C3.3 DTO remains available for typed representation and lossless-or-reject round trips.

Implemented rules include version and record checks; stable ID/key and duplicate identity checks; component and authored slot node graph membership, parent/child reciprocity, ordered roots, unreachable nodes, and containment cycles; component-set membership and variant tuple checks; property kinds and target compatibility; source-reference and selection checks; typed values and overrides; slot advisory/hard limits and preferred-instance restrictions; and contextual expansion cycles that preserve the TypeScript v1 behavior for nested instance-specific state.

The text validator accepts JSON syntax only. JavaScript-only input states that cannot be represented in JSON—cyclic object references, non-plain prototypes, `undefined`, `NaN`, and infinities—cannot cross this boundary and are not included in the 37-case count. TypeScript's guard rejects those values when called directly. No production Go/Rust handshake capability, canonical Document field, saved-file format, resolver, or persistence path was changed.

## Reproduction

From the repository root:

```sh
cargo fmt --manifest-path spikes/fg-c3-2-1/rust-planner/Cargo.toml -- --check
cargo test --manifest-path spikes/fg-c3-2-1/rust-planner/Cargo.toml
node --test test/figma-semantics/component-instance-domain-v1.test.mjs
node test/figma-semantics/component-instance-differential.mjs
```

The Rust crate test run passes **11 tests** (including existing planner tests). The TypeScript Component/Instance suite passes **13 tests**. The direct differential runner reports **37 matched, 0 divergent, 0 unsupported**. Rust emits two existing unused-import warnings from `rust/logos-layout/src/flex/{bounds,positions}.rs`; this milestone does not modify those files. These are local results; no hosted workflow or production document corpus was run.

## Qualification limits and status

This is a source-level differential result for a deterministic generated and adversarial JSON corpus, not proof of all possible v1 inputs, production Figma file compatibility, production Logos document compatibility, or Rust authority. The TS validator remains the reference. Saved-file migration, persistent component ownership, Go integration, production resolution, and component rendering remain outside scope and unverified. G1-B9.6 retained memory remains **NOT MEASURED**; G1-B10 authority transfer remains **NO-GO**.
