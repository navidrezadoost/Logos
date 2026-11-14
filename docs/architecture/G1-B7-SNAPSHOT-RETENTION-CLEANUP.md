# G1-B7 — Snapshot retention cleanup: source audit and safe extension proposal

## Decision

**EXTEND the existing legacy `objects-gc` mechanism if that worker remains an
operated service for the target database. Do not add a Go scheduler or a new
worker.** The worker in legacy source revision
`665bb1ef13c3eeff2e84bf6e0d588a6bdecc6a92` already removes expired
`file_data` and `file_change` rows in chunks. The current verified `main`
(`300df61df9fec640f7f388156573d838db2bc8a4`) does not contain that Clojure
runtime or worker source. Therefore this repository cannot establish that the
legacy worker is still deployed, enabled, connected to the same database, or
safe to extend here.

This PR stops at the architecture boundary and adds a disposable PostgreSQL
characterization test. **Physical cleanup through the current Go deployment is
not implemented or verified.** A deployment owner must establish worker
ownership and database reachability before a follow-up can extend it. If the
legacy worker is not operated, this remains an architecture decision; this PR
does not substitute a standalone service.

## Source audit

The following facts were checked against the current baseline and, where
identified as historical, against source revision
`665bb1ef13c3eeff2e84bf6e0d588a6bdecc6a92`:

| Area | Source evidence | Finding |
| --- | --- | --- |
| Go runtime | `backend-go/cmd/server`, `backend-go/internal/server`, `backend-go/internal/handler/files_snapshot.go` | Snapshot create/list/restore exists. No Go worker, scheduler, or physical snapshot/blob purge exists in this tree. |
| Legacy `:delete-object` | `backend/src/app/tasks/delete_object.clj`, historical revision above | `:snapshot` updates `deleted_at` on matching `file_change` and `file_data`; it is a logical marker operation, not physical removal. |
| Legacy file GC | `backend/src/app/tasks/file_gc.clj` and `file_gc_scheduler.clj`, historical revision above | Trims media, thumbnails, fragments and deleted components for eligible files. It is not the recovery-point payload purge. |
| Legacy object GC | `backend/src/app/tasks/objects_gc.clj`, historical revision above | Deletes expired `file_data` and `file_change` rows. Each query orders by expiration, uses `LIMIT 100` and `FOR UPDATE SKIP LOCKED`; the task repeats chunk transactions until empty. The `file_data` selector is generic (`deleted_at IS NOT NULL AND deleted_at <= timestamp`) and does not distinguish snapshots, fragments, or recovery points. |
| Legacy schedule | `backend/src/app/main.clj`, `worker-config`, historical revision above | Schedules `:objects-gc` daily at midnight. This proves the schedule in that source revision, not current production deployment or execution. |
| Storage-object GC | historical `objects_gc.clj`, `storage-gc-deleted`, `storage-gc-touched` task schedule | Storage-object byte collection is a separate lifecycle. `file_data` snapshot bytes are PostgreSQL rows, not `storage_object` assets. |
| Go snapshot records | `backend-go/internal/handler/files_snapshot.go` | Generated pre-restore rows use `file_change.created_by='system'`, label `internal/pre-restore/<revision>`, `file_change.deleted_at = now()+48h`; the matching `file_data` row uses the same `(file_id,id)`, `type='snapshot'`, and expiry. |
| PostgreSQL relations | Historical migration `0141-add-file-data-table.sql` and `0022-page-file-refactor.sql` | `file_data` has PK `(file_id,id)` and a file FK; it is hash partitioned by `file_id`. `file_change.id` is its own PK and also references `file`; there is no FK from a snapshot blob to its `file_change`. The join and `type='snapshot'` are application-level identity checks. |

### Retention states

- **ACTIVE:** the system recovery marker is absent or its 48-hour `deleted_at`
  timestamp is at or after database `now()`. Existing authorized Go list/restore
  paths allow system snapshots only through this cutoff. User snapshots remain
  governed by their separate user lifecycle.
- **EXPIRED:** the matching system pre-restore `file_change` and snapshot
  `file_data` expiries have passed. The Go list/restore path hides/rejects it;
  current Go code may still retain its bytes.
- **PURGED:** matching snapshot payload bytes are absent after a committed
  cleanup operation. Metadata may remain for diagnostics unless a separately
  authorized policy removes it. A missing payload must report unavailable and
  must never be restored as empty content.

The compatibility window remains 48 hours. This does not set a physical purge
SLA. The historical daily job, where operated, may run later than the logical
expiry; current scheduling and downtime recovery have not been verified.

## Safe extension contract

Before changing a production cleanup path, establish that the legacy worker is
the currently operated owner for this PostgreSQL database. Then extend that
worker in place; retain its bounded transactions, retry behavior and operator
visibility. Do not start a second scheduler against the same rows.

