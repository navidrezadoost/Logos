# Universal Migration Architecture — P0 Baseline

**Baseline:** `964676b90398f4458a72e3d41fd87e0dc91f51f9`
**Decision:** preserve existing importers and binary contracts; do not claim zero-loss or add a parallel canonical model in P0.

## Source inventory

| Source | Present implementation | Observed integration boundary / loss risk |
|---|---|---|
| Sketch | `logos-app/src/migration/sketch/{sketch-importer,sketch-format,sketch-shape-converter,sketch-token-converter}.ts` parses a ZIP, `document.json`, page JSON, layers, styles, some text/symbol/layout fields, and token-like values. | Archive extraction filters `images/`; converter produces Logos shapes/tokens, but `ImportMigrationDialog` only merges token sets/themes into `tokenStore`. Returned shapes are reported by count/warnings but are not inserted into `documentStore`. |
| Adobe XD | `logos-app/src/migration/xd/{xd-importer,xd-format,xd-shape-converter,xd-token-converter}.ts` parses the ZIP/OPC manifest and JSON `graphicContent.agx`, then produces shape/token results. | Bitmap resources are skipped by extraction; the dialog likewise only applies tokens. No supported-version matrix or round-trip check was found. |
| Figma | No Figma file importer in `logos-app/src/migration/`. The roadmap’s API/plugin export idea is a proposal, not an implemented parser. | `.fig` is not shown as a supported public parser or import path. API access, plan/scope limits, and plugin export fidelity need separate research and fixtures. |
| Penpot | Historical source tree includes Penpot-era frontend/common and interoperable data structures; current Logos has explicit Malli/Transit compatibility types and Go binfile support. | No current `logos-app` Penpot archive importer is present. Historical code availability is not proof of present Logos file compatibility. |
| SVG | Shape type/source references include `svg-raw`; no standalone SVG import adapter or fidelity suite is present in the importer directory. | SVG resource resolution, filters, transforms, text/font handling, and embedded/external resources are not tested as an import contract. |
| Native Logos | Go `backend-go/internal/binfile/v3.go` handles ZIP `.logos` v3, media, metadata, raw data and Go-readable changes. | v1 is detected but not decoded by Go; legacy Clojure Transit+zstd change rows are preserved/imported raw but are not decoded for Go rebase. This is not equivalent to a version-independent native round trip. |

## Proposed neutral pipeline

```text
Versioned source adapter
  → bounded parse + provenance
  → source-neutral migration IR
  → validation + identity/resource mapping
  → canonical Logos transaction(s)
  → preview + structural/visual diff + fidelity report
  → user approval
  → durable save and audit record
```

Do not let each source converter construct persistent document state independently. The IR is a migration staging contract, not a second durable document schema. Its initial model should retain source ID/version, stable source path, hierarchy/order, geometry/transforms, text runs/font references, assets, components/instances/overrides, variables/styles/modes, layout/constraints, interactions, and unknown fields where safe.

## Fidelity model and acceptance

Report independent measures:

- **Structural:** identity, parent/child, order, pages/frames, components/instances, text runs, constraints, token references, prototype links, assets.
- **Visual:** deterministic render comparison with fonts/assets recorded; separately classify approximation versus exact mapping.
- **Behavioral:** responsive/layout changes, component variants, prototype actions, and supported interactions.

Every unsupported or approximated property must produce a stable machine-readable loss entry and user-readable warning. No importer may say “complete” merely because parsing succeeded.

## Fixture plan

For each source, check in or legally obtain small synthetic fixtures with expected IR and expected output; representative nested components/variants, text/variable fonts, gradients/effects, assets, layout/constraints, aliases/modes, and interactions; malformed/oversized archives; and fixtures from every explicitly supported source version. Tests should assert identity/order/property preservation, warning codes, deterministic re-import, rendered image diffs, and resource limits. Keep proprietary source files out unless redistribution rights are documented.

## P0 decisions and risks

- **Implemented:** Sketch and XD parsing/conversion source; Go native v3 ZIP import/export code.
- **Verified:** parser functions are not backed by a discovered `logos-app` unit/spec suite; Go binfile package has unit/round-trip tests, not a complete cross-version editor round trip.
- **Blocked:** shape conversion is not applied by the current import dialog; no source-neutral IR; no Figma/Penpot/SVG importer; asset fidelity absent; no repeatable visual diff/loss report.
- **Proposed:** after G1–G3 contracts are approved, make importer actions produce validated canonical transactions with preview and reversible apply. Research Figma API/plugin export and licensing/access limits independently; do not promise support for private `.fig` internals.

See [canonical document contract](../architecture/CANONICAL-DOCUMENT-CONTRACT.md) for the dependency this workstream must consume.
