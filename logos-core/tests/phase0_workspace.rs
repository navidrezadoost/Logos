//! Phase 0 Integration Tests — Hybrid Workspace Foundation
//!
//! Coverage:
//!   §1 WorkspaceMode + DocumentMode  (tests t001–t025)
//!   §2 ConstraintSystem              (tests t026–t050)
//!   §3 RepeatGrid (via logos-layout) (tests t051–t075)
//!   §4 ComponentVariant + State      (tests t076–t100)

use logos_core::constraint::{
    Constraints, HorizontalConstraint, VerticalConstraint, resolve_constraints,
};
use logos_core::container::{
    ComponentRef, ComponentVariant, PropertyOverride, VariantState,
};
use logos_core::{
    Document, DocumentMode, Rect, WorkspaceMode,
};
use uuid::Uuid;

// ── §1: WorkspaceMode + DocumentMode ─────────────────────────────────────────

#[test]
fn t001_workspace_mode_default_is_hybrid() {
    let dm = DocumentMode::default();
    assert_eq!(dm.mode, WorkspaceMode::Hybrid);
}

#[test]
fn t002_document_mode_flat_constructor() {
    let dm = DocumentMode::flat();
    assert_eq!(dm.mode, WorkspaceMode::FlatPage);
}

#[test]
fn t003_document_mode_artboard_constructor() {
    let dm = DocumentMode::artboard();
    assert_eq!(dm.mode, WorkspaceMode::ArtboardSection);
}

#[test]
fn t004_document_mode_hybrid_constructor() {
    let dm = DocumentMode::hybrid();
    assert_eq!(dm.mode, WorkspaceMode::Hybrid);
}

#[test]
fn t005_document_new_has_hybrid_mode() {
    let doc = Document::new();
    assert_eq!(doc.doc_mode.mode, WorkspaceMode::Hybrid);
}

#[test]
fn t006_document_with_mode_flat() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    assert_eq!(doc.doc_mode.mode, WorkspaceMode::FlatPage);
}

#[test]
fn t007_document_with_mode_artboard_section() {
    let doc = Document::with_mode(WorkspaceMode::ArtboardSection);
    assert_eq!(doc.doc_mode.mode, WorkspaceMode::ArtboardSection);
}

#[test]
fn t008_workspace_mode_supports_artboards_hybrid() {
    assert!(WorkspaceMode::Hybrid.supports_artboards());
}

#[test]
fn t009_workspace_mode_supports_artboards_artboard_only() {
    assert!(WorkspaceMode::ArtboardSection.supports_artboards());
}

#[test]
fn t010_flatpage_does_not_support_artboards() {
    assert!(!WorkspaceMode::FlatPage.supports_artboards());
}

#[test]
fn t011_workspace_mode_supports_flat_hybrid() {
    assert!(WorkspaceMode::Hybrid.supports_flat());
}

#[test]
fn t012_workspace_mode_supports_flat_flatpage() {
    assert!(WorkspaceMode::FlatPage.supports_flat());
}

#[test]
fn t013_artboard_section_does_not_support_flat() {
    assert!(!WorkspaceMode::ArtboardSection.supports_flat());
}

#[test]
fn t014_workspace_mode_label_flat() {
    assert_eq!(WorkspaceMode::FlatPage.label(), "Flat Page");
}

#[test]
fn t015_workspace_mode_label_artboard() {
    assert_eq!(WorkspaceMode::ArtboardSection.label(), "Artboard / Section");
}

#[test]
fn t016_workspace_mode_label_hybrid() {
    assert_eq!(WorkspaceMode::Hybrid.label(), "Hybrid");
}

#[test]
fn t017_document_mode_show_grid_default_false() {
    let dm = DocumentMode::default();
    assert!(!dm.show_grid);
}

#[test]
fn t018_document_mode_snap_to_objects_default_true() {
    let dm = DocumentMode::default();
    assert!(dm.snap_to_objects);
}

#[test]
fn t019_document_mode_new_preserves_defaults() {
    let dm = DocumentMode::new(WorkspaceMode::FlatPage);
    assert!(!dm.show_grid);
    assert!(dm.snap_to_objects);
}

