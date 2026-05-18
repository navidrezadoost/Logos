// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) KALEIDOS INC
//
// Rust port of `common/src/app/common/files/rebase.cljc`
//
// P2.3 — Operational Transform rebase engine.
//
// Algorithm
// ─────────
// We follow the standard two-argument transform approach (Ellis & Gibbs 1989)
// adapted to the Logos change model.  The transform matrix covers five change
// types; the `transform_against` function is the 5×5 dispatch table.
//
// All dispatch is by *type pair* (incoming, competing) matching on the enum
// discriminant — identical to the Clojure `defmulti transform-against`.

/// Opaque shape / page identifier.  u64 is sufficient for tests and covers
/// every UUID packed as two u32s.
pub type Uuid = u64;

// =============================================================================
// Types
// =============================================================================

/// A single attribute-set operation inside a [`Change::ModObj`].
///
/// Maps to the inner `{:type :set :attr … :val …}` maps in the Clojure model.
/// The value is kept as a `String` here to stay dependency-free; a production
/// build would use `serde_json::Value`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetOp {
    /// Attribute name (e.g. `"name"`, `"fill-color"`).
    pub attr: String,
    /// Serialised value.
    pub val: String,
}

impl SetOp {
    pub fn new(attr: impl Into<String>, val: impl Into<String>) -> Self {
        SetOp { attr: attr.into(), val: val.into() }
    }
}

/// One change-set operation.
///
/// Variants mirror the five Clojure change types used by `transform-against`.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// Modify an existing object's attributes.
    ///
    /// Mirrors `{:type :mod-obj :id … :page-id … :operations […]}`.
    ModObj {
        id: Uuid,
        page_id: Uuid,
        operations: Vec<SetOp>,
    },

    /// Delete an existing object.
    ///
    /// Mirrors `{:type :del-obj :id … :page-id …}`.
    DelObj {
        id: Uuid,
        page_id: Uuid,
    },

    /// Add a new object.
    ///
    /// Mirrors `{:type :add-obj :id … :parent-id … :index … :page-id …}`.
    AddObj {
        id: Uuid,
        parent_id: Uuid,
        index: i64,
        page_id: Uuid,
    },

    /// Move a list of objects to a new parent at a given index.
    ///
    /// Mirrors `{:type :mov-objects :shapes […] :parent-id … :index …}`.
    MovObjects {
        shapes: Vec<Uuid>,
        parent_id: Uuid,
        index: i64,
        page_id: Uuid,
    },
}

// =============================================================================
// Helpers — mirror the private Clojure helpers
// =============================================================================

/// Merge operations from two `ModObj` changes.
///
/// For operations touching the same attribute, `incoming_ops` wins.
/// Mirrors Clojure `merge-set-ops`.
fn merge_set_ops(incoming_ops: &[SetOp], competing_ops: &[SetOp]) -> Vec<SetOp> {
    let incoming_attrs: std::collections::HashSet<&str> =
        incoming_ops.iter().map(|op| op.attr.as_str()).collect();

    let mut result: Vec<SetOp> = competing_ops
        .iter()
        .filter(|op| !incoming_attrs.contains(op.attr.as_str()))
        .cloned()
        .collect();

    result.extend(incoming_ops.iter().cloned());
    result
}

// =============================================================================
// Transform matrix  (the 5×5 dispatch table)
// =============================================================================

