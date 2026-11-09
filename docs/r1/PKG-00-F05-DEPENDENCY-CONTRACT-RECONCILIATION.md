# R1-F05 — Dependency Contract Reconciliation Proposal

- **Status:** Read-only proposal; no manifests, lockfiles, package sources, or toolchains changed
- **Current baseline:** `c8669d9e64502822989111c490a8f189cb0694a7`
- **Historical recovery target:** `665bb1ef13c3eeff2e84bf6e0d588a6bdecc6a92`

## Executive finding

There is no safe lockfile-only repair for the complete historical producer yet. Two different conditions are involved:

1. The historical frontend lock importer is stale relative to `plugins-runtime/package.json`, and the package's type dependency/import contract is itself ambiguous.
2. The historical MCP manifest, source imports, and lockfile use different package scopes. Public npm currently has `@penpot/*@1.4.1`, but not `@logos/plugin-types` or `@logos/plugin-styles` at `1.4.1` or `1.4.2`. The lock entries renamed to `@logos` retain the integrity hashes of the published `@penpot` tarballs.

The minimum version-preserving candidate for the exact historical snapshot is to restore MCP manifest names to the `@penpot/*@1.4.1` packages that the historical source imports and the existing lock records, subject to package-owner approval of that scope. The alternative is an explicit publisher decision to make authoritative `@logos` artifacts available. Neither option is implemented or approved. For the frontend importer, the package owner must first decide whether `plugin-types` is a required published dependency, a peer, or only a workspace build dependency; regenerating its lock before that decision could hide a type/API defect.

This is not authorization to reconcile dependencies, rebuild, restore the frontend, or change the product route. The accepted Option A compatibility direction remains unchanged.

## F05-A — Frontend manifest and lock importer

The frontend source and lockfile are recoverable at `665bb1e`, but `frontend/` is absent from current `origin/main` following transition commit `ea3b966b`. The evidence below therefore refers to the historical snapshot and Git history, not a currently active frontend workspace.

| Evidence | Historical state |
|---|---|
| `plugins/libs/plugins-runtime/package.json` at `665bb1e` | `@logos/plugins-runtime@1.4.2`; runtime dependencies are `ses` and `zod`; six devDependencies are declared: Vite, `vite-plugin-dts`, `vite-plugin-static-copy`, `vite-tsconfig-paths`, `vite-plugin-checker`, and TypeScript. |
| `frontend/pnpm-lock.yaml` importer `../plugins/libs/plugins-runtime` | Still lists `@logos/plugin-types` as `workspace:../plugin-types`, `ses`, and `zod`; it lacks those six devDependency specifiers. |
| `plugins/libs/plugins-runtime` source at `665bb1e` | TypeScript imports `@penpot/plugin-types`; `plugins/tsconfig.base.json` maps that name to `libs/plugin-types/index.d.ts`. The local types package is named `@logos/plugin-types@1.4.2`. |

### Historical origin

- `3684f391` added the frontend workspace link to `plugins-runtime`, added `@logos/plugin-types` as a `workspace:../plugin-types` dependency, and updated the frontend lock importer accordingly.
- `aed3bba2` removed that dependency from the package manifest and added the six build devDependencies, but did not update the frontend importer. This produced the exact F04 frozen-lockfile error.
- The separate `plugins/pnpm-lock.yaml` at current `origin/main` still has an `@logos/plugin-types@^1.4.2` dependency under `libs/plugins-runtime`, while the current package manifest has no such dependency and does declare the six devDependencies. This is the same unresolved manifest/lock drift in the plugin workspace; it was not installed or tested as part of F05.
- The adjacent `plugins/libs/plugins-runtime/package-lock.json` is older metadata: it records package version `0.12.0` and a `@logos/plugin-types@^0.12.0` npm dependency, while the current manifest is version `1.4.2`. It is not the pnpm lock used by the monorepo, but it is not trustworthy evidence for the current package contract.

The source also means that “make the importer match the current manifest” is not automatically a safe solution. Removing the importer edge may satisfy pnpm metadata while leaving package declarations dependent on `plugin-types`; retaining it requires an explicit manifest dependency class and a consistent source import/type-resolution contract. The source-only evidence does not establish whether the published runtime should use a runtime dependency, peer dependency, or workspace-only type dependency.

### Version-preserving frontend proposal

Before any lock change, the package/release owner should decide and document the `plugin-types` contract. A candidate for the monorepo is the previously used local workspace relationship to the same `@logos/plugin-types@1.4.2` source, together with a published-package dependency policy that preserves the emitted declaration imports. Then reconcile the frontend and `plugins` lock importers with the manifest using the already pinned pnpm version for each workspace, and review the full resolution diff. Do not assume the lock-only correction is graph-neutral: it adds six dev dependencies, removes/adds a workspace edge depending on the chosen contract, and may select packages not currently present in that lock.

## F05-B — MCP package names, versions, imports, and registry