#[test]
fn t020_workspace_mode_serde_flatpage() {
    let m = WorkspaceMode::FlatPage;
    let j = serde_json::to_string(&m).unwrap();
    let back: WorkspaceMode = serde_json::from_str(&j).unwrap();
    assert_eq!(back, m);
}

#[test]
fn t021_workspace_mode_serde_hybrid() {
    let m = WorkspaceMode::Hybrid;
    let j = serde_json::to_string(&m).unwrap();
    let back: WorkspaceMode = serde_json::from_str(&j).unwrap();
    assert_eq!(back, m);
}

#[test]
fn t022_document_mode_serde_roundtrip() {
    let dm = DocumentMode::artboard();
    let j = serde_json::to_string(&dm).unwrap();
    let back: DocumentMode = serde_json::from_str(&j).unwrap();
    assert_eq!(back.mode, WorkspaceMode::ArtboardSection);
}

#[test]
fn t023_two_documents_independent_modes() {
    let d1 = Document::with_mode(WorkspaceMode::FlatPage);
    let d2 = Document::with_mode(WorkspaceMode::ArtboardSection);
    assert_ne!(d1.doc_mode.mode, d2.doc_mode.mode);
}

#[test]
fn t024_document_mode_can_be_mutated() {
    let mut doc = Document::new();
    doc.doc_mode.mode = WorkspaceMode::FlatPage;
    assert_eq!(doc.doc_mode.mode, WorkspaceMode::FlatPage);
}

#[test]
fn t025_workspace_mode_eq_reflexive() {
    assert_eq!(WorkspaceMode::Hybrid, WorkspaceMode::Hybrid);
    assert_ne!(WorkspaceMode::FlatPage, WorkspaceMode::ArtboardSection);
}

// ── §2: ConstraintSystem ─────────────────────────────────────────────────────

fn r(x: f32, y: f32, w: f32, h: f32) -> Rect { Rect { x, y, width: w, height: h } }

#[test]
fn t026_default_constraints_top_left() {
    let c = Constraints::default();
    assert_eq!(c.horizontal, HorizontalConstraint::Left);
    assert_eq!(c.vertical, VerticalConstraint::Top);
}

#[test]
fn t027_left_constraint_x_unchanged_on_resize() {
    let po = r(0.0, 0.0, 400.0, 300.0);
    let pn = r(0.0, 0.0, 800.0, 300.0);
    let child = r(50.0, 0.0, 100.0, 50.0);
    let result = resolve_constraints(po, pn, child, &Constraints::default());
    assert_eq!(result.x, 50.0);
}

#[test]
fn t028_right_constraint_maintains_right_margin() {
    let po = r(0.0, 0.0, 400.0, 300.0);
    let pn = r(0.0, 0.0, 600.0, 300.0);
    let child = r(300.0, 0.0, 80.0, 50.0); // right margin = 400-380 = 20
    let c = Constraints::new(HorizontalConstraint::Right, VerticalConstraint::Top);
    let result = resolve_constraints(po, pn, child, &c);
    assert_eq!(result.x, 500.0); // 600 - 20 - 80
}

#[test]
fn t029_stretch_horizontal_grows_child() {
    let po = r(0.0, 0.0, 400.0, 300.0);
    let pn = r(0.0, 0.0, 600.0, 300.0);
    let child = r(10.0, 0.0, 380.0, 50.0); // margins: left=10, right=10
    let c = Constraints::stretch();
    let result = resolve_constraints(po, pn, child, &c);
    assert_eq!(result.width, 580.0); // 600 - 10 - 10
}

#[test]
fn t030_center_h_tracks_parent_center() {
    let po = r(0.0, 0.0, 200.0, 200.0);
    let pn = r(0.0, 0.0, 400.0, 200.0);
    let child = r(75.0, 0.0, 50.0, 30.0); // center=100 = parent center
    let c = Constraints::new(HorizontalConstraint::Center, VerticalConstraint::Top);
    let result = resolve_constraints(po, pn, child, &c);
    assert_eq!(result.x, 175.0); // new center=200, x=200-25
}

