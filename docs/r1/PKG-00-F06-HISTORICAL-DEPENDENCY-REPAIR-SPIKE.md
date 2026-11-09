# R1-F06 — Historical Dependency Repair Spike

- **Result:** Outcome C for the complete historical producer; MCP sub-gate passed, frontend frozen install remained blocked
- **Historical source:** `665bb1ef13c3eeff2e84bf6e0d588a6bdecc6a92`
- **Execution:** Disposable detached worktree at `/tmp/opencode/logos-r1-f06`; no changes made to active `main` or the protected development worktree

## F06-A — Recovery and baseline

Created a new detached worktree at the exact authorized source SHA. It was clean at the start. Baseline hashes:

| File | SHA-256 before experiment |
|---|---|
| `mcp/packages/plugin/package.json` | `25e0cf6f12386f771b27e15bc1ef8d2cac920331ec7792daed97b7c5e59cf8a2` |
| `mcp/pnpm-lock.yaml` | `413cf2d38ddc1c924303a18e496308770faa990bbafc9d44fc987fbd4e2e142b` |
| `frontend/package.json` | `0f2b45ec4a32cd799e121ed0e363d4710372944f7ae1baadf080c9a2c47a90ae` |
| `frontend/pnpm-lock.yaml` | `6cb5d9729ae2a1d23111cfdd7c803d102ef06679da5baf5776d25f425949b807` |
| `plugins/libs/plugins-runtime/package.json` | `9e63c7fae9ee9338c732508f5d19b57528a9f37c75ddae09ef6b9fb97603ac17` |
| `plugins/pnpm-lock.yaml` | `f6efda97f4b5c1af4c31bfbb2f14787277ffd6fba775530c5af40180ecded595` |

## F06-B — MCP at historical `@penpot/*@1.4.1`

The isolated experiment changed only the two `mcp/packages/plugin/package.json` names from `@logos` to `@penpot`, keeping both versions at `1.4.1`. This matches the historical plugin source imports, the published tarballs, and the original lock entries. It did **not** substitute local Logos 1.4.2 packages for the MCP dependencies.

The first frozen install exposed another historical manifest/lock mismatch in local MCP workspace package names (`penpot-mcp`/`@penpot/mcp-common` in the lock versus `logos-mcp`/`@logos/mcp-common` in manifests). In the disposable copy, ran the pinned pnpm 10.28.2 lockfile-only command. Its entire lock diff changed only those two local package names; no package versions or integrity values changed. A subsequent frozen install succeeded:

```text
corepack pnpm install --recursive --lockfile-only --store-dir /cache/pnpm-store
exit: 0 (pnpm 10.28.2; two local importer/package-name corrections only)

corepack pnpm install --recursive --frozen-lockfile --store-dir /cache/pnpm-store
exit: 0; scope: all 4 workspace projects; 219 packages; pnpm 10.28.2
```

The containers used the public `node:22.22.0-bookworm-slim` image pinned by digest `sha256:dd9d21971ec4395903fa6143c2b9267d048ae01ca6d3ea96f16cb30df6187d94`, with a cleared environment, no credentials/database, and no published ports. The installed packages were confirmed as `@penpot/plugin-types@1.4.1` and `@penpot/plugin-styles@1.4.1`. Their lock/registry integrity values were retained and verified by the frozen install:

- types: `sha512-pHE2B3GI8M5JR03S/NdBoN+z6e1R1IEh3vpFbLG9LN0EZpQE6nEbmCo5jWAWI73Jqlg6CHG/RWVJNmWECnkDTA==`
- styles: `sha512-6TuJqKQsq1Xmhn2A02R+kCOzIzIdqgFg5z6ncLH2PlAflKIX6aYsGiOF7yFx4RYgCegRVMFPnVis6/hwO+YGQg==`

The install emitted pnpm's warning that install scripts for `esbuild` and `sharp` were ignored; no approval/allowlist was added. Despite that warning, both requested plugin checks passed without networking or starting a service:

```text
corepack pnpm --filter mcp-plugin types:check
exit: 0

corepack pnpm --filter mcp-plugin build:multi-user
exit: 0; Vite 7.3.1 emitted dist/index.html, index.js, plugin.js and CSS
```

This establishes a version-preserving MCP install and plugin build for the historical Penpot-named package contract. It does not establish current `@logos` publication or editor/server runtime compatibility.

## F06-C — Frontend `plugins-runtime` drift

**Explicit type-dependency decision for this local frontend workspace test:** restore the existing historical frontend lock relationship as a local build/workspace dependency:

```json
"@logos/plugin-types": "workspace:../plugin-types"
```

This is limited to `plugins-runtime@1.4.2` inside the historical frontend monorepo, whose lock already recorded that local edge and whose TypeScript path mapping resolves the historical `@penpot/plugin-types` source import to the local types file. It is not a substitute for MCP's published `@penpot/*@1.4.1` dependencies and does not prove a publishable `plugins-runtime` dependency contract.

