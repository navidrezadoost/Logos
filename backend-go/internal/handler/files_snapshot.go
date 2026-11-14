// Package handler — file version snapshot handlers.
//
// Ported from app.rpc.commands.files-snapshot in the Clojure backend.
//
// # Storage model
//
// Snapshots are stored as labelled file_change rows:
//
//	file_change (id, file_id, revn, label, created_by, profile_id, …)
//
// When a snapshot is created from user action, a copy of the current file.data
// blob is written to the file_data table so the state can be restored later:
//
//	file_data (id = snapshot-id, file_id, type = 'snapshot', data bytea)
//
// Snapshot and restore handlers preserve this pairing transactionally.
// Historical snapshots may predate content blobs or integrity metadata;
// missing content is rejected and old complete blobs are labeled unverified.
//
// # RPC surface
//
//	get-file-snapshots        — list labelled file_change rows
//	create-file-snapshot      — insert labelled file_change + required file_data
//	update-file-snapshot      — rename label
//	delete-file-snapshot      — soft-delete (user snapshots; checks locked_by)
//	restore-file-snapshot     — restore CRDT state from snapshot (partial in Go)
//	lock-file-snapshot        — set locked_by = profile_id
//	unlock-file-snapshot      — clear locked_by
package handler

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/db"
	"github.com/logos-design/logos/backend-go/internal/perms"
)

// Snapshot is the public view of a labelled file_change row.
type Snapshot struct {
	ID            string    `json:"id"`
	FileID        string    `json:"fileId"`
	Revn          int64     `json:"revn"`
	Label         string    `json:"label"`
	CreatedBy     string    `json:"createdBy"`
	ProfileID     *string   `json:"profileId,omitempty"`
	CreatedAt     time.Time `json:"createdAt"`
	LockedBy      *string   `json:"lockedBy,omitempty"`
	IsLocked      bool      `json:"isLocked"`
	RestoreStatus string    `json:"restoreStatus"`
}

const snapshotIntegrityMetadataKey = "snapshot-integrity"

const (
	snapshotStatusVerified         = "verified"
	snapshotStatusLegacyUnverified = "legacy-unverified"
	snapshotStatusUnavailable      = "unavailable"
	snapshotStatusCorrupt          = "corrupt"
)

var (
	errSnapshotContentUnavailable = errors.New("snapshot-content-unavailable")
	errSnapshotContentCorrupt     = errors.New("snapshot-content-corrupt")
)

type snapshotIntegrityMetadata struct {
	Version   int    `json:"version"`
	Algorithm string `json:"algorithm"`
	Digest    string `json:"digest"`
}

func integrityMetadata(data []byte) ([]byte, error) {
	digest := sha256.Sum256(data)
	return json.Marshal(map[string]snapshotIntegrityMetadata{
		snapshotIntegrityMetadataKey: {Version: 1, Algorithm: "sha256", Digest: hex.EncodeToString(digest[:])},
	})
}

// verifySnapshotBlob treats old blobs without an integrity envelope as
// restorable but unverified. A present envelope must be recognized and match.
func verifySnapshotBlob(data, rawMetadata []byte) (string, error) {
	if len(data) == 0 {
		return snapshotStatusUnavailable, errSnapshotContentUnavailable
	}
	var metadata map[string]json.RawMessage
	if len(rawMetadata) > 0 {
		if err := json.Unmarshal(rawMetadata, &metadata); err != nil {
			return snapshotStatusCorrupt, errSnapshotContentCorrupt
		}
	}
	raw, hasIntegrity := metadata[snapshotIntegrityMetadataKey]
	if !hasIntegrity {
		return snapshotStatusLegacyUnverified, nil
	}
	var integrity snapshotIntegrityMetadata
	if json.Unmarshal(raw, &integrity) != nil || integrity.Version != 1 || integrity.Algorithm != "sha256" || len(integrity.Digest) != 64 {
		return snapshotStatusCorrupt, errSnapshotContentCorrupt
	}
	digest := sha256.Sum256(data)
	if hex.EncodeToString(digest[:]) != integrity.Digest {
		return snapshotStatusCorrupt, errSnapshotContentCorrupt
	}
	return snapshotStatusVerified, nil
}

