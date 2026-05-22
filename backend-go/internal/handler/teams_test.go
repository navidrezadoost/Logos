package handler_test

import (
	"bytes"
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"testing"

	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/db"
	"github.com/logos-design/logos/backend-go/internal/handler"
)

// TestGetTeamsHandler_NoAuth verifies that unauthenticated requests are rejected.
func TestGetTeamsHandler_NoAuth(t *testing.T) {
	pool := openTestDB(t)
	h := handler.GetTeamsHandler(pool, nil)

	req := httptest.NewRequest(http.MethodGet, "/api/rpc/command/get-teams", nil)
	rec := httptest.NewRecorder()
	h.ServeHTTP(rec, req)

	if rec.Code != http.StatusUnauthorized {
		t.Fatalf("expected 401, got %d", rec.Code)
	}
}

// TestCreateTeamHandler_Integration creates a team and verifies the response.
func TestCreateTeamHandler_Integration(t *testing.T) {
	pool := openTestDB(t)
	profileID := createTestProfile(t, pool)

	body, _ := json.Marshal(map[string]string{"name": "Integration Test Team"})
	req := httptest.NewRequest(http.MethodPost, "/api/rpc/command/create-team", bytes.NewReader(body))
	req = req.WithContext(auth.WithProfileID(req.Context(), profileID))
	rec := httptest.NewRecorder()

	handler.CreateTeamHandler(pool, nil).ServeHTTP(rec, req)

	if rec.Code != http.StatusOK {
		t.Fatalf("expected 200, got %d: %s", rec.Code, rec.Body.String())
	}

	var team map[string]interface{}
	if err := json.NewDecoder(rec.Body).Decode(&team); err != nil {
		t.Fatalf("decode team: %v", err)
	}
	if team["name"] != "Integration Test Team" {
		t.Errorf("unexpected name %v", team["name"])
	}
}

// TestGetTeamsHandler_Integration lists teams for a profile.
func TestGetTeamsHandler_Integration(t *testing.T) {
	pool := openTestDB(t)
	profileID := createTestProfile(t, pool)

	// create a team first
	body, _ := json.Marshal(map[string]string{"name": "List Me"})
	req := httptest.NewRequest(http.MethodPost, "/api/rpc/command/create-team", bytes.NewReader(body))
	req = req.WithContext(auth.WithProfileID(req.Context(), profileID))
	rec := httptest.NewRecorder()
	handler.CreateTeamHandler(pool, nil).ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("create team failed: %d", rec.Code)
	}

	req2 := httptest.NewRequest(http.MethodGet, "/api/rpc/command/get-teams", nil)
	req2 = req2.WithContext(auth.WithProfileID(req2.Context(), profileID))
	rec2 := httptest.NewRecorder()
	handler.GetTeamsHandler(pool, nil).ServeHTTP(rec2, req2)

	if rec2.Code != http.StatusOK {
		t.Fatalf("expected 200, got %d: %s", rec2.Code, rec2.Body.String())
	}
}

// ─── helpers ─────────────────────────────────────────────────────────────────

// openTestDB opens a DB pool from TEST_DATABASE_URL env or skips the test.
func openTestDB(t *testing.T) *db.Pool {
	t.Helper()
	dsn := os.Getenv("TEST_DATABASE_URL")
	if dsn == "" {
		t.Skip("TEST_DATABASE_URL not set; skipping integration test")
	}
	pool, err := db.New(context.Background(), dsn)
	if err != nil {
		t.Fatalf("open test db: %v", err)
	}
	t.Cleanup(func() { pool.Close() })
	return pool
}

// createTestProfile inserts a minimal profile row and returns its ID.
func createTestProfile(t *testing.T, pool *db.Pool) string {
	t.Helper()
	var id string
	err := pool.QueryRow(context.Background(), `
		INSERT INTO profile (email, fullname, lang, theme)
		VALUES ('test-'||gen_random_uuid()||'@example.com', 'Test User', 'en', 'default')
		RETURNING id`).Scan(&id)
	if err != nil {
		t.Fatalf("create test profile: %v", err)
	}
	t.Cleanup(func() {
		pool.Exec(context.Background(), //nolint:errcheck
			`DELETE FROM profile WHERE id = $1`, id)
	})
	return id
}
