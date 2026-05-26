// Package handler — authentication handlers.
//
// Ported from app.rpc.commands.auth in the Clojure backend.
// Handlers: login-with-password, logout, prepare-register-profile,
//           register-profile, request-profile-recovery, recover-profile,
//           get-sso-provider.
//
// Password hashing: Argon2id (buddy-hashers compatible) via internal/auth/password.go.
// Session tokens:   JWE + Transit JSON via internal/auth/issue.go.
package handler

import (
	"encoding/json"
	"net/http"
	"os"
	"strings"
	"time"

	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/db"
	"github.com/logos-design/logos/backend-go/internal/email"
)

// ─── login-with-password ─────────────────────────────────────────────────────

type loginParams struct {
	Email    string `json:"email"`
	Password string `json:"password"`
}

// LoginHandler implements POST /api/rpc/command/login-with-password.
//
// Verifies the Argon2id password, creates an http_session_v2 row, and sets
// the auth-token cookie.  Returns the public profile JSON on success.
func LoginHandler(pool *db.Pool, tokensKey []byte, cookieName string) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		var params loginParams
		if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}

		params.Email = cleanEmail(params.Email)
		if params.Email == "" || params.Password == "" {
			writeError(w, http.StatusUnprocessableEntity, "email and password are required")
			return
		}

		// Load profile by email.
		type row struct {
			ID        string
			Password  string
			IsActive  bool
			IsBlocked bool
			DeletedAt *time.Time
		}

		var p row
		err := pool.QueryRow(r.Context(),
			`SELECT id::text, password, is_active, is_blocked, deleted_at
			   FROM profile
			  WHERE email = lower($1) AND deleted_at IS NULL`,
			params.Email,
		).Scan(&p.ID, &p.Password, &p.IsActive, &p.IsBlocked, &p.DeletedAt)
		if err != nil {
			// Return the same generic error for not-found and any other DB error
			// to avoid leaking account existence.
			writeError(w, http.StatusUnprocessableEntity, "wrong-credentials")
			return
		}

		if !p.IsActive {
			writeError(w, http.StatusUnprocessableEntity, "wrong-credentials")
			return
		}
		if p.IsBlocked {
			writeError(w, http.StatusUnprocessableEntity, "profile-blocked")
			return
		}
		if p.Password == "!" {
			writeError(w, http.StatusUnprocessableEntity, "account-without-password")
			return
		}

		valid, needsRehash := auth.VerifyPassword(params.Password, p.Password)
		if !valid {
			writeError(w, http.StatusUnprocessableEntity, "wrong-credentials")
			return
		}

		// Silently rehash if parameters are outdated.
		if needsRehash {
			if newHash, err := auth.DerivePassword(params.Password); err == nil {
				_, _ = pool.Exec(r.Context(),
					`UPDATE profile SET password = $1 WHERE id = $2`,
					newHash, p.ID)
			}
		}

		ua := r.Header.Get("User-Agent")
		token, _, err := auth.CreateSession(r.Context(), pool, p.ID, ua, tokensKey)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}

		auth.SetSessionCookie(w, cookieName, token)

		profile, err := fetchProfile(r.Context(), pool, p.ID)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}

		writeJSON(w, http.StatusOK, profile)
	}
}

// ─── logout ──────────────────────────────────────────────────────────────────

// LogoutHandler implements POST /api/rpc/command/logout.
//
// Deletes the current session row and clears the auth-token cookie.
func LogoutHandler(pool *db.Pool, cookieName string) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		sessionID := auth.SessionID(r.Context())
		if sessionID != "" {
			_ = auth.DeleteSession(r.Context(), pool, sessionID)
		}
		auth.ClearSessionCookie(w, cookieName)
		writeJSON(w, http.StatusOK, map[string]any{})
	}
}

// ─── prepare-register-profile ────────────────────────────────────────────────

type prepareRegisterParams struct {
	FullName string `json:"fullname"`
	Email    string `json:"email"`
	Password string `json:"password"`
}

// PrepareRegisterHandler implements POST /api/rpc/command/prepare-register-profile.
//
// Validates the registration attempt and returns a short-lived preparation token
// (JWE) that register-profile consumes.  The plaintext password travels inside
// the encrypted token so it can be hashed during the actual registration step.
func PrepareRegisterHandler(pool *db.Pool, tokensKey []byte) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		var params prepareRegisterParams
		if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}

		params.Email = cleanEmail(params.Email)
		if params.Email == "" || params.FullName == "" || params.Password == "" {
			writeError(w, http.StatusUnprocessableEntity, "fullname, email and password are required")
			return
		}
		if strings.EqualFold(params.Email, params.Password) {
			writeError(w, http.StatusUnprocessableEntity, "email-as-password")
			return
		}

		// Resolve existing profile-id if any (for duplicate-registration detection).
		var existingID *string
		var dummy string
		err := pool.QueryRow(r.Context(),
			`SELECT id::text FROM profile WHERE email = $1 AND deleted_at IS NULL`,
			params.Email,
		).Scan(&dummy)
		if err == nil {
			existingID = &dummy
		}

		claims := auth.TokenClaims{
			Iss:      "prepared-register",
			Email:    params.Email,
			FullName: params.FullName,
			Password: params.Password,
			Backend:  "penpot",
			Exp:      time.Now().Add(7 * 24 * time.Hour),
		}
		if existingID != nil {
			claims.ProfileID = *existingID
		}

		token, err := auth.EncryptToken(claims, tokensKey)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}

		writeJSON(w, http.StatusOK, map[string]string{"token": token})
	}
}

