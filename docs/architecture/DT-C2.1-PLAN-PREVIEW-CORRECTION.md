# DT-C2.1 — Agent-plan validation and property preview

**Status:** proposed in-memory validation foundation; no commit integration  
**Baseline:** merged DT-C2 resolver (`f22643777bc42615c00029378f5998019e8e67b0`)

## Preview contract

- Plans and contexts must match the same snapshot revision. `SetProperty` and
  `SetTokenValue` require the exact sequential `expectedProperty` or
  `expectedValue`; omission and null reject with `INVALID_PLAN`, while a
  mismatch rejects with `STALE_REVISION`. `CreateToken` instead requires the
  full `(namespace, UUID, setId)` identity to be absent. The supplied
  `DocumentContext` is the caller's authoritative read snapshot; its revision
  must match both plan base revision and token snapshot revision. Client
  assumptions never replace these checks.
- Operations apply in order to private clones. A later operation sees the
  preceding proposed value for the same token or property. A rejected plan
  never returns `READY` or partial successful property previews.
- A set-property operation must target an existing property in the supplied
  `DocumentContext`, and its shape must be selected. Adding new property paths
  is unsupported. Namespace writes and cross-namespace references require the
  namespace to appear in `allowedNamespaces`.
- After the full operation list and token dependency graph validate, each
  affected property is resolved against the proposed token snapshot through
  `contract_resolver::resolve`, once per requested preview context (or the
  default context). `ImpactSummary.propertyPreviews` carries the proposed
  property value and complete `ResolveResult`. An error result rejects the
  plan with the property path attached to its diagnostics; fallback resolution
  remains visible with its warning and resolved value.
- Contextual set-value updates use the active matching overlay with highest
  precedence, or the base token definition when no active overlay matches.
  Ambiguous duplicate property identities in the supplied context reject
  before simulation.
- `READY` means this deterministic preview passed. It does not authorize or
  perform a commit. No persistent document ownership, undo, or transaction
  boundary is introduced.

## Regression coverage

The Rust tests cover create-token then bind, create-alias then bind, token
update followed by a dependent property bind, sequential repeated writes,
conflicting and missing expected values, missing explicit modes, unauthorized
namespaces, cycles introduced by ordered operations, late-plan rejection,
deterministic output, typed impact and unchanged source inputs. No benchmark or
production data access is part of this correction.
