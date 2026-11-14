//! Shared conformance vectors executed by native Rust and Node/WASM tests.
use logos_types::{
    apply_operation, decode_and_validate_json, stable_import_id, DocumentOperation, Uuid,
    ValidationDiagnostic, ValidationDiagnosticCode,
};
use serde_json::{json, Value};

const INPUT: &str = include_str!("fixtures/g1-b1/rust-contract-candidate.json");
const NORMALIZED: &str = include_str!("fixtures/g1-b1/rust-contract-candidate-normalized.json");
const NESTED: &str = include_str!("fixtures/g1-b1/nested-component-group.json");

pub fn run_shared_vectors() {
    valid_document_full_normalized_output();
    invalid_graph_full_diagnostics();
    unsupported_schema_full_diagnostics();
    malformed_schema_full_diagnostics();
    transform_inverse_full_diagnostics();
    stable_import_id_fixed_vector();
    deep_frame_group_membership_full_output();
    cross_page_reparent_full_output();
    dependent_delete_rejected_without_mutation();
}

fn valid_document_full_normalized_output() {
    let document = decode_and_validate_json(INPUT).unwrap();
    let actual = serde_json::to_value(document).unwrap();
    let expected: Value = serde_json::from_str(NORMALIZED).unwrap();
    assert_eq!(actual, expected);
}

fn invalid_graph_full_diagnostics() {
    let invalid = include_str!("fixtures/g1-b1/invalid-page-graph.json");
    let diagnostics = decode_and_validate_json(invalid).unwrap_err();
    let actual = serde_json::to_value(diagnostics).unwrap();
    let expected = json!([{
        "code": "DUPLICATE_ROOT_MEMBERSHIP",
        "path": "/pages/0/root-shape-ids/1",
        "objectId": "cccccccc-cccc-4ccc-8ccc-cccccccccccc"
    }]);
    assert_eq!(actual, expected);
}

fn unsupported_schema_full_diagnostics() {
    let input = INPUT.replace("\"schema-version\": 1", "\"schema-version\": 99");
    let diagnostics = decode_and_validate_json(&input).unwrap_err();
    let actual = serde_json::to_value(diagnostics).unwrap();
    let expected = json!([{
        "code": "UNSUPPORTED_SCHEMA_VERSION",
        "path": "/schema-version"
    }]);
    assert_eq!(actual, expected);
}

fn malformed_schema_full_diagnostics() {
    let diagnostics = decode_and_validate_json("{\"schema-version\":").unwrap_err();
    let actual = serde_json::to_value(diagnostics).unwrap();
    assert_eq!(
        actual,
        json!([{"code":"DOCUMENT_SCHEMA_INVALID", "path":"$"}])
    );
}

fn transform_inverse_full_diagnostics() {
    let mut input: Value = serde_json::from_str(INPUT).unwrap();
    input["shapes"][0]["transform-inverse"]["e"] = json!(12.0);
    let diagnostics = decode_and_validate_json(&input.to_string()).unwrap_err();
    let actual = serde_json::to_value(diagnostics).unwrap();
    assert_eq!(
        actual,
        json!([{
            "code": "TRANSFORM_INVERSE_MISMATCH",
            "path": "/shapes/0/transform-inverse",
            "objectId": "33333333-3333-4333-8333-333333333333"
        }])
    );
}

fn stable_import_id_fixed_vector() {
    let actual = stable_import_id(Uuid::NAMESPACE_DNS, "xd", "www.widgets.com");
    assert_eq!(actual.to_string(), "a83792ea-2a6e-586f-84a6-b87d00a2f765");
}

fn deep_frame_group_membership_full_output() {
    let mut expected: Value = serde_json::from_str(NESTED).unwrap();
    // The shared nested fixture is shaped Component -> Group -> Rect. Turn its
    // root into a Frame and assign the nearest-frame membership through Group.
    expected["shapes"][0]["type"] = json!("frame");
    expected["shapes"][1]["frame-id"] = expected["shapes"][0]["id"].clone();
    expected["shapes"][2]["frame-id"] = expected["shapes"][0]["id"].clone();
    add_shape_defaults(&mut expected);
    let document = decode_and_validate_json(&expected.to_string()).unwrap();
    assert_eq!(serde_json::to_value(document).unwrap(), expected);
}

