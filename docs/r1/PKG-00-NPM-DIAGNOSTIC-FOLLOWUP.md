# R1 PKG-00 — Node 20 npm Install Diagnostic

**Status:** React install remains blocked by registry transport; M0 remains open
**Base commit:** `ddae16ad048cf429b386f14e66095ba8faddb256` (`origin/main`, after PRs #27–32)
**Branch/worktree:** `r1/pkg00-m0-followup` — `/home/navidrezadoost/Documents/Github/Logos-r1-pkg00-m0-followup`

## Reproduction

Ran `npm ci` in `logos-app/` from a clean Node 20 container against the checked-in lockfile. Fetch retries/timeouts were bounded to make the first transport failure observable; no dependency or lockfile edits were made.

```text
$ docker run --rm \
  -v /home/navidrezadoost/Documents/Github/Logos-r1-pkg00-m0-followup:/workspace \
  -w /workspace/logos-app node:20.11.1-bookworm-slim \
  npm ci --fetch-retries=0 --fetch-timeout=30000
npm ERR! code ERR_SSL_WRONG_VERSION_NUMBER
npm ERR! errno ERR_SSL_WRONG_VERSION_NUMBER
npm ERR! request to https://registry.npmjs.org/zustand/-/zustand-5.0.13.tgz failed, reason: ...
... SSL routines:ssl3_get_record:wrong version number ...
Exited with code 1
```

Environment: Node `v20.11.1`, npm `10.2.4`. This is a registry/network TLS transport failure, not a dependency-resolution, lifecycle-script, or lockfile error. The install did not complete, so React build/type-check evidence is still unavailable. The failure is distinct from the earlier Node 20 run recorded in `PKG-00-VERIFICATION-EVIDENCE.md` (`Exit handler never called`); neither is evidence of a source compilation failure.

## Sanitized transport configuration

In the Node 20 reproduction container, the effective npm registry was `https://registry.npmjs.org/`, `strict-ssl=true`, and `cafile=null`. npm `proxy` and `https-proxy` were unset/null. `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY`, their lowercase forms, `npm_config_proxy`, `npm_config_https_proxy`, `NODE_TLS_REJECT_UNAUTHORIZED`, `NODE_EXTRA_CA_CERTS`, and `SSL_CERT_FILE` were all unset in that container. The failed request destination was the locked tarball URL shown above.

For comparison, the host's npm config also reports the same registry, `strict-ssl=true`, and no custom CA file; both npm proxy settings are configured to an HTTP proxy (endpoint/credentials omitted here). Host proxy environment variables are unset. The Node 20 container did not inherit that host npm configuration, so the reproduced TLS failure occurred with no npm proxy configured. No certificate verification or registry security setting was disabled.

## Independent Cargo access check

```text
$ cargo test --workspace --locked --no-run
Downloading crates ...
warning: ... Timeout was reached (download of `khronos_api v3.1.0` failed to transfer more than 10 bytes in 30s)
warning: ... Timeout was reached (failed to download any data for `glam v0.24.2` within 30s)
... additional crates.io download timeouts ...
Command exceeded timeout of 150000 ms
```

Cargo dependency access also timed out before compilation; this is independent of the npm TLS error and is not a Rust compile failure. `--locked` was used, and no lockfile changes were made. Re-run in a CI environment with approved crates.io access before classifying Rust build health.

The temporary Node container was removed. The install attempt left no source or lockfile changes; this follow-up document is the only branch change. Do not alter dependency versions to work around the transport failure. Retry in a network/CI environment with working npm registry TLS, then run the pinned install and release-target checks.

No rollout approval is implied; M0 remains open.
