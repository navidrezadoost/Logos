// Package handler contains HTTP handler functions for the Go backend.
package handler

import (
	"context"
	"crypto/rand"
	"encoding/json"
	"fmt"
	"log"
	"net/http"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/redis/go-redis/v9"

	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/db"
	"github.com/logos-design/logos/backend-go/internal/storage"
	"github.com/logos-design/logos/backend-go/internal/transit"
)

// cacheKey returns the Redis key for a cached profile.
func cacheKey(profileID string) string {
	return "logos:cache:profile:" + profileID
}

// cacheTTL matches the Clojure backend's 5-minute TTL.
const cacheTTL = 5 * time.Minute

// Profile is the JSON-serialisable shape returned by get-profile.
// Field names use kebab-case so the Transit middleware produces correct keyword
// keys for the ClojureScript frontend (e.g. "~:is-active", "~:full-name").
type Profile struct {
	ID               string         `json:"id"`
	FullName         string         `json:"fullname"`
	Email            string         `json:"email"`
	Lang             string         `json:"lang"`
	Theme            string         `json:"theme"`
	PhotoID          *string        `json:"photo-id,omitempty"`
	IsActive         bool           `json:"is-active"`
	IsBlocked        bool           `json:"is-blocked"`
	IsDemo           bool           `json:"is-demo"`
	IsMuted          bool           `json:"is-muted"`
	CreatedAt        time.Time      `json:"created-at"`
	ModifiedAt       time.Time      `json:"modified-at"`
	DefaultTeamID    *string        `json:"default-team-id,omitempty"`
	DefaultProjectID *string        `json:"default-project-id,omitempty"`
	Props            map[string]any `json:"props,omitempty"`
}

// now is a helper so zero-value Profile.CreatedAt is sensible.
var epoch = time.Date(2024, 1, 1, 0, 0, 0, 0, time.UTC)

// anonymousProfile is returned when no authenticated session is present.
// Mirrors the shape Penpot's Clojure backend returns for anonymous callers.
var anonymousProfile = Profile{
	ID:         "00000000-0000-0000-0000-000000000000",
	FullName:   "Anonymous User",
	Email:      "",
	Lang:       "",
	Theme:      "default",
	IsActive:   true,
	IsBlocked:  false,
	IsDemo:     false,
	IsMuted:    false,
	CreatedAt:  epoch,
	ModifiedAt: epoch,
}

// apiError is a Penpot-compatible error response.
// Using transit.Keyword for Type and Code causes them to be serialised as
// Transit keywords ("~:error", "~:not-found", …) by the Transit middleware.
type apiError struct {
	Type transit.Keyword `json:"type"`
	Code transit.Keyword `json:"code"`
	Hint string          `json:"hint"`
}

// writeJSON encodes v as JSON and writes to w with the given HTTP status.
func writeJSON(w http.ResponseWriter, status int, v any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(v)
}

// writePlainStringMap writes a Penpot [:map-of string string] RPC result.
// Keys and values remain plain strings in Transit (object-id → media-id).
func writePlainStringMap(w http.ResponseWriter, status int, m map[string]string) {
	if m == nil {
		m = map[string]string{}
	}
	body, err := transit.EncodePlainStringMap(m)
	if err != nil {
		writeError(w, http.StatusInternalServerError, "internal server error")
		return
	}
	if len(body) == 0 {
		body = []byte(`["^ "]`)
	}
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	if n, err := w.Write(body); err != nil {
		log.Printf("[rpc] writePlainStringMap write failed: %v", err)
	} else if n != len(body) {
		log.Printf("[rpc] writePlainStringMap short write: wrote=%d want=%d", n, len(body))
	}
}

// writeError writes a Penpot-style error response compatible with Transit
// decoding on the ClojureScript side.
func writeError(w http.ResponseWriter, status int, code string) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(apiError{
		Type: "error",
		Code: transit.Keyword(code),
		Hint: code,
	})
}

// writeAuthError writes a session/auth failure.  The ClojureScript frontend
// dispatches on :type :authentication to redirect to login; :type :error falls
// through to the default handler and shows "Unexpected error: …".
func writeAuthError(w http.ResponseWriter, status int, code string) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(apiError{
		Type: "authentication",
		Code: transit.Keyword(code),
		Hint: code,
	})
}

// RPCNotFoundHandler returns a Penpot-compatible JSON 404 for unknown RPC methods.
func RPCNotFoundHandler() http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		writeError(w, http.StatusNotFound, "method-not-found")
	}
}

// newUUID generates a random UUID v4 string without external dependencies.
func newUUID() string {
	var b [16]byte
	_, _ = rand.Read(b[:])
	b[6] = (b[6] & 0x0f) | 0x40 // version 4
	b[8] = (b[8] & 0x3f) | 0x80 // variant bits
	return fmt.Sprintf("%08x-%04x-%04x-%04x-%012x", b[0:4], b[4:6], b[6:8], b[8:10], b[10:16])
}

