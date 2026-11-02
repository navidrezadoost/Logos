package assetauth_test

import (
	"bytes"
	"context"
	"crypto/rand"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/go-chi/chi/v5"
	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/db"
	"github.com/logos-design/logos/backend-go/internal/handler"
	"github.com/logos-design/logos/backend-go/internal/storage"
)

const privateAssetFixture = "private asset fixture bytes"

type memoryAssetStorage struct {
	mu      sync.RWMutex
	objects map[string][]byte
}

func (s *memoryAssetStorage) Put(_ context.Context, bucket, id string, content io.Reader, _ int64, _ string) error {
	data, err := io.ReadAll(content)
	if err != nil {
		return err
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	s.objects[bucket+"/"+id] = data
	return nil
}

func (s *memoryAssetStorage) Get(_ context.Context, bucket, id string) (io.ReadCloser, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	data, ok := s.objects[bucket+"/"+id]
	if !ok {
		return nil, fmt.Errorf("object not found")
	}
	return io.NopCloser(bytes.NewReader(data)), nil
}

func (s *memoryAssetStorage) Delete(_ context.Context, bucket, id string) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	delete(s.objects, bucket+"/"+id)
	return nil
}

func TestAssetByIDHandlerEnforcesResourceAuthorization(t *testing.T) {
	pool := openAssetTestDB(t)
	ownerID := createAssetTestProfile(t, pool)
	memberID := createAssetTestProfile(t, pool)
	otherID := createAssetTestProfile(t, pool)
	teamFontUserID := createAssetTestProfile(t, pool)
	teamID, projectID := createAssetTestTeamAndProject(t, pool, ownerID)

	if _, err := pool.Exec(context.Background(), `
		INSERT INTO project_profile_rel (project_id, profile_id, is_owner, is_admin, can_edit)
		VALUES ($1, $2, false, false, false)`, projectID, memberID); err != nil {
		t.Fatalf("add project reader: %v", err)
	}
	if _, err := pool.Exec(context.Background(), `
		INSERT INTO team_profile_rel (team_id, profile_id, is_owner, is_admin, can_edit)
		VALUES ($1, $2, false, false, false)`, teamID, teamFontUserID); err != nil {
		t.Fatalf("add team font reader: %v", err)
	}

	fileID := createAssetTestFile(t, pool, ownerID, projectID)
	mediaID := randomTestUUID(t)
	objectThumbnailID := randomTestUUID(t)
	fileThumbnailID := randomTestUUID(t)
	fontFileID := randomTestUUID(t)
	projectPhotoID := randomTestUUID(t)
	profilePhotoID := randomTestUUID(t)
	unknownBucketID := randomTestUUID(t)
	mediaRowID := randomTestUUID(t)
	fontVariantID := randomTestUUID(t)
	assetIDs := []string{mediaID, objectThumbnailID, fileThumbnailID, fontFileID, projectPhotoID, profilePhotoID, unknownBucketID}
	store := &memoryAssetStorage{objects: map[string][]byte{}}
	for _, entry := range []struct{ id, bucket string }{
		{mediaID, "file-media-object"},
		{objectThumbnailID, "file-object-thumbnail"},
		{fileThumbnailID, "file-thumbnail"},
		{fontFileID, "team-font-variant"},
		{projectPhotoID, "project"},
		{profilePhotoID, "profile"},
		{unknownBucketID, "file-data-fragment"},
	} {
		metadata, _ := json.Marshal(map[string]string{"bucket": entry.bucket, "content-type": "image/png"})
		if _, err := pool.Exec(context.Background(), `
			INSERT INTO storage_object (id, backend, size, metadata)
			VALUES ($1, 'local', $2, $3)`, entry.id, len(privateAssetFixture), metadata); err != nil {
			t.Fatalf("insert %s storage object: %v", entry.bucket, err)
		}
		store.objects[entry.bucket+"/"+entry.id] = []byte(privateAssetFixture)
	}

	if _, err := pool.Exec(context.Background(), `
		INSERT INTO file_media_object
		  (id, file_id, is_local, name, media_id, width, height, mtype, created_at)
		VALUES ($1, $2, true, 'private fixture', $3, 1, 1, 'image/png', $4)`,
		mediaRowID, fileID, mediaID, time.Now().UTC()); err != nil {
		t.Fatalf("insert file media row: %v", err)
	}
	if _, err := pool.Exec(context.Background(), `
		INSERT INTO file_tagged_object_thumbnail (file_id, tag, object_id, media_id, updated_at)
		VALUES ($1, 'frame', 'frame-1', $2, now())`, fileID, objectThumbnailID); err != nil {
		t.Fatalf("insert file object thumbnail: %v", err)
	}
	if _, err := pool.Exec(context.Background(), `
		INSERT INTO file_thumbnail (file_id, revn, media_id, created_at, updated_at)
		VALUES ($1, 1, $2, now(), now())`, fileID, fileThumbnailID); err != nil {
		t.Fatalf("insert file thumbnail: %v", err)
	}
	if _, err := pool.Exec(context.Background(), `
		INSERT INTO team_font_variant
		  (id, team_id, profile_id, font_id, font_family, font_weight, font_style, woff2_file_id)
		VALUES ($1, $2, $3, $4, 'Fixture Font', 400, 'normal', $5)`,
		fontVariantID, teamID, ownerID, randomTestUUID(t), fontFileID); err != nil {
		t.Fatalf("insert font variant: %v", err)
	}
	if _, err := pool.Exec(context.Background(), `UPDATE project SET photo_id = $1 WHERE id = $2`, projectPhotoID, projectID); err != nil {
		t.Fatalf("set project photo: %v", err)
	}
	if _, err := pool.Exec(context.Background(), `UPDATE profile SET photo_id = $1 WHERE id = $2`, profilePhotoID, ownerID); err != nil {
		t.Fatalf("set profile photo: %v", err)
	}
	t.Cleanup(func() {
		pool.Exec(context.Background(), `DELETE FROM file_media_object WHERE id = $1`, mediaRowID)             //nolint:errcheck
		pool.Exec(context.Background(), `DELETE FROM file_tagged_object_thumbnail WHERE file_id = $1`, fileID) //nolint:errcheck
		pool.Exec(context.Background(), `DELETE FROM file_thumbnail WHERE file_id = $1`, fileID)               //nolint:errcheck
		pool.Exec(context.Background(), `DELETE FROM team_font_variant WHERE id = $1`, fontVariantID)          //nolint:errcheck
		pool.Exec(context.Background(), `UPDATE project SET photo_id = NULL WHERE id = $1`, projectID)         //nolint:errcheck
		pool.Exec(context.Background(), `UPDATE profile SET photo_id = NULL WHERE id = $1`, ownerID)           //nolint:errcheck
		pool.Exec(context.Background(), `DELETE FROM storage_object WHERE id = ANY($1::uuid[])`, assetIDs)     //nolint:errcheck
	})

	router := chi.NewRouter()
	router.Get("/assets/by-id/{id}", handler.AssetByIDHandler(pool, storage.Backend(store)))
	request := func(profileID, assetID, query string) *httptest.ResponseRecorder {
		req := httptest.NewRequest(http.MethodGet, "/assets/by-id/"+assetID+query, nil)
		if profileID != "" {
			req = req.WithContext(auth.WithProfileID(req.Context(), profileID))
		}
		rec := httptest.NewRecorder()
		router.ServeHTTP(rec, req)
		return rec
	}
	assertAllowed := func(name string, rec *httptest.ResponseRecorder) {
		t.Helper()
		if rec.Code != http.StatusOK || rec.Body.String() != privateAssetFixture {
			t.Fatalf("%s: expected authorized asset response, got %d %q", name, rec.Code, rec.Body.String())
		}
		if got := rec.Header().Get("Cache-Control"); got != "private, no-store" {
			t.Errorf("%s: Cache-Control = %q", name, got)
		}
	}
	assertHidden := func(name string, rec *httptest.ResponseRecorder) {
		t.Helper()
		if rec.Code != http.StatusNotFound {
			t.Fatalf("%s: expected indistinguishable 404, got %d", name, rec.Code)
		}
		if strings.Contains(rec.Body.String(), privateAssetFixture) || rec.Header().Get("Content-Type") == "image/png" {
			t.Fatalf("%s: unauthorized response disclosed asset metadata or bytes", name)
		}
	}

	assertAllowed("file owner", request(ownerID, mediaID, ""))
	assertAllowed("project reader", request(memberID, mediaID, ""))
	assertHidden("unrelated logged-in user", request(otherID, mediaID, ""))
	assertHidden("anonymous without share capability", request("", mediaID, ""))
	assertAllowed("file object thumbnail", request(memberID, objectThumbnailID, ""))
	assertAllowed("file thumbnail", request(memberID, fileThumbnailID, ""))
	assertAllowed("team font owner", request(ownerID, fontFileID, ""))
	assertAllowed("team font member", request(teamFontUserID, fontFileID, ""))
	assertHidden("project-only member cannot read team font", request(memberID, fontFileID, ""))
	assertHidden("unrelated user cannot read team font", request(otherID, fontFileID, ""))
	assertAllowed("project photo member", request(memberID, projectPhotoID, ""))
	assertHidden("unrelated user cannot read project photo", request(otherID, projectPhotoID, ""))
	assertAllowed("profile photo owner", request(ownerID, profilePhotoID, ""))
	assertHidden("other profile photo remains private", request(memberID, profilePhotoID, ""))
	assertHidden("unmapped storage bucket", request(ownerID, unknownBucketID, ""))
	assertHidden("unmapped storage bucket anonymous", request("", unknownBucketID, ""))

	shareBody, _ := json.Marshal(map[string]any{
		"fileId": fileID, "whoComment": "all", "whoInspect": "all", "pages": []string{},
	})
	shareReq := httptest.NewRequest(http.MethodPost, "/api/rpc/command/create-share-link", bytes.NewReader(shareBody))
	shareReq = shareReq.WithContext(auth.WithProfileID(shareReq.Context(), ownerID))
	shareRec := httptest.NewRecorder()
	handler.CreateShareLinkHandler(pool).ServeHTTP(shareRec, shareReq)
	if shareRec.Code != http.StatusOK {
		t.Fatalf("create all-pages share link: %d %s", shareRec.Code, shareRec.Body.String())
	}
	var share map[string]any
	if err := json.Unmarshal(shareRec.Body.Bytes(), &share); err != nil {
		t.Fatalf("decode share link: %v", err)
	}
	shareID, _ := share["id"].(string)
	assertAllowed("valid file share capability", request("", mediaID, "?share-id="+shareID))
	assertAllowed("share capability for file thumbnail", request("", fileThumbnailID, "?share-id="+shareID))
	assertHidden("file share does not grant team font", request("", fontFileID, "?share-id="+shareID))
	viewerReq := httptest.NewRequest(http.MethodGet,
		"/api/rpc/command/get-view-only-bundle?file-id="+fileID+"&share-id="+shareID, nil)
	viewerRec := httptest.NewRecorder()
	handler.GetViewOnlyBundleHandler(pool).ServeHTTP(viewerRec, viewerReq)
	if viewerRec.Code != http.StatusOK {
		t.Fatalf("view through share capability: %d %s", viewerRec.Code, viewerRec.Body.String())
	}
	if _, err := pool.Exec(context.Background(), `UPDATE file SET deleted_at = now() WHERE id = $1`, fileID); err != nil {
		t.Fatalf("soft-delete shared file: %v", err)
	}
	assertHidden("share capability for deleted file", request("", mediaID, "?share-id="+shareID))
	if _, err := pool.Exec(context.Background(), `UPDATE file SET deleted_at = NULL WHERE id = $1`, fileID); err != nil {
		t.Fatalf("restore fixture file: %v", err)
	}
	if _, err := pool.Exec(context.Background(), `UPDATE project SET deleted_at = now() WHERE id = $1`, projectID); err != nil {
		t.Fatalf("soft-delete project: %v", err)
	}
	assertHidden("asset under deleted project", request(ownerID, mediaID, ""))
	assertHidden("share under deleted project", request("", mediaID, "?share-id="+shareID))
	if _, err := pool.Exec(context.Background(), `UPDATE project SET deleted_at = NULL WHERE id = $1`, projectID); err != nil {
		t.Fatalf("restore fixture project: %v", err)
	}

	pageScopedShareID := randomTestUUID(t)
	pageID := randomTestUUID(t)
	if _, err := pool.Exec(context.Background(), `
		INSERT INTO share_link (id, file_id, owner_id, who_comment, who_inspect, pages)
		VALUES ($1, $2, $3, 'team', 'team', $4::uuid[])`,
		pageScopedShareID, fileID, ownerID, []string{pageID}); err != nil {
		t.Fatalf("insert page-scoped share: %v", err)
	}
	t.Cleanup(func() {
		pool.Exec(context.Background(), `DELETE FROM share_link WHERE id = $1`, pageScopedShareID) //nolint:errcheck
	})
	assertHidden("page-scoped share without asset-to-page proof", request("", mediaID, "?share-id="+pageScopedShareID))

	if _, err := pool.Exec(context.Background(), `DELETE FROM share_link WHERE id = $1`, shareID); err != nil {
		t.Fatalf("revoke share link: %v", err)
	}
	assertHidden("revoked share capability", request("", mediaID, "?share-id="+shareID))

	if _, err := pool.Exec(context.Background(), `
		DELETE FROM project_profile_rel WHERE project_id = $1 AND profile_id = $2`, projectID, memberID); err != nil {
		t.Fatalf("revoke project membership: %v", err)
	}
	assertHidden("revoked project membership", request(memberID, mediaID, ""))
}

