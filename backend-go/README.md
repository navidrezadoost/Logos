# backend-go — Logos Go Backend (Phase G3)

A Go service that runs alongside the existing Clojure backend, serving the same
HTTP API contract against the same PostgreSQL database.

## Purpose

G3 proves the migration path: one endpoint at a time, the Go service takes over
until the Clojure JVM is retired entirely.

## Structure

```
backend-go/
├── cmd/server/main.go          — HTTP server entrypoint
├── internal/
│   ├── config/config.go        — env-var configuration
│   ├── db/postgres.go          — pgx connection pool
│   ├── redis/redis.go          — optional Redis client
│   ├── handler/
│   │   ├── health.go           — GET /api/_health
│   │   └── profile.go          — GET /api/rpc/command/get-profile
│   └── server/server.go        — chi router + middleware
├── migrations -> ../backend/src/app/migrations  (symlink)
├── go.mod
└── go.sum
```

## Endpoints

| Method | Path                               | Description                     |
|--------|------------------------------------|---------------------------------|
| GET    | `/api/_health`                     | Liveness probe → `{"status":"ok"}` |
| GET    | `/api/rpc/command/get-profile`     | Returns profile JSON (pass `x-profile-id` header for a real user; omit for anonymous) |

## Quick Start

```bash
# Requires Go ≥ 1.22 and a running PostgreSQL instance.

export DATABASE_URL=postgres://logos:logos@localhost:5432/logos
make run-go-backend

# In another terminal:
curl http://localhost:8080/api/_health
# → {"status":"ok"}

curl -H "x-profile-id: <uuid>" http://localhost:8080/api/rpc/command/get-profile
# → {"id":"...","fullname":"...","email":"...",...}
```

## Environment Variables

| Variable            | Default                                          | Description                  |
|---------------------|--------------------------------------------------|------------------------------|
| `BACKEND_GO_ADDR`   | `:8080`                                          | Listen address               |
| `DATABASE_URL`      | `postgres://logos:logos@localhost:5432/logos`    | PostgreSQL DSN               |
| `REDIS_URL`         | *(empty — caching disabled)*                     | Redis DSN (optional)         |
| `CACHE_ENABLED`     | `false`                                          | Enable read-through cache     |
| `CACHE_TTL_SECONDS` | `300`                                            | Profile cache TTL in seconds |

## Development

```bash
# Download / tidy dependencies
make go-mod-tidy

# Build binary to backend-go/bin/server
make build-go-backend

# Run tests (once tests are added)
cd backend-go && go test ./...
```

## G3 Completion Criteria

- [x] `GET /api/_health` returns `{"status":"ok"}`
- [x] `GET /api/rpc/command/get-profile` reads from the `profile` table and
      returns the same JSON shape as the Clojure handler
- [x] Service starts alongside `backend/` on a different port without conflict
- [ ] Session middleware (cookie → profile-id resolution) — G4
- [ ] CRDT change endpoint — G4
