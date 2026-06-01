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
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"
	"sync"
	"time"

	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/db"
	"github.com/logos-design/logos/backend-go/internal/perms"
	"github.com/logos-design/logos/backend-go/internal/storage"
)

// Project is the JSON-serialisable project record.
// Field names use kebab-case for Transit keyword keys (~:team-id, ~:is-default, …).
type Project struct {
	ID          string     `json:"id"`
	TeamID      string     `json:"team-id"`
	Name        string     `json:"name"`
	Description string     `json:"description,omitempty"`
	PhotoID     *string    `json:"photo-id,omitempty"`
	FileID      *string    `json:"file-id,omitempty"`
	IsDefault   bool       `json:"is-default"`
	IsPinned    bool       `json:"is-pinned"`
	Count       int64      `json:"count"`
	TotalCount  int64      `json:"total-count"`
	CreatedAt   time.Time  `json:"created-at"`
	ModifiedAt  time.Time  `json:"modified-at"`
	DeletedAt   *time.Time `json:"deleted-at,omitempty"`
}

// CreatedProject is returned by create-project. Includes the auto-seeded design
// file so the client can navigate straight into the workspace (Figma-style flow).
type CreatedProject struct {
	Project
	Pages []string `json:"pages"`
}

const sqlProjectFileID = `(SELECT f.id::text FROM file AS f
         WHERE f.project_id = p.id AND f.deleted_at IS NULL
         ORDER BY f.created_at ASC LIMIT 1)`

const sqlProjectMetaSelect = `'' AS description, NULL::text AS photo_id`

var (
	projectMetaOnce sync.Once
	projectMetaOK   bool
)

func projectMetaColumnsEnabled(ctx context.Context, pool *db.Pool) bool {
	projectMetaOnce.Do(func() {
		_ = pool.QueryRow(ctx, `
			SELECT EXISTS (
			  SELECT 1 FROM information_schema.columns
			  WHERE table_schema = 'public'
			    AND table_name = 'project'
			    AND column_name = 'description'
			)`).Scan(&projectMetaOK)
	})
	return projectMetaOK
}

// AllProject extends Project with team context (get-all-projects response).
type AllProject struct {
	Project
	TeamName      string `json:"team-name"`
	IsDefaultTeam bool   `json:"is-default-team"`
}

const sqlAllProjects = `
SELECT p.id, p.team_id, p.name, p.is_default,
       ` + sqlProjectMetaSelect + `,
       ` + sqlProjectFileID + ` AS file_id,
       COALESCE(tpp.is_pinned, false) AS is_pinned,
       (SELECT count(*) FROM file AS f
         WHERE f.project_id = p.id AND f.deleted_at IS NULL) AS count,
       (SELECT count(*) FROM file AS f
         WHERE f.project_id = p.id) AS total_count,
       p.created_at, p.modified_at, p.deleted_at,
       t.name AS team_name, t.is_default AS is_default_team
  FROM project AS p
  JOIN team AS t ON (t.id = p.team_id)
  JOIN team_profile_rel AS tpr
        ON (tpr.team_id = t.id AND tpr.profile_id = $1)
  LEFT JOIN team_project_profile_rel AS tpp
        ON (tpp.project_id = p.id AND tpp.team_id = p.team_id AND tpp.profile_id = $1)
 WHERE t.deleted_at IS NULL
   AND p.deleted_at IS NULL
 ORDER BY t.name, p.name`

// ─── GET /api/rpc/command/get-all-projects ───────────────────────────────────

