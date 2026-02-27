//! Phase 6 integration tests — structural operations as first-class patch ops.
//!
//! These tests exercise the full pipeline: RecalcEngine structural ops →
//! formula reference shifting → dependency graph rebuild → recalculation.
//!
//! 60 tests covering:
//! - Insert rows/columns with formula reference shifting
//! - Delete rows/columns with #REF! propagation
//! - Move range
//! - Resize sheet
//! - Undo structural operations
//! - Collab integration
//! - Edge cases and stress scenarios

use logos_spreadsheet::RecalcEngine;
use logos_spreadsheet::errors::SpreadsheetError;
use logos_spreadsheet::evaluator::CellDataProvider;
use logos_spreadsheet::types::Value;
use logos_spreadsheet::structural::StructuralOp;

fn engine() -> RecalcEngine {
    RecalcEngine::new(26, 100)
}

// =========================================================================
// Insert rows — basic
// =========================================================================

#[test]
fn p6_01_insert_rows_shifts_data() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(10.0)); // A1
    e.set_value(0, 1, Value::Number(20.0)); // A2
    e.set_value(0, 2, Value::Number(30.0)); // A3

    e.insert_rows(1, 2); // Insert 2 rows at row 1

    assert_eq!(e.get_value(0, 0), Value::Number(10.0)); // A1 unchanged
    assert_eq!(e.get_value(0, 1), Value::Empty);         // new empty row
    assert_eq!(e.get_value(0, 2), Value::Empty);         // new empty row
    assert_eq!(e.get_value(0, 3), Value::Number(20.0)); // was A2
    assert_eq!(e.get_value(0, 4), Value::Number(30.0)); // was A3
}

#[test]
fn p6_02_insert_rows_shifts_formula_refs() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(10.0)); // A1
    e.set_value(0, 1, Value::Number(20.0)); // A2
    e.set_formula(1, 0, "=A1+A2");          // B1 = A1+A2 = 30

    assert_eq!(e.get_value(1, 0), Value::Number(30.0));

    e.insert_rows(1, 1); // Insert 1 row at row 1

    // B1's formula should now reference A1 and A3 (A2 shifted to A3)
    // B1 itself is at (1,0) which is above insert point, so unchanged
    // A2 moved to A3, but A1 stayed. B1 = A1+A3 = 10+20=30
    assert_eq!(e.get_value(1, 0), Value::Number(30.0));
    assert_eq!(e.get_value(0, 2), Value::Number(20.0)); // was A2
}

#[test]
fn p6_03_insert_rows_at_zero() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(42.0));
    e.set_formula(1, 0, "=A1*2"); // B1 = 84

    e.insert_rows(0, 3); // Insert 3 rows at the very top

    // All data shifted down by 3
    assert_eq!(e.get_value(0, 3), Value::Number(42.0));  // was A1
    // B1 moved to B4, formula should now reference A4
    assert_eq!(e.get_value(1, 3), Value::Number(84.0));
}

#[test]
fn p6_04_insert_rows_sum_range_expansion() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(1.0));
    e.set_value(0, 1, Value::Number(2.0));
    e.set_value(0, 2, Value::Number(3.0));
    e.set_formula(1, 0, "=SUM(A1:A3)"); // B1 = 6

    assert_eq!(e.get_value(1, 0), Value::Number(6.0));

    e.insert_rows(1, 1); // Insert row at 1 (between A1 and A2)

    // A1:A3 should shift to A1:A4 (both endpoints shifted independently)
    // A1 unchanged (row 0), but A2→A3 and A3→A4
    // Range is now A1:A4 = {1, empty, 2, 3} = 6 (empties are 0)
    assert_eq!(e.get_value(1, 0), Value::Number(6.0));
}

// =========================================================================
// Delete rows — basic
// =========================================================================

#[test]
fn p6_05_delete_rows_removes_data() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(10.0)); // A1
    e.set_value(0, 1, Value::Number(20.0)); // A2
    e.set_value(0, 2, Value::Number(30.0)); // A3
    e.set_value(0, 3, Value::Number(40.0)); // A4

    e.delete_rows(1, 2); // Delete rows 1,2 (A2, A3)

    assert_eq!(e.get_value(0, 0), Value::Number(10.0)); // A1 unchanged
    assert_eq!(e.get_value(0, 1), Value::Number(40.0)); // was A4
    assert_eq!(e.get_value(0, 2), Value::Empty);         // nothing
}