| Surface | At historical SHA `665bb1e` | Current `origin/main` at `c8669d9e` |
|---|---|---|
| MCP plugin manifest | Depends on `@logos/plugin-types@1.4.1` and `@logos/plugin-styles@1.4.1`. | Same declared package names and versions. |
| MCP plugin imports | `LogosUtils.ts` imports from `@penpot/plugin-types`; `style.css` imports `@penpot/plugin-styles/styles.css`. | Imports were renamed to `@logos/*` in later source history. |
| MCP lock importer | Uses `@penpot/*@1.4.1`. | Uses `@logos/*@1.4.1`. |
| Local library sources | `plugins/libs/plugin-types`, `plugins-styles`, and `plugins-runtime` are named `@logos/*` at `1.4.2`. | Same names/versions; `plugins-runtime` source now imports the `Logos` API type. |

The scope transition is traceable:

- `ddf7061d` (Feb 12, 2026) changed package manifests from `@penpot` to `@logos`, including MCP's exact `1.4.1` requirements and local library manifests at `1.4.2`. Some source imports and docs still used `@penpot` at the historical F04 SHA.
- `a1278a33` (Jun 17, 2026) changed MCP source imports and lock package keys to `@logos`, retaining `1.4.1` and copying the same integrity strings from the earlier `@penpot` lock entries. It did not establish that corresponding `@logos` tarballs had been published.
- `plugins/docs/publish-package.md` describes authenticated manual npm publication. `plugins/tools/scripts/publish.ts` builds the local packages and publishes them publicly; it increments a shared package version (or accepts an exact version). No package publication was performed or evidenced by this assessment.

Read-only public npm metadata checks on Nov 9, 2026 returned:

