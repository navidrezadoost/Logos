// Phase 4 – DataSourceTable Integration Tests (t431–t445)
//
// Tests for `logos_layout::data_table::DataSourceTable` and its integration
// with `RepeatGrid`.

use logos_layout::data_table::DataSourceTable;
use logos_layout::repeat_grid::{DataSource, RepeatGrid};
use serde_json::json;

// ── Helpers ────────────────────────────────────────────────────────────────────

fn text_col(name: &str, vals: &[&str]) -> DataSource {
    DataSource::new(name, "label.text", vals.iter().map(|v| json!(*v)).collect())
}

fn num_col(name: &str, vals: &[i64]) -> DataSource {
    DataSource::new(name, "price.text", vals.iter().map(|v| json!(*v)).collect())
}

fn table_with_two_cols() -> DataSourceTable {
    let mut t = DataSourceTable::new("products");
    t.add_column(text_col("title", &["Apple", "Banana", "Cherry"]));
    t.add_column(num_col("price", &[100, 200, 300]));
    t
}

// ── §1 Construction & column operations ──────────────────────────────────────

#[test]
fn t431_new_table_has_name() {
    let t = DataSourceTable::new("my_table");
    assert_eq!(t.name, "my_table");
}

#[test]
fn t432_add_two_columns_gives_count_two() {
    let t = table_with_two_cols();
    assert_eq!(t.column_count(), 2);
}

#[test]
fn t433_column_lookup_by_name() {
    let t = table_with_two_cols();
    let col = t.column("title").unwrap();
    assert_eq!(col.column, "label.text");
}

#[test]
fn t434_column_lookup_missing_returns_none() {
    let t = DataSourceTable::new("t");
    assert!(t.column("ghost").is_none());
}

#[test]
fn t435_remove_existing_column() {
    let mut t = table_with_two_cols();
    let removed = t.remove_column("price");
    assert!(removed.is_some());
    assert_eq!(t.column_count(), 1);
    assert!(t.column("price").is_none());
}

#[test]
fn t436_replace_column_with_same_name() {
    let mut t = DataSourceTable::new("t");
    t.add_column(text_col("title", &["A"]));
    t.add_column(text_col("title", &["X", "Y", "Z"]));
    assert_eq!(t.column_count(), 1);
    assert_eq!(t.column("title").unwrap().len(), 3);
}

// ── §2 Row / cell access ──────────────────────────────────────────────────────

#[test]
fn t437_row_count_is_max_column_length() {
    let t = table_with_two_cols();
    assert_eq!(t.row_count(), 3); // both cols have 3 values
}

#[test]
fn t438_row_count_zero_for_empty_table() {
    let t = DataSourceTable::new("t");
    assert_eq!(t.row_count(), 0);
}

#[test]
fn t439_header_row_lists_all_column_names() {
    let t = table_with_two_cols();
    let headers = t.header_row();
    assert!(headers.contains(&"title"));
    assert!(headers.contains(&"price"));
}

#[test]
fn t440_data_row_returns_values_for_each_column() {
    let t = table_with_two_cols();
    let row = t.data_row(1);
    assert_eq!(row.len(), 2);
    // Row 1: "Banana" and 200
    let title_val = row.iter().find(|(col, _)| *col == "title").map(|(_, v)| *v);
    assert_eq!(title_val, Some(&json!("Banana")));
}

#[test]
fn t441_cell_value_returns_correct() {
    let t = table_with_two_cols();
    assert_eq!(t.cell_value(0, "title"), Some(&json!("Apple")));
    assert_eq!(t.cell_value(2, "price"), Some(&json!(300)));
}

#[test]
fn t442_cell_value_cycles_for_short_column() {
    let mut t = DataSourceTable::new("t");
    t.add_column(text_col("x", &["A", "B"]));
    // Row 2 should cycle: 2 % 2 = 0 → "A"
    assert_eq!(t.cell_value(2, "x"), Some(&json!("A")));
    // Row 3 should cycle: 3 % 2 = 1 → "B"
    assert_eq!(t.cell_value(3, "x"), Some(&json!("B")));
}

// ── §3 Grid integration ───────────────────────────────────────────────────────

#[test]
fn t443_apply_to_grid_attaches_sources() {
    let t = table_with_two_cols();
    let mut grid = RepeatGrid::new(3, 2, 100.0, 80.0);
    assert_eq!(grid.source_count(), 0);
    t.apply_to_grid(&mut grid);
    assert_eq!(grid.source_count(), 2);
}

#[test]
fn t444_detach_from_grid_removes_all_columns() {
    let t = table_with_two_cols();
    let mut grid = RepeatGrid::new(3, 2, 100.0, 80.0);
    t.apply_to_grid(&mut grid);
    t.detach_from_grid(&mut grid);
    assert_eq!(grid.source_count(), 0);
}

#[test]
fn t445_apply_then_auto_fill_populates_overrides() {
    let t = table_with_two_cols();
    let mut grid = RepeatGrid::new(3, 2, 100.0, 80.0); // 6 cells
    t.apply_to_grid(&mut grid);
    // auto_fill should generate overrides for each cell × source
    grid.auto_fill();
    // 6 cells × 2 sources = 12 overrides
    assert_eq!(grid.data_overrides.len(), 12,
        "Expected 12 override entries (6 cells × 2 sources), got {}",
        grid.data_overrides.len()
    );
}