// ─── register-profile ────────────────────────────────────────────────────────

type registerProfileParams struct {
	Token string `json:"token"`
}

// RegisterProfileHandler implements POST /api/rpc/command/register-profile.
//
// Consumes the preparation token produced by prepare-register-profile,
// creates the profile, default team and project in a single transaction,
// then creates a session and returns the profile.
func RegisterProfileHandler(pool *db.Pool, tokensKey []byte, cookieName string) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		var params registerProfileParams
		if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}

		claims, err := auth.DecryptTokenClaims(params.Token, tokensKey)
		if err != nil || claims.Iss != "prepared-register" {
			writeError(w, http.StatusUnprocessableEntity, "invalid-token")
			return
		}
		if !claims.Exp.IsZero() && time.Now().After(claims.Exp) {
			writeError(w, http.StatusUnprocessableEntity, "token-expired")
			return
		}

		// Check for duplicate email.
		var existingID string
		if err := pool.QueryRow(r.Context(),
			`SELECT id::text FROM profile WHERE email = $1 AND deleted_at IS NULL`,
			claims.Email,
		).Scan(&existingID); err == nil {
			// Email already registered.
			writeError(w, http.StatusUnprocessableEntity, "email-already-exists")
			return
		}

		// Hash the password that was stored plaintext in the preparation token.
		hashedPwd, err := auth.DerivePassword(claims.Password)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}

		// Create profile + default team + project in a single transaction.
		tx, err := pool.Begin(r.Context())
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		defer tx.Rollback(r.Context()) //nolint:errcheck

		profileID := newUUID()
		teamID := newUUID()
		projectID := newUUID()

		// Insert profile.
		if _, err = tx.Exec(r.Context(),
			`INSERT INTO profile
			   (id, fullname, email, password, auth_backend, is_active, is_blocked, is_demo, is_muted, props)
			 VALUES ($1, $2, lower($3), $4, 'penpot', true, false, false, false, '{}')`,
			profileID, claims.FullName, claims.Email, hashedPwd,
		); err != nil {
			writeError(w, http.StatusInternalServerError, "profile insert failed")
			return
		}

		// Insert default team.
		if _, err = tx.Exec(r.Context(),
			`INSERT INTO team (id, name, photo, is_default) VALUES ($1, 'Default', '', true)`,
			teamID,
		); err != nil {
			writeError(w, http.StatusInternalServerError, "team insert failed")
			return
		}

		// Owner role on team.
		if _, err = tx.Exec(r.Context(),
			`INSERT INTO team_profile_rel (team_id, profile_id, is_owner, is_admin, can_edit)
			 VALUES ($1, $2, true, true, true)`,
			teamID, profileID,
		); err != nil {
			writeError(w, http.StatusInternalServerError, "team role failed")
			return
		}

		// Default project "Drafts".
		if _, err = tx.Exec(r.Context(),
			`INSERT INTO project (id, team_id, name, is_default) VALUES ($1, $2, 'Drafts', true)`,
			projectID, teamID,
		); err != nil {
			writeError(w, http.StatusInternalServerError, "project insert failed")
			return
		}

		// Owner role on project.
		if _, err = tx.Exec(r.Context(),
			`INSERT INTO project_profile_rel (project_id, profile_id, is_owner, is_admin, can_edit)
			 VALUES ($1, $2, true, true, true)`,
			projectID, profileID,
		); err != nil {
			writeError(w, http.StatusInternalServerError, "project role failed")
			return
		}

		// Update profile with default IDs.
		if _, err = tx.Exec(r.Context(),
			`UPDATE profile
			    SET default_team_id = $1, default_project_id = $2, modified_at = now()
			  WHERE id = $3`,
			teamID, projectID, profileID,
		); err != nil {
			writeError(w, http.StatusInternalServerError, "profile update failed")
			return
		}

		if err = tx.Commit(r.Context()); err != nil {
			writeError(w, http.StatusInternalServerError, "commit failed")
			return
		}

		// Create session.
		ua := r.Header.Get("User-Agent")
		token, _, err := auth.CreateSession(r.Context(), pool, profileID, ua, tokensKey)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "session create failed")
			return
		}

		auth.SetSessionCookie(w, cookieName, token)

		profile, err := fetchProfile(r.Context(), pool, profileID)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}

		writeJSON(w, http.StatusOK, profile)
	}
}

