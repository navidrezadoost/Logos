---
title: 3.06. Backend Guide
desc: "Logos Backend Guide: Go server, RPC handlers, database migrations, auth, and file format."
---

# Backend Guide

The Logos backend is a Go HTTP server located in `backend-go/`. It handles all RPC commands
from the frontend, manages authentication and sessions, reads/writes PostgreSQL, publishes
to Redis, and serves the `.logos` file format.

---

## Running the Backend

### Development mode

```bash
export DATABASE_URL="postgres://logos:logos@localhost:5432/logos"
export REDIS_URL="redis://localhost:6379"
export LOGOS_SECRET_KEY="dev-secret-at-least-32-characters"

cd backend-go
go run ./cmd/server
```

The server starts on `:6060` by default. Set `BACKEND_GO_ADDR` to change the address.

### Apply database migrations

```bash
go run ./cmd/server -migrate
```

Migrations are pure SQL files in `backend-go/migrations/`. They run in filename order and
are idempotent (tracked in a `migrations` table). Never modify an existing migration file —
add a new one instead.

### Health check

```bash
curl http://localhost:6060/api/_health
# → {"status":"ok","version":"dev"}
```

---

## Project Structure

```
backend-go/
├── cmd/
│   └── server/main.go       # entry point — env config, DB pool, Redis, HTTP server
├── internal/
│   ├── auth/
│   │   ├── issue.go          # JWE token issuance (sessions + API tokens)
│   │   ├── password.go       # Argon2id hashing (compatible with legacy hashes)
│   │   └── session.go        # JWE middleware — cookie → profile ID
│   ├── binfile/
│   │   ├── v3.go             # .logos / .penpot ZIP export/import
│   │   └── v3_test.go        # round-trip tests
│   ├── config/config.go      # env-var configuration struct
│   ├── db/postgres.go        # pgx/v5 connection pool wrapper
│   ├── email/email.go        # SMTP client (transactional email)
│   ├── handler/              # one file per RPC namespace
│   │   ├── auth.go           # login, register, logout, magic link, recovery
│   │   ├── access_token.go   # API token CRUD
│   │   ├── binfile.go        # .logos export/import HTTP handlers
│   │   ├── comments.go       # comments + threads
│   │   ├── demo.go           # demo profile creation
│   │   ├── feedback.go       # user feedback submission
│   │   ├── files.go          # file CRUD, sharing, viewers
│   │   ├── files_snapshot.go # labeled version snapshots
│   │   ├── files_thumbnails.go
│   │   ├── files_update.go   # OT rebase, Redis broadcast, row-level lock
│   │   ├── fonts.go          # custom font upload/list/delete
│   │   ├── management.go     # duplicate project, move files/projects
│   │   ├── media.go          # media object upload/list/delete
│   │   ├── profile.go        # profile CRUD
│   │   ├── projects.go       # project CRUD + pin
│   │   ├── search.go         # full-text file search
│   │   ├── teams.go          # team CRUD, members, roles
│   │   ├── teams_invitations.go
│   │   ├── verify_token.go   # email/magic-link verification
│   │   └── webhooks.go       # webhook CRUD + async delivery
│   ├── perms/perms.go        # team/project/file permission helpers
│   ├── rebase/               # pure-Go OT rebase engine
│   │   ├── rebase.go
│   │   └── rebase_test.go    # 20 test cases (5×5 conflict matrix)
│   ├── redis/redis.go        # Redis/Valkey client wrapper
│   ├── server/server.go      # chi router — all 25+ routes wired
│   └── storage/storage.go   # storage backend (local FS / S3)
└── migrations/               # SQL migration files
```

---

## Authentication

### Sessions

Sessions are JWE tokens (alg=A256KW, enc=A256GCM) stored in an HTTP-only cookie named
`logos-auth`. The token contains:

```
{iss: "authentication", aud: "logos", sid: <session-id>, uid: <profile-id>, iat: <unix-ts>}
```

The `JWEMiddleware` in `internal/server/server.go` verifies the token and injects the
`profileID` into the request context. Handlers access it with `auth.ProfileIDFromContext(ctx)`.

### API Tokens