func createAssetTestFile(t *testing.T, pool *db.Pool, ownerID, projectID string) string {
	t.Helper()
	fileID := randomTestUUID(t)
	dataRefID := randomTestUUID(t)
	if _, err := pool.Exec(context.Background(), `
		INSERT INTO file
		  (id, project_id, name, is_shared, features, data, version, data_backend, data_ref_id)
		VALUES ($1, $2, 'Asset authorization fixture', false, '{}', $3, 1, 'postgres', $4)`,
		fileID, projectID, []byte("{}"), dataRefID); err != nil {
		t.Fatalf("insert asset test file: %v", err)
	}
	if _, err := pool.Exec(context.Background(), `
		INSERT INTO file_profile_rel (file_id, profile_id, is_owner, is_admin, can_edit)
		VALUES ($1, $2, true, true, true)`, fileID, ownerID); err != nil {
		t.Fatalf("add asset test file owner: %v", err)
	}
	return fileID
}

func openAssetTestDB(t *testing.T) *db.Pool {
	t.Helper()
	dsn := os.Getenv("TEST_DATABASE_URL")
	if dsn == "" {
		t.Skip("TEST_DATABASE_URL not set; skipping asset authorization integration test")
	}
	pool, err := db.New(context.Background(), dsn)
	if err != nil {
		t.Fatalf("open asset test database: %v", err)
	}
	t.Cleanup(func() { pool.Close() })
	return pool
}