#[test]
fn t031_scale_h_doubles_with_parent() {
    let po = r(0.0, 0.0, 100.0, 200.0);
    let pn = r(0.0, 0.0, 200.0, 200.0);
    let child = r(10.0, 0.0, 40.0, 20.0);
    let c = Constraints::scale();
    let result = resolve_constraints(po, pn, child, &c);
    assert_eq!(result.x, 20.0);
    assert_eq!(result.width, 80.0);
}

#[test]
fn t032_top_constraint_y_unchanged() {
    let po = r(0.0, 0.0, 400.0, 300.0);
    let pn = r(0.0, 0.0, 400.0, 600.0);
    let child = r(0.0, 40.0, 100.0, 50.0);
    let result = resolve_constraints(po, pn, child, &Constraints::default());
    assert_eq!(result.y, 40.0);
}

#[test]
fn t033_bottom_constraint_maintains_bottom_margin() {
    let po = r(0.0, 0.0, 400.0, 300.0);
    let pn = r(0.0, 0.0, 400.0, 500.0);
    let child = r(0.0, 240.0, 100.0, 40.0); // bottom margin = 300-280 = 20
    let c = Constraints::new(HorizontalConstraint::Left, VerticalConstraint::Bottom);
    let result = resolve_constraints(po, pn, child, &c);
    assert_eq!(result.y, 440.0); // 500 - 20 - 40
}

#[test]
fn t034_stretch_vertical_grows_child() {
    let po = r(0.0, 0.0, 400.0, 300.0);
    let pn = r(0.0, 0.0, 400.0, 500.0);
    let child = r(0.0, 20.0, 100.0, 260.0); // margins top=20, bottom=20
    let c = Constraints::new(HorizontalConstraint::Left, VerticalConstraint::TopAndBottom);
    let result = resolve_constraints(po, pn, child, &c);
    assert_eq!(result.height, 460.0);
}

#[test]
fn t035_center_v_tracks_parent_center() {
    let po = r(0.0, 0.0, 200.0, 200.0);
    let pn = r(0.0, 0.0, 200.0, 400.0);
    let child = r(0.0, 75.0, 50.0, 50.0); // center=100 = parent center
    let c = Constraints::new(HorizontalConstraint::Left, VerticalConstraint::Center);
    let result = resolve_constraints(po, pn, child, &c);
    assert_eq!(result.y, 175.0);
}

#[test]
fn t036_scale_v_doubles_with_parent() {
    let po = r(0.0, 0.0, 200.0, 100.0);
    let pn = r(0.0, 0.0, 200.0, 200.0);
    let child = r(0.0, 10.0, 50.0, 30.0);
    let c = Constraints::scale();
    let result = resolve_constraints(po, pn, child, &c);
    assert_eq!(result.y, 20.0);
    assert_eq!(result.height, 60.0);
}

#[test]
fn t037_no_resize_identity_left_top() {
    let parent = r(0.0, 0.0, 400.0, 300.0);
    let child = r(50.0, 50.0, 100.0, 80.0);
    let result = resolve_constraints(parent, parent, child, &Constraints::default());
    assert_eq!(result.x, child.x);
    assert_eq!(result.y, child.y);
    assert!((result.width - child.width).abs() < 0.001);
    assert!((result.height - child.height).abs() < 0.001);
}

#[test]
fn t038_constraints_serde_roundtrip() {
    let c = Constraints::new(HorizontalConstraint::Scale, VerticalConstraint::Bottom);
    let j = serde_json::to_string(&c).unwrap();
    let back: Constraints = serde_json::from_str(&j).unwrap();
    assert_eq!(back, c);
}

#[test]
fn t039_stretch_clamps_to_zero() {
    let po = r(0.0, 0.0, 400.0, 300.0);
    let pn = r(0.0, 0.0, 5.0, 5.0);
    let child = r(10.0, 10.0, 380.0, 280.0);
    let c = Constraints::stretch();
    let result = resolve_constraints(po, pn, child, &c);
    assert_eq!(result.width, 0.0);
    assert_eq!(result.height, 0.0);
}

#[test]
fn t040_convenience_top_left() {
    let c = Constraints::top_left();
    assert_eq!(c.horizontal, HorizontalConstraint::Left);
    assert_eq!(c.vertical, VerticalConstraint::Top);
}

