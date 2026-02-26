//! Comprehensive integration tests for the spreadsheet engine.
//!
//! Week 1 (21 tests): Cell references, basic arithmetic, precedence
//! Week 2 (39 tests): Member access, error matrix, operator precedence
//! Week 3 (67 tests): Functions, ranges, arrays, conditionals, lookups

use logos_spreadsheet::evaluator::{eval_formula, Spreadsheet};
use logos_spreadsheet::errors::SpreadsheetError;
use logos_spreadsheet::types::*;
use logos_spreadsheet::parser::parse_formula;

/// Helper: create a spreadsheet with some pre-loaded data
fn sample_sheet() -> Spreadsheet {
    let mut s = Spreadsheet::new(26, 100); // A–Z, 100 rows
    // Numbers
    s.set_cell(0, 0, Value::Number(10.0));   // A1 = 10
    s.set_cell(0, 1, Value::Number(20.0));   // A2 = 20
    s.set_cell(0, 2, Value::Number(30.0));   // A3 = 30
    s.set_cell(0, 3, Value::Number(40.0));   // A4 = 40
    s.set_cell(0, 4, Value::Number(50.0));   // A5 = 50

    s.set_cell(1, 0, Value::Number(100.0));  // B1 = 100
    s.set_cell(1, 1, Value::Number(200.0));  // B2 = 200
    s.set_cell(1, 2, Value::Number(300.0));  // B3 = 300
    s.set_cell(1, 3, Value::Number(400.0));  // B4 = 400
    s.set_cell(1, 4, Value::Number(500.0));  // B5 = 500

    // Text
    s.set_cell(2, 0, Value::Text("Hello".into()));  // C1 = "Hello"
    s.set_cell(2, 1, Value::Text("World".into()));  // C2 = "World"

    // Boolean
    s.set_cell(3, 0, Value::Boolean(true));   // D1 = TRUE
    s.set_cell(3, 1, Value::Boolean(false));  // D2 = FALSE

    // Properties for member access tests
    s.set_property(0, 0, "Price", Value::Number(99.99));      // A1.Price = 99.99
    s.set_property(0, 1, "Price", Value::Number(149.99));     // A2.Price = 149.99
    s.set_property(0, 0, "Market Cap", Value::Number(1e9));   // A1["Market Cap"]
    s.set_property(0, 0, "note", Value::Text("test note".into()));
    s.set_property(0, 0, "format", Value::Text("currency".into()));
    s.set_property(0, 0, "style", Value::Text("bold".into()));

    // Error cell
    s.set_cell(4, 0, Value::Error(SpreadsheetError::Ref));    // E1 = #REF!

    // Lookup table in F1:H5
    // Name      | Dept       | Salary
    // Alice     | Eng        | 100000
    // Bob       | Sales      | 80000
    // Charlie   | Eng        | 120000
    // Diana     | Marketing  | 90000
    // Eve       | Sales      | 85000
    s.set_cell(5, 0, Value::Text("Alice".into()));     // F1
    s.set_cell(6, 0, Value::Text("Eng".into()));       // G1
    s.set_cell(7, 0, Value::Number(100000.0));          // H1
    s.set_cell(5, 1, Value::Text("Bob".into()));       // F2
    s.set_cell(6, 1, Value::Text("Sales".into()));     // G2
    s.set_cell(7, 1, Value::Number(80000.0));           // H2
    s.set_cell(5, 2, Value::Text("Charlie".into()));   // F3
    s.set_cell(6, 2, Value::Text("Eng".into()));       // G3
    s.set_cell(7, 2, Value::Number(120000.0));          // H3
    s.set_cell(5, 3, Value::Text("Diana".into()));     // F4
    s.set_cell(6, 3, Value::Text("Marketing".into())); // G4
    s.set_cell(7, 3, Value::Number(90000.0));           // H4
    s.set_cell(5, 4, Value::Text("Eve".into()));       // F5
    s.set_cell(6, 4, Value::Text("Sales".into()));     // G5
    s.set_cell(7, 4, Value::Number(85000.0));           // H5

    s
}

fn eval(formula: &str) -> Value {
    eval_formula(formula, &sample_sheet())
}

fn assert_num(formula: &str, expected: f64) {
    let result = eval(formula);
    match &result {
        Value::Number(n) => {
            assert!(
                (n - expected).abs() < 1e-6,
                "formula `{formula}` = {n}, expected {expected}"
            );
        }
        other => panic!("formula `{formula}` = {other:?}, expected Number({expected})"),
    }
}

fn assert_text(formula: &str, expected: &str) {
    let result = eval(formula);
    match &result {
        Value::Text(s) => assert_eq!(s, expected, "formula `{formula}`"),
        other => panic!("formula `{formula}` = {other:?}, expected Text(\"{expected}\")"),
    }
}

fn assert_bool(formula: &str, expected: bool) {
    let result = eval(formula);
    match &result {
        Value::Boolean(b) => assert_eq!(*b, expected, "formula `{formula}`"),
        other => panic!("formula `{formula}` = {other:?}, expected Boolean({expected})"),
    }
}

fn assert_err(formula: &str, expected: SpreadsheetError) {
    let result = eval(formula);
    match &result {
        Value::Error(e) => assert_eq!(e, &expected, "formula `{formula}`"),
        other => panic!("formula `{formula}` = {other:?}, expected Error({expected})"),
    }
}

// ===================================================================
// WEEK 1: Cell references, basic arithmetic, precedence  (21 tests)
// ===================================================================

mod week1 {
    use super::*;

    // --- Cell References (5 tests) ---

    #[test]
    fn w1_01_simple_cell_ref() {
        assert_num("=A1", 10.0);
    }

    #[test]
    fn w1_02_cell_ref_different_cell() {
        assert_num("=B3", 300.0);
    }

    #[test]
    fn w1_03_cell_ref_text() {
        assert_text("=C1", "Hello");
    }

    #[test]
    fn w1_04_cell_ref_boolean() {
        assert_bool("=D1", true);
    }

    #[test]
    fn w1_05_empty_cell() {
        // Empty cell returns Empty value; becomes 0 only in arithmetic context
        let result = eval("=Z99");
        assert!(matches!(result, Value::Empty), "expected Empty, got {result:?}");
    }

    // --- Basic Arithmetic (8 tests) ---

    #[test]
    fn w1_06_addition() {
        assert_num("=1 + 2", 3.0);
    }

    #[test]
    fn w1_07_subtraction() {
        assert_num("=10 - 3", 7.0);
    }

    #[test]
    fn w1_08_multiplication() {
        assert_num("=4 * 5", 20.0);
    }

    #[test]
    fn w1_09_division() {
        assert_num("=20 / 4", 5.0);
    }

    #[test]
    fn w1_10_exponentiation() {
        assert_num("=2 ^ 10", 1024.0);
    }

    #[test]
    fn w1_11_cell_arithmetic() {
        assert_num("=A1 + B1", 110.0);
    }

    #[test]
    fn w1_12_cell_subtraction() {
        assert_num("=B1 - A1", 90.0);
    }

    #[test]
    fn w1_13_complex_arithmetic() {
        assert_num("=A1 * 2 + B1 / 10", 30.0);
    }

    // --- Operator Precedence (5 tests) ---

    #[test]
    fn w1_14_mul_before_add() {
        assert_num("=2 + 3 * 4", 14.0);
    }

    #[test]
    fn w1_15_parens_override() {
        assert_num("=(2 + 3) * 4", 20.0);
    }

    #[test]
    fn w1_16_nested_parens() {
        assert_num("=((1 + 2) * (3 + 4))", 21.0);
    }

    #[test]
    fn w1_17_right_assoc_pow() {
        // 2^3^2 = 2^(3^2) = 2^9 = 512
        assert_num("=2^3^2", 512.0);
    }

    #[test]
    fn w1_18_unary_minus() {
        assert_num("=-5 + 10", 5.0);
    }

    // --- String & Comparison (3 tests) ---

    #[test]
    fn w1_19_string_concat() {
        assert_text("=C1 & \" \" & C2", "Hello World");
    }

    #[test]
    fn w1_20_comparison_gt() {
        assert_bool("=A1 > 5", true);
    }

    #[test]
    fn w1_21_comparison_eq() {
        assert_bool("=A1 = 10", true);
    }
}

// ===================================================================
// WEEK 2: Member access, error matrix, operator precedence (39 tests)
// ===================================================================

mod week2 {
    use super::*;

    // --- Dot notation (13 tests) ---

    #[test]
    fn w2_01_dot_price() {
        assert_num("=A1.Price", 99.99);
    }

    #[test]
    fn w2_02_dot_price_different_cell() {
        assert_num("=A2.Price", 149.99);
    }

    #[test]
    fn w2_03_dot_value_builtin() {
        // .value returns the cell value itself
        assert_num("=A1.value", 10.0);
    }

    #[test]
    fn w2_04_dot_note() {
        assert_text("=A1.note", "test note");
    }

    #[test]
    fn w2_05_dot_format() {
        assert_text("=A1.format", "currency");
    }

    #[test]
    fn w2_06_dot_style() {
        assert_text("=A1.style", "bold");
    }

    #[test]
    fn w2_07_dot_nonexistent() {
        assert_err("=A1.Nonexistent", SpreadsheetError::Field);
    }

    #[test]
    fn w2_08_dot_price_addition() {
        // A1.Price + A2.Price = 99.99 + 149.99 = 249.98
        assert_num("=A1.Price + A2.Price", 249.98);
    }

    #[test]
    fn w2_09_dot_price_multiplication() {
        assert_num("=A1.Price * 2", 199.98);
    }

    #[test]
    fn w2_10_dot_in_comparison() {
        assert_bool("=A1.Price > 50", true);
    }

    #[test]
    fn w2_11_dot_in_complex_expr() {
        assert_num("=(A1.Price + A2.Price) / 2", 124.99);
    }

    #[test]
    fn w2_12_dot_chained_arithmetic() {
        assert_num("=A1.Price - A2.Price + 100", 49.999999999999986);
    }

    #[test]
    fn w2_13_dot_with_cell_value() {
        // Mix dot member and regular cell ref
        assert_num("=A1.Price + A1", 109.99);
    }

    // --- Bracket notation (12 tests) ---

    #[test]
    fn w2_14_bracket_simple() {
        assert_num("=A1[\"Price\"]", 99.99);
    }

    #[test]
    fn w2_15_bracket_space_in_key() {
        assert_num("=A1[\"Market Cap\"]", 1e9);
    }

    #[test]
    fn w2_16_bracket_value() {
        assert_num("=A1[\"value\"]", 10.0);
    }

    #[test]
    fn w2_17_bracket_note() {
        assert_text("=A1[\"note\"]", "test note");
    }

