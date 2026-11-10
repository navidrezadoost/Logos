# P0 Editor Entrypoint Verification Procedure

**Purpose:** reproducibly identify the artifact launched from the dashboard and verify a real file journey. This procedure is deliberately fail-closed; do not bypass missing schema/artifact checks with mocks.

## Safety and preconditions

1. Use the P0 worktree or an equivalent clean checkout at the approved commit. Record `git rev-parse HEAD`, `git status --short --branch`, tool versions, and artifact hashes.
2. Use an isolated environment and disposable account/data. Bind services to host loopback. Do not use an existing user or production database.
3. **Do not initialize an empty database from the current Go migration folder.** It contains only an additive migration; schema provenance and a clean bootstrap are not approved. Obtain a schema owner’s approved, data-free test database/restore process first. Do not replay the historical Clojure migration filenames by sorting them.
4. Have a supported browser with DevTools, a browser executable/automation harness, and capture storage/log/network evidence. Renderer-only CI is not a substitute.

## Stage 1 — identify the source route and build result

```bash
git rev-parse HEAD
git status --short --branch
node --version
npm --version
go version
rustc --version

cd logos-app
npm ci --no-audit --no-fund
npm run build
python3 scripts/sync-spa-to-dist.py
```

Record whether the following are present and non-empty (the current baseline is expected to fail at the legacy JavaScript check):

```bash
test -s dist/index.html
test -d dist/assets
find dist/assets -maxdepth 1 -type f -size +0c -print -quit | grep -q .
test -s dist/workspace.html
test -s dist/js/main.js
test -s dist/js/shared.js
test -s dist/js/worker/main.js
```

Inspect `sync-logos-workspace.py` for its exact imports and add every referenced JS, worker, WASM, CSS, font, translation, image, and configuration asset to the check list. Record producer, source revision, SHA-256, size, and consumer for each. The SPA sync script’s exit status is insufficient: it calls the legacy workspace sync with `check=False`, so explicitly assert workspace shell and required bundle presence.

**Current P0 observation:** `npm run build` passes; `sync-spa-to-dist.py` prints `Missing workspace bundles in dist/js/. Ensure dist/js/main.js exists.` and still prints that the React SPA was synced. Thus the build is not a complete editor artifact.

## Stage 2 — start isolated services (only after schema approval)

Use the exact database/Redis versions and ports specified by the schema/runtime owner. Start only the isolated services; bind published ports to `127.0.0.1`. Record image digests and service health. Provision the approved schema using its canonical initializer/restore instructions. Do not use the current `-migrate` README command as a bootstrap: `cmd/server` has no such flag handler in the current source.

Set the Go server port to match the Vite/serve proxy (`BACKEND_GO_ADDR=:8080` unless the proxy configuration is intentionally changed in a reviewed task). Supply `DATABASE_URL`, Redis configuration where required, and a test-only secret without printing credentials. Start the Go server and verify:

```bash
curl --fail --show-error http://127.0.0.1:8080/api/_health
```

Confirm registration/login works against the approved schema and that file/project authorization is enforced. The README quickstart says 6060, but the current Go config and frontend proxy use 8080; record the actual listener and requested target.

## Stage 3 — browser journey

Start the supported frontend artifact on loopback using its documented server. In a fresh browser profile:

1. Capture console/network from page load. Register or authenticate a disposable user.
2. Create a project and design file through the dashboard. Record project, file, and page IDs (redact secrets).
3. Click the project. Capture the exact URL, server response, editor HTML, JS/worker/WASM URLs and status codes. Fail on missing assets, blank canvas, runtime exceptions, or dashboard-only content.
4. Confirm the active implementation from loaded module URLs/source maps—not from file names alone. Identify renderer backend/context and verify one canvas has one owner.
5. Create a rectangle in the real document, change geometry and color, and capture a rendered pixel inside and outside it. Confirm the server `update-file`/revision or supported persistence API acknowledges the change; a local IndexedDB write does not count.
6. Reload and reopen the same file. Verify exact shape identity/geometry/style and a nonblank render; confirm the saved server revision/data matches.
7. With the same disposable file, edit offline, reconnect, and verify journal status, server acknowledgement, revision, and reopened state. Do not claim collaboration convergence without two independent clients.
8. Exercise direct URL, refresh, browser Back/Forward, second file, invalid/missing ID, unauthorized file, missing renderer asset, and browser console/network error handling.

Save screenshots, a redacted HAR, console logs, server logs, DB revision evidence, artifact hashes, browser/GPU versions, and pass/fail per step. The test fails if it opens a mock/static screen or only the SPA dashboard.

## Stage 4 — separate renderer regression check

Run the existing hosted Rust workflow and retain its artifact and browser logs. Its standalone Emscripten smoke validates the renderer binary/pixel path only. It does not satisfy Stages 2–3. For product acceptance, run golden pixels through the actual React/editor adapter as well.

## Current execution record and blockers

- Local Go `go test ./...`: pass; `TEST_DATABASE_URL` was unset, so this is not DB integration evidence.
- Local Rust domain workspace locked tests: pass; separate `render-wasm` browser test is hosted, not this browser journey.
- Local React `npm run build`: pass; SPA sync reports the missing `dist/js/main.js` legacy bundle.
- Full browser run: **not executed**. No browser executable was available on PATH, no database URL was configured, and the current checkout has no approved complete schema initializer. No services or production/user data were touched.
- Gate remains **Blocked** until an owner-approved schema and full editor artifact are available, then all browser steps pass.