#[test]
fn t041_scale_then_serde_roundtrip() {
    let c = Constraints::scale();
    let j = serde_json::to_string(&c).unwrap();
    let back: Constraints = serde_json::from_str(&j).unwrap();
    assert_eq!(back.horizontal, HorizontalConstraint::Scale);
}

#[test]
fn t042_right_shrink_does_not_clip() {
    let po = r(0.0, 0.0, 400.0, 300.0);
    let pn = r(0.0, 0.0, 150.0, 300.0);
    let child = r(300.0, 0.0, 80.0, 50.0); // right margin = 20
    let c = Constraints::new(HorizontalConstraint::Right, VerticalConstraint::Top);
    let result = resolve_constraints(po, pn, child, &c);
    assert_eq!(result.x, 50.0); // 150 - 20 - 80
    assert_eq!(result.width, 80.0);
}

#[test]
fn t043_center_h_offset_preserved() {
    let po = r(0.0, 0.0, 200.0, 200.0);
    let pn = r(0.0, 0.0, 400.0, 200.0);
    // child center at 120 (+20 from parent center 100)
    let child = r(95.0, 0.0, 50.0, 30.0);
    let c = Constraints::new(HorizontalConstraint::Center, VerticalConstraint::Top);
    let result = resolve_constraints(po, pn, child, &c);
    // new center = 200 + 20 = 220, x = 220 - 25 = 195
    assert_eq!(result.x, 195.0);
}

#[test]
fn t044_scale_both_quarter() {
    let po = r(0.0, 0.0, 400.0, 200.0);
    let pn = r(0.0, 0.0, 100.0, 50.0);
    let child = r(100.0, 40.0, 200.0, 100.0);
    let c = Constraints::scale();
    let result = resolve_constraints(po, pn, child, &c);
    assert!((result.x - 25.0).abs() < 0.01);
    assert!((result.y - 10.0).abs() < 0.01);
    assert!((result.width - 50.0).abs() < 0.01);
    assert!((result.height - 25.0).abs() < 0.01);
}

#[test]
fn t045_horizontal_constraint_eq() {
    assert_eq!(HorizontalConstraint::Left, HorizontalConstraint::Left);
    assert_ne!(HorizontalConstraint::Left, HorizontalConstraint::Right);
}

#[test]
fn t046_vertical_constraint_eq() {
    assert_eq!(VerticalConstraint::Top, VerticalConstraint::Top);
    assert_ne!(VerticalConstraint::Top, VerticalConstraint::Bottom);
}

#[test]
fn t047_constraints_eq() {
    let a = Constraints::scale();
    let b = Constraints::scale();
    assert_eq!(a, b);
}

#[test]
fn t048_stretch_h_only() {
    let po = r(0.0, 0.0, 400.0, 300.0);
    let pn = r(0.0, 0.0, 600.0, 300.0);
    let child = r(20.0, 50.0, 360.0, 80.0);
    let c = Constraints::new(HorizontalConstraint::LeftAndRight, VerticalConstraint::Top);
    let result = resolve_constraints(po, pn, child, &c);
    assert_eq!(result.width, 560.0);
    assert_eq!(result.y, 50.0);
    assert_eq!(result.height, 80.0);
}

#[test]
fn t049_stretch_v_only() {
    let po = r(0.0, 0.0, 400.0, 300.0);
    let pn = r(0.0, 0.0, 400.0, 500.0);
    let child = r(50.0, 30.0, 80.0, 240.0);
    let c = Constraints::new(HorizontalConstraint::Left, VerticalConstraint::TopAndBottom);
    let result = resolve_constraints(po, pn, child, &c);
    assert_eq!(result.x, 50.0);
    assert_eq!(result.width, 80.0);
    assert_eq!(result.height, 440.0);
}

#[test]
fn t050_default_constraints_copy() {
    let c = Constraints::default();
    let c2 = c; // Copy
    assert_eq!(c, c2);
}

// ── §3: Constraint + DocumentMode integration ─────────────────────────────────

#[test]
fn t051_flat_doc_constraint_left_top() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    assert!(!doc.doc_mode.mode.supports_artboards());
    let c = Constraints::top_left();
    assert_eq!(c.horizontal, HorizontalConstraint::Left);
}

