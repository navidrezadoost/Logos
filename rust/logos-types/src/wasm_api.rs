use wasm_bindgen::prelude::*;

use crate::document::decode_and_validate_json;

/// Validate a serialized candidate document without retaining or mutating it.
/// The JSON result is `{ ok, document?, diagnostics }` and keeps the existing
/// Rust document module as the sole semantic validator.
#[wasm_bindgen]
pub fn validate_document_json(input: &str) -> String {
    let result = match decode_and_validate_json(input) {
        Ok(document) => serde_json::json!({
            "ok": true,
            "document": document,
            "diagnostics": [],
        }),
        Err(diagnostics) => serde_json::json!({
            "ok": false,
            "diagnostics": diagnostics,
        }),
    };
    result.to_string()
}

#[cfg(all(test, target_arch = "wasm32"))]
mod tests {
    use wasm_bindgen_test::wasm_bindgen_test;

    use super::validate_document_json;
    use crate::document_geometry::{
        local_to_world, object_to_new_parent, point_to_local, point_to_world, rect_to_space,
    };
    use logos_layout::{Matrix, Point, Rect};

    const VALID: &str =
        include_str!("../../../logos-app/src/document-core/fixtures/rust-candidate-v1.json");

    #[wasm_bindgen_test]
    fn browser_abi_accepts_the_shared_react_adapter_candidate() {
        let response: serde_json::Value =
            serde_json::from_str(&validate_document_json(VALID)).unwrap();
        assert_eq!(response["ok"], true);
        assert_eq!(response["document"]["shapes"].as_array().unwrap().len(), 4);
        assert_eq!(response["diagnostics"], serde_json::json!([]));
    }

    #[wasm_bindgen_test]
    fn browser_abi_returns_rust_parent_child_diagnostic_for_invalid_candidate() {
        let mut candidate: serde_json::Value = serde_json::from_str(VALID).unwrap();
        candidate["shapes"][1]["shapes"] = serde_json::json!([]);
        let response: serde_json::Value =
            serde_json::from_str(&validate_document_json(&candidate.to_string())).unwrap();
        assert_eq!(response["ok"], false);
        assert!(response["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| { item["code"] == "PARENT_CHILD_MISMATCH" }));
    }

    #[wasm_bindgen_test]
    fn browser_executes_shared_g1_b4_coordinate_golden_vectors() {
        let vectors: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/g1-b4-coordinate-golden-v1.json"
        ))
        .unwrap();
        let tolerance = vectors["tolerance"].as_f64().unwrap();
        let cases = vectors["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 12);
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let chain = case["chain"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| {
                    let values = value.as_array().unwrap();
                    Matrix::new(
                        values[0].as_f64().unwrap(),
                        values[1].as_f64().unwrap(),
                        values[2].as_f64().unwrap(),
                        values[3].as_f64().unwrap(),
                        values[4].as_f64().unwrap(),
                        values[5].as_f64().unwrap(),
                    )
                })
                .collect::<Vec<_>>();
            if case.get("error").is_some() {
                assert!(
                    local_to_world(&chain).unwrap().inverse().is_none(),
                    "{name}"
                );
                continue;
            }
            let world = local_to_world(&chain).unwrap();
            let point = &case["point"];
            let actual = point_to_world(
                Point::new(point[0].as_f64().unwrap(), point[1].as_f64().unwrap()),
                world,
            )
            .unwrap();
            assert!(
                (actual.x - case["worldPoint"][0].as_f64().unwrap()).abs() <= tolerance,
                "{name}: x"
            );
            assert!(
                (actual.y - case["worldPoint"][1].as_f64().unwrap()).abs() <= tolerance,
                "{name}: y"
            );
            if case.get("newParent").is_some() {
                let parent = &case["newParent"];
                let values = parent.as_array().unwrap();
                let parent = Matrix::new(
                    values[0].as_f64().unwrap(),
                    values[1].as_f64().unwrap(),
                    values[2].as_f64().unwrap(),
                    values[3].as_f64().unwrap(),
                    values[4].as_f64().unwrap(),
                    values[5].as_f64().unwrap(),
                );
                let result = object_to_new_parent(world, parent).unwrap();
                let expected = &case["expectedNewLocal"];
                let values = expected.as_array().unwrap();
                let expected = Matrix::new(
                    values[0].as_f64().unwrap(),
                    values[1].as_f64().unwrap(),
                    values[2].as_f64().unwrap(),
                    values[3].as_f64().unwrap(),
                    values[4].as_f64().unwrap(),
                    values[5].as_f64().unwrap(),
                );
                assert!(result.close_to(&expected, tolerance), "{name}: reparent");
            }
        }
    }

