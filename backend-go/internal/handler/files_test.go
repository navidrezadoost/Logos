package handler_test

import (
	"bytes"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/db"
	"github.com/logos-design/logos/backend-go/internal/handler"
)

// ─── get-file ─────────────────────────────────────────────────────────────────

func TestGetFileHandler_NoAuth(t *testing.T) {
	pool := openTestDB(t)
	h := handler.GetFileHandler(pool)

	req := httptest.NewRequest(http.MethodGet, "/api/rpc/command/get-file?id=00000000-0000-0000-0000-000000000000", nil)
	rec := httptest.NewRecorder()
	h.ServeHTTP(rec, req)

	if rec.Code != http.StatusUnauthorized {
		t.Fatalf("expected 401, got %d", rec.Code)
	}
}

// ─── create-file + get-file integration ──────────────────────────────────────

func TestCreateFileHandler_Integration(t *testing.T) {
	pool := openTestDB(t)
	profileID := createTestProfile(t, pool)

	// Need a team + project first.
	teamID, projectID := createTestTeamAndProject(t, pool, profileID)
	_ = teamID

	body, _ := json.Marshal(map[string]string{"projectId": projectID, "name": "My Test File"})
	req := httptest.NewRequest(http.MethodPost, "/api/rpc/command/create-file", bytes.NewReader(body))
	req = req.WithContext(auth.WithProfileID(req.Context(), profileID))
	rec := httptest.NewRecorder()
	handler.CreateFileHandler(pool).ServeHTTP(rec, req)

	if rec.Code != http.StatusOK {
		t.Fatalf("expected 200, got %d: %s", rec.Code, rec.Body.String())
	}

	var f map[string]interface{}
	if err := json.NewDecoder(rec.Body).Decode(&f); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if f["name"] != "My Test File" {
		t.Errorf("unexpected name %v", f["name"])
	}
}

func TestGetFileHandler_Integration(t *testing.T) {
	pool := openTestDB(t)
	profileID := createTestProfile(t, pool)
	_, projectID := createTestTeamAndProject(t, pool, profileID)

	// Create file.
	body, _ := json.Marshal(map[string]string{"projectId": projectID, "name": "Readable File"})
	req := httptest.NewRequest(http.MethodPost, "/api/rpc/command/create-file", bytes.NewReader(body))
	req = req.WithContext(auth.WithProfileID(req.Context(), profileID))
	rec := httptest.NewRecorder()
	handler.CreateFileHandler(pool).ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("create file: %d", rec.Code)
	}
	var f map[string]interface{}
	json.NewDecoder(rec.Body).Decode(&f) //nolint:errcheck
	fileID := f["id"].(string)

	// Get the file back.
	req2 := httptest.NewRequest(http.MethodGet, "/api/rpc/command/get-file?id="+fileID, nil)
	req2 = req2.WithContext(auth.WithProfileID(req2.Context(), profileID))
	rec2 := httptest.NewRecorder()
	handler.GetFileHandler(pool).ServeHTTP(rec2, req2)
	if rec2.Code != http.StatusOK {
		t.Fatalf("get file: %d %s", rec2.Code, rec2.Body.String())
	}
}

// ─── share links ─────────────────────────────────────────────────────────────

func TestCreateShareLinkHandler_Integration(t *testing.T) {
	pool := openTestDB(t)
	profileID := createTestProfile(t, pool)
	_, projectID := createTestTeamAndProject(t, pool, profileID)

	// Create file.
	body, _ := json.Marshal(map[string]string{"projectId": projectID, "name": "Shared File"})
	req := httptest.NewRequest(http.MethodPost, "/api/rpc/command/create-file", bytes.NewReader(body))
	req = req.WithContext(auth.WithProfileID(req.Context(), profileID))
	rec := httptest.NewRecorder()
	handler.CreateFileHandler(pool).ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("create file: %d", rec.Code)
	}
	var f map[string]interface{}
	json.NewDecoder(rec.Body).Decode(&f) //nolint:errcheck
	fileID := f["id"].(string)

	// Create share link.
	slBody, _ := json.Marshal(map[string]interface{}{
		"fileId":     fileID,
		"whoComment": "all",
		"whoInspect": "all",
		"pages":      []string{},
	})
	req2 := httptest.NewRequest(http.MethodPost, "/api/rpc/command/create-share-link", bytes.NewReader(slBody))
	req2 = req2.WithContext(auth.WithProfileID(req2.Context(), profileID))
	rec2 := httptest.NewRecorder()
	handler.CreateShareLinkHandler(pool).ServeHTTP(rec2, req2)
	if rec2.Code != http.StatusOK {
		t.Fatalf("create share link: %d %s", rec2.Code, rec2.Body.String())
	}

	var sl map[string]interface{}
	json.NewDecoder(rec2.Body).Decode(&sl) //nolint:errcheck
	if sl["id"] == "" {
		t.Errorf("expected share link id")
	}
}

// ─── viewer bundle ────────────────────────────────────────────────────────────

func TestGetViewOnlyBundleHandler_NoAuth(t *testing.T) {
	pool := openTestDB(t)
	req := httptest.NewRequest(http.MethodGet,
		"/api/rpc/command/get-view-only-bundle?file-id=00000000-0000-0000-0000-000000000000", nil)
	rec := httptest.NewRecorder()
	handler.GetViewOnlyBundleHandler(pool).ServeHTTP(rec, req)
	if rec.Code != http.StatusUnauthorized {
		t.Fatalf("expected 401, got %d", rec.Code)
	}
}

// ─── helpers ─────────────────────────────────────────────────────────────────

// createTestTeamAndProject creates a team and default project, returns (teamID, projectID).
func createTestTeamAndProject(t *testing.T, pool *db.Pool, profileID string) (string, string) {
	t.Helper()
	body, _ := json.Marshal(map[string]string{"name": "File Test Team"})
	req := httptest.NewRequest(http.MethodPost, "/api/rpc/command/create-team", bytes.NewReader(body))
	req = req.WithContext(auth.WithProfileID(req.Context(), profileID))
	rec := httptest.NewRecorder()
	handler.CreateTeamHandler(pool, nil).ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("create team: %d %s", rec.Code, rec.Body.String())
	}
	var team map[string]interface{}
	json.NewDecoder(rec.Body).Decode(&team) //nolint:errcheck
	teamID := team["id"].(string)

	// Create a project under the team.
	projBody, _ := json.Marshal(map[string]string{"teamId": teamID, "name": "Test Project"})
	req2 := httptest.NewRequest(http.MethodPost, "/api/rpc/command/create-project", bytes.NewReader(projBody))
	req2 = req2.WithContext(auth.WithProfileID(req2.Context(), profileID))
	rec2 := httptest.NewRecorder()
	handler.CreateProjectHandler(pool).ServeHTTP(rec2, req2)
	if rec2.Code != http.StatusOK {
		t.Fatalf("create project: %d %s", rec2.Code, rec2.Body.String())
	}
	var proj map[string]interface{}
	json.NewDecoder(rec2.Body).Decode(&proj) //nolint:errcheck
	return teamID, proj["id"].(string)
}
