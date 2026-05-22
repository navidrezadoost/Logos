// Package handler — file metadata handlers.
//
// Ported from app.rpc.commands.files in the Clojure backend.
// Covers METADATA-ONLY operations:
//   get-file, get-project-files, get-file-libraries,
//   get-file-collaborators, update-file-metadata.
//
// Does NOT touch file data/content — that is files_update.
package handler

import (
	"encoding/json"
	"net/http"
	"time"

	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/db"
	"github.com/logos-design/logos/backend-go/internal/perms"
)

// File is the JSON-serialisable file record (metadata only).
type File struct {
	ID                string     `json:"id"`
	ProjectID         string     `json:"projectId"`
	Name              string     `json:"name"`
	IsShared          bool       `json:"isShared"`
	Revn              int        `json:"revn"`
	Vern              int        `json:"vern"`
	CommentThreadSeqn int        `json:"commentThreadSeqn"`
	ThumbnailID       *string    `json:"thumbnailId,omitempty"`
	CreatedAt         time.Time  `json:"createdAt"`
	ModifiedAt        time.Time  `json:"modifiedAt"`
	DeletedAt         *time.Time `json:"deletedAt,omitempty"`
}

// FileCollaborator is a profile that has explicit access to a file.
type FileCollaborator struct {
	ID       string `json:"id"`
	Name     string `json:"name"`
	Email    string `json:"email"`
	IsOwner  bool   `json:"isOwner"`
	IsAdmin  bool   `json:"isAdmin"`
	CanEdit  bool   `json:"canEdit"`
}

// FileLibrary is a minimal record for a library file linked to a file.
type FileLibrary struct {
	ID         string    `json:"id"`
	Name       string    `json:"name"`
	ProjectID  string    `json:"projectId"`
	ModifiedAt time.Time `json:"modifiedAt"`
}

// ─── GET /api/rpc/command/get-file ───────────────────────────────────────────

// GetFileHandler returns a single file by ?id=.
// Access is granted when the caller has project-level read permission on
// the file's parent project. Share-link access is not checked here (that's
// the viewer endpoint).
func GetFileHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeError(w, http.StatusUnauthorized, "unauthorized")
			return
		}
		fileID := r.URL.Query().Get("id")
		if fileID == "" {
			writeError(w, http.StatusBadRequest, "id required")
			return
		}
		if !perms.CheckFileRead(w, r, pool, profileID, fileID) {
			return
		}

		var f File
		err := pool.QueryRow(r.Context(), `
			SELECT f.id, f.project_id, f.name, f.is_shared, f.revn, f.vern,
			       f.comment_thread_seqn,
			       ft.media_id AS thumbnail_id,
			       f.created_at, f.modified_at, f.deleted_at
			  FROM file AS f
			  LEFT JOIN file_thumbnail AS ft ON (ft.file_id = f.id
			            AND ft.revn = f.revn AND ft.deleted_at IS NULL)
			 WHERE f.id = $1 AND f.deleted_at IS NULL`, fileID).
			Scan(&f.ID, &f.ProjectID, &f.Name, &f.IsShared, &f.Revn, &f.Vern,
				&f.CommentThreadSeqn, &f.ThumbnailID,
				&f.CreatedAt, &f.ModifiedAt, &f.DeletedAt)
		if err != nil {
			writeError(w, http.StatusNotFound, "file-not-found")
			return
		}
		writeJSON(w, http.StatusOK, &f)
	}
}

// ─── GET /api/rpc/command/get-project-files ──────────────────────────────────

// GetProjectFilesHandler lists all non-deleted files in a project.
func GetProjectFilesHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeError(w, http.StatusUnauthorized, "unauthorized")
			return
		}
		projectID := r.URL.Query().Get("project-id")
		if projectID == "" {
			writeError(w, http.StatusBadRequest, "project-id required")
			return
		}

		p, err := perms.GetProjectPermissions(r.Context(), pool, profileID, projectID)
		if err != nil || p == nil || !p.CanRead {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		rows, err := pool.Query(r.Context(), `
			SELECT f.id, f.project_id, f.name, f.is_shared, f.revn, f.vern,
			       f.comment_thread_seqn,
			       ft.media_id AS thumbnail_id,
			       f.created_at, f.modified_at, f.deleted_at
			  FROM file AS f
			  JOIN project AS p ON (p.id = f.project_id)
			  LEFT JOIN file_thumbnail AS ft ON (ft.file_id = f.id
			            AND ft.revn = f.revn AND ft.deleted_at IS NULL)
			 WHERE f.project_id = $1
			   AND f.deleted_at IS NULL
			 ORDER BY f.modified_at DESC`, projectID)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		defer rows.Close()

		var files []*File
		for rows.Next() {
			f := &File{}
			if err := rows.Scan(&f.ID, &f.ProjectID, &f.Name, &f.IsShared, &f.Revn, &f.Vern,
				&f.CommentThreadSeqn, &f.ThumbnailID,
				&f.CreatedAt, &f.ModifiedAt, &f.DeletedAt); err != nil {
				writeError(w, http.StatusInternalServerError, "internal server error")
				return
			}
			files = append(files, f)
		}
		if err := rows.Err(); err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		writeJSON(w, http.StatusOK, files)
	}
}

