package handler_test

import (
	"bytes"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/handler"
)

// TestGetProjectsHandler_NoAuth verifies unauthenticated requests are rejected.
func TestGetProjectsHandler_NoAuth(t *testing.T) {
	pool := openTestDB(t)
	h := handler.GetProjectsHandler(pool)

	req := httptest.NewRequest(http.MethodGet, "/api/rpc/command/get-projects?team-id=nope", nil)
	rec := httptest.NewRecorder()
	h.ServeHTTP(rec, req)

	if rec.Code != http.StatusUnauthorized {
		t.Fatalf("expected 401, got %d", rec.Code)
	}
}

// TestCreateProjectHandler_Integration creates a project under a team.
func TestCreateProjectHandler_Integration(t *testing.T) {
	pool := openTestDB(t)
	profileID := createTestProfile(t, pool)

	// First create a team.
	teamBody, _ := json.Marshal(map[string]string{"name": "Project Test Team"})
	req := httptest.NewRequest(http.MethodPost, "/api/rpc/command/create-team", bytes.NewReader(teamBody))
	req = req.WithContext(auth.WithProfileID(req.Context(), profileID))
	rec := httptest.NewRecorder()
	handler.CreateTeamHandler(pool, nil).ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("create team failed: %d %s", rec.Code, rec.Body.String())
	}

	var team map[string]interface{}
	json.NewDecoder(rec.Body).Decode(&team) //nolint:errcheck
	teamID, _ := team["id"].(string)

	// Now create a project.
	projBody, _ := json.Marshal(map[string]string{"teamId": teamID, "name": "My Project"})
	req2 := httptest.NewRequest(http.MethodPost, "/api/rpc/command/create-project", bytes.NewReader(projBody))
	req2 = req2.WithContext(auth.WithProfileID(req2.Context(), profileID))
	rec2 := httptest.NewRecorder()
	handler.CreateProjectHandler(pool).ServeHTTP(rec2, req2)

	if rec2.Code != http.StatusOK {
		t.Fatalf("expected 200, got %d: %s", rec2.Code, rec2.Body.String())
	}
	var proj map[string]interface{}
	if err := json.NewDecoder(rec2.Body).Decode(&proj); err != nil {
		t.Fatalf("decode project: %v", err)
	}
	if proj["name"] != "My Project" {
		t.Errorf("unexpected name %v", proj["name"])
	}
}

// TestGetProjectsHandler_Integration lists projects after creation.
func TestGetProjectsHandler_Integration(t *testing.T) {
	pool := openTestDB(t)
	profileID := createTestProfile(t, pool)

	// Create team.
	teamBody, _ := json.Marshal(map[string]string{"name": "GP Team"})
	req := httptest.NewRequest(http.MethodPost, "/api/rpc/command/create-team", bytes.NewReader(teamBody))
	req = req.WithContext(auth.WithProfileID(req.Context(), profileID))
	rec := httptest.NewRecorder()
	handler.CreateTeamHandler(pool, nil).ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("create team failed: %d", rec.Code)
	}
	var team map[string]interface{}
	json.NewDecoder(rec.Body).Decode(&team) //nolint:errcheck
	teamID, _ := team["id"].(string)

	// List projects.
	req2 := httptest.NewRequest(http.MethodGet, "/api/rpc/command/get-projects?team-id="+teamID, nil)
	req2 = req2.WithContext(auth.WithProfileID(req2.Context(), profileID))
	rec2 := httptest.NewRecorder()
	handler.GetProjectsHandler(pool).ServeHTTP(rec2, req2)
	if rec2.Code != http.StatusOK {
		t.Fatalf("expected 200, got %d: %s", rec2.Code, rec2.Body.String())
	}
}
