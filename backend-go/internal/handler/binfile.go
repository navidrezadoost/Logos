// Package handler — .penpot file export/import handlers.
//
// Ported from app.rpc.commands.binfile in the Clojure backend.
//
// # Export flow (export-binfile)
//
//  1. Verify the caller has read access to the file.
//  2. Load file attributes, change history, and media objects from PostgreSQL.
//  3. Load raw media blobs from the storage backend.
//  4. If the file has a raw CRDT blob (file.data ≠ nil), include it in the ZIP
//     so the Clojure backend can continue to manage it after a round-trip.
//  5. Call binfile.WriteZIP → return ZIP as application/zip response.
//
// # Import flow (import-binfile)
//
//  1. Verify the caller has edit access to the destination project.
//  2. Read multipart "file" part (up to maxBinfileBytes).
//  3. Detect format: v1 → 400 (not supported); v3 ZIP → parse with ReadZIP.
//  4. Create the new file record in the target project.
//  5. Import media objects (insert rows, store blobs).
//  6. Insert change rows so the Go backend can replay the change history.
//  7. If a raw CRDT blob was included (data.bin), write it to file.data so the
//     Clojure backend finds valid state when it opens the file.
//
// # Interoperability
//
// Files exported by Go are importable by the Clojure backend:
//   - manifest.json follows the standard v3 schema.
//   - files/{id}/attrs.json contains the file metadata.
//   - media/ and objects/ sections follow the standard layout.
//   - files/{id}/data.bin is a Go extension; Clojure ignores unknown entries.
//
// Files exported by Clojure are importable by Go:
//   - Manifest and attrs sections are parsed normally.
//   - Page entries (pages/{id}.json, pages/{id}/{shape-id}.json) contain
//     CRDT data that Go cannot decode yet; they are preserved as data.bin if
//     present, or skipped.  The importer creates a valid (but initially empty)
//     file.  The full CRDT state machine import will be added in Session 6.
package handler

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"time"

	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/binfile"
	"github.com/logos-design/logos/backend-go/internal/db"
	"github.com/logos-design/logos/backend-go/internal/perms"
	"github.com/logos-design/logos/backend-go/internal/storage"
)

const maxBinfileBytes = 500 << 20 // 500 MiB

// ─── POST /api/rpc/command/export-binfile ─────────────────────────────────────

type exportBinfileParams struct {
	FileID          string `json:"fileId"`
	IncludeLibraries bool   `json:"includeLibraries,omitempty"`
	EmbedAssets     bool   `json:"embedAssets,omitempty"`
}

