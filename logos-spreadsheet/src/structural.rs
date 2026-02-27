//! Structural operations — first-class patch ops for spreadsheet topology.
//!
//! This module implements insert/delete row/column, move-range, and
//! resize-sheet as composable operations that correctly shift:
//!
//! - Cell data (values + properties)
//! - Formula ASTs (all `CellRef` / `RangeRef` nodes)
//! - Formula source strings (regenerated from shifted ASTs)
//! - Dependency graph edges (rebuilt from shifted formulas)
//!
//! Each operation produces a [`StructuralChange`] snapshot that captures
//! enough state for the caller to implement undo.
//!
//! # Reference shifting rules
//!
//! All references (including `$`-anchored absolute refs) shift for
//! structural operations. The `abs_col` / `abs_row` flags on [`CellRef`]
//! only govern copy-paste / fill behaviour — they do **not** prevent
//! shifting when the underlying topology changes.
//!
//! References that fall inside a deleted range become `#REF!` errors.

use std::collections::HashMap;

use crate::deps::CellCoord;
use crate::errors::SpreadsheetError;
use crate::types::*;

// ---------------------------------------------------------------------------
// StructuralOp — the patch-op enum
// ---------------------------------------------------------------------------

/// A structural operation that changes spreadsheet topology.
#[derive(Debug, Clone, PartialEq)]
pub enum StructuralOp {
    /// Insert `count` empty rows starting at row index `at`.
    /// Existing rows at `at..` shift down by `count`.
    InsertRows { at: u32, count: u32 },

    /// Delete `count` rows starting at row index `at`.
    /// Rows in `[at, at+count)` are removed; rows `at+count..` shift up.
    DeleteRows { at: u32, count: u32 },

    /// Insert `count` empty columns starting at column index `at`.
    InsertCols { at: u32, count: u32 },

    /// Delete `count` columns starting at column index `at`.
    DeleteCols { at: u32, count: u32 },

    /// Move a rectangular region to a new position.
    ///
    /// The source region `[src_col..=src_end_col, src_row..=src_end_row]`
    /// is moved so that its top-left corner lands at `(dst_col, dst_row)`.
    MoveRange {
        src_col: u32,
        src_row: u32,
        src_end_col: u32,
        src_end_row: u32,
        dst_col: u32,
        dst_row: u32,
    },

    /// Resize the sheet to new dimensions, trimming out-of-bounds cells.
    ResizeSheet { new_cols: u32, new_rows: u32 },
}

// ---------------------------------------------------------------------------
// Undo snapshot
// ---------------------------------------------------------------------------

/// A snapshot that captures enough state to reverse a structural operation.
#[derive(Debug, Clone)]
pub struct StructuralChange {
    /// The operation that was applied.
    pub op: StructuralOp,
    /// Cells that existed before the operation: `(coord, value)`.
    pub cell_snapshot: Vec<(CellCoord, Value)>,
    /// Formula cells before the operation: `(coord, source, ast)`.
    pub formula_snapshot: Vec<(CellCoord, String, Expression)>,
    /// Cell properties before the operation.
    pub property_snapshot: Vec<((u32, u32, String), Value)>,
    /// Sheet dimensions before the operation.
    pub old_max_cols: u32,
    pub old_max_rows: u32,
}

// ---------------------------------------------------------------------------
// Reference shifting
// ---------------------------------------------------------------------------

/// Outcome of shifting a single [`CellRef`].
#[derive(Debug, Clone, PartialEq)]
pub enum RefShift {
    /// Reference moved to a new position.
    Shifted(CellRef),
    /// The referenced row/column was deleted — becomes `#REF!`.
    Deleted,
    /// The reference was not affected.
    Unchanged,
}

