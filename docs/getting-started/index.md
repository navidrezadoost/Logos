---
title: Getting Started
desc: Build Logos Community Edition from source — backend, frontend, and Rust engine.
eleventyNavigation:
  key: Getting Started
  order: 2
---

# Getting Started with Logos Community Edition

Logos Community Edition is a multi-language monorepo. You only need to set up
the layers you are actively developing. This guide covers all three layers and
a complete end-to-end local setup.

## Prerequisites

| Tool | Version | Layer |
|---|---|---|
| Go | 1.22+ | Backend |
| Node.js | 20 LTS+ | Frontend, MCP, Plugins |
| Rust (stable) | 1.75+ | Core engine, WASM renderer |
| PostgreSQL | 14+ | Backend (integration tests + runtime) |
| Redis / Valkey | 7+ | Backend (pub/sub + cache) |

Install all of them or just the ones you need.

**Go:**
```bash
# From https://go.dev/dl/ — or your package manager
go version   # 1.22+
```

**Node.js (via nvm — recommended):**
```bash
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.7/install.sh | bash
nvm install 20
nvm use 20
```

**Rust:**
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
rustup update stable
rustup target add wasm32-unknown-unknown   # required for render-wasm
```

---

## Clone

```bash
git clone https://github.com/navidrezadoost/Logos.git
cd Logos
```

---

## Backend

```bash
cd backend-go

# Download dependencies
go mod download

# Export required environment variables
export DATABASE_URL="postgres://logos:logos@localhost:5432/logos"
export REDIS_URL="redis://localhost:6379"
export LOGOS_SECRET_KEY="dev-secret-at-least-32-characters"

# Apply database migrations
go run ./cmd/server -migrate

# Start the server
go run ./cmd/server
# Listening on :6060

# Verify
curl http://localhost:6060/api/_health
# → {"status":"ok","version":"dev"}
```

For a complete list of environment variables see [backend-go/README.md](../../backend-go/README.md).

---

## Frontend

The frontend dev server proxies API requests to the running backend at port 6060.

```bash
cd logos-app

# Install dependencies (uses npm lockfile)
npm ci

# Start hot-reload dev server
npm run dev
# → http://localhost:3449

# Type-check (no emitting, zero errors required)
npx tsc --noEmit

# Unit tests
npx vitest run

# Production build
npm run build
# Output: logos-app/dist/
```

---

## Rust / WASM Renderer (optional)

The pre-built WASM binary is committed to the repository, so you only need
to rebuild it when modifying the render engine.

```bash
cd render-wasm

# Build optimized WASM + JS bindings
wasm-pack build --target web --release
# Output written to render-wasm/pkg/
# The Vite build in logos-app/ picks this up automatically.

# Run Rust tests (non-WASM targets only)
cargo test
```

---

## MCP Server (optional)

The MCP server exposes Logos canvas operations as Model Context Protocol tools,
enabling AI assistants to design directly in Logos.

```bash
cd mcp/packages/server
npm ci
npm run dev
# → http://localhost:7998
```

---

## All at once (CI reference)

```bash
./run-ci.sh
```

This runs:
- `go build ./...`, `go vet ./...`, `go test ./...` (backend)
- `npx tsc --noEmit`, `npx vitest run` (frontend)
- `cargo fmt --check`, `cargo clippy`, `cargo test --workspace` (Rust)

---

## Project Layout

```
Logos/
├── logos-app/              TypeScript React SPA
├── backend-go/             Go API server (port 6060)
│   ├── cmd/server/         Entry point
│   ├── internal/handler/   25 RPC command namespaces
│   ├── internal/rebase/    OT rebase engine
│   ├── internal/binfile/   .logos file format
│   └── migrations/         PostgreSQL migration files
├── rust/                   Rust workspace
│   ├── logos-types/        Canonical domain types
│   ├── logos-layout/       Layout engine
│   ├── logos-rebase/       CRDT rebase (rlib)
│   └── logos-vector/       Vector / bezier math
├── render-wasm/            WebGPU renderer → WASM
├── mcp/                    Model Context Protocol server
├── plugins/                Plugin SDK + runtime + examples
├── docker/devenv/          Development environment containers
├── docs/                   Documentation site (Eleventy)
├── ARCHITECTURE.md         Full architecture reference
└── CHANGELOG.md            Release history
```

---

## What's Next

| Resource | Where |
|---|---|
| Full architecture | [`ARCHITECTURE.md`](../../ARCHITECTURE.md) |
| Backend RPC reference | [`backend-go/README.md`](../../backend-go/README.md) |
| Developer environment | [`docs/technical-guide/developer/devenv.md`](../technical-guide/developer/devenv.md) |
| Self-hosting guide | [`docs/technical-guide/getting-started/`](../technical-guide/getting-started/) |
| Plugin development | [`docs/plugin-guide/`](../plugin-guide/) |
| API reference | [`docs/api-reference/`](../api-reference/) |
| Contributing guide | [`CONTRIBUTING.md`](../../CONTRIBUTING.md) |