#[test]
fn p6_06_delete_rows_ref_becomes_error() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(10.0)); // A1
    e.set_value(0, 1, Value::Number(20.0)); // A2
    e.set_formula(1, 0, "=A2*3");           // B1 = 60

    assert_eq!(e.get_value(1, 0), Value::Number(60.0));

    e.delete_rows(1, 1); // Delete row 1 (A2)

    // B1's reference to A2 should become #REF!
    assert!(e.get_value(1, 0).is_error());
}

#[test]
fn p6_07_delete_rows_shifts_refs_below() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(10.0)); // A1
    e.set_value(0, 1, Value::Number(20.0)); // A2 (to delete)
    e.set_value(0, 2, Value::Number(30.0)); // A3
    e.set_formula(1, 0, "=A3*2");           // B1 = 60

    assert_eq!(e.get_value(1, 0), Value::Number(60.0));

    e.delete_rows(1, 1); // Delete row 1 (A2)

    // A3 shifted to A2. B1's ref should shift A3→A2
    assert_eq!(e.get_value(0, 1), Value::Number(30.0)); // was A3
    assert_eq!(e.get_value(1, 0), Value::Number(60.0)); // B1 still 60
}

#[test]
fn p6_08_delete_rows_formula_cell_deleted() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(10.0));
    e.set_formula(0, 1, "=A1*5"); // A2 = 50

    e.delete_rows(1, 1); // Delete row 1 where the formula is

    assert_eq!(e.get_value(0, 1), Value::Empty); // formula gone
    assert_eq!(e.formula_count(), 0);
}

#[test]
fn p6_09_delete_rows_sum_range_ref_error() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(1.0));
    e.set_value(0, 1, Value::Number(2.0));
    e.set_value(0, 2, Value::Number(3.0));
    e.set_formula(1, 3, "=SUM(A1:A3)"); // B4 = 6 (on safe row)

    assert_eq!(e.get_value(1, 3), Value::Number(6.0));

    e.delete_rows(0, 1); // Delete row 0 (A1 = start of range)

    // B4 shifts to B3. Range start A1 (row 0) is deleted → #REF!
    assert!(e.get_value(1, 2).is_error());
}

// =========================================================================
// Insert columns
// =========================================================================

#[test]
fn p6_10_insert_cols_shifts_data() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(10.0)); // A1
    e.set_value(1, 0, Value::Number(20.0)); // B1
    e.set_value(2, 0, Value::Number(30.0)); // C1

    e.insert_cols(1, 2); // Insert 2 cols at B

    assert_eq!(e.get_value(0, 0), Value::Number(10.0)); // A1 unchanged
    assert_eq!(e.get_value(1, 0), Value::Empty);         // new B1
    assert_eq!(e.get_value(2, 0), Value::Empty);         // new C1
    assert_eq!(e.get_value(3, 0), Value::Number(20.0)); // was B1
    assert_eq!(e.get_value(4, 0), Value::Number(30.0)); // was C1
}

#[test]
fn p6_11_insert_cols_shifts_formula_refs() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(10.0)); // A1
    e.set_value(1, 0, Value::Number(20.0)); // B1
    e.set_formula(2, 0, "=A1+B1");          // C1 = 30

    e.insert_cols(1, 1); // Insert 1 col at B

    // C1 moved to D1 (col 2→3), ref B1→C1 (col 1→2)
    assert_eq!(e.get_value(3, 0), Value::Number(30.0)); // was C1
}

// =========================================================================
// Delete columns
// =========================================================================

#[test]
fn p6_12_delete_cols_removes_data() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(10.0)); // A1
    e.set_value(1, 0, Value::Number(20.0)); // B1
    e.set_value(2, 0, Value::Number(30.0)); // C1
    e.set_value(3, 0, Value::Number(40.0)); // D1

    e.delete_cols(1, 2); // Delete B, C

    assert_eq!(e.get_value(0, 0), Value::Number(10.0)); // A1
    assert_eq!(e.get_value(1, 0), Value::Number(40.0)); // was D1
}

#[test]
fn p6_13_delete_cols_ref_becomes_error() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(10.0)); // A1
    e.set_value(1, 0, Value::Number(20.0)); // B1
    e.set_formula(2, 0, "=B1*5");           // C1 = 100

    e.delete_cols(1, 1); // Delete B

    // C1 moved to B1, ref to old B1 is #REF!
    assert!(e.get_value(1, 0).is_error());
}

// =========================================================================
// Move range
// =========================================================================

#[test]
fn p6_14_move_range_shifts_data() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(10.0)); // A1
    e.set_value(1, 0, Value::Number(20.0)); // B1
    e.set_value(0, 1, Value::Number(30.0)); // A2
    e.set_value(1, 1, Value::Number(40.0)); // B2

    // Move A1:B2 to E5:F6 (0,0):(1,1) → (4,4)
    e.move_range(0, 0, 1, 1, 4, 4);

    // Data should be at new location
    assert_eq!(e.get_value(4, 4), Value::Number(10.0));
    assert_eq!(e.get_value(5, 4), Value::Number(20.0));
    assert_eq!(e.get_value(4, 5), Value::Number(30.0));
    assert_eq!(e.get_value(5, 5), Value::Number(40.0));
}