// ExportBinfileHandler implements POST /api/rpc/command/export-binfile.
//
// Returns the .penpot ZIP directly as the response body
// (Content-Type: application/zip).
func ExportBinfileHandler(pool *db.Pool, store storage.Backend) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeError(w, http.StatusUnauthorized, "unauthorized")
			return
		}

		var params exportBinfileParams
		if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}
		if params.FileID == "" {
			writeError(w, http.StatusUnprocessableEntity, "fileId is required")
			return
		}

		// Permission: read access is sufficient for export.
		fp, err := perms.GetFilePermissions(r.Context(), pool, profileID, params.FileID)
		if err != nil || fp == nil {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		// ── Load file attributes ───────────────────────────────────────────
		var attrs binfile.FileAttrs
		var rawData []byte
		err = pool.QueryRow(r.Context(),
			`SELECT id::text, name,
			        COALESCE(project_id::text, ''),
			        revn, vern, is_shared, modified_at,
			        data
			   FROM file
			  WHERE id = $1 AND deleted_at IS NULL`,
			params.FileID,
		).Scan(&attrs.ID, &attrs.Name, &attrs.ProjectID,
			&attrs.Revn, &attrs.Vern, &attrs.IsShared, &attrs.ModifiedAt,
			&rawData)
		if err != nil {
			writeError(w, http.StatusNotFound, "file not found")
			return
		}

		// ── Load change history ────────────────────────────────────────────
		var changeRows []binfile.ChangeRow
		rows, err := pool.Query(r.Context(),
			`SELECT id::text, COALESCE(session_id::text,''), COALESCE(profile_id::text,''),
			        revn, base_revn, rebased, created_at, changes
			   FROM file_change
			  WHERE file_id = $1 AND deleted_at IS NULL
			  ORDER BY revn ASC`,
			params.FileID,
		)
		if err == nil {
			defer rows.Close()
			for rows.Next() {
				var cr binfile.ChangeRow
				var raw []byte
				if err := rows.Scan(&cr.ID, &cr.SessionID, &cr.ProfileID,
					&cr.Revn, &cr.BaseRevn, &cr.Rebased, &cr.CreatedAt, &raw); err != nil {
					continue
				}
				// Only include JSON-parseable rows (Go-written).
				if raw != nil && json.Valid(raw) {
					cr.Changes = raw
				}
				changeRows = append(changeRows, cr)
			}
		}

		// ── Load media objects ─────────────────────────────────────────────
		var mediaMeta []binfile.MediaMeta
		mrows, err := pool.Query(r.Context(),
			`SELECT id::text, file_id::text, name,
			        media_id::text, thumbnail_id::text,
			        width, height, mtype, is_local
			   FROM file_media_object
			  WHERE file_id = $1 AND deleted_at IS NULL`,
			params.FileID,
		)
		if err == nil {
			defer mrows.Close()
			for mrows.Next() {
				var m binfile.MediaMeta
				if err := mrows.Scan(&m.ID, &m.FileID, &m.Name,
					&m.MediaID, &m.ThumbnailID,
					&m.Width, &m.Height, &m.Mtype, &m.IsLocal); err != nil {
					continue
				}
				mediaMeta = append(mediaMeta, m)
			}
		}

		// ── Load storage objects (media blobs) ─────────────────────────────
		var exportObjs []binfile.ExportObject
		if store != nil {
			for _, m := range mediaMeta {
				rc, err := store.Get(r.Context(), "file-media-object", m.MediaID)
				if err != nil {
					continue // missing blob — skip, not fatal
				}
				data, _ := io.ReadAll(rc)
				rc.Close()
				exportObjs = append(exportObjs, binfile.ExportObject{
					ID:          m.MediaID,
					Bucket:      "file-media-object",
					ContentType: m.Mtype,
					Data:        data,
				})
			}
		}

		// ── Build payload ──────────────────────────────────────────────────
		payload := binfile.ExportPayload{
			Attrs:   attrs,
			Changes: changeRows,
			Media:   mediaMeta,
			Objects: exportObjs,
			RawData: rawData,
		}

		// ── Write ZIP ──────────────────────────────────────────────────────
		var buf bytes.Buffer
		if err := binfile.WriteZIP(&buf, payload); err != nil {
			writeError(w, http.StatusInternalServerError, "zip creation failed")
			return
		}

		filename := sanitizeFilename(attrs.Name) + ".penpot"
		w.Header().Set("Content-Type", "application/zip")
		w.Header().Set("Content-Disposition", fmt.Sprintf(`attachment; filename="%s"`, filename))
		w.Header().Set("Content-Length", fmt.Sprintf("%d", buf.Len()))
		w.WriteHeader(http.StatusOK)
		_, _ = buf.WriteTo(w)
	}
}

// ─── POST /api/rpc/command/import-binfile ─────────────────────────────────────