// ─── request-profile-recovery ────────────────────────────────────────────────

type requestRecoveryParams struct {
	Email string `json:"email"`
}

// RequestProfileRecoveryHandler implements POST /api/rpc/command/request-profile-recovery.
//
// Generates a short-lived password-recovery token (15 min) and "sends" it via
// the email stub (logged to stdout).  Always returns 200 regardless of whether
// the email exists, to prevent account enumeration.
func RequestProfileRecoveryHandler(pool *db.Pool, tokensKey []byte) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		var params requestRecoveryParams
		if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}

		params.Email = cleanEmail(params.Email)

		type profileRow struct {
			ID       string
			FullName string
		}

		var p profileRow
		err := pool.QueryRow(r.Context(),
			`SELECT id::text, fullname FROM profile
			  WHERE email = $1 AND deleted_at IS NULL AND is_active = true`,
			params.Email,
		).Scan(&p.ID, &p.FullName)
		if err != nil {
			// Silently succeed for non-existent / inactive profiles.
			writeJSON(w, http.StatusOK, map[string]any{})
			return
		}

		// Touch modified_at so the retry threshold can be calculated.
		_, _ = pool.Exec(r.Context(),
			`UPDATE profile SET modified_at = now() WHERE id = $1`, p.ID)

		claims := auth.TokenClaims{
			Iss:       "password-recovery",
			ProfileID: p.ID,
			Exp:       time.Now().Add(15 * time.Minute),
		}
		token, err := auth.EncryptToken(claims, tokensKey)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}

		email.Send(email.Message{
			Kind:      email.KindPasswordRecovery,
			To:        params.Email,
			Name:      p.FullName,
			Token:     token,
			PublicURI: publicURI(),
		})

		writeJSON(w, http.StatusOK, map[string]any{})
	}
}

// ─── recover-profile ─────────────────────────────────────────────────────────

type recoverProfileParams struct {
	Token    string `json:"token"`
	Password string `json:"password"`
}

// RecoverProfileHandler implements POST /api/rpc/command/recover-profile.
//
// Verifies the password-recovery token, hashes the new password, and updates
// the profile.  Also sets is_active=true (consistent with Clojure behaviour).
func RecoverProfileHandler(pool *db.Pool, tokensKey []byte) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		var params recoverProfileParams
		if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}
		if params.Token == "" || params.Password == "" {
			writeError(w, http.StatusUnprocessableEntity, "token and password are required")
			return
		}

		claims, err := auth.DecryptTokenClaims(params.Token, tokensKey)
		if err != nil || claims.Iss != "password-recovery" || claims.ProfileID == "" {
			writeError(w, http.StatusUnprocessableEntity, "invalid-token")
			return
		}
		if !claims.Exp.IsZero() && time.Now().After(claims.Exp) {
			writeError(w, http.StatusUnprocessableEntity, "token-expired")
			return
		}

		newHash, err := auth.DerivePassword(params.Password)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}

		if _, err = pool.Exec(r.Context(),
			`UPDATE profile SET password = $1, is_active = true, modified_at = now()
			  WHERE id = $2`,
			newHash, claims.ProfileID,
		); err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}

		writeJSON(w, http.StatusOK, map[string]any{})
	}
}

// ─── get-sso-provider ────────────────────────────────────────────────────────

type getSSOProviderParams struct {
	Email string `json:"email"`
}

// GetSSOProviderHandler implements POST /api/rpc/command/get-sso-provider.
//
// Looks up the SSO provider for the email's domain.  Returns {id: <uuid>} if
// found, or an empty JSON object when no SSO provider matches.
func GetSSOProviderHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		var params getSSOProviderParams
		if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}

		domain := extractDomain(params.Email)
		if domain == "" {
			writeJSON(w, http.StatusOK, map[string]any{})
			return
		}

		var id string
		err := pool.QueryRow(r.Context(),
			`SELECT id::text FROM sso_provider WHERE domain = $1 LIMIT 1`,
			domain,
		).Scan(&id)
		if err != nil {
			writeJSON(w, http.StatusOK, map[string]any{})
			return
		}

		writeJSON(w, http.StatusOK, map[string]string{"id": id})
	}
}

// ─── helpers ─────────────────────────────────────────────────────────────────

// extractDomain returns the lowercase domain part of an email address.
func extractDomain(emailAddr string) string {
	at := strings.LastIndex(emailAddr, "@")
	if at < 0 || at == len(emailAddr)-1 {
		return ""
	}
	return strings.ToLower(strings.TrimSpace(emailAddr[at+1:]))
}

// publicURI returns the configured public URI for building email links.
// Falls back to http://localhost:3449 (the default Penpot development URL).
func publicURI() string {
	if v := os.Getenv("PUBLIC_URI"); v != "" {
		return v
	}
	return "http://localhost:3449"
}
