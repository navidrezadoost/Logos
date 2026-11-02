# PKG-02 — MCP Containment Evidence

**Implementation status:** Implemented on the R1 package branch  
**Verification status:** Partial — helper behavior and source syntax verified; listener integration suite is authored but could not run without workspace dependencies  
**Rollout approval:** Not approved; source-level dev-compose inventory is documented, but production deployment exposure remains unverified
**Base commit:** `ac57a34cd0b5869aaa9580c4773c79e908aa4a95`  
**Implementation commit:** `8fb13db127e192100ed58bdef583156c2efacc2c`  
**Branch/worktree:** `r1/pkg02-mcp-containment` — `/home/navidrezadoost/Documents/Github/Logos-r1-pkg02-mcp-containment`

## Changed-file scope

- MCP server configuration, HTTP bearer middleware, WebSocket first-frame authentication, REPL bearer middleware and listener shutdown/bind behavior.
- Local plugin handshake/configuration, plugin preview loopback bind and exact-origin-only optional CORS.
- MCP local security unit/integration tests, workflow, and local/remote-mode documentation.
- No dependency or lockfile versions changed. An offline/frozen install was attempted for verification only and did not complete; no tracked lockfile changed.

## Containment behavior

- All MCP HTTP/SSE and REPL requests require `Authorization: Bearer <LOGOS_MCP_LOCAL_TOKEN>`; the credential must be at least 32 bytes.
- HTTP, plugin WebSocket and REPL listeners bind only to literal `127.0.0.1` or `::1`. Non-loopback values are rejected. The plugin preview binds to `127.0.0.1`.
- The plugin WebSocket is not eligible to receive tasks until its first frame proves the local token. Query-string `userToken` is no longer treated as identity.
- `--multi-user` and `LOGOS_MCP_REMOTE_MODE=true` fail closed before listener startup. No remote profile is implemented or enabled.
- Plugin preview CORS defaults off. An optional exact `http(s)` origin can be configured; wildcard, paths and non-origin URLs are rejected. The local token is embedded in the locally built plugin for the loopback WebSocket handshake; that bundle must not be published.
- Generic code execution remains available only in the authenticated local profile. The remote profile is unavailable, so this is not a remote execution service.

## Tests authored

`mcp/packages/server/test/security.integration.test.ts` starts the actual HTTP, WebSocket and REPL listeners on loopback test ports. It checks unauthenticated MCP HTTP and REPL requests return 401; a valid bearer reaches the MCP handler; unauthenticated WebSocket task-response frames close with policy code 1008; a valid first-frame token receives an authentication acknowledgment; and remote/non-loopback settings are rejected by unit tests.

## Executed evidence and blockers

Observed runtime: Node `v24.13.1`, pnpm `10.28.2`.

| Command | Result |
|---|---|
| `node --experimental-strip-types --check` on changed server/plugin TypeScript and the listener test | **Pass** — syntax parses. This is not TypeScript type-check evidence. |
| Node inline checks of `loadLocalMcpSecurityConfig`, token equality and bearer parsing | **Pass** — loopback default, strong-token requirement, remote/non-loopback rejection and valid/invalid bearer cases. |
| Parse MCP server/plugin `package.json`, parse workflow YAML with PyYAML, and `git diff --check` | **Pass**. |
| `pnpm install --offline --frozen-lockfile` | **Blocked** — local store lacks `require-directory@2.1.1`. |
| `pnpm install --frozen-lockfile` (300-second attempt) | **Blocked** — repeated `registry.npmjs.org` `ETIMEDOUT` fetching lockfile-pinned tarballs; no lockfile change. |
| `pnpm --filter mcp-server types:check` | **Blocked** — `tsc: command not found`; `node_modules` unavailable. |
| `pnpm --filter mcp-plugin types:check` | **Blocked** — `tsc: command not found`; `node_modules` unavailable. |
| `pnpm --filter mcp-server test:security` | **Blocked** — `esbuild: command not found`; listener integration assertions were not executed. |

No public deployment reachability check, external penetration test, or listener rollout was performed. Authenticated-client compatibility with each MCP host remains to be demonstrated; clients unable to set bearer headers are unsupported by this profile. No security control should be relaxed to accommodate them.

At the local inspection time, `ss -ltnp` showed no listeners on MCP's default ports 4401–4403. This is evidence only about this development environment and does not establish whether any production deployment is reachable.

## R1-C01 — Exposure inventory

Source-level inventory at the approved baseline:

- No `infra/` or production ingress/deployment manifests are present in this checkout. Production hosts, load balancers, security groups, ingress policy and deployed image digest/version were not provided and remain **unknown**.
- `docker/devenv/docker-compose.yaml` publishes host ports `4400:4400`, `4401:4401` and `4402:4402` without a host IP. Docker therefore defaults those publications to all host interfaces. The `main` service uses the mutable `logos/devenv:latest` image; no running Logos container was present during inspection.
- `docker/devenv/files/nginx.conf` listens on container port 4449 and proxies `/mcp/ws`, `/mcp/stream` and `/mcp/sse` to loopback ports 4402/4401. The Compose file maps 3447/3448/3449, not 4449, and also publishes 4401/4402 directly. This topology/port mismatch means the intended ingress path and the version actually exposed cannot be inferred without an operator/runtime check. The Nginx MCP locations do not add an independent authentication layer; they rely on the MCP server.
- `docker/devenv/files/Caddyfile` configures `localhost:3449` → Nginx `localhost:4449` and HTTP `localhost:3450`, while Compose publishes `3449:3449` but not 3450. Both Caddy/Nginx use loopback names inside the container. The binding/forwarding path therefore needs an actual stack test before any port restriction or MCP local-profile rollout.
- The Compose `main` service does not pass `LOGOS_MCP_LOCAL_TOKEN`; with this branch, MCP startup therefore fails closed in that container unless an operator supplies the secret. The browser plugin's default WebSocket URL targets host-loopback port 4402, while the MCP process now binds container-loopback. Direct Docker publication to a container's loopback listener is not a verified/working path. Do not change the listener bind or publish a token to compensate without a tested container-boundary design.
- `docker/devenv/Dockerfile` pins development components (Ubuntu 24.04, Node 22.22.0, Go 1.23.8, Rust 1.91.0), but the mutable running image digest/version is unknown. No Logos MCP/Go listener was running locally on the inspected ports; this does not establish production state.
- The host's local `logos` PostgreSQL catalog is Postgres 17.2. It was accessed read-only for a schema-only export used in isolated PKG-01 verification; it is not evidence about a deployed service or its application version.

No Compose/Nginx port change was made in this package: loopback binding inside a container, host port publication and the existing Nginx route/port mismatch need a verified local topology to avoid either leaving an exposure or silently breaking the local profile. Until the deployment owner supplies that inventory, treat the all-interface Compose publications as potentially reachable, keep the dev stack off untrusted networks, and keep remote MCP disabled. R1-C01 remains open.

## Three-state package status

| State | PKG-02 status |
|---|---|
| **Implemented** | Yes — code/config/docs and executable listener test cases are present on this branch. |
| **Verified** | Partial — helper/syntax checks pass; TypeScript compilation and HTTP/WS/REPL end-to-end tests remain blocked by unavailable packages. |
| **Rollout-approved** | No — do not deploy until type-check, listener tests, actual exposure inventory and security review pass. |

While exposure is unknown, keep the existing public MCP deployment disabled or network-restricted. This branch is implementation evidence, not proof that a running deployment is contained.
