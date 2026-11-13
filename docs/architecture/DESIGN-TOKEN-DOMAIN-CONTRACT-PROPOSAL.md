# Design token domain contract — discovery proposal

**Status:** Proposal for architecture discovery; no canonical document owner or
storage format selected. This document inventories repository evidence and
defines a contract to evaluate. It authorizes no token-engine implementation.

## 1. Repository inventory (DT-B1)

### Existing representations

| Area | Current representation | Evidence and boundary |
|---|---|---|
| React import tokens | `LogosToken` has ID, name, a small type union, string value and description. `LogosTokenSet` groups tokens; `LogosTokenTheme` has a group and string overrides. | `logos-app/src/migration/tokens/token-types.ts`; Sketch/XD converters produce these import types. |
| React token state | Zustand `tokenStore` holds sets, themes and active theme IDs in browser local storage. It resolves `{name}` aliases by name, set order and active theme override; missing/circular aliases return `undefined`. | `logos-app/src/stores/tokenStore.ts`; the import dialog loads converted data. Search finds no document/render integration beyond import UI and the store. |
| Rust legacy tokens | `TokensLib` maps set names and token paths to raw strings, with active sets. The lookup returns a stored string; it does not follow aliases. | `rust/logos-types/src/token.rs` (`TokensLib`). |
| Rust typed candidate | `TokenRegistry` schema v1 includes primitive tokens, semantic tokens, collections, modes, per-mode values, sources, and `VariableBinding`. `TokenValueKind` has Color, Number, String, Boolean and Alias(UUID); `TokenType` covers additional design values. | `rust/logos-types/src/token.rs`; generated TypeScript is in `logos-app/src/types/rust-generated/token.ts`. These types are not presently wired into React state, renderer, or Go persistence. |
| Rust resolver candidate | Resolves semantic aliases, collection mode inheritance/defaults, scope and binding fallback; detects cycles and caps alias depth at 32. | `rust/logos-tokens/src/variable_resolver.rs`. Standalone crate with its own tests; repository search finds no app, render-WASM, or Go consumer. It does not resolve primitive-token aliases or validate final value type against the target property's type. |
| Shape appearance | Rust `Shape` stores direct fills, strokes, shadows, blur, opacity and radii. Solid fill stores color string and opacity. React `Shape` is a separate projection; render-WASM serializes style values for rendering. | `rust/logos-types/src/shape.rs`, `fill.rs`, `stroke.rs`, `logos-app/src/types/shapes.ts`, `logos-app/src/render-wasm/scene.ts`. No token binding is present on the current shape style model. |
| Typography | Rust `Typography` is a reusable style record with font and spacing fields and an optional library path. | `rust/logos-types/src/typography.rs`. This does not establish an integrated token reference on text properties. |

### Document, runtime and persistence boundaries

- React `documentStore` contains pages and shapes. Its IndexedDB snapshot
  persists those fields (`logos-app/src/stores/documentStore.ts`,
  `logos-app/src/offline/persist.ts`). The separate token store uses local
  storage; it is not included in that document snapshot.
- Rust domain types and generated TS types are candidate contracts, not proof
  of canonical ownership. React/render-WASM currently transfers concrete style
  values to the renderer; no token-resolution step is connected in that path.
- Go `get-file` reads and returns generic JSON `file.data`; file changes and
  history are persisted with the file data/change mechanism. The inspected Go
  code has no token-specific tables/model. `FileVersion = 67` and feature flags
  include `design-tokens/v1` and `styles/v2`; these labels do not prove that
  the registry or binding model is implemented or round-trips end to end.
- Current compatibility material records mismatches among legacy Transit
  solid-fill fields, React shape projections and Rust serde forms. Existing
  adapter fixtures are test support, not the production persistence contract.

### Implications

There are multiple unconnected token representations and no verified
document-to-render-to-save token path. Token names, IDs, storage location,
document operation encoding, unknown-field preservation, library ownership and
resolution runtime remain architecture decisions. The current Rust candidate
is useful source evidence, not an accepted canonical model. Go is a generic
document transport/persistence candidate, not an assumed token-domain owner.

## 2. Proposed domain contract (DT-B2)

This is a logical contract, independent of which runtime owns its canonical
implementation.

### Optional organization and token values

- A document may contain no token metadata at all. A property can keep a direct
  literal indefinitely; creating a collection, group, alias, or mode is always
  optional.
- Tokens have stable identity, a user-facing name, a declared type, and a
  value. An optional path/group supports organization without requiring a
  prescribed taxonomy. Grouping is organizational and carries no resolution
  semantics.
- Primitive tokens are reusable literal values. Semantic tokens express
  purpose and may resolve to a literal or reference another token. Both are
  optional classifications in the user experience; the contract must allow a
  flat token list without forcing a primitive/semantic hierarchy.
