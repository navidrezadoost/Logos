# R1 PKG-00 — Node 20 npm Install Diagnostic

**Status:** Pinned Node 20 install and corrected React build pass locally; full root Rust workspace and release target remain open
**Base commit:** `ddae16ad048cf429b386f14e66095ba8faddb256` (`origin/main`, after PRs #27–32)
**Branch/worktree:** `r1/pkg00-m0-followup` — `/home/navidrezadoost/Documents/Github/Logos-r1-pkg00-m0-followup`

## Reproduction

The first Node 20 container run used Docker's default bridge network and failed fetching the locked `zustand` tarball. The same install was retried over the host network, with TLS verification enabled and the existing lockfile unchanged.

```text
$ docker run --rm --network host \
  -v /home/navidrezadoost/Documents/Github/Logos-r1-pkg00-m0-followup:/workspace \
  -w /workspace/logos-app node:20.11.1-bookworm-slim \
  npm ci --no-audit --no-fund --fetch-retries=0 --fetch-timeout=120000
added 94 packages in 39s
```

Environment: Node `v20.11.1`, npm `10.2.4`. The install completed with existing lockfile versions; no dependencies or lockfiles were changed. The earlier Node 20 run recorded in `PKG-00-VERIFICATION-EVIDENCE.md` (`Exit handler never called`) and the bridge-network failure below are separate transport observations.

## Sanitized transport configuration

The effective npm registry was `https://registry.npmjs.org/`, `strict-ssl=true`, and `cafile=null`. In the initial Node 20 bridge-network container, npm `proxy` and `https-proxy` and the proxy/TLS environment variables were unset. The failed destination was `https://registry.npmjs.org/zustand/-/zustand-5.0.13.tgz`; the error was `ERR_SSL_WRONG_VERSION_NUMBER`.

Host Node/npm diagnostics: Node `v24.13.1`, npm `11.8.0`; the same registry and `strict-ssl=true`; npm proxy settings are configured (endpoint omitted); proxy environment variables are unset. `curl -Iv --max-time 15 https://registry.npmjs.org/` from the host completed with TLS 1.2 and HTTP/2 200. A Node 20 container using `--network host` returned npm `PING`/`PONG` in 766 ms. These checks show the registry is reachable on the host route, while Docker bridge routing is not reliable; they do not identify the network operator's underlying TLS/proxy mechanism.

I also repeated the Node 20/npm 10.2.4 install with the host's configured proxy explicitly passed as `npm_config_proxy` and `npm_config_https_proxy` (strict TLS verification remained enabled):

```text
npm ERR! code ETIMEDOUT
npm ERR! request to https://registry.npmjs.org/zustand/-/zustand-5.0.13.tgz failed
npm ERR! connect ETIMEDOUT <configured-proxy>:8080
Exited with code 1
```

The configured proxy endpoint is redacted; the container could not connect to it. This proxy-path timeout is distinct from the bridge-network TLS protocol error. Neither failure reached package resolution or lifecycle scripts. No TLS verification or registry security setting was disabled.

## React build result

```text
$ docker run --rm --network host \
  -v /home/navidrezadoost/Documents/Github/Logos-r1-pkg00-m0-followup:/workspace \
  -w /workspace/logos-app node:20.11.1-bookworm-slim npm run build
> npm run icons:build && tsc -b && vite build
icons:build: 23 toolbar and 5 system icons generated successfully
tsc -b: failed with 15 TypeScript errors; `vite build` was not reached
```

Error groups and locations:

- `src/components/prototype/PrototypePreview.tsx:31`, `src/plugins/api.ts:65`, and `src/utils/cssCodegen.ts:99`: `.color` read from the `SolidFill | GradientFill` union without narrowing.
- `src/render-webgpu/{composite-pipeline,glyph-atlas,gradient-atlas,tile-pipeline}.ts`: `GPUDevice.createSampler` absent from the checked-in minimal ambient declarations.
- `src/render-webgpu/{glyph-atlas,gradient-atlas}.ts`: `GPUQueue.writeTexture` absent from those declarations.
- `src/render-webgpu/llm-pipeline.ts`: missing `GPUBufferUsageFlags`, `GPUComputePipelineDescriptor`, and three `GPUQueue.onSubmittedWorkDone` declarations.
- `src/stores/aiStore.ts:318`: nullable `requestWebGPUDevice()` result dereferenced without a null check.

This was a real TypeScript build failure under the pinned install, not a download failure. The generated icon index formatting and `tsconfig.tsbuildinfo` created during the first build were restored/removed. A separate package-scoped correction branch, `r1/pkg00-react-build-fix` at `ae54bb91`, addresses these errors; its Node 20 `npm run build` (including `tsc -b` and Vite) passes. See `docs/r1/PKG-00-REACT-BUILD-FOLLOWUP.md` on that branch. This diagnostics branch remains documentation-only.

## Independent Cargo access check

```text
$ cargo test --workspace --locked --no-run
Downloading crates ...
warning: ... Timeout was reached (download of `khronos_api v3.1.0` failed to transfer more than 10 bytes in 30s)
warning: ... Timeout was reached (failed to download any data for `glam v0.24.2` within 30s)
... additional crates.io download timeouts ...
Command exceeded timeout of 150000 ms
```

The initial root-workspace Cargo command timed out before compilation. Additional transport checks from the host got TLS-verified HTTP 200 responses from `index.crates.io` and `static.crates.io`, and an HTTPS GET for `approx 0.5.1` returned bytes. Cargo itself still timed out fetching that locked crate, including with `CARGO_HTTP_TIMEOUT=120` and HTTP multiplexing disabled. Of the 48 registry archives in `rust/Cargo.lock`, 47 were already checksum-verified in cache; the missing `approx 0.5.1` was fetched from `static.crates.io` and SHA-256-checked against the lock. No lockfile changed.

Using that verified cache, the CI-equivalent domain-workspace command passed:

```text
$ cargo test --manifest-path rust/Cargo.toml --workspace --locked --offline
test result: 386 unit tests passed; 0 failed
doc-tests: 28 passed; 0 failed
```

This verifies the `rust/` domain workspace on Cargo/rustc 1.93.0. The root workspace (`cargo test --workspace --locked`, including `render-wasm`) remains unverified: its complete locked dependency cache was not restored; a root-cache HTTPS transfer for `anyhow 1.0.101` timed out part-way through. This is still a transport/cache blocker, not a Rust compilation result. GitHub-hosted CI status remains unavailable.

## Release artifact path

The React correction branch also resolved a CI output-directory mismatch: Vite writes `logos-app/build/`, while `logos-app CI` uploaded `logos-app/dist/` without syncing the build. The workflow now runs `scripts/sync-spa-to-dist.py` before upload. That sync created `dist/index.html` and `dist/assets/`, but reported `Missing workspace bundles in dist/js/. Ensure dist/js/main.js exists.` Therefore only the React SPA shell is evidenced; the full design workspace/WASM artifact remains incomplete. Separately, `.github/workflows/release.yml` still targets absent `logos-desktop` paths. The supported production frontend/server entrypoint remains unverified.

The temporary Node containers were removed. Both React and the `rust/` domain workspace now pass locally with existing locks; root Rust workspace CI and a complete release artifact remain unresolved. Do not alter dependencies, regenerate locks, or weaken TLS; M0 remains open.

No rollout approval is implied; M0 remains open.