- [`@logos/plugin-types@1.4.1`](https://registry.npmjs.org/%40logos%2fplugin-types/1.4.1), [`@logos/plugin-styles@1.4.1`](https://registry.npmjs.org/%40logos%2fplugin-styles/1.4.1), and both `@logos/*@1.4.2` endpoints: **404**.
- [`@penpot/plugin-types@1.4.1`](https://registry.npmjs.org/%40penpot%2fplugin-types/1.4.1) and [`@penpot/plugin-styles@1.4.1`](https://registry.npmjs.org/%40penpot%2fplugin-styles/1.4.1): **published**. Their registry `dist.integrity` values exactly match the hashes retained under the renamed `@logos/*@1.4.1` lock entries:
  - types: `sha512-pHE2B3GI8M5JR03S/NdBoN+z6e1R1IEh3vpFbLG9LN0EZpQE6nEbmCo5jWAWI73Jqlg6CHG/RWVJNmWECnkDTA==`
  - styles: `sha512-6TuJqKQsq1Xmhn2A02R+kCOzIzIdqgFg5z6ncLH2PlAflKIX6aYsGiOF7yFx4RYgCegRVMFPnVis6/hwO+YGQg==`
- The published `@penpot/plugin-types@1.4.1` declaration exports `Penpot`, `Board`, `Fill`, `FlexLayout`, `GridLayout`, `Page`, and `Shape`, matching the historical MCP imports. It does not export the later `Logos` API type used by current `plugins-runtime` source.

This establishes that the `@logos` lock-key rename is not proof of an `@logos` registry package. Reusing the old integrity alongside an unavailable package name cannot make a frozen install work. Conversely, simply reverting current source imports to `@penpot` would be an API/branding change and is not approved.

### MCP resolution choices — not implemented

| Choice | Version impact | Benefit | Material risk / decision required |
|---|---|---|---|
| **Manifest correction for the exact historical snapshot:** use `@penpot/*@1.4.1`, matching its existing imports, lock entries and published artifacts. | No version change; likely no MCP lock graph change. | Smallest technically coherent repair for SHA `665bb1e`. | Requires explicit approval to retain the Penpot package scope. It does not directly fit the later `@logos` source state. |
| **Preserve the `@logos` public contract:** publish/restore authoritative `@logos/*@1.4.1` artifacts, then lock their actual registry integrity. | Keeps declared `1.4.1`; registry content/hash must be confirmed. | Aligns package name with the Logos source/API. | Publisher/scope ownership and authoritative `1.4.1` source are unresolved; npm versions are immutable. The local `1.4.2` source is not proof of the `1.4.1` artifact. Publishing is an external, irreversible action. |
| **Manifest alias to the published `@penpot/*@1.4.1` packages.** | Keeps `1.4.1`; changes manifest resolution specifier. | Could let old `@logos` imports resolve to verified `@penpot` tarballs without changing source. | Must be tested for package-name metadata, bundler/type declaration behavior, and current `Logos`-specific types; the historical `Penpot` API evidence does not validate current `Logos` runtime declarations. Requires package-owner approval. |
| **Workspace-resolution change to local `plugins/libs/*`.** | Local sources are `1.4.2`, not the MCP pin `1.4.1`. | Avoids unavailable registry artifacts for an isolated source build. | Changes the dependency model and version graph; not a version-preserving fix and not authorized. |

For the exact F04 source, the leading candidate is the historical `@penpot` manifest correction because its source imports, lockfile, public artifacts, versions, and integrity hashes agree. For current Logos-branded source, package-owner resolution of the `@logos` publication contract is required. Do not mechanically port the later lock rename as a repair.

## F05-C — Historical toolchain proposal

The historical source pins useful pieces, but not a complete reproducible environment:

| Component | Historical declaration/evidence | Reconciliation needed |
|---|---|---|
| Node/pnpm | Node `22.22.0`; frontend/MCP pin pnpm `10.28.2` with integrity. `plugins` pins pnpm `10.26.2`; `render-wasm` pins `10.28.0`. | These workspaces declare three distinct pnpm versions; preserve each pin or explicitly justify consolidation. The Node tarball download has no checksum in the Dockerfile. |
| Java/Clojure | JDK 25.0.2 has a SHA-256 check; Clojure CLI `1.12.4.1602`; Clojure `1.12.2`; Shadow-CLJS `3.2.2`. | The downloaded Clojure install script is not checksum-verified. `deps.edn` has direct Maven versions and abbreviated Git SHAs, but no resolved dependency lock. |
| Rust/WASM | Rust `1.91.0`, rustup `1.28.2` with checksum, Emscripten `4.0.6`, `Cargo.lock`; target `wasm32-unknown-emscripten`. | `emsdk` is cloned from the default GitHub branch without a commit pin; `cargo-watch` is installed unversioned. The Skia binary URL is version/revision-named but has no checksum verification in `_build_env`. |
| Other image/tool downloads | Ubuntu `24.04`, tagged ImageMagick image, versioned Caddy/clj-kondo/etc. download URLs. | Base/image tags are not digest-pinned; `apt-get update/upgrade` is mutable; several GitHub release downloads have no checksum. F04 recorded reset GitHub downloads while trying to construct this environment. |
| Artifact identity | Build script injects a timestamp into `VERSION_TAG` and rewrites worker/render URLs. | Record source/toolchain/output hashes; byte-identical output is not established without normalizing build-time metadata. |

Smallest defensible environment proposal: preserve the historical versions; pin base and dependent images by digest; verify SHA-256 for downloaded tools and Skia assets; pin the emsdk commit and OS package snapshot; preserve separate pnpm pins unless an approved change consolidates them; and provide a resolved/mirrored Clojure dependency graph. No toolchain or build was run for F05.

## F05-D — Dependency graph and rollback risk

- A lock-only frontend update can change the dependency graph even without editing version ranges: the six dev dependencies need concrete resolutions, and the existing type-package edge may be removed. Review exact package/version/integrity changes, not only the importer diff.
- The current MCP lock uses `@logos` keys with hashes copied from `@penpot` registry artifacts; a future lock must be generated from the actual chosen package source, not renamed by string substitution.
- Manifest correction changes published package contracts and potentially declaration consumers. Publication is not reversible by overwriting a version; at most it can be deprecated, so publication requires a separate release approval.
- Workspace resolution couples the MCP package to a local `1.4.2` source tree and changes the frozen `1.4.1` graph. It requires an explicit dependency-model decision.
- Any later approved implementation should be isolated from `main` and protected worktrees, retain the original manifests/lockfiles, produce a reviewed lock diff with no unapproved version changes, and be revertible by restoring the exact baseline. Do not update or publish locks/packages during this assessment.

## F05-E — Product cost/risk comparison

| Direction | Near-term work and risk | Longer-term tradeoff |
|---|---|---|
| **Preserve Option A** | First settle package scope/ownership, align the workspace dependency declaration, and establish the pinned toolchain; then obtain separate approval for a complete build/runtime spike. Build and plugin compatibility remain unverified. | Best preserves current routes, existing document access and plugin expectations, but retains ClojureScript/WASM/toolchain maintenance and nREPL isolation requirements. |
| **Controlled replacement/retirement** | Requires a product capability gap analysis, routes/deep links, document import/export and plugin migration plan. Current React `WorkspacePage` is not routed and has no accepted feature/runtime parity evidence. | Could reduce legacy stack burden after migration, but carries higher immediate compatibility/user disruption risk and cannot be treated as a simple route change. |

No quantitative estimate is supported by this source-only review. Recommendation remains **preserve the compatibility contract**; do not restore or retire until a package-owner decision and a separately approved complete-artifact feasibility test resolve the above risks.

## Exit status and authorization boundary

- F05-A/B/C/D/E: **Assessment complete; proposal only**.
- Frozen installation/build: **Not run** (explicitly outside this read-only assignment).
- Lockfile regeneration, manifest/source edits, package publication, workspace changes: **Not performed**.
- Complete editor, runtime, root Rust, hosted CI, schema provenance, and production exposure gates: **remain open**.
- M0/M1: **remain open**.
