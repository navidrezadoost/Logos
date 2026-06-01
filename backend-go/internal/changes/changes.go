// Package changes applies Penpot file change-sets to the JSON file.data blob.
package changes

import (
	"encoding/json"
	"fmt"

	"github.com/logos-design/logos/backend-go/internal/filedata"
	"github.com/logos-design/logos/backend-go/internal/rebase"
)

// ProcessChanges applies a batch of changes to file data and returns the updated map.
// Unsupported change types are skipped without error.
func ProcessChanges(data map[string]any, items []rebase.Change) (map[string]any, error) {
	if data == nil {
		return nil, fmt.Errorf("file data is nil")
	}
	for _, ch := range items {
		if err := applyChange(data, ch); err != nil {
			return data, err
		}
	}
	return data, nil
}

func applyChange(data map[string]any, ch rebase.Change) error {
	switch ch.Type {
	case rebase.TypeAddObj:
		return applyAddObj(data, ch)
	case rebase.TypeModObj:
		return applyModObj(data, ch)
	case rebase.TypeDelObj:
		return applyDelObj(data, ch)
	case rebase.TypeMovObjects:
		return applyMovObjects(data, ch)
	default:
		return nil
	}
}

func applyAddObj(data map[string]any, ch rebase.Change) error {
	if ch.PageID == "" || ch.ID == "" || len(ch.Obj) == 0 {
		return nil
	}
	page, ok := pageByID(data, ch.PageID)
	if !ok {
		return nil
	}
	objects, ok := objectsMap(page)
	if !ok {
		return nil
	}

	var raw any
	if err := json.Unmarshal(ch.Obj, &raw); err != nil {
		return fmt.Errorf("add-obj decode: %w", err)
	}
	shape, ok := Unwrap(raw).(map[string]any)
	if !ok {
		return fmt.Errorf("add-obj shape is not a map")
	}

	parentID := firstString(ch.ParentID, stringField(shape, "parent-id"), filedata.RootShapeID)
	frameID := firstString(ch.FrameID, stringField(shape, "frame-id"), filedata.RootShapeID)
	if parentID == "" {
		parentID = frameID
	}
	if frameID == "" {
		frameID = filedata.RootShapeID
	}

	shape["id"] = ch.ID
	shape["parent-id"] = parentID
	shape["frame-id"] = frameID
	objects[ch.ID] = shape
	filedata.NormalizeShape(shape)

	parent, ok := objects[parentID].(map[string]any)
	if !ok {
		return nil
	}
	shapes := stringSlice(parent["shapes"])
	if !containsString(shapes, ch.ID) {
		if ch.Index >= 0 && ch.Index <= len(shapes) {
			shapes = insertAt(shapes, ch.Index, ch.ID)
		} else {
			shapes = append(shapes, ch.ID)
		}
		parent["shapes"] = shapes
	}
	return nil
}

func applyModObj(data map[string]any, ch rebase.Change) error {
	if ch.PageID == "" || ch.ID == "" {
		return nil
	}
	page, ok := pageByID(data, ch.PageID)
	if !ok {
		return nil
	}
	objects, ok := objectsMap(page)
	if !ok {
		return nil
	}
	shape, ok := objects[ch.ID].(map[string]any)
	if !ok {
		return nil
	}
	for _, op := range ch.Operations {
		if op.Attr == "" {
			continue
		}
		var val any
		if len(op.Val) > 0 && string(op.Val) != "null" {
			if err := json.Unmarshal(op.Val, &val); err != nil {
				return fmt.Errorf("mod-obj %q: %w", op.Attr, err)
			}
			val = Unwrap(val)
		} else {
			val = nil
		}
		if val == nil {
			delete(shape, op.Attr)
		} else {
			shape[op.Attr] = val
		}
	}
	objects[ch.ID] = shape
	filedata.NormalizeShape(shape)
	return nil
}

func applyDelObj(data map[string]any, ch rebase.Change) error {
	if ch.PageID == "" || ch.ID == "" {
		return nil
	}
	page, ok := pageByID(data, ch.PageID)
	if !ok {
		return nil
	}
	objects, ok := objectsMap(page)
	if !ok {
		return nil
	}

	target, ok := objects[ch.ID].(map[string]any)
	if !ok {
		return nil
	}
	parentID := firstString(stringField(target, "parent-id"), stringField(target, "frame-id"))
	toDelete := collectDescendants(objects, ch.ID)
	for _, id := range toDelete {
		delete(objects, id)
	}
	if parentID != "" {
		if parent, ok := objects[parentID].(map[string]any); ok {
			parent["shapes"] = removeString(stringSlice(parent["shapes"]), ch.ID)
		}
	}
	return nil
}

