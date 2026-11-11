package handler_test

import (
	"bytes"
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"

	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/filedata"
	"github.com/logos-design/logos/backend-go/internal/handler"
)

// TestP04FixturePersistenceSaveReloadIntegration must run only against a fresh,
// disposable PostgreSQL database. The test seeds the committed compatibility
// vector, then exercises the real Go update-file and get-file handlers.
func TestP04FixturePersistenceSaveReloadIntegration(t *testing.T) {
	updatePath := os.Getenv("P04_UPDATE_FILE")
	beforePath := os.Getenv("P04_BEFORE_OUT")
	reloadPath := os.Getenv("P04_RELOAD_OUT")
	if updatePath == "" || beforePath == "" || reloadPath == "" {
		t.Skip("run test/p0.4/document-contract/run-disposable-db-proof.sh to supply the versioned React adapter operation")
	}

	pool := openTestDB(t)
	profileID := createTestProfile(t, pool)
	_, projectID := createTestTeamAndProject(t, pool, profileID)

	var input struct {
		FileID  string          `json:"fileId"`
		Name    string          `json:"name"`
		Request json.RawMessage `json:"request"`
	}
	inputBytes, err := os.ReadFile(updatePath)
	if err != nil {
		t.Fatalf("read React adapter update: %v", err)
	}
	if err := json.Unmarshal(inputBytes, &input); err != nil {
		t.Fatalf("decode React adapter update: %v", err)
	}
	if input.FileID == "" || len(input.Request) == 0 {
		t.Fatal("adapter input is missing fileId or request")
	}

	createBody, err := json.Marshal(map[string]string{
		"id":        input.FileID,
		"projectId": projectID,
		"name":      input.Name,
	})
	if err != nil {
		t.Fatalf("encode create-file body: %v", err)
	}
	createReq := httptest.NewRequest(http.MethodPost, "/api/rpc/command/create-file", bytes.NewReader(createBody))
	createReq = createReq.WithContext(auth.WithProfileID(createReq.Context(), profileID))
	createRec := httptest.NewRecorder()
	handler.CreateFileHandler(pool).ServeHTTP(createRec, createReq)
	if createRec.Code != http.StatusOK {
		t.Fatalf("create-file: status=%d body=%s", createRec.Code, createRec.Body.String())
	}
	var created struct {
		ID string `json:"id"`
	}
	if err := json.NewDecoder(createRec.Body).Decode(&created); err != nil {
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

	unauthenticatedGet := httptest.NewRecorder()
	handler.GetFileHandler(pool).ServeHTTP(
		unauthenticatedGet,
		httptest.NewRequest(http.MethodGet, "/api/rpc/command/get-file?id="+input.FileID, nil),
	)
	if unauthenticatedGet.Code != http.StatusUnauthorized {
		t.Fatalf("unauthenticated get-file status=%d, want 401", unauthenticatedGet.Code)
	}

	unauthorizedProfileID := createTestProfile(t, pool)
	unauthorizedUpdateReq := httptest.NewRequest(http.MethodPost, "/api/rpc/command/update-file", bytes.NewReader(input.Request))
	unauthorizedUpdateReq = unauthorizedUpdateReq.WithContext(auth.WithProfileID(unauthorizedUpdateReq.Context(), unauthorizedProfileID))
	unauthorizedUpdate := httptest.NewRecorder()
	handler.UpdateFileHandler(pool, nil).ServeHTTP(unauthorizedUpdate, unauthorizedUpdateReq)
	if unauthorizedUpdate.Code != http.StatusForbidden {
		t.Fatalf("unauthorized update-file status=%d, want 403: %s", unauthorizedUpdate.Code, unauthorizedUpdate.Body.String())
	}

	get := func() map[string]any {
		t.Helper()
		req := httptest.NewRequest(http.MethodGet, "/api/rpc/command/get-file?id="+input.FileID, nil)
		req = req.WithContext(auth.WithProfileID(req.Context(), profileID))
		rec := httptest.NewRecorder()
		handler.GetFileHandler(pool).ServeHTTP(rec, req)
		if rec.Code != http.StatusOK {
			t.Fatalf("get-file: status=%d body=%s", rec.Code, rec.Body.String())
		}
		var detail map[string]any
		if err := json.NewDecoder(rec.Body).Decode(&detail); err != nil {
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

	updateReq := httptest.NewRequest(http.MethodPost, "/api/rpc/command/update-file", bytes.NewReader(input.Request))
	updateReq.Header.Set("Content-Type", "application/json")
	updateReq = updateReq.WithContext(auth.WithProfileID(updateReq.Context(), profileID))
	updateRec := httptest.NewRecorder()
	handler.UpdateFileHandler(pool, nil).ServeHTTP(updateRec, updateReq)
	if updateRec.Code != http.StatusOK {
		t.Fatalf("update-file: status=%d body=%s", updateRec.Code, updateRec.Body.String())
	}
	var updateResponse struct {
		Revn int64 `json:"revn"`
	}
	if err := json.NewDecoder(updateRec.Body).Decode(&updateResponse); err != nil {
		t.Fatalf("decode update-file response: %v", err)
	}
	if updateResponse.Revn != 0 {
		t.Fatalf("update-file base revision=%d, want 0", updateResponse.Revn)
	}

	var persistedRevision int64
	if err := pool.QueryRow(context.Background(), `SELECT revn FROM file WHERE id = $1`, input.FileID).Scan(&persistedRevision); err != nil {
		t.Fatalf("read authoritative file revision: %v", err)
	}
	if persistedRevision != 1 {
		t.Fatalf("authoritative revision=%d, want 1", persistedRevision)
	}
	var changeCount int
	if err := pool.QueryRow(context.Background(), `SELECT COUNT(*) FROM file_change WHERE file_id = $1`, input.FileID).Scan(&changeCount); err != nil {
		t.Fatalf("read persisted revision rows: %v", err)
	}
	if changeCount != 1 {
		t.Fatalf("persisted file_change rows=%d, want 1", changeCount)
	}

	reloaded := get()
	if reloaded["revn"] != float64(1) {
		t.Fatalf("get-file reload revision=%v, want 1", reloaded["revn"])
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
