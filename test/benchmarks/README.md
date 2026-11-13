# Memory benchmark readiness and handoff

The scheduled/manual workflow verifies infrastructure and Go `.logos` parser
compatibility. It currently reports **Memory measurement not run**. No browser
heap result exists, and the performance gate remains **NOT MEASURED**. See
[the recovery record](CI-RECOVERY.md) for evidence and blockers.

## Evidence levels

| Capability | Current evidence | Status |
|---|---|---|
| Go parser compatibility | Go generator output is read by `binfile.ReadZIP`; generated fixture has five pages and 10,000 Go change rows | Verified format/parser compatibility only |
| Backend infrastructure | PostgreSQL and Redis readiness; backend health and dependency connections | Verified service readiness only |
| Application schema | Required auth/import tables and columns are checked independently by preflight | Not initialized in the current CI database |
| Authentication | Supported Go flow is `prepare-register-profile` followed by `register-profile`; login uses `login-with-password` | Blocked: no prepared test account or signing key in current workflow |
| Project/import | Authenticated multipart `import-binfile` requires an editable destination `project-id` and `.logos` file | Blocked: no authorized test account/project in current workflow |
| Browser workspace | Must identify the actual editor build, load the imported fixture, and verify visible shapes and page controls | Blocked: Go API exposes no editor UI |
| Heap-capture capability | Requires renderer evidence and Chromium/CDP snapshot support | Blocked until the real editor adapter is available |
| Benchmark execution | Requires a separate measurement runner, metrics, and retained result | **NOT MEASURED** |

The fixture generator creates 10,000 change-history rows. That count does not
mean the browser rendered 10,000 shapes.

## Infrastructure workflow

The infrastructure gate starts only PostgreSQL and Redis from the development
Compose model. It binds test ports to runner loopback, uses disposable Compose
token values that are not persisted, starts the Go backend, and validates its
dependencies. It does not start the privileged development container or
unrelated services. It also exercises the Go generator/parser compatibility
test. A green run is not evidence of login, import, rendering, or memory
performance.

Run the deterministic readiness checks locally with Node 20+, Go, `psql`, and
`redis-cli` available:

```sh
cd backend-go
go test ./cmd/gen-benchmark ./internal/binfile
cd ../test/benchmarks
node --test readiness_preflight.test.js
DATABASE_URL='postgres://…@127.0.0.1:5432/logos' \
REDIS_URL='redis://127.0.0.1:6379/0' \
BACKEND_DEPENDENCIES_READY=true \
LOGOS_URL='http://127.0.0.1:6060' \
node readiness_preflight.js
```

The v2 report has three independent states: `infrastructure` (`READY` or
`BLOCKED`), `heap_capture_capability` (`READY` or `BLOCKED`), and
`benchmark_execution` (`NOT_MEASURED`). A successful CDP snapshot capability is
never a memory benchmark result. The workflow currently does not execute a
benchmark or fabricate performance metrics.

Import is disabled unless `BENCHMARK_DISPOSABLE_ENVIRONMENT=true`,
`BENCHMARK_PROJECT_ID` is a valid UUID, and
`BENCHMARK_DISPOSABLE_CONFIRMATION` exactly repeats that project ID. This is an
explicit guard, not proof that a project is disposable; only use an environment
whose owner has approved its isolation. Hosted infrastructure CI supplies none
of these values and performs no import.

The current editor route does not independently prove that the backend-imported
file reached its active document store or rendered canvas pixels. A prior
page-global metadata object was insufficient: it could report matching IDs,
counts, and dimensions without showing file load or visible content. The
preflight now fails closed after navigation and does not accept that metadata
or run heap capture. Browser workspace and heap-capture capability remain
`BLOCKED` until an approved integration observes the exact `get-file` response,
confirms the same file/page is active in the editor, and captures actual
canvas pixels from the Rust renderer. DOM shape counts are not used.

The machine-readable report, stdout, and GitHub step summary pass through
secret redaction; unexpected exception details are suppressed. Report-write
failure is an explicit job failure with a generic message.

## Browser measurement handoff

An approved runtime integration must provide all of the following before a
memory performance gate can run:

1. The real editor URL and build identifier.
2. A supported test-account preparation/authentication flow and isolated
   editable destination project.
3. A successful authenticated fixture import and selected project/file.
4. A workspace verification showing the fixture's visible shapes and expected
   page controls; change-row counts are not rendering evidence.
5. A deterministic warm-up and interaction sequence with explicit failure and
   timeout handling.
6. Baseline and post-operation heap snapshots, retained heap growth, detached
   DOM-node counts, ArrayBuffer growth, and browser/OS/build/fixture metadata.
7. Retained snapshot and JSON report artifacts. Missing reports, empty
   directories, or skipped work must be `BLOCKED`, never `PASSED`.

Keep JavaScript heap, WASM memory, GPU allocations, and backend RSS as separate
metrics. A JavaScript heap snapshot does not measure total browser memory.
Previously documented numeric limits are historical expectations, not a
current baseline or performance claim. The browser runtime, renderer evidence
adapter, fixture import path, warm-up, and actual measurement remain a separate approved
runtime integration change; this work does not select or implement an editor
runtime.
