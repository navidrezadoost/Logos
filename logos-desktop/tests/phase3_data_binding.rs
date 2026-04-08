// Phase 3 – Repeat Grid Data Binding Tests (t351–t365)
//
// Tests for `logos_layout::repeat_grid::DataSource` and
// `RepeatGrid`'s data-binding API, plus logo-desktop command integration.

use logos_desktop::commands::{command_to_id, Command, CommandRegistry};
use logos_layout::repeat_grid::{DataSource, RepeatGrid};
use serde_json::json;
use uuid::Uuid;

// ── Helpers ────────────────────────────────────────────────────────────────────

fn grid(rows: u32, cols: u32) -> RepeatGrid {
    RepeatGrid::new(rows, cols, 100.0, 80.0)
}

fn text_source(name: &str, values: &[&str]) -> DataSource {
    DataSource::new(
        name,
        "label.text",
        values.iter().map(|v| json!(*v)).collect(),
    )
}

// ── §1 DataSource fundamentals ────────────────────────────────────────────────

#[test]
fn t351_data_source_new_fields_correct() {
    let src = DataSource::new(
        "titles",
        "heading.text",
        vec![json!("A"), json!("B"), json!("C")],
    );
    assert_eq!(src.name, "titles");
    assert_eq!(src.column, "heading.text");
    assert_eq!(src.len(), 3);
}

#[test]
fn t352_value_for_index_cycles_through_values() {
    let src = DataSource::new("s", "k", vec![json!(1), json!(2), json!(3)]);
    assert_eq!(src.value_for_index(0), &json!(1));
    assert_eq!(src.value_for_index(1), &json!(2));
    assert_eq!(src.value_for_index(2), &json!(3));
    // Cycles:
    assert_eq!(src.value_for_index(3), &json!(1));
    assert_eq!(src.value_for_index(5), &json!(3));
    assert_eq!(src.value_for_index(7), &json!(2));
}

#[test]
fn t353_value_for_index_empty_source_returns_null() {
    let src = DataSource::new("empty", "k", vec![]);
    assert_eq!(src.value_for_index(0), &json!(null));
    assert_eq!(src.value_for_index(99), &json!(null));
}

#[test]
fn t354_data_source_serde_roundtrip() {
    let src = DataSource::new(
        "colors",
        "bg.fill",
        vec![json!("#ff0000"), json!("#00ff00"), json!("#0000ff")],
    );
    let json_str = serde_json::to_string(&src).unwrap();
    let back: DataSource = serde_json::from_str(&json_str).unwrap();
    assert_eq!(back.name, "colors");
    assert_eq!(back.column, "bg.fill");
    assert_eq!(back.len(), 3);
    assert_eq!(back.value_for_index(1), &json!("#00ff00"));
}

// ── §2 attach / detach / source_count ─────────────────────────────────────────

#[test]
fn t355_attach_data_source_increments_source_count() {
    let mut g = grid(2, 3);
    assert_eq!(g.source_count(), 0);
    g.attach_data_source(text_source("src1", &["a", "b"]));
    assert_eq!(g.source_count(), 1);
}

#[test]
fn t356_attach_same_name_replaces_existing_source() {
    let mut g = grid(2, 3);
    g.attach_data_source(text_source("mine", &["old"]));
    g.attach_data_source(text_source("mine", &["new1", "new2"]));
    assert_eq!(g.source_count(), 1, "duplicate name should replace, not append");
    assert_eq!(g.data_sources[0].len(), 2);
}

#[test]
fn t357_detach_data_source_returns_true_when_found() {
    let mut g = grid(2, 3);
    g.attach_data_source(text_source("remove_me", &["x"]));
    assert!(g.detach_data_source("remove_me"));
    assert_eq!(g.source_count(), 0);
}

#[test]
fn t358_detach_data_source_returns_false_when_not_found() {
    let mut g = grid(2, 3);
    assert!(!g.detach_data_source("ghost_source"));
}

// ── §3 auto_fill ──────────────────────────────────────────────────────────────

