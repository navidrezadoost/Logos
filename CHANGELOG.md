# Changelog

All notable changes to the **Logos** project are documented in this file.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) · Versioning: [Semantic Versioning](https://semver.org/spec/v2.0.0.html)

---

## [Unreleased] — 2026-05-16

### Logos Branding, Auth & Dev Infrastructure

#### Branding
- Replaced all Penpot logo references on login and static error pages with the Logos PNG (`frontend/resources/public/images/logos-logo.png`)
- Updated `auth.cljs` to render `<img src="images/logos-logo.png">` in place of the deprecated SVG icon
- Added `.logos-logo` CSS class in `auth.scss` for proper image sizing
- Updated `static.cljs` login modal and error container headers to use the Logos image
- Updated English translations (`translations/en.po` and pre-compiled `translation.en.js`) — page title, login tagline, not-found strings, and dashboard references now say "Logos" instead of "Penpot"

#### CORS & Session Fixes
- Added `x-external-session-id`, `x-event-origin`, and `x-client` to `access-control-allow-headers` in `backend/src/app/http/middleware.clj` — eliminates CORS preflight failures from the frontend
- Fixed session cookie `SameSite` logic in `backend/src/app/http/session.clj`: only set `SameSite=None` when both `cors` flag **and** `secure-session-cookies` flag are active; otherwise falls back to `Lax` — resolves cross-origin cookie rejection in HTTP-only local dev where `SameSite=None` requires `Secure`

#### Rasterizer Fix
- Patched `frontend/src/app/main/rasterizer.cljs` to HEAD `origin` (pointing to `rasterizer.html`) instead of `cf/rasterizer-uri` root (`/`) which returns 404 under shadow-cljs dev server — eliminates the adblocker-detection false positive and rasterizer fallback loop

#### Local Dev Infrastructure
- Added `backend/scripts/start-dev-local` — single-command backend launcher using `flatpak-spawn --host` to run Clojure in the host JVM from inside a Flatpak container
- Added `backend/scripts/_env.local` — local environment overrides: PostgreSQL on `127.0.0.1:5432`, Redis on `127.0.0.1:6379`, filesystem storage (no Minio), correct `PENPOT_PUBLIC_URI`, and `enable-cors` flag with `PENPOT_CORS_ALLOWED_ORIGINS=http://localhost:8888`
- Added Duotone icon assets (`frontend/resources/images/icons/duotone/`) and supporting ClojureScript component (`duotone_icon.cljs`, `duotone_icon.scss`)

#### User Account Bootstrap
- Created initial admin account (`admin@logos.app`) via the two-step registration API and activated it directly in PostgreSQL — ready for immediate login at `http://localhost:8888`

---

## [Unreleased] — 2026-05-10

### 2026-05-09 — Major Frames & Hierarchy System (d2053a6)

**Identity**: navidrezadoost / navidrezadoost07@gmail.com  
**Push**: `d2053a6` to navidrezadoost/Logos (no proxy used)

#### Core Architecture
- Full hierarchical layer system with `LayerRecord.parent_id: Option<Uuid>`
- Local coordinate system for all children (positions relative to parent)
- Recursive child handling and traversal helpers

#### Figma-Style Frames
- `Frame` layer type with `clip_content` (default `true`), `frame_expanded`, corner radius, fill, stroke, etc.
- Hierarchical rendering with optional clipping, frame name label (Figma-style), parent chain tint, and dashed overflow indicator
- Frame helpers: `wrap_in_frame()`, `ungroup_frame()`, `resize_frame_to_fit()`, `frame_children()`

#### Auto Layout & Constraints
- `AutoLayout` struct (direction, padding, gap, sizing modes: Hug/Fixed/Fill)
- `Constraints` struct for regular frames
- Full right-panel controls and layout pre-pass

#### Drag, Drop & Reparenting
- Canvas auto-reparenting when dropping layers into frames (Spacebar to suppress)
- Layers panel tree drag & drop with indentation and insertion preview
- When a frame is dragged, all children move together maintaining their local position relative to the frame

#### UI/UX Improvements
- Unicode icon system for tools and layer types
- Layers panel: DFS tree, expand/collapse, context menu, visibility toggles
- Right panel Frame section with Clip Content + Auto Layout controls
- Keyboard shortcuts: `Ctrl+Alt+G` (wrap in frame), `Shift+Ctrl+G` (ungroup), `Enter` (drill in), `Shift+Enter` (select parent)
- Effects system (7 types) + Blend Modes (18 CSS Compositing Level 1 modes) with hover preview

This update establishes a solid, Figma-like foundation for hierarchy, frames, and responsive layout capabilities.

---

## [Unreleased] — 2026-05-09

### Added — `logos-wasm` editor (Figma-style Frames, Auto Layout, interaction polish)

#### Unicode Icon System
- **All toolbar tool icons** replaced with meaningful Unicode symbols: `↖` (Move), `⤡` (Scale), `#` (Frame), `▭` (Rect), `◯` (Ellipse), `⬡` (Polygon), `T` (Text), `✎` (Pen), `✋` (Pan), `╱` (Line), `→` (Arrow), `★` (Star).
- **Layer type icons** in both the Layers panel tree and the right-panel header badge updated to match: `▭`, `#`, `T`, `◯`, `✎`, `⊞`, `⬡`, `╱`, `→`, `★`.
- All icons derived from egui's built-in NotoEmoji range; no external font required.

#### Figma-Style Frame Layer System
- **`parent_id: Option<Uuid>`** — each layer now tracks its parent frame, enabling true hierarchical containment.
- **`clip_content: bool`** — frames clip child layers to their bounds by default (matching Figma). Togglable in the right panel.
- **`frame_expanded: bool`** — controls expand/collapse state in the Layers panel tree.
- **`LayerRecord::new_frame()`** — sets `clip_content = true` and white fill by default.
- **Frame name label** — rendered above the frame on canvas, but only when the frame is selected or hovered (not always-on). Accent-purple when selected, gray when hovered.
- **Parent chain tint** — when a child inside a frame is selected, the parent frame's border softens to accent purple at 30 % opacity, visualising containment hierarchy.
- **Dashed overflow outline** — when `clip_content = false` and the frame has children, a hand-crafted dashed border is drawn around the frame to signal that children may overflow.
- **Hierarchical rendering** — root-level render loop skips layers that have a `parent_id`; frame render arm draws its own background, then iterates and renders all children inside a clipped (or unclipped) `Painter` scope.

#### Auto Layout (Horizontal / Vertical)
- **`AutoLayout` struct** — `direction`, `gap`, `padding` (per-side), `gap_auto`, `sizing_h`, `sizing_v`, `align`.
- **`SizingMode`** — `Fixed`, `HugContents`, `FillContainer`.
- **`Padding`** — four-sided; `uniform()` constructor; `is_uniform()` helper.
- **`AutoLayoutDirection`** — `Horizontal` / `Vertical`.
- **`apply_auto_layout(frame_id)`** — pure layout pass: repositions children according to direction + gap + padding; resizes the frame when `HugContents`; does not push to undo history (called every render frame as a pre-pass).
- **Auto Layout pre-pass** in `canvas_panel` — all frames with `auto_layout.is_some()` are reflowed before the draw loop so children are always positioned correctly without explicit "Apply" calls.
- **Right panel Auto Layout section** (visible only when a Frame is selected):
  - Toggle (`+ Add` / `− Remove`) to enable or disable Auto Layout.
  - Direction picker (→ horizontal / ↓ vertical) with icon buttons.
  - Gap slider (0–80 px).
  - Padding slider (uniform mode with single control).
  - Width / Height sizing (Fixed / Hug / Fill) segmented control.
  - Alignment picker (⇤ start / ⟺ center / ⇥ end).
  - **⟳ Apply Layout** button for one-shot repositioning with history push.

#### Constraints
- **`Constraints { horizontal: ConstraintType, vertical: ConstraintType }`** — five horizontal variants (Left, Right, LeftRight, Center, Scale) and vertical equivalents. Added to `LayerRecord`; defaults to `Left` / `Left`.

#### Canvas Auto-Reparenting on Drop
- When a move-drag ends **without Spacebar held**: finds the smallest (deepest) frame that fully contains the bounding box of each dropped layer and sets its `parent_id` accordingly — automatic nesting, matching Figma's default drag-onto-frame behaviour.
- **Spacebar suppresses auto-reparenting** (hold before or during drag) — exact parity with Figma's power-user modifier.
- Works per-layer across multi-selection drags.

#### Frame Helpers (state.rs)
- **`frame_children(frame_id)`** — returns ordered children of a frame from the page list.
- **`reparent_layer(layer_id, Option<Uuid>)`** — sets `parent_id`; pass `None` to detach.
- **`wrap_in_frame()`** — wraps the current selection in a new Frame sized to the selection's bounding box + 16 px padding; inserts the frame at the first selected layer's position; reparents all selected layers; selects the new frame. Undo-able.
- **`ungroup_frame(frame_id)`** — removes a frame but keeps all children in place (they become top-level siblings at the frame's former page position, preserving absolute world-space positions). Undo-able.
- **`resize_frame_to_fit(frame_id, padding)`** — shrinks/grows a frame to tightly wrap its visible children + specified padding. Undo-able.
- **`remove_layer()`** — now recursively removes all children when a frame is deleted.

#### Keyboard Shortcuts
| Shortcut | Action |
|---|---|
| `Ctrl + Alt + G` | Wrap selection in Frame |
| `Shift + Ctrl + G` | Unwrap / Ungroup selected Frame |
| `Enter` | Drill into selected Frame (select first child) |
| `Shift + Enter` | Select parent of selected layer |

#### Layers Panel Tree View
- **Hierarchical DFS tree** — root layers at top level; children indented 16 px per nesting depth. Iterative DFS using a stack ensures stable ordering without recursion depth limits.
- **Expand / collapse triangles** (`▸` / `▾`) on non-empty frames; toggle stored in `frame_expanded`.
- **Visibility toggle** updated to `◎` / `○` Unicode icons.
- **Children rendered at smaller font size** (12 pt vs 13 pt default) to visually distinguish nesting depth.
- **Context menu** extended:
  - "Unwrap Frame (Shift+Ctrl+G)" → `ungroup_frame()`, children preserved in place.
  - "Resize to Fit Contents" → `resize_frame_to_fit(16.0)`.
  - "Wrap in Frame (Ctrl+Alt+G)" → `wrap_in_frame()`.

#### Right Panel Frame Section
- Visible **only** when a single Frame layer is selected.
- **Clip Content** checkbox (live-togglable, writes to history on change).
- Full **Auto Layout** sub-section (see above).
- **Resize to Fit** and **Unwrap** quick-action buttons.

#### Interaction Improvements
- **Shift + axis lock** during move drag — locks movement to the dominant axis (horizontal or vertical) once the pointer moves more than 2 px.
- **Alt + drag clone** — duplicates all selected layers in-place when Alt is held at drag start; the drag then moves the clones, leaving originals intact.
- **Rotation-parallel edge snap** — during move drag, detects other layers' rotation angles; snaps the dragged layer's rotation and flushes perpendicular edges when within 15 ° (mod π). Draws a guide line along the snapped edge.
- **Alt cursor** — `CursorIcon::Copy` shown when hovering an unlocked layer with Alt held.

#### Effects & Blend Mode System
- **`BlendMode` enum** — 18 modes in 5 groups (Normal, Darken, Multiply, PlusDarker, ColorBurn, Lighten, Screen, PlusLighter, ColorDodge, Overlay, SoftLight, HardLight, Difference, Exclusion, Hue, Saturation, Color, Luminosity).
- **`EffectKind` enum** — 7 types: Drop Shadow, Inner Shadow, Layer Blur, Background Blur, Noise, Texture, Glass. Each has capability flags (`has_offset`, `has_blur`, `has_spread`, `has_color`, `has_amount`).
- **`Effect` struct** — `kind, enabled, x, y, blur, spread, opacity, color:[f32;4], blend_mode, amount`.
- **`LayerRecord.effects: Vec<Effect>`** replaces old single `DropShadow`.
- **`LayerRecord.blend_mode: BlendMode`** — layer-level blend mode.
- **CSS Compositing Level 1 math** implemented entirely in Rust: `blend_channel`, `blend_rgb`, `rgb_to_hsl`, `hsl_to_rgb`, `blend_effect_color`, `apply_layer_blend` — all 18 modes for both layer fill and every effect.
- **Hover-preview for blend modes** — `EditorState.blend_preview: Option<(Uuid, usize, BlendMode)>` cleared every frame; set on ComboBox hover; read by renderer before committed value — live canvas preview while the user browses, commit on click.
- **Blend mode inside Effects section** — layer-level blend appears as the first row inside the Effects collapsible section (not as a standalone panel), matching Figma's panel structure.
- **Right panel Effects section** — multi-row effect list; per-effect enable/disable checkbox, all parameter controls conditional on `EffectKind` flags; "Add Effect" combo.

### Changed — `logos-wasm`
- `type_icon()` returns Unicode glyphs instead of ASCII bracket codes (`[R]` → `▭`, `[F]` → `#`, etc.).
- `Tool::icon()` updated to Unicode set.
- Right-panel header badge now calls `rec.type_icon()` directly instead of a local match arm.
- Frame auto-reparenting replaces the old "always top-level on canvas" behaviour.
- `remove_layer()` now recursively removes all descendants of a frame.

---



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
