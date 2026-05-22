// Package handler — share-link handlers.
//
// Ported from app.rpc.commands.files-share in the Clojure backend.
// Share links allow external (unauthenticated) users to view specific pages
// of a file with specified comment/inspect permissions.
package handler

import (
	"encoding/json"
	"net/http"
	"time"

	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/db"
	"github.com/logos-design/logos/backend-go/internal/perms"
)

// ShareLink is the JSON-serialisable share_link row.
type ShareLink struct {
	ID         string    `json:"id"`
	FileID     string    `json:"fileId"`
	OwnerID    *string   `json:"ownerId,omitempty"`
	WhoComment string    `json:"whoComment"`
	WhoInspect string    `json:"whoInspect"`
	CreatedAt  time.Time `json:"createdAt"`
}

// ─── POST /api/rpc/command/create-share-link ─────────────────────────────────

// CreateShareLinkHandler creates a share link for a file.
// The caller must have edit permissions on the file.
func CreateShareLinkHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeError(w, http.StatusUnauthorized, "unauthorized")
			return
		}

		var body struct {
			FileID     string   `json:"fileId"`
			WhoComment string   `json:"whoComment"`
			WhoInspect string   `json:"whoInspect"`
			Pages      []string `json:"pages"`
		}
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}
		if body.FileID == "" {
			writeError(w, http.StatusBadRequest, "fileId required")
			return
		}
		if !perms.CheckFileEdit(w, r, pool, profileID, body.FileID) {
			return
		}

		whoComment := body.WhoComment
		if whoComment == "" {
			whoComment = "team"
		}
		whoInspect := body.WhoInspect
		if whoInspect == "" {
			whoInspect = "team"
		}

		id := newUUID()
		// pages are stored as a postgres uuid array.
		pagesArr := make([]string, 0, len(body.Pages))
		pagesArr = append(pagesArr, body.Pages...)

		var sl ShareLink
		err := pool.QueryRow(r.Context(), `
			INSERT INTO share_link (id, file_id, owner_id, who_comment, who_inspect, pages)
			VALUES ($1, $2, $3, $4, $5, $6::uuid[])
			RETURNING id, file_id, owner_id::text, who_comment, who_inspect, created_at`,
			id, body.FileID, profileID, whoComment, whoInspect, pagesArr).
			Scan(&sl.ID, &sl.FileID, &sl.OwnerID, &sl.WhoComment, &sl.WhoInspect, &sl.CreatedAt)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "create share link failed")
			return
		}
		writeJSON(w, http.StatusOK, &sl)
	}
}

// ─── DELETE /api/rpc/command/delete-share-link ───────────────────────────────

// DeleteShareLinkHandler removes a share link. Caller must have file edit permissions.
func DeleteShareLinkHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeError(w, http.StatusUnauthorized, "unauthorized")
			return
		}

		var body struct {
			ID string `json:"id"`
		}
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}
		if body.ID == "" {
			writeError(w, http.StatusBadRequest, "id required")
			return
		}

		// Resolve the file to check permissions.
		var fileID string
		if err := pool.QueryRow(r.Context(),
			`SELECT file_id FROM share_link WHERE id = $1`, body.ID).
			Scan(&fileID); err != nil {
			writeError(w, http.StatusNotFound, "share-link-not-found")
			return
		}
		if !perms.CheckFileEdit(w, r, pool, profileID, fileID) {
			return
		}

		if _, err := pool.Exec(r.Context(),
			`DELETE FROM share_link WHERE id = $1`, body.ID); err != nil {
			writeError(w, http.StatusInternalServerError, "delete share link failed")
			return
		}
		w.WriteHeader(http.StatusNoContent)
	}
}

// ─── GET /api/rpc/command/get-share-link ─────────────────────────────────────

// GetShareLinkHandler returns a share link by ?id=.
func GetShareLinkHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeError(w, http.StatusUnauthorized, "unauthorized")
			return
		}
		id := r.URL.Query().Get("id")
		if id == "" {
			writeError(w, http.StatusBadRequest, "id required")
			return
		}

		var sl ShareLink
		var fileID string
		if err := pool.QueryRow(r.Context(), `
			SELECT id, file_id, owner_id::text, who_comment, who_inspect, created_at
			  FROM share_link WHERE id = $1`, id).
			Scan(&sl.ID, &fileID, &sl.OwnerID, &sl.WhoComment, &sl.WhoInspect, &sl.CreatedAt); err != nil {
			writeError(w, http.StatusNotFound, "share-link-not-found")
			return
		}
		sl.FileID = fileID

		if !perms.CheckFileRead(w, r, pool, profileID, fileID) {
			return
		}
		writeJSON(w, http.StatusOK, &sl)
	}
}
