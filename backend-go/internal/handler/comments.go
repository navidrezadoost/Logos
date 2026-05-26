// Package handler — comment thread and comment handlers.
//
// Ported from app.rpc.commands.comments in the Clojure backend.
// Covers all comment-thread and comment CRUD operations used by the canvas editor.
//
// Tables involved:
//   comment_thread  — one thread anchors to a canvas position/page/frame.
//   comment         — individual messages in a thread.
//   comment_thread_status — per-profile read cursor.
//   file            — comment_thread_seqn counter (locked FOR UPDATE).
//
// Participants and mentions
// ─────────────────────────
// The comment_thread.participants column is JSONB (array of UUID strings).
// The comment_thread.mentions and comment.mentions columns are uuid[].
// Both are stored/read as JSON arrays of UUID strings in this implementation.
//
// Canvas position
// ───────────────
// The position is a PostgreSQL `point` type stored as "(x,y)".
// It is exposed to the API as {x: float, y: float}.
package handler

import (
	"encoding/json"
	"fmt"
	"net/http"
	"strings"
	"time"

	"github.com/go-chi/chi/v5"
	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/db"
	"github.com/logos-design/logos/backend-go/internal/perms"
)

// ─── Wire types ───────────────────────────────────────────────────────────────

// Point is a 2D canvas position.
type Point struct {
	X float64 `json:"x"`
	Y float64 `json:"y"`
}

// CommentThread is the public view of a comment_thread row.
type CommentThread struct {
	ID          string    `json:"id"`
	FileID      string    `json:"fileId"`
	PageID      string    `json:"pageId"`
	PageName    string    `json:"pageName,omitempty"`
	OwnerID     string    `json:"ownerId"`
	FrameID     *string   `json:"frameId,omitempty"`
	Position    Point     `json:"position"`
	Seqn        int       `json:"seqn"`
	IsResolved  bool      `json:"isResolved"`
	Participants []string `json:"participants"`
	CreatedAt   time.Time `json:"createdAt"`
	ModifiedAt  time.Time `json:"modifiedAt"`
}

// Comment is the public view of a comment row.
type Comment struct {
	ID        string    `json:"id"`
	ThreadID  string    `json:"threadId"`
	OwnerID   string    `json:"ownerId"`
	Content   string    `json:"content"`
	CreatedAt time.Time `json:"createdAt"`
	ModifiedAt time.Time `json:"modifiedAt"`
}

// ─── GET /api/rpc/command/get-comment-threads ─────────────────────────────────

type getCommentThreadsParams struct {
	FileID string `json:"fileId"`
}

// GetCommentThreadsHandler implements GET /api/rpc/command/get-comment-threads.
func GetCommentThreadsHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		fileID := r.URL.Query().Get("file-id")
		if fileID == "" {
			fileID = r.URL.Query().Get("fileId")
		}
		if fileID == "" {
			writeError(w, http.StatusUnprocessableEntity, "file-id is required")
			return
		}

		// Require at least viewer access.
		fp, err := perms.GetFilePermissions(r.Context(), pool, profileID, fileID)
		if err != nil || fp == nil {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		rows, err := pool.Query(r.Context(),
			`SELECT id::text, file_id::text, page_id::text,
			        COALESCE(page_name, ''), owner_id::text,
			        frame_id::text, position::text,
			        seqn, is_resolved,
			        COALESCE(participants::text, '[]'),
			        created_at, modified_at
			   FROM comment_thread
			  WHERE file_id = $1
			  ORDER BY seqn ASC`,
			fileID,
		)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		defer rows.Close()

		threads := make([]CommentThread, 0)
		for rows.Next() {
			t, err := scanThread(rows)
			if err != nil {
				continue
			}
			threads = append(threads, t)
		}

		writeJSON(w, http.StatusOK, threads)
	}
}

// ─── GET /api/rpc/command/get-comments ───────────────────────────────────────

