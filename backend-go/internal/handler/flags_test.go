package handler_test

import (
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/logos-design/logos/backend-go/internal/handler"
)

func TestGetEnabledFlagsHandler_Empty(t *testing.T) {
	t.Setenv("LOGOS_FLAGS", "login-with-password disable-email-verification")

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/api/rpc/command/get-enabled-flags", nil)
	handler.GetEnabledFlagsHandler().ServeHTTP(rec, req)

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200", rec.Code)
	}
	if body := rec.Body.String(); body != "[]\n" && body != "[]" {
		t.Fatalf("body = %q, want empty array", body)
	}
}

func TestGetEnabledFlagsHandler_WithTelemetry(t *testing.T) {
	t.Setenv("LOGOS_FLAGS", "telemetry audit-log login-with-password")

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/api/rpc/command/get-enabled-flags", nil)
	handler.GetEnabledFlagsHandler().ServeHTTP(rec, req)

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200", rec.Code)
	}
	body := rec.Body.String()
	if body != "[\"telemetry\",\"audit-log\"]\n" && body != "[\"audit-log\",\"telemetry\"]\n" {
		// order not guaranteed from map iteration — check contains
		if !(containsAll(body, "telemetry", "audit-log")) {
			t.Fatalf("body = %q, want telemetry and audit-log", body)
		}
	}
}

func containsAll(s string, parts ...string) bool {
	for _, p := range parts {
		if !contains(s, p) {
			return false
		}
	}
	return true
}

func contains(s, sub string) bool {
	return len(s) >= len(sub) && (s == sub || len(sub) == 0 || indexOf(s, sub) >= 0)
}

func indexOf(s, sub string) int {
	for i := 0; i+len(sub) <= len(s); i++ {
		if s[i:i+len(sub)] == sub {
			return i
		}
	}
	return -1
}
