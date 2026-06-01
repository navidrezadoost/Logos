// Package handler — file deletion.
//
// Ported from app.rpc.commands.files/delete-file in the Clojure backend.
package handler

import (
	"net/http"
	"os"
	"strconv"
	"time"

	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/db"
	"github.com/logos-design/logos/backend-go/internal/perms"
)

func fileDeletionDelay() time.Duration {
	days := 7
	if v := os.Getenv("LOGOS_DELETION_DELAY_DAYS"); v != "" {
		if n, err := strconv.Atoi(v); err == nil && n > 0 {
			days = n
		}
	}
	return time.Duration(days) * 24 * time.Hour
}

// DeleteFileHandler soft-deletes a file (sets deleted_at to a future purge time).
func DeleteFileHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
			return
		}

		fileID := rpcParam(r, "id", "file-id", "fileId")
		if fileID == "" {
			writeError(w, http.StatusUnprocessableEntity, "id is required")
			return
		}

		fp, err := perms.GetFilePermissions(r.Context(), pool, profileID, fileID)
		if err != nil || fp == nil || !fp.CanEdit {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		deletedAt := time.Now().UTC().Add(fileDeletionDelay())
		tag, err := pool.Exec(r.Context(),
			`UPDATE file SET deleted_at = $1, modified_at = now()
			  WHERE id = $2 AND deleted_at IS NULL`,
			deletedAt, fileID)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		if tag.RowsAffected() == 0 {
			writeError(w, http.StatusNotFound, "file-not-found")
			return
		}

		_, _ = pool.Exec(r.Context(),
			`DELETE FROM file_library_rel WHERE library_file_id = $1`, fileID)

		writeJSON(w, http.StatusOK, map[string]any{})
	}
}
