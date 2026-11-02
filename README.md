# Logos — Community Edition

<p align="center">
  <img src="https://img.shields.io/badge/Edition-Community-brightgreen.svg" alt="Community Edition">
  <img src="https://img.shields.io/badge/Status-Stable-blue.svg" alt="Stable">
  <img src="https://img.shields.io/badge/License-MPL--2.0-lightgrey.svg" alt="License">
  <img src="https://img.shields.io/badge/TypeScript-50%25-3178C6.svg" alt="TypeScript">
  <img src="https://img.shields.io/badge/Go-30%25-00ADD8.svg" alt="Go">
  <img src="https://img.shields.io/badge/Rust-17%25-orange.svg" alt="Rust">
  <img src="https://img.shields.io/badge/Renderer-WebGPU-blueviolet.svg" alt="WebGPU">
</p>

**Logos** is a GPU-accelerated, AI-powered, privacy-first open-source design platform.

The Community Edition is the complete, unrestricted product — every feature built across
Phases 0–5 is available at no cost, forever, under the Mozilla Public License 2.0.

---

## What You Get in the Community Edition

| Capability | Details |
|---|---|
| **Design canvas** | Infinite canvas, 60fps with WebGPU compute shaders, sub-pixel AA |
| **Collaboration** | Real-time multi-user editing via OT rebase; attribute-level conflict resolution |
| **AI assistant** | Dual-path: cloud MCP tools + local LLM inference via WebGPU (privacy-first) |
| **Design tokens** | DTCG-compliant token runtime with aliasing, modes, and live theme switching |
| **Dev Mode** | CSS / variables export, redlines, annotation links, REST integration API |
| **Plugin system** | TypeScript sandbox, `@logos/plugin-types`, hot-reload developer mode |
| **File import** | Zero-loss migration from external design tool, Sketch, Adobe XD, SVG |
| **File format** | Open ``.logos` v3 ZIP archive (SVG/CSS-based) |
| **Self-hosted** | Single Go binary (~20 MB) + static React SPA — no JVM, no Node.js at runtime |

Nothing is paywalled in this edition. There is no feature flag that unlocks paid content.
SSO/SAML, audit-log retention guarantees, and managed cloud hosting are planned for a future
Enterprise Edition and will be additive — the Community Edition will not lose features.

---

## Architecture

```
logos/
├── logos-app/           TypeScript · React 19 · Zustand · Vite
│   ├── src/             Application source
│   │   ├── design/      Design canvas (WebGPU renderer bridge)
│   │   ├── workspace/   Layer panel, properties, design tokens
│   │   ├── collab/      Real-time collaboration (WebSocket + OT)
│   │   ├── ai/          AI assistant (cloud MCP + local LLM)
│   │   └── plugins/     Plugin runtime sandbox
│   └── workers/         Offscreen GPU, file-processing web workers
│
├── backend-go/          Go · chi router · pgx · go-jose
│   ├── cmd/server/      HTTP/WebSocket server entry point (port 6060)
│   └── internal/
│       ├── handler/     25 RPC namespaces (auth, files, teams, media …)
│       ├── binfile/     .logos v3 ZIP export/import
│       ├── rebase/      Pure-Go OT rebase engine
│       └── auth/        Argon2id · JWE sessions · API tokens
│
├── rust/                Rust · WASM · WGSL
│   ├── logos-types/     Canonical domain types → generates TS .d.ts
│   ├── logos-layout/    Rust-native flex/grid layout engine
│   ├── logos-rebase/    CRDT operational-transform rebase (rlib)
│   ├── logos-vector/    Bezier / path computation
│   └── render-wasm/     WebGPU render pipeline compiled to WASM
│
├── mcp/                 TypeScript · Model Context Protocol server
│   ├── packages/server/ MCP tools (layout, palette, code execution …)
│   └── packages/plugin/ Logos plugin that bridges the MCP ↔ canvas
│
└── plugins/             TypeScript · Plugin SDK + example apps
    ├── libs/
    │   ├── plugin-types/      @logos/plugin-types — public API types
    │   └── plugins-runtime/   Sandboxed plugin execution runtime
    └── apps/                  Reference plugin implementations
```

### Language Breakdown

| Language | Role | Share |
|---|---|---|
| **TypeScript** | Frontend SPA, Plugin SDK, MCP server, workers | ~50% |
| **Go** | Backend HTTP/WS, DB, auth, background tasks, file export | ~30% |
| **Rust** | Layout, vector, CRDT rebase, type codegen, WebGPU WASM | ~17% |
| WGSL | GPU compute & render shaders | ~2% |
| SQL | PostgreSQL migrations | ~1% |

> **Type source of truth:** Canonical domain types are Rust structs in `rust/logos-types/`.
> Run `make generate-rust-types` to regenerate `logos-app/src/types/rust-generated/`.

---

## Quick Start — Build from Source

### Prerequisites

| Tool | Version | Purpose |
|---|---|---|
| Go | 1.22+ | Backend |
| Node.js | 20 LTS | Frontend |
| Rust + `wasm-pack` | stable | Core engine + WASM |
| PostgreSQL | 14+ | Database |
| Redis / Valkey | 7+ | Pub/Sub + cache |

### 1 — Clone

```bash
git clone https://github.com/navidrezadoost/Logos.git
cd Logos
```

### 2 — Backend

```bash
cd backend-go

