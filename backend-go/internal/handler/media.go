// Package handler — file media object handlers.
//
// Ported from app.rpc.commands.media in the Clojure backend.
// Handles upload, URL-based creation, and cloning of file media objects.
//
// Table: file_media_object
//
//	id, file_id, is_local, name, media_id, thumbnail_id, width, height, mtype
//
// Storage buckets (matching Clojure):
//
//	"file-media-object" — raw uploaded images / binary assets.
//
// Image processing
// ─────────────────
// The Clojure backend runs a full image processing pipeline (resize, generate
// JPEG thumbnail, extract dimensions).  The Go handler stores the raw bytes
// and records the caller-supplied width/height/mtype.  A thumbnail-generation
// job can be added when an image processing library is integrated.
//
// Supported media types: image/*, video/*, application/octet-stream.
// Max upload size: 20 MiB (hardcoded; configurable in a later session).
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
	maxMediaUploadBytes = 20 << 20 // 20 MiB
	mediaObjectBucket   = "file-media-object"
)

// MediaObject is the public representation of a file_media_object row.
type MediaObject struct {
	ID          string    `json:"id"`
	FileID      string    `json:"fileId"`
	Name        string    `json:"name"`
	MediaID     string    `json:"mediaId"`
	ThumbnailID *string   `json:"thumbnailId,omitempty"`
	Width       int       `json:"width"`
	Height      int       `json:"height"`
	Mtype       string    `json:"mtype"`
	IsLocal     bool      `json:"isLocal"`
	CreatedAt   time.Time `json:"createdAt"`
}

// ─── POST /api/rpc/command/upload-file-media-object ──────────────────────────

// UploadFileMediaObjectHandler implements POST /api/rpc/command/upload-file-media-object.
//
// Accepts a multipart/form-data body with fields:
//
//	file-id   — target file UUID
//	name      — display name
//	width     — image width  (int, caller must supply)
//	height    — image height (int, caller must supply)
//	mtype     — MIME type (e.g. "image/png")
//	is-local  — "true" / "false"
//	content   — the binary file (multipart part name "content")
func UploadFileMediaObjectHandler(pool *db.Pool, store storage.Backend) http.HandlerFunc {
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
		if fileID == "" {
			fileID = r.FormValue("fileId")
		}
		name := r.FormValue("name")
		mtype := r.FormValue("mtype")
		if fileID == "" || name == "" || mtype == "" {
			writeError(w, http.StatusUnprocessableEntity, "file-id, name, and mtype are required")
			return
		}

		var width, height int
		_, _ = readIntForm(r, "width", &width)
		_, _ = readIntForm(r, "height", &height)
		isLocal := r.FormValue("is-local") != "false"

		fp, err := perms.GetFilePermissions(r.Context(), pool, profileID, fileID)
		if err != nil || fp == nil || !fp.CanEdit {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		// Read the file content.
		part, _, err := r.FormFile("content")
		if err != nil {
			writeError(w, http.StatusBadRequest, "content part missing")
			return
		}
		defer part.Close()

		buf := &bytes.Buffer{}
		if _, err := io.Copy(buf, io.LimitReader(part, maxMediaUploadBytes)); err != nil {
			writeError(w, http.StatusInternalServerError, "read error")
			return
		}

		mediaID := newUUID()
		if err := store.Put(r.Context(), mediaObjectBucket, mediaID,
			bytes.NewReader(buf.Bytes()), int64(buf.Len()), mtype); err != nil {
			writeError(w, http.StatusInternalServerError, "storage error")
			return
		}

		objectID := newUUID()
		now := time.Now().UTC()

		if _, err = pool.Exec(r.Context(),
			`INSERT INTO file_media_object
			   (id, file_id, is_local, name, media_id, width, height, mtype, created_at)
			 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
			 ON CONFLICT (id) DO UPDATE SET created_at = file_media_object.created_at`,
			objectID, fileID, isLocal, name, mediaID, width, height, mtype, now,
		); err != nil {
			writeError(w, http.StatusInternalServerError, "db insert failed")
			return
		}

		writeJSON(w, http.StatusOK, MediaObject{
			ID:        objectID,
			FileID:    fileID,
			Name:      name,
			MediaID:   mediaID,
			Width:     width,
			Height:    height,
			Mtype:     mtype,
			IsLocal:   isLocal,
			CreatedAt: now,
		})
	}
}