    #[test]
    fn w2_18_bracket_nonexistent() {
        assert_err("=A1[\"xyz\"]", SpreadsheetError::Field);
    }

    #[test]
    fn w2_19_bracket_in_expression() {
        assert_num("=A1[\"Price\"] + A2[\"Price\"]", 249.98);
    }

    #[test]
    fn w2_20_bracket_multiplication() {
        assert_num("=A1[\"Price\"] * 3", 299.97);
    }

    #[test]
    fn w2_21_bracket_comparison() {
        assert_bool("=A1[\"Market Cap\"] > 500000000", true);
    }

    #[test]
    fn w2_22_bracket_mixed_with_dot() {
        assert_num("=A1.Price + A1[\"Market Cap\"]", 99.99 + 1e9);
    }

    #[test]
    fn w2_23_bracket_in_parens() {
        assert_num("=(A1[\"Price\"])", 99.99);
    }

    #[test]
    fn w2_24_bracket_division() {
        assert_num("=A1[\"Market Cap\"] / 1000000", 1000.0);
    }

    #[test]
    fn w2_25_bracket_with_cell_arithmetic() {
        assert_num("=A1[\"Price\"] + B1", 199.99);
    }

    // --- Error Matrix (9 tests) ---

    #[test]
    fn w2_26_error_value_text_plus_num() {
        assert_err("=\"text\" + 5", SpreadsheetError::Value);
    }

    #[test]
    fn w2_27_error_div_zero() {
        assert_err("=1/0", SpreadsheetError::DivZero);
    }

    #[test]
    fn w2_28_error_ref_propagation() {
        // E1 contains #REF!, so E1 + 5 → #REF!
        assert_err("=E1 + 5", SpreadsheetError::Ref);
    }

    #[test]
    fn w2_29_error_field_nonexistent() {
        assert_err("=A1.NonExistent", SpreadsheetError::Field);
    }

    #[test]
    fn w2_30_error_ref_propagates_to_member() {
        // E1 = #REF!, so E1.Price → #REF! (propagates)
        assert_err("=E1.Price", SpreadsheetError::Ref);
    }

    #[test]
    fn w2_31_error_ref_propagates_bracket() {
        assert_err("=E1[\"Price\"]", SpreadsheetError::Ref);
    }

    #[test]
    fn w2_32_error_ref_in_binary() {
        assert_err("=E1 * 2", SpreadsheetError::Ref);
    }

    #[test]
    fn w2_33_error_literal_value() {
        assert_err("=#VALUE!", SpreadsheetError::Value);
    }

    #[test]
    fn w2_34_error_literal_ref() {
        assert_err("=#REF!", SpreadsheetError::Ref);
    }

    // --- Operator Precedence with member access (5 tests) ---

    #[test]
    fn w2_35_neg_member() {
        // -A1.Price = -99.99
        assert_num("=-A1.Price", -99.99);
    }

    #[test]
    fn w2_36_neg_member_in_expr() {
        // -A1.Price + 200 = -99.99 + 200 = 100.01
        assert_num("=-A1.Price + 200", 100.01);
    }

    #[test]
    fn w2_37_member_before_mul() {
        // A1.Price * 2 = 199.98
        assert_num("=A1.Price * 2", 199.98);
    }

    #[test]
    fn w2_38_member_pow() {
        // A1.value ^ 2 = 10^2 = 100
        assert_num("=A1.value ^ 2", 100.0);
    }

    #[test]
    fn w2_39_not_member() {
        assert_bool("=NOT D1", false); // NOT TRUE = FALSE
    }
}

// ===================================================================
// WEEK 3: Functions, ranges, arrays, conditionals, lookups (67 tests)
// ===================================================================

mod week3_aggregation {
    use super::*;

    // --- SUM (5 tests) ---

    #[test]
    fn w3_01_sum_range() {
        // SUM(A1:A5) = 10+20+30+40+50 = 150
        assert_num("=SUM(A1:A5)", 150.0);
    }

    #[test]
    fn w3_02_sum_multiple_args() {
        assert_num("=SUM(A1, A2, A3)", 60.0);
    }

    #[test]
    fn w3_03_sum_mixed_range_and_value() {
        assert_num("=SUM(A1:A3, 100)", 160.0);
    }

    #[test]
    fn w3_04_sum_single_cell() {
        assert_num("=SUM(A1)", 10.0);
    }

    #[test]
    fn w3_05_sum_with_empty_cells() {
        // Z1:Z5 are all empty → SUM = 0
        assert_num("=SUM(A1:A5, 0)", 150.0);
    }

    // --- AVERAGE (4 tests) ---

    #[test]
    fn w3_06_average_range() {
        assert_num("=AVERAGE(A1:A5)", 30.0);
    }

    #[test]
    fn w3_07_average_args() {
        assert_num("=AVERAGE(10, 20, 30)", 20.0);
    }

    #[test]
    fn w3_08_average_single() {
        assert_num("=AVERAGE(42)", 42.0);
    }

    #[test]
    fn w3_09_average_mixed() {
        assert_num("=AVERAGE(A1:A3, 100)", 40.0);
    }

    // --- COUNT (3 tests) ---

    #[test]
    fn w3_10_count_range() {
        // A1:A5 all numbers → 5
        assert_num("=COUNT(A1:A5)", 5.0);
    }

    #[test]
    fn w3_11_count_mixed() {
        // A1 (num), C1 (text), D1 (bool) - COUNT counts numbers + booleans
        assert_num("=COUNT(A1, C1, D1)", 2.0);
    }

    #[test]
    fn w3_12_count_with_blanks() {
        assert_num("=COUNT(A1, A2, 5)", 3.0);
    }

    // --- MIN / MAX (3 tests) ---

    #[test]
    fn w3_13_min_range() {
        assert_num("=MIN(A1:A5)", 10.0);
    }

    #[test]
    fn w3_14_max_range() {
        assert_num("=MAX(A1:A5)", 50.0);
    }

    #[test]
    fn w3_15_min_max_args() {
        assert_num("=MAX(1, 5, 3, 2, 4)", 5.0);
    }
}

mod week3_range {
    use super::*;

    // --- Range Parsing and Evaluation (12 tests) ---

    #[test]
    fn w3_16_range_basic() {
        assert_num("=SUM(A1:A3)", 60.0);
    }

    #[test]
    fn w3_17_range_2d() {
        // SUM(A1:B2) = A1+A2+B1+B2 = 10+20+100+200 = 330
        assert_num("=SUM(A1:B2)", 330.0);
    }

    #[test]
    fn w3_18_range_single_row() {
        assert_num("=SUM(A1:B1)", 110.0);
    }

    #[test]
    fn w3_19_range_single_col() {
        assert_num("=SUM(A1:A1)", 10.0);
    }

    #[test]
    fn w3_20_range_in_average() {
        assert_num("=AVERAGE(B1:B5)", 300.0);
    }

    #[test]
    fn w3_21_range_in_count() {
        assert_num("=COUNT(B1:B5)", 5.0);
    }

    #[test]
    fn w3_22_range_in_min() {
        assert_num("=MIN(B1:B5)", 100.0);
    }

    #[test]
    fn w3_23_range_in_max() {
        assert_num("=MAX(B1:B5)", 500.0);
    }

    #[test]
    fn w3_24_range_sum_in_expr() {
        assert_num("=SUM(A1:A5) + SUM(B1:B5)", 1650.0);
    }

    #[test]
    fn w3_25_range_nested_function() {
        // MAX of a SUM result = just the sum
        assert_num("=MAX(SUM(A1:A5), 100)", 150.0);
    }

    #[test]
    fn w3_26_range_multicolumn_sum() {
        // SUM(A1:B5) = sum of all A1..A5 + B1..B5
        // = (10+20+30+40+50) + (100+200+300+400+500) = 150 + 1500 = 1650
        assert_num("=SUM(A1:B5)", 1650.0);
    }

    #[test]
    fn w3_27_range_reversed() {
        // B1:A1 should work the same as A1:B1
        assert_num("=SUM(B1:A1)", 110.0);
    }
}

mod week3_arrays {
    use super::*;

    // --- Array Literals (10 tests) ---

    #[test]
    fn w3_28_array_literal_row() {
        let result = eval("{1,2,3}");
        assert_eq!(
            result,
            Value::Array(vec![vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0),
            ]])
        );
    }

    #[test]
    fn w3_29_array_literal_2d() {
        let result = eval("{1,2;3,4}");
        assert_eq!(
            result,
            Value::Array(vec![
                vec![Value::Number(1.0), Value::Number(2.0)],
                vec![Value::Number(3.0), Value::Number(4.0)],
            ])
        );
    }

    #[test]
    fn w3_30_sum_array_literal() {
        assert_num("=SUM({1,2,3,4,5})", 15.0);
    }

    #[test]
    fn w3_31_average_array_literal() {
        assert_num("=AVERAGE({10,20,30})", 20.0);
    }

    #[test]
    fn w3_32_min_array_literal() {
        assert_num("=MIN({5,3,8,1,4})", 1.0);
    }

    #[test]
    fn w3_33_max_array_literal() {
        assert_num("=MAX({5,3,8,1,4})", 8.0);
    }

    #[test]
    fn w3_34_count_array_literal() {
        assert_num("=COUNT({1,2,3})", 3.0);
    }

    #[test]
    fn w3_35_array_2d_sum() {
        assert_num("=SUM({1,2,3;4,5,6})", 21.0);
    }

    #[test]
    fn w3_36_array_with_text() {
        // SUM ignores text in arrays
        let s = sample_sheet();
        let result = eval_formula("=SUM({1,2,3})", &s);
        assert_eq!(result, Value::Number(6.0));
    }

    #[test]
    fn w3_37_array_nested_in_function() {
        assert_num("=SUM({10,20}, {30,40})", 100.0);
    }

    // --- Array Operations (8 tests) ---

    #[test]
    fn w3_38_index_into_array() {
        assert_num("=INDEX({10,20,30}, 1, 2)", 20.0);
    }

    #[test]
    fn w3_39_index_2d() {
        assert_num("=INDEX({1,2,3;4,5,6;7,8,9}, 2, 3)", 6.0);
    }

    #[test]
    fn w3_40_index_row_0() {
        // INDEX with row=0 returns whole column
        let result = eval("=SUM(INDEX({1,2;3,4;5,6}, 0, 1))");
        assert_eq!(result, Value::Number(9.0)); // 1+3+5
    }

    #[test]
    fn w3_41_index_col_0() {
        // INDEX with col=0 returns whole row
        let result = eval("=SUM(INDEX({1,2,3;4,5,6}, 2, 0))");
        assert_eq!(result, Value::Number(15.0)); // 4+5+6
    }

    #[test]
    fn w3_42_index_out_of_bounds() {
        assert_err("=INDEX({1,2,3}, 1, 5)", SpreadsheetError::Ref);
    }

    #[test]
    fn w3_43_choose() {
        assert_num("=CHOOSE(2, 10, 20, 30)", 20.0);
    }

    #[test]
    fn w3_44_choose_first() {
        assert_num("=CHOOSE(1, 100, 200)", 100.0);
    }

    #[test]
    fn w3_45_choose_out_of_range() {
        assert_err("=CHOOSE(5, 1, 2, 3)", SpreadsheetError::Value);
    }
}

