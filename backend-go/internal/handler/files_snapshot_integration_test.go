package handler_test

import (
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/db"
	"github.com/logos-design/logos/backend-go/internal/handler"
)

func snapshotFixture(t *testing.T, payload []byte) (*db.Pool, string, string) {
	t.Helper()
	pool := openTestDB(t)
	profileID := createTestProfile(t, pool)
	_, projectID := createTestTeamAndProject(t, pool, profileID)
	fileID := integrationUUID(t)
	if _, err := pool.Exec(context.Background(), `
		INSERT INTO file (id, project_id, name, data)
		VALUES ($1, $2, 'Snapshot integrity fixture', $3)`, fileID, projectID, payload); err != nil {
		t.Fatalf("insert snapshot fixture file: %v", err)
	}
	t.Cleanup(func() {
		pool.Exec(context.Background(), `DELETE FROM file_data WHERE file_id = $1`, fileID)   //nolint:errcheck
		pool.Exec(context.Background(), `DELETE FROM file_change WHERE file_id = $1`, fileID) //nolint:errcheck
		if _, err := pool.Exec(context.Background(), `DELETE FROM file WHERE id = $1`, fileID); err != nil {
			t.Errorf("delete snapshot fixture file: %v", err)
		}
	})
	return pool, profileID, fileID
}

func insertSnapshotFixture(t *testing.T, profileID, fileID string, revn int64, label string, data []byte, metadata []byte) string {
	t.Helper()
	pool := openTestDB(t)
	id := integrationUUID(t)
	ctx := context.Background()
	if _, err := pool.Exec(ctx, `
		INSERT INTO file_change (id, file_id, profile_id, created_at, updated_at, revn, label, created_by)
		VALUES ($1, $2, $3, now(), now(), $4, $5, 'user')`, id, fileID, profileID, revn, label); err != nil {
		t.Fatalf("insert snapshot metadata fixture: %v", err)
	}
	if data != nil {
		if _, err := pool.Exec(ctx, `
			INSERT INTO file_data (id, file_id, type, data, metadata)
			VALUES ($1, $2, 'snapshot', $3, $4)`, id, fileID, data, metadata); err != nil {
			t.Fatalf("insert snapshot blob fixture: %v", err)
		}
	}
	t.Cleanup(func() {
		pool.Exec(ctx, `DELETE FROM file_data WHERE file_id = $1 AND id = $2`, fileID, id)   //nolint:errcheck
		pool.Exec(ctx, `DELETE FROM file_change WHERE file_id = $1 AND id = $2`, fileID, id) //nolint:errcheck
	})
	return id
}

// Fixtures use only TEST_DATABASE_URL and are intended for disposable DBs.

func TestCreateSnapshotPersistsRevisionBoundBlobIntegration(t *testing.T) {
	pool, profileID, fileID := snapshotFixture(t, []byte(`{"document":"revision-zero"}`))
	if _, err := pool.Exec(context.Background(), `UPDATE file SET revn = 7 WHERE id = $1`, fileID); err != nil {
		t.Fatal(err)
	}
	body := mustJSON(t, map[string]string{"fileId": fileID, "label": "revision seven"})
	req := httptest.NewRequest(http.MethodPost, "/api/rpc/command/create-file-snapshot", bytes.NewReader(body))
	req = req.WithContext(auth.WithProfileID(req.Context(), profileID))
	rec := httptest.NewRecorder()
	handler.CreateFileSnapshotHandler(pool).ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("create snapshot: status=%d body=%s", rec.Code, rec.Body.String())
	}
	var result struct {
		ID            string `json:"id"`
		Revn          int64  `json:"revn"`
		RestoreStatus string `json:"restoreStatus"`
	}
	if err := json.Unmarshal(rec.Body.Bytes(), &result); err != nil {
		t.Fatal(err)
	}
	if result.ID == "" || result.Revn != 7 || result.RestoreStatus != "verified" {
		t.Fatalf("snapshot response=%+v, want revision 7 and verified content", result)
	}
	var blob []byte
	var metadata []byte
	if err := pool.QueryRow(context.Background(), `
		SELECT data, metadata FROM file_data WHERE file_id=$1 AND id=$2 AND type='snapshot'`, fileID, result.ID).
		Scan(&blob, &metadata); err != nil {
		t.Fatalf("read snapshot blob: %v", err)
	}
	if string(blob) != `{"document":"revision-zero"}` {
		t.Fatalf("snapshot payload=%q", blob)
	}
	var meta map[string]any
	if err := json.Unmarshal(metadata, &meta); err != nil {
		t.Fatalf("decode integrity metadata: %v", err)
	}
	if meta["snapshot-integrity"] == nil {
		t.Fatalf("snapshot metadata lacks integrity digest: %s", metadata)
	}
}

