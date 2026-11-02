package filedata

import (
	"strings"
	"testing"

	"github.com/logos-design/logos/backend-go/internal/transit"
)

func TestBuildEmptyDataHasPageAndRootFrame(t *testing.T) {
	fileID := "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
	data := BuildEmptyData(fileID, "11111111-2222-3333-4444-555555555555")
	if data["id"] != fileID {
		t.Fatalf("id = %v", data["id"])
	}
	pages, ok := data["pages"].([]string)
	if !ok || len(pages) != 1 {
		t.Fatalf("pages = %T %v", data["pages"], data["pages"])
	}
	page := FirstPage(data)
	if page == nil {
		t.Fatal("first page missing")
	}
	objects, ok := page["objects"].(map[string]any)
	if !ok {
		t.Fatalf("objects = %T", page["objects"])
	}
	root, ok := objects[RootShapeID].(map[string]any)
	if !ok {
		t.Fatal("root frame missing")
	}
	if root["type"] != kw("frame") {
		t.Fatalf("root type = %v, want %q", root["type"], kw("frame"))
	}
	if _, ok := root["transform"].(map[string]any); !ok {
		t.Fatalf("root transform should be matrix map, got %T", root["transform"])
	}
}

func TestRootFrameTypeEncodesAsTransitKeyword(t *testing.T) {
	data := BuildEmptyData("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee", "11111111-2222-3333-4444-555555555555")
	out, err := transit.JSONToTransit(mustJSON(data))
	if err != nil {
		t.Fatal(err)
	}
	s := string(out)
	if !strings.Contains(s, "~:frame") {
		t.Fatalf("expected ~:frame in transit output:\n%s", s)
	}
	if strings.Contains(s, `"frame"`) {
		t.Fatalf("root type must not remain plain string in transit output:\n%s", s)
	}
}

func TestNormalizeFileDataFixesLegacyRootFrame(t *testing.T) {
	legacy := map[string]any{
		"id": "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
		"pages": []string{"11111111-2222-3333-4444-555555555555"},
		"pages-index": map[string]any{
			"11111111-2222-3333-4444-555555555555": map[string]any{
				"id":   "11111111-2222-3333-4444-555555555555",
				"name": "Page 1",
				"objects": map[string]any{
					RootShapeID: map[string]any{
						"id":        RootShapeID,
						"type":      "frame",
						"parent-id": RootShapeID,
						"frame-id":  RootShapeID,
						"transform": map[string]any{"a": 1.0, "b": 0.0, "c": 0.0, "d": 1.0, "e": 0.0, "f": 0.0},
						"points": []any{
							map[string]any{"x": 0.0, "y": 0.0},
						},
					},
				},
			},
		},
	}

	if !NormalizeFileData(legacy) {
		t.Fatal("expected normalization to report changes")
	}
	page := FirstPage(legacy)
	root := page["objects"].(map[string]any)[RootShapeID].(map[string]any)
	if root["type"] != kw("frame") {
		t.Fatalf("type after normalize = %v", root["type"])
	}
	if _, ok := root["transform"].(map[string]any); !ok {
		t.Fatalf("transform should be matrix map, got %T: %v", root["transform"], root["transform"])
	}
}

func TestNormalizeShapeKeywordsEllipseTransit(t *testing.T) {
	shape := map[string]any{
		"id":   "22222222-3333-4444-5555-666666666666",
		"type": "ellipse",
		"name": "Ellipse",
		"fills": []any{
			map[string]any{"fill-color": "#112769", "fill-opacity": 1},
		},
	}
	if !NormalizeShape(shape) {
		t.Fatal("expected keyword normalization")
	}
	if shape["type"] != Kw("ellipse") {
		t.Fatalf("type = %v, want %q", shape["type"], Kw("ellipse"))
	}

	data := BuildEmptyData("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee", "11111111-2222-3333-4444-555555555555")
	page := FirstPage(data)
	objects := page["objects"].(map[string]any)
	objects[shape["id"].(string)] = shape
	NormalizeFileData(data)

	out, err := transit.JSONToTransit(mustJSON(data))
	if err != nil {
		t.Fatal(err)
	}
	s := string(out)
	if !strings.Contains(s, "~:ellipse") {
		t.Fatalf("expected ~:ellipse in transit output:\n%s", s)
	}
}

func mustJSON(v map[string]any) []byte {
	b, err := EncodeJSON(v)
	if err != nil {
		panic(err)
	}
	return b
}