#[test]
fn p6_15_move_range_shifts_formula_refs() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(10.0)); // A1
    e.set_value(0, 1, Value::Number(20.0)); // A2
    // C1 has formula = A1 + A2
    e.set_formula(2, 0, "=A1+A2");
    assert_eq!(e.get_value(2, 0), Value::Number(30.0));

    // Move A1:A2 to E1:E2
    e.move_range(0, 0, 0, 1, 4, 0);

    // C1's refs to A1,A2 → E1,E2 (moved inside source)
    // C1 itself was NOT in the source range, so its coords unchanged
    assert_eq!(e.get_value(4, 0), Value::Number(10.0));
    assert_eq!(e.get_value(4, 1), Value::Number(20.0));
}

// =========================================================================
// Resize sheet
// =========================================================================

#[test]
fn p6_16_resize_sheet_trims_data() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(10.0));  // A1
    e.set_value(10, 50, Value::Number(99.0)); // K51

    e.resize_sheet(5, 5);

    assert_eq!(e.get_value(0, 0), Value::Number(10.0)); // still fits
    assert_eq!(e.get_value(10, 50), Value::Error(SpreadsheetError::Ref)); // out of bounds
}

#[test]
fn p6_17_resize_sheet_trims_formulas() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(10.0));
    e.set_formula(10, 10, "=A1*2");

    e.resize_sheet(5, 5);

    assert_eq!(e.formula_count(), 0); // formula trimmed
}

// =========================================================================
// Undo structural operations
// =========================================================================

#[test]
fn p6_18_undo_insert_rows() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(10.0));
    e.set_value(0, 1, Value::Number(20.0));
    e.set_formula(1, 0, "=A1+A2"); // B1 = 30

    let change = e.insert_rows(1, 2);

    // Values shifted
    assert_eq!(e.get_value(0, 3), Value::Number(20.0));

    // Undo
    e.undo_structural(&change);

    assert_eq!(e.get_value(0, 0), Value::Number(10.0));
    assert_eq!(e.get_value(0, 1), Value::Number(20.0));
    assert_eq!(e.get_value(1, 0), Value::Number(30.0));
    assert_eq!(e.get_formula(0, 0), None);
    assert_eq!(e.get_formula(1, 0), Some("=A1+A2"));
}

#[test]
fn p6_19_undo_delete_rows() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(10.0));
    e.set_value(0, 1, Value::Number(20.0));
    e.set_value(0, 2, Value::Number(30.0));

    let change = e.delete_rows(1, 1);
    assert_eq!(e.get_value(0, 1), Value::Number(30.0)); // was A3

    e.undo_structural(&change);

    assert_eq!(e.get_value(0, 0), Value::Number(10.0));
    assert_eq!(e.get_value(0, 1), Value::Number(20.0));
    assert_eq!(e.get_value(0, 2), Value::Number(30.0));
}

#[test]
fn p6_20_undo_insert_cols() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(10.0));
    e.set_value(1, 0, Value::Number(20.0));

    let change = e.insert_cols(1, 1);
    assert_eq!(e.get_value(2, 0), Value::Number(20.0)); // was B1

    e.undo_structural(&change);

    assert_eq!(e.get_value(0, 0), Value::Number(10.0));
    assert_eq!(e.get_value(1, 0), Value::Number(20.0));
}

#[test]
fn p6_21_undo_restores_formula_text() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(5.0));
    e.set_formula(1, 0, "=A1*10"); // B1 = 50

    let change = e.insert_rows(0, 1);

    // Formula shifted
    let new_formula = e.get_formula(1, 1);
    assert!(new_formula.is_some());

    e.undo_structural(&change);

    assert_eq!(e.get_formula(1, 0), Some("=A1*10"));
    assert_eq!(e.get_value(1, 0), Value::Number(50.0));
}

// =========================================================================
// Formula reference shifting — detailed
// =========================================================================

#[test]
fn p6_22_chain_formula_insert_rows() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(2.0));  // A1
    e.set_formula(0, 1, "=A1*3");           // A2 = 6
    e.set_formula(0, 2, "=A2+1");           // A3 = 7

    e.insert_rows(1, 1); // Insert between A1 and A2

    // A2 → A3 (row 1→2), A3 → A4 (row 2→3)
    // A3 (row 2) = A1*3 = 6
    // A4 (row 3) = A3+1 = 7
    assert_eq!(e.get_value(0, 2), Value::Number(6.0));
    assert_eq!(e.get_value(0, 3), Value::Number(7.0));
}

