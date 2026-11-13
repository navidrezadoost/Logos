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
| Measurement | Requires a verified workspace and a Chromium/CDP heap snapshot runtime | Not reached |

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

The preflight prints one `READY` or `BLOCKED` result per capability and names
the missing evidence. It never treats HTTP 200 alone as readiness. For auth and
import verification, provide a provisioned disposable `LOGOS_EMAIL`,
`LOGOS_PASSWORD`, and editable `BENCHMARK_PROJECT_ID`; credentials are not
printed. Browser verification also requires `BENCHMARK_WORKSPACE_URL` with a
`{fileId}` path placeholder and `BENCHMARK_EXPECTED_PAGES`; the workspace must
expose `meta[name="logos-build-id"]`, `data-testid="workspace-canvas"` with a
matching `data-file-id`, `data-testid="page-item"` controls, and visible
`data-testid="rendered-shape"` elements. The import check writes into the
destination project, so use only a disposable benchmark environment. Do not
set a credential in hosted CI until the schema, account lifecycle, and project
isolation are approved.

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
current baseline or performance claim. The browser runtime, UI selectors,
fixture import path, warm-up, and actual measurement remain a separate approved
runtime integration change; this work does not select or implement an editor
runtime.
