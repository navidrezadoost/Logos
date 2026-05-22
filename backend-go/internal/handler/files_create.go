// Package handler — file creation handlers.
//
// Ported from app.rpc.commands.files-create in the Clojure backend.
// Creates file records + initial ownership entries.
// File data/content setup (CRDT page structure) is handled by files_update.
package handler

import (
	"encoding/json"
	"net/http"

	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/db"
	"github.com/logos-design/logos/backend-go/internal/perms"
)

// ─── POST /api/rpc/command/create-file ───────────────────────────────────────

// CreateFileHandler creates a new file in a project and makes the caller its owner.
func CreateFileHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeError(w, http.StatusUnauthorized, "unauthorized")
			return
		}

		var body struct {
			ProjectID string  `json:"projectId"`
			Name      string  `json:"name"`
			ID        string  `json:"id,omitempty"`
			IsShared  bool    `json:"isShared,omitempty"`
		}
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}
		if body.ProjectID == "" || body.Name == "" {
			writeError(w, http.StatusBadRequest, "projectId and name required")
			return
		}

		// File creation requires project edit permissions.
		p, err := perms.GetProjectPermissions(r.Context(), pool, profileID, body.ProjectID)
		if err != nil || p == nil || !p.CanEdit {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		fileID := body.ID
		if fileID == "" {
			fileID = newUUID()
		}

		tx, err := pool.Begin(r.Context())
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		defer tx.Rollback(r.Context()) //nolint:errcheck

		// Insert the file record.
		if _, err := tx.Exec(r.Context(), `
			INSERT INTO file (id, project_id, name, is_shared)
			VALUES ($1, $2, $3, $4)`,
			fileID, body.ProjectID, body.Name, body.IsShared); err != nil {
			writeError(w, http.StatusInternalServerError, "create file failed")
			return
		}

		// Grant owner role on the file to the creator.
		if _, err := tx.Exec(r.Context(), `
			INSERT INTO file_profile_rel (file_id, profile_id, is_owner, is_admin, can_edit)
			VALUES ($1, $2, true, true, true)`,
			fileID, profileID); err != nil {
			writeError(w, http.StatusInternalServerError, "create file role failed")
			return
		}

		// Touch the parent project modified_at.
		if _, err := tx.Exec(r.Context(), `
			UPDATE project SET modified_at = now() WHERE id = $1`, body.ProjectID); err != nil {
			writeError(w, http.StatusInternalServerError, "update project failed")
			return
		}

		if err := tx.Commit(r.Context()); err != nil {
			writeError(w, http.StatusInternalServerError, "commit failed")
			return
		}

		var f File
		_ = pool.QueryRow(r.Context(), `
			SELECT id, project_id, name, is_shared, revn, vern,
			       comment_thread_seqn, NULL::uuid AS thumbnail_id,
			       created_at, modified_at, deleted_at
			  FROM file WHERE id = $1`, fileID).
			Scan(&f.ID, &f.ProjectID, &f.Name, &f.IsShared, &f.Revn, &f.Vern,
				&f.CommentThreadSeqn, &f.ThumbnailID, &f.CreatedAt, &f.ModifiedAt, &f.DeletedAt)

		writeJSON(w, http.StatusOK, &f)
	}
}

// ─── POST /api/rpc/command/duplicate-file ────────────────────────────────────

// DuplicateFileHandler creates a copy of an existing file (metadata only).
// The file data blob is NOT copied here — that requires the files_update CRDT layer.
func DuplicateFileHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeError(w, http.StatusUnauthorized, "unauthorized")
			return
		}

		var body struct {
			FileID    string `json:"fileId"`
			Name      string `json:"name,omitempty"`
			ProjectID string `json:"projectId,omitempty"`
		}
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}
		if body.FileID == "" {
			writeError(w, http.StatusBadRequest, "fileId required")
			return
		}

		// Caller must be able to read the source file.
		if !perms.CheckFileRead(w, r, pool, profileID, body.FileID) {
			return
		}

		// Read source file.
		var src File
		err := pool.QueryRow(r.Context(), `
			SELECT id, project_id, name, is_shared, revn, vern,
			       comment_thread_seqn, NULL::uuid,
			       created_at, modified_at, deleted_at
			  FROM file WHERE id = $1 AND deleted_at IS NULL`, body.FileID).
			Scan(&src.ID, &src.ProjectID, &src.Name, &src.IsShared, &src.Revn, &src.Vern,
				&src.CommentThreadSeqn, &src.ThumbnailID, &src.CreatedAt, &src.ModifiedAt, &src.DeletedAt)
		if err != nil {
			writeError(w, http.StatusNotFound, "file-not-found")
			return
		}

		targetProjectID := src.ProjectID
		if body.ProjectID != "" {
			targetProjectID = body.ProjectID
		}
		// Verify edit permissions on destination project.
		pp, err := perms.GetProjectPermissions(r.Context(), pool, profileID, targetProjectID)
		if err != nil || pp == nil || !pp.CanEdit {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		newName := src.Name + " (copy)"
		if body.Name != "" {
			newName = body.Name
		}
		newID := newUUID()

		tx, err := pool.Begin(r.Context())
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		defer tx.Rollback(r.Context()) //nolint:errcheck

		if _, err := tx.Exec(r.Context(), `
			INSERT INTO file (id, project_id, name, is_shared)
			VALUES ($1, $2, $3, false)`,
			newID, targetProjectID, newName); err != nil {
			writeError(w, http.StatusInternalServerError, "duplicate file failed")
			return
		}

		if _, err := tx.Exec(r.Context(), `
			INSERT INTO file_profile_rel (file_id, profile_id, is_owner, is_admin, can_edit)
			VALUES ($1, $2, true, true, true)`,
			newID, profileID); err != nil {
			writeError(w, http.StatusInternalServerError, "create file role failed")
			return
		}

		if err := tx.Commit(r.Context()); err != nil {
			writeError(w, http.StatusInternalServerError, "commit failed")
			return
		}

		var f File
		_ = pool.QueryRow(r.Context(), `
			SELECT id, project_id, name, is_shared, revn, vern,
			       comment_thread_seqn, NULL::uuid,
			       created_at, modified_at, deleted_at
			  FROM file WHERE id = $1`, newID).
			Scan(&f.ID, &f.ProjectID, &f.Name, &f.IsShared, &f.Revn, &f.Vern,
				&f.CommentThreadSeqn, &f.ThumbnailID, &f.CreatedAt, &f.ModifiedAt, &f.DeletedAt)

		writeJSON(w, http.StatusOK, &f)
	}
}