func TestCreateSnapshotRejectsUnavailableContentWithoutMetadataIntegration(t *testing.T) {
	pool, profileID, fileID := snapshotFixture(t, nil)
	req := httptest.NewRequest(http.MethodPost, "/api/rpc/command/create-file-snapshot", strings.NewReader(fmt.Sprintf(`{"fileId":%q,"label":"empty content"}`, fileID)))
	req = req.WithContext(auth.WithProfileID(req.Context(), profileID))
	rec := httptest.NewRecorder()
	handler.CreateFileSnapshotHandler(pool).ServeHTTP(rec, req)
	if rec.Code != http.StatusConflict || !strings.Contains(rec.Body.String(), "snapshot-content-unavailable") {
		t.Fatalf("status=%d body=%s, want 409 snapshot-content-unavailable", rec.Code, rec.Body.String())
	}
	var count int
	if err := pool.QueryRow(context.Background(), `SELECT count(*) FROM file_change WHERE file_id=$1 AND label='empty content'`, fileID).Scan(&count); err != nil {
		t.Fatal(err)
	}
	if count != 0 {
		t.Fatalf("rejected snapshot inserted %d history rows", count)
	}
}

func TestCreateSnapshotBlobFailureRollsBackMetadataIntegration(t *testing.T) {
	pool, profileID, fileID := snapshotFixture(t, []byte(`{"document":"snapshot"}`))
	ctx := context.Background()
	if _, err := pool.Exec(ctx, `CREATE OR REPLACE FUNCTION fail_snapshot_blob_insert() RETURNS trigger LANGUAGE plpgsql AS $$
		BEGIN IF NEW.type = 'snapshot' THEN RAISE EXCEPTION 'injected snapshot blob failure'; END IF; RETURN NEW; END $$;
		`); err != nil {
		t.Fatalf("install blob failure trigger: %v", err)
	}
	if _, err := pool.Exec(ctx, `CREATE TRIGGER fail_snapshot_blob_insert BEFORE INSERT ON file_data FOR EACH ROW EXECUTE FUNCTION fail_snapshot_blob_insert()`); err != nil {
		t.Fatalf("install blob failure trigger: %v", err)
	}
	t.Cleanup(func() {
		pool.Exec(ctx, `DROP TRIGGER IF EXISTS fail_snapshot_blob_insert ON file_data`) //nolint:errcheck
		pool.Exec(ctx, `DROP FUNCTION IF EXISTS fail_snapshot_blob_insert()`)           //nolint:errcheck
	})
	req := httptest.NewRequest(http.MethodPost, "/api/rpc/command/create-file-snapshot", strings.NewReader(fmt.Sprintf(`{"fileId":%q,"label":"injected failure"}`, fileID)))
	req = req.WithContext(auth.WithProfileID(req.Context(), profileID))
	rec := httptest.NewRecorder()
	handler.CreateFileSnapshotHandler(pool).ServeHTTP(rec, req)
	if rec.Code != http.StatusInternalServerError {
		t.Fatalf("status=%d body=%s, want 500", rec.Code, rec.Body.String())
	}
	var count int
	if err := pool.QueryRow(ctx, `SELECT count(*) FROM file_change WHERE file_id=$1 AND label='injected failure'`, fileID).Scan(&count); err != nil {
		t.Fatal(err)
	}
	if count != 0 {
		t.Fatalf("failed blob write committed %d snapshot metadata rows", count)
	}
}

