// Package binfile implements the .penpot v3 file export/import format.
//
// # Format overview
//
// A .penpot file is a standard ZIP archive.  The root always contains:
//
//	manifest.json              — type, version, file list
//
// Per exported file (at path prefix files/{file-id}/):
//
//	files/{file-id}/attrs.json       — file attributes (name, revn, is-shared…)
//	files/{file-id}/pages.json       — ordered page-id list
//	files/{file-id}/changes.json     — change history rows (Go extension)
//	files/{file-id}/data.bin         — raw file.data blob when present (Clojure
//	                                   blob encoding preserved for interoperability)
//
// Media objects  (reused across files):
//
//	media/{media-id}.json            — FileMediaObject row
//	objects/{storage-id}.json        — storage object metadata
//	objects/{storage-id}{ext}        — raw bytes (.png, .jpg, .woff2, …)
//
// # Format detection
//
// `ParseFormat` inspects the first 4 bytes of a reader:
//   - "PK\x03\x04" (ZIP magic) → FormatV3
//   - anything else            → FormatV1 (legacy zstd+Fressian; not decoded by Go)
//
// # Interoperability
//
// During the Clojure→Go migration both backends may write change rows:
//
//   - Rows written by Go: JSON bytes in changes bytea column.
//   - Rows written by Clojure: Transit+zstd bytes (not readable by Go).
//
// On export, Go writes only the rows it can decode (JSON-parseable).
// On import, Go inserts all rows as-is; the Go backend will only rebase
// JSON rows during files_update (Clojure rows are skipped with a safe fallback).
//
// A Clojure-exported ZIP that contains a `files/{id}/data.bin` entry is
// imported faithfully: the raw blob is written back to the `file` table's
// `data` column so the Clojure backend can continue to manage the CRDT state.
package binfile

import (
	"archive/zip"
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"strings"
	"time"
)

// ─── Constants ────────────────────────────────────────────────────────────────

const (
	// FormatV1 is the legacy zstd+Fressian binary format.  Go can detect it
	// but does not decode it; import returns ErrFormatV1NotSupported.
	FormatV1 = 1
	// FormatV3 is the current ZIP-based format.  Go fully supports it.
	FormatV3 = 3

	manifestVersion = 1
	generatedBy     = "logos-go/1.0"
)

// ErrFormatV1NotSupported is returned when a v1 binary file is supplied for import.
var ErrFormatV1NotSupported = fmt.Errorf("binfile: v1 binary format is not supported by the Go backend; use the Clojure backend for v1 import")

// ─── Manifest ─────────────────────────────────────────────────────────────────

// Manifest is the root metadata entry in manifest.json.
type Manifest struct {
	Type        string      `json:"type"`                  // always "penpot/export-files"
	Version     int         `json:"version"`               // always 1
	GeneratedBy string      `json:"generated-by"`
	GoExtension bool        `json:"go-extension,omitempty"` // true when written by Go
	Files       []FileEntry `json:"files"`
	Relations   []Relation  `json:"relations,omitempty"` // library links
}

// FileEntry describes one exported file in the manifest.
type FileEntry struct {
	ID       string   `json:"id"`
	Name     string   `json:"name"`
	Features []string `json:"features"`
}

// Relation describes a library dependency link.
type Relation struct {
	FileID    string `json:"file-id"`
	LibraryID string `json:"library-id"`
}

// ─── Per-file entries ─────────────────────────────────────────────────────────

// FileAttrs is stored as files/{id}/attrs.json.
type FileAttrs struct {
	ID         string    `json:"id"`
	Name       string    `json:"name"`
	ProjectID  string    `json:"project-id"`
	Revn       int64     `json:"revn"`
	Vern       int64     `json:"vern"`
	IsShared   bool      `json:"is-shared,omitempty"`
	ModifiedAt time.Time `json:"modified-at"`
}

