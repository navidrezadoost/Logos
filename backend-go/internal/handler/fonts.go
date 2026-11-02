// Package handler — team font variant handlers.
//
// Ported from app.rpc.commands.fonts in the Clojure backend.
// Manages font variants belonging to a team (woff1/woff2/otf/ttf format files).
//
// Table: team_font_variant
//
//	id, team_id, profile_id (owner), font_id, font_family, font_weight, font_style
//	woff1_file_id, woff2_file_id, otf_file_id, ttf_file_id → storage_object
//
// Storage bucket: "team-font-variant" (one object per format file).
//
// Font format uploads
// ────────────────────
// The Clojure backend generates missing formats (e.g. ttf → woff2 conversion).
// The Go handler stores only the formats the client provides.  Format
// conversion can be added when a font processing library is integrated.
//
// Validation mirrors Clojure:
//   - font_weight ∈ {100, 200, …, 900, 950}
//   - font_style  ∈ {"normal", "italic"}
package handler

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"net/http"
	"time"

	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/db"
	"github.com/logos-design/logos/backend-go/internal/storage"
)

const fontVariantBucket = "team-font-variant"

var validFontWeights = map[int]bool{
	100: true, 200: true, 300: true, 400: true, 500: true,
	600: true, 700: true, 800: true, 900: true, 950: true,
}

// FontVariant is the public representation of a team_font_variant row.
type FontVariant struct {
	ID          string    `json:"id"`
	TeamID      string    `json:"team-id"`
	FontID      string    `json:"font-id"`
	FontFamily  string    `json:"font-family"`
	FontWeight  int       `json:"font-weight"`
	FontStyle   string    `json:"font-style"`
	Woff1FileID *string   `json:"woff1-file-id,omitempty"`
	Woff2FileID *string   `json:"woff2-file-id,omitempty"`
	OtfFileID   *string   `json:"otf-file-id,omitempty"`
	TtfFileID   *string   `json:"ttf-file-id,omitempty"`
	CreatedAt   time.Time `json:"created-at"`
	ModifiedAt  time.Time `json:"modified-at"`
}

// ─── GET /api/rpc/command/get-font-variants ───────────────────────────────────

// GetFontVariantsHandler implements GET /api/rpc/command/get-font-variants.
// Returns all non-deleted variants for the given team.
func GetFontVariantsHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		teamID := rpcParam(r, "team-id", "teamId")
		if teamID == "" {
			writeError(w, http.StatusUnprocessableEntity, "team-id is required")
			return
		}

		// Verify membership.
		if !teamMember(r.Context(), pool, profileID, teamID) {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		rows, err := pool.Query(r.Context(),
			`SELECT id::text, team_id::text, font_id::text,
			        font_family, font_weight, font_style,
			        woff1_file_id::text, woff2_file_id::text,
			        otf_file_id::text, ttf_file_id::text,
			        created_at, modified_at
			   FROM team_font_variant
			  WHERE team_id = $1 AND deleted_at IS NULL
			  ORDER BY font_family, font_weight, font_style`,
			teamID,
		)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}
		defer rows.Close()

		variants := make([]FontVariant, 0)
		for rows.Next() {
			v, err := scanFontVariant(rows)
			if err != nil {
				continue
			}
			variants = append(variants, v)
		}

		writeJSON(w, http.StatusOK, variants)
	}
}

// ─── POST /api/rpc/command/create-font-variant ───────────────────────────────

