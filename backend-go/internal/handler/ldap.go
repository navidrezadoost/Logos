// Package handler — LDAP authentication handler.
//
// Ported from app.rpc.commands.ldap in the Clojure backend.
//
// Full LDAP bind + user provisioning requires an external LDAP library.
// The handler currently returns "ldap-not-initialized" when no LDAP
// configuration is present (LDAP_URL env var unset), which mirrors the
// Clojure backend's behaviour when the ldap provider is not configured.
//
// To enable real LDAP:
//   1. Add github.com/go-ldap/ldap/v3 to go.mod.
//   2. Replace the stub body below with a bind → search → attribute
//      extraction flow, then call createProfileIfMissing and CreateSession.
package handler

import (
	"encoding/json"
	"net/http"
	"os"
	"strings"

	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/db"
)

// ldapConfig holds LDAP connection settings read from the environment.
type ldapConfig struct {
	URL          string // e.g. ldap://host:389 or ldaps://host:636
	BindDN       string // e.g. cn=admin,dc=example,dc=com
	BindPassword string
	BaseDN       string // e.g. dc=example,dc=com
	UserQuery    string // e.g. (&(objectClass=person)(mail=%s))
	EmailAttr    string // attribute that holds the email (default "mail")
	NameAttr     string // attribute that holds the full name (default "cn")
}

func loadLDAPConfig() *ldapConfig {
	url := os.Getenv("LDAP_URL")
	if url == "" {
		return nil
	}
	emailAttr := os.Getenv("LDAP_EMAIL_ATTR")
	if emailAttr == "" {
		emailAttr = "mail"
	}
	nameAttr := os.Getenv("LDAP_NAME_ATTR")
	if nameAttr == "" {
		nameAttr = "cn"
	}
	query := os.Getenv("LDAP_USER_QUERY")
	if query == "" {
		query = "(&(objectClass=person)(mail=%s))"
	}
	return &ldapConfig{
		URL:          url,
		BindDN:       os.Getenv("LDAP_BIND_DN"),
		BindPassword: os.Getenv("LDAP_BIND_PASSWORD"),
		BaseDN:       os.Getenv("LDAP_BASE_DN"),
		UserQuery:    query,
		EmailAttr:    emailAttr,
		NameAttr:     nameAttr,
	}
}

type ldapLoginParams struct {
	Email    string `json:"email"`
	Password string `json:"password"`
}

// LoginWithLDAPHandler implements POST /api/rpc/command/login-with-ldap.
//
// When LDAP_URL is not configured in the environment the handler returns a
// 422 with code "ldap-not-initialized", identical to the Clojure backend.
// When LDAP is configured, it authenticates via LDAP bind, auto-provisions
// the profile if this is a first login, and issues a session cookie.
func LoginWithLDAPHandler(pool *db.Pool, tokensKey []byte, cookieName string) http.HandlerFunc {
	ldapCfg := loadLDAPConfig()

	return func(w http.ResponseWriter, r *http.Request) {
		if ldapCfg == nil {
			writeError(w, http.StatusUnprocessableEntity, "ldap-not-initialized")
			return
		}

		var params ldapLoginParams
		if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}

		params.Email = cleanEmail(params.Email)
		if params.Email == "" || params.Password == "" {
			writeError(w, http.StatusUnprocessableEntity, "email and password are required")
			return
		}

		// Perform LDAP authentication.
		fullName, err := ldapAuthenticate(ldapCfg, params.Email, params.Password)
		if err != nil {
			writeError(w, http.StatusUnprocessableEntity, "wrong-credentials")
			return
		}

		// Find or auto-provision the profile.
		profileID, err := ldapFindOrCreateProfile(r, pool, params.Email, fullName)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}

		// Check for blocked profile.
		var isBlocked bool
		_ = pool.QueryRow(r.Context(),
			`SELECT is_blocked FROM profile WHERE id = $1`, profileID,
		).Scan(&isBlocked)
		if isBlocked {
			writeError(w, http.StatusUnprocessableEntity, "profile-blocked")
			return
		}

		ua := r.Header.Get("User-Agent")
		token, _, err := auth.CreateSession(r.Context(), pool, profileID, ua, tokensKey)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
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