// ChangeRow is one row from file_change, stored in files/{id}/changes.json.
// Changes are stored as raw JSON so the importer can replay them verbatim.
type ChangeRow struct {
	ID        string          `json:"id"`
	SessionID string          `json:"session-id,omitempty"`
	ProfileID string          `json:"profile-id,omitempty"`
	Revn      int64           `json:"revn"`
	BaseRevn  int64           `json:"base-revn"`
	Rebased   bool            `json:"rebased,omitempty"`
	CreatedAt time.Time       `json:"created-at"`
	Changes   json.RawMessage `json:"changes"` // JSON-encoded change array; nil if Clojure-encoded
}

// MediaMeta is stored as media/{media-id}.json.
type MediaMeta struct {
	ID          string  `json:"id"`
	FileID      string  `json:"file-id"` // source file (remapped on import)
	Name        string  `json:"name"`
	MediaID     string  `json:"media-id"`   // storage object UUID
	ThumbnailID *string `json:"thumbnail-id,omitempty"`
	Width       int     `json:"width"`
	Height      int     `json:"height"`
	Mtype       string  `json:"mtype"`
	IsLocal     bool    `json:"is-local"`
}

// StorageMeta is stored as objects/{storage-id}.json.
type StorageMeta struct {
	ID          string `json:"id"`
	ContentType string `json:"content-type"`
	Size        int64  `json:"size"`
	Bucket      string `json:"bucket"`
}

// ─── Export payload (in-memory DTO) ──────────────────────────────────────────

// ExportPayload carries all the data for one file export.
// Callers populate it from DB / storage and pass it to WriteZIP.
type ExportPayload struct {
	Attrs    FileAttrs
	Changes  []ChangeRow
	Media    []MediaMeta
	// Objects maps storage-object-id → (metadata, raw bytes, content-type).
	Objects  []ExportObject
	// RawData is the raw file.data blob (Clojure encoding), may be nil.
	RawData  []byte
	// PageIDs is the ordered list of page UUIDs (from file.data or change history).
	PageIDs  []string
}

// ExportObject is a storage object to be included in the ZIP.
type ExportObject struct {
	ID          string
	Bucket      string
	ContentType string
	Data        []byte
}

// ─── Import result ────────────────────────────────────────────────────────────

// ImportPayload is what the importer returns after parsing a ZIP.
type ImportPayload struct {
	Manifest Manifest
	Files    []ParsedFile
}

// ParsedFile holds the parsed data for one file from the ZIP.
type ParsedFile struct {
	Attrs   FileAttrs
	Changes []ChangeRow
	Media   []MediaMeta
	Objects []ImportObject
	// RawData is the raw file.data blob from data.bin, if present.
	RawData []byte
	// PageIDs from pages.json, if present.
	PageIDs []string
}

// ImportObject is a storage object read from the ZIP.
type ImportObject struct {
	Meta StorageMeta
	Data []byte
}

// ─── Format detection ─────────────────────────────────────────────────────────

// ParseFormat inspects the first 4 bytes of the supplied bytes and returns
// FormatV1 or FormatV3.
func ParseFormat(header []byte) int {
	if len(header) >= 4 &&
		header[0] == 'P' && header[1] == 'K' &&
		header[2] == 0x03 && header[3] == 0x04 {
		return FormatV3
	}
	return FormatV1
}

// ─── ZIP writer ───────────────────────────────────────────────────────────────