#[test]
fn t052_artboard_doc_constraint_center() {
    let doc = Document::with_mode(WorkspaceMode::ArtboardSection);
    assert!(doc.doc_mode.mode.supports_artboards());
    let c = Constraints::center();
    assert_eq!(c.vertical, VerticalConstraint::Center);
}

#[test]
fn t053_hybrid_doc_supports_both_modes() {
    let doc = Document::with_mode(WorkspaceMode::Hybrid);
    assert!(doc.doc_mode.mode.supports_artboards());
    assert!(doc.doc_mode.mode.supports_flat());
}

#[test]
fn t054_constraint_resolve_with_hybrid_doc() {
    let _doc = Document::with_mode(WorkspaceMode::Hybrid);
    let po = r(0.0, 0.0, 200.0, 200.0);
    let pn = r(0.0, 0.0, 400.0, 200.0);
    let child = r(50.0, 50.0, 100.0, 100.0);
    let c = Constraints::stretch();
    let res = resolve_constraints(po, pn, child, &c);
    assert_eq!(res.width, 300.0); // 400 - 50(left) - 50(right)
}

#[test]
fn t055_document_mode_mutate_snap() {
    let mut doc = Document::new();
    doc.doc_mode.snap_to_objects = false;
    assert!(!doc.doc_mode.snap_to_objects);
}

#[test]
fn t056_document_mode_show_grid_toggle() {
    let mut doc = Document::new();
    doc.doc_mode.show_grid = true;
    assert!(doc.doc_mode.show_grid);
}

#[test]
fn t057_constraint_right_with_artboard_doc() {
    let _doc = Document::with_mode(WorkspaceMode::ArtboardSection);
    let po = r(0.0, 0.0, 500.0, 300.0);
    let pn = r(0.0, 0.0, 700.0, 300.0);
    let child = r(400.0, 0.0, 80.0, 50.0); // right margin = 20
    let c = Constraints::new(HorizontalConstraint::Right, VerticalConstraint::Top);
    let res = resolve_constraints(po, pn, child, &c);
    assert_eq!(res.x, 600.0); // 700 - 20 - 80
}

#[test]
fn t058_workspace_mode_can_clone() {
    let m = WorkspaceMode::Hybrid;
    let m2 = m;
    assert_eq!(m, m2);
}

#[test]
fn t059_document_mode_new_sets_mode() {
    let dm = DocumentMode::new(WorkspaceMode::FlatPage);
    assert_eq!(dm.mode, WorkspaceMode::FlatPage);
    assert!(dm.snap_to_objects); // default
}

#[test]
fn t060_constraint_scale_with_flat_doc() {
    let _doc = Document::with_mode(WorkspaceMode::FlatPage);
    let po = r(0.0, 0.0, 100.0, 100.0);
    let pn = r(0.0, 0.0, 200.0, 200.0);
    let child = r(10.0, 10.0, 80.0, 80.0);
    let c = Constraints::scale();
    let res = resolve_constraints(po, pn, child, &c);
    assert_eq!(res.x, 20.0);
    assert_eq!(res.width, 160.0);
}

#[test]
fn t061_constraint_bottom_v_flat() {
    let po = r(0.0, 0.0, 400.0, 300.0);
    let pn = r(0.0, 0.0, 400.0, 600.0);
    let child = r(0.0, 250.0, 100.0, 30.0); // bottom margin = 300-280 = 20
    let c = Constraints::new(HorizontalConstraint::Left, VerticalConstraint::Bottom);
    let res = resolve_constraints(po, pn, child, &c);
    assert_eq!(res.y, 550.0); // 600 - 20 - 30
}

#[test]
fn t062_document_version_default() {
    let doc = Document::new();
    assert_eq!(doc.version, 1);
}

#[test]
fn t063_document_ids_unique() {
    let d1 = Document::new();
    let d2 = Document::new();
    assert_ne!(d1.id, d2.id);
}

#[test]
fn t064_document_mode_artboard_snap_default() {
    let dm = DocumentMode::artboard();
    assert!(dm.snap_to_objects);
}

