package handler_test

import (
	"bytes"
	"context"
	"crypto/rand"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/handler"
)

// TestUpdateFileHandlerRevisionPersistenceIntegration exercises real PostgreSQL
// persistence and stale-base rebase. Run only against a disposable database:
// the existing integration helpers create rows and do not comprehensively
// clean historical deletion-protection relationships.
func TestUpdateFileHandlerRevisionPersistenceIntegration(t *testing.T) {
	pool := openTestDB(t)
	profileID := createTestProfile(t, pool)
	_, projectID := createTestTeamAndProject(t, pool, profileID)

	createBody, err := json.Marshal(map[string]string{
		"projectId": projectID,
		"name":      "Revision integration fixture",
	})
	if err != nil {
		t.Fatal(err)
	}
	createReq := httptest.NewRequest(http.MethodPost, "/api/rpc/command/create-file", bytes.NewReader(createBody))
	createReq = createReq.WithContext(auth.WithProfileID(createReq.Context(), profileID))
	createRec := httptest.NewRecorder()
	handler.CreateFileHandler(pool).ServeHTTP(createRec, createReq)
	if createRec.Code != http.StatusOK {
		t.Fatalf("create file: status=%d body=%s", createRec.Code, createRec.Body.String())
	}
	var created struct {
		ID    string   `json:"id"`
		Pages []string `json:"pages"`
	}
	if err := json.NewDecoder(createRec.Body).Decode(&created); err != nil {
		t.Fatalf("decode created file: %v", err)
	}
	if created.ID == "" || len(created.Pages) == 0 {
		t.Fatalf("created file missing ID/page: %+v", created)
	}

	shapeA := integrationUUID(t)
	shapeB := integrationUUID(t)
	makeAdd := func(id, name string) map[string]any {
		return map[string]any{
			"type":      "add-obj",
			"id":        id,
			"page-id":   created.Pages[0],
			"parent-id": "00000000-0000-0000-0000-000000000000",
			"index":     0,
			"obj": map[string]any{
				"id":        id,
				"type":      "rect",
				"name":      name,
				"x":         10,
				"y":         20,
				"width":     30,
				"height":    40,
				"parent-id": "00000000-0000-0000-0000-000000000000",
				"frame-id":  "00000000-0000-0000-0000-000000000000",
				"shapes":    []string{},
			},
		}
	}
	update := func(baseRevn int64, change map[string]any) *httptest.ResponseRecorder {
		t.Helper()
		body, err := json.Marshal(map[string]any{
			"id":         created.ID,
			"session-id": integrationUUID(t),
			"revn":       baseRevn,
			"vern":       0,
			"changes":    []map[string]any{change},
		})
		if err != nil {
			t.Fatal(err)
		}
		req := httptest.NewRequest(http.MethodPost, "/api/rpc/command/update-file", bytes.NewReader(body))
		req = req.WithContext(auth.WithProfileID(req.Context(), profileID))
		rec := httptest.NewRecorder()
		handler.UpdateFileHandler(pool, nil).ServeHTTP(rec, req)
		return rec
	}

	first := update(0, makeAdd(shapeA, "First rectangle"))
	if first.Code != http.StatusOK {
		t.Fatalf("first update: status=%d body=%s", first.Code, first.Body.String())
	}
	second := update(0, makeAdd(shapeB, "Stale-base rectangle"))
	if second.Code != http.StatusOK {
		t.Fatalf("stale-base update: status=%d body=%s", second.Code, second.Body.String())
	}

	var response struct {
		Revn int64 `json:"revn"`
	}
	if err := json.NewDecoder(second.Body).Decode(&response); err != nil {
		t.Fatalf("decode update response: %v", err)
	}
	if response.Revn != 1 {
		t.Fatalf("second response base revision = %d, want prior server revision 1", response.Revn)
	}

	var revn int64
	var fileData []byte
	if err := pool.QueryRow(context.Background(), `SELECT revn, data FROM file WHERE id = $1`, created.ID).Scan(&revn, &fileData); err != nil {
		t.Fatalf("read persisted file: %v", err)
	}
	if revn != 2 {
		t.Fatalf("persisted revision = %d, want 2", revn)
	}
	var document map[string]any
	if err := json.Unmarshal(fileData, &document); err != nil {
		t.Fatalf("decode persisted file data: %v", err)
	}
	pages := document["pages-index"].(map[string]any)
	page := pages[created.Pages[0]].(map[string]any)
	objects := page["objects"].(map[string]any)
	for id, wantName := range map[string]string{shapeA: "First rectangle", shapeB: "Stale-base rectangle"} {
		shape, ok := objects[id].(map[string]any)
		if !ok {
			t.Fatalf("shape %s missing from persisted document objects", id)
		}
		if got := shape["name"]; got != wantName {
			t.Errorf("shape %s name = %v, want %q", id, got, wantName)
		}
	}

	var changeCount int
	var rebased bool
	if err := pool.QueryRow(context.Background(), `
		SELECT COUNT(*), bool_or(rebased)
		  FROM file_change
		 WHERE file_id = $1`, created.ID).Scan(&changeCount, &rebased); err != nil {
		t.Fatalf("read revision history: %v", err)
	}
	if changeCount != 2 || !rebased {
		t.Fatalf("revision rows=%d rebased=%v, want 2 rows and a stale-base rebase", changeCount, rebased)
	}
}

