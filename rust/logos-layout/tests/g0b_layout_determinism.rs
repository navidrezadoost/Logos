//! G0-B characterization: the current pure Flex calculation is deterministic
//! for an identical ordered input. This does not claim parity with render-wasm.

use logos_layout::{
    compute_positions, AlignSelf, ChildLayoutData, FlexContainer, FlexDirection,
    FlexWrap,
};

fn inputs() -> (FlexContainer, Vec<(u64, ChildLayoutData)>) {
    let container = FlexContainer {
        direction: FlexDirection::Row,
        wrap: FlexWrap::NoWrap,
        ..Default::default()
    };
    let child = ChildLayoutData {
        main_min: 40.0,
        main_max: f64::INFINITY,
        main_fill: false,
        main_auto: false,
        cross_min: 12.0,
        cross_max: f64::INFINITY,
        cross_fill: false,
        cross_auto: false,
        width: Some(40.0),
        height: Some(12.0),
        flex_grow: 0.0,
        flex_shrink: 1.0,
        flex_basis: Some(40.0),
        align_self: AlignSelf::Auto,
        absolute: false,
    };
    (container, vec![(7, child)])
}

#[test]
fn identical_flex_input_has_identical_geometry_and_order() {
    let (container, children) = inputs();
    let first = compute_positions(&container, &children, 100.0, 50.0);
    let second = compute_positions(&container, &children, 100.0, 50.0);

    assert_eq!(first, second);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].children.len(), 1);
    assert_eq!(first[0].children[0].id, 7);
    assert_eq!(first[0].children[0].x, 0.0);
    assert_eq!(first[0].children[0].y, 0.0);
    assert_eq!(first[0].children[0].width, 40.0);
    assert_eq!(first[0].children[0].height, 12.0);
}
