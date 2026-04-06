//! Phase 0 — RepeatGrid Integration Tests
//!
//! §3 RepeatGrid (t051–t075, 25 tests)

use logos_layout::repeat_grid::{DataOverride, RepeatGrid};
use uuid::Uuid;

#[test]
fn t051_new_grid_dimensions() {
    let g = RepeatGrid::new(4, 5, 60.0, 40.0);
    assert_eq!(g.rows, 4);
    assert_eq!(g.columns, 5);
}

#[test]
fn t052_total_cells() {
    let g = RepeatGrid::new(3, 4, 100.0, 80.0);
    assert_eq!(g.total_cells(), 12);
}

#[test]
fn t053_cell_bounds_origin() {
    let g = RepeatGrid::new(3, 4, 100.0, 80.0);
    assert_eq!(g.cell_bounds(0, 0).unwrap(), (0.0, 0.0, 100.0, 80.0));
}

#[test]
fn t054_cell_bounds_with_gap() {
    let g = RepeatGrid::new(3, 4, 100.0, 80.0).with_gap(10.0, 5.0);
    let b = g.cell_bounds(2, 3).unwrap();
    // x = 3*(100+10)=330, y = 2*(80+5)=170
    assert_eq!(b, (330.0, 170.0, 100.0, 80.0));
}

#[test]
fn t055_cell_bounds_out_of_range() {
    let g = RepeatGrid::new(2, 2, 50.0, 50.0);
    assert!(g.cell_bounds(2, 0).is_none());
    assert!(g.cell_bounds(0, 2).is_none());
}

#[test]
fn t056_total_width_no_gap() {
    let g = RepeatGrid::new(2, 3, 100.0, 50.0);
    assert_eq!(g.total_width(), 300.0);
}

#[test]
fn t057_total_height_with_gap() {
    let g = RepeatGrid::new(3, 2, 100.0, 50.0).with_gap(0.0, 8.0);
    // 3*50 + 2*8 = 166
    assert_eq!(g.total_height(), 166.0);
}

#[test]
fn t058_add_template_layer_no_dups() {
    let mut g = RepeatGrid::new(2, 2, 50.0, 50.0);
    let id = Uuid::new_v4();
    g.add_template_layer(id);
    g.add_template_layer(id);
    assert_eq!(g.template_count(), 1);
}

#[test]
fn t059_remove_template_layer() {
    let mut g = RepeatGrid::new(2, 2, 50.0, 50.0);
    let id = Uuid::new_v4();
    g.add_template_layer(id);
    g.remove_template_layer(&id);
    assert_eq!(g.template_count(), 0);
}

#[test]
fn t060_set_data_override_insert() {
    let mut g = RepeatGrid::new(3, 3, 80.0, 60.0);
    g.set_data_override(1, 2, "text", serde_json::json!("Hello"));
    assert!(g.get_data_override(1, 2, "text").is_some());
}

#[test]
fn t061_set_data_override_replaces() {
    let mut g = RepeatGrid::new(3, 3, 80.0, 60.0);
    g.set_data_override(0, 0, "fill", serde_json::json!("red"));
    g.set_data_override(0, 0, "fill", serde_json::json!("green"));
    assert_eq!(g.data_overrides.len(), 1);
    assert_eq!(
        g.get_data_override(0, 0, "fill").unwrap().value,
        serde_json::json!("green")
    );
}

#[test]
fn t062_get_data_override_none() {
    let g = RepeatGrid::new(2, 2, 50.0, 50.0);
    assert!(g.get_data_override(0, 0, "ghost").is_none());
}

#[test]
fn t063_remove_data_override_found() {
    let mut g = RepeatGrid::new(2, 2, 50.0, 50.0);
    g.set_data_override(0, 0, "x", serde_json::json!(1));
    assert!(g.remove_data_override(0, 0, "x"));
}

#[test]
fn t064_remove_data_override_not_found() {
    let mut g = RepeatGrid::new(2, 2, 50.0, 50.0);
    assert!(!g.remove_data_override(0, 0, "ghost"));
}

#[test]
fn t065_clear_all_overrides() {
    let mut g = RepeatGrid::new(3, 3, 80.0, 60.0);
    for i in 0..5u32 {
        g.set_data_override(i % 3, i % 3, "v", serde_json::json!(i));
    }
    g.clear_all_overrides();
    assert!(g.data_overrides.is_empty());
}

#[test]
fn t066_add_row() {
    let mut g = RepeatGrid::new(2, 4, 60.0, 40.0);
    g.add_row();
    assert_eq!(g.rows, 3);
}

#[test]
fn t067_add_column() {
    let mut g = RepeatGrid::new(2, 4, 60.0, 40.0);
    g.add_column();
    assert_eq!(g.columns, 5);
}

#[test]
fn t068_remove_last_row_clears_overrides() {
    let mut g = RepeatGrid::new(3, 3, 60.0, 40.0);
    g.set_data_override(2, 0, "v", serde_json::json!("gone"));
    g.set_data_override(0, 0, "v", serde_json::json!("stay"));
    g.remove_last_row();
    assert_eq!(g.rows, 2);
    assert_eq!(g.data_overrides.len(), 1);
}

#[test]
fn t069_remove_last_row_minimum_one() {
    let mut g = RepeatGrid::new(1, 2, 60.0, 40.0);
    g.remove_last_row();
    assert_eq!(g.rows, 1);
}

#[test]
fn t070_remove_last_column_minimum_one() {
    let mut g = RepeatGrid::new(2, 1, 60.0, 40.0);
    g.remove_last_column();
    assert_eq!(g.columns, 1);
}

#[test]
fn t071_cell_bounds_absolute_with_origin() {
    let g = RepeatGrid::new(2, 2, 100.0, 80.0)
        .with_gap(10.0, 5.0)
        .with_origin(30.0, 20.0);
    let abs = g.cell_bounds_absolute(1, 1).unwrap();
    // local: x=110, y=85; absolute: 140, 105
    assert_eq!(abs, (140.0, 105.0, 100.0, 80.0));
}

#[test]
fn t072_data_override_cell_index() {
    let o = DataOverride::new(3, 2, "p", serde_json::json!(0));
    assert_eq!(o.cell_index(5), 17); // 3*5 + 2
}

#[test]
fn t073_grid_serde_roundtrip() {
    let mut g = RepeatGrid::new(4, 4, 80.0, 60.0).with_gap(5.0, 5.0);
    g.set_data_override(1, 1, "label", serde_json::json!("test"));
    let json = serde_json::to_string(&g).unwrap();
    let back: RepeatGrid = serde_json::from_str(&json).unwrap();
    assert_eq!(back.rows, 4);
    assert_eq!(back.col_gap, 5.0);
    assert_eq!(back.data_overrides.len(), 1);
}

#[test]
fn t074_overrides_for_cell() {
    let mut g = RepeatGrid::new(3, 3, 60.0, 40.0);
    g.set_data_override(0, 0, "a", serde_json::json!(1));
    g.set_data_override(0, 0, "b", serde_json::json!(2));
    g.set_data_override(1, 1, "c", serde_json::json!(3));
    assert_eq!(g.overrides_for_cell(0, 0).len(), 2);
    assert_eq!(g.overrides_for_cell(1, 1).len(), 1);
}

#[test]
fn t075_total_width_with_gap() {
    let g = RepeatGrid::new(1, 3, 100.0, 50.0).with_gap(16.0, 0.0);
    // 3*100 + 2*16 = 332
    assert_eq!(g.total_width(), 332.0);
}
