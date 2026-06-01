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
	"log"
	"net/http"
	"strconv"
	"time"

	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/db"
	"github.com/logos-design/logos/backend-go/internal/filedata"
	"github.com/logos-design/logos/backend-go/internal/perms"
	"github.com/logos-design/logos/backend-go/internal/storage"
)

const (
	objectThumbnailBucket = "file-object-thumbnail"
	fileThumbnailBucket   = "file-thumbnail"
)

// ─── GET /api/rpc/command/get-file-data-for-thumbnail ────────────────────────

// GetFileDataForThumbnailHandler returns page data used to render dashboard thumbnails.
func GetFileDataForThumbnailHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
			return
		}
		fileID := rpcParam(r, "file-id", "fileId")
		if fileID == "" {
			writeError(w, http.StatusBadRequest, "file-id required")
			return
		}
		if !perms.CheckFileRead(w, r, pool, profileID, fileID) {
			return
		}

		var revn int
		var features []string
		err := pool.QueryRow(r.Context(), `
			SELECT revn, COALESCE(features, '{}')
			  FROM file
			 WHERE id = $1 AND deleted_at IS NULL`, fileID).Scan(&revn, &features)
		if err != nil {
			writeError(w, http.StatusNotFound, "file-not-found")
			return
		}
		if len(features) == 0 {
			features = filedata.DefaultFeatures
		}

		data, err := loadOrInitFileData(r.Context(), pool, fileID, features)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		page := filedata.FirstPage(data)
		if page == nil {
			page = map[string]any{}
		}

		writeJSON(w, http.StatusOK, map[string]any{
			"file-id": fileID,
			"revn":    revn,
			"page":    page,
		})
	}
}

// ─── GET /api/rpc/command/get-file-object-thumbnails ─────────────────────────

// GetFileObjectThumbnailsHandler implements GET /api/rpc/command/get-file-object-thumbnails.
//
// Returns a map of {object-id → media-id} for all non-deleted tagged-object
// thumbnails belonging to the file.  An optional "tag" query param filters by tag.
func GetFileObjectThumbnailsHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		fileID := rpcParam(r, "file-id", "fileId")
		tag := rpcParam(r, "tag") // optional; default is "frame"

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

		log.Printf("[get-file-object-thumbnails] file=%s tag=%q profile=%s count=%d",
			fileID, tag, profileID, len(result))
		writePlainStringMap(w, http.StatusOK, result)
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
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
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
		size := int64(buf.Len())
		if err := insertStorageObject(r.Context(), pool, mediaID, objectThumbnailBucket, mtype, size); err != nil {
			writeError(w, http.StatusInternalServerError, "storage object insert failed")
			return
		}
		if store != nil {
			if err := store.Put(r.Context(), objectThumbnailBucket, mediaID,
				bytes.NewReader(buf.Bytes()), size, mtype); err != nil {
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

// DeleteFileObjectThumbnailHandler implements DELETE /api/rpc/command/delete-file-object-thumbnail.
func DeleteFileObjectThumbnailHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
			return
		}

		fileID := rpcParam(r, "file-id", "fileId")
		objectID := rpcParam(r, "object-id", "objectId")
		tag := rpcParam(r, "tag")
		if tag == "" {
			tag = "frame"
		}
		if fileID == "" || objectID == "" {
			writeError(w, http.StatusBadRequest, "file-id and object-id required")
			return
		}

		fp, err := perms.GetFilePermissions(r.Context(), pool, profileID, fileID)
		if err != nil || fp == nil || !fp.CanEdit {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		_, _ = pool.Exec(r.Context(),
			`UPDATE file_tagged_object_thumbnail
			    SET deleted_at = now()
			  WHERE file_id = $1 AND tag = $2 AND object_id = $3 AND deleted_at IS NULL`,
			fileID, tag, objectID,
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
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
			return
		}

		if err := r.ParseMultipartForm(maxMediaUploadBytes); err != nil {
			// Some clients send thumbnail metadata in the query string with an
			// empty or non-multipart body.
			_ = r.ParseForm()
		}

		fileID := r.URL.Query().Get("file-id")
		if fileID == "" {
			fileID = r.URL.Query().Get("fileId")
		}
		if fileID == "" {
			fileID = r.FormValue("file-id")
		}
		if fileID == "" {
			fileID = r.FormValue("fileId")
		}
		mtype := r.FormValue("mtype")
		if fileID == "" {
			writeError(w, http.StatusUnprocessableEntity, "file-id is required")
			return
		}
		if mtype == "" {
			mtype = "image/png"
		}

		var revn int64
		if revnStr := firstNonEmpty(r.URL.Query().Get("revn"), r.FormValue("revn")); revnStr != "" {
			var rv int
			if err := json.Unmarshal([]byte(revnStr), &rv); err != nil {
				rv, _ = strconv.Atoi(revnStr)
			}
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
		size := int64(buf.Len())
		if err := insertStorageObject(r.Context(), pool, mediaID, fileThumbnailBucket, mtype, size); err != nil {
			writeError(w, http.StatusInternalServerError, "storage object insert failed")
			return
		}
		if store != nil {
			if err := store.Put(r.Context(), fileThumbnailBucket, mediaID,
				bytes.NewReader(buf.Bytes()), size, mtype); err != nil {
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
			"id":       mediaID,
			"file-id":  fileID,
			"revn":     revn,
			"media-id": mediaID,
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
