# PKG-00 — Reproducible Baseline Evidence

**Implementation status:** Implemented (baseline workflow and compatibility fixes)  
**Verification status:** Partial; Go is green, Rust and React remain blocked by local dependency/toolchain availability  
**Rollout approval:** Not approved / not applicable to CI-only changes  
**Base commit:** `ac57a34cd0b5869aaa9580c4773c79e908aa4a95`  
**Branch/worktree:** `r1/pkg00-baseline` — `/home/navidrezadoost/Documents/Github/Logos-r1-stabilization`  
**Report scope:** PKG-00 only. A31/A32 implementation is on separate R1 package branches.

## Provenance

- The approved baseline is the clean audit commit `ac57a34c`; the audit worktree remains unchanged as a reference.
- The original `Logos` development worktree and its pre-existing edits were not used or changed.
- No separate R0.3 diagnostic artifact was present in the audited `docs` directories. This report uses the starting blockers provided in the CTO directive and distinguishes reproduced results from that supplied list.
- No implementation/data migration, manifest/dependency requirement change, or repository move was performed. `rust/Cargo.lock` was generated offline for the existing `rust/` workspace because that workspace had no lockfile; it pins versions compatible with the current manifests and is checked with `--locked` in CI.

## Changed-file scope

- `.github/workflows/backend-go.yml` — actual Go module test/vet commands.
- `.github/workflows/logos-app.yml` — actual app build; removed nonexistent Vitest/ESLint commands; enabled `r1/**` pushes.
- `.github/workflows/rust.yml` — root renderer and separate Rust-domain workspace commands; removed nonexistent package/count assertions.
- `.gitignore` — allow the new `rust/Cargo.lock` while retaining the existing lockfile policy.
- `rust/Cargo.lock` — lock for the previously unpinned Rust subworkspace.
- `backend-go/internal/handler/{files_test.go,projects_test.go}` — pass the current optional storage dependency (`nil`) to `CreateProjectHandler` in two stale test calls.
- `backend-go/internal/handler/teams_test.go` — supply a test-only password for the supported profile schema's `NOT NULL` constraint.
- `backend-go/internal/handler/teams_test.go` — provide a test-only password value required by the full application schema's profile constraint.

No app source files remain modified by the attempted app build; generated icon indexes were restored to their baseline content.

## Reproduction matrix

Tool versions observed in this worktree: `go version go1.27.1-X:nodwarf5`, `rustc 1.93.0`, Node `v24.13.1`, npm `11.8.0`, pnpm `10.28.2`. This branch pins Go from `backend-go/go.mod`, Node 20 for the app and Rust 1.93.0; the separate PKG-02 branch pins pnpm 10.28.2 for MCP CI.

| Workspace | Command / setup | Result | Evidence / remaining blocker |
|---|---|---|---|
| Go backend — baseline | `go test ./...` | **Failed before fix** | Compile errors at `internal/handler/files_test.go:164` and `projects_test.go:52`: `CreateProjectHandler` requires `(*db.Pool, storage.Backend)`. |
| Go backend — after fix | `go test ./...` | **Pass** | All packages built/tested. `TEST_DATABASE_URL` was unset, so DB integration tests using that helper were skipped; the Go CI job does not claim database integration coverage. |
| Go backend — static check | `go vet ./...` | **Pass** | Exit 0, no diagnostics. |
| Go file/share integration — schema clone | `go test ./internal/handler -run 'Test(CreateFileHandler_Integration\|GetFileHandler_Integration\|CreateShareLinkHandler_Integration)$' -count=1 -v` | **Pass** | The create, get and share-link integration tests passed against a data-free schema clone of local `logos` (Postgres 17.2) after applying repo migration `0001`. This run used the PKG-00 test fixes in this worktree; no overlay. The source schema has not been certified as canonical. |
| Rust root workspace (`render-wasm`) | `cargo test --workspace --locked --no-run --offline` | **Blocked** | Local Cargo cache lacks `glam v0.24.2`; an earlier online attempt timed out while fetching crates. |
| Rust domain workspace | `cargo metadata --manifest-path rust/Cargo.toml --locked --no-deps --format-version 1` | **Pass** | All nine declared workspace members resolve from the manifests and new `rust/Cargo.lock` in this worktree. |
| Rust domain workspace | `cargo test --manifest-path rust/Cargo.toml --workspace --locked --no-run --offline` | **Blocked** | Local Cargo cache lacks `approx v0.5.1`; no tests were reported as passing. |
| Rust root formatting | `cargo fmt --all -- --check` | **Pass** | No formatting differences. |
| Rust domain formatting | `cargo fmt --manifest-path rust/Cargo.toml --all -- --check` | **Fail — pre-existing drift** | Reports formatting differences in existing `logos-vector`, `logos-vector-ops`, and `logos-vector-wasm` source files. No source formatting was changed in PKG-00. This check is not represented as passing in the workflow. |
| React app | `npm run typecheck` | **Invalid command** | `logos-app/package.json` has no `typecheck` script. |
| React app | `npm run build` | **Blocked** | SVG generation completed, then shell failed with `tsc: command not found`; the clean worktree had no `node_modules`. The generated index-only changes from this attempt were restored. No app tests/spec files or Vitest dependency exist in this baseline. |
| Release packaging | Existing `.github/workflows/release.yml` | **Unresolved blocker** | It targets absent `logos-desktop` paths. The production release entrypoint/artifact is not established by this task, so the workflow was not rewritten to invent a release target. |

The targeted full-schema file/share run produced:

```text
=== RUN   TestCreateFileHandler_Integration
--- PASS: TestCreateFileHandler_Integration (0.01s)
=== RUN   TestGetFileHandler_Integration
--- PASS: TestGetFileHandler_Integration (0.01s)
=== RUN   TestCreateShareLinkHandler_Integration
--- PASS: TestCreateShareLinkHandler_Integration (0.01s)
PASS
ok   github.com/logos-design/logos/backend-go/internal/handler 0.035s
```

The original R0.3 diagnostics also cited unavailable Rust/Node caches. The local results above reproduce that limitation. No build is described as healthy or production-ready on the basis of these partial checks.

## Workflow changes

- Go CI runs `go mod download`, `go test ./...`, and `go vet ./...` from `backend-go/` using the module's declared Go version.
- React CI runs `npm ci` and the manifest's real `npm run build`, which includes TypeScript project checking. The nonexistent Vitest and ESLint commands were removed; this is not a claim that app unit tests exist.
- Rust CI tests the actual root renderer workspace and `rust/` workspace separately with `--locked`, and checks the renderer WASM target. It no longer references absent package names or test-count thresholds.
- Release packaging remains explicitly blocked pending confirmation of the supported product artifact and entrypoint.
- PyYAML parsed the modified Go/app/Rust workflow YAML successfully. This is syntax validation, not a GitHub Actions execution.

## Three-state package status

| State | PKG-00 status |
|---|---|
| **Implemented** | Yes — CI targets and the two stale Go test calls are corrected; separate Rust workspace lock is present. |
| **Verified** | Partial — Go test/vet and metadata/root formatting pass. Rust compilation and React build did not complete because dependencies are unavailable; Rust-domain formatting drift and release-target mismatch remain. |
| **Rollout-approved** | No. PKG-00 makes no product rollout or user-data write decision. |

The next verification should run these workflow changes from a clean checkout with CI dependency access and a separately provisioned database test environment. Runtime/deployment verification remains outstanding.
