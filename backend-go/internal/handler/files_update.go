// Package handler — file update handler.
//
// Ported from app.rpc.commands.files-update in the Clojure backend.
// Handler: update-file (POST /api/rpc/command/update-file).
//
// Change-set format
// ─────────────────
// The Clojure backend encodes file_change.changes with app.util.blob/encode
// (Transit+zstd).  The Go backend stores plain JSON bytes in the same bytea
// column. When loading competing changes for OT rebase, Go attempts a JSON
// parse; rows encoded by Clojure are skipped (treated as no competition from
// that revision), giving a conservative but correct result.
//
// File CRDT state
// ───────────────
// Incoming change-sets are applied to file.data (internal/changes) and persisted
// on every update, matching the Clojure backend behaviour.
//
// OT rebase
// ─────────
// Pure-Go implementation via internal/rebase.  The logos-rebase Rust crate
// (rust/logos-rebase) has crate-type = ["rlib"] (no C FFI today).  When the
// Rust crate grows a cdylib / .h header, the CGo path can replace this.
//
// Redis broadcast
// ───────────────
// Messages are JSON-encoded and published on:
//   <file-id>               — all changes (legacy / full-file subscribers)
//   <file-id>:page:<page-id> — per-page changes (P2.1)
//   <file-id>:meta           — meta notification for other pages (P2.1)
//   <team-id>               — library changes when file is_shared
package handler

import (
	"context"
	"encoding/json"
	"fmt"
	"log"
	"net/http"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/redis/go-redis/v9"

	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/changes"
	"github.com/logos-design/logos/backend-go/internal/db"
	"github.com/logos-design/logos/backend-go/internal/filedata"
	"github.com/logos-design/logos/backend-go/internal/perms"
	"github.com/logos-design/logos/backend-go/internal/rebase"
)

// libraryChangeTypes mirrors app.rpc.commands.files-update/library-change-types.
var libraryChangeTypes = map[rebase.Type]bool{
	"add-color":           true,
	"mod-color":           true,
	"del-color":           true,
	"add-media":           true,
	"mod-media":           true,
	"del-media":           true,
	"add-component":       true,
	"mod-component":       true,
	"del-component":       true,
	"restore-component":   true,
	"add-typography":      true,
	"mod-typography":      true,
	"del-typography":      true,
}

// ─── Request / response types ─────────────────────────────────────────────────

type updateFileParams struct {
	ID          string           `json:"id"`
	SessionID   string           `json:"session-id"`
	Revn        int64            `json:"revn"`
	Vern        int64            `json:"vern"`
	Changes     []rebase.Change  `json:"changes,omitempty"`
	ChangesMeta []changeWithMeta `json:"changes-with-metadata,omitempty"`
}

type changeWithMeta struct {
	Changes []rebase.Change `json:"changes"`
}

type laggedChangeRow struct {
	ID        string          `json:"id"`
	Revn      int64           `json:"revn"`
	FileID    string          `json:"file-id"`
	SessionID string          `json:"session-id"`
	Changes   []rebase.Change `json:"changes"`
}

type updateFileResponse struct {
	Revn   int64             `json:"revn"`
	Lagged []laggedChangeRow `json:"lagged"`
}

// ─── file row loaded under lock ───────────────────────────────────────────────

type lockedFileRow struct {
	ID        string
	ProjectID string
	TeamID    string
	Revn      int64
	Vern      int64
	IsShared  bool
	Version   int
	Data      []byte
}

// ─── UpdateFileHandler ────────────────────────────────────────────────────────

