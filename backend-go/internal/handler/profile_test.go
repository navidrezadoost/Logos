// Integration tests for the profile handler family.
//
// Requires a running PostgreSQL instance (with the Logos schema applied).
// Set TEST_DATABASE_URL to run; tests are skipped otherwise.
//
// Example:
//
//	TEST_DATABASE_URL=postgres://logos:logos@localhost:5432/logos \
//	  go test -v ./internal/handler/ -run TestProfile
package handler_test

import (
	"bytes"
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"testing"

	goredis "github.com/redis/go-redis/v9"

	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/db"
	"github.com/logos-design/logos/backend-go/internal/handler"
)

// testDB returns a pool connected to TEST_DATABASE_URL, or skips the test.
func testDB(t *testing.T) *db.Pool {
	t.Helper()
	url := os.Getenv("TEST_DATABASE_URL")
	if url == "" {
		t.Skip("TEST_DATABASE_URL not set — skipping integration tests")
	}
	pool, err := db.New(context.Background(), url)
	if err != nil {
		t.Fatalf("connect to test DB: %v", err)
	}
	t.Cleanup(pool.Close)
	return pool
}

// injectProfileID returns a copy of r with profileID set in-context,
// simulating what the auth middleware does for authenticated requests.
func injectProfileID(r *http.Request, profileID string) *http.Request {
	return r.WithContext(auth.WithProfileID(r.Context(), profileID))
}

// ─── helpers ─────────────────────────────────────────────────────────────────

