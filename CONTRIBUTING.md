# Contributing to Logos

Thank you for your interest in contributing to Logos Community Edition.
This guide covers everything you need to get started — from setting up your
environment to submitting your first pull request.

For architecture deep-dives see [`ARCHITECTURE.md`](./ARCHITECTURE.md) and the
[Developer Guide](./docs/technical-guide/developer/).

---

## Table of Contents

- [Tech Stack Overview](#tech-stack-overview)
- [Prerequisites](#prerequisites)
- [Setting Up the Development Environment](#setting-up-the-development-environment)
- [Running Tests](#running-tests)
- [Code Style and Linting](#code-style-and-linting)
- [Commit Guidelines](#commit-guidelines)
- [Pull Request Process](#pull-request-process)
- [Reporting Bugs](#reporting-bugs)
- [Release Process](#release-process)
- [Developer's Certificate of Origin](#developers-certificate-of-origin)

---

## Tech Stack Overview

Logos is a multi-language monorepo. You only need the toolchain for the layer you are changing:

| Directory | Language | Role | Toolchain |
|---|---|---|---|
| `logos-app/` | TypeScript / React 19 | Frontend SPA, plugin sandbox | Node.js 20 LTS |
| `backend-go/` | Go 1.22+ | HTTP/WS API, auth, DB, file export | Go toolchain |
| `rust/` | Rust (stable) | Core types, layout, vector, CRDT rebase, WASM renderer | rustup |
| `mcp/` | TypeScript | Model Context Protocol server + Logos plugin bridge | Node.js 20 LTS |
| `plugins/` | TypeScript | Plugin SDK (`@logos/plugin-types`), runtime, example apps | Node.js 20 LTS |

> There is **no Clojure**, no JVM, and no ClojureScript in the repository.
> The full migration to Go + TypeScript + Rust was completed in Phase G5.

---

## Prerequisites

### Go (backend)

```bash
# Download from https://go.dev/dl/ or use your package manager
go version   # 1.22+
```

### Node.js (frontend, MCP, plugins)

```bash
# Use nvm or your package manager — LTS 20 or 22
node --version   # v20+
npm --version    # 10+
```

### Rust (core engine + WASM)

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup update stable
rustup target add wasm32-unknown-unknown   # for WASM builds
cargo install wasm-pack                    # for render-wasm
```

### Database and cache (for backend integration tests)

```bash
# PostgreSQL 14+ and Redis/Valkey 7+
# The docker/devenv/ compose file starts both automatically.
```

---

## Setting Up the Development Environment

### Option A — Docker devenv (recommended)

The devenv container bundles PostgreSQL, Valkey, MailCatcher, and all toolchains:

```bash
# Start the full development stack
./manage.sh run-devenv

# Useful subcommands
./manage.sh build-devenv-local   # build the devenv image locally
./manage.sh start-devenv         # start containers in the background
./manage.sh stop-devenv          # stop containers
./manage.sh drop-devenv          # destroy containers + volumes
```

Once running, browse to **http://localhost:3449** for the application.

### Option B — Native setup (fast iteration on a single layer)

**Backend only:**

```bash
# Start PostgreSQL + Valkey however you like, then:
export DATABASE_URL="postgres://logos:logos@localhost:5432/logos"
export REDIS_URL="redis://localhost:6379"
export LOGOS_SECRET_KEY="dev-secret-key-at-least-32-chars"

cd backend-go
go run ./cmd/server
```

**Frontend only (proxies API to a running backend):**

```bash
cd logos-app
npm ci
npm run dev   # http://localhost:3449
```

**Rust crates:**

```bash
cd rust
cargo build --workspace
```

---

## Running Tests

### Backend (Go)

```bash
cd backend-go

# Unit + integration tests (integration tests skip without TEST_DATABASE_URL)
go test ./...

# With integration tests
export TEST_DATABASE_URL="postgres://logos:logos@localhost:5432/logos_test"
go test ./... -count=1

# Specific package
go test ./internal/rebase/...    # OT rebase engine (20 tests)
go test ./internal/binfile/...   # .logos file format
go test ./internal/handler/...   # HTTP handlers
```

### Frontend (TypeScript)

```bash
cd logos-app
npx tsc --noEmit        # type-check (zero errors required)
npx vitest run          # unit tests
npx vitest              # watch mode
```

### Rust

```bash
cd rust
cargo test --workspace                # all crates
cargo test -p logos-types             # type codegen
cargo test -p logos-layout            # layout engine
cargo test -p logos-rebase            # OT rebase

# Benchmarks
cargo bench -p logos-layout
```

### All at once (what CI runs)

```bash
./run-ci.sh
```

---

## Code Style and Linting

### Go

```bash
cd backend-go
go fmt ./...
go vet ./...
```

- Follow standard Go idioms (`gofmt`, package-level error types, table-driven tests)
- No `panic` in library code; return errors
- Keep handlers thin — business logic in `internal/` packages

### TypeScript

```bash
cd logos-app          # or mcp/, plugins/
npx tsc --noEmit      # must pass with zero errors
npx eslint src/       # follow existing ESLint config
```

### Rust

```bash
cd rust
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
```

- Use `thiserror` for library errors, `anyhow` only in binaries
- Document public APIs with `///` doc comments
- Prefer `#[must_use]` on functions that return important values

---

## Commit Guidelines

We follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <short summary>

[optional body]

[optional footer: Signed-off-by: ...]
```

### Types

| Type | When to use |
|---|---|
| `feat` | New feature or capability |
| `fix` | Bug fix |
| `perf` | Performance improvement |
| `refactor` | Code restructuring without behavior change |
| `docs` | Documentation only |
| `test` | Adding or updating tests |
| `ci` | CI/CD configuration changes |
| `chore` | Build, deps, tooling |

### Scopes

Use the relevant directory or subsystem name:

| Scope | Covers |
|---|---|
| `backend` | `backend-go/` |
| `frontend` | `logos-app/` |
| `binfile` | `.logos` file format |
| `auth` | Authentication and sessions |
| `rebase` | OT rebase engine |
| `handler` | RPC command handlers |
| `rust` | Any Rust crate |
| `layout` | `logos-layout` crate |
| `render` | `render-wasm` |
| `types` | `logos-types` + codegen |
| `mcp` | MCP server |
| `plugins` | Plugin SDK |
| `devenv` | Development environment |
| `ci` | GitHub Actions workflows |
| `docs` | Documentation |

### Examples

```
feat(auth): add magic-link login flow
fix(binfile): accept legacy penpot/export-files manifest type
perf(rebase): skip no-op set-ops during merge
docs(backend): document all 25 RPC handler endpoints
test(handler): add integration tests for file export
```

---

## Pull Request Process

**Before starting**, search open issues and PRs to avoid duplicate work.

**For bug fixes** — open a PR directly; file an issue too so we can track it.

**For new features** — open a Discussion or Feature Request issue first.
No feature PR will be accepted without prior design discussion.

**For documentation** — PRs welcome without an issue.

### PR Checklist

- [ ] `go vet ./...` passes (backend)
- [ ] `npx tsc --noEmit` passes (frontend / MCP / plugins)
- [ ] `cargo clippy --workspace -- -D warnings` passes (Rust)
- [ ] New functionality has tests
- [ ] Public APIs have doc comments
- [ ] `CHANGELOG.md` updated if the change is user-visible
- [ ] PR description explains *why* the change is needed, not just what it does

### Review process

1. At least one maintainer approval required
2. CI must be green (build + test + lint)
3. Squash-merge into `main`

---

## Reporting Bugs

Use [GitHub Issues](https://github.com/navidrezadoost/Logos/issues) with the structured templates.

Before filing:
1. Search existing issues to avoid duplicates
2. Include Logos version, OS, browser (for frontend issues), and reproduction steps

For security vulnerabilities see [SECURITY.md](./SECURITY.md) — report privately, never as a public issue.

---

## Release Process

1. All changes land on `main` via PR (CI must be green)
2. Create a release branch: `release/vX.Y.Z`
3. Update versions:
   - `backend-go/go.mod` module path version (if breaking)
   - `rust/Cargo.toml` workspace version
   - `logos-app/package.json`
4. Update `CHANGELOG.md` with the new version section
5. Open PR to `main`, merge after review
6. Tag: `git tag -a vX.Y.Z -m "Release vX.Y.Z"`
7. Push: `git push origin main --tags`
8. GitHub Actions creates the release artifacts

### Version Scheme

```
vMAJOR.MINOR.PATCH[-rc.N]
```

- Major: breaking API or file-format changes
- Minor: new features, backward-compatible
- Patch: bug fixes

---

## Developer's Certificate of Origin

By submitting code you certify the following (DCO 1.1):

> (a) The contribution was created in whole or in part by me and I have the right to
>     submit it under the open-source license indicated in the file; or
>
> (b) The contribution is based upon previous work that, to the best of my knowledge,
>     is covered under an appropriate open-source license and I have the right under
>     that license to submit that work with modifications under the same open-source
>     license, as indicated in the file; or
>
> (c) The contribution was provided directly to me by some other person who certified
>     (a), (b), or (c) and I have not modified it.
>
> (d) I understand that this project and the contribution are public and that a record
>     of the contribution is maintained indefinitely and may be redistributed consistent
>     with this project or the open-source license(s) involved.

Add a sign-off to every commit (documentation excluded):

```bash
git commit -s -m "feat(scope): summary"
# Adds: Signed-off-by: Your Name <you@example.com>
```

Use your real name — anonymous contributions are not accepted.

---

## Code of Conduct

See [CODE_OF_CONDUCT.md](./CODE_OF_CONDUCT.md). Participation in this project requires
adherence to it. Instances of unacceptable behavior may be reported at `conduct@logos.app`.
