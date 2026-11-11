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
	var createdTeam map[string]any
	if err := json.NewDecoder(rec.Body).Decode(&createdTeam); err != nil {
		t.Fatalf("decode created team: %v", err)
	}
	teamID, _ := createdTeam["id"].(string)
	if teamID == "" {
		t.Fatal("create team response has no id")
	}

	req2 := httptest.NewRequest(http.MethodGet, "/api/rpc/command/get-teams", nil)
	req2 = req2.WithContext(auth.WithProfileID(req2.Context(), profileID))
	rec2 := httptest.NewRecorder()
	handler.GetTeamsHandler(pool, nil).ServeHTTP(rec2, req2)

	if rec2.Code != http.StatusOK {
		t.Fatalf("expected 200, got %d: %s", rec2.Code, rec2.Body.String())
	}
	var teams []map[string]any
	if err := json.NewDecoder(rec2.Body).Decode(&teams); err != nil {
		t.Fatalf("decode teams: %v", err)
	}
	if len(teams) != 1 {
		t.Fatalf("expected one team for profile with no default_team_id, got %d", len(teams))
	}
	if isDefault, ok := teams[0]["is-default-team"].(bool); ok && isDefault {
		t.Fatal("team without profile default_team_id must not be marked default")
	}

	// A non-null profile default must still be marked, rather than treating all
	// teams as non-default after making the nullable path safe.
	if _, err := pool.Exec(context.Background(), `UPDATE profile SET default_team_id = $1 WHERE id = $2`, teamID, profileID); err != nil {
		t.Fatalf("set profile default_team_id: %v", err)
	}
	req3 := httptest.NewRequest(http.MethodGet, "/api/rpc/command/get-teams", nil)
	req3 = req3.WithContext(auth.WithProfileID(req3.Context(), profileID))
	rec3 := httptest.NewRecorder()
	handler.GetTeamsHandler(pool, nil).ServeHTTP(rec3, req3)
	if rec3.Code != http.StatusOK {
		t.Fatalf("get-teams after setting default: %d %s", rec3.Code, rec3.Body.String())
	}
	var teamsWithDefault []map[string]any
	if err := json.NewDecoder(rec3.Body).Decode(&teamsWithDefault); err != nil {
		t.Fatalf("decode teams with default: %v", err)
	}
	if len(teamsWithDefault) != 1 || teamsWithDefault[0]["is-default-team"] != true {
		t.Fatalf("expected configured default team to be marked true, got %#v", teamsWithDefault)
	}
}

func TestGetTeamHandler_IntegrationWithNullDefaultTeam(t *testing.T) {
	pool := openTestDB(t)
	profileID := createTestProfile(t, pool)
	teamID, _ := createTestTeamAndProject(t, pool, profileID)

	req := httptest.NewRequest(http.MethodGet, "/api/rpc/command/get-team?id="+teamID, nil)
	req = req.WithContext(auth.WithProfileID(req.Context(), profileID))
	rec := httptest.NewRecorder()
	handler.GetTeamHandler(pool, nil).ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("expected 200 for profile with no default_team_id, got %d: %s", rec.Code, rec.Body.String())
	}
	var team map[string]any
	if err := json.NewDecoder(rec.Body).Decode(&team); err != nil {
		t.Fatalf("decode team: %v", err)
	}
	if team["id"] != teamID {
		t.Fatalf("get-team returned id %v, want %s", team["id"], teamID)
	}
	if isDefault, ok := team["is-default-team"].(bool); ok && isDefault {
		t.Fatal("team without profile default_team_id must not be marked default")
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
		INSERT INTO profile (email, password, fullname, lang, theme)
		VALUES ('test-'||gen_random_uuid()||'@example.com', 'test-only-not-an-auth-credential', 'Test User', 'en', 'default')
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
