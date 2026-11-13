# Memory benchmark CI recovery and current blocker

## CI infrastructure repaired

Scheduled/manual workflow runs now start **only PostgreSQL and Redis**. The
privileged `main` development container, MinIO, mailer, and LDAP are not started.
`test/benchmarks/benchmark-memory-compose.yaml` publishes PostgreSQL and Redis
on runner loopback so the host-run Go backend can use its configured
`127.0.0.1:5432` and `127.0.0.1:6379` addresses. The workflow waits for both
services, then requires the backend health endpoint and its successful database
and Redis connection log entries before proceeding.

The dev Compose file requires `LOGOS_MCP_LOCAL_TOKEN` even when `main` is not
selected because Compose interpolates the full model. The workflow therefore
creates a random throwaway value in the environment of each individual Compose
command. It is not printed, written to a GitHub output/environment file, passed
to a started service, or stored as a repository/workflow secret. Production
Compose auth is unchanged.

The infrastructure gate runs on scheduled/manual invocations and on relevant
pull-request changes to its workflow, Compose inputs, generator/parser, or
benchmark fixtures/docs. Its Compose override is under `test/benchmarks/`, not
`.github/workflows/`, so GitHub does not discover it as a workflow definition.

The PostgreSQL init script creates the `logos_test` and `logos_telemetry`
databases, not the application tables in `logos`; the Go backend health handler
only answers HTTP and startup pings PostgreSQL/Redis. The workflow checks and
reports whether `public.profile` exists so process health is not mistaken for a
fully bootstrapped/auth-capable application. The CI Go-backend start also leaves
`LOGOS_SECRET_KEY` unset, which disables its auth handlers. No login is attempted
against this incomplete setup.

The generated fixture is validated as a readable Go `.logos` v3 ZIP, and a Go
round-trip test checks that the benchmark generator's payload is accepted by
the Go `binfile.ReadZIP` parser. This establishes file-format/parser
compatibility only; it does not establish that the fixture imports into a
browser app or renders in a workspace.

## Failed run #43398244325

The scheduled run at `fb0ea41d73a553b31d6dea0c7b1a341d68fdba7c` failed at
**Start Logos services**. Compose rejected the missing required
`LOGOS_MCP_LOCAL_TOKEN`; teardown failed for the same interpolation reason.
No service, backend, browser scenario, or measurement ran in that attempt.

## Why memory measurement remains blocked

The current workflow's browser target is the Go API process on port 6060. Its
router registers `/api/...`, WebSocket, and asset routes but no `GET /` browser
application route. The browser benchmark instead requires all of the following:

1. A login page with `name=email`, `name=password`, and a login button.
2. An authenticated dashboard with an import control and a card named
   `Logos Memory Benchmark`.
3. A rendered workspace canvas and page-navigation UI.

The Go backend does not serve those UI routes; running the privileged dev
container to supply unrelated development services would not be a safe or
verified substitute. The hosted workflow records the root-route status and
skips Playwright and the measurement while this is true.

There are additional harness mismatches that must be resolved before enabling
the benchmark:

- The workflow's former test-user request posted `email`, `password`, and
  `fullName` directly to `register-profile`. The Go handler expects a
  preparation `token` from `prepare-register-profile`; moreover, the former
  Go-backend invocation did not set `LOGOS_SECRET_KEY`, so authentication was
  disabled. The CI database also does not initialize the `profile` table.
- `memory_profile.js` uploads through a UI file chooser and locates the imported
  dashboard card, while the Go API exposes only an authenticated multipart
  `import-binfile` command requiring a destination `project-id`. No browser
  import-to-workspace path was exercised.
- The Go generator names the file `large-canvas-benchmark`, but the browser
  script searches for `Logos Memory Benchmark`.
- The browser script assumes five pages. The old workflow generated ten; the
  repaired infrastructure gate now generates five pages and 10,000 change
  rows, but this does not prove that the workspace consumes those rows.

The initial HTTP/browser compatibility check is therefore the first stop. The
deterministic `readiness_preflight.js` reports PostgreSQL schema, Redis,
backend dependency evidence, authentication, authorized project import,
browser workspace rendering, and Chromium heap collection as separate
capabilities. It requires more than an HTTP 200: PostgreSQL must contain the
required auth/import tables and columns, backend dependency connections must
be independently established, auth must return a session through
`login-with-password`, import must succeed through the authenticated
`import-binfile` route with an editable `project-id`, and the editor must expose
a build ID, workspace canvas, page controls, and visible fixture shapes.

The actual CI database currently lacks the application schema, `LOGOS_SECRET_KEY`
and a prepared test account are not configured, there is no authorized project
ID, and the Go backend does not serve the editor UI. Those capabilities report
specific blockers. When Chromium installation is available, it navigates to
the configured workspace URL and tests CDP heap collection only after fixture
verification; unavailable Chromium is reported as a blocker. The infra workflow
keeps succeeding when these capabilities are blocked, uploads readiness JSON,
and prints **Memory measurement not run**. No heap snapshot/report is emitted
or treated as a performance pass. A future measurement job must separately
report PASSED, FAILED, or BLOCKED based on a real execution and retained report.

The current supported auth API is `prepare-register-profile` with
`fullname`/`email`/`password`, followed by `register-profile` with its returned
`token`; login is `login-with-password`. The import route is multipart and
requires an authenticated profile with edit permission on the destination
`project-id`. This package does not create a fake login route, change API
contracts, or provision credentials in hosted CI. The browser measurement
handoff and metrics are documented in [README.md](README.md); selecting or
implementing an editor runtime is a separate approved change.

## Evidence boundaries

- Run #43398244325: token interpolation failure before service startup.
- Local source inspection: Go server has no `/` UI route; registration/import
  contracts and fixture naming/page-count assumptions differ as above.
- CI round-trip test: generated Go payload is readable by the Go v3 parser.
- Current preflight contract: `readiness_preflight.js` independently records
  the seven capability states and writes a machine-readable readiness report.
  Its tests reject invalid/missing database URLs without printing credentials.
  Infrastructure success and measurement status remain separate.
- Hosted run [#43405395359](https://github.com/navidrezadoost/Logos/actions/runs/43405395359)
  at `f85ced619adc00801a5bf4aff3190cd49d21b3aa` passed in 44 seconds: PostgreSQL
  and Redis readiness, Go backend health/connections, and the generated fixture
  check passed. It observed no `public.profile` table and HTTP 404 at `/`; the
  memory step was skipped. No heap snapshots or memory measurements were
  produced.
- The Compose overlay was moved from `.github/workflows/` to
  `test/benchmarks/benchmark-memory-compose.yaml` because GitHub discovered the
  former `.yaml` path as a workflow definition and reported that generated
  workflow check as failed. The infrastructure workflow now has a
  pull-request trigger for relevant inputs. Hosted [run #43411514803](https://github.com/navidrezadoost/Logos/actions/runs/43411514803)
  on head `aae9e0fa` was triggered by PR #73 and passed the infrastructure gate;
  it explicitly reported “Memory measurement not run” because `GET /` returned
  404. This is infrastructure execution evidence only, not a performance result.
- The later hosted infrastructure run [#43412424652](https://github.com/navidrezadoost/Logos/actions/runs/43412424652)
  passed on PR #73 head `42abd22da8bffd4d16a8b208d929879ea798ade9`. It is the
  current pre-change infrastructure baseline; the readiness-contract changes
  still require hosted workflow validation on their submitted head.
