//! End-to-end migration integration tests using in-memory fixture builders.

use logos_import_figma::fixtures::{generate_test_fig, TestFixture};
use logos_import_xd::archive::build_test_xd;
use logos_import_xd::model::XdNode;
use logos_import_sketch::archive::build_test_sketch;
use logos_import_sketch::model::{SketchLayer, SketchFrame};
use logos_migrate::wizard::{MigrationWizard, SourceFormat, WizardConfig};

// ── helpers ──────────────────────────────────────────────────────────────────

fn minimal_xd_bytes() -> Vec<u8> {
    let node = XdNode {
        id: "rect-1".into(),
        name: "Rectangle".into(),
        node_type: "shape".into(),
        shape_type: "rect".into(),
        visible: true,
        opacity: 1.0,
        ..Default::default()
    };
    build_test_xd(&[node], "Artboard 1")
}

fn minimal_sketch_bytes() -> Vec<u8> {
    let layer = SketchLayer {
        id: "layer-1".into(),
        name: "Rectangle".into(),
        class: "rectangle".into(),
        isVisible: true,
        opacity: 1.0,
        frame: SketchFrame { x: 0.0, y: 0.0, width: 100.0, height: 100.0, class: "rect".into() },
        ..Default::default()
    };
    build_test_sketch(&[layer])
}

fn minimal_figma_bytes() -> Vec<u8> {
    generate_test_fig(TestFixture::SingleRectangle)
}

// ── XD end-to-end ────────────────────────────────────────────────────────────

#[test]
fn xd_migration_succeeds() {
    let data = minimal_xd_bytes();
    let result = MigrationWizard::new().migrate_bytes(&data, SourceFormat::AdobeXd).unwrap();
    assert!(result.snapshot.is_current_schema());
}

#[test]
fn xd_report_has_no_errors() {
    let data = minimal_xd_bytes();
    let result = MigrationWizard::new().migrate_bytes(&data, SourceFormat::AdobeXd).unwrap();
    assert_eq!(result.report.errors, 0);
}

#[test]
fn xd_snapshot_serialises_to_json() {
    let data = minimal_xd_bytes();
    let result = MigrationWizard::new().migrate_bytes(&data, SourceFormat::AdobeXd).unwrap();
    let json = result.snapshot.to_json().unwrap();
    assert!(json.contains("schema_version"));
}

#[test]
fn xd_snapshot_round_trips() {
    let data = minimal_xd_bytes();
    let result = MigrationWizard::new().migrate_bytes(&data, SourceFormat::AdobeXd).unwrap();
    let json = result.snapshot.to_json().unwrap();
    let restored = logos_core::persistence::DocumentSnapshot::from_json(&json).unwrap();
    assert!(restored.is_current_schema());
}

#[test]
fn xd_report_source_format_is_adobe_xd() {
    let data = minimal_xd_bytes();
    let result = MigrationWizard::new().migrate_bytes(&data, SourceFormat::AdobeXd).unwrap();
    assert_eq!(result.report.source_format, "Adobe XD");
}

#[test]
fn xd_preview_is_parseable() {
    let data = minimal_xd_bytes();
    let wizard = MigrationWizard::new();
    let preview = wizard.preview(&data, SourceFormat::AdobeXd);
    assert!(preview.is_parseable);
}

#[test]
fn xd_migrate_result_format_matches() {
    let data = minimal_xd_bytes();
    let result = MigrationWizard::new().migrate_bytes(&data, SourceFormat::AdobeXd).unwrap();
    assert_eq!(result.source_format, SourceFormat::AdobeXd);
}

#[test]
fn xd_with_no_layer_limit_no_warning() {
    let data = minimal_xd_bytes();
    let cfg = WizardConfig { layer_limit: 0, ..Default::default() };
    let wizard = MigrationWizard::with_config(cfg);
    let result = wizard.migrate_bytes(&data, SourceFormat::AdobeXd).unwrap();
    assert_eq!(result.report.warnings, 0);
}

#[test]
fn xd_convert_multiple_nodes() {
    let nodes = vec![
        XdNode { id: "r1".into(), name: "Rect1".into(), node_type: "shape".into(),
            shape_type: "rect".into(), visible: true, opacity: 1.0, ..Default::default() },
        XdNode { id: "r2".into(), name: "Rect2".into(), node_type: "shape".into(),
            shape_type: "rect".into(), visible: true, opacity: 1.0, ..Default::default() },
    ];
    let data = build_test_xd(&nodes, "Board");
    let result = MigrationWizard::new().migrate_bytes(&data, SourceFormat::AdobeXd).unwrap();
    assert!(result.snapshot.is_current_schema());
}

// ── Sketch end-to-end ────────────────────────────────────────────────────────

#[test]
fn sketch_migration_succeeds() {
    let data = minimal_sketch_bytes();
    let result = MigrationWizard::new().migrate_bytes(&data, SourceFormat::Sketch).unwrap();
    assert!(result.snapshot.is_current_schema());
}

#[test]
fn sketch_report_has_no_errors() {
    let data = minimal_sketch_bytes();
    let result = MigrationWizard::new().migrate_bytes(&data, SourceFormat::Sketch).unwrap();
    assert_eq!(result.report.errors, 0);
}