/// Shift a [`CellRef`] according to a [`StructuralOp`].
pub fn shift_cell_ref(r: &CellRef, op: &StructuralOp) -> RefShift {
    match op {
        StructuralOp::InsertRows { at, count } => {
            if r.row >= *at {
                RefShift::Shifted(CellRef {
                    row: r.row + count,
                    ..r.clone()
                })
            } else {
                RefShift::Unchanged
            }
        }
        StructuralOp::DeleteRows { at, count } => {
            if r.row >= *at && r.row < at + count {
                RefShift::Deleted
            } else if r.row >= at + count {
                RefShift::Shifted(CellRef {
                    row: r.row - count,
                    ..r.clone()
                })
            } else {
                RefShift::Unchanged
            }
        }
        StructuralOp::InsertCols { at, count } => {
            if r.col >= *at {
                RefShift::Shifted(CellRef {
                    col: r.col + count,
                    ..r.clone()
                })
            } else {
                RefShift::Unchanged
            }
        }
        StructuralOp::DeleteCols { at, count } => {
            if r.col >= *at && r.col < at + count {
                RefShift::Deleted
            } else if r.col >= at + count {
                RefShift::Shifted(CellRef {
                    col: r.col - count,
                    ..r.clone()
                })
            } else {
                RefShift::Unchanged
            }
        }
        StructuralOp::MoveRange {
            src_col, src_row, src_end_col, src_end_row,
            dst_col, dst_row,
        } => {
            // References pointing inside the source region follow the move.
            if r.col >= *src_col && r.col <= *src_end_col
                && r.row >= *src_row && r.row <= *src_end_row
            {
                let delta_col = *dst_col as i64 - *src_col as i64;
                let delta_row = *dst_row as i64 - *src_row as i64;
                let new_col = (r.col as i64 + delta_col).max(0) as u32;
                let new_row = (r.row as i64 + delta_row).max(0) as u32;
                RefShift::Shifted(CellRef {
                    col: new_col,
                    row: new_row,
                    ..r.clone()
                })
            } else {
                RefShift::Unchanged
            }
        }
        StructuralOp::ResizeSheet { .. } => RefShift::Unchanged,
    }
}