// UpdateFileHandler implements POST /api/rpc/command/update-file.
//
// Sequence:
//  1. Parse + validate params
//  2. Check file-edit permissions
//  3. Begin transaction; lock the file row FOR UPDATE
//  4. Load file metadata (revn, vern, team, is_shared)
//  5. Validate vern / revn conflicts
//  6. OT rebase if client is behind (base-revn < current revn)
//  7. Insert file_change row
//  8. Increment file.revn; update file.modified_at
//  9. Update project.modified_at
//  10. Commit
//  11. Publish Redis notifications (best-effort)
//  12. Return {revn: old-revn, lagged: [...]}
func UpdateFileHandler(pool *db.Pool, rdb *redis.Client) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
			return
		}

		var raw map[string]any
		if err := json.NewDecoder(r.Body).Decode(&raw); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}
		rawBytes, err := json.Marshal(raw)
		if err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}
		var params updateFileParams
		if err := json.Unmarshal(rawBytes, &params); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}
		if params.ID == "" {
			params.ID = jsonFieldString(raw, "id")
		}
		if params.SessionID == "" {
			params.SessionID = jsonFieldString(raw, "session-id", "sessionId")
		}
		if params.ID == "" {
			writeError(w, http.StatusUnprocessableEntity, "id is required")
			return
		}

		// Flatten changes-with-metadata if provided.
		changes := params.Changes
		if len(params.ChangesMeta) > 0 {
			changes = nil
			for _, m := range params.ChangesMeta {
				changes = append(changes, m.Changes...)
			}
		}

		// ── Permission check ──────────────────────────────────────────────
		fp, err := perms.GetFilePermissions(r.Context(), pool, profileID, params.ID)
		if err != nil || fp == nil || !fp.CanEdit {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		// ── Transaction ───────────────────────────────────────────────────
		tx, err := pool.Begin(r.Context())
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		defer tx.Rollback(r.Context()) //nolint:errcheck

		// Lock the file row for the duration of this transaction (prevents
		// concurrent updates from interleaving revn increments).
		var file lockedFileRow
		err = tx.QueryRow(r.Context(),
			`SELECT f.id::text, f.project_id::text, p.team_id::text,
			        f.revn, f.vern, f.is_shared, COALESCE(f.version, 0), f.data
			   FROM file f
			   JOIN project p ON p.id = f.project_id
			  WHERE f.id = $1 AND f.deleted_at IS NULL
			    FOR UPDATE OF f`,
			params.ID,
		).Scan(&file.ID, &file.ProjectID, &file.TeamID,
			&file.Revn, &file.Vern, &file.IsShared, &file.Version, &file.Data)
		if err != nil {
			writeError(w, http.StatusNotFound, "file not found")
			return
		}

		// ── Conflict checks ───────────────────────────────────────────────
		if params.Vern != file.Vern {
			writeError(w, http.StatusUnprocessableEntity, "vern-conflict")
			return
		}
		if params.Revn > file.Revn {
			writeError(w, http.StatusUnprocessableEntity, "revn-conflict")
			return
		}

		// ── P2.3 OT rebase ────────────────────────────────────────────────
		baseRevn := params.Revn
		rebased := false
		if baseRevn < file.Revn {
			competing, err := loadCompetingChangeSets(r.Context(), tx, file.ID, baseRevn, file.Revn)
			if err != nil {
				log.Printf("[update-file] competing load error: %v", err)
				// non-fatal — proceed without rebase (conservative)
			} else if len(competing) > 0 {
				changes = rebase.RebaseChangeSet(changes, competing)
				rebased = true
			}
		}

		// ── Encode changes as JSON for storage ────────────────────────────
		changesJSON, err := json.Marshal(changes)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}

		fileData, err := applyChangesToFileData(file.Data, file.ID, changes)
		if err != nil {
			log.Printf("[update-file] apply changes failed file=%s: %v", file.ID, err)
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		logUpdateFileShapeHitTest(file.ID, changes, fileData)

		now := time.Now().UTC()
		deletedAt := now.Add(1 * time.Hour) // GC after 1h (matches Clojure)
		newRevn := file.Revn + 1
		changeID := newUUID()

		// ── Insert file_change ────────────────────────────────────────────
		if _, err = tx.Exec(r.Context(),
			`INSERT INTO file_change
			   (id, session_id, profile_id, created_at, updated_at, deleted_at,
			    file_id, revn, version, base_revn, rebased, changes)
			 VALUES ($1, $2, $3, $4, $4, $5, $6, $7, $8, $9, $10, $11)`,
			changeID,
			params.SessionID,
			profileID,
			now,
			deletedAt,
			file.ID,
			newRevn,
			file.Version,
			baseRevn,
			rebased,
			changesJSON,
		); err != nil {
			writeError(w, http.StatusInternalServerError, "file_change insert failed")
			return
		}

		// ── Bump file revn + persist file.data ────────────────────────────
		if _, err = tx.Exec(r.Context(),
			`UPDATE file SET revn = $1, modified_at = $2, data = $3 WHERE id = $4`,
			newRevn, now, fileData, file.ID,
		); err != nil {
			writeError(w, http.StatusInternalServerError, "file revn update failed")
			return
		}

		// ── Touch project modified_at ─────────────────────────────────────
		_, _ = tx.Exec(r.Context(),
			`UPDATE project SET modified_at = $1 WHERE id = $2`,
			now, file.ProjectID,
		)

		if err = tx.Commit(r.Context()); err != nil {
			writeError(w, http.StatusInternalServerError, "commit failed")
			return
		}

		// ── Redis broadcast (best-effort, errors logged not returned) ─────
		if rdb != nil {
			go broadcastFileChange(rdb, broadcastParams{
				FileID:    file.ID,
				TeamID:    file.TeamID,
				ProfileID: profileID,
				SessionID: params.SessionID,
				Revn:      newRevn,
				Vern:      file.Vern,
				Changes:   changes,
				IsShared:  file.IsShared,
			})
		}

		// ── Return old revn + lagged changes ─────────────────────────────
		lagged, err := loadLaggedChanges(r.Context(), pool, file.ID, baseRevn)
		if err != nil {
			log.Printf("[update-file] lagged load error: %v", err)
			lagged = nil
		}

		writeJSON(w, http.StatusOK, updateFileResponse{
			Revn:   file.Revn, // old revn (before this commit)
			Lagged: lagged,
		})
	}
}

