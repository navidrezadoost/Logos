# Logos Unified Platform Roadmap

**Planning baseline:** `fb0ea41d73a553b31d6dea0c7b1a341d68fdba7c` (`main`, 2026-11-13)

**Status:** dependency-ordered product direction, not a dated delivery commitment or authorization for code, route, runtime, schema, or platform changes. Sequence and gates require owner review.

The roadmap turns the [product vision](../product/LOGOS-PRODUCT-VISION.md) and [architecture principles](../architecture/LOGOS-ARCHITECTURE-PRINCIPLES.md) into evidence-based stages. Continue P0.5 stabilization first; do not interpret this strategy work as declaring it complete.

## Active P0.5 stabilization gates

| Gate | Current status | What the status does and does not establish |
|---|---|---|
| **#64 — renderer/editor pixels** | **Blocked** | React/render integration pixel acceptance remains open. Standalone renderer pixel evidence is narrower and does not close the editor integration gate. |
| **#71 — security** | **Draft** | Security work remains under review. No product-level security certification or agent/plugin approval is implied. |
| **#73 — benchmark** | **Setup repaired; measurements blocked** | Benchmark setup is available/repaired; actual supported-browser interaction and heap measurements have not been established. No latency or memory target is verified. |
| **#67 — CI runner qualification** | **Merged foundation** | CI environment/apt path foundation only; not editor, UX, or performance acceptance. |
| **#69 — compatibility fixture boundary** | **Merged foundation** | Historical fixture boundary only; not general format support or migration fidelity. |
| **#70 — browser persistence boundary** | **Merged foundation** | Isolated browser/API persistence proof only; not full editor route, visual fidelity, or collaboration. |
| **#72 — layout-WASM ABI test isolation** | **Merged foundation** | Test isolation only; not full layout/renderer integration or product acceptance. |

**P0.5 is not complete.** Closure requires the responsible track owners to publish evidence and accepted exit criteria. This roadmap does not redefine those criteria. Existing [P0.5 persistence](../r1/P0.5-BROWSER-API-PERSISTENCE-BOUNDARY.md), [renderer](../architecture/RENDERER-EVOLUTION-PLAN.md), and [verification](../architecture/P0-VERIFICATION-PROCEDURE.md) records retain their narrower scopes.

## Dependency-ordered stages

| Stage | Intent and work products | Exit evidence / dependency |
|---|---|---|
| **A. Finish stabilization evidence** *(in progress; gates open)* | Continue P0.5 work: close or explicitly disposition #64, complete security review for #71, and capture the browser/heap measurements blocked by #73. Preserve merged #67/#69/#70/#72 as foundations only. | Track owners publish reproducible results and remaining limitations; no status promotion based on setup, source presence, or isolated tests alone. Does not imply product readiness. |
| **B. Agree on product and evidence baselines** | Accept the product vision; inventory supported workflows, users, source formats/access, platform scope, current P0 constraints, and UX baselines. Pin Figma benchmark workflows/build/account conditions where access permits. | Owners sign off on scope and a capability/support matrix. Record unavailable API/export access, legal constraints, unknowns, and out-of-scope features. |
| **C. Establish Logos document and operation contracts** | Resolve the canonical document ownership/versioning gap through the existing review process. Define schema evolution, identity, validation, unknown-field behavior, resources, validated operations, inverse/undo, and migration boundaries. | Architecture acceptance matrix passes with golden documents, round-trip/loss tests, save/reload, rejection, undo/redo, and recovery. No schema choice is made by this roadmap. |
| **D. Deliver a dependable core interaction loop** | Based on accepted existing route/runtime decisions, make core select, create, edit, keyboard, undo/redo, save/reopen, and failure recovery work end to end. Measure against the [UX test candidates](../acceptance/UX-ACCEPTANCE-CRITERIA.md). | Supported-route browser tests, structural/render assertions, permissions and durable revisions, reproducible p50/p95/p99 and task outcomes on named fixtures/platforms. Any route/runtime changes require a separate reviewed decision. |
| **E. Expand fidelity and migration adapters** | Choose prioritized sources based on user demand and verifiable access. Add versioned adapters, provenance, source-neutral staging, preview, structural/visual/behavioral diff, loss report, resource mapping, export, and rollback. Start with legally usable, representative fixtures. | Per-source/version fidelity matrices and support disclosures; rollback/cost criteria passed. Figma APIs/plugins/private formats and Adobe XD access are investigated rather than presumed. “Universal” does not mean every file is lossless. |
| **F. Build collaboration and safe agent workflows** | Add version/review/history/workflow and human+agent collaboration on accepted operation/auth models. Add scoped semantic APIs, approvals, audit records, conflict handling, and reliable undo/recovery. | Independent-client and revoked-permission tests; agent evaluation includes correct operations, unintended changes, refusal boundaries, preview/approval, undo, audit, and security review. |
| **G. Extend platforms and ecosystem** | Expand supported platforms and plugin/API/automation surface using shared contracts. Add capabilities based on product demand rather than cloning an entire competitor suite. | Platform parity matrix, extension versioning/security/isolation, accessibility and performance evidence, release/update/rollback and support-cost review. |

## Prioritization rules

1. Close evidence gaps and contract dependencies before layering features on unverified behavior.
2. Prioritize frequently used, high-consequence design tasks and migration sources with demonstrated user value and feasible access.
3. Treat performance, security, accessibility, history, and migration rollback as product requirements, not later polish.
4. Do not promise calendar dates, full competitor parity, 100% migration, or source/API access until independently validated.
5. Make each stage independently reviewable; retain a stop/rollback point and document assumptions, dependencies, and operating cost.

## Acceptance linkage

All architecture-heavy stages use the [architecture acceptance matrix](../architecture/LOGOS-ARCHITECTURE-PRINCIPLES.md#architecture-acceptance-matrix). User-visible interaction work uses the [UX acceptance criteria](../acceptance/UX-ACCEPTANCE-CRITERIA.md), where all numeric thresholds remain proposed until a representative baseline is collected and approved. Existing [capability](../strategy/LOGOS-CAPABILITY-REGISTRY.md), [migration](../strategy/UNIVERSAL-MIGRATION-ARCHITECTURE.md), and [Figma reference](../strategy/FIGMA-FEATURE-PARITY-MATRIX.md) documents are evidence inputs, not commitments to implement every inventoried feature.
