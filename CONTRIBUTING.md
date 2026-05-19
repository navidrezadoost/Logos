# Contributing to Logos

Thank you for your interest in contributing to Logos! This guide covers
everything you need to get started — from building the project to
submitting your first pull request.

For architecture deep-dives, see the `docs/` directory or the
rendered documentation at the [Help Center](https://docs.logos.app/).

## Architecture Overview — 19 Crates

Logos is a Rust workspace with 19 crates organized into 7 layers:

### Core Engine
| Crate | Purpose | Tests |
|-------|---------|-------|
| `logos-core` | CRDT document model, layer operations, batch API | 47 |
| `logos-layout` | Taffy constraint layout, spatial hash, hit testing | 59 |
| `logos-render` | wgpu GPU pipeline, instance batching, frame cache | 47 |
| `logos-text` | cosmic-text shaping, glyph atlas, typography | 48 |

### Collaboration
| Crate | Purpose | Tests |
|-------|---------|-------|
| `logos-collab` | WebSocket server, CRDT sync, RocksDB, presence, JWT auth | 213 |

### Platform Targets
| Crate | Purpose | Tests |
|-------|---------|-------|
| `logos-desktop` | winit 0.30 + wgpu 24, UI modules (commands, panels, tabs) | 212 |
| `logos-wasm` | WebGPU via wasm-bindgen, 23 JS-exported methods | 28 |

### Extensibility
| Crate | Purpose | Tests |
|-------|---------|-------|
| `logos-plugins` | Dual JS/WASM runtime, 21 host functions, TOML manifests | 596 |

### AI
| Crate | Purpose | Tests |
|-------|---------|-------|
| `logos-ai` | ONNX Runtime inference, quantized models, embeddings | 235 |

### File Import (7 crates)
| Crate | Purpose | Tests |
|-------|---------|-------|
| `logos-import-common` | Shared `ImportDocument` trait and types | — |
| `logos-import-figma` | .fig binary parser (20 node types) | 9 |
| `logos-import-svg` | Dependency-free XML + path parser | 17 |
| `logos-import-sketch` | ZIP + JSON model mapping | 10 |
| `logos-import-pdf` | Content stream tokenizer | 13 |
| `logos-import-xd` | ZIP/AGC extraction | — |
| `logos-import-canva` | JSON template parser | — |

### Marketplace (3 crates)
| Crate | Purpose | Tests |
|-------|---------|-------|
| `logos-marketplace-auth` | Ed25519 keypairs, JWT sessions | 32 |
| `logos-marketplace-db` | PostgreSQL schema (7 tables) | 24 |
| `logos-marketplace-api` | REST server with 18+ routes | 44 |

**Total: 2,007 tests across 92,201 lines of Rust.**

## Multi-Language Architecture

Logos uses three primary languages.  You only need the toolchain for the
layer you're changing:

| Layer | Language | Toolchain needed |
|---|---|---|
| `logos-app/` — Frontend SPA | TypeScript | Node.js 20+ + npm |
| `backend/` — API server | Clojure (JVM) | OpenJDK 21 + Clojure CLI |
| `common/` — Shared schemas | Clojure (JVM) | OpenJDK 21 + Clojure CLI |
| Rust crates | Rust | rustup (see below) |

> **No ClojureScript toolchain required.**  The TypeScript frontend `logos-app/` does
> not use shadow-cljs, leiningen, or any ClojureScript compiler.  The `frontend/`
> directory (legacy Penpot-compatible ClojureScript) is retained for the exporter and
> is built via shadow-cljs only when running the full Penpot stack.

### Schema Workflow

`common/src/app/common/types/` contains [Malli](https://github.com/metosin/malli)
schema definitions.  They are the single source of truth for shared data types.

- **Clojure backend** — consumes schemas natively via `(require '[app.common.types.shape ...])`.
- **TypeScript frontend** — consumes auto-generated `.d.ts` files in
  `logos-app/src/types/generated/`.

**If you change a Malli schema, you must regenerate the TypeScript types:**

```bash
# Requires Babashka (https://babashka.org)
bin/generate-types

# Verify no drift (same check the CI runs)
bin/generate-types --check
```

Commit both the schema change and the regenerated `.d.ts` files in the same PR.
The `generated-types-drift` CI job will fail if they are out of sync.

### Frontend (TypeScript) Setup

```bash
cd logos-app
npm ci
npx tsc --noEmit   # type-check
npx vitest run     # unit tests
npm run dev        # dev server at http://localhost:5173
```

### Backend (Clojure) Setup

```bash
# From repo root — starts backend + REPL
cd backend
clj -M:dev

# Run backend tests
clj -M:test
```

## Prerequisites

- **Rust** 1.75+ (stable) — install via [rustup](https://rustup.rs/)
- **System dependencies:**
  - Linux: `sudo apt-get install libclang-dev libfontconfig1-dev`
  - macOS: `brew install llvm fontconfig`
  - Windows: Install Visual Studio Build Tools + LLVM
- **WASM target** (optional): `rustup target add wasm32-unknown-unknown`

## Building

```bash
# Clone the repository
git clone https://github.com/navidrezadoost/Logos.git
cd Logos

# Build the entire workspace (19 crates)
cargo build --workspace

# Build a specific crate
cargo build -p logos-desktop
cargo build -p logos-core

# Build the WASM web target
cargo build --target wasm32-unknown-unknown -p logos-wasm

# Release build
cargo build --workspace --release
```

## Testing

```bash
# Run all workspace tests (2,007 tests)
cargo test --workspace

# Run tests for a specific crate
cargo test -p logos-core
cargo test -p logos-plugins
cargo test -p logos-desktop

# Run tests with output
cargo test -p logos-core -- --nocapture

# Run a specific test
cargo test -p logos-plugins -- wasm_runtime::tests::test_fuel_limit

# Skip GPU-dependent tests (if no GPU available)
cargo test --workspace -- \
  --skip "headless" --skip "surface" \
  --skip "prepare_uploads" --skip "atlas" \
  --skip "demo_scene_creates" --skip "font_registry"
```

## Benchmarks

```bash
# Run all benchmarks
cargo bench --workspace

# Run benchmarks for a specific crate
cargo bench -p logos-core
cargo bench -p logos-render

# Results are saved to target/criterion/
# Open target/criterion/report/index.html for interactive reports
```

## Reporting Bugs

We use [GitHub Issues](https://github.com/navidrezadoost/Logos/issues)
with structured templates. Before filing a new issue:

1. Search existing issues to avoid duplicates
2. Choose the appropriate template:
   - **Bug Report** — crashes, performance issues, unexpected behavior
   - **Feature Request** — new capabilities or improvements
   - **Plugin Submission** — submit your plugin to the marketplace
3. Include your Logos version, platform, and reproduction steps

Security vulnerabilities should be reported privately via
[GitHub Security Advisories](https://github.com/navidrezadoost/Logos/security/advisories/new).


## Pull Requests

Before submitting a PR, please read the **Developer's Certificate of
Origin** section below. Format your code and commits according to these
guidelines.

**Bug fixes** — feel free to submit a PR directly. We still recommend
filing an issue first so we can track it even if the specific fix isn't
accepted.

**New features** — open a Discussion or Feature Request issue first.
No PR will be accepted without prior discussion about the design.

**Good first issues** — look for the `good first issue` label for
beginner-friendly tasks.

### PR Checklist

- [ ] `cargo test --workspace` passes (2,007+ tests)
- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --workspace` has no new warnings
- [ ] New code includes tests
- [ ] Public APIs are documented with `///` doc comments
- [ ] `CHANGELOG.md` is updated if applicable

## Commit Guidelines

We follow [Conventional Commits](https://www.conventionalcommits.org/) with
scope. The format is:

```
<type>(<scope>): <subject>

[body]

[footer]
```

### Types

| Type | Description |
|------|-------------|
| `feat` | A new feature |
| `fix` | A bug fix |
| `perf` | Performance improvement |
| `refactor` | Code change that neither fixes a bug nor adds a feature |
| `docs` | Documentation only |
| `test` | Adding or updating tests |
| `ci` | CI/CD changes |
| `chore` | Build process, dependencies, tooling |

### Scopes

Use the crate name without the `logos-` prefix: `core`, `layout`, `render`,
`text`, `collab`, `desktop`, `wasm`, `plugins`, `ai`, `import-figma`,
`marketplace-auth`, etc.

### Examples

```
feat(plugins): add viewport host functions for camera control
fix(core): prevent panic on empty layer batch commit
perf(render): reduce GPU uploads by 99.9% via dirty-slot tracking
docs(desktop): add keyboard shortcut reference table
test(collab): add WebSocket reconnection integration tests
ci: add WASM build verification to release workflow
```

## Formatting and Linting

All code must pass CI checks before merging:

```bash
# Format all Rust code
cargo fmt --all

# Check formatting (CI mode — fails on diff)
cargo fmt --all -- --check

# Run clippy lints
cargo clippy --workspace -- -D warnings

# Full pre-submit check (what CI runs)
cargo fmt --all -- --check && \
cargo clippy --workspace -- -D warnings && \
cargo test --workspace
```

### Code Style

- Use `rustfmt` defaults (no custom `rustfmt.toml`)
- Document all public APIs with `///` doc comments and examples
- Prefer `#[must_use]` on functions that return values
- Use `thiserror` for error types, `anyhow` only in binaries
- Keep functions under 50 lines; extract helpers for complex logic

## Release Process

1. All changes land on `main` via PR (CI must pass)
2. Create a release branch: `release/vX.Y.Z`
3. Update version in workspace `Cargo.toml` files
4. Update `CHANGELOG.md` with the new version section
5. Create PR to `main`, merge after review
6. Tag: `git tag -a vX.Y.Z -m "Release vX.Y.Z"`
7. Push: `git push origin main --tags`
8. CI automatically creates a GitHub Release with artifacts

### Version Scheme

- `vX.Y.Z-rc.N` — release candidates for testing
- `vX.Y.Z` — stable releases
- Major bumps for breaking API changes
- Minor bumps for new features
- Patch bumps for bug fixes

## Code of Conduct ##

As contributors and maintainers of this project, we pledge to respect
all people who contribute through reporting issues, posting feature
requests, updating documentation, submitting pull requests or patches,
and other activities.

We are committed to making participation in this project a
harassment-free experience for everyone, regardless of level of
experience, gender, gender identity and expression, sexual
orientation, disability, personal appearance, body size, race,
ethnicity, age, or religion.

Examples of unacceptable behavior by participants include the use of
sexual language or imagery, derogatory comments or personal attacks,
trolling, public or private harassment, insults, or other
unprofessional conduct.

Project maintainers have the right and responsibility to remove, edit,
or reject comments, commits, code, wiki edits, issues, and other
contributions that are not aligned with this Code of Conduct. Project
maintainers who do not follow the Code of Conduct may be removed from
the project team.

This Code of Conduct applies both within project spaces and in public
spaces when an individual is representing the project or its
community.

Instances of abusive, harassing, or otherwise unacceptable behavior
may be reported by opening an issue or contacting one or more of the
project maintainers.

This Code of Conduct is adapted from the Contributor Covenant, version
1.1.0, available from [http://contributor-covenant.org/version/1/1/0/](http://contributor-covenant.org/version/1/1/0/)

## Developer's Certificate of Origin (DCO)

By submitting code you agree to and can certify the following:

    Developer's Certificate of Origin 1.1

    By making a contribution to this project, I certify that:

    (a) The contribution was created in whole or in part by me and I
        have the right to submit it under the open source license
        indicated in the file; or

    (b) The contribution is based upon previous work that, to the best
        of my knowledge, is covered under an appropriate open source
        license and I have the right under that license to submit that
        work with modifications, whether created in whole or in part
        by me, under the same open source license (unless I am
        permitted to submit under a different license), as indicated
        in the file; or

    (c) The contribution was provided directly to me by some other
        person who certified (a), (b) or (c) and I have not modified
        it.

    (d) I understand and agree that this project and the contribution
        are public and that a record of the contribution (including all
        personal information I submit with it, including my sign-off) is
        maintained indefinitely and may be redistributed consistent with
        this project or the open source license(s) involved.

Then, all your code patches (**documentation is excluded**) should
contain a sign-off at the end of the patch/commit description body. It
can be automatically added by adding the `-s` parameter to `git commit`.

This is an example of what the line should look like:

```
Signed-off-by: Andrey Antukh <niwi@niwi.nz>
```

Please, use your real name (sorry, no pseudonyms or anonymous
contributions are allowed).