    #[wasm_bindgen_test]
    fn browser_executes_g1_b5_transform_corners_bounds_and_reparent_vectors() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/g1-b5-transform-golden-v1.json"
        ))
        .unwrap();
        let tolerance = fixture["tolerance"].as_f64().unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 12);

        let matrix = |value: &serde_json::Value| {
            let values = value.as_array().unwrap();
            Matrix::new(
                values[0].as_f64().unwrap(),
                values[1].as_f64().unwrap(),
                values[2].as_f64().unwrap(),
                values[3].as_f64().unwrap(),
                values[4].as_f64().unwrap(),
                values[5].as_f64().unwrap(),
            )
        };
        let point = |value: &serde_json::Value| {
            Point::new(value[0].as_f64().unwrap(), value[1].as_f64().unwrap())
        };

        for case in cases {
            let name = case["name"].as_str().unwrap();
            let world = matrix(&case["worldMatrix"]);
            let inverse = matrix(&case["inverseMatrix"]);
            assert!(
                (world * inverse).close_to(&Matrix::identity(), tolerance),
                "{name}: inverse"
            );
            for (local, expected) in case["localCorners"]
                .as_array()
                .unwrap()
                .iter()
                .zip(case["worldCorners"].as_array().unwrap())
            {
                let mapped = point_to_world(point(local), world).unwrap();
                let expected = point(expected);
                assert!((mapped.x - expected.x).abs() <= tolerance, "{name}: x");
                assert!((mapped.y - expected.y).abs() <= tolerance, "{name}: y");
                let round_trip = point_to_local(expected, world).unwrap();
                let local = point(local);
                assert!(
                    (round_trip.x - local.x).abs() <= tolerance,
                    "{name}: inverse x"
                );
                assert!(
                    (round_trip.y - local.y).abs() <= tolerance,
                    "{name}: inverse y"
                );
            }

            let bounds = &case["localBounds"];
            let actual = rect_to_space(
                Rect::new(
                    bounds[0].as_f64().unwrap(),
                    bounds[1].as_f64().unwrap(),
                    bounds[2].as_f64().unwrap(),
                    bounds[3].as_f64().unwrap(),
                ),
                world,
            )
            .unwrap();
            for (actual, index) in [
                (actual.x, 0),
                (actual.y, 1),
                (actual.width, 2),
                (actual.height, 3),
            ] {
                assert!(
                    (actual - case["expectedBoundingBox"][index].as_f64().unwrap()).abs()
                        <= tolerance,
                    "{name}: bounds"
                );
            }

            if case.get("oldParentWorld").is_some() {
                let converted = object_to_new_parent(
                    matrix(&case["oldParentWorld"]) * matrix(&case["objectToOldParent"]),
                    matrix(&case["newParentWorld"]),
                )
                .unwrap();
                assert!(
                    converted.close_to(&matrix(&case["expectedNewLocal"]), tolerance),
                    "{name}: reparent"
                );
            }
            if let Some(chain) = case.get("matrixChain") {
                let chain = chain
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(matrix)
                    .collect::<Vec<_>>();
                assert!(
                    local_to_world(&chain).unwrap().close_to(&world, tolerance),
                    "{name}: root-to-leaf composition"
                );
            }
        }
    }
}