// ─── Competing / lagged change helpers ───────────────────────────────────────

// loadCompetingChangeSets fetches the change-sets from (baseRevn, currentRevn]
// for OT rebase.  Rows with non-JSON encoding (Clojure blobs) are skipped.
func loadCompetingChangeSets(ctx context.Context, tx pgx.Tx, fileID string, baseRevn, currentRevn int64) ([][]rebase.Change, error) {
	rows, err := tx.Query(ctx,
		`SELECT changes
		   FROM file_change
		  WHERE file_id = $1 AND revn > $2 AND revn <= $3
		    AND changes IS NOT NULL
		  ORDER BY revn ASC`,
		fileID, baseRevn, currentRevn,
	)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var result [][]rebase.Change
	for rows.Next() {
		var raw []byte
		if err := rows.Scan(&raw); err != nil {
			continue
		}
		var cs []rebase.Change
		if err := json.Unmarshal(raw, &cs); err != nil {
			// Skip Clojure-encoded rows — conservative: treat as no competition.
			continue
		}
		result = append(result, cs)
	}
	return result, nil
}

// loadLaggedChanges returns all file_change rows after baseRevn for the client
// to fast-forward its local state.  Non-JSON rows get an empty Changes slice.
func loadLaggedChanges(ctx context.Context, pool *db.Pool, fileID string, baseRevn int64) ([]laggedChangeRow, error) {
	rows, err := pool.Query(ctx,
		`SELECT id::text, revn, file_id::text, COALESCE(session_id::text, ''), changes
		   FROM file_change
		  WHERE file_id = $1 AND revn > $2
		  ORDER BY created_at ASC`,
		fileID, baseRevn,
	)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var result []laggedChangeRow
	for rows.Next() {
		var r laggedChangeRow
		var raw []byte
		if err := rows.Scan(&r.ID, &r.Revn, &r.FileID, &r.SessionID, &raw); err != nil {
			continue
		}
		if raw != nil {
			_ = json.Unmarshal(raw, &r.Changes)
		}
		result = append(result, r)
	}
	return result, nil
}

// ─── Redis broadcast ──────────────────────────────────────────────────────────

type broadcastParams struct {
	FileID    string
	TeamID    string
	ProfileID string
	SessionID string
	Revn      int64
	Vern      int64
	Changes   []rebase.Change
	IsShared  bool
}