The target classifier must require all of the following:

1. `file_change.file_id` and `file_data.file_id` both equal the target file ID.
2. `file_change.id` equals `file_data.id`.
3. `file_change.created_by = 'system'` and
   `file_change.label LIKE 'internal/pre-restore/%'`.
4. `file_data.type = 'snapshot'`.
5. Both expiration timestamps are non-null, equal, and strictly earlier than a
   single transaction cutoff captured at the start of the batch.

Do not use `deleted_at IS NOT NULL` alone, do not infer system recovery status
from a generic system creator, and do not touch ordinary history rows,
user-created snapshots, fragments, thumbnails, media, or unrelated assets.
Rows that fail the classifier (including incomplete historical rows) should be
reported/skipped for an explicit compatibility decision, not opportunistically
deleted.

### Transaction, locks and concurrency

The restore path locks `file_change` before locking the file and matching
`file_data`. A safe cleanup extension must preserve that lock order. In one
short transaction, select no more than the configured batch limit by locking
eligible `file_change` rows first with `FOR UPDATE SKIP LOCKED`, then lock the
matching `file_data` rows, recheck the strict expiry and exact identity/type
conditions, and delete only the payload rows. If restore holds the recovery
metadata lock, cleanup skips it and retries in a later batch. If cleanup locks
first after expiry, restore must continue to fail closed. Keep metadata so the
expired item remains diagnosable as unavailable; do not cascade into
`file_change` or the file row.

The historical generic `objects-gc` implementation locks `file_data` and
`file_change` in separate per-table passes and does not coordinate the target
recovery metadata row before deleting payload. Its general-purpose semantics
are useful reuse evidence, but they are not proof of the stronger restore race
contract above. This is why code changes stop until the worker's active owner
can be identified and the lock-order extension can be made in its maintained
source.

On interruption, a transaction rolls back and the next run retries. Repeated
runs find no deleted payload for an already purged target and must report zero
additional purges. Candidate count, purged count, duration, errors, last
successful completion and a safely computed retained-byte estimate should be
reported without filenames, document contents, payloads or credentials. A
retained-byte estimate should use `octet_length(data)` only for the exact
eligible classifier and be labeled approximate until validated against
PostgreSQL TOAST/storage accounting.

## Characterization and evidence

`TestExpiredPreRestorePurgeScopeCharacterizationIntegration` inserts disposable
fixtures for an expired and active generated recovery point, an expired user
snapshot, another system snapshot label, and an expired fragment. It proves a
generic `deleted_at` scan would select four rows while the proposed exact
recovery-point classifier selects only one. It then demonstrates that deleting
only that expired fixture payload leaves its metadata and all non-target
payloads. The SQL is deliberately test-only; it is not wired to a production
worker and does not prove worker operation, concurrency safety, or a purge SLA.

Existing `TestExpiredPreRestoreRecoveryPointIsUnavailableButRemainsStoredIntegration`
continues to prove logical expiry and retention of bytes in the Go API path.
The PostgreSQL integration workflow provisions a fresh disposable PostgreSQL
service and runs both tests. Hosted CI on the final PR head is required before
this characterization is accepted.

## Deployment-owner checklist (not performed by this PR)

- [ ] Identify the currently deployed backend and cleanup-worker build SHAs.
- [ ] Confirm whether the legacy `objects-gc` schedule is enabled and operating
  against the same database used by Go snapshot endpoints.
- [ ] Confirm owner, schedule, task retries, last successful run, and alerting;
  capture sanitized evidence with credentials and database identifiers
  redacted.
- [ ] Verify cleanup is paused or safely bounded during any schema/worker
  rollback; confirm the worker does not remove active recovery payloads.
- [ ] Approve a lock-order-aware extension and a disposable PostgreSQL
  concurrency/failure test before enabling physical pre-restore cleanup.
- [ ] Observe candidate/purge counts and errors in non-production first. Define
  a purge SLO only after schedule, downtime catch-up and rollback behavior are
  verified.
- [ ] Do not enable, reconfigure, or test against production as part of this
  PR.

## Status and residual risks

- Current Go production security status: **UNVERIFIED**.
- Recovery-point physical purge through the current deployment: **UNVERIFIED**.
- Historical daily `objects-gc` source and generic cleanup semantics: **SOURCE
  VERIFIED AT THE STATED HISTORICAL SHA ONLY**.
- Worker deployment continuity, DB ownership, schedule, and last successful
  execution: **UNVERIFIED**.
- Physical cleanup race safety: **NOT IMPLEMENTED / NOT VERIFIED**.
- Retention SLA: **NOT ESTABLISHED**.

No production database, configuration, React/Rust renderer, or canonical
document code was changed.
