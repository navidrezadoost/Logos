# ADR-002: Canonical Document Authority and Transaction Policy

**Date:** 2026-11-19
**Status:** Target architecture policy — conditionally approved; implementation not authorized
**Decision authority:** Chief Architect (policy selection delegated to CTO)
**Baseline:** `8b9e6e08a80ee1336dbb151abb23d79aa31305ea`
**Related:** G1-D1; G0-B; G1-B1; G1-B4; G1-B9.3; G1-B9.4; FG-C3.2; FG-C3.8

## Context

Logos currently has multiple document representations and mutation boundaries. React/Zustand owns interactive editor state, Go persists legacy file data and coordinates existing revisions, Rust `logos-types` is a candidate semantic contract, offline storage contains local snapshots and pending-operation metadata, and renderer state is a derived projection. These do not yet form one approved, lossless, end-to-end canonical document transaction system.

This ADR records the selected target direction. It does not describe the production system as already migrated, transfer runtime authority, or authorize implementation.

## Decision

### D1 — Semantic and durable authority

Rust Core is the target owner of the versioned canonical document semantic contract, graph invariants, and deterministic pure operations. Go remains the target durable coordinator for authenticated authorization, revision coordination, atomic database commits, and durable operation receipts. React owns interaction state, optimistic previews, and editor projections. Renderer/WASM state remains derived and rebuildable.

Existing production owners remain unchanged until separate integration and compatibility gates pass. Rust becomes no production authority by virtue of this target decision.

### D2 — Authored document aggregate

The target is one versioned authored document aggregate containing local scene content, component definitions and sets, instances, variants, overrides, and authored slot content. The authored main component subtree is the definition graph; expanded instance children are derived projections, not independently authored copies.

External component libraries remain separately owned and versioned. A document reference must pin an immutable library identity and revision or content identity. A durable operation receipt must preserve dependency identity directly or through an immutable committed document revision from which the dependency can be recovered. It must never resolve against whichever library version happens to be latest later.

Before integration, this aggregate must be reconciled with the TypeScript Component/Instance v1 semantic reference and saved-file compatibility policy.

### D3 — Identity

Document, page, shape, component, and operation identities are issued at explicit creation boundaries and preserved thereafter. New online and offline issuance requires one documented UUID validation, ownership, collision, and first-acceptance protocol. Existing IDs are never silently regenerated during load, validation, or migration. Legacy non-UUID IDs remain under a versioned compatibility path or are explicitly rejected.

Imported identities require stable source provenance and namespace ownership before deterministic remapping can be relied upon. The legacy nil-root structural identity and renderer synthetic nil root are distinct concepts; any adapter must use an explicit, versioned mapping rather than silently rewriting persisted IDs.

### D4 — Compatibility

The policy is lossless conversion or explicit rejection. Unknown or unsupported fields may be preserved only when losslessness is demonstrated. Otherwise conversion rejects or remains read-only with original data intact; a partial conversion must never be silently saved over the source.

Fixture classifications are `LOSSLESS`, `UNSUPPORTED_PRESERVED`, `INVALID`, or `AMBIGUOUS`. `AMBIGUOUS` blocks qualification and leaves the source unchanged; it does not authorize a quarantine runtime.

Corpus governance is binding: the Chief Architect authorizes scope, source classes, and acceptance policy; the accountable data/environment owner authorizes fixture use, sanitization, storage, access, and retention; engineering produces reproducible read-only evidence; the CTO independently reviews the technical evidence; and the Chief Architect retains final migration authority. G1-D2 requires separately approved scope and an authorized sanitized representative corpus.

### D5 — Revision fencing

New typed canonical operations use exact expected-revision and restore-epoch fencing by default. A stale base is rejected unless that operation kind has a separately qualified deterministic rebase rule. Existing Go legacy OT/revision behavior remains unchanged during transition. `revn` and `vern` must have distinct documented meanings; a snapshot-origin revision must not be represented as the current row revision.

No production API shape is approved by this ADR.

### D6 — Durable operation identity and receipts

Every future typed logical mutation carries a stable operation identity. The server durably associates it with its scope, authenticated actor, canonical request fingerprint, committed result, resulting revision, and applicable external dependency revisions. Mutation and successful receipt commit atomically. A repeat delivery of the same logical request returns its original committed result; reuse of the identity for a different request is rejected.

`(documentId, actorId, operationId)` is a candidate scope, not a finalized contract; actor stability, uniqueness, receipt lookup authorization, retention, deletion, and document lifecycle remain to be decided. The goal is at most one committed mutation per logical operation identity with a durable repeatable outcome. This is not a claim of exactly-once execution or network delivery. Fault injection must show response loss after commit cannot cause a second logical mutation.

### D7 — Authorization linearization and lock ordering