// GetCommentsHandler implements GET /api/rpc/command/get-comments.
func GetCommentsHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		threadID := r.URL.Query().Get("thread-id")
		if threadID == "" {
			threadID = chi.URLParam(r, "threadId")
		}
		if threadID == "" {
			writeError(w, http.StatusUnprocessableEntity, "thread-id is required")
			return
		}

		// Resolve file for permission check.
		var fileID string
		if err := pool.QueryRow(r.Context(),
			`SELECT file_id::text FROM comment_thread WHERE id = $1`, threadID,
		).Scan(&fileID); err != nil {
			writeError(w, http.StatusNotFound, "thread not found")
			return
		}

		fp, err := perms.GetFilePermissions(r.Context(), pool, profileID, fileID)
		if err != nil || fp == nil {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		rows, err := pool.Query(r.Context(),
			`SELECT id::text, thread_id::text, owner_id::text,
			        content, created_at, modified_at
			   FROM comment
			  WHERE thread_id = $1
			  ORDER BY created_at ASC`,
			threadID,
		)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		defer rows.Close()

		comments := make([]Comment, 0)
		for rows.Next() {
			var c Comment
			if err := rows.Scan(&c.ID, &c.ThreadID, &c.OwnerID,
				&c.Content, &c.CreatedAt, &c.ModifiedAt); err != nil {
				continue
			}
			comments = append(comments, c)
		}

		writeJSON(w, http.StatusOK, comments)
	}
}

// ─── POST /api/rpc/command/create-comment-thread ─────────────────────────────

type createCommentThreadParams struct {
	FileID   string  `json:"fileId"`
	PageID   string  `json:"pageId"`
	PageName string  `json:"pageName"`
	FrameID  *string `json:"frameId,omitempty"`
	Position Point   `json:"position"`
	Content  string  `json:"content"`
}

// CreateCommentThreadHandler implements POST /api/rpc/command/create-comment-thread.
//
// Creates the thread + first comment in a single transaction, incrementing
// the file's comment_thread_seqn under a FOR UPDATE lock on the file row.
func CreateCommentThreadHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeError(w, http.StatusUnauthorized, "unauthorized")
			return
		}

		var params createCommentThreadParams
		if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}
		if params.FileID == "" || params.PageID == "" || params.Content == "" {
			writeError(w, http.StatusUnprocessableEntity, "fileId, pageId and content are required")
			return
		}

		fp, err := perms.GetFilePermissions(r.Context(), pool, profileID, params.FileID)
		if err != nil || fp == nil {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		tx, err := pool.Begin(r.Context())
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		defer tx.Rollback(r.Context()) //nolint:errcheck

		// Lock file row and get the current seqn, then increment it.
		var seqn int
		if err = tx.QueryRow(r.Context(),
			`UPDATE file
			    SET comment_thread_seqn = comment_thread_seqn + 1, modified_at = now()
			  WHERE id = $1
			  RETURNING comment_thread_seqn`,
			params.FileID,
		).Scan(&seqn); err != nil {
			writeError(w, http.StatusInternalServerError, "seqn increment failed")
			return
		}

		threadID := newUUID()
		commentID := newUUID()
		now := time.Now().UTC()
		participants := `["` + profileID + `"]`

		posStr := fmt.Sprintf("(%f,%f)", params.Position.X, params.Position.Y)

		if _, err = tx.Exec(r.Context(),
			`INSERT INTO comment_thread
			   (id, file_id, page_id, page_name, owner_id, frame_id,
			    position, seqn, is_resolved, participants, created_at, modified_at)
			 VALUES ($1, $2, $3, $4, $5, $6,
			    $7::point, $8, false, $9::jsonb, $10, $10)`,
			threadID, params.FileID, params.PageID, params.PageName, profileID, params.FrameID,
			posStr, seqn, participants, now,
		); err != nil {
			writeError(w, http.StatusInternalServerError, "thread insert failed")
			return
		}

		if _, err = tx.Exec(r.Context(),
			`INSERT INTO comment (id, thread_id, owner_id, content, created_at, modified_at)
			 VALUES ($1, $2, $3, $4, $5, $5)`,
			commentID, threadID, profileID, params.Content, now,
		); err != nil {
			writeError(w, http.StatusInternalServerError, "comment insert failed")
			return
		}

		if err = tx.Commit(r.Context()); err != nil {
			writeError(w, http.StatusInternalServerError, "commit failed")
			return
		}

		writeJSON(w, http.StatusOK, CommentThread{
			ID:           threadID,
			FileID:       params.FileID,
			PageID:       params.PageID,
			PageName:     params.PageName,
			OwnerID:      profileID,
			FrameID:      params.FrameID,
			Position:     params.Position,
			Seqn:         seqn,
			IsResolved:   false,
			Participants: []string{profileID},
			CreatedAt:    now,
			ModifiedAt:   now,
		})
	}
}

