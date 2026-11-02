// Package handler — access token (API key) handlers.
//
// Ported from app.rpc.commands.access-token in the Clojure backend.
// Covers: create-access-token, delete-access-token, get-access-tokens.
//
// Access tokens are JWE compact tokens (same format as session tokens) stored
// in the access_token table.  The token payload carries:
//   {iss: "access-token", uid: <profile-uuid>, iat: <created-at>, tid: <token-uuid>}
//
// On future bearer-token auth support, the session middleware should also
// accept an "Authorization: Bearer <access-token>" header and validate it
// against the access_token table (not yet implemented).
package handler

import (
	"encoding/json"
	"net/http"
	"time"

	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/db"
)

// AccessToken is the public view of an access_token row (no token secret).
type AccessToken struct {
	ID        string     `json:"id"`
	Name      string     `json:"name"`
	CreatedAt time.Time  `json:"createdAt"`
	UpdatedAt time.Time  `json:"updatedAt"`
	ExpiresAt *time.Time `json:"expiresAt,omitempty"`
}

// ─── POST /api/rpc/command/create-access-token ───────────────────────────────

type createAccessTokenParams struct {
	Name       string `json:"name"`
	Expiration string `json:"expiration"` // optional duration string, e.g. "30d", "1y"
}

// CreateAccessTokenHandler implements POST /api/rpc/command/create-access-token.
//
// Creates a JWE access token, stores it in the access_token table, and
// returns the token string along with the row metadata (one-time reveal).
func CreateAccessTokenHandler(pool *db.Pool, tokensKey []byte) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
			return
		}

		var params createAccessTokenParams
		if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}
		if params.Name == "" {
			writeError(w, http.StatusUnprocessableEntity, "name is required")
			return
		}

		tokenID := newUUID()
		now := time.Now().UTC()

		var expiresAt *time.Time
		if params.Expiration != "" {
			if d, err := parseDuration(params.Expiration); err == nil {
				t := now.Add(d)
				expiresAt = &t
			}
		}

		claims := auth.TokenClaims{
			Iss: "access-token",
			Uid: profileID,
			Tid: tokenID,
			Iat: now,
		}
		tokenStr, err := auth.EncryptToken(claims, tokensKey)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "token generation failed")
			return
		}

		_, err = pool.Exec(r.Context(),
			`INSERT INTO access_token
			   (id, name, token, profile_id, created_at, updated_at, expires_at, perms)
			 VALUES ($1, $2, $3, $4, $5, $5, $6, '{}')`,
			tokenID, params.Name, tokenStr, profileID, now, expiresAt,
		)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "token insert failed")
			return
		}

		writeJSON(w, http.StatusOK, map[string]any{
			"id":        tokenID,
			"name":      params.Name,
			"token":     tokenStr,
			"createdAt": now,
			"expiresAt": expiresAt,
		})
	}
}

// ─── DELETE /api/rpc/command/delete-access-token ─────────────────────────────

type deleteAccessTokenParams struct {
	ID string `json:"id"`
}

// DeleteAccessTokenHandler implements DELETE /api/rpc/command/delete-access-token.
//
// Deletes the token identified by id, scoped to the authenticated profile.
func DeleteAccessTokenHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
			return
		}

		var params deleteAccessTokenParams
		if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}
		if params.ID == "" {
			writeError(w, http.StatusUnprocessableEntity, "id is required")
			return
		}

		_, err := pool.Exec(r.Context(),
			`DELETE FROM access_token WHERE id = $1 AND profile_id = $2`,
			params.ID, profileID,
		)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}

		writeJSON(w, http.StatusOK, map[string]any{})
	}
}

// ─── GET /api/rpc/command/get-access-tokens ──────────────────────────────────

// GetAccessTokensHandler implements GET /api/rpc/command/get-access-tokens.
//
// Lists all access tokens for the authenticated profile, ordered by expiry
// then creation time (matching Clojure's [:expires-at :asc] [:created-at :asc]).
func GetAccessTokensHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
			return
		}

		rows, err := pool.Query(r.Context(),
			`SELECT id::text, name, created_at, updated_at, expires_at
			   FROM access_token
			  WHERE profile_id = $1
			  ORDER BY expires_at ASC NULLS LAST, created_at ASC`,
			profileID,
		)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		defer rows.Close()

		tokens := make([]AccessToken, 0)
		for rows.Next() {
			var t AccessToken
			if err := rows.Scan(&t.ID, &t.Name, &t.CreatedAt, &t.UpdatedAt, &t.ExpiresAt); err != nil {
				continue
			}
			tokens = append(tokens, t)
		}

		writeJSON(w, http.StatusOK, tokens)
	}
}

// ─── duration parsing ────────────────────────────────────────────────────────

// parseDuration parses a duration string.
// Accepts Go's standard format ("72h", "30m") and simple unit suffixes:
// "<n>d" for days, "<n>y" for years (365 days).
func parseDuration(s string) (time.Duration, error) {
	if s == "" {
		return 0, &invalidDurationError{s}
	}

	// Try Go's built-in parser first (handles "72h", "30m", "1h30m", etc.).
	if d, err := time.ParseDuration(s); err == nil {
		return d, nil
	}

	// Parse "<digits><suffix>" for d/y which Go doesn't support.
	if len(s) < 2 {
		return 0, &invalidDurationError{s}
	}
	suffix := s[len(s)-1]
	var count int64
	for _, c := range s[:len(s)-1] {
		if c < '0' || c > '9' {
			return 0, &invalidDurationError{s}
		}
		count = count*10 + int64(c-'0')
	}
	switch suffix {
	case 'd':
		return time.Duration(count) * 24 * time.Hour, nil
	case 'y':
		return time.Duration(count) * 365 * 24 * time.Hour, nil
	}
	return 0, &invalidDurationError{s}
}

type invalidDurationError struct{ s string }

func (e *invalidDurationError) Error() string {
	return "invalid duration: " + e.s
}
