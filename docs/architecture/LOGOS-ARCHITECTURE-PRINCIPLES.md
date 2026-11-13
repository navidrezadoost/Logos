# Logos Architecture Principles

**Planning baseline:** `fb0ea41d73a553b31d6dea0c7b1a341d68fdba7c` (`main`, 2026-11-13)

**Status:** proposed future-state principles. These guide discovery and review; they do not supersede existing architecture documents or ADRs, approve a schema, mandate an implementation option, or authorize route/runtime changes.

See the [product vision](../product/LOGOS-PRODUCT-VISION.md) for product intent and the [unified roadmap](../roadmap/LOGOS-UNIFIED-PLATFORM-ROADMAP.md) for sequencing.

## Principles

### 1. Logos owns its domain; formats are adapters

The durable product model must be designed and governed by Logos, with explicit versioning, stable identity, documented invariants, and extensibility. Legacy Logos/Penpot formats and third-party formats are compatibility concerns—not the authority that limits new product concepts.

Keep source adapters, migration staging representations, durable document contracts, editor projections, and renderer ABIs distinct. A migration IR may preserve source provenance and unknown extensions but is not a second durable document model. Select concrete model ownership and serialization through a reviewed decision that reconciles the existing [canonical document contract](CANONICAL-DOCUMENT-CONTRACT.md), compatibility findings, and ADRs. This principle does not declare Rust, Go, React, or any existing wire format to be the approved canonical owner.

### 2. Preserve meaning and make loss explicit

Every import/export path has a versioned support matrix and structural, visual, behavioral, and resource-fidelity evidence. Unsupported, transformed, or omitted content produces stable machine-readable findings and a user-readable preview. Unknown fields should round-trip where safe; unsafe or unrepresentable content must not silently disappear. Report fidelity by capability and fixture, never as a single “100%” assertion without evidence.

### 3. Durable edits are validated, reversible operations

Document changes flow through bounded operations with explicit targets, preconditions/base revision, validation, author/agent identity, and a result. Where an operation is accepted, define its inverse or a recoverable history entry. Support preview, apply, undo/redo, rejection, retry/idempotency, conflict handling, and audit history as part of the operation contract—not UI-only behavior.

Agents and automation use the same operation and authorization path as people. Preview and approval requirements are policy-driven; high-impact or destructive operations require meaningful review. Undo/rollback must restore document semantics, including dependent assets and references, not merely reverse visible geometry.

### 4. Share core semantics across platforms

Separate shared domain semantics from host-specific UI, input, persistence, and rendering adapters. Supported platforms consume compatible versioned document and operation contracts. Document meaning, permission checks, and operation results must not diverge by host. Differences in available capability, keyboard convention, accessibility, or rendering may be platform-specific, but are documented and covered by a parity matrix.

Do not choose a platform framework, renderer, editor route, or runtime by implication. Preserve and evaluate current paths using the existing [renderer evolution plan](RENDERER-EVOLUTION-PLAN.md), ADRs, and measured acceptance evidence.

### 5. Collaboration is a permissioned, reviewable state transition

Authorization is checked at the operation and resource boundary, not inferred from UI visibility. Durable history identifies actor, time/order, base/result revision, and outcome. Collaboration design must define conflict/rebase behavior, offline recovery, version comparison, review/approval, comments or workflow state, and access revocation. Notifications are not durable truth. Tests use independent clients and exercise concurrent edit/delete, stale revision, reconnect, denied access, and recovery.

### 6. Treat agents, plugins, and imported content as untrusted

Use least privilege, explicit capability grants, bounded input and execution, origin/package identity checks, isolation, and auditable access. A plugin or agent must not acquire broader access through an adapter, export, or host bridge. Imported files are hostile inputs: enforce archive/resource limits, safe parsing, and error isolation. Threat models and security tests cover prompt/content injection, data exfiltration, cross-document access, malicious plugins, and unsafe generated operations.

### 7. Measure performance on representative large documents

Define fixtures that vary object count and complexity (for example paths, text, effects, component nesting, assets, and layout), and publish hardware/browser/OS and cold/warm conditions. Measure input-to-visible latency, frame-time distribution, startup, memory/heap, and recovery. Report p50/p95/p99 and failures; optimize against an observed baseline without trading away correctness or fidelity. Candidate targets in the [UX criteria](../acceptance/UX-ACCEPTANCE-CRITERIA.md) are not measured claims.

### 8. Make APIs and extensions versioned product surfaces

Plugins, APIs, and automation use documented, versioned contracts, explicit permissions, stable semantic identifiers, compatibility policy, and deprecation windows. External consumers must not depend on private editor state or bypass document validation. Test extension failure, cancellation, timeouts, resource limits, and revocation.