/// Rebase one `incoming` change against one `competing` change.
///
/// Returns the (possibly modified) incoming change, or `None` to drop it.
///
/// This is the Rust equivalent of Clojure's `defmulti transform-against`.
/// The dispatch key is `(incoming_type, competing_type)`.
fn transform_against(incoming: Change, competing: &Change) -> Option<Change> {
    match (&incoming, competing) {
        // ── :mod-obj vs :del-obj (same id) ─────────────────────────────────
        // A competing client deleted the object we are modifying → drop our change.
        (Change::ModObj { id: in_id, .. }, Change::DelObj { id: co_id, .. })
            if in_id == co_id =>
        {
            None
        }

        // ── :mod-obj vs :mod-obj (same id) — merge operations ──────────────
        // Incoming's attributes win; competing adds the rest.
        (
            Change::ModObj { id: in_id, .. },
            Change::ModObj { id: co_id, operations: co_ops, .. },
        ) if in_id == co_id => {
            if let Change::ModObj { id, page_id, operations: in_ops } = incoming {
                let merged = merge_set_ops(&in_ops, co_ops);
                Some(Change::ModObj { id, page_id, operations: merged })
            } else {
                unreachable!()
            }
        }

        // ── :del-obj vs :del-obj (same id) — idempotent ────────────────────
        // Both clients delete the same object.  Keep incoming (safe no-op on server).
        (Change::DelObj { id: in_id, .. }, Change::DelObj { id: co_id, .. })
            if in_id == co_id =>
        {
            Some(incoming)
        }

        // ── :add-obj vs :del-obj (same id) — add wins ──────────────────────
        // Re-creation takes precedence (collaborative resilience rule).
        (Change::AddObj { id: in_id, .. }, Change::DelObj { id: co_id, .. })
            if in_id == co_id =>
        {
            Some(incoming)
        }

        // ── :mov-objects vs :del-obj — prune deleted id from shapes list ───
        (Change::MovObjects { .. }, Change::DelObj { id: deleted_id, .. }) => {
            if let Change::MovObjects { shapes, parent_id, index, page_id } = incoming {
                let pruned: Vec<Uuid> =
                    shapes.into_iter().filter(|id| id != deleted_id).collect();
                if pruned.is_empty() {
                    None // all shapes deleted — drop the move
                } else {
                    Some(Change::MovObjects { shapes: pruned, parent_id, index, page_id })
                }
            } else {
                unreachable!()
            }
        }

        // ── :mov-objects vs :add-obj — adjust index if insert is before target
        (
            Change::MovObjects { parent_id: in_parent, index: in_idx, .. },
            Change::AddObj { parent_id: co_parent, index: co_idx, .. },
        ) if in_parent == co_parent && co_idx <= in_idx => {
            if let Change::MovObjects { shapes, parent_id, index, page_id } = incoming {
                Some(Change::MovObjects { shapes, parent_id, index: index + 1, page_id })
            } else {
                unreachable!()
            }
        }

        // ── :mov-objects vs :mov-objects — adjust index ─────────────────────
        (
            Change::MovObjects { parent_id: in_parent, .. },
            Change::MovObjects { parent_id: co_parent, .. },
        ) if in_parent == co_parent => {
            if let Change::MovObjects { shapes, parent_id, index: in_idx, page_id } = incoming {
                if let Change::MovObjects {
                    shapes: co_shapes,
                    index: co_idx,
                    ..
                } = competing
                {
                    let our_set: std::collections::HashSet<Uuid> =
                        shapes.iter().copied().collect();
                    let competing_set: std::collections::HashSet<Uuid> =
                        co_shapes.iter().copied().collect();

                    let interfering: std::collections::HashSet<Uuid> =
                        competing_set.difference(&our_set).copied().collect();

                    let adjustment: i64 = if co_idx < &in_idx {
                        -(interfering.len() as i64)
                    } else if co_idx <= &in_idx {
                        interfering.len() as i64
                    } else {
                        0
                    };

                    let new_idx = (in_idx + adjustment).max(0);
                    Some(Change::MovObjects { shapes, parent_id, index: new_idx, page_id })
                } else {
                    unreachable!()
                }
            } else {
                unreachable!()
            }
        }

        // ── Default: preserve incoming ─────────────────────────────────────
        _ => Some(incoming),
    }
}

// =============================================================================
// Public API
// =============================================================================