#[test]
fn sketch_snapshot_serialises_to_json() {
    let data = minimal_sketch_bytes();
    let result = MigrationWizard::new().migrate_bytes(&data, SourceFormat::Sketch).unwrap();
    let json = result.snapshot.to_json().unwrap();
    assert!(json.contains("schema_version"));
}

#[test]
fn sketch_snapshot_round_trips() {
    let data = minimal_sketch_bytes();
    let result = MigrationWizard::new().migrate_bytes(&data, SourceFormat::Sketch).unwrap();
    let json = result.snapshot.to_json().unwrap();
    let restored = logos_core::persistence::DocumentSnapshot::from_json(&json).unwrap();
    assert!(restored.is_current_schema());
}

#[test]
fn sketch_report_source_format_is_sketch() {
    let data = minimal_sketch_bytes();
    let result = MigrationWizard::new().migrate_bytes(&data, SourceFormat::Sketch).unwrap();
    assert_eq!(result.report.source_format, "Sketch");
}

#[test]
fn sketch_preview_is_parseable() {
    let data = minimal_sketch_bytes();
    let wizard = MigrationWizard::new();
    let preview = wizard.preview(&data, SourceFormat::Sketch);
    assert!(preview.is_parseable);
}

#[test]
fn sketch_migrate_result_format_matches() {
    let data = minimal_sketch_bytes();
    let result = MigrationWizard::new().migrate_bytes(&data, SourceFormat::Sketch).unwrap();
    assert_eq!(result.source_format, SourceFormat::Sketch);
}

#[test]
fn sketch_convert_multiple_layers() {
    let layers = vec![
        SketchLayer { id: "l1".into(), name: "L1".into(), class: "rectangle".into(),
            isVisible: true, opacity: 1.0,
            frame: SketchFrame { x: 0.0, y: 0.0, width: 50.0, height: 50.0, class: "rect".into() },
            ..Default::default() },
        SketchLayer { id: "l2".into(), name: "L2".into(), class: "oval".into(),
            isVisible: true, opacity: 1.0,
            frame: SketchFrame { x: 60.0, y: 0.0, width: 50.0, height: 50.0, class: "rect".into() },
            ..Default::default() },
    ];
    let data = build_test_sketch(&layers);
    let result = MigrationWizard::new().migrate_bytes(&data, SourceFormat::Sketch).unwrap();
    assert!(result.snapshot.is_current_schema());
}

// ── Figma end-to-end ─────────────────────────────────────────────────────────

#[test]
fn figma_migration_succeeds() {
    let data = minimal_figma_bytes();
    let result = MigrationWizard::new().migrate_bytes(&data, SourceFormat::Figma).unwrap();
    assert!(result.snapshot.is_current_schema());
}

#[test]
fn figma_report_has_no_errors() {
    let data = minimal_figma_bytes();
    let result = MigrationWizard::new().migrate_bytes(&data, SourceFormat::Figma).unwrap();
    assert_eq!(result.report.errors, 0);
}

#[test]
fn figma_snapshot_serialises_to_json() {
    let data = minimal_figma_bytes();
    let result = MigrationWizard::new().migrate_bytes(&data, SourceFormat::Figma).unwrap();
    let json = result.snapshot.to_json().unwrap();
    assert!(json.contains("schema_version"));
}

#[test]
fn figma_snapshot_round_trips() {
    let data = minimal_figma_bytes();
    let result = MigrationWizard::new().migrate_bytes(&data, SourceFormat::Figma).unwrap();
    let json = result.snapshot.to_json().unwrap();
    let restored = logos_core::persistence::DocumentSnapshot::from_json(&json).unwrap();
    assert!(restored.is_current_schema());
}

#[test]
fn figma_report_source_format_is_figma() {
    let data = minimal_figma_bytes();
    let result = MigrationWizard::new().migrate_bytes(&data, SourceFormat::Figma).unwrap();
    assert_eq!(result.report.source_format, "Figma");
}

#[test]
fn figma_preview_is_parseable() {
    let data = minimal_figma_bytes();
    let wizard = MigrationWizard::new();
    let preview = wizard.preview(&data, SourceFormat::Figma);
    assert!(preview.is_parseable);
}

#[test]
fn figma_migrate_result_format_matches() {
    let data = minimal_figma_bytes();
    let result = MigrationWizard::new().migrate_bytes(&data, SourceFormat::Figma).unwrap();
    assert_eq!(result.source_format, SourceFormat::Figma);
}

#[test]
fn figma_basic_shapes_fixture_migrates() {
    let data = generate_test_fig(TestFixture::BasicShapes);
    let result = MigrationWizard::new().migrate_bytes(&data, SourceFormat::Figma).unwrap();
    assert!(result.snapshot.is_current_schema());
}

#[test]
fn figma_mobile_app_fixture_migrates() {
    let data = generate_test_fig(TestFixture::MobileAppScreen);
    let result = MigrationWizard::new().migrate_bytes(&data, SourceFormat::Figma).unwrap();
    assert!(result.snapshot.is_current_schema());
}