// broadcastFileChange publishes P2.1 file-change and library-change messages.
// Runs in a goroutine; errors are logged, never returned to the client.
func broadcastFileChange(rdb *redis.Client, p broadcastParams) {
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	baseMsg := map[string]any{
		"type":      "file-change",
		"fileId":    p.FileID,
		"profileId": p.ProfileID,
		"sessionId": p.SessionID,
		"revn":      p.Revn,
		"vern":      p.Vern,
	}

	// Legacy topic: all changes on the file-id channel.
	fullMsg := copyMap(baseMsg)
	fullMsg["changes"] = p.Changes
	publish(ctx, rdb, p.FileID, fullMsg)

	// P2.1 page-scoped topics.
	changesByPage := groupChangesByPage(p.Changes)
	for pageID, pageChanges := range changesByPage {
		pageMsg := copyMap(baseMsg)
		pageMsg["pageId"] = pageID
		pageMsg["changes"] = pageChanges
		publish(ctx, rdb, fmt.Sprintf("%s:page:%s", p.FileID, pageID), pageMsg)
	}

	// P2.1 meta notifications for pages that received changes.
	for pageID := range changesByPage {
		metaMsg := map[string]any{
			"type":      "page-updated",
			"fileId":    p.FileID,
			"pageId":    pageID,
			"revn":      p.Revn,
			"sessionId": p.SessionID,
		}
		publish(ctx, rdb, p.FileID+":meta", metaMsg)
	}

	// Library changes on the team channel.
	if p.IsShared {
		var libChanges []rebase.Change
		for _, ch := range p.Changes {
			if libraryChangeTypes[ch.Type] {
				libChanges = append(libChanges, ch)
			}
		}
		if len(libChanges) > 0 {
			libMsg := map[string]any{
				"type":       "library-change",
				"fileId":     p.FileID,
				"profileId":  p.ProfileID,
				"sessionId":  p.SessionID,
				"revn":       p.Revn,
				"modifiedAt": time.Now().UTC().Format(time.RFC3339),
				"changes":    libChanges,
			}
			publish(ctx, rdb, p.TeamID, libMsg)
		}
	}
}

func publish(ctx context.Context, rdb *redis.Client, topic string, msg map[string]any) {
	data, err := json.Marshal(msg)
	if err != nil {
		log.Printf("[msgbus] marshal error topic=%s: %v", topic, err)
		return
	}
	if err := rdb.Publish(ctx, topic, data).Err(); err != nil {
		log.Printf("[msgbus] publish error topic=%s: %v", topic, err)
	}
}

func groupChangesByPage(changes []rebase.Change) map[string][]rebase.Change {
	m := make(map[string][]rebase.Change)
	for _, ch := range changes {
		if ch.PageID != "" {
			m[ch.PageID] = append(m[ch.PageID], ch)
		}
	}
	return m
}

func copyMap(src map[string]any) map[string]any {
	dst := make(map[string]any, len(src))
	for k, v := range src {
		dst[k] = v
	}
	return dst
}

func applyChangesToFileData(raw []byte, fileID string, changeSet []rebase.Change) ([]byte, error) {
	var data map[string]any
	if len(raw) > 0 && raw[0] == '{' {
		if err := json.Unmarshal(raw, &data); err != nil {
			return nil, err
		}
	}
	if data == nil {
		data = filedata.BuildEmptyData(fileID, "")
	}
	filedata.NormalizeFileData(data)
	if len(changeSet) == 0 {
		return filedata.EncodeJSON(data)
	}
	updated, err := changes.ProcessChanges(data, changeSet)
	if err != nil {
		return nil, err
	}
	filedata.NormalizeFileData(updated)
	return filedata.EncodeJSON(updated)
}

func logUpdateFileShapeHitTest(fileID string, changeSet []rebase.Change, fileData []byte) {
	var data map[string]any
	if err := json.Unmarshal(fileData, &data); err != nil {
		return
	}
	page := filedata.FirstPage(data)
	if page == nil {
		return
	}
	objects, _ := page["objects"].(map[string]any)
	for _, ch := range changeSet {
		if ch.ID == "" {
			continue
		}
		if shape, ok := objects[ch.ID].(map[string]any); ok {
			filedata.LogShapeHitTest("update-file:"+string(ch.Type), fileID, ch.ID, shape)
		}
	}
}