/// Shift a `CellCoord` (the cell position itself, not the reference
/// inside a formula).
pub fn shift_coord(coord: CellCoord, op: &StructuralOp) -> Option<CellCoord> {
    let (col, row) = coord;
    match op {
        StructuralOp::InsertRows { at, count } => {
            if row >= *at {
                Some((col, row + count))
            } else {
                Some(coord)
            }
        }
        StructuralOp::DeleteRows { at, count } => {
            if row >= *at && row < at + count {
                None // cell is in deleted range
            } else if row >= at + count {
                Some((col, row - count))
            } else {
                Some(coord)
            }
        }
        StructuralOp::InsertCols { at, count } => {
            if col >= *at {
                Some((col + count, row))
            } else {
                Some(coord)
            }
        }
        StructuralOp::DeleteCols { at, count } => {
            if col >= *at && col < at + count {
                None
            } else if col >= at + count {
                Some((col - count, row))
            } else {
                Some(coord)
            }
        }
        StructuralOp::MoveRange {
            src_col, src_row, src_end_col, src_end_row,
            dst_col, dst_row,
        } => {
            if col >= *src_col && col <= *src_end_col
                && row >= *src_row && row <= *src_end_row
            {
                let dc = *dst_col as i64 - *src_col as i64;
                let dr = *dst_row as i64 - *src_row as i64;
                Some(((col as i64 + dc).max(0) as u32, (row as i64 + dr).max(0) as u32))
            } else {
                Some(coord)
            }
        }
        StructuralOp::ResizeSheet { new_cols, new_rows } => {
            if col >= *new_cols || row >= *new_rows {
                None
            } else {
                Some(coord)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Expression shifting — walk the AST and adjust references
// ---------------------------------------------------------------------------

/// Shift all cell references inside an [`Expression`] according to `op`.
///
/// Returns `Some(shifted_expr)` if any reference changed (or was deleted),
/// `None` if the expression was entirely unaffected.
pub fn shift_expression(expr: &Expression, op: &StructuralOp) -> Option<Expression> {
    match expr {
        Expression::CellReference(r) => match shift_cell_ref(r, op) {
            RefShift::Shifted(new_ref) => Some(Expression::CellReference(new_ref)),
            RefShift::Deleted => Some(Expression::Literal(Value::Error(SpreadsheetError::Ref))),
            RefShift::Unchanged => None,
        },

        Expression::Range(range) => {
            let s = shift_cell_ref(&range.start, op);
            let e = shift_cell_ref(&range.end, op);

            match (&s, &e) {
                (RefShift::Unchanged, RefShift::Unchanged) => None,
                _ => {
                    // If either endpoint is deleted, the whole range is #REF!
                    let new_start = match s {
                        RefShift::Shifted(r) => r,
                        RefShift::Deleted => {
                            return Some(Expression::Literal(Value::Error(SpreadsheetError::Ref)));
                        }
                        RefShift::Unchanged => range.start.clone(),
                    };
                    let new_end = match e {
                        RefShift::Shifted(r) => r,
                        RefShift::Deleted => {
                            return Some(Expression::Literal(Value::Error(SpreadsheetError::Ref)));
                        }
                        RefShift::Unchanged => range.end.clone(),
                    };
                    Some(Expression::Range(RangeRef {
                        start: new_start,
                        end: new_end,
                    }))
                }
            }
        }

        Expression::UnaryOp(op_kind, inner) => {
            shift_expression(inner, op).map(|shifted| {
                Expression::UnaryOp(*op_kind, Box::new(shifted))
            })
        }

        Expression::BinaryOp(op_kind, lhs, rhs) => {
            let l = shift_expression(lhs, op);
            let r = shift_expression(rhs, op);
            if l.is_none() && r.is_none() {
                None
            } else {
                Some(Expression::BinaryOp(
                    *op_kind,
                    Box::new(l.unwrap_or_else(|| *lhs.clone())),
                    Box::new(r.unwrap_or_else(|| *rhs.clone())),
                ))
            }
        }

        Expression::FunctionCall(name, args) => {
            let mut any_changed = false;
            let new_args: Vec<Expression> = args
                .iter()
                .map(|a| {
                    if let Some(shifted) = shift_expression(a, op) {
                        any_changed = true;
                        shifted
                    } else {
                        a.clone()
                    }
                })
                .collect();
            if any_changed {
                Some(Expression::FunctionCall(name.clone(), new_args))
            } else {
                None
            }
        }

        Expression::Member(base, key) => {
            shift_expression(base, op).map(|shifted_base| {
                Expression::Member(Box::new(shifted_base), key.clone())
            })
        }

        Expression::ArrayLiteral(rows) => {
            let mut any_changed = false;
            let new_rows: Vec<Vec<Expression>> = rows
                .iter()
                .map(|row| {
                    row.iter()
                        .map(|e| {
                            if let Some(shifted) = shift_expression(e, op) {
                                any_changed = true;
                                shifted
                            } else {
                                e.clone()
                            }
                        })
                        .collect()
                })
                .collect();
            if any_changed {
                Some(Expression::ArrayLiteral(new_rows))
            } else {
                None
            }
        }

        Expression::Literal(_) => None, // literals don't contain refs
    }
}

// ---------------------------------------------------------------------------
// Expression formatter — AST → formula string
// ---------------------------------------------------------------------------

/// Render an [`Expression`] AST back into a formula string (without
/// the leading `=`). The caller should prepend `=` if needed.
pub fn format_expression(expr: &Expression) -> String {
    match expr {
        Expression::Literal(v) => format_value(v),

        Expression::CellReference(r) => r.to_a1(),

        Expression::Range(r) => {
            format!("{}:{}", r.start.to_a1(), r.end.to_a1())
        }

        Expression::UnaryOp(op, inner) => {
            let inner_s = format_expression(inner);
            match op {
                UnaryOp::Negate => format!("-{inner_s}"),
                UnaryOp::Plus => format!("+{inner_s}"),
                UnaryOp::Not => format!("NOT({inner_s})"),
            }
        }

        Expression::BinaryOp(op, lhs, rhs) => {
            let l = format_expression(lhs);
            let r = format_expression(rhs);
            let op_str = binary_op_str(*op);
            // Always parenthesise to guarantee correctness after shifting.
            format!("({l}{op_str}{r})")
        }

        Expression::FunctionCall(name, args) => {
            let args_s: Vec<String> = args.iter().map(|a| format_expression(a)).collect();
            format!("{}({})", name, args_s.join(","))
        }

        Expression::Member(base, key) => {
            let base_s = format_expression(base);
            match key {
                MemberKey::Dot(k) => format!("{base_s}.{k}"),
                MemberKey::Bracket(k) => format!("{base_s}[\"{k}\"]"),
            }
        }

        Expression::ArrayLiteral(rows) => {
            let rows_s: Vec<String> = rows
                .iter()
                .map(|row| {
                    row.iter()
                        .map(|e| format_expression(e))
                        .collect::<Vec<_>>()
                        .join(",")
                })
                .collect();
            format!("{{{}}}", rows_s.join(";"))
        }
    }
}

fn format_value(v: &Value) -> String {
    match v {
        Value::Number(n) => format_number(*n),
        Value::Text(s) => format!("\"{s}\""),
        Value::Boolean(true) => "TRUE".to_string(),
        Value::Boolean(false) => "FALSE".to_string(),
        Value::Error(e) => format!("{e}"),
        Value::Empty => "0".to_string(), // empty renders as 0 in formulas
        Value::Array(_) | Value::DesignRef(_) => "#VALUE!".to_string(),
    }
}

fn format_number(n: f64) -> String {
    if n == n.trunc() && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

fn binary_op_str(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::Pow => "^",
        BinaryOp::Eq => "=",
        BinaryOp::Neq => "<>",
        BinaryOp::Lt => "<",
        BinaryOp::Gt => ">",
        BinaryOp::Lte => "<=",
        BinaryOp::Gte => ">=",
        BinaryOp::And => " AND ",
        BinaryOp::Or => " OR ",
        BinaryOp::Concat => "&",
    }
}

// ---------------------------------------------------------------------------
// Coordinate-map utilities for bulk shifting
// ---------------------------------------------------------------------------

/// Build a mapping from old coordinate → new coordinate for all entries in
/// a `HashMap` keyed by `CellCoord`. Keys whose coordinates get deleted
/// (fall inside a removed range) are returned in `deleted`.
pub fn build_coord_map<V>(
    map: &HashMap<CellCoord, V>,
    op: &StructuralOp,
) -> (Vec<(CellCoord, CellCoord)>, Vec<CellCoord>) {
    let mut moves = Vec::new();
    let mut deleted = Vec::new();
    for &old in map.keys() {
        match shift_coord(old, op) {
            Some(new) if new != old => moves.push((old, new)),
            Some(_) => {} // unchanged
            None => deleted.push(old),
        }
    }
    (moves, deleted)
}

/// Shift all keys in a `HashMap<CellCoord, V>`. Deleted keys are removed.
pub fn shift_hashmap<V: Clone>(
    map: &HashMap<CellCoord, V>,
    op: &StructuralOp,
) -> HashMap<CellCoord, V> {
    let mut out = HashMap::with_capacity(map.len());
    for (old, val) in map {
        if let Some(new) = shift_coord(*old, op) {
            out.insert(new, val.clone());
        }
    }
    out
}

/// Shift property keys `(col, row, name)`.
pub fn shift_property_map(
    map: &HashMap<(u32, u32, String), Value>,
    op: &StructuralOp,
) -> HashMap<(u32, u32, String), Value> {
    let mut out = HashMap::with_capacity(map.len());
    for ((col, row, name), val) in map {
        if let Some((nc, nr)) = shift_coord((*col, *row), op) {
            out.insert((nc, nr, name.clone()), val.clone());
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ===== shift_cell_ref — InsertRows =====================================

    #[test]
    fn insert_rows_shifts_ref_at_or_below() {
        let r = CellRef::new(0, 5); // A6
        let op = StructuralOp::InsertRows { at: 3, count: 2 };
        assert_eq!(
            shift_cell_ref(&r, &op),
            RefShift::Shifted(CellRef::new(0, 7))
        );
    }

    #[test]
    fn insert_rows_unchanged_above() {
        let r = CellRef::new(0, 2); // A3
        let op = StructuralOp::InsertRows { at: 3, count: 2 };
        assert_eq!(shift_cell_ref(&r, &op), RefShift::Unchanged);
    }

    #[test]
    fn insert_rows_shifts_at_boundary() {
        let r = CellRef::new(1, 3); // B4
        let op = StructuralOp::InsertRows { at: 3, count: 1 };
        assert_eq!(
            shift_cell_ref(&r, &op),
            RefShift::Shifted(CellRef::new(1, 4))
        );
    }

    #[test]
    fn insert_rows_shifts_absolute_ref() {
        let r = CellRef {
            col: 0, row: 5, abs_col: true, abs_row: true,
        };
        let op = StructuralOp::InsertRows { at: 3, count: 2 };
        match shift_cell_ref(&r, &op) {
            RefShift::Shifted(new) => {
                assert_eq!(new.row, 7);
                assert!(new.abs_row); // flag preserved
                assert!(new.abs_col);
            }
            other => panic!("expected Shifted, got {other:?}"),
        }
    }

    // ===== shift_cell_ref — DeleteRows =====================================

    #[test]
    fn delete_rows_deletes_ref_in_range() {
        let r = CellRef::new(0, 3);
        let op = StructuralOp::DeleteRows { at: 2, count: 3 };
        assert_eq!(shift_cell_ref(&r, &op), RefShift::Deleted);
    }

    #[test]
    fn delete_rows_shifts_ref_below() {
        let r = CellRef::new(0, 8);
        let op = StructuralOp::DeleteRows { at: 2, count: 3 };
        assert_eq!(
            shift_cell_ref(&r, &op),
            RefShift::Shifted(CellRef::new(0, 5))
        );
    }

    #[test]
    fn delete_rows_unchanged_above() {
        let r = CellRef::new(0, 1);
        let op = StructuralOp::DeleteRows { at: 2, count: 3 };
        assert_eq!(shift_cell_ref(&r, &op), RefShift::Unchanged);
    }

    #[test]
    fn delete_rows_boundary_start() {
        let r = CellRef::new(0, 2);
        let op = StructuralOp::DeleteRows { at: 2, count: 3 };
        assert_eq!(shift_cell_ref(&r, &op), RefShift::Deleted);
    }

    #[test]
    fn delete_rows_boundary_end() {
        let r = CellRef::new(0, 4);
        let op = StructuralOp::DeleteRows { at: 2, count: 3 };
        assert_eq!(shift_cell_ref(&r, &op), RefShift::Deleted);
    }

    #[test]
    fn delete_rows_just_after_range() {
        let r = CellRef::new(0, 5);
        let op = StructuralOp::DeleteRows { at: 2, count: 3 };
        assert_eq!(
            shift_cell_ref(&r, &op),
            RefShift::Shifted(CellRef::new(0, 2))
        );
    }

    // ===== shift_cell_ref — InsertCols =====================================

    #[test]
    fn insert_cols_shifts_ref() {
        let r = CellRef::new(3, 0);
        let op = StructuralOp::InsertCols { at: 1, count: 2 };
        assert_eq!(
            shift_cell_ref(&r, &op),
            RefShift::Shifted(CellRef::new(5, 0))
        );
    }

    #[test]
    fn insert_cols_unchanged_before() {
        let r = CellRef::new(0, 0);
        let op = StructuralOp::InsertCols { at: 1, count: 2 };
        assert_eq!(shift_cell_ref(&r, &op), RefShift::Unchanged);
    }

    // ===== shift_cell_ref — DeleteCols =====================================

    #[test]
    fn delete_cols_deletes_ref() {
        let r = CellRef::new(2, 0);
        let op = StructuralOp::DeleteCols { at: 1, count: 3 };
        assert_eq!(shift_cell_ref(&r, &op), RefShift::Deleted);
    }

    #[test]
    fn delete_cols_shifts_after() {
        let r = CellRef::new(5, 0);
        let op = StructuralOp::DeleteCols { at: 1, count: 3 };
        assert_eq!(
            shift_cell_ref(&r, &op),
            RefShift::Shifted(CellRef::new(2, 0))
        );
    }

    // ===== shift_cell_ref — MoveRange ======================================

    #[test]
    fn move_range_shifts_ref_inside_source() {
        let r = CellRef::new(1, 1); // B2 — inside source A1:C3
        let op = StructuralOp::MoveRange {
            src_col: 0, src_row: 0, src_end_col: 2, src_end_row: 2,
            dst_col: 5, dst_row: 5,
        };
        assert_eq!(
            shift_cell_ref(&r, &op),
            RefShift::Shifted(CellRef::new(6, 6))
        );
    }

    #[test]
    fn move_range_unchanged_outside_source() {
        let r = CellRef::new(4, 4);
        let op = StructuralOp::MoveRange {
            src_col: 0, src_row: 0, src_end_col: 2, src_end_row: 2,
            dst_col: 5, dst_row: 5,
        };
        assert_eq!(shift_cell_ref(&r, &op), RefShift::Unchanged);
    }

    // ===== shift_coord =====================================================

    #[test]
    fn shift_coord_insert_rows() {
        assert_eq!(
            shift_coord((2, 5), &StructuralOp::InsertRows { at: 3, count: 2 }),
            Some((2, 7))
        );
        assert_eq!(
            shift_coord((2, 1), &StructuralOp::InsertRows { at: 3, count: 2 }),
            Some((2, 1))
        );
    }

    #[test]
    fn shift_coord_delete_rows_removes() {
        assert_eq!(
            shift_coord((0, 3), &StructuralOp::DeleteRows { at: 2, count: 3 }),
            None
        );
    }

    #[test]
    fn shift_coord_delete_rows_shifts() {
        assert_eq!(
            shift_coord((0, 7), &StructuralOp::DeleteRows { at: 2, count: 3 }),
            Some((0, 4))
        );
    }

    #[test]
    fn shift_coord_insert_cols() {
        assert_eq!(
            shift_coord((5, 0), &StructuralOp::InsertCols { at: 2, count: 3 }),
            Some((8, 0))
        );
    }

    #[test]
    fn shift_coord_delete_cols() {
        assert_eq!(
            shift_coord((3, 0), &StructuralOp::DeleteCols { at: 1, count: 2 }),
            Some((1, 0))
        );
    }

    #[test]
    fn shift_coord_resize_trims() {
        assert_eq!(
            shift_coord((5, 5), &StructuralOp::ResizeSheet { new_cols: 3, new_rows: 3 }),
            None
        );
        assert_eq!(
            shift_coord((2, 2), &StructuralOp::ResizeSheet { new_cols: 3, new_rows: 3 }),
            Some((2, 2))
        );
    }

    #[test]
    fn shift_coord_move_range() {
        let op = StructuralOp::MoveRange {
            src_col: 0, src_row: 0, src_end_col: 2, src_end_row: 2,
            dst_col: 10, dst_row: 10,
        };
        assert_eq!(shift_coord((1, 1), &op), Some((11, 11)));
        assert_eq!(shift_coord((5, 5), &op), Some((5, 5))); // outside
    }

    // ===== shift_expression ================================================

    #[test]
    fn shift_expr_cell_ref() {
        let expr = Expression::CellReference(CellRef::new(0, 5));
        let op = StructuralOp::InsertRows { at: 3, count: 2 };
        let shifted = shift_expression(&expr, &op).unwrap();
        assert_eq!(shifted, Expression::CellReference(CellRef::new(0, 7)));
    }

    #[test]
    fn shift_expr_unchanged_literal() {
        let expr = Expression::Literal(Value::Number(42.0));
        let op = StructuralOp::InsertRows { at: 0, count: 5 };
        assert!(shift_expression(&expr, &op).is_none());
    }

    #[test]
    fn shift_expr_range() {
        let expr = Expression::Range(RangeRef {
            start: CellRef::new(0, 2),
            end: CellRef::new(0, 5),
        });
        let op = StructuralOp::InsertRows { at: 1, count: 3 };
        let shifted = shift_expression(&expr, &op).unwrap();
        match shifted {
            Expression::Range(r) => {
                assert_eq!(r.start.row, 5);
                assert_eq!(r.end.row, 8);
            }
            other => panic!("expected Range, got {other:?}"),
        }
    }

    #[test]
    fn shift_expr_range_deleted_endpoint() {
        let expr = Expression::Range(RangeRef {
            start: CellRef::new(0, 2),
            end: CellRef::new(0, 5),
        });
        let op = StructuralOp::DeleteRows { at: 2, count: 2 };
        let shifted = shift_expression(&expr, &op).unwrap();
        // Start (row=2) is deleted → whole range becomes #REF!
        assert_eq!(
            shifted,
            Expression::Literal(Value::Error(SpreadsheetError::Ref))
        );
    }

    #[test]
    fn shift_expr_binary_op() {
        let expr = Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(Expression::CellReference(CellRef::new(0, 5))),
            Box::new(Expression::Literal(Value::Number(1.0))),
        );
        let op = StructuralOp::InsertRows { at: 3, count: 2 };
        let shifted = shift_expression(&expr, &op).unwrap();
        match shifted {
            Expression::BinaryOp(BinaryOp::Add, lhs, _rhs) => {
                match *lhs {
                    Expression::CellReference(r) => assert_eq!(r.row, 7),
                    other => panic!("expected CellRef, got {other:?}"),
                }
            }
            other => panic!("expected BinaryOp, got {other:?}"),
        }
    }

    #[test]
    fn shift_expr_function_call() {
        let expr = Expression::FunctionCall(
            "SUM".to_string(),
            vec![Expression::Range(RangeRef {
                start: CellRef::new(0, 0),
                end: CellRef::new(0, 4),
            })],
        );
        let op = StructuralOp::InsertRows { at: 0, count: 1 };
        let shifted = shift_expression(&expr, &op).unwrap();
        match shifted {
            Expression::FunctionCall(name, args) => {
                assert_eq!(name, "SUM");
                match &args[0] {
                    Expression::Range(r) => {
                        assert_eq!(r.start.row, 1);
                        assert_eq!(r.end.row, 5);
                    }
                    other => panic!("expected Range, got {other:?}"),
                }
            }
            other => panic!("expected FunctionCall, got {other:?}"),
        }
    }

    #[test]
    fn shift_expr_delete_produces_ref_error() {
        let expr = Expression::CellReference(CellRef::new(1, 3));
        let op = StructuralOp::DeleteRows { at: 2, count: 3 };
        let shifted = shift_expression(&expr, &op).unwrap();
        assert_eq!(
            shifted,
            Expression::Literal(Value::Error(SpreadsheetError::Ref))
        );
    }

    #[test]
    fn shift_expr_nested_member() {
        let expr = Expression::Member(
            Box::new(Expression::CellReference(CellRef::new(0, 5))),
            MemberKey::Dot("Price".to_string()),
        );
        let op = StructuralOp::InsertRows { at: 3, count: 2 };
        let shifted = shift_expression(&expr, &op).unwrap();
        match shifted {
            Expression::Member(base, MemberKey::Dot(k)) => {
                assert_eq!(k, "Price");
                match *base {
                    Expression::CellReference(r) => assert_eq!(r.row, 7),
                    other => panic!("expected CellRef, got {other:?}"),
                }
            }
            other => panic!("expected Member, got {other:?}"),
        }
    }

    #[test]
    fn shift_expr_array_literal() {
        let expr = Expression::ArrayLiteral(vec![
            vec![Expression::CellReference(CellRef::new(0, 0))],
            vec![Expression::CellReference(CellRef::new(0, 5))],
        ]);
        let op = StructuralOp::InsertRows { at: 3, count: 2 };
        let shifted = shift_expression(&expr, &op).unwrap();
        match shifted {
            Expression::ArrayLiteral(rows) => {
                // First ref (row=0) unchanged, second (row=5) shifted to 7
                match &rows[0][0] {
                    Expression::CellReference(r) => assert_eq!(r.row, 0),
                    other => panic!("expected CellRef, got {other:?}"),
                }
                match &rows[1][0] {
                    Expression::CellReference(r) => assert_eq!(r.row, 7),
                    other => panic!("expected CellRef, got {other:?}"),
                }
            }
            other => panic!("expected ArrayLiteral, got {other:?}"),
        }
    }

    // ===== format_expression ===============================================

    #[test]
    fn format_cell_ref() {
        let expr = Expression::CellReference(CellRef::new(0, 0));
        assert_eq!(format_expression(&expr), "A1");
    }

    #[test]
    fn format_abs_cell_ref() {
        let expr = Expression::CellReference(CellRef {
            col: 1, row: 2, abs_col: true, abs_row: true,
        });
        assert_eq!(format_expression(&expr), "$B$3");
    }

    #[test]
    fn format_range() {
        let expr = Expression::Range(RangeRef {
            start: CellRef::new(0, 0),
            end: CellRef::new(0, 4),
        });
        assert_eq!(format_expression(&expr), "A1:A5");
    }

    #[test]
    fn format_binary_op() {
        let expr = Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(Expression::CellReference(CellRef::new(0, 0))),
            Box::new(Expression::Literal(Value::Number(1.0))),
        );
        assert_eq!(format_expression(&expr), "(A1+1)");
    }

    #[test]
    fn format_function_call() {
        let expr = Expression::FunctionCall(
            "SUM".to_string(),
            vec![Expression::Range(RangeRef {
                start: CellRef::new(0, 0),
                end: CellRef::new(0, 4),
            })],
        );
        assert_eq!(format_expression(&expr), "SUM(A1:A5)");
    }

    #[test]
    fn format_unary_negate() {
        let expr = Expression::UnaryOp(
            UnaryOp::Negate,
            Box::new(Expression::CellReference(CellRef::new(0, 0))),
        );
        assert_eq!(format_expression(&expr), "-A1");
    }

    #[test]
    fn format_member_dot() {
        let expr = Expression::Member(
            Box::new(Expression::CellReference(CellRef::new(0, 0))),
            MemberKey::Dot("Price".to_string()),
        );
        assert_eq!(format_expression(&expr), "A1.Price");
    }

    #[test]
    fn format_nested_expression() {
        let expr = Expression::BinaryOp(
            BinaryOp::Mul,
            Box::new(Expression::BinaryOp(
                BinaryOp::Add,
                Box::new(Expression::CellReference(CellRef::new(0, 0))),
                Box::new(Expression::CellReference(CellRef::new(1, 0))),
            )),
            Box::new(Expression::Literal(Value::Number(2.0))),
        );
        assert_eq!(format_expression(&expr), "((A1+B1)*2)");
    }

    #[test]
    fn format_comparison() {
        let expr = Expression::BinaryOp(
            BinaryOp::Gte,
            Box::new(Expression::CellReference(CellRef::new(0, 0))),
            Box::new(Expression::Literal(Value::Number(10.0))),
        );
        assert_eq!(format_expression(&expr), "(A1>=10)");
    }

    #[test]
    fn format_boolean_literal() {
        assert_eq!(
            format_expression(&Expression::Literal(Value::Boolean(true))),
            "TRUE"
        );
        assert_eq!(
            format_expression(&Expression::Literal(Value::Boolean(false))),
            "FALSE"
        );
    }

    #[test]
    fn format_text_literal() {
        assert_eq!(
            format_expression(&Expression::Literal(Value::Text("hello".into()))),
            "\"hello\""
        );
    }

    #[test]
    fn format_error() {
        assert_eq!(
            format_expression(&Expression::Literal(Value::Error(SpreadsheetError::Ref))),
            "#REF!"
        );
    }

    // ===== shift_hashmap ===================================================

    #[test]
    fn shift_hashmap_insert_rows() {
        let mut map: HashMap<CellCoord, i32> = HashMap::new();
        map.insert((0, 0), 10);
        map.insert((0, 5), 50);
        map.insert((1, 3), 30);

        let op = StructuralOp::InsertRows { at: 3, count: 2 };
        let shifted = shift_hashmap(&map, &op);

        assert_eq!(shifted.get(&(0, 0)), Some(&10)); // unchanged
        assert_eq!(shifted.get(&(0, 7)), Some(&50)); // 5→7
        assert_eq!(shifted.get(&(1, 5)), Some(&30)); // 3→5
        assert_eq!(shifted.len(), 3);
    }

    #[test]
    fn shift_hashmap_delete_rows() {
        let mut map: HashMap<CellCoord, i32> = HashMap::new();
        map.insert((0, 0), 10);
        map.insert((0, 3), 30);
        map.insert((0, 7), 70);

        let op = StructuralOp::DeleteRows { at: 2, count: 3 };
        let shifted = shift_hashmap(&map, &op);

        assert_eq!(shifted.get(&(0, 0)), Some(&10)); // unchanged
        assert!(shifted.get(&(0, 3)).is_none()); // deleted
        assert_eq!(shifted.get(&(0, 4)), Some(&70)); // 7→4
        assert_eq!(shifted.len(), 2);
    }

    // ===== Composite tests — shift expr then format ========================

    #[test]
    fn shift_and_format_insert_rows() {
        // =SUM(A1:A5) with insert 2 rows at row 2
        let expr = Expression::FunctionCall(
            "SUM".to_string(),
            vec![Expression::Range(RangeRef {
                start: CellRef::new(0, 0),
                end: CellRef::new(0, 4),
            })],
        );
        let op = StructuralOp::InsertRows { at: 2, count: 2 };
        let shifted = shift_expression(&expr, &op).unwrap();
        assert_eq!(format_expression(&shifted), "SUM(A1:A7)");
    }

    #[test]
    fn shift_and_format_insert_cols() {
        // =A1+C1 with insert 1 col at col 1 (B)
        let expr = Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(Expression::CellReference(CellRef::new(0, 0))),
            Box::new(Expression::CellReference(CellRef::new(2, 0))),
        );
        let op = StructuralOp::InsertCols { at: 1, count: 1 };
        let shifted = shift_expression(&expr, &op).unwrap();
        // A1 unchanged (col 0 < 1), C1 → D1 (col 2 → 3)
        assert_eq!(format_expression(&shifted), "(A1+D1)");
    }

    #[test]
    fn shift_and_format_delete_cols() {
        // =B1+D1, delete col 1 (B) count 1 → B1 deleted, D1→C1
        let expr = Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(Expression::CellReference(CellRef::new(1, 0))),
            Box::new(Expression::CellReference(CellRef::new(3, 0))),
        );
        let op = StructuralOp::DeleteCols { at: 1, count: 1 };
        let shifted = shift_expression(&expr, &op).unwrap();
        // B1 (col 1) deleted → #REF!, D1 (col 3) → C1 (col 2)
        assert_eq!(format_expression(&shifted), "(#REF!+C1)");
    }

    #[test]
    fn roundtrip_no_shift() {
        // Expression that shouldn't be affected
        let expr = Expression::CellReference(CellRef::new(0, 0));
        let op = StructuralOp::InsertRows { at: 5, count: 1 };
        assert!(shift_expression(&expr, &op).is_none());
    }
}
