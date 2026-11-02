// Package handler — project/file management operations.
//
// Ported from app.rpc.commands.management in the Clojure backend.
//
// # RPC surface
//
//	duplicate-project        Copy a project and all its files.
//	move-files               Reassign files to a different project.
//	move-project             Reassign a project to a different team.
//	get-builtin-templates    Return the list of built-in file templates.
//
// # duplicate-project
//
// Creates a new project in the same team with name "{original} (copy)".
// For each file in the source project a new file row is created (no CRDT
// copy — same limitation as duplicate-file: the file.data blob is not yet
// populated by the Go backend).  file_media_object rows are duplicated to
// reference the same storage objects (no byte copy — mirrors Clojure's clone).
// file_library_rel rows within the same project scope are carried over;
// cross-project library links are dropped (would create dangling refs).
//
// # move-files
//
// Reassigns the given file IDs to the destination project.  Cross-team moves
// will have their file_library_rel rows cleaned up (dangling team refs removed).
//
// # move-project
//
// Reassigns the entire project to a destination team.  Same library-rel cleanup.
//
// # get-builtin-templates
//
// Returns an empty list.  Clojure reads from ::setup/templates config; the Go
// backend has no template registry yet.  The frontend handles an empty list
// gracefully (shows no template picker).
package handler

import (
	"encoding/json"
	"net/http"
	"time"

	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/db"
	"github.com/logos-design/logos/backend-go/internal/perms"
)

// ─── GET /api/rpc/command/get-builtin-templates ──────────────────────────────

// GetBuiltinTemplatesHandler implements GET /api/rpc/command/get-builtin-templates.
// Returns an empty template list (no template registry configured yet).
func GetBuiltinTemplatesHandler(_ *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		writeJSON(w, http.StatusOK, []map[string]any{})
	}
}

// ─── POST /api/rpc/command/duplicate-project ─────────────────────────────────

type duplicateProjectParams struct {
	ProjectID string  `json:"projectId"`
	Name      *string `json:"name,omitempty"` // override name; default = "{original} (copy)"
}

// DuplicateProjectHandler implements POST /api/rpc/command/duplicate-project.
func DuplicateProjectHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
			return
		}

		var params duplicateProjectParams
		if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}
		if params.ProjectID == "" {
			writeError(w, http.StatusUnprocessableEntity, "projectId is required")
			return
		}

		pp, err := perms.GetProjectPermissions(r.Context(), pool, profileID, params.ProjectID)
		if err != nil || pp == nil || !pp.CanEdit {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		// Load source project.
		var srcName, teamID string
		if err = pool.QueryRow(r.Context(),
			`SELECT name, team_id::text FROM project WHERE id = $1 AND deleted_at IS NULL`,
			params.ProjectID,
		).Scan(&srcName, &teamID); err != nil {
			writeError(w, http.StatusNotFound, "project not found")
			return
		}

		newName := srcName + " (copy)"
		if params.Name != nil && *params.Name != "" {
			newName = *params.Name
		}

		tx, err := pool.Begin(r.Context())
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		defer tx.Rollback(r.Context()) //nolint:errcheck

		now := time.Now().UTC()
		newProjectID := newUUID()

		if _, err = tx.Exec(r.Context(),
			`INSERT INTO project (id, team_id, name, is_default, created_at, modified_at)
			 VALUES ($1, $2, $3, false, $4, $4)`,
			newProjectID, teamID, newName, now,
		); err != nil {
			writeError(w, http.StatusInternalServerError, "project insert failed")
			return
		}

		// Duplicate each file in the source project.
		fileRows, err := tx.Query(r.Context(),
			`SELECT id::text, name, is_shared FROM file
			  WHERE project_id = $1 AND deleted_at IS NULL`,
			params.ProjectID,
		)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "file query failed")
			return
		}

		type srcFile struct{ id, name string; isShared bool }
		var srcFiles []srcFile
		for fileRows.Next() {
			var f srcFile
			if err := fileRows.Scan(&f.id, &f.name, &f.isShared); err == nil {
				srcFiles = append(srcFiles, f)
			}
		}
		fileRows.Close()

		fileIDMap := make(map[string]string, len(srcFiles)) // old → new
		for _, f := range srcFiles {
			newFileID := newUUID()
			fileIDMap[f.id] = newFileID

			if _, err = tx.Exec(r.Context(),
				`INSERT INTO file
				   (id, project_id, name, is_shared, revn, vern, created_at, modified_at)
				 VALUES ($1, $2, $3, false, 0, 0, $4, $4)`,
				newFileID, newProjectID, f.name, now,
			); err != nil {
				continue
			}

			// Caller is owner of new file.
			_, _ = tx.Exec(r.Context(),
				`INSERT INTO file_profile_rel (file_id, profile_id, is_owner, is_admin, can_edit)
				 VALUES ($1, $2, true, true, true)`,
				newFileID, profileID,
			)

			// Clone media objects (reference same storage objects).
			_, _ = tx.Exec(r.Context(),
				`INSERT INTO file_media_object
				   (id, file_id, is_local, name, media_id, thumbnail_id, width, height, mtype, created_at)
				 SELECT gen_random_uuid(), $1, is_local, name, media_id, thumbnail_id,
				        width, height, mtype, $2
				   FROM file_media_object
				  WHERE file_id = $3 AND deleted_at IS NULL`,
				newFileID, now, f.id,
			)
		}

		// Copy intra-project library relations.
		for oldFileID, newFileID := range fileIDMap {
			if _, err = tx.Exec(r.Context(),
				`INSERT INTO file_library_rel (file_id, library_file_id)
				 SELECT $1, $2
				  WHERE EXISTS (
				    SELECT 1 FROM file_library_rel
				     WHERE file_id = $3 AND library_file_id = $4
				  )
				 ON CONFLICT DO NOTHING`,
				newFileID, fileIDMap[oldFileID], oldFileID, oldFileID,
			); err != nil {
				continue
			}
		}

		if err = tx.Commit(r.Context()); err != nil {
			writeError(w, http.StatusInternalServerError, "commit failed")
			return
		}

		writeJSON(w, http.StatusOK, map[string]any{
			"id":   newProjectID,
			"name": newName,
		})
	}
}

