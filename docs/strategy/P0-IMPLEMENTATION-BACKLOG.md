# P0 Baseline — Acceptance and Dependency-Ordered Backlog

**Branch:** `p0/penpot-capability-baseline`
**Base:** `964676b90398f4458a72e3d41fd87e0dc91f51f9`
**Gate result:** **Not accepted yet.** This branch records the source/build baseline; it cannot certify the actual editor journey because the supported workspace artifact and an approved disposable database schema are unavailable.

## P0 acceptance matrix

| Acceptance criterion | Status | Evidence / remaining action |
|---|---|---|
| Major capabilities inventoried with source evidence | **Complete (source inventory)** | `LOGOS-CAPABILITY-REGISTRY.md`; source presence is kept separate from product verification. |
| Actual editor entrypoint identified and exercised | **Partial / Blocked** | Source intent maps dashboard open to `/workspace.html` and historical legacy bundle. React `WorkspacePage` is not routed. SPA build does not produce `dist/js/main.js`; no full browser journey was run. |
| Reusable Penpot/Logos functionality explicitly preserved | **Partial** | Historical snapshot and port/compat comments are recorded; no route removal or algorithm rewrite. Exact upstream ancestry and capability semantic diffs remain to be reviewed. |
| Canonical document ownership documented | **Documented / Decision open** | Rust, React, Go Malli/Transit, renderer ABI and side stores are mapped in `CANONICAL-DOCUMENT-CONTRACT.md`; no approved single serialized contract. |
| Renderer/layout ownership documented | **Documented / Blocked** | Skia has hosted standalone pixels; React’s Skia/WebGPU context collision and unconnected Rust layout worker are recorded. |
| Migration fidelity gaps categorized | **Complete (source inventory)** | `UNIVERSAL-MIGRATION-ARCHITECTURE.md`; import dialog not applying converted shapes is a blocking integration finding. |
| Design-system dependencies mapped | **Complete (source inventory)** | `DESIGN-SYSTEM-ENGINE-ROADMAP.md`; DTCG and app/Rust mapping are not yet verified. |
| Database and persistence blockers identified | **Complete (source evidence)** | Existing schema provenance follow-up plus current migration inventory; local React sync default is no-op. Schema owner still required. |
| Prioritized dependency-aware backlog exists | **Complete** | This document. |
| Every claim has one of required evidence labels | **Complete for these reports** | Reports use Implemented / Verified / Blocked / Proposed and identify the exact check scope. |

## Backlog

| Order / task | Owner role | Dependency | Scope and acceptance evidence | Status |
|---|---|---|---|---|
| **P0-A: approve editor-entrypoint ownership** | Product + frontend/release lead | None | Decide legacy producer recovery vs controlled React migration. Preserve route compatibility until signed decision. Record intended supported artifact, owner, URLs, and rollback. | **Open** |
| **P0-B: establish reproducible browser environment** | QA/automation + backend/DB lead | P0-A, canonical schema source | Disposable loopback-only PostgreSQL/Redis; owner-approved schema bootstrap; Go + frontend start; disposable user/file. Browser recording, network statuses, console, artifact hashes. Never use production/user data. | **Blocked on schema and producer** |
| **P0-C: verify actual entrypoint journey** | QA/automation | P0-B | Login → dashboard → create/open real file → route and assets → editor initialization and visible pixels → console/network capture. Direct URL, refresh, Back/Forward, missing ID and unauthorized cases. | **Not run** |
| **P0-D: canonical schema and document/revision contract discovery** | Domain/Rust + backend/DB lead | P0 source findings | Reconcile Rust Shape/token types, React projection, Malli/Transit persisted fields, Go change encoding, asset references and historical versions. Publish versioned mapping and fail-closed policy. | **Open; no schema edits in P0** |
| **P0-E: renderer/layout ownership decision** | Graphics + frontend lead | P0-A and document contract | Select one renderer per canvas; document Skia/WebGPU roles; test lifecycle, fallback, root/child scene and layout worker through React. Keep Skia pixel smoke. | **Open** |
| **P0-F: importer/design-system research** | Interoperability + design-system lead | Approved canonical contracts for implementation | Build source inventory, support/version matrix, legal fixture plan, DTCG mapping, and loss/fidelity metrics. Research may proceed independently; do not change stored model. | **Research documented; validation open** |

After P0 evidence and acceptance, dependent code work should be independently testable and delivered in bounded increments: G1 entrypoint/artifact; G2 schema/contracts; G3 validated commands and renderer integration; G4 durable journal/server sync; G5 collaboration/rebase; G6 product/security/release. Do not start AI, cross-platform packaging, or new suite products as substitutes for these gates.

## Exit decision

P0 **does not pass the final acceptance gate** until P0-A through P0-C provide a supported artifact and empirical browser evidence. G2 schema provenance and G3 renderer/document ownership remain blockers for implementation claims. No merge is an approval to begin dependent work; merge authorization remains an explicit Chief Architect decision under the engineering workflow.
