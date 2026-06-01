// Package filedata builds Penpot-compatible empty file/page structures for the
// Go backend when creating files or repairing rows that have metadata only.
package filedata

import (
	"crypto/rand"
	"encoding/json"
	"fmt"
	"regexp"
	"strings"
)

var uuidRE = regexp.MustCompile(`(?i)^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$`)

const (
	RootShapeID  = "00000000-0000-0000-0000-000000000000"
	FileVersion  = 67
	BaseFontSize = "16"
)

// DefaultFeatures matches the feature set requested by the compiled frontend
// when creating a new file.
var DefaultFeatures = []string{
	"fdata/path-data",
	"design-tokens/v1",
	"variants/v1",
	"layout/grid",
	"styles/v2",
	"fdata/objects-map",
	"fdata/shape-data-type",
	"components/v2",
	"plugins/runtime",
}

// NewPageID returns a random UUID v4 for a new page.
func NewPageID() string { return newUUID() }

func newUUID() string {
	var b [16]byte
	_, _ = rand.Read(b[:])
	b[6] = (b[6] & 0x0f) | 0x40
	b[8] = (b[8] & 0x3f) | 0x80
	return fmt.Sprintf("%08x-%04x-%04x-%04x-%012x", b[0:4], b[4:6], b[6:8], b[8:10], b[10:16])
}

// Kw returns a Transit keyword token for Penpot enum fields (e.g. :frame).
// Plain JSON strings like "frame" are not decoded as keywords by the frontend.
func Kw(name string) string { return "~:" + name }

func kw(name string) string { return Kw(name) }

// BuildEmptyData returns a Penpot file-data map with one empty page and root frame.
func BuildEmptyData(fileID, pageID string) map[string]any {
	if pageID == "" {
		pageID = NewPageID()
	}
	page := map[string]any{
		"id":   pageID,
		"name": "Page 1",
		"objects": map[string]any{
			RootShapeID: rootFrameShape(),
		},
	}
	return map[string]any{
		"id":    fileID,
		"pages": []string{pageID},
		"pages-index": map[string]any{
			pageID: page,
		},
		"options": map[string]any{
			"components-v2":  true,
			"base-font-size": BaseFontSize,
		},
	}
}

// FirstPage returns the first page map from file data, if present.
func FirstPage(data map[string]any) map[string]any {
	if data == nil {
		return nil
	}
	var pageID string
	switch pages := data["pages"].(type) {
	case []any:
		if len(pages) == 0 {
			return nil
		}
		pageID, _ = pages[0].(string)
	case []string:
		if len(pages) == 0 {
			return nil
		}
		pageID = pages[0]
	default:
		return nil
	}
	if pageID == "" {
		return nil
	}
	index, ok := data["pages-index"].(map[string]any)
	if !ok {
		return nil
	}
	page, _ := index[pageID].(map[string]any)
	return page
}

// NormalizeFileData repairs Penpot file blobs produced by earlier Go versions.
// Returns true when data was modified and should be persisted.
func NormalizeFileData(data map[string]any) bool {
	if data == nil {
		return false
	}
	index, ok := data["pages-index"].(map[string]any)
	if !ok {
		return false
	}
	changed := false
	for _, pageRaw := range index {
		page, ok := pageRaw.(map[string]any)
		if !ok {
			continue
		}
		objects, ok := page["objects"].(map[string]any)
		if !ok {
			continue
		}
		for id, objRaw := range objects {
			shape, ok := objRaw.(map[string]any)
			if !ok {
				continue
			}
			if id == RootShapeID && needsLegacyRootRepair(shape) {
				if normalizeRootFrame(shape) {
					changed = true
				}
			}
			if NormalizeShape(shape) {
				changed = true
			}
		}
	}
	return changed
}

// NormalizeShapeKeywords ensures Penpot enum fields on a shape use Transit keyword tokens.
func NormalizeShapeKeywords(shape map[string]any) bool {
	return normalizeShapeKeywords(shape)
}

