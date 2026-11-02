// Package handler — dashboard file listing handlers.
//
// Ported from app.rpc.commands.files (team recent/shared/deleted queries).
package handler

import (
	"net/http"
	"time"

	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/db"
	"github.com/logos-design/logos/backend-go/internal/perms"
)

// SharedFile is a shared library file shown on the dashboard libraries tab.
type SharedFile struct {
	File
	LibraryFileIDs []string `json:"libraryFileIds,omitempty"`
}

// DeletedFile is a soft-deleted file shown on the dashboard trash tab.
type DeletedFile struct {
	File
	WillBeDeletedAt time.Time `json:"willBeDeletedAt"`
}

const sqlTeamRecentFiles = `
WITH recent_files AS (
  SELECT f.id,
         f.revn,
         f.vern,
         f.project_id,
         f.created_at,
         f.modified_at,
         f.name,
         f.is_shared,
         f.comment_thread_seqn,
         ft.media_id AS thumbnail_id,
         row_number() OVER w AS row_num
    FROM file AS f
    JOIN project AS p ON (p.id = f.project_id)
    LEFT JOIN file_thumbnail AS ft ON (ft.file_id = f.id
           AND ft.revn = f.revn AND ft.deleted_at IS NULL)
   WHERE p.team_id = $1
     AND p.deleted_at IS NULL
     AND f.deleted_at IS NULL
 WINDOW w AS (PARTITION BY f.project_id ORDER BY f.modified_at DESC)
   ORDER BY f.modified_at DESC
)
SELECT id, project_id, name, is_shared, revn, vern, comment_thread_seqn,
       thumbnail_id, created_at, modified_at
  FROM recent_files
 WHERE row_num <= 10`

const sqlTeamSharedFiles = `
WITH file_library_agg AS (
  SELECT flr.file_id,
         coalesce(array_agg(flr.library_file_id)
           FILTER (WHERE flr.library_file_id IS NOT NULL), '{}') AS library_file_ids
    FROM file_library_rel AS flr
   GROUP BY flr.file_id
)
SELECT f.id, f.project_id, f.name, f.is_shared, f.revn, f.vern,
       f.comment_thread_seqn,
       ft.media_id AS thumbnail_id,
       f.created_at, f.modified_at,
       fla.library_file_ids
  FROM file AS f
  JOIN project AS p ON (p.id = f.project_id)
  LEFT JOIN file_thumbnail AS ft ON (ft.file_id = f.id
         AND ft.revn = f.revn AND ft.deleted_at IS NULL)
  LEFT JOIN file_library_agg AS fla ON (fla.file_id = f.id)
 WHERE f.is_shared = true
   AND f.deleted_at IS NULL
   AND p.deleted_at IS NULL
   AND p.team_id = $1
 ORDER BY f.modified_at DESC`

const sqlTeamDeletedFiles = `
WITH deleted_files AS (
  SELECT f.id,
         f.revn,
         f.vern,
         f.project_id,
         f.created_at,
         f.modified_at,
         f.name,
         f.is_shared,
         f.comment_thread_seqn,
         f.deleted_at AS will_be_deleted_at,
         ft.media_id AS thumbnail_id,
         row_number() OVER w AS row_num
    FROM file AS f
    JOIN project AS p ON (p.id = f.project_id)
    LEFT JOIN file_thumbnail AS ft ON (ft.file_id = f.id AND ft.revn = f.revn)
   WHERE p.team_id = $1
     AND p.deleted_at IS NULL
     AND f.deleted_at IS NOT NULL
 WINDOW w AS (PARTITION BY f.project_id ORDER BY f.deleted_at DESC)
   ORDER BY f.deleted_at DESC
)
SELECT id, project_id, name, is_shared, revn, vern, comment_thread_seqn,
       thumbnail_id, created_at, modified_at, will_be_deleted_at
  FROM deleted_files
 WHERE row_num <= 10`

