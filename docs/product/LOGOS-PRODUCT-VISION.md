# Logos Product Vision

**Planning baseline:** `fb0ea41d73a553b31d6dea0c7b1a341d68fdba7c` (`main`, 2026-11-13)

**Status:** codifies the supplied product direction. It is not a release commitment, technical ADR, schema approval, or authorization to change routes or runtimes.

## Vision

**Logos is an independent, AI-native, cross-platform, universal design and collaboration platform.** People and AI agents should be able to create, understand, adapt, review, and move high-fidelity design work across tools and platforms while keeping people in control of consequential changes.

Logos should feel deliberate and responsive in everyday work, support collaborative teams and extensible workflows, and preserve the meaning of a design—not just its screenshot—when work moves between systems.

## Product position

- **Independent product architecture.** Logos owns its product direction, domain concepts, durable document contracts, and evolution. Existing Penpot-derived code and data are valuable heritage and compatibility inputs, not the ceiling for future architecture.
- **Measured UX benchmark.** Figma is a reference for interaction quality and task outcomes. Compare specific, pinned workflows and observable results; do not infer or clone Figma's internal architecture, or promise parity with every Figma product.
- **Universal migration as a fidelity goal.** Penpot, Figma, Adobe XD, Sketch, SVG, and other sources are potential migration inputs. Support depends on real export/API access, format and version coverage, legal rights, and representative fixtures. A supported path must report what was preserved, approximated, or omitted. “Universal” is a direction, not a 100% migration guarantee.
- **AI-native and human-led.** Agents can inspect and propose semantic edits, but changes must be permission-scoped, reviewable, and reversible. AI does not bypass the document contract, validation, or user authorization.
- **One product across platforms.** Share domain behavior and document semantics across supported platforms. Platform-specific presentation may differ when accessibility, input devices, or operating-system conventions require it; semantic parity and supported capability differences must be explicit.

## Who Logos serves

| User | Product outcome |
|---|---|
| Designers and design-system owners | Create and maintain detailed design documents, reusable systems, and prototypes with predictable tools, keyboard workflows, and history. |
| Product teams and reviewers | Collaborate with clear permissions, comments/review, version history, and recoverable changes. |
| Teams moving from another tool | Preview and understand migration fidelity before committing, retain provenance, and recover the original when an import is not acceptable. |
| Developers and workflow builders | Use stable, permissioned APIs, plugins, and automation to connect design intent with downstream work. |
| AI agents working for people | Read only authorized context, propose bounded semantic operations, and receive auditable approval or rejection. |

## Product outcomes

1. **Interaction quality:** common actions are discoverable, accessible, consistent, and measurably responsive. Task-level comparison to pinned Figma workflows is evidence for decisions, not a claim of internal or total feature parity. See [UX acceptance criteria](../acceptance/UX-ACCEPTANCE-CRITERIA.md).
2. **Meaningful portability:** supported imports preserve structure, geometry, typography, assets, components, variables, layout, and interactions to explicitly measured levels; exports and rollback are independently tested.
3. **Trustworthy collaboration:** people can see who or what changed a document, review versions, control access, and recover from errors or conflicts.
4. **Safe intelligence:** agent operations are understandable before application, constrained by permissions and validation, and undoable after application.
5. **Scale and reach:** large documents and supported platforms remain usable under published, reproducible performance and compatibility criteria.
6. **Sustainable extension:** versioned APIs, plugins, and automation extend Logos without bypassing isolation, security, or document invariants.

## Current evidence and intended future state

These are deliberately separate:

| Topic | Evidence at the planning baseline | Product direction (not yet established) |
|---|---|---|
| Product surface | The repository has React, Rust, Go, and historical Penpot-era assets, but source presence does not establish one complete supported product journey. The [capability registry](../strategy/LOGOS-CAPABILITY-REGISTRY.md) records the evidence limits. | A coherent Logos-owned product across supported platforms, with a published capability and parity matrix. |
| Document model | Current documents cross several Rust, React, Go/Malli/Transit, and renderer representations; the [canonical document contract](../architecture/CANONICAL-DOCUMENT-CONTRACT.md) says no single end-to-end canonical representation is proven. | An extensible, versioned Logos-owned domain/document contract with explicit adapters and migrations. This vision does not select a concrete implementation or replace existing ADRs. |
| Rendering and UX | Standalone renderer pixel evidence exists, but the React editor integration and broader fidelity are not thereby proven. See [renderer evolution](../architecture/RENDERER-EVOLUTION-PLAN.md). | End-to-end correctness and performance measured through supported user workflows and representative documents. |
| Migration | Partial Sketch/XD conversion source exists; the current migration assessment identifies missing application of converted shapes and no complete Figma/Penpot path. See [migration baseline](../strategy/UNIVERSAL-MIGRATION-ARCHITECTURE.md). | Versioned, access-aware adapters with preview, fidelity reports, reversible apply, and tested export/rollback. |
| AI and collaboration | Current source does not establish a complete, durable human/agent transaction and review loop. | Permissioned co-work with explicit authorship, review, versioning, history, and undo. |

The active P0.5 stabilization gates are not product achievements: **#64** remains a React/render integration pixel blocker; **#71** security work remains Draft; **#73** benchmark setup is repaired but actual browser/heap measurements remain blocked. Merged **#67, #69, #70, and #72** provide CI, fixture, persistence-boundary, and test-isolation foundations only. P0.5 is not declared complete. The [unified roadmap](../roadmap/LOGOS-UNIFIED-PLATFORM-ROADMAP.md) records the gates and sequence.

## Guardrails

- No claim of 100% competitor migration, certified source API/data access, universal compatibility, or achieved Figma parity without scoped evidence.
- No new canonical schema, renderer/runtime, editor route, or platform host is selected by this vision. Preserve existing architecture/ADR decisions until a reviewed, evidence-based decision changes them.
- No agent or plugin may silently apply unbounded changes or bypass user permissions, validation, review, and recovery.
- Performance thresholds in the UX criteria are proposed targets until a reproducible baseline is collected and reviewed.

## Related direction

- [Architecture principles](../architecture/LOGOS-ARCHITECTURE-PRINCIPLES.md) translate this vision into constraints and acceptance evidence.
- [Unified platform roadmap](../roadmap/LOGOS-UNIFIED-PLATFORM-ROADMAP.md) orders stabilization and future product work without promising dates.
- [UX acceptance criteria](../acceptance/UX-ACCEPTANCE-CRITERIA.md) defines measurable test candidates and the baseline needed before targets become commitments.