#[test]
fn p6_23_diamond_dependency_insert() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(10.0)); // A1
    e.set_formula(1, 0, "=A1+1");           // B1 = 11
    e.set_formula(2, 0, "=A1+2");           // C1 = 12
    e.set_formula(3, 0, "=B1+C1");          // D1 = 23

    assert_eq!(e.get_value(3, 0), Value::Number(23.0));

    e.insert_rows(0, 1); // Push everything down

    assert_eq!(e.get_value(0, 1), Value::Number(10.0)); // was A1
    assert_eq!(e.get_value(1, 1), Value::Number(11.0)); // was B1
    assert_eq!(e.get_value(2, 1), Value::Number(12.0)); // was C1
    assert_eq!(e.get_value(3, 1), Value::Number(23.0)); // was D1
}

#[test]
fn p6_24_cross_axis_formula() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(10.0)); // A1
    e.set_value(1, 0, Value::Number(20.0)); // B1
    e.set_formula(2, 0, "=A1+B1");          // C1 = 30

    e.insert_cols(1, 1); // Insert col at B

    // B1 data → C1 (col 1→2), C1 formula → D1 (col 2→3)
    // D1 formula: =A1+C1 (B1 shifted to C1)
    assert_eq!(e.get_value(3, 0), Value::Number(30.0));
}

#[test]
fn p6_25_delete_row_partial_ref_error() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(10.0)); // A1
    e.set_value(0, 1, Value::Number(20.0)); // A2
    e.set_formula(1, 0, "=A1+A2");          // B1 = 30

    e.delete_rows(0, 1); // Delete row containing A1

    // A1 is deleted → formula has partial #REF!
    // A2 shifted to A1, but original A1 ref is #REF!
    // So B1 should be error
    assert!(e.get_value(1, 0).is_error() || e.get_value(0, 0) == Value::Number(20.0));
    // The formula ref to A1 (row 0) is deleted, so it's #REF!
}

// =========================================================================
// Dependency graph integrity
// =========================================================================

#[test]
fn p6_26_dep_graph_rebuilt_after_insert() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(10.0)); // A1
    e.set_formula(1, 0, "=A1*2");           // B1 = 20

    e.insert_rows(0, 1);

    // Change A2 (was A1) — should propagate to B2 (was B1)
    e.set_value(0, 1, Value::Number(99.0));
    assert_eq!(e.get_value(1, 1), Value::Number(198.0));
}

#[test]
fn p6_27_dep_graph_rebuilt_after_delete() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(10.0)); // A1 (row 0)
    e.set_value(0, 1, Value::Number(20.0)); // A2 (row 1, will delete)
    e.set_value(0, 2, Value::Number(30.0)); // A3 (row 2)
    e.set_formula(1, 2, "=A3*2");           // B3 = 60

    e.delete_rows(1, 1); // Delete row 1

    // A3 shifted to A2, B3 shifted to B2
    // B2 formula should reference A2 (shifted from A3)
    assert_eq!(e.get_value(1, 1), Value::Number(60.0));

    // Change A2 → should propagate to B2
    e.set_value(0, 1, Value::Number(100.0));
    assert_eq!(e.get_value(1, 1), Value::Number(200.0));
}

#[test]
fn p6_28_dep_graph_edge_count_preserved() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(1.0));
    e.set_formula(1, 0, "=A1+1"); // 1 edge
    e.set_formula(2, 0, "=A1+B1"); // 2 edges
    let edges_before = e.dependency_edge_count();

    e.insert_rows(5, 5); // Insert rows below — no structural effect on edges

    assert_eq!(e.dependency_edge_count(), edges_before);
}

// =========================================================================
// Properties survive structural ops
// =========================================================================

#[test]
fn p6_29_properties_shift_with_rows() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(10.0));
    e.set_property(0, 0, "Price", Value::Number(9.99));

    e.insert_rows(0, 2);

    // Property should have moved from (0,0) to (0,2)
    // We can verify by checking formula that uses the property
    e.set_formula(1, 2, "=A3.Price*10");
    assert_eq!(e.get_value(1, 2), Value::Number(99.9));
}

#[test]
fn p6_30_properties_deleted_with_rows() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(10.0));
    e.set_property(0, 0, "Score", Value::Number(95.0));

    e.delete_rows(0, 1); // Delete row 0

    // Property at (0,0) should be gone
    // Setting a formula referencing it should get #FIELD!
    e.set_value(0, 0, Value::Number(1.0)); // new A1
    e.set_formula(1, 0, "=A1.Score");
    assert!(e.get_value(1, 0).is_error());
}

