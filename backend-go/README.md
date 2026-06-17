# backend-go — Logos Go Backend

The Go backend is the production API server for Logos Community Edition.
It replaced the original Clojure backend in Phase G5 and is the only backend
implementation in the repository.

## Overview

| Attribute | Value |
|---|---|
| Language | Go 1.22+ |
| Binary size | ~20 MB (static, no JVM) |
| Cold start | <1 s |
| Database | PostgreSQL 14+ via `pgx/v5` |
| Cache / PubSub | Redis / Valkey 7+ |
| Auth | Argon2id passwords · JWE sessions (A256KW/A256GCM) · API tokens |
| File format | `.logos` v3 ZIP (`binfile` package) |
| Router | `go-chi/chi` |
| Listen address | `:6060` (configurable via `BACKEND_GO_ADDR`) |

## Structure

```
backend-go/
├── cmd/
│   ├── server/          — main entry point (HTTP server)
│   └── gen-benchmark/   — CLI tool: generate .logos fixture files for CI
├── internal/
│   ├── auth/            — Argon2id hashing, JWE token issuance/verification
│   ├── binfile/         — .logos v3 ZIP export/import (backward-compat with .logos)
│   ├── config/          — env-var configuration
│   ├── db/              — pgx connection pool wrapper
│   ├── email/           — transactional email (SMTP)
│   ├── handler/         — 25 RPC command handlers (one file per namespace)
│   ├── perms/           — permission helpers (team/project/file access checks)
│   ├── rebase/          — pure-Go OT rebase engine (20 tests, 5×5 conflict matrix)
│   ├── redis/           — Redis/Valkey client wrapper
│   ├── server/          — chi router, middleware, route wiring
│   └── storage/         — storage backend (local FS or S3-compatible)
└── migrations/          — PostgreSQL migration SQL files
```

## RPC Command Namespaces

All 25 Clojure namespaces are ported. Every endpoint is a `POST /api/rpc/command/<name>`:

| Namespace | Handlers | File |
|---|---|---|
| `profile` | get/update profile, photo, props, delete | `handler/profile.go` |
| `teams` | CRUD teams, invitations, members, roles | `handler/teams.go` + `handler/teams_invitations.go` |
| `projects` | CRUD projects, pin, duplicate | `handler/projects.go` |
| `files` | CRUD files, share, viewers | `handler/files.go` |
| `files_update` | OT rebase, Redis broadcast, row-level lock | `handler/files_update.go` |
| `files_thumbnails` | create/list/delete thumbnails | `handler/files_thumbnails.go` |
| `files_snapshot` | labeled version snapshots | `handler/files_snapshot.go` |
| `comments` | CRUD comments, threads | `handler/comments.go` |
| `media` | upload/list/delete media objects | `handler/media.go` |
| `fonts` | upload/list/delete custom fonts | `handler/fonts.go` |
| `binfile` | export `.logos` ZIP, import `.logos` / `.logos` | `handler/binfile.go` |
| `auth` | login, register, logout, recovery, magic link | `handler/auth.go` |
| `ldap` | LDAP authentication | `handler/auth.go` |
| `access_token` | create/list/delete API tokens | `handler/access_token.go` |
| `verify_token` | verify email / magic-link tokens | `handler/verify_token.go` |
| `search` | full-text file search | `handler/search.go` |
| `audit` | push audit events | `handler/audit.go` |
| `demo` | create demo profiles | `handler/demo.go` |
| `feedback` | user feedback submission | `handler/feedback.go` |
| `management` | duplicate project, move files/projects | `handler/management.go` |
| `webhooks` | CRUD webhooks, async delivery | `handler/webhooks.go` |

## Quick Start

```bash
# Prerequisites: Go 1.22+, running PostgreSQL, running Redis

export DATABASE_URL="postgres://logos:logos@localhost:5432/logos"
export REDIS_URL="redis://localhost:6379"
export LOGOS_SECRET_KEY="change-me-at-least-32-characters"

cd backend-go

# Download dependencies
go mod download

# Build and start
go run ./cmd/server

# Verify
curl http://localhost:6060/api/_health
# → {"status":"ok","version":"dev"}
```

## Environment Variables

| Variable | Default | Required | Description |
|---|---|---|---|
| `DATABASE_URL` | — | ✓ | PostgreSQL DSN |
| `REDIS_URL` | — | ✓ | Redis/Valkey DSN |
| `LOGOS_SECRET_KEY` | — | ✓ | Master key (32+ bytes) for HKDF token derivation |
| `BACKEND_GO_ADDR` | `:6060` | | HTTP listen address |
| `STORAGE_BACKEND` | `local` | | `local` or `s3` |
| `STORAGE_LOCAL_DIR` | `./data` | | Local storage root directory |
| `S3_BUCKET` | — | | S3 bucket name (when `STORAGE_BACKEND=s3`) |
| `S3_REGION` | — | | S3 region |
| `S3_ENDPOINT` | — | | S3-compatible endpoint URL (MinIO, etc.) |
| `COOKIE_NAME` | `logos-auth` | | Auth cookie name |
| `LOGOS_SMTP_HOST` | — | | SMTP host for transactional email |
| `LOGOS_SMTP_PORT` | `587` | | SMTP port |
| `LOGOS_SMTP_USER` | — | | SMTP username |
| `LOGOS_SMTP_PASS` | — | | SMTP password |
| `LOGOS_SMTP_FROM` | — | | Sender address |
| `LOGOS_ENABLE_AUDIT_LOG` | `false` | | Enable `push-audit-events` handler |
| `LOGOS_ENABLE_DEMO_USERS` | `false` | | Enable `create-demo-profile` handler |
| `LOGOS_ENABLE_USER_FEEDBACK` | `false` | | Enable `send-user-feedback` handler |

## Building

```bash
# Development binary
go build -o bin/logos-backend ./cmd/server

# Optimized production binary
CGO_ENABLED=0 GOOS=linux go build \
  -trimpath \
  -ldflags="-s -w" \
  -o bin/logos-backend \
  ./cmd/server

# Run all tests
go test ./...

# Run with race detector
go test -race ./...

# Specific packages
go test ./internal/rebase/...    # OT engine — 20 tests
go test ./internal/binfile/...   # file format
go test ./internal/handler/...   # RPC handlers

# Benchmark fixture generator
go run ./cmd/gen-benchmark --output fixtures/large-canvas.logos --pages 5 --objects 500
```

## Key Design Decisions

### OT Rebase

The rebase engine (`internal/rebase/`) implements a pure-Go Operational Transform
algorithm covering the 5×5 conflict matrix across `add-obj`, `mod-obj`, `del-obj`,
`move-obj`, and `mov-objects` change types. This replaced the `logos-rebase` Rust crate
(which is an `rlib` and not FFI-ready). See `rebase_test.go` for 20 coverage cases.

### Mixed Change History

Files updated by the Go backend store change-sets as JSON in `file_change.changes`.
The rebase engine skips rows it cannot parse (legacy Clojure Transit+zstd blobs),
providing a safe migration window for databases that still contain Clojure-era data.

### File Format Backward Compatibility

The `binfile` package writes `"logos/export-files"` as the manifest type in new `.logos`
archives. It accepts `"logos/export-files"` on import and logs a deprecation message.
Both `.logos` and `.logos` file extensions are accepted by the import handler.

### Token Compatibility

JWE session tokens issued by the Go backend use `Aud: "logos"`. Legacy tokens issued
by the Clojure backend with `Aud: "logos"` continue to decrypt correctly — the Go
verifier does not assert the audience value, only decrypts and reads it.
