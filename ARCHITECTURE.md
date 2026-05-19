# Logos Architecture

> Logos is a fork of [Penpot](https://penpot.app) — an open-source design and prototyping platform. This document describes the full technical architecture: algorithms, subsystems, data flows, and implementation patterns.

---

## Table of Contents

1. [High-Level Overview](#1-high-level-overview)
2. [Frontend — React/TypeScript SPA](#2-frontend--reacttypescript-spa)
3. [Backend — Clojure API Server](#3-backend--clojure-api-server)
4. [Common — Shared Logic](#4-common--shared-logic)
5. [WebAssembly Renderer](#5-webassembly-renderer)
6. [Real-Time Collaboration](#6-real-time-collaboration)
7. [Plugin System](#7-plugin-system)
8. [MCP Tool Server](#8-mcp-tool-server)
9. [Exporter](#9-exporter)
10. [Authentication & Session Flow](#10-authentication--session-flow)
11. [State Management — PTK](#11-state-management--ptk)
12. [File Format & CRDT](#12-file-format--crdt)
13. [Duotone Icon System](#13-duotone-icon-system)
14. [Infrastructure & Deployment](#14-infrastructure--deployment)

---

## 1. High-Level Overview

```
Browser (React + TypeScript SPA — logos-app)
       │  HTTP RPC + WebSocket
       ▼
Clojure Backend (API + WebSocket)
       │
       ├── PostgreSQL  (persistent data)
       ├── Redis       (pub/sub, sessions, rate-limiting)
       └── S3 / object store (media assets)
```

### Runtime Languages

| Layer | Language | Build Tool |
|---|---|---|
| **Frontend SPA** | **TypeScript 5 + React 18** | **Vite 6** |
| Backend | Clojure (JVM) | deps.edn / Integrant |
| Shared schemas | Clojure (Malli) + generated TypeScript | `bin/generate-types` |
| Renderer | Rust → WebAssembly | cargo / wasm-pack |
| Shaders | WGSL | naga / wgpu pipeline |
| Exporter | ClojureScript (Node.js) | shadow-cljs |
| Plugins | TypeScript (sandboxed iframe) | esbuild / vite |
| MCP server | TypeScript | tsup / Node.js |
| Migrations | PostgreSQL / SQL | Flyway |

### Language Breakdown

| Language | Role | Approx. % |
|---|---|---|
| **TypeScript** | Frontend SPA, workers, MCP server, Plugin SDK | **~45%** |
| **Clojure** | Backend server, shared schemas (`common/`), exporter | **~35%** |
| **Rust** | Layout engine, renderer, rebase, vector graphics, WASM | **~15%** |
| WGSL | WebGPU compute & render shaders | ~3% |
| SQL | PostgreSQL migrations | ~2% |

### Development Ports

| Service | Port | Command |
|---|---|---|
| **logos-app dev server** | **5173** | **`logos dev`** |
| Backend HTTP | 3449 | `bash scripts/start-dev-local` |
| nREPL (backend) | 6061 | auto-started by Integrant |

---

## 2. Frontend — React/TypeScript SPA

> **Migration complete (CS1–CS2)**: The ClojureScript frontend (`frontend/`) has been superseded
> by `logos-app/`, a Vite + React 18 + TypeScript 5 application backed by the same Rust/WASM
> rendering core.  The `common/` shared library is now pure Clojure (JVM); TypeScript consumer
> types are auto-generated from Malli schemas by `bin/generate-types`.  No ClojureScript
> toolchain is required to work on `logos-app/` or `backend/`.

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

### 2.7 Thumbnail Rasterizer

A separate `rasterizer.js` chunk runs in a `<iframe>` sandboxed context:
1. Dashboard requests a thumbnail via `postMessage`
2. Rasterizer renders the page to an offscreen `<canvas>` using SVG rendering
3. Returns a PNG data-URL back to the parent
4. Backend stores thumbnails in object storage; freshness tracked per file revision

---

## 3. Backend — Clojure API Server

### 3.1 Technology Stack

- **Integrant** — component lifecycle (start/stop, dependency graph defined in `backend/src/app/config.clj`)
- **Yetti** / Jetty — HTTP server
- **next.jdbc** — PostgreSQL adapter
- **Carmine** — Redis client
- **Buddy** — JWT / session auth

### 3.2 RPC Command System

All API calls are **RPC commands** — plain Clojure maps dispatched by `:type` key.

```
HTTP POST /api/rpc/command/<cmd-name>
         → backend reads body as Transit-JSON
         → resolves handler in app.rpc.<module>/<cmd-name>
         → calls handler with (ctx params) → returns Transit
```

Handler namespaces:
- `app.rpc.commands.auth` — login, logout, register, OAuth
- `app.rpc.commands.files` — CRUD for design files
- `app.rpc.commands.projects` — project management
- `app.rpc.commands.profile` — user profile
- `app.rpc.commands.media` — image upload / transcoding
- `app.rpc.commands.search` — full-text search via PostgreSQL `tsvector`

### 3.3 Middleware Stack

```
SSL termination (reverse proxy)
  → Content-Security-Policy headers
  → Rate limiter (per-IP, per-user via Redis)
  → Session cookie validation
  → Transit JSON decoder
  → RPC dispatch
  → Error handler → Transit JSON encoder
```

### 3.4 Database Schema

Key tables (PostgreSQL):

| Table | Purpose |
|---|---|
| `profile` | User accounts |
| `team` | Workspaces / organisations |
| `team_profile_rel` | Team membership + role |
| `project` | Grouping of files |
| `file` | Design file metadata |
| `file_data` | Binary (Transit) serialisation of the file's shape tree |
| `file_object_thumbnail` | Per-frame/object thumbnail metadata |
| `file_change` | Append-only log of CRDT operations |
| `share_link` | Shareable view links |
| `media_object` | Uploaded images/videos |

### 3.5 Task Queue / Background Workers

Implemented via `app.worker` namespace using database-backed job queues:
- `file-media-gc` — garbage-collect orphaned media
- `file-snapshot` — periodically compact CRDT history
- `send-email` — async email delivery (Sendgrid or SMTP)
- `object-storage-gc` — remove orphaned S3 objects

### 3.6 Email Templates

Located in `backend/resources/app/email/`. Each template is an HTML + text pair. Template variables are Clojure maps rendered with Selmer.

---

## 4. Common — Shared Logic

`common/` is a library compiled for both JVM (backend) and JS (frontend/exporter).

### 4.1 Shape Data Model

`app.common.types.shape` — every shape is a plain Clojure map. Core keys:

```clojure
{:id     uuid
 :type   :rect | :path | :text | :group | :frame | :image | :bool | :svg-raw | :component
 :name   string
 :x :y  number   ;; absolute position
 :width :height  number
 :rotation number  ;; degrees
 :transform   Matrix   ;; optional affine transform
 :fills   [{:fill-color :fill-opacity ...}]
 :strokes [{:stroke-color :stroke-width ...}]
 :shadow  [...shadow-attrs]
 :blur    {:type :blur, :value n}
 :constraints-h :left|:right|:leftright|:center|:scale
 :constraints-v :top|:bottom|:topbottom|:center|:scale}
```

### 4.2 Path Algorithms

`app.common.path.*` — Bézier path manipulation:

- **Segment operations**: split, join, add/remove nodes
- **Boolean ops** (`app.common.path.bool`): union, intersection, difference, exclusion — implemented using the Sutherland-Hodgman and Greiner-Hormann polygon clipping algorithms adapted for cubic Bézier curves
- **Curve approximation**: recursive de Casteljau subdivision to approximate Bézier arcs with line segments for hit-testing

### 4.3 Geometry

`app.common.geom.*`:

- `app.common.geom.rect` — axis-aligned bounding box (AABB) operations
- `app.common.geom.point` — 2D vector arithmetic
- `app.common.geom.matrix` — 3×3 affine transform matrices (multiply, invert, apply-to-point)
- `app.common.geom.shapes` — bounding box of a transformed shape considering strokes
- Hit-testing: point-in-polygon (ray casting for closed paths), point-near-segment for strokes

### 4.4 Layout Engine

`app.common.types.shape.layout` — implements CSS Flexbox semantics for "Auto Layout" frames:

- **Main axis** determination (row / column / row-reverse / column-reverse)
- **Cross axis** alignment (align-items: start / center / end / stretch)
- **Gap** computation including "space-between" / "space-around"
- Children are positioned by solving the layout independently of the SVG renderer — results are applied as `:x :y` updates to child shapes

### 4.5 Components (Main/Copy)

`app.common.types.component`:

- Every component has a **main copy** stored in the component library file
- **Copies** spread across files hold `:component-id` + `:component-file` references
- Sync algorithm (`app.common.logic.component-sync`): walks the shape tree recursively, computes a diff between main and copy, applies non-locally-overridden attributes (a "deep merge" with override tracking)

---

## 5. WebAssembly Renderer

`render-wasm/` — Rust crate compiled to WASM via `wasm-bindgen`.

### 5.1 Architecture

```
ClojureScript  →  JS wrappers (render_wasm.cljs)
                         │
                   wasm-bindgen glue
                         │
                   Rust render loop
                         │
                   Skia (Skia-safe crate) → GPU canvas
```

### 5.2 Key Algorithms

- **Shape rasterisation**: each shape type (rect, path, text, image) dispatched to a Skia `Canvas` draw call
- **Layer compositing**: blend modes (`multiply`, `screen`, `overlay`, etc.) applied using Skia's `Paint::set_blend_mode`
- **Font rendering**: Skia's `TextBlob` for shaped text; font data passed as `ArrayBuffer` from the JS side
- **Anti-aliasing**: MSAA via Skia's GPU backend when WebGL2 is available; software rasteriser fallback
- **Viewport culling**: AABB intersection test against the current viewport before issuing draw calls

### 5.3 Build

```bash
cd render-wasm
wasm-pack build --target web --out-dir ../frontend/target/wasm
```

---

## 6. Real-Time Collaboration

### 6.1 Transport

WebSocket connection per workspace session to backend endpoint `/ws/notifications`.

### 6.2 CRDT Operations

`app.common.files.changes` — every edit produces a vector of **change operations**:

```clojure
{:type :add-obj   :id uuid :obj shape-map}
{:type :mod-obj   :id uuid :operations [{:type :set :attr :x :val 100}]}
{:type :del-obj   :id uuid}
{:type :mov-objects :parent-id uuid :shapes [uuid...]}
```

Changes are sent to the backend RPC `update-file`. Backend:
1. Validates and applies changes to the canonical file state
2. Appends changes to `file_change` log (append-only)
3. Broadcasts changes to all other active sessions via Redis pub/sub

### 6.3 Conflict Resolution

- **Last-write-wins** per attribute: concurrent edits to different attributes of the same shape merge cleanly
- **Presence**: each client sends cursor position / selection as ephemeral presence events (not persisted); displayed as coloured cursors on collaborators' screens

---

## 7. Plugin System

`plugins/` — TypeScript SDK for third-party plugins.

### 7.1 Sandboxing

Each plugin runs in a **sandboxed `<iframe>`** served from a separate origin. Communication with the host app uses `postMessage` with a structured API.

### 7.2 Plugin Lifecycle

1. Manifest loaded (`plugin.cljs` in host) — specifies name, host, permissions
2. Host creates `<iframe>` pointing to plugin URL
3. Plugin calls `penpot.ui.open()` to show a panel inside the app
4. API calls (`penpot.selection.get()`, `penpot.page.createShape()`, etc.) are serialised as messages, executed in the host context, and results returned

### 7.3 API Surface

Key namespaces exposed to plugins:
- `penpot.selection` — get/set selected shapes
- `penpot.page` — create, update, delete shapes
- `penpot.viewport` — pan/zoom
- `penpot.library` — read shared styles/components
- `penpot.theme` — current dark/light theme

---

## 8. MCP Tool Server

`mcp/` — Model Context Protocol server enabling AI agents to interact with Logos.

### 8.1 Architecture

```
AI Agent (e.g. Claude)  ←→  MCP Protocol (stdio/SSE)
                                    │
                          LogosMcpServer (TypeScript)
                                    │
                          Logos Plugin (iframe bridge)
                                    │
                          Logos frontend canvas
```

### 8.2 Key Classes

| File | Class | Role |
|---|---|---|
| `server/src/LogosMcpServer.ts` | `LogosMcpServer` | Lifecyle, tool registry, WebSocket bridge |
| `server/src/PluginBridge.ts` | `PluginBridge` | Per-session WebSocket ↔ browser plugin channel |
| `server/src/tools/ExecuteCodeTool.ts` | `ExecuteCodeTool` | Runs arbitrary JS in the Logos plugin context |
| `server/src/tools/ExportShapeTool.ts` | `ExportShapeTool` | Exports a shape as SVG/PNG |
| `server/src/tools/LogosApiInfoTool.ts` | `LogosApiInfoTool` | Returns Logos plugin API documentation |
| `server/src/tools/ImportImageTool.ts` | `ImportImageTool` | Imports an image into the current page |
| `plugin/src/LogosUtils.ts` | `LogosUtils` | Base64 helpers, shape serialisation utilities |

### 8.3 Session Flow

1. MCP client starts the server process
2. `LogosMcpServer` starts an HTTP server for the browser plugin to connect via WebSocket
3. Designer opens Logos → enables the MCP plugin → plugin connects WebSocket to MCP server
4. AI agent calls a tool; `PluginBridge` serialises to a task message and sends to the plugin
5. Plugin executes the task (using the Logos plugin API) and returns results
6. Results are returned to the AI agent as MCP tool output

---

## 9. Exporter

`exporter/` — a headless Node.js ClojureScript process for server-side export.

### 9.1 How It Works

1. Backend spawns the exporter process via `java.lang.ProcessBuilder`
2. Exporter receives a file ID + export parameters over stdin (Transit)
3. Loads a headless Chromium (via Playwright) and navigates to the viewer URL with auth token
4. Captures the rendered SVG/canvas and converts to the requested format (PNG, SVG, PDF)
5. Returns binary output on stdout back to the backend, which streams it to the HTTP response

---

## 10. Authentication & Session Flow

### 10.1 Cookie-Based Sessions

1. `POST /api/rpc/command/login-with-password` → backend validates credentials → sets `auth-token` HttpOnly cookie (SameSite=Lax)
2. All subsequent requests carry the cookie → middleware extracts it → validates JWT → populates `::session/profile-id` in request context
3. Frontend detects auth state via `GET /api/rpc/command/get-profile`

### 10.2 OAuth Providers

Supported: GitHub, GitLab, Google, OpenID Connect generic.

Flow: frontend redirects to `/api/rpc/command/auth/oauth/<provider>` → backend issues provider redirect → callback at `/api/rpc/command/auth/oauth/<provider>/callback` → creates/links profile → sets session cookie → redirects to `/#/auth/verify-token`.

### 10.3 CORS / Same-Site Dev Config

In local dev with split ports (frontend :8888, backend :3449):
- Backend `allowed-origins` includes `http://localhost:8888`
- Cookie `SameSite=Lax` allows cross-port same-host cookies
- `valid-location?` in routes.cljs accepts any `http://localhost` origin

---

## 11. State Management — PTK

PTK (Potok) is the event-driven architecture used throughout the frontend.

### 11.1 Core Concepts

```
App State (atom)    ←── deref ── Rumext components
     │
     └── PTK events (records implementing protocols)
             │
        `:update`   → pure function (state → state)
        `:watch`    → returns an observable of more events
        `:effect`   → side effects (DOM, network, etc.)
```

### 11.2 Event Lifecycle

```clojure
(defrecord MoveShape [id delta]
  ptk/UpdateEvent
  (update [_ state]
    (update-in state [:workspace-data :shapes id] move-by delta))

  ptk/WatchEvent
  (watch [_ state stream]
    (rx/of (sync-shape-to-server id))))
```

Events are dispatched via `st/emit!`. The PTK store processes them serially, ensuring consistent state transitions.

### 11.3 Undo/Redo

Every workspace mutation generates a **change log entry**. Undo replays the inverse operations (using `:undo-changes` alongside `:redo-changes` in each operation record). The stack is capped at 50 steps per session.

---

## 12. File Format & CRDT

### 12.1 Binary Format

Design files are serialised as **Transit-JSON** (a superset of JSON with efficient typed extensions for UUIDs, dates, etc.) then optionally gzip-compressed before storage in PostgreSQL as `BYTEA`.

### 12.2 Change Log (Event Sourcing)

Every `update-file` call appends to `file_change`. This enables:
- **Collaboration**: broadcast deltas to other clients
- **History / time travel**: replay changes up to a revision
- **Compaction**: background worker snaps the full state periodically and truncates old log entries

### 12.3 CRDT Properties

The change model is a **state-based CRDT**:
- Each attribute last-write-wins (via server timestamp / version counter)
- Structural operations (add/delete shapes, reorder layers) are ordered by the server's log sequence
- No operational transform required; conflicts produce deterministic merged state

---

## 13. Duotone Icon System

`frontend/src/app/main/ui/ds/foundations/assets/duotone_icon.cljs`

### 13.1 Overview

- ~4000 SVG icons from the Duotone Font Awesome Pro set, embedded as ClojureScript
- Icons are used in: layer panel sidebar (`layer_item.cljs`), keyboard shortcut hints (`shortcuts.cljs`), component browser (`component.cljs`)

### 13.2 Implementation

- `frontend/resources/images/icons/duotone/` — source SVG files
- `frontend/scripts/generate-duotone-icons.mjs` — reads SVGs and generates the 4768-line ClojureScript file
- Each icon is a `(defn icon-name [])` Rumext component rendering the SVG inline
- Supports foreground/background color via CSS `currentColor` on two `<path>` elements with different `opacity`

---

## 14. Infrastructure & Deployment

### 14.1 Docker Compose (Dev & Self-Hosted)

`docker/` contains:
- `images/penpot/backend/` — Clojure backend JVM image
- `images/penpot/frontend/` — Nginx serving built JS/CSS assets
- `images/penpot/exporter/` — Node.js exporter
- PostgreSQL and Redis as sidecar containers

### 14.2 Helm Chart (Kubernetes)

`deploy/helm/` provides a production-grade Helm chart with:
- Separate `Deployment` per service (frontend, backend, exporter)
- `HorizontalPodAutoscaler` for backend
- `PersistentVolumeClaims` for PostgreSQL
- Configmap for `PENPOT_*` environment variables

### 14.3 Key Environment Variables (Backend)

| Variable | Purpose |
|---|---|
| `PENPOT_PUBLIC_URI` | Public URL of the instance |
| `PENPOT_DATABASE_URI` | PostgreSQL JDBC URL |
| `PENPOT_REDIS_URI` | Redis connection string |
| `PENPOT_STORAGE_BACKEND` | `fs` or `s3` |
| `PENPOT_FLAGS` | Feature flags (e.g. `enable-registration`) |
| `PENPOT_SECRET_KEY` | JWT signing secret |

### 14.4 CI / CD

`run-ci.sh` — runs tests across backend (Clojure), frontend (ClojureScript vitest), common library, and plugins.

`netlify.toml` — docs site deployment config.

---

## Appendix: Key File Map

```
frontend/
  src/app/
    config.cljs              ← reads runtime JS globals
    main.cljs                ← app entry point
    main/
      ui/routes.cljs         ← client-side router
      data/workspace/        ← workspace PTK events
      ui/workspace/          ← Rumext canvas components
      ui/dashboard/          ← Dashboard page
      ui/ds/                 ← Design System components
        foundations/assets/
          duotone_icon.cljs  ← 4000+ icon components
  resources/public/
    index.html               ← HTML shell
    js/config.js             ← runtime config (generated)

backend/
  src/app/
    config.clj               ← Integrant component map
    rpc/commands/            ← API RPC handlers
    db.clj                   ← DB helpers (next.jdbc)
    redis.clj                ← Redis client
    worker/                  ← background job processors
  scripts/
    start-dev-local          ← local dev start script
    _env.local               ← local env overrides

common/
  src/app/common/
    types/shape.cljc          ← shape data model
    geom/                     ← geometry algorithms
    path/                     ← Bézier / boolean ops
    types/component.cljc      ← component model
    files/changes.cljc        ← CRDT change operations

render-wasm/
  src/                        ← Rust Skia renderer
  build.rs                    ← wasm-bindgen build

mcp/
  packages/server/src/
    LogosMcpServer.ts         ← MCP server entry
    PluginBridge.ts           ← WebSocket ↔ browser
    tools/                    ← individual MCP tools
  packages/plugin/src/
    LogosUtils.ts             ← plugin utilities
```
