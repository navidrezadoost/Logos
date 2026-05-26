// Package auth — token issuance and session lifecycle.
//
// Clojure's app.tokens uses Transit JSON payloads inside JWE compact tokens
// (AES-256-KW key wrap, AES-256-GCM content encryption) with a custom header
// {:kid 1 :ver 1}.  Transit JSON encoding rules that matter here:
//   • Keyword keys   → "~:<name>"     (e.g. {:iss ...} → {"~:iss": ...})
//   • Keyword values → "~:<name>"     (e.g. :prepared-register → "~:prepared-register")
//   • UUID values    → "~u<uuid>"     (e.g. #uuid "abc..." → "~uabc...")
//   • Instant values → "~t<ISO8601>"  (e.g. #inst "..." → "~t...")
//   • String values  → plain string   (e.g. "authentication" → "authentication")
//
// The iss field is special: session tokens use a plain string ("authentication")
// while all other token types use a keyword value ("~:prepared-register", etc.).
// DecryptTokenClaims normalises all iss values by stripping the "~:" prefix so
// callers compare against the plain form ("prepared-register", "verify-email", …).
package auth

import (
	"context"
	"crypto/rand"
	"encoding/json"
	"fmt"
	"net/http"
	"strings"
	"time"

	gojose "github.com/go-jose/go-jose/v3"
	"github.com/logos-design/logos/backend-go/internal/db"
)

// TokenClaims holds the decoded fields from any Transit-JSON JWE token.
// UUID-valued fields (Sid, Uid, ProfileID, Tid) are stored as plain UUID strings
// (the "~u" prefix is stripped on decode).
// Iss is stored without the "~:" prefix even when the source was a keyword value.
type TokenClaims struct {
	Iss             string     // "authentication", "prepared-register", "password-recovery", …
	Aud             string
	Sid             string    // session UUID (~:sid)
	Uid             string    // profile UUID (~:uid) — used in session tokens
	ProfileID       string    // profile UUID (~:profile-id) — used in verify/recovery tokens
	Tid             string    // access-token UUID (~:tid)
	Email           string
	FullName        string
	Password        string    // plaintext only in preparation tokens
	Backend         string
	InvitationToken string    // ~:invitation-token
	Iat             time.Time
	Exp             time.Time
	IsActive        *bool
}

// issIsKeyword returns true for token types where Clojure encodes iss as a keyword.
// Session tokens (iss="authentication") use a plain string; everything else is a keyword.
func issIsKeyword(iss string) bool { return iss != "authentication" && iss != "" }

// EncryptToken encodes claims as Transit JSON and wraps them in a compact JWE token.
// Uses AES-256-KW + AES-256-GCM with header {kid:1, ver:1} — matching Clojure.
func EncryptToken(claims TokenClaims, tokensKey []byte) (string, error) {
	payload, err := marshalTransitClaims(claims)
	if err != nil {
		return "", fmt.Errorf("auth: marshal claims: %w", err)
	}

	opts := (&gojose.EncrypterOptions{}).
		WithHeader("ver", 1).
		WithHeader("kid", 1)

	enc, err := gojose.NewEncrypter(
		gojose.A256GCM,
		gojose.Recipient{Algorithm: gojose.A256KW, Key: tokensKey},
		opts,
	)
	if err != nil {
		return "", fmt.Errorf("auth: create encrypter: %w", err)
	}

	jwe, err := enc.Encrypt(payload)
	if err != nil {
		return "", fmt.Errorf("auth: encrypt: %w", err)
	}

	return jwe.CompactSerialize()
}

// DecryptTokenClaims decrypts a compact JWE and returns parsed TokenClaims.
// The Iss field is normalised (no "~:" prefix).
func DecryptTokenClaims(token string, tokensKey []byte) (TokenClaims, error) {
	enc, err := gojose.ParseEncrypted(token)
	if err != nil {
		return TokenClaims{}, fmt.Errorf("auth: parse jwe: %w", err)
	}
	plain, err := enc.Decrypt(tokensKey)
	if err != nil {
		return TokenClaims{}, fmt.Errorf("auth: decrypt jwe: %w", err)
	}
	return unmarshalTransitClaims(plain)
}

// ─── Transit JSON encode / decode ────────────────────────────────────────────

// marshalTransitClaims serialises TokenClaims to Transit JSON.
func marshalTransitClaims(c TokenClaims) ([]byte, error) {
	m := make(map[string]any, 14)

	addStr := func(key, val string) {
		if val != "" {
			m["~:"+key] = val
		}
	}
	addKeyword := func(key, val string) {
		if val != "" {
			m["~:"+key] = "~:" + val
		}
	}
	addUUID := func(key, val string) {
		if val != "" {
			m["~:"+key] = "~u" + val
		}
	}
	addTime := func(key string, val time.Time) {
		if !val.IsZero() {
			// Transit JSON instants: "~t<ISO8601>" with millisecond precision.
			m["~:"+key] = "~t" + val.UTC().Format("2006-01-02T15:04:05.000") + "Z"
		}
	}

	// iss: plain string for "authentication", keyword for everything else.
	if c.Iss != "" {
		if issIsKeyword(c.Iss) {
			addKeyword("iss", c.Iss)
		} else {
			addStr("iss", c.Iss)
		}
	}

	addStr("aud", c.Aud)
	addUUID("sid", c.Sid)
	addUUID("uid", c.Uid)
	addUUID("profile-id", c.ProfileID)
	addUUID("tid", c.Tid)
	addStr("email", c.Email)
	addStr("fullname", c.FullName)
	addStr("password", c.Password)
	addStr("backend", c.Backend)
	addStr("invitation-token", c.InvitationToken)
	addTime("iat", c.Iat)
	addTime("exp", c.Exp)

	if c.IsActive != nil {
		m["~:is-active"] = *c.IsActive
	}

	return json.Marshal(m)
}

