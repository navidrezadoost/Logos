// Package handler — outbound webhook CRUD and async dispatcher.
//
// Ported from app.rpc.commands.webhooks in the Clojure backend.
//
// # Tables
//
//	webhook          — configuration (id, team_id, uri, mtype, is_active, …)
//	webhook_delivery — delivery log (webhook_id, created_at, req_data, rsp_data, error_code)
//
// # Delivery
//
// Events are dispatched asynchronously from a goroutine. The payload is a
// JSON object carrying `type`, `file-id`, `team-id`, and event-specific
// fields.  Each dispatch:
//  1. Loads all active webhooks for the team.
//  2. For each webhook: POST the event body; log to webhook_delivery.
//  3. Increments error_count on failure; deactivates webhook after 3 errors.
//
// DispatchEvent is the entry point for other handlers (e.g. files_update).
// It is safe to call concurrently from goroutines.
//
// Max webhooks per team: 8 (matches Clojure backend).
package handler

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"log"
	"net/http"
	"time"

	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/db"
)

const maxWebhooksPerTeam = 8

// Webhook is the public view of a webhook row.
type Webhook struct {
	ID         string    `json:"id"`
	TeamID     string    `json:"teamId"`
	URI        string    `json:"uri"`
	Mtype      string    `json:"mtype"`
	IsActive   bool      `json:"isActive"`
	ErrorCode  *string   `json:"errorCode,omitempty"`
	ErrorCount int       `json:"errorCount"`
	CreatedAt  time.Time `json:"createdAt"`
	UpdatedAt  time.Time `json:"updatedAt"`
}

// ─── GET /api/rpc/command/get-webhooks ───────────────────────────────────────

// GetWebhooksHandler implements GET /api/rpc/command/get-webhooks.
func GetWebhooksHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		teamID := r.URL.Query().Get("team-id")
		if teamID == "" {
			teamID = r.URL.Query().Get("teamId")
		}
		if teamID == "" {
			writeError(w, http.StatusUnprocessableEntity, "team-id is required")
			return
		}

		if !teamMember(r.Context(), pool, profileID, teamID) {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		rows, err := pool.Query(r.Context(),
			`SELECT id::text, team_id::text, uri, mtype, is_active,
			        error_code, error_count, created_at, updated_at
			   FROM webhook
			  WHERE team_id = $1
			  ORDER BY uri`,
			teamID,
		)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		defer rows.Close()

		webhooks := make([]Webhook, 0)
		for rows.Next() {
			var wh Webhook
			if err := rows.Scan(&wh.ID, &wh.TeamID, &wh.URI, &wh.Mtype, &wh.IsActive,
				&wh.ErrorCode, &wh.ErrorCount, &wh.CreatedAt, &wh.UpdatedAt); err != nil {
				continue
			}
			webhooks = append(webhooks, wh)
		}

		writeJSON(w, http.StatusOK, webhooks)
	}
}

// ─── POST /api/rpc/command/create-webhook ────────────────────────────────────

type createWebhookParams struct {
	TeamID string `json:"teamId"`
	URI    string `json:"uri"`
	Mtype  string `json:"mtype"`
}

