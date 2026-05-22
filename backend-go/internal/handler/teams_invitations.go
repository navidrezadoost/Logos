// Package handler — teams invitations handlers.
//
// Ported from app.rpc.commands.teams-invitations in the Clojure backend.
// Covers: create-team-invitations, update-team-invitation-role,
//         delete-team-invitation, get-team-invitations (list via teams.go).
//
// Email delivery is intentionally stubbed — the invitation record is upserted
// in the database and the token fields are left empty in this implementation.
// A separate email-service package will handle delivery when integrated.
package handler

import (
	"encoding/json"
	"net/http"
	"strings"
	"time"

	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/db"
	"github.com/logos-design/logos/backend-go/internal/perms"
)

// invitationTTL is 7 days, matching the Clojure backend ("168h").
const invitationTTL = 7 * 24 * time.Hour

// ─── POST /api/rpc/command/create-team-invitations ───────────────────────────

type invitationEntry struct {
	Email string `json:"email"`
	Role  string `json:"role"` // "owner" | "admin" | "editor" | "viewer"
}

type createTeamInvitationsParams struct {
	TeamID      string             `json:"teamId"`
	Emails      []string           `json:"emails,omitempty"`      // format 1: emails + role
	Role        string             `json:"role,omitempty"`        // format 1:
	Invitations []*invitationEntry `json:"invitations,omitempty"` // format 2
}

// CreateTeamInvitationsHandler upserts invitation rows and skips already-member
// addresses. Email delivery is a no-op stub.
func CreateTeamInvitationsHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeError(w, http.StatusUnauthorized, "unauthorized")
			return
		}

		var params createTeamInvitationsParams
		if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}

		// Resolve invitation list from either format.
		var entries []invitationEntry
		if len(params.Emails) > 0 && params.Role != "" {
			for _, e := range params.Emails {
				entries = append(entries, invitationEntry{Email: cleanEmail(e), Role: params.Role})
			}
		} else if len(params.Invitations) > 0 {
			for _, inv := range params.Invitations {
				entries = append(entries, invitationEntry{Email: cleanEmail(inv.Email), Role: inv.Role})
			}
		} else {
			writeError(w, http.StatusBadRequest, "emails+role or invitations required")
			return
		}

		p, err := perms.GetTeamPermissions(r.Context(), pool, profileID, params.TeamID)
		if err != nil || p == nil || !p.IsAdmin {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		if len(entries) > 25 {
			writeError(w, http.StatusUnprocessableEntity, "max-invitations-by-request")
			return
		}

		// Fetch existing team members to skip re-invitation.
		existingRows, _ := pool.Query(r.Context(),
			`SELECT p.email FROM team_profile_rel AS tpr
			   JOIN profile AS p ON (p.id = tpr.profile_id)
			  WHERE tpr.team_id = $1`, params.TeamID)
		members := map[string]bool{}
		if existingRows != nil {
			for existingRows.Next() {
				var em string
				if existingRows.Scan(&em) == nil {
					members[strings.ToLower(em)] = true
				}
			}
			existingRows.Close()
		}

		validUntil := time.Now().Add(invitationTTL)
		created := 0
		for _, entry := range entries {
			if members[strings.ToLower(entry.Email)] {
				continue // already a member
			}
			if _, err := pool.Exec(r.Context(), `
				INSERT INTO team_invitation (team_id, email_to, role, valid_until, created_by)
				VALUES ($1, $2, $3, $4, $5)
				ON CONFLICT (team_id, email_to)
				DO UPDATE SET role = $3, valid_until = $4, updated_at = now()`,
				params.TeamID, strings.ToLower(entry.Email), entry.Role, validUntil, profileID,
			); err == nil {
				created++
			}
		}

		writeJSON(w, http.StatusOK, map[string]int{"total": created})
	}
}

// cleanEmail trims and lowercases an email address (mirrors Clojure's clean-email).
func cleanEmail(email string) string {
	return strings.ToLower(strings.TrimSpace(email))
}

// ─── PATCH /api/rpc/command/update-team-invitation-role ──────────────────────

// UpdateTeamInvitationRoleHandler updates the role on an existing invitation.
func UpdateTeamInvitationRoleHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeError(w, http.StatusUnauthorized, "unauthorized")
			return
		}

		var body struct {
			TeamID string `json:"teamId"`
			Email  string `json:"email"`
			Role   string `json:"role"`
		}
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}

		p, err := perms.GetTeamPermissions(r.Context(), pool, profileID, body.TeamID)
		if err != nil || p == nil || !p.IsAdmin {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		if _, err := pool.Exec(r.Context(),
			`UPDATE team_invitation SET role = $1, updated_at = now()
			  WHERE team_id = $2 AND email_to = $3`,
			body.Role, body.TeamID, cleanEmail(body.Email)); err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		w.WriteHeader(http.StatusNoContent)
	}
}

// ─── DELETE /api/rpc/command/delete-team-invitation ──────────────────────────

// DeleteTeamInvitationHandler removes a pending invitation (admin+).
func DeleteTeamInvitationHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeError(w, http.StatusUnauthorized, "unauthorized")
			return
		}

		var body struct {
			TeamID string `json:"teamId"`
			Email  string `json:"email"`
		}
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}

		p, err := perms.GetTeamPermissions(r.Context(), pool, profileID, body.TeamID)
		if err != nil || p == nil || !p.IsAdmin {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		if _, err := pool.Exec(r.Context(),
			`DELETE FROM team_invitation WHERE team_id = $1 AND email_to = $2`,
			body.TeamID, cleanEmail(body.Email)); err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		w.WriteHeader(http.StatusNoContent)
	}
}

// Suppress unused import warning — db is used via pool parameter type.
var _ *db.Pool