// ─── GET /api/rpc/command/get-profile ────────────────────────────────────────

// ProfileHandler returns the caller's profile (or anonymous if not authenticated).
// It reads from the Redis read-through cache (logos:cache:profile:<id>, 5-min TTL).
func ProfileHandler(pool *db.Pool, rdb *redis.Client) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeJSON(w, http.StatusOK, anonymousProfile)
			return
		}

		// Try Redis cache first.
		if rdb != nil {
			if cached, ok := getCachedProfile(r.Context(), rdb, profileID); ok {
				cached.Props = mergeOnboardingSkipProps(cached.Props)
				writeJSON(w, http.StatusOK, cached)
				return
			}
		}

		profile, err := fetchProfile(r.Context(), pool, profileID)
		if err != nil {
			if err == pgx.ErrNoRows {
				writeJSON(w, http.StatusOK, anonymousProfile)
				return
			}
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}

		// Populate Redis cache.
		if rdb != nil {
			setCachedProfile(r.Context(), rdb, profileID, profile)
		}

		writeJSON(w, http.StatusOK, profile)
	}
}

// ─── PATCH /api/rpc/command/update-profile ───────────────────────────────────

type updateProfileParams struct {
	FullName string `json:"fullname"`
	Lang     string `json:"lang"`
	Theme    string `json:"theme"`
}

// UpdateProfileHandler updates fullname / lang / theme and invalidates the cache.
func UpdateProfileHandler(pool *db.Pool, rdb *redis.Client) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
			return
		}

		var params updateProfileParams
		if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}

		const q = `
			UPDATE profile
			SET fullname = $1, lang = $2, theme = $3, modified_at = now()
			WHERE id = $4 AND deleted_at IS NULL`

		if _, err := pool.Exec(r.Context(), q, params.FullName, params.Lang, params.Theme, profileID); err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}

		invalidateCache(r.Context(), rdb, profileID)

		profile, err := fetchProfile(r.Context(), pool, profileID)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}

		writeJSON(w, http.StatusOK, profile)
	}
}

// ─── PATCH /api/rpc/command/update-profile-props ─────────────────────────────

// UpdateProfilePropsHandler merges the supplied props map into the profile's
// existing JSONB props column (nil values remove the key, matching Clojure).
func UpdateProfilePropsHandler(pool *db.Pool, rdb *redis.Client) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
			return
		}

		var incoming map[string]any
		if err := json.NewDecoder(r.Body).Decode(&incoming); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}

		tx, err := pool.Begin(r.Context())
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		defer tx.Rollback(r.Context()) //nolint:errcheck

		// Lock row and read current props.
		var propsRaw []byte
		err = tx.QueryRow(r.Context(),
			`SELECT props FROM profile WHERE id = $1 AND deleted_at IS NULL FOR UPDATE`,
			profileID).Scan(&propsRaw)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}

		existing := make(map[string]any)
		if propsRaw != nil {
			_ = json.Unmarshal(propsRaw, &existing)
		}

		for k, v := range incoming {
			if v == nil {
				delete(existing, k)
			} else {
				existing[k] = v
			}
		}

		merged, _ := json.Marshal(existing)
		if _, err = tx.Exec(r.Context(),
			`UPDATE profile SET props = $1, modified_at = now() WHERE id = $2`, merged, profileID); err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}

		if err = tx.Commit(r.Context()); err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}

		invalidateCache(r.Context(), rdb, profileID)
		writeJSON(w, http.StatusOK, existing)
	}
}

// ─── POST /api/rpc/command/update-profile-photo ──────────────────────────────

