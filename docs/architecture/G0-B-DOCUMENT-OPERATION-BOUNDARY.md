# G0-B Document Operation and Persistence Boundary

**Status:** Proposal only. No operation protocol or Go persistence behavior is changed here.

## Current persistence findings

Go `UpdateFileHandler` authenticates, checks edit permission, begins a transaction, locks the file row, checks `vern`/`revn`, loads complete stale history for rebase, inserts a `file_change`, increments revision and updates `file.data` within the same DB transaction (`backend-go/internal/handler/files_update.go:130-289`). Missing or malformed competing revisions fail closed (`:338-386`). This is a meaningful transactional core, but it stores legacy operation payloads, has no domain graph validation, and is not proof of complete collaboration/undo semantics.

The handler checks permission before acquiring its file row lock (`:173-197`); any permission revocation semantics during a long-running update should be specified. It ignores failure to update project `modified_at` (`:283-287`). Redis broadcast occurs after DB commit and is best effort/asynchronous (`:294-305`); clients need durable change-history polling/replay as recovery, not an assumption of exactly-once event delivery. Lagged history is loaded after commit and load errors become an empty list (`:308-317`).

Snapshot handling is less complete. `CreateFileSnapshotHandler` reads file revision/data before starting its insert transaction (`files_snapshot.go:145-164`), inserts the label row and ignores snapshot blob insert failure (`:166-185`). Restore (`:316-424`) writes `file.data` only when snapshot blob exists; otherwise it bumps `vern` without restoring data and still returns snapshot `revn`. The source comment explicitly calls Go restore partial. Auto-snapshot and thumbnail errors are ignored in parts of restore.

| Concern | Status | Evidence / next proof |
|---|---|---|
| Revision owner | IMPLEMENTED in Go handler | File row `FOR UPDATE`; increments under transaction. Verify all writers use same path/lock. |
| Atomic change + file blob + revision | IMPLEMENTED for main update | Change insert and file update occur in same transaction. Project timestamp update error ignored. |
| Optimistic concurrency | PARTIAL | `vern` equality and future `revn` rejected; stale revn rebased using ordered history. Validate request limits and semantics across every writer. |
| Operation ordering | PARTIAL | `ORDER BY revn ASC` and Go rebase; payload types remain Penpot operations, not approved typed domain commands. |
| Idempotency/retry | MISSING / UNVERIFIED | No stable client operation ID/idempotency key checked in this handler. HTTP retry could submit same semantic edit twice. |
| Authorization | PARTIAL | `CanEdit` checked before transaction; exact atomicity with permission revocation is not established. |
| Change-history integrity | PARTIAL, fail-closed for stale rebase | Missing/malformed revision blocks stale update. Ensure all legacy row encodings are understood or explicitly migrated. |
| Snapshot consistency | PARTIAL | Read-before-transaction; data copy errors ignored; restore without data is metadata-only. |
| Rebase/conflict handling | PARTIAL | Go rebase exists; Rust `logos-rebase` also exists, creating duplicate algorithm implementations. No shared differential fixture evidence cited here. |
| Undo/redo | UNVERIFIED | No complete typed inverse/compensation contract found across UI, Go, Rust. |
| Collaboration replay | PARTIAL | DB history plus Redis broadcast; delivery is best-effort; idempotent client replay/ack contract not established. |

## Typed operation proposal

Conceptual operation kinds: `CreateShape`, `UpdateShape`, `DeleteSubtree`, `ReparentShape`, `ReorderChildren`, `SetStyle`, `SetLayout`, `BindToken`, `CreatePrototypeInteraction`. These are future contract examples, not new runtime interfaces.

Every operation envelope should eventually carry:

```text
protocolVersion
operationId                 // stable UUID, unique for retry scope
documentId
baseRevision                 // exact expected state revision
actor/session metadata       // authenticated by server, not trusted from body
kind + typed payload
preconditions                // field or object hashes/revisions as required
conflict policy              // reject/rebase/merge, defined per kind
```

Application must be deterministic over an immutable snapshot, validate all affected references and graph invariants, produce structured accepted/rejected result and diagnostics, and preserve an inverse or compensation record. Duplicate operation IDs return the original receipt rather than applying twice. Rebase must be operation-kind-aware and tested against golden histories; blind generic patch merging is insufficient. Undo is a new revision-bound inverse command, not a history rewind that invalidates collaborator state.

## Boundary proposal and decision gate

1. React creates user intent and preview only; it cannot claim a server revision or authorization from local state.
2. A typed domain core validates a candidate operation against a versioned snapshot and returns deterministic diagnostics/inverse data.
3. The persistence owner (not selected by this audit) authenticates actor, checks authorization and exact base revision, stores accepted operation + canonical state atomically, and returns a receipt.
4. Renderer receives a derived projection; renderer recovery never changes durable state.
5. Clients replay accepted operations by revision and recover gaps from durable history; Redis/WebSocket notification is only a hint.

Go's row-lock transaction is a plausible durable transaction mechanism, but currently applies legacy operation semantics and has partial snapshots. Alternatives are server-authoritative operations with Rust FFI validation, a Rust service/core behind a versioned API, or Go transport plus deterministic Rust validation. Each requires a transaction/rollback boundary, authorization recheck policy, versioned payloads, operational deployment plan and compatibility fixtures. The Chief Architect must decide canonical ownership; this audit does not.

## Migration and risk controls

- Preserve current `file.data`, `file_change` and Transit-compatible values until historical schema provenance and golden fixtures exist.
- Introduce a versioned operation envelope alongside existing protocol only after dual-read/compatibility strategy and unknown-field policy are approved.
- Shadow-validate candidate operations first; compare without mutation; log diagnostic codes only, never document contents/secrets.
- Include idempotency, snapshot consistency, malformed history, permission change, retry-after-timeout and replay-gap tests before enabling writes.
- No production configuration or Go code was changed in G0-B.
