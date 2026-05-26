// Package rebase implements the P2.3 Operational Transform rebase engine.
//
// Pure-Go port of common/src/app/common/files/rebase.cljc and the Rust crate
// rust/logos-rebase/src/lib.rs.  The algorithm, transform matrix, and test
// suite all mirror those sources exactly so correctness can be verified by
// running the same test vectors in all three implementations.
//
// Algorithm summary (OT, two-argument transform):
//
//	│ incoming ╲ competing │ :mod-obj │ :del-obj │ :add-obj │ :mov-objects │
//	│──────────────────────┼──────────┼──────────┼──────────┼──────────────│
//	│ :mod-obj             │ merge    │ no-op    │ keep     │ keep         │
//	│ :del-obj             │ keep     │ identity │ keep     │ keep         │
//	│ :add-obj             │ keep     │ keep     │ keep     │ keep         │
//	│ :mov-objects         │ keep     │ prune-id │ adj-idx  │ adj-idx      │
//
// "keep"    = incoming change is returned unchanged.
// "no-op"   = incoming change is dropped (returns nil).
// "merge"   = incoming :set-ops win over competing ones for the same attr.
// "prune-id"= deleted id is removed from the :shapes list; nil if list empties.
// "adj-idx" = :index is adjusted up/down to account for concurrent inserts/removes.
//
// References:
//   - Ellis & Gibbs (1989) — "Concurrency Control in Groupware Systems"
//   - Nichols et al. (1995) — Jupiter server-authoritative OT
package rebase

import "encoding/json"

// ─── Change types ─────────────────────────────────────────────────────────────

// Type identifies the kind of shape operation in a change-set.
type Type string

const (
	TypeModObj     Type = "mod-obj"
	TypeDelObj     Type = "del-obj"
	TypeAddObj     Type = "add-obj"
	TypeMovObjects Type = "mov-objects"
)

// SetOp is a single attribute mutation inside a ModObj change.
// It mirrors {:type :set :attr "…" :val …} in the Clojure model.
// Val is left as raw JSON so no type assumptions are made about the value.
type SetOp struct {
	Attr string          `json:"attr"`
	Val  json.RawMessage `json:"val"`
}

