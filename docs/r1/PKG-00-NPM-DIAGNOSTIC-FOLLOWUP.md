# R1 PKG-00 — Node 20 npm Install Diagnostic

**Status:** Pinned Node 20 install succeeds on host network; React build fails at TypeScript; M0 remains open
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

This is a real TypeScript build failure under the pinned install, not a download failure. The generated icon index formatting and `tsconfig.tsbuildinfo` created during the build were restored/removed; no source or lockfile changes are part of this documentation branch. A separate, package-scoped correction branch is needed before rerunning the build.

## Independent Cargo access check

```text
$ cargo test --workspace --locked --no-run
Downloading crates ...
warning: ... Timeout was reached (download of `khronos_api v3.1.0` failed to transfer more than 10 bytes in 30s)
warning: ... Timeout was reached (failed to download any data for `glam v0.24.2` within 30s)
... additional crates.io download timeouts ...
Command exceeded timeout of 150000 ms
```

Cargo dependency access also timed out before compilation; this is independent of the npm TLS error and is not a Rust compile failure. `--locked` was used, and no lockfile changes were made. Re-run in a CI environment with approved crates.io access before classifying Rust build health.

The temporary containers were removed. The pinned install succeeded through host networking, but React type-check/build remains failed until the listed source/declaration errors are corrected. Cargo still has independent registry timeouts. Do not alter dependencies, regenerate locks, or weaken TLS; M0 remains open pending successful React and Rust builds and release-target verification.

No rollout approval is implied; M0 remains open.
