// Package handler — teams handlers.
//
// Ported from app.rpc.commands.teams in the Clojure backend.
// Covers: get-teams, get-team, get-team-members, get-team-stats,
//         get-team-invitations, create-team, update-team, delete-team,
//         leave-team, update-team-member-role, delete-team-member.
//
// Redis cache key: logos:cache:team:<id>  (TTL 5 min, matching Clojure).
// Cache is DEL-eted on every mutating operation.
package handler

import (
	"context"
	"encoding/json"
	"net/http"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/redis/go-redis/v9"

	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/db"
	"github.com/logos-design/logos/backend-go/internal/perms"
	"github.com/logos-design/logos/backend-go/internal/transit"
)

// teamCacheKey returns the Redis key for a cached team.
func teamCacheKey(teamID string) string { return "logos:cache:team:" + teamID }

// teamCacheTTL mirrors `team-ttl` in redis_cache.clj (5 minutes).
const teamCacheTTL = 5 * time.Minute

// ─── Wire types ──────────────────────────────────────────────────────────────

// TeamPermissions is the nested permissions map returned to clients.
type TeamPermissions struct {
	Type    transit.Keyword `json:"type"`
	IsOwner bool            `json:"is-owner"`
	IsAdmin bool            `json:"is-admin"`
	CanEdit bool            `json:"can-edit"`
}

// Team is the full team record returned to authenticated callers.
type Team struct {
	ID           string          `json:"id"`
	Name         string          `json:"name"`
	PhotoID      *string         `json:"photo-id,omitempty"`
	IsDefault    bool            `json:"is-default"`
	Features     []string        `json:"features"`
	CreatedAt    time.Time       `json:"created-at"`
	ModifiedAt   time.Time       `json:"modified-at"`
	IsDefaultRef bool            `json:"is-default-team,omitempty"`
	Permissions  TeamPermissions `json:"permissions"`
}

// TeamMember is one row in the team members list.
type TeamMember struct {
	ID       string  `json:"id"`
	Email    string  `json:"email"`
	Name     string  `json:"name"`
	FullName string  `json:"fullname"`
	PhotoID  *string `json:"photoId,omitempty"`
	IsActive bool    `json:"isActive"`
	IsOwner  bool    `json:"isOwner"`
	IsAdmin  bool    `json:"isAdmin"`
	CanEdit  bool    `json:"canEdit"`
}

// TeamStats is the project/file count summary.
type TeamStats struct {
	Projects int64 `json:"projects"`
	Files    int64 `json:"files"`
}

// TeamInvitation is one pending invitation row.
type TeamInvitation struct {
	Email   string `json:"email"`
	Role    string `json:"role"`
	Expired bool   `json:"expired"`
}

// ─── GET /api/rpc/command/get-teams ──────────────────────────────────────────

// GetTeamsHandler returns all teams for the authenticated profile.
func GetTeamsHandler(pool *db.Pool, rdb *redis.Client) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			// Anonymous callers get an empty list (mirrors get-profile returning
			// the anonymous profile).  The frontend bootstrap loads get-profile
			// then get-teams; returning 401 here triggers an error toast before
			// the login redirect can run.
			writeJSON(w, http.StatusOK, []*Team{})
			return
		}

		// Fetch the profile's default-team-id first.
		var defaultTeamID string
		_ = pool.QueryRow(r.Context(),
			`SELECT default_team_id FROM profile WHERE id = $1 AND deleted_at IS NULL`,
			profileID).Scan(&defaultTeamID)

		teams, err := getTeams(r.Context(), pool, profileID, defaultTeamID)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		writeJSON(w, http.StatusOK, teams)
	}
}