func TestCreateSnapshotSerializesWithConcurrentFileUpdateIntegration(t *testing.T) {
	pool, profileID, fileID := snapshotFixture(t, []byte(`{"document":"before"}`))
	ctx := context.Background()
	blocker, err := pool.Begin(ctx)
	if err != nil {
		t.Fatal(err)
	}
	defer blocker.Rollback(ctx) //nolint:errcheck
	var lockedID string
	if err := blocker.QueryRow(ctx, `SELECT id::text FROM file WHERE id=$1 FOR UPDATE`, fileID).Scan(&lockedID); err != nil {
		t.Fatal(err)
	}
	done := make(chan *httptest.ResponseRecorder, 1)
	go func() {
		body := fmt.Sprintf(`{"fileId":%q,"label":"concurrent"}`, fileID)
		req := httptest.NewRequest(http.MethodPost, "/api/rpc/command/create-file-snapshot", strings.NewReader(body))
		req = req.WithContext(auth.WithProfileID(req.Context(), profileID))
		rec := httptest.NewRecorder()
		handler.CreateFileSnapshotHandler(pool).ServeHTTP(rec, req)
		done <- rec
	}()
	select {
	case rec := <-done:
		t.Fatalf("snapshot completed while another transaction held file row lock: %d %s", rec.Code, rec.Body.String())
	case <-time.After(150 * time.Millisecond):
	}
	updated := []byte(`{"document":"after","revision":8}`)
	if _, err := blocker.Exec(ctx, `UPDATE file SET revn=8, data=$1 WHERE id=$2`, updated, fileID); err != nil {
		t.Fatal(err)
	}
	if err := blocker.Commit(ctx); err != nil {
		t.Fatal(err)
	}
	select {
	case rec := <-done:
		if rec.Code != http.StatusOK {
			t.Fatalf("snapshot status=%d body=%s", rec.Code, rec.Body.String())
		}
		var snapshot struct {
			ID   string `json:"id"`
			Revn int64  `json:"revn"`
		}
		if err := json.Unmarshal(rec.Body.Bytes(), &snapshot); err != nil {
			t.Fatal(err)
		}
		var blob []byte
		if err := pool.QueryRow(ctx, `SELECT data FROM file_data WHERE file_id=$1 AND id=$2`, fileID, snapshot.ID).Scan(&blob); err != nil {
			t.Fatal(err)
		}
		if snapshot.Revn != 8 || !bytes.Equal(blob, updated) {
			t.Fatalf("concurrent snapshot revision/payload=%d/%s; want revision 8 and new bytes", snapshot.Revn, blob)
		}
	case <-time.After(5 * time.Second):
		t.Fatal("snapshot did not resume after concurrent update committed")
	}
}