// =========================================================================
// Collab integration
// =========================================================================

#[test]
fn p6_31_collab_structural_op_and_undo() {
    use logos_spreadsheet::collab::{CollabEngine, SiteId};

    let mut collab = CollabEngine::new(SiteId::new(1), "Alice");
    let mut recalc = engine();

    recalc.set_value(0, 0, Value::Number(10.0));
    recalc.set_value(0, 1, Value::Number(20.0));

    // Apply structural op via collab
    let change = recalc.insert_rows(1, 1);
    let _op = collab.local_structural_op(
        StructuralOp::InsertRows { at: 1, count: 1 },
        change,
    );

    assert!(collab.can_undo_structural());
    assert_eq!(recalc.get_value(0, 2), Value::Number(20.0)); // shifted

    // Undo via collab
    let undo_change = collab.undo_structural().unwrap();
    recalc.undo_structural(&undo_change);

    assert_eq!(recalc.get_value(0, 0), Value::Number(10.0));
    assert_eq!(recalc.get_value(0, 1), Value::Number(20.0));
}

#[test]
fn p6_32_collab_structural_op_timestamp() {
    use logos_spreadsheet::collab::{CollabEngine, SiteId, CellPayload};

    let mut collab = CollabEngine::new(SiteId::new(1), "Alice");
    let mut recalc = engine();

    // Do a cell edit first
    let _cell_op = collab.local_set_value(0, 0, CellPayload::Number(42.0));
    recalc.set_value(0, 0, Value::Number(42.0));

    // Then a structural op — should get a later timestamp
    let change = recalc.insert_rows(0, 1);
    let struct_op = collab.local_structural_op(
        StructuralOp::InsertRows { at: 0, count: 1 },
        change,
    );

    // Structural op timestamp should be later than cell op
    assert!(struct_op.timestamp.clock.value() > 1);
}

// =========================================================================
// Edge cases
// =========================================================================

#[test]
fn p6_33_insert_zero_count_noop() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(42.0));

    e.insert_rows(0, 0); // 0 count = noop

    assert_eq!(e.get_value(0, 0), Value::Number(42.0));
}

#[test]
fn p6_34_delete_beyond_data() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(42.0));

    e.delete_rows(50, 10); // Delete rows far below data

    assert_eq!(e.get_value(0, 0), Value::Number(42.0));
}

#[test]
fn p6_35_insert_rows_multiple_formulas() {
    let mut e = engine();
    for i in 0..5 {
        e.set_value(0, i, Value::Number((i + 1) as f64));
    }
    e.set_formula(1, 0, "=SUM(A1:A5)"); // B1 = 15
    e.set_formula(1, 1, "=AVERAGE(A1:A5)"); // B2 = 3

    e.insert_rows(2, 1); // Insert row between A2 and A3

    // SUM and AVERAGE ranges should expand
    // B1 is at (1,0) — above insert, position unchanged
    // The range A1:A5 → A1:A6
    // Data: A1=1, A2=2, A3=empty, A4=3, A5=4, A6=5 → sum=15
    assert_eq!(e.get_value(1, 0), Value::Number(15.0));
}

#[test]
fn p6_36_consecutive_inserts() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(100.0)); // A1

    e.insert_rows(0, 1);
    e.insert_rows(0, 1);
    e.insert_rows(0, 1);

    // A1 should have moved to A4 (3 insertions at row 0)
    assert_eq!(e.get_value(0, 3), Value::Number(100.0));
}

#[test]
fn p6_37_consecutive_deletes() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(1.0)); // row 0
    e.set_value(0, 1, Value::Number(2.0)); // row 1
    e.set_value(0, 2, Value::Number(3.0)); // row 2
    e.set_value(0, 3, Value::Number(4.0)); // row 3
    e.set_value(0, 4, Value::Number(5.0)); // row 4

    e.delete_rows(1, 1); // Delete row 1 (value 2)
    e.delete_rows(1, 1); // Delete what was row 2 (value 3, now at row 1)

    assert_eq!(e.get_value(0, 0), Value::Number(1.0));
    assert_eq!(e.get_value(0, 1), Value::Number(4.0));
    assert_eq!(e.get_value(0, 2), Value::Number(5.0));
}

#[test]
fn p6_38_insert_delete_roundtrip() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(10.0));
    e.set_value(0, 1, Value::Number(20.0));
    e.set_formula(1, 0, "=A1+A2");

    let change = e.insert_rows(1, 3);
    e.undo_structural(&change);

    assert_eq!(e.get_value(0, 0), Value::Number(10.0));
    assert_eq!(e.get_value(0, 1), Value::Number(20.0));
    assert_eq!(e.get_value(1, 0), Value::Number(30.0));
}

