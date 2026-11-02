// Package perms implements the team/project permission model used by all
// handler packages.
//
// Permission hierarchy (highest to lowest):
//
//	owner > admin > editor (can_edit) > viewer (can_read)
//
// Rule: owner implies admin; admin implies can_edit; can_edit implies can_read.
// This matches the Clojure `app.rpc.permissions` namespace exactly.
package perms

import (
	"context"
	"net/http"

	"github.com/logos-design/logos/backend-go/internal/db"
)

// TeamPerms holds the resolved permission set for one (profile, team) pair.
type TeamPerms struct {
	IsOwner bool
	IsAdmin bool
	CanEdit bool
	CanRead bool
}

// GetTeamPermissions resolves the permission set for profile-id / team-id.
// Returns nil if the profile has no relationship to the team.
func GetTeamPermissions(ctx context.Context, pool *db.Pool, profileID, teamID string) (*TeamPerms, error) {
	const q = `
		SELECT tpr.is_owner, tpr.is_admin, tpr.can_edit
		  FROM team_profile_rel AS tpr
		  JOIN team AS t ON (t.id = tpr.team_id)
		 WHERE tpr.profile_id = $1
		   AND tpr.team_id = $2
		   AND t.deleted_at IS NULL`

	rows, err := pool.Query(ctx, q, profileID, teamID)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var isOwner, isAdmin, canEdit bool
	found := false
	for rows.Next() {
		var o, a, e bool
		if err := rows.Scan(&o, &a, &e); err != nil {
			return nil, err
		}
		if o {
			isOwner = true
		}
		if a {
			isAdmin = true
		}
		if e {
			canEdit = true
		}
		found = true
	}
	if err := rows.Err(); err != nil {
		return nil, err
	}
	if !found {
		return nil, nil
	}

	return &TeamPerms{
		IsOwner: isOwner,
		IsAdmin: isOwner || isAdmin,
		CanEdit: isOwner || isAdmin || canEdit,
		CanRead: true,
	}, nil
}

// ProjectPerms holds the resolved permissions for one (profile, project) pair.
// It unions team-level and project-level permission rows.
type ProjectPerms struct {
	IsOwner bool
	IsAdmin bool
	CanEdit bool
	CanRead bool
}

// GetProjectPermissions unions team-level and project-level rows.
func GetProjectPermissions(ctx context.Context, pool *db.Pool, profileID, projectID string) (*ProjectPerms, error) {
	const q = `
		SELECT tpr.is_owner, tpr.is_admin, tpr.can_edit
		  FROM team_profile_rel AS tpr
		  JOIN project AS p ON (p.team_id = tpr.team_id)
		 WHERE p.id = $1
		   AND tpr.profile_id = $2
		UNION ALL
		SELECT ppr.is_owner, ppr.is_admin, ppr.can_edit
		  FROM project_profile_rel AS ppr
		 WHERE ppr.project_id = $1
		   AND ppr.profile_id = $2`

	rows, err := pool.Query(ctx, q, projectID, profileID)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var isOwner, isAdmin, canEdit bool
	found := false
	for rows.Next() {
		var o, a, e bool
		if err := rows.Scan(&o, &a, &e); err != nil {
			return nil, err
		}
		if o {
			isOwner = true
		}
		if a {
			isAdmin = true
		}
		if e {
			canEdit = true
		}
		found = true
	}
	if err := rows.Err(); err != nil {
		return nil, err
	}
	if !found {
		return nil, nil
	}

	return &ProjectPerms{
		IsOwner: isOwner,
		IsAdmin: isOwner || isAdmin,
		CanEdit: isOwner || isAdmin || canEdit,
		CanRead: true,
	}, nil
}

// DenyForbidden writes a 403 and returns false when p is nil or the check fails.
// Use as a guard at the top of handlers.
func DenyForbidden(w http.ResponseWriter, canProceed bool) bool {
	if !canProceed {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusForbidden)
		_, _ = w.Write([]byte(`{"error":"insufficient-permissions"}`))
		return false
	}
	return true
}

// ─── File permissions ─────────────────────────────────────────────────────────
// A file inherits permissions from its parent project.

// GetFilePermissions returns project-level permissions for the given file.
// It resolves the project for the file and delegates to GetProjectPermissions.
func GetFilePermissions(ctx context.Context, pool *db.Pool, profileID, fileID string) (*ProjectPerms, error) {
	var projectID string
	err := pool.QueryRow(ctx,
		`SELECT project_id FROM file WHERE id = $1 AND deleted_at IS NULL`, fileID,
	).Scan(&projectID)
	if err != nil {
		return nil, err
	}
	return GetProjectPermissions(ctx, pool, profileID, projectID)
}

// CheckFileRead returns true when the profile has at least read access to the file.
// On failure it writes 403 and returns false so the caller can "return" immediately.
func CheckFileRead(w http.ResponseWriter, r *http.Request, pool *db.Pool, profileID, fileID string) bool {
	p, err := GetFilePermissions(r.Context(), pool, profileID, fileID)
	if err != nil || p == nil || !p.CanRead {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusForbidden)
		_, _ = w.Write([]byte(`{"error":"insufficient-permissions"}`))
		return false
	}
	return true
}

// CheckFileEdit returns true when the profile has edit access to the file.
func CheckFileEdit(w http.ResponseWriter, r *http.Request, pool *db.Pool, profileID, fileID string) bool {
	p, err := GetFilePermissions(r.Context(), pool, profileID, fileID)
	if err != nil || p == nil || !p.CanEdit {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusForbidden)
		_, _ = w.Write([]byte(`{"error":"insufficient-permissions"}`))
		return false
	}
	return true
}
