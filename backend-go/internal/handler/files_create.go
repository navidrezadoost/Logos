// Package handler — file creation handlers.
//
// Ported from app.rpc.commands.files-create in the Clojure backend.
// Creates file records + initial ownership entries.
// File data/content setup (CRDT page structure) is handled by files_update.
package handler

import (
	"context"
	"encoding/json"
	"net/http"

	"github.com/jackc/pgx/v5"

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
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
			return
		}

		var body map[string]any
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}
		projectID := jsonFieldString(body, "projectId", "project-id")
		name := jsonFieldString(body, "name")
		fileID := jsonFieldString(body, "id")
		isShared := jsonFieldBool(body, "isShared", "is-shared")
		features := jsonFieldStringSlice(body, "features")
		if projectID == "" || name == "" {
			writeError(w, http.StatusBadRequest, "projectId and name required")
			return
		}

		// File creation requires project edit permissions.
		p, err := perms.GetProjectPermissions(r.Context(), pool, profileID, projectID)
		if err != nil || p == nil || !p.CanEdit {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		if fileID == "" {
			fileID = newUUID()
		}

		tx, err := pool.Begin(r.Context())
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		defer tx.Rollback(r.Context()) //nolint:errcheck

		created, err := insertFileInTx(r.Context(), tx, profileID, projectID, name, fileID, isShared, features)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "create file failed")
			return
		}

		if err := tx.Commit(r.Context()); err != nil {
			writeError(w, http.StatusInternalServerError, "commit failed")
			return
		}

		loadCreatedFile(r.Context(), pool, &created)
		writeJSON(w, http.StatusOK, &created)
	}
}

func loadCreatedFile(ctx context.Context, pool *db.Pool, created *CreatedFile) {
	_ = pool.QueryRow(ctx, `
		SELECT id, project_id, name, is_shared, revn, vern,
		       comment_thread_seqn, NULL::uuid AS thumbnail_id,
		       created_at, modified_at, deleted_at
		  FROM file WHERE id = $1`, created.ID).
		Scan(&created.ID, &created.ProjectID, &created.Name, &created.IsShared,
			&created.Revn, &created.Vern, &created.CommentThreadSeqn, &created.ThumbnailID,
			&created.CreatedAt, &created.ModifiedAt, &created.DeletedAt)
}

// starterFileName is the default design file seeded into new projects.
const starterFileName = "New file"

func seedStarterFile(ctx context.Context, tx pgx.Tx, profileID, projectID, fileName string) (CreatedFile, error) {
	if fileName == "" {
		fileName = starterFileName
	}
	return insertFileInTx(ctx, tx, profileID, projectID, fileName, "", false, nil)
}

func seedStarterFileStandalone(ctx context.Context, pool *db.Pool, profileID, projectID string) error {
	tx, err := pool.Begin(ctx)
	if err != nil {
		return err
	}
	defer tx.Rollback(ctx) //nolint:errcheck
	if _, err := seedStarterFile(ctx, tx, profileID, projectID, ""); err != nil {
		return err
	}
	return tx.Commit(ctx)
}

// ─── POST /api/rpc/command/duplicate-file ────────────────────────────────────

// DuplicateFileHandler creates a copy of an existing file (metadata only).
// The file data blob is NOT copied here — that requires the files_update CRDT layer.
func DuplicateFileHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
			return
		}

		var body map[string]any
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}
		sourceFileID := jsonFieldString(body, "fileId", "file-id")
		name := jsonFieldString(body, "name")
		targetProjectID := jsonFieldString(body, "projectId", "project-id")
		if sourceFileID == "" {
			writeError(w, http.StatusBadRequest, "fileId required")
			return
		}

		// Caller must be able to read the source file.
		if !perms.CheckFileRead(w, r, pool, profileID, sourceFileID) {
			return
		}

		// Read source file.
		var src File
		err := pool.QueryRow(r.Context(), `
			SELECT id, project_id, name, is_shared, revn, vern,
			       comment_thread_seqn, NULL::uuid,
			       created_at, modified_at, deleted_at
			  FROM file WHERE id = $1 AND deleted_at IS NULL`, sourceFileID).
			Scan(&src.ID, &src.ProjectID, &src.Name, &src.IsShared, &src.Revn, &src.Vern,
				&src.CommentThreadSeqn, &src.ThumbnailID, &src.CreatedAt, &src.ModifiedAt, &src.DeletedAt)
		if err != nil {
			writeError(w, http.StatusNotFound, "file-not-found")
			return
		}

		destProjectID := src.ProjectID
		if targetProjectID != "" {
			destProjectID = targetProjectID
		}
		// Verify edit permissions on destination project.
		pp, err := perms.GetProjectPermissions(r.Context(), pool, profileID, destProjectID)
		if err != nil || pp == nil || !pp.CanEdit {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		newName := src.Name + " (copy)"
		if name != "" {
			newName = name
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
			newID, destProjectID, newName); err != nil {
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
