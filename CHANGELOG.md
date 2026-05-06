# Changelog

All notable changes to the **Logos** project are documented in this file.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) · Versioning: [Semantic Versioning](https://semver.org/spec/v2.0.0.html)

---

## [Unreleased] — 2026-05-06

### Added — `logos-wasm` editor (Figma-style toolbar & interaction improvements)

#### Toolbar
- **Floating bottom-centre toolbar** — `TopBottomPanel::top` removed; toolbar now renders as a dark rounded pill anchored to `Align2::CENTER_BOTTOM` (20 px from bottom edge), matching Figma's layout exactly.
- **Move-mode dropdown** — Replaces the flat Select + Pan buttons with a single Figma-style dropdown button showing the active tool icon + chevron. Opens upward with a dark popup listing all three move tools:
  - **Move** (`V`) — select and drag layers
  - **Scale** (`K`) — select with proportional-scale intent (new `Tool::Scale` variant)
  - **Hand** (`H`) — persistent pan mode; no Space key required
- **Shape-tool dropdown** — popup now opens upward (was downward); `pivot(Align2::LEFT_BOTTOM)` applied.
- **Logo label** removed from toolbar; toolbar now contains only interaction tools, zoom controls, grid toggle, and fit button.

#### Hand tool (H)
- **Persistent pan mode** — `Tool::Pan` now works exactly like Space+drag but without holding any key. Open hand (`Grab`) cursor shown when hovering; closed fist (`Grabbing`) cursor shown while dragging.
- **No accidental selection** — single-click and drag-start selection logic explicitly skipped when Hand tool is active.

#### Scale tool (K)
- New `Tool::Scale` variant added to `tools.rs` with icon `K`, label `Scale`, shortcut `K`.
- Keyboard shortcut `K` registered in tool-input handler.
- Canvas drag-start and hover-pos match arms extended to `Tool::Select | Tool::Scale` so scale mode participates in the full selection/resize interaction.

#### Alignment fixes (multi-selection)
- **Root cause fixed** — alignment actions previously only moved `selection[0]`, leaving all other selected layers untouched.
- **Single selection** — aligns against page/canvas bounds (unchanged behaviour).
- **Multi-selection** — computes the union bounding box of all selected layers; each layer is moved so its edge/center aligns to the group's collective edge/center. All 6 alignment operations (left, center-H, right, top, center-V, bottom) now work correctly on any number of simultaneously selected layers.

### Changed — `logos-wasm`
- `Tool::Select.label()` renamed `"Select"` → `"Move"` to match Figma terminology.
- `Tool::Pan.label()` renamed `"Pan"` → `"Hand"` to match Figma terminology.

### Removed
- `CHANGES.md` — duplicate changelog, deleted.
- `PHASE5_ISSUE_STATUS.md` — stale issue tracker snapshot, deleted.
- `PROJECT_REPORT.md` — outdated v2.0.0 report, deleted.

---

## [v3.0.0] — 2026-04-22

### Summary

Five focused engineering sprints completing the full collaborative backend stack:
multi-tenant identity, desktop UI state, HTTP client networking, Axum REST server,
and offline conflict resolution. Total test suite: **676 passing** in `logos-collab`,
**279 passing** in `logos-desktop`, 0 failures.

---

### Sprint 5 — Offline Conflict Resolution (commit `465ac35`) — 2026-04-22

#### Added — `logos-collab`

- **`conflict.rs`** — `ConflictStore` with full lifecycle management
  - `ElementVersion` — captures editor metadata, element type, JSON properties, parent version
  - `ResolutionStrategy` — `AcceptLocal | AcceptRemote | AcceptBoth | RejectAll`
  - `ConflictStatus` — `Pending → UnderReview → Resolved | Rejected`
  - `ConflictRecord` — project-scoped, indexed by element ID, with reviewer assignment and resolution audit trail
  - 12 unit tests (CF-01..CF-12)
- **`sync_status.rs`** — `SyncStatusStore` for per-element sync state tracking
  - `SyncState` — `Synced | Pending | Conflicted | Rejected | Syncing`
  - `SyncStatusRecord` — tracks `last_sync_at`, `pending_since`, `conflict_id`, `retry_count`, `error_message`
  - Filter by project and state; `clear_rejected()` batch operation
  - 12 unit tests (SS-01..SS-12)
- **`offline_tracker.rs`** — `OfflineTracker` for local edit queue while disconnected
  - `EditType` — `Create | Update | Delete`
  - `LocalEdit` — captures element ID, project, timestamp, properties, and version
  - `set_offline(bool)` mode switch; query pending edits by element or project; bulk clear operations
  - 10 unit tests (OT-01..OT-10)
- **`http_server/handlers/conflicts.rs`** — REST handlers for conflict management
  - `list_conflicts` — `GET /api/projects/:id/conflicts`
  - `create_conflict` — `POST /api/projects/:id/conflicts`
  - `get_conflict` — `GET /api/conflicts/:id`
  - `mark_under_review` — `POST /api/conflicts/:id/review`
  - `resolve_conflict` — `POST /api/conflicts/:id/resolve`
  - `reject_conflict` — `POST /api/conflicts/:id/reject`
  - `get_sync_status` — `GET /api/projects/:id/sync-status`
  - Feature-gated `#[cfg(feature = "http-server")]`; DTOs always compiled for test portability
  - 4 handler unit tests (HCON-01..HCON-04)
- **`tests/conflict_workflow.rs`** — end-to-end integration tests
  - Full workflow: offline edit → conflict detection → reviewer assignment → resolution → sync propagation
  - 10 integration tests (CW-01..CW-10)
- `AppState` extended with `conflicts: Arc<RwLock<ConflictStore>>` and `sync_status: Arc<RwLock<SyncStatusStore>>`
- 7 new REST routes registered in `routes.rs`

#### Added — `logos-desktop`

- **`conflict_reviewer.rs`** — `ConflictReviewer` state machine for split-screen review UX
  - `ConflictReviewerState` — `Idle | FetchingConflicts | ConflictList | ReviewingConflict | SubmittingResolution | ResolutionComplete | Error`
  - `ConflictReviewerEvent` — `Open | ConflictsFetched | SelectConflict | ConflictDetailsFetched | SelectStrategy | SubmitResolution | ResolutionSuccess | Error | Close`
  - `ConflictSummary` and `ElementVersionPreview` for UI data binding
  - `can_submit()` guard; `transition()` with exhaustive state-event handling
  - 10 unit tests (CR-01..CR-10)

---

### Sprint 4 — Axum REST Server (commit `2c1b6c7`) — 2026-04-21

#### Added — `logos-collab` (feature: `http-server`)

- `AppState` — shared Arc state holding all domain stores (company, project, user, session, collab)
- `app_router()` — Axum 0.8 router with Tower CORS + tracing middleware
- **13 REST endpoints** across 4 handler modules:
  - `auth` — `POST /api/auth/register`, `POST /api/auth/login`, `POST /api/auth/logout`
  - `companies` — `GET/POST /api/companies`, `GET/PATCH /api/companies/:id`, `POST /api/companies/:id/members`
  - `projects` — `GET/POST /api/projects`, `GET /api/projects/:id`, `GET /api/projects/:id/sessions`
  - `sessions` — `GET /api/sessions/:id`, `POST /api/sessions/:id/join`, `POST /api/sessions/:id/leave`
- Axum extractors: `Path`, `Json`, `State`; unified `ApiError` → HTTP status mapping
- 4 integration tests against live Axum router (AT-01..AT-04)
- Feature-gated Cargo deps: `axum 0.8`, `tower 0.5`, `tower-http 0.6`

---

### Sprint 3 — Network Layer / HTTP Client (commit `cf228fd`) — 2026-04-20

#### Added — `logos-desktop` (feature: `http-client`)

- `network/client.rs` — `LogosClient` async HTTP client wrapping `reqwest 0.12` + rustls-tls
  - `login()`, `register()`, `get_projects()`, `create_project()`, `join_session()`, `leave_session()`
  - Bearer token auth; configurable base URL; `ClientError` with `Unauthorized | NotFound | ServerError | Network | Parse`
  - 4 unit tests (NC-01..NC-04)
- `network/api_types.rs` — shared DTOs mirroring server JSON contracts
  - `LoginRequest/Response`, `RegisterRequest`, `ProjectDto`, `CreateProjectRequest`, `SessionDto`, `JoinSessionRequest`, `ApiErrorResponse`
  - Full `serde` derives; `#[serde(rename_all = "camelCase")]` for JS interop
  - 3 unit tests (DT-01..DT-03)

---

### Sprint 2 — Desktop UI State Layer (commit `c30fe61`) — 2026-04-19

#### Added — `logos-desktop`

- **`app_state.rs`** — `AppState` central store (selected tool, active layer, zoom, theme, modal stack)
  - 8 unit tests (AS-01..AS-08)
- **`tool_state.rs`** — `ToolState` FSM: `Idle → Active → Dragging → Committed`
  - `ToolKind` — `Select | Pan | Pen | Rectangle | Ellipse | Text | Image | Hand`
  - 7 unit tests (TS-01..TS-07)
- **`layer_state.rs`** — `LayerState` tree with visibility, lock, expand, reorder, delete
  - 8 unit tests (LS-01..LS-08)
- **`selection_state.rs`** — `SelectionState` with single/multi-select, bounding-box, property merge
  - 8 unit tests (SL-01..SL-08)
- **`history_state.rs`** — `HistoryState` undo/redo stack with `Command` trait and snapshot diffing
  - 7 unit tests (HS-01..HS-07)
- **`viewport_state.rs`** — `ViewportState` with pan, zoom (0.1×–32×), fit-to-canvas
  - 7 unit tests (VS-01..VS-07)
- Total: 45 new unit tests across 6 state modules

---

### Sprint 1 — Multi-Tenant Identity & Desktop Sync (commit `0b379ca`) — 2026-04-18

#### Added — `logos-collab`

- **`company.rs`** — `CompanyStore` multi-tenant container
  - `Company::new(name, owner_id)` auto-enrolls creator as `CompanyRole::Admin`
  - Roles: `Admin | Editor | Viewer`; `add_member`, `remove_member`, `change_role`, `list_members`
  - 10 unit tests (CO-01..CO-10)
- **`project.rs`** — `ProjectStore` scoped to company
  - `Project::new(name, desc, company_id, creator_id)` with auto-membership
  - `add_collaborator`, `remove_collaborator`, `set_active`, `archive`
  - 12 unit tests (PJ-01..PJ-12)
- **`session.rs`** (enhanced) — per-project sessions with presence tracking
  - `join_session`, `leave_session`, `list_active_sessions`
  - 8 unit tests
- **`auth.rs`** — `UserStore` + Argon2id password hashing + HMAC-SHA2 token issuance
  - `register`, `login`, `validate_token`, `revoke_token`, `change_password`
  - 15 unit tests (AU-01..AU-15)
- **Stress test suite** — 50-user concurrent simulation (`logos-collab/tests/stress/`)
  - Metrics: operations/sec, conflict rate, sync latency percentiles (p50/p95/p99)
  - HTML report generation

---

## [v2.0.0-rc.1] — 2026-02-16

25 commits across 16 feature branches. All 19 workspace crates compile. **2,007 tests pass**.

### Performance

- CRDT hot path 24% faster — deferred delta encoding (`d30e153`)
- Batch transaction API 50% faster at N=10 (`858d046`)
- Atlas lookup 86% faster with O(1) flat-array indexing (`c9f250a`)
- Spatial hash 88% faster hit testing with bitflag permissions + inline AABB (`afc5a4a`)
- Text shaping 97.5% latency reduction via shaped-run cache (`17fe0ac`)
- Layout diffing — FxHashMap + reusable buffers (`008acc1`)
- Frame coherence — retained instance buffer with O(Δ) incremental updates (`c12ecf0`)
- GPU-driven rendering — partial buffer uploads + draw indirect + dirty-slot tracking (`2260724`)

### Added — Web Platform

- WASM + WebGPU target compiled to `wasm32-unknown-unknown` (`058dbf4`)
- 23 JS-exported methods via `wasm-bindgen`
- Camera module: pan, zoom, screen-to-world coordinate mapping

### Added — Plugin System

- Wasmtime WASM runtime — sandboxed execution with fuel and memory limits (`a79a5a9`)
- 21 host functions across 6 categories: document, selection, viewport, UI, lifecycle, state (`cd208ec`)
- TOML manifest with permission declarations (`7d2dfbf`)
- Ed25519 signature verification — cryptographic plugin signing
- Marketplace HTTP client — search, download, publish, rate, review with caching
- 3 example plugins: hello-world, shape-generator, color-palette

### Added — Desktop UI

- Command system — 60+ command variants, `CommandRegistry`, `CommandHistory` (`80194ef`)
- Shortcut registry — Figma-compatible tool shortcuts (V/R/O/T/P/H/Z/F/L/I)
- Toolbar — 3 preset toolbars with layout-computed hit testing
- Panel manager — 7 dockable panels (Layers, Properties, Library, History, Color, Typography, Export)
- Command palette — fuzzy-search with MRU tracking and category filtering
- Tab bar — multi-document tabs with dirty indicators, pinning, and reorder

### Added — File Format Importers

- Figma (.fig) — binary parser, 20 node types (`16983d8`)
- SVG — dependency-free XML parser with full path data support (`b00f858`)
- Sketch — ZIP extraction + JSON model mapping
- PDF — content stream tokenizer + page extraction
- Adobe XD — ZIP/AGC extraction + artboard mapping
- Canva — JSON template parser + element conversion

### Added — Marketplace

- `logos-marketplace-auth` — Ed25519 keypair generation, JWT sessions, permission scoping (`b4795cb`)
- `logos-marketplace-db` — PostgreSQL schema, 7 tables (publishers, plugins, versions, reviews, downloads, categories, audit_log)
- `logos-marketplace-api` — REST server, 18+ routes for publishing, search, review, admin
- Marketplace UI — 6-step publisher onboarding, plugin submission, gallery, analytics, moderation (`9bce46a`)

### Added — AI Engine

- ONNX Runtime integration — real inference via `ort` v2 (`4c57e96`)
- Model quantization — FP32 → FP16 compression, 6.48 MB → 1.63 MB (-75%) (`1b015bf`)
- Embedding pipeline — style extraction and layout suggestion via quantized models
- Criterion benchmarks — layout generation 30.9 µs/10 variations, style transfer 32.2 µs, asset decoding 8.6 µs (`9b57791`)

---

## [v1.1.0] — 2026-02-16

- AI engine scaffolding — `logos-ai` crate with simulated ONNX inference (`e6c499b`)
- ONNX Runtime real inference backend via `ort` v2 (`8957548`)
- AI benchmarks — Criterion suite for inference latency (`569cedd`)
- Merged `release/v1.0.0-rc.1` into main (`24319e5`)

---

## [v1.0.0-rc.1] — 2026-02-10

- Core engine — CRDT-based document model with operational transform
- Layout engine — constraint-based layout with Taffy integration
- Render pipeline — wgpu-based GPU rendering with instance batching
- Text engine — cosmic-text shaping with glyph atlas
- Collaboration — WebSocket server with RocksDB persistence and presence tracking
- Plugin system — dual JS (QuickJS) and WASM runtime with permission model
- Desktop shell — winit 0.30 window with mouse/keyboard input and GPU surface
- WASM target — `logos-wasm` crate for WebAssembly compilation
- CI pipeline — GitHub Actions with build, test, and WASM verification

---

[v3.0.0]: https://github.com/navidrezadoost/Logos/compare/v2.0.0-rc.1...v3.0.0
[v2.0.0-rc.1]: https://github.com/navidrezadoost/Logos/compare/v1.1.0...v2.0.0-rc.1
[v1.1.0]: https://github.com/navidrezadoost/Logos/compare/v1.0.0-rc.1...v1.1.0
[v1.0.0-rc.1]: https://github.com/navidrezadoost/Logos/releases/tag/v1.0.0-rc.1
