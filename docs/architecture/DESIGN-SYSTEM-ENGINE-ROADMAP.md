# Design System Engine Roadmap — P0 Baseline

**Baseline:** `964676b90398f4458a72e3d41fd87e0dc91f51f9`
**Interoperability target:** DTCG 2025.10, as specified by the CTO directive. This document does not claim conformance.

## Existing assets

| Layer | Source | State and boundary |
|---|---|---|
| Rust model | `rust/logos-types/src/token.rs` | Defines primitive/semantic tokens, values, aliases by UUID, collections, modes, mode strategies, source/scope, and variable bindings. Schema version constant is currently `1`. Rust-generated TypeScript exists. |
| Rust resolution | `rust/logos-tokens/src/variable_resolver.rs` | Resolves aliases, mode inheritance, scope, bounded alias depth, and binding fallback. Local locked Rust tests passed for aliases, cycles, deleted/missing values, mode fallback/inheritance, and scope violations. |
| React import projection | `logos-app/src/migration/tokens/token-types.ts`, `stores/tokenStore.ts` | Simpler token/set/theme records with a small type union and string aliases such as `{token.name}`. Zustand persists to a browser key, not a document-scoped server contract. No bridge to Rust’s UUID registry/binding model is shown. |
| Import sources | Sketch/XD token converters and `ImportMigrationDialog.tsx` | Convert some swatches, shared styles, resource values, and themes. The dialog imports token data but not converted shape trees. No repeatable DTCG fixture suite was found. |
| Shape binding | Rust `VariableBinding`; React `Shape` and React renderer projection | Rust has binding values, but the React shape model/render path does not demonstrate a document binding-to-resolved-value lifecycle. |

## Target architecture (proposed)

```text
DTCG interchange adapters
      ↕ versioned, loss-reporting boundary
Rust token registry (identity, type, scope, collection, mode, binding)
      ↓ alias/mode resolution + dependency graph
Canonical document transaction + validation
      ↓
Canvas · components · libraries · Dev Mode · export · AI semantic reads
```

Rust remains the candidate token semantic owner. The editor may keep a React projection/cache, but it must not invent a parallel token identity or alias system. Imports/exports should preserve unknown extension data where safe and report conversions/losses.

## P0 dependency map and gaps

1. **Contract reconciliation (G2):** map DTCG 2025.10 `$type`, `$value`, group, alias/reference, metadata, extension, and mode semantics to existing Rust types. Record exactly which schema version is accepted and emitted. Do not assume the current schema version `1` is DTCG-compatible.
2. **Resolution semantics (G3):** freeze alias cycle/missing-token behavior, type compatibility, scope, nested collection/mode fallback, and explicit/inherited mode rules. Reuse Rust resolver tests and add conformance fixtures.
3. **Document binding (G3):** define stable token ID bindings on shape properties and component properties; serialize, resolve, render, and update without replacing semantic references with computed values.
4. **Dependency graph (G3/G6):** add invalidation/impact graph only after a correct baseline. Test transitive dependencies and change fanout; measure before optimizing.
5. **Color model (G3):** specify source/profile and conversions for sRGB, Display-P3, Lab/LCH, and OKLCH. Add reference conversion fixtures, contrast calculations, and visual output tests. Current `Color` support is not evidence of all these spaces.
6. **Modes and themes (G3):** reconcile Rust collection modes with React themes and document scoping. Test parent context, component overrides, dark/light changes, and render updates.
7. **Consumers (G4+):** CSS/JSON export, component properties, canvas rendering, accessibility diagnostics, MCP/AI read APIs, and impact preview must consume the same resolver and bindings.

## Required gates

- DTCG import/export golden fixtures for the accepted standard version; alias round-trips and explicit loss reporting.
- Stable IDs and deterministic serialized output; unknown extension preservation policy.
- Tests for cycles, missing aliases, wrong token type/scope, deleted token fallback, mode inheritance, and nested components.
- One browser test that changes a token and checks affected shapes/components, then reloads and verifies the same semantic binding and pixels.
- Export checks for CSS variables and JSON; round-trip imported fixtures without value flattening.
- Performance baseline for large token graphs and incremental updates before setting a latency goal.

**P0 status:** reusable Rust model/resolver **Implemented and locally unit-verified**; React/import integration and DTCG conformance **Blocked**; semantic impact/code/AI consumers **Proposed**. Keep M1+ features dependent on the contract review.