#[test]
fn t065_constraints_new_custom() {
    let c = Constraints::new(HorizontalConstraint::Center, VerticalConstraint::Bottom);
    assert_eq!(c.horizontal, HorizontalConstraint::Center);
    assert_eq!(c.vertical, VerticalConstraint::Bottom);
}

#[test]
fn t066_constraint_center_h_no_resize_stays() {
    let parent = r(0.0, 0.0, 200.0, 200.0);
    let child = r(75.0, 50.0, 50.0, 50.0); // centered
    let c = Constraints::new(HorizontalConstraint::Center, VerticalConstraint::Top);
    let res = resolve_constraints(parent, parent, child, &c);
    assert!((res.x - 75.0).abs() < 0.01);
}

#[test]
fn t067_workspace_mode_serde_artboard() {
    let m = WorkspaceMode::ArtboardSection;
    let j = serde_json::to_string(&m).unwrap();
    let back: WorkspaceMode = serde_json::from_str(&j).unwrap();
    assert_eq!(back, WorkspaceMode::ArtboardSection);
}

#[test]
fn t068_constraint_stretch_both_resize() {
    let po = r(0.0, 0.0, 200.0, 200.0);
    let pn = r(0.0, 0.0, 300.0, 400.0);
    let child = r(10.0, 10.0, 180.0, 180.0);
    let c = Constraints::stretch();
    let res = resolve_constraints(po, pn, child, &c);
    assert_eq!(res.width, 280.0);  // 300 - 10 - 10
    assert_eq!(res.height, 380.0); // 400 - 10 - 10
}

#[test]
fn t069_document_default_has_page() {
    let doc = Document::new();
    let page = doc.root.read().unwrap();
    assert_eq!(page.name, "Page 1");
}

#[test]
fn t070_constraint_scale_then_center() {
    // scale then re-apply center
    let po = r(0.0, 0.0, 100.0, 100.0);
    let pm = r(0.0, 0.0, 200.0, 200.0);
    let child = r(25.0, 25.0, 50.0, 50.0);
    let scaled = resolve_constraints(po, pm, child, &Constraints::scale());
    // now apply center to scaled result
    let c2 = Constraints::center();
    let res = resolve_constraints(pm, pm, scaled, &c2);
    // no resize: same as scaled (identity)
    assert!((res.x - scaled.x).abs() < 0.01);
}

#[test]
fn t071_flat_mode_label() {
    assert_eq!(WorkspaceMode::FlatPage.label(), "Flat Page");
}

#[test]
fn t072_document_mode_clone() {
    let dm = DocumentMode::hybrid();
    let dm2 = dm.clone();
    assert_eq!(dm.mode, dm2.mode);
}

#[test]
fn t073_constraint_copy_semantics() {
    let a = Constraints::scale();
    let b = a; // Copy
    assert_eq!(a, b);
}

#[test]
fn t074_document_with_mode_inherits_defaults() {
    let doc = Document::with_mode(WorkspaceMode::FlatPage);
    assert!(doc.doc_mode.snap_to_objects);
    assert!(!doc.doc_mode.show_grid);
}

#[test]
fn t075_constraints_serde_all_combos() {
    let combos = [
        Constraints::top_left(),
        Constraints::stretch(),
        Constraints::scale(),
        Constraints::center(),
    ];
    for c in &combos {
        let j = serde_json::to_string(c).unwrap();
        let back: Constraints = serde_json::from_str(&j).unwrap();
        assert_eq!(back, *c);
    }
}

// ── §4: ComponentVariant + VariantState ──────────────────────────────────────

#[test]
fn t076_variant_state_default_is_default() {
    assert_eq!(VariantState::default(), VariantState::Default);
}

#[test]
fn t077_variant_state_all_count() {
    assert_eq!(VariantState::all().len(), 6);
}

#[test]
fn t078_variant_state_labels() {
    assert_eq!(VariantState::Default.label(), "Default");
    assert_eq!(VariantState::Disabled.label(), "Disabled");
    assert_eq!(VariantState::Focus.label(), "Focus");
}

