// Package handler — full-text search handlers.
//
// Ported from app.rpc.commands.search in the Clojure backend.
// Uses PostgreSQL ILIKE (matching Clojure implementation — no tsvector).
//
// Permission model (mirrors Clojure):
// The caller must have edit access to at least one project in the team,
// either via team_profile_rel or project_profile_rel.  Files in non-editable
// projects are excluded.
//
// Result: up to 100 files ordered by modified_at DESC, each with an optional
// thumbnail media_id (most recent non-deleted file_thumbnail row).
package handler

import (
	"net/http"

	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/db"
)

// FileSearchResult is one file returned by search-files.
type FileSearchResult struct {
	ID          string  `json:"id"`
	Name        string  `json:"name"`
	ProjectID   string  `json:"projectId"`
	Revn        int64   `json:"revn"`
	IsShared    bool    `json:"isShared"`
	ThumbnailID *string `json:"thumbnailId,omitempty"`
}

// SearchFilesHandler implements GET /api/rpc/command/search-files.
//
// Params (query string or JSON body):
//
//	team-id      — required; scope the search to this team
//	search-term  — optional; empty returns no results (matches Clojure `some->>` guard)
func SearchFilesHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
			return
		}

		teamID := rpcParam(r, "team-id", "teamId")
		searchTerm := rpcParam(r, "search-term", "searchTerm")

		if teamID == "" {
			writeError(w, http.StatusUnprocessableEntity, "team-id is required")
			return
		}
		// Clojure guard: return nil when search-term is absent.
		if searchTerm == "" {
			writeJSON(w, http.StatusOK, []FileSearchResult{})
			return
		}

		// ── Permission-aware ILIKE search (mirrors Clojure SQL) ────────────
		//
		// Included files: the caller can edit the containing project via
		//   (a) team_profile_rel with edit/admin/owner role, OR
		//   (b) project_profile_rel with edit/admin/owner role.
		//
		// A lateral sub-select fetches the latest file_thumbnail.media_id.
		const searchSQL = `
			WITH editable_projects AS (
			  SELECT p.id
			    FROM project p
			   WHERE p.team_id = $1
			     AND p.deleted_at IS NULL
			     AND (
			       EXISTS (
			         SELECT 1 FROM team_profile_rel tpr
			          WHERE tpr.profile_id = $2
			            AND tpr.team_id = $1
			            AND (tpr.is_owner OR tpr.is_admin OR tpr.can_edit)
			       )
			       OR
			       EXISTS (
			         SELECT 1 FROM project_profile_rel ppr
			          WHERE ppr.profile_id = $2
			            AND ppr.project_id = p.id
			            AND (ppr.is_owner OR ppr.is_admin OR ppr.can_edit)
			       )
			     )
			)
			SELECT f.id::text,
			       f.name,
			       f.project_id::text,
			       f.revn,
			       f.is_shared,
			       thumb.media_id::text
			  FROM file f
			  JOIN editable_projects ep ON ep.id = f.project_id
			  LEFT JOIN LATERAL (
			    SELECT media_id
			      FROM file_thumbnail
			     WHERE file_id = f.id
			       AND deleted_at IS NULL
			     ORDER BY revn DESC
			     LIMIT 1
			  ) thumb ON true
			 WHERE f.name ILIKE '%' || $3 || '%'
			   AND f.deleted_at IS NULL
			 ORDER BY f.modified_at DESC
			 LIMIT 100`

		rows, err := pool.Query(r.Context(), searchSQL, teamID, profileID, searchTerm)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		defer rows.Close()

		results := make([]FileSearchResult, 0)
		for rows.Next() {
			var f FileSearchResult
			if err := rows.Scan(&f.ID, &f.Name, &f.ProjectID,
				&f.Revn, &f.IsShared, &f.ThumbnailID); err != nil {
				continue
			}
			results = append(results, f)
		}

		writeJSON(w, http.StatusOK, results)
	}
}