mod week3_conditional {
    use super::*;

    // --- IF (4 tests) ---

    #[test]
    fn w3_46_if_true() {
        assert_text("=IF(A1>5, \"big\", \"small\")", "big");
    }

    #[test]
    fn w3_47_if_false() {
        assert_text("=IF(A1>100, \"big\", \"small\")", "small");
    }

    #[test]
    fn w3_48_if_numeric() {
        assert_num("=IF(TRUE, 42, 0)", 42.0);
    }

    #[test]
    fn w3_49_if_nested() {
        assert_text("=IF(A1>50, \"high\", IF(A1>5, \"mid\", \"low\"))", "mid");
    }

    // --- AND/OR/NOT functions (6 tests) ---

    #[test]
    fn w3_50_and_true() {
        assert_bool("=AND(TRUE, TRUE, TRUE)", true);
    }

    #[test]
    fn w3_51_and_false() {
        assert_bool("=AND(TRUE, FALSE, TRUE)", false);
    }

    #[test]
    fn w3_52_or_true() {
        assert_bool("=OR(FALSE, TRUE, FALSE)", true);
    }

    #[test]
    fn w3_53_or_false() {
        assert_bool("=OR(FALSE, FALSE, FALSE)", false);
    }

    #[test]
    fn w3_54_not_true() {
        assert_bool("=NOT(TRUE)", false);
    }

    #[test]
    fn w3_55_not_false() {
        assert_bool("=NOT(FALSE)", true);
    }

    // --- IFERROR / IFNA / IFS / XOR (5 tests) ---

    #[test]
    fn w3_56_iferror_no_error() {
        assert_num("=IFERROR(A1 + 5, 0)", 15.0);
    }

    #[test]
    fn w3_57_iferror_with_error() {
        assert_num("=IFERROR(1/0, -1)", -1.0);
    }

    #[test]
    fn w3_58_ifs() {
        // IFS: first true condition wins
        assert_text("=IFS(A1>100, \"x\", A1>5, \"y\", TRUE, \"z\")", "y");
    }

    #[test]
    fn w3_59_xor_odd() {
        assert_bool("=XOR(TRUE, FALSE, TRUE)", false); // 2 trues = even
    }

    #[test]
    fn w3_60_xor_single() {
        assert_bool("=XOR(TRUE, FALSE)", true);
    }
}

mod week3_lookup {
    use super::*;

    // --- VLOOKUP (5 tests) ---

    #[test]
    fn w3_61_vlookup_exact() {
        // Look up "Bob" in F1:H5, return column 3 (salary)
        assert_num("=VLOOKUP(\"Bob\", F1:H5, 3, FALSE)", 80000.0);
    }

    #[test]
    fn w3_62_vlookup_exact_first() {
        assert_num("=VLOOKUP(\"Alice\", F1:H5, 3, FALSE)", 100000.0);
    }

    #[test]
    fn w3_63_vlookup_exact_last() {
        assert_num("=VLOOKUP(\"Eve\", F1:H5, 3, FALSE)", 85000.0);
    }

    #[test]
    fn w3_64_vlookup_not_found() {
        assert_err("=VLOOKUP(\"Frank\", F1:H5, 3, FALSE)", SpreadsheetError::NA);
    }

    #[test]
    fn w3_65_vlookup_col2() {
        // Return dept (column 2) for Charlie
        assert_text("=VLOOKUP(\"Charlie\", F1:H5, 2, FALSE)", "Eng");
    }

    // --- MATCH (4 tests) ---

    #[test]
    fn w3_66_match_exact() {
        // MATCH("Charlie", F1:F5, 0) → 3 (third position)
        assert_num("=MATCH(\"Charlie\", F1:F5, 0)", 3.0);
    }

    #[test]
    fn w3_67_match_exact_first() {
        assert_num("=MATCH(\"Alice\", F1:F5, 0)", 1.0);
    }

    #[test]
    fn w3_68_match_not_found() {
        assert_err("=MATCH(\"Zoe\", F1:F5, 0)", SpreadsheetError::NA);
    }

    #[test]
    fn w3_69_match_numeric() {
        // MATCH in numeric sorted array
        assert_num("=MATCH(300, B1:B5, 0)", 3.0);
    }

    // --- INDEX (3 tests) ---

    #[test]
    fn w3_70_index_from_range() {
        // INDEX(A1:B5, 2, 1) → A2 = 20
        assert_num("=INDEX(A1:B5, 2, 1)", 20.0);
    }

    #[test]
    fn w3_71_index_from_range_2() {
        // INDEX(A1:B5, 3, 2) → B3 = 300
        assert_num("=INDEX(A1:B5, 3, 2)", 300.0);
    }

    #[test]
    fn w3_72_index_match_combo() {
        // INDEX(H1:H5, MATCH("Diana", F1:F5, 0), 1)
        // MATCH → 4, INDEX(H1:H5, 4, 1) → H4 = 90000
        assert_num("=INDEX(H1:H5, MATCH(\"Diana\", F1:F5, 0), 1)", 90000.0);
    }
}

mod week3_math {
    use super::*;

    // --- Math functions (10 tests) ---

    #[test]
    fn w3_73_abs() {
        assert_num("=ABS(-42)", 42.0);
    }

    #[test]
    fn w3_74_round() {
        assert_num("=ROUND(3.14159, 2)", 3.14);
    }

    #[test]
    fn w3_75_roundup() {
        assert_num("=ROUNDUP(3.141, 2)", 3.15);
    }

    #[test]
    fn w3_76_rounddown() {
        assert_num("=ROUNDDOWN(3.149, 2)", 3.14);
    }

    #[test]
    fn w3_77_int() {
        assert_num("=INT(7.9)", 7.0);
    }

    #[test]
    fn w3_78_mod() {
        assert_num("=MOD(10, 3)", 1.0);
    }

    #[test]
    fn w3_79_power() {
        assert_num("=POWER(2, 8)", 256.0);
    }

    #[test]
    fn w3_80_sqrt() {
        assert_num("=SQRT(144)", 12.0);
    }

    #[test]
    fn w3_81_sqrt_negative() {
        assert_err("=SQRT(-1)", SpreadsheetError::Num);
    }

    #[test]
    fn w3_82_sign() {
        assert_num("=SIGN(-42)", -1.0);
    }
}

mod week3_text {
    use super::*;

    // --- Text functions (7 tests) ---

    #[test]
    fn w3_83_len() {
        assert_num("=LEN(\"Hello\")", 5.0);
    }

    #[test]
    fn w3_84_upper() {
        assert_text("=UPPER(\"hello\")", "HELLO");
    }

    #[test]
    fn w3_85_lower() {
        assert_text("=LOWER(\"WORLD\")", "world");
    }

    #[test]
    fn w3_86_left() {
        assert_text("=LEFT(\"Hello\", 3)", "Hel");
    }

    #[test]
    fn w3_87_right() {
        assert_text("=RIGHT(\"Hello\", 2)", "lo");
    }

    #[test]
    fn w3_88_mid() {
        assert_text("=MID(\"Hello World\", 7, 5)", "World");
    }

    #[test]
    fn w3_89_trim() {
        assert_text("=TRIM(\"  hello   world  \")", "hello world");
    }

    #[test]
    fn w3_90_concatenate() {
        assert_text("=CONCATENATE(\"Hello\", \" \", \"World\")", "Hello World");
    }

    #[test]
    fn w3_91_substitute() {
        assert_text("=SUBSTITUTE(\"Hello World\", \"World\", \"Rust\")", "Hello Rust");
    }

    #[test]
    fn w3_92_find() {
        assert_num("=FIND(\"World\", \"Hello World\")", 7.0);
    }

    #[test]
    fn w3_93_exact_true() {
        assert_bool("=EXACT(\"abc\", \"abc\")", true);
    }

    #[test]
    fn w3_94_exact_false() {
        assert_bool("=EXACT(\"abc\", \"ABC\")", false);
    }

    #[test]
    fn w3_95_rept() {
        assert_text("=REPT(\"ab\", 3)", "ababab");
    }
}

mod week3_info {
    use super::*;

    // --- Info functions (6 tests) ---

    #[test]
    fn w3_96_isblank_true() {
        assert_bool("=ISBLANK(Z99)", true);
    }

    #[test]
    fn w3_97_isblank_false() {
        assert_bool("=ISBLANK(A1)", false);
    }

    #[test]
    fn w3_98_iserror() {
        assert_bool("=ISERROR(E1)", true);
    }

    #[test]
    fn w3_99_iserror_false() {
        assert_bool("=ISERROR(A1)", false);
    }

    #[test]
    fn w3_100_isnumber() {
        assert_bool("=ISNUMBER(A1)", true);
    }

    #[test]
    fn w3_101_istext() {
        assert_bool("=ISTEXT(C1)", true);
    }
}

// ===================================================================
// Additional edge case tests to reach 127
// ===================================================================

mod edge_cases {
    use super::*;

    #[test]
    fn e_01_formula_without_equals() {
        assert_num("42", 42.0);
    }

    #[test]
    fn e_02_whitespace_handling() {
        assert_num("=  1  +  2  ", 3.0);
    }

    #[test]
    fn e_03_percentage() {
        assert_num("=50%", 0.5);
    }

    #[test]
    fn e_04_percentage_in_expr() {
        assert_num("=100 * 25%", 25.0);
    }

    #[test]
    fn e_05_scientific_notation() {
        assert_num("=1e3", 1000.0);
    }

    #[test]
    fn e_06_negative_number_direct() {
        assert_num("=-42", -42.0);
    }

    #[test]
    fn e_07_double_negative() {
        assert_num("=--5", 5.0);
    }

    #[test]
    fn e_08_string_equality() {
        assert_bool("=\"hello\" = \"HELLO\"", true); // case-insensitive
    }

    #[test]
    fn e_09_empty_equals_zero() {
        assert_bool("=Z99 = 0", true);
    }

    #[test]
    fn e_10_not_equal() {
        assert_bool("=1 <> 2", true);
    }

