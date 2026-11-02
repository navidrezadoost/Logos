package rebase_test

import (
	"encoding/json"
	"testing"

	. "github.com/logos-design/logos/backend-go/internal/rebase"
)

// Test IDs mirror the Rust test constants so results can be compared directly.
const (
	idA      = "00000000-0000-0000-0000-000000000065" // 101
	idB      = "00000000-0000-0000-0000-000000000066" // 102
	idC      = "00000000-0000-0000-0000-000000000067" // 103
	parentID = "00000000-0000-0000-0000-0000000000c9" // 201
	pageID1  = "00000000-0000-0000-0000-00000000012d" // 301
)

// ─── Constructors ─────────────────────────────────────────────────────────────

func rawJSON(s string) json.RawMessage { return json.RawMessage(s) }

func modObj(id string, ops ...SetOp) Change {
	return Change{Type: TypeModObj, ID: id, PageID: pageID1, Operations: ops}
}
func delObj(id string) Change { return Change{Type: TypeDelObj, ID: id, PageID: pageID1} }
func addObj(id, parent string, index int) Change {
	return Change{Type: TypeAddObj, ID: id, PageID: pageID1, ParentID: parent, Index: index}
}
func movObjects(shapes []string, parent string, index int) Change {
	return Change{Type: TypeMovObjects, PageID: pageID1, Shapes: shapes, ParentID: parent, Index: index}
}
func setOp(attr, val string) SetOp { return SetOp{Attr: attr, Val: rawJSON(`"` + val + `"`)} }

func rebase(changes []Change, competing [][]Change) []Change {
	return RebaseChangeSet(changes, competing)
}

// ─── 1. Identity / idempotency ────────────────────────────────────────────────

func TestRebaseAgainstEmptyCompeting(t *testing.T) {
	cs := []Change{modObj(idA, setOp("name", "foo")), delObj(idB)}
	result := rebase(cs, nil)
	if len(result) != 2 {
		t.Errorf("expected 2 changes, got %d", len(result))
	}
}

func TestRebaseEmptyChangeSetIsEmpty(t *testing.T) {
	result := rebase(nil, [][]Change{{delObj(idA)}})
	if len(result) != 0 {
		t.Errorf("expected 0 changes, got %d", len(result))
	}
}

func TestRebaseChangeSingleNoConflict(t *testing.T) {
	ch := modObj(idA, setOp("name", "foo"))
	result := RebaseChange(ch, nil)
	if result == nil {
		t.Error("expected non-nil result")
	}
}

// ─── 2. Non-conflicting preservation ─────────────────────────────────────────

func TestDifferentShapeIdsBothPreserved(t *testing.T) {
	incoming := []Change{modObj(idA, setOp("name", "Alice"))}
	competing := [][]Change{{modObj(idB, setOp("name", "Bob"))}}
	rebased := rebase(incoming, competing)
	if len(rebased) != 1 {
		t.Errorf("expected 1 change, got %d", len(rebased))
	}
}

func TestDelObjOnDifferentIdPreserved(t *testing.T) {
	rebased := rebase([]Change{delObj(idA)}, [][]Change{{delObj(idB)}})
	if len(rebased) != 1 {
		t.Errorf("expected 1 change, got %d", len(rebased))
	}
}

// ─── 3. :mod-obj vs :mod-obj ──────────────────────────────────────────────────

func TestModObjSameAttrIncomingWins(t *testing.T) {
	incoming := []Change{modObj(idA, setOp("name", "incoming-name"))}
	competing := [][]Change{{modObj(idA, setOp("name", "competing-name"))}}
	rebased := rebase(incoming, competing)
	if len(rebased) != 1 {
		t.Fatalf("expected 1 change, got %d", len(rebased))
	}
	found := false
	for _, op := range rebased[0].Operations {
		if op.Attr == "name" {
			if string(op.Val) != `"incoming-name"` {
				t.Errorf("incoming name should win; got %s", op.Val)
			}
			found = true
		}
	}
	if !found {
		t.Error("name attribute not found in rebased operations")
	}
}