#[test]
fn t079_component_ref_new_empty() {
    let id = Uuid::new_v4();
    let cr = ComponentRef::new(id);
    assert!(cr.variants.is_empty());
    assert!(cr.overrides.is_empty());
    assert_eq!(cr.current_state, VariantState::Default);
}

#[test]
fn t080_set_state_changes_current_state() {
    let mut cr = ComponentRef::new(Uuid::new_v4());
    cr.set_state(VariantState::Hover);
    assert_eq!(cr.current_state, VariantState::Hover);
}

#[test]
fn t081_add_variant_stores_variant() {
    let mut cr = ComponentRef::new(Uuid::new_v4());
    cr.add_variant(ComponentVariant::new(VariantState::Active));
    assert_eq!(cr.variants.len(), 1);
}

#[test]
fn t082_add_variant_replaces_same_state() {
    let mut cr = ComponentRef::new(Uuid::new_v4());
    cr.add_variant(ComponentVariant::new(VariantState::Hover));
    cr.add_variant(ComponentVariant::new(VariantState::Hover));
    assert_eq!(cr.variants.len(), 1);
}

#[test]
fn t083_remove_variant_found() {
    let mut cr = ComponentRef::new(Uuid::new_v4());
    cr.add_variant(ComponentVariant::new(VariantState::Disabled));
    assert!(cr.remove_variant(VariantState::Disabled));
    assert!(cr.variants.is_empty());
}

#[test]
fn t084_remove_variant_not_found_returns_false() {
    let mut cr = ComponentRef::new(Uuid::new_v4());
    assert!(!cr.remove_variant(VariantState::Error));
}

#[test]
fn t085_get_active_overrides_base_only() {
    let mut cr = ComponentRef::new(Uuid::new_v4());
    cr.set_base_override("fill", serde_json::json!("red"));
    let ao = cr.get_active_overrides();
    assert_eq!(ao.len(), 1);
    assert_eq!(ao[0].value, serde_json::json!("red"));
}

#[test]
fn t086_get_active_overrides_state_shadows_base() {
    let mut cr = ComponentRef::new(Uuid::new_v4());
    cr.set_base_override("fill", serde_json::json!("green"));
    let mut v = ComponentVariant::new(VariantState::Hover);
    v.set_override("fill", serde_json::json!("blue"));
    cr.add_variant(v);
    cr.set_state(VariantState::Hover);
    let ao = cr.get_active_overrides();
    assert_eq!(ao.len(), 1);
    assert_eq!(ao[0].value, serde_json::json!("blue"));
}

#[test]
fn t087_get_active_overrides_state_adds_extra_props() {
    let mut cr = ComponentRef::new(Uuid::new_v4());
    cr.set_base_override("fill", serde_json::json!("green"));
    let mut v = ComponentVariant::new(VariantState::Active);
    v.set_override("opacity", serde_json::json!(0.8));
    cr.add_variant(v);
    cr.set_state(VariantState::Active);
    let ao = cr.get_active_overrides();
    assert_eq!(ao.len(), 2);
}

#[test]
fn t088_active_overrides_no_matching_variant_uses_base() {
    let mut cr = ComponentRef::new(Uuid::new_v4());
    cr.set_base_override("padding", serde_json::json!(8));
    cr.set_state(VariantState::Focus);
    let ao = cr.get_active_overrides();
    assert_eq!(ao.len(), 1);
}

#[test]
fn t089_component_variant_set_override_replace() {
    let mut v = ComponentVariant::new(VariantState::Error);
    v.set_override("border", serde_json::json!("1px red"));
    v.set_override("border", serde_json::json!("2px red"));
    assert_eq!(v.overrides.len(), 1);
    assert_eq!(v.overrides[0].value, serde_json::json!("2px red"));
}

#[test]
fn t090_component_variant_remove_override_true() {
    let mut v = ComponentVariant::new(VariantState::Focus);
    v.set_override("glow", serde_json::json!(true));
    assert!(v.remove_override("glow"));
    assert!(v.overrides.is_empty());
}

#[test]
fn t091_component_variant_remove_override_false() {
    let mut v = ComponentVariant::new(VariantState::Hover);
    assert!(!v.remove_override("ghost"));
}