- Values are typed scalars or structured values, including color, number,
  dimension/unit, font properties, typography composites, shadow and gradient
  where supported. Preserve source units/precision and avoid coercing strings
  into numbers implicitly.
- Alias values use stable token references internally. Names are import/export
  identifiers and display labels, not durable identity. Alias target and source
  types must be compatible. Composite values may reference compatible tokens
  at defined sub-properties if the chosen schema supports that explicitly.

### Collections, modes and contextual values

- A user-defined collection is optional and may own zero or more tokens and
  modes. Tokens outside collections resolve their single value without a mode
  context. Collection/group labels remain user-defined.
- A mode is an optional named context such as Light/Dark, Compact/Comfortable,
  or Brand A/Brand B; names and number of modes are not prescribed.
- A mode-specific value may be a literal or alias. Resolution selects the
  property's explicit context, then documented inherited contexts, then the
  collection's default. The precedence must be deterministic and serialized
  context identity must not depend on display name.
- If no collection or mode is used, direct values and token literals resolve
  without synthetic default collections.

### Property values and overrides

Each design property accepts either a direct value or a token binding. A
binding records the stable token reference, optional context/mode strategy and
an optional last-known-good typed literal fallback. An explicit local override
is a separate property value that takes precedence while retaining the binding
for later reversion; detaching a token is an explicit edit. Direct literals
remain first-class and require no token metadata. The persisted schema must
distinguish these states and must not confuse an intentional local override
with a broken alias fallback.

Resolution is pure: it reads the document/library token snapshot and property
context, returns a typed concrete value plus provenance/diagnostics, and never
mutates the document. Rendering receives concrete values through the existing
style projection unless the Chief Architect selects another renderer contract.

### Import, export and versioning

- Define a versioned native envelope with a major schema version and explicit
  optional metadata. Read older supported versions through deterministic
  migrations; write the selected current version only after preserving
  unsupported fields or reporting loss.
- Provide adapters for current Logos legacy values and sets/themes, and for
  external formats such as DTCG token JSON. Map external name-based aliases to
  stable IDs in an import namespace; report collisions, missing targets,
  unsupported types, unit conversions and lossy export mappings.
- Preserve unknown extension data at token/value boundaries when safe. Reject
  unsupported major versions for editing with a clear diagnostic while
  retaining the original payload for recovery; do not silently discard data.
- Export emits an explicit loss/unsupported-feature report. Round trips must
  preserve identity, values, modes and alias edges for the supported subset.

## 3. Dependency and alias validation specification

Validation occurs at import, edit/commit, and document-open boundaries; runtime
resolution still fails closed if persisted data is corrupt or stale.

1. **Identity:** token, collection, mode and binding IDs are unique within
   their declared scope. References must resolve in an explicit namespace
   (document first, then only authorized/selected library scopes); no implicit
   global name lookup.
2. **Names:** display names may repeat where UX permits. Name/path lookup for
   imports must define deterministic collision handling and map to stable IDs.
   Renaming a token does not break ID-based bindings.
3. **Types:** literal values validate against declared token type and property
   type. Aliases must resolve to a compatible type; incompatible aliases are
   validation errors, not implicit casts. Composite sub-property types are
   checked recursively.
4. **Modes:** mode belongs to one collection; collection default exists and is
   among its modes; token values refer to a mode in the token's collection.
   Mode inheritance must be acyclic, bounded and explicitly ordered. A
   collection-free token cannot be given a collection mode binding.
5. **Dependencies:** build a directed graph over `(token ID, selected context)`
   nodes after mode selection. Detect self-reference and longer cycles with a
   diagnostic cycle path. Apply a documented maximum traversal/depth and node
   budget to malformed inputs.
6. **Missing or removed references:** report a stable `missing-token` or
   `missing-mode` diagnostic. If a property binding has a valid compatible
   fallback, render that fallback and retain the broken binding for repair;
   otherwise retain the existing direct value when available or use the
   renderer's established safe default. Never silently rewrite/delete the
   reference during resolution.
7. **Scope and access:** if tokens declare valid property scopes, validate the
   binding against the destination property. Library resolution must respect
   authorization and immutable/versioned source identity; missing access is a
   diagnostic and cannot trigger a broader lookup.
8. **No mutation on failure:** parsing, validation and resolution do not write
   document data. Migration/import produces a staged result and explicit
   diagnostics; commit happens only after user/operation acceptance.

Resolver results should include resolved typed value, source token chain,
selected context/mode, fallback/override provenance, and stable machine-readable
diagnostics. UI wording may be beginner-friendly, while detailed chain/path
information remains available for professional debugging.

## 4. Integration boundaries (DT-B3)

The intended data flow is:

