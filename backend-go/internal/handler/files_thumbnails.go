// Package handler — file and object thumbnail handlers.
//
// Ported from app.rpc.commands.files-thumbnails in the Clojure backend.
//
// # Two thumbnail concepts
//
//   file_tagged_object_thumbnail  — per-frame/object thumbnails rendered by the
//                                   canvas client (bucket: "file-object-thumbnail").
//                                   PK: (file_id, tag, object_id)
//                                   object_id format: {file-id}/{page-id}/{frame-id}
//
//   file_thumbnail                — file-level dashboard thumbnail keyed by revn
//                                   (bucket: "file-thumbnail").
//                                   PK: (file_id, revn)
//
// Both tables use soft-delete (deleted_at column).
// Storage objects for deleted rows are GC'd by the background worker.
package handler

import (
	"bytes"
	"encoding/json"
	"io"
	"net/http"
	"time"

	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/db"
	"github.com/logos-design/logos/backend-go/internal/perms"
	"github.com/logos-design/logos/backend-go/internal/storage"
)

const (
	objectThumbnailBucket = "file-object-thumbnail"
	fileThumbnailBucket   = "file-thumbnail"
)

// ─── GET /api/rpc/command/get-file-object-thumbnails ─────────────────────────

// GetFileObjectThumbnailsHandler implements GET /api/rpc/command/get-file-object-thumbnails.
//
// Returns a map of {object-id → media-id} for all non-deleted tagged-object
// thumbnails belonging to the file.  An optional "tag" query param filters by tag.
func GetFileObjectThumbnailsHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		fileID := r.URL.Query().Get("file-id")
		if fileID == "" {
			fileID = r.URL.Query().Get("fileId")
		}
		tag := r.URL.Query().Get("tag") // optional; default is "frame"

		if fileID == "" {
			writeError(w, http.StatusUnprocessableEntity, "file-id is required")
			return
		}

		fp, err := perms.GetFilePermissions(r.Context(), pool, profileID, fileID)
		if err != nil || fp == nil {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		var rows interface{ Close() }
		var queryErr error
		if tag != "" {
			rows, queryErr = pool.Query(r.Context(),
				`SELECT object_id, media_id::text
				   FROM file_tagged_object_thumbnail
				  WHERE file_id = $1 AND tag = $2 AND deleted_at IS NULL`,
				fileID, tag,
			)
		} else {
			rows, queryErr = pool.Query(r.Context(),
				`SELECT object_id, media_id::text
				   FROM file_tagged_object_thumbnail
				  WHERE file_id = $1 AND deleted_at IS NULL`,
				fileID,
			)
		}
		if queryErr != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		// rows is pgx.Rows — use type assertion
		type pgxRows interface {
			Next() bool
			Scan(dest ...any) error
			Close()
		}
		prows := rows.(pgxRows)
		defer prows.Close()

		result := make(map[string]string)
		for prows.Next() {
			var objectID, mediaID string
			if err := prows.Scan(&objectID, &mediaID); err != nil {
				continue
			}
			result[objectID] = mediaID
		}

		writeJSON(w, http.StatusOK, result)
	}
}

// ─── POST /api/rpc/command/create-file-object-thumbnail ──────────────────────

// CreateFileObjectThumbnailHandler implements POST /api/rpc/command/create-file-object-thumbnail.
//
// Accepts multipart/form-data with fields:
//
//	file-id     — file UUID
//	object-id   — "{file-id}/{page-id}/{frame-id}"
//	tag         — thumbnail category (default "frame")
//	media       — the rendered thumbnail image (part name "media")
//	mtype       — MIME type of the thumbnail (e.g. "image/png")
func CreateFileObjectThumbnailHandler(pool *db.Pool, store storage.Backend) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeError(w, http.StatusUnauthorized, "unauthorized")
			return
		}

		if err := r.ParseMultipartForm(maxMediaUploadBytes); err != nil {
			writeError(w, http.StatusBadRequest, "invalid multipart body")
			return
		}

		fileID := r.FormValue("file-id")
		objectID := r.FormValue("object-id")
		tag := r.FormValue("tag")
		mtype := r.FormValue("mtype")
		if fileID == "" || objectID == "" {
			writeError(w, http.StatusUnprocessableEntity, "file-id and object-id are required")
			return
		}
		if tag == "" {
			tag = "frame"
		}
		if mtype == "" {
			mtype = "image/png"
		}

		fp, err := perms.GetFilePermissions(r.Context(), pool, profileID, fileID)
		if err != nil || fp == nil {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		part, _, err := r.FormFile("media")
		if err != nil {
			writeError(w, http.StatusBadRequest, "media part missing")
			return
		}
		defer part.Close()

		buf := &bytes.Buffer{}
		if _, err := io.Copy(buf, io.LimitReader(part, maxMediaUploadBytes)); err != nil {
			writeError(w, http.StatusInternalServerError, "read error")
			return
		}

		mediaID := newUUID()
		if store != nil {
			if err := store.Put(r.Context(), objectThumbnailBucket, mediaID,
				bytes.NewReader(buf.Bytes()), int64(buf.Len()), mtype); err != nil {
				writeError(w, http.StatusInternalServerError, "storage error")
				return
			}
		}

		now := time.Now().UTC()
		if _, err = pool.Exec(r.Context(),
			`INSERT INTO file_tagged_object_thumbnail
			   (file_id, tag, object_id, media_id, updated_at)
			 VALUES ($1, $2, $3, $4, $5)
			 ON CONFLICT (file_id, tag, object_id)
			 DO UPDATE SET media_id = EXCLUDED.media_id, updated_at = EXCLUDED.updated_at,
			               deleted_at = NULL`,
			fileID, tag, objectID, mediaID, now,
		); err != nil {
			writeError(w, http.StatusInternalServerError, "db upsert failed")
			return
		}

		writeJSON(w, http.StatusOK, map[string]string{
			"objectId": objectID,
			"mediaId":  mediaID,
		})
	}
}

