// Package handler — file version snapshot handlers.
//
// Ported from app.rpc.commands.files-snapshot in the Clojure backend.
//
// # Storage model
//
// Snapshots are stored as labelled file_change rows:
//
//   file_change (id, file_id, revn, label, created_by, profile_id, …)
//
// When a snapshot is created from user action, a copy of the current file.data
// blob is written to the file_data table so the state can be restored later:
//
//   file_data (id = snapshot-id, file_id, type = 'snapshot', data bytea)
//
// The Go backend stores change history in file_change but does not (yet)
// maintain file.data.  Snapshots created by Go therefore have the
// file_change row but may have no corresponding file_data entry until the
// CRDT state machine is ported.  The restore operation documents this
// limitation and falls back to a metadata-only update.
//
// # RPC surface
//
//	get-file-snapshots        — list labelled file_change rows
//	create-file-snapshot      — insert labelled file_change + optional file_data
//	update-file-snapshot      — rename label
//	delete-file-snapshot      — soft-delete (user snapshots; checks locked_by)
//	restore-file-snapshot     — restore CRDT state from snapshot (partial in Go)
//	lock-file-snapshot        — set locked_by = profile_id
//	unlock-file-snapshot      — clear locked_by
package handler

import (
	"encoding/json"
	"net/http"
	"time"

	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/db"
	"github.com/logos-design/logos/backend-go/internal/perms"
)

// Snapshot is the public view of a labelled file_change row.
type Snapshot struct {
	ID         string    `json:"id"`
	FileID     string    `json:"fileId"`
	Revn       int64     `json:"revn"`
	Label      string    `json:"label"`
	CreatedBy  string    `json:"createdBy"`
	ProfileID  *string   `json:"profileId,omitempty"`
	CreatedAt  time.Time `json:"createdAt"`
	LockedBy   *string   `json:"lockedBy,omitempty"`
	IsLocked   bool      `json:"isLocked"`
}

// ─── GET /api/rpc/command/get-file-snapshots ─────────────────────────────────

// GetFileSnapshotsHandler implements GET /api/rpc/command/get-file-snapshots.
func GetFileSnapshotsHandler(pool *db.Pool) http.HandlerFunc {
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

		fp, err := perms.GetFilePermissions(r.Context(), pool, profileID, fileID)
		if err != nil || fp == nil {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		rows, err := pool.Query(r.Context(),
			`SELECT id::text, file_id::text, revn, label,
			        COALESCE(created_by, 'system'),
			        profile_id::text, created_at, locked_by::text
			   FROM file_change
			  WHERE file_id = $1
			    AND label IS NOT NULL
			    AND deleted_at IS NULL
			  ORDER BY created_at DESC
			  LIMIT 1000`,
			fileID,
		)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		defer rows.Close()

		snapshots := make([]Snapshot, 0)
		for rows.Next() {
			var s Snapshot
			if err := rows.Scan(&s.ID, &s.FileID, &s.Revn, &s.Label,
				&s.CreatedBy, &s.ProfileID, &s.CreatedAt, &s.LockedBy); err != nil {
				continue
			}
			s.IsLocked = s.LockedBy != nil
			snapshots = append(snapshots, s)
		}

		writeJSON(w, http.StatusOK, snapshots)
	}
}

// ─── POST /api/rpc/command/create-file-snapshot ──────────────────────────────

type createSnapshotParams struct {
	FileID string `json:"fileId"`
	Label  string `json:"label"`
}

