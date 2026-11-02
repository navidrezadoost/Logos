# R1 PKG-02 — Runtime Verification Follow-up

**Status:** Verification incomplete; remote MCP remains disabled; no rollout approval
**Base commit:** `ddae16ad048cf429b386f14e66095ba8faddb256` (`origin/main`, after PRs #27–32)
**Branch/worktree:** `r1/pkg02-mcp-runtime-followup` — `/home/navidrezadoost/Documents/Github/Logos-r1-pkg02-mcp-runtime-followup`

## Current security profile

The merged implementation requires a per-run local bearer for MCP HTTP/SSE/REPL, uses an authenticated one-time 60-second plugin pairing code, rejects remote/multi-user mode, and keeps the master bearer out of plugin source/config. Docker Compose publishes development host ports only on `127.0.0.1`, removes direct publication of MCP upstream ports, and requires the operator to supply a token.

## Evidence reviewed and reproduced

| Command/check | Result |
|---|---|
| Node 24 inline checks of the current `localSecurity.ts` helper | **Pass** — loopback defaults, reject off-loopback/remote mode, bearer validation, pairing expiry and single-use behavior. Not a TypeScript type check or live listener test. |
| `docker-compose config --format json` with a generated disposable token (`docker-compose` 5.6.0) | **Pass** — six services resolve; all 21 host-published ports use `127.0.0.1`; no MCP upstream ports 4400–4402 are host-published. Compose emitted an unset `CURRENT_USER_ID` warning. |
| `docker-compose config` without `LOGOS_MCP_LOCAL_TOKEN` | **Expected fail-closed** — configuration rejects the missing per-run token before startup. |
| Temporary Node 20 container mapped to host `127.0.0.1:55440` | **Previously recorded in merged PKG-02 evidence; not rerun in this follow-up pass.** The recorded check showed a host loopback response and the container was removed. |
| `pnpm install --offline --frozen-lockfile` | **Blocked** — local store lacks lockfile-pinned packages (observed `concurrently@9.2.1`). Lockfile remains unchanged. |
| `pnpm install --frozen-lockfile` | **Blocked** — repeated registry tarball `ETIMEDOUT` warnings; this attempt reached the 180-second tool timeout. No lockfile change. |
| `pnpm --filter mcp-server types:check` / `pnpm --filter mcp-plugin types:check` | **Blocked** — `tsc` absent because dependencies are unavailable. |
| `pnpm --filter mcp-server test:security` | **Blocked** — `esbuild` absent; actual listener assertions did not execute. |

An earlier online frozen install attempt timed out against `registry.npmjs.org`. No dependency/lockfile version was changed.

## Remaining runtime/exposure gates

- Run the type checks and actual listener suite (invalid/missing bearer, pairing-code expiry/replay, WebSocket task before auth, REPL authorization, startup/shutdown and port cleanup) in a dependency-provisioned Node 20 environment.
- The follow-up branch adds listener assertions for unauthenticated and authenticated requests to legacy SSE `/sse` and `/messages`; these are authored but unexecuted until dependencies are available. Passing the suite remains a mandatory gate.
- The Compose/Caddy/Nginx path is not running in this environment. Host-port config and a generic Docker port smoke test pass, but the full proxy/upgrade path is not tested. Caddy/Nginx binaries and the mutable `logos/devenv:latest` runtime are unavailable here.
- Compose binds Caddy on the container interface so Docker forwarding works while host-published ports stay loopback-only. Other containers on the Compose bridge can reach that proxy; verify the bridge trust boundary and actual ingress in an isolated stack.
- Production hosts, firewall/security-group rules, ingress routes and deployed versions remain unknown. The local listener inventory showed no Logos process; this is not production evidence.
- The one-time pairing code is per-connection and single-use but is not bound to a persistent plugin identity. Keep remote access disabled; do not distribute the local profile or relax auth for clients lacking bearer support.

No production rollout is authorized.
