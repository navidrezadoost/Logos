# Logos — Comprehensive Technical Analysis & Optimization Plan

> Document date: May 17, 2026  
> Codebase commit: `4c90543` (branch `main`)

---

## Table of Contents

1. [Project Identity & Purpose](#1-project-identity--purpose)
2. [Repository Layout](#2-repository-layout)
3. [Language Inventory](#3-language-inventory)
4. [Runtime Architecture Diagram](#4-runtime-architecture-diagram)
5. [Frontend — ClojureScript SPA](#5-frontend--clojurescript-spa)
   - 5.1 State Management (PTK)
   - 5.2 Rendering Pipeline
   - 5.3 WebAssembly Renderer
   - 5.4 Worker Thread
   - 5.5 Plugin System
6. [Common Library — Shared Algorithms](#6-common-library--shared-algorithms)
   - 6.1 Geometry Engine
   - 6.2 Layout Engine (Flex & Grid)
   - 6.3 Path & Boolean Geometry
   - 6.4 CRDT File Format
   - 6.5 Change-Set / Undo System
   - 6.6 Snapping (Range Tree)
   - 6.7 Schema / Validation
7. [Backend — Clojure Server](#7-backend--clojure-server)
   - 7.1 HTTP & RPC Layer
   - 7.2 Database Layer
   - 7.3 Real-Time Collaboration (WebSocket + Redis PubSub)
   - 7.4 File Storage
   - 7.5 Binary File Format (BinFile v1-v3)
   - 7.6 Background Task System
   - 7.7 Authentication & Session
   - 7.8 Quota & Rate Limiting
8. [Render-WASM — Rust / Skia Renderer](#8-render-wasm--rust--skia-renderer)
9. [MCP Tool Server](#9-mcp-tool-server)
10. [Exporter Service](#10-exporter-service)
11. [Infrastructure & Deployment](#11-infrastructure--deployment)
12. [Current State Assessment](#12-current-state-assessment)
    - 12.1 Strengths
    - 12.2 Bottlenecks & Risks
13. [Phased Optimization Plan](#13-phased-optimization-plan)
    - Phase 0 — Foundation Hygiene (weeks 1-2)
    - Phase 1 — Performance Hotspots (weeks 3-6)
    - Phase 2 — Scalability (weeks 7-12)
    - Phase 3 — Developer Experience (weeks 13-18)
    - Phase 4 — Feature Quality (weeks 19-26)
14. [Key File Reference Map](#14-key-file-reference-map)

---

## 1. Project Identity & Purpose

**Logos** (formerly Logos) is an open-source, browser-based vector design tool built for multi-user real-time collaboration. It is the only design tool in the world whose file format is based on open web standards (SVG/CSS). It runs as a self-hosted application backed by a Clojure API server, served to users as a ClojureScript single-page application.

Core differentiators:
- **SVG-native** — all design data is expressed as SVG, no proprietary binary opaque format.
- **CSS-aware** — grid/flex layouts directly map to CSS layout models.
- **CRDT-based collaboration** — concurrent edits from multiple clients are merged without conflict.
- **Plugin API** — third-party JavaScript plugins run inside a sandboxed iframe.
- **WASM renderer** — a Rust/Skia GPU renderer delivers high-fidelity pixel-accurate output.

---

## 2. Repository Layout

```
Logos/
├── backend/          Clojure HTTP + RPC + DB + WebSocket server
├── common/           Shared ClojureScript/Clojure algorithms (geometry, CRDT, types)
├── frontend/         ClojureScript SPA (shadow-cljs)
├── render-wasm/      Rust → WebAssembly GPU renderer (Skia via wasm-bindgen)
├── exporter/         Puppeteer-based PDF/PNG/SVG headless exporter
├── mcp/              Model Context Protocol TypeScript AI tool server
├── plugins/          Plugin SDK + documentation (TypeScript)
├── library/          Shared UI component library (ClojureScript)
├── deploy/           Helm charts
├── docker/           Dockerfile images + devenv
└── docs/             11ty static documentation site
```

---

## 3. Language Inventory

| Language | Files | Primary Role |
|---|---|---|
| ClojureScript (`.cljs` / `.cljc`) | ~940 | Frontend SPA, worker thread, shared algorithms |
| Clojure (`.clj` / `.cljc`) | ~134 | Backend server, migrations, shared utilities |
| Rust (`.rs`) | ~78 | WebAssembly renderer (render-wasm) |
| TypeScript (`.ts` / `.tsx`) | ~142 | MCP server, plugin SDK, exporter |
| JavaScript (`.js`) | ~2,575 | Build tooling, generated code, test helpers |
| SQL | ~144 migrations | PostgreSQL schema |
| CSS / SCSS | ~hundreds | Frontend styling |
| HTML (Mustache) | ~5 templates | Server-rendered shell pages |
| WGSL (shader) | removed | Previously in deleted logos-render crate |

**Dependency managers in use:**

| Ecosystem | Tool | Config file |
|---|---|---|
| Clojure/ClojureScript | deps.edn + shadow-cljs | `deps.edn`, `shadow-cljs.edn` |
| Rust | Cargo | `Cargo.toml` |
| Node.js | pnpm (workspaces) | `pnpm-workspace.yaml` |

---

## 4. Runtime Architecture Diagram

```
Browser (User)
│
├── Main Thread  ─────────────────────────────────────────────────────────────┐
│   ClojureScript SPA (shadow-cljs bundle)                                    │
│   • PTK event loop (RxJS streams)                                           │
│   • React/Reagent UI components                                             │
│   • Canvas 2D renderer (thumbnail & thumbnail fallback)                     │
│   • WASM bridge (render_wasm API)                                           │
│   • WebSocket client (real-time collaboration)                              │
│                                                                             │
├── Worker Thread ────────────────────────────────────────────────────────────┤
│   • Snapping engine (balanced range tree)                                   │
│   • Thumbnail generation                                                    │
│   • File import parsing                                                     │
│   • Selection & path hit-testing                                            │
│                                                                             │
└── WebAssembly Module (render-wasm, Rust/Skia) ──────────────────────────────┘
    • GPU-accelerated shape rendering
    • Font rasterization (TrueType / OpenType)
    • Tile-based incremental rendering

        ↕ HTTPS / WebSocket
        
Backend (Clojure / Integrant)
│
├── HTTP layer (Yetti/Jetty)
│   • REST-like RPC endpoints (/api/rpc/command/*)
│   • WebSocket endpoint (/ws/notifications)
│   • Static file serving
│
├── RPC Commands (~25 namespaces)
│   • auth, profile, teams, projects, files, comments, media, fonts …
│
├── Worker threads
│   • File GC, thumbnails, telemetry, task queue
│
├── PostgreSQL (JDBC via next.jdbc)
│   • 144 SQL migrations
│   • JSONB for shape data, indices for queries
│
├── Redis
│   • PubSub for real-time message broadcast
│   • Rate-limit counters
│   • Session cache
│
└── Object Storage (S3-compatible or local FS)
    • Media (images, videos)
    • Exported assets
    • Font files
```

---

## 5. Frontend — ClojureScript SPA

### 5.1 State Management (PTK)

**`frontend/src/app/main/store.cljs`** — global Reagent atom holding the entire application state.

PTK (Logos ToolKit) is a custom reactive state library built on top of RxJS. It follows the Elm architecture pattern:

```
Event dispatched  →  event.prepare()  →  event.handle(state)  →  new state atom
                                      ↘  event.effect(state, stream)  →  side-effects (HTTP, WebSocket …)
```

Key namespaces:
- `app.main.data.workspace.*` — ~30 sub-namespaces covering selection, drawing, path editing, layout, assets, libraries, history, clipboard, guides, grids, comments, media, fonts.
- `app.main.data.persistence` — debounced auto-save, change batching, optimistic updates.
- `app.main.data.changes` — assembles `ChangeSet` objects and dispatches them via WebSocket.

**Undo/Redo:**
- Implemented as a double-ended stack in `common/src/app/common/data/undo_stack.cljc`.
- Each undoable action stores a `ChangeSet` and its inverse.
- Groups of low-level changes can be coalesced into a single undo step.

### 5.2 Rendering Pipeline

Two rendering paths coexist:

| Path | When used | Implementation |
|---|---|---|
| Canvas 2D (fallback) | Thumbnails, viewer snapshots | `frontend/src/app/main/render.cljs` |
| WebAssembly / Skia | Interactive workspace canvas | `frontend/src/app/render_wasm/` |

The WASM path follows a serialization protocol:
1. ClojureScript serializes shape trees into typed binary buffers (`serializers.cljs`).
2. Buffers are transferred to the WASM module via `postMessage` (zero-copy SharedArrayBuffer where available).
3. Rust deserializes and hands shapes to Skia draw calls.
4. Output is a GPU frame presented to an `OffscreenCanvas`.

### 5.3 WebAssembly Renderer (`render-wasm/`)

Written in Rust (~78 source files). Key subsystems:

| Module | Algorithm |
|---|---|
| `render/fills.rs` | Gradient fills (linear, radial, angular), image fills with transform |
| `render/strokes.rs` | Dashed / dotted strokes, inner/outer/center alignment, cap styles |
| `render/shadows.rs` | Drop shadow, inner shadow using Skia `imageFilter` |
| `render/filters.rs` | Blur (Gaussian), color-matrix, blend filters |
| `render/fonts.rs` | Load OTF/TTF via Skia's `FontMgr`, glyph rasterization |
| `render/text.rs` | Paragraph layout using Skia `Paragraph` API (ICU line breaking) |
| `render/grid_layout.rs` | Grid track sizing & cell painting |
| `shapes/paths.rs` | Bézier curve → Skia path conversion |
| `shapes/bools.rs` | Boolean path ops (union, difference, intersect, exclude) via Skia |
| `shapes/transform.rs` | 2D affine matrix decompose / recompose |
| `shapes/corners.rs` | Independent corner radius (superellipse approximation) |
| `tiles.rs` | Tile-based dirty-region tracking for incremental re-renders |
| `math/` | 2D vector/matrix arithmetic, AABB, hit-testing |
| `state/shapes_pool.rs` | Arena allocator for shape objects to amortize GC |
| `state/text_editor.rs` | In-canvas rich-text cursor & selection state |

**Render loop:**
```
frame() called by rAF
  → dirty tile set collected
  → for each dirty tile: clip, clear, draw shapes intersecting tile
  → composite tiles → surface → present
```

### 5.4 Worker Thread (`frontend/src/app/worker/`)

A dedicated Web Worker isolates CPU-heavy operations from the UI:

| File | Responsibility |
|---|---|
| `snap.cljs` | Snapping data structure (see §6.6) |
| `thumbnails.cljs` | Off-thread Canvas 2D thumbnail rendering |
| `import.cljs` | `.logos` / `.fig` file ingestion and normalization |
| `selection.cljs` | Hit-testing of shapes against a point/rectangle |

### 5.5 Plugin System (`frontend/src/app/plugins/`)

Plugins run in a sandboxed `<iframe>` with no direct DOM access. Communication uses `postMessage` with a defined API surface documented in `plugins/libs/`. The plugin bridge (`frontend/src/app/plugins.cljs`) routes events, validates permissions (read / content / allow), and serializes shape data through a stable public API.

---

## 6. Common Library — Shared Algorithms

`common/` is a pure ClojureScript/Clojure library used by both browser and server. It contains all core algorithms and data types.

### 6.1 Geometry Engine

**`common/src/app/common/geom/`**

| File | Algorithm |
|---|---|
| `point.cljc` | 2D point arithmetic, dot product, cross product, distance, normalize |
| `matrix.cljc` | 3×3 affine matrix: `translate`, `rotate`, `scale`, `skew`; matrix multiply; inverse; decompose into TRS |
| `rect.cljc` | Axis-aligned bounding box (AABB): union, intersection, contains, expand, center |
| `shapes.cljc` | Derived bounding box from shape polygon corners |
| `shapes/transforms.cljc` | Apply modifier stacks to shapes (translate, resize, rotate, flip); propagation to children |
| `shapes/bounds.cljc` | Tight bounding box including stroke width and shadow spread |
| `shapes/constraints.cljc` | Auto-layout constraint resolution (left/right/center/scale/stretch) |
| `shapes/intersect.cljc` | Shape–shape intersection test (OBB via SAT) |
| `shapes/pixel_precision.cljc` | Round coordinates to pixel grid |
| `modifiers.cljc` | Immutable modifier records: `move`, `resize`, `rotation`, `scale-content`; composition |
| `modif_tree.cljc` | Apply modifier trees to the full shape hierarchy |
| `proportions.cljc` | Maintain aspect ratio on resize |
| `bounds_map.cljc` | Cached bounding-box index for fast frame queries |
| `align.cljc` | Distribute / align shapes (top, bottom, left, right, center, spacing) |
| `grid.cljc` | Grid snap-point generation for column/row/square grids |
| `snap.cljc` | Candidate snap point computation (edge midpoints, centers, guides) |

**Transformation model:**
Every shape stores:
- `transform` — a 3×3 affine matrix (the shape's own rotation/skew, relative to its parent).
- `transform-inverse` — precomputed inverse for hit-testing.
- `selrect` — axis-aligned bounding rectangle in page space.
- `points` — the four corners in page space after applying the full parent chain transform.

When a shape is moved or resized, the algorithm:
1. Builds a modifier (e.g., a `resize` modifier with a pivot point).
2. Propagates the modifier down the shape tree (components, groups, frames).
3. Recomputes `selrect` and `points` after modifier application.
4. Regenerates the `transform-inverse` field.

### 6.2 Layout Engine (Flex & Grid)

**`common/src/app/common/geom/shapes/flex_layout/`**
**`common/src/app/common/geom/shapes/grid_layout/`**

Logos implements its own CSS-equivalent flex/grid layout engine in pure ClojureScript running in the worker thread:

**Flex layout pipeline:**
1. `params.cljc` — parse flex container properties (`direction`, `align-items`, `justify-content`, `gap`, `wrap`).
2. `layout_data.cljc` — compute per-child sizing (min/max, fill, fixed).
3. `positions.cljc` — main/cross axis position assignment.
4. `modifiers.cljc` — emit geometry modifiers for each child.
5. `bounds.cljc` — recompute container bounds after laying out children.
6. `drop_area.cljc` — compute valid drag-drop insertion zones.

**Grid layout pipeline:**
1. `params.cljc` — parse grid template (explicit tracks, `fr` units, named areas).
2. `layout_data.cljc` — resolve track sizes using the CSS Grid sizing algorithm (min-content, max-content, available space distribution).
3. `areas.cljc` — place items using auto-placement and explicit `grid-column`/`grid-row`.
4. `positions.cljc` — compute cell pixel positions.
5. `bounds.cljc` — recompute container bounds.

Both engines run on every shape modification and produce modifier records that are then applied by the geometry engine.

### 6.3 Path & Boolean Geometry

**`common/src/app/common/types/path/`**

Paths are stored as vectors of SVG path segments (`:M`, `:L`, `:C`, `:Z`). Key algorithms:

| File | Algorithm |
|---|---|
| `segment.cljc` | Segment representation, point-on-segment evaluation |
| `impl.cljc` | Path normalization: convert all relative commands to absolute |
| `subpath.cljc` | Split path into closed subpaths |
| `bool.cljc` | Boolean operation dispatcher (union / difference / intersect / exclude) using Sutherland–Hodgman clipping for convex cases and a winding-number algorithm for general cases |
| `shape_to_path.cljc` | Convert rect/ellipse/star shapes to Bézier path representation |
| `helpers.cljc` | Control point flipping, segment splitting at parameter `t` (de Casteljau) |

In the WASM renderer, boolean operations are delegated to **Skia's native path ops** for GPU-side accuracy.

### 6.4 CRDT File Format

**`common/src/app/common/files/changes.cljc`**
**`common/src/app/common/files/changes_builder.cljc`**

The file data model is a set of immutable maps (Clojure persistent data structures). All mutations are expressed as **operation records** (change-sets):

```clojure
{:type :add-obj     :id <uuid> :obj <shape-map>      :page-id <uuid>}
{:type :mod-obj     :id <uuid> :operations [{:type :set :attr :x :val 10}]}
{:type :del-obj     :id <uuid>                        :page-id <uuid>}
{:type :mov-obj     :id <uuid> :parent-id <uuid> :index <int>}
{:type :add-page    :id <uuid> :name <str>}
{:type :del-page    :id <uuid>}
{:type :set-option  :option <kw> :value <any>}
…
```

Change-sets are:
- **Applied locally** via `app.common.files.changes/process-changes` producing a new file state.
- **Serialized** (Transit-JSON) and pushed to the server over WebSocket.
- **Broadcast** by the server to all other connected clients via Redis PubSub.
- **Stored** in the `file_change` PostgreSQL table for audit and merge.

Conflict resolution strategy: **last-writer-wins at the attribute level**. There is no vector clock or OT algorithm — the server applies changes sequentially and broadcasts them; clients apply remote changes immediately after local ones, relying on the attribute-level granularity to keep conflicts rare and naturally resolved.

### 6.5 Change-Set / Undo System

**`common/src/app/common/data/undo_stack.cljc`**

A bounded double-ended stack (default depth: 50 steps). Each entry is a pair `[undo-changes, redo-changes]` where both are `ChangeSet` vectors. The stack supports:
- `push` (clears redo history)
- `undo` (pops from undo-stack, pushes to redo-stack, emits undo-changes)
- `redo` (pops from redo-stack, pushes to undo-stack, emits redo-changes)
- `group` / `transaction` — coalesce multiple push calls into one logical step

### 6.6 Snapping (Range Tree)

**`frontend/src/app/worker/snap.cljs`**
**`frontend/src/app/util/range_tree.cljs`**

The snapping engine maintains a **2D range tree** (one per axis, one per frame):

```
snap-data = {frame-id → {:x <range-tree>, :y <range-tree>}}
```

The range tree is a balanced binary search tree keyed on coordinate values; each node stores a list of snap-point descriptors (shape edge, center, guide line, grid line, etc.). Query complexity: **O(log n + k)** where k is the number of results in range.

On every mouse-move, the engine queries both trees for points within the snap threshold (default 5px) and returns the nearest snap candidates. Grid snap points are computed from frame grid definitions and added transiently to the query.

### 6.7 Schema / Validation

**`common/src/app/common/schema.cljc`**
**`common/src/app/common/types/shape.cljc`**

Logos uses **Malli** (a data-driven schema library) to declaratively describe all types. Schemas are used for:
- Runtime validation of RPC request/response bodies.
- Test data generation (property-based testing with `malli.generator`).
- Auto-generated API documentation.

---

## 7. Backend — Clojure Server

### 7.1 HTTP & RPC Layer

**`backend/src/app/http.clj`** — Yetti (Jetty 12) adapter.

RPC endpoints follow the pattern:
```
POST /api/rpc/command/<name>
```

Each command namespace (`backend/src/app/rpc/commands/*.clj`) defines:
```clojure
(sm/defn command-name
  [{:keys [pool redis s3 ...]} params]
  ...)
```

Cross-cutting concerns are applied as middleware:
| Middleware | File | Function |
|---|---|---|
| Rate limiting | `rpc/rlimit.clj` | Token-bucket per user/IP using Redis |
| Concurrency limiting | `rpc/climit.clj` | Semaphore per command |
| Retry | `rpc/retry.clj` | Exponential backoff on transient DB errors |
| Permissions | `rpc/permissions.clj` | Role-based: owner / admin / editor / viewer |
| Quotes | `rpc/quotes.clj` | Storage quotas per team |

### 7.2 Database Layer

**PostgreSQL** via `next.jdbc`. The schema has evolved through **144 SQL migrations** (numbered `0001` → `0144`). Notable tables:

| Table | Purpose |
|---|---|
| `profile` | User accounts, hashed passwords, OAuth tokens |
| `team` | Organizations (personal + shared) |
| `project` | Containers for files within a team |
| `file` | File metadata, current `revn` (revision counter) |
| `file_data` | JSONB blob of the page/shape tree (large files fragmented) |
| `file_change` | Append-only change log (CRDT ops, base64-encoded Transit) |
| `file_media_object` | Binary media references: images, fonts, videos |
| `file_thumbnail` | Pre-rendered thumbnail blobs |
| `team_font_variant` | Uploaded font files per team |
| `http_session` / `http_session_v2` | Session tokens |
| `token` | Verify / access / invitation tokens |
| `audit_log` | Immutable audit trail of all mutations |

**Query pattern:** All queries use parameterized `next.jdbc` calls. A thin SQL builder in `backend/src/app/db/sql.clj` generates `WHERE` clauses safely.

### 7.3 Real-Time Collaboration (WebSocket + Redis PubSub)

**`backend/src/app/ws.clj`** (WebSocket handler)
**`backend/src/app/msgbus.clj`** (Redis PubSub bus)

Protocol:
1. Client opens `wss://<host>/ws/notifications` and sends `{:type :subscribe :file-id <uuid>}`.
2. Server registers the connection in a per-file topic on Redis.
3. When any client sends change-sets, the server:
   a. Persists them in `file_change`.
   b. Publishes to Redis topic `file:<uuid>`.
4. All subscribers (other clients on the same file) receive and apply the changes.
5. On disconnect, the server publishes a presence-leave event.

This is a **broadcast-to-all** model without differential/delta filtering per client.

### 7.4 File Storage

**`backend/src/app/storage.clj`**

Pluggable storage backend (configured via environment):
- `s3` — Amazon S3 / MinIO (production default)
- `fs` — local filesystem (development default)
- `db` — store blobs directly in PostgreSQL BYTEA (not recommended)

Media objects are referenced by a content-addressed hash (`sha256`). De-duplication happens at upload time.

### 7.5 Binary File Format (BinFile v1-v3)

**`backend/src/app/binfile/`**

Export/import of complete files as a single `.logos` archive. Three format versions exist:

| Version | Format | Notes |
|---|---|---|
| v1 | Zip (Transit-JSON + media blobs) | Legacy, still importable |
| v2 | Zip (Transit-JSON with shared media refs) | De-duplicated media |
| v3 | Zip (Transit-JSON + Fressian binary pages) | Smaller, faster deserialization |

`migrations.clj` upgrades v1/v2 to v3 on import.

### 7.6 Background Task System

**`backend/src/app/worker.clj`** — a pool of Clojure threads polling a `task_runner` table.

| Task | File | Trigger |
|---|---|---|
| `file-gc` | `tasks/file_gc.clj` | Nightly — remove orphaned objects, compress file data |
| `file-gc-scheduler` | `tasks/file_gc_scheduler.clj` | Schedule per-file GC based on change-count |
| `objects-gc` | `tasks/objects_gc.clj` | Remove unreferenced media/storage objects |
| `tasks-gc` | `tasks/tasks_gc.clj` | Prune old completed task entries |
| `offload-file-data` | `tasks/offload_file_data.clj` | Move large JSONB to `file_data` table |
| `telemetry` | `tasks/telemetry.clj` | Aggregate & report usage metrics |

### 7.7 Authentication & Session

**`backend/src/app/auth/`**

Strategies supported:
- **Email + password** — bcrypt hashed (cost 12).
- **LDAP** — `backend/src/app/rpc/commands/ldap.clj`.
- **OAuth2** — Google, GitHub, GitLab, etc. via `backend/src/app/auth/`.
- **SSO providers** — `0142-add-sso-provider-table.sql` introduces a per-team SSO config.

Session tokens are opaque random UUIDs stored in `http_session_v2` with expiry timestamps. A cookie carries the session ID; the server validates and refreshes on each request.

### 7.8 Quota & Rate Limiting

- **Rate limiting** (`rlimit.clj`): Redis-backed token bucket. Configurable per command, per user, per IP.
- **Concurrency limiting** (`climit.clj`): JVM semaphore per command to prevent overload.
- **Storage quotas** (`quotes.clj`): Checked on media upload and file save against per-team limits.

---

## 8. Render-WASM — Rust / Skia Renderer

**`render-wasm/`** (~78 `.rs` files)

The renderer is compiled to WebAssembly using `wasm-bindgen` and `emscripten`. It uses the **Skia** C++ graphics library via Rust FFI bindings.

### Rendering Architecture

```
ClojureScript (main thread)
  │
  │  Typed binary buffer (shape descriptors)
  ▼
WASM module entry point  (wasm.rs / main.rs)
  │
  │  Deserialize into Rust shape structs
  ▼
State Pool (state/shapes_pool.rs)
  │  arena-allocated, avoids GC pauses
  │
  ▼
Render loop (render.rs)
  ├── Tile dirty tracking (tiles.rs)
  │
  ├── Per-shape draw calls
  │   ├── fills.rs        → Skia Paint (solid, gradient, pattern)
  │   ├── strokes.rs      → Skia PathEffect (dash/dot)
  │   ├── shadows.rs      → Skia ImageFilter
  │   ├── filters.rs      → Skia ColorFilter / BlurFilter
  │   ├── fonts.rs        → Skia FontMgr + TypeFace cache
  │   └── text.rs         → Skia Paragraph (line breaks, direction)
  │
  └── GPU surface present  (surfaces.rs)
```

### Key Algorithms in render-wasm

**Path → Skia conversion** (`shapes/paths.rs`):
Converts SVG cubic Bézier segments to `skia_safe::Path` objects. Handles degenerate cases (zero-length segments, collinear control points).

**Boolean path operations** (`shapes/bools.rs`):
Delegates to `skia_safe::PathOp` with operations: Union, Difference, Intersect, XOR (Exclude). Results are cached per shape to avoid re-computation on pan/zoom.

**Independent corner radius** (`shapes/corners.rs`):
Approximates a superellipse (squircle) using two cubic Bézier segments per corner. The magic constant `0.5523` (≈ `4/3 * tan(π/8)`) is used for quarter-circle approximation.

**Text layout** (`shapes/text.rs`):
Uses ICU-based Unicode line breaking (built into Skia's Paragraph module). Supports:
- LTR / RTL / Bidi mixed text.
- Multiple `TextStyle` spans per paragraph.
- Vertical alignment and overflow clipping.

**Tile-based rendering** (`tiles.rs`):
The canvas is divided into fixed-size tiles (default 512×512 CSS pixels). A dirty set tracks which tiles need repainting. On each frame, only dirty tiles are redrawn. This reduces GPU work by 80-95% for typical editing operations.

**Font management** (`shapes/fonts.rs`, `render/fonts.rs`):
- Fonts are fetched as ArrayBuffer from ClojureScript and registered into a Skia `FontMgr` instance.
- A per-family, per-weight/style TypeFace cache avoids re-parsing TTF data.
- Fallback chain is configured for system fonts (Noto for CJK, Arabic, Hebrew, etc.).

---

## 9. MCP Tool Server

**`mcp/packages/server/`** (TypeScript)

A Model Context Protocol server exposing Logos design tools to AI assistants (Claude, GPT, etc.):

| Tool class | File | Purpose |
|---|---|---|
| `LogosMcpServer` | `LogosMcpServer.ts` | Server entry, tool registry |
| `LogosApiInfoTool` | `tools/LogosApiInfoTool.ts` | Describe available Plugin APIs |
| `HighLevelOverviewTool` | `tools/HighLevelOverviewTool.ts` | Summarize the current design file |
| `ExecuteCodeTool` | `tools/ExecuteCodeTool.ts` | Run plugin JS code inside Logos |
| `ExportShapeTool` | `tools/ExportShapeTool.ts` | Export a shape as PNG/SVG |
| `ImportImageTool` | `tools/ImportImageTool.ts` | Insert an image into a frame |
| `PluginBridge` | `PluginBridge.ts` | WebSocket tunnel to Logos plugin runtime |
| `LogosUtils` | `mcp/packages/plugin/src/LogosUtils.ts` | Plugin-side helpers |

**Transport:** stdio (for Claude Desktop) or SSE HTTP.

---

## 10. Exporter Service

**`exporter/`** (TypeScript + Puppeteer)

Headless Chromium instance that:
1. Opens the Logos viewer URL for the target file + page.
2. Waits for WASM rendering to complete.
3. Captures the canvas as PNG (via `page.screenshot`) or generates PDF.
4. Returns the result to the backend caller.

SVG export is handled differently: the backend serializes shapes directly from the file data model to SVG strings without involving the browser.

---

## 11. Infrastructure & Deployment

**Docker Compose (development):**
```
logos-frontend   → shadow-cljs dev server :8888
logos-backend    → Clojure Integrant :3449
logos-postgres   → PostgreSQL 15
logos-redis      → Redis 7
logos-exporter   → Node.js Puppeteer service
logos-mcp        → MCP TypeScript server
```

**Helm (production):**
`deploy/helm/` contains charts for Kubernetes. Production recommends:
- Multiple backend replicas behind an L7 load balancer (sticky sessions for WebSocket).
- External managed PostgreSQL (RDS / CloudSQL).
- External Redis (ElastiCache / Memorystore).
- S3-compatible object storage.
- CDN in front of static assets.

**Environment configuration:**
All runtime configuration flows through environment variables (`LOGOS_*` prefix, e.g. `LOGOS_DATABASE_URI`, `LOGOS_REDIS_URI`, `LOGOS_STORAGE_BACKEND`).

---

## 12. Current State Assessment

### 12.1 Strengths

1. **Pure functional core** — The common library is entirely pure functions on immutable data. Testing, reasoning, and refactoring are easier than in mutable OOP equivalents.
2. **Open file format** — SVG-based file format prevents vendor lock-in. `.logos` archives are inspectable and portable.
3. **Real-time collaboration** — The Redis PubSub + CRDT change-set model works well at moderate concurrency.
4. **GPU renderer** — The WASM/Skia renderer provides high fidelity output and is significantly faster than Canvas 2D for complex scenes.
5. **Plugin ecosystem** — The isolated sandboxed plugin model is secure and extensible.
6. **Comprehensive layout support** — Full flex and grid layout engines with CSS-equivalent semantics.
7. **Snapping performance** — O(log n) range tree snapping scales to thousands of objects.
8. **Test coverage** — The common library has property-based tests via Malli generators.

### 12.2 Bottlenecks & Risks

| # | Area | Problem | Severity |
|---|---|---|---|
| B1 | CRDT collaboration | Last-writer-wins with no vector clocks leads to silent data loss on concurrent edits of the same attribute. | HIGH |
| B2 | File data size | The entire file data tree is stored as a single JSONB blob, causing full reads/writes on every save even for small changes. | HIGH |
| B3 | WebSocket broadcast | Every client receives every change. No client-side delta filtering; large files with many collaborators flood the wire. | MEDIUM |
| B4 | WASM serialization | Every shape must be serialized to binary on the main JS thread before being passed to WASM, which blocks the event loop on large frames. | MEDIUM |
| B5 | Worker thread post-message | No shared memory between main and worker; large snapshots are copied, increasing GC pressure and latency. | MEDIUM |
| B6 | Flex/grid layout re-runs on every modifier | Full layout pass triggered even for non-layout changes (e.g., colour changes do not affect geometry). | MEDIUM |
| B7 | Font loading | Fonts are loaded lazily and synchronously block text rendering until available. No preloading heuristic. | LOW-MEDIUM |
| B8 | Thumbnail generation | Thumbnails re-generated on every change, even for off-screen frames. No debounce or dirty-bit per frame. | LOW |
| B9 | Plugin API surface undocumented | Many plugin capabilities are inferred from source code rather than a formal spec. | LOW |
| B10 | Environment variable naming | All env vars still use `LOGOS_` prefix despite branding as Logos. | LOW |
| B11 | Build tooling fragmentation | Three separate package managers (Clojure `deps.edn`, `pnpm`, `Cargo) require different workflows per module. | LOW |
| B12 | No caching layer on RPC reads | Database queries for hot paths (get-file, get-profile) run on every request with no memoization or HTTP cache headers. | MEDIUM |

---

## 13. Phased Optimization Plan

Each phase is designed to be independent and deliverable without blocking other phases.

---

### Phase 0 — Foundation Hygiene (Weeks 1-2)

Goal: Zero technical debt blockers, correct naming, reproducible builds.

| ID | Task | Owner Area | Effort |
|---|---|---|---|
| P0.1 | Rename all `LOGOS_*` environment variables to `LOGOS_*` with backward-compatible aliases in `config.clj`. Update `docker-compose.yaml`, Helm values, and docs. | Backend/Infra | 1 day |
| P0.2 | Rename internal JS globals (`logosVersion` → `logosVersion`, etc.) in `index.html` and all ClojureScript callers. Update the global `js/logosVersion` reads to `js/logosVersion`. | Frontend | 0.5 day |
| P0.3 | Add `target/` and `node_modules/` to all sub-project `.gitignore` files to prevent accidental staging. | All | 0.5 day |
| P0.4 | Pin all dependency versions: lock Clojure deps via `deps-lock.edn`, upgrade `pnpm` lockfiles to v9. | All | 1 day |
| P0.5 | Add a top-level `Makefile` / `manage.sh` command: `make dev` starts all services in correct order. | Infra | 1 day |
| P0.6 | Set up CI pipeline: lint (clj-kondo, eslint), unit tests, WASM compile check on every PR. | CI | 2 days |

---

### Phase 1 — Performance Hotspots (Weeks 3-6)

Goal: Reduce UI latency and memory usage on large files.

#### P1.1 — WASM Serialization off Main Thread

**Problem (B4):** Shape tree serialization to binary runs on the main JS thread, causing jank.  
**Solution:** Move serialization to the Web Worker. The worker already holds the canonical shape state; have it produce the binary buffer and transfer it to the WASM module via `postMessage` with `Transferable`.

```
Before: main thread → serialize → postMessage(buffer, [buffer]) → WASM
After:  worker thread → serialize → postMessage(buffer, [buffer]) → WASM
```

Files to change:
- `frontend/src/app/render_wasm/serializers.cljs` — make callable from worker context.
- `frontend/src/app/worker/index.cljs` — add new message handler.
- `frontend/src/app/render_wasm/wasm.cljs` — receive buffer from worker instead of self-generating.

Expected gain: eliminate main-thread serialization stalls (estimated 30-80ms per large frame).

#### P1.2 — SharedArrayBuffer for WASM ↔ Worker

**Problem (B5):** Large state snapshots are copied across threads.  
**Solution:** Where browser security headers allow (COOP + COEP), allocate the shape buffer in a `SharedArrayBuffer`. Worker writes, WASM reads — zero copy.

Prerequisite: Serve app with `Cross-Origin-Opener-Policy: same-origin` and `Cross-Origin-Embedder-Policy: require-corp`. This is already partially in place for WebGL; extend to the shape buffer.

#### P1.3 — Incremental Layout (Skip on Non-Geometry Changes)

**Problem (B6):** Full flex/grid layout re-runs on every change-set even when geometry is unchanged.  
**Solution:** Tag each change-set with a `geometry-affecting?` boolean. In `frontend/src/app/main/data/workspace/modifiers.cljs`, skip layout computation when the flag is false.

Geometry-affecting changes: resize, move, add-child, remove-child, change layout prop.  
Non-geometry changes: fill colour, stroke colour, opacity, shadow, blur.

Expected gain: 40-60% reduction in layout computation during style-only edits.

#### P1.4 — Thumbnail Debounce with Per-Frame Dirty Bits

**Problem (B8):** Thumbnails regenerated on every change-set application.  
**Solution:**
1. Track a `dirty-thumbnail?` flag per frame UUID.
2. Set flag only when geometry changes affect the frame.
3. Debounce thumbnail generation 2s after last change.
4. Cancel pending generation if a new change arrives.

Files: `frontend/src/app/worker/thumbnails.cljs`, `frontend/src/app/main/data/workspace/thumbnails.cljs`.

#### P1.5 — RPC Read-Through Cache

**Problem (B12):** `get-file`, `get-profile`, `get-team` queries are high-frequency reads with no caching.  
**Solution:** Add a Redis read-through cache layer in the RPC command helpers.

```clojure
(defn cache-get [redis key ttl-sec fetch-fn]
  (or (redis/get redis key)
      (let [val (fetch-fn)]
        (redis/setex redis key ttl-sec val)
        val)))
```

Invalidate cache on every write to the corresponding entity. TTL: 30s for file metadata, 5m for profile, 10m for team.

Expected gain: 60-80% reduction in DB reads for read-heavy workloads.

---

### Phase 2 — Scalability (Weeks 7-12)

Goal: Support 10× more concurrent collaborators without degradation.

#### P2.1 — Delta-Compressed WebSocket Messages

**Problem (B3):** Every client receives full change-sets regardless of which part of the file they are viewing.  
**Solution:** Add a `page-id` field to the collaboration session registration. The server only forwards changes whose `page-id` matches the subscriber's current page, plus page-agnostic changes (file metadata).

This is a **selective fan-out** model:
```
Before: publish to all file subscribers
After:  publish to subscribers on same page + all for cross-page changes
```

#### P2.2 — File Data Fragmentation (Streaming Writes)

**Problem (B2):** The `file_data` JSONB blob is fully read and written on every save.  
**Solution:** The `offload_file_data` task already moves data to a separate `file_data` table. Extend this with **page-level fragmentation**: store each page's shape tree in a separate row, identified by `(file_id, page_id)`. Saves to a single page only read/write that page's row.

Migration path:
1. New column `file_data_page` table (already partially there as `file-data-fragment`).
2. Migration to split existing large files on first access.
3. Update `backend/src/app/rpc/commands/files.clj` to load only the required page on read.

#### P2.3 — Operational Transform for Same-Attribute Conflicts

**Problem (B1):** Last-writer-wins silently drops concurrent attribute edits.  
**Solution:** Implement **server-side sequencing with client-side rebase**:

1. Server assigns a monotonically increasing `revn` to each change-set.
2. Each change-set includes the `revn` it was based on.
3. If server receives a change-set based on an older `revn`, it applies a simple commutative OT transform (for `set-attr` ops: resolve to the higher-revn value; for `move-obj` ops: compose the translations).
4. Rebased change-set is broadcast with the server's `revn`.

This requires:
- New field in `file_change` table: `base_revn`.
- New function in `common/files/changes.cljc`: `rebase-changes`.
- Updated WebSocket handler to perform rebase before persistence.

#### P2.4 — Horizontal Scaling of WebSocket Sessions

Currently WebSocket sessions are in-process (JVM memory). With multiple backend replicas, a client on replica A cannot receive messages from a write on replica B.

**Solution:** Make all session state go through Redis:
1. Store `{session-id → {file-id, page-id, node-id}}` in Redis.
2. Publish to a Redis channel; all replicas subscribe.
3. Each replica forwards messages only to its locally connected clients.

The `msgbus.clj` module already uses Redis PubSub; extend it to include per-page routing keys.

#### P2.5 — PostgreSQL Connection Pool Tuning

Current pool is configured with default settings. Recommended changes:
- Set `maximumPoolSize = CPU_COUNT × 2 + 1` (HikariCP formula).
- Enable `preparedStatementCaching = true`.
- Add `statement_timeout = 30s` to prevent long-running queries.
- Add `pg_stat_statements` monitoring.

---

### Phase 3 — Developer Experience (Weeks 13-18)

Goal: Faster iteration cycles, lower onboarding friction.

#### P3.1 — Unified CLI

Replace the fragmented `manage.sh` / separate `pnpm` / `clj` / `cargo` commands with a single top-level `logos` CLI (written as a shell script or a Babashka script):

```bash
logos dev          # start all services
logos dev frontend # start only frontend
logos dev backend  # start only backend
logos test all     # run all test suites
logos build wasm   # compile render-wasm
logos lint         # run all linters
logos migrate      # run DB migrations
```

#### P3.2 — clj-kondo Lint Integration

Add `clj-kondo` with custom hooks for:
- PTK event shape validation (`handle` must return a map, `effect` must return a stream).
- Malli schema references (warn on undefined schema keys).
- Unused namespace imports.

Integrate with CI (already planned in P0.6).

#### P3.3 — Performance Tracing Dashboard

Add OpenTelemetry instrumentation to:
- Backend RPC commands (trace duration, DB query count).
- Frontend render loop (frame time, shape count, dirty tile count).
- WASM (Skia draw call count, GPU memory usage via `render/performance.rs`).

Expose traces to Jaeger or Tempo for local development; provide a pre-built Grafana dashboard in `docker/devenv/`.

#### P3.4 — Property-Based Test Coverage for Geometry

Extend existing Malli-generator tests to cover:
- Modifier composition commutativity (translate then resize = resize then translate?).
- Layout engine round-trips (apply layout → read positions → apply inverse modifiers → verify original).
- Boolean path ops idempotency (A ∪ A = A, A ∩ A = A).

Target: ≥ 80% branch coverage on `common/geom/`.

#### P3.5 — Plugin API Formal Specification

Generate a TypeScript declaration file from the Clojure plugin type definitions. Use `malli-to-ts` or a custom Malli → JSON Schema → TypeScript pipeline.

Publish `@logos/plugin-types` as a versioned package consumers can install for autocomplete.

---

### Phase 4 — Feature Quality (Weeks 19-26)

Goal: Elevate feature completeness and correctness.

#### P4.1 — Variable Fonts Support

Current font rendering loads a single OTF/TTF file per family/weight. Variable fonts (`.woff2` with `fvar` axes) are not supported.

**Plan:**
1. In `render-wasm`: upgrade Skia binding to expose `SkFontArguments` for axis values.
2. In `frontend/src/app/main/ui/workspace/sidebar/options/menus/text.cljs`: add UI controls for font axes (weight, width, slant, optical size).
3. In common types: extend `typography.cljc` with `font-variation-settings` map.

#### P4.2 — Vector Network (Non-Tree Path Topology)

Currently paths must be simple open/closed chains. external design tool-style **vector networks** allow multiple segments sharing anchor points without a tree topology.

**Plan:**
1. Extend `common/types/path/segment.cljc` to support a graph structure (adjacency list of anchor → [segment]).
2. Update `shapes/paths.rs` in WASM to handle fan-in/fan-out anchors.
3. Update the path editor in `frontend/src/app/main/data/workspace/path/` to support multi-segment selection.

This is the highest complexity feature in Phase 4.

#### P4.3 — Offline / Local-First Mode

Allow users to work offline with changes re-synced on reconnect.

**Plan:**
1. Persist change-sets to IndexedDB when WebSocket is disconnected.
2. On reconnect, send queued change-sets in order with base `revn`.
3. Use the OT rebase mechanism from P2.3 to resolve conflicts with server.

Requires P2.3 as a prerequisite.

#### P4.4 — AI Design Assistant Integration

Leverage the MCP server (§9) to provide an in-app AI assistant:
1. Add an AI panel in the right sidebar.
2. Connect to the MCP server via the plugin bridge.
3. Allow AI to: generate components, suggest layouts, apply colour palettes, resize for multiple breakpoints.

The MCP TypeScript infrastructure is already in place; P4.4 is the UX integration layer.

#### P4.5 — Accessibility Audit & Compliance

- Run axe-core against all UI components.
- Add ARIA labels to all canvas-overlaid controls.
- Ensure keyboard-only workflow for: selection, drawing, text editing, layer panel navigation.
- Target WCAG 2.1 AA compliance.

---

## 14. Key File Reference Map

| Concern | File |
|---|---|
| App entry (Frontend) | `frontend/src/app/main.cljs` |
| App entry (Backend) | `backend/src/app/main.clj` |
| Global state atom | `frontend/src/app/main/store.cljs` |
| Change application | `common/src/app/common/files/changes.cljc` |
| Shape type definition | `common/src/app/common/types/shape.cljc` |
| Affine matrix | `common/src/app/common/geom/matrix.cljc` |
| Modifier system | `common/src/app/common/geom/modifiers.cljc` |
| Modifier propagation | `common/src/app/common/geom/modif_tree.cljc` |
| Flex layout | `common/src/app/common/geom/shapes/flex_layout/` |
| Grid layout | `common/src/app/common/geom/shapes/grid_layout/` |
| Path boolean ops (CLJS) | `common/src/app/common/types/path/bool.cljc` |
| Path boolean ops (WASM) | `render-wasm/src/shapes/bools.rs` |
| Snap range tree | `frontend/src/app/util/range_tree.cljs` |
| Snap engine | `frontend/src/app/worker/snap.cljs` |
| WASM render loop | `render-wasm/src/render.rs` |
| WASM tile system | `render-wasm/src/tiles.rs` |
| WASM shape pool | `render-wasm/src/state/shapes_pool.rs` |
| WASM font manager | `render-wasm/src/shapes/fonts.rs` |
| WASM text layout | `render-wasm/src/shapes/text.rs` |
| CLJS → WASM bridge | `frontend/src/app/render_wasm/api.cljs` |
| HTTP server | `backend/src/app/http.clj` |
| RPC middleware | `backend/src/app/rpc/` |
| WebSocket handler | `backend/src/app/ws.clj` |
| Redis PubSub | `backend/src/app/msgbus.clj` |
| Task scheduler | `backend/src/app/worker.clj` |
| File GC | `backend/src/app/tasks/file_gc.clj` |
| BinFile export | `backend/src/app/binfile/v3.clj` |
| Undo stack | `common/src/app/common/data/undo_stack.cljc` |
| Malli schemas | `common/src/app/common/schema.cljc` |
| Plugin bridge (backend) | `frontend/src/app/plugins.cljs` |
| Plugin bridge (MCP) | `mcp/packages/server/src/PluginBridge.ts` |
| MCP server | `mcp/packages/server/src/LogosMcpServer.ts` |
| Exporter | `exporter/src/` |

---

---

# Part II — CTO Strategic Development Plan

> **"Our foundation is already stronger than any rival in terms of openness and web-standards alignment. By systematically layering performance, collaboration, and AI, Logos will become the obvious choice for design teams who value control, privacy, and the open web."**

---

## 15. Immediate Action: Complete Rebranding to Logos (v0.1.0)

Phase 0 of the technical analysis covers internal hygiene (P0.1–P0.6). The following extended actions ensure the product is **born** as Logos before any feature work begins.

| # | Action | Implementation Status |
|---|--------|-----------------------|
| 15.1 | **Environment variables** — Rename `LOGOS_*` → `LOGOS_*` across all config, Docker Compose, and Helm charts. Backward-compatible `LOGOS_*` aliases preserved in `read-config` (config.clj) for one full release cycle. | ✅ **Done** — `docker/images/docker-compose.yaml`, `backend/src/app/config.clj` updated |
| 15.2 | **Global JS namespace** — Replace `globalThis.logosVersion`, `logosBuildDate`, `logosWorkerURI`, `logosFlags`, `logosPublicURI` etc. in all HTML templates and ClojureScript config readers. | ✅ **Done** — `index.html`, `index.mustache`, `rasterizer.mustache`, `render.mustache`, `config.cljs`, `notifications.cljs` updated |
| 15.3 | **Repository docs** — Update `CONTRIBUTING.md` help-center links. `README.md` retains "formerly Logos" notice. | ✅ **Done** |
| 15.4 | **Build identifiers** — SMTP from/reply-to names, telemetry URI, default database names updated to Logos. | ✅ **Done** — `backend/src/app/config.clj` defaults updated |
| 15.5 | **Database & Redis keys** — No data migration needed; key prefixes are internal. A `branding` metadata entry will be written in an upcoming migration. | 📋 Scheduled for next migration slot |
| 15.6 | **MCP TypeScript files** — Renamed: `LogosMcpServer` → `LogosMcpServer`, `LogosApiInfoTool` → `LogosApiInfoTool`, `LogosUtils` → `LogosUtils`. | ✅ **Done** (previous session) |

**Deliverable:** `v0.1.0` — a clean `main` branch that boots, serves, and self-identifies as **Logos**.

---

## 16. Competitive Landscape Analysis

To build a tool that systematically wins, we must dissect the incumbents. Below is a competitive matrix across the four main players.

### 16.1 Competitor Strengths We Must Match or Surpass

| Competitor | Key Strengths |
|------------|---------------|
| **external design tool** | Vector networks, advanced auto-layout (v4.0), component variants + properties, Dev Mode (CSS/React extraction), real-time collaboration with hybrid OT/CRDT, rich plugin ecosystem, browser-only (zero install), robust prototyping, AI auto-layout suggestions. |
| **Canva** | Exceptional ease of use, massive template library, brand kits, AI-powered generation (Magic Design), collaborative editing for non-designers, strong export/print, mobile app. |
| **Sketch** | Mature macOS-native vector editor, enormous plugin ecosystem, symbol/component system, offline-first, excellent performance with large documents. |
| **Adobe XD** | (maintenance mode) Smart animate, voice prototyping, coediting. Mostly relevant as a migration source. |

**Common denominators across all competitors:** Pixel-perfect rendering, intuitive multi-select, seamless collaboration, rich plugin layer, short learning curve.

### 16.2 Competitor Weaknesses We Can Exploit

| Competitor | Exploitable Weaknesses |
|------------|------------------------|
| **external design tool** | Proprietary format (vendor lock-in), no offline mode, limited illustration/drawing, web-only means perf suffers on very large canvases, prohibitive pricing tiers, no self-hosted option, limited production-code export. |
| **Canva** | Weak vector editing (no Bézier, no boolean ops), limited design-system management, no dev handoff, no offline mode, generic template aesthetics. |
| **Sketch** | macOS-only, no real-time multiplayer built-in (requires Sketch Mirror/Cloud), infrequent release cadence. |

### 16.3 Logos Differentiation Strategy

Logos already has a **structurally superior foundation**:

| Advantage | Why It Wins |
|-----------|-------------|
| **SVG-native format** | Every design is readable, editable, and portable without Logos. No lock-in. |
| **CSS-aware layouts** | Flex/grid that produces real CSS — directly usable by developers. No translation layer. |
| **Self-hosted & private** | The only full-featured design tool teams can run on their own infrastructure. Directly addresses enterprise procurement objections against external design tool. |
| **Open source** | Community-driven improvements, enterprise auditability, no third-party data exposure. |
| **AI with data sovereignty** | AI features that run client-side (WebGPU/WASM) or via the MCP server — user choice, no cloud dependency. |

Strategic priorities in order:
1. **Close the performance gap** (Phases 1-2) — must feel as fast as external design tool.
2. **Bulletproof collaboration** (Phase 2) — must be as reliable as external design tool's OT.
3. **Complete core feature parity** (Phase 4) — vector networks, offline, component variants.
4. **Surpass on AI + openness** (Phase 4-5) — privacy-preserving AI is our biggest differentiator.

---

## 17. Extended Strategic Roadmap

The phased plan from §13 is retained and extended with competitive targeting, performance benchmarks, and reference-backed technical decisions.

### Phase 0: Foundation Hygiene & Rebranding (Weeks 1-2) ✅

**Status:** Executing. Rename PRs merged. CI pipeline setup in progress.

**Architecture principle applied:** *Clean Architecture* — branding is centralized in configuration. Domain logic (`common/`) is untouched and name-agnostic.

---

### Phase 1: Performance Hotspots (Weeks 3-6) — "Solid 60 fps"

Competitive parity requires a **solid 60 fps editor** on 10k-shape files with near-zero input latency. external design tool maintains this through proprietary C++ rendering; we achieve it through our Rust/Skia WASM pipeline.

| ID | Task | Technical Method | Reference |
|----|------|-----------------|-----------|
| P1.1 | WASM serialization off main thread | Worker owns serialization buffers; transfer ownership not copy | *Data-Oriented Design* — buffer ownership |
| P1.2 | SharedArrayBuffer for zero-copy | COOP/COEP headers + SAB fallback to `Transferable` | *High Performance Browser Networking* |
| P1.3 | Incremental layout (dirty flag) | Tag change-sets `:geometry-affects? bool`; memoize layout results | *Game Programming Patterns* — Dirty Flag |
| P1.4 | Thumbnail debounce + dirty bits | Per-frame dirty flag, 2s debounce, cancel on new change | — |
| P1.5 | RPC read-through cache | Redis cache-aside with 30s/5m TTL + write-invalidation | *Designing Data-Intensive Applications* ch.11 |
| P1.6 | Tile renderer improvements | Increase tile size to 1024×1024 on 4K; rAF budget checker (2ms); occlusion cull off-screen tiles | *Real-Time Rendering* — culling |
| P1.7 | Memory profiling regression suite | `memlab`-style heap snapshots in CI; target ≤ 200 MB for 100-artboard doc | Chromium tracing |

**Phase 1 Performance Targets:**

| Metric | Target | Measurement |
|--------|--------|-------------|
| Canvas frame time (drag) | ≤ 4 ms p99 | OpenTelemetry + Chrome tracing |
| File open (1,000 shapes) | ≤ 1.5 s | Custom profiler |
| Memory peak (50 MB import) | ≤ 300 MB | `performance.measureUserAgentSpecificMemory()` |
| WASM binary size | ≤ 2 MB gzip | CI artifact check |

---

### Phase 2: Scalability & Robust Collaboration (Weeks 7-12) — "Enterprise-Grade Multiplayer"

external design tool's real-time collaboration is the benchmark. We implement a **CRDT + OT hybrid** that guarantees eventual consistency without silent data loss. This directly addresses our current B1 (highest severity) bottleneck.

| ID | Task | Competitive Reason | Technical Approach |
|----|------|--------------------|--------------------|
| P2.1 | Delta-compressed WebSocket (page-scoped fan-out) | 70% bandwidth reduction | Only forward changes whose `page-id` matches subscriber's current page |
| P2.2 | Page-level file fragmentation | Match external design tool's implicit per-page loading; drastically cut initial load for large files | Extend `file_data` table to per-page rows `(file_id, page_id)` |
| P2.3 | **OT attribute-level conflict resolution** | Replace last-writer-wins; end silent data loss | Server assigns `revn`; clients send `base_revn`; server rebases using commutative transforms for `set-attr` and `move` ops |
| P2.4 | Horizontal WebSocket scaling via Redis | Multi-server deployments | Extend `msgbus.clj` with per-page Redis routing keys |
| P2.5 | **CRDT shape PoC** (research branch) | Long-term: true peer-to-peer offline | Yjs/Automerge-like representation; guided by *Purely Functional Data Structures* |
| P2.6 | Automated load testing | Validate at scale | 200 simulated collaborators; p95 merge latency ≤ 100 ms using `k6` |

**Reference:** *Designing Data-Intensive Applications*, Chapter 5 — replication and conflict resolution. The OT rebase model (P2.3) follows the operational transformation approach for commutative set operations.

**Competitive advantage:** external design tool has no self-hosted option. Our scalable self-hosted collab directly wins enterprise procurement decisions.

---

### Phase 3: Developer Experience & Ecosystem (Weeks 13-18)

| ID | Task | Outcome |
|----|------|---------|
| P3.1 | Unified `logos` CLI (Babashka) | `logos dev`, `logos test`, `logos build wasm`, `logos migrate` |
| P3.2 | Full clj-kondo integration | Architectural boundary enforcement; pre-commit hooks |
| P3.3 | OpenTelemetry + performance dashboards | Developer-visible frame graphs, DB query times, GPU metrics (USE method from *Systems Performance* by Brendan Gregg) |
| P3.4 | Property-based tests ≥ 80% on geometry | Regression safety for layout engine (critical for external design tool parity) |
| P3.5 | `@logos/plugin-types` npm package | Auto-generated from Malli schemas → TypeScript declarations; matches external design tool's typed plugin API |
| P3.6 | **Contributor Lab** | Docker Compose environment with sample plugins, MCP integration, and AI agent playground |

---

### Phase 4: Feature Quality & Competitive Parity (Weeks 19-32)

Systematic closure of the feature gap. Each item targets a specific competitor weakness or Logos-exclusive advantage.

| # | Feature | Competitor Target | Implementation |
|---|---------|------------------|----------------|
| **4.1** | **Variable Fonts** | external design tool + Sketch | Extend Skia bindings to `SkFontArguments`; UI for axis values (weight, width, slant, optical size); extend `typography.cljc` with `font-variation-settings` map. Ref: *Real-Time Rendering* — text chapter |
| **4.2** | **Vector Networks** | external design tool's core differentiator | Non-tree path topology using **half-edge data structure**. Extend `segment.cljc` to adjacency-list graph. Update `shapes/paths.rs` for fan-in/fan-out anchors. Ref: *Geometric Tools for Computer Graphics* |
| **4.3** | **Auto-Layout Enhancements** | external design tool Auto-Layout v4.0 | `min/max` child constraints, `stretch` with wrapping, negative gap, CSS Grid Level 2 subgrid. Our flex/grid engines have the foundation. Ref: *Discrete Mathematics* — constraint solving |
| **4.4** | **Component Variants & Props** | external design tool Components | Boolean/text properties on components; swap-instance based on variant metadata. Model as CRDT change-sets + metadata. Ref: *Game Programming Patterns* — Component pattern |
| **4.5** | **Offline / Local-First Mode** | Exploit external design tool's biggest weakness | IndexedDB change-set persistence; reconnect re-sync via OT layer (P2.3). Long-term: local-first DB (custom store). Ref: *Designing Data-Intensive Applications* + *Event-Driven Architecture* |
| **4.6** | **AI Design Assistant** | Canva Magic Design + external design tool AI | MCP server integration in-app: "Generate layout from prompt", "Apply colour palette", "Resize for breakpoints". **Local LLM via WebGPU/WASM** for offline + privacy-preserving AI — our biggest differentiator vs. all competitors |
| **4.7** | **Template Library & Brand Kits** | Canva | Community-contributed gallery stored as standard Logos files. One-click "Use template". No proprietary format |
| **4.8** | **Prototyping & Interactions** | external design tool + Adobe XD | Connect frames with triggers; CSS transitions + SVG animations as implementation layer |
| **4.9** | **Dev Mode (Code Export)** | external design tool Dev Mode | Because Logos is CSS-aware, generate production-ready HTML/CSS/React/Tailwind/Svelte directly via pluggable template system |

---

### Phase 5: WebGPU & Next-Gen Graphics (Weeks 33-40, ongoing)

Port the rendering core from Skia/WebGL to **WebGPU (WGSL shaders)**:

| Capability | Gain |
|-----------|------|
| Compute shaders for layout | GPU-side flex/grid calculation — eliminates worker thread bottleneck |
| Parallel hit-testing and snapping | Massive parallelism; snapping scales to millions of objects |
| True 3D transforms | Perspective editing — a step beyond external design tool |
| Wide-gamut color (Display P3) | Future-proof for HDR displays |

**Implementation:** Begin with a proof-of-concept tile renderer in WGSL alongside the existing WASM module, toggled via feature flag. Reference: *WebGPU Fundamentals*, *Real-Time Rendering* (4th ed.).

---

## 18. Key Performance Metrics & Benchmarks

All metrics are publicly tracked on a **Performance Dashboard** and measured against external design tool (browser) on every release.

| Metric | Logos Target (p95) | external design tool Baseline | Measurement Tool |
|--------|-------------------|----------------|------------------|
| Canvas frame time | ≤ 4 ms | ~6-8 ms (10k shapes) | OpenTelemetry + Chrome trace |
| File open (1,000 shapes) | ≤ 1.5 s | ~2 s | Custom profiler |
| Collaboration merge latency | ≤ 50 ms | ~80-120 ms | Server RPC timings |
| Memory (idle, 10 pages) | ≤ 300 MB | ~350-500 MB | `performance.measureUserAgentSpecificMemory()` |
| WASM binary size | ≤ 2 MB gzip | N/A (C++ native) | CI artifact check |
| Test coverage (common) | ≥ 85% line | N/A | cloverage |
| Serialization cost | Moved to worker (0 ms main thread) | ~40-80 ms main thread | Lighthouse audit |

A monthly **Competitive Scorecard** comparing Logos vs. external design tool and Canva on each feature dimension will be published to keep focus.

---

## 19. Tech Stack Decisions & Governance

### 19.1 Language Decisions (Rationale)

| Language | Role | Decision |
|----------|------|----------|
| **Clojure/Script** | Business logic backbone | **Retain.** Immutable persistent data structures perfectly match CRDT and undo/redo patterns. *Purely Functional Data Structures* (Okasaki) is the theoretical foundation. |
| **Rust** | Performance-critical rendering | **Extend.** Explore `wasm-bindgen` improvements to reduce serialization overhead. Phase 5 adds WGSL alongside Rust. |
| **TypeScript** | Plugin SDK + MCP + Exporter | **Retain.** Types generated from Malli schemas ensure Clojure ↔ TS consistency. |

### 19.2 Data Architecture

We adopt **Event Sourcing** for file changes:
- The `file_change` append-only log is the source of truth.
- This gives us: audit trail, time-travel (version history), and easy offline sync.
- Reference: *Designing Data-Intensive Applications* — event sourcing and CQRS.

### 19.3 CI/CD Requirements

All changes touching geometry, rendering, or collaboration must pass:
1. Unit tests (clj-kondo + clojure.test + vitest)
2. Property-based tests (Malli generators)
3. Performance regression benchmark (frame time must not regress > 5%)
4. WASM compile check (no increase in binary size > 10% without justification)

---

## 20. Execution Timeline Summary

| Week | Milestone |
|------|-----------|
| 1 | ✅ Phase 0 complete: full rename to Logos, `v0.1.0` baseline, green CI |
| 2-6 | Phase 1: performance wins; publishing benchmark dashboard |
| 7-12 | Phase 2: OT conflict resolution, page fragmentation, horizontal scaling |
| 13-18 | Phase 3: unified CLI, full DX improvement, plugin API formal spec |
| 19-32 | Phase 4: vector networks, offline, component variants, AI assistant |
| 33-40 | Phase 5: WebGPU foundation, compute shaders for layout |
| 40+ | Ongoing: template library, community growth, competitive scorecard |

---

*Document updated: May 17, 2026. Part I: Technical Analysis (§1-14). Part II: CTO Strategic Plan (§15-20).*
*Total coverage: 1,074 ClojureScript files · 78 Rust files · 142 TypeScript files · 144 SQL migrations · 5 strategic phases.*
