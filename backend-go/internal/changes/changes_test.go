package changes_test

import (
	"encoding/json"
	"testing"

	"github.com/logos-design/logos/backend-go/internal/changes"
	"github.com/logos-design/logos/backend-go/internal/filedata"
	"github.com/logos-design/logos/backend-go/internal/rebase"
)

func TestProcessChangesAddAndModObj(t *testing.T) {
	pageID := "11111111-2222-3333-4444-555555555555"
	data := filedata.BuildEmptyData("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee", pageID)
	shapeID := "22222222-3333-4444-5555-666666666666"

	obj := map[string]any{
		"~#shape": map[string]any{
			"type":     "rect",
			"name":     "Rectangle",
			"x":        10.0,
			"y":        20.0,
			"width":    100.0,
			"height":   80.0,
			"transform": map[string]any{
				"a": 1.0, "b": 0.0, "c": 0.0, "d": 1.0, "e": 0.0, "f": 0.0,
			},
			"points": []any{
				[]any{10.0, 20.0},
				[]any{110.0, 20.0},
				[]any{110.0, 100.0},
				[]any{10.0, 100.0},
			},
			"fills":    []any{map[string]any{"fill-color": "#FFFFFF", "fill-opacity": 1}},
			"strokes":  []any{},
			"rotation": 0,
		},
	}
	objJSON, err := json.Marshal(obj)
	if err != nil {
		t.Fatal(err)
	}

	batch := []rebase.Change{
		{
			Type:     rebase.TypeAddObj,
			ID:       shapeID,
			PageID:   pageID,
			ParentID: filedata.RootShapeID,
			Obj:      objJSON,
		},
		{
			Type:   rebase.TypeModObj,
			ID:     shapeID,
			PageID: pageID,
			Operations: []rebase.SetOp{
				{Attr: "fills", Val: json.RawMessage(`[{"fill-color":"#112769","fill-opacity":1}]`)},
			},
		},
	}

	out, err := changes.ProcessChanges(data, batch)
	if err != nil {
		t.Fatal(err)
	}
	page := filedata.FirstPage(out)
	objects := page["objects"].(map[string]any)
	shape := objects[shapeID].(map[string]any)
	if shape["type"] != filedata.Kw("rect") {
		t.Fatalf("type = %v, want %q", shape["type"], filedata.Kw("rect"))
	}
	if _, ok := shape["transform-inverse"].(map[string]any); !ok {
		t.Fatalf("transform-inverse missing: %#v", shape["transform-inverse"])
	}
	fills, ok := shape["fills"].([]any)
	if !ok || len(fills) == 0 {
		t.Fatalf("fills = %#v", shape["fills"])
	}
	fill := fills[0].(map[string]any)
	if fill["fill-color"] != "#112769" {
		t.Fatalf("fill-color = %v", fill["fill-color"])
	}
	root := objects[filedata.RootShapeID].(map[string]any)
	rootShapes := root["shapes"]
	switch rs := rootShapes.(type) {
	case []any:
		if len(rs) != 1 || rs[0] != shapeID {
			t.Fatalf("root shapes = %#v", rs)
		}
	case []string:
		if len(rs) != 1 || rs[0] != shapeID {
			t.Fatalf("root shapes = %#v", rs)
		}
	default:
		t.Fatalf("root shapes = %#v", rootShapes)
	}
}

func TestUnwrapTaggedShape(t *testing.T) {
	raw := map[string]any{
		"~#matrix": map[string]any{"a": 1.0, "b": 0.0, "c": 0.0, "d": 1.0, "e": 0.0, "f": 0.0},
	}
	out := changes.Unwrap(raw).(map[string]any)
	if out["a"].(float64) != 1.0 {
		t.Fatalf("matrix = %#v", out)
	}
}