    #[test]
    fn e_11_lte() {
        assert_bool("=5 <= 5", true);
    }

    #[test]
    fn e_12_gte() {
        assert_bool("=5 >= 6", false);
    }

    #[test]
    fn e_13_bool_arithmetic() {
        // TRUE = 1 in numeric context
        assert_num("=TRUE + 1", 2.0);
    }

    #[test]
    fn e_14_division_precedence() {
        assert_num("=10 / 2 / 5", 1.0);
    }

    #[test]
    fn e_15_complex_formula() {
        // SUM(A1:A3) * 2 + MAX(B1:B3) = 60*2 + 300 = 420
        assert_num("=SUM(A1:A3) * 2 + MAX(B1:B3)", 420.0);
    }

    #[test]
    fn e_16_islogical() {
        assert_bool("=ISLOGICAL(TRUE)", true);
    }

    #[test]
    fn e_17_type_number() {
        assert_num("=TYPE(42)", 1.0);
    }

    #[test]
    fn e_18_type_text() {
        assert_num("=TYPE(\"hi\")", 2.0);
    }

    #[test]
    fn e_19_type_bool() {
        assert_num("=TYPE(TRUE)", 4.0);
    }

    #[test]
    fn e_20_counta() {
        // COUNTA counts all non-empty: A1(num), C1(text), D1(bool) = 3
        assert_num("=COUNTA(A1, C1, D1)", 3.0);
    }

    #[test]
    fn e_21_pi() {
        let result = eval("=PI()");
        match result {
            Value::Number(n) => assert!((n - std::f64::consts::PI).abs() < 1e-10),
            other => panic!("expected PI, got {other:?}"),
        }
    }

    #[test]
    fn e_22_ln() {
        let result = eval("=LN(1)");
        assert_eq!(result, Value::Number(0.0));
    }

    #[test]
    fn e_23_exp() {
        let result = eval("=EXP(0)");
        assert_eq!(result, Value::Number(1.0));
    }

    #[test]
    fn e_24_log10() {
        assert_num("=LOG10(1000)", 3.0);
    }

    #[test]
    fn e_25_ceiling() {
        assert_num("=CEILING(4.3, 1)", 5.0);
    }

    #[test]
    fn e_26_floor() {
        assert_num("=FLOOR(4.7, 1)", 4.0);
    }
}

// ===================================================================
// Parser-specific unit tests (imported from parser module)
// ===================================================================

mod parser_extras {
    use super::*;

    #[test]
    fn parse_empty_formula() {
        let expr = parse_formula("=").unwrap();
        assert_eq!(expr, Expression::Literal(Value::Empty));
    }

    #[test]
    fn parse_abs_cell_ref() {
        let expr = parse_formula("=$A$1").unwrap();
        match expr {
            Expression::CellReference(c) => {
                assert_eq!(c.col, 0);
                assert_eq!(c.row, 0);
                assert!(c.abs_col);
                assert!(c.abs_row);
            }
            other => panic!("expected CellReference, got {other:?}"),
        }
    }

    #[test]
    fn parse_concat_operator() {
        let expr = parse_formula("=\"a\" & \"b\"").unwrap();
        assert!(matches!(expr, Expression::BinaryOp(BinaryOp::Concat, _, _)));
    }
}

// ===========================================================================
// Week 5: Design binding integration tests
// ===========================================================================

mod week5_design_binding {
    use std::collections::HashMap;

    use logos_spreadsheet::binding::resolver::{PropertyResolver, PropertyInfo, PropertyType, ElementInfo};
    use logos_spreadsheet::binding::types::{DesignRef, ElementKind, ElementRef, PropertyPath};
    use logos_spreadsheet::errors::SpreadsheetError;
    use logos_spreadsheet::evaluator::{Evaluator, Spreadsheet};
    use logos_spreadsheet::recalc::RecalcEngine;
    use logos_spreadsheet::types::*;

    // -----------------------------------------------------------------------
    // Test resolver
    // -----------------------------------------------------------------------

    /// A mock design resolver for integration testing.
    struct TestResolver {
        elements: HashMap<String, HashMap<String, Value>>,
    }

    impl TestResolver {
        fn new() -> Self {
            let mut r = Self {
                elements: HashMap::new(),
            };
            // Set up test elements
            let mut rect1 = HashMap::new();
            rect1.insert("x".into(), Value::Number(50.0));
            rect1.insert("y".into(), Value::Number(100.0));
            rect1.insert("width".into(), Value::Number(200.0));
            rect1.insert("height".into(), Value::Number(150.0));
            rect1.insert("opacity".into(), Value::Number(1.0));
            rect1.insert("visible".into(), Value::Boolean(true));
            r.elements.insert("rect-1".into(), rect1);

            let mut header = HashMap::new();
            header.insert("x".into(), Value::Number(0.0));
            header.insert("y".into(), Value::Number(0.0));
            header.insert("width".into(), Value::Number(800.0));
            header.insert("height".into(), Value::Number(60.0));
            header.insert("text".into(), Value::Text("Welcome".into()));
            header.insert("opacity".into(), Value::Number(0.8));
            r.elements.insert("header".into(), header);

            r
        }

        #[allow(dead_code)]
        fn set_property(&mut self, element: &str, property: &str, value: Value) {
            self.elements
                .entry(element.into())
                .or_default()
                .insert(property.into(), value);
        }
    }

    impl PropertyResolver for TestResolver {
        fn resolve_element(&self, name: &str, _kind: ElementKind) -> Option<DesignRef> {
            if self.elements.contains_key(name) {
                Some(DesignRef::new(ElementRef::named(name), _kind))
            } else {
                None
            }
        }

        fn get_property(&self, element: &ElementRef, path: &PropertyPath) -> Value {
            let key = element.key();
            match self.elements.get(key) {
                Some(props) => props
                    .get(path.root())
                    .cloned()
                    .unwrap_or(Value::Error(SpreadsheetError::Field)),
                None => Value::Error(SpreadsheetError::Ref),
            }
        }

        fn set_property(
            &self,
            _element: &ElementRef,
            _path: &PropertyPath,
            _value: &Value,
        ) -> bool {
            false // read-only for tests
        }

        fn list_properties(&self, element: &ElementRef) -> Vec<PropertyInfo> {
            match self.elements.get(element.key()) {
                Some(props) => props
                    .keys()
                    .map(|k| PropertyInfo::new(k.clone(), PropertyType::Number, false))
                    .collect(),
                None => Vec::new(),
            }
        }

        fn list_elements(&self, _kind: ElementKind) -> Vec<ElementInfo> {
            self.elements
                .keys()
                .map(|k| ElementInfo::new(k.clone(), ElementKind::Layer))
                .collect()
        }
    }

    // -----------------------------------------------------------------------
    // LAYER function tests
    // -----------------------------------------------------------------------

    #[test]
    fn w5_01_layer_returns_design_ref() {
        let sheet = Spreadsheet::new(10, 10);
        let resolver = TestResolver::new();
        let evaluator = Evaluator::with_resolver(&sheet, &resolver);
        let expr = logos_spreadsheet::parse_formula("=LAYER(\"rect-1\")").unwrap();
        let val = evaluator.eval(&expr);
        assert!(val.is_design_ref());
    }

    #[test]
    fn w5_02_layer_width_property() {
        let sheet = Spreadsheet::new(10, 10);
        let resolver = TestResolver::new();
        let evaluator = Evaluator::with_resolver(&sheet, &resolver);
        let expr = logos_spreadsheet::parse_formula("=LAYER(\"rect-1\").width").unwrap();
        let val = evaluator.eval(&expr);
        assert_eq!(val, Value::Number(200.0));
    }

    #[test]
    fn w5_03_layer_x_property() {
        let sheet = Spreadsheet::new(10, 10);
        let resolver = TestResolver::new();
        let evaluator = Evaluator::with_resolver(&sheet, &resolver);
        let expr = logos_spreadsheet::parse_formula("=LAYER(\"rect-1\").x").unwrap();
        let val = evaluator.eval(&expr);
        assert_eq!(val, Value::Number(50.0));
    }

    #[test]
    fn w5_04_layer_nonexistent() {
        let sheet = Spreadsheet::new(10, 10);
        let resolver = TestResolver::new();
        let evaluator = Evaluator::with_resolver(&sheet, &resolver);
        let expr = logos_spreadsheet::parse_formula("=LAYER(\"nope\").width").unwrap();
        let val = evaluator.eval(&expr);
        assert!(val.is_error());
    }

    #[test]
    fn w5_05_layer_invalid_property() {
        let sheet = Spreadsheet::new(10, 10);
        let resolver = TestResolver::new();
        let evaluator = Evaluator::with_resolver(&sheet, &resolver);
        let expr = logos_spreadsheet::parse_formula("=LAYER(\"rect-1\").banana").unwrap();
        let val = evaluator.eval(&expr);
        assert_eq!(val, Value::Error(SpreadsheetError::Field));
    }

    #[test]
    fn w5_06_element_function() {
        let sheet = Spreadsheet::new(10, 10);
        let resolver = TestResolver::new();
        let evaluator = Evaluator::with_resolver(&sheet, &resolver);
        let expr = logos_spreadsheet::parse_formula("=ELEMENT(\"header\").width").unwrap();
        let val = evaluator.eval(&expr);
        assert_eq!(val, Value::Number(800.0));
    }

    #[test]
    fn w5_07_layer_no_args() {
        let sheet = Spreadsheet::new(10, 10);
        let resolver = TestResolver::new();
        let evaluator = Evaluator::with_resolver(&sheet, &resolver);
        let expr = logos_spreadsheet::parse_formula("=LAYER()").unwrap();
        let val = evaluator.eval(&expr);
        assert_eq!(val, Value::Error(SpreadsheetError::Value));
    }

    #[test]
    fn w5_08_layer_numeric_arg() {
        let sheet = Spreadsheet::new(10, 10);
        let resolver = TestResolver::new();
        let evaluator = Evaluator::with_resolver(&sheet, &resolver);
        let expr = logos_spreadsheet::parse_formula("=LAYER(42)").unwrap();
        let val = evaluator.eval(&expr);
        assert_eq!(val, Value::Error(SpreadsheetError::Value));
    }

    // -----------------------------------------------------------------------
    // Arithmetic with design properties
    // -----------------------------------------------------------------------

    #[test]
    fn w5_09_layer_property_add() {
        let sheet = Spreadsheet::new(10, 10);
        let resolver = TestResolver::new();
        let evaluator = Evaluator::with_resolver(&sheet, &resolver);
        let expr =
            logos_spreadsheet::parse_formula("=LAYER(\"rect-1\").width + 50").unwrap();
        let val = evaluator.eval(&expr);
        assert_eq!(val, Value::Number(250.0));
    }