// GetAllProjectsHandler lists every project the profile can access across all teams.
func GetAllProjectsHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
			return
		}

		rows, err := pool.Query(r.Context(), sqlAllProjects, profileID)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		defer rows.Close()

		var projects []*AllProject
		for rows.Next() {
			proj := &AllProject{}
			if err := rows.Scan(
				&proj.ID, &proj.TeamID, &proj.Name, &proj.IsDefault,
				&proj.Description, &proj.PhotoID, &proj.FileID,
				&proj.IsPinned, &proj.Count, &proj.TotalCount,
				&proj.CreatedAt, &proj.ModifiedAt, &proj.DeletedAt,
				&proj.TeamName, &proj.IsDefaultTeam,
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
		if projects == nil {
			projects = []*AllProject{}
		}
		writeJSON(w, http.StatusOK, projects)
	}
}

// ─── GET /api/rpc/command/get-projects ───────────────────────────────────────

// GetProjectsHandler lists all projects the profile can see in a team.
func GetProjectsHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
			return
		}
		teamID := rpcParam(r, "team-id", "teamId")
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
			       ` + sqlProjectMetaSelect + `,
			       ` + sqlProjectFileID + ` AS file_id,
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
			   AND p.deleted_at IS NULL
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
				&proj.Description, &proj.PhotoID, &proj.FileID,
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
		if projects == nil {
			projects = []*Project{}
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
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
			return
		}
		projectID := rpcParam(r, "id", "project-id", "projectId")
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
			SELECT id, team_id, name, is_default,
			       `+sqlProjectMetaSelect+`,
			       (SELECT f.id::text FROM file AS f
			         WHERE f.project_id = project.id AND f.deleted_at IS NULL
			         ORDER BY f.created_at ASC LIMIT 1) AS file_id,
			       false AS is_pinned,
			       (SELECT count(*) FROM file AS f
			         WHERE f.project_id = project.id AND f.deleted_at IS NULL) AS count,
			       (SELECT count(*) FROM file AS f
			         WHERE f.project_id = project.id) AS total_count,
			       created_at, modified_at, deleted_at
			  FROM project
			 WHERE id = $1 AND deleted_at IS NULL`, projectID).Scan(
			&proj.ID, &proj.TeamID, &proj.Name, &proj.IsDefault,
			&proj.Description, &proj.PhotoID, &proj.FileID,
			&proj.IsPinned, &proj.Count, &proj.TotalCount,
			&proj.CreatedAt, &proj.ModifiedAt, &proj.DeletedAt,
		)
		if err != nil {
			writeError(w, http.StatusNotFound, "project-not-found")
			return
		}
		writeJSON(w, http.StatusOK, &proj)
	}
}

// ─── POST /api/rpc/command/create-project ────────────────────────────────────

type createProjectInput struct {
	TeamID      string
	Name        string
	Description string
	ProjectID   string
	Photo       io.Reader
	PhotoName   string
	PhotoSize   int64
	PhotoType   string
}

// CreateProjectHandler creates a new project under a team and seeds a starter
// design file so the client can open the editor immediately (Figma-style).
func CreateProjectHandler(pool *db.Pool, store storage.Backend) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
			return
		}

		input, err := parseCreateProjectInput(r)
		if err != nil {
			writeError(w, http.StatusBadRequest, err.Error())
			return
		}
		if input.TeamID == "" || input.Name == "" {
			writeError(w, http.StatusBadRequest, "teamId and name required")
			return
		}

		p, err := perms.GetTeamPermissions(r.Context(), pool, profileID, input.TeamID)
		if err != nil || p == nil || !p.CanEdit {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		projectID := input.ProjectID
		if projectID == "" {
			projectID = newUUID()
		}

		var photoID *string
		metaCols := projectMetaColumnsEnabled(r.Context(), pool)
		if input.Photo != nil && store != nil && metaCols {
			id, err := saveProjectPhoto(pool, store, r.Context(), input)
			if err != nil {
				writeError(w, http.StatusInternalServerError, "photo upload failed")
				return
			}
			photoID = id
		}

		tx, err := pool.Begin(r.Context())
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		defer tx.Rollback(r.Context()) //nolint:errcheck

		if metaCols {
			if _, err := tx.Exec(r.Context(),
				`INSERT INTO project (id, team_id, name, is_default, description, photo_id)
				 VALUES ($1, $2, $3, false, $4, $5)`,
				projectID, input.TeamID, input.Name, input.Description, photoID); err != nil {
				writeError(w, http.StatusInternalServerError, "create project failed")
				return
			}
		} else if _, err := tx.Exec(r.Context(),
			`INSERT INTO project (id, team_id, name, is_default) VALUES ($1, $2, $3, false)`,
			projectID, input.TeamID, input.Name); err != nil {
			writeError(w, http.StatusInternalServerError, "create project failed")
			return
		}

		if _, err := tx.Exec(r.Context(),
			`INSERT INTO project_profile_rel (project_id, profile_id, is_owner, is_admin, can_edit)
			 VALUES ($1, $2, true, true, true)`,
			projectID, profileID); err != nil {
			writeError(w, http.StatusInternalServerError, "create project role failed")
			return
		}

		if _, err := tx.Exec(r.Context(),
			`INSERT INTO team_project_profile_rel (team_id, project_id, profile_id, is_pinned)
			 VALUES ($1, $2, $3, false)
			 ON CONFLICT (team_id, project_id, profile_id) DO NOTHING`,
			input.TeamID, projectID, profileID); err != nil {
			writeError(w, http.StatusInternalServerError, "link project to team failed")
			return
		}

		createdFile, err := seedStarterFile(r.Context(), tx, profileID, projectID, input.Name)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "create starter file failed")
			return
		}

		if err := tx.Commit(r.Context()); err != nil {
			writeError(w, http.StatusInternalServerError, "commit failed")
			return
		}

		var proj Project
		_ = pool.QueryRow(r.Context(), `
			SELECT id, team_id, name, is_default,
			       `+sqlProjectMetaSelect+`,
			       $2::text AS file_id,
			       false AS is_pinned,
			       (SELECT count(*) FROM file AS f
			         WHERE f.project_id = project.id AND f.deleted_at IS NULL) AS count,
			       (SELECT count(*) FROM file AS f
			         WHERE f.project_id = project.id) AS total_count,
			       created_at, modified_at, deleted_at
			  FROM project WHERE id = $1`, projectID, createdFile.ID).Scan(
			&proj.ID, &proj.TeamID, &proj.Name, &proj.IsDefault,
			&proj.Description, &proj.PhotoID, &proj.FileID,
			&proj.IsPinned, &proj.Count, &proj.TotalCount,
			&proj.CreatedAt, &proj.ModifiedAt, &proj.DeletedAt,
		)
		if !metaCols && input.Description != "" {
			proj.Description = input.Description
		}
		if !metaCols && photoID != nil {
			proj.PhotoID = photoID
		}

		writeJSON(w, http.StatusOK, &CreatedProject{
			Project: proj,
			Pages:   createdFile.Pages,
		})
	}
}

func parseCreateProjectInput(r *http.Request) (createProjectInput, error) {
	ct := strings.ToLower(r.Header.Get("Content-Type"))
	if strings.HasPrefix(ct, "multipart/form-data") {
		return parseCreateProjectMultipart(r)
	}
	return parseCreateProjectJSON(r)
}

func parseCreateProjectJSON(r *http.Request) (createProjectInput, error) {
	var body map[string]any
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
		return createProjectInput{}, fmt.Errorf("invalid JSON body")
	}
	return createProjectInput{
		TeamID:      jsonFieldString(body, "teamId", "team-id"),
		Name:        jsonFieldString(body, "name"),
		Description: jsonFieldString(body, "description"),
		ProjectID:   jsonFieldString(body, "id"),
	}, nil
}

func parseCreateProjectMultipart(r *http.Request) (createProjectInput, error) {
	if err := r.ParseMultipartForm(10 << 20); err != nil {
		return createProjectInput{}, fmt.Errorf("invalid multipart body")
	}
	input := createProjectInput{
		TeamID:      r.FormValue("team-id"),
		Name:        r.FormValue("name"),
		Description: r.FormValue("description"),
		ProjectID:   r.FormValue("id"),
	}
	if input.TeamID == "" {
		input.TeamID = r.FormValue("teamId")
	}

	file, header, err := r.FormFile("photo")
	if err == nil && file != nil {
		input.Photo = file
		input.PhotoName = header.Filename
		input.PhotoSize = header.Size
		input.PhotoType = header.Header.Get("Content-Type")
		if input.PhotoType == "" {
			input.PhotoType = "image/jpeg"
		}
	}
	return input, nil
}

func saveProjectPhoto(pool *db.Pool, store storage.Backend, ctx context.Context, input createProjectInput) (*string, error) {
	if input.Photo == nil {
		return nil, nil
	}
	defer func() {
		if c, ok := input.Photo.(io.Closer); ok {
			_ = c.Close()
		}
	}()

	objectID := newUUID()
	metaJSON := fmt.Sprintf(`{"bucket":"project","content-type":%q}`, input.PhotoType)
	if _, err := pool.Exec(ctx,
		`INSERT INTO storage_object (id, backend, size, metadata) VALUES ($1, $2, $3, $4)`,
		objectID, "local", input.PhotoSize, metaJSON); err != nil {
		return nil, err
	}
	if err := store.Put(ctx, "project", objectID, input.Photo, input.PhotoSize, input.PhotoType); err != nil {
		return nil, err
	}
	return &objectID, nil
}

// ─── PATCH /api/rpc/command/rename-project ───────────────────────────────────

// RenameProjectHandler renames a project.
func RenameProjectHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
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
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
			return
		}

		var body map[string]any
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}
		projectID := jsonFieldString(body, "id", "project-id", "projectId")
		if projectID == "" {
			writeError(w, http.StatusBadRequest, "id required")
			return
		}

		p, err := perms.GetProjectPermissions(r.Context(), pool, profileID, projectID)
		if err != nil || p == nil || !p.CanEdit {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		var isDefault bool
		err = pool.QueryRow(r.Context(),
			`SELECT is_default FROM project WHERE id = $1 AND deleted_at IS NULL`, projectID).Scan(&isDefault)
		if err != nil {
			writeError(w, http.StatusNotFound, "project-not-found")
			return
		}
		if isDefault {
			writeError(w, http.StatusUnprocessableEntity, "non-deletable-project")
			return
		}

		tag, err := pool.Exec(r.Context(),
			`UPDATE project SET deleted_at = now(), modified_at = now()
			  WHERE id = $1 AND deleted_at IS NULL`, projectID)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		if tag.RowsAffected() == 0 {
			writeError(w, http.StatusNotFound, "project-not-found")
			return
		}
		writeJSON(w, http.StatusOK, map[string]any{})
	}
}

// ─── POST /api/rpc/command/update-project-pin ────────────────────────────────

// UpdateProjectPinHandler toggles the pinned state for a project.
func UpdateProjectPinHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
			return
		}

		var body map[string]any
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}
		teamID := jsonFieldString(body, "teamId", "team-id")
		projectID := jsonFieldString(body, "id")
		isPinned := jsonFieldBool(body, "isPinned", "is-pinned")

		p, err := perms.GetProjectPermissions(r.Context(), pool, profileID, projectID)
		if err != nil || p == nil || !p.CanRead {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		if _, err := pool.Exec(r.Context(), `
			INSERT INTO team_project_profile_rel (team_id, project_id, profile_id, is_pinned)
			VALUES ($1, $2, $3, $4)
			ON CONFLICT (team_id, project_id, profile_id)
			DO UPDATE SET is_pinned = $4`,
			teamID, projectID, profileID, isPinned); err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		w.WriteHeader(http.StatusNoContent)
	}
}

// Ensure db package is used.
var _ *db.Pool
