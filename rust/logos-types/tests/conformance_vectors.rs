//! Shared conformance vectors executed by native Rust and Node/WASM tests.
use logos_types::{
    apply_operation, decode_and_validate_json, stable_import_id, DocumentOperation,
    ShapeRecord, Uuid, ValidationDiagnostic, ValidationDiagnosticCode,
};
use logos_layout::Rect;
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
    shared_react_operation_fixture_runs_deterministically();
}

fn shared_react_operation_fixture_runs_deterministically() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../logos-app/src/document-core/fixtures/g1-b8-operation-conformance-v1.json"
    )).unwrap();
    let names: Vec<_> = fixture["cases"].as_array().unwrap().iter()
        .map(|case| case["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"create-root-rectangle"));
    assert!(names.contains(&"create-root-frame"));
    assert!(names.contains(&"insert-child"));
    assert!(names.contains(&"rename"));
    assert!(names.contains(&"reorder-roots"));
    assert!(names.contains(&"reorder-siblings"));
    assert!(names.contains(&"delete-subtree"));
    assert!(names.contains(&"reparent-world-position"));
    assert!(names.contains(&"reject-cycle"));
    assert!(names.contains(&"reject-invalid-sibling-membership"));
    assert!(names.contains(&"reject-duplicate-id"));
    assert!(names.contains(&"reject-locked-mutation"));

    let baseline_json = serde_json::to_string(&fixture["rustBaseline"]).unwrap();
    let baseline = decode_and_validate_json(&baseline_json).unwrap();
    let page_id = baseline.pages[0].id;
    let frame_id = Uuid::parse_str("33333333-3333-4333-8333-333333333333").unwrap();
    let group_id = Uuid::parse_str("44444444-4444-4444-8444-444444444444").unwrap();
    let rect_id = Uuid::parse_str("55555555-5555-4555-8555-555555555555").unwrap();
    let root_rect_id = Uuid::parse_str("66666666-6666-4666-8666-666666666666").unwrap();
    let sibling_id = Uuid::parse_str("77777777-7777-4777-8777-777777777777").unwrap();

    let run_success = |operation: DocumentOperation| {
        let before = serde_json::to_value(&baseline).unwrap();
        let first = apply_operation(&baseline, &operation).unwrap();
        let second = apply_operation(&baseline, &operation).unwrap();
        assert_eq!(serde_json::to_value(&baseline).unwrap(), before, "input mutated");
        assert_eq!(first, second, "repeated operation diverged");
        assert!(logos_types::validate_document(&first).is_empty());
        first
    };

    let mut new_rect: Value = fixture["rustBaseline"]["shapes"].as_array().unwrap()
        .iter().find(|record| record["id"] == root_rect_id.to_string()).unwrap().clone();
    new_rect["id"] = json!("88888888-8888-4888-8888-888888888888");
    new_rect["name"] = json!("Created root rectangle");
    new_rect["parent-id"] = Value::Null;
    new_rect["frame-id"] = Value::Null;
    new_rect["shapes"] = json!([]);
    let created_rect: ShapeRecord = serde_json::from_value(new_rect.clone()).unwrap();
    let result = run_success(DocumentOperation::CreateShape { record: created_rect,
        parent_id: None, index: 0 });
    assert_eq!(result.pages[0].root_shape_ids[0].to_string(), "88888888-8888-4888-8888-888888888888");

    let mut new_frame = fixture["rustBaseline"]["shapes"].as_array().unwrap()
        .iter().find(|record| record["id"] == frame_id.to_string()).unwrap().clone();
    new_frame["id"] = json!("99999999-9999-4999-8999-999999999999");
    new_frame["name"] = json!("Created root frame");
    new_frame["shapes"] = json!([]);
    let created_frame: ShapeRecord = serde_json::from_value(new_frame).unwrap();
    let result = run_success(DocumentOperation::CreateShape { record: created_frame,
        parent_id: None, index: 0 });
    assert_eq!(result.pages[0].root_shape_ids[0].to_string(), "99999999-9999-4999-8999-999999999999");

    let mut child_record_json = fixture["rustBaseline"]["shapes"].as_array().unwrap()
        .iter().find(|record| record["id"] == rect_id.to_string()).unwrap().clone();
    child_record_json["id"] = json!("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa");
    child_record_json["parent-id"] = Value::Null;
    child_record_json["frame-id"] = Value::Null;
    child_record_json["shapes"] = json!([]);
    let child_record: ShapeRecord = serde_json::from_value(child_record_json).unwrap();
    let inserted = run_success(DocumentOperation::CreateShape { record: child_record,
        parent_id: Some(group_id), index: 0 });
    assert_eq!(inserted.shapes.iter().find(|record| record.shape.id == group_id).unwrap().shape.shapes[0].to_string(),
        "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa");

    let renamed = run_success(DocumentOperation::UpdateShape { shape_id: root_rect_id,
        expected_parent_id: None, name: Some("Renamed root".into()), selrect: None,
        transform: None, opacity: None });
    assert_eq!(renamed.shapes.iter().find(|record| record.shape.id == root_rect_id).unwrap().shape.name, "Renamed root");

    let reordered_roots = run_success(DocumentOperation::ReorderChildren { page_id,
        parent_id: None, mutation_id: None, expected_ids: vec![frame_id, root_rect_id], ordered_ids: vec![root_rect_id, frame_id] });
    assert_eq!(reordered_roots.pages[0].root_shape_ids, vec![root_rect_id, frame_id]);

    let reordered_children = run_success(DocumentOperation::ReorderChildren { page_id,
        parent_id: Some(frame_id), mutation_id: Some(sibling_id), expected_ids: vec![group_id, sibling_id], ordered_ids: vec![sibling_id, group_id] });
    assert_eq!(reordered_children.shapes.iter().find(|record| record.shape.id == frame_id).unwrap().shape.shapes,
        vec![sibling_id, group_id]);

    let deleted = run_success(DocumentOperation::DeleteSubtree { shape_id: group_id,
        expected_parent_id: Some(frame_id) });
    assert!(!deleted.shapes.iter().any(|record| record.shape.id == group_id || record.shape.id == rect_id));
    assert_eq!(deleted.shapes.iter().find(|record| record.shape.id == frame_id).unwrap().shape.shapes, vec![sibling_id]);

    let world_position_move = DocumentOperation::Sequence { operations: vec![
        DocumentOperation::ReparentShape { shape_id: rect_id, expected_parent_id: Some(group_id),
            new_parent_id: Some(frame_id), new_page_id: page_id, index: 2 },
        DocumentOperation::UpdateShape { shape_id: rect_id, expected_parent_id: Some(frame_id),
            name: None, selrect: Some(Rect::new(25.0, 35.0, 180.0, 90.0)),
            transform: None, opacity: None },
    ] };
    let moved = run_success(world_position_move);
    let moved_record = moved.shapes.iter().find(|record| record.shape.id == rect_id).unwrap();
    assert_eq!(moved_record.shape.parent_id, Some(frame_id));
    assert_eq!(moved_record.shape.selrect.x, 25.0);

    let before = serde_json::to_value(&baseline).unwrap();
    let cycle = apply_operation(&baseline, &DocumentOperation::ReparentShape { shape_id: frame_id,
        expected_parent_id: None, new_parent_id: Some(group_id), new_page_id: page_id, index: 0 }).unwrap_err();
    assert_eq!(cycle[0].code, ValidationDiagnosticCode::Cycle);
    assert_eq!(serde_json::to_value(&baseline).unwrap(), before);

    let invalid_order = apply_operation(&baseline, &DocumentOperation::ReorderChildren { page_id,
        parent_id: None, mutation_id: None, expected_ids: vec![frame_id, root_rect_id], ordered_ids: vec![frame_id, sibling_id] }).unwrap_err();
    assert_eq!(invalid_order[0].code, ValidationDiagnosticCode::InvalidSiblingOrder);
    assert_eq!(serde_json::to_value(&baseline).unwrap(), before);

    let duplicate: ShapeRecord = serde_json::from_value(fixture["rustBaseline"]["shapes"].as_array().unwrap()
        .iter().find(|record| record["id"] == root_rect_id.to_string()).unwrap().clone()).unwrap();
    let duplicate_result = apply_operation(&baseline, &DocumentOperation::CreateShape { record: duplicate,
        parent_id: None, index: 0 }).unwrap_err();
    assert_eq!(duplicate_result[0].code, ValidationDiagnosticCode::OperationPreconditionFailed);

    let mut locked = baseline.clone();
    locked.shapes.iter_mut().find(|record| record.shape.id == group_id).unwrap()
        .extensions.insert("react-locked".into(), json!(true));
    let locked_before = serde_json::to_value(&locked).unwrap();
    let locked_result = apply_operation(&locked, &DocumentOperation::UpdateShape { shape_id: group_id,
        expected_parent_id: Some(frame_id), name: Some("forbidden".into()), selrect: None,
        transform: None, opacity: None }).unwrap_err();
    assert_eq!(locked_result[0].code, ValidationDiagnosticCode::OperationPreconditionFailed);
    assert_eq!(locked_result[0].path, "/operation/locked");
    assert_eq!(serde_json::to_value(&locked).unwrap(), locked_before);

    let mut extensions = baseline.clone();
    extensions.extensions.insert("future-document-field".into(), json!({"stable": [1, 2]}));
    extensions.pages[0].extensions.insert("future-page-field".into(), json!("keep"));
    extensions.shapes.iter_mut().find(|record| record.shape.id == root_rect_id).unwrap()
        .extensions.insert("future-shape-field".into(), json!({"keep": true}));
    let preserved = apply_operation(&extensions, &DocumentOperation::UpdateShape { shape_id: root_rect_id,
        expected_parent_id: None, name: Some("Extension-safe".into()), selrect: None,
        transform: None, opacity: None }).unwrap();
    assert_eq!(preserved.extensions, extensions.extensions);
    assert_eq!(preserved.pages[0].extensions, extensions.pages[0].extensions);
    assert_eq!(preserved.shapes.iter().find(|record| record.shape.id == root_rect_id).unwrap().extensions,
        extensions.shapes.iter().find(|record| record.shape.id == root_rect_id).unwrap().extensions);
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