// CreateFontVariantHandler implements POST /api/rpc/command/create-font-variant.
//
// Accepts multipart/form-data with fields:
//
//	team-id      — team UUID
//	font-id      — logical font UUID (groups variants of the same typeface)
//	font-family  — e.g. "Inter"
//	font-weight  — integer 100–950
//	font-style   — "normal" or "italic"
//	woff1        — woff1 binary (optional)
//	woff2        — woff2 binary (optional)
//	otf          — otf binary   (optional)
//	ttf          — ttf binary   (optional)
//
// At least one format file must be supplied.
func CreateFontVariantHandler(pool *db.Pool, store storage.Backend) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
			return
		}

		if err := r.ParseMultipartForm(maxMediaUploadBytes); err != nil {
			writeError(w, http.StatusBadRequest, "invalid multipart body")
			return
		}

		teamID := r.FormValue("team-id")
		if teamID == "" {
			teamID = r.FormValue("teamId")
		}
		fontID := r.FormValue("font-id")
		if fontID == "" {
			fontID = r.FormValue("fontId")
		}
		fontFamily := r.FormValue("font-family")
		if fontFamily == "" {
			fontFamily = r.FormValue("fontFamily")
		}
		fontStyle := r.FormValue("font-style")
		if fontStyle == "" {
			fontStyle = r.FormValue("fontStyle")
		}

		var fontWeight int
		_, _ = readIntForm(r, "font-weight", &fontWeight)
		if fontWeight == 0 {
			_, _ = readIntForm(r, "fontWeight", &fontWeight)
		}

		if teamID == "" || fontID == "" || fontFamily == "" || fontStyle == "" || fontWeight == 0 {
			writeError(w, http.StatusUnprocessableEntity,
				"team-id, font-id, font-family, font-weight and font-style are required")
			return
		}
		if !validFontWeights[fontWeight] {
			writeError(w, http.StatusUnprocessableEntity, "invalid font-weight")
			return
		}
		if fontStyle != "normal" && fontStyle != "italic" {
			writeError(w, http.StatusUnprocessableEntity, "font-style must be 'normal' or 'italic'")
			return
		}

		if !teamMember(r.Context(), pool, profileID, teamID) {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		type formatInfo struct {
			formKey string
			mtype   string
		}
		formats := []formatInfo{
			{"woff1", "font/woff"},
			{"woff2", "font/woff2"},
			{"otf", "font/otf"},
			{"ttf", "font/ttf"},
		}

		storedIDs := make(map[string]*string, 4) // format → storage UUID or nil
		anyFormat := false
		for _, f := range formats {
			part, _, err := r.FormFile(f.formKey)
			if err != nil {
				storedIDs[f.formKey] = nil
				continue
			}
			defer part.Close()
			anyFormat = true

			buf := &bytes.Buffer{}
			if _, err := io.Copy(buf, io.LimitReader(part, maxMediaUploadBytes)); err != nil {
				writeError(w, http.StatusInternalServerError, "read error")
				return
			}
			sid := newUUID()
			if err := store.Put(r.Context(), fontVariantBucket, sid,
				bytes.NewReader(buf.Bytes()), int64(buf.Len()), f.mtype); err != nil {
				writeError(w, http.StatusInternalServerError, "storage error")
				return
			}
			cp := sid
			storedIDs[f.formKey] = &cp
		}

		if !anyFormat {
			writeError(w, http.StatusUnprocessableEntity, "at least one font format file must be provided")
			return
		}

		variantID := newUUID()
		now := time.Now().UTC()

		if _, err := pool.Exec(r.Context(),
			`INSERT INTO team_font_variant
			   (id, team_id, profile_id, font_id, font_family, font_weight, font_style,
			    woff1_file_id, woff2_file_id, otf_file_id, ttf_file_id,
			    created_at, modified_at)
			 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $12)`,
			variantID, teamID, profileID, fontID, fontFamily, fontWeight, fontStyle,
			storedIDs["woff1"], storedIDs["woff2"], storedIDs["otf"], storedIDs["ttf"],
			now,
		); err != nil {
			writeError(w, http.StatusInternalServerError, "db insert failed")
			return
		}

		writeJSON(w, http.StatusOK, FontVariant{
			ID:          variantID,
			TeamID:      teamID,
			FontID:      fontID,
			FontFamily:  fontFamily,
			FontWeight:  fontWeight,
			FontStyle:   fontStyle,
			Woff1FileID: storedIDs["woff1"],
			Woff2FileID: storedIDs["woff2"],
			OtfFileID:   storedIDs["otf"],
			TtfFileID:   storedIDs["ttf"],
			CreatedAt:   now,
			ModifiedAt:  now,
		})
	}
}

// ─── PATCH /api/rpc/command/update-font ──────────────────────────────────────

