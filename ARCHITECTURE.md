# Logos Architecture

> Logos is a fork of [Logos](https://logos.app) — an open-source design and prototyping platform. This document describes the full technical architecture: algorithms, subsystems, data flows, and implementation patterns.

---

## Table of Contents

1. [High-Level Overview](#1-high-level-overview)
2. [Frontend — React/TypeScript SPA](#2-frontend--reacttypescript-spa)
3. [Backend — Go API Server](#3-backend--go-api-server)
4. [WebAssembly Renderer](#4-webassembly-renderer)
5. [Real-Time Collaboration](#5-real-time-collaboration)
6. [Plugin System](#6-plugin-system)
7. [MCP Tool Server](#7-mcp-tool-server)
8. [Authentication & Session Flow](#8-authentication--session-flow)
9. [State Management — Zustand](#9-state-management--zustand)
10. [File Format & CRDT](#10-file-format--crdt)
11. [Duotone Icon System](#11-duotone-icon-system)
12. [Infrastructure & Deployment](#12-infrastructure--deployment)

---

## 1. High-Level Overview

```
Browser (React + TypeScript SPA — logos-app)
       │  HTTP RPC + WebSocket
       ▼
Go Backend (HTTP API + WebSocket notifications)
       │
       ├── PostgreSQL  (persistent data)
       ├── Redis/Valkey (pub/sub, sessions, rate-limiting)
       └── S3 / object store (media assets)
```

### Runtime Languages

| Layer | Language | Build Tool |
|---|---|---|
| **Frontend SPA** | **TypeScript 5 + React 18** | **Vite 6** |
| **Backend** | **Go 1.23** | **go build** |
| Renderer | Rust → WebAssembly | cargo / wasm-pack |
| Shaders | WGSL | naga / wgpu pipeline |
| Plugins | TypeScript (sandboxed iframe) | esbuild / vite |
| MCP server | TypeScript | tsup / Node.js |
| Migrations | PostgreSQL / SQL | Flyway |

### Language Breakdown

| Language | Role | Approx. % |
|---|---|---|
| **TypeScript** | Frontend SPA, workers, MCP server, Plugin SDK | **~50%** |
| **Go** | Backend HTTP/WS, DB, auth, background tasks, file export | **~30%** |
| **Rust** | Layout engine, renderer, rebase, vector graphics, WASM | **~17%** |
| WGSL | WebGPU compute & render shaders | ~2% |
| SQL | PostgreSQL migrations | ~1% |

### Development Ports

| Service | Port | Command |
|---|---|---|
| **logos-app dev server** | **5173** | **`logos dev`** |
| Go backend HTTP | 8080 | `make run-go-backend` |

---

## 2. Frontend — React/TypeScript SPA

> **Migration complete (CS1–CS2)**: The ClojureScript frontend (`frontend/`) has been
> superseded by `logos-app/`, a Vite + React 18 + TypeScript 5 application backed by the
> same Rust/WASM rendering core.

### 2.1 Technology Stack

- **Vite 6** — dev server with HMR, production bundler
- **React 18** — functional components, concurrent rendering
- **TypeScript 5** — strict mode, project-wide type safety
- **Zustand** — lightweight flux-like state (documentStore, selectionStore, uiStore)
- **@tanstack/react-virtual** — virtualised layers panel (handles 10 000+ shapes)
- **Workers** — layout, snap, and serialization in dedicated Web Workers

### 2.2 Directory Layout

```
logos-app/src/
  components/
    canvas/     Canvas.tsx — WASM bridge + Canvas 2D fallback, mouse-drag shape creation
    toolbar/    Toolbar.tsx — tool selection (V/R/O/T/P/H)
    layers/     LayersPanel.tsx — virtualised shape tree
    inspector/  Inspector.tsx — x/y/w/h + fill picker
  stores/
    documentStore.ts  — pages + shapes (Zustand)
    selectionStore.ts — selected IDs
    uiStore.ts        — active tool, panel visibility
  render-wasm/
    module.ts   — Emscripten module typings
    scene.ts    — uploadShapeBatched() 104-byte WASM ABI
  worker/
    layout.worker.ts    — flex/grid layout
    snap.worker.ts      — snap guides (range-tree)
    serialize.worker.ts — shape → 104-byte binary
    index.ts            — WorkerPool singleton
  plugins/
    bridge.ts   — sandboxed iframe + postMessage protocol
    api.ts      — host-side plugin API dispatch
    types.ts    — stable TypeScript types for plugin authors
    sample/     — hello-world sample plugin
  types/
    shapes.ts   — Shape, Fill, Rect, Transform
```

### 2.3 Canvas Rendering Pipeline

**Canvas 2D fallback (default — no Emscripten required):**
1. Zustand shapes → `syncScene2D()` in `scene.ts`
2. `requestAnimationFrame` loop paints each shape as a filled rect/ellipse
3. Selection handles drawn as dashed overlays

**WebAssembly mode (when Emscripten/Skia is available):**
1. `uploadShapeBatched()` writes 104-byte records into WASM heap
2. Calls `_set_shape_base_props()`, `_set_shape_fills()` → Rust scene graph
3. `_render_frame()` rasterises via Skia onto the canvas element

### 2.4 Worker Orchestration

| Worker | Protocol | Purpose |
|---|---|---|
| `layout.worker` | `COMPUTE_LAYOUT → LAYOUT_RESULT` | Flex/grid auto-layout |
| `snap.worker` | `SNAP → SNAP_RESULT` | Snap guides (binary range tree) |
| `serialize.worker` | `SERIALIZE → SERIALIZED` | Shape → 104-byte binary (zero-copy transfer) |

### 2.5 Plugin System

See `logos-app/src/plugins/README.md` for the full specification.

- Each plugin runs in `<iframe sandbox="allow-scripts">` (null origin — no DOM access)
- `logos.call(method, params)` → host dispatches to `api.ts` with permission check
- Permissions granted at `connectPlugin()` time; cannot be escalated at runtime
- Push events: `selectionChange`, `pageChange`, `documentChange`

### 2.6 Thumbnail Rasterizer

A separate `rasterizer.js` chunk runs in a `<iframe>` sandboxed context:
1. Dashboard requests a thumbnail via `postMessage`
2. Rasterizer renders the page to an offscreen `<canvas>` using SVG rendering
3. Returns a PNG data-URL back to the parent
4. Backend stores thumbnails in object storage; freshness tracked per file revision

---

## 3. Backend — Go API Server

The backend is a single statically-linked Go binary (~20 MB). There is no JVM, no Clojure, and no runtime dependency beyond the OS libc.

### 3.1 Technology Stack

| Component | Library |
|---|---|
| HTTP router | [chi v5](https://github.com/go-chi/chi) |
| PostgreSQL | [pgx v5](https://github.com/jackc/pgx) |
| Redis/Valkey | [rueidis](https://github.com/redis/rueidis) |
| Object storage | local FS or S3-compatible (MinIO) |
| Password hashing | Argon2id (golang.org/x/crypto) — compatible with former Clojure backend |
| Session tokens | JWE (JOSE) + Transit JSON — interoperable with the former Clojure backend |
| Auth | Cookie-based sessions + API access tokens |

### 3.2 RPC Command System

All API calls are **HTTP RPC commands** dispatched by URL path.

```
POST/GET /api/rpc/command/<cmd-name>
         → JSON body parsed by handler
         → handler executes (ctx, pool, params)
         → returns JSON response
```

The 25 ported RPC namespaces:

| Session | Namespaces |
|---|---|
| G1 | `profile`, `teams`, `teams_invitations`, `projects` |
| G2 | `files`, `files_create`, `files_share`, `viewer` |
| G3 | `auth`, `ldap`, `access_token`, `verify_token` |
| G4 | `files_update`, `comments`, `media`, `fonts` |
| G5 | `binfile`, `files_thumbnails`, `files_snapshot`, `webhooks` |
| G6 | `search`, `audit`, `demo`, `feedback`, `management` |

### 3.3 Middleware Stack

```
Reverse proxy (TLS termination)
  → Content-Security-Policy headers
  → Rate limiter (per-IP, per-user via Redis)
  → Session cookie / Authorization header validation
  → JSON body parser
  → Chi router dispatch
  → Handler
  → JSON response encoder
```

### 3.4 Database Schema

Key tables (PostgreSQL):

| Table | Purpose |
|---|---|
| `profile` | User accounts (Argon2id hashed passwords, `is_demo` flag) |
| `team` | Workspaces / organisations |
| `team_profile_rel` | Team membership + role |
| `project` | Grouping of files |
| `file` | Design file metadata (`revn` sequence, `vern` conflict sentinel) |
| `file_data` | Raw CRDT state blob (Transit+zstd from Clojure; Go stores JSON) |
| `file_change` | Append-only log of CRDT operations |
| `file_tagged_object_thumbnail` | Per-frame/object thumbnail metadata |
| `file_thumbnail` | File-level thumbnail metadata |
| `share_link` | Shareable view links |
| `file_media_object` | Uploaded images/videos linked to a file |
| `team_font_variant` | Custom font files uploaded per team |
| `comment_thread` | Collaboration comment threads on a canvas |
| `comment` | Individual comment messages |
| `webhook` | Outbound webhook configurations per team |
| `webhook_delivery` | Webhook dispatch log + error tracking |
| `audit_log` | Frontend user action events (optional; feature-flagged) |

### 3.5 Operational Transform Rebase

The `internal/rebase` package implements a pure-Go OT rebase engine that is
byte-for-byte compatible with the original Clojure (`rebase.cljc`) and Rust
(`logos-rebase`) implementations. It covers the full 5×5 conflict matrix:

| Type | mod-obj | del-obj | add-obj | mov-obj | mov-pages |
|---|---|---|---|---|---|
| **mod-obj** | merge set-ops | drop incoming | preserve | preserve | preserve |
| **del-obj** | preserve | preserve | preserve | drop competing | preserve |
| **add-obj** | preserve | preserve | preserve (adjust idx) | preserve | preserve |
| **mov-obj** | preserve | drop incoming | preserve | adjust index | preserve |
| **mov-pages** | preserve | preserve | preserve | preserve | merge |

20 unit tests mirror the full Rust test suite.

### 3.6 .logos File Format (binfile)

The `internal/binfile` package handles the `.logos` v3 ZIP archive format.
Go-generated archives include:
- `manifest.json` — format version + `go-extension: true` flag
- `files/{id}/attrs.json` — file metadata
- `files/{id}/changes.json` — Go change history (JSON-encoded)
- `files/{id}/data.bin` — raw Clojure CRDT blob (preserved for round-trips)
- `files/{id}/pages.json` — ordered page UUIDs
- `media/{id}.json` + `media/{id}` — media metadata and raw blobs

8 round-trip tests validate the full import/export cycle.

### 3.7 Background Tasks

Implemented as goroutines started at server startup:
- `broadcastFileChange` — Redis pub/sub fan-out to WebSocket clients
- `DispatchEvent` — async webhook delivery with retry + deactivation logic

---

## 4. WebAssembly Renderer

`render-wasm/` — Rust crate compiled to WASM via `wasm-bindgen`.

### 4.1 Architecture

```
TypeScript (logos-app) → wasm-bindgen glue → Rust render loop → Skia (GPU canvas)
```

### 4.2 Key Algorithms

- **Shape rasterisation**: each shape type (rect, path, text, image) dispatched to a Skia `Canvas` draw call
- **Layer compositing**: blend modes (`multiply`, `screen`, `overlay`, etc.) applied using Skia's `Paint::set_blend_mode`
- **Font rendering**: Skia's `TextBlob` for shaped text; font data passed as `ArrayBuffer` from the JS side
- **Anti-aliasing**: MSAA via Skia's GPU backend when WebGL2 is available; software rasteriser fallback
- **Viewport culling**: AABB intersection test against the current viewport before issuing draw calls

### 4.3 Build

```bash
make build-wasm
# Output → logos-app/public/logos-layout/
```

---

## 5. Real-Time Collaboration

### 5.1 Transport

WebSocket connection per workspace session to backend endpoint `/ws/notifications`.

### 5.2 CRDT Operations

Every edit produces a vector of **change operations**:

```json
{"type": "add-obj",   "id": "<uuid>", "obj": { ... shape fields ... }}
{"type": "mod-obj",   "id": "<uuid>", "operations": [{"type": "set", "attr": "x", "val": 100}]}
{"type": "del-obj",   "id": "<uuid>"}
{"type": "mov-objects","parentId": "<uuid>", "shapes": ["<uuid>", ...]}
```

Changes are sent to the backend RPC `update-file`. Backend:
1. Acquires a row-level lock (`SELECT … FOR UPDATE`) on the file
2. Checks `vern` (concurrent edit sentinel) and `revn` (change revision)
3. Loads competing change-sets from `file_change`; runs OT rebase if needed
4. Inserts the rebased change-set into `file_change`
5. Broadcasts to all other active sessions via Redis pub/sub

### 5.3 Conflict Resolution

- **Last-write-wins** per attribute: concurrent edits to different attributes of the same shape merge cleanly via the OT rebase engine
- **Presence**: each client sends cursor position / selection as ephemeral events (not persisted); displayed as coloured cursors

---

## 6. Plugin System

`plugins/` — TypeScript SDK for third-party plugins.

### 6.1 Sandboxing

Each plugin runs in a **sandboxed `<iframe>`** served from a separate origin. Communication with the host app uses `postMessage` with a structured API.

### 6.2 Plugin Lifecycle

1. Manifest loaded — specifies name, host, permissions
2. Host creates `<iframe>` pointing to plugin URL
3. Plugin calls `logos.ui.open()` to show a panel inside the app
4. API calls are serialised as messages, executed in the host context, and results returned

### 6.3 API Surface

Key namespaces exposed to plugins:
- `logos.selection` — get/set selected shapes
- `logos.page` — create, update, delete shapes
- `logos.viewport` — pan/zoom
- `logos.library` — read shared styles/components
- `logos.theme` — current dark/light theme

---

## 7. MCP Tool Server

`mcp/` — Model Context Protocol server enabling AI agents to interact with Logos.

### 7.1 Architecture

```
AI Agent (e.g. Claude)  ←→  MCP Protocol (stdio/SSE)
                                    │
                          LogosMcpServer (TypeScript)
                                    │
                          Logos Plugin (iframe bridge)
                                    │
                          Logos frontend canvas
```

### 7.2 Key Classes

| File | Class | Role |
|---|---|---|
| `server/src/LogosMcpServer.ts` | `LogosMcpServer` | Lifecycle, tool registry, WebSocket bridge |
| `server/src/PluginBridge.ts` | `PluginBridge` | Per-session WebSocket ↔ browser plugin channel |
| `server/src/tools/ExecuteCodeTool.ts` | `ExecuteCodeTool` | Runs arbitrary JS in the Logos plugin context |
| `server/src/tools/ExportShapeTool.ts` | `ExportShapeTool` | Exports a shape as SVG/PNG |
| `server/src/tools/LogosApiInfoTool.ts` | `LogosApiInfoTool` | Returns Logos plugin API documentation |
| `server/src/tools/ImportImageTool.ts` | `ImportImageTool` | Imports an image into the current page |
| `plugin/src/LogosUtils.ts` | `LogosUtils` | Base64 helpers, shape serialisation utilities |

### 7.3 Session Flow

1. MCP client starts the server process
2. `LogosMcpServer` starts an HTTP server for the browser plugin to connect via WebSocket
3. Designer opens Logos → enables the MCP plugin → plugin connects WebSocket to MCP server
4. AI agent calls a tool; `PluginBridge` serialises to a task message and sends to the plugin
5. Plugin executes the task (using the Logos plugin API) and returns results
6. Results are returned to the AI agent as MCP tool output

---

## 8. Authentication & Session Flow

### 8.1 Cookie-Based Sessions

1. `POST /api/rpc/command/login-with-password` → Go backend validates Argon2id hash → creates JWE session token → sets `auth-token` HttpOnly cookie (SameSite=Lax)
2. All subsequent requests carry the cookie → middleware decrypts JWE → populates `profileID` in request context
3. Frontend detects auth state via `GET /api/rpc/command/get-profile`

### 8.2 Token Format

- **JWE (JSON Web Encryption)** with `ver=1` header — interoperable with the former Clojure backend
- **Transit JSON** payload encoding — keywords (`~:`), UUIDs (`~u`), timestamps (`~t`) match Clojure conventions exactly
- **Argon2id** password hashing — PHC string format `$argon2id$v=19$m=32768,t=3,p=2$…` matches `buddy-hashers` default parameters

### 8.3 API Access Tokens

API tokens are stored in `access_token` table with configurable expiry. The `Authorization: Bearer <token>` header is an alternative to the session cookie for programmatic API access.

---

## 9. State Management — Zustand

The `logos-app` frontend uses **Zustand** stores for all client-side state.

### 9.1 Core Stores

```
documentStore  — pages array, shapes map (id → Shape)
selectionStore — Set<id> of selected shape IDs
uiStore        — activeTool, panel visibility flags
```

### 9.2 Collaboration Sync

Changes from `update-file` responses (rebased change-sets from the server) are applied to `documentStore` via a reducer. WebSocket push events from other collaborators follow the same path.

### 9.3 Undo/Redo

Every workspace mutation produces an inverse change-set stored in a bounded stack (50 steps). Undo replays the inverse ops against `documentStore`; the inverse is also sent to the server via `update-file` to keep the server state consistent.

---

## 10. File Format & CRDT

### 10.1 Change Log (Event Sourcing)

Design files are stored as an **append-only log** in `file_change`. This enables:
- **Collaboration**: broadcast deltas to other clients
- **History / time travel**: replay changes up to a revision
- **Compaction**: labeled `file_change` rows act as snapshots; the `restore-file-snapshot` RPC replays history back to a snapshot

### 10.2 Storage Encoding

| Writer | Encoding | `file_change.changes` format |
|---|---|---|
| Clojure backend | Transit+zstd BLOB | Binary — skipped by Go rebase (conservative) |
| Go backend | JSON | `[{"type":"add-obj", ...}]` array |

During the migration window, the Go backend loads only JSON-parseable rows for OT rebase. Clojure-encoded rows are skipped, preserving correctness at the cost of potentially producing a wider rebase window.

### 10.3 .logos v3 Archive Format

See §3.6 for the full archive structure. The `go-extension: true` manifest flag allows:
- Clojure importers to skip Go-specific entries (unknown JSON keys are ignored)
- Go importers to switch on Go-specific behaviour (e.g. read `changes.json`)

---

## 11. Duotone Icon System

`logos-app/src/assets/icons/`

- ~4000 SVG icons from the Duotone Font Awesome Pro set, re-exported as React components
- Icons support foreground/background color via CSS `currentColor` on two `<path>` elements with different `opacity`
- Used in: layers panel sidebar, keyboard shortcut hints, component browser

---

## 12. Infrastructure & Deployment

### 12.1 Docker

> **Note:** Release Docker images and the production `docker-compose.yaml` have been removed
> from the repository pending the community edition release. They will be recreated under the
> `logos/` Docker Hub namespace when the first stable release is cut.

Development containers remain in `docker/devenv/` (local dev environment) and `docker/gitpod/`
(Gitpod workspace).

The Go backend produces a single ~20 MB statically-linked binary with no JVM dependency. A
future `Dockerfile.backend` will be a simple two-stage build:

```dockerfile
FROM golang:1.23-alpine AS builder
WORKDIR /src
COPY backend-go/ .
RUN CGO_ENABLED=0 go build -trimpath -o /bin/logos-backend ./cmd/server

FROM debian:bookworm-slim
COPY --from=builder /bin/logos-backend /bin/logos-backend
ENTRYPOINT ["/bin/logos-backend"]
```

### 12.2 Helm Chart (Kubernetes)

> **Note:** The Helm chart (`deploy/helm/`) has been removed along with the Docker images.
> It will be re-created for the community release with up-to-date image references and
> enterprise feature gates.

### 12.3 Key Environment Variables (Go Backend)

| Variable | Purpose |
|---|---|
| `DATABASE_URL` | PostgreSQL connection string |
| `REDIS_URL` | Redis/Valkey connection string |
| `LOGOS_SECRET_KEY` | Master key for token derivation (HKDF-Blake2b-512) |
| `BACKEND_GO_ADDR` | HTTP listen address (default `:8080`) |
| `STORAGE_BACKEND` | `fs` (default) or `s3` |
| `STORAGE_LOCAL_DIR` | Root directory for local storage |
| `STORAGE_S3_BUCKET` | S3/MinIO bucket name |
| `LOGOS_ENABLE_AUDIT_LOG` | Enable `audit_log` writes (`true`/`false`) |
| `LOGOS_ENABLE_DEMO_USERS` | Enable demo user creation endpoint |
| `LOGOS_ENABLE_USER_FEEDBACK` | Enable feedback submission endpoint |

### 12.4 CI / CD

| Workflow | Trigger | Purpose |
|---|---|---|
| `logos-app.yml` | Push to main / logos-app paths | TypeScript type-check + Vite build |
| `typecheck.yml` | Rust type changes | Verify rust-generated TypeScript types are up-to-date |
| `benchmark-memory.yml` | Nightly 02:00 UTC | Heap regression benchmark (Go backend + Playwright) |
| `release.yml` | Tag push | Build + publish Docker images |
| `rust.yml` | Push to main / rust paths | Rust test suite |

---

## Appendix: Key File Map

```
logos-app/src/
  components/canvas/     Canvas.tsx — WASM bridge + Canvas 2D fallback
  components/toolbar/    Toolbar.tsx — tool selection
  components/layers/     LayersPanel.tsx — virtualised shape tree
  stores/documentStore.ts
  stores/selectionStore.ts
  stores/uiStore.ts
  render-wasm/scene.ts   — 104-byte WASM ABI
  worker/                — layout, snap, serialize workers

backend-go/
  cmd/server/main.go     — entry point, config, dependency injection
  cmd/gen-benchmark/     — .logos fixture generator (CI use)
  internal/
    auth/                — Argon2id hashing, JWE tokens, Transit JSON
    binfile/             — .logos v3 ZIP export/import
    config/              — environment variable loading
    db/                  — pgx connection pool helpers
    email/               — email stub (stdout logger)
    handler/             — all 25 RPC command handlers
    perms/               — project/file permission helpers
    rebase/              — pure-Go OT rebase engine (20 tests)
    server/              — Chi router, route wiring
    storage/             — local FS + S3 object storage

rust/
  logos-layout/          — Flexbox/Grid layout engine (WASM)
  logos-types/           — Canonical type definitions + TS code generator
  logos-rebase/          — OT rebase (rlib; Go fallback used by backend)
  render-wasm/           — Skia-based WebGPU/Canvas renderer

mcp/
  packages/server/src/LogosMcpServer.ts
  packages/plugin/src/LogosUtils.ts
```
