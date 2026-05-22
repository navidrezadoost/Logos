// Package handler contains HTTP handlers for the Go backend.
package handler

import (
	"encoding/json"
	"net/http"
)

// Health responds to GET /api/_health with {"status":"ok"}.
func Health(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusOK)
	_ = json.NewEncoder(w).Encode(map[string]string{"status": "ok"})
}