// CreateFileSnapshotHandler implements POST /api/rpc/command/create-file-snapshot.
//
// Inserts a labelled file_change row capturing the current revn.
// Copies file.data to file_data(type='snapshot') when the blob is available.
func CreateFileSnapshotHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeError(w, http.StatusUnauthorized, "unauthorized")
			return
		}

		var params createSnapshotParams
		if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}
		if params.FileID == "" || params.Label == "" {
			writeError(w, http.StatusUnprocessableEntity, "fileId and label are required")
			return
		}

		fp, err := perms.GetFilePermissions(r.Context(), pool, profileID, params.FileID)
		if err != nil || fp == nil || !fp.CanEdit {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		// Load current file revn + raw data blob.
		var revn int64
		var rawData []byte
		if err = pool.QueryRow(r.Context(),
			`SELECT revn, data FROM file WHERE id = $1 AND deleted_at IS NULL`,
			params.FileID,
		).Scan(&revn, &rawData); err != nil {
			writeError(w, http.StatusNotFound, "file not found")
			return
		}

		snapshotID := newUUID()
		now := time.Now().UTC()

		tx, err := pool.Begin(r.Context())
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		defer tx.Rollback(r.Context()) //nolint:errcheck

		// Insert the labelled file_change row.
		if _, err = tx.Exec(r.Context(),
			`INSERT INTO file_change
			   (id, file_id, profile_id, created_at, updated_at,
			    revn, label, created_by)
			 VALUES ($1, $2, $3, $4, $4, $5, $6, 'user')`,
			snapshotID, params.FileID, profileID, now, revn, params.Label,
		); err != nil {
			writeError(w, http.StatusInternalServerError, "snapshot insert failed")
			return
		}

		// Copy file.data to file_data when the blob is available.
		if len(rawData) > 0 {
			_, _ = tx.Exec(r.Context(),
				`INSERT INTO file_data (id, file_id, type, data)
				 VALUES ($1, $2, 'snapshot', $3)
				 ON CONFLICT DO NOTHING`,
				snapshotID, params.FileID, rawData,
			)
		}

		if err = tx.Commit(r.Context()); err != nil {
			writeError(w, http.StatusInternalServerError, "commit failed")
			return
		}

		writeJSON(w, http.StatusOK, Snapshot{
			ID:        snapshotID,
			FileID:    params.FileID,
			Revn:      revn,
			Label:     params.Label,
			CreatedBy: "user",
			CreatedAt: now,
		})
	}
}

// ─── PATCH /api/rpc/command/update-file-snapshot ─────────────────────────────

type updateSnapshotParams struct {
	ID    string `json:"id"`
	Label string `json:"label"`
}

// UpdateFileSnapshotHandler implements PATCH /api/rpc/command/update-file-snapshot.
// Renames the snapshot label (user-created snapshots only).
func UpdateFileSnapshotHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeError(w, http.StatusUnauthorized, "unauthorized")
			return
		}

		var params updateSnapshotParams
		if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}
		if params.ID == "" || params.Label == "" {
			writeError(w, http.StatusUnprocessableEntity, "id and label are required")
			return
		}

		tag, err := pool.Exec(r.Context(),
			`UPDATE file_change
			    SET label = $1, updated_at = now()
			  WHERE id = $2 AND profile_id = $3 AND created_by = 'user'
			    AND label IS NOT NULL AND deleted_at IS NULL`,
			params.Label, params.ID, profileID,
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

// ─── DELETE /api/rpc/command/delete-file-snapshot ────────────────────────────

type deleteSnapshotParams struct {
	ID string `json:"id"`
}

// DeleteFileSnapshotHandler implements DELETE /api/rpc/command/delete-file-snapshot.
// Soft-deletes a user-created snapshot.  Locked snapshots cannot be deleted.
func DeleteFileSnapshotHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeError(w, http.StatusUnauthorized, "unauthorized")
			return
		}

		var params deleteSnapshotParams
		if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}

		tag, err := pool.Exec(r.Context(),
			`UPDATE file_change
			    SET deleted_at = now(), updated_at = now()
			  WHERE id = $1 AND profile_id = $2
			    AND created_by = 'user'
			    AND label IS NOT NULL
			    AND deleted_at IS NULL
			    AND locked_by IS NULL`,
			params.ID, profileID,
		)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		if tag.RowsAffected() == 0 {
			writeError(w, http.StatusForbidden, "not-allowed (locked or not found)")
			return
		}

		writeJSON(w, http.StatusOK, map[string]any{})
	}
}

// ─── POST /api/rpc/command/restore-file-snapshot ─────────────────────────────

type restoreSnapshotParams struct {
	ID string `json:"id"`
}

