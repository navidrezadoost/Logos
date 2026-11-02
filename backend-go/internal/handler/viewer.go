// Package handler — viewer endpoint.
//
// Ported from app.rpc.commands.viewer in the Clojure backend.
// The viewer endpoint is not protected by session auth — it accepts either:
//   a) a valid share-id (UUID) that maps to a share_link row, OR
//   b) an authenticated session with read permissions on the file.
//
// Returns a "view-only bundle" with minimal file data needed by the viewer UI.
package handler

import (
	"net/http"
	"strings"
	"time"

	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/db"
)

// ViewerBundle is the JSON bundle returned by get-view-only-bundle.
type ViewerBundle struct {
	File        *ViewerFile    `json:"file"`
	Project     *ViewerProject `json:"project"`
	Team        *ViewerTeam    `json:"team"`
	Libraries   []*ViewerFile  `json:"libraries"`
	ShareLinks  []*ShareLink   `json:"shareLinks"`
	Users       []*ViewerUser  `json:"users"`
	Fonts       []*TeamFont    `json:"fonts"`
	Permissions *ViewerPerms   `json:"permissions"`
}

// ViewerFile is the minimal file shape needed by the viewer.
type ViewerFile struct {
	ID         string    `json:"id"`
	ProjectID  string    `json:"projectId"`
	Name       string    `json:"name"`
	IsShared   bool      `json:"isShared"`
	Revn       int       `json:"revn"`
	ModifiedAt time.Time `json:"modifiedAt"`
}

// ViewerProject is the minimal project shape needed by the viewer.
type ViewerProject struct {
	ID     string `json:"id"`
	Name   string `json:"name"`
	TeamID string `json:"teamId"`
}

// ViewerTeam is the minimal team shape needed by the viewer.
type ViewerTeam struct {
	ID   string `json:"id"`
	Name string `json:"name"`
}

// ViewerUser is an anonymised team member for viewer contexts.
type ViewerUser struct {
	ID       string `json:"id"`
	Name     string `json:"name"`
	Email    string `json:"email"`
	CanRead  bool   `json:"canRead"`
}

// ViewerPerms describes how the viewer obtained access.
type ViewerPerms struct {
	Type       string `json:"type"` // "profile" | "share-link"
	CanComment bool   `json:"canComment"`
	CanInspect bool   `json:"canInspect"`
	InTeam     bool   `json:"inTeam"`
}

// TeamFont is a minimal font record from team_font_variant.
type TeamFont struct {
	ID     string `json:"id"`
	Name   string `json:"name"`
	TeamID string `json:"teamId"`
}

// ─── GET /api/rpc/command/get-view-only-bundle ───────────────────────────────