// Change is one operation in a change-set.
//
// All fields are present in the JSON representation; the fields used by the
// rebase algorithm depend on the Type:
//
//	TypeModObj:     Type, ID, PageID, Operations
//	TypeDelObj:     Type, ID, PageID
//	TypeAddObj:     Type, ID, PageID, ParentID, Index, Obj (opaque)
//	TypeMovObjects: Type, PageID, Shapes, ParentID, Index
//
// Other types (library ops, page-level ops, etc.) pass through the transform
// matrix unchanged via the default rule.
type Change struct {
	Type       Type            `json:"type"`
	ID         string          `json:"id,omitempty"`
	PageID     string          `json:"pageId,omitempty"`
	ParentID   string          `json:"parentId,omitempty"`
	Index      int             `json:"index,omitempty"`
	Shapes     []string        `json:"shapes,omitempty"`
	Operations []SetOp         `json:"operations,omitempty"`
	Obj        json.RawMessage `json:"obj,omitempty"` // opaque add-obj payload
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

// mergeSetOps merges operations from two ModObj changes.
// For operations touching the same attribute, incomingOps wins.
// Mirrors Clojure's merge-set-ops / Rust's merge_set_ops.
func mergeSetOps(incomingOps, competingOps []SetOp) []SetOp {
	incomingAttrs := make(map[string]struct{}, len(incomingOps))
	for _, op := range incomingOps {
		incomingAttrs[op.Attr] = struct{}{}
	}

	result := make([]SetOp, 0, len(incomingOps)+len(competingOps))
	for _, op := range competingOps {
		if _, ok := incomingAttrs[op.Attr]; !ok {
			result = append(result, op)
		}
	}
	return append(result, incomingOps...)
}

// ─── Transform matrix (5×5 dispatch) ─────────────────────────────────────────

// transformAgainst rebases one incoming change against one competing change.
// Returns the (possibly modified) incoming change, or nil to drop it.
// Mirrors Clojure's defmulti transform-against and Rust's transform_against.
func transformAgainst(incoming Change, competing Change) *Change {
	switch {

	// ── :mod-obj vs :del-obj (same id) → drop incoming ─────────────────────
	case incoming.Type == TypeModObj && competing.Type == TypeDelObj:
		if incoming.ID == competing.ID {
			return nil
		}
		return &incoming

	// ── :mod-obj vs :mod-obj (same id) → merge set-ops ─────────────────────
	case incoming.Type == TypeModObj && competing.Type == TypeModObj:
		if incoming.ID != competing.ID {
			return &incoming
		}
		merged := mergeSetOps(incoming.Operations, competing.Operations)
		result := incoming
		result.Operations = merged
		return &result

	// ── :del-obj vs :del-obj (same id) → idempotent, keep ──────────────────
	case incoming.Type == TypeDelObj && competing.Type == TypeDelObj:
		// Two deletes of the same object; safe to re-delete.
		return &incoming

	// ── :add-obj vs :del-obj (same id) → re-add wins ───────────────────────
	case incoming.Type == TypeAddObj && competing.Type == TypeDelObj:
		// Collaborative resilience: re-creation takes precedence.
		return &incoming

	// ── :mov-objects vs :del-obj → prune deleted id from :shapes ───────────
	case incoming.Type == TypeMovObjects && competing.Type == TypeDelObj:
		pruned := make([]string, 0, len(incoming.Shapes))
		for _, s := range incoming.Shapes {
			if s != competing.ID {
				pruned = append(pruned, s)
			}
		}
		if len(pruned) == 0 {
			return nil // all shapes were deleted — drop the move entirely
		}
		result := incoming
		result.Shapes = pruned
		return &result

	// ── :mov-objects vs :add-obj → adjust index if insert is before target ──
	case incoming.Type == TypeMovObjects && competing.Type == TypeAddObj:
		if incoming.ParentID == competing.ParentID && competing.Index <= incoming.Index {
			result := incoming
			result.Index++
			return &result
		}
		return &incoming

	// ── :mov-objects vs :mov-objects → adjust index ─────────────────────────
	case incoming.Type == TypeMovObjects && competing.Type == TypeMovObjects:
		if incoming.ParentID != competing.ParentID {
			return &incoming
		}
		// For each competing shape that we don't also move, determine how it
		// shifts our target index (mirrors Clojure's conservative approximation).
		incomingSet := make(map[string]struct{}, len(incoming.Shapes))
		for _, s := range incoming.Shapes {
			incomingSet[s] = struct{}{}
		}
		interfering := 0
		for _, s := range competing.Shapes {
			if _, ok := incomingSet[s]; !ok {
				interfering++
			}
		}
		var adjustment int
		if competing.Index < incoming.Index {
			adjustment = -interfering // competing freed slots before our index
		} else if competing.Index <= incoming.Index {
			adjustment = interfering // competing filled slots at or before our index
		}
		result := incoming
		result.Index = max(0, incoming.Index+adjustment)
		return &result

	// ── Default: preserve incoming, no adjustment needed ────────────────────
	default:
		return &incoming
	}
}

// ─── Public API ───────────────────────────────────────────────────────────────

// RebaseChange rebases a single incoming change against all competing changes
// (in the order they were applied by the server).
//
// Returns the rebased change, or nil if the change becomes a no-op
// (e.g., modifying a shape that was concurrently deleted).
//
// Mirrors Clojure rebase-change / Rust rebase_change.
func RebaseChange(change Change, competing []Change) *Change {
	result := &change
	for _, comp := range competing {
		if result == nil {
			return nil
		}
		result = transformAgainst(*result, comp)
	}
	return result
}

// RebaseChangeSet rebases an entire change-set against a sequence of competing
// change-sets (each representing one server-applied revision the client has
// not yet seen).
//
// Returns a (possibly smaller) change-set that is safe to apply on top of
// the current server state.
//
// Example: client built its change-set based on server revn 5.
// Server is now at revn 8.  Provide competing change-sets for revn 6, 7, 8.
// The returned change-set is safe to apply at revn 9.
//
// Mirrors Clojure rebase-change-set / Rust rebase_change_set.
func RebaseChangeSet(changes []Change, competingChangeSets [][]Change) []Change {
	// Flatten all competing change-sets into a single ordered sequence.
	var flat []Change
	for _, cs := range competingChangeSets {
		flat = append(flat, cs...)
	}

	result := make([]Change, 0, len(changes))
	for _, ch := range changes {
		r := RebaseChange(ch, flat)
		if r != nil {
			result = append(result, *r)
		}
	}
	return result
}
