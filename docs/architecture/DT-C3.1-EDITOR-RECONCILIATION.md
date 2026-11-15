# DT-C3.1 — Rust color-token preview reconciliation

**Status:** experimental, transient preview; durable token persistence is not authorized
**Baseline:** `f920411cd190473af8cce23960e28471ddfa85dd`
**Source:** Draft PR #90, head `bc3bf3b87dd141e0f670f726158b00cf43072d21`

## Reconciliation inventory

| PR #90 path | Classification | DT-C3.1 disposition |
| --- | --- | --- |
| `logos-app/src/tokens/contract.ts` | Required feature logic | Ported as typed v0.1 Rust wire DTOs; `DocumentContext.pageId` is represented explicitly. |
| `logos-app/src/tokens/rustTokenEngine.ts` | Required feature logic | Ported as a cached Rust WASM bridge client; there is no JavaScript resolver. Failed initialization is evicted so a later attempt can retry. |
| `rust/logos-tokens-wasm/{Cargo.toml,src/lib.rs}` | Required feature logic | Ported as a JSON/linear-memory adapter around the existing `logos-tokens` resolver and plan validator. It adds no resolution semantics. |
| `scripts/build-logos-tokens-wasm.sh` | Required build integration | Ported; emits the bridge under the static `logos-app/dist/js` asset directory, with `OUT_DIR` available for isolated builds. |
| `rust/Cargo.toml`, `rust/logos-tokens/Cargo.toml`, `rust/logos-types/Cargo.toml`, `rust/Cargo.lock` | Required dependency integration | Added the bridge workspace member and direct UUID serialization dependency. Moved UUID v4 generation to test-only features and removed the wasm-bindgen-backed UUID JS feature from the bridge dependency graph; production UUID v5 derivation remains enabled. Lockfile is regenerated. No canonical document or persistence model is changed. |
| `logos-app/src/tokens/tokenWorkflow.ts` | Required feature logic | Ported with an ephemeral revision and scoped page/document context. An unverified document uses a contract-local UUID only; it is not assigned to the document store. |
| `logos-app/src/stores/tokenWorkflowPreviewStore.ts` | Required feature logic, adapted | Replaced the old per-shape map with one page/document-scoped preview and an owner token. Only the owning Inspector session may clear it. |
| `logos-app/src/components/inspector/TokenWorkflowControl.tsx` | Required feature logic, adapted | Integrated with current selection/page/document state. Generation and live-ownership checks discard late WASM results after selection, page, fill, identity, or lifecycle changes. Failure clears the owned preview. |
| `logos-app/src/components/inspector/Inspector.tsx` | Conflict requiring adaptation | Kept the current unconditional hook order and selection authority. Offers preview only for the first solid fill of a renderer-known shape; ordinary color edits clear transient preview state. |
| `logos-app/src/components/canvas/Canvas.tsx` | Conflict requiring adaptation | Applied preview color only to a render projection derived from the active page scene, then sent that projection through existing hierarchical Rust, WebGPU, or Canvas 2D paths. The document store and page graph remain the source of IDs, geometry, membership, and order. Retained renderer recreation and added explicit resize stale-frame clearing. |
| `logos-app/src/test-support/a7-renderer-compatibility-smoke.tsx` | Test synchronization update | Retained the stale-frame assertion and made it wait for the Layers reflow and actual canvas resize, avoiding a one-frame timing assumption. |
| `logos-app/src/test-support/dt-c3-token-workflow-smoke.tsx` | Required test, adapted | Uses the production Inspector and Canvas, real compiled Rust token WASM, actual canvas pixels, and lifecycle changes. It checks non-mutation, ownership disposal, errors, resize, and repeated mount/unmount. |
| `test/benchmarks/dt-c3-token-workflow-browser.mjs` | Required test runner | Uses Playwright against installed Chromium, records the sanitized browser/renderer/pixel report, and fails on runtime exceptions or failed acceptance assertions. |
| `logos-app/test-harness/dt-c3-token-workflow.html`, `logos-app/vite.dt-c3-token-workflow-smoke.config.ts` | Required test infrastructure | Isolated browser entry/build. It uses the normal production renderer loader and the compiled Rust token artifact; local runs fall back only when renderer assets are absent. Existing production renderer browser checks remain enabled unchanged. |
| `.github/workflows/rust.yml` | Required CI integration | Added narrow token paths, a bridge build using the existing `wasm32-unknown-unknown` target, and isolated Chromium smoke. Existing A8/A9/A10 steps and assertions remain in place and run normally. |
| `logos-app/src/vite-env.d.ts`, `logos-app/vite.config.ts` | Required build integration | Added the bridge asset URL declaration and production static asset path. |
| `logos-app/src/components/inspector/TokenWorkflowControl.tsx` error/permission behavior | Conflict requiring adaptation | Proposal permissions and revisions remain validator inputs for an experiment. They do not prove authentication, authorization, or server revision ownership. No durable operation is called. |
| `logos-app/src/components/canvas/Canvas.tsx` flat-scene sync from PR #90 | Obsolete infrastructure for current main | Not ported. Current Canvas uses `useCurrentPageScene` and `projectAndSyncScene`; no legacy flat uploader or competing geometry path is introduced. |
| PR #90 changes to `logos-app/src/components/canvas/Canvas.tsx` WebGPU and fallback plumbing | Already implemented or superseded | Kept the current renderer ownership and injected only a visual fill projection into each existing backend. No parallel renderer is added. |
| PR #90 broad benchmark, Go-transaction, and migration discussion | Documentation not required for this slice | Retained only the production authorization and persistence limitations relevant to this preview. No Go API, schema, or persistence code is changed. |

