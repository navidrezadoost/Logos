# G1-B6: Go snapshot and restore integrity

## Contract

The Go snapshot endpoints now treat `file_change` metadata and the associated
`file_data` bytes as one logical record:

- Snapshot creation locks the active file row, reads its revision and content
  under that lock, rechecks edit permission inside the transaction, and inserts
  the labelled history row and content blob atomically. Empty or absent content
  is rejected with HTTP 409 (`snapshot-content-unavailable`). A SHA-256 digest
  is stored under `file_data.metadata["snapshot-integrity"]` with envelope
  version 1. No database schema migration is required.
- Restore locks the selected history row and file row, rechecks edit permission,
  and requires a non-empty blob belonging to that exact `(file_id, id)` with
  type `snapshot`. A missing blob returns 409
  (`snapshot-content-unavailable`); a malformed or mismatching versioned
  integrity envelope returns 409 (`snapshot-content-corrupt`). Both responses
  leave file content, revision counters, history, and thumbnails untouched.
- A historical snapshot with bytes but without the digest envelope remains
  restorable for compatibility and is reported as `legacy-unverified`. New
  snapshots report `verified`; snapshots without usable bytes report
  `unavailable`. This classification is included in snapshot-list responses.
- Before restore, the current file bytes are saved as a system
  `internal/pre-restore/<revn>` snapshot, including their digest. That history
  row, blob, restored file bytes, generation increment, and thumbnail
  invalidations commit together. Any write error rolls the operation back.
- Permission rows used for authorization are held `FOR SHARE` until commit.
  Permission revocation therefore serializes with snapshot creation and
  restoration. The transaction rechecks both team and project membership.

The content digest detects accidental or partial corruption; it is not a
cryptographic authenticity mechanism. Historical blobs without a digest are
intentionally labeled as unverified instead of being silently upgraded.

## Verification

The isolated integration suite is
`backend-go/internal/handler/files_snapshot_integration_test.go`. It covers
revision/content binding, blob-write rollback, serialization with concurrent
file updates, permission revocation, missing/empty/corrupt snapshot rejection
without mutation, restore rollback and retry, and legacy complete/incomplete
snapshot behavior.

Run against a fresh disposable PostgreSQL instance only:

```sh
TEST_DATABASE_URL='postgres://<user>:<password>@127.0.0.1:<port>/logos_p04_snapshot?sslmode=disable' \
  bash test/p0.5/browser-persistence/initialize-disposable-db-schema.sh

cd backend-go
TEST_DATABASE_URL='postgres://<user>:<password>@127.0.0.1:<port>/logos_p04_snapshot?sslmode=disable' \
  go test -race ./internal/handler \
    -run 'Test(CreateSnapshot|Restore|HistoricalComplete)' \
    -count=1 -timeout=180s -v
```

The existing schema initializer checks loopback URL shape, the `logos_p04`
database prefix, and schema emptiness. Those checks do **not** prove that a
database is disposable or authorized. The operator must independently verify
that the selected database instance is isolated, newly provisioned for this
test, contains no valuable data, and is authorized for destructive test setup.
The integration tests create and delete test rows and install temporary
database triggers; never point them at production or a shared environment.

## Remaining limitations

- Old snapshots with non-empty bytes and no digest can only be classified
  `legacy-unverified`; their historical integrity cannot be established.
- The digest has no secret key and does not protect against a database actor
  who can modify both content and metadata.
- Restore continues to leave `revn` unchanged and increments `vern`, matching
  the existing Go behavior. The CRDT restore semantics remain outside this
  change.
