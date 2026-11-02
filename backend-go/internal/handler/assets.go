// Package handler — static asset serving.
//
// Ported from app.http.assets in the Clojure backend.
// Serves storage objects at GET /assets/by-id/{id}.
package handler

import (
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"time"

	"github.com/go-chi/chi/v5"

	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/db"
	"github.com/logos-design/logos/backend-go/internal/perms"
	"github.com/logos-design/logos/backend-go/internal/storage"
)

// Asset visibility is resource-scoped, never bucket-scoped. No current storage
// bucket has a durable public-publication flag, so anonymous access is allowed
// only through a valid, unscoped share link for the file that references it.
// Page-scoped share links are denied here until assets can be mapped to pages.
var errAssetResourceNotFound = errors.New("asset resource not found")

type storageObjectMeta struct {
	Bucket      string `json:"bucket"`
	ContentType string `json:"content-type"`
}

type assetResource struct {
	fileIDs    []string
	teamIDs    []string
	projectIDs []string
	profileIDs []string
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
		if len(metaJSON) == 0 || json.Unmarshal(metaJSON, &meta) != nil {
			http.NotFound(w, r)
			return
		}
		if meta.Bucket == "" {
			http.NotFound(w, r)
			return
		}

		resource, err := resolveAssetResource(r, pool, meta.Bucket, id)
		if errors.Is(err, errAssetResourceNotFound) {
			http.NotFound(w, r)
			return
		}
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		if !canReadAsset(r, pool, resource) {
			// Keep missing and unauthorized objects indistinguishable. In
			// particular, do not reveal metadata or content type before this check.
			http.NotFound(w, r)
			return
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
		w.Header().Set("Cache-Control", "private, no-store")
		w.WriteHeader(http.StatusOK)
		_, _ = io.Copy(w, rc)
	}
}

func resolveAssetResource(r *http.Request, pool *db.Pool, bucket, id string) (assetResource, error) {
	resource := assetResource{}
	var query string
	switch bucket {
	case "file-media-object":
		query = `SELECT file_id::text FROM file_media_object WHERE media_id::text = $1 AND deleted_at IS NULL`
		rows, err := pool.Query(r.Context(), query, id)
		if err != nil {
			return resource, err
		}
		defer rows.Close()
		for rows.Next() {
			var fileID string
			if err := rows.Scan(&fileID); err != nil {
				return resource, err
			}
			resource.fileIDs = append(resource.fileIDs, fileID)
		}
		if err := rows.Err(); err != nil {
			return resource, err
		}
	case "file-object-thumbnail":
		query = `SELECT file_id::text FROM file_tagged_object_thumbnail WHERE media_id::text = $1 AND deleted_at IS NULL`
		rows, err := pool.Query(r.Context(), query, id)
		if err != nil {
			return resource, err
		}
		defer rows.Close()
		for rows.Next() {
			var fileID string
			if err := rows.Scan(&fileID); err != nil {
				return resource, err
			}
			resource.fileIDs = append(resource.fileIDs, fileID)
		}
		if err := rows.Err(); err != nil {
			return resource, err
		}
	case "file-thumbnail":
		query = `SELECT file_id::text FROM file_thumbnail WHERE media_id::text = $1 AND deleted_at IS NULL`
		rows, err := pool.Query(r.Context(), query, id)
		if err != nil {
			return resource, err
		}
		defer rows.Close()
		for rows.Next() {
			var fileID string
			if err := rows.Scan(&fileID); err != nil {
				return resource, err
			}
			resource.fileIDs = append(resource.fileIDs, fileID)
		}
		if err := rows.Err(); err != nil {
			return resource, err
		}
	case "team-font-variant":
		query = `SELECT team_id::text FROM team_font_variant
			WHERE deleted_at IS NULL AND $1 IN (woff1_file_id::text, woff2_file_id::text, otf_file_id::text, ttf_file_id::text)`
		rows, err := pool.Query(r.Context(), query, id)
		if err != nil {
			return resource, err
		}
		defer rows.Close()
		for rows.Next() {
			var teamID string
			if err := rows.Scan(&teamID); err != nil {
				return resource, err
			}
			resource.teamIDs = append(resource.teamIDs, teamID)
		}
		if err := rows.Err(); err != nil {
			return resource, err
		}
	case "project":
		query = `SELECT id::text FROM project WHERE photo_id::text = $1 AND deleted_at IS NULL`
		rows, err := pool.Query(r.Context(), query, id)
		if err != nil {
			return resource, err
		}
		defer rows.Close()
		for rows.Next() {
			var projectID string
			if err := rows.Scan(&projectID); err != nil {
				return resource, err
			}
			resource.projectIDs = append(resource.projectIDs, projectID)
		}
		if err := rows.Err(); err != nil {
			return resource, err
		}
	case "profile":
		query = `SELECT id::text FROM profile WHERE photo_id::text = $1 AND deleted_at IS NULL`
		rows, err := pool.Query(r.Context(), query, id)
		if err != nil {
			return resource, err
		}
		defer rows.Close()
		for rows.Next() {
			var profileID string
			if err := rows.Scan(&profileID); err != nil {
				return resource, err
			}
			resource.profileIDs = append(resource.profileIDs, profileID)
		}
		if err := rows.Err(); err != nil {
			return resource, err
		}
	default:
		// Unknown/unmapped buckets fail closed. In particular, a bucket name
		// alone cannot confer anonymous access.
		return resource, errAssetResourceNotFound
	}

	if len(resource.fileIDs)+len(resource.teamIDs)+len(resource.projectIDs)+len(resource.profileIDs) == 0 {
		return resource, errAssetResourceNotFound
	}
	return resource, nil
}

func canReadAsset(r *http.Request, pool *db.Pool, resource assetResource) bool {
	profileID := auth.ProfileID(r.Context())
	shareID := r.URL.Query().Get("share-id")

	for _, fileID := range resource.fileIDs {
		if profileID != "" {
			filePerms, err := perms.GetFilePermissions(r.Context(), pool, profileID, fileID)
			if err == nil && filePerms != nil && filePerms.CanRead {
				return true
			}
		}
		if shareID != "" && shareLinkAllowsAsset(r, pool, shareID, fileID) {
			return true
		}
	}

	if profileID == "" {
		return false
	}
	for _, teamID := range resource.teamIDs {
		teamPerms, err := perms.GetTeamPermissions(r.Context(), pool, profileID, teamID)
		if err == nil && teamPerms != nil && teamPerms.CanRead {
			return true
		}
	}
	for _, projectID := range resource.projectIDs {
		projectPerms, err := perms.GetProjectPermissions(r.Context(), pool, profileID, projectID)
		if err == nil && projectPerms != nil && projectPerms.CanRead {
			return true
		}
	}
	for _, ownerID := range resource.profileIDs {
		if ownerID == profileID {
			return true
		}
	}
	return false
}

func shareLinkAllowsAsset(r *http.Request, pool *db.Pool, shareID, fileID string) bool {
	var allowed bool
	err := pool.QueryRow(r.Context(), `
		SELECT EXISTS (
		  SELECT 1 FROM share_link AS sl
		  JOIN file AS f ON (f.id = sl.file_id AND f.deleted_at IS NULL)
		  JOIN project AS p ON (p.id = f.project_id AND p.deleted_at IS NULL)
		   WHERE sl.id = $1 AND sl.file_id = $2
		     AND COALESCE(cardinality(sl.pages), 0) = 0
		)`, shareID, fileID).Scan(&allowed)
	return err == nil && allowed
}