```text
authoring UI → logical document operation → canonical document state
             → pure token resolution for property/context
             → concrete style projection → renderer
             → versioned serialized document → persistence boundary
```

The diagram is a target boundary, not a statement about the current wiring.
Authoring may create direct values without touching token metadata. Token edits
and property bindings must be part of the same accepted document change/undo
model as the design property they affect. Persistence must round-trip identity,
references, modes, overrides and fallback values. Renderer input should be
resolved values and should not require the renderer to own token authoring or
import semantics unless an architecture decision establishes otherwise.

### Decisions requiring canonical-document ownership

The Chief Architect's canonical-document ownership decision is required before
choosing:

- the authoritative schema and runtime for token definitions, references,
  modes, bindings and migration;
- whether React state, Rust domain types, another shared contract, or a
  combination owns mutation validation and stable IDs;
- how token/property edits are represented in undo, operation history,
  collaboration/rebase, save/reload and document versioning;
- where file-local tokens and library references are stored, and how external
  libraries are identified, version-pinned, permission-checked and cached;
- which runtime performs resolution and how a consistent snapshot reaches the
  renderer and export/import adapters;
- whether Go remains opaque JSON transport or gains a validated/persisted
  token-specific responsibility;
- unknown-field compatibility, migration timing, and supported old-document
  read/write policy.

Do not add a second persistent token store, token database table, or duplicated
registry in React/Rust/Go before these decisions. Generated Rust-to-TypeScript
types are one candidate bridge, not proof of ownership.

## 5. Backward compatibility and migration

- Existing documents without token metadata remain valid and render direct
  values unchanged. Adding an optional field must not require eager migration.
- Existing React token sets/themes are local-storage import state, not verified
  document data. A later adapter must offer explicit attach/import into a
  document and preserve original source data until conversion is accepted.
- Legacy `TokensLib` string paths and aliases require parsing and reference
  mapping; its current lookup does not resolve aliases. Migration must retain
  unresolved text and report it instead of dropping it.
- Existing direct fill/stroke/typography values are the compatibility fallback.
  The first binding-capable migration should preserve the concrete value as a
  fallback and avoid rewriting unbound documents.
- Current file version/feature flags and React/Rust/Transit representations
  need a verified mapping before changing them. Feature labels alone do not
  authorize interpreting new fields.
- Golden fixtures should cover legacy documents, absent/unknown token
  extensions, supported schema versions, unresolved alias, cycle, type mismatch,
  override, mode fallback, import collision, save/reload and downgrade behavior.
  A migration must be deterministic, idempotent, loss-reported and must not
  mutate persisted data merely because it was opened or rendered.

## 6. Next-phase implementation tasks and acceptance criteria

These are proposed sequenced tasks; implementation requires the ownership
decisions above and separate assignment.

| Order | Task | Acceptance criteria |
|---|---|---|
| 1 | Canonical document/token ownership decision | Signed decision records authority, schema/version, operations/undo, renderer projection, persistence boundary, library scope and migration compatibility; unresolved items are explicit. |
| 2 | Versioned logical contract and golden fixture set | Contract specifies optional flat tokens, typed literal/alias, groups, modes, binding/override/fallback and unknown-field policy; fixtures round-trip supported values and preserve legacy documents byte/semantically as defined. |
| 3 | Compatibility adapter and migration plan | Adapter reads current imported token structures and legacy document forms without implicit persistence; reports collisions, missing refs and loss; tests establish deterministic idempotence and no write on read. |
| 4 | Pure validator/resolver | Tests cover every validation rule above, deterministic context precedence, alias chain provenance, cycle/depth, missing references, type/scope mismatch, fallback and no mutation. Resolver result is independent of UI and renderer. |
| 5 | Document operation and undo integration | Token/property changes are atomic, undoable/replayable, survive save/reload, and preserve direct-value editing without creating organization structures. |
| 6 | Property authoring and resolved-style projection | User can choose direct value or token binding, optionally create collection/group/mode, set local override, detach/revert, and repair missing refs. Beginner path requires no taxonomy. Rendering receives tested concrete values and existing direct styles remain unchanged. |
| 7 | Import/export and library boundaries | Supported format/version matrix, permission behavior, stable IDs, unknown extension retention and machine-readable loss reports pass round-trip fixtures. |

Each task's PR follows the repository engineering workflow: relevant automated
tests and required CI, acceptance evidence, known limitations, an accurate PR
summary, and explicit Chief Architect authorization before merge. Independent
engineer review is not a prerequisite.

## 7. Non-goals and current status

This proposal does not implement the token engine, select canonical ownership,
change React/Rust renderer code, create Go token persistence, or assert current
end-to-end token capability. It records current source evidence and proposed
acceptance boundaries. Token integration remains **PROPOSED** until decisions
and implementation evidence exist.