func applyMovObjects(data map[string]any, ch rebase.Change) error {
	if ch.PageID == "" || ch.ParentID == "" || len(ch.Shapes) == 0 {
		return nil
	}
	page, ok := pageByID(data, ch.PageID)
	if !ok {
		return nil
	}
	objects, ok := objectsMap(page)
	if !ok {
		return nil
	}
	parent, ok := objects[ch.ParentID].(map[string]any)
	if !ok {
		return nil
	}

	frameID := ch.ParentID
	if typeField(parent) != "frame" && typeField(parent) != "~:frame" {
		frameID = firstString(stringField(parent, "frame-id"), filedata.RootShapeID)
	}

	for _, shapeID := range ch.Shapes {
		if shape, ok := objects[shapeID].(map[string]any); ok {
			oldParentID := stringField(shape, "parent-id")
			if oldParentID != "" && oldParentID != ch.ParentID {
				if oldParent, ok := objects[oldParentID].(map[string]any); ok {
					oldParent["shapes"] = removeString(stringSlice(oldParent["shapes"]), shapeID)
				}
			}
			shape["parent-id"] = ch.ParentID
			shape["frame-id"] = frameID
			objects[shapeID] = shape
		}
	}

	shapes := stringSlice(parent["shapes"])
	for _, shapeID := range ch.Shapes {
		shapes = removeString(shapes, shapeID)
	}
	if ch.Index >= 0 && ch.Index <= len(shapes) {
		shapes = insertStringsAt(shapes, ch.Index, ch.Shapes)
	} else {
		shapes = append(shapes, ch.Shapes...)
	}
	parent["shapes"] = uniqueStrings(shapes)
	return nil
}

// NeedsReplay reports whether file.data looks stale while file_change rows exist.
func NeedsReplay(data map[string]any, revn int64) bool {
	if revn <= 0 {
		return false
	}
	page := filedata.FirstPage(data)
	if page == nil {
		return true
	}
	objects, ok := page["objects"].(map[string]any)
	if !ok {
		return true
	}
	return len(objects) <= 1
}

func pageByID(data map[string]any, pageID string) (map[string]any, bool) {
	index, ok := data["pages-index"].(map[string]any)
	if !ok {
		return nil, false
	}
	page, ok := index[pageID].(map[string]any)
	return page, ok
}

func objectsMap(page map[string]any) (map[string]any, bool) {
	objects, ok := page["objects"].(map[string]any)
	return objects, ok
}

func collectDescendants(objects map[string]any, rootID string) []string {
	var out []string
	var walk func(string)
	walk = func(id string) {
		shape, ok := objects[id].(map[string]any)
		if !ok {
			return
		}
		for _, childID := range stringSlice(shape["shapes"]) {
			walk(childID)
		}
		out = append(out, id)
	}
	walk(rootID)
	return out
}

func stringField(m map[string]any, key string) string {
	if m == nil {
		return ""
	}
	switch v := m[key].(type) {
	case string:
		return v
	default:
		return fmt.Sprint(v)
	}
}

func typeField(m map[string]any) string {
	if m == nil {
		return ""
	}
	if v, ok := m["type"].(string); ok {
		return v
	}
	return fmt.Sprint(m["type"])
}

func firstString(values ...string) string {
	for _, v := range values {
		if v != "" {
			return v
		}
	}
	return ""
}

func stringSlice(v any) []string {
	switch s := v.(type) {
	case []string:
		return append([]string(nil), s...)
	case []any:
		out := make([]string, 0, len(s))
		for _, item := range s {
			if str, ok := item.(string); ok && str != "" {
				out = append(out, str)
			}
		}
		return out
	default:
		return nil
	}
}

func containsString(list []string, id string) bool {
	for _, v := range list {
		if v == id {
			return true
		}
	}
	return false
}

func insertAt(list []string, index int, value string) []string {
	if index < 0 || index > len(list) {
		return append(list, value)
	}
	out := make([]string, 0, len(list)+1)
	out = append(out, list[:index]...)
	out = append(out, value)
	out = append(out, list[index:]...)
	return out
}

func insertStringsAt(list []string, index int, values []string) []string {
	if index < 0 || index > len(list) {
		return append(list, values...)
	}
	out := make([]string, 0, len(list)+len(values))
	out = append(out, list[:index]...)
	out = append(out, values...)
	out = append(out, list[index:]...)
	return out
}

func removeString(list []string, value string) []string {
	out := list[:0]
	for _, v := range list {
		if v != value {
			out = append(out, v)
		}
	}
	return out
}

func uniqueStrings(list []string) []string {
	seen := make(map[string]struct{}, len(list))
	out := make([]string, 0, len(list))
	for _, v := range list {
		if v == "" {
			continue
		}
		if _, ok := seen[v]; ok {
			continue
		}
		seen[v] = struct{}{}
		out = append(out, v)
	}
	return out
}