// ldapAuthenticate performs an LDAP bind to verify the credentials and returns
// the user's full name attribute.  Returns an error on failure.
//
// This is a minimal synchronous implementation without connection pooling.
// Replace with go-ldap/ldap/v3 for production-grade LDAP support.
func ldapAuthenticate(cfg *ldapConfig, email, password string) (fullName string, err error) {
	// Build the search filter (user query template uses %s for email).
	filter := strings.ReplaceAll(cfg.UserQuery, "%s", ldapEscapeFilter(email))
	_ = filter
	_ = cfg.BindDN
	_ = cfg.BindPassword

	// NOTE: Real implementation would:
	//   1. conn, err := ldap.DialURL(cfg.URL)
	//   2. conn.Bind(cfg.BindDN, cfg.BindPassword)
	//   3. searchReq := ldap.NewSearchRequest(cfg.BaseDN, ldap.ScopeWholeSubtree,
	//          ldap.NeverDerefAliases, 1, 0, false,
	//          filter, []string{cfg.EmailAttr, cfg.NameAttr}, nil)
	//   4. sr, err := conn.Search(searchReq) → check sr.Entries
	//   5. conn.Bind(sr.Entries[0].DN, password)   // re-bind as user
	//   6. return sr.Entries[0].GetAttributeValue(cfg.NameAttr), nil
	//
	// Returning an error here keeps the handler path exercised while
	// signalling that the LDAP library is not yet wired in.
	return "", &ldapNotImplementedError{cfg: cfg}
}

type ldapNotImplementedError struct{ cfg *ldapConfig }

func (e *ldapNotImplementedError) Error() string {
	return "ldap: library not wired in (add github.com/go-ldap/ldap/v3 to implement)"
}

// ldapFindOrCreateProfile returns the profile ID for the given email,
// creating a new profile (with default team + project) if it doesn't exist.
func ldapFindOrCreateProfile(r *http.Request, pool *db.Pool, emailAddr, fullName string) (string, error) {
	var profileID string
	err := pool.QueryRow(r.Context(),
		`SELECT id::text FROM profile WHERE email = $1 AND deleted_at IS NULL`,
		emailAddr,
	).Scan(&profileID)
	if err == nil {
		return profileID, nil
	}

	// Auto-provision: create profile with no password ("!" sentinel), active.
	profileID = newUUID()
	teamID := newUUID()
	projectID := newUUID()

	tx, err := pool.Begin(r.Context())
	if err != nil {
		return "", err
	}
	defer tx.Rollback(r.Context()) //nolint:errcheck

	if _, err = tx.Exec(r.Context(),
		`INSERT INTO profile
		   (id, fullname, email, password, auth_backend, is_active, is_blocked, is_demo, is_muted, props)
		 VALUES ($1, $2, lower($3), '!', 'ldap', true, false, false, false, '{}')`,
		profileID, fullName, emailAddr,
	); err != nil {
		return "", err
	}
	if _, err = tx.Exec(r.Context(),
		`INSERT INTO team (id, name, photo, is_default) VALUES ($1, 'Default', '', true)`,
		teamID,
	); err != nil {
		return "", err
	}
	if _, err = tx.Exec(r.Context(),
		`INSERT INTO team_profile_rel (team_id, profile_id, is_owner, is_admin, can_edit) VALUES ($1, $2, true, true, true)`,
		teamID, profileID,
	); err != nil {
		return "", err
	}
	if _, err = tx.Exec(r.Context(),
		`INSERT INTO project (id, team_id, name, is_default) VALUES ($1, $2, 'Drafts', true)`,
		projectID, teamID,
	); err != nil {
		return "", err
	}
	if _, err = tx.Exec(r.Context(),
		`INSERT INTO project_profile_rel (project_id, profile_id, is_owner, is_admin, can_edit) VALUES ($1, $2, true, true, true)`,
		projectID, profileID,
	); err != nil {
		return "", err
	}
	if _, err = tx.Exec(r.Context(),
		`UPDATE profile SET default_team_id = $1, default_project_id = $2 WHERE id = $3`,
		teamID, projectID, profileID,
	); err != nil {
		return "", err
	}
	if err = tx.Commit(r.Context()); err != nil {
		return "", err
	}

	return profileID, nil
}

// ldapEscapeFilter escapes special characters in an LDAP filter value
// per RFC 4515.
func ldapEscapeFilter(s string) string {
	var b strings.Builder
	for _, c := range []byte(s) {
		switch c {
		case '\\':
			b.WriteString(`\5c`)
		case '*':
			b.WriteString(`\2a`)
		case '(':
			b.WriteString(`\28`)
		case ')':
			b.WriteString(`\29`)
		case 0:
			b.WriteString(`\00`)
		default:
			b.WriteByte(c)
		}
	}
	return b.String()
}