func getTeams(ctx context.Context, pool *db.Pool, profileID, defaultTeamID string) ([]*Team, error) {
	const q = `
		SELECT t.id, t.name, t.photo_id, t.is_default, t.features,
		       t.created_at, t.modified_at,
		       tp.is_owner, tp.is_admin, tp.can_edit,
		       (t.id = $1) AS is_default_team
		  FROM team_profile_rel AS tp
		  JOIN team AS t ON (t.id = tp.team_id)
		 WHERE t.deleted_at IS NULL
		   AND tp.profile_id = $2
		 ORDER BY tp.created_at ASC`

	rows, err := pool.Query(ctx, q, defaultTeamID, profileID)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var teams []*Team
	for rows.Next() {
		t, err := scanTeamRow(rows)
		if err != nil {
			return nil, err
		}
		teams = append(teams, t)
	}
	return teams, rows.Err()
}

// ─── GET /api/rpc/command/get-team ───────────────────────────────────────────

// GetTeamHandler returns a single team by ?id= or ?file-id=.
func GetTeamHandler(pool *db.Pool, rdb *redis.Client) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
			return
		}

		teamID := r.URL.Query().Get("id")
		fileID := r.URL.Query().Get("file-id")

		if teamID == "" && fileID == "" {
			writeError(w, http.StatusBadRequest, "id or file-id required")
			return
		}

		// Cache only for the direct by-id path (matching Clojure).
		if teamID != "" && fileID == "" && rdb != nil {
			if cached, ok := getTeamFromCache(r.Context(), rdb, teamID); ok {
				writeJSON(w, http.StatusOK, cached)
				return
			}
		}

		team, err := getTeam(r.Context(), pool, profileID, teamID, fileID)
		if err != nil {
			if err == pgx.ErrNoRows {
				writeError(w, http.StatusNotFound, "team-does-not-exist")
				return
			}
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}

		if teamID != "" && fileID == "" && rdb != nil {
			setTeamInCache(r.Context(), rdb, teamID, team)
		}

		writeJSON(w, http.StatusOK, team)
	}
}

// TeamInfo is the minimal public team record returned by get-team-info.
type TeamInfo struct {
	ID        string   `json:"id"`
	IsDefault bool     `json:"is-default"`
	Features  []string `json:"features"`
}

// ─── GET /api/rpc/command/get-team-info ──────────────────────────────────────

// GetTeamInfoHandler returns minimal team metadata. No authentication required.
func GetTeamInfoHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		teamID := rpcParam(r, "id", "team-id", "teamId")
		if teamID == "" {
			teamID = r.URL.Query().Get("id")
		}
		if teamID == "" {
			writeError(w, http.StatusBadRequest, "id required")
			return
		}

		var info TeamInfo
		err := pool.QueryRow(r.Context(),
			`SELECT id, is_default, COALESCE(features, '{}')
			   FROM team
			  WHERE id = $1 AND deleted_at IS NULL`, teamID).
			Scan(&info.ID, &info.IsDefault, &info.Features)
		if err != nil {
			writeError(w, http.StatusNotFound, "team-does-not-exist")
			return
		}
		writeJSON(w, http.StatusOK, info)
	}
}

func getTeam(ctx context.Context, pool *db.Pool, profileID, teamID, fileID string) (*Team, error) {
	// Resolve the caller's default-team-id for the is_default_team column.
	var defaultTeamID string
	_ = pool.QueryRow(ctx,
		`SELECT default_team_id FROM profile WHERE id = $1 AND deleted_at IS NULL`,
		profileID).Scan(&defaultTeamID)

	var q string
	var args []any

	base := `
		SELECT t.id, t.name, t.photo_id, t.is_default, t.features,
		       t.created_at, t.modified_at,
		       tp.is_owner, tp.is_admin, tp.can_edit,
		       (t.id = $1) AS is_default_team
		  FROM team_profile_rel AS tp
		  JOIN team AS t ON (t.id = tp.team_id)
		 WHERE t.deleted_at IS NULL
		   AND tp.profile_id = $2`

	if teamID != "" {
		q = base + " AND t.id = $3"
		args = []any{defaultTeamID, profileID, teamID}
	} else {
		q = base + `
		   AND t.id = (
		     SELECT p.team_id FROM project AS p
		       JOIN file AS f ON (f.project_id = p.id)
		      WHERE f.id = $3
		   )`
		args = []any{defaultTeamID, profileID, fileID}
	}

	row := pool.QueryRow(ctx, q, args...)
	return scanTeamRow(row)
}