// ─── GET /api/rpc/command/get-file-libraries ─────────────────────────────────

// GetFileLibrariesHandler returns the library files linked to the given file.
func GetFileLibrariesHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeError(w, http.StatusUnauthorized, "unauthorized")
			return
		}
		fileID := r.URL.Query().Get("file-id")
		if fileID == "" {
			writeError(w, http.StatusBadRequest, "file-id required")
			return
		}
		if !perms.CheckFileRead(w, r, pool, profileID, fileID) {
			return
		}

		rows, err := pool.Query(r.Context(), `
			SELECT lf.id, lf.name, lf.project_id, lf.modified_at
			  FROM file_library_rel AS flr
			  JOIN file AS lf ON (lf.id = flr.library_file_id)
			 WHERE flr.file_id = $1
			   AND (lf.deleted_at IS NULL OR lf.deleted_at > now())`, fileID)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		defer rows.Close()

		var libs []*FileLibrary
		for rows.Next() {
			lib := &FileLibrary{}
			if err := rows.Scan(&lib.ID, &lib.Name, &lib.ProjectID, &lib.ModifiedAt); err != nil {
				writeError(w, http.StatusInternalServerError, "internal server error")
				return
			}
			libs = append(libs, lib)
		}
		if err := rows.Err(); err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		writeJSON(w, http.StatusOK, libs)
	}
}

// ─── GET /api/rpc/command/get-file-collaborators ─────────────────────────────

// GetFileCollaboratorsHandler returns profiles with explicit file-level access via
// file_profile_rel.
func GetFileCollaboratorsHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeError(w, http.StatusUnauthorized, "unauthorized")
			return
		}
		fileID := r.URL.Query().Get("file-id")
		if fileID == "" {
			writeError(w, http.StatusBadRequest, "file-id required")
			return
		}
		if !perms.CheckFileRead(w, r, pool, profileID, fileID) {
			return
		}

		rows, err := pool.Query(r.Context(), `
			SELECT p.id, COALESCE(p.fullname, p.name, '') AS name, p.email,
			       fpr.is_owner, fpr.is_admin, fpr.can_edit
			  FROM file_profile_rel AS fpr
			  JOIN profile AS p ON (p.id = fpr.profile_id)
			 WHERE fpr.file_id = $1`, fileID)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		defer rows.Close()

		var collabs []*FileCollaborator
		for rows.Next() {
			c := &FileCollaborator{}
			if err := rows.Scan(&c.ID, &c.Name, &c.Email, &c.IsOwner, &c.IsAdmin, &c.CanEdit); err != nil {
				writeError(w, http.StatusInternalServerError, "internal server error")
				return
			}
			collabs = append(collabs, c)
		}
		if err := rows.Err(); err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		writeJSON(w, http.StatusOK, collabs)
	}
}

// ─── PATCH /api/rpc/command/update-file-metadata ─────────────────────────────

// UpdateFileMetadataHandler updates the name and/or is_shared flag of a file.
func UpdateFileMetadataHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeError(w, http.StatusUnauthorized, "unauthorized")
			return
		}

		var body struct {
			ID       string  `json:"id"`
			Name     *string `json:"name,omitempty"`
			IsShared *bool   `json:"isShared,omitempty"`
		}
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}
		if body.ID == "" {
			writeError(w, http.StatusBadRequest, "id required")
			return
		}
		if !perms.CheckFileEdit(w, r, pool, profileID, body.ID) {
			return
		}

		if body.Name != nil {
			if _, err := pool.Exec(r.Context(),
				`UPDATE file SET name = $1, modified_at = now() WHERE id = $2`,
				*body.Name, body.ID); err != nil {
				writeError(w, http.StatusInternalServerError, "update file name failed")
				return
			}
		}
		if body.IsShared != nil {
			if _, err := pool.Exec(r.Context(),
				`UPDATE file SET is_shared = $1, modified_at = now() WHERE id = $2`,
				*body.IsShared, body.ID); err != nil {
				writeError(w, http.StatusInternalServerError, "update is_shared failed")
				return
			}
		}
		w.WriteHeader(http.StatusNoContent)
	}
}
