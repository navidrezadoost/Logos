# R1-F07 — Pinned Dependency Supply Assessment

- **Status:** Read-only investigation complete; no archive was retrieved or used
- **Current baseline:** `ed1e476fa8c773ebcc3f7fd4a226fb1b1f2c59fb`
- **Historical target:** `665bb1ef13c3eeff2e84bf6e0d588a6bdecc6a92`

## Executive result

The two historical frontend dependencies are pinned to exact commits in the original Penpot GitHub repositories, and both commits exist. GitHub's API metadata endpoint was reachable for the commit checks, but the F06 pnpm lockfile-only attempt received repeated `ECONNRESET` errors from `codeload.github.com`. No F07 retry of those archive URLs was made.

No codeload mirror or exact archive was found in the inspected experiment-owned pnpm caches. The source-configured Verdaccio proxy uplinks only to npmjs; it is not a GitHub archive mirror. The historical pnpm lock entries pin the tarball URLs and package versions but do **not** record an `integrity` hash for either archive. Therefore the current evidence can identify the commits, but cannot establish the bytes of an archive that was never retrieved.

**Stop condition:** Do not retry package installation or use a mirror in a build until a trusted route or owner-approved immutable artifact source and checksum process are supplied. This assessment did not install packages, download the archives, modify manifests/locks, or build.

## F07-A — Repository and commit identity

The historical manifests and lockfile identify these exact source revisions:

| Dependency use | Repository / package | Commit | Lock version | Commit evidence |
|---|---|---|---|---|
| `@logos/svgo` | [`penpot/svgo`](https://github.com/penpot/svgo) | `8c9b0e32e9cb5f106085260bd9375f3c91a5010b` | `4.0.0` | GitHub API confirms the commit and reports its signature as verified. Commit: [`Enable inline styles cleaning by default`](https://github.com/penpot/svgo/commit/8c9b0e32e9cb5f106085260bd9375f3c91a5010b). |
| `draft-js` under local `@logos/draft-js` | [`penpot/draft-js`](https://github.com/penpot/draft-js) | `4a99b2a6020b2af97f6dc5fa1b4275ec16b559a0` | `0.11.7` | GitHub API confirms the commit; the commit signature is reported **unsigned**. Commit: [`Update yarn.lock file`](https://github.com/penpot/draft-js/commit/4a99b2a6020b2af97f6dc5fa1b4275ec16b559a0). |

The historical dependency declarations point directly to:

```text
https://codeload.github.com/penpot/svgo/tar.gz/8c9b0e32e9cb5f106085260bd9375f3c91a5010b
https://codeload.github.com/penpot/draft-js/tar.gz/4a99b2a6020b2af97f6dc5fa1b4275ec16b559a0
```

The source commit SHA pins the Git tree identity. It is not, by itself, a checksum of the HTTP tarball bytes or of GitHub's archive packaging.

## F07-B — Network route evidence

- F06's disposable Node 22 container used the default Docker bridge for dependency resolution, with a cleared environment, no production credentials/database, and no published ports. pnpm issued `HEAD` requests to both codeload URLs; both reset, and the final failure was `svgo` after pnpm's retries. The captured F06 log is `/tmp/opencode/f06-logs/frontend-lockfile-only.log` (SHA-256 `5470c0312512cd8fb72f1ba2e5a93d3ea17f664d7250665218f8a9cb69baded2`).
- F07 made one read-only GitHub API metadata request per commit. Those API requests returned the commit records, while the earlier archive requests did not. API reachability is not evidence that codeload/archive downloads work.
- No `HTTP_PROXY`, `HTTPS_PROXY`, `NPM_CONFIG_REGISTRY`, or mirror environment variable name was present; only `NPM_CONFIG_GLOBALCONFIG` appeared. Its value was not inspected. No alternate route was configured or tested.
- No repeated codeload request, Git clone/fetch, package install, or source download was attempted in F07.

## F07-C — Cache and mirror inventory

- The historical source contains `plugins/.verdaccio/config.yml`, a local npm registry proxy with an `npmjs` uplink to `https://registry.npmjs.org/`. It is not configured to proxy or cache `codeload.github.com` archives. No Verdaccio service was started and no storage contents were inspected.
- `plugins/.npmrc` contains only the `ignore-workspace-root-check` setting when inspected with values redacted; no mirror configuration was identified there.
- Searches of the F04/F06 experiment-owned pnpm caches found no filename or searchable index entry for either full commit SHA or package archive. F06's frontend attempt failed during lockfile-only resolution before downloading either archive. The cache search was limited to those experiment-owned temporary directories; no user-home or production cache was inspected.
- No trusted archive mirror, pre-populated internal artifact store, or artifact access endpoint was provided for this assignment.

Verdaccio could potentially proxy registry packages, but it cannot satisfy these direct GitHub tarball URLs as configured. It is not a provenance-preserving solution by itself.

## F07-D — Integrity evidence and validation requirement

The historical `frontend/pnpm-lock.yaml` entries for `svgo@https://codeload...` and `draft-js@https://codeload...` contain a tarball URL and package version, but no `integrity` field. No archive bytes were retrieved in F06/F07, so no archive checksum was computed. Do not invent one from the URL or package version.

For a later approved supply test, require:

1. Verify each full commit SHA against its canonical repository. Verify the SVGO commit signature; because the draft-js commit is unsigned, obtain an independent source-owner attestation or review before accepting a mirror artifact.
2. Obtain the exact archive through an approved route. Compute SHA-512 (and record SHA-256 if required by the artifact store) over the exact downloaded bytes. Have the expected digest attested through an independent trusted source; a digest computed only from the same untrusted transfer is not a trust anchor.
3. Record repository, commit, archive URL, package version, artifact digest, acquisition source and verification result together. If a lock integrity field must be added, do so only in a separately authorized disposable change and review the resulting lock diff.
4. Before any use, verify the mirrored bytes against that approved digest and confirm the unpacked package content corresponds to the pinned source tree and expected package root.

## F07-E — Proposed offline supply method and rollback

**Proposal only; not executed:** after an approved source route and checksum authority are identified, acquire the exact commit artifacts from the canonical repository or approved mirror, verify signatures/digests, and place them in immutable read-only artifact storage keyed by repository plus commit plus digest. A separate disposable worktree may then test an offline pnpm supply mechanism, with frozen installation and unchanged package versions. Any URL/integrity edits needed for that test require separate authorization before the test begins.

Keep the baseline manifests and lockfiles unchanged in active repositories. In a later approved experiment, retain their original hashes and the exact mirror mapping; if digest validation or frozen install fails, discard only that experiment's mirror configuration and restore its original manifest/lock snapshot. Do not publish or deploy the mirrored artifacts.

## Exit status

- F07-A — exact repository URLs/commits identified; GitHub API confirmed both commits. SVGO signature verified; draft-js unsigned.
- F07-B — codeload transport reset is reproduced in F06 evidence; no blind retry or alternate route was tested.
- F07-C — no trusted codeload cache/mirror identified; Verdaccio is npm-only.
- F07-D — commit identity is pinned, but tarball integrity is absent from the lock and archive digests remain unknown.
- F07-E — offline supply plan proposed only; no mirror or archive used.
- No install, build, editor runtime, database, production endpoint, or package publication was used in F07. M0/M1 remain open.