func jsonBody(v any) *bytes.Buffer {
	b, _ := json.Marshal(v)
	return bytes.NewBuffer(b)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

// TestGetProfileAnonymous checks that an unauthenticated request returns the
// anonymous profile object (id == "00000000-...").
func TestGetProfileAnonymous(t *testing.T) {
	pool := testDB(t)

	h := handler.ProfileHandler(pool, nil)

	req := httptest.NewRequest(http.MethodGet, "/api/rpc/command/get-profile", nil)
	w := httptest.NewRecorder()
	h(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("want 200, got %d: %s", w.Code, w.Body.String())
	}

	var p handler.Profile
	if err := json.NewDecoder(w.Body).Decode(&p); err != nil {
		t.Fatalf("decode response: %v", err)
	}

	if p.ID != "00000000-0000-0000-0000-000000000000" {
		t.Errorf("want anonymous ID, got %q", p.ID)
	}
}

// TestGetProfileAuthenticated checks that a valid (seeded) profile ID is
// returned from the database.  It uses the first profile it can find.
func TestGetProfileAuthenticated(t *testing.T) {
	pool := testDB(t)

	// Find any non-deleted profile.
	profileID := ""
	row := pool.QueryRow(context.Background(),
		`SELECT id FROM profile WHERE deleted_at IS NULL LIMIT 1`)
	if err := row.Scan(&profileID); err != nil || profileID == "" {
		t.Skip("no profiles in database — skipping authenticated test")
	}

	h := handler.ProfileHandler(pool, nil)

	req := httptest.NewRequest(http.MethodGet, "/api/rpc/command/get-profile", nil)
	req = injectProfileID(req, profileID)

	w := httptest.NewRecorder()
	h(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("want 200, got %d: %s", w.Code, w.Body.String())
	}

	var p handler.Profile
	if err := json.NewDecoder(w.Body).Decode(&p); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	if p.ID != profileID {
		t.Errorf("want profile %q, got %q", profileID, p.ID)
	}
}

// TestUpdateProfileUnauthenticated checks that PATCH without a session returns 401.
func TestUpdateProfileUnauthenticated(t *testing.T) {
	pool := testDB(t)

	h := handler.UpdateProfileHandler(pool, nil)

	req := httptest.NewRequest(http.MethodPatch, "/api/rpc/command/update-profile",
		jsonBody(map[string]string{"fullname": "Test", "lang": "en", "theme": "default"}))
	req.Header.Set("Content-Type", "application/json")

	w := httptest.NewRecorder()
	h(w, req)

	if w.Code != http.StatusUnauthorized {
		t.Errorf("want 401, got %d", w.Code)
	}
}

// TestUpdateProfileRoundTrip does a full PATCH → GET round-trip for a real DB profile.
func TestUpdateProfileRoundTrip(t *testing.T) {
	pool := testDB(t)

	// Pick a test user (first non-deleted profile).
	profileID := ""
	var origFullname string
	row := pool.QueryRow(context.Background(),
		`SELECT id, fullname FROM profile WHERE deleted_at IS NULL LIMIT 1`)
	if err := row.Scan(&profileID, &origFullname); err != nil {
		t.Skip("no profiles in database")
	}

	newName := "GoCopilot-TestUser"

	// Patch.
	patchH := handler.UpdateProfileHandler(pool, nil)
	patchReq := httptest.NewRequest(http.MethodPatch, "/api/rpc/command/update-profile",
		jsonBody(map[string]string{"fullname": newName, "lang": "en", "theme": "light"}))
	patchReq.Header.Set("Content-Type", "application/json")
	patchReq = injectProfileID(patchReq, profileID)

	pw := httptest.NewRecorder()
	patchH(pw, patchReq)
	if pw.Code != http.StatusOK {
		t.Fatalf("PATCH returned %d: %s", pw.Code, pw.Body.String())
	}

	// Read back via GET.
	getH := handler.ProfileHandler(pool, nil)
	getReq := httptest.NewRequest(http.MethodGet, "/api/rpc/command/get-profile", nil)
	getReq = injectProfileID(getReq, profileID)

	gw := httptest.NewRecorder()
	getH(gw, getReq)

	var p handler.Profile
	_ = json.NewDecoder(gw.Body).Decode(&p)
	if p.FullName != newName {
		t.Errorf("after PATCH fullname want %q, got %q", newName, p.FullName)
	}

	// Restore original name so repeated test runs are idempotent.
	_ = pool.QueryRow(context.Background(),
		`UPDATE profile SET fullname = $1 WHERE id = $2`, origFullname, profileID)
}

// TestUpdateProfilePropsUnauthenticated checks 401 without session.
func TestUpdateProfilePropsUnauthenticated(t *testing.T) {
	pool := testDB(t)
	h := handler.UpdateProfilePropsHandler(pool, nil)

	req := httptest.NewRequest(http.MethodPatch, "/api/rpc/command/update-profile-props",
		jsonBody(map[string]any{"testKey": "testValue"}))
	req.Header.Set("Content-Type", "application/json")

	w := httptest.NewRecorder()
	h(w, req)

	if w.Code != http.StatusUnauthorized {
		t.Errorf("want 401, got %d", w.Code)
	}
}

// TestDeleteProfileUnauthenticated checks 401 without session.
func TestDeleteProfileUnauthenticated(t *testing.T) {
	pool := testDB(t)
	h := handler.DeleteProfileHandler(pool, nil)

	req := httptest.NewRequest(http.MethodDelete, "/api/rpc/command/delete-profile", nil)
	w := httptest.NewRecorder()
	h(w, req)

	if w.Code != http.StatusUnauthorized {
		t.Errorf("want 401, got %d", w.Code)
	}
}

// TestRedisProfileCaching verifies that after a GET the profile is cached,
// and a second GET returns the cached copy without hitting the DB.
// Skipped when REDIS_URL is not set.
func TestRedisProfileCaching(t *testing.T) {
	redisURL := os.Getenv("TEST_REDIS_URL")
	if redisURL == "" {
		t.Skip("TEST_REDIS_URL not set — skipping Redis cache test")
	}

	pool := testDB(t)

	opts, err := goredis.ParseURL(redisURL)
	if err != nil {
		t.Fatalf("parse REDIS_URL: %v", err)
	}
	rdb := goredis.NewClient(opts)
	t.Cleanup(func() { _ = rdb.Close() })

	// Pick a test user.
	profileID := ""
	row := pool.QueryRow(context.Background(),
		`SELECT id FROM profile WHERE deleted_at IS NULL LIMIT 1`)
	if err := row.Scan(&profileID); err != nil {
		t.Skip("no profiles in database")
	}

	// Flush the key to start clean.
	_ = rdb.Del(context.Background(), "logos:cache:profile:"+profileID)

	h := handler.ProfileHandler(pool, rdb)

	// First request — should populate cache.
	req1 := httptest.NewRequest(http.MethodGet, "/", nil)
	req1 = injectProfileID(req1, profileID)
	w1 := httptest.NewRecorder()
	h(w1, req1)
	if w1.Code != http.StatusOK {
		t.Fatalf("first GET: %d", w1.Code)
	}

	// Verify key exists in Redis.
	cached, err := rdb.Get(context.Background(), "logos:cache:profile:"+profileID).Result()
	if err != nil {
		t.Fatalf("key not in Redis after first GET: %v", err)
	}
	if !strings.Contains(cached, profileID) {
		t.Errorf("cached value doesn't look like a profile: %q", cached[:min(50, len(cached))])
	}

	// Second request — should hit cache.
	req2 := httptest.NewRequest(http.MethodGet, "/", nil)
	req2 = injectProfileID(req2, profileID)
	w2 := httptest.NewRecorder()
	h(w2, req2)
	if w2.Code != http.StatusOK {
		t.Fatalf("second GET (cached): %d", w2.Code)
	}
}

func min(a, b int) int {
	if a < b {
		return a
	}
	return b
}
