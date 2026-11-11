package handler_test

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/logos-design/logos/backend-go/internal/auth"
	logosserver "github.com/logos-design/logos/backend-go/internal/server"
)

// TestP05BrowserToAPIPersistence exercises the versioned React projection and
// existing documentStore command in a real browser, with same-origin cookie
// authentication through the production Go router. The HTML is a test-only
// harness, not an editor route.
func TestP05BrowserToAPIPersistence(t *testing.T) {
	assetsDir := os.Getenv("P05_BROWSER_ASSETS")
	chromeBin := os.Getenv("P05_BROWSER_BIN")
	updatePath := os.Getenv("P04_UPDATE_FILE")
	if assetsDir == "" || updatePath == "" {
		t.Skip("set P05_BROWSER_ASSETS and P04_UPDATE_FILE via test/p0.5/browser-persistence/run-browser-api-proof.sh")
	}
	if chromeBin == "" {
		for _, candidate := range []string{"google-chrome", "google-chrome-stable", "chromium", "chromium-browser"} {
			if path, err := exec.LookPath(candidate); err == nil {
				chromeBin = path
				break
			}
		}
	}
	if chromeBin == "" {
		t.Skip("no headless Chrome/Chromium binary available")
	}
	pagePath := filepath.Join(assetsDir, "test-harness", "p05-browser-persistence-smoke.html")
	if _, err := os.Stat(pagePath); err != nil {
		t.Fatalf("browser harness asset missing: %s: %v", pagePath, err)
	}

	pool := openTestDB(t)
	var input struct {
		BrowserFileID string `json:"browserFileId"`
		BrowserName   string `json:"browserName"`
	}
	inputBytes, err := os.ReadFile(updatePath)
	if err != nil {
		t.Fatalf("read prepared browser fixture: %v", err)
	}
	if err := json.Unmarshal(inputBytes, &input); err != nil {
		t.Fatalf("decode prepared browser fixture: %v", err)
	}
	if input.BrowserFileID == "" || input.BrowserName == "" {
		t.Fatal("prepared fixture omitted browserFileId or browserName")
	}

	tokensKey, err := auth.DeriveTokensKey("p0.5-disposable-browser-boundary-test-key")
	if err != nil {
		t.Fatalf("derive test session key: %v", err)
	}
	profileID := createTestProfile(t, pool)
	_, projectID := createTestTeamAndProject(t, pool, profileID)
	token, sessionID, err := auth.CreateSession(context.Background(), pool, profileID, "P0.5 browser persistence test", tokensKey)
	if err != nil {
		t.Fatalf("create browser test session: %v", err)
	}
	t.Cleanup(func() {
		if err := auth.DeleteSession(context.Background(), pool, sessionID); err != nil {
			t.Errorf("delete browser test session: %v", err)
		}
	})

	apiHandler := logosserver.New(logosserver.Deps{
		Pool:       pool,
		AuthMW:     auth.NewMiddleware(pool, tokensKey, "auth-token"),
		TokensKey:  tokensKey,
		CookieName: "auth-token",
	})
	httpHandler := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/__test/session":
			http.SetCookie(w, &http.Cookie{
				Name: "auth-token", Value: token, Path: "/", HttpOnly: true,
				SameSite: http.SameSiteLaxMode,
			})
			http.Redirect(w, r, "/__test/p05-browser-persistence-smoke.html", http.StatusFound)
			return
		case "/__test/config":
			if cookie, err := r.Cookie("auth-token"); err != nil || cookie.Value != token {
				http.Error(w, "browser session cookie missing", http.StatusUnauthorized)
				return
			}
			w.Header().Set("Content-Type", "application/json")
			_ = json.NewEncoder(w).Encode(map[string]string{"fileId": input.BrowserFileID})
			return
		case "/__test/p05-browser-persistence-smoke.html":
			http.ServeFile(w, r, pagePath)
			return
		}
		if strings.HasPrefix(r.URL.Path, "/assets/") {
			http.StripPrefix("/assets/", http.FileServer(http.Dir(filepath.Join(assetsDir, "assets")))).ServeHTTP(w, r)
			return
		}
		apiHandler.ServeHTTP(w, r)
	})

	createServer := httptest.NewServer(apiHandler)
	createBody, err := json.Marshal(map[string]string{
		"id": input.BrowserFileID, "projectId": projectID, "name": input.BrowserName,
	})
	if err != nil {
		createServer.Close()
		t.Fatalf("encode browser create-file request: %v", err)
	}
	createReq, err := http.NewRequest(http.MethodPost, createServer.URL+"/api/rpc/command/create-file", strings.NewReader(string(createBody)))
	if err != nil {
		createServer.Close()
		t.Fatalf("create browser test file request: %v", err)
	}
	createReq.Header.Set("Content-Type", "application/json")
	createReq.AddCookie(&http.Cookie{Name: "auth-token", Value: token})
	createResponse, err := createServer.Client().Do(createReq)
	if err != nil {
		createServer.Close()
		t.Fatalf("create browser test file: %v", err)
	}
	createResponse.Body.Close()
	if createResponse.StatusCode != http.StatusOK {
		createServer.Close()
		t.Fatalf("create browser test file status=%d", createResponse.StatusCode)
	}
	fixturePath := filepath.Join("..", "..", "..", "test", "fixtures", "p0-document-compat", "source-document.json")
	fixtureBytes, err := os.ReadFile(fixturePath)
	if err != nil {
		createServer.Close()
		t.Fatalf("read browser compatibility fixture: %v", err)
	}
	var fixture map[string]any
	if err := json.Unmarshal(fixtureBytes, &fixture); err != nil {
		createServer.Close()
		t.Fatalf("decode browser compatibility fixture: %v", err)
	}
	fixture["id"] = input.BrowserFileID
	fixtureBytes, err = json.Marshal(fixture)
	if err != nil {
		createServer.Close()
		t.Fatalf("encode browser compatibility fixture: %v", err)
	}
	if _, err := pool.Exec(context.Background(), `UPDATE file SET data = $1 WHERE id = $2`, fixtureBytes, input.BrowserFileID); err != nil {
		createServer.Close()
		t.Fatalf("seed browser disposable file: %v", err)
	}
	createServer.Close()

	browserServer := httptest.NewServer(httpHandler)
	defer browserServer.Close()
	ctx, cancel := context.WithTimeout(context.Background(), 45*time.Second)
	defer cancel()
	profileDir := t.TempDir()
	command := exec.CommandContext(ctx, chromeBin,
		"--headless=new", "--no-sandbox", "--disable-gpu", "--disable-dev-shm-usage",
		"--no-first-run", "--no-default-browser-check", "--user-data-dir="+profileDir,
		"--virtual-time-budget=30000", "--dump-dom", browserServer.URL+"/__test/session",
	)
	output, err := command.CombinedOutput()
	if evidencePath := os.Getenv("P05_BROWSER_EVIDENCE_OUT"); evidencePath != "" {
		if writeErr := os.WriteFile(evidencePath, output, 0o600); writeErr != nil {
			t.Errorf("write headless browser evidence: %v", writeErr)
		}
	}
	if err != nil {
		t.Fatalf("headless browser failed: %v\n%s", err, output)
	}
	if !strings.Contains(string(output), `id="p05-status" data-status="passed"`) {
		t.Fatalf("browser persistence smoke did not pass:\n%s", output)
	}
	t.Logf("headless browser DOM evidence:\n%s", output)
}
