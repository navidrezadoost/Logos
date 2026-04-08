// Phase 3 – Property Override Serialization Tests (t301–t320)
//
// Tests for `logos_core::persistence::DocumentSnapshot`:
// round-trips, schema versioning, component registry, and grid payloads.

use std::collections::HashMap;

use logos_core::container::{
    ComponentRef, ComponentVariant, PropertyOverride, VariantState,
};
use logos_core::persistence::{DocumentSnapshot, SCHEMA_VERSION};
use logos_core::{Document, Layer, RectLayer};
use serde_json::json;
use uuid::Uuid;

// ── Helpers ────────────────────────────────────────────────────────────────────

fn empty_doc() -> Document {
    Document::new()
}

fn make_component_ref(state: VariantState) -> ComponentRef {
    let mut cref = ComponentRef {
        component_id: Uuid::new_v4(),
        overrides: Vec::new(),
        variants: Vec::new(),
        current_state: state,
    };
    cref.add_variant(ComponentVariant::new(state));
    cref
}

fn make_registry(n: usize) -> HashMap<Uuid, ComponentRef> {
    (0..n)
        .map(|_| {
            let id = Uuid::new_v4();
            let cref = make_component_ref(VariantState::Default);
            (id, cref)
        })
        .collect()
}

// ── §1 Basic construction ──────────────────────────────────────────────────────

#[test]
fn t301_capture_empty_doc_schema_version_is_current() {
    let doc = empty_doc();
    let snap = DocumentSnapshot::capture(&doc, &HashMap::new(), &[]);
    assert_eq!(snap.schema_version, SCHEMA_VERSION);
    assert_eq!(snap.schema_version, 3);
    assert!(snap.is_current_schema());
}

#[test]
fn t302_roundtrip_empty_doc_to_json_from_json() {
    let doc = empty_doc();
    let snap = DocumentSnapshot::capture(&doc, &HashMap::new(), &[]);
    let json = snap.to_json().unwrap();
    let restored = DocumentSnapshot::from_json(&json).unwrap();
    assert_eq!(restored.schema_version, SCHEMA_VERSION);
    assert_eq!(restored.component_count(), 0);
    assert_eq!(restored.grid_count(), 0);
}

#[test]
fn t303_from_json_error_on_malformed_input() {
    assert!(DocumentSnapshot::from_json("not valid json!!!").is_err());
}

#[test]
fn t304_schema_version_constant_is_3() {
    assert_eq!(SCHEMA_VERSION, 3);
}

#[test]
fn t305_is_current_schema_true_for_fresh_snapshot() {
    let snap = DocumentSnapshot::capture(&empty_doc(), &HashMap::new(), &[]);
    assert!(snap.is_current_schema());
}

// ── §2 Document content round-trips ───────────────────────────────────────────

#[test]
fn t306_roundtrip_doc_with_layers_preserves_layer_count() {
    let doc = Document::new();
    doc.add_layer(Layer::Rect(RectLayer::new(0.0, 0.0, 100.0, 100.0))).unwrap();
    doc.add_layer(Layer::Rect(RectLayer::new(10.0, 10.0, 50.0, 50.0))).unwrap();

    let snap = DocumentSnapshot::capture(&doc, &HashMap::new(), &[]);
    let json = snap.to_json().unwrap();
    let restored = DocumentSnapshot::from_json(&json).unwrap();
    let (rdoc, _, _) = restored.restore();
    let layer_count = rdoc.root.read().unwrap().layers.len();
    assert_eq!(layer_count, 2);
}

#[test]
fn t307_roundtrip_doc_with_component_ref_preserves_count() {
    let doc = empty_doc();
    let id = Uuid::new_v4();
    let mut registry = HashMap::new();
    registry.insert(id, make_component_ref(VariantState::Hover));

    let snap = DocumentSnapshot::capture(&doc, &registry, &[]);
    let json = snap.to_json().unwrap();
    let restored = DocumentSnapshot::from_json(&json).unwrap();
    assert_eq!(restored.component_count(), 1);
}

#[test]
fn t308_all_six_variant_states_survive_roundtrip() {
    let states = [
        VariantState::Default,
        VariantState::Hover,
        VariantState::Active,
        VariantState::Disabled,
        VariantState::Focus,
        VariantState::Error,
    ];
    for state in states {
        let id = Uuid::new_v4();
        let mut registry = HashMap::new();
        registry.insert(id, make_component_ref(state));

        let snap = DocumentSnapshot::capture(&empty_doc(), &registry, &[]);
        let json = snap.to_json().unwrap();
        let restored = DocumentSnapshot::from_json(&json).unwrap();
        let (_, reg, _) = restored.restore();
        let cref = reg.get(&id).unwrap();
        assert_eq!(cref.current_state, state, "state {state:?} failed roundtrip");
    }
}

#[test]
fn t309_property_override_json_value_survives_roundtrip() {
    let id = Uuid::new_v4();
    let mut cref = make_component_ref(VariantState::Hover);
    // Set a base override with a complex JSON value.
    cref.set_base_override("fill", json!({"r": 255, "g": 0, "b": 0, "a": 1.0}));

    let mut registry = HashMap::new();
    registry.insert(id, cref.clone());

    let snap = DocumentSnapshot::capture(&empty_doc(), &registry, &[]);
    let json_str = snap.to_json().unwrap();
    let restored = DocumentSnapshot::from_json(&json_str).unwrap();
    let (_, reg, _) = restored.restore();
    let rcref = reg.get(&id).unwrap();
    assert_eq!(rcref.overrides.len(), 1);
    assert_eq!(rcref.overrides[0].value, json!({"r": 255, "g": 0, "b": 0, "a": 1.0}));
}

