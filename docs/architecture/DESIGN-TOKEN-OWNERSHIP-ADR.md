# DT-ADR-01 — Design-token contract and runtime ownership options

**Status:** Proposed recommendation; Chief Architect decision pending

**Scope:** Contract ownership, deterministic evaluation boundary, and migration path

**Baseline:** `ceaa58192fe21a500894eb005ba0c24375ba3e95` (main, after PR #82)

**Related proposal:** [Design token domain contract](DESIGN-TOKEN-DOMAIN-CONTRACT-PROPOSAL.md)

**Fixture:** [Minimal illustrative contract v0.1](fixtures/design-token-contract/v0.1/minimal.json)

This decision record compares the three requested ownership options and
recommends a contract-first version of Option C for Chief Architect evaluation.
Use the attached **Agentic Design Token Framework v1.0** as the proposed product
reference for architecture evaluation and task decomposition, subject to the
evidence qualifications below. It is not an accepted production contract. This
record does not choose the canonical persisted document owner, approve a runtime
implementation, or authorize persistence changes. PR #82 is merged as the
evidence baseline; its deferral of canonical document ownership still applies.

## Decision to make

Logos needs one interoperable, typed token contract for direct property values,
optional token bindings, aliases, modes/contexts, optional sets, and future
agent-proposed changes. The same contract must support React authoring, Rust
resolution/render projection, offline snapshots, file persistence and operation
replay without creating competing persistent token registries.

The proposed UX stays property-first: the supplied Figma inspector reference
shows normal fill/stroke/opacity controls as the everyday editing surface; the
Penpot Tokens reference shows a separate, richer management view for sets and
themes. These images inform placement and progressive disclosure only; they do
not establish Logos semantics or implementation capability. Direct editing
must work with no tokens, collection, group, set, theme, or agent interaction.

## Repository evidence

| Boundary | Current evidence | What it establishes |
|---|---|---|
| Rust domain | `rust/logos-types/src/token.rs` contains `TokenRegistry` schema v1, primitive and semantic token structs, collections, modes, mode values, `TokenSource`, and `VariableBinding`. `TokenValueKind::Alias` is UUID-based. | A typed Rust candidate exists, but does not establish canonical persisted ownership. |
| Rust evaluation | `rust/logos-tokens/src/variable_resolver.rs` resolves semantic-token aliases, collection modes, scope and binding fallback; it detects cycles and caps alias depth at 32. Rust unit tests exercise the crate. | A deterministic resolver foundation exists. The current resolver does not evaluate arbitrary formula ASTs, does not resolve primitive-token aliases, and is not found wired to React, render-WASM, or Go. |
| Type generation | `logos-app/src/types/rust-generated/token.ts` exports legacy `Token`, `TokenGroup`, and `TokensLib` types. Search of checked-in generated TS finds no `TokenRegistry`, `SemanticToken`, or `VariableBinding`. | Do not treat the current generated token file as a projection of the newer Rust candidate. A generator/schema parity task is required for any shared contract. |
| React token state | `logos-app/src/stores/tokenStore.ts` stores imported token sets/themes in Zustand local storage and resolves name-based `{name}` aliases. `ImportMigrationDialog.tsx` loads converted sets/themes. | This is an import/runtime island, separate from the document snapshot and not evidence of bindings on rendered properties. |
| React document/offline state | `documentStore.ts` stores pages and shapes. `offline/persist.ts` snapshots these fields to IndexedDB and queues generic patches. `offline/sync.ts` contains a lightweight rebase shim; full operation parity is not established. | A React-side offline boundary exists, but tokenStore data is currently outside the persisted document snapshot. Token operations and conflict semantics are unproven. |
| Shape/render path | Rust `Shape` stores direct fills, strokes, opacity, radii and other concrete styles. React's shape projection and `render-wasm/scene.ts` pass concrete style values. | There is no connected property-binding or token-resolution stage in the current render path. |
| Go document persistence | `get-file` exposes generic JSON `file.data`; updates apply file changes and history. `FileVersion` is 67; `DefaultFeatures` contains `design-tokens/v1` and `styles/v2`. No token-specific Go table/model was found. | Go can remain an opaque transport candidate. Feature names are not proof of a working token schema or round trip. |
| Libraries | Rust `TokenSource` has File/Library/Remote variants. Go has file-library relations and team-scoped library-change broadcasts. | These are separate, partial foundations; token-level authorization, pinned revisions and offline library behavior are not established. |
| Compatibility | Existing direct style shapes differ between legacy Transit data, React projections and Rust serde. PR #82 records that the current token stores are not durably document-scoped. | Any migration needs golden fixtures and a no-rewrite path for existing direct-value documents. |

## Options and relative assessment

Scores are an explicit planning rubric, not measurements or delivery estimates.
Each criterion is 1–5, where 5 is the more favorable outcome. Weights reflect
the requirements in the architecture assignment. A score is based only on the
repository evidence above and carries medium-to-low confidence because there is
no end-to-end token journey to measure.

| Criterion | Weight | A: Rust domain authority | B: React-first authority | C: Shared contract and deterministic core |
|---|---:|---:|---:|---:|
| Fit with current repository | 20% | 5 | 3 | 3 |
| Migration safety | 15% | 2 | 3 | 4 |
| Type contract / language bindings | 15% | 4 | 2 | 5 |
| Offline persistence fit | 10% | 2 | 3 | 3 |
| Undo, operations and rebase fit | 15% | 2 | 2 | 4 |
| Library boundary fit | 10% | 3 | 2 | 3 |
| Deterministic testability | 15% | 4 | 2 | 4 |
| **Weighted planning score / 5** | **100%** | **3.30** | **2.45** | **3.75** |

### A. Rust domain authority, generated TypeScript projection, Go opaque transport

**Shape:** Rust owns the token registry and evaluator; React uses generated
types and calls the Rust implementation; Go stores and transports document
payloads without token-specific persistence logic.

**Advantages:** Strongest current domain fit: typed structures and a resolver
already exist, and Rust unit tests provide a natural place for deterministic
validation. It keeps a single evaluator and avoids introducing token-specific
Go tables.

**Migration risks:** The checked-in TS file does not currently generate the new
registry/binding candidate. React token imports live in a separate local-storage
store; React document snapshots and operation patches do not include token
semantics. This option would require introducing a reliable React-to-Rust
boundary and moving/importing current token state without losing data. Rust's
resolver does not yet cover formulas, multi-axis set precedence or complete
property binding integration.

**Relative effort:** Medium-high. Most domain logic has a starting point, but
the app, offline and operation boundaries remain unconnected.

### B. React-first document authority, Rust resolver projection

**Shape:** React owns definitions, property bindings and document operations;
Rust receives snapshots or resolved projections for rendering/evaluation; Go
transports the document.

**Advantages:** The inspector and document store are already React-facing, so
property-first authoring can be prototyped in the same runtime. The model could
be serialized by the existing React document pipeline after that pipeline's
ownership and operation limits are addressed.

**Migration risks:** Current token storage is separate from `documentStore`,
and direct styles are only partially integrated with the document persistence
path. A second Rust representation or evaluator adapter risks semantic drift.
The generated type bridge and UI-level resolver tests are currently absent.
Existing generic patches and the sync shim do not prove token-aware undo or
concurrent conflict behavior.

**Relative effort:** Medium-high. UI iteration is direct, but canonical state,
offline changes, renderer projections and Rust parity all need definition.

### C. Shared schema and deterministic core consumed by React and Rust; Go retains transport

**Shape:** Define one versioned, language-neutral contract for
`TokenSnapshot`, `ResolveRequest`, `ResolveResult`, declarative `Plan`,
`ImpactSummary`, `TransactionReceipt` and diagnostics. Generate or validate
Rust and TypeScript bindings from it. Keep one deterministic evaluator behind
that contract; React authors and previews through typed commands, Rust remains
the leading implementation candidate for evaluation, and Go transports opaque
document data unless a later decision assigns it a validated responsibility.

**Advantages:** The schema can be shared without making React and Rust mutable
owners of separate registries. Contract fixtures can test both language
projections before persistence is selected. The boundary can include revision,
context and diagnostics needed for offline changes, undo/rebase and agents.

**Migration risks:** Requires a new schema/generation or conformance mechanism,
version policy, and an explicit rule that exactly one evaluator defines
resolution. Until generated bindings and transaction semantics exist, it can
become another abandoned contract layer. This is the largest initial contract
investment, so the first phase must stop at fixtures and a prototype, not a
second persistent store.

**Relative effort:** High initial architecture/tooling effort; lower expected
cross-runtime drift if the generation and conformance gates are enforced.

## Recommendation for Chief Architect evaluation

**Recommend Option C as the contract architecture**, with one deterministic
resolver implementation and generated/validated language bindings. Use the
Rust `logos-tokens` crate as the first evaluation/prototype candidate because it
already has typed alias/mode resolution and unit tests. This recommendation
does not make Rust the canonical document owner or approve using the current
crate in production. The existing model and resolver must first satisfy the
accepted contract and interop tests.

Keep the canonical persisted document owner as a separate explicit decision.
An approved shared token contract can define stable types and message shapes
without deciding whether React state, Rust types, or another system owns the
durable document and operations. Preserve Go as opaque transport in the initial
proposal; revisit only if concrete validation or persistence requirements call
for a Go responsibility.

Initial sequencing:

1. Accept or amend the logical contract and fixture below; do not persist it.
2. Decide the authoritative document state and operation/undo boundary.
3. Prove generated/conformant Rust and TypeScript bindings from the same
   contract.
4. Compare the Rust candidate resolver against golden contract cases; list
   unsupported formulas/context behavior rather than silently extending it.
5. Design a read-only adapter for existing React imports and direct-value docs.
6. Only after the owner/operation decisions, authorize a separately scoped
   persistence and editor integration task.

## Migration and compatibility assessment

- **Existing direct-value files:** no eager migration. Old files without token
  metadata must remain valid and render the same concrete styles.
- **React imports:** `tokenStore` sets/themes are source material, not current
  document state. Stage conversion and diagnostics; preserve the source until
  an explicit user/operation applies it.
- **Existing Rust candidate:** map schema v1 fields in a compatibility matrix.
  Do not assume that `PrimitiveToken`, `SemanticToken` or `VariableBinding` are
  already serialized by the app or understood by Go.
- **File features/version:** `design-tokens/v1`, `styles/v2` and file version
  67 require a field-level compatibility proof before use as migration signals.
- **Operations:** token creation plus first binding must be one undoable
  transaction; override, detach, rename, delete, mode change and alias change
  need defined inverse/rebase behavior before persistence. Failed validation
  must leave the document unchanged.
- **Unknown data:** adapters preserve unsupported extension payloads where
  possible and report lossy conversions. Unsupported major versions fail
  closed for editing while retaining a recoverable source payload.
- **Libraries:** start with document-local IDs. Do not claim secure shared
  token libraries until scope, permission checks, pinned revision, revocation,
  cache invalidation and offline behavior are specified and tested.

## Contract fixture v0.1 — illustrative only

The companion JSON fixture demonstrates just two essentials: existing direct
property values remain representable without tokens, and an optional local
token binding can carry a type-compatible fallback. It intentionally includes
no collections, groups, sets, modes, formulas, libraries, renderer ABI fields,
or persistence envelope. Those fields and semantics require further contract
decisions. The JSON is not a production schema and does not authorize a write.

## Acceptance criteria for the ownership decision

Before any persistent model or operation implementation begins, the Chief
Architect decision should:

- select the canonical document state and serialization owner, or explicitly
  identify the unresolved owner and prohibit persistence work;
- select the source of truth for contract evolution and binding generation;
- identify the single evaluator and the React/Rust/Go adapter responsibilities;
- define token/property commands, atomicity, undo/redo, revision conflicts and
  offline replay expectations;
- define document-local versus library namespace and authorization behavior;
- approve the first schema version, unknown-field policy and direct-value
  compatibility policy; and
- list the golden fixtures and automated acceptance checks required before
  implementation is considered complete.

Until then, keep token architecture at **PROPOSED**. Production security remains
**UNVERIFIED** and memory performance remains **NOT MEASURED**; this decision
record does not change either status.

## UX and standards references

- [Figma variable modes](https://help.figma.com/hc/en-us/articles/15343816063383-Modes-for-variables): Auto/default inheritance and collection defaults inform the proposed contextual UX; Logos precedence still requires its own decision.
- [Penpot Design Tokens](https://help.penpot.app/user-guide/design-systems/design-tokens/): token-set activation order and multidimensional themes inform the optional advanced organization model.
- [DTCG Format Module 2025.10](https://www.designtokens.org/TR/2025.10/format/): token exchange syntax, types, groups, extensions and reference rules; it does not define the Logos document owner or product operations.
- [DTCG Resolver Module 2025.10](https://www.designtokens.org/TR/2025.10/resolver/): separate resolver semantics for source sets and modifiers. Logos should declare its compatibility profile instead of conflating resolver behavior with the format module.
- [Merged PR #82](https://github.com/navidrezadoost/Logos/pull/82): repository inventory and proposed domain contract baseline.

The Penpot user-guide pages checked for this record substantiate token sets,
ordered activation and multidimensional themes; they do not specify a formula
grammar or evaluator. Treat Penpot formula compatibility as an unverified
product requirement until a named-version source and golden fixtures establish
the supported syntax and semantics. Do not describe arbitrary expressions as
DTCG Format Module behavior.