func TestModObjDifferentAttrsBothPreserved(t *testing.T) {
	incoming := []Change{modObj(idA, setOp("name", "inc-name"))}
	competing := [][]Change{{modObj(idA, setOp("fill-color", "#ff0000"))}}
	rebased := rebase(incoming, competing)
	if len(rebased) != 1 {
		t.Fatalf("expected 1 change, got %d", len(rebased))
	}
	attrs := make(map[string]bool)
	for _, op := range rebased[0].Operations {
		attrs[op.Attr] = true
	}
	if !attrs["name"] {
		t.Error("'name' attribute missing from merged operations")
	}
	if !attrs["fill-color"] {
		t.Error("'fill-color' attribute missing from merged operations")
	}
}

// ─── 4. Delete semantics ──────────────────────────────────────────────────────

func TestModObjDroppedWhenCompetingDelObjSameId(t *testing.T) {
	rebased := rebase(
		[]Change{modObj(idA, setOp("name", "foo"))},
		[][]Change{{delObj(idA)}},
	)
	if len(rebased) != 0 {
		t.Errorf("expected empty result, got %d changes", len(rebased))
	}
}

func TestDelObjKeptWhenCompetingDelObjSameId(t *testing.T) {
	rebased := rebase([]Change{delObj(idA)}, [][]Change{{delObj(idA)}})
	if len(rebased) != 1 {
		t.Errorf("expected 1 change (idempotent delete), got %d", len(rebased))
	}
}

func TestDelObjPreservedWhenCompetingModObjSameId(t *testing.T) {
	rebased := rebase(
		[]Change{delObj(idA)},
		[][]Change{{modObj(idA, setOp("name", "bar"))}},
	)
	if len(rebased) != 1 {
		t.Fatalf("expected 1 change, got %d", len(rebased))
	}
	if rebased[0].Type != TypeDelObj {
		t.Errorf("expected del-obj, got %s", rebased[0].Type)
	}
}

func TestAddObjPreservedAfterCompetingDelObjSameId(t *testing.T) {
	rebased := rebase(
		[]Change{addObj(idA, parentID, 0)},
		[][]Change{{delObj(idA)}},
	)
	if len(rebased) != 1 {
		t.Fatalf("expected 1 change, got %d", len(rebased))
	}
	if rebased[0].Type != TypeAddObj {
		t.Errorf("expected add-obj, got %s", rebased[0].Type)
	}
}

// ─── 5. Move / index adjustment ───────────────────────────────────────────────

func TestMovObjectsIndexIncrementedWhenCompetingAddBefore(t *testing.T) {
	rebased := rebase(
		[]Change{movObjects([]string{idA}, parentID, 2)},
		[][]Change{{addObj(idB, parentID, 1)}},
	)
	if len(rebased) != 1 {
		t.Fatalf("expected 1 change, got %d", len(rebased))
	}
	if rebased[0].Index != 3 {
		t.Errorf("expected index 3, got %d", rebased[0].Index)
	}
}

func TestMovObjectsIndexUnchangedWhenCompetingAddAfter(t *testing.T) {
	rebased := rebase(
		[]Change{movObjects([]string{idA}, parentID, 2)},
		[][]Change{{addObj(idB, parentID, 5)}},
	)
	if len(rebased) != 1 {
		t.Fatalf("expected 1 change, got %d", len(rebased))
	}
	if rebased[0].Index != 2 {
		t.Errorf("expected index 2, got %d", rebased[0].Index)
	}
}

func TestMovObjectsShapesPrunedWhenCompetingDelObj(t *testing.T) {
	rebased := rebase(
		[]Change{movObjects([]string{idA, idB, idC}, parentID, 0)},
		[][]Change{{delObj(idB)}},
	)
	if len(rebased) != 1 {
		t.Fatalf("expected 1 change, got %d", len(rebased))
	}
	shapes := rebased[0].Shapes
	if len(shapes) != 2 {
		t.Errorf("expected 2 shapes after pruning, got %d", len(shapes))
	}
	for _, s := range shapes {
		if s == idB {
			t.Error("idB should have been pruned")
		}
	}
}

