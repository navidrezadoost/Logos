// Package auth provides session management middleware for the Go backend.
//
// Session tokens are JWE compact tokens (AES-256-KW + AES-256-GCM) signed
// with a key derived from LOGOS_SECRET_KEY using HKDF-Blake2b-512.
// After decryption, the payload is a Transit JSON map carrying:
//   - "~:sid"  — session UUID  (looked up in http_session_v2)
//   - "~:uid"  — profile UUID  (stored in context, used by handlers)
//   - "~:iss"  — "authentication"  (issuer assertion)
//
// The session record is loaded from PostgreSQL (the same db the Clojure
// backend uses). Redis is not involved in session storage.
package auth

import (
	"context"
	"encoding/json"
	"fmt"
	"hash"
	"io"
	"net/http"
	"strings"
	"time"

	gojose "github.com/go-jose/go-jose/v3"
	"github.com/jackc/pgx/v5"
	"github.com/logos-design/logos/backend-go/internal/db"
	"golang.org/x/crypto/blake2b"
	"golang.org/x/crypto/hkdf"
)

// ctxKey is the unexported type for context keys set by this package.
type ctxKey int

const (
	ctxProfileID ctxKey = iota
	ctxSessionID ctxKey = iota
)

// DeriveTokensKey reproduces Clojure's `(keys/derive secret :salt "tokens")`:
// HKDF-Blake2b-512, IKM = []byte(secret), salt = []byte("tokens"), 32 bytes.
// Returns an error if secret is empty (caller should guard for this).
func DeriveTokensKey(secret string) ([]byte, error) {
	if secret == "" {
		return nil, fmt.Errorf("auth: LOGOS_SECRET_KEY is not set")
	}
	newHash := func() hash.Hash {
		h, err := blake2b.New512(nil)
		if err != nil {
			// blake2b.New512(nil) only fails if len(nil key) > 64, which nil cannot be.
			panic(fmt.Sprintf("blake2b.New512: %v", err))
		}
		return h
	}
	r := hkdf.New(newHash, []byte(secret), []byte("tokens"), nil)
	key := make([]byte, 32)
	if _, err := io.ReadFull(r, key); err != nil {
		return nil, fmt.Errorf("auth: HKDF read: %w", err)
	}
	return key, nil
}

// transitClaims is the unmarshalled view of the Transit JSON payload inside
// a session JWE token.  We only need the session-id and profile-id fields.
type transitClaims struct {
	Iss       string
	SessionID string // ~:sid
	ProfileID string // ~:uid
}

// decodeTransitClaims parses the Transit JSON produced by Clojure's
// (t/encode claims) from the decrypted JWE payload.
//
// Transit JSON encodes keywords as "~:<name>" and UUIDs as "~u<uuid>".
// E.g.: {"~:iss":"authentication","~:sid":"~u<uuid>","~:uid":"~u<uuid>"}
func decodeTransitClaims(data []byte) (transitClaims, error) {
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return transitClaims{}, fmt.Errorf("auth: transit json unmarshal: %w", err)
	}

	extract := func(key string) string {
		val, ok := raw[key]
		if !ok {
			return ""
		}
		var s string
		if err := json.Unmarshal(val, &s); err != nil {
			return ""
		}
		// UUID: "~u550e8400-..."  →  "550e8400-..."
		if strings.HasPrefix(s, "~u") {
			return s[2:]
		}
		return s
	}

	return transitClaims{
		Iss:       extract("~:iss"),
		SessionID: extract("~:sid"),
		ProfileID: extract("~:uid"),
	}, nil
}

// DecryptToken decrypts a compact JWE token with the given tokens-key and
// returns the Transit claims.
func DecryptToken(token string, tokensKey []byte) (transitClaims, error) {
	enc, err := gojose.ParseEncrypted(token)
	if err != nil {
		return transitClaims{}, fmt.Errorf("auth: parse jwe: %w", err)
	}
	plain, err := enc.Decrypt(tokensKey)
	if err != nil {
		return transitClaims{}, fmt.Errorf("auth: decrypt jwe: %w", err)
	}
	return decodeTransitClaims(plain)
}

// Session is a row from http_session_v2.
type Session struct {
	ID        string
	ProfileID string
	CreatedAt time.Time
	UpdatedAt time.Time
}

// LookupSession fetches the session by UUID from the database.
func LookupSession(ctx context.Context, pool *db.Pool, sessionID string) (*Session, error) {
	const q = `
		SELECT id::text, profile_id::text, created_at, modified_at
		  FROM http_session_v2
		 WHERE id = $1`

	row := pool.QueryRow(ctx, q, sessionID)
	var s Session
	if err := row.Scan(&s.ID, &s.ProfileID, &s.CreatedAt, &s.UpdatedAt); err != nil {
		if err == pgx.ErrNoRows {
			return nil, nil
		}
		return nil, fmt.Errorf("auth: lookup session: %w", err)
	}
	return &s, nil
}

// Middleware is the chi-compatible session authentication middleware.
//
// It reads the auth-token cookie, decrypts the JWE, looks up the session in
// the database, and sets the profile-id on the request context.
//
// If the cookie is absent or invalid the request proceeds without a profile-id
// set; handlers that require auth should call RequireAuth.
type Middleware struct {
	pool       *db.Pool
	tokensKey  []byte
	cookieName string
}

// NewMiddleware creates an auth middleware. Both pool and a non-empty tokensKey
// must be provided. Use DeriveTokensKey to compute tokensKey from the secret.
func NewMiddleware(pool *db.Pool, tokensKey []byte, cookieName string) *Middleware {
	return &Middleware{pool: pool, tokensKey: tokensKey, cookieName: cookieName}
}

// Handler implements http.Handler wrapping.
func (m *Middleware) Handler(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		cookie, err := r.Cookie(m.cookieName)
		if err != nil || cookie.Value == "" {
			// No session cookie; continue as anonymous.
			next.ServeHTTP(w, r)
			return
		}

		claims, err := DecryptToken(cookie.Value, m.tokensKey)
		if err != nil || claims.Iss != "authentication" || claims.SessionID == "" {
			next.ServeHTTP(w, r)
			return
		}

		sess, err := LookupSession(r.Context(), m.pool, claims.SessionID)
		if err != nil || sess == nil {
			next.ServeHTTP(w, r)
			return
		}

		ctx := context.WithValue(r.Context(), ctxProfileID, sess.ProfileID)
		ctx = context.WithValue(ctx, ctxSessionID, sess.ID)
		next.ServeHTTP(w, r.WithContext(ctx))
	})
}

// ProfileID returns the authenticated profile-id from the context, or "".
func ProfileID(ctx context.Context) string {
	v, _ := ctx.Value(ctxProfileID).(string)
	return v
}

// WithProfileID returns a copy of ctx with the given profile ID stored,
// exactly as the session middleware would set it.  Used in tests.
func WithProfileID(ctx context.Context, profileID string) context.Context {
	return context.WithValue(ctx, ctxProfileID, profileID)
}

// SessionID returns the session id from the context, or "".
func SessionID(ctx context.Context) string {
	v, _ := ctx.Value(ctxSessionID).(string)
	return v
}

// RequireAuth writes 403 and returns false if no profile-id is in the context.
// Use at the top of any handler that requires authentication.
func RequireAuth(w http.ResponseWriter, r *http.Request) bool {
	if ProfileID(r.Context()) == "" {
		http.Error(w, `{"type":"~:authentication","code":"~:not-authenticated","hint":"not-authenticated"}`, http.StatusUnauthorized)
		return false
	}
	return true
}
