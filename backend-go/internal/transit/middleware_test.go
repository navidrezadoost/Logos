package transit_test

import (
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/logos-design/logos/backend-go/internal/transit"
)

func TestMiddleware_passesThroughPlainStringMap(t *testing.T) {
	body, _ := transit.EncodePlainStringMap(map[string]string{})
	handler := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusOK)
		w.Write(body)
	})
	mw := transit.Middleware(handler)
	req := httptest.NewRequest(http.MethodGet, "/get-file-object-thumbnails", nil)
	req.Header.Set("Accept", "application/transit+json")
	rec := httptest.NewRecorder()
	mw.ServeHTTP(rec, req)
	if rec.Body.Len() == 0 {
		t.Fatalf("empty body, status=%d headers=%v", rec.Code, rec.Header())
	}
	t.Logf("body=%q", rec.Body.String())
}

func TestMiddleware_writeJSONEmptyStringMap(t *testing.T) {
	handler := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write([]byte("{}\n"))
	})
	mw := transit.Middleware(handler)
	req := httptest.NewRequest(http.MethodGet, "/", nil)
	req.Header.Set("Accept", "application/transit+json")
	rec := httptest.NewRecorder()
	mw.ServeHTTP(rec, req)
	if rec.Body.Len() == 0 {
		t.Fatalf("empty body from writeJSON path, status=%d", rec.Code)
	}
	t.Logf("body=%q", rec.Body.String())
}