    #[test]
    fn w5_10_layer_property_multiply() {
        let sheet = Spreadsheet::new(10, 10);
        let resolver = TestResolver::new();
        let evaluator = Evaluator::with_resolver(&sheet, &resolver);
        let expr =
            logos_spreadsheet::parse_formula("=LAYER(\"rect-1\").width * 2").unwrap();
        let val = evaluator.eval(&expr);
        assert_eq!(val, Value::Number(400.0));
    }

    #[test]
    fn w5_11_layer_two_properties() {
        let sheet = Spreadsheet::new(10, 10);
        let resolver = TestResolver::new();
        let evaluator = Evaluator::with_resolver(&sheet, &resolver);
        let expr = logos_spreadsheet::parse_formula(
            "=LAYER(\"rect-1\").width + LAYER(\"rect-1\").height",
        )
        .unwrap();
        let val = evaluator.eval(&expr);
        assert_eq!(val, Value::Number(350.0)); // 200 + 150
    }

    #[test]
    fn w5_12_cross_element_arithmetic() {
        let sheet = Spreadsheet::new(10, 10);
        let resolver = TestResolver::new();
        let evaluator = Evaluator::with_resolver(&sheet, &resolver);
        let expr = logos_spreadsheet::parse_formula(
            "=LAYER(\"rect-1\").width + ELEMENT(\"header\").width",
        )
        .unwrap();
        let val = evaluator.eval(&expr);
        assert_eq!(val, Value::Number(1000.0)); // 200 + 800
    }

    #[test]
    fn w5_13_mixed_cell_and_design() {
        let mut sheet = Spreadsheet::new(10, 10);
        sheet.set_cell(0, 0, Value::Number(10.0)); // A1 = 10
        let resolver = TestResolver::new();
        let evaluator = Evaluator::with_resolver(&sheet, &resolver);
        let expr =
            logos_spreadsheet::parse_formula("=A1 + LAYER(\"rect-1\").x").unwrap();
        let val = evaluator.eval(&expr);
        assert_eq!(val, Value::Number(60.0)); // 10 + 50
    }

    #[test]
    fn w5_14_design_ref_in_if() {
        let sheet = Spreadsheet::new(10, 10);
        let resolver = TestResolver::new();
        let evaluator = Evaluator::with_resolver(&sheet, &resolver);
        let expr = logos_spreadsheet::parse_formula(
            "=IF(LAYER(\"rect-1\").visible, LAYER(\"rect-1\").width, 0)",
        )
        .unwrap();
        let val = evaluator.eval(&expr);
        assert_eq!(val, Value::Number(200.0));
    }

    #[test]
    fn w5_15_design_ref_comparison() {
        let sheet = Spreadsheet::new(10, 10);
        let resolver = TestResolver::new();
        let evaluator = Evaluator::with_resolver(&sheet, &resolver);
        let expr =
            logos_spreadsheet::parse_formula("=LAYER(\"rect-1\").width > 100").unwrap();
        let val = evaluator.eval(&expr);
        assert_eq!(val, Value::Boolean(true));
    }

    #[test]
    fn w5_16_design_text_property() {
        let sheet = Spreadsheet::new(10, 10);
        let resolver = TestResolver::new();
        let evaluator = Evaluator::with_resolver(&sheet, &resolver);
        let expr =
            logos_spreadsheet::parse_formula("=ELEMENT(\"header\").text").unwrap();
        let val = evaluator.eval(&expr);
        assert_eq!(val, Value::Text("Welcome".into()));
    }

    #[test]
    fn w5_17_without_resolver_bare_ref() {
        // Without resolver, LAYER returns a ref optimistically
        let sheet = Spreadsheet::new(10, 10);
        let evaluator = Evaluator::new(&sheet);
        let expr = logos_spreadsheet::parse_formula("=LAYER(\"rect-1\")").unwrap();
        let val = evaluator.eval(&expr);
        assert!(val.is_design_ref());
    }

    #[test]
    fn w5_18_without_resolver_member_access() {
        // Without resolver, member access on DesignRef returns #VALUE!
        let sheet = Spreadsheet::new(10, 10);
        let evaluator = Evaluator::new(&sheet);
        let expr = logos_spreadsheet::parse_formula("=LAYER(\"rect-1\").width").unwrap();
        let val = evaluator.eval(&expr);
        assert_eq!(val, Value::Error(SpreadsheetError::Value));
    }

    #[test]
    fn w5_19_layer_bracket_access() {
        let sheet = Spreadsheet::new(10, 10);
        let resolver = TestResolver::new();
        let evaluator = Evaluator::with_resolver(&sheet, &resolver);
        let expr =
            logos_spreadsheet::parse_formula("=LAYER(\"rect-1\")[\"height\"]").unwrap();
        let val = evaluator.eval(&expr);
        assert_eq!(val, Value::Number(150.0));
    }

    #[test]
    fn w5_20_design_ref_display() {
        let design_ref = DesignRef::layer("rect-1");
        let val = Value::DesignRef(design_ref);
        assert_eq!(format!("{}", val), "[LAYER(\"rect-1\")]");
    }

    // -----------------------------------------------------------------------
    // RecalcEngine design integration
    // -----------------------------------------------------------------------

    #[test]
    fn w5_21_recalc_design_deps_tracked() {
        let mut engine = RecalcEngine::new(10, 10);
        engine.set_formula(0, 0, "=LAYER(\"rect-1\").width");
        let deps = engine.get_design_deps(0, 0);
        assert!(!deps.is_empty());
        assert!(deps.iter().any(|d| d.element.key() == "rect-1"));
    }

    #[test]
    fn w5_22_recalc_no_design_deps_for_normal() {
        let mut engine = RecalcEngine::new(10, 10);
        engine.set_formula(0, 0, "=42 + 1");
        let deps = engine.get_design_deps(0, 0);
        assert!(deps.is_empty());
    }

    #[test]
    fn w5_23_cells_depending_on_element() {
        let mut engine = RecalcEngine::new(10, 10);
        engine.set_formula(0, 0, "=LAYER(\"rect-1\").width");
        engine.set_formula(1, 0, "=LAYER(\"rect-1\").height");
        engine.set_formula(2, 0, "=42"); // no design dep
        let cells = engine.cells_depending_on_element("rect-1");
        assert_eq!(cells.len(), 2);
        assert!(cells.contains(&(0, 0)));
        assert!(cells.contains(&(1, 0)));
    }

    #[test]
    fn w5_24_design_deps_cleared_on_formula_change() {
        let mut engine = RecalcEngine::new(10, 10);
        engine.set_formula(0, 0, "=LAYER(\"rect-1\").width");
        assert!(!engine.get_design_deps(0, 0).is_empty());

        // Change formula to a normal one
        engine.set_formula(0, 0, "=42");
        assert!(engine.get_design_deps(0, 0).is_empty());
        assert!(engine.cells_depending_on_element("rect-1").is_empty());
    }

    #[test]
    fn w5_25_design_deps_cleared_on_set_value() {
        let mut engine = RecalcEngine::new(10, 10);
        engine.set_formula(0, 0, "=LAYER(\"rect-1\").width");
        assert!(!engine.get_design_deps(0, 0).is_empty());

        engine.set_value(0, 0, Value::Number(42.0));
        assert!(engine.get_design_deps(0, 0).is_empty());
    }

    #[test]
    fn w5_26_notify_design_change_marks_dirty() {
        let mut engine = RecalcEngine::new(10, 10);
        engine.set_formula(0, 0, "=LAYER(\"rect-1\").width");
        engine.set_formula(1, 0, "=42");

        let dirty = engine.notify_design_change("rect-1", Some("width"));
        // Cell (0,0) depends on rect-1 width → should be dirtied
        assert!(dirty.contains(&(0, 0)));
    }

    #[test]
    fn w5_27_notify_design_change_any_property() {
        let mut engine = RecalcEngine::new(10, 10);
        engine.set_formula(0, 0, "=LAYER(\"rect-1\").width");

        // Notify change without specifying property → should still match
        let dirty = engine.notify_design_change("rect-1", None);
        assert!(dirty.contains(&(0, 0)));
    }

    #[test]
    fn w5_28_notify_unrelated_element() {
        let mut engine = RecalcEngine::new(10, 10);
        engine.set_formula(0, 0, "=LAYER(\"rect-1\").width");

        let dirty = engine.notify_design_change("rect-2", Some("width"));
        assert!(dirty.is_empty());
    }

    #[test]
    fn w5_29_multiple_elements_notify() {
        let mut engine = RecalcEngine::new(10, 10);
        engine.set_formula(0, 0, "=LAYER(\"rect-1\").width");
        engine.set_formula(1, 0, "=ELEMENT(\"header\").height");

        // Only notify rect-1 change
        let dirty = engine.notify_design_change("rect-1", Some("width"));
        assert!(dirty.contains(&(0, 0)));
        // header cell should NOT be dirtied
        assert!(!dirty.contains(&(1, 0)));
    }

    #[test]
    fn w5_30_type_function_design_ref() {
        let sheet = Spreadsheet::new(10, 10);
        let evaluator = Evaluator::new(&sheet);
        let expr = logos_spreadsheet::parse_formula("=TYPE(LAYER(\"a\"))").unwrap();
        let val = evaluator.eval(&expr);
        assert_eq!(val, Value::Number(128.0)); // custom type code
    }
}

// ===========================================================================
// Phase 5.4: Collaborative Editing
// ===========================================================================

mod collaborative_editing {
    use logos_spreadsheet::collab::{
        ApplyResult, CellOp, CellPayload, CollabEngine, LamportClock, OpTimestamp,
        PeerPresence, SiteId,
    };
    use logos_spreadsheet::RecalcEngine;

    fn site(id: u64) -> SiteId {
        SiteId::new(id)
    }

    /// Helper: apply a CellOp to a RecalcEngine based on its payload.
    fn apply_to_engine(engine: &mut RecalcEngine, result: &ApplyResult) {
        match result {
            ApplyResult::Applied { col, row, payload } => match payload {
                CellPayload::Number(n) => {
                    engine.set_value(*col, *row, logos_spreadsheet::Value::Number(*n));
                }
                CellPayload::Text(s) => {
                    engine.set_value(
                        *col,
                        *row,
                        logos_spreadsheet::Value::Text(s.clone()),
                    );
                }
                CellPayload::Boolean(b) => {
                    engine.set_value(*col, *row, logos_spreadsheet::Value::Boolean(*b));
                }
                CellPayload::Formula(f) => {
                    engine.set_formula(*col, *row, f);
                }
                CellPayload::Clear => {
                    engine.clear_cell(*col, *row);
                }
            },
            _ => {}
        }
    }

