# PKG-01 — Private Asset Authorization Evidence

**Implementation status:** Implemented on the R1 package branch  
**Verification status:** Focused tests pass against a schema-only snapshot restored into PostgreSQL 16.14; authoritative schema provenance and deployment verification remain outstanding  
**Rollout approval:** Not approved  
**Base commit:** `ac57a34cd0b5869aaa9580c4773c79e908aa4a95`  
**Implementation commit:** `f4493763a7d1eccc0487acfad6bbd2f54ed23124`  
**Branch/worktree:** `r1/pkg01-asset-auth` — `/home/navidrezadoost/Documents/Github/Logos-r1-pkg01-asset-auth`

## Changed-file scope

- `backend-go/internal/handler/assets.go` — resolve each supported asset bucket to its owning resource, apply resource-level access checks, fail closed for unmapped buckets, and prevent shared-cache storage of private bytes.
- `backend-go/internal/perms/perms.go` — exclude soft-deleted projects from project permission resolution.
- `backend-go/internal/assetauth/assets_integration_test.go` — isolated DB-backed access-control cases, kept outside the legacy handler test package so PKG-00 test-call-site blockers do not prevent urgent security verification.
- `backend-go/internal/handler/testdata/asset_auth_schema.sql` — minimal disposable schema used by focused CI; not a production schema/migration.
- `.github/workflows/asset-auth-integration.yml` — disposable PostgreSQL service and focused A31 test job.

No user data, storage objects, production database, or deployment configuration was changed.

## Authorization policy implemented

- No bucket name is globally public. The audited storage schema has no persistent per-object publication flag, so the anonymous-public bucket allowlist is empty.
- File media, tagged-object thumbnails and file thumbnails require current read access to a referencing file, or a valid **unscoped** share link for that exact file. Deleted parent files/projects cannot authorize access.
- Page-scoped share links are denied for raw asset retrieval until asset-to-page ownership can be verified. Deleted files cannot authorize a share.
- Team font bytes require team read membership. Project photos require project read membership. Profile photos are owner-only.
- Unknown/unmapped buckets (including the current `file-data-fragment` bucket without an audited owner relation) are denied. A future public asset must use explicit publication state/policy, not a bucket-name exception.
- Missing and unauthorized resources both return 404; content type and bytes are not written before authorization. Successful private responses use `Cache-Control: private, no-store`.

## Consumer/bypass review

The raw storage byte path found in this audit baseline is `/assets/by-id/{id}`. Go thumbnail/font/media RPCs return metadata/IDs and perform file/team checks; `.logos` export reads blobs only after file-read authorization; the share-view bundle was exercised with a valid share capability. The development Nginx `/assets` location proxies to the backend without `proxy_cache`; its extension-based static cache rule does not match `/assets/by-id/<uuid>`, and authorized responses now carry `private, no-store`. This source review is not a substitute for an inventory of every deployed proxy/CDN. Deployment exposure has not been independently established, and no production rollout is claimed.

## Executed evidence

Two isolated environments were used; neither copied user rows or modified an existing database:

1. `logos_r1_asset_auth_fixture_20261102`, initialized from `backend-go/internal/handler/testdata/asset_auth_schema.sql`, for the initial focused test.
2. A schema-only dump (`pg_dump --schema-only --no-owner --no-acl`) of the local `logos` catalog (PostgreSQL 17.2), restored without data into a disposable PostgreSQL 16.14 container bound to `127.0.0.1:55432`. The repository migration `backend-go/migrations/0001_project_description_photo.sql` was applied to that clone. The test-only dump removed the PostgreSQL 17+ `SET transaction_timeout` session line so the DDL could be restored by PostgreSQL 16.14. The clone/container have since been removed.

The local catalog contained the complete application tables inspected for this test, but the repository does not identify it as the canonical supported schema snapshot. The repository's available migrations were applied; a schema owner still needs to confirm snapshot provenance and exact supported minor version (the dev Compose file specifies PostgreSQL 16.8, while the isolated test used 16.14). The schema-only snapshot test was local evidence; the committed CI workflow still uses the minimal fixture until a canonical schema artifact is identified.

