// Package handler — generic token verification handler.
//
// Ported from app.rpc.commands.verify-token in the Clojure backend.
// Handler: verify-token.
//
// Decrypts the JWE token, checks expiry, then dispatches on the iss claim:
//   • "verify-email"      — activate the profile and create a session
//   • "change-email"      — update the profile email
//   • "team-invitation"   — accept a team invitation
//   • "auth"              — return the claims + profile (no side effects)
//   • anything else       — return 422 invalid-token
package handler

import (
	"encoding/json"
	"net/http"
	"time"

	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/db"
)

type verifyTokenParams struct {
	Token string `json:"token"`
}

// VerifyTokenHandler implements POST /api/rpc/command/verify-token.
//
// All token types share this single endpoint; the handler dispatches based
// on the decrypted iss claim.
func VerifyTokenHandler(pool *db.Pool, tokensKey []byte, cookieName string) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		var params verifyTokenParams
		if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}
		if params.Token == "" {
			writeError(w, http.StatusUnprocessableEntity, "token is required")
			return
		}

		claims, err := auth.DecryptTokenClaims(params.Token, tokensKey)
		if err != nil {
			writeError(w, http.StatusUnprocessableEntity, "invalid-token")
			return
		}

		// Reject expired tokens.
		if !claims.Exp.IsZero() && time.Now().After(claims.Exp) {
			writeError(w, http.StatusUnprocessableEntity, "token-expired")
			return
		}

		switch claims.Iss {
		case "verify-email":
			processVerifyEmail(w, r, pool, tokensKey, cookieName, claims)
		case "change-email":
			processChangeEmail(w, r, pool, claims)
		case "team-invitation":
			processTeamInvitation(w, r, pool, tokensKey, cookieName, claims)
		case "auth":
			processAuthToken(w, r, pool, claims)
		default:
			writeError(w, http.StatusUnprocessableEntity, "invalid-token")
		}
	}
}

// ─── verify-email ─────────────────────────────────────────────────────────────

func processVerifyEmail(
	w http.ResponseWriter, r *http.Request,
	pool *db.Pool, tokensKey []byte, cookieName string,
	claims auth.TokenClaims,
) {
	if claims.ProfileID == "" {
		writeError(w, http.StatusUnprocessableEntity, "invalid-token")
		return
	}

	// Activate the profile if not already active.
	var isActive bool
	var currentEmail string
	err := pool.QueryRow(r.Context(),
		`SELECT is_active, email FROM profile WHERE id = $1 AND deleted_at IS NULL`,
		claims.ProfileID,
	).Scan(&isActive, &currentEmail)
	if err != nil {
		writeError(w, http.StatusUnprocessableEntity, "invalid-token")
		return
	}

	if !isActive {
		// Verify the email in the token matches the profile email.
		if claims.Email != "" && cleanEmail(claims.Email) != currentEmail {
			writeError(w, http.StatusUnprocessableEntity, "invalid-token")
			return
		}
		if _, err = pool.Exec(r.Context(),
			`UPDATE profile SET is_active = true, modified_at = now() WHERE id = $1`,
			claims.ProfileID,
		); err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
	}

	// Create a session.
	ua := r.Header.Get("User-Agent")
	token, _, err := auth.CreateSession(r.Context(), pool, claims.ProfileID, ua, tokensKey)
	if err != nil {
		writeError(w, http.StatusInternalServerError, "internal server error")
		return
	}

	auth.SetSessionCookie(w, cookieName, token)

	writeJSON(w, http.StatusOK, map[string]any{
		"iss":       claims.Iss,
		"profileId": claims.ProfileID,
	})
}

// ─── change-email ─────────────────────────────────────────────────────────────

func processChangeEmail(
	w http.ResponseWriter, r *http.Request,
	pool *db.Pool,
	claims auth.TokenClaims,
) {
	if claims.ProfileID == "" || claims.Email == "" {
		writeError(w, http.StatusUnprocessableEntity, "invalid-token")
		return
	}

	newEmail := cleanEmail(claims.Email)

	// Reject if email is already taken.
	var collision string
	if err := pool.QueryRow(r.Context(),
		`SELECT id::text FROM profile WHERE email = $1 AND deleted_at IS NULL`,
		newEmail,
	).Scan(&collision); err == nil {
		writeError(w, http.StatusUnprocessableEntity, "email-already-exists")
		return
	}

	if _, err := pool.Exec(r.Context(),
		`UPDATE profile SET email = $1, modified_at = now() WHERE id = $2`,
		newEmail, claims.ProfileID,
	); err != nil {
		writeError(w, http.StatusInternalServerError, "internal server error")
		return
	}

	writeJSON(w, http.StatusOK, map[string]any{
		"iss":       claims.Iss,
		"profileId": claims.ProfileID,
		"email":     newEmail,
	})
}

// ─── team-invitation ──────────────────────────────────────────────────────────

func processTeamInvitation(
	w http.ResponseWriter, r *http.Request,
	pool *db.Pool, tokensKey []byte, cookieName string,
	claims auth.TokenClaims,
) {
	if claims.ProfileID == "" {
		// No logged-in user — redirect to login / register.
		writeJSON(w, http.StatusOK, map[string]any{
			"iss":            claims.Iss,
			"invitationToken": claims.InvitationToken,
			"redirectTo":     "auth-login",
			"state":          "pending",
		})
		return
	}

	// Accept the invitation: upsert team_profile_rel.
	teamID := claims.Sid // team-id is stored in sid for invitation tokens
	if teamID == "" {
		writeError(w, http.StatusUnprocessableEntity, "invalid-token")
		return
	}

	// Determine role from claims (default to editor).
	role := "editor"
	isAdmin := false
	canEdit := true
	if claims.Aud == "admin" {
		role = "admin"
		isAdmin = true
	}
	_ = role

	if _, err := pool.Exec(r.Context(),
		`INSERT INTO team_profile_rel (team_id, profile_id, is_owner, is_admin, can_edit)
		 VALUES ($1, $2, false, $3, $4)
		 ON CONFLICT (team_id, profile_id) DO UPDATE
		   SET is_admin = EXCLUDED.is_admin, can_edit = EXCLUDED.can_edit`,
		teamID, claims.ProfileID, isAdmin, canEdit,
	); err != nil {
		writeError(w, http.StatusInternalServerError, "internal server error")
		return
	}

	// Activate the profile if not already active.
	_, _ = pool.Exec(r.Context(),
		`UPDATE profile SET is_active = true, modified_at = now()
		  WHERE id = $1 AND is_active = false`,
		claims.ProfileID,
	)

	writeJSON(w, http.StatusOK, map[string]any{
		"iss":   claims.Iss,
		"state": "created",
	})
	_ = tokensKey
	_ = cookieName
}

// ─── auth ─────────────────────────────────────────────────────────────────────

func processAuthToken(
	w http.ResponseWriter, r *http.Request,
	pool *db.Pool,
	claims auth.TokenClaims,
) {
	profileID := claims.ProfileID
	if profileID == "" {
		profileID = claims.Uid
	}
	if profileID == "" {
		writeError(w, http.StatusUnprocessableEntity, "invalid-token")
		return
	}

	profile, err := fetchProfile(r.Context(), pool, profileID)
	if err != nil {
		writeError(w, http.StatusUnprocessableEntity, "invalid-token")
		return
	}

	writeJSON(w, http.StatusOK, map[string]any{
		"iss":     claims.Iss,
		"profile": profile,
	})
}
