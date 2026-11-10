package filedata

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

func TestP0CompatibilityFixtureUnknownExtensionAndTreeOrderSurvive(t *testing.T) {
	fixturePath := filepath.Join("..", "..", "..", "test", "fixtures", "p0-document-compat", "source-document.json")
	raw, err := os.ReadFile(fixturePath)
	if err != nil {
		t.Fatalf("read P0 compatibility fixture: %v", err)
	}
	var document map[string]any
	if err := json.Unmarshal(raw, &document); err != nil {
		t.Fatalf("decode P0 compatibility fixture: %v", err)
	}

	const pageID = "f0000000-0000-4000-8000-000000000002"
	const shapeID = "f0000000-0000-4000-8000-000000000003"
	if !NormalizeFileData(document) {
		t.Fatal("expected legacy enum normalization to be reported")
	}

	encoded, err := EncodeJSON(document)
	if err != nil {
		t.Fatalf("encode normalized document: %v", err)
	}
	var reopened map[string]any
	if err := json.Unmarshal(encoded, &reopened); err != nil {
		t.Fatalf("decode normalized document: %v", err)
	}
	page := FirstPage(reopened)
	if page == nil || page["id"] != pageID {
		t.Fatalf("page identity did not survive JSON round trip: %#v", page)
	}
	objects, ok := page["objects"].(map[string]any)
	if !ok {
		t.Fatalf("page objects missing after JSON round trip: %#v", page["objects"])
	}
	root, ok := objects[RootShapeID].(map[string]any)
	if !ok {
		t.Fatalf("root frame missing after JSON round trip")
	}
	rootChildren, ok := root["shapes"].([]any)
	if !ok || len(rootChildren) != 1 || rootChildren[0] != shapeID {
		t.Fatalf("ordered root child reference lost: %#v", root["shapes"])
	}
	shape, ok := objects[shapeID].(map[string]any)
	if !ok {
		t.Fatalf("rectangle missing after JSON round trip")
	}
	if shape["type"] != Kw("rect") {
		t.Fatalf("rectangle discriminant = %v, want %s", shape["type"], Kw("rect"))
	}
	extension, ok := shape["extension-p0"].(map[string]any)
	if !ok || extension["sentinel"] != "preserve-me" {
		t.Fatalf("unknown source extension was not preserved: %#v", shape["extension-p0"])
	}
}
