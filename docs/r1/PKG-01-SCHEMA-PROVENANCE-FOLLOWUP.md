# R1 PKG-01 — Schema Provenance Follow-up

**Status:** Full-schema test evidence reproduced; canonical schema provenance still open
**Base commit:** `ddae16ad048cf429b386f14e66095ba8faddb256` (`origin/main`)
**Branch/worktree:** `r1/pkg01-schema-followup` — `/home/navidrezadoost/Documents/Github/Logos-r1-pkg01-schema-followup`

## Read-only schema inventory

- Local `logos` catalog reports PostgreSQL 17.2 and contains the application tables exercised by PKG-01 (`storage_object`, `file_media_object`, both thumbnail tables, `team_font_variant`, `file`, `project`, `profile`, and `share_link`).
- A `pg_dump --schema-only --no-owner --no-acl` export was restored into a disposable PostgreSQL 16.14 container. No table rows were exported/copied. Repository migration `backend-go/migrations/0001_project_description_photo.sql` was applied to the clone before the tests.
- The checked-in `backend-go/migrations/` currently contains only migration 0001. The local catalog has `file_migration`, which is per-document migration state; no schema-migration history table or canonical schema snapshot is present in this checkout.
- `docker/devenv` declares PostgreSQL 16.8. The full-schema test used 16.14 (same supported major line, different patch version). The local source catalog was 17.2; its owner/provenance is not established by repository evidence.

## Repository initialization-path audit

- The only checked-in SQL file under `backend-go/migrations/` is `0001_project_description_photo.sql`; it adds two columns to an already-existing `project` table. No backend-Go schema bootstrap or migration runner is present in `cmd/server` or `internal/`.
- `docker/devenv/files/postgresql_init.sql`, mounted by Compose into PostgreSQL's `docker-entrypoint-initdb.d`, only creates `logos_test` and `logos_telemetry`. Compose creates the `logos` database through `POSTGRES_DB=logos`; it does not load the application tables.
- The top-level README says `go run ./cmd/server -migrate`, but `backend-go/cmd/server/main.go` has no migration-flag handling; it starts the API after connecting to a pre-existing schema. Treat that instruction as stale, not as an approved initialization path.
- The only full table DDL in the current tree is `backend-go/internal/handler/testdata/asset_auth_schema.sql`, explicitly documented as a minimal test harness, not a complete production schema.

Git history contains the legacy Clojure runner and migration resources under `backend/src/app/migrations/` immediately before Go-backend transition commit `ea3b966b96a95e6d258e0e5f259529865195a167`, whose message says it removes the legacy Clojure backend. The historical tree has 151 SQL files; the runner has 151 ordered steps, of which 149 reference SQL resources and two invoke Clojure functions (`0023-adapt-old-pages-and-files`, `0145-fix-plugins-uri-on-profile`). Two SQL files in that tree are not referenced by the runner (`0122-mod-file-fragment-table.sql`, `XXXX-drop-obsolete-tables.sql`). The sequence uses explicit runner order, has numeric gaps and duplicate prefixes (`0105`, `0106`, `0107`, `0122`, `0140`, `0141`); sorting filenames is not a valid replay plan.

This history is a candidate lineage source, not a current Go deployment initializer. Replaying only SQL omits the two Clojure steps and would not reproduce the runner's history. The legacy backend and migrations were removed from the current main tree during the Go transition.

### Legacy objects relevant to PKG-01

| Historical step | Relevant object(s) introduced/changed |
|---|---|
| `0002-add-profile-tables.sql` | `profile`, `team`, `team_profile_rel` |
| `0003-add-project-tables.sql` | `project`, `project_profile_rel`, `file`, `file_profile_rel` |
| `0028-add-team-project-profile-rel-table.sql` | `team_project_profile_rel` (project permission relation) |
| `0035-add-storage-tables.sql`, `0036-mod-storage-referenced-tables.sql` | `storage_object`; profile/team photo references |
| `0053-add-team-font-variant-table.sql` | `team_font_variant` |
| `0063-add-share-link-table.sql` | `share_link` |
| `0069-add-file-thumbnail-table.sql`, `0071-add-file-object-thumbnail-table.sql`, `0106-add-file-tagged-object-thumbnail-table.sql` | `file_thumbnail`, `file_object_thumbnail`, `file_tagged_object_thumbnail` |
| `0137-add-file-migration-table.sql` | Per-file `file_migration` state |
| `0141-add-file-data-table.sql`, `0146-add-file-data-page.sql` | `file_data` and `file_data_page` storage tables |

The local PostgreSQL 17.2 catalog contains these selected PKG-01 authorization/asset tables, plus `file_data`, `file_data_page`, and `file_migration`. Local `file_migration` columns are `file_id uuid`, `name text`, `created_at timestamptz`, matching the historical 0137 definition. This is a concrete lineage match, but not a schema-version record or proof that every migration step ran.

The local source snapshot also differs from the legacy chain in a concrete Go-specific way: its `project` table has neither `description` nor `photo_id`. Historical migrations add `photo_id` to `profile` and `team`, not `project`. The PKG-01 integration clone had `backend-go/migrations/0001_project_description_photo.sql` applied, adding `project.description` and `project.photo_id`; therefore the test clone is not byte-for-byte the source catalog.

### Current Go query dependencies

- `backend-go/internal/perms/perms.go` and `internal/handler/projects.go` use `team_profile_rel`, `team_project_profile_rel`, `project_profile_rel`, `team`, `project`, and `file` for membership/project authorization.
- `internal/handler/assets.go` resolves `storage_object`, `file_media_object`, `file_tagged_object_thumbnail`, `file_thumbnail`, `team_font_variant`, `profile`, `project`, `share_link`, and parent file/project relations before returning bytes.
- `internal/handler/viewer.go`, `files_share.go`, `fonts.go`, `media.go`, and `files_thumbnails.go` query the corresponding team/project/file/share/font/media relations. The A31 regression test exercises these access paths against the full local schema clone.

The historical chain and current local snapshot align on these selected objects. A full constraint/index/default reconciliation between the 149 referenced SQL resources plus two Clojure migrations and the PostgreSQL 17.2 catalog has **not** been completed. Neither the local catalog nor current Go repository provides a trusted deployed migration version/checksum. The schema owner must confirm lineage and deployed state before canonical-schema status can be asserted.

Therefore the current checkout has no approved path to build a fresh complete Go application schema. I did not create or modify a database to fill that gap and did not infer migration history. A schema owner must confirm whether the historical Clojure chain is the intended lineage, provide the deployed migration state, and certify the supported schema before tests can be rerun against a fresh authoritative disposable database.

## Verification results

Against the isolated full-schema clone, the A31 access test passed. It includes owner/project-reader, unrelated/revoked membership, share revocation/page scope, deleted parents, file media, thumbnails, fonts, project/profile photos, preview metadata, export authorization, and share-view bundle access.

The existing `TestCreateFileHandler_Integration`, `TestGetFileHandler_Integration`, and `TestCreateShareLinkHandler_Integration` tests also passed against the same schema. No production database or source data was touched. Test outputs and exact commands remain in `PKG-01-VERIFICATION-EVIDENCE.md` on the initial package branch.

## Remaining gate

This confirms compatibility with the available local full-schema snapshot, not that it is the canonical schema for supported deployments. A schema owner must identify the authoritative schema/migration source and confirm the supported PostgreSQL version before PKG-01 is closed. The automated asset-auth workflow still uses its minimal focused schema because no canonical full-schema artifact is checked in.

No production exposure assessment or rollout approval is implied.
