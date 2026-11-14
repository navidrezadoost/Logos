# G1-B1 — Canonical Document Ownership (Proposal)

**Status:** Proposed; requires Chief Architect decision. This document does not approve migration or production adoption.

## Context

Logos currently has multiple document representations with different fields and boundaries: React/Zustand editing shapes and page state, Rust `logos-types::Shape`, Go Transit-compatible file data, and renderer projections. They are not interchangeable, and there is no end-to-end lossless conversion contract. G0-B also found that component, token, prototype, and document state do not yet share a proven persistence boundary.

## Candidate decision

Adopt a Core-first, Web-first ownership model for future work:

| Concern | Candidate authority | Boundary |
|---|---|---|
| Document meaning, typed values, graph invariants, pure operations | Rust core | Versioned domain contract; deterministic diagnostics; no I/O |
| Durable file revisions, authorization, transaction boundary | Go service | Persists an approved contract revision only after architecture and transactional integrity decisions |
| Selection, viewport, inspector state, transient edit preview | React/Zustand | Editing projection and UI state; typed operations proposed to core |
| Scene graph, GPU objects, layout/render caches | Renderer/WASM | Derived and rebuildable; synthetic IDs never persisted |
| Offline state | Client cache | Recoverable snapshot plus revision-bound pending operations; reconciliation policy remains to be designed |
| Agent context | Read-only semantic projection | Agents may propose typed operations; no direct document mutation |

This model preserves the useful distinction between Rust semantic authority and Go durable revision authority. It does not assume that the Go server stores Rust structs directly.

## Alternative: retain legacy wire as canonical domain schema

Keeping Transit/file-data as the canonical schema minimizes immediate storage migration and matches existing reads/writes. It also leaves semantic rules spread across Clojure-compatible Go maps, React models, and Rust DTOs; graph validation becomes format-specific; unknown fields and importer behavior remain difficult to prove. Rust can still validate an adapter projection, but the adapter becomes a permanent authority boundary with its own compatibility and failure modes.

## Costs and risks

- **Compatibility:** current React fields (`bounds`, `children`, `parentId`, UI-only `locked`, text/layout/component payloads) differ from Rust fields (`selrect`, `shapes`, `parent-id`, `frame-id`, fills/strokes, plugin data). Conversion can reject or lose data unless an adapter enumerates every field.
- **Migration:** historical file blobs, feature gates, enum keyword encoding, root-frame conventions, and file versions need real-fixture coverage. Synthetic fixtures in this PR do not establish historical compatibility.
- **Operations:** optimistic revision conflicts, offline queues, atomic multi-operation transactions, authorization, and collaboration ordering require an approved protocol.
- **Failure modes:** unknown-field loss, stale page ownership, partial multi-shape mutation, importer ID collisions, and divergence between client candidate and persisted revision must fail closed.
- **Operational complexity:** versioned adapters, observability, rollback, and staged dual-read validation are required before changing durable formats.

## Decision requested

Approve or amend the ownership boundaries above, and separately decide whether/when the proposed Rust contract may become a production read/write boundary. Until then, the G1-B1 contract and operation semantics are an executable candidate only. No Go persistence, saved-file format, React store, or renderer integration is changed here.

## Consequences if approved later

Rust becomes the place where document meaning and pure invariants are defined; Go remains the authority for durable revisions and transactions; React adapts editor interactions into validated operations; render data remains derived. A separate versioned migration plan and historical corpus are prerequisites.