type updateFontParams struct {
	TeamID     string `json:"teamId"`
	FontID     string `json:"fontId"`
	FontFamily string `json:"fontFamily"`
}

// UpdateFontHandler implements PATCH /api/rpc/command/update-font.
// Renames the font-family for all variants with the given font-id.
func UpdateFontHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
			return
		}

		var params updateFontParams
		if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}
		if params.TeamID == "" || params.FontID == "" || params.FontFamily == "" {
			writeError(w, http.StatusUnprocessableEntity, "teamId, fontId and fontFamily are required")
			return
		}

		if !teamMember(r.Context(), pool, profileID, params.TeamID) {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		if _, err := pool.Exec(r.Context(),
			`UPDATE team_font_variant
			    SET font_family = $1, modified_at = now()
			  WHERE team_id = $2 AND font_id = $3 AND deleted_at IS NULL`,
			params.FontFamily, params.TeamID, params.FontID,
		); err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}

		writeJSON(w, http.StatusOK, map[string]any{})
	}
}

// ─── DELETE /api/rpc/command/delete-font ─────────────────────────────────────

type deleteFontParams struct {
	TeamID string `json:"teamId"`
	FontID string `json:"fontId"`
}

// DeleteFontHandler implements DELETE /api/rpc/command/delete-font.
// Soft-deletes all variants for the given font-id within the team.
func DeleteFontHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
			return
		}

		var params deleteFontParams
		if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}
		if params.TeamID == "" || params.FontID == "" {
			writeError(w, http.StatusUnprocessableEntity, "teamId and fontId are required")
			return
		}

		if !teamMember(r.Context(), pool, profileID, params.TeamID) {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		if _, err := pool.Exec(r.Context(),
			`UPDATE team_font_variant
			    SET deleted_at = now(), modified_at = now()
			  WHERE team_id = $1 AND font_id = $2 AND deleted_at IS NULL`,
			params.TeamID, params.FontID,
		); err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}

		writeJSON(w, http.StatusOK, map[string]any{})
	}
}

// ─── DELETE /api/rpc/command/delete-font-variant ─────────────────────────────

type deleteFontVariantParams struct {
	TeamID string `json:"teamId"`
	ID     string `json:"id"`
}

// DeleteFontVariantHandler implements DELETE /api/rpc/command/delete-font-variant.
// Soft-deletes a single variant.
func DeleteFontVariantHandler(pool *db.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
			return
		}

		var params deleteFontVariantParams
		if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
			writeError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}
		if params.TeamID == "" || params.ID == "" {
			writeError(w, http.StatusUnprocessableEntity, "teamId and id are required")
			return
		}

		if !teamMember(r.Context(), pool, profileID, params.TeamID) {
			writeError(w, http.StatusForbidden, "insufficient-permissions")
			return
		}

		if _, err := pool.Exec(r.Context(),
			`UPDATE team_font_variant
			    SET deleted_at = now(), modified_at = now()
			  WHERE id = $1 AND team_id = $2 AND deleted_at IS NULL`,
			params.ID, params.TeamID,
		); err != nil {
			writeError(w, http.StatusInternalServerError, "internal server error")
			return
		}

		writeJSON(w, http.StatusOK, map[string]any{})
	}
}

// ─── helpers ─────────────────────────────────────────────────────────────────

type fontVariantRow interface {
	Scan(dest ...any) error
}

func scanFontVariant(row fontVariantRow) (FontVariant, error) {
	var v FontVariant
	err := row.Scan(
		&v.ID, &v.TeamID, &v.FontID,
		&v.FontFamily, &v.FontWeight, &v.FontStyle,
		&v.Woff1FileID, &v.Woff2FileID, &v.OtfFileID, &v.TtfFileID,
		&v.CreatedAt, &v.ModifiedAt,
	)
	return v, err
}

// teamMember returns true if profileID holds any role in the given team.
func teamMember(ctx context.Context, pool *db.Pool, profileID, teamID string) bool {
	var count int
	_ = pool.QueryRow(ctx,
		`SELECT COUNT(*) FROM team_profile_rel
		  WHERE profile_id = $1 AND team_id = $2`,
		profileID, teamID,
	).Scan(&count)
	return count > 0
}