func TestUpdateFileHandlerRejectsMalformedLegacyCompetingHistoryIntegration(t *testing.T) {
	pool := openTestDB(t)
	profileID := createTestProfile(t, pool)
	_, projectID := createTestTeamAndProject(t, pool, profileID)
	fileID := createIntegrationFile(t, pool, profileID, projectID)

	// Historical Clojure file_change rows store Transit+zstd bytes. This zstd
	// frame marker represents a legacy row that the current JSON rebase path
	// cannot safely decode.
	legacyChanges := []byte{0x28, 0xb5, 0x2f, 0xfd}
	if _, err := pool.Exec(context.Background(), `
		INSERT INTO file_change
			(id, session_id, profile_id, created_at, updated_at, deleted_at,
			 file_id, revn, version, base_revn, rebased, changes)
		VALUES ($1, $2, $3, now(), now(), now() + interval '1 hour',
			$4, 1, 0, 0, false, $5)`,
		integrationUUID(t), integrationUUID(t), profileID, fileID, legacyChanges); err != nil {
		t.Fatalf("insert malformed legacy competing change row: %v", err)
	}
	if _, err := pool.Exec(context.Background(), `UPDATE file SET revn = 1 WHERE id = $1`, fileID); err != nil {
		t.Fatalf("set authoritative file revision: %v", err)
	}

	body, err := json.Marshal(map[string]any{
		"id":         fileID,
		"session-id": integrationUUID(t),
		"revn":       0,
		"vern":       0,
		"changes": []map[string]any{{
			"type": "del-obj",
			"id":   integrationUUID(t),
		}},
	})
	if err != nil {
		t.Fatalf("encode stale update request: %v", err)
	}
	req := httptest.NewRequest(http.MethodPost, "/api/rpc/command/update-file", bytes.NewReader(body))
	req = req.WithContext(auth.WithProfileID(req.Context(), profileID))
	rec := httptest.NewRecorder()
	handler.UpdateFileHandler(pool, nil).ServeHTTP(rec, req)
	if rec.Code != http.StatusConflict {
		t.Fatalf("stale update with malformed competing history: status=%d body=%s, want 409", rec.Code, rec.Body.String())
	}

	var revision int64
	var changeCount int
	if err := pool.QueryRow(context.Background(), `SELECT revn FROM file WHERE id = $1`, fileID).Scan(&revision); err != nil {
		t.Fatalf("read file revision after rejected update: %v", err)
	}
	if err := pool.QueryRow(context.Background(), `SELECT COUNT(*) FROM file_change WHERE file_id = $1`, fileID).Scan(&changeCount); err != nil {
		t.Fatalf("read change history after rejected update: %v", err)
	}
	if revision != 1 || changeCount != 1 {
		t.Fatalf("rejected update left revision=%d change rows=%d; want revision 1 and original row only", revision, changeCount)
	}
}

func integrationUUID(t *testing.T) string {
	t.Helper()
	var b [16]byte
	if _, err := rand.Read(b[:]); err != nil {
		t.Fatalf("generate test UUID: %v", err)
	}
	b[6] = (b[6] & 0x0f) | 0x40
	b[8] = (b[8] & 0x3f) | 0x80
	return fmt.Sprintf("%08x-%04x-%04x-%04x-%012x", b[0:4], b[4:6], b[6:8], b[8:10], b[10:16])
}