// CreateWebhookHandler implements POST /api/rpc/command/create-webhook.
// Validates the URI with an HTTP HEAD request before inserting.
func CreateWebhookHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeError(w, http.StatusUnauthorized, "unauthorized")
			return
		}

		var params createWebhookParams
		if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}
		if params.TeamID == "" || params.URI == "" {
			writeError(w, http.StatusUnprocessableEntity, "teamId and uri are required")
			return
		}
		if params.Mtype == "" {
			params.Mtype = "application/json"
		}

		if !teamMember(r.Context(), pool, profileID, params.TeamID) {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		// Enforce max-8 limit.
		var count int
		_ = pool.QueryRow(r.Context(),
			`SELECT COUNT(*) FROM webhook WHERE team_id = $1`, params.TeamID,
		).Scan(&count)
		if count >= maxWebhooksPerTeam {
			writeError(w, http.StatusUnprocessableEntity, "too-many-webhooks")
			return
		}

		// HEAD validation.
		if err := validateWebhookURI(params.URI); err != nil {
			writeError(w, http.StatusUnprocessableEntity, "webhook-invalid-uri")
			return
		}

		webhookID := newUUID()
		now := time.Now().UTC()

		if _, err := pool.Exec(r.Context(),
			`INSERT INTO webhook (id, team_id, profile_id, uri, mtype, is_active, created_at, updated_at)
			 VALUES ($1, $2, $3, $4, $5, true, $6, $6)`,
			webhookID, params.TeamID, profileID, params.URI, params.Mtype, now,
		); err != nil {
			writeError(w, http.StatusInternalServerError, "db insert failed")
			return
		}

		writeJSON(w, http.StatusOK, Webhook{
			ID:        webhookID,
			TeamID:    params.TeamID,
			URI:       params.URI,
			Mtype:     params.Mtype,
			IsActive:  true,
			CreatedAt: now,
			UpdatedAt: now,
		})
	}
}

// ─── PATCH /api/rpc/command/update-webhook ───────────────────────────────────

type updateWebhookParams struct {
	ID       string `json:"id"`
	URI      string `json:"uri"`
	Mtype    string `json:"mtype"`
	IsActive *bool  `json:"isActive,omitempty"`
}

// UpdateWebhookHandler implements PATCH /api/rpc/command/update-webhook.
func UpdateWebhookHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeError(w, http.StatusUnauthorized, "unauthorized")
			return
		}

		var params updateWebhookParams
		if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}
		if params.ID == "" {
			writeError(w, http.StatusUnprocessableEntity, "id is required")
			return
		}

		// Verify ownership or team edit rights.
		var teamID string
		if err := pool.QueryRow(r.Context(),
			`SELECT team_id::text FROM webhook WHERE id = $1`, params.ID,
		).Scan(&teamID); err != nil {
			writeError(w, http.StatusNotFound, "webhook not found")
			return
		}
		if !teamMember(r.Context(), pool, profileID, teamID) {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		isActive := true
		if params.IsActive != nil {
			isActive = *params.IsActive
		}

		if _, err := pool.Exec(r.Context(),
			`UPDATE webhook
			    SET uri = $1, mtype = $2, is_active = $3,
			        error_code = NULL, error_count = 0, updated_at = now()
			  WHERE id = $4`,
			params.URI, params.Mtype, isActive, params.ID,
		); err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}

		writeJSON(w, http.StatusOK, map[string]any{})
	}
}

// ─── DELETE /api/rpc/command/delete-webhook ──────────────────────────────────

type deleteWebhookParams struct {
	ID string `json:"id"`
}

// DeleteWebhookHandler implements DELETE /api/rpc/command/delete-webhook.
func DeleteWebhookHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeError(w, http.StatusUnauthorized, "unauthorized")
			return
		}

		var params deleteWebhookParams
		if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}

		var teamID string
		if err := pool.QueryRow(r.Context(),
			`SELECT team_id::text FROM webhook WHERE id = $1`, params.ID,
		).Scan(&teamID); err != nil {
			writeError(w, http.StatusNotFound, "webhook not found")
			return
		}
		if !teamMember(r.Context(), pool, profileID, teamID) {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		_, _ = pool.Exec(r.Context(), `DELETE FROM webhook WHERE id = $1`, params.ID)

		writeJSON(w, http.StatusOK, map[string]any{})
	}
}

// ─── Async event dispatcher ───────────────────────────────────────────────────

// WebhookEvent is a structured event dispatched to registered webhooks.
type WebhookEvent struct {
	Type      string         `json:"type"`
	TeamID    string         `json:"teamId,omitempty"`
	FileID    string         `json:"fileId,omitempty"`
	ProfileID string         `json:"profileId,omitempty"`
	Payload   map[string]any `json:"payload,omitempty"`
	CreatedAt time.Time      `json:"createdAt"`
}