// ImportBinfileHandler implements POST /api/rpc/command/import-binfile.
//
// Accepts multipart/form-data with fields:
//
//	project-id  — destination project UUID
//	name        — override file name (optional; uses name from manifest if absent)
//	version     — format version (1 or 3; default 3)
//	file        — the .penpot binary (multipart part name "file")
func ImportBinfileHandler(pool *db.Pool, store storage.Backend) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeError(w, http.StatusUnauthorized, "unauthorized")
			return
		}

		if err := r.ParseMultipartForm(maxBinfileBytes); err != nil {
			writeError(w, http.StatusBadRequest, "invalid multipart body")
			return
		}

		projectID := r.FormValue("project-id")
		if projectID == "" {
			projectID = r.FormValue("projectId")
		}
		if projectID == "" {
			writeError(w, http.StatusUnprocessableEntity, "project-id is required")
			return
		}

		// Permission: edit access to destination project.
		pp, err := perms.GetProjectPermissions(r.Context(), pool, profileID, projectID)
		if err != nil || pp == nil || !pp.CanEdit {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		// Read the uploaded file.
		part, _, err := r.FormFile("file")
		if err != nil {
			writeError(w, http.StatusBadRequest, "file part missing")
			return
		}
		defer part.Close()

		data, err := io.ReadAll(io.LimitReader(part, maxBinfileBytes))
		if err != nil {
			writeError(w, http.StatusInternalServerError, "read error")
			return
		}

		// Detect format.
		if len(data) < 4 {
			writeError(w, http.StatusBadRequest, "file too short")
			return
		}
		if binfile.ParseFormat(data[:4]) == binfile.FormatV1 {
			writeError(w, http.StatusBadRequest, "v1 binary format is not supported; use the Clojure backend to import v1 files")
			return
		}

		// Parse ZIP.
		payload, err := binfile.ReadZIP(data)
		if err != nil {
			writeError(w, http.StatusBadRequest, fmt.Sprintf("invalid .penpot file: %v", err))
			return
		}
		if len(payload.Files) == 0 {
			writeError(w, http.StatusBadRequest, "empty export: no files found in manifest")
			return
		}

		nameOverride := r.FormValue("name")

		// Import each file in a transaction.
		type importedFile struct {
			OldID string `json:"oldId"`
			NewID string `json:"newId"`
			Name  string `json:"name"`
		}
		var imported []importedFile

		for _, pf := range payload.Files {
			name := pf.Attrs.Name
			if nameOverride != "" {
				name = nameOverride
			}

			tx, err := pool.Begin(r.Context())
			if err != nil {
				writeError(w, http.StatusInternalServerError, "internal server error")
				return
			}

			newFileID := newUUID()
			now := time.Now().UTC()

			// Insert file record.
			if _, err = tx.Exec(r.Context(),
				`INSERT INTO file
				   (id, project_id, name, revn, vern, is_shared,
				    modified_at, created_at, data)
				 VALUES ($1, $2, $3, $4, $5, $6, $7, $7, $8)`,
				newFileID, projectID, name,
				pf.Attrs.Revn, pf.Attrs.Vern, pf.Attrs.IsShared,
				now, pf.RawData,
			); err != nil {
				tx.Rollback(r.Context()) //nolint:errcheck
				writeError(w, http.StatusInternalServerError, "file insert failed")
				return
			}

			// Insert file_profile_rel (owner).
			_, _ = tx.Exec(r.Context(),
				`INSERT INTO file_profile_rel (file_id, profile_id, is_owner, is_admin, can_edit)
				 VALUES ($1, $2, true, true, true)`,
				newFileID, profileID,
			)

			// Import media objects.
			for _, m := range pf.Media {
				newMediaObjID := newUUID()
				if _, err = tx.Exec(r.Context(),
					`INSERT INTO file_media_object
					   (id, file_id, is_local, name, media_id, thumbnail_id, width, height, mtype, created_at)
					 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
					 ON CONFLICT (id) DO NOTHING`,
					newMediaObjID, newFileID, m.IsLocal, m.Name, m.MediaID,
					m.ThumbnailID, m.Width, m.Height, m.Mtype, now,
				); err != nil {
					continue // skip duplicate / constraint violations
				}
			}

			// Import change rows.
			for _, cr := range pf.Changes {
				newChangeID := newUUID()
				deletedAt := now.Add(24 * time.Hour) // 1-day GC window for imported changes
				if _, err = tx.Exec(r.Context(),
					`INSERT INTO file_change
					   (id, file_id, session_id, profile_id, created_at, updated_at,
					    deleted_at, revn, base_revn, rebased, changes)
					 VALUES ($1, $2, $3, $4, $5, $5, $6, $7, $8, $9, $10)
					 ON CONFLICT DO NOTHING`,
					newChangeID, newFileID,
					nilIfEmpty(cr.SessionID), nilIfEmpty(cr.ProfileID),
					now, deletedAt,
					cr.Revn, cr.BaseRevn, cr.Rebased, []byte(cr.Changes),
				); err != nil {
					continue
				}
			}

			// Update project.modified_at.
			_, _ = tx.Exec(r.Context(),
				`UPDATE project SET modified_at = $1 WHERE id = $2`, now, projectID,
			)

			if err = tx.Commit(r.Context()); err != nil {
				tx.Rollback(r.Context()) //nolint:errcheck
				writeError(w, http.StatusInternalServerError, "commit failed")
				return
			}

			// Store media blobs after commit (non-fatal failures).
			if store != nil {
				for _, obj := range pf.Objects {
					_ = store.Put(r.Context(), obj.Meta.Bucket, obj.Meta.ID,
						bytes.NewReader(obj.Data), obj.Meta.Size, obj.Meta.ContentType)
				}
			}

			imported = append(imported, importedFile{
				OldID: pf.Attrs.ID,
				NewID: newFileID,
				Name:  name,
			})
		}

		writeJSON(w, http.StatusOK, map[string]any{"files": imported})
	}
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

func sanitizeFilename(name string) string {
	safe := make([]byte, 0, len(name))
	for _, b := range []byte(name) {
		switch {
		case b >= 'a' && b <= 'z', b >= 'A' && b <= 'Z', b >= '0' && b <= '9',
			b == '-', b == '_', b == ' ':
			safe = append(safe, b)
		default:
			safe = append(safe, '_')
		}
	}
	if len(safe) == 0 {
		return "export"
	}
	return string(safe)
}

func nilIfEmpty(s string) any {
	if s == "" {
		return nil
	}
	return s
}