Attempted to update the frontend lock in lockfile-only mode, using its pinned pnpm 10.28.2. Resolution stopped before writing the lockfile because the historical frontend directly references commit-pinned GitHub codeload tarballs that reset in this environment:

```text
corepack pnpm install --lockfile-only --store-dir /cache/pnpm-store
exit: 1

ECONNRESET https://codeload.github.com/penpot/svgo/tar.gz/8c9b0e32e9cb5f106085260bd9375f3c91a5010b
ECONNRESET https://codeload.github.com/penpot/draft-js/tar.gz/4a99b2a6020b2af97f6dc5fa1b4275ec16b559a0
```

The final failure was the `svgo` tarball after retries. The original `frontend/pnpm-lock.yaml` hash remains `6cb5d9729ae2a1d23111cfdd7c803d102ef06679da5baf5776d25f425949b807`; no frontend frozen install was run. Per the CTO stop condition, no alternate source, mirror, or retry was attempted.

## F06-D — Dependency graph review

- The MCP lock-only correction changed only two local package identifiers. The `@penpot/*@1.4.1` entries and all recorded package versions/integrities remained unchanged.
- MCP frozen install resolved the four locked workspace projects; its ignored `esbuild`/`sharp` install scripts did not prevent the targeted type check or multi-user plugin build.
- The frontend graph was not resolved, so no claim is made about selected versions for its six missing `plugins-runtime` devDependencies, transitive changes, or emitted declarations.
- The workspace type dependency decision above is local to the historical frontend tree. The MCP 1.4.1 pins were kept separate and were not replaced with local 1.4.2 packages.

## F06-E — Historical producer build

**Not attempted.** F06-C did not reach a frozen frontend install. The full producer build, Shadow-CLJS, Rust/WASM, workers, and complete editor artifact remain unverified. No server, database, listener, or editor runtime was started.

## F06-F — Isolation, patch and rollback

- Install/type/build commands ran in disposable Node 22.22.0 containers with a minimal environment, no production credentials/database, and no published ports. The plugin build ran with Docker `--network=none`.
- The only effective dependency changes in the temporary tree were: two MCP manifest scope corrections; two lockfile-only local package-name corrections; and the `plugins-runtime` local workspace type dependency above. No package version changed; no lock integrity was manually edited. Original hashes were `mcp/packages/plugin/package.json` `25e0cf6f…`, `mcp/pnpm-lock.yaml` `413cf2d3…`, `frontend/pnpm-lock.yaml` `6cb5d972…`, and `plugins-runtime/package.json` `9e63c7fa…`; the frontend lock remained unchanged.
- Patch captured at `/tmp/opencode/f06-logs/f06-reconciliation.patch` (SHA-256 `5a7328afbff46ac585977503058c09a29647c47eee0a6d4e5f0d55e1b7d343a5`). Evidence logs and SHA-256:

| Evidence log | SHA-256 | Exit |
|---|---|---:|
| `mcp-frozen-install.log` | `98bf3cafde123c8d599505c91f6b97322ccff7b4b727dd92c87735b7b161a8b2` | 1 |
| `mcp-lockfile-only.log` | `0718fd5c7e231826f223ade514b4326868ebe44d40fcb2a9f954f96bd3f8d096` | 0 |
| `mcp-frozen-install-corrected.log` | `d628e9b70eae33376a0cc20b5f7e233f3d637e45ebc2f7eaa4cb976d3f6f0209` | 0 |
| `mcp-types-check.log` | `4233eb36a5ba9da4d764e941365cf9b2650f0b9b87eb381b55aac70da98a83e9` | 0 |
| `mcp-plugin-build.log` | `ee23f5e9101a582362c7f98d7599bba4d652e8a65b7c906cead9fe30b581ad3a` | 0 |
| `frontend-lockfile-only.log` | `5470c0312512cd8fb72f1ba2e5a93d3ea17f664d7250665218f8a9cb69baded2` | 1 |

- After capturing the patch, restored the three edited tracked files in the disposable worktree to the historical baseline. Its tracked Git tree is clean again. Generated ignored `node_modules`/plugin build outputs remain only in the disposable worktree for evidence; no patch was committed or applied elsewhere.

## Exit decision

**Overall Outcome C:** the MCP dependency repair candidate is viable at frozen install, TypeScript, and plugin-build level, but the complete historical producer remains blocked at F06-C by unavailable pinned codeload dependencies. No full editor build can be claimed.

Stop here as authorized. A further attempt requires an approved, trusted way to resolve those exact commit-pinned tarballs. Do not retry through an alternate source, change dependency versions, proceed to the full build, or restore anything to the active application without separate approval. Preserve Option A; complete editor artifact and M0/M1 remain open.