// ─── GET /api/rpc/command/get-team-members ───────────────────────────────────

// GetTeamMembersHandler returns all members of a team.
func GetTeamMembersHandler(pool *db.Pool) http.HandlerFunc {
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

		members, err := getTeamMembers(r.Context(), pool, teamID)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		writeJSON(w, http.StatusOK, members)
	}
}

func getTeamMembers(ctx context.Context, pool *db.Pool, teamID string) ([]*TeamMember, error) {
	const q = `
		SELECT tp.is_owner, tp.is_admin, tp.can_edit,
		       p.id, p.email, p.fullname, p.photo_id, p.is_active
		  FROM team_profile_rel AS tp
		  JOIN profile AS p ON (p.id = tp.profile_id)
		 WHERE tp.team_id = $1`

	rows, err := pool.Query(ctx, q, teamID)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var members []*TeamMember
	for rows.Next() {
		m := &TeamMember{}
		if err := rows.Scan(
			&m.IsOwner, &m.IsAdmin, &m.CanEdit,
			&m.ID, &m.Email, &m.FullName, &m.PhotoID, &m.IsActive,
		); err != nil {
			return nil, err
		}
		m.Name = m.FullName
		members = append(members, m)
	}
	return members, rows.Err()
}

// ─── GET /api/rpc/command/get-team-stats ─────────────────────────────────────

// GetTeamStatsHandler returns project and file counts for a team.
func GetTeamStatsHandler(pool *db.Pool) http.HandlerFunc {
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

		var stats TeamStats
		err = pool.QueryRow(r.Context(), `
			SELECT
			  (SELECT count(*) FROM project WHERE team_id = $1) AS projects,
			  (SELECT count(*) FROM file AS f
			     JOIN project AS p ON (p.id = f.project_id)
			    WHERE p.team_id = $1) AS files`,
			teamID).Scan(&stats.Projects, &stats.Files)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		writeJSON(w, http.StatusOK, stats)
	}
}

// ─── GET /api/rpc/command/get-team-invitations ───────────────────────────────

// GetTeamInvitationsHandler lists pending/expired invitations for a team.
func GetTeamInvitationsHandler(pool *db.Pool) http.HandlerFunc {
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

		rows, err := pool.Query(r.Context(), `
			SELECT email_to, role, (valid_until < now()) AS expired
			  FROM team_invitation
			 WHERE team_id = $1
			 ORDER BY valid_until DESC, created_at DESC`,
			teamID)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		defer rows.Close()

		var invitations []*TeamInvitation
		for rows.Next() {
			inv := &TeamInvitation{}
			if err := rows.Scan(&inv.Email, &inv.Role, &inv.Expired); err != nil {
				writeError(w, http.StatusInternalServerError, "internal server error")
				return
			}
			invitations = append(invitations, inv)
		}
		if err := rows.Err(); err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		writeJSON(w, http.StatusOK, invitations)
	}
}

// ─── POST /api/rpc/command/create-team ───────────────────────────────────────

type createTeamParams struct {
	Name string `json:"name"`
	ID   string `json:"id,omitempty"`
}