Write authorization must have a defined transactional linearization point relative to revocation and commit, producing deterministic ordering between an authorized commit and permission revocation.

Before transaction qualification, perform a cross-writer lock-order and concurrency analysis covering ordinary Go updates, snapshot creation, snapshot restore, permission revocation, file deletion, and future typed commits. Include offline synchronization when it reaches these writers. Document lock order, cycles, isolation assumptions, deadlock behavior, and test revocation-versus-commit ordering under controlled concurrency. A lock sequence analyzed only for a future endpoint is insufficient.

### D8 — History, offline replay, undo, and redo

Durable document history and authoritative server revisions define accepted operation order. Redis/WebSocket messages are delivery notifications, not commit truth. Offline pending operations retain stable identity, original base revision, restore epoch, payload, and relevant dependencies until resolved.

If history needed for replay has been compacted or is unavailable, the client fetches the authoritative snapshot, re-evaluates pending intent against the new revision, and either submits an authorized commit or exposes a conflict. It must not treat missing events as absent, replay stale serialized state as an unconditional replacement, or silently discard pending user work.

Undo and redo are newly validated and authorized logical operations with stable identities and explicit conflict rules, not blind document replacement. The future protocol must define compaction horizons, acknowledgments, rebase eligibility, dependency ordering, inverse behavior, history gaps, and reconnect recovery. This does not select a new CRDT or replace legacy OT.

### D9 — Snapshot restore

The future canonical restore is a new forward-moving revisioned operation with a durable receipt. Snapshot-origin revision and resulting current revision are distinct. Restore-aware fencing and reconciliation remain required for existing clients, pending operations, and history cursors.

During transition, preserve existing Go behavior: restore increments `vern` while leaving the current row `revn` unchanged. Qualify and clarify legacy response semantics before typed clients depend on them. This decision alone does not authorize an API or persistence change.

### D10 — Server-side semantic enforcement gate

The Go durable commit boundary must eventually enforce the approved semantic contract through a qualified trusted execution mechanism. The mechanism is **not selected**. Candidates are a native Rust library/FFI, a supervised local Rust process, or a dedicated Rust service.

The CTO's provisional investigation preference is a supervised local Rust process, subject to proof of actual document-operation semantic parity, process lifecycle and recovery, crash containment, executable provenance, bounded resources, and deployability. Existing FG-C3 experiments concern an isolated Component/Instance validator; they do not qualify a canonical Document mutation engine.

No typed canonical production commit may be activated until the Chief Architect approves the enforcement mechanism and failure policy. If the semantic engine is unavailable, the path fails closed; Go must not silently use an unqualified duplicate validator.

## Unresolved gates

These gates remain open and block their stated downstream work:

| Gate | Unresolved requirement | Blocks |
| --- | --- | --- |
| G1-D1-A1 | Choose and qualify the Rust enforcement location and failure policy | Typed production commits |
| G1-D1-A2 | Authorize a sanitized corpus, name its data owner, and approve acceptance criteria | G1-D2 compatibility qualification |
| G1-D1-A3 | Complete cross-writer lock-order, isolation, and concurrency analysis | Transaction qualification |
| Identity | Define online/offline issuance, promotion, and legacy nil-root mapping | Canonical adapter adoption |
| Receipt | Finalize identity scope, retention, lookup security, lifecycle, and recovery contract | Durable idempotency implementation |
| Restore | Define legacy API interpretation and canonical migration contract | Typed restore integration |
| Component references | Pin external library revisions and ensure dependency reconstruction | Cross-document component integration |

Additional conditions before any new engineering authorization include accepted compatibility classifications, a clear boundary between typed and legacy APIs, and a separately scoped work order with tests, rollback conditions, and freeze-point checks.

## Consequences

This target prioritizes consistent semantics, data preservation, and recoverable commit correctness over immediate Rust integration. It preserves current React/Go production behavior while the target execution boundary and compatibility evidence are qualified. It also requires versioned adapters and separately authorized migration steps rather than a big-bang replacement.

## Explicit non-authorizations

This ADR does not authorize G1-D2 execution or customer-derived corpus use; production React, Rust, or Go implementation; persistence/schema changes; canonical ownership transfer; Rust authority transfer or G1-B10; Component/Instance capability advertisement or reopening FG-C3.8; production deployment or performance claims; a new PR or merge; or changes to `main`.

FG-C3.8 remains **CLOSED — NO-GO** for capability exposure. G1-B9.6 retained memory remains **NOT MEASURED**. G1-B10 and Rust production authority transfer remain **NO-GO**.

## Final disposition

**Target policy: CONDITIONALLY APPROVED. Implementation and migration: NOT AUTHORIZED.** All nine policy areas have selected target directions. D10 remains an explicit blocking decision gate; no production Rust enforcement mechanism has been selected or activated.
