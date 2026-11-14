use logos_layout::{Matrix, Point, Rect};
use logos_types::document_geometry::{
    local_to_world, object_to_new_parent, point_to_local, point_to_world, rect_to_space,
};

const GOLDEN: &str = include_str!("fixtures/g1-b5-transform-golden-v1.json");

fn matrix(value: &serde_json::Value) -> Matrix {
    let values = value.as_array().expect("six-component affine matrix");
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
    let values = value.as_array().expect("2D point");
    Point::new(values[0].as_f64().unwrap(), values[1].as_f64().unwrap())
}

fn close(actual: f64, expected: f64, tolerance: f64, label: &str) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{label}: {actual} != {expected} (tolerance {tolerance})"
    );
}

#[test]
fn shared_transform_golden_corners_inverse_bounds_and_reparent_match() {
    let fixture: serde_json::Value = serde_json::from_str(GOLDEN).unwrap();
    assert_eq!(fixture["schema"], "logos.g1-b5-transform-golden/v1");
    let tolerance = fixture["tolerance"].as_f64().unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 12, "all requested transform cases are present");

    for case in cases {
        let name = case["name"].as_str().unwrap();
        let world = matrix(&case["worldMatrix"]);
        let inverse = matrix(&case["inverseMatrix"]);
        let local_corners = case["localCorners"].as_array().unwrap();
        let world_corners = case["worldCorners"].as_array().unwrap();
        assert_eq!(local_corners.len(), 4, "{name}: local corners");
        assert_eq!(world_corners.len(), 4, "{name}: world corners");

        for (index, (local, expected_world)) in local_corners.iter().zip(world_corners).enumerate()
        {
            let actual_world = point_to_world(point(local), world).unwrap();
            let expected_world = point(expected_world);
            close(
                actual_world.x,
                expected_world.x,
                tolerance,
                &format!("{name} corner {index} x"),
            );
            close(
                actual_world.y,
                expected_world.y,
                tolerance,
                &format!("{name} corner {index} y"),
            );
            let round_trip = point_to_local(expected_world, world).unwrap();
            let local = point(local);
            close(
                round_trip.x,
                local.x,
                tolerance,
                &format!("{name} inverse x"),
            );
            close(
                round_trip.y,
                local.y,
                tolerance,
                &format!("{name} inverse y"),
            );
            assert!(
                (world * inverse).close_to(&Matrix::identity(), tolerance),
                "{name}: inverse matrix"
            );
        }

        let bounds = &case["localBounds"];
        let local_rect = Rect::new(
            bounds[0].as_f64().unwrap(),
            bounds[1].as_f64().unwrap(),
            bounds[2].as_f64().unwrap(),
            bounds[3].as_f64().unwrap(),
        );
        let actual_aabb = rect_to_space(local_rect, world).unwrap();
        let expected_aabb = &case["expectedBoundingBox"];
        for (actual, index, label) in [
            (actual_aabb.x, 0, "x"),
            (actual_aabb.y, 1, "y"),
            (actual_aabb.width, 2, "width"),
            (actual_aabb.height, 3, "height"),
        ] {
            close(
                actual,
                expected_aabb[index].as_f64().unwrap(),
                tolerance,
                &format!("{name} bbox {label}"),
            );
        }

        if case.get("oldParentWorld").is_some() {
            let old_world = matrix(&case["oldParentWorld"]);
            let object_local = matrix(&case["objectToOldParent"]);
            let new_world = matrix(&case["newParentWorld"]);
            let expected_local = matrix(&case["expectedNewLocal"]);
            let actual_local = object_to_new_parent(old_world * object_local, new_world).unwrap();
            assert!(
                actual_local.close_to(&expected_local, tolerance),
                "{name}: reparent local"
            );
            assert!(
                (new_world * actual_local).close_to(&(old_world * object_local), tolerance),
                "{name}: world corners are preserved by reparent"
            );
        }
    }

    let nested = cases
        .iter()
        .find(|case| case["name"] == "deep-mixed-parents")
        .unwrap();
    let chain = nested["matrixChain"]
        .as_array()
        .unwrap()
        .iter()
        .map(matrix)
        .collect::<Vec<_>>();
    assert!(
        local_to_world(&chain)
            .unwrap()
            .close_to(&matrix(&nested["worldMatrix"]), tolerance),
        "deep nested root-to-leaf matrix order"
    );
}
