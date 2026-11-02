package transit

import (
	"bytes"
	"io"
	"log"
	"net/http"
	"strings"
)

// captureWriter intercepts handler writes so the middleware can re-encode them.
type captureWriter struct {
	http.ResponseWriter
	body   bytes.Buffer
	status int
}

func (c *captureWriter) WriteHeader(status int) { c.status = status }
func (c *captureWriter) Write(b []byte) (int, error) {
	return c.body.Write(b)
}

// Middleware adapts the HTTP layer between the ClojureScript frontend (Transit+JSON)
// and the Go handlers (plain JSON):
//   - Incoming POST/PATCH bodies with Content-Type application/transit+json are
//     decoded to plain JSON before reaching the handler.
//   - Outgoing JSON responses are re-encoded as Transit+JSON when the client
//     sends Accept: application/transit+json.
func Middleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		// ── Decode incoming Transit body ──────────────────────────────────────
		ct := r.Header.Get("Content-Type")
		if strings.Contains(ct, "application/transit+json") {
			body, err := io.ReadAll(r.Body)
			_ = r.Body.Close()
			if err == nil && len(body) > 0 {
				if jsonBody, err := TransitToJSON(body); err == nil {
					r.Body = io.NopCloser(bytes.NewReader(jsonBody))
					r.ContentLength = int64(len(jsonBody))
					r.Header.Set("Content-Type", "application/json")
				}
			} else {
				r.Body = io.NopCloser(bytes.NewReader(body))
			}
		}

		// ── Re-encode response as Transit if requested ────────────────────────
		accept := r.Header.Get("Accept")
		if !strings.Contains(accept, "application/transit+json") {
			next.ServeHTTP(w, r)
			return
		}

		cw := &captureWriter{ResponseWriter: w, status: http.StatusOK}
		next.ServeHTTP(cw, r)

		captured := cw.body.Bytes()
		if len(captured) == 0 && cw.status >= 200 && cw.status < 300 {
			log.Printf("[transit] WARNING empty handler body on %s %s status=%d accept=%q",
				r.Method, r.URL.Path, cw.status, accept)
			captured = []byte(`["^ "]`)
		}

		// Handlers that emit plain string-keyed Transit maps pass them through.
		if IsTransitMapBody(captured) {
			w.Header().Set("Content-Type", "application/transit+json; charset=utf-8")
			w.WriteHeader(cw.status)
			n, err := w.Write(captured)
			log.Printf("[transit] passthrough %s %s status=%d in=%d out=%d err=%v",
				r.Method, r.URL.Path, cw.status, len(captured), n, err)
			return
		}

		transitBody, err := JSONToTransit(captured)
		if err != nil {
			log.Printf("[transit] encode failed on %s %s status=%d in=%d: %v",
				r.Method, r.URL.Path, cw.status, len(captured), err)
			// Fall back to raw JSON so the client at least gets something.
			w.Header().Set("Content-Type", "application/json")
			w.WriteHeader(cw.status)
			_, _ = w.Write(captured)
			return
		}

		w.Header().Set("Content-Type", "application/transit+json; charset=utf-8")
		w.WriteHeader(cw.status)
		n, err := w.Write(transitBody)
		log.Printf("[transit] encoded %s %s status=%d in=%d out=%d err=%v",
			r.Method, r.URL.Path, cw.status, len(captured), n, err)
	})
}
