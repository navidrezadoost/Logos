# R1-F03 — Legacy Workspace Compatibility Decision

- **Purpose:** Decision package only; no frontend/build/runtime changes made
- **Current baseline:** `058e328f40bf27b9ddcb06085d76560cb2a5508a` (PR #41 merged)
- **Historical source snapshot:** `665bb1ef13c3eeff2e84bf6e0d588a6bdecc6a92`, the parent of transition commit `ea3b966b96a95e6d258e0e5f259529865195a167`

## Decision recommendation

**Choose Option A: preserve the legacy workspace compatibility contract and pursue recovery of its authoritative producer as the preferred feasibility track. Do not retire the workspace now.** The React shell currently sends users and old workspace URLs to `/workspace.html`; the React editor page is present in source but is not routed. Retiring the legacy entrypoint now would remove the only routed editor path without an equivalent supported replacement.

This is not a finding that the historical editor is release-ready, nor authorization to rebuild, restore, or deploy it. The next implementation decision must wait for a separately authorized feasibility gate. If the historical build or compatibility gate fails, return to the product owner for an explicit controlled-retirement decision rather than fabricating assets or silently switching routes.

## Capability and entrypoint inventory

| Area | Historical workspace at `665bb1e` | Current maintained source at `058e328f` |
|---|---|---|
| Product routes | Shadow-CLJS modules for auth, dashboard, settings, viewer and workspace; workspace is the design-editor route. | React login/dashboard are routed. `App.tsx` maps `/workspace/:projectId/:fileId` to `WorkspaceRedirect`, and `main.tsx`/`LegacyHashRedirect.tsx` send workspace hashes to `/workspace.html`. |
| Editing | The historical workspace/data trees include canvas drawing and path editing, selection/transforms, shapes, text, undo/history, pages/layers, color/gradients, grids/layout, assets/media, libraries/tokens, comments, collaboration/presence and interactions. The build also emits renderer/rasterizer and worker modules. | `WorkspacePage.tsx` composes Canvas, Toolbar, Layers, Inspector, Assets, AI, templates, prototype preview, dev mode and token import UI, with document persistence code. However, `WorkspacePage` has no import/use in `App.tsx` or elsewhere under `logos-app/src`; these features are not the current routed editor entrypoint and have not been shown to be feature-equivalent. |
| Import/export and integrations | Historical sources include workspace media/SVG import, token/library flows and `app.plugins.api`; the editor host is part of the plugin runtime contract. | React source includes a Sketch/XD token migration dialog. Separate plugin packages exist, but their presence does not establish a compatible host in the routed React app. |
| Artifact and links | `frontend/shadow-cljs.edn` defines `main`, `main-auth`, `main-dashboard`, `main-settings`, `main-viewer`, `main-workspace`, `shared`, `render`, `rasterizer` and worker outputs. | `sync-logos-workspace.py` still expects the legacy JS module set and writes `workspace.html`; `workspaceURL()` emits `/workspace.html#/workspace?...`. SPA sync invokes this synchronizer with `check=False`, so a React build can succeed while the editor bundle is missing. |

This inventory is source-based, not a claim that every historical feature was enabled in production or that every current React component is functional. Retiring the legacy application would at minimum break workspace routes/deep links, the dashboard's open-project flow, the server's workspace-shell reload behavior, and compatibility for documents/plugins that depend on the legacy host. Stored backend documents are not necessarily deleted by route retirement, but editor access and export/migration paths would need an explicit plan.

## Source recovery and reproducibility finding

The historical sources are recoverable from Git: the snapshot contains `frontend/`, its sibling `common/`, `render-wasm/`, `mcp/` and `plugins/`. The legacy frontend includes `package.json`, `pnpm-lock.yaml`, `deps.edn`, `shadow-cljs.edn` and build scripts. Its package metadata pins pnpm 10.28.2; the pnpm lockfile is present. Clojure source declares versions for direct Maven dependencies and Git SHAs for several Git dependencies. `shadow-cljs.edn` declares the output modules and worker.

The old `frontend/scripts/build` describes a full chain: install frontend JS dependencies, build renderer WASM, build the MCP plugin, compile Shadow-CLJS main/worker, then package JS libraries and assets. This is evidence that a producer existed and that its sources/configuration can be recovered. It is **not sufficient evidence of a reproducible clean release build**:

- No historical build was run for this assessment.
- The build crosses frontend, shared Clojure (`common`), WASM and MCP/plugin toolchains and needs external dependency registries.
- The script calls `pnpm install` without explicitly requesting frozen-lockfile mode; the pnpm lock exists, but the script does not itself enforce that contract.
- The Clojure dependency manifest has no checked-in dependency lock in the historical tree; direct versions/Git SHAs do not by themselves prove the resolved transitive graph or toolchain is reproducible.
- The build script injects the current date/time into build metadata, so byte-identical artifacts are not established even if a successful functional build can be reproduced.

Conclusion: **source recovery is feasible; reproducible artifact recovery remains unverified.** Do not call the legacy editor supported until an authorized, isolated build and artifact/runtime verification succeeds.

## Risk assessment

- **Security:** The recovered frontend is a large, historically coupled application (735 files under `frontend/src/app/main/ui` in that snapshot). No dependency vulnerability audit or security review of the recovered revision was performed. Its Shadow configuration binds the development nREPL host to `0.0.0.0`; any future build/dev setup must keep development endpoints isolated and must not expose them as production services. This is a review trigger, not a claim of a confirmed exploitable vulnerability.
- **Runtime/API compatibility:** The historical app expects its corresponding shared code, RPC/API behavior, WebSocket services, WASM/worker modules and plugin host. Current backend/API compatibility has not been established. Rebuilding only the JavaScript bundle would not validate those contracts.
- **Maintenance:** Restoring the producer also restores responsibility for the ClojureScript, Clojure, pnpm, WASM, MCP plugin and generated-asset toolchains. No current owner, support window or passing historical CI evidence was identified in this source assessment.
- **Retirement:** Removing the route before documenting affected URLs, documents, imports/exports and plugins would create user-visible compatibility loss. The current React editor source is not a drop-in replacement based on source presence alone.

No vulnerability scan, legacy build, deployment scan or runtime execution was performed.

## Required feasibility gate before any producer restoration

Run only after separate implementation authorization, in an isolated worktree/environment:

1. Recover the exact historical source snapshot and establish a clean, locked toolchain for pnpm, Clojure CLI/JDK, WASM and MCP dependencies; do not update dependency versions as part of recovery.
2. Build the complete artifact and verify `workspace.html`, all modules expected by `PENPOT_MODULES`, worker outputs, WASM, renderer and required static assets. Record hashes, versions and provenance.
3. Smoke-test authenticated dashboard-to-editor launch, direct legacy deep links, refresh/back navigation, open/edit/save/reopen of an existing document, import/export, renderer/worker startup, and any supported collaboration/plugin flows.
4. Review dependencies, development listeners, content/security policy and backend/API contracts against the intended deployment. A local successful build alone does not clear security or production gates.
5. Obtain product/release-owner acceptance of supported capabilities, maintenance owner and release artifact contract before changing CI or shipping the producer.

## Migration and rollback approach

**Preferred staged path (Option A):** keep existing routes and compatibility redirects unchanged; build and verify the recovered producer separately; package the React shell and complete workspace artifact together; only then consider wiring the producer into CI/release under a reviewed change. Do not fabricate `main.js`, remove the workspace route, or perform data/writer migration in this assessment.

**Rollback:** retain the current route and whichever prior release artifact is actually in use while validating any future producer; make any later CI/artifact switch independently revertible; on a failed smoke test, revert the artifact/CI switch and continue to block the incomplete editor release. If no complete prior editor artifact is available, keep the release blocked rather than treating the React shell as a fallback editor. Keep document identifiers and legacy links unchanged. No production rollout is authorized.

## Gate status

- Historical source recoverable from repository history: **Yes**.
- Historical dependency/build description present: **Yes**.
- Clean reproducible legacy build: **Not tested / unverified**.
- Complete editor artifact: **Open**.
- Product direction recommended: **Preserve compatibility (Option A); feasibility gate required before restoration**.
- M0/M1: **Remain open**. This assessment does not close root Rust, hosted CI, schema provenance, MCP publication, or A31/A32 deployment gates.