// ─── POST /api/rpc/command/move-files ────────────────────────────────────────

type moveFilesParams struct {
	IDs       []string `json:"ids"`
	ProjectID string   `json:"projectId"`
}

// MoveFilesHandler implements POST /api/rpc/command/move-files.
func MoveFilesHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
			return
		}

		var params moveFilesParams
		if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}
		if len(params.IDs) == 0 || params.ProjectID == "" {
			writeError(w, http.StatusUnprocessableEntity, "ids and projectId are required")
			return
		}

		// Destination project edit permission.
		pp, err := perms.GetProjectPermissions(r.Context(), pool, profileID, params.ProjectID)
		if err != nil || pp == nil || !pp.CanEdit {
			writeError(w, http.StatusForbidden, "insufficient-permissions on destination project")
			return
		}

		// Destination team.
		var destTeamID string
		if err = pool.QueryRow(r.Context(),
			`SELECT team_id::text FROM project WHERE id = $1 AND deleted_at IS NULL`,
			params.ProjectID,
		).Scan(&destTeamID); err != nil {
			writeError(w, http.StatusNotFound, "destination project not found")
			return
		}

		tx, err := pool.Begin(r.Context())
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		defer tx.Rollback(r.Context()) //nolint:errcheck

		now := time.Now().UTC()

		// Move files.
		for _, fileID := range params.IDs {
			fp, err := perms.GetFilePermissions(r.Context(), pool, profileID, fileID)
			if err != nil || fp == nil || !fp.CanEdit {
				continue // skip files without permission
			}
			_, _ = tx.Exec(r.Context(),
				`UPDATE file SET project_id = $1, modified_at = $2 WHERE id = $3`,
				params.ProjectID, now, fileID,
			)
		}

		// Remove cross-team library relations for moved files.
		for _, fileID := range params.IDs {
			_, _ = tx.Exec(r.Context(),
				`DELETE FROM file_library_rel
				  WHERE (file_id = $1 OR library_file_id = $1)
				    AND (
				      SELECT team_id FROM project p
				       JOIN file f ON f.project_id = p.id
				      WHERE f.id = $1
				    ) != (
				      SELECT team_id FROM project p
				       JOIN file f ON f.project_id = p.id
				      WHERE f.id = CASE WHEN file_id = $1 THEN library_file_id ELSE file_id END
				    )`,
				fileID,
			)
		}

		_, _ = tx.Exec(r.Context(),
			`UPDATE project SET modified_at = $1 WHERE id = $2`, now, params.ProjectID,
		)

		if err = tx.Commit(r.Context()); err != nil {
			writeError(w, http.StatusInternalServerError, "commit failed")
			return
		}

		writeJSON(w, http.StatusOK, map[string]any{})
	}
}

// ─── POST /api/rpc/command/move-project ──────────────────────────────────────

type moveProjectParams struct {
	ProjectID string `json:"projectId"`
	TeamID    string `json:"teamId"` // destination team
}

// MoveProjectHandler implements POST /api/rpc/command/move-project.
func MoveProjectHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
			return
		}

		var params moveProjectParams
		if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}
		if params.ProjectID == "" || params.TeamID == "" {
			writeError(w, http.StatusUnprocessableEntity, "projectId and teamId are required")
			return
		}

		// Must be owner/admin of source project.
		pp, err := perms.GetProjectPermissions(r.Context(), pool, profileID, params.ProjectID)
		if err != nil || pp == nil || !pp.IsOwner {
			writeError(w, http.StatusForbidden, "insufficient-permissions on source project")
			return
		}

		// Must be member of destination team.
		if !teamMember(r.Context(), pool, profileID, params.TeamID) {
			writeError(w, http.StatusForbidden, "insufficient-permissions on destination team")
			return
		}

		now := time.Now().UTC()

		tx, err := pool.Begin(r.Context())
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		defer tx.Rollback(r.Context()) //nolint:errcheck

		if _, err = tx.Exec(r.Context(),
			`UPDATE project SET team_id = $1, modified_at = $2
			  WHERE id = $3 AND deleted_at IS NULL`,
			params.TeamID, now, params.ProjectID,
		); err != nil {
			writeError(w, http.StatusInternalServerError, "project update failed")
			return
		}

		// Remove library relations that now span different teams.
		_, _ = tx.Exec(r.Context(),
			`DELETE FROM file_library_rel flr
			  USING file f_src, project p_src,
			        file f_lib, project p_lib
			  WHERE flr.file_id = f_src.id
			    AND f_src.project_id = p_src.id
			    AND flr.library_file_id = f_lib.id
			    AND f_lib.project_id = p_lib.id
			    AND p_src.team_id != p_lib.team_id
			    AND (p_src.id = $1 OR p_lib.id = $1)`,
			params.ProjectID,
		)

		if err = tx.Commit(r.Context()); err != nil {
			writeError(w, http.StatusInternalServerError, "commit failed")
			return
		}

		writeJSON(w, http.StatusOK, map[string]any{})
	}
}