// WriteZIP encodes a single-file export as a .penpot ZIP and writes it to w.
func WriteZIP(w io.Writer, p ExportPayload) error {
	zw := zip.NewWriter(w)
	defer zw.Close()

	fileID := p.Attrs.ID

	// ── manifest.json ─────────────────────────────────────────────────────
	manifest := Manifest{
		Type:        "penpot/export-files",
		Version:     manifestVersion,
		GeneratedBy: generatedBy,
		GoExtension: true,
		Files: []FileEntry{{
			ID:       fileID,
			Name:     p.Attrs.Name,
			Features: []string{},
		}},
	}
	if err := writeJSON(zw, "manifest.json", manifest); err != nil {
		return err
	}

	// ── files/{id}/attrs.json ─────────────────────────────────────────────
	if err := writeJSON(zw, path("files", fileID, "attrs.json"), p.Attrs); err != nil {
		return err
	}

	// ── files/{id}/pages.json ─────────────────────────────────────────────
	if err := writeJSON(zw, path("files", fileID, "pages.json"), p.PageIDs); err != nil {
		return err
	}

	// ── files/{id}/changes.json ───────────────────────────────────────────
	if err := writeJSON(zw, path("files", fileID, "changes.json"), p.Changes); err != nil {
		return err
	}

	// ── files/{id}/data.bin (raw CRDT blob when present) ──────────────────
	if len(p.RawData) > 0 {
		if err := writeBytes(zw, path("files", fileID, "data.bin"), p.RawData); err != nil {
			return err
		}
	}

	// ── media/{media-id}.json ─────────────────────────────────────────────
	for _, m := range p.Media {
		if err := writeJSON(zw, fmt.Sprintf("media/%s.json", m.MediaID), m); err != nil {
			return err
		}
	}

	// ── objects/{id}.json + objects/{id}{ext} ─────────────────────────────
	for _, obj := range p.Objects {
		meta := StorageMeta{
			ID:          obj.ID,
			ContentType: obj.ContentType,
			Size:        int64(len(obj.Data)),
			Bucket:      obj.Bucket,
		}
		if err := writeJSON(zw, fmt.Sprintf("objects/%s.json", obj.ID), meta); err != nil {
			return err
		}
		ext := extForContentType(obj.ContentType)
		if err := writeBytes(zw, fmt.Sprintf("objects/%s%s", obj.ID, ext), obj.Data); err != nil {
			return err
		}
	}

	return zw.Close()
}

// ─── ZIP reader ───────────────────────────────────────────────────────────────

// ReadZIP parses a .penpot ZIP from the supplied bytes.
// Returns ErrFormatV1NotSupported if the bytes represent a v1 binary file.
func ReadZIP(data []byte) (*ImportPayload, error) {
	if len(data) < 4 {
		return nil, fmt.Errorf("binfile: file too short")
	}
	if ParseFormat(data[:4]) == FormatV1 {
		return nil, ErrFormatV1NotSupported
	}

	zr, err := zip.NewReader(bytes.NewReader(data), int64(len(data)))
	if err != nil {
		return nil, fmt.Errorf("binfile: open zip: %w", err)
	}

	// Index ZIP entries by name.
	entries := make(map[string]*zip.File, len(zr.File))
	for _, f := range zr.File {
		entries[f.Name] = f
	}

	// ── manifest.json ─────────────────────────────────────────────────────
	manifest, err := readJSONEntry[Manifest](entries, "manifest.json")
	if err != nil {
		return nil, fmt.Errorf("binfile: read manifest: %w", err)
	}
	if manifest.Type != "penpot/export-files" {
		return nil, fmt.Errorf("binfile: unexpected manifest type %q", manifest.Type)
	}

	payload := &ImportPayload{Manifest: manifest}

	// ── Per-file sections ─────────────────────────────────────────────────
	for _, fe := range manifest.Files {
		pf, err := parseFileSection(entries, fe.ID)
		if err != nil {
			return nil, fmt.Errorf("binfile: parse file %s: %w", fe.ID, err)
		}
		payload.Files = append(payload.Files, pf)
	}

	return payload, nil
}

