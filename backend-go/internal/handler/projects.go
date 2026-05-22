// Package handler — projects handlers.
//
// Ported from app.rpc.commands.projects in the Clojure backend.
// Covers: get-projects, get-project, create-project, rename-project,
//         delete-project, update-project-pin.
//
// Permission model: project permissions are resolved by unioning
// team_profile_rel (via project.team_id) and project_profile_rel rows.
package handler

import (
	"encoding/json"
	"net/http"
	"time"

	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/db"
	"github.com/logos-design/logos/backend-go/internal/perms"
)

// Project is the JSON-serialisable project record.
type Project struct {
	ID         string    `json:"id"`
	TeamID     string    `json:"teamId"`
	Name       string    `json:"name"`
	IsDefault  bool      `json:"isDefault"`
	IsPinned   bool      `json:"isPinned"`
	Count      int64     `json:"count"`      // non-deleted file count
	TotalCount int64     `json:"totalCount"` // all files including deleted
	CreatedAt  time.Time `json:"createdAt"`
	ModifiedAt time.Time `json:"modifiedAt"`
	DeletedAt  *time.Time `json:"deletedAt,omitempty"`
}

// ─── GET /api/rpc/command/get-projects ───────────────────────────────────────

// GetProjectsHandler lists all projects the profile can see in a team.
func GetProjectsHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeError(w, http.StatusUnauthorized, "unauthorized")
			return
		}
		teamID := r.URL.Query().Get("team-id")
		if teamID == "" {
			writeError(w, http.StatusBadRequest, "team-id required")
			return
		}

		p, err := perms.GetTeamPermissions(r.Context(), pool, profileID, teamID)
		if err != nil || p == nil || !p.CanRead {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		const q = `
			SELECT p.id, p.team_id, p.name, p.is_default,
			       COALESCE(tpp.is_pinned, false) AS is_pinned,
			       (SELECT count(*) FROM file AS f
			         WHERE f.project_id = p.id AND f.deleted_at IS NULL) AS count,
			       (SELECT count(*) FROM file AS f
			         WHERE f.project_id = p.id) AS total_count,
			       p.created_at, p.modified_at, p.deleted_at
			  FROM project AS p
			  JOIN team AS t ON (t.id = p.team_id)
			  LEFT JOIN team_project_profile_rel AS tpp
			         ON (tpp.project_id = p.id AND
			             tpp.team_id = p.team_id AND
			             tpp.profile_id = $1)
			 WHERE p.team_id = $2
			   AND t.deleted_at IS NULL
			 ORDER BY p.modified_at DESC`

		rows, err := pool.Query(r.Context(), q, profileID, teamID)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		defer rows.Close()

		var projects []*Project
		for rows.Next() {
			proj := &Project{}
			if err := rows.Scan(
				&proj.ID, &proj.TeamID, &proj.Name, &proj.IsDefault,
				&proj.IsPinned, &proj.Count, &proj.TotalCount,
				&proj.CreatedAt, &proj.ModifiedAt, &proj.DeletedAt,
			); err != nil {
				writeError(w, http.StatusInternalServerError, "internal server error")
				return
			}
			projects = append(projects, proj)
		}
		if err := rows.Err(); err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		writeJSON(w, http.StatusOK, projects)
	}
}

// ─── GET /api/rpc/command/get-project ────────────────────────────────────────

// GetProjectHandler returns a single project by ?id=.
func GetProjectHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeError(w, http.StatusUnauthorized, "unauthorized")
			return
		}
		projectID := r.URL.Query().Get("id")
		if projectID == "" {
			writeError(w, http.StatusBadRequest, "id required")
			return
		}

		p, err := perms.GetProjectPermissions(r.Context(), pool, profileID, projectID)
		if err != nil || p == nil || !p.CanRead {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		var proj Project
		err = pool.QueryRow(r.Context(), `
			SELECT id, team_id, name, is_default, false AS is_pinned,
			       0 AS count, 0 AS total_count,
			       created_at, modified_at, deleted_at
			  FROM project WHERE id = $1`, projectID).Scan(
			&proj.ID, &proj.TeamID, &proj.Name, &proj.IsDefault, &proj.IsPinned,
			&proj.Count, &proj.TotalCount, &proj.CreatedAt, &proj.ModifiedAt, &proj.DeletedAt,
		)
		if err != nil {
			writeError(w, http.StatusNotFound, "project-not-found")
			return
		}
		writeJSON(w, http.StatusOK, &proj)
	}
}

// ─── POST /api/rpc/command/create-project ────────────────────────────────────

