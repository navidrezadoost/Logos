use logos_types::document::{decode_and_validate_json, ValidationDiagnosticCode};

const CANDIDATE: &str = include_str!(
    "../../../logos-app/src/document-core/fixtures/rust-candidate-v1.json"
);

#[test]
fn g1_b3_react_supported_subset_candidate_is_valid_and_retains_mapped_semantics() {
    let document = decode_and_validate_json(CANDIDATE).expect("G1-B3 adapter fixture should validate");
    assert_eq!(document.pages.len(), 1);
    assert_eq!(document.pages[0].root_shape_ids.len(), 2);
    assert_eq!(document.shapes.len(), 4);

    let frame = &document.shapes[0];
    let group = &document.shapes[1];
    let rectangle = &document.shapes[2];
    assert_eq!(frame.shape.shapes, vec![group.shape.id]);
    assert_eq!(group.shape.parent_id, Some(frame.shape.id));
    assert_eq!(group.shape.frame_id, Some(frame.shape.id));
    assert_eq!(rectangle.shape.parent_id, Some(group.shape.id));
    assert_eq!(rectangle.shape.frame_id, Some(frame.shape.id));
    assert_eq!(group.extensions.get("react-locked"), Some(&serde_json::Value::Bool(true)));
    assert_eq!(rectangle.shape.rotation, 12.0);
    assert_eq!(rectangle.shape.fills.len(), 1);
    assert!(document.extensions.is_empty());
}

#[test]
fn g1_b3_fixture_rejects_a_broken_parent_child_backlink() {
    let mut value: serde_json::Value = serde_json::from_str(CANDIDATE).unwrap();
    value["shapes"][1]["shapes"] = serde_json::json!([]);
    let result = decode_and_validate_json(&value.to_string());
    let diagnostics = result.expect_err("broken backlink must be rejected");
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == ValidationDiagnosticCode::ParentChildMismatch
    }));
}
