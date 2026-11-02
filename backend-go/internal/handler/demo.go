// Package handler — demo user provisioning.
//
// Ported from app.rpc.commands.demo in the Clojure backend.
//
// # create-demo-profile
//
// Creates a disposable demo account that self-destructs after a configurable
// delay (default 30 days).  The handler is intentionally unauthenticated so
// that the landing page can spin up a sandboxed editor without a sign-up flow.
//
// Feature flag: LOGOS_ENABLE_DEMO_USERS (default false).
// Deletion delay: LOGOS_DELETION_DELAY_DAYS (default 30).
//
// Profile fields (matching Clojure):
//
//	email       = "demo-{unix-ms}@example.com"
//	full_name   = "Demo User {unix-ms}"
//	is_demo     = true
//	is_active   = true
//	deleted_at  = now() + deletion-delay
//	password    = bcrypt/argon2id of a random 16-byte base64 token
//
// After creating the profile the handler creates:
//   - A default "My Team" team
//   - A team_profile_rel (owner)
//   - A "Drafts" project in the team
//   - A project_profile_rel
//
// Response: {"email": "…", "password": "…"}
// The client uses these credentials to call login-with-password.
package handler

import (
	"crypto/rand"
	"encoding/base64"
	"fmt"
	"net/http"
	"os"
	"strconv"
	"time"

	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/db"
)

// CreateDemoProfileHandler implements POST /api/rpc/command/create-demo-profile.
func CreateDemoProfileHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		if os.Getenv("LOGOS_ENABLE_DEMO_USERS") != "true" {
			writeError(w, http.StatusForbidden, "demo-users-not-allowed")
			return
		}

		now := time.Now().UTC()
		ts := now.UnixMilli()

		// Generate random 16-byte plain-text password.
		rawPwd := make([]byte, 16)
		if _, err := rand.Read(rawPwd); err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		plainPwd := base64.StdEncoding.EncodeToString(rawPwd)

		hashedPwd, err := auth.DerivePassword(plainPwd)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}

		email := fmt.Sprintf("demo-%d@example.com", ts)
		fullName := fmt.Sprintf("Demo User %d", ts)

		// Parse deletion delay from env (default 30 days).
		deletionDays := 30
		if v := os.Getenv("LOGOS_DELETION_DELAY_DAYS"); v != "" {
			if n, err := strconv.Atoi(v); err == nil && n > 0 {
				deletionDays = n
			}
		}
		deletedAt := now.Add(time.Duration(deletionDays) * 24 * time.Hour)

		tx, err := pool.Begin(r.Context())
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		defer tx.Rollback(r.Context()) //nolint:errcheck

		profileID := newUUID()

		// ── Insert demo profile ────────────────────────────────────────────
		if _, err = tx.Exec(r.Context(),
			`INSERT INTO profile
			   (id, email, full_name, password, is_active, is_demo,
			    created_at, modified_at, deleted_at, props)
			 VALUES ($1, $2, $3, $4, true, true, $5, $5, $6, '{}')`,
			profileID, email, fullName, hashedPwd, now, deletedAt,
		); err != nil {
			writeError(w, http.StatusInternalServerError, "profile insert failed")
			return
		}

		// ── Default team ────────────────────────────────────────────────────
		teamID := newUUID()
		if _, err = tx.Exec(r.Context(),
			`INSERT INTO team (id, name, is_default, created_at, modified_at)
			 VALUES ($1, $2, true, $3, $3)`,
			teamID, fullName+"'s Team", now,
		); err != nil {
			writeError(w, http.StatusInternalServerError, "team insert failed")
			return
		}
		if _, err = tx.Exec(r.Context(),
			`INSERT INTO team_profile_rel
			   (team_id, profile_id, is_owner, is_admin, can_edit, created_at)
			 VALUES ($1, $2, true, true, true, $3)`,
			teamID, profileID, now,
		); err != nil {
			writeError(w, http.StatusInternalServerError, "team_profile_rel insert failed")
			return
		}

		// ── Default project (Drafts) ────────────────────────────────────────
		projectID := newUUID()
		if _, err = tx.Exec(r.Context(),
			`INSERT INTO project
			   (id, team_id, name, is_default, created_at, modified_at)
			 VALUES ($1, $2, 'Drafts', true, $3, $3)`,
			projectID, teamID, now,
		); err != nil {
			writeError(w, http.StatusInternalServerError, "project insert failed")
			return
		}
		if _, err = tx.Exec(r.Context(),
			`INSERT INTO project_profile_rel
			   (project_id, profile_id, is_owner, is_admin, can_edit, created_at)
			 VALUES ($1, $2, true, true, true, $3)`,
			projectID, profileID, now,
		); err != nil {
			writeError(w, http.StatusInternalServerError, "project_profile_rel insert failed")
			return
		}

		if err = tx.Commit(r.Context()); err != nil {
			writeError(w, http.StatusInternalServerError, "commit failed")
			return
		}

		writeJSON(w, http.StatusOK, map[string]string{
			"email":    email,
			"password": plainPwd,
		})
	}
}