### 9. Roll out changes with evidence and recovery

Every material architecture transition has a compatibility and migration plan, golden fixtures, observability, rollback/recovery procedure, and an estimate of implementation and operational cost. Preserve existing behavior while evidence is gathered. A proposal must state what it replaces, which consumers are affected, how data is backed up/restored, and the conditions to stop or roll back.

### 10. Keep design-system registry and runtime responsibilities distinct

The token registry owns semantic identity and definitions—stable IDs, types, scope, collections, modes, aliases, and bindings. A resolver/runtime evaluates an authorized registry snapshot plus document/platform context and returns resolved values; it may cache derived results but does not own token identity or silently flatten document bindings. Editor stores and renderer inputs are projections/consumers, not competing registries. Keep these boundaries versioned and test alias, mode, scope, binding, invalidation, and reload behavior across supported consumers.

The current design-system roadmap describes the Rust token registry/resolver as a **candidate** semantic owner; the canonical document contract records that token state is split and not durably document-scoped today. The [design-token domain contract proposal](DESIGN-TOKEN-DOMAIN-CONTRACT-PROPOSAL.md) inventories the current representations and proposes integration boundaries without selecting an owner. This principle preserves that distinction without approving Rust as the canonical document/schema owner or promoting the current registry to a completed product contract.

### 11. Make privacy and data governance architectural requirements

Classify document, asset, identity, and collaboration data; minimize collection and retention; enforce tenant/document/asset boundaries at service and agent/plugin interfaces; and make exports, deletion, backup, and recovery behavior explicit. External AI/model processing must be visible, limited to authorized context, and governed by explicit policy/consent. Do not claim local/private deployment, encryption, deletion guarantees, or training exclusions until each is specified, implemented, and independently tested.

## Architecture acceptance matrix

The following are required evidence categories for architecture proposals that affect documents, migration, agents, collaboration, platforms, or rendering. Detailed scope and target values are set per decision; a source inventory alone is not acceptance.

| Category | Minimum evidence before acceptance |
|---|---|
| **Fidelity** | Versioned source/target matrix; synthetic or legally usable golden fixtures; structural + visual + behavioral comparisons; fonts/assets and environment recorded; unsupported/approximate mappings emitted as loss reports; deterministic re-import/export where applicable. |
| **Operations and reversibility** | Valid/invalid operation fixtures; permission and precondition checks; preview/apply; undo and redo or explicit recovery path; save/reload; crash/retry/idempotency; rejected and partially failing operations leave no unintended state. |
| **Platform parity** | Named supported platform/device matrix; common contract tests for document and operations; representative workflows on each supported host; explicit exceptions with accessibility/input rationale; no silent semantic divergence. |
| **Performance and scale** | Frozen representative large-document corpus and hardware/browser matrix; input-to-visible latency, frames, startup, heap/memory and recovery measurements with percentiles; comparison against a validated baseline; no claim based solely on synthetic microbenchmarks. |
| **Security and isolation** | Threat model; least-privilege authorization tests; plugin/agent/import isolation; malicious and oversized input cases; access-revocation and cross-document denial; audit evidence; security review findings resolved or accepted by an owner. |
| **Privacy and data governance** | Data-flow/classification and retention inventory; authorized-context tests for agents/plugins; explicit consent/disclosure for external processing; deletion/export/recovery evidence; tenant and private-asset isolation tests; operational policy matches effective deployed configuration. |
| **Migration, rollback, and cost** | Source/API/export access verified for each proposed source/version; legal and operational constraints recorded; preview and fidelity report; rollback/data recovery tested; implementation, support, compute/storage, and maintenance costs estimated with assumptions. |

## Current constraints and relationship to existing records

The baseline [canonical contract](CANONICAL-DOCUMENT-CONTRACT.md) explicitly reports split representations and an unresolved end-to-end owner; this document does not resolve that finding. The [universal migration baseline](../strategy/UNIVERSAL-MIGRATION-ARCHITECTURE.md) describes current importer and access gaps. The [renderer plan](RENDERER-EVOLUTION-PLAN.md) preserves current evidence boundaries. These current-state documents remain intact and authoritative for their stated baseline until changed through the applicable architecture decision and engineering acceptance process.

The active P0.5 status is still gated: **#64** React/render integration pixels blocked; **#71** security work Draft; **#73** benchmark setup repaired, browser/heap measurement blocked. Merged **#67/#69/#70/#72** are foundations, not proof that these principles are implemented or that P0.5 is complete. See the [roadmap gate table](../roadmap/LOGOS-UNIFIED-PLATFORM-ROADMAP.md#active-p05-stabilization-gates).
