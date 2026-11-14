//! G0-B serialization compatibility characterization for the current Rust DTO.

use logos_types::Shape;

#[test]
fn react_shape_projection_is_not_directly_deserializable_as_rust_domain_shape() {
    let fixture = include_str!("../../../test/fixtures/g0-b/react-shape-projection.json");
    let result = serde_json::from_str::<Shape>(fixture);
    assert!(result.is_err(), "a React projection is not the Rust wire contract");
}

#[test]
fn unknown_extension_fields_are_accepted_but_dropped_on_rust_round_trip() {
    let fixture = include_str!("../../../test/fixtures/p0-document-compat/rust-domain-shape.json");
    let mut value: serde_json::Value = serde_json::from_str(fixture).unwrap();
    value["future-extension"] = serde_json::json!({"opaque": [1, 2, 3]});
    let shape: Shape = serde_json::from_value(value).expect("Serde ignores unknown fields by default");
    let round_trip = serde_json::to_value(shape).unwrap();
    assert!(round_trip.get("future-extension").is_none());
}
