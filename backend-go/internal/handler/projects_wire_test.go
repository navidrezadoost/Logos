package handler

import (
	"encoding/json"
	"testing"
	"time"
)

func TestProjectJSONUsesKebabCaseKeys(t *testing.T) {
	now := time.Date(2026, 5, 30, 12, 0, 0, 0, time.UTC)
	p := Project{
		ID: "428cd363-180f-4b38-95c9-58fc9e95a293",
		TeamID: "b8b5d9a9-5b1f-4b53-b49c-6d0965227c75",
		Name: "Drafts",
		IsDefault: true,
		IsPinned: false,
		Count: 1,
		TotalCount: 1,
		CreatedAt: now,
		ModifiedAt: now,
	}

	b, err := json.Marshal(p)
	if err != nil {
		t.Fatal(err)
	}
	var m map[string]any
	if err := json.Unmarshal(b, &m); err != nil {
		t.Fatal(err)
	}
	for _, key := range []string{"team-id", "is-default", "is-pinned", "total-count", "created-at", "modified-at"} {
		if _, ok := m[key]; !ok {
			t.Fatalf("missing %q in %v", key, m)
		}
	}
}
