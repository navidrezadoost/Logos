# R1 PKG-00 — Frontend Entrypoint Assessment

**Status:** Source-level contract identified; complete editor artifact and supported release entrypoint remain unverified
**Base commit:** `8a3a4b0847f64fb073b36e8f36373876381a2663` (`origin/main` after PR #40)
**Branch/worktree:** `r1/pkg00-entrypoint-assessment` — `/home/navidrezadoost/Documents/Github/Logos-r1-pkg00-entrypoint-assessment`

## Current source contract

- Vite builds the React app from `logos-app/src/main.tsx` into `logos-app/build/`.
- The root React app handles dashboard/auth routes, but `src/main.tsx` and `LegacyHashRedirect.tsx` explicitly redirect `#/workspace` routes to `/workspace.html`.
- `scripts/sync-logos-workspace.py` is the consumer for the legacy editor bundle. It requires `dist/js/main.js`, and its shell preloads/imports the legacy main, worker, renderer, and related JS modules.
- `scripts/serve-frontend.py` uses the `logos-workspace-shell` cookie to serve `/workspace.html` on reload. `sync-spa-to-dist.py` copies the Vite shell into `dist/` and invokes the workspace sync with `check=False`; that caller can therefore succeed while the workspace sync reports missing bundles.

The current source-level product contract is a **composition**: React dashboard/auth shell plus a separately generated legacy design workspace. The Vite build alone is not the complete editor.

## Producer and deployment evidence

- The current sync run produced `dist/index.html` and `dist/assets/`, but reported `Missing workspace bundles in dist/js/. Ensure dist/js/main.js exists.` It did not produce `dist/workspace.html` or `dist/js/main.js`.
- No `logos-app/dist/**` or `frontend/resources/public/js/**` producer artifacts are tracked in current main.
- Historical source before transition commit `ea3b966b` had `frontend/package.json` build commands using `shadow-cljs release main worker`; `manage.sh` packaged `frontend/target/dist`. The transition removed the legacy `frontend/` source tree. The current `manage.sh` build target still references that absent path.
- The remaining `Makefile build-wasm` target builds layout WASM into `logos-app/public/logos-layout/`; it does not produce the legacy `dist/js/main.js` module set. Historical `frontend/resources/public/js/` contained config, two WASM files, and translations, but no `main.js` bundle.
- `.github/workflows/logos-app.yml` now synchronizes `build/` into its uploaded `dist/` path. That fixes the directory mismatch, but the sync warning shows the artifact is still only the React shell/assets. The separate desktop release workflow still points to absent `logos-desktop` paths.

## Recommendation

For compatibility, treat the source's intended entrypoint as **React shell + separately produced legacy workspace assets**. Do not mark the Vite build or synced `dist/` as a complete editor release while `dist/js/main.js` is absent.

The product/release owner must decide whether the legacy workspace remains supported. If yes, identify and pin its actual producer and verify a runtime smoke test that loads `/workspace.html`, `main.js`, its worker, renderer, and required WASM assets. If no, make an explicit compatibility/deprecation decision for the workspace route before changing or removing that contract.

No placeholder bundle, legacy reconstruction, route removal, or production artifact change was made by this investigation.