func TestMovObjectsDroppedWhenAllShapesDeleted(t *testing.T) {
	rebased := rebase(
		[]Change{movObjects([]string{idA, idB}, parentID, 0)},
		[][]Change{{delObj(idA), delObj(idB)}},
	)
	if len(rebased) != 0 {
		t.Errorf("expected empty result, got %d changes", len(rebased))
	}
}

// ─── 6. Multiple competing change-sets ───────────────────────────────────────

func TestRebaseAgainstMultipleCompetingSets(t *testing.T) {
	incoming := []Change{
		modObj(idA, setOp("name", "a")),
		modObj(idB, setOp("name", "b")),
		modObj(idC, setOp("name", "c")),
	}
	competing := [][]Change{
		{delObj(idA)},
		{delObj(idB)},
		{},
	}
	rebased := rebase(incoming, competing)
	if len(rebased) != 1 {
		t.Errorf("expected 1 surviving change (idC), got %d", len(rebased))
	}
	if rebased[0].ID != idC {
		t.Errorf("expected idC to survive, got %s", rebased[0].ID)
	}
}

func TestRebaseChangeSetPreservesOrder(t *testing.T) {
	incoming := []Change{
		modObj(idA, setOp("name", "a")),
		modObj(idB, setOp("name", "b")),
		modObj(idC, setOp("name", "c")),
	}
	result := rebase(incoming, nil)
	if len(result) != 3 {
		t.Fatalf("expected 3 changes, got %d", len(result))
	}
	expectedOrder := []string{idA, idB, idC}
	for i, ch := range result {
		if ch.ID != expectedOrder[i] {
			t.Errorf("position %d: expected %s, got %s", i, expectedOrder[i], ch.ID)
		}
	}
}

// ─── Extra edge cases ─────────────────────────────────────────────────────────

func TestMergeSetOpsIncomingWinsOnSameAttr(t *testing.T) {
	incoming := []Change{modObj(idA, setOp("name", "incoming"), setOp("x", "10"))}
	competing := [][]Change{{modObj(idA, setOp("name", "competing"), setOp("opacity", "0.5"))}}
	rebased := rebase(incoming, competing)
	if len(rebased) != 1 {
		t.Fatalf("expected 1 change, got %d", len(rebased))
	}
	ops := rebased[0].Operations
	nameCount := 0
	found := map[string]string{}
	for _, op := range ops {
		found[op.Attr] = string(op.Val)
		if op.Attr == "name" {
			nameCount++
		}
	}
	if nameCount != 1 {
		t.Errorf("expected exactly 1 name op, got %d", nameCount)
	}
	if found["name"] != `"incoming"` {
		t.Errorf("incoming name should win; got %s", found["name"])
	}
	if _, ok := found["opacity"]; !ok {
		t.Error("opacity from competing should be preserved")
	}
	if _, ok := found["x"]; !ok {
		t.Error("x from incoming should be preserved")
	}
}

func TestShortCircuitAfterNil(t *testing.T) {
	// After mod-obj is dropped by del-obj, a subsequent mod-obj on the same
	// id should not resurrect it (short-circuit to nil).
	ch := modObj(idA, setOp("name", "foo"))
	result := RebaseChange(ch, []Change{
		delObj(idA),
		modObj(idA, setOp("name", "bar")),
	})
	if result != nil {
		t.Errorf("expected nil (short-circuited), got %+v", result)
	}
}

func TestNonRebaseTypesPassThrough(t *testing.T) {
	// Changes with types not in the transform matrix pass through unchanged.
	addColorChange := Change{Type: "add-color", ID: idA, PageID: pageID1}
	result := rebase([]Change{addColorChange}, [][]Change{{delObj(idA), delObj(idB)}})
	if len(result) != 1 {
		t.Errorf("expected add-color to pass through unchanged, got %d changes", len(result))
	}
	if result[0].Type != "add-color" {
		t.Errorf("type should be preserved as add-color, got %s", result[0].Type)
	}
}