func teamIDFromRequest(w http.ResponseWriter, r *http.Request, profileID string, pool *db.Pool) (string, bool) {
	teamID := rpcParam(r, "team-id", "teamId")
	if teamID == "" {
		writeError(w, http.StatusBadRequest, "team-id required")
		return "", false
	}
	p, err := perms.GetTeamPermissions(r.Context(), pool, profileID, teamID)
	if err != nil || p == nil || !p.CanRead {
		writeError(w, http.StatusForbidden, "insufficient-permissions")
		return "", false
	}
	return teamID, true
}

func scanFile(row interface {
	Scan(dest ...any) error
}) (*File, error) {
	f := &File{}
	if err := row.Scan(
		&f.ID, &f.ProjectID, &f.Name, &f.IsShared, &f.Revn, &f.Vern,
		&f.CommentThreadSeqn, &f.ThumbnailID, &f.CreatedAt, &f.ModifiedAt,
	); err != nil {
		return nil, err
	}
	return f, nil
}

// GetTeamRecentFilesHandler implements POST /api/rpc/command/get-team-recent-files.
func GetTeamRecentFilesHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
			return
		}
		teamID, ok := teamIDFromRequest(w, r, profileID, pool)
		if !ok {
			return
		}

		rows, err := pool.Query(r.Context(), sqlTeamRecentFiles, teamID)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		defer rows.Close()

		var files []*File
		for rows.Next() {
			f, err := scanFile(rows)
			if err != nil {
				writeError(w, http.StatusInternalServerError, "internal server error")
				return
			}
			files = append(files, f)
		}
		if err := rows.Err(); err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		if files == nil {
			files = []*File{}
		}
		writeJSON(w, http.StatusOK, files)
	}
}

// GetTeamSharedFilesHandler implements POST /api/rpc/command/get-team-shared-files.
func GetTeamSharedFilesHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
			return
		}
		teamID, ok := teamIDFromRequest(w, r, profileID, pool)
		if !ok {
			return
		}

		rows, err := pool.Query(r.Context(), sqlTeamSharedFiles, teamID)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		defer rows.Close()

		var files []*SharedFile
		for rows.Next() {
			f := &SharedFile{}
			var libIDs []string
			if err := rows.Scan(
				&f.ID, &f.ProjectID, &f.Name, &f.IsShared, &f.Revn, &f.Vern,
				&f.CommentThreadSeqn, &f.ThumbnailID, &f.CreatedAt, &f.ModifiedAt,
				&libIDs,
			); err != nil {
				writeError(w, http.StatusInternalServerError, "internal server error")
				return
			}
			f.LibraryFileIDs = libIDs
			files = append(files, f)
		}
		if err := rows.Err(); err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		if files == nil {
			files = []*SharedFile{}
		}
		writeJSON(w, http.StatusOK, files)
	}
}

// GetTeamDeletedFilesHandler implements POST /api/rpc/command/get-team-deleted-files.
func GetTeamDeletedFilesHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
			return
		}
		teamID, ok := teamIDFromRequest(w, r, profileID, pool)
		if !ok {
			return
		}

		rows, err := pool.Query(r.Context(), sqlTeamDeletedFiles, teamID)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		defer rows.Close()

		var files []*DeletedFile
		for rows.Next() {
			f := &DeletedFile{}
			if err := rows.Scan(
				&f.ID, &f.ProjectID, &f.Name, &f.IsShared, &f.Revn, &f.Vern,
				&f.CommentThreadSeqn, &f.ThumbnailID, &f.CreatedAt, &f.ModifiedAt,
				&f.WillBeDeletedAt,
			); err != nil {
				writeError(w, http.StatusInternalServerError, "internal server error")
				return
			}
			files = append(files, f)
		}
		if err := rows.Err(); err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		if files == nil {
			files = []*DeletedFile{}
		}
		writeJSON(w, http.StatusOK, files)
	}
}
