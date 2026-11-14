# G0-B AI/ML Data Readiness

**Status:** Future interface proposal only. No model training, document collection, embedding store, semantic sidecar or autonomous commit is introduced.

## Current document context available

The app has shape/page data in `documentStore`, selection in a UI store, component metadata split between shape values and `componentStore`, prototype interactions in `prototypeStore`, tokens in separate stores, and renderer projection in `scene-projection`. These are incomplete and partly transient. There is no one versioned semantic document view that a tool can inspect; current AI store includes direct intent-to-action shortcuts. Therefore agent document editing is not safe to treat as a domain operation source today.

## Proposed read-only context and proposal contract

An eventual `DocumentContext` read API should return a revision-bound, versioned snapshot:

- document/page identity and schema version;
- stable node IDs, node kinds, parent/ordered children, page membership and semantic roles (Frame/Artboard, Group, Component, Instance);
- local and world geometry with units and transform provenance;
- layout container/item constraints and whether geometry is authored or derived;
- direct style values plus optional typed token refs, fallback values and resolution status;
- component definitions, variant/property definitions and instance overrides;
- prototype flow/interaction graph and unresolved target diagnostics;
- available operation kinds, required scopes, parameter schemas and capability limits;
- current revision/hash plus read-only validation diagnostics.

Agent output is a **proposal**, not mutation:

```text
Proposal { protocolVersion, documentId, baseRevision, intentId,
           operations: TypedOperation[], rationaleRefs, expectedImpact }
Validate(snapshotAtBase, authenticatedContext, proposal)
  -> Rejected { diagnostics }
  | Preview { normalizedOperations, previewProjection, impact, inversePreview }
User approval + server authorization + matching revision
  -> Commit(approved plan) -> revision receipt
```

Required boundary: Natural-language interpretation → typed proposal → deterministic validation → user preview → authorized commit. The model cannot allocate durable IDs, assert server permissions/revisions, bypass domain validation, or become a document-state authority. A stale revision must return conflict diagnostics and require a fresh read/preview.

## Safety, provenance and evaluation requirements (deferred)

- Context is least-privilege and document-scoped; redact private text, comments, user identity, external URLs and hidden/off-page content unless explicitly required and consented.
- Tool inputs should identify source revision and field provenance. Never train on customer documents by default; obtain purpose-specific opt-in, retention limits and deletion support before any future dataset use.
- Keep dataset provenance, license/consent, redaction audit, tenant isolation and synthetic-vs-user origin metadata.
- Use document-level (not shape-level) train/eval split to prevent near-duplicate leakage; reserve adversarial/imported/collaborative cases; publish task-specific quality and safety metrics.
- Replay every proposal from a versioned snapshot and deterministic operation implementation; store outcome/diagnostics only under a defined, consented retention policy.
- Test invalid IDs, cross-file refs, deleted targets, cycles, oversized operation plans, unauthorized actions, stale revisions, prompt injection embedded in document text and sensitive-data exfiltration attempts.

## Current gaps and sequencing

No model training, embedding/vector database or dataset capture is justified. First establish the canonical document/schema and operation boundary, expose read-only structured context, and prove deterministic preview. Only then evaluate real use cases and consent/governance. Token and prototype sources must be unified or explicitly referenced before agents can reason over them consistently.
