package handler

import (
	"net/http"
	"os"
	"strings"
)

// GetEnabledFlagsHandler implements GET/POST /api/rpc/command/get-enabled-flags.
//
// Returns the subset of server feature flags that enable frontend audit/telemetry
// collection. When neither flag is active the frontend skips push-audit-events.
func GetEnabledFlagsHandler() http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		flags := enabledAuditFlags()
		writeJSON(w, http.StatusOK, flags)
	}
}

func enabledAuditFlags() []string {
	raw := os.Getenv("LOGOS_FLAGS")
	if raw == "" {
		raw = os.Getenv("PENPOT_FLAGS")
	}

	var out []string
	for _, f := range strings.Fields(raw) {
		switch f {
		case "audit-log", "telemetry":
			out = append(out, f)
		}
	}
	if out == nil {
		out = []string{}
	}
	return out
}