## Supported workflow

The Inspector exposes “Preview as token” only for a selected renderer-known shape
whose first fill is solid. The action creates an ephemeral typed proposal,
validates direct fill resolution, and submits create-token plus property-binding
operations to the existing Rust validator. A second action proposes a token
value change through that same validator. Rust's resolved color is applied to a
single transient Canvas projection. Revert, direct fill editing, changing
selection/page/document, deleting the shape, or unmounting the Inspector
disposes of that preview.

The render projection changes one fill color only. Shape IDs, fill order,
geometry, transforms, parent/child membership, root order, and persistent
`Shape.fills` are passed through unchanged. Page-scoped identity prevents a
preview from appearing on another page. A monotonically changing owner
generation plus a live page/selection/fill check prevents delayed WASM results
from restoring a disposed preview.

Unsupported fills and renderer-unknown shape types do not expose the workflow.
If WASM initialization, contract validation, or resolution fails, the current
owner's preview is cleared and the diagnostic remains in the Inspector. An
unmounted or stale owner cannot publish a result or clear a newer owner's
preview.

## Capability and evidence matrix

| Capability | Supported path | Evidence |
| --- | --- | --- |
| Create an ephemeral proposal | Selected single shape, first fill is solid | Rust contract result; actual Inspector control |
| Bind a fill and resolve its color | Rust `logos-tokens` WASM resolver/plan validator | Rust contract tests and browser status/source |
| Preview changed token value | One transient page/document/shape/fill projection | Sampled real Canvas pixels before and after |
| Cancel and restore visual state | Owner-scoped clear; canonical fill remains untouched | Pixel equality and store snapshot equality |
| Selection/page/delete/unmount disposal | Active Inspector owns its transient preview | Browser lifecycle regression checks |
| Unsupported shape/fill | Control unavailable; no preview projection | Browser rejection/absence checks |
| Renderer failure/recovery and resize | Current renderer diagnostics/recreation and dimension lifecycle | Existing A7/A8/A9/A10 regressions plus token smoke |
| Persist token definitions or bindings | Not supported | No persistent model or write path is added |
| Authenticated production permission or server revision | Not established | Proposal context is explicitly caller-provided and ephemeral |

## Production limitations

`READY` means the Rust validator accepts this experimental proposal. It is not
evidence of an authenticated user, server authorization, canonical document
revision, or permission to commit. The token proposal and binding remain
transient; `Shape.fills`, IndexedDB, the legacy token store, Go persistence,
collaboration replay, undo history, and saved-file formats are not changed.
This slice does not authorize Rust document mutations or production token
persistence.

## Adjacent identity lifecycle follow-up

The G1-B9.1 review identified a separate P1 risk: `WorkspacePage` can finish an
older asynchronous `loadPersistedDocument()` / `getFile()` sequence after the
user has navigated to another workspace. DT-C3.1 scopes its own transient
preview to the current page/document and discards stale Rust responses, but it
does not qualify or repair workspace hydration ownership. A deterministic
delayed-response test and a WorkspacePage generation/cancellation guard remain
required before G1-B10; production cross-workspace identity isolation remains
unverified.
