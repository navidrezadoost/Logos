package main

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

type response struct {
	ProtocolVersion int               `json:"protocolVersion"`
	OK              bool              `json:"ok"`
	Phase           string            `json:"phase"`
	Document        json.RawMessage   `json:"document"`
	Diagnostics     []json.RawMessage `json:"diagnostics"`
	IDs             []string          `json:"ids"`
}

func fixture(t *testing.T, name string) []byte {
	t.Helper()
	data, err := os.ReadFile(filepath.Join("..", "fixtures", name))
	if err != nil {
		t.Fatal(err)
	}
	return data
}
func runFixture(t *testing.T, name string) (response, []byte) {
	t.Helper()
	input := fixture(t, name)
	output, err := invokeRustPlanner(input)
	if err != nil {
		t.Fatal(err)
	}
	var got response
	if err := json.Unmarshal(output, &got); err != nil {
		t.Fatalf("invalid response: %v: %s", err, output)
	}
	if got.ProtocolVersion != 1 {
		t.Fatalf("protocol version = %d", got.ProtocolVersion)
	}
	return got, output
}

func TestNativeRustPlannerReordersRootsAndPreservesShapeIdentities(t *testing.T) {
	got, _ := runFixture(t, "reorder.json")
	if !got.OK {
		t.Fatalf("planner rejected valid reorder: %s", got.Diagnostics)
	}
	var document struct {
		Pages []struct {
			RootShapeIDs []string `json:"root-shape-ids"`
		} `json:"pages"`
		Shapes []struct {
			ID string `json:"id"`
		} `json:"shapes"`
	}
	if err := json.Unmarshal(got.Document, &document); err != nil {
		t.Fatal(err)
	}
	want := []string{"66666666-6666-4666-8666-666666666666", "33333333-3333-4333-8333-333333333333"}
	if len(document.Pages) != 1 || !bytes.Equal(mustJSON(t, document.Pages[0].RootShapeIDs), mustJSON(t, want)) {
		t.Fatalf("root order = %v, want %v", document.Pages, want)
	}
	if len(document.Shapes) != 5 {
		t.Fatalf("shape count/identities not preserved: %d", len(document.Shapes))
	}
}

func TestRejectedCycleIsStableAndDoesNotProduceCandidate(t *testing.T) {
	a, rawA := runFixture(t, "cycle-reject.json")
	_, rawB := runFixture(t, "cycle-reject.json")
	if a.OK || a.Phase != "operation-rejected" || len(a.Diagnostics) == 0 {
		t.Fatalf("unexpected cycle result: %s", rawA)
	}
	if !bytes.Equal(rawA, rawB) {
		t.Fatalf("unstable repeated diagnostics:\n%s\n%s", rawA, rawB)
	}
	var first map[string]any
	if err := json.Unmarshal(a.Diagnostics[0], &first); err != nil {
		t.Fatal(err)
	}
	if first["code"] != "CYCLE" {
		t.Fatalf("diagnostic = %v", first)
	}
}

func TestProjectionIdentityFixtureRelations(t *testing.T) {
	var cases []struct {
		Name     string          `json:"name"`
		Request  json.RawMessage `json:"request"`
		Relation string          `json:"relation"`
	}
	if err := json.Unmarshal(fixture(t, "projection-identities.json"), &cases); err != nil {
		t.Fatal(err)
	}
	for _, tc := range cases {
		t.Run(tc.Name, func(t *testing.T) {
			got, err := invokeRustPlanner(tc.Request)
			if err != nil {
				t.Fatal(err)
			}
			var result response
			if err := json.Unmarshal(got, &result); err != nil {
				t.Fatal(err)
			}
			if !result.OK || len(result.IDs) != 2 {
				t.Fatalf("bad identity response: %s", got)
			}
			equal := result.IDs[0] == result.IDs[1]
			if (tc.Relation == "same") != equal {
				t.Fatalf("IDs %v relation=%s", result.IDs, tc.Relation)
			}
		})
	}
}

func TestDetachAllocationFixtureAndCollision(t *testing.T) {
	ok, _ := runFixture(t, "detach-ids.json")
	if !ok.OK || len(ok.IDs) != 2 {
		t.Fatalf("detach IDs rejected: %+v", ok)
	}
	bad, _ := runFixture(t, "detach-collision.json")
	if bad.OK || len(bad.Diagnostics) == 0 {
		t.Fatalf("collision accepted: %+v", bad)
	}
}

func TestUnsupportedProtocolFailsClosed(t *testing.T) {
	got, _ := invokeRustPlanner([]byte(`{"protocolVersion":2,"kind":"deriveIds","addresses":[]}`))
	var result response
	if err := json.Unmarshal(got, &result); err != nil {
		t.Fatal(err)
	}
	if result.OK || len(result.Diagnostics) == 0 {
		t.Fatalf("unsupported protocol accepted: %s", got)
	}
}

func mustJSON(t *testing.T, v any) []byte {
	t.Helper()
	b, err := json.Marshal(v)
	if err != nil {
		t.Fatal(err)
	}
	return b
}