API tokens are long-lived JWE tokens with `iss: "token"`. They are created via
`create-access-token` and verified via `verify-token`. They bypass the session cookie
and are passed as `Authorization: Token <token>`.

### Password Hashing

Passwords are stored as Argon2id PHC strings (memory=32768, time=3, parallelism=2).
This matches the parameters from the original Clojure backend (`buddy-hashers`), so
existing password hashes continue to work without re-hashing.

---

## RPC API Pattern

All RPC commands are `POST /api/rpc/command/<name>` with `Content-Type: application/json`.

Request body varies per handler but always goes through `json.NewDecoder(r.Body).Decode(&req)`.
Response is always JSON. Errors use the standard envelope:

```json
{"type": "error", "code": "not-found", "hint": "file not found"}
```

### Adding a New Handler

1. Create `internal/handler/myfeature.go`:

```go
package handler

func MyFeatureHandler(deps *deps) http.HandlerFunc {
    return func(w http.ResponseWriter, r *http.Request) {
        profileID, ok := auth.RequireAuth(w, r)
        if !ok {
            return
        }
        var req struct {
            Name string `json:"name"`
        }
        if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
            writeError(w, http.StatusBadRequest, "invalid request")
            return
        }
        // ... business logic ...
        writeJSON(w, http.StatusOK, result)
    }
}
```

2. Register in `internal/server/server.go`:

```go
r.Post("/api/rpc/command/my-feature", h.MyFeatureHandler(deps))
```

---

## Database Migrations

Migrations live in `backend-go/migrations/`. The naming convention is:

```
NNNN-<verb>-<table>-<description>.sql
```

Examples:
```
0025-add-file-thumbnail-table.sql
0026-mod-profile-add-is-active.sql
0027-del-deprecated-tokens-table.sql
```

To create a new migration:
1. Add `NNNN-description.sql` with the SQL
2. Run `go run ./cmd/server -migrate`
3. Include the migration file in your PR

**Never modify an existing migration.** If a schema change is needed, add a new migration.

---

## File Format (.logos)

The `internal/binfile` package handles `.logos` (formerly `.penpot`) v3 ZIP archives.

```
myfile.logos        (ZIP)
├── manifest.json   {"type": "logos/export-files", "version": 1, ...}
├── files/
│   └── <file-id>/
│       ├── attrs.json     file metadata (name, revn, is_shared)
│       ├── pages.json     ordered page ID list
│       ├── changes.json   change history rows (Go extension)
│       └── data.bin       raw file.data blob (when present)
├── media/
│   └── <media-id>.json    FileMediaObject row
└── objects/
    └── <storage-id>.*     raw media bytes
```

**Backward compatibility:** `.penpot` files (manifest type `"penpot/export-files"`)
are accepted on import with a deprecation log message. Both `.logos` and `.penpot`
file extensions are accepted by the HTTP import handler.

---

## OT Rebase Engine

`files_update` is the most performance-critical handler. On every save:

1. Lock the file row (`SELECT ... FOR UPDATE`)
2. Load competing change-sets written since the client's base revision
3. Rebase the incoming changes against each competing set (OT)
4. Insert the rebased change-set
5. Broadcast a `file-change:<file-id>` event via Redis Pub/Sub

The rebase algorithm is in `internal/rebase/rebase.go`. It handles the 5×5 conflict
matrix for change types: `add-obj`, `mod-obj`, `del-obj`, `move-obj`, `mov-objects`.
See `rebase_test.go` for 20 test cases covering every conflict pair.

---

## Testing

```bash
cd backend-go

# All tests (integration tests skip without TEST_DATABASE_URL)
go test ./...

# With integration tests
export TEST_DATABASE_URL="postgres://logos:logos@localhost:5432/logos_test"
go test ./... -count=1

# Specific packages
go test ./internal/rebase/...
go test ./internal/binfile/...
go test ./internal/handler/... -v

# Race detector
go test -race ./...

# Benchmarks
go test -bench=. ./internal/rebase/...
```

---

## Benchmark Fixture Generator

```bash
cd backend-go
go run ./cmd/gen-benchmark \
  --output fixtures/large-canvas.logos \
  --pages 5 \
  --objects 500

# Use in the benchmark workflow:
# .github/workflows/benchmark-memory.yml
```
