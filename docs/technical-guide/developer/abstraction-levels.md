---
title: 3.07. Abstraction Levels
desc: "Logos Technical Guide: how the codebase is organized into well-defined abstraction layers."
---

# Code Organization — Abstraction Levels

Logos is organized into distinct abstraction layers with clear boundaries.
Each layer may only depend on the same layer or layers below it, never on higher layers.

---

## Layer Map

```
┌────────────────────────────────────────────────────────────────┐
│  5. Application / UI Layer                                      │
│     logos-app/src/  (React components, Zustand stores, views)  │
├────────────────────────────────────────────────────────────────┤
│  4. Feature Layer                                               │
│     collab/, ai/, plugins/, workspace/, dashboard/             │
├────────────────────────────────────────────────────────────────┤
│  3. Domain Services Layer                                       │
│     backend-go/internal/handler/  (RPC command implementations)│
│     backend-go/internal/auth/     (authentication logic)       │
│     backend-go/internal/binfile/  (file format)                │
├────────────────────────────────────────────────────────────────┤
│  2. Domain Types Layer (shared between all layers)              │
│     rust/logos-types/             (canonical Rust structs)     │
│     logos-app/src/types/rust-generated/  (generated TS types)  │
├────────────────────────────────────────────────────────────────┤
│  1. Infrastructure Layer                                        │
│     backend-go/internal/db/       (PostgreSQL pool)            │
│     backend-go/internal/redis/    (Redis/Valkey client)        │
│     backend-go/internal/storage/  (FS / S3 backend)            │
│     render-wasm/                  (WebGPU WASM engine)         │
│     rust/logos-layout/            (layout computation)         │
│     rust/logos-vector/            (vector/bezier math)         │
└────────────────────────────────────────────────────────────────┘
```

---

## Layer 1 — Infrastructure

Pure computation and I/O with no domain knowledge.

- **`backend-go/internal/db/`** — pgx connection pool, query helpers, transaction helpers
- **`backend-go/internal/redis/`** — Redis/Valkey client, Pub/Sub helpers
- **`backend-go/internal/storage/`** — storage interface (`Put`/`Get`/`Delete`) with local-FS and S3 backends
- **`render-wasm/`** — GPU render pipeline (Rust → WASM). Knows nothing about Logos shapes; takes draw commands
- **`rust/logos-layout/`** — CSS flex/grid layout computation. Input: node trees. Output: resolved geometry
- **`rust/logos-vector/`** — Bezier math, path operations, hit testing

---

## Layer 2 — Domain Types

Canonical representations of every domain entity. No business logic — just data shapes and serialization.

- **`rust/logos-types/`** — Rust source of truth:
  `Shape`, `Fill`, `Stroke`, `Color`, `Shadow`, `Blur`, `DesignToken`, geometry types, CRDT compat shim
- **`logos-app/src/types/rust-generated/`** — Auto-generated TypeScript (run `make generate-rust-types`)
- **`backend-go/internal/`** — Go structs for DB rows and API request/response shapes (in-package, no shared package)

Types in this layer:
- Use `serde(rename_all = "kebab-case")` in Rust (matches JSON keys expected by TypeScript)
- Are `readonly` in TypeScript (no mutation)
- Derive `Debug + Clone + Serialize + Deserialize` in Rust

---

## Layer 3 — Domain Services

Implementations of specific business operations. May use Layer 1 and Layer 2.

- **`backend-go/internal/handler/`** — Each file is a self-contained RPC namespace:
  `auth.go`, `files_update.go`, `binfile.go`, etc.
- **`backend-go/internal/rebase/`** — OT rebase engine (pure logic, no I/O)
- **`backend-go/internal/perms/`** — Permission checks (pure queries)
- **`backend-go/internal/auth/`** — Token issuance/verification, password hashing

Rule: handlers may call the DB, Redis, and storage layer. They never call other handlers.

---

## Layer 4 — Feature Layer

Composed features that orchestrate multiple domain services.

- **`logos-app/src/collab/`** — WebSocket client + OT apply loop
- **`logos-app/src/ai/`** — AI assistant orchestration (cloud MCP + local LLM)
- **`logos-app/src/plugins/`** — Plugin sandbox lifecycle and bridge
- **`logos-app/src/workspace/`** — Design canvas state management

---

## Layer 5 — Application / UI

React components, views, and routing. Only depends on Layer 4 and below via React hooks and Zustand stores.

- **`logos-app/src/design/`** — Canvas components (calls WebGPU bridge)
- **`logos-app/src/dashboard/`** — File browser, team management views
- **`logos-app/src/auth/`** — Login / register / profile pages

---

## Cross-cutting Concerns

### Error handling

- Go: functions return `(T, error)`. Handlers convert domain errors to JSON error responses via `writeError()`
- TypeScript: `Result<T, E>` types or `try/catch` with typed error classes
- Rust: `Result<T, E>` with `thiserror` for library errors

### Logging

- Go: `log/slog` structured JSON to stdout
- TypeScript: `logger.debug/info/warn/error(namespace, message, context)` → browser console
- Rust: `tracing` crate (when instrumented)

### Configuration

- Go backend: environment variables only, loaded at startup via `internal/config/`
- Frontend: Vite `.env.*` files for build-time flags; `LOGOS_FLAGS` injected at runtime by nginx