func TestCreateSnapshotRechecksPermissionAfterConcurrentRevocationIntegration(t *testing.T) {
	pool, profileID, fileID := snapshotFixture(t, []byte(`{"document":"permission race"}`))
	ctx := context.Background()
	blocker, err := pool.Begin(ctx)
	if err != nil {
		t.Fatal(err)
	}
	defer blocker.Rollback(ctx) //nolint:errcheck
	if _, err := blocker.Exec(ctx, `UPDATE team_profile_rel SET is_owner=false, is_admin=false, can_edit=false WHERE profile_id=$1`, profileID); err != nil {
		t.Fatal(err)
	}
	if _, err := blocker.Exec(ctx, `UPDATE project_profile_rel SET is_owner=false, is_admin=false, can_edit=false WHERE profile_id=$1`, profileID); err != nil {
		t.Fatal(err)
	}
	done := make(chan *httptest.ResponseRecorder, 1)
	go func() {
		req := httptest.NewRequest(http.MethodPost, "/api/rpc/command/create-file-snapshot", strings.NewReader(fmt.Sprintf(`{"fileId":%q,"label":"permission race"}`, fileID)))
		req = req.WithContext(auth.WithProfileID(req.Context(), profileID))
		rec := httptest.NewRecorder()
		handler.CreateFileSnapshotHandler(pool).ServeHTTP(rec, req)
		done <- rec
	}()
	select {
	case rec := <-done:
		t.Fatalf("snapshot did not wait on the changed permission row: %d %s", rec.Code, rec.Body.String())
	case <-time.After(150 * time.Millisecond):
	}
	if err := blocker.Commit(ctx); err != nil {
		t.Fatal(err)
	}
	select {
	case rec := <-done:
		if rec.Code != http.StatusForbidden {
			t.Fatalf("status=%d body=%s, want 403 after revocation", rec.Code, rec.Body.String())
		}
	case <-time.After(5 * time.Second):
		t.Fatal("snapshot permission check did not resume after revocation")
	}
	var count int
	if err := pool.QueryRow(ctx, `SELECT count(*) FROM file_change WHERE file_id=$1 AND label='permission race'`, fileID).Scan(&count); err != nil {
		t.Fatal(err)
	}
	if count != 0 {
		t.Fatalf("revoked request inserted %d snapshot rows", count)
	}
}

func TestRestoreRejectsMissingOrCorruptSnapshotWithoutMutationIntegration(t *testing.T) {
	for _, tc := range []struct {
		name     string
		data     []byte
		metadata []byte
		want     string
	}{
		{name: "historical metadata only", want: "snapshot-content-unavailable"},
		{name: "empty blob", data: []byte{}, want: "snapshot-content-unavailable"},
		{name: "digest mismatch", data: []byte(`{"snapshot":"corrupt"}`), metadata: []byte(`{"snapshot-integrity":{"version":1,"algorithm":"sha256","digest":"0000000000000000000000000000000000000000000000000000000000000000"}}`), want: "snapshot-content-corrupt"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			pool, profileID, fileID := snapshotFixture(t, []byte(`{"current":"unchanged"}`))
			snapshotID := insertSnapshotFixture(t, profileID, fileID, 3, tc.name, tc.data, tc.metadata)
			before := readFileIntegrityState(t, pool, fileID)
			req := httptest.NewRequest(http.MethodPost, "/api/rpc/command/restore-file-snapshot", strings.NewReader(fmt.Sprintf(`{"id":%q}`, snapshotID)))
			req = req.WithContext(auth.WithProfileID(req.Context(), profileID))
			rec := httptest.NewRecorder()
			handler.RestoreFileSnapshotHandler(pool).ServeHTTP(rec, req)
			if rec.Code != http.StatusConflict || !strings.Contains(rec.Body.String(), tc.want) {
				t.Fatalf("status=%d body=%s, want 409 %q", rec.Code, rec.Body.String(), tc.want)
			}
			if after := readFileIntegrityState(t, pool, fileID); !bytes.Equal(before.data, after.data) || before.revn != after.revn || before.vern != after.vern {
				t.Fatalf("rejected restore mutated file state: before=%+v after=%+v", before, after)
			}
			var retained int
			if err := pool.QueryRow(context.Background(), `SELECT count(*) FROM file_change WHERE file_id=$1 AND id=$2 AND deleted_at IS NULL`, fileID, snapshotID).Scan(&retained); err != nil {
				t.Fatal(err)
			}
			if retained != 1 {
				t.Fatalf("historical snapshot disappeared after failed restore")
			}
		})
	}
}