    // --- End-to-end: collab + recalc ---

    #[test]
    fn e2e_01_local_edit_recalc() {
        let mut collab = CollabEngine::new(site(1), "Alice");
        let mut engine = RecalcEngine::new(10, 10);

        // Alice sets A1 = 42
        let op = collab.local_set_value(0, 0, CellPayload::Number(42.0));
        engine.set_value(op.col, op.row, logos_spreadsheet::Value::Number(42.0));

        assert_eq!(
            engine.get_value(0, 0),
            logos_spreadsheet::Value::Number(42.0)
        );
    }

    #[test]
    fn e2e_02_remote_edit_applied_to_engine() {
        let mut alice_collab = CollabEngine::new(site(1), "Alice");
        let mut alice_engine = RecalcEngine::new(10, 10);

        let mut bob_collab = CollabEngine::new(site(2), "Bob");

        // Bob sets A1 = 99
        let op = bob_collab.local_set_value(0, 0, CellPayload::Number(99.0));

        // Alice receives Bob's op
        let result = alice_collab.apply_remote_op(&op);
        apply_to_engine(&mut alice_engine, &result);

        assert_eq!(
            alice_engine.get_value(0, 0),
            logos_spreadsheet::Value::Number(99.0)
        );
    }

    #[test]
    fn e2e_03_formula_sync() {
        let mut alice_collab = CollabEngine::new(site(1), "Alice");
        let mut alice_engine = RecalcEngine::new(10, 10);

        let mut bob_collab = CollabEngine::new(site(2), "Bob");
        let mut bob_engine = RecalcEngine::new(10, 10);

        // Alice sets A1 = 10
        let op_a1 = alice_collab.local_set_value(0, 0, CellPayload::Number(10.0));
        alice_engine.set_value(0, 0, logos_spreadsheet::Value::Number(10.0));

        // Alice sets A2 = 20
        let op_a2 = alice_collab.local_set_value(0, 1, CellPayload::Number(20.0));
        alice_engine.set_value(0, 1, logos_spreadsheet::Value::Number(20.0));

        // Alice sets A3 = =A1+A2
        let op_a3 = alice_collab.local_set_formula(0, 2, "=A1+A2");
        alice_engine.set_formula(0, 2, "=A1+A2");

        // Bob receives all ops
        let r1 = bob_collab.apply_remote_op(&op_a1);
        apply_to_engine(&mut bob_engine, &r1);
        let r2 = bob_collab.apply_remote_op(&op_a2);
        apply_to_engine(&mut bob_engine, &r2);
        let r3 = bob_collab.apply_remote_op(&op_a3);
        apply_to_engine(&mut bob_engine, &r3);

        // Both should compute A3 = 30
        assert_eq!(
            alice_engine.get_value(0, 2),
            logos_spreadsheet::Value::Number(30.0)
        );
        assert_eq!(
            bob_engine.get_value(0, 2),
            logos_spreadsheet::Value::Number(30.0)
        );
    }

    #[test]
    fn e2e_04_concurrent_different_cells() {
        let mut alice_collab = CollabEngine::new(site(1), "Alice");
        let mut alice_engine = RecalcEngine::new(10, 10);

        let mut bob_collab = CollabEngine::new(site(2), "Bob");
        let mut bob_engine = RecalcEngine::new(10, 10);

        // Concurrent: Alice edits A1, Bob edits B1
        let op_a = alice_collab.local_set_value(0, 0, CellPayload::Number(100.0));
        alice_engine.set_value(0, 0, logos_spreadsheet::Value::Number(100.0));

        let op_b = bob_collab.local_set_value(1, 0, CellPayload::Number(200.0));
        bob_engine.set_value(1, 0, logos_spreadsheet::Value::Number(200.0));

        // Exchange
        let r_b = alice_collab.apply_remote_op(&op_b);
        apply_to_engine(&mut alice_engine, &r_b);

        let r_a = bob_collab.apply_remote_op(&op_a);
        apply_to_engine(&mut bob_engine, &r_a);

        // Both see both values
        assert_eq!(
            alice_engine.get_value(0, 0),
            logos_spreadsheet::Value::Number(100.0)
        );
        assert_eq!(
            alice_engine.get_value(1, 0),
            logos_spreadsheet::Value::Number(200.0)
        );
        assert_eq!(
            bob_engine.get_value(0, 0),
            logos_spreadsheet::Value::Number(100.0)
        );
        assert_eq!(
            bob_engine.get_value(1, 0),
            logos_spreadsheet::Value::Number(200.0)
        );
    }

    #[test]
    fn e2e_05_concurrent_same_cell_lww_convergence() {
        let mut alice_collab = CollabEngine::new(site(1), "Alice");
        let mut alice_engine = RecalcEngine::new(10, 10);

        let mut bob_collab = CollabEngine::new(site(2), "Bob");
        let mut bob_engine = RecalcEngine::new(10, 10);

        // Both edit A1 concurrently
        let op_a = alice_collab.local_set_value(0, 0, CellPayload::Number(111.0));
        alice_engine.set_value(0, 0, logos_spreadsheet::Value::Number(111.0));

        let op_b = bob_collab.local_set_value(0, 0, CellPayload::Number(222.0));
        bob_engine.set_value(0, 0, logos_spreadsheet::Value::Number(222.0));

        // Exchange — LWW resolves deterministically
        let r_b = alice_collab.apply_remote_op(&op_b);
        apply_to_engine(&mut alice_engine, &r_b);

        let r_a = bob_collab.apply_remote_op(&op_a);
        apply_to_engine(&mut bob_engine, &r_a);

        // Both should converge — Bob wins (same clock, higher site_id)
        let alice_val = alice_engine.get_value(0, 0);
        let bob_val = bob_engine.get_value(0, 0);
        assert_eq!(alice_val, bob_val);
        assert_eq!(alice_val, logos_spreadsheet::Value::Number(222.0));
    }

    #[test]
    fn e2e_06_formula_with_remote_dependency() {
        let mut alice_collab = CollabEngine::new(site(1), "Alice");
        let mut alice_engine = RecalcEngine::new(10, 10);

        let mut bob_collab = CollabEngine::new(site(2), "Bob");
        let mut bob_engine = RecalcEngine::new(10, 10);

        // Alice sets B1 = =A1*2
        let op_formula = alice_collab.local_set_formula(1, 0, "=A1*2");
        alice_engine.set_formula(1, 0, "=A1*2");

        // Bob receives formula
        let r = bob_collab.apply_remote_op(&op_formula);
        apply_to_engine(&mut bob_engine, &r);

        // Bob sets A1 = 5
        let op_val = bob_collab.local_set_value(0, 0, CellPayload::Number(5.0));
        bob_engine.set_value(0, 0, logos_spreadsheet::Value::Number(5.0));

        // Alice receives A1 = 5
        let r2 = alice_collab.apply_remote_op(&op_val);
        apply_to_engine(&mut alice_engine, &r2);

        // Both: A1=5, B1=10
        assert_eq!(
            alice_engine.get_value(1, 0),
            logos_spreadsheet::Value::Number(10.0)
        );
        assert_eq!(
            bob_engine.get_value(1, 0),
            logos_spreadsheet::Value::Number(10.0)
        );
    }

    #[test]
    fn e2e_07_clear_cell_syncs() {
        let mut alice_collab = CollabEngine::new(site(1), "Alice");
        let mut alice_engine = RecalcEngine::new(10, 10);

        let mut bob_collab = CollabEngine::new(site(2), "Bob");
        let mut bob_engine = RecalcEngine::new(10, 10);

        // Alice sets A1 = 42
        let op1 = alice_collab.local_set_value(0, 0, CellPayload::Number(42.0));
        alice_engine.set_value(0, 0, logos_spreadsheet::Value::Number(42.0));
        let r1 = bob_collab.apply_remote_op(&op1);
        apply_to_engine(&mut bob_engine, &r1);

        // Bob clears A1
        let op_clear = bob_collab.local_clear(0, 0);
        bob_engine.clear_cell(0, 0);

        let r2 = alice_collab.apply_remote_op(&op_clear);
        apply_to_engine(&mut alice_engine, &r2);

        assert_eq!(alice_engine.get_value(0, 0), logos_spreadsheet::Value::Empty);
        assert_eq!(bob_engine.get_value(0, 0), logos_spreadsheet::Value::Empty);
    }

    #[test]
    fn e2e_08_stale_remote_ignored() {
        let mut collab = CollabEngine::new(site(1), "Alice");
        let mut engine = RecalcEngine::new(10, 10);

        // Local edit at clock=1
        let _local = collab.local_set_value(0, 0, CellPayload::Number(100.0));
        engine.set_value(0, 0, logos_spreadsheet::Value::Number(100.0));

        // Stale remote at clock=0
        let stale = CellOp::new(
            0, 0,
            CellPayload::Number(1.0),
            OpTimestamp::new(LamportClock::new(0), site(2)),
        );
        let result = collab.apply_remote_op(&stale);
        assert_eq!(result, ApplyResult::Discarded);

        // Engine unchanged
        assert_eq!(
            engine.get_value(0, 0),
            logos_spreadsheet::Value::Number(100.0)
        );
    }

    #[test]
    fn e2e_09_delta_batch_sync() {
        let mut alice_collab = CollabEngine::new(site(1), "Alice");
        let mut bob_collab = CollabEngine::new(site(2), "Bob");
        let mut bob_engine = RecalcEngine::new(10, 10);

        // Alice makes 5 edits
        for i in 0..5 {
            alice_collab.local_set_value(i, 0, CellPayload::Number(i as f64 * 10.0));
        }

        // Build delta and apply to Bob
        let batch = alice_collab.build_delta(LamportClock::new(0));
        assert_eq!(batch.len(), 5);

        let results = bob_collab.apply_op_batch(&batch);
        for result in &results {
            apply_to_engine(&mut bob_engine, result);
        }

        // Bob should have all 5 values
        for i in 0..5 {
            assert_eq!(
                bob_engine.get_value(i, 0),
                logos_spreadsheet::Value::Number(i as f64 * 10.0)
            );
        }
    }