// ─── POST /api/rpc/command/create-comment ────────────────────────────────────

type createCommentParams struct {
	ThreadID string   `json:"threadId"`
	Content  string   `json:"content"`
	Mentions []string `json:"mentions,omitempty"`
}

// CreateCommentHandler implements POST /api/rpc/command/create-comment.
func CreateCommentHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeError(w, http.StatusUnauthorized, "unauthorized")
			return
		}

		var params createCommentParams
		if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}
		if params.ThreadID == "" || params.Content == "" {
			writeError(w, http.StatusUnprocessableEntity, "threadId and content are required")
			return
		}

		// Resolve file for permission check.
		var fileID string
		if err := pool.QueryRow(r.Context(),
			`SELECT file_id::text FROM comment_thread WHERE id = $1`, params.ThreadID,
		).Scan(&fileID); err != nil {
			writeError(w, http.StatusNotFound, "thread not found")
			return
		}

		fp, err := perms.GetFilePermissions(r.Context(), pool, profileID, fileID)
		if err != nil || fp == nil {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		commentID := newUUID()
		now := time.Now().UTC()

		if _, err = pool.Exec(r.Context(),
			`INSERT INTO comment (id, thread_id, owner_id, content, created_at, modified_at)
			 VALUES ($1, $2, $3, $4, $5, $5)`,
			commentID, params.ThreadID, profileID, params.Content, now,
		); err != nil {
			writeError(w, http.StatusInternalServerError, "comment insert failed")
			return
		}

		// Update thread participants (add profileID if not already there).
		_, _ = pool.Exec(r.Context(),
			`UPDATE comment_thread
			    SET participants = (
			        SELECT jsonb_agg(DISTINCT val)
			        FROM jsonb_array_elements(
			            COALESCE(participants, '[]'::jsonb) || $2::jsonb
			        ) val
			    ),
			    modified_at = $3
			  WHERE id = $1`,
			params.ThreadID,
			`["`+profileID+`"]`,
			now,
		)

		writeJSON(w, http.StatusOK, Comment{
			ID:         commentID,
			ThreadID:   params.ThreadID,
			OwnerID:    profileID,
			Content:    params.Content,
			CreatedAt:  now,
			ModifiedAt: now,
		})
	}
}

// ─── PATCH /api/rpc/command/update-comment ────────────────────────────────────

type updateCommentParams struct {
	ID      string `json:"id"`
	Content string `json:"content"`
}

// UpdateCommentHandler implements PATCH /api/rpc/command/update-comment.
func UpdateCommentHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeError(w, http.StatusUnauthorized, "unauthorized")
			return
		}

		var params updateCommentParams
		if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}

		tag, err := pool.Exec(r.Context(),
			`UPDATE comment SET content = $1, modified_at = now()
			  WHERE id = $2 AND owner_id = $3`,
			params.Content, params.ID, profileID,
		)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		if tag.RowsAffected() == 0 {
			writeError(w, http.StatusForbidden, "not-allowed")
			return
		}

		writeJSON(w, http.StatusOK, map[string]any{})
	}
}

// ─── PATCH /api/rpc/command/update-comment-thread ────────────────────────────

type updateCommentThreadParams struct {
	ID         string  `json:"id"`
	IsResolved *bool   `json:"isResolved,omitempty"`
	FrameID    *string `json:"frameId,omitempty"`
}

// UpdateCommentThreadHandler implements PATCH /api/rpc/command/update-comment-thread.
func UpdateCommentThreadHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeError(w, http.StatusUnauthorized, "unauthorized")
			return
		}

		var params updateCommentThreadParams
		if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}

		// Resolve file for permission check.
		var fileID string
		if err := pool.QueryRow(r.Context(),
			`SELECT file_id::text FROM comment_thread WHERE id = $1`, params.ID,
		).Scan(&fileID); err != nil {
			writeError(w, http.StatusNotFound, "thread not found")
			return
		}

		fp, err := perms.GetFilePermissions(r.Context(), pool, profileID, fileID)
		if err != nil || fp == nil {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		if params.IsResolved != nil {
			if _, err = pool.Exec(r.Context(),
				`UPDATE comment_thread SET is_resolved = $1, modified_at = now() WHERE id = $2`,
				*params.IsResolved, params.ID,
			); err != nil {
				writeError(w, http.StatusInternalServerError, "internal server error")
				return
			}
		}

		writeJSON(w, http.StatusOK, map[string]any{})
	}
}