func TestRestoreFailureRollsBackAndRetryRestoresCompleteSnapshotIntegration(t *testing.T) {
	pool, profileID, fileID := snapshotFixture(t, []byte(`{"current":"state"}`))
	want := []byte(`{"snapshot":"restored"}`)
	metadata := []byte(`{"snapshot-integrity":{"version":1,"algorithm":"sha256","digest":"4b8e365353073486e2c605032d04ab012e29ac0b4e4b1468d1da90b52730bb85"}}`)
	// The digest is calculated below so the fixture is independent of line-ending changes.
	metadata = snapshotIntegrityMetadata(t, want)
	snapshotID := insertSnapshotFixture(t, profileID, fileID, 2, "complete", want, metadata)
	ctx := context.Background()
	if _, err := pool.Exec(ctx, `CREATE OR REPLACE FUNCTION fail_snapshot_restore_update() RETURNS trigger LANGUAGE plpgsql AS $$
		BEGIN IF NEW.vern > OLD.vern THEN RAISE EXCEPTION 'injected restore update failure'; END IF; RETURN NEW; END $$`); err != nil {
		t.Fatalf("install restore failure trigger: %v", err)
	}
	if _, err := pool.Exec(ctx, `CREATE TRIGGER fail_snapshot_restore_update BEFORE UPDATE ON file FOR EACH ROW EXECUTE FUNCTION fail_snapshot_restore_update()`); err != nil {
		t.Fatalf("install restore failure trigger: %v", err)
	}
	if rec := callRestore(t, pool, profileID, snapshotID); rec.Code != http.StatusInternalServerError {
		t.Fatalf("injected restore status=%d body=%s", rec.Code, rec.Body.String())
	}
	t.Cleanup(func() {
		pool.Exec(ctx, `DROP TRIGGER IF EXISTS fail_snapshot_restore_update ON file`) //nolint:errcheck
		pool.Exec(ctx, `DROP FUNCTION IF EXISTS fail_snapshot_restore_update()`)      //nolint:errcheck
	})
	if state := readFileIntegrityState(t, pool, fileID); string(state.data) != `{"current":"state"}` || state.revn != 0 || state.vern != 0 {
		t.Fatalf("failed transaction changed file: %+v", state)
	}
	var autos int
	if err := pool.QueryRow(ctx, `SELECT count(*) FROM file_change WHERE file_id=$1 AND label LIKE 'internal/pre-restore/%'`, fileID).Scan(&autos); err != nil {
		t.Fatal(err)
	}
	if autos != 0 {
		t.Fatalf("failed transaction retained %d auto snapshots", autos)
	}
	if _, err := pool.Exec(ctx, `DROP TRIGGER fail_snapshot_restore_update ON file`); err != nil {
		t.Fatal(err)
	}
	if _, err := pool.Exec(ctx, `DROP FUNCTION fail_snapshot_restore_update()`); err != nil {
		t.Fatal(err)
	}
	rec := callRestore(t, pool, profileID, snapshotID)
	if rec.Code != http.StatusOK {
		t.Fatalf("retry restore: status=%d body=%s", rec.Code, rec.Body.String())
	}
	state := readFileIntegrityState(t, pool, fileID)
	if !bytes.Equal(state.data, want) || state.vern != 1 || state.revn != 0 {
		t.Fatalf("successful restore state=%+v, want restored bytes, vern 1, unchanged revn 0", state)
	}
	var autosWithData int
	if err := pool.QueryRow(ctx, `
		SELECT count(*) FROM file_change fc JOIN file_data fd ON fd.file_id=fc.file_id AND fd.id=fc.id
		 WHERE fc.file_id=$1 AND fc.label LIKE 'internal/pre-restore/%' AND fd.data=$2`, fileID, []byte(`{"current":"state"}`)).Scan(&autosWithData); err != nil {
		t.Fatal(err)
	}
	if autosWithData != 1 {
		t.Fatalf("pre-restore snapshot rows with original content=%d, want 1", autosWithData)
	}
}

