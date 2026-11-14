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
	var profileID string
	if err := pool.QueryRow(context.Background(), `
		INSERT INTO profile (email, password, fullname, lang, theme)
		VALUES ('snapshot-'||gen_random_uuid()||'@example.com', 'test-only-not-an-auth-credential', 'Snapshot Test User', 'en', 'default')
		RETURNING id::text`).Scan(&profileID); err != nil {
		t.Fatalf("create snapshot test profile: %v", err)
	}
	teamID, projectID := integrationUUID(t), integrationUUID(t)
	ctx := context.Background()
	if _, err := pool.Exec(ctx, `INSERT INTO team (id,name) VALUES ($1,'Snapshot integrity test team')`, teamID); err != nil {
		t.Fatalf("create snapshot test team: %v", err)
	}
	if _, err := pool.Exec(ctx, `INSERT INTO team_profile_rel (team_id,profile_id,is_owner,is_admin,can_edit) VALUES ($1,$2,true,true,true)`, teamID, profileID); err != nil {
		t.Fatalf("grant snapshot test team access: %v", err)
	}
	if _, err := pool.Exec(ctx, `INSERT INTO project (id,team_id,name) VALUES ($1,$2,'Snapshot integrity test project')`, projectID, teamID); err != nil {
		t.Fatalf("create snapshot test project: %v", err)
	}
	if _, err := pool.Exec(ctx, `INSERT INTO project_profile_rel (project_id,profile_id,is_owner,is_admin,can_edit) VALUES ($1,$2,true,true,true)`, projectID, profileID); err != nil {
		t.Fatalf("grant snapshot test project access: %v", err)
	}
	fileID := integrationUUID(t)
	if _, err := pool.Exec(context.Background(), `
		INSERT INTO file (id, project_id, name, data)
		VALUES ($1, $2, 'Snapshot integrity fixture', $3)`, fileID, projectID, payload); err != nil {
		t.Fatalf("insert snapshot fixture file: %v", err)
	}
	t.Cleanup(func() {
		cleanup, err := pool.Begin(context.Background())
		if err != nil {
			t.Errorf("begin snapshot fixture cleanup: %v", err)
			return
		}
		defer cleanup.Rollback(context.Background()) //nolint:errcheck
		if _, err := cleanup.Exec(context.Background(), `SET LOCAL rules.deletion_protection TO off`); err != nil {
			t.Errorf("disable deletion protection for disposable fixture cleanup: %v", err)
			return
		}
		for _, stmt := range []struct {
			query string
			args  []any
		}{
			{`UPDATE profile SET default_project_id=NULL, default_team_id=NULL WHERE id=$1`, []any{profileID}},
			{`DELETE FROM file_data WHERE file_id=$1`, []any{fileID}},
			{`DELETE FROM file_change WHERE file_id=$1`, []any{fileID}},
			{`DELETE FROM file WHERE id=$1`, []any{fileID}},
			{`DELETE FROM project WHERE id=$1`, []any{projectID}},
			{`DELETE FROM team WHERE id=$1`, []any{teamID}},
			{`DELETE FROM profile WHERE id=$1`, []any{profileID}},
		} {
			if _, err := cleanup.Exec(context.Background(), stmt.query, stmt.args...); err != nil {
				t.Errorf("clean snapshot fixture: %v", err)
				return
			}
		}
		if err := cleanup.Commit(context.Background()); err != nil {
			t.Errorf("commit snapshot fixture cleanup: %v", err)
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
	var recoveryID string
	var fileExpiry, blobExpiry time.Time
	var recoveryData, recoveryMetadata []byte
	if err := pool.QueryRow(ctx, `
		SELECT fc.id::text, fc.deleted_at, fd.data, fd.metadata, fd.deleted_at
		  FROM file_change fc JOIN file_data fd ON fd.file_id=fc.file_id AND fd.id=fc.id
		 WHERE fc.file_id=$1 AND fc.label LIKE 'internal/pre-restore/%' AND fd.data=$2`, fileID, []byte(`{"current":"state"}`)).
		Scan(&recoveryID, &fileExpiry, &recoveryData, &recoveryMetadata, &blobExpiry); err != nil {
		t.Fatalf("locate pre-restore recovery point: %v", err)
	}
	if !fileExpiry.After(time.Now()) || !blobExpiry.After(time.Now()) || !bytes.Equal(recoveryData, []byte(`{"current":"state"}`)) {
		t.Fatalf("recovery point content/expiry invalid: expiry=%s/%s content=%s", fileExpiry, blobExpiry, recoveryData)
	}
	var digestEnvelope map[string]map[string]any
	if err := json.Unmarshal(recoveryMetadata, &digestEnvelope); err != nil || digestEnvelope["snapshot-integrity"]["digest"] == nil {
		t.Fatalf("pre-restore digest metadata invalid: %s (%v)", recoveryMetadata, err)
	}
	listReq := httptest.NewRequest(http.MethodGet, "/api/rpc/command/get-file-snapshots?file-id="+fileID, nil)
	listReq = listReq.WithContext(auth.WithProfileID(listReq.Context(), profileID))
	listRec := httptest.NewRecorder()
	handler.GetFileSnapshotsHandler(pool).ServeHTTP(listRec, listReq)
	if listRec.Code != http.StatusOK {
		t.Fatalf("list recovery point: %d %s", listRec.Code, listRec.Body.String())
	}
	var listed []struct {
		ID            string `json:"id"`
		RestoreStatus string `json:"restoreStatus"`
	}
	if err := json.Unmarshal(listRec.Body.Bytes(), &listed); err != nil {
		t.Fatal(err)
	}
	listedRecovery := false
	for _, item := range listed {
		if item.ID == recoveryID && item.RestoreStatus == "verified" {
			listedRecovery = true
		}
	}
	if !listedRecovery {
		t.Fatalf("future-expiring recovery point %s not listed as verified", recoveryID)
	}
	if rec := callRestore(t, pool, profileID, recoveryID); rec.Code != http.StatusOK {
		t.Fatalf("restore pre-restore recovery point: status=%d body=%s", rec.Code, rec.Body.String())
	}
	if recovered := readFileIntegrityState(t, pool, fileID); !bytes.Equal(recovered.data, []byte(`{"current":"state"}`)) {
		t.Fatalf("recovery restore content=%s, want original pre-restore content", recovered.data)
	}
}

func TestHistoricalCompleteSnapshotRestoresAndIncompleteStatusIsReportedIntegration(t *testing.T) {
	pool, profileID, fileID := snapshotFixture(t, []byte(`{"current":"state"}`))
	completeData := []byte(`{"legacy":"complete"}`)
	completeID := insertSnapshotFixture(t, profileID, fileID, 4, "legacy complete", completeData, nil)
	incompleteID := insertSnapshotFixture(t, profileID, fileID, 5, "legacy incomplete", nil, nil)
	corruptID := insertSnapshotFixture(t, profileID, fileID, 6, "corrupt", []byte(`{"corrupt":true}`), []byte(`{"snapshot-integrity":{"version":1,"algorithm":"sha256","digest":"0000000000000000000000000000000000000000000000000000000000000000"}}`))
	unsupportedID := insertSnapshotFixture(t, profileID, fileID, 7, "unsupported envelope", []byte(`{"future":true}`), []byte(`{"snapshot-integrity":{"version":2,"algorithm":"sha256","digest":"0000000000000000000000000000000000000000000000000000000000000000"}}`))
	malformedEnvelopeID := insertSnapshotFixture(t, profileID, fileID, 8, "malformed envelope", []byte(`{"future":true}`), []byte(`{"snapshot-integrity":"not-an-envelope"}`))
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
	if statusByID[completeID] != "legacy-unverified" || statusByID[incompleteID] != "unavailable" || statusByID[corruptID] != "corrupt" || statusByID[unsupportedID] != "corrupt" || statusByID[malformedEnvelopeID] != "corrupt" {
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

func TestExpiredPreRestoreRecoveryPointIsUnavailableButRemainsStoredIntegration(t *testing.T) {
	pool, profileID, fileID := snapshotFixture(t, []byte(`{"before":"restore"}`))
	snapshotData := []byte(`{"snapshot":"target"}`)
	snapshotID := insertSnapshotFixture(t, profileID, fileID, 2, "target", snapshotData, snapshotIntegrityMetadata(t, snapshotData))
	if rec := callRestore(t, pool, profileID, snapshotID); rec.Code != http.StatusOK {
		t.Fatalf("initial restore: status=%d body=%s", rec.Code, rec.Body.String())
	}
	var recoveryID string
	if err := pool.QueryRow(context.Background(), `SELECT id::text FROM file_change WHERE file_id=$1 AND created_by='system' AND label LIKE 'internal/pre-restore/%' ORDER BY created_at DESC LIMIT 1`, fileID).Scan(&recoveryID); err != nil {
		t.Fatalf("locate recovery point: %v", err)
	}
	if _, err := pool.Exec(context.Background(), `UPDATE file_change SET deleted_at=now()-interval '1 second' WHERE file_id=$1 AND id=$2`, fileID, recoveryID); err != nil {
		t.Fatal(err)
	}
	if _, err := pool.Exec(context.Background(), `UPDATE file_data SET deleted_at=now()-interval '1 second' WHERE file_id=$1 AND id=$2`, fileID, recoveryID); err != nil {
		t.Fatal(err)
	}
	listReq := httptest.NewRequest(http.MethodGet, "/api/rpc/command/get-file-snapshots?file-id="+fileID, nil)
	listReq = listReq.WithContext(auth.WithProfileID(listReq.Context(), profileID))
	listRec := httptest.NewRecorder()
	handler.GetFileSnapshotsHandler(pool).ServeHTTP(listRec, listReq)
	if listRec.Code != http.StatusOK {
		t.Fatalf("list after recovery expiry: %d %s", listRec.Code, listRec.Body.String())
	}
	var listed []struct {
		ID string `json:"id"`
	}
	if err := json.Unmarshal(listRec.Body.Bytes(), &listed); err != nil {
		t.Fatal(err)
	}
	for _, item := range listed {
		if item.ID == recoveryID {
			t.Fatalf("expired recovery point %s remained visible", recoveryID)
		}
	}
	if rec := callRestore(t, pool, profileID, recoveryID); rec.Code != http.StatusNotFound {
		t.Fatalf("restore expired recovery point: status=%d body=%s, want 404", rec.Code, rec.Body.String())
	}
	beforeRejectedRestore := readFileIntegrityState(t, pool, fileID)
	if !bytes.Equal(beforeRejectedRestore.data, snapshotData) {
		t.Fatalf("expired recovery attempt changed current file: %s", beforeRejectedRestore.data)
	}
	var retained int
	if err := pool.QueryRow(context.Background(), `SELECT count(*) FROM file_data WHERE file_id=$1 AND id=$2`, fileID, recoveryID).Scan(&retained); err != nil {
		t.Fatal(err)
	}
	if retained != 1 {
		t.Fatalf("expired content row count=%d, want soft-expired row retained pending physical cleanup", retained)
	}
}

// This test characterizes the proposed B7 purge scope on a disposable database.
// It is not a production cleanup implementation: its SQL documents the minimum
// classifier a future extension of the existing objects-gc mechanism must use.
func TestExpiredPreRestorePurgeScopeCharacterizationIntegration(t *testing.T) {
	pool, profileID, fileID := snapshotFixture(t, []byte(`{"document":"current"}`))
	ctx := context.Background()
	past := time.Now().Add(-time.Hour)
	future := time.Now().Add(time.Hour)
	type fixture struct {
		id, label, creator string
		deletedAt          time.Time
		dataType           string
	}
	fixtures := []fixture{
		{integrationUUID(t), "internal/pre-restore/10", "system", past, "snapshot"},
		{integrationUUID(t), "internal/pre-restore/11", "system", future, "snapshot"},
		{integrationUUID(t), "user snapshot", "user", past, "snapshot"},
		{integrationUUID(t), "internal/other-system-row", "system", past, "snapshot"},
		{integrationUUID(t), "", "", past, "fragment"},
	}
	for _, item := range fixtures {
		if _, err := pool.Exec(ctx, `
			INSERT INTO file_change (id,file_id,profile_id,created_at,updated_at,deleted_at,revn,label,created_by)
			VALUES ($1,$2,$3,now(),now(),$4,10,$5,$6)`, item.id, fileID, profileID, item.deletedAt, item.label, item.creator); err != nil {
			t.Fatalf("insert characterization change row (%s): %v", item.id, err)
		}
		if _, err := pool.Exec(ctx, `
			INSERT INTO file_data (id,file_id,type,data,metadata,deleted_at)
			VALUES ($1,$2,$3,$4,'{}',$5)`, item.id, fileID, item.dataType, []byte("fixture-only"), item.deletedAt); err != nil {
			t.Fatalf("insert characterization data row (%s): %v", item.id, err)
		}
	}

	// Explicit identity + type + mirrored-retention checks are required. A
	// deleted_at-only sweep would include the user snapshot and fragment.
	var legacyCandidates, eligible int
	if err := pool.QueryRow(ctx, `
		SELECT count(*) FROM file_data WHERE deleted_at IS NOT NULL AND deleted_at <= now() AND file_id=$1`, fileID).Scan(&legacyCandidates); err != nil {
		t.Fatal(err)
	}
	if err := pool.QueryRow(ctx, `
		SELECT count(*)
		  FROM file_change fc JOIN file_data fd ON fd.file_id=fc.file_id AND fd.id=fc.id
		 WHERE fc.file_id=$1 AND fc.created_by='system'
		   AND fc.label LIKE 'internal/pre-restore/%'
		   AND fd.type='snapshot'
		   AND fc.deleted_at < now() AND fd.deleted_at=fc.deleted_at
		   AND fd.deleted_at < now()`, fileID).Scan(&eligible); err != nil {
		t.Fatal(err)
	}
	if legacyCandidates != 4 || eligible != 1 {
		t.Fatalf("generic deleted_at candidates=%d and strict pre-restore candidates=%d; want 4 and 1", legacyCandidates, eligible)
	}

	// Characterize physical blob removal while preserving expired metadata and
	// all non-target bytes. The transaction and test DB are disposable.
	var targetID string
	if err := pool.QueryRow(ctx, `
		SELECT fc.id::text
		  FROM file_change fc JOIN file_data fd ON fd.file_id=fc.file_id AND fd.id=fc.id
		 WHERE fc.file_id=$1 AND fc.created_by='system'
		   AND fc.label LIKE 'internal/pre-restore/%' AND fd.type='snapshot'
		   AND fc.deleted_at < now() AND fd.deleted_at=fc.deleted_at AND fd.deleted_at < now()`, fileID).Scan(&targetID); err != nil {
		t.Fatal(err)
	}
	tx, err := pool.Begin(ctx)
	if err != nil {
		t.Fatal(err)
	}
	committed := false
	defer func() {
		if !committed {
			_ = tx.Rollback(ctx)
		}
	}()
	if _, err := tx.Exec(ctx, `DELETE FROM file_data WHERE file_id=$1 AND id=$2 AND type='snapshot'`, fileID, targetID); err != nil {
		t.Fatalf("remove characterized recovery payload: %v", err)
	}
	var remaining int
	if err := tx.QueryRow(ctx, `SELECT count(*) FROM file_change WHERE file_id=$1 AND id=$2`, fileID, targetID).Scan(&remaining); err != nil {
		t.Fatal(err)
	}
	if remaining != 1 {
		t.Fatalf("purging payload removed %d metadata rows; want retained metadata row", 1-remaining)
	}
	if err := tx.QueryRow(ctx, `SELECT count(*) FROM file_data WHERE file_id=$1`, fileID).Scan(&remaining); err != nil {
		t.Fatal(err)
	}
	if remaining != len(fixtures)-1 {
		t.Fatalf("remaining payload rows=%d, want %d", remaining, len(fixtures)-1)
	}
	if err := tx.Commit(ctx); err != nil {
		t.Fatalf("commit characterized payload purge: %v", err)
	}
	committed = true
	if err := pool.QueryRow(ctx, `SELECT count(*) FROM file_data WHERE file_id=$1 AND id=$2`, fileID, targetID).Scan(&remaining); err != nil {
		t.Fatal(err)
	}
	// This proves row-level removal after commit; it does not measure PostgreSQL
	// table/TOAST disk reclamation or secure erasure.
	if remaining != 0 {
		t.Fatalf("committed target payload rows=%d; want physical row removal", remaining)
	}
}

func TestCreatedSnapshotRecoveryPointCanBeListedAndRestoredIntegration(t *testing.T) {
	pool, profileID, fileID := snapshotFixture(t, []byte(`{"document":"before"}`))
	createReq := httptest.NewRequest(http.MethodPost, "/api/rpc/command/create-file-snapshot", strings.NewReader(fmt.Sprintf(`{"fileId":%q,"label":"before change"}`, fileID)))
	createReq = createReq.WithContext(auth.WithProfileID(createReq.Context(), profileID))
	createRec := httptest.NewRecorder()
	handler.CreateFileSnapshotHandler(pool).ServeHTTP(createRec, createReq)
	if createRec.Code != http.StatusOK {
		t.Fatalf("create source snapshot: status=%d body=%s", createRec.Code, createRec.Body.String())
	}
	var source struct {
		ID string `json:"id"`
	}
	if err := json.Unmarshal(createRec.Body.Bytes(), &source); err != nil {
		t.Fatal(err)
	}
	afterChange := []byte(`{"document":"after"}`)
	if _, err := pool.Exec(context.Background(), `UPDATE file SET data=$1 WHERE id=$2`, afterChange, fileID); err != nil {
		t.Fatal(err)
	}
	if rec := callRestore(t, pool, profileID, source.ID); rec.Code != http.StatusOK {
		t.Fatalf("restore source snapshot: status=%d body=%s", rec.Code, rec.Body.String())
	}
	if state := readFileIntegrityState(t, pool, fileID); string(state.data) != `{"document":"before"}` {
		t.Fatalf("source restore content=%s", state.data)
	}
	var recoveryID string
	var recoveryExpiry time.Time
	var recoveryBytes, recoveryMetadata []byte
	if err := pool.QueryRow(context.Background(), `
		SELECT fc.id::text, fc.deleted_at, fd.data, fd.metadata
		  FROM file_change fc JOIN file_data fd ON fd.file_id=fc.file_id AND fd.id=fc.id
		 WHERE fc.file_id=$1 AND fc.created_by='system' AND fc.label LIKE 'internal/pre-restore/%'
		 ORDER BY fc.created_at DESC LIMIT 1`, fileID).Scan(&recoveryID, &recoveryExpiry, &recoveryBytes, &recoveryMetadata); err != nil {
		t.Fatalf("read generated recovery point: %v", err)
	}
	if !recoveryExpiry.After(time.Now()) || !bytes.Equal(recoveryBytes, afterChange) {
		t.Fatalf("recovery point expiry/content=%s/%s", recoveryExpiry, recoveryBytes)
	}
	var metadata map[string]map[string]any
	if err := json.Unmarshal(recoveryMetadata, &metadata); err != nil || metadata["snapshot-integrity"]["digest"] == nil {
		t.Fatalf("recovery digest metadata=%s err=%v", recoveryMetadata, err)
	}
	listReq := httptest.NewRequest(http.MethodGet, "/api/rpc/command/get-file-snapshots?file-id="+fileID, nil)
	listReq = listReq.WithContext(auth.WithProfileID(listReq.Context(), profileID))
	listRec := httptest.NewRecorder()
	handler.GetFileSnapshotsHandler(pool).ServeHTTP(listRec, listReq)
	if listRec.Code != http.StatusOK {
		t.Fatalf("list recovery point: %d %s", listRec.Code, listRec.Body.String())
	}
	var listed []struct {
		ID            string `json:"id"`
		RestoreStatus string `json:"restoreStatus"`
	}
	if err := json.Unmarshal(listRec.Body.Bytes(), &listed); err != nil {
		t.Fatal(err)
	}
	visibleAndVerified := false
	for _, item := range listed {
		if item.ID == recoveryID && item.RestoreStatus == "verified" {
			visibleAndVerified = true
		}
	}
	if !visibleAndVerified {
		t.Fatalf("generated recovery point %s was not listed as verified", recoveryID)
	}
	if rec := callRestore(t, pool, profileID, recoveryID); rec.Code != http.StatusOK {
		t.Fatalf("restore generated recovery point: status=%d body=%s", rec.Code, rec.Body.String())
	}
	if state := readFileIntegrityState(t, pool, fileID); !bytes.Equal(state.data, afterChange) {
		t.Fatalf("recovery restore content=%s, want %s", state.data, afterChange)
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