// UpdateProfilePhotoHandler accepts a multipart upload, stores the photo via the
// storage backend, records it in storage_object, and updates the profile row.
func UpdateProfilePhotoHandler(pool *db.Pool, rdb *redis.Client, sto storage.Backend) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
			return
		}

		// 10 MB max.
		if err := r.ParseMultipartForm(10 << 20); err != nil {
			writeError(w, http.StatusBadRequest, "multipart parse error")
			return
		}

		file, header, err := r.FormFile("file")
		if err != nil {
			writeError(w, http.StatusBadRequest, "missing file field")
			return
		}
		defer file.Close()

		contentType := header.Header.Get("Content-Type")
		if contentType == "" {
			contentType = "image/jpeg"
		}

		objectID := newUUID()

		// Register the storage object row.
		metaJSON := fmt.Sprintf(`{"bucket":"profile","content-type":%q}`, contentType)
		if _, err = pool.Exec(r.Context(),
			`INSERT INTO storage_object (id, backend, size, metadata) VALUES ($1, $2, $3, $4)`,
			objectID, "local", header.Size, metaJSON); err != nil {
			writeError(w, http.StatusInternalServerError, "storage object insert failed")
			return
		}

		if err = sto.Put(r.Context(), "profile", objectID, file, header.Size, contentType); err != nil {
			writeError(w, http.StatusInternalServerError, "storage write failed")
			return
		}

		// Fetch old photo ID for cleanup.
		var oldPhotoID *string
		_ = pool.QueryRow(r.Context(),
			`SELECT photo_id FROM profile WHERE id = $1`, profileID).Scan(&oldPhotoID)

		if _, err = pool.Exec(r.Context(),
			`UPDATE profile SET photo_id = $1, modified_at = now() WHERE id = $2`,
			objectID, profileID); err != nil {
			writeError(w, http.StatusInternalServerError, "profile update failed")
			return
		}

		if oldPhotoID != nil {
			_, _ = pool.Exec(r.Context(),
				`UPDATE storage_object SET deleted_at = now() WHERE id = $1`, *oldPhotoID)
		}

		invalidateCache(r.Context(), rdb, profileID)
		writeJSON(w, http.StatusOK, map[string]string{"photoId": objectID})
	}
}

// ─── DELETE /api/rpc/command/delete-profile ───────────────────────────────────

// DeleteProfileHandler soft-deletes the authenticated profile (sets deleted_at).
// Rejects the request if the user owns any team that still has other members.
func DeleteProfileHandler(pool *db.Pool, rdb *redis.Client) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
			return
		}

		// Reject if any owned team still has other members.
		const ownedTeamsQ = `
			SELECT COUNT(*)
			FROM team_profile_rel tpr
			WHERE tpr.is_owner = true
			  AND tpr.profile_id = $1
			  AND (SELECT COUNT(*) FROM team_profile_rel WHERE team_id = tpr.team_id) > 1`

		var teamsWithPeople int
		if err := pool.QueryRow(r.Context(), ownedTeamsQ, profileID).Scan(&teamsWithPeople); err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		if teamsWithPeople > 0 {
			writeError(w, http.StatusUnprocessableEntity, "owner-teams-with-people")
			return
		}

		if _, err := pool.Exec(r.Context(),
			`UPDATE profile SET deleted_at = now() WHERE id = $1`, profileID); err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}

		invalidateCache(r.Context(), rdb, profileID)
		w.WriteHeader(http.StatusNoContent)
	}
}

// ─── Shared helpers ───────────────────────────────────────────────────────────

func fetchProfile(ctx context.Context, pool *db.Pool, id string) (*Profile, error) {
	const q = `
		SELECT
			id::text,
			COALESCE(fullname, ''),
			COALESCE(email, ''),
			COALESCE(lang, ''),
			COALESCE(theme, 'default'),
			photo_id,
			COALESCE(is_active, false),
			COALESCE(is_blocked, false),
			COALESCE(is_demo, false),
			COALESCE(is_muted, false),
			created_at,
			modified_at,
			default_team_id,
			default_project_id,
			props
		FROM profile
		WHERE id = $1
		  AND deleted_at IS NULL`

	var p Profile
	var propsRaw []byte

	err := pool.QueryRow(ctx, q, id).Scan(
		&p.ID, &p.FullName, &p.Email, &p.Lang, &p.Theme, &p.PhotoID,
		&p.IsActive, &p.IsBlocked, &p.IsDemo, &p.IsMuted,
		&p.CreatedAt, &p.ModifiedAt,
		&p.DefaultTeamID, &p.DefaultProjectID,
		&propsRaw,
	)
	if err != nil {
		return nil, err
	}

	if propsRaw != nil {
		p.Props = make(map[string]any)
		_ = json.Unmarshal(propsRaw, &p.Props)
	}
	p.Props = mergeOnboardingSkipProps(p.Props)

	return &p, nil
}

func getCachedProfile(ctx context.Context, rdb *redis.Client, profileID string) (*Profile, bool) {
	data, err := rdb.Get(ctx, cacheKey(profileID)).Bytes()
	if err != nil {
		return nil, false
	}
	var p Profile
	if err := json.Unmarshal(data, &p); err != nil {
		return nil, false
	}
	return &p, true
}

func setCachedProfile(ctx context.Context, rdb *redis.Client, profileID string, p *Profile) {
	data, err := json.Marshal(p)
	if err != nil {
		return
	}
	_ = rdb.Set(ctx, cacheKey(profileID), data, cacheTTL).Err()
}

func invalidateCache(ctx context.Context, rdb *redis.Client, profileID string) {
	if rdb == nil {
		return
	}
	_ = rdb.Del(ctx, cacheKey(profileID)).Err()
}