// CreateProjectHandler creates a new project under a team.
func CreateProjectHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeError(w, http.StatusUnauthorized, "unauthorized")
			return
		}

		var body struct {
			TeamID string `json:"teamId"`
			Name   string `json:"name"`
			ID     string `json:"id,omitempty"`
		}
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}
		if body.TeamID == "" || body.Name == "" {
			writeError(w, http.StatusBadRequest, "teamId and name required")
			return
		}

		p, err := perms.GetTeamPermissions(r.Context(), pool, profileID, body.TeamID)
		if err != nil || p == nil || !p.CanEdit {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		projectID := body.ID
		if projectID == "" {
			projectID = newUUID()
		}

		tx, err := pool.Begin(r.Context())
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		defer tx.Rollback(r.Context()) //nolint:errcheck

		if _, err := tx.Exec(r.Context(),
			`INSERT INTO project (id, team_id, name, is_default) VALUES ($1, $2, $3, false)`,
			projectID, body.TeamID, body.Name); err != nil {
			writeError(w, http.StatusInternalServerError, "create project failed")
			return
		}

		// Owner role on the project.
		if _, err := tx.Exec(r.Context(),
			`INSERT INTO project_profile_rel (project_id, profile_id, is_owner, is_admin, can_edit)
			 VALUES ($1, $2, true, true, true)`,
			projectID, profileID); err != nil {
			writeError(w, http.StatusInternalServerError, "create project role failed")
			return
		}

		// team_project_profile_rel for pin state.
		if _, err := tx.Exec(r.Context(),
			`INSERT INTO team_project_profile_rel (team_id, project_id, profile_id, is_pinned)
			 VALUES ($1, $2, $3, false)
			 ON CONFLICT (team_id, project_id, profile_id) DO NOTHING`,
			body.TeamID, projectID, profileID); err != nil {
			writeError(w, http.StatusInternalServerError, "link project to team failed")
			return
		}

		if err := tx.Commit(r.Context()); err != nil {
			writeError(w, http.StatusInternalServerError, "commit failed")
			return
		}

		var proj Project
		_ = pool.QueryRow(r.Context(), `
			SELECT id, team_id, name, is_default, false AS is_pinned, 0, 0,
			       created_at, modified_at, deleted_at
			  FROM project WHERE id = $1`, projectID).Scan(
			&proj.ID, &proj.TeamID, &proj.Name, &proj.IsDefault, &proj.IsPinned,
			&proj.Count, &proj.TotalCount, &proj.CreatedAt, &proj.ModifiedAt, &proj.DeletedAt,
		)
		writeJSON(w, http.StatusOK, &proj)
	}
}

// ─── PATCH /api/rpc/command/rename-project ───────────────────────────────────

// RenameProjectHandler renames a project.
func RenameProjectHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeError(w, http.StatusUnauthorized, "unauthorized")
			return
		}

		var body struct {
			ID   string `json:"id"`
			Name string `json:"name"`
		}
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}

		p, err := perms.GetProjectPermissions(r.Context(), pool, profileID, body.ID)
		if err != nil || p == nil || !p.CanEdit {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		if _, err := pool.Exec(r.Context(),
			`UPDATE project SET name = $1, modified_at = now() WHERE id = $2`,
			body.Name, body.ID); err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		w.WriteHeader(http.StatusNoContent)
	}
}

// ─── DELETE /api/rpc/command/delete-project ──────────────────────────────────

// DeleteProjectHandler soft-deletes a project (requires edit permissions).
// Cannot delete the team's default project.
func DeleteProjectHandler(pool *db.Pool) http.HandlerFunc {
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

		p, err := perms.GetProjectPermissions(r.Context(), pool, profileID, body.ID)
		if err != nil || p == nil || !p.CanEdit {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		var isDefault bool
		_ = pool.QueryRow(r.Context(),
			`SELECT is_default FROM project WHERE id = $1`, body.ID).Scan(&isDefault)
		if isDefault {
			writeError(w, http.StatusUnprocessableEntity, "non-deletable-project")
			return
		}

		if _, err := pool.Exec(r.Context(),
			`UPDATE project SET deleted_at = now() WHERE id = $1`, body.ID); err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		w.WriteHeader(http.StatusNoContent)
	}
}

// ─── POST /api/rpc/command/update-project-pin ────────────────────────────────

// UpdateProjectPinHandler toggles the pinned state for a project.
func UpdateProjectPinHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeError(w, http.StatusUnauthorized, "unauthorized")
			return
		}

		var body struct {
			TeamID   string `json:"teamId"`
			ID       string `json:"id"`
			IsPinned bool   `json:"isPinned"`
		}
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}

		p, err := perms.GetProjectPermissions(r.Context(), pool, profileID, body.ID)
		if err != nil || p == nil || !p.CanRead {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		if _, err := pool.Exec(r.Context(), `
			INSERT INTO team_project_profile_rel (team_id, project_id, profile_id, is_pinned)
			VALUES ($1, $2, $3, $4)
			ON CONFLICT (team_id, project_id, profile_id)
			DO UPDATE SET is_pinned = $4`,
			body.TeamID, body.ID, profileID, body.IsPinned); err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		w.WriteHeader(http.StatusNoContent)
	}
}

// Ensure db package is used.
var _ *db.Pool
