package handler_test

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"

	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/filedata"
	logosserver "github.com/logos-design/logos/backend-go/internal/server"
)

// TestP04FixturePersistenceSaveReloadIntegration must run only against a fresh,
// disposable PostgreSQL database. The adapter supplies the browser-shaped
// update payloads; this test sends them over HTTP through the production router
// and session middleware, then checks persistence and a stale-base rebase.
func TestP04FixturePersistenceSaveReloadIntegration(t *testing.T) {
	updatePath := os.Getenv("P04_UPDATE_FILE")
	beforePath := os.Getenv("P04_BEFORE_OUT")
	reloadPath := os.Getenv("P04_RELOAD_OUT")
	if updatePath == "" || beforePath == "" || reloadPath == "" {
		t.Skip("run test/p0.4/document-contract/run-disposable-db-proof.sh to supply the versioned React adapter operation")
	}

	pool := openTestDB(t)

	var input struct {
		FileID       string          `json:"fileId"`
		Name         string          `json:"name"`
		Request      json.RawMessage `json:"request"`
		StaleRequest json.RawMessage `json:"staleRequest"`
	}
	inputBytes, err := os.ReadFile(updatePath)
	if err != nil {
		t.Fatalf("read React adapter update: %v", err)
	}
	if err := json.Unmarshal(inputBytes, &input); err != nil {
		t.Fatalf("decode React adapter update: %v", err)
	}
	if input.FileID == "" || len(input.Request) == 0 || len(input.StaleRequest) == 0 {
		t.Fatal("adapter input is missing fileId, request, or staleRequest")
	}

	// Use the real server router and session middleware. The test session is
	// created directly rather than exercising browser login (see the Track C
	// boundary note in docs/r1/P0.5-BROWSER-API-PERSISTENCE-BOUNDARY.md).
	tokensKey, err := auth.DeriveTokensKey("p0.5-disposable-browser-boundary-test-key")
	if err != nil {
		t.Fatalf("derive test session key: %v", err)
	}
	profileID := createTestProfile(t, pool)
	_, projectID := createTestTeamAndProject(t, pool, profileID)
	makeSessionCookie := func(id string) string {
		t.Helper()
		token, sessionID, err := auth.CreateSession(context.Background(), pool, id, "Track C persistence boundary test", tokensKey)
		if err != nil {
			t.Fatalf("create test session: %v", err)
		}
		t.Cleanup(func() {
			if err := auth.DeleteSession(context.Background(), pool, sessionID); err != nil {
				t.Errorf("delete test session: %v", err)
			}
		})
		return token
	}
	ownerCookie := makeSessionCookie(profileID)

	httpServer := httptest.NewServer(logosserver.New(logosserver.Deps{
		Pool:       pool,
		AuthMW:     auth.NewMiddleware(pool, tokensKey, "auth-token"),
		TokensKey:  tokensKey,
		CookieName: "auth-token",
	}))
	defer httpServer.Close()
	client := httpServer.Client()
	request := func(method, path string, body []byte, cookie string) (int, []byte) {
		t.Helper()
		var bodyReader io.Reader
		if body != nil {
			bodyReader = bytes.NewReader(body)
		}
		req, err := http.NewRequest(method, httpServer.URL+path, bodyReader)
		if err != nil {
			t.Fatalf("create HTTP request %s %s: %v", method, path, err)
		}
		if body != nil {
			req.Header.Set("Content-Type", "application/json")
		}
		if cookie != "" {
			req.AddCookie(&http.Cookie{Name: "auth-token", Value: cookie})
		}
		response, err := client.Do(req)
		if err != nil {
			t.Fatalf("HTTP request %s %s: %v", method, path, err)
		}
		defer response.Body.Close()
		responseBody, err := io.ReadAll(response.Body)
		if err != nil {
			t.Fatalf("read HTTP response %s %s: %v", method, path, err)
		}
		return response.StatusCode, responseBody
	}

	createBody, err := json.Marshal(map[string]string{
		"id":        input.FileID,
		"projectId": projectID,
		"name":      input.Name,
	})
	if err != nil {
		t.Fatalf("encode create-file body: %v", err)
	}
	createStatus, createResponse := request(http.MethodPost, "/api/rpc/command/create-file", createBody, ownerCookie)
	if createStatus != http.StatusOK {
		t.Fatalf("create-file: status=%d body=%s", createStatus, createResponse)
	}
	var created struct {
		ID string `json:"id"`
	}
	if err := json.Unmarshal(createResponse, &created); err != nil {
		t.Fatalf("decode create-file response: %v", err)
	}
	if created.ID != input.FileID {
		t.Fatalf("create-file id=%s, adapter file id=%s", created.ID, input.FileID)
	}

	fixturePath := filepath.Join("..", "..", "..", "test", "fixtures", "p0-document-compat", "source-document.json")
	fixtureBytes, err := os.ReadFile(fixturePath)
	if err != nil {
		t.Fatalf("read committed P0 fixture: %v", err)
	}
	var fileData map[string]any
	if err := json.Unmarshal(fixtureBytes, &fileData); err != nil {
		t.Fatalf("decode committed P0 fixture: %v", err)
	}
	fileData["id"] = input.FileID
	encodedFixture, err := json.Marshal(fileData)
	if err != nil {
		t.Fatalf("encode fixture data: %v", err)
	}
	if _, err := pool.Exec(context.Background(), `UPDATE file SET data = $1 WHERE id = $2`, encodedFixture, input.FileID); err != nil {
		t.Fatalf("seed disposable file.data fixture: %v", err)
	}

	unauthenticatedStatus, _ := request(http.MethodGet, "/api/rpc/command/get-file?id="+input.FileID, nil, "")
	if unauthenticatedStatus != http.StatusUnauthorized {
		t.Fatalf("unauthenticated get-file status=%d, want 401", unauthenticatedStatus)
	}

	unauthorizedProfileID := createTestProfile(t, pool)
	unauthorizedCookie := makeSessionCookie(unauthorizedProfileID)
	unauthorizedStatus, unauthorizedBody := request(http.MethodPost, "/api/rpc/command/update-file", input.Request, unauthorizedCookie)
	if unauthorizedStatus != http.StatusForbidden {
		t.Fatalf("unauthorized update-file status=%d, want 403: %s", unauthorizedStatus, unauthorizedBody)
	}

	get := func() map[string]any {
		t.Helper()
		status, body := request(http.MethodGet, "/api/rpc/command/get-file?id="+input.FileID, nil, ownerCookie)
		if status != http.StatusOK {
			t.Fatalf("get-file: status=%d body=%s", status, body)
		}
		var detail map[string]any
		if err := json.Unmarshal(body, &detail); err != nil {
			t.Fatalf("decode get-file response: %v", err)
		}
		return detail
	}

	before := get()
	if before["id"] != input.FileID || before["revn"] != float64(0) {
		t.Fatalf("before save identity/revision = %v/%v, want %s/0", before["id"], before["revn"], input.FileID)
	}
	writeFileDetail(t, beforePath, before)

	var updateMeta struct {
		ID      string `json:"id"`
		Revn    int64  `json:"revn"`
		Changes []struct {
			Type string `json:"type"`
			ID   string `json:"id"`
		} `json:"changes"`
	}
	if err := json.Unmarshal(input.Request, &updateMeta); err != nil {
		t.Fatalf("decode update metadata: %v", err)
	}
	if updateMeta.ID != input.FileID || updateMeta.Revn != 0 || len(updateMeta.Changes) != 1 ||
		updateMeta.Changes[0].Type != "mod-obj" {
		t.Fatalf("unexpected versioned adapter operation: %+v", updateMeta)
	}

	updateStatus, updateBody := request(http.MethodPost, "/api/rpc/command/update-file", input.Request, ownerCookie)
	if updateStatus != http.StatusOK {
		t.Fatalf("update-file: status=%d body=%s", updateStatus, updateBody)
	}
	var updateResponse struct {
		Revn int64 `json:"revn"`
	}
	if err := json.Unmarshal(updateBody, &updateResponse); err != nil {
		t.Fatalf("decode update-file response: %v", err)
	}
	if updateResponse.Revn != 0 {
		t.Fatalf("update-file base revision=%d, want 0", updateResponse.Revn)
	}

	var staleMeta struct {
		ID      string `json:"id"`
		Revn    int64  `json:"revn"`
		Changes []struct {
			Type string `json:"type"`
			ID   string `json:"id"`
		} `json:"changes"`
	}
	if err := json.Unmarshal(input.StaleRequest, &staleMeta); err != nil {
		t.Fatalf("decode stale adapter request metadata: %v", err)
	}
	if staleMeta.ID != input.FileID || staleMeta.Revn != 0 || len(staleMeta.Changes) != 1 ||
		staleMeta.Changes[0].Type != "mod-obj" || staleMeta.Changes[0].ID != updateMeta.Changes[0].ID {
		t.Fatalf("unexpected stale adapter operation: %+v", staleMeta)
	}
	staleStatus, staleBody := request(http.MethodPost, "/api/rpc/command/update-file", input.StaleRequest, ownerCookie)
	if staleStatus != http.StatusOK {
		t.Fatalf("stale-base update-file: status=%d body=%s", staleStatus, staleBody)
	}
	var staleResponse struct {
		Revn int64 `json:"revn"`
	}
	if err := json.Unmarshal(staleBody, &staleResponse); err != nil {
		t.Fatalf("decode stale-base update response: %v", err)
	}
	if staleResponse.Revn != 1 {
		t.Fatalf("stale-base response revision=%d, want current server revision 1", staleResponse.Revn)
	}

	var persistedRevision int64
	if err := pool.QueryRow(context.Background(), `SELECT revn FROM file WHERE id = $1`, input.FileID).Scan(&persistedRevision); err != nil {
		t.Fatalf("read authoritative file revision: %v", err)
	}
	if persistedRevision != 2 {
		t.Fatalf("authoritative revision=%d, want 2 after stale-base update", persistedRevision)
	}
	var changeCount int
	var rebased bool
	if err := pool.QueryRow(context.Background(), `
		SELECT COUNT(*), bool_or(rebased)
		  FROM file_change
		 WHERE file_id = $1`, input.FileID).Scan(&changeCount, &rebased); err != nil {
		t.Fatalf("read persisted revision rows: %v", err)
	}
	if changeCount != 2 || !rebased {
		t.Fatalf("persisted file_change rows=%d rebased=%v, want 2 rows and a stale-base rebase", changeCount, rebased)
	}

	reloaded := get()
	if reloaded["revn"] != float64(2) {
		t.Fatalf("get-file reload revision=%v, want 2", reloaded["revn"])
	}
	writeFileDetail(t, reloadPath, reloaded)

	data, ok := reloaded["data"].(map[string]any)
	if !ok {
		t.Fatalf("get-file reload has no document data: %#v", reloaded["data"])
	}
	page := filedata.FirstPage(data)
	objects, _ := page["objects"].(map[string]any)
	shapeID := updateMeta.Changes[0].ID
	shape, ok := objects[shapeID].(map[string]any)
	if !ok {
		t.Fatalf("reloaded shape %s missing", shapeID)
	}
	if extension, ok := shape["extension-p0"].(map[string]any); !ok || extension["sentinel"] != "preserve-me" {
		t.Fatalf("unknown extension sentinel was lost: %#v", shape["extension-p0"])
	}
}

func writeFileDetail(t *testing.T, path string, detail map[string]any) {
	t.Helper()
	data, err := json.Marshal(detail)
	if err != nil {
		t.Fatalf("encode file detail: %v", err)
	}
	if err := os.WriteFile(path, data, 0o600); err != nil {
		t.Fatalf("write file detail %s: %v", path, err)
	}
}
