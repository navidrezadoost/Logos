use logos_layout::{Matrix, Point};
use logos_types::document_geometry::{
    local_to_world, object_to_new_parent, point_to_world, CoordinateError,
};

const GOLDEN: &str = include_str!("fixtures/g1-b4-coordinate-golden-v1.json");

fn matrix(value: &serde_json::Value) -> Matrix {
    let values = value.as_array().expect("matrix array");
    assert_eq!(values.len(), 6);
    Matrix::new(
        values[0].as_f64().unwrap(),
        values[1].as_f64().unwrap(),
        values[2].as_f64().unwrap(),
        values[3].as_f64().unwrap(),
        values[4].as_f64().unwrap(),
        values[5].as_f64().unwrap(),
    )
}

fn point(value: &serde_json::Value) -> Point {
    let values = value.as_array().expect("point array");
    Point::new(values[0].as_f64().unwrap(), values[1].as_f64().unwrap())
}

#[test]
fn shared_coordinate_golden_vectors_match_native_primitives() {
    let document: serde_json::Value = serde_json::from_str(GOLDEN).unwrap();
    let tolerance = document["tolerance"].as_f64().unwrap();
    let cases = document["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 12, "all required named cases must execute");

    for case in cases {
        let name = case["name"].as_str().unwrap();
        let chain = case["chain"]
            .as_array()
            .unwrap()
            .iter()
            .map(matrix)
            .collect::<Vec<_>>();

        if case.get("error").is_some() {
            assert_eq!(
                local_to_world(&chain)
                    .unwrap()
                    .inverse()
                    .ok_or(CoordinateError::SingularMatrix),
                Err(CoordinateError::SingularMatrix),
                "{name}"
            );
            continue;
        }

        let world = local_to_world(&chain).unwrap();
        let expected_world = matrix(&case["worldMatrix"]);
        assert!(world.close_to(&expected_world, tolerance), "{name}: matrix");
        let mapped = point_to_world(point(&case["point"]), world).unwrap();
        let expected_point = point(&case["worldPoint"]);
        assert!(
            (mapped.x - expected_point.x).abs() <= tolerance,
            "{name}: x"
        );
        assert!(
            (mapped.y - expected_point.y).abs() <= tolerance,
            "{name}: y"
        );
        let inverse = world.inverse().unwrap();
        assert!(
            inverse.close_to(&matrix(&case["inverse"]), tolerance),
            "{name}: inverse"
        );

        if let Some(new_parent) = case.get("newParent") {
            let converted = object_to_new_parent(world, matrix(new_parent)).unwrap();
            assert!(
                converted.close_to(&matrix(&case["expectedNewLocal"]), tolerance),
                "{name}: reparent conversion"
            );
        }
    }
}