    #[test]
    fn e2e_10_three_peer_convergence() {
        let mut alice = CollabEngine::new(site(1), "Alice");
        let mut bob = CollabEngine::new(site(2), "Bob");
        let mut carol = CollabEngine::new(site(3), "Carol");

        let mut alice_eng = RecalcEngine::new(10, 10);
        let mut bob_eng = RecalcEngine::new(10, 10);
        let mut carol_eng = RecalcEngine::new(10, 10);

        // Each edits a different cell
        let op_a = alice.local_set_value(0, 0, CellPayload::Number(1.0));
        alice_eng.set_value(0, 0, logos_spreadsheet::Value::Number(1.0));

        let op_b = bob.local_set_value(1, 0, CellPayload::Number(2.0));
        bob_eng.set_value(1, 0, logos_spreadsheet::Value::Number(2.0));

        let op_c = carol.local_set_value(2, 0, CellPayload::Number(3.0));
        carol_eng.set_value(2, 0, logos_spreadsheet::Value::Number(3.0));

        // Full exchange
        for (collab, eng) in [(&mut bob, &mut bob_eng), (&mut carol, &mut carol_eng)] {
            let r = collab.apply_remote_op(&op_a);
            apply_to_engine(eng, &r);
        }
        for (collab, eng) in [(&mut alice, &mut alice_eng), (&mut carol, &mut carol_eng)] {
            let r = collab.apply_remote_op(&op_b);
            apply_to_engine(eng, &r);
        }
        for (collab, eng) in [(&mut alice, &mut alice_eng), (&mut bob, &mut bob_eng)] {
            let r = collab.apply_remote_op(&op_c);
            apply_to_engine(eng, &r);
        }

        // All three converge
        for col in 0..3 {
            let expected = logos_spreadsheet::Value::Number((col + 1) as f64);
            assert_eq!(alice_eng.get_value(col, 0), expected);
            assert_eq!(bob_eng.get_value(col, 0), expected);
            assert_eq!(carol_eng.get_value(col, 0), expected);
        }
    }

    #[test]
    fn e2e_11_presence_cursors() {
        let mut alice = CollabEngine::new(site(1), "Alice");
        alice.start_session("Test");

        let mut bob_presence = PeerPresence::new(site(2), "Bob");
        bob_presence.set_cursor(5, 10);
        alice.peer_joined(bob_presence);

        let cursors = alice.remote_cursors();
        assert_eq!(cursors.len(), 1);
        assert_eq!(cursors[0].cursor, (5, 10));
        assert_eq!(cursors[0].name, "Bob");
    }

    #[test]
    fn e2e_12_presence_editing_indicator() {
        let mut alice = CollabEngine::new(site(1), "Alice");
        let mut bob_presence = PeerPresence::new(site(2), "Bob");
        bob_presence.set_cursor(2, 3);
        bob_presence.set_editing(true);
        alice.peer_joined(bob_presence);

        assert!(alice
            .presence()
            .is_cell_being_edited_by_remote(2, 3)
            .is_some());
        assert!(alice
            .presence()
            .is_cell_being_edited_by_remote(0, 0)
            .is_none());
    }

    #[test]
    fn e2e_13_presence_peer_leave() {
        let mut alice = CollabEngine::new(site(1), "Alice");
        let bob = PeerPresence::new(site(2), "Bob");
        alice.peer_joined(bob);
        assert_eq!(alice.session_info().peer_count, 2);

        alice.peer_left(site(2));
        assert_eq!(alice.session_info().peer_count, 1);
    }

    #[test]
    fn e2e_14_undo_local_edit() {
        let mut collab = CollabEngine::new(site(1), "Alice");
        let mut engine = RecalcEngine::new(10, 10);

        // Set A1 = 10
        collab.local_set_value(0, 0, CellPayload::Number(10.0));
        engine.set_value(0, 0, logos_spreadsheet::Value::Number(10.0));

        // Set A1 = 20
        collab.local_set_value(0, 0, CellPayload::Number(20.0));
        engine.set_value(0, 0, logos_spreadsheet::Value::Number(20.0));

        // Undo → should restore 10
        let undo_op = collab.undo().unwrap();
        assert_eq!(undo_op.payload, CellPayload::Number(10.0));

        // Apply undo to engine
        engine.set_value(0, 0, logos_spreadsheet::Value::Number(10.0));
        assert_eq!(
            engine.get_value(0, 0),
            logos_spreadsheet::Value::Number(10.0)
        );
    }

    #[test]
    fn e2e_15_session_info() {
        let mut alice = CollabEngine::new(site(1), "Alice");
        alice.start_session("Budget Q3");

        let bob = PeerPresence::new(site(2), "Bob");
        let carol = PeerPresence::new(site(3), "Carol");
        alice.peer_joined(bob);
        alice.peer_joined(carol);

        let info = alice.session_info();
        assert_eq!(info.name, "Budget Q3");
        assert_eq!(info.peer_count, 3);
    }

    #[test]
    fn e2e_16_text_value_sync() {
        let mut alice = CollabEngine::new(site(1), "Alice");
        let mut bob = CollabEngine::new(site(2), "Bob");
        let mut bob_engine = RecalcEngine::new(10, 10);

        let op = alice.local_set_value(0, 0, CellPayload::Text("Hello".into()));
        let result = bob.apply_remote_op(&op);
        apply_to_engine(&mut bob_engine, &result);

        assert_eq!(
            bob_engine.get_value(0, 0),
            logos_spreadsheet::Value::Text("Hello".into())
        );
    }

    #[test]
    fn e2e_17_boolean_value_sync() {
        let mut alice = CollabEngine::new(site(1), "Alice");
        let mut bob = CollabEngine::new(site(2), "Bob");
        let mut bob_engine = RecalcEngine::new(10, 10);

        let op = alice.local_set_value(0, 0, CellPayload::Boolean(true));
        let result = bob.apply_remote_op(&op);
        apply_to_engine(&mut bob_engine, &result);

        assert_eq!(
            bob_engine.get_value(0, 0),
            logos_spreadsheet::Value::Boolean(true)
        );
    }

    #[test]
    fn e2e_18_rapid_edits_same_cell() {
        let mut alice = CollabEngine::new(site(1), "Alice");
        let mut bob = CollabEngine::new(site(2), "Bob");
        let mut bob_engine = RecalcEngine::new(10, 10);

        // Alice rapidly edits A1 ten times
        let mut last_op = None;
        for i in 0..10 {
            let op = alice.local_set_value(0, 0, CellPayload::Number(i as f64));
            last_op = Some(op);
        }

        // Bob receives only the last op (simulating network batching)
        let result = bob.apply_remote_op(last_op.as_ref().unwrap());
        apply_to_engine(&mut bob_engine, &result);

        assert_eq!(
            bob_engine.get_value(0, 0),
            logos_spreadsheet::Value::Number(9.0)
        );
    }

    #[test]
    fn e2e_19_presence_selection_range() {
        let mut alice = CollabEngine::new(site(1), "Alice");
        alice.set_selection(1, 2, 5, 8);

        let presence = alice.local_presence();
        assert_eq!(presence.selection, Some((1, 2, 5, 8)));
        assert_eq!(presence.cursor, (1, 2));
    }

    #[test]
    fn e2e_20_stats_tracking() {
        let mut alice = CollabEngine::new(site(1), "Alice");
        let mut bob = CollabEngine::new(site(2), "Bob");

        // 3 local ops
        alice.local_set_value(0, 0, CellPayload::Number(1.0));
        alice.local_set_value(1, 0, CellPayload::Number(2.0));
        alice.local_set_value(2, 0, CellPayload::Number(3.0));
        assert_eq!(alice.stats().local_ops, 3);

        // Send all to bob
        let batch = alice.build_delta(LamportClock::new(0));
        bob.apply_op_batch(&batch);
        assert_eq!(bob.stats().remote_ops, 3);
        assert_eq!(bob.stats().remote_applied, 3);
    }
}

// ===========================================================================
// Phase 5.5: Charting
// ===========================================================================

mod charting {
    use logos_spreadsheet::chart::{
        CategorySource, ChartEngine,
        ChartSpec, ChartStyle, ChartTheme, DataResolver, DataSeries,
        StackMode, compute_layout, render_chart, palette_color,
    };
    use logos_spreadsheet::{RecalcEngine, Value};

    // --- Data resolution with RecalcEngine ---

    #[test]
    fn chart_01_bar_from_recalc_engine() {
        let mut engine = RecalcEngine::new(10, 10);
        engine.set_value(0, 0, Value::Number(10.0));
        engine.set_value(0, 1, Value::Number(20.0));
        engine.set_value(0, 2, Value::Number(30.0));

        let spec = ChartSpec::bar(1, "Revenue", (0, 0, 0, 2));
        let resolved = DataResolver::resolve(&spec, engine.sheet());

        assert_eq!(resolved.series[0].values, vec![Some(10.0), Some(20.0), Some(30.0)]);
        assert_eq!(resolved.data_max, 30.0);
    }

    #[test]
    fn chart_02_formula_values_charted() {
        let mut engine = RecalcEngine::new(10, 10);
        engine.set_value(0, 0, Value::Number(10.0));
        engine.set_value(0, 1, Value::Number(20.0));
        engine.set_formula(0, 2, "=A1+A2"); // A3 = 30

        let spec = ChartSpec::bar(1, "Computed", (0, 0, 0, 2));
        let resolved = DataResolver::resolve(&spec, engine.sheet());

        assert_eq!(resolved.series[0].values[2], Some(30.0));
    }

    #[test]
    fn chart_03_line_chart_layout() {
        let mut engine = RecalcEngine::new(10, 10);
        for i in 0..5 {
            engine.set_value(0, i, Value::Number((i as f64 + 1.0) * 10.0));
        }

        let spec = ChartSpec::line(1, "Trend", (0, 0, 0, 4));
        let resolved = DataResolver::resolve(&spec, engine.sheet());
        let style = ChartStyle::default();
        let layout = compute_layout(&resolved, &style, 0.0, 0.0, 400.0, 300.0);

        assert_eq!(layout.points.len(), 5);
        assert_eq!(layout.lines.len(), 4); // 5 points → 4 segments
    }

    #[test]
    fn chart_04_pie_chart_percentages() {
        let mut engine = RecalcEngine::new(10, 10);
        engine.set_value(0, 0, Value::Number(25.0));
        engine.set_value(0, 1, Value::Number(25.0));
        engine.set_value(0, 2, Value::Number(50.0));

        let spec = ChartSpec::pie(1, "Market", (0, 0, 0, 2));
        let resolved = DataResolver::resolve(&spec, engine.sheet());
        let style = ChartStyle::default();
        let layout = compute_layout(&resolved, &style, 0.0, 0.0, 400.0, 300.0);

        assert_eq!(layout.slices.len(), 3);
        assert!((layout.slices[0].percent - 25.0).abs() < 0.1);
        assert!((layout.slices[2].percent - 50.0).abs() < 0.1);
    }