// ─── DELETE /api/rpc/command/delete-comment ──────────────────────────────────

type deleteCommentParams struct {
	ID string `json:"id"`
}

// DeleteCommentHandler implements DELETE /api/rpc/command/delete-comment.
func DeleteCommentHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeError(w, http.StatusUnauthorized, "unauthorized")
			return
		}

		var params deleteCommentParams
		if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}

		tag, err := pool.Exec(r.Context(),
			`DELETE FROM comment WHERE id = $1 AND owner_id = $2`,
			params.ID, profileID,
		)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		if tag.RowsAffected() == 0 {
			writeError(w, http.StatusForbidden, "not-allowed")
			return
		}

		writeJSON(w, http.StatusOK, map[string]any{})
	}
}

// ─── DELETE /api/rpc/command/delete-comment-thread ───────────────────────────

type deleteCommentThreadParams struct {
	ID string `json:"id"`
}

// DeleteCommentThreadHandler implements DELETE /api/rpc/command/delete-comment-thread.
func DeleteCommentThreadHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeError(w, http.StatusUnauthorized, "unauthorized")
			return
		}

		var params deleteCommentThreadParams
		if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}

		// Only the owner can delete the thread.
		tag, err := pool.Exec(r.Context(),
			`DELETE FROM comment_thread WHERE id = $1 AND owner_id = $2`,
			params.ID, profileID,
		)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		if tag.RowsAffected() == 0 {
			writeError(w, http.StatusForbidden, "not-allowed")
			return
		}

		writeJSON(w, http.StatusOK, map[string]any{})
	}
}

// ─── PATCH /api/rpc/command/update-comment-thread-status ─────────────────────

type updateThreadStatusParams struct {
	ID string `json:"id"`
}

// UpdateCommentThreadStatusHandler implements PATCH /api/rpc/command/update-comment-thread-status.
//
// Marks the thread as read for the current profile (upserts a status row).
func UpdateCommentThreadStatusHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeError(w, http.StatusUnauthorized, "unauthorized")
			return
		}

		var params updateThreadStatusParams
		if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}

		now := time.Now().UTC()
		_, err := pool.Exec(r.Context(),
			`INSERT INTO comment_thread_status (thread_id, profile_id, modified_at)
			 VALUES ($1, $2, $3)
			 ON CONFLICT (thread_id, profile_id)
			 DO UPDATE SET modified_at = EXCLUDED.modified_at`,
			params.ID, profileID, now,
		)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}

		writeJSON(w, http.StatusOK, map[string]string{"modifiedAt": now.Format(time.RFC3339)})
	}
}

// ─── helpers ─────────────────────────────────────────────────────────────────

// scanRow is the minimal interface needed by scanThread.
type scanRow interface {
	Scan(dest ...any) error
}

func scanThread(row scanRow) (CommentThread, error) {
	var t CommentThread
	var posStr string
	var participantsJSON string

	err := row.Scan(
		&t.ID, &t.FileID, &t.PageID, &t.PageName,
		&t.OwnerID, &t.FrameID,
		&posStr,
		&t.Seqn, &t.IsResolved,
		&participantsJSON,
		&t.CreatedAt, &t.ModifiedAt,
	)
	if err != nil {
		return t, err
	}

	t.Position = parsePoint(posStr)

	var ids []string
	if err := json.Unmarshal([]byte(participantsJSON), &ids); err == nil {
		t.Participants = ids
	}
	if t.Participants == nil {
		t.Participants = []string{}
	}

	return t, nil
}

// parsePoint converts PostgreSQL point literal "(x,y)" to a Point.
func parsePoint(s string) Point {
	s = strings.TrimPrefix(s, "(")
	s = strings.TrimSuffix(s, ")")
	parts := strings.SplitN(s, ",", 2)
	if len(parts) != 2 {
		return Point{}
	}
	var p Point
	fmt.Sscanf(parts[0], "%f", &p.X)
	fmt.Sscanf(parts[1], "%f", &p.Y)
	return p
}