fn cross_page_reparent_full_output() {
    let mut source: Value = serde_json::from_str(NESTED).unwrap();
    let second_page_id = "ffffffff-ffff-4fff-8fff-ffffffffffff";
    source["pages"].as_array_mut().unwrap().push(json!({
        "id": second_page_id, "name": "Destination", "root-shape-ids": []
    }));
    let before = decode_and_validate_json(&source.to_string()).unwrap();
    let root_id = Uuid::parse_str("cccccccc-cccc-4ccc-8ccc-cccccccccccc").unwrap();
    let destination = Uuid::parse_str(second_page_id).unwrap();
    let result = apply_operation(
        &before,
        &DocumentOperation::ReparentShape {
            shape_id: root_id,
            expected_parent_id: None,
            new_parent_id: None,
            new_page_id: destination,
            index: 0,
        },
    )
    .unwrap();

    let mut expected = source;
    expected["pages"][0]["root-shape-ids"] = json!([]);
    expected["pages"][1]["root-shape-ids"] = json!([root_id.to_string()]);
    for record in expected["shapes"].as_array_mut().unwrap() {
        record["page-id"] = json!(second_page_id);
    }
    let shapes = expected["shapes"].as_array_mut().unwrap();
    let root = shapes.remove(0);
    shapes.push(root);
    add_shape_defaults(&mut expected);
    assert_eq!(serde_json::to_value(result).unwrap(), expected);
}

fn add_shape_defaults(document: &mut Value) {
    for shape in document["shapes"].as_array_mut().unwrap() {
        let defaults = json!({
            "hidden": false, "opacity": 1.0, "masked-group": false,
            "constraints-h": "left", "constraints-v": "top",
            "fixed-scroll": false, "component-root": false,
            "main-instance": false, "rotation": 0.0
        });
        for (key, value) in defaults.as_object().unwrap() {
            shape
                .as_object_mut()
                .unwrap()
                .entry(key.clone())
                .or_insert(value.clone());
        }
        for key in ["x", "y", "width", "height"] {
            let number = shape["selrect"][key].as_f64().unwrap();
            shape["selrect"][key] = json!(number);
        }
        for key in ["a", "b", "c", "d", "e", "f"] {
            for transform in ["transform", "transform-inverse"] {
                let number = shape[transform][key].as_f64().unwrap();
                shape[transform][key] = json!(number);
            }
        }
    }
}

fn dependent_delete_rejected_without_mutation() {
    let mut input: Value = serde_json::from_str(NESTED).unwrap();
    let mut instance = input["shapes"][2].clone();
    instance["id"] = json!("99999999-9999-4999-8999-999999999999");
    instance["name"] = json!("Dependent instance");
    instance.as_object_mut().unwrap().remove("parent-id");
    instance["component-id"] = input["shapes"][0]["id"].clone();
    input["pages"][0]["root-shape-ids"]
        .as_array_mut()
        .unwrap()
        .push(instance["id"].clone());
    input["shapes"].as_array_mut().unwrap().push(instance);
    let before = decode_and_validate_json(&input.to_string()).unwrap();
    let snapshot = serde_json::to_value(&before).unwrap();
    let root_id = Uuid::parse_str("cccccccc-cccc-4ccc-8ccc-cccccccccccc").unwrap();
    let error = apply_operation(
        &before,
        &DocumentOperation::DeleteSubtree {
            shape_id: root_id,
            expected_parent_id: None,
        },
    )
    .unwrap_err();
    assert_eq!(serde_json::to_value(&before).unwrap(), snapshot);
    let actual = serde_json::to_value(error).unwrap();
    let expected = serde_json::to_value(vec![ValidationDiagnostic {
        code: ValidationDiagnosticCode::InvalidComponentReference,
        path: "/shapes/0/component-id".into(),
        object_id: Some(Uuid::parse_str("99999999-9999-4999-8999-999999999999").unwrap()),
        related_id: Some(root_id),
    }])
    .unwrap();
    assert_eq!(actual, expected);
}