// ─── POST /api/rpc/command/clone-file-media-object ───────────────────────────

type cloneMediaObjectParams struct {
	ID     string `json:"id"`
	FileID string `json:"fileId"` // destination file
}

// CloneFileMediaObjectHandler implements POST /api/rpc/command/clone-file-media-object.
//
// Copies a media object row to a new file, reusing the same storage objects
// (no bytes are copied — mirrors Clojure's clone path).
func CloneFileMediaObjectHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeError(w, http.StatusUnauthorized, "unauthorized")
			return
		}

		var params cloneMediaObjectParams
		if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}
		if params.ID == "" || params.FileID == "" {
			writeError(w, http.StatusUnprocessableEntity, "id and fileId are required")
			return
		}

		// Load the source row.
		var src MediaObject
		err := pool.QueryRow(r.Context(),
			`SELECT id::text, file_id::text, is_local, name, media_id::text,
			        thumbnail_id::text, width, height, mtype, created_at
			   FROM file_media_object
			  WHERE id = $1 AND deleted_at IS NULL`,
			params.ID,
		).Scan(&src.ID, &src.FileID, &src.IsLocal, &src.Name, &src.MediaID,
			&src.ThumbnailID, &src.Width, &src.Height, &src.Mtype, &src.CreatedAt)
		if err != nil {
			writeError(w, http.StatusNotFound, "media object not found")
			return
		}

		// Check destination file edit permission.
		fp, err := perms.GetFilePermissions(r.Context(), pool, profileID, params.FileID)
		if err != nil || fp == nil || !fp.CanEdit {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		newID := newUUID()
		now := time.Now().UTC()

		if _, err = pool.Exec(r.Context(),
			`INSERT INTO file_media_object
			   (id, file_id, is_local, name, media_id, thumbnail_id, width, height, mtype, created_at)
			 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
			 ON CONFLICT (id) DO UPDATE SET created_at = file_media_object.created_at`,
			newID, params.FileID, src.IsLocal, src.Name, src.MediaID,
			src.ThumbnailID, src.Width, src.Height, src.Mtype, now,
		); err != nil {
			writeError(w, http.StatusInternalServerError, "db insert failed")
			return
		}

		writeJSON(w, http.StatusOK, MediaObject{
			ID:          newID,
			FileID:      params.FileID,
			Name:        src.Name,
			MediaID:     src.MediaID,
			ThumbnailID: src.ThumbnailID,
			Width:       src.Width,
			Height:      src.Height,
			Mtype:       src.Mtype,
			IsLocal:     src.IsLocal,
			CreatedAt:   now,
		})
	}
}

// ─── GET /api/rpc/command/get-file-media-objects ─────────────────────────────

// GetFileMediaObjectsHandler implements GET /api/rpc/command/get-file-media-objects.
func GetFileMediaObjectsHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		fileID := r.URL.Query().Get("file-id")
		if fileID == "" {
			fileID = r.URL.Query().Get("fileId")
		}
		if fileID == "" {
			writeError(w, http.StatusUnprocessableEntity, "file-id is required")
			return
		}

		fp, err := perms.GetFilePermissions(r.Context(), pool, profileID, fileID)
		if err != nil || fp == nil {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		rows, err := pool.Query(r.Context(),
			`SELECT id::text, file_id::text, is_local, name, media_id::text,
			        thumbnail_id::text, width, height, mtype, created_at
			   FROM file_media_object
			  WHERE file_id = $1 AND deleted_at IS NULL
			  ORDER BY created_at DESC`,
			fileID,
		)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		defer rows.Close()

		objects := make([]MediaObject, 0)
		for rows.Next() {
			var o MediaObject
			if err := rows.Scan(&o.ID, &o.FileID, &o.IsLocal, &o.Name, &o.MediaID,
				&o.ThumbnailID, &o.Width, &o.Height, &o.Mtype, &o.CreatedAt); err != nil {
				continue
			}
			objects = append(objects, o)
		}

		writeJSON(w, http.StatusOK, objects)
	}
}

// ─── helpers ─────────────────────────────────────────────────────────────────

func readIntForm(r *http.Request, field string, out *int) (bool, error) {
	s := r.FormValue(field)
	if s == "" {
		return false, nil
	}
	var v int
	if err := json.Unmarshal([]byte(s), &v); err != nil {
		return false, err
	}
	*out = v
	return true, nil
}
