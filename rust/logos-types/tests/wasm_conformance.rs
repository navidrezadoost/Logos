#![cfg(all(target_arch = "wasm32", feature = "wasm"))]

use logos_types::{decode_and_validate_json, stable_import_id, Uuid};
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_node);

#[wasm_bindgen_test]
fn candidate_golden_decodes_to_same_stable_identity_and_geometry() {
    let fixture = include_str!("fixtures/g1-b1/rust-contract-candidate.json");
    let candidate = decode_and_validate_json(fixture).expect("candidate fixture validates");
    assert_eq!(
        candidate.id,
        Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap()
    );
    assert_eq!(
        candidate.shapes[0].shape.id,
        Uuid::parse_str("33333333-3333-4333-8333-333333333333").unwrap()
    );
    assert_eq!(candidate.shapes[0].shape.selrect.width, 30.0);
}

#[wasm_bindgen_test]
fn stable_import_identity_uses_the_same_uuidv5_vector() {
    let result = stable_import_id(Uuid::NAMESPACE_DNS, "xd", "www.widgets.com");
    assert_eq!(
        result,
        stable_import_id(Uuid::NAMESPACE_DNS, "xd", "www.widgets.com")
    );
    assert_eq!(
        Uuid::new_v5(&Uuid::NAMESPACE_DNS, b"www.widgets.com").to_string(),
        "21f7f8de-8051-5b89-8680-0195ef798b6a"
    );
}

#[wasm_bindgen_test]
fn malformed_candidate_diagnostic_is_stable_on_wasm() {
    let invalid = r#"{"id":"bad","schema-version":1,"pages":[],"shapes":[]}"#;
    let first = decode_and_validate_json(invalid).unwrap_err();
    let second = decode_and_validate_json(invalid).unwrap_err();
    assert_eq!(first, second);
    assert_eq!(first[0].path, "/id");
}