#[test]
fn t359_auto_fill_populates_all_cells_cycling_values() {
    // 2 rows × 3 cols = 6 cells; source has 2 values → cycles.
    let mut g = grid(2, 3);
    g.attach_data_source(DataSource::new("s", "label.text", vec![json!("X"), json!("Y")]));
    g.auto_fill();

    assert_eq!(g.data_overrides.len(), 6, "should have one override per cell");
    // Cell (0,0) → X, (0,1) → Y, (0,2) → X, (1,0) → Y, (1,1) → X, (1,2) → Y
    let get = |row: u32, col: u32| -> &serde_json::Value {
        g.data_overrides
            .iter()
            .find(|o| o.row == row && o.col == col && o.layer_path == "label.text")
            .map(|o| &o.value)
            .unwrap()
    };
    assert_eq!(get(0, 0), &json!("X"));
    assert_eq!(get(0, 1), &json!("Y"));
    assert_eq!(get(0, 2), &json!("X"));
    assert_eq!(get(1, 0), &json!("Y"));
}

#[test]
fn t360_auto_fill_with_empty_source_produces_no_overrides() {
    let mut g = grid(3, 3);
    g.attach_data_source(DataSource::new("empty", "k", vec![]));
    g.auto_fill();
    assert_eq!(g.data_overrides.len(), 0);
}

#[test]
fn t361_clear_data_sources_removes_all_sources() {
    let mut g = grid(2, 2);
    g.attach_data_source(text_source("s1", &["a"]));
    g.attach_data_source(text_source("s2", &["b"]));
    assert_eq!(g.source_count(), 2);
    g.clear_data_sources();
    assert_eq!(g.source_count(), 0);
}

#[test]
fn t362_multiple_sources_auto_fill_writes_both_columns() {
    let mut g = grid(1, 2);
    g.attach_data_source(DataSource::new("texts", "label.text", vec![json!("Hi"), json!("Bye")]));
    g.attach_data_source(DataSource::new("fills", "bg.fill", vec![json!("#red"), json!("#blue")]));
    g.auto_fill();

    // Each cell gets 2 overrides (one from each source) → 2 cells × 2 = 4.
    assert_eq!(g.data_overrides.len(), 4);
}

#[test]
fn t363_auto_fill_overwrites_prior_overrides_for_same_column() {
    let mut g = grid(1, 2);
    // Pre-populate with an old value.
    g.set_data_override(0, 0, "label.text", json!("old"));
    g.set_data_override(0, 1, "label.text", json!("old"));

    // Attach a source for the same column and auto-fill.
    g.attach_data_source(DataSource::new("s", "label.text", vec![json!("new1"), json!("new2")]));
    g.auto_fill();

    assert_eq!(g.data_overrides.len(), 2);
    let v0 = g.get_data_override(0, 0, "label.text").unwrap();
    assert_eq!(v0.value, json!("new1"));
    let v1 = g.get_data_override(0, 1, "label.text").unwrap();
    assert_eq!(v1.value, json!("new2"));
}

// ── §4 Command integration ────────────────────────────────────────────────────

#[test]
fn t364_command_to_id_bind_grid_data_source() {
    let grid_id = Uuid::new_v4();
    let src = DataSource::new("s", "k", vec![]);
    let cmd = Command::BindGridDataSource { grid_id, source: src };
    assert_eq!(command_to_id(&cmd), "grid.bind-source");
}

#[test]
fn t365_registry_contains_all_three_grid_commands() {
    let reg = CommandRegistry::new();
    assert!(reg.get("grid.bind-source").is_some(), "grid.bind-source not registered");
    assert!(reg.get("grid.unbind-source").is_some(), "grid.unbind-source not registered");
    assert!(reg.get("grid.auto-fill").is_some(), "grid.auto-fill not registered");

    // Verify IDs for the other two commands as well.
    let unbind_cmd = Command::UnbindGridDataSource {
        grid_id: Uuid::new_v4(),
        name: "s".into(),
    };
    let fill_cmd = Command::AutoFillGrid { grid_id: Uuid::new_v4() };
    assert_eq!(command_to_id(&unbind_cmd), "grid.unbind-source");
    assert_eq!(command_to_id(&fill_cmd), "grid.auto-fill");
}