    #[test]
    fn chart_05_multi_series_bar() {
        let mut engine = RecalcEngine::new(10, 10);
        // Series 1 (A column)
        engine.set_value(0, 0, Value::Number(10.0));
        engine.set_value(0, 1, Value::Number(20.0));
        // Series 2 (B column)
        engine.set_value(1, 0, Value::Number(15.0));
        engine.set_value(1, 1, Value::Number(25.0));

        let spec = ChartSpec::bar(1, "Q1", (0, 0, 0, 1))
            .with_series(DataSeries::new("Q2", (1, 0, 1, 1)));
        let resolved = DataResolver::resolve(&spec, engine.sheet());

        assert_eq!(resolved.series.len(), 2);
        assert_eq!(resolved.series[0].label, "Q1");
        assert_eq!(resolved.series[1].label, "Q2");

        let style = ChartStyle::default();
        let layout = compute_layout(&resolved, &style, 0.0, 0.0, 400.0, 300.0);
        assert_eq!(layout.bars.len(), 4); // 2 series × 2 categories
    }

    // --- ChartEngine CRUD + rebuild ---

    #[test]
    fn chart_06_engine_add_rebuild() {
        let mut chart_engine = ChartEngine::new();
        let mut recalc = RecalcEngine::new(10, 10);
        recalc.set_value(0, 0, Value::Number(42.0));

        let id = chart_engine.add_bar("Test", (0, 0, 0, 0));
        assert!(chart_engine.has_dirty());

        chart_engine.rebuild(recalc.sheet());
        assert!(!chart_engine.has_dirty());

        let rd = chart_engine.render_data(id).unwrap();
        assert!(!rd.is_empty());
    }

    #[test]
    fn chart_07_engine_cell_change_triggers_dirty() {
        let mut chart_engine = ChartEngine::new();
        let mut recalc = RecalcEngine::new(10, 10);
        recalc.set_value(0, 0, Value::Number(10.0));

        chart_engine.add_bar("Watch", (0, 0, 0, 2));
        chart_engine.rebuild(recalc.sheet());
        assert!(!chart_engine.has_dirty());

        // Simulate a cell change in A2
        let mut changed = std::collections::HashSet::new();
        changed.insert((0, 1));
        chart_engine.notify_cell_changes(&changed);
        assert!(chart_engine.has_dirty());
    }

    #[test]
    fn chart_08_engine_remove_chart() {
        let mut engine = ChartEngine::new();
        let id = engine.add_bar("Temp", (0, 0, 0, 2));
        assert_eq!(engine.chart_count(), 1);
        engine.remove_chart(id);
        assert_eq!(engine.chart_count(), 0);
    }

    #[test]
    fn chart_09_engine_update_chart() {
        let mut engine = ChartEngine::new();
        let id = engine.add_bar("Old Title", (0, 0, 0, 2));
        let recalc = RecalcEngine::new(10, 10);
        engine.rebuild(recalc.sheet());

        let mut spec = engine.get_spec(id).unwrap().clone();
        spec.title = Some("New Title".into());
        engine.update_chart(spec);
        assert!(engine.has_dirty());
    }

    // --- Render pipeline end-to-end ---

    #[test]
    fn chart_10_full_render_pipeline() {
        let mut recalc = RecalcEngine::new(10, 10);
        recalc.set_value(0, 0, Value::Number(100.0));
        recalc.set_value(0, 1, Value::Number(200.0));
        recalc.set_value(0, 2, Value::Number(150.0));

        let spec = ChartSpec::bar(1, "Sales", (0, 0, 0, 2))
            .with_title("Quarterly Sales")
            .with_position(50.0, 50.0)
            .with_size(400.0, 300.0);

        let resolved = DataResolver::resolve(&spec, recalc.sheet());
        let style = ChartStyle::default();
        let layout = compute_layout(&resolved, &style, 50.0, 50.0, 400.0, 300.0);
        let rd = render_chart(&layout);

        // Should have rects (bg + plot + bars) and texts (title, axis labels, legend)
        assert!(rd.rects.len() >= 5);
        assert!(rd.texts.iter().any(|t| t.text == "Quarterly Sales"));
    }

    #[test]
    fn chart_11_scatter_chart() {
        let mut recalc = RecalcEngine::new(10, 10);
        for i in 0..5 {
            recalc.set_value(0, i, Value::Number(i as f64 * i as f64));
        }

        let spec = ChartSpec::scatter(1, "XY", (0, 0, 0, 4));
        let resolved = DataResolver::resolve(&spec, recalc.sheet());
        let style = ChartStyle::default();
        let layout = compute_layout(&resolved, &style, 0.0, 0.0, 400.0, 300.0);

        assert_eq!(layout.points.len(), 5);
        assert!(layout.lines.is_empty()); // scatter: no connecting lines
    }

    #[test]
    fn chart_12_area_chart() {
        let mut recalc = RecalcEngine::new(10, 10);
        recalc.set_value(0, 0, Value::Number(5.0));
        recalc.set_value(0, 1, Value::Number(15.0));
        recalc.set_value(0, 2, Value::Number(10.0));

        let spec = ChartSpec::area(1, "Fill", (0, 0, 0, 2));
        let resolved = DataResolver::resolve(&spec, recalc.sheet());
        let style = ChartStyle::default();
        let layout = compute_layout(&resolved, &style, 0.0, 0.0, 400.0, 300.0);

        assert_eq!(layout.area_fills.len(), 1);
        assert_eq!(layout.points.len(), 3);
    }

    #[test]
    fn chart_13_stacked_bar() {
        let mut recalc = RecalcEngine::new(10, 10);
        recalc.set_value(0, 0, Value::Number(10.0));
        recalc.set_value(0, 1, Value::Number(20.0));
        recalc.set_value(1, 0, Value::Number(5.0));
        recalc.set_value(1, 1, Value::Number(15.0));

        let spec = ChartSpec::bar(1, "S1", (0, 0, 0, 1))
            .with_series(DataSeries::new("S2", (1, 0, 1, 1)))
            .with_stack(StackMode::Stacked);
        let resolved = DataResolver::resolve(&spec, recalc.sheet());

        // Stacked max = 10+5=15 or 20+15=35
        assert_eq!(resolved.data_max, 35.0);
    }

    #[test]
    fn chart_14_category_labels_from_cells() {
        let mut recalc = RecalcEngine::new(10, 10);
        // Data in A
        recalc.set_value(0, 0, Value::Number(10.0));
        recalc.set_value(0, 1, Value::Number(20.0));
        // Labels in B
        recalc.set_value(1, 0, Value::Text("Q1".into()));
        recalc.set_value(1, 1, Value::Text("Q2".into()));

        let spec = ChartSpec::bar(1, "Rev", (0, 0, 0, 1))
            .with_categories(CategorySource::Range(1, 0, 1, 1));
        let resolved = DataResolver::resolve(&spec, recalc.sheet());

        assert_eq!(resolved.categories, vec!["Q1", "Q2"]);
    }

    #[test]
    fn chart_15_donut_chart() {
        let mut recalc = RecalcEngine::new(10, 10);
        recalc.set_value(0, 0, Value::Number(60.0));
        recalc.set_value(0, 1, Value::Number(40.0));

        let spec = ChartSpec::pie(1, "Donut", (0, 0, 0, 1));
        let resolved = DataResolver::resolve(&spec, recalc.sheet());
        let style = ChartStyle::default().with_donut_hole(0.5);
        let layout = compute_layout(&resolved, &style, 0.0, 0.0, 400.0, 300.0);

        assert!(layout.slices[0].inner_radius > 0.0);
    }

    #[test]
    fn chart_16_custom_theme() {
        let style = ChartStyle::default().with_theme(ChartTheme::HighContrast);
        let colors = style.series_colors(2, &[None, None]);
        // High contrast starts with black
        assert_eq!(colors[0].r, 0);
        assert_eq!(colors[0].g, 0);
        assert_eq!(colors[0].b, 0);
    }

    #[test]
    fn chart_17_data_labels() {
        let mut recalc = RecalcEngine::new(10, 10);
        recalc.set_value(0, 0, Value::Number(42.0));

        let spec = ChartSpec::bar(1, "X", (0, 0, 0, 0));
        let resolved = DataResolver::resolve(&spec, recalc.sheet());
        let style = ChartStyle::default().with_data_labels(true);
        let layout = compute_layout(&resolved, &style, 0.0, 0.0, 400.0, 300.0);
        let rd = render_chart(&layout);

        assert!(rd.texts.iter().any(|t| t.text == "42"));
    }

    #[test]
    fn chart_18_engine_multiple_charts() {
        let mut chart_engine = ChartEngine::new();
        let mut recalc = RecalcEngine::new(10, 10);
        recalc.set_value(0, 0, Value::Number(10.0));
        recalc.set_value(0, 1, Value::Number(20.0));

        chart_engine.add_bar("Bar1", (0, 0, 0, 1));
        chart_engine.add_line("Line1", (0, 0, 0, 1));
        chart_engine.add_pie("Pie1", (0, 0, 0, 1));

        assert_eq!(chart_engine.chart_count(), 3);
        let rebuilt = chart_engine.rebuild(recalc.sheet());
        assert_eq!(rebuilt, 3);

        let all = chart_engine.all_render_data();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn chart_19_recalc_triggers_chart_update() {
        let mut chart_engine = ChartEngine::new();
        let mut recalc = RecalcEngine::new(10, 10);
        recalc.set_value(0, 0, Value::Number(10.0));
        recalc.set_value(0, 1, Value::Number(20.0));
        recalc.set_formula(0, 2, "=SUM(A1:A2)");

        let id = chart_engine.add_bar("Sum Chart", (0, 0, 0, 2));
        chart_engine.rebuild(recalc.sheet());

        // Verify initial chart data
        let spec = chart_engine.get_spec(id).unwrap();
        let resolved = DataResolver::resolve(spec, recalc.sheet());
        assert_eq!(resolved.series[0].values[2], Some(30.0));

        // Change A1 → triggers recalc
        recalc.set_value(0, 0, Value::Number(100.0));
        // A3 = SUM(A1:A2) = 100 + 20 = 120

        let mut changed = std::collections::HashSet::new();
        changed.insert((0, 0));
        changed.insert((0, 2)); // A3 recalced
        chart_engine.notify_cell_changes(&changed);
        chart_engine.rebuild(recalc.sheet());

        let spec = chart_engine.get_spec(id).unwrap();
        let resolved = DataResolver::resolve(spec, recalc.sheet());
        assert_eq!(resolved.series[0].values[0], Some(100.0));
        assert_eq!(resolved.series[0].values[2], Some(120.0));
    }

    #[test]
    fn chart_20_palette_colors() {
        // Verify palette provides distinguishable colors
        let c0 = palette_color(0);
        let c1 = palette_color(1);
        assert_ne!(c0, c1);
        // Wraps around
        assert_eq!(palette_color(0), palette_color(8));
    }
}