// RestoreFileSnapshotHandler implements POST /api/rpc/command/restore-file-snapshot.
//
// Full restore (Clojure behaviour):
//  1. Auto-snapshot the current file state (system snapshot, scheduled delete).
//  2. Copy the snapshot's file_data blob back to file.data.
//  3. Increment file.vern (marks a new "generation" for conflict detection).
//  4. Soft-delete all thumbnails for the file.
//
// Partial restore (Go; CRDT state machine not yet ported):
//
//	If a file_data blob exists for the snapshot, it is written back to file.data.
//	If not (Go-created snapshot), only metadata (revn, vern) is updated.
//
// In both cases the response includes the new revn so clients can sync.
func RestoreFileSnapshotHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeError(w, http.StatusUnauthorized, "unauthorized")
			return
		}

		var params restoreSnapshotParams
		if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}

		// Load the snapshot row.
		var snap Snapshot
		var snapRawData []byte
		err := pool.QueryRow(r.Context(),
			`SELECT fc.id::text, fc.file_id::text, fc.revn, fc.label,
			        COALESCE(fc.created_by, 'system'),
			        fd.data
			   FROM file_change fc
			   LEFT JOIN file_data fd ON fd.id = fc.id
			  WHERE fc.id = $1 AND fc.label IS NOT NULL AND fc.deleted_at IS NULL`,
			params.ID,
		).Scan(&snap.ID, &snap.FileID, &snap.Revn, &snap.Label,
			&snap.CreatedBy, &snapRawData)
		if err != nil {
			writeError(w, http.StatusNotFound, "snapshot not found")
			return
		}

		fp, err := perms.GetFilePermissions(r.Context(), pool, profileID, snap.FileID)
		if err != nil || fp == nil || !fp.CanEdit {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		tx, err := pool.Begin(r.Context())
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		defer tx.Rollback(r.Context()) //nolint:errcheck

		now := time.Now().UTC()

		// Auto-snapshot current state (system, expires in 48h).
		autoID := newUUID()
		autoDeleted := now.Add(48 * time.Hour)
		_, _ = tx.Exec(r.Context(),
			`INSERT INTO file_change
			   (id, file_id, created_at, updated_at, deleted_at,
			    revn, label, created_by)
			 SELECT $1, id, $3, $3, $4,
			        revn, 'internal/pre-restore/' || revn::text, 'system'
			   FROM file
			  WHERE id = $2`,
			autoID, snap.FileID, now, autoDeleted,
		)

		// Restore file.data if blob available; increment vern.
		if len(snapRawData) > 0 {
			_, err = tx.Exec(r.Context(),
				`UPDATE file
				    SET data = $1, vern = vern + 1, modified_at = $3
				  WHERE id = $2`,
				snapRawData, snap.FileID, now,
			)
		} else {
			_, err = tx.Exec(r.Context(),
				`UPDATE file
				    SET vern = vern + 1, modified_at = $2
				  WHERE id = $1`,
				snap.FileID, now,
			)
		}
		if err != nil {
			writeError(w, http.StatusInternalServerError, "restore update failed")
			return
		}

		// Invalidate thumbnails.
		_, _ = tx.Exec(r.Context(),
			`UPDATE file_thumbnail SET deleted_at = $1 WHERE file_id = $2 AND deleted_at IS NULL`,
			now, snap.FileID,
		)
		_, _ = tx.Exec(r.Context(),
			`UPDATE file_tagged_object_thumbnail SET deleted_at = $1
			  WHERE file_id = $2 AND deleted_at IS NULL`,
			now, snap.FileID,
		)

		// Read new vern after update.
		var newVern int64
		_ = tx.QueryRow(r.Context(),
			`SELECT vern FROM file WHERE id = $1`, snap.FileID,
		).Scan(&newVern)

		if err = tx.Commit(r.Context()); err != nil {
			writeError(w, http.StatusInternalServerError, "commit failed")
			return
		}

		writeJSON(w, http.StatusOK, map[string]any{
			"fileId": snap.FileID,
			"revn":   snap.Revn,
			"vern":   newVern,
		})
	}
}

// ─── POST /api/rpc/command/lock-file-snapshot ────────────────────────────────

type lockSnapshotParams struct {
	ID string `json:"id"`
}

// LockFileSnapshotHandler implements POST /api/rpc/command/lock-file-snapshot.
func LockFileSnapshotHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeError(w, http.StatusUnauthorized, "unauthorized")
			return
		}
		var params lockSnapshotParams
		if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}
		_, _ = pool.Exec(r.Context(),
			`UPDATE file_change SET locked_by = $1, updated_at = now()
			  WHERE id = $2 AND profile_id = $3 AND created_by = 'user'
			    AND deleted_at IS NULL`,
			profileID, params.ID, profileID,
		)
		writeJSON(w, http.StatusOK, map[string]any{})
	}
}

// ─── POST /api/rpc/command/unlock-file-snapshot ──────────────────────────────

// UnlockFileSnapshotHandler implements POST /api/rpc/command/unlock-file-snapshot.
func UnlockFileSnapshotHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeError(w, http.StatusUnauthorized, "unauthorized")
			return
		}
		var params lockSnapshotParams
		if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}
		_, _ = pool.Exec(r.Context(),
			`UPDATE file_change SET locked_by = NULL, updated_at = now()
			  WHERE id = $1 AND profile_id = $2 AND created_by = 'user'
			    AND deleted_at IS NULL`,
			params.ID, profileID,
		)
		writeJSON(w, http.StatusOK, map[string]any{})
	}
}