// normalizeShapeKeywords ensures Penpot enum fields use Transit keyword tokens (~:…).
// Without this, get-file returns plain strings and the editor treats shapes as rects.
func normalizeShapeKeywords(shape map[string]any) bool {
	if shape == nil {
		return false
	}
	changed := false
	for _, key := range []string{
		"type", "bool-type", "constraints-h", "constraints-v", "blend-mode",
		"vertical-align", "grow-type", "shape-blur-type",
	} {
		if ensureKeywordField(shape, key) {
			changed = true
		}
	}
	if strokes, ok := shape["strokes"].([]any); ok {
		strokesChanged := false
		for i, item := range strokes {
			stroke, ok := item.(map[string]any)
			if !ok {
				continue
			}
			itemChanged := false
			for _, key := range []string{
				"stroke-style", "stroke-alignment", "stroke-cap", "stroke-linejoin",
			} {
				if ensureKeywordField(stroke, key) {
					itemChanged = true
				}
			}
			if itemChanged {
				strokes[i] = stroke
				strokesChanged = true
			}
		}
		if strokesChanged {
			shape["strokes"] = strokes
			changed = true
		}
	}
	if fills, ok := shape["fills"].([]any); ok {
		fillsChanged := false
		for i, item := range fills {
			fill, ok := item.(map[string]any)
			if !ok {
				continue
			}
			if ensureKeywordField(fill, "fill-color-type") {
				fills[i] = fill
				fillsChanged = true
			}
		}
		if fillsChanged {
			shape["fills"] = fills
			changed = true
		}
	}
	return changed
}

func ensureKeywordField(m map[string]any, key string) bool {
	v, ok := m[key]
	if !ok || v == nil {
		return false
	}
	s, ok := v.(string)
	if !ok || s == "" {
		return false
	}
	if strings.HasPrefix(s, "~:") {
		return false
	}
	if uuidRE.MatchString(s) || strings.HasPrefix(s, "#") {
		return false
	}
	m[key] = Kw(s)
	return true
}

// RootFrameDebug returns type/id strings for logging after load/repair.
func RootFrameDebug(data map[string]any) (typeVal, idVal string, ok bool) {
	page := FirstPage(data)
	if page == nil {
		return "", "", false
	}
	objects, _ := page["objects"].(map[string]any)
	root, _ := objects[RootShapeID].(map[string]any)
	if root == nil {
		return "", "", false
	}
	typeVal, _ = root["type"].(string)
	idVal, _ = root["id"].(string)
	return typeVal, idVal, true
}

func needsLegacyRootRepair(root map[string]any) bool {
	if root["type"] == "frame" {
		return true
	}
	if points, ok := root["points"].([]any); ok && len(points) > 0 {
		if _, isArr := points[0].([]any); isArr {
			return true
		}
	}
	if _, isArr := root["transform"].([]any); isArr {
		return true
	}
	return false
}

func normalizeRootFrame(root map[string]any) bool {
	if !needsLegacyRootRepair(root) {
		return false
	}
	shapes := root["shapes"]
	repaired := rootFrameShape()
	if shapes != nil {
		repaired["shapes"] = shapes
	}
	for k, v := range repaired {
		root[k] = v
	}
	return true
}

// EncodeJSON marshals file data for storage in file.data.
func EncodeJSON(data map[string]any) ([]byte, error) {
	return json.Marshal(data)
}

func rootFrameShape() map[string]any {
	const w = 0.01
	const h = 0.01
	selrect := map[string]any{"x": 0.0, "y": 0.0, "width": w, "height": h, "x1": 0.0, "y1": 0.0, "x2": w, "y2": h}
	points := []any{
		map[string]any{"x": 0.0, "y": 0.0},
		map[string]any{"x": w, "y": 0.0},
		map[string]any{"x": w, "y": h},
		map[string]any{"x": 0.0, "y": h},
	}
	matrix := matrixMap(1, 0, 0, 1, 0, 0)
	return map[string]any{
		"id":                  RootShapeID,
		"type":                kw("frame"),
		"name":                "Root Frame",
		"parent-id":           RootShapeID,
		"frame-id":            RootShapeID,
		"x":                   0.0,
		"y":                   0.0,
		"width":               w,
		"height":              h,
		"rotation":            0.0,
		"selrect":             selrect,
		"points":              points,
		"transform":           matrix,
		"transform-inverse":   matrix,
		"fills":               []map[string]any{{"fill-color": "#FFFFFF", "fill-opacity": 1}},
		"strokes":             []any{},
		"shapes":              []any{},
		"r1":                  0,
		"r2":                  0,
		"r3":                  0,
		"r4":                  0,
		"show-content":        true,
		"hide-in-viewer":      false,
		"hide-fill-on-export": false,
	}
}