// ── §3 Grid payloads ───────────────────────────────────────────────────────────

#[test]
fn t310_grids_as_json_values_survive_roundtrip() {
    let grid_val = json!({ "id": Uuid::new_v4().to_string(), "rows": 3, "columns": 4 });
    let snap = DocumentSnapshot::capture(&empty_doc(), &HashMap::new(), &[grid_val.clone()]);
    let json_str = snap.to_json().unwrap();
    let restored = DocumentSnapshot::from_json(&json_str).unwrap();
    assert_eq!(restored.grid_count(), 1);
    let (_, _, grids) = restored.restore();
    assert_eq!(grids[0]["rows"], json!(3));
    assert_eq!(grids[0]["columns"], json!(4));
}

#[test]
fn t311_component_count_matches_registry_input() {
    let registry = make_registry(5);
    let snap = DocumentSnapshot::capture(&empty_doc(), &registry, &[]);
    assert_eq!(snap.component_count(), 5);
}

#[test]
fn t312_grid_count_matches_input() {
    let grids = vec![json!({"a": 1}), json!({"b": 2}), json!({"c": 3})];
    let snap = DocumentSnapshot::capture(&empty_doc(), &HashMap::new(), &grids);
    assert_eq!(snap.grid_count(), 3);
}

#[test]
fn t313_restore_returns_owned_parts() {
    let registry = make_registry(2);
    let grids = vec![json!({"x": 42})];
    let snap = DocumentSnapshot::capture(&empty_doc(), &registry, &grids);
    let (doc, reg, gs) = snap.restore();
    // Check ownership — all three parts are now owned values.
    assert_eq!(reg.len(), 2);
    assert_eq!(gs.len(), 1);
    assert_eq!(gs[0]["x"], json!(42));
    let _ = doc; // moved successfully
}

// ── §4 Edge cases ──────────────────────────────────────────────────────────────

#[test]
fn t314_from_json_error_on_empty_string() {
    assert!(DocumentSnapshot::from_json("").is_err());
}

#[test]
fn t315_unicode_in_property_override_path_survives_roundtrip() {
    let id = Uuid::new_v4();
    let mut cref = make_component_ref(VariantState::Default);
    cref.set_base_override("颜色.填充", json!("红色"));

    let mut registry = HashMap::new();
    registry.insert(id, cref);

    let snap = DocumentSnapshot::capture(&empty_doc(), &registry, &[]);
    let json_str = snap.to_json().unwrap();
    let restored = DocumentSnapshot::from_json(&json_str).unwrap();
    let (_, reg, _) = restored.restore();
    let rcref = reg.get(&id).unwrap();
    assert_eq!(rcref.overrides[0].path, "颜色.填充");
    assert_eq!(rcref.overrides[0].value, json!("红色"));
}

#[test]
fn t316_multiple_components_all_survive_roundtrip() {
    let registry = make_registry(10);
    let original_ids: std::collections::HashSet<_> = registry.keys().cloned().collect();

    let snap = DocumentSnapshot::capture(&empty_doc(), &registry, &[]);
    let json_str = snap.to_json().unwrap();
    let restored = DocumentSnapshot::from_json(&json_str).unwrap();
    assert_eq!(restored.component_count(), 10);

    let (_, reg, _) = restored.restore();
    let restored_ids: std::collections::HashSet<_> = reg.keys().cloned().collect();
    assert_eq!(original_ids, restored_ids);
}

#[test]
fn t317_snapshot_with_no_grids_has_grid_count_zero() {
    let snap = DocumentSnapshot::capture(&empty_doc(), &HashMap::new(), &[]);
    assert_eq!(snap.grid_count(), 0);
    let json_str = snap.to_json().unwrap();
    let restored = DocumentSnapshot::from_json(&json_str).unwrap();
    assert_eq!(restored.grid_count(), 0);
}

#[test]
fn t318_snapshot_with_multiple_grids_all_survive() {
    let grids: Vec<serde_json::Value> = (0..5)
        .map(|i| json!({ "index": i, "rows": i + 1 }))
        .collect();
    let snap = DocumentSnapshot::capture(&empty_doc(), &HashMap::new(), &grids);
    let json_str = snap.to_json().unwrap();
    let restored = DocumentSnapshot::from_json(&json_str).unwrap();
    assert_eq!(restored.grid_count(), 5);
    let (_, _, rgrids) = restored.restore();
    for (i, g) in rgrids.iter().enumerate() {
        assert_eq!(g["index"], json!(i as i64));
    }
}

#[test]
fn t319_restore_after_roundtrip_preserves_document_layer_count() {
    let doc = Document::new();
    for _ in 0..7 {
        doc.add_layer(Layer::Rect(RectLayer::new(0.0, 0.0, 10.0, 10.0))).unwrap();
    }
    let snap = DocumentSnapshot::capture(&doc, &HashMap::new(), &[]);
    let json_str = snap.to_json().unwrap();
    let restored = DocumentSnapshot::from_json(&json_str).unwrap();
    let (rdoc, _, _) = restored.restore();
    assert_eq!(rdoc.root.read().unwrap().layers.len(), 7);
}

#[test]
fn t320_large_registry_20_entries_round_trips() {
    let registry = make_registry(20);
    let snap = DocumentSnapshot::capture(&empty_doc(), &registry, &[]);
    let json_str = snap.to_json().unwrap();
    let restored = DocumentSnapshot::from_json(&json_str).unwrap();
    assert_eq!(restored.component_count(), 20);
}
