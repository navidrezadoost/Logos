---
title: 3.03. Dev environment
desc: Set up the Logos Community Edition development environment with Docker or natively.
---

# Development Environment

Logos offers two paths for local development: a **Docker devenv** (everything in one container —
recommended for first-time setup or full-stack work) and a **native setup** (recommended for
fast iteration on a single layer).

---

## System Requirements

| Tool | Minimum | Purpose |
|---|---|---|
| Docker + Compose V2 | Latest stable | devenv containers |
| Go | 1.22 | Backend development |
| Node.js | 20 LTS | Frontend / MCP / Plugins |
| Rust (stable) | 1.75 | Core engine, WASM renderer |
| PostgreSQL | 14+ | Database (devenv includes one) |
| Redis / Valkey | 7+ | Pub/Sub + cache (devenv includes one) |

---

## Option A — Docker devenv (recommended)

The devenv container bundles PostgreSQL, Valkey, MailCatcher, Go, Node.js, and Rust.
It uses [tmux](https://github.com/tmux/tmux/wiki) to run all services in one terminal.

```bash
# From the repository root:
./manage.sh run-devenv
```

This will:
1. Build the devenv image if not already built
2. Start PostgreSQL, Valkey, and MailCatcher in the background
3. Attach your terminal to a tmux session inside the devenv container

Once the session starts, browse to **http://localhost:3449** for the application.

### manage.sh subcommands

```bash
./manage.sh build-devenv-local   # build the devenv image locally
./manage.sh start-devenv         # start background containers
./manage.sh run-devenv           # attach to tmux inside devenv
./manage.sh stop-devenv          # stop all containers
./manage.sh drop-devenv          # remove containers, volumes, and networks
```

### tmux shortcuts

| Shortcut | Action |
|---|---|
| `Ctrl+b c` | New window |
| `Ctrl+b w` | Window list |
| `Ctrl+b &` | Kill current window |
| `Ctrl+b "` | Split pane horizontally |
| `Ctrl+b %` | Split pane vertically |
| `Ctrl+b d` | Detach (containers keep running) |

For a full reference: https://tmuxcheatsheet.com/

---

## Option B — Native setup

### 1. Backend (Go)

```bash
# Start PostgreSQL and Valkey (or use Docker):
docker run -d -p 5432:5432 -e POSTGRES_USER=logos -e POSTGRES_PASSWORD=logos \
  -e POSTGRES_DB=logos postgres:16-alpine
docker run -d -p 6379:6379 valkey/valkey:7-alpine

# Configure and start the backend
export DATABASE_URL="postgres://logos:logos@localhost:5432/logos"
export REDIS_URL="redis://localhost:6379"
export LOGOS_SECRET_KEY="dev-secret-at-least-32-characters"

cd backend-go
go run ./cmd/server -migrate   # apply migrations
go run ./cmd/server             # start server on :6060
```

### 2. Frontend (TypeScript)

```bash
cd logos-app
npm ci
npm run dev   # http://localhost:3449 (proxies /api to :6060)
```

The dev server configuration in `vite.config.ts` proxies all `/api` requests to
`http://localhost:6060`, so the Go backend must be running.

### 3. Rust WASM renderer (when modifying render-wasm)

```bash
cd render-wasm
wasm-pack build --target web --release
# The output (render-wasm/pkg/) is automatically picked up by Vite.
```

---

## Email Testing

The devenv includes **MailCatcher** — a local SMTP server that captures all outgoing
email without delivering it. Open the web interface at:

```
http://localhost:1080
```

This covers all transactional emails: registration, password recovery, magic links,
and team invitations.

When running natively, set `LOGOS_SMTP_HOST=localhost` and start MailCatcher separately:

```bash
gem install mailcatcher
mailcatcher   # SMTP on :1025, web UI on :1080
export LOGOS_SMTP_HOST=localhost
export LOGOS_SMTP_PORT=1025
```

---

## Creating a Test User

```bash
# From the backend-go directory with DATABASE_URL set:
curl -X POST http://localhost:6060/api/rpc/command/register-profile \
  -H "Content-Type: application/json" \
  -d '{"email":"dev@example.com","fullname":"Dev User","password":"devpassword"}'
```

Or register through the UI at http://localhost:3449.

---

## Feature Flags

Feature flags are injected at runtime via the `LOGOS_FLAGS` environment variable and
can be toggled per-team through the debug page at **http://localhost:3449/dbg**.

Common flags:

| Flag | Effect |
|---|---|
| `enable-plugins` | Enable the plugin panel and sandbox |
| `enable-storybook` | Show Storybook link in the dashboard |
| `enable-ai` | Enable AI assistant panel |
| `enable-dev-mode` | Enable developer mode panel (CSS export, redlines) |

---

## Running the Test Suite

```bash
# Backend
cd backend-go
go test ./...                          # unit tests
TEST_DATABASE_URL=... go test ./...    # + integration tests

# Frontend
cd logos-app
npx vitest run

# Rust
cd rust
cargo test --workspace
```

---

## Troubleshooting

**Port 3449 / 6060 already in use:**
```bash
lsof -i :3449    # find the process
kill -9 <pid>
```

**Database connection refused:**
- Check `DATABASE_URL` is set correctly
- Verify PostgreSQL is running: `pg_isready -h localhost -p 5432`
- In devenv: `./manage.sh start-devenv` to restart containers

**WASM not found / stale:**
```bash
cd render-wasm && wasm-pack build --target web --release
```

**Go module cache issues:**
```bash
cd backend-go && go clean -modcache && go mod download
```