// DispatchEvent sends a webhook event to all active webhooks for the given team.
// It runs asynchronously in a goroutine so callers are never blocked.
// Delivery results are logged to webhook_delivery.
func DispatchEvent(pool *db.Pool, evt WebhookEvent) {
	if pool == nil || evt.TeamID == "" {
		return
	}
	evt.CreatedAt = time.Now().UTC()
	go func() {
		ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
		defer cancel()

		// Load active webhooks for this team.
		rows, err := pool.Query(ctx,
			`SELECT id::text, uri, mtype FROM webhook
			  WHERE team_id = $1 AND is_active = true`,
			evt.TeamID,
		)
		if err != nil {
			return
		}
		defer rows.Close()

		type webhookRow struct{ id, uri, mtype string }
		var hooks []webhookRow
		for rows.Next() {
			var h webhookRow
			if err := rows.Scan(&h.id, &h.uri, &h.mtype); err == nil {
				hooks = append(hooks, h)
			}
		}
		rows.Close()

		for _, h := range hooks {
			deliverWebhook(ctx, pool, h.id, h.uri, h.mtype, evt)
		}
	}()
}

// deliverWebhook makes the HTTP POST for one webhook and records the delivery.
func deliverWebhook(ctx context.Context, pool *db.Pool, webhookID, uri, mtype string, evt WebhookEvent) {
	body, err := json.Marshal(evt)
	if err != nil {
		return
	}

	req, err := http.NewRequestWithContext(ctx, http.MethodPost, uri, bytes.NewReader(body))
	if err != nil {
		return
	}
	req.Header.Set("Content-Type", mtype)
	req.Header.Set("X-Logos-Event", evt.Type)

	client := &http.Client{Timeout: 15 * time.Second}
	resp, err := client.Do(req)

	now := time.Now().UTC()
	var errCode *string
	var rspData []byte

	if err != nil {
		code := "network-error"
		errCode = &code
	} else {
		defer resp.Body.Close()
		rspData, _ = io.ReadAll(io.LimitReader(resp.Body, 4096))
		if resp.StatusCode != 200 && resp.StatusCode != 204 {
			code := resp.Status
			errCode = &code
		}
	}

	reqData, _ := json.Marshal(map[string]any{"uri": uri, "body": string(body)})
	_, _ = pool.Exec(ctx,
		`INSERT INTO webhook_delivery (webhook_id, created_at, error_code, req_data, rsp_data)
		 VALUES ($1, $2, $3, $4, $5)`,
		webhookID, now, errCode,
		reqData, rspData,
	)

	if errCode != nil {
		// Increment error counter; deactivate after 3 consecutive errors.
		_, _ = pool.Exec(ctx,
			`UPDATE webhook
			    SET error_code  = $1,
			        error_count = error_count + 1,
			        is_active   = (error_count + 1 < 3),
			        updated_at  = $2
			  WHERE id = $3`,
			*errCode, now, webhookID,
		)
		log.Printf("[webhook] delivery failure webhook=%s uri=%s error=%s", webhookID, uri, *errCode)
	} else {
		// Clear error state on success.
		_, _ = pool.Exec(ctx,
			`UPDATE webhook SET error_code = NULL, error_count = 0, updated_at = $1 WHERE id = $2`,
			now, webhookID,
		)
	}
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

// validateWebhookURI sends a HEAD request to verify the URI is reachable.
// Non-fatal: a non-2xx status is still accepted (the endpoint may not support HEAD).
func validateWebhookURI(uri string) error {
	client := &http.Client{Timeout: 5 * time.Second}
	req, err := http.NewRequest(http.MethodHead, uri, nil)
	if err != nil {
		return err
	}
	resp, err := client.Do(req)
	if err != nil {
		return err
	}
	resp.Body.Close()
	return nil
}