// fileEditPermissionTx locks the membership rows used for authorization until
// commit. Revocation therefore serializes with snapshot creation/restoration.
func fileEditPermissionTx(ctx context.Context, tx pgx.Tx, profileID, projectID string) (bool, error) {
	var teamID string
	if err := tx.QueryRow(ctx, `SELECT team_id::text FROM project WHERE id=$1 AND deleted_at IS NULL`, projectID).Scan(&teamID); err != nil {
		return false, err
	}
	isOwner, isAdmin, canEdit := false, false, false
	rows, err := tx.Query(ctx, `SELECT is_owner, is_admin, can_edit FROM team_profile_rel WHERE team_id=$1 AND profile_id=$2 FOR SHARE`, teamID, profileID)
	if err != nil {
		return false, err
	}
	for rows.Next() {
		var owner, admin, edit bool
		if rows.Scan(&owner, &admin, &edit) != nil {
			rows.Close()
			return false, fmt.Errorf("read team permission")
		}
		isOwner = isOwner || owner
		isAdmin = isAdmin || admin
		canEdit = canEdit || edit
	}
	rows.Close()
	if rows.Err() != nil {
		return false, rows.Err()
	}
	rows, err = tx.Query(ctx, `SELECT is_owner, is_admin, can_edit FROM project_profile_rel WHERE project_id=$1 AND profile_id=$2 FOR SHARE`, projectID, profileID)
	if err != nil {
		return false, err
	}
	for rows.Next() {
		var owner, admin, edit bool
		if rows.Scan(&owner, &admin, &edit) != nil {
			rows.Close()
			return false, fmt.Errorf("read project permission")
		}
		isOwner = isOwner || owner
		isAdmin = isAdmin || admin
		canEdit = canEdit || edit
	}
	rows.Close()
	if err := rows.Err(); err != nil {
		return false, err
	}
	return isOwner || isAdmin || canEdit, nil
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
			`WITH snapshot_rows AS (
				SELECT fc.id::text AS id, fc.file_id::text AS file_id, fc.revn, fc.label,
				       COALESCE(fc.created_by, 'system') AS created_by,
				       fc.profile_id::text AS profile_id, fc.created_at, fc.locked_by::text AS locked_by,
				       fd.data, COALESCE(fd.metadata, '{}'::jsonb) AS metadata
				  FROM file_change fc
				  LEFT JOIN file_data fd ON fd.file_id=fc.file_id AND fd.id=fc.id AND fd.type='snapshot'
				       AND (fd.deleted_at IS NULL OR (fc.created_by='system' AND fd.deleted_at >= now()))
				 WHERE fc.file_id=$1 AND fc.label IS NOT NULL
				   AND ((fc.created_by='system' AND (fc.deleted_at IS NULL OR fc.deleted_at >= now()))
				        OR (fc.created_by<>'system' AND fc.deleted_at IS NULL))
			), visible_snapshots AS (
				(SELECT * FROM snapshot_rows WHERE created_by='system' ORDER BY created_at DESC LIMIT 500)
				UNION ALL
				(SELECT * FROM snapshot_rows WHERE created_by<>'system' ORDER BY created_at DESC LIMIT 500)
			)
			SELECT * FROM visible_snapshots ORDER BY created_at DESC`,
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
			var data, metadata []byte
			if err := rows.Scan(&s.ID, &s.FileID, &s.Revn, &s.Label,
				&s.CreatedBy, &s.ProfileID, &s.CreatedAt, &s.LockedBy, &data, &metadata); err != nil {
				writeError(w, http.StatusInternalServerError, "snapshot listing failed")
				return
			}
			s.IsLocked = s.LockedBy != nil
			status, verifyErr := verifySnapshotBlob(data, metadata)
			if errors.Is(verifyErr, errSnapshotContentUnavailable) {
				s.RestoreStatus = snapshotStatusUnavailable
			} else if errors.Is(verifyErr, errSnapshotContentCorrupt) {
				s.RestoreStatus = snapshotStatusCorrupt
			} else if verifyErr != nil {
				writeError(w, http.StatusInternalServerError, "snapshot integrity check failed")
				return
			} else {
				s.RestoreStatus = status
			}
			snapshots = append(snapshots, s)
		}
		if err := rows.Err(); err != nil {
			writeError(w, http.StatusInternalServerError, "snapshot listing failed")
			return
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
// Inserts a labelled file_change row and revision-bound blob atomically.
func CreateFileSnapshotHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
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

		tx, err := pool.Begin(r.Context())
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		defer tx.Rollback(r.Context()) //nolint:errcheck
		// Lock the file before reading either revision or content.
		var projectID string
		var revn int64
		var rawData []byte
		if err = tx.QueryRow(r.Context(),
			`SELECT project_id::text, revn, data FROM file WHERE id = $1 AND deleted_at IS NULL FOR UPDATE`,
			params.FileID,
		).Scan(&projectID, &revn, &rawData); err != nil {
			writeError(w, http.StatusNotFound, "file not found")
			return
		}
		allowed, permissionErr := fileEditPermissionTx(r.Context(), tx, profileID, projectID)
		if permissionErr != nil {
			writeError(w, http.StatusInternalServerError, "permission check failed")
			return
		}
		if !allowed {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}
		if len(rawData) == 0 {
			writeError(w, http.StatusConflict, "snapshot-content-unavailable")
			return
		}

		snapshotID := newUUID()
		now := time.Now().UTC()

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

		metadata, err := integrityMetadata(rawData)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "snapshot integrity metadata failed")
			return
		}
		if _, err = tx.Exec(r.Context(),
			`INSERT INTO file_data (id, file_id, type, data, metadata)
			 VALUES ($1, $2, 'snapshot', $3, $4)`, snapshotID, params.FileID, rawData, metadata); err != nil {
			writeError(w, http.StatusInternalServerError, "snapshot content insert failed")
			return
		}

		if err = tx.Commit(r.Context()); err != nil {
			writeError(w, http.StatusInternalServerError, "commit failed")
			return
		}

		writeJSON(w, http.StatusOK, Snapshot{
			ID:            snapshotID,
			FileID:        params.FileID,
			Revn:          revn,
			Label:         params.Label,
			CreatedBy:     "user",
			CreatedAt:     now,
			RestoreStatus: "verified",
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
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
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
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
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
// Restore requires complete content. Snapshot row, content, pre-restore copy,
// file mutation, and thumbnail invalidation commit as one transaction.
func RestoreFileSnapshotHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
			return
		}

		var params restoreSnapshotParams
		if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}

		tx, err := pool.Begin(r.Context())
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		defer tx.Rollback(r.Context()) //nolint:errcheck
		// Read and lock the snapshot first to identify its file, then lock the
		// file row and recheck authorization inside the same transaction.
		var snap Snapshot
		if err := tx.QueryRow(r.Context(),
			`SELECT id::text, file_id::text, revn, label, COALESCE(created_by,'system')
			   FROM file_change
			  WHERE id=$1 AND label IS NOT NULL
			    AND (deleted_at IS NULL OR (created_by='system' AND deleted_at >= now()))
			  FOR SHARE`, params.ID).
			Scan(&snap.ID, &snap.FileID, &snap.Revn, &snap.Label, &snap.CreatedBy); err != nil {
			writeError(w, http.StatusNotFound, "snapshot not found")
			return
		}
		var projectID string
		var currentData []byte
		if err := tx.QueryRow(r.Context(), `SELECT project_id::text, data FROM file WHERE id=$1 AND deleted_at IS NULL FOR UPDATE`, snap.FileID).
			Scan(&projectID, &currentData); err != nil {
			writeError(w, http.StatusNotFound, "file not found")
			return
		}
		allowed, permissionErr := fileEditPermissionTx(r.Context(), tx, profileID, projectID)
		if permissionErr != nil {
			writeError(w, http.StatusInternalServerError, "permission check failed")
			return
		}
		if !allowed {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}
		var snapRawData, snapMetadata []byte
		if err := tx.QueryRow(r.Context(),
			`SELECT data, COALESCE(metadata, '{}'::jsonb) FROM file_data
			  WHERE file_id=$1 AND id=$2 AND type='snapshot'
			    AND (deleted_at IS NULL OR ($3='system' AND deleted_at >= now()))
			  FOR SHARE`, snap.FileID, snap.ID, snap.CreatedBy).
			Scan(&snapRawData, &snapMetadata); err != nil {
			writeError(w, http.StatusConflict, "snapshot-content-unavailable")
			return
		}
		restoreStatus, err := verifySnapshotBlob(snapRawData, snapMetadata)
		if err != nil {
			writeError(w, http.StatusConflict, err.Error())
			return
		}
		if len(currentData) == 0 {
			writeError(w, http.StatusConflict, "pre-restore-content-unavailable")
			return
		}

		now := time.Now().UTC()

		// Auto-snapshot current state using the legacy system-snapshot retention
		// convention: future deleted_at means logically available until expiry.
		autoID := newUUID()
		autoDeleted := now.Add(48 * time.Hour)
		if _, err = tx.Exec(r.Context(),
			`INSERT INTO file_change
			   (id, file_id, created_at, updated_at, deleted_at,
			    revn, label, created_by)
			 SELECT $1, id, $3, $3, $4,
			        revn, 'internal/pre-restore/' || revn::text, 'system'
			   FROM file
			  WHERE id = $2`,
			autoID, snap.FileID, now, autoDeleted,
		); err != nil {
			writeError(w, http.StatusInternalServerError, "pre-restore snapshot insert failed")
			return
		}
		preRestoreMetadata, err := integrityMetadata(currentData)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "pre-restore integrity metadata failed")
			return
		}
		if _, err = tx.Exec(r.Context(), `INSERT INTO file_data (id,file_id,type,data,metadata,deleted_at) VALUES ($1,$2,'snapshot',$3,$4,$5)`, autoID, snap.FileID, currentData, preRestoreMetadata, autoDeleted); err != nil {
			writeError(w, http.StatusInternalServerError, "pre-restore content insert failed")
			return
		}

		if _, err = tx.Exec(r.Context(), `UPDATE file SET data=$1, vern=vern+1, modified_at=$2 WHERE id=$3`, snapRawData, now, snap.FileID); err != nil {
			writeError(w, http.StatusInternalServerError, "restore update failed")
			return
		}

		// Invalidate thumbnails.
		if _, err = tx.Exec(r.Context(),
			`UPDATE file_thumbnail SET deleted_at = $1 WHERE file_id = $2 AND deleted_at IS NULL`,
			now, snap.FileID,
		); err != nil {
			writeError(w, http.StatusInternalServerError, "thumbnail invalidation failed")
			return
		}
		if _, err = tx.Exec(r.Context(),
			`UPDATE file_tagged_object_thumbnail SET deleted_at = $1
			  WHERE file_id = $2 AND deleted_at IS NULL`,
			now, snap.FileID,
		); err != nil {
			writeError(w, http.StatusInternalServerError, "tagged thumbnail invalidation failed")
			return
		}

		// Read new vern after update.
		var newVern int64
		if err = tx.QueryRow(r.Context(),
			`SELECT vern FROM file WHERE id = $1`, snap.FileID,
		).Scan(&newVern); err != nil {
			writeError(w, http.StatusInternalServerError, "restore revision read failed")
			return
		}

		if err = tx.Commit(r.Context()); err != nil {
			writeError(w, http.StatusInternalServerError, "commit failed")
			return
		}

		writeJSON(w, http.StatusOK, map[string]any{
			"fileId":        snap.FileID,
			"revn":          snap.Revn,
			"vern":          newVern,
			"restoreStatus": restoreStatus,
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
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
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
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
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
