// Package handler — audit log handlers.
//
// Ported from app.rpc.commands.audit in the Clojure backend.
//
// # push-audit-events
//
// The frontend submits batches of user-action events to be stored in audit_log.
// This handler is the write path; there is no public RPC read/pagination endpoint.
//
// Feature flag: LOGOS_ENABLE_AUDIT_LOG (default false).
// When disabled, the handler accepts the request but does nothing, matching the
// Clojure behaviour (events are silently dropped when :audit-log is not enabled).
//
// # audit_log table schema
//
//	id uuid, name text, type text, created_at timestamptz, archived_at timestamptz,
//	profile_id uuid, props jsonb, ip_addr inet, tracked_at timestamptz,
//	source text, context jsonb
//
// # Timestamp skew
//
// If the event's tracked_at is more than 1 hour ahead of the server's now(),
// we pin tracked_at to now() and stash the original value in context.original-tracked-at.
// (Mirrors Clojure's skew guard.)
package handler

import (
	"encoding/json"
	"net"
	"net/http"
	"os"
	"time"

	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/db"
)

// AuditEventType values match the Clojure enum.
const (
	AuditTypeAction   = "action"
	AuditTypeIdentify = "identify"
	AuditTypeTrigger  = "trigger"
)

// AuditEvent is one element in the push-audit-events payload.
type AuditEvent struct {
	Name      string          `json:"name"`
	Type      string          `json:"type"`
	TrackedAt *time.Time      `json:"timestamp,omitempty"`
	Props     json.RawMessage `json:"props,omitempty"`
	Context   json.RawMessage `json:"context,omitempty"`
}

type pushAuditEventsParams struct {
	Events []AuditEvent `json:"events"`
}

// PushAuditEventsHandler implements POST /api/rpc/command/push-audit-events.
func PushAuditEventsHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
			return
		}

		var params pushAuditEventsParams
		if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
			// Match Logos: accept and ignore malformed batches when logging is off.
			if os.Getenv("LOGOS_ENABLE_AUDIT_LOG") != "true" {
				writeJSON(w, http.StatusOK, map[string]any{})
				return
			}
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}

		// When audit-log is disabled, silently accept and return.
		if os.Getenv("LOGOS_ENABLE_AUDIT_LOG") != "true" {
			writeJSON(w, http.StatusOK, map[string]any{})
			return
		}

		if len(params.Events) == 0 {
			writeJSON(w, http.StatusOK, map[string]any{})
			return
		}

		// Parse client IP from common proxy headers.
		ipStr := r.Header.Get("X-Real-IP")
		if ipStr == "" {
			ipStr = r.Header.Get("X-Forwarded-For")
		}
		if ipStr == "" {
			ipStr, _, _ = net.SplitHostPort(r.RemoteAddr)
		}
		// Validate the IP; use null if unparseable.
		if net.ParseIP(ipStr) == nil {
			ipStr = ""
		}

		now := time.Now().UTC()
		const skewLimit = time.Hour

		tx, err := pool.Begin(r.Context())
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		defer tx.Rollback(r.Context()) //nolint:errcheck

		for _, evt := range params.Events {
			evtType := evt.Type
			if evtType == "" {
				evtType = AuditTypeAction
			}

			trackedAt := now
			ctxData := evt.Context
			if evt.TrackedAt != nil {
				ta := evt.TrackedAt.UTC()
				if ta.Sub(now) > skewLimit {
					// Event tracked_at is too far in the future — pin to server time
					// and record the original in context.
					adjusted, _ := mergeJSONKey(ctxData, "original-tracked-at", ta.Format(time.RFC3339Nano))
					ctxData = adjusted
				} else {
					trackedAt = ta
				}
			}

			propsJSON := []byte(`{}`)
			if len(evt.Props) > 0 {
				propsJSON = evt.Props
			}
			ctxJSON := []byte(`{}`)
			if len(ctxData) > 0 {
				ctxJSON = ctxData
			}

			id := newUUID()
			var ipParam any = nil
			if ipStr != "" {
				ipParam = ipStr
			}

			_, _ = tx.Exec(r.Context(),
				`INSERT INTO audit_log
				   (id, name, type, profile_id, source,
				    created_at, tracked_at, ip_addr, props, context)
				 VALUES ($1, $2, $3, $4, 'frontend',
				         $5,  $6,  $7::inet, $8::jsonb, $9::jsonb)`,
				id, evt.Name, evtType, profileID,
				now, trackedAt, ipParam, propsJSON, ctxJSON,
			)
		}

		if err = tx.Commit(r.Context()); err != nil {
			writeError(w, http.StatusInternalServerError, "commit failed")
			return
		}

		writeJSON(w, http.StatusOK, map[string]any{})
	}
}

// mergeJSONKey returns a JSON object with the given key added.
// If src is not a valid JSON object, a new one is returned.
func mergeJSONKey(src json.RawMessage, key string, val any) (json.RawMessage, error) {
	m := make(map[string]any)
	if len(src) > 0 {
		_ = json.Unmarshal(src, &m)
	}
	m[key] = val
	return json.Marshal(m)
}
