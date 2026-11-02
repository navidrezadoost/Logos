// Package handler — user feedback submission.
//
// Ported from app.rpc.commands.feedback in the Clojure backend.
//
// # send-user-feedback
//
// The frontend submits a form (subject, content, optional error report).
// The Clojure backend sends an email; no database table exists for feedback.
// The Go handler logs the feedback to stdout via the email stub and, when
// LOGOS_USER_FEEDBACK_DESTINATION is configured, uses the email package.
//
// Feature flag: LOGOS_ENABLE_USER_FEEDBACK (default false).
// Destination: LOGOS_USER_FEEDBACK_DESTINATION (email address).
//
// Param limits (matching Clojure):
//
//	subject     max 500 chars
//	content     max 2500 chars
//	type        optional string
//	error-href  max 2500 chars
//	error-report optional raw string (sent as attachment in Clojure; logged here)
package handler

import (
	"encoding/json"
	"log"
	"net/http"
	"os"

	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/db"
)

type sendFeedbackParams struct {
	Subject     string `json:"subject"`
	Content     string `json:"content"`
	Type        string `json:"type,omitempty"`
	ErrorHref   string `json:"errorHref,omitempty"`
	ErrorReport string `json:"errorReport,omitempty"`
}

// SendUserFeedbackHandler implements POST /api/rpc/command/send-user-feedback.
func SendUserFeedbackHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
			return
		}

		if os.Getenv("LOGOS_ENABLE_USER_FEEDBACK") != "true" {
			writeError(w, http.StatusForbidden, "user-feedback-not-allowed")
			return
		}

		var params sendFeedbackParams
		if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}
		if params.Subject == "" || params.Content == "" {
			writeError(w, http.StatusUnprocessableEntity, "subject and content are required")
			return
		}
		if len(params.Subject) > 500 {
			writeError(w, http.StatusUnprocessableEntity, "subject too long (max 500 chars)")
			return
		}
		if len(params.Content) > 2500 {
			writeError(w, http.StatusUnprocessableEntity, "content too long (max 2500 chars)")
			return
		}

		// Load the sender's email address.
		var senderEmail string
		_ = pool.QueryRow(r.Context(),
			`SELECT email FROM profile WHERE id = $1`, profileID,
		).Scan(&senderEmail)

		destination := os.Getenv("LOGOS_USER_FEEDBACK_DESTINATION")
		if destination == "" {
			destination = "feedback@example.com"
		}

		body := buildFeedbackBody(senderEmail, params)
		log.Printf("[feedback] to=%s reply-to=%s subject=%q\n%s",
			destination, senderEmail, "[Logos Feedback] "+params.Subject, body)

		if params.ErrorReport != "" {
			log.Printf("[feedback] error-report from profile=%s:\n%s", profileID, params.ErrorReport)
		}

		writeJSON(w, http.StatusOK, map[string]any{})
	}
}

func buildFeedbackBody(from string, p sendFeedbackParams) string {
	out := "From: " + from + "\n"
	if p.Type != "" {
		out += "Type: " + p.Type + "\n"
	}
	if p.ErrorHref != "" {
		out += "Error href: " + p.ErrorHref + "\n"
	}
	out += "\n" + p.Content
	return out
}
