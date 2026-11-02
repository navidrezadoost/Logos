// Package handler — static asset serving.
//
// Ported from app.http.assets in the Clojure backend.
// Serves storage objects at GET /assets/by-id/{id}.
package handler

import (
	"encoding/json"
	"io"
	"net/http"
	"time"

	"github.com/go-chi/chi/v5"

	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/db"
	"github.com/logos-design/logos/backend-go/internal/storage"
)

// Buckets that may be fetched without a session (public share / embed flows).
var publicAssetBuckets = map[string]bool{
	"file-media-object":       true,
	"file-object-thumbnail":   true,
	"team-font-variant":       true,
	"file-data-fragment":      true,
	"file-thumbnail":          true,
}

type storageObjectMeta struct {
	Bucket      string `json:"bucket"`
	ContentType string `json:"content-type"`
}

// AssetByIDHandler serves a storage object by UUID.
func AssetByIDHandler(pool *db.Pool, store storage.Backend) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		id := chi.URLParam(r, "id")
		if id == "" {
			http.NotFound(w, r)
			return
		}
		if store == nil {
			writeError(w, http.StatusInternalServerError, "storage not configured")
			return
		}

		var metaJSON []byte
		var deletedAt *time.Time
		err := pool.QueryRow(r.Context(),
			`SELECT metadata, deleted_at FROM storage_object WHERE id = $1`, id).
			Scan(&metaJSON, &deletedAt)
		if err != nil || deletedAt != nil {
			http.NotFound(w, r)
			return
		}

		var meta storageObjectMeta
		if len(metaJSON) > 0 {
			_ = json.Unmarshal(metaJSON, &meta)
		}
		if meta.Bucket == "" {
			http.NotFound(w, r)
			return
		}

		if !publicAssetBuckets[meta.Bucket] {
			if auth.ProfileID(r.Context()) == "" {
				writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
				return
			}
		}

		rc, err := store.Get(r.Context(), meta.Bucket, id)
		if err != nil {
			http.NotFound(w, r)
			return
		}
		defer rc.Close()

		if meta.ContentType != "" {
			w.Header().Set("Content-Type", meta.ContentType)
		}
		w.Header().Set("Cache-Control", "max-age=86400")
		w.WriteHeader(http.StatusOK)
		_, _ = io.Copy(w, rc)
	}
}