#[test]
fn t092_property_override_new_helper() {
    let o = PropertyOverride::new("x.y", serde_json::json!(99));
    assert_eq!(o.path, "x.y");
}

#[test]
fn t093_component_ref_serde_roundtrip() {
    let mut cr = ComponentRef::new(Uuid::new_v4());
    cr.set_base_override("z", serde_json::json!(1));
    let mut v = ComponentVariant::new(VariantState::Hover);
    v.set_override("z", serde_json::json!(2));
    cr.add_variant(v);
    let json = serde_json::to_string(&cr).unwrap();
    let back: ComponentRef = serde_json::from_str(&json).unwrap();
    assert_eq!(back.variants.len(), 1);
    assert_eq!(back.overrides.len(), 1);
}

#[test]
fn t094_variant_state_serde_all() {
    for s in VariantState::all() {
        let j = serde_json::to_string(s).unwrap();
        let back: VariantState = serde_json::from_str(&j).unwrap();
        assert_eq!(&back, s);
    }
}

#[test]
fn t095_multiple_variants_get_correct_one() {
    let mut cr = ComponentRef::new(Uuid::new_v4());
    cr.add_variant(ComponentVariant::new(VariantState::Hover));
    cr.add_variant(ComponentVariant::new(VariantState::Disabled));
    cr.add_variant(ComponentVariant::new(VariantState::Focus));
    assert!(cr.get_variant(VariantState::Hover).is_some());
    assert!(cr.get_variant(VariantState::Disabled).is_some());
    assert!(cr.get_variant(VariantState::Error).is_none());
}

#[test]
fn t096_set_state_to_default_uses_base_overrides() {
    let mut cr = ComponentRef::new(Uuid::new_v4());
    cr.set_base_override("fill", serde_json::json!("base"));
    cr.set_state(VariantState::Default);
    let ao = cr.get_active_overrides();
    assert_eq!(ao[0].value, serde_json::json!("base"));
}

#[test]
fn t097_two_component_refs_independent() {
    let mut cr1 = ComponentRef::new(Uuid::new_v4());
    let cr2 = ComponentRef::new(Uuid::new_v4());
    cr1.set_state(VariantState::Hover);
    assert_eq!(cr2.current_state, VariantState::Default);
}

#[test]
fn t098_remove_all_variants() {
    let mut cr = ComponentRef::new(Uuid::new_v4());
    for s in VariantState::all() {
        cr.add_variant(ComponentVariant::new(*s));
    }
    assert_eq!(cr.variants.len(), 6);
    for s in VariantState::all() {
        cr.remove_variant(*s);
    }
    assert!(cr.variants.is_empty());
}

#[test]
fn t099_variant_state_copy() {
    let s = VariantState::Active;
    let s2 = s; // Copy
    assert_eq!(s, s2);
}

#[test]
fn t100_full_workflow_document_with_components() {
    // Create a Hybrid document, bind a component ref with variants
    let doc = Document::with_mode(WorkspaceMode::Hybrid);
    assert!(doc.doc_mode.mode.supports_artboards());
    assert!(doc.doc_mode.mode.supports_flat());

    let mut cr = ComponentRef::new(Uuid::new_v4());
    cr.set_base_override("fill", serde_json::json!("#007bff"));
    let mut hover = ComponentVariant::new(VariantState::Hover);
    hover.set_override("fill", serde_json::json!("#0056b3"));
    cr.add_variant(hover);

    // Default state → base override
    let ao_default = cr.get_active_overrides();
    assert_eq!(ao_default[0].value, serde_json::json!("#007bff"));

    // Switch to hover → state override shadows base
    cr.set_state(VariantState::Hover);
    let ao_hover = cr.get_active_overrides();
    assert_eq!(ao_hover[0].value, serde_json::json!("#0056b3"));

    // Constraint interaction: hybrid doc + scale constraint
    let po = r(0.0, 0.0, 200.0, 200.0);
    let pn = r(0.0, 0.0, 400.0, 400.0);
    let child = r(40.0, 40.0, 120.0, 120.0);
    let res = resolve_constraints(po, pn, child, &Constraints::scale());
    assert_eq!(res.x, 80.0);
    assert_eq!(res.width, 240.0);
}