func createAssetTestProfile(t *testing.T, pool *db.Pool) string {
	t.Helper()
	var id string
	err := pool.QueryRow(context.Background(), `
		INSERT INTO profile (email, password, fullname, lang, theme)
		VALUES ('asset-test-'||gen_random_uuid()||'@example.com', 'test-only-not-an-auth-credential', 'Asset Test User', 'en', 'default')
		RETURNING id`).Scan(&id)
	if err != nil {
		t.Fatalf("create asset test profile: %v", err)
	}
	t.Cleanup(func() {
		pool.Exec(context.Background(), `DELETE FROM profile WHERE id = $1`, id) //nolint:errcheck
	})
	return id
}

func createAssetTestTeamAndProject(t *testing.T, pool *db.Pool, profileID string) (string, string) {
	t.Helper()
	teamID := randomTestUUID(t)
	projectID := randomTestUUID(t)
	if _, err := pool.Exec(context.Background(), `
		INSERT INTO team (id, name, is_default, features)
		VALUES ($1, 'Asset authorization team', false, '{}')`, teamID); err != nil {
		t.Fatalf("create asset test team: %v", err)
	}
	if _, err := pool.Exec(context.Background(), `
		INSERT INTO team_profile_rel (team_id, profile_id, is_owner, is_admin, can_edit)
		VALUES ($1, $2, true, true, true)`, teamID, profileID); err != nil {
		t.Fatalf("add asset test team owner: %v", err)
	}
	if _, err := pool.Exec(context.Background(), `
		INSERT INTO project (id, team_id, name, is_default)
		VALUES ($1, $2, 'Asset authorization project', false)`, projectID, teamID); err != nil {
		t.Fatalf("create asset test project: %v", err)
	}
	if _, err := pool.Exec(context.Background(), `
		INSERT INTO project_profile_rel (project_id, profile_id, is_owner, is_admin, can_edit)
		VALUES ($1, $2, true, true, true)`, projectID, profileID); err != nil {
		t.Fatalf("add asset test project owner: %v", err)
	}
	if _, err := pool.Exec(context.Background(), `
		INSERT INTO team_project_profile_rel (team_id, project_id, profile_id, is_pinned)
		VALUES ($1, $2, $3, false)`, teamID, projectID, profileID); err != nil {
		t.Fatalf("link asset test project: %v", err)
	}
	return teamID, projectID
}

func randomTestUUID(t *testing.T) string {
	t.Helper()
	var b [16]byte
	if _, err := rand.Read(b[:]); err != nil {
		t.Fatalf("generate test UUID: %v", err)
	}
	b[6] = (b[6] & 0x0f) | 0x40
	b[8] = (b[8] & 0x3f) | 0x80
	return fmt.Sprintf("%08x-%04x-%04x-%04x-%012x", b[0:4], b[4:6], b[6:8], b[8:10], b[10:16])
}