/// Rebase a single incoming `change` against all `competing` changes
/// (in the order they were applied by the server).
///
/// Returns the rebased change, or `None` if the change becomes a no-op
/// (e.g., modifying a shape that was concurrently deleted).
///
/// Mirrors Clojure `rebase-change`.
///
/// # Example
///
/// ```
/// use logos_rebase::{Change, SetOp, rebase_change};
///
/// let incoming = Change::ModObj { id: 1, page_id: 10, operations: vec![SetOp::new("name", "foo")] };
/// let result = rebase_change(incoming, &[]);
/// assert!(result.is_some());
/// ```
pub fn rebase_change(change: Change, competing: &[Change]) -> Option<Change> {
    competing.iter().try_fold(change, |ch, comp| transform_against(ch, comp))
}

/// Rebase an entire change-set against a sequence of competing change-sets.
///
/// Each element of `competing_change_sets` represents one server-applied
/// revision that was not yet visible when the client built `changes`.
///
/// Mirrors Clojure `rebase-change-set`.
///
/// # Example
///
/// ```
/// use logos_rebase::{Change, SetOp, rebase_change_set};
///
/// let incoming = vec![
///     Change::ModObj { id: 1, page_id: 10, operations: vec![SetOp::new("name", "a")] },
/// ];
/// let competing: Vec<Vec<Change>> = vec![
///     vec![Change::DelObj { id: 1, page_id: 10 }],
/// ];
/// let result = rebase_change_set(
///     incoming,
///     &competing.iter().map(|v| v.as_slice()).collect::<Vec<_>>(),
/// );
/// assert!(result.is_empty());
/// ```
pub fn rebase_change_set(changes: Vec<Change>, competing_change_sets: &[&[Change]]) -> Vec<Change> {
    let flat: Vec<&Change> = competing_change_sets
        .iter()
        .flat_map(|cs| cs.iter())
        .collect();

    changes
        .into_iter()
        .filter_map(|ch| {
            flat.iter().try_fold(ch, |acc, comp| transform_against(acc, comp))
        })
        .collect()
}