// parseFileSection reads all entries belonging to one file from the ZIP index.
func parseFileSection(entries map[string]*zip.File, fileID string) (ParsedFile, error) {
	var pf ParsedFile

	// attrs.json (required)
	attrs, err := readJSONEntry[FileAttrs](entries, path("files", fileID, "attrs.json"))
	if err != nil {
		return pf, fmt.Errorf("attrs.json: %w", err)
	}
	pf.Attrs = attrs

	// pages.json (optional — may be absent in Clojure-exported ZIPs)
	if e := entries[path("files", fileID, "pages.json")]; e != nil {
		var pageIDs []string
		if err := readJSON(e, &pageIDs); err == nil {
			pf.PageIDs = pageIDs
		}
	}

	// changes.json (Go extension; optional)
	if e := entries[path("files", fileID, "changes.json")]; e != nil {
		var changes []ChangeRow
		if err := readJSON(e, &changes); err == nil {
			pf.Changes = changes
		}
	}

	// data.bin (Clojure raw blob; optional)
	if e := entries[path("files", fileID, "data.bin")]; e != nil {
		rc, err := e.Open()
		if err == nil {
			pf.RawData, _ = io.ReadAll(rc)
			rc.Close()
		}
	}

	// media/*.json — collect all media entries for this file's storage objects
	mediaPrefix := "media/"
	for name, entry := range entries {
		if !strings.HasPrefix(name, mediaPrefix) || !strings.HasSuffix(name, ".json") {
			continue
		}
		var m MediaMeta
		if err := readJSON(entry, &m); err != nil {
			continue
		}
		if m.FileID == fileID {
			pf.Media = append(pf.Media, m)
		}
	}

	// objects/*.json + matching data files
	objectsPrefix := "objects/"
	for name, entry := range entries {
		if !strings.HasPrefix(name, objectsPrefix) || !strings.HasSuffix(name, ".json") {
			continue
		}
		var meta StorageMeta
		if err := readJSON(entry, &meta); err != nil {
			continue
		}
		// Read the raw bytes file (same id, different extension).
		var raw []byte
		for ext := range extensionMap {
			candidate := fmt.Sprintf("objects/%s%s", meta.ID, ext)
			if dataEntry, ok := entries[candidate]; ok {
				rc, err := dataEntry.Open()
				if err == nil {
					raw, _ = io.ReadAll(rc)
					rc.Close()
				}
				break
			}
		}
		pf.Objects = append(pf.Objects, ImportObject{Meta: meta, Data: raw})
	}

	return pf, nil
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

func path(parts ...string) string { return strings.Join(parts, "/") }

func writeJSON(zw *zip.Writer, name string, v any) error {
	w, err := zw.Create(name)
	if err != nil {
		return err
	}
	enc := json.NewEncoder(w)
	enc.SetIndent("", "  ")
	return enc.Encode(v)
}

func writeBytes(zw *zip.Writer, name string, data []byte) error {
	w, err := zw.Create(name)
	if err != nil {
		return err
	}
	_, err = io.Copy(w, bytes.NewReader(data))
	return err
}

func readJSONEntry[T any](entries map[string]*zip.File, name string) (T, error) {
	var zero T
	e, ok := entries[name]
	if !ok {
		return zero, fmt.Errorf("entry %q not found", name)
	}
	var v T
	if err := readJSON(e, &v); err != nil {
		return zero, err
	}
	return v, nil
}

func readJSON(f *zip.File, v any) error {
	rc, err := f.Open()
	if err != nil {
		return err
	}
	defer rc.Close()
	return json.NewDecoder(rc).Decode(v)
}

// extensionMap maps known content-type suffixes to file extensions.
var extensionMap = map[string]string{
	".png":   "image/png",
	".jpg":   "image/jpeg",
	".jpeg":  "image/jpeg",
	".svg":   "image/svg+xml",
	".gif":   "image/gif",
	".webp":  "image/webp",
	".mp4":   "video/mp4",
	".woff":  "font/woff",
	".woff2": "font/woff2",
	".otf":   "font/otf",
	".ttf":   "font/ttf",
	".bin":   "application/octet-stream",
}

// extForContentType returns a file extension for the given MIME type.
func extForContentType(ct string) string {
	ct = strings.ToLower(strings.Split(ct, ";")[0])
	switch ct {
	case "image/png":
		return ".png"
	case "image/jpeg":
		return ".jpg"
	case "image/svg+xml":
		return ".svg"
	case "image/gif":
		return ".gif"
	case "image/webp":
		return ".webp"
	case "video/mp4":
		return ".mp4"
	case "font/woff":
		return ".woff"
	case "font/woff2":
		return ".woff2"
	case "font/otf":
		return ".otf"
	case "font/ttf":
		return ".ttf"
	}
	return ".bin"
}