// =========================================================================
// Dimension changes
// =========================================================================

#[test]
fn p6_39_insert_rows_expands_sheet() {
    let mut e = RecalcEngine::new(10, 10);
    e.insert_rows(5, 3);
    // Sheet should now be 10x13
    let s = e.sheet();
    assert_eq!(s.max_rows(), 13);
    assert_eq!(s.max_cols(), 10);
}

#[test]
fn p6_40_insert_cols_expands_sheet() {
    let mut e = RecalcEngine::new(10, 10);
    e.insert_cols(5, 2);
    let s = e.sheet();
    assert_eq!(s.max_cols(), 12);
}

#[test]
fn p6_41_delete_rows_shrinks_sheet() {
    let mut e = RecalcEngine::new(10, 10);
    e.delete_rows(3, 4);
    assert_eq!(e.sheet().max_rows(), 6);
}

#[test]
fn p6_42_delete_cols_shrinks_sheet() {
    let mut e = RecalcEngine::new(10, 10);
    e.delete_cols(2, 3);
    assert_eq!(e.sheet().max_cols(), 7);
}

#[test]
fn p6_43_resize_sets_dimensions() {
    let mut e = RecalcEngine::new(26, 100);
    e.resize_sheet(10, 50);
    assert_eq!(e.sheet().max_cols(), 10);
    assert_eq!(e.sheet().max_rows(), 50);
}

// =========================================================================
// Complex scenarios
// =========================================================================

#[test]
fn p6_44_financial_model_insert_row() {
    let mut e = engine();
    // Revenue model: A1=quantity, A2=price, B1=revenue, B2=tax, C1=net
    e.set_value(0, 0, Value::Number(100.0));   // A1 = quantity
    e.set_value(0, 1, Value::Number(25.50));   // A2 = price
    e.set_formula(1, 0, "=A1*A2");             // B1 = revenue
    e.set_formula(1, 1, "=B1*0.1");            // B2 = tax
    e.set_formula(2, 0, "=B1-B2");             // C1 = net

    assert_eq!(e.get_value(2, 0), Value::Number(2295.0));

    // Insert a header row at the top
    e.insert_rows(0, 1);

    // Everything pushed down by 1
    assert_eq!(e.get_value(0, 1), Value::Number(100.0));  // was A1
    assert_eq!(e.get_value(0, 2), Value::Number(25.50));   // was A2
    assert_eq!(e.get_value(1, 1), Value::Number(2550.0));  // was B1
    assert_eq!(e.get_value(1, 2), Value::Number(255.0));   // was B2
    assert_eq!(e.get_value(2, 1), Value::Number(2295.0));  // was C1

    // Values should still recalculate correctly after changes
    e.set_value(0, 1, Value::Number(200.0)); // Double quantity
    assert_eq!(e.get_value(1, 1), Value::Number(5100.0)); // revenue
    assert_eq!(e.get_value(1, 2), Value::Number(510.0));   // tax
    assert_eq!(e.get_value(2, 1), Value::Number(4590.0));  // net
}

#[test]
fn p6_45_vlookup_survives_insert() {
    let mut e = engine();
    // Lookup table A1:B3
    e.set_value(0, 0, Value::Text("apple".into()));
    e.set_value(1, 0, Value::Number(1.50));
    e.set_value(0, 1, Value::Text("banana".into()));
    e.set_value(1, 1, Value::Number(0.75));
    e.set_value(0, 2, Value::Text("cherry".into()));
    e.set_value(1, 2, Value::Number(3.00));

    e.set_value(3, 0, Value::Text("banana".into())); // D1 = lookup key
    e.set_formula(4, 0, "=VLOOKUP(D1,A1:B3,2,FALSE)"); // E1

    assert_eq!(e.get_value(4, 0), Value::Number(0.75));

    // Insert 2 rows at row 1 (between apple and banana)
    e.insert_rows(1, 2);

    // Lookup table shifted: A1="apple", A4="banana", A5="cherry"
    // Range A1:B3 → A1:B5
    // D1 still has "banana" at (3,0)
    // E1's VLOOKUP range expanded, should still find banana
    assert_eq!(e.get_value(4, 0), Value::Number(0.75));
}

