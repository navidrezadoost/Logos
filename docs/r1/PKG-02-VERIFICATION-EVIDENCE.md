# R1 PKG-02 — Verification Corrections

**Status:** Implementation correction in the verification worktree; verification incomplete; not rollout-approved  
**Base commit:** `d387cc83fd3f45d1ffe019bf86b1c52dcf27c866` (the package merge commit on `origin/main`)  
**Correction commit:** `3285785268a3d45b76bc299dbfc6b6b64c5b3657`  
**Branch/worktree:** `r1/pkg02-verification` — `/home/navidrezadoost/Documents/Github/Logos-r1-pkg02-verification`

## Scope

Replace the local plugin's embedded shared master token with a short-lived, one-time pairing code. The master bearer remains only at the local server/MCP-client boundary. An authenticated, loopback-only `POST /local/plugin-pairing` endpoint issues a random 256-bit code with a 60-second expiry, stores only its SHA-256 digest, caps outstanding codes, and returns it with `Cache-Control: no-store`. The plugin UI accepts the code at connection time; the WebSocket consumes it once before registering the plugin. Stop clears pending codes.

This is still a local profile, not a remote identity/authorization system. The one-time code is user-mediated and consumed by one connection, but it is not bound to a persistent client identity. No master bearer is compiled into the plugin bundle or placed in a URL.

## Changed files

- `mcp/packages/server/src/localSecurity.ts` — one-time pairing-code store with expiry/consume tests.
- `mcp/packages/server/src/LogosMcpServer.ts` — bearer-protected, no-store pairing endpoint and lifecycle cleanup.
- `mcp/packages/server/src/PluginBridge.ts` — require a valid one-time pairing code before task eligibility.
- `mcp/packages/server/test/security.integration.test.ts` — unauthorized pairing, invalid credentials, one-use redemption, and post-shutdown listener checks.
- `mcp/packages/plugin/index.html`, `src/main.ts`, `src/style.css`, `src/vite-env.d.ts`, `vite.config.ts` — explicit masked pairing-code field; remove master-token bundle definition.
- `mcp/README.md` — local pairing and credential instructions.
- `.github/workflows/mcp-server.yml` — run MCP listener tests/type checks and assert loopback-only Compose publications in CI.
- `docker/devenv/docker-compose.yaml` — bind every published development port to host loopback, stop publishing raw MCP upstream ports, require a per-run local token, and configure the plugin WS proxy URL.
- `docker/devenv/files/Caddyfile`, `docker/devenv/files/nginx.conf` — Caddy listens on the container interface behind host-loopback-only publication; Nginx proxies pairing, WS and REPL paths to container-loopback MCP listeners.

No dependency or lockfile changes are included.

## R1-C01 — Development ingress correction

The merged baseline Compose file published 4400–4402 and other development ports without a host IP, which defaults to all host interfaces. This verification branch binds all 21 published development ports to `127.0.0.1`, removes direct host publication of MCP upstream ports 4400–4402, and requires a per-run `LOGOS_MCP_LOCAL_TOKEN` at Compose resolution/startup. The plugin WebSocket, pairing endpoint and REPL are routed through Caddy/Nginx at loopback port 3450; the MCP application listeners remain container-loopback.

Caddy binds the container interface so Docker's published loopback port can reach it; the host port is still restricted to loopback. Other containers on the Compose bridge can reach that edge, so the bridge remains a local trust boundary and MCP endpoints still require bearer/pairing authentication. The resolved Compose config and generic Docker host-bind behavior were checked below, but the complete Logos Caddy/Nginx image is unavailable locally and its route behavior has not been exercised end-to-end. Production ingress, firewall rules and deployed versions remain unknown.

## Executed evidence

| Check | Result |
|---|---|
| Node 24 `--experimental-strip-types --check` for modified server/plugin TypeScript and tests | **Pass** — syntax only, not a type check. |
| Node inline checks for loopback configuration, bearer parsing, pairing expiry, one-use consume and replay rejection | **Pass**. |
| Source inventory for `LOGOS_MCP_LOCAL_TOKEN` under `mcp/packages/plugin/` | **Pass** — master bearer is no longer referenced or defined in plugin source/config. This is not a built-bundle inspection. |
| `docker-compose config` with a disposable token and structural assertions | **Pass** — all 21 published development ports resolve to host IP `127.0.0.1`; MCP upstream ports 4400–4402 are not host-published; token, loopback bind and proxied WS URI resolve. The token was generated in-process and not printed. |
| `docker-compose config` with `LOGOS_MCP_LOCAL_TOKEN` unset | **Expected fail-closed** — Compose rejects configuration before startup because the required per-run token is missing. |
| Isolated Docker port-publication smoke test | **Pass** — a temporary Node 20 listener mapped to `127.0.0.1:55440` answered on host loopback; `ss` showed `127.0.0.1:55440`, not a wildcard host bind. The test container was removed. |
| Full Logos Docker/Caddy/Nginx stack runtime test | **Not run** — no `logos/devenv` container or local Caddy/Nginx binaries/images were available. This verifies Compose resolution and generic Docker host binding, not the complete proxy path. |
| `pnpm --filter mcp-server types:check` | **Blocked** — `tsc: command not found`; workspace `node_modules` is absent. |
| `pnpm --filter mcp-plugin types:check` | **Blocked** — `tsc: command not found`; workspace `node_modules` is absent. |
| `pnpm --filter mcp-server test:security` | **Blocked** — `esbuild: command not found`; live listener test did not execute. |
| Live HTTP/WS/REPL integration suite | Authored, not yet executed; requires lockfile-pinned MCP dependencies. |

## Remaining security gates

- Run type checks and the listener integration suite in a reproducible Node 20 environment. The previous frozen install attempt repeatedly timed out against `registry.npmjs.org`; no lockfile was changed.
- Confirm the plugin bundle contains no master token and only a one-time pairing value is sent on the WebSocket.
- Verify the local Docker/Nginx/Caddy path in a running isolated stack, including the new pairing-code proxy route and the trusted bridge-network boundary. Host ports are loopback-only, but container-network reachability still depends on Docker bridge isolation. No Logos container/listener was active during inventory; production hosts and ingress remain unknown.
- Keep remote/multi-user mode disabled. Do not deploy until a reviewed exposure inventory and actual transport tests pass.

## Three-state status

| State | PKG-02 verification correction |
|---|---|
| **Implemented** | In progress on this branch; one-time plugin pairing replaces bundling the master bearer, and Compose host publications are loopback-only. |
| **Verified** | Partial — source syntax/helper checks, Compose host-binding resolution, and a generic Docker host-loopback port test pass. Type-checking and the actual MCP/Caddy/Nginx integration remain unverified. |
| **Rollout-approved** | No. |