// CreateTeamHandler creates a team, default project, and sets the caller as owner.
func CreateTeamHandler(pool *db.Pool, rdb *redis.Client) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
			return
		}

		var params createTeamParams
		if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}
		if params.Name == "" {
			writeError(w, http.StatusBadRequest, "name required")
			return
		}

		// Use a transaction: create team → owner role → default project.
		tx, err := pool.Begin(r.Context())
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		defer tx.Rollback(r.Context()) //nolint:errcheck

		teamID := params.ID
		if teamID == "" {
			teamID = newUUID()
		}

		// Insert team.
		if _, err = tx.Exec(r.Context(),
			`INSERT INTO team (id, name, is_default) VALUES ($1, $2, false)`,
			teamID, params.Name); err != nil {
			writeError(w, http.StatusInternalServerError, "create team failed")
			return
		}

		// Insert owner role.
		if _, err = tx.Exec(r.Context(),
			`INSERT INTO team_profile_rel (team_id, profile_id, is_owner, is_admin, can_edit)
			 VALUES ($1, $2, true, true, true)`,
			teamID, profileID); err != nil {
			writeError(w, http.StatusInternalServerError, "create owner role failed")
			return
		}

		// Insert default project "Drafts".
		projectID := newUUID()
		if _, err = tx.Exec(r.Context(),
			`INSERT INTO project (id, team_id, name, is_default) VALUES ($1, $2, 'Drafts', true)`,
			projectID, teamID); err != nil {
			writeError(w, http.StatusInternalServerError, "create default project failed")
			return
		}

		// Set project owner role.
		if _, err = tx.Exec(r.Context(),
			`INSERT INTO project_profile_rel (project_id, profile_id, is_owner, is_admin, can_edit)
			 VALUES ($1, $2, true, true, true)`,
			projectID, profileID); err != nil {
			writeError(w, http.StatusInternalServerError, "create project role failed")
			return
		}

		if err = tx.Commit(r.Context()); err != nil {
			writeError(w, http.StatusInternalServerError, "commit failed")
			return
		}

		writeJSON(w, http.StatusOK, map[string]string{
			"id":              teamID,
			"name":            params.Name,
			"defaultProjectId": projectID,
		})
	}
}

// ─── PATCH /api/rpc/command/update-team ──────────────────────────────────────

// UpdateTeamHandler renames a team (requires edit permissions).
func UpdateTeamHandler(pool *db.Pool, rdb *redis.Client) http.HandlerFunc {
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

		p, err := perms.GetTeamPermissions(r.Context(), pool, profileID, body.ID)
		if err != nil || p == nil || !p.CanEdit {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		if _, err := pool.Exec(r.Context(),
			`UPDATE team SET name = $1, modified_at = now() WHERE id = $2`,
			body.Name, body.ID); err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}

		invalidateTeamCache(r.Context(), rdb, body.ID)
		w.WriteHeader(http.StatusNoContent)
	}
}

// ─── DELETE /api/rpc/command/delete-team ─────────────────────────────────────

// DeleteTeamHandler soft-deletes a team (owner only; cannot delete default team).
func DeleteTeamHandler(pool *db.Pool, rdb *redis.Client) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
			return
		}

		var body struct {
			ID string `json:"id"`
		}
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}

		p, err := perms.GetTeamPermissions(r.Context(), pool, profileID, body.ID)
		if err != nil || p == nil || !p.IsOwner {
			writeError(w, http.StatusForbidden, "only-owner-can-delete-team")
			return
		}

		// Reject default-team deletion.
		var isDefault bool
		_ = pool.QueryRow(r.Context(),
			`SELECT is_default FROM team WHERE id = $1`, body.ID).Scan(&isDefault)
		if isDefault {
			writeError(w, http.StatusUnprocessableEntity, "non-deletable-team")
			return
		}

		if _, err := pool.Exec(r.Context(),
			`UPDATE team SET deleted_at = now() WHERE id = $1`, body.ID); err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}

		invalidateTeamCache(r.Context(), rdb, body.ID)
		w.WriteHeader(http.StatusNoContent)
	}
}

// ─── POST /api/rpc/command/leave-team ────────────────────────────────────────