# Install dependencies
go mod download

# Set required environment variables
export DATABASE_URL="postgres://logos:logos@localhost:5432/logos"
export REDIS_URL="redis://localhost:6379"
export LOGOS_SECRET_KEY="change-me-32-bytes-or-longer"

# Run database migrations
go run ./cmd/server -migrate

# Start the server (port 6060)
go run ./cmd/server

# Verify
curl http://localhost:6060/api/_health
# → {"status":"ok"}
```

### 3 — Frontend

```bash
cd logos-app

# Install dependencies
npm ci

# Development server (hot-reload, proxies API to port 6060)
npm run dev
# → http://localhost:3449

# Type-check
npx tsc --noEmit

# Unit tests
npx vitest run
```

### 4 — Rust / WASM (optional — pre-built WASM is included)

```bash
# Install Rust target
rustup target add wasm32-unknown-unknown
cargo install wasm-pack

# Build the render engine WASM
cd render-wasm
wasm-pack build --target web --release
# Output: pkg/ — automatically picked up by the Vite build
```

### 5 — MCP Server (optional)

```bash
cd mcp/packages/server
npm ci
npm run dev
# → http://localhost:7998
```

---

## Self-Hosting

> **Docker images are being prepared** for the community edition release and will be
> published under the `logos/` Docker Hub namespace. A single `docker compose up` will
> start the full stack.
>
> In the meantime, use the development containers in `docker/devenv/` for a
> fully-configured local environment, or build directly from source as shown above.

For full self-hosting documentation see [docs/technical-guide/getting-started/](./docs/technical-guide/getting-started/).

---

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `DATABASE_URL` | — | PostgreSQL connection string (required) |
| `REDIS_URL` | — | Redis/Valkey connection string (required) |
| `LOGOS_SECRET_KEY` | — | Master key for HKDF token derivation (required) |
| `BACKEND_GO_ADDR` | `:6060` | HTTP listen address |
| `LOGOS_FLAGS` | — | Feature flags string |
| `LOGOS_ENABLE_AUDIT_LOG` | `false` | Enable audit event logging |
| `LOGOS_ENABLE_DEMO_USERS` | `false` | Enable demo account provisioning |
| `LOGOS_ENABLE_USER_FEEDBACK` | `false` | Enable feedback submission |
| `LOGOS_SMTP_HOST` | — | SMTP server for transactional email |
| `STORAGE_BACKEND` | `local` | `local` or `s3` |
| `STORAGE_LOCAL_DIR` | `./data` | Local storage path |

---

## Development

```bash
# Go backend — build, vet, test
cd backend-go && go build ./... && go vet ./... && go test ./...

# TypeScript frontend — type-check and test
cd logos-app && npx tsc --noEmit && npx vitest run

# Rust crates — format, lint, test
cd rust && cargo fmt --check && cargo clippy && cargo test --workspace

# All together (CI)
./run-ci.sh
```

---

## Contributing

Read [CONTRIBUTING.md](./CONTRIBUTING.md) before opening a pull request.

The short version:
- Fork → branch → PR against `main`
- One PR per logical change; reference the issue
- `go vet`, `npx tsc --noEmit`, `cargo clippy` must all pass
- New code needs tests; public APIs need doc comments
- Commit format: `feat(scope):`, `fix(scope):`, `docs:`, `test:`, `chore:`

See [docs/technical-guide/developer/](./docs/technical-guide/developer/) for architecture
deep-dives, data model, devenv setup, and subsystem guides.

---

## Roadmap — What's Coming

The Community Edition is feature-complete for the design workflow. The open roadmap items are:

| Area | Status |
|---|---|
| Official Docker images (`logos/frontend`, `logos/backend`) | In progress |
| Helm chart for Kubernetes | Planned |
| Offline PWA mode (full service-worker) | Planned |
| Plugin marketplace | Planned |
| Enterprise Edition (SSO, audit-log retention, managed cloud) | Planned |

---

## License

Logos is licensed under the **Mozilla Public License 2.0 (MPL-2.0)**.
See [LICENSE](./LICENSE) for the full text.

MPL-2.0 means: you can use Logos freely, modify it, and redistribute it. If you
modify an MPL-licensed file you must publish those modifications under MPL-2.0.
You can combine Logos with proprietary software in the same product as long as
the MPL-licensed files remain open.

---

## Acknowledgements

Built on the shoulders of:

- [Logos](https://logos.app/) — the open-source design tool this project started from
- [wgpu](https://github.com/gfx-rs/wgpu) — cross-platform GPU abstraction
- [pgx](https://github.com/jackc/pgx) — PostgreSQL driver for Go
- [go-jose](https://github.com/go-jose/go-jose) — JWE/JWS for Go
- [chi](https://github.com/go-chi/chi) — lightweight Go router
- [React](https://react.dev/) + [Zustand](https://github.com/pmndrs/zustand) — frontend framework
- [Vite](https://vitejs.dev/) — frontend build tooling
