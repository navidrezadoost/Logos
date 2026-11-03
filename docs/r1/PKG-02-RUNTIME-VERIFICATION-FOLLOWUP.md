# R1 PKG-02 — Runtime Verification Follow-up

**Status:** Local Node 20 listener/type-check and isolated Caddy/Nginx proxy tests pass; full workspace/hosted-CI/deployment gates remain open
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
| Actual `node dist/index.js` with token unset / host `0.0.0.0` | **Pass** — each process exited 1 with the expected missing-token / non-loopback error. |
| Full `pnpm install --frozen-lockfile` in Node 20/pnpm 10.28.2 | **Blocked** — registry returns 404 for locked `@logos/plugin-styles@1.4.1` (no auth header); this repo contains local `@logos/plugin-styles@1.4.2` and `@logos/plugin-types@1.4.2`. No versions/lockfile changed. |
| Filtered lockfile installs for MCP server/common | **Pass** — server/common dependency sets installed without the plugin-only 404. pnpm warned that esbuild/sharp build scripts were ignored; esbuild's binary was available and the security suite ran. |
| `pnpm --filter mcp-common build` then `pnpm --filter mcp-server types:check` | **Pass** — server type-check succeeds when the shared package's `dist` declarations are built first. The workflow now includes this build step. |
| `pnpm --filter mcp-server test:security` | **Pass** — 4 tests, 0 failures, Node 20.11.1/pnpm 10.28.2; listener bound to `127.0.0.1` on ephemeral ports. |
| Isolated actual Caddy/Nginx/MCP harness using the repository Caddyfile/Nginx config | **Pass, scoped** — Caddy 2.10.2 and Nginx 1.22.1; host publishes only `127.0.0.1:3450`. Through both proxies, unauthorized HTTP/SSE/REPL/pairing was denied; authorized MCP initialize/tools/list, pairing, and WebSocket auth worked; replay/unpaired WebSockets closed with 1008. Host listener was removed at shutdown. |
| Separate container on the test bridge | **Pass** — Compose-style `logos-devenv-main` alias reached Caddy/Nginx but unauthenticated MCP requests returned 401; raw MCP HTTP/WS/REPL upstream ports 4401–4403 refused bridge connections. Unknown IP-based Host received 421 for both HTTP and WebSocket upgrade requests. |
| `gh run list` for the pushed follow-up branch | **Blocked** — GitHub Actions API connection reset; hosted CI status was not verified. |

Earlier default Docker-bridge installs also timed out. Host-network access enabled the targeted server install and tests, but the full plugin workspace still needs an authorized source for its locked 1.4.1 `@logos/*` packages. Do not substitute the repository's 1.4.2 packages or change the lock without approval.

### Workspace package contract investigation

- `pnpm --version` in the Node 20 test container was `10.28.2`. There is no repository-root `pnpm-workspace.yaml`; `mcp/pnpm-workspace.yaml` and `plugins/pnpm-workspace.yaml` are separate roots. The MCP workspace contains only `mcp/packages/{common,server,plugin}`; the plugin workspace separately includes `plugins/libs/**`.

| Package | MCP manifest/lock | Local workspace package | Documented source and observed result |
|---|---|---|---|
| `@logos/plugin-styles` | Exact `1.4.1`; lock resolves registry tarball `1.4.1` | `plugins/libs/plugins-styles/package.json` is `1.4.2`, outside the MCP workspace | README says `npm install @logos/plugin-styles`; full frozen install gets registry 404 for locked 1.4.1 with no authorization header. |
| `@logos/plugin-types` | Exact `1.4.1`; lock resolves registry tarball `1.4.1` | `plugins/libs/plugin-types/package.json` is `1.4.2`, outside the MCP workspace | README says `npm install @logos/plugin-types`; the full install stops at the styles 404 before independently confirming this tarball. |

The in-repository manual release contract is `plugins/package.json` → `pnpm run release` → `plugins/tools/scripts/publish.ts`; it builds `libs/plugin-types`, `libs/plugins-styles`, and `libs/plugins-runtime`, then publishes with `--access public`. `plugins/docs/publish-package.md` says publishing requires a temporary npm token. No automated publish workflow was found. `plugins/.npmrc` only sets `ignore-workspace-root-check`; root and MCP `.npmrc` files are absent, and no npm/GitHub token was present in the test container.

Unauthenticated `npm view @logos/plugin-types versions` and `npm view @logos/plugin-styles versions` requests to `registry.npmjs.org` both returned package-level 404. The full frozen install likewise got 404 for `@logos/plugin-styles@1.4.1` with no authorization header. This proves the pinned registry contract is not reproducible from the available route; it does not establish whether versions were never published, removed, or auth-hidden.

**Contract finding:** MCP is configured for registry-published packages: its workspace excludes `plugins/libs/**`, its manifests pin exact registry versions, and its lock has registry resolutions. The separate plugins workspace uses a local `workspace:` link for plugin types and has local 1.4.2 source. The repository documents manual public npm releases but does not identify the package owner or confirm which MCP-compatible version is published. Do not substitute 1.4.2 based on version proximity. The package owner must make the intended artifact/credentials available or explicitly approve a local-workspace migration with compatibility tests. No manifest/lock changes were made.

## Remaining runtime/exposure gates

- The executed suite covers missing-token/off-loopback constructor rejection; missing/wrong bearer and query-token bypass; a valid MCP initialize/notifications/tools-list flow; unauthenticated and authenticated SSE `/sse`/`/messages`; REPL auth; one-time pairing expiry and replay; forged WebSocket task before auth; and shutdown listener cleanup. The listener assertions passed locally.
- The listener test includes constructor fail-closed assertions; separate process-level checks confirm missing-token and non-loopback startup exit 1. Pairing expiry is covered by the helper and by an expired-code WebSocket rejection using a controlled clock; replay is rejected on a live WebSocket. The bearer token itself has no expiry field in this local profile.
- The isolated harness ran the repository's actual Caddyfile and Nginx config, but not the full `logos/devenv:latest` image or all Compose services. The Caddy TLS site on 3449 and a restart cycle were not tested.
- The prior Caddy default for an unrecognized IP Host returned an empty `200`. The follow-up Caddyfile now routes only `localhost`, `127.0.0.1`, and `logos-devenv-main` to Nginx; all other Host values receive an inert `421 Misdirected Request`. The runtime harness verified both ordinary HTTP and WebSocket upgrade rejection for an unknown IP Host. Production bridge membership and ingress rules remain unknown.
- Production hosts, firewall/security-group rules, ingress routes and deployed versions remain unknown. The local listener inventory showed no Logos process; this is not production evidence.
- The one-time pairing code is per-connection and single-use but is not bound to a persistent plugin identity. Keep remote access disabled; do not distribute the local profile or relax auth for clients lacking bearer support.

The local bearer is a per-run static secret with no expiry field; invalid bearer requests are denied. The one-minute expiry applies to plugin pairing codes and was tested with controlled helper timestamps.

No production rollout is authorized.