// ─── DELETE /api/rpc/command/delete-file-object-thumbnail ────────────────────

type deleteObjectThumbnailParams struct {
	FileID   string `json:"fileId"`
	ObjectID string `json:"objectId"`
	Tag      string `json:"tag"`
}

// DeleteFileObjectThumbnailHandler implements DELETE /api/rpc/command/delete-file-object-thumbnail.
func DeleteFileObjectThumbnailHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeError(w, http.StatusUnauthorized, "unauthorized")
			return
		}

		var params deleteObjectThumbnailParams
		if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}
		if params.Tag == "" {
			params.Tag = "frame"
		}

		fp, err := perms.GetFilePermissions(r.Context(), pool, profileID, params.FileID)
		if err != nil || fp == nil {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		_, _ = pool.Exec(r.Context(),
			`UPDATE file_tagged_object_thumbnail
			    SET deleted_at = now()
			  WHERE file_id = $1 AND tag = $2 AND object_id = $3 AND deleted_at IS NULL`,
			params.FileID, params.Tag, params.ObjectID,
		)

		writeJSON(w, http.StatusOK, map[string]any{})
	}
}

// ─── POST /api/rpc/command/create-file-thumbnail ─────────────────────────────

// CreateFileThumbnailHandler implements POST /api/rpc/command/create-file-thumbnail.
//
// Accepts multipart/form-data with fields:
//
//	file-id  — file UUID
//	revn     — revision number this thumbnail corresponds to
//	media    — thumbnail image (part name "media")
//	mtype    — MIME type (default "image/png")
func CreateFileThumbnailHandler(pool *db.Pool, store storage.Backend) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeError(w, http.StatusUnauthorized, "unauthorized")
			return
		}

		if err := r.ParseMultipartForm(maxMediaUploadBytes); err != nil {
			writeError(w, http.StatusBadRequest, "invalid multipart body")
			return
		}

		fileID := r.FormValue("file-id")
		mtype := r.FormValue("mtype")
		if fileID == "" {
			writeError(w, http.StatusUnprocessableEntity, "file-id is required")
			return
		}
		if mtype == "" {
			mtype = "image/png"
		}

		var revn int64
		_, _ = readIntForm(r, "revn", (*int)(nil))
		// Re-read as int64.
		revnStr := r.FormValue("revn")
		if revnStr != "" {
			var rv int
			_ = json.Unmarshal([]byte(revnStr), &rv)
			revn = int64(rv)
		}

		// Viewers are allowed to create thumbnails (read permission sufficient).
		fp, err := perms.GetFilePermissions(r.Context(), pool, profileID, fileID)
		if err != nil || fp == nil {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		part, _, err := r.FormFile("media")
		if err != nil {
			writeError(w, http.StatusBadRequest, "media part missing")
			return
		}
		defer part.Close()

		buf := &bytes.Buffer{}
		if _, err := io.Copy(buf, io.LimitReader(part, maxMediaUploadBytes)); err != nil {
			writeError(w, http.StatusInternalServerError, "read error")
			return
		}

		mediaID := newUUID()
		if store != nil {
			if err := store.Put(r.Context(), fileThumbnailBucket, mediaID,
				bytes.NewReader(buf.Bytes()), int64(buf.Len()), mtype); err != nil {
				writeError(w, http.StatusInternalServerError, "storage error")
				return
			}
		}

		now := time.Now().UTC()
		if _, err = pool.Exec(r.Context(),
			`INSERT INTO file_thumbnail (file_id, revn, media_id, created_at, updated_at)
			 VALUES ($1, $2, $3, $4, $4)
			 ON CONFLICT (file_id, revn)
			 DO UPDATE SET media_id = EXCLUDED.media_id, updated_at = EXCLUDED.updated_at,
			               deleted_at = NULL`,
			fileID, revn, mediaID, now,
		); err != nil {
			writeError(w, http.StatusInternalServerError, "db upsert failed")
			return
		}

		writeJSON(w, http.StatusOK, map[string]any{
			"id":      mediaID,
			"fileId":  fileID,
			"revn":    revn,
			"mediaId": mediaID,
		})
	}
}

// ─── GET /api/rpc/command/get-file-thumbnail ─────────────────────────────────

// GetFileThumbnailHandler implements GET /api/rpc/command/get-file-thumbnail.
// Returns the most recent non-deleted file_thumbnail for the given file.
func GetFileThumbnailHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		fileID := r.URL.Query().Get("file-id")
		if fileID == "" {
			fileID = r.URL.Query().Get("fileId")
		}

		fp, err := perms.GetFilePermissions(r.Context(), pool, profileID, fileID)
		if err != nil || fp == nil {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		var mediaID string
		var revn int64
		err = pool.QueryRow(r.Context(),
			`SELECT media_id::text, revn
			   FROM file_thumbnail
			  WHERE file_id = $1 AND deleted_at IS NULL
			  ORDER BY revn DESC
			  LIMIT 1`,
			fileID,
		).Scan(&mediaID, &revn)
		if err != nil {
			writeJSON(w, http.StatusOK, map[string]any{"mediaId": nil})
			return
		}

		writeJSON(w, http.StatusOK, map[string]any{
			"fileId":  fileID,
			"revn":    revn,
			"mediaId": mediaID,
		})
	}
}