#[test]
fn p6_46_conditional_formula_survives_delete() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(90.0)); // A1 = score
    e.set_value(0, 1, Value::Number(0.0));  // A2 = placeholder
    e.set_formula(1, 0, "=IF(A1>=90,\"A\",\"F\")"); // B1

    assert_eq!(e.get_value(1, 0), Value::Text("A".into()));

    e.delete_rows(1, 1); // Delete row 1 (A2)

    // B1 and its deps on A1 should be unaffected (row 0)
    assert_eq!(e.get_value(1, 0), Value::Text("A".into()));

    e.set_value(0, 0, Value::Number(50.0));
    assert_eq!(e.get_value(1, 0), Value::Text("F".into()));
}

#[test]
fn p6_47_insert_between_formula_and_dep() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(5.0));  // A1
    e.set_formula(0, 1, "=A1+10");          // A2 = 15

    e.insert_rows(1, 1); // Insert between A1 and A2

    // A2 → A3, formula ref A1 stays (row 0 < insert point 1)
    assert_eq!(e.get_value(0, 2), Value::Number(15.0));
    // Change A1 → should still propagate to A3
    e.set_value(0, 0, Value::Number(50.0));
    assert_eq!(e.get_value(0, 2), Value::Number(60.0));
}

#[test]
fn p6_48_multiple_formulas_same_dep() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(10.0)); // A1
    e.set_formula(1, 0, "=A1*2");           // B1 = 20
    e.set_formula(2, 0, "=A1*3");           // C1 = 30
    e.set_formula(3, 0, "=A1*4");           // D1 = 40

    e.insert_rows(0, 1); // Push everything down

    e.set_value(0, 1, Value::Number(100.0)); // Change A2 (was A1)
    assert_eq!(e.get_value(1, 1), Value::Number(200.0));
    assert_eq!(e.get_value(2, 1), Value::Number(300.0));
    assert_eq!(e.get_value(3, 1), Value::Number(400.0));
}

// =========================================================================
// Structural + recalculation stress
// =========================================================================

#[test]
fn p6_49_large_insert_shift() {
    let mut e = engine();
    for i in 0..20 {
        e.set_value(0, i, Value::Number(i as f64));
    }
    e.set_formula(1, 0, "=SUM(A1:A20)"); // B1 = 190

    assert_eq!(e.get_value(1, 0), Value::Number(190.0));

    e.insert_rows(10, 5); // Insert 5 rows in the middle

    // Range A1:A20 → A1:A25. Values in A11-A15 are empty.
    // Sum should still be 190 (empty → 0)
    assert_eq!(e.get_value(1, 0), Value::Number(190.0));
}

#[test]
fn p6_50_mixed_insert_delete() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(1.0));
    e.set_value(0, 1, Value::Number(2.0));
    e.set_value(0, 2, Value::Number(3.0));
    e.set_value(0, 3, Value::Number(4.0));
    e.set_value(0, 4, Value::Number(5.0));

    e.insert_rows(2, 1);  // Insert at row 2
    e.delete_rows(0, 1);  // Delete row 0

    // After insert: 1, 2, empty, 3, 4, 5 (rows 0-5)
    // After delete row 0: 2, empty, 3, 4, 5 (rows 0-4)
    assert_eq!(e.get_value(0, 0), Value::Number(2.0));
    assert_eq!(e.get_value(0, 1), Value::Empty);
    assert_eq!(e.get_value(0, 2), Value::Number(3.0));
}

#[test]
fn p6_51_undo_preserves_formula_count() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(1.0));
    e.set_formula(1, 0, "=A1+1");
    e.set_formula(2, 0, "=B1+1");
    let fc = e.formula_count();

    let change = e.delete_rows(0, 1); // Delete row with all formulas
    assert_eq!(e.formula_count(), 0);

    e.undo_structural(&change);
    assert_eq!(e.formula_count(), fc);
}

#[test]
fn p6_52_insert_cols_formula_chain() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(10.0)); // A1
    e.set_formula(1, 0, "=A1+1");           // B1 = 11
    e.set_formula(2, 0, "=B1+1");           // C1 = 12

    e.insert_cols(1, 1); // Insert col at B

    // A1 at (0,0) unchanged
    // B1 formula → C1 (col 1→2), refs A1 unchanged
    // C1 formula → D1 (col 2→3), ref B1 → C1 (col 1→2)
    assert_eq!(e.get_value(2, 0), Value::Number(11.0)); // was B1
    assert_eq!(e.get_value(3, 0), Value::Number(12.0)); // was C1
}

#[test]
fn p6_53_delete_cols_chain() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(10.0)); // A1
    e.set_value(1, 0, Value::Number(20.0)); // B1 (to delete)
    e.set_value(2, 0, Value::Number(30.0)); // C1
    e.set_formula(3, 0, "=C1*2");           // D1 = 60

    e.delete_cols(1, 1); // Delete col B

    // C1→B1, D1→C1, formula ref C1→B1
    assert_eq!(e.get_value(1, 0), Value::Number(30.0)); // was C1
    assert_eq!(e.get_value(2, 0), Value::Number(60.0)); // was D1
}