// LeaveTeamHandler removes the current profile from a team.
// If the caller is owner and there are other members, they must reassign first.
func LeaveTeamHandler(pool *db.Pool, rdb *redis.Client) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
			return
		}

		var body struct {
			ID         string `json:"id"`
			ReassignTo string `json:"reassignTo,omitempty"`
		}
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}

		tx, err := pool.Begin(r.Context())
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		defer tx.Rollback(r.Context()) //nolint:errcheck

		// Count members.
		var memberCount int
		_ = tx.QueryRow(r.Context(),
			`SELECT count(*) FROM team_profile_rel WHERE team_id = $1`, body.ID).Scan(&memberCount)
		if memberCount <= 1 {
			writeError(w, http.StatusUnprocessableEntity, "no-enough-members-for-leave")
			return
		}

		p, _ := perms.GetTeamPermissions(r.Context(), pool, profileID, body.ID)

		// Reassign owner if requested.
		if body.ReassignTo != "" && body.ReassignTo != profileID {
			// Unset current owner.
			if _, err := tx.Exec(r.Context(),
				`UPDATE team_profile_rel SET is_owner = false WHERE team_id = $1 AND profile_id = $2`,
				body.ID, profileID); err != nil {
				writeError(w, http.StatusInternalServerError, "reassign failed")
				return
			}
			// Set new owner.
			if _, err := tx.Exec(r.Context(),
				`UPDATE team_profile_rel SET is_owner = true, is_admin = true, can_edit = true
				  WHERE team_id = $1 AND profile_id = $2`,
				body.ID, body.ReassignTo); err != nil {
				writeError(w, http.StatusInternalServerError, "reassign failed")
				return
			}
		} else if p != nil && p.IsOwner {
			writeError(w, http.StatusUnprocessableEntity, "owner-cant-leave-team")
			return
		}

		if _, err := tx.Exec(r.Context(),
			`DELETE FROM team_profile_rel WHERE team_id = $1 AND profile_id = $2`,
			body.ID, profileID); err != nil {
			writeError(w, http.StatusInternalServerError, "leave team failed")
			return
		}

		if err := tx.Commit(r.Context()); err != nil {
			writeError(w, http.StatusInternalServerError, "commit failed")
			return
		}

		invalidateTeamCache(r.Context(), rdb, body.ID)
		w.WriteHeader(http.StatusNoContent)
	}
}

// ─── PATCH /api/rpc/command/update-team-member-role ──────────────────────────

// UpdateTeamMemberRoleHandler changes the role of a team member.
// Only owner can promote to owner. Admins can otherwise change roles.
func UpdateTeamMemberRoleHandler(pool *db.Pool, rdb *redis.Client) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
			return
		}

		var body struct {
			TeamID   string `json:"teamId"`
			MemberID string `json:"memberId"`
			Role     string `json:"role"` // "owner" | "admin" | "editor" | "viewer"
		}
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}

		p, err := perms.GetTeamPermissions(r.Context(), pool, profileID, body.TeamID)
		if err != nil || p == nil || (!p.IsOwner && !p.IsAdmin) {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		// Only owner can promote to owner.
		if body.Role == "owner" && !p.IsOwner {
			writeError(w, http.StatusForbidden, "cant-promote-to-owner")
			return
		}

		// Verify target is not already an owner.
		var targetIsOwner bool
		_ = pool.QueryRow(r.Context(),
			`SELECT is_owner FROM team_profile_rel WHERE team_id = $1 AND profile_id = $2`,
			body.TeamID, body.MemberID).Scan(&targetIsOwner)
		if targetIsOwner {
			writeError(w, http.StatusUnprocessableEntity, "cant-change-role-to-owner")
			return
		}

		newFlags := roleToFlags(body.Role)
		tx, err := pool.Begin(r.Context())
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		defer tx.Rollback(r.Context()) //nolint:errcheck

		if body.Role == "owner" {
			// Only one owner allowed — demote current owner first.
			if _, err := tx.Exec(r.Context(),
				`UPDATE team_profile_rel SET is_owner = false
				  WHERE team_id = $1 AND profile_id = $2`, body.TeamID, profileID); err != nil {
				writeError(w, http.StatusInternalServerError, "internal server error")
				return
			}
		}

		if _, err := tx.Exec(r.Context(),
			`UPDATE team_profile_rel
			    SET is_owner = $3, is_admin = $4, can_edit = $5
			  WHERE team_id = $1 AND profile_id = $2`,
			body.TeamID, body.MemberID,
			newFlags[0], newFlags[1], newFlags[2]); err != nil {
			writeError(w, http.StatusInternalServerError, "update role failed")
			return
		}

		if err := tx.Commit(r.Context()); err != nil {
			writeError(w, http.StatusInternalServerError, "commit failed")
			return
		}

		invalidateTeamCache(r.Context(), rdb, body.TeamID)
		w.WriteHeader(http.StatusNoContent)
	}
}

