package handler

import (
	"encoding/json"
	"testing"
)

func TestCreateFileBodyParsesProjectIDKebabCase(t *testing.T) {
	body := map[string]any{
		"project-id": "428cd363-180f-4b38-95c9-58fc9e95a293",
		"name":       "New File 1",
		"features": map[string]any{
			"~#set": []any{"layout/grid", "variants/v1"},
		},
	}
	if got := jsonFieldString(body, "projectId", "project-id"); got == "" {
		t.Fatal("project-id not parsed")
	}
	if got := jsonFieldStringSlice(body, "features"); len(got) != 2 {
		t.Fatalf("features = %v", got)
	}
}

func TestFileJSONUsesKebabCaseKeys(t *testing.T) {
	f := File{ID: "x", ProjectID: "p", Name: "n", IsShared: false}
	b, err := json.Marshal(f)
	if err != nil {
		t.Fatal(err)
	}
	var m map[string]any
	if err := json.Unmarshal(b, &m); err != nil {
		t.Fatal(err)
	}
	for _, key := range []string{"project-id", "is-shared", "comment-thread-seqn"} {
		if _, ok := m[key]; !ok {
			t.Fatalf("missing %q in %v", key, m)
		}
	}
}