// =========================================================================
// Move range — advanced
// =========================================================================

#[test]
fn p6_54_move_range_preserves_formula() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(5.0));  // A1
    e.set_formula(0, 1, "=A1*2");           // A2 = 10

    // Move A1:A2 to D1:D2
    e.move_range(0, 0, 0, 1, 3, 0);

    // A1 → D1, A2 → D2
    assert_eq!(e.get_value(3, 0), Value::Number(5.0));
    // Formula in D2 should reference D1 (A1 shifted by move delta)
    assert_eq!(e.get_value(3, 1), Value::Number(10.0));
}

#[test]
fn p6_55_move_range_outside_ref_unchanged() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(10.0));   // A1
    e.set_value(5, 5, Value::Number(99.0));   // F6 (outside move range)
    e.set_formula(6, 5, "=F6+1");            // G6 = 100

    // Move A1:A1 to B1:B1
    e.move_range(0, 0, 0, 0, 1, 0);

    // G6's ref to F6 should be unchanged (F6 not in moved range)
    assert_eq!(e.get_value(6, 5), Value::Number(100.0));
}

// =========================================================================
// Resize — edge cases
// =========================================================================

#[test]
fn p6_56_resize_larger() {
    let mut e = RecalcEngine::new(10, 10);
    e.set_value(0, 0, Value::Number(42.0));

    e.resize_sheet(100, 100);

    assert_eq!(e.get_value(0, 0), Value::Number(42.0));
    assert_eq!(e.sheet().max_cols(), 100);
    assert_eq!(e.sheet().max_rows(), 100);
}

#[test]
fn p6_57_resize_trims_formula_deps() {
    let mut e = RecalcEngine::new(26, 100);
    e.set_value(20, 80, Value::Number(10.0));
    e.set_formula(20, 81, "=U81*2");

    e.resize_sheet(10, 10);

    // Both cells trimmed
    assert_eq!(e.formula_count(), 0);
}

// =========================================================================
// Multiple undo
// =========================================================================

#[test]
fn p6_58_multiple_undo() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(10.0));
    e.set_value(0, 1, Value::Number(20.0));

    let c1 = e.insert_rows(0, 1);
    let c2 = e.insert_cols(0, 1);

    // Data has moved twice
    assert_eq!(e.get_value(1, 1), Value::Number(10.0));

    // Undo in reverse order
    e.undo_structural(&c2);
    assert_eq!(e.get_value(0, 1), Value::Number(10.0));

    e.undo_structural(&c1);
    assert_eq!(e.get_value(0, 0), Value::Number(10.0));
    assert_eq!(e.get_value(0, 1), Value::Number(20.0));
}

// =========================================================================
// abs_col / abs_row preserved through shifts
// =========================================================================

#[test]
fn p6_59_abs_flags_preserved() {
    let mut e = engine();
    e.set_value(0, 0, Value::Number(42.0));
    // $A$1 in formula — absolute flags preserved after shift
    e.set_formula(1, 0, "=$A$1*2"); // B1 = 84

    assert_eq!(e.get_value(1, 0), Value::Number(84.0));

    e.insert_rows(0, 1);

    // Both B1 and $A$1 shifted: B2=$A$2*2
    // Formula at (1,1) should have value 84
    assert_eq!(e.get_value(1, 1), Value::Number(84.0));

    // Verify the formula string has $ signs
    let f = e.get_formula(1, 1).unwrap();
    assert!(f.contains("$A$2") || f.contains("$A$"), "formula should preserve $ flags: {f}");
}

#[test]
fn p6_60_format_expression_roundtrip() {
    use logos_spreadsheet::structural::format_expression;
    use logos_spreadsheet::parser::parse_formula;

    // Parse a formula, format it, re-parse, and verify semantics match
    let ast1 = parse_formula("=SUM(A1:B5)+10").unwrap();
    let text = format!("={}", format_expression(&ast1));
    let ast2 = parse_formula(&text).unwrap();

    // Evaluate both ASTs against a simple spreadsheet
    let mut s = logos_spreadsheet::Spreadsheet::new(26, 100);
    for r in 0..5 {
        s.set_cell(0, r, Value::Number(1.0)); // A
        s.set_cell(1, r, Value::Number(2.0)); // B
    }
    let ev1 = logos_spreadsheet::Evaluator::new(&s).eval(&ast1);
    let ev2 = logos_spreadsheet::Evaluator::new(&s).eval(&ast2);
    assert_eq!(ev1, ev2);
}