The clean baseline started with two unrelated Go test call-site compile failures in `internal/handler/files_test.go` and `projects_test.go`, and the full schema requires a test-only profile password. The A31 test is in a separate Go package and its focused integration command compiles and passes **without** an overlay or dependency on PKG-00. The broader backend suite used a temporary `-overlay` under `/tmp/opencode` for the two stale handler calls; the full-schema file/share tests also overlaid the profile test fixture to include its required password. These compatibility corrections are now present in the separate PKG-00 worktree. No overlay modified the audit/original worktrees.

```text
$ go build ./...
PASS (exit 0)

$ TEST_DATABASE_URL='postgres://postgres@127.0.0.1:5432/logos_r1_asset_auth_fixture_20261102?sslmode=disable' \
  go test ./internal/assetauth -run '^TestAssetByIDHandlerEnforcesResourceAuthorization$' -count=1 -v
=== RUN   TestAssetByIDHandlerEnforcesResourceAuthorization
--- PASS: TestAssetByIDHandlerEnforcesResourceAuthorization (0.06s)
PASS
ok   github.com/logos-design/logos/backend-go/internal/assetauth 0.062s

$ TEST_DATABASE_URL='postgres://logos:logos@127.0.0.1:55432/logos_test?sslmode=disable' \
  go test ./internal/assetauth -run '^TestAssetByIDHandlerEnforcesResourceAuthorization$' -count=1 -v
=== RUN   TestAssetByIDHandlerEnforcesResourceAuthorization
--- PASS: TestAssetByIDHandlerEnforcesResourceAuthorization (0.04s)
PASS
ok   github.com/logos-design/logos/backend-go/internal/assetauth 0.046s

$ TEST_DATABASE_URL='postgres://logos:logos@127.0.0.1:55432/logos_test?sslmode=disable' \
  go test -overlay=/tmp/opencode/pkg01-fullschema-overlay/overlay.json ./internal/handler \
  -run 'Test(CreateFileHandler_Integration|GetFileHandler_Integration|CreateShareLinkHandler_Integration)$' -count=1 -v
=== RUN   TestCreateFileHandler_Integration
--- PASS: TestCreateFileHandler_Integration (0.02s)
=== RUN   TestGetFileHandler_Integration
--- PASS: TestGetFileHandler_Integration (0.02s)
=== RUN   TestCreateShareLinkHandler_Integration
--- PASS: TestCreateShareLinkHandler_Integration (0.01s)
PASS
ok   github.com/logos-design/logos/backend-go/internal/handler 0.057s

$ go test -overlay=/tmp/opencode/pkg01-test-overlay/overlay.json ./...
PASS (all Go packages; DB-dependent tests skipped without TEST_DATABASE_URL)

$ go vet -overlay=/tmp/opencode/pkg01-test-overlay/overlay.json ./...
PASS (exit 0)

$ git diff --check
PASS (exit 0)

$ python3 YAML parse for .github/workflows/asset-auth-integration.yml
PASS
```

The test exercises owner and project-reader access; unrelated and revoked access; anonymous access with/without a file-scoped share; page-limited, revoked, deleted-file and deleted-project share denial; team membership for font bytes; profile/project photo scope; thumbnail scope; unmapped-bucket denial; view-only bundle access through a share; and absence of byte/content-type disclosure on denied responses. Existing create/get/share RPC tests also pass against the full-schema clone.

## Three-state package status

| State | PKG-01 status |
|---|---|
| **Implemented** | Yes — object/resource resolution and fail-closed policy are in the package branch. |
| **Verified** | Focused tests pass on a full local schema snapshot restored to PostgreSQL 16.14, plus the minimal CI fixture. The exact supported schema provenance/minor version, the same workflow against an authoritative schema artifact, alternate deployments and live exposure remain unverified. |
| **Rollout-approved** | No — wait for independent A31 exposure review, full-schema CI evidence, and explicit deployment approval. |

## Risks and rollout boundary

- Existing anonymous embeds that relied on bucket-wide public access will stop working unless they have a valid unscoped share capability or an explicit publication policy is separately implemented and approved.
- Page-scoped links cannot retrieve raw assets under this safe profile. Mapping assets to selected pages is outside PKG-01.
- The full-schema test used a schema-only snapshot from the local `logos` database, not an owner-certified production schema artifact. The CI workflow still uses the focused minimal schema; converting it to the canonical full schema awaits schema provenance confirmation.
- Until deployment reachability is checked, this code is not evidence that any exposed production endpoint is contained. If a private-asset route is currently public, restrict it immediately; do not wait for W0 completion.
