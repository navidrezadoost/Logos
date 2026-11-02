# R1 PKG-01 — Full-Schema Asset Authorization Verification

**Status:** Focused verification passed; canonical schema/deployment provenance remains open  
**Base commit:** `d387cc83fd3f45d1ffe019bf86b1c52dcf27c866` (`origin/main`, after PKG-00–02 merges)  
**Verification commit:** `f0ac2795118767b6045d8c36fec9a79174673520`  
**Branch/worktree:** `r1/pkg01-verification` — `/home/navidrezadoost/Documents/Github/Logos-r1-pkg01-verification`

## Schema provenance and isolation

For verification, a **schema-only** (`pg_dump --schema-only --no-owner --no-acl`) dump of the local `logos` database (PostgreSQL 17.2) was restored, without rows, into a disposable PostgreSQL 16.14 container. The repository migration `backend-go/migrations/0001_project_description_photo.sql` was then applied. No source application data or existing database was copied or modified. The test container was removed after use.

This snapshot contains the application tables exercised by the auth and file/share paths, but no schema owner has certified it as the canonical supported schema. The development Compose file specifies PostgreSQL 16.8; verification used 16.14. Treat this as strong compatibility evidence, not final schema-provenance approval.

## Tests and outcomes

```text
$ TEST_DATABASE_URL='postgres://logos:logos@127.0.0.1:55432/logos_test?sslmode=disable' \
  go test ./internal/assetauth -run '^TestAssetByIDHandlerEnforcesResourceAuthorization$' -count=1 -v
=== RUN   TestAssetByIDHandlerEnforcesResourceAuthorization
--- PASS: TestAssetByIDHandlerEnforcesResourceAuthorization (0.05s)
PASS
ok   github.com/logos-design/logos/backend-go/internal/assetauth 0.055s

$ TEST_DATABASE_URL='postgres://logos:logos@127.0.0.1:55432/logos_test?sslmode=disable' \
  go test ./internal/handler -run 'Test(CreateFileHandler_Integration|GetFileHandler_Integration|CreateShareLinkHandler_Integration)$' -count=1 -v
=== RUN   TestCreateFileHandler_Integration
--- PASS: TestCreateFileHandler_Integration (0.03s)
=== RUN   TestGetFileHandler_Integration
--- PASS: TestGetFileHandler_Integration (0.02s)
=== RUN   TestCreateShareLinkHandler_Integration
--- PASS: TestCreateShareLinkHandler_Integration (0.02s)
PASS
ok   github.com/logos-design/logos/backend-go/internal/handler 0.067s
```

The A31 integration test covers file owner/project-reader access, unrelated and revoked membership, anonymous requests, full-file share capabilities and revocation, page-scoped share denial, deleted-file/project denial, media and thumbnail metadata, thumbnail-preview data, team-font metadata/bytes, project/profile photos, native archive export authorization, share-view bundle access, unmapped buckets, private `Cache-Control`, and denied-response metadata/byte disclosure.

## Alternate-route/cache review

- Source search found one raw storage-byte handler: `/assets/by-id/{id}`. All mapped buckets are resolved to their file, team, project or profile owners before storage `Get`.
- Media/font/thumbnail RPCs return metadata or IDs and have their own file/team permission checks. `.logos` export checks file-read permission before loading blobs.
- The development Nginx `/assets` location proxies to the Go service without `proxy_cache`; its extension cache rule does not match `/assets/by-id/<uuid>`. Authorized asset responses use `Cache-Control: private, no-store`.
- This is source/config review, not an external CDN/proxy deployment audit. The full running ingress path remains unverified.

## Remaining compatibility/security consequences

- There is no bucket-wide anonymous-public exception. Existing embeds that relied on it must use a valid unscoped share capability or wait for an explicit publication policy.
- Page-scoped share links are deliberately denied for raw asset access until asset-to-page ownership is represented and checked.
- The CI asset workflow still uses the small focused test schema because no owner-certified full schema artifact exists in the repository. Schema owner confirmation and repeatable CI against the authoritative snapshot remain required.
- No production database, storage bytes, or deployment was modified. Production exposure is still unknown; no rollout is approved.

## Three-state status

| State | PKG-01 verification |
|---|---|
| **Implemented** | Resource-scoped policy and asset handler are merged in `main`; this branch adds fuller verification coverage. |
| **Verified** | Focused asset and existing create/get/share-link tests pass against the isolated full-schema snapshot; schema provenance and production ingress remain unverified. |
| **Rollout-approved** | No. |
