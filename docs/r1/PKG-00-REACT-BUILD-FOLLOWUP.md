# R1 PKG-00 — React Build Correction

**Status:** Local Node 20 React build and SPA sync pass; full workspace artifact, hosted CI, and release target remain open
**Base commit:** `ddae16ad048cf429b386f14e66095ba8faddb256` (`origin/main`)
**Branch/worktree:** `r1/pkg00-react-build-fix` — `/home/navidrezadoost/Documents/Github/Logos-r1-pkg00-react-build-fix`

## Baseline failure

The locked npm install succeeds in Node `v20.11.1` / npm `10.2.4` when run with Docker host networking and TLS verification enabled. `npm run build` initially reached `tsc -b` and failed with 15 TypeScript errors: three reads of `.color` from the `SolidFill | GradientFill` union, missing methods/types in the minimal WebGPU declarations, and a nullable WebGPU handle dereference. The full error classification and transport observations are recorded in `PKG-00-NPM-DIAGNOSTIC-FOLLOWUP.md` on the separate diagnostics branch.

## Corrections

- Narrow solid and gradient fills explicitly; preserve gradient fills in the plugin API and generate CSS gradient backgrounds for preview/codegen.
- Complete the checked-in minimal WebGPU declarations for the APIs used by the application, without adding dependencies.
- Fail with an explicit error when local LLM code requests WebGPU on an unsupported device.
- Update logos-app CI to sync Vite's `build/` output into the documented/uploaded `dist/` artifact after the type-checked build.

## Executed build

```text
$ docker run --rm --network host \
  -v /home/navidrezadoost/Documents/Github/Logos-r1-pkg00-react-build-fix:/workspace \
  -w /workspace/logos-app node:20.11.1-bookworm-slim \
  npm ci --no-audit --no-fund --fetch-retries=0 --fetch-timeout=120000
added 94 packages in 42s

$ docker run --rm --network host \
  -v /home/navidrezadoost/Documents/Github/Logos-r1-pkg00-react-build-fix:/workspace \
  -w /workspace/logos-app node:20.11.1-bookworm-slim npm run build
icons:build: 23 toolbar and 5 system icons generated successfully
tsc -b: pass
vite v6.4.2: 38 modules transformed; build completed successfully
```

No lockfile or dependency versions changed. The app has no unit-test/spec suite; this is a successful TypeScript+Vite production build, not unit-test coverage. GitHub Actions was not available to confirm a clean runner result. The release workflow still targets absent `logos-desktop` paths, so this does not resolve release artifact provenance or close M0.

## Artifact path audit

`vite.config.ts` sets `build.outDir` to `build/`; the separate `scripts/sync-spa-to-dist.py` copies that SPA into `dist/`. Before this correction, `logos-app CI` ran `npm run build` and uploaded `logos-app/dist/` without running the sync step. The workflow now runs the sync step before uploading.

The sync script succeeded locally and created `dist/index.html` plus `dist/assets/`, but it warned: `Missing workspace bundles in dist/js/. Ensure dist/js/main.js exists.` Thus the React dashboard shell is buildable, while the complete design workspace/WASM payload is still not available from the current checkout. `dist/` contains no tracked source artifact in this branch. The desktop release workflow still points at absent `logos-desktop`; the supported production frontend/server entrypoint remains unverified.

No production rollout is authorized.