// ─── DELETE /api/rpc/command/delete-team-member ──────────────────────────────

// DeleteTeamMemberHandler removes a member from a team (admin or owner only).
func DeleteTeamMemberHandler(pool *db.Pool, rdb *redis.Client) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
			return
		}

		var body struct {
			TeamID   string `json:"teamId"`
			MemberID string `json:"memberId"`
		}
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}

		p, err := perms.GetTeamPermissions(r.Context(), pool, profileID, body.TeamID)
		if err != nil || p == nil || (!p.IsOwner && !p.IsAdmin) {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		if body.MemberID == profileID {
			writeError(w, http.StatusUnprocessableEntity, "cant-remove-yourself")
			return
		}

		if _, err := pool.Exec(r.Context(),
			`DELETE FROM team_profile_rel WHERE team_id = $1 AND profile_id = $2`,
			body.TeamID, body.MemberID); err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}

		invalidateTeamCache(r.Context(), rdb, body.TeamID)
		w.WriteHeader(http.StatusNoContent)
	}
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

// scanTeamRow scans a row (from Query or QueryRow) into a *Team.
func scanTeamRow(row interface {
	Scan(dest ...any) error
}) (*Team, error) {
	t := &Team{Features: []string{}}
	var isOwner, isAdmin, canEdit bool
	var features []string

	err := row.Scan(
		&t.ID, &t.Name, &t.PhotoID, &t.IsDefault, &features,
		&t.CreatedAt, &t.ModifiedAt,
		&isOwner, &isAdmin, &canEdit, &t.IsDefaultRef,
	)
	if err != nil {
		return nil, err
	}
	if features != nil {
		t.Features = features
	}
	t.Permissions = TeamPermissions{
		Type:    transit.Keyword("membership"),
		IsOwner: isOwner,
		IsAdmin: isOwner || isAdmin,
		CanEdit: isOwner || isAdmin || canEdit,
	}
	return t, nil
}

// roleToFlags converts a role string to (isOwner, isAdmin, canEdit).
func roleToFlags(role string) [3]bool {
	switch role {
	case "owner":
		return [3]bool{true, true, true}
	case "admin":
		return [3]bool{false, true, true}
	case "editor":
		return [3]bool{false, false, true}
	default: // viewer
		return [3]bool{false, false, false}
	}
}

// invalidateTeamCache removes the cached team entry from Redis.
func invalidateTeamCache(ctx context.Context, rdb *redis.Client, teamID string) {
	if rdb == nil {
		return
	}
	_ = rdb.Del(ctx, teamCacheKey(teamID)).Err()
}

func getTeamFromCache(ctx context.Context, rdb *redis.Client, teamID string) (*Team, bool) {
	data, err := rdb.Get(ctx, teamCacheKey(teamID)).Bytes()
	if err != nil {
		return nil, false
	}
	var t Team
	if json.Unmarshal(data, &t) != nil {
		return nil, false
	}
	return &t, true
}

func setTeamInCache(ctx context.Context, rdb *redis.Client, teamID string, t *Team) {
	data, err := json.Marshal(t)
	if err != nil {
		return
	}
	_ = rdb.Set(ctx, teamCacheKey(teamID), data, teamCacheTTL).Err()
}