// unmarshalTransitClaims deserialises a Transit JSON byte slice into TokenClaims.
func unmarshalTransitClaims(data []byte) (TokenClaims, error) {
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return TokenClaims{}, fmt.Errorf("auth: unmarshal transit: %w", err)
	}

	var c TokenClaims

	stripUUID := func(s string) string {
		return strings.TrimPrefix(s, "~u")
	}
	parseInstant := func(s string) time.Time {
		s = strings.TrimPrefix(s, "~t")
		for _, layout := range []string{
			"2006-01-02T15:04:05.000Z",
			"2006-01-02T15:04:05.000Z07:00",
			time.RFC3339Nano,
			time.RFC3339,
		} {
			if t, err := time.Parse(layout, s); err == nil {
				return t
			}
		}
		return time.Time{}
	}

	for k, v := range raw {
		field := strings.TrimPrefix(k, "~:")

		var s string
		if err := json.Unmarshal(v, &s); err == nil {
			switch field {
			case "iss":
				// Normalise: strip "~:" prefix if it was a keyword value.
				c.Iss = strings.TrimPrefix(s, "~:")
			case "aud":
				c.Aud = s
			case "sid":
				c.Sid = stripUUID(s)
			case "uid":
				c.Uid = stripUUID(s)
			case "profile-id":
				c.ProfileID = stripUUID(s)
			case "tid":
				c.Tid = stripUUID(s)
			case "email":
				c.Email = s
			case "fullname":
				c.FullName = s
			case "password":
				c.Password = s
			case "backend":
				c.Backend = s
			case "invitation-token":
				c.InvitationToken = s
			case "iat":
				c.Iat = parseInstant(s)
			case "exp":
				c.Exp = parseInstant(s)
			}
			continue
		}

		// Non-string values.
		var b bool
		if err := json.Unmarshal(v, &b); err == nil && field == "is-active" {
			active := b
			c.IsActive = &active
		}
	}

	return c, nil
}

// ─── Session lifecycle ────────────────────────────────────────────────────────

// CreateSession inserts a new http_session_v2 row and returns the compact JWE
// session token.  The token is compatible with both the Go session middleware
// (session.go: DecryptToken) and the Clojure backend (app.http.session ver=1).
func CreateSession(ctx context.Context, pool *db.Pool, profileID, userAgent string, tokensKey []byte) (token, sessionID string, err error) {
	sessionID = newUUID()
	now := time.Now().UTC()

	var uaArg *string
	if userAgent != "" {
		uaArg = &userAgent
	}

	_, err = pool.Exec(ctx,
		`INSERT INTO http_session_v2 (id, profile_id, user_agent, created_at, modified_at)
		 VALUES ($1, $2, $3, $4, $4)`,
		sessionID, profileID, uaArg, now)
	if err != nil {
		err = fmt.Errorf("auth: insert session: %w", err)
		return
	}

	claims := TokenClaims{
		Iss: "authentication",
		Aud: "penpot",
		Sid: sessionID,
		Uid: profileID,
		Iat: now,
	}

	token, err = EncryptToken(claims, tokensKey)
	if err != nil {
		err = fmt.Errorf("auth: encrypt session token: %w", err)
	}
	return
}

// DeleteSession removes the session row from http_session_v2.
func DeleteSession(ctx context.Context, pool *db.Pool, sessionID string) error {
	_, err := pool.Exec(ctx, `DELETE FROM http_session_v2 WHERE id = $1`, sessionID)
	return err
}

// SetSessionCookie writes the auth-token cookie with a 7-day max-age,
// matching Clojure's default-cookie-max-age.
func SetSessionCookie(w http.ResponseWriter, cookieName, token string) {
	http.SetCookie(w, &http.Cookie{
		Name:     cookieName,
		Value:    token,
		Path:     "/",
		MaxAge:   int((7 * 24 * time.Hour).Seconds()),
		HttpOnly: true,
		SameSite: http.SameSiteLaxMode,
	})
}

// ClearSessionCookie expires the auth cookie immediately.
func ClearSessionCookie(w http.ResponseWriter, cookieName string) {
	http.SetCookie(w, &http.Cookie{
		Name:     cookieName,
		Value:    "",
		Path:     "/",
		MaxAge:   0,
		HttpOnly: true,
	})
}

// ─── UUID helper (local copy; handler package has its own) ───────────────────

// newUUID generates a random UUID v4 string.
func newUUID() string {
	var b [16]byte
	_, _ = rand.Read(b[:])
	b[6] = (b[6] & 0x0f) | 0x40
	b[8] = (b[8] & 0x3f) | 0x80
	return fmt.Sprintf("%08x-%04x-%04x-%04x-%012x",
		b[0:4], b[4:6], b[6:8], b[8:10], b[10:16])
}