// GetViewOnlyBundleHandler returns all data the viewer UI needs to render a file.
// Auth: share-id (query param) or authenticated session. Returns 401 if neither.
func GetViewOnlyBundleHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		fileID := r.URL.Query().Get("file-id")
		shareID := r.URL.Query().Get("share-id")
		profileID := auth.ProfileID(r.Context())

		if fileID == "" {
			writeError(w, http.StatusBadRequest, "file-id required")
			return
		}

		// Determine access mode.
		var accessType string
		var shareLink *auth.ShareLink
		if shareID != "" {
			sl, ok := auth.ValidateShareLink(r.Context(), pool, shareID, fileID)
			if !ok {
				writeError(w, http.StatusUnauthorized, "invalid-share-token")
				return
			}
			shareLink = sl
			accessType = "share-link"
		} else if profileID != "" {
			// Validate via session: profile must have project-level read access.
			var projectID string
			err := pool.QueryRow(r.Context(),
				`SELECT project_id FROM file WHERE id = $1 AND deleted_at IS NULL`, fileID).
				Scan(&projectID)
			if err != nil {
				writeError(w, http.StatusNotFound, "file-not-found")
				return
			}

			// Look up project permissions (team + project-level union).
			var found bool
			_ = pool.QueryRow(r.Context(), `
				SELECT true FROM team_profile_rel AS tpr
				  JOIN project AS p ON (p.team_id = tpr.team_id AND p.id = $1)
				 WHERE tpr.profile_id = $2
				UNION ALL
				SELECT true FROM project_profile_rel AS ppr
				 WHERE ppr.project_id = $1 AND ppr.profile_id = $2
				LIMIT 1`, projectID, profileID).Scan(&found)

			if !found {
				writeError(w, http.StatusForbidden, "insufficient-permissions")
				return
			}
			accessType = "profile"
		} else {
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
			return
		}

		// ── Load file ──────────────────────────────────────────────────────
		var file ViewerFile
		err := pool.QueryRow(r.Context(), `
			SELECT id, project_id, name, is_shared, revn, modified_at
			  FROM file WHERE id = $1 AND deleted_at IS NULL`, fileID).
			Scan(&file.ID, &file.ProjectID, &file.Name, &file.IsShared, &file.Revn, &file.ModifiedAt)
		if err != nil {
			writeError(w, http.StatusNotFound, "file-not-found")
			return
		}

		// ── Load project ───────────────────────────────────────────────────
		var project ViewerProject
		_ = pool.QueryRow(r.Context(), `
			SELECT id, name, team_id FROM project WHERE id = $1`, file.ProjectID).
			Scan(&project.ID, &project.Name, &project.TeamID)

		// ── Load team ──────────────────────────────────────────────────────
		var team ViewerTeam
		_ = pool.QueryRow(r.Context(), `
			SELECT id, name FROM team WHERE id = $1`, project.TeamID).
			Scan(&team.ID, &team.Name)

		// ── Load team members (anonymised for share-link access) ───────────
		memberRows, _ := pool.Query(r.Context(), `
			SELECT pr.id,
			       COALESCE(pr.fullname, pr.name, '') AS name,
			       pr.email
			  FROM team_profile_rel AS tpr
			  JOIN profile AS pr ON (pr.id = tpr.profile_id)
			 WHERE tpr.team_id = $1`, project.TeamID)
		var users []*ViewerUser
		if memberRows != nil {
			defer memberRows.Close()
			memberIDs := map[string]bool{}
			for memberRows.Next() {
				u := &ViewerUser{CanRead: true}
				_ = memberRows.Scan(&u.ID, &u.Name, &u.Email)
				if accessType == "share-link" {
					u.Email = obfuscateEmail(u.Email)
				}
				memberIDs[u.ID] = true
				users = append(users, u)
			}
		}

		// ── Load linked library files ──────────────────────────────────────
		libRows, _ := pool.Query(r.Context(), `
			SELECT lf.id, lf.project_id, lf.name, lf.is_shared, lf.revn, lf.modified_at
			  FROM file_library_rel AS flr
			  JOIN file AS lf ON (lf.id = flr.library_file_id)
			 WHERE flr.file_id = $1
			   AND (lf.deleted_at IS NULL OR lf.deleted_at > now())`, fileID)
		var libs []*ViewerFile
		if libRows != nil {
			defer libRows.Close()
			for libRows.Next() {
				lib := &ViewerFile{}
				_ = libRows.Scan(&lib.ID, &lib.ProjectID, &lib.Name, &lib.IsShared, &lib.Revn, &lib.ModifiedAt)
				libs = append(libs, lib)
			}
		}

		// ── Load share links for this file ────────────────────────────────
		slRows, _ := pool.Query(r.Context(), `
			SELECT id, file_id, owner_id::text, who_comment, who_inspect, created_at
			  FROM share_link WHERE file_id = $1`, fileID)
		var shareLinks []*ShareLink
		if slRows != nil {
			defer slRows.Close()
			for slRows.Next() {
				sl := &ShareLink{}
				_ = slRows.Scan(&sl.ID, &sl.FileID, &sl.OwnerID, &sl.WhoComment, &sl.WhoInspect, &sl.CreatedAt)
				shareLinks = append(shareLinks, sl)
			}
		}

		// ── Load team fonts ───────────────────────────────────────────────
		fontRows, _ := pool.Query(r.Context(), `
			SELECT id::text, font_family, team_id::text
			  FROM team_font_variant
			 WHERE team_id = $1 AND deleted_at IS NULL`, project.TeamID)
		var fonts []*TeamFont
		if fontRows != nil {
			defer fontRows.Close()
			for fontRows.Next() {
				tf := &TeamFont{}
				_ = fontRows.Scan(&tf.ID, &tf.Name, &tf.TeamID)
				fonts = append(fonts, tf)
			}
		}

		// ── Build permissions summary ─────────────────────────────────────
		viewerPerms := &ViewerPerms{Type: accessType}
		if shareLink != nil {
			viewerPerms.CanComment = shareLink.WhoComment != "team"
			viewerPerms.CanInspect = shareLink.WhoInspect != "team"
		} else {
			viewerPerms.CanComment = true
			viewerPerms.CanInspect = true
			viewerPerms.InTeam = true
		}

		bundle := &ViewerBundle{
			File:        &file,
			Project:     &project,
			Team:        &team,
			Libraries:   libs,
			ShareLinks:  shareLinks,
			Users:       users,
			Fonts:       fonts,
			Permissions: viewerPerms,
		}
		writeJSON(w, http.StatusOK, bundle)
	}
}

// obfuscateEmail masks username and domain parts for anonymous viewers.
func obfuscateEmail(email string) string {
	parts := strings.SplitN(email, "@", 2)
	if len(parts) != 2 {
		return "****@****.***"
	}
	name := parts[0]
	domainParts := strings.SplitN(parts[1], ".", 2)
	var domain string
	if len(domainParts) == 2 {
		domain = "****." + domainParts[1]
	} else {
		domain = "****"
	}
	masked := "****"
	if len(name) > 3 {
		masked = string(name[0]) + strings.Repeat("*", len(name)-1)
	}
	return masked + "@" + domain
}