// =============================================================================
// Tests — all 17 cases from files_rebase_test.cljc + 4 extras
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    const ID_A: Uuid = 101;
    const ID_B: Uuid = 102;
    const ID_C: Uuid = 103;
    const PARENT_ID: Uuid = 201;
    const PAGE_ID_1: Uuid = 301;

    fn set_op(attr: &str, val: &str) -> SetOp { SetOp::new(attr, val) }

    fn mod_obj(id: Uuid, ops: Vec<SetOp>) -> Change {
        Change::ModObj { id, page_id: PAGE_ID_1, operations: ops }
    }

    fn del_obj(id: Uuid) -> Change { Change::DelObj { id, page_id: PAGE_ID_1 } }

    fn add_obj(id: Uuid, parent: Uuid, index: i64) -> Change {
        Change::AddObj { id, parent_id: parent, index, page_id: PAGE_ID_1 }
    }

    fn mov_objects(shapes: Vec<Uuid>, parent: Uuid, index: i64) -> Change {
        Change::MovObjects { shapes, parent_id: parent, index, page_id: PAGE_ID_1 }
    }

    fn rebase(changes: Vec<Change>, competing: Vec<Vec<Change>>) -> Vec<Change> {
        let slices: Vec<&[Change]> = competing.iter().map(|v| v.as_slice()).collect();
        rebase_change_set(changes, &slices)
    }

    // ── 1. Identity / idempotency ──────────────────────────────────────────

    #[test]
    fn rebase_against_empty_competing_is_identity() {
        let cs = vec![mod_obj(ID_A, vec![set_op("name", "foo")]), del_obj(ID_B)];
        let result = rebase(cs.clone(), vec![]);
        assert_eq!(result, cs);
    }

    #[test]
    fn rebase_empty_change_set_is_empty() {
        let result = rebase(vec![], vec![vec![del_obj(ID_A)]]);
        assert!(result.is_empty());
    }

    #[test]
    fn rebase_change_single_no_conflict() {
        let ch = mod_obj(ID_A, vec![set_op("name", "foo")]);
        assert_eq!(rebase_change(ch.clone(), &[]), Some(ch));
    }

    // ── 2. Non-conflicting preservation ───────────────────────────────────

    #[test]
    fn different_shape_ids_both_preserved() {
        let incoming = vec![mod_obj(ID_A, vec![set_op("name", "Alice")])];
        let competing = vec![vec![mod_obj(ID_B, vec![set_op("name", "Bob")])]];
        let rebased = rebase(incoming.clone(), competing);
        assert_eq!(rebased, incoming);
    }

    #[test]
    fn del_obj_on_different_id_preserved() {
        let rebased = rebase(vec![del_obj(ID_A)], vec![vec![del_obj(ID_B)]]);
        assert_eq!(rebased.len(), 1);
    }

    // ── 3. :mod-obj vs :mod-obj ────────────────────────────────────────────

    #[test]
    fn mod_obj_same_attr_incoming_wins() {
        let incoming = vec![mod_obj(ID_A, vec![set_op("name", "incoming-name")])];
        let competing = vec![vec![mod_obj(ID_A, vec![set_op("name", "competing-name")])]];
        let rebased = rebase(incoming, competing);
        assert_eq!(rebased.len(), 1);
        let Change::ModObj { operations, .. } = &rebased[0] else { panic!() };
        let name_val = operations.iter().find(|op| op.attr == "name").unwrap().val.as_str();
        assert_eq!(name_val, "incoming-name");
    }

    #[test]
    fn mod_obj_different_attrs_both_preserved() {
        let incoming = vec![mod_obj(ID_A, vec![set_op("name", "inc-name")])];
        let competing = vec![vec![mod_obj(ID_A, vec![set_op("fill-color", "#ff0000")])]];
        let rebased = rebase(incoming, competing);
        let Change::ModObj { operations, .. } = &rebased[0] else { panic!() };
        let attrs: std::collections::HashSet<&str> =
            operations.iter().map(|op| op.attr.as_str()).collect();
        assert!(attrs.contains("name"));
        assert!(attrs.contains("fill-color"));
    }

    // ── 4. Delete semantics ────────────────────────────────────────────────

    #[test]
    fn mod_obj_dropped_when_competing_del_obj_same_id() {
        let rebased = rebase(
            vec![mod_obj(ID_A, vec![set_op("name", "foo")])],
            vec![vec![del_obj(ID_A)]],
        );
        assert!(rebased.is_empty());
    }

    #[test]
    fn del_obj_kept_when_competing_del_obj_same_id() {
        let rebased = rebase(vec![del_obj(ID_A)], vec![vec![del_obj(ID_A)]]);
        assert_eq!(rebased.len(), 1);
    }

    #[test]
    fn del_obj_preserved_when_competing_mod_obj_same_id() {
        let rebased = rebase(
            vec![del_obj(ID_A)],
            vec![vec![mod_obj(ID_A, vec![set_op("name", "bar")])]],
        );
        assert_eq!(rebased.len(), 1);
        assert!(matches!(rebased[0], Change::DelObj { .. }));
    }

    #[test]
    fn add_obj_preserved_after_competing_del_obj_same_id() {
        let rebased = rebase(
            vec![add_obj(ID_A, PARENT_ID, 0)],
            vec![vec![del_obj(ID_A)]],
        );
        assert_eq!(rebased.len(), 1);
        assert!(matches!(rebased[0], Change::AddObj { .. }));
    }

    // ── 5. Move / index adjustment ─────────────────────────────────────────

    #[test]
    fn mov_objects_index_incremented_when_competing_add_before() {
        let rebased = rebase(
            vec![mov_objects(vec![ID_A], PARENT_ID, 2)],
            vec![vec![add_obj(ID_B, PARENT_ID, 1)]],
        );
        let Change::MovObjects { index, .. } = &rebased[0] else { panic!() };
        assert_eq!(*index, 3);
    }

    #[test]
    fn mov_objects_index_unchanged_when_competing_add_after() {
        let rebased = rebase(
            vec![mov_objects(vec![ID_A], PARENT_ID, 2)],
            vec![vec![add_obj(ID_B, PARENT_ID, 5)]],
        );
        let Change::MovObjects { index, .. } = &rebased[0] else { panic!() };
        assert_eq!(*index, 2);
    }

    #[test]
    fn mov_objects_shapes_pruned_when_competing_del_obj() {
        let rebased = rebase(
            vec![mov_objects(vec![ID_A, ID_B, ID_C], PARENT_ID, 0)],
            vec![vec![del_obj(ID_B)]],
        );
        let Change::MovObjects { shapes, .. } = &rebased[0] else { panic!() };
        assert_eq!(shapes, &vec![ID_A, ID_C]);
    }

    #[test]
    fn mov_objects_dropped_when_all_shapes_deleted() {
        let rebased = rebase(
            vec![mov_objects(vec![ID_A, ID_B], PARENT_ID, 0)],
            vec![vec![del_obj(ID_A), del_obj(ID_B)]],
        );
        assert!(rebased.is_empty());
    }

    // ── 6. Multiple competing change-sets ─────────────────────────────────

    #[test]
    fn rebase_against_multiple_competing_sets() {
        let incoming = vec![
            mod_obj(ID_A, vec![set_op("name", "a")]),
            mod_obj(ID_B, vec![set_op("name", "b")]),
            mod_obj(ID_C, vec![set_op("name", "c")]),
        ];
        let competing = vec![vec![del_obj(ID_A)], vec![del_obj(ID_B)], vec![]];
        let rebased = rebase(incoming, competing);
        assert_eq!(rebased.len(), 1);
        let Change::ModObj { id, .. } = &rebased[0] else { panic!() };
        assert_eq!(*id, ID_C);
    }

    #[test]
    fn rebase_change_set_preserves_order() {
        let incoming = vec![
            mod_obj(ID_A, vec![set_op("name", "a")]),
            mod_obj(ID_B, vec![set_op("name", "b")]),
            mod_obj(ID_C, vec![set_op("name", "c")]),
        ];
        let result = rebase(incoming, vec![]);
        assert_eq!(result.len(), 3);
        let ids: Vec<Uuid> = result.iter().map(|ch| {
            if let Change::ModObj { id, .. } = ch { *id } else { 0 }
        }).collect();
        assert_eq!(ids, vec![ID_A, ID_B, ID_C]);
    }

    // ── Extra edge cases ───────────────────────────────────────────────────

    #[test]
    fn merge_set_ops_incoming_wins_on_same_attr() {
        let incoming = vec![set_op("name", "winner"), set_op("x", "10")];
        let competing = vec![set_op("name", "loser"), set_op("opacity", "0.5")];
        let merged = merge_set_ops(&incoming, &competing);
        let name = merged.iter().find(|op| op.attr == "name").unwrap();
        assert_eq!(name.val, "winner");
        assert!(merged.iter().any(|op| op.attr == "opacity"));
        assert!(merged.iter().any(|op| op.attr == "x"));
        assert_eq!(merged.iter().filter(|op| op.attr == "name").count(), 1);
    }

    #[test]
    fn mod_obj_preserved_when_competing_del_obj_different_id() {
        let ch = mod_obj(ID_A, vec![set_op("name", "foo")]);
        let result = rebase_change(ch.clone(), &[del_obj(ID_B)]);
        assert_eq!(result, Some(ch));
    }

    #[test]
    fn short_circuit_after_none() {
        let ch = mod_obj(ID_A, vec![set_op("name", "foo")]);
        let result = rebase_change(
            ch,
            &[del_obj(ID_A), mod_obj(ID_A, vec![set_op("name", "bar")])],
        );
        assert!(result.is_none());
    }
}