func TestHistoricalCompleteSnapshotRestoresAndIncompleteStatusIsReportedIntegration(t *testing.T) {
	pool, profileID, fileID := snapshotFixture(t, []byte(`{"current":"state"}`))
	completeData := []byte(`{"legacy":"complete"}`)
	completeID := insertSnapshotFixture(t, profileID, fileID, 4, "legacy complete", completeData, nil)
	incompleteID := insertSnapshotFixture(t, profileID, fileID, 5, "legacy incomplete", nil, nil)
	listReq := httptest.NewRequest(http.MethodGet, "/api/rpc/command/get-file-snapshots?file-id="+fileID, nil)
	listReq = listReq.WithContext(auth.WithProfileID(listReq.Context(), profileID))
	listRec := httptest.NewRecorder()
	handler.GetFileSnapshotsHandler(pool).ServeHTTP(listRec, listReq)
	if listRec.Code != http.StatusOK {
		t.Fatalf("list snapshots: %d %s", listRec.Code, listRec.Body.String())
	}
	var snapshots []struct {
		ID            string `json:"id"`
		RestoreStatus string `json:"restoreStatus"`
	}
	if err := json.Unmarshal(listRec.Body.Bytes(), &snapshots); err != nil {
		t.Fatal(err)
	}
	statusByID := map[string]string{}
	for _, snapshot := range snapshots {
		statusByID[snapshot.ID] = snapshot.RestoreStatus
	}
	if statusByID[completeID] != "legacy-unverified" || statusByID[incompleteID] != "unavailable" {
		t.Fatalf("historical restore statuses=%v", statusByID)
	}
	if rec := callRestore(t, pool, profileID, incompleteID); rec.Code != http.StatusConflict {
		t.Fatalf("metadata-only historical restore status=%d body=%s", rec.Code, rec.Body.String())
	}
	if rec := callRestore(t, pool, profileID, completeID); rec.Code != http.StatusOK {
		t.Fatalf("complete historical restore status=%d body=%s", rec.Code, rec.Body.String())
	}
	if state := readFileIntegrityState(t, pool, fileID); !bytes.Equal(state.data, completeData) {
		t.Fatalf("legacy complete snapshot did not restore bytes: %s", state.data)
	}
}

type fileIntegrityState struct {
	data       []byte
	revn, vern int64
}

func readFileIntegrityState(t *testing.T, pool *db.Pool, fileID string) fileIntegrityState {
	t.Helper()
	var state fileIntegrityState
	if err := pool.QueryRow(context.Background(), `SELECT data, revn, vern FROM file WHERE id=$1`, fileID).Scan(&state.data, &state.revn, &state.vern); err != nil {
		t.Fatal(err)
	}
	return state
}

func mustJSON(t *testing.T, value any) []byte {
	t.Helper()
	b, err := json.Marshal(value)
	if err != nil {
		t.Fatal(err)
	}
	return b
}

func callRestore(t *testing.T, pool *db.Pool, profileID, snapshotID string) *httptest.ResponseRecorder {
	t.Helper()
	req := httptest.NewRequest(http.MethodPost, "/api/rpc/command/restore-file-snapshot", strings.NewReader(fmt.Sprintf(`{"id":%q}`, snapshotID)))
	req = req.WithContext(auth.WithProfileID(req.Context(), profileID))
	rec := httptest.NewRecorder()
	handler.RestoreFileSnapshotHandler(pool).ServeHTTP(rec, req)
	return rec
}

func snapshotIntegrityMetadata(t *testing.T, data []byte) []byte {
	t.Helper()
	digest := sha256.Sum256(data)
	metadata, err := json.Marshal(map[string]any{"snapshot-integrity": map[string]any{
		"version": 1, "algorithm": "sha256", "digest": hex.EncodeToString(digest[:]),
	}})
	if err != nil {
		t.Fatal(err)
	}
	return metadata
}
