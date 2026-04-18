//! Phase 2 Integration Tests — Shape Toolkit
//!
//! Coverage:
//!   §1  RectLayer corner smoothing / squircle    (s001–s010)
//!   §2  LineLayer                                (s011–s020)
//!   §3  PolygonLayer                             (s021–s030)
//!   §4  StarLayer                                (s031–s040)
//!   §5  BooleanGroupLayer                        (s041–s050)
//!   §6  VectorNetworkLayer                       (s051–s065)
//!   §7  Layer enum + bounds + children           (s066–s080)

use logos_core::{
    BooleanGroupLayer, BooleanOp, EllipseLayer, Layer, LineLayer, PolygonLayer,
    Rect, RectLayer, StarLayer, TextLayer, VNEdge, VNNode, VectorNetworkLayer,
};
use logos_core::hierarchy::{ContainerKind, LayerCategory, validate_add_layer};
use logos_core::WorkspaceMode;
use uuid::Uuid;

// ── §1 RectLayer corner smoothing ────────────────────────────────────────────

/// s001: Default RectLayer has zero corner_radius.
#[test]
fn s001_rect_default_corner_radius_zero() {
    let r = RectLayer::new(0.0, 0.0, 100.0, 50.0);
    assert_eq!(r.corner_radius, 0.0);
}

/// s002: Default RectLayer has zero corner_smoothing.
#[test]
fn s002_rect_default_corner_smoothing_zero() {
    let r = RectLayer::new(0.0, 0.0, 100.0, 50.0);
    assert_eq!(r.corner_smoothing, 0.0);
}

/// s003: with_corner_radius sets value.
#[test]
fn s003_rect_with_corner_radius() {
    let r = RectLayer::new(0.0, 0.0, 100.0, 50.0).with_corner_radius(12.0);
    assert_eq!(r.corner_radius, 12.0);
}

/// s004: with_corner_smoothing sets value.
#[test]
fn s004_rect_with_corner_smoothing() {
    let r = RectLayer::new(0.0, 0.0, 100.0, 50.0).with_corner_smoothing(0.6);
    assert!((r.corner_smoothing - 0.6).abs() < 1e-6);
}

/// s005: corner_smoothing clamps above 1.0.
#[test]
fn s005_rect_smoothing_clamps_above_one() {
    let r = RectLayer::new(0.0, 0.0, 100.0, 50.0).with_corner_smoothing(5.0);
    assert_eq!(r.corner_smoothing, 1.0);
}

/// s006: corner_smoothing clamps below 0.0.
#[test]
fn s006_rect_smoothing_clamps_below_zero() {
    let r = RectLayer::new(0.0, 0.0, 100.0, 50.0).with_corner_smoothing(-1.0);
    assert_eq!(r.corner_smoothing, 0.0);
}

/// s007: is_squircle returns false when no radius.
#[test]
fn s007_rect_not_squircle_without_radius() {
    let r = RectLayer::new(0.0, 0.0, 100.0, 50.0).with_corner_smoothing(1.0);
    assert!(!r.is_squircle(), "no radius → not squircle");
}

/// s008: is_squircle returns false when no smoothing.
#[test]
fn s008_rect_not_squircle_without_smoothing() {
    let r = RectLayer::new(0.0, 0.0, 100.0, 50.0).with_corner_radius(10.0);
    assert!(!r.is_squircle(), "no smoothing → not squircle");
}

/// s009: is_squircle returns true when both radius and smoothing set.
#[test]
fn s009_rect_is_squircle_with_radius_and_smoothing() {
    let r = RectLayer::new(0.0, 0.0, 200.0, 200.0)
        .with_corner_radius(32.0)
        .with_corner_smoothing(0.6);
    assert!(r.is_squircle());
}

/// s010: negative corner_radius is clamped to 0.
#[test]
fn s010_rect_negative_radius_clamped() {
    let r = RectLayer::new(0.0, 0.0, 100.0, 50.0).with_corner_radius(-5.0);
    assert_eq!(r.corner_radius, 0.0);
}

// ── §2 LineLayer ─────────────────────────────────────────────────────────────

/// s011: LineLayer::new creates valid endpoints.
#[test]
fn s011_line_new_endpoints() {
    let l = LineLayer::new(0.0, 0.0, 100.0, 0.0);
    assert_eq!(l.x1, 0.0);
    assert_eq!(l.x2, 100.0);
}

/// s012: Length of horizontal line.
#[test]
fn s012_line_horizontal_length() {
    let l = LineLayer::new(0.0, 0.0, 100.0, 0.0);
    assert!((l.length() - 100.0).abs() < 1e-4);
}

/// s013: Length of diagonal line (Pythagoras).
#[test]
fn s013_line_diagonal_length() {
    let l = LineLayer::new(0.0, 0.0, 3.0, 4.0);
    assert!((l.length() - 5.0).abs() < 1e-4);
}

/// s014: Default stroke_width is 1.
#[test]
fn s014_line_default_stroke_width() {
    let l = LineLayer::new(0.0, 0.0, 50.0, 50.0);
    assert_eq!(l.stroke_width, 1.0);
}

/// s015: with_stroke_width sets value.
#[test]
fn s015_line_with_stroke_width() {
    let l = LineLayer::new(0.0, 0.0, 50.0, 50.0).with_stroke_width(4.0);
    assert_eq!(l.stroke_width, 4.0);
}

/// s016: Negative stroke width is clamped to 0.
#[test]
fn s016_line_negative_stroke_clamped() {
    let l = LineLayer::new(0.0, 0.0, 50.0, 0.0).with_stroke_width(-3.0);
    assert_eq!(l.stroke_width, 0.0);
}

/// s017: bounds() min_x/min_y correct for reversed line.
#[test]
fn s017_line_bounds_reversed() {
    let l = LineLayer::new(100.0, 50.0, 0.0, 0.0);
    let b = l.bounds();
    assert_eq!(b.x, 0.0);
    assert_eq!(b.y, 0.0);
}

/// s018: bounds() width for horizontal line equals length.
#[test]
fn s018_line_bounds_width() {
    let l = LineLayer::new(10.0, 20.0, 80.0, 20.0);
    let b = l.bounds();
    assert!((b.width - 70.0).abs() < 1e-4);
}

/// s019: A zero-length line bounds falls back to stroke_width.
#[test]
fn s019_line_zero_length_bounds_fallback() {
    let l = LineLayer::new(50.0, 50.0, 50.0, 50.0).with_stroke_width(3.0);
    let b = l.bounds();
    assert!(b.width >= 3.0 && b.height >= 3.0);
}

/// s020: Layer::Line reports correct id.
#[test]
fn s020_layer_line_id() {
    let l = LineLayer::new(0.0, 0.0, 100.0, 0.0);
    let id = l.id;
    let layer = Layer::Line(l);
    assert_eq!(layer.id(), id);
}

// ── §3 PolygonLayer ──────────────────────────────────────────────────────────

/// s021: Triangle has 3 sides.
#[test]
fn s021_polygon_triangle_three_sides() {
    let p = PolygonLayer::new(0.0, 0.0, 100.0, 100.0, 3);
    assert_eq!(p.sides, 3);
}

/// s022: Minimum sides clamped to 3 (input=1).
#[test]
fn s022_polygon_min_sides_clamped() {
    let p = PolygonLayer::new(0.0, 0.0, 100.0, 100.0, 1);
    assert_eq!(p.sides, 3);
}

/// s023: Hexagon has 6 sides.
#[test]
fn s023_polygon_hexagon_six_sides() {
    let p = PolygonLayer::new(0.0, 0.0, 100.0, 100.0, 6);
    assert_eq!(p.sides, 6);
}

/// s024: vertices_normalised returns N vertices.
#[test]
fn s024_polygon_vertex_count_matches_sides() {
    let p = PolygonLayer::new(0.0, 0.0, 100.0, 100.0, 5);
    assert_eq!(p.vertices_normalised().len(), 5);
}

/// s025: First vertex of triangle is near top-centre (x≈0.5).
#[test]
fn s025_polygon_triangle_first_vertex_top_centre() {
    let p = PolygonLayer::new(0.0, 0.0, 100.0, 100.0, 3);
    let v = p.vertices_normalised();
    assert!((v[0].0 - 0.5).abs() < 1e-4, "x should be ~0.5, got {}", v[0].0);
    assert!(v[0].1 < 0.5, "first vertex should be above centre");
}

/// s026: All normalised vertices are in [0.0, 1.0].
#[test]
fn s026_polygon_vertices_in_unit_square() {
    let p = PolygonLayer::new(0.0, 0.0, 200.0, 200.0, 8);
    for (x, y) in p.vertices_normalised() {
        assert!((0.0..=1.0).contains(&x), "x={x} out of range");
        assert!((0.0..=1.0).contains(&y), "y={y} out of range");
    }
}

/// s027: Layer::Polygon id is correct.
#[test]
fn s027_layer_polygon_id() {
    let p = PolygonLayer::new(0.0, 0.0, 100.0, 100.0, 6);
    let id = p.id;
    let layer = Layer::Polygon(p);
    assert_eq!(layer.id(), id);
}

/// s028: Layer::Polygon bounds are correct.
#[test]
fn s028_layer_polygon_bounds() {
    let p = PolygonLayer::new(10.0, 20.0, 80.0, 60.0, 5);
    let layer = Layer::Polygon(p);
    let b = layer.bounds();
    assert_eq!(b.x, 10.0);
    assert_eq!(b.y, 20.0);
}

/// s029: Zero-sided polygon clamped to 3, vertex count = 3.
#[test]
fn s029_polygon_zero_sides_clamped_to_three() {
    let p = PolygonLayer::new(0.0, 0.0, 50.0, 50.0, 0);
    assert_eq!(p.vertices_normalised().len(), 3);
}

/// s030: PolygonLayer serialises and deserialises correctly.
#[test]
fn s030_polygon_serde_roundtrip() {
    let p = PolygonLayer::new(5.0, 5.0, 100.0, 100.0, 7);
    let json = serde_json::to_string(&p).unwrap();
    let back: PolygonLayer = serde_json::from_str(&json).unwrap();
    assert_eq!(back.sides, 7);
    assert_eq!(back.bounds.x, 5.0);
}

// ── §4 StarLayer ─────────────────────────────────────────────────────────────

/// s031: StarLayer default has golden-ratio inner_ratio.
#[test]
fn s031_star_default_inner_ratio() {
    let s = StarLayer::new(0.0, 0.0, 100.0, 100.0, 5);
    assert!((s.inner_ratio - 0.382).abs() < 0.001);
}

/// s032: StarLayer::new clamps points to minimum 3.
#[test]
fn s032_star_min_points_clamped() {
    let s = StarLayer::new(0.0, 0.0, 100.0, 100.0, 2);
    assert_eq!(s.points, 3);
}

/// s033: with_inner_ratio sets value.
#[test]
fn s033_star_with_inner_ratio() {
    let s = StarLayer::new(0.0, 0.0, 100.0, 100.0, 5).with_inner_ratio(0.5);
    assert!((s.inner_ratio - 0.5).abs() < 1e-6);
}

/// s034: inner_ratio clamps above 0.99.
#[test]
fn s034_star_inner_ratio_clamp_high() {
    let s = StarLayer::new(0.0, 0.0, 100.0, 100.0, 5).with_inner_ratio(2.0);
    assert_eq!(s.inner_ratio, 0.99);
}

/// s035: inner_ratio clamps below 0.01.
#[test]
fn s035_star_inner_ratio_clamp_low() {
    let s = StarLayer::new(0.0, 0.0, 100.0, 100.0, 5).with_inner_ratio(0.0);
    assert_eq!(s.inner_ratio, 0.01);
}

/// s036: vertices_normalised returns 2N vertices.
#[test]
fn s036_star_vertex_count_is_2n() {
    let s = StarLayer::new(0.0, 0.0, 100.0, 100.0, 5);
    assert_eq!(s.vertices_normalised().len(), 10);
}

/// s037: All star vertices in unit square.
#[test]
fn s037_star_vertices_in_unit_square() {
    let s = StarLayer::new(0.0, 0.0, 100.0, 100.0, 6);
    for (x, y) in s.vertices_normalised() {
        assert!((0.0..=1.0).contains(&x), "x={x}");
        assert!((0.0..=1.0).contains(&y), "y={y}");
    }
}

/// s038: Layer::Star id matches.
#[test]
fn s038_layer_star_id() {
    let s = StarLayer::new(0.0, 0.0, 100.0, 100.0, 5);
    let id = s.id;
    let layer = Layer::Star(s);
    assert_eq!(layer.id(), id);
}

/// s039: Layer::Star bounds are correct.
#[test]
fn s039_layer_star_bounds() {
    let s = StarLayer::new(30.0, 40.0, 60.0, 60.0, 5);
    let layer = Layer::Star(s);
    let b = layer.bounds();
    assert_eq!(b.x, 30.0);
    assert_eq!(b.y, 40.0);
}

/// s040: StarLayer serialisation roundtrip.
#[test]
fn s040_star_serde_roundtrip() {
    let s = StarLayer::new(0.0, 0.0, 80.0, 80.0, 4).with_inner_ratio(0.45);
    let json = serde_json::to_string(&s).unwrap();
    let back: StarLayer = serde_json::from_str(&json).unwrap();
    assert_eq!(back.points, 4);
    assert!((back.inner_ratio - 0.45).abs() < 1e-5);
}

// ── §5 BooleanGroupLayer ──────────────────────────────────────────────────────

/// s041: BooleanGroupLayer::new is empty.
#[test]
fn s041_boolean_group_starts_empty() {
    let b = BooleanGroupLayer::new(BooleanOp::Union);
    assert_eq!(b.children.len(), 0);
}

/// s042: with_child adds a child.
#[test]
fn s042_boolean_group_with_child() {
    let rect = Layer::Rect(RectLayer::new(0.0, 0.0, 50.0, 50.0));
    let b = BooleanGroupLayer::new(BooleanOp::Union).with_child(rect);
    assert_eq!(b.children.len(), 1);
}

/// s043: Subtract op is stored correctly.
#[test]
fn s043_boolean_group_subtract_op() {
    let b = BooleanGroupLayer::new(BooleanOp::Subtract);
    assert_eq!(b.op, BooleanOp::Subtract);
}

/// s044: Intersect op.
#[test]
fn s044_boolean_group_intersect_op() {
    let b = BooleanGroupLayer::new(BooleanOp::Intersect);
    assert_eq!(b.op, BooleanOp::Intersect);
}

/// s045: Exclude op.
#[test]
fn s045_boolean_group_exclude_op() {
    let b = BooleanGroupLayer::new(BooleanOp::Exclude);
    assert_eq!(b.op, BooleanOp::Exclude);
}

/// s046: BooleanOp Display strings.
#[test]
fn s046_boolean_op_display() {
    assert_eq!(BooleanOp::Union.to_string(), "union");
    assert_eq!(BooleanOp::Subtract.to_string(), "subtract");
    assert_eq!(BooleanOp::Intersect.to_string(), "intersect");
    assert_eq!(BooleanOp::Exclude.to_string(), "exclude");
}

/// s047: Layer::BooleanGroup id is correct.
#[test]
fn s047_layer_boolean_group_id() {
    let b = BooleanGroupLayer::new(BooleanOp::Union);
    let id = b.id;
    let layer = Layer::BooleanGroup(b);
    assert_eq!(layer.id(), id);
}

/// s048: Layer::BooleanGroup children() returns children slice.
#[test]
fn s048_layer_boolean_group_children() {
    let rect = Layer::Rect(RectLayer::new(0.0, 0.0, 50.0, 50.0));
    let b = BooleanGroupLayer::new(BooleanOp::Union).with_child(rect);
    let layer = Layer::BooleanGroup(b);
    assert_eq!(layer.children().unwrap().len(), 1);
}

/// s049: BooleanGroupLayer serialisation roundtrip preserves op.
#[test]
fn s049_boolean_group_serde_roundtrip() {
    let b = BooleanGroupLayer::new(BooleanOp::Intersect);
    let json = serde_json::to_string(&b).unwrap();
    let back: BooleanGroupLayer = serde_json::from_str(&json).unwrap();
    assert_eq!(back.op, BooleanOp::Intersect);
}

/// s050: Two children in BooleanGroup.
#[test]
fn s050_boolean_group_two_children() {
    let r1 = Layer::Rect(RectLayer::new(0.0, 0.0, 60.0, 60.0));
    let r2 = Layer::Ellipse(EllipseLayer::new(10.0, 10.0, 40.0, 40.0));
    let b = BooleanGroupLayer::new(BooleanOp::Subtract)
        .with_child(r1)
        .with_child(r2);
    assert_eq!(b.children.len(), 2);
}

// ── §6 VectorNetworkLayer ───────────────────────────────────────────────────

/// s051: New VectorNetwork has no nodes.
#[test]
fn s051_vn_starts_empty() {
    let vn = VectorNetworkLayer::new();
    assert_eq!(vn.node_count(), 0);
    assert_eq!(vn.edge_count(), 0);
}

/// s052: add_node increases node count.
#[test]
fn s052_vn_add_node() {
    let mut vn = VectorNetworkLayer::new();
    vn.add_node(0.0, 0.0);
    assert_eq!(vn.node_count(), 1);
}

/// s053: add_node returns a valid UUID.
#[test]
fn s053_vn_add_node_returns_uuid() {
    let mut vn = VectorNetworkLayer::new();
    let id = vn.add_node(10.0, 20.0);
    assert_ne!(id, Uuid::nil());
}

/// s054: add_edge between two existing nodes succeeds.
#[test]
fn s054_vn_add_edge_success() {
    let mut vn = VectorNetworkLayer::new();
    let a = vn.add_node(0.0, 0.0);
    let b = vn.add_node(100.0, 0.0);
    let e = vn.add_edge(a, b);
    assert!(e.is_some());
    assert_eq!(vn.edge_count(), 1);
}

/// s055: add_edge with unknown node fails (returns None).
#[test]
fn s055_vn_add_edge_invalid_node() {
    let mut vn = VectorNetworkLayer::new();
    let a = vn.add_node(0.0, 0.0);
    let bogus = Uuid::new_v4();
    let result = vn.add_edge(a, bogus);
    assert!(result.is_none());
    assert_eq!(vn.edge_count(), 0);
}

/// s056: Multiple edges from one node (multi-branch).
#[test]
fn s056_vn_multi_branch_edges() {
    let mut vn = VectorNetworkLayer::new();
    let center = vn.add_node(50.0, 50.0);
    let left   = vn.add_node(0.0, 50.0);
    let right  = vn.add_node(100.0, 50.0);
    let top    = vn.add_node(50.0, 0.0);
    vn.add_edge(center, left).unwrap();
    vn.add_edge(center, right).unwrap();
    vn.add_edge(center, top).unwrap();
    assert_eq!(vn.edge_count(), 3);
}

/// s057: Bounds recomputed after adding nodes.
#[test]
fn s057_vn_bounds_recomputed() {
    let mut vn = VectorNetworkLayer::new();
    vn.add_node(10.0, 20.0);
    vn.add_node(110.0, 80.0);
    let b = vn.bounds;
    assert_eq!(b.x, 10.0);
    assert_eq!(b.y, 20.0);
    assert!((b.width - 100.0).abs() < 1e-4);
    assert!((b.height - 60.0).abs() < 1e-4);
}

/// s058: VNNode x/y stored correctly.
#[test]
fn s058_vn_node_position() {
    let n = VNNode::new(42.0, 99.0);
    assert_eq!(n.x, 42.0);
    assert_eq!(n.y, 99.0);
}

/// s059: VNEdge control points stored.
#[test]
fn s059_vn_edge_control_points() {
    let mut vn = VectorNetworkLayer::new();
    let a = vn.add_node(0.0, 0.0);
    let b = vn.add_node(100.0, 0.0);
    vn.add_edge(a, b);
    let edge = &vn.edges[0];
    // default control points
    assert_eq!(edge.cp_from, (0.0, 0.0));
    assert_eq!(edge.cp_to, (0.0, 0.0));
}

/// s060: VNEdge with_control_points stores values.
#[test]
fn s060_vn_edge_with_control_points() {
    let from = Uuid::new_v4();
    let to = Uuid::new_v4();
    let e = VNEdge::new(from, to).with_control_points((10.0, 5.0), (90.0, 5.0));
    assert_eq!(e.cp_from, (10.0, 5.0));
    assert_eq!(e.cp_to, (90.0, 5.0));
}

/// s061: Layer::VectorNetwork id correct.
#[test]
fn s061_layer_vn_id() {
    let vn = VectorNetworkLayer::new();
    let id = vn.id;
    let layer = Layer::VectorNetwork(vn);
    assert_eq!(layer.id(), id);
}

/// s062: Layer::VectorNetwork has no children (not a container).
#[test]
fn s062_layer_vn_no_children() {
    let vn = VectorNetworkLayer::new();
    let layer = Layer::VectorNetwork(vn);
    assert!(layer.children().is_none());
}

/// s063: VectorNetworkLayer serialisation roundtrip.
#[test]
fn s063_vn_serde_roundtrip() {
    let mut vn = VectorNetworkLayer::new();
    let a = vn.add_node(0.0, 0.0);
    let b = vn.add_node(100.0, 100.0);
    vn.add_edge(a, b);
    let json = serde_json::to_string(&vn).unwrap();
    let back: VectorNetworkLayer = serde_json::from_str(&json).unwrap();
    assert_eq!(back.node_count(), 2);
    assert_eq!(back.edge_count(), 1);
}

/// s064: Single-node network has near-zero width (floor 1.0).
#[test]
fn s064_vn_single_node_bounds_floor() {
    let mut vn = VectorNetworkLayer::new();
    vn.add_node(50.0, 50.0);
    assert!(vn.bounds.width >= 1.0);
    assert!(vn.bounds.height >= 1.0);
}

/// s065: LayerCategory::of for VectorNetwork is Shape.
#[test]
fn s065_layer_category_vn_is_shape() {
    let vn = VectorNetworkLayer::new();
    let layer = Layer::VectorNetwork(vn);
    assert_eq!(logos_core::hierarchy::LayerCategory::of(&layer), LayerCategory::Shape);
}

// ── §7 Layer enum + bounds + children integration ────────────────────────────

/// s066: LayerCategory::of for Line is Shape.
#[test]
fn s066_layer_category_line_is_shape() {
    let l = Layer::Line(LineLayer::new(0.0, 0.0, 100.0, 0.0));
    assert_eq!(LayerCategory::of(&l), LayerCategory::Shape);
}

/// s067: LayerCategory::of for Polygon is Shape.
#[test]
fn s067_layer_category_polygon_is_shape() {
    let p = Layer::Polygon(PolygonLayer::new(0.0, 0.0, 100.0, 100.0, 5));
    assert_eq!(LayerCategory::of(&p), LayerCategory::Shape);
}

/// s068: LayerCategory::of for Star is Shape.
#[test]
fn s068_layer_category_star_is_shape() {
    let s = Layer::Star(StarLayer::new(0.0, 0.0, 100.0, 100.0, 5));
    assert_eq!(LayerCategory::of(&s), LayerCategory::Shape);
}

/// s069: LayerCategory::of for BooleanGroup is Shape.
#[test]
fn s069_layer_category_boolean_is_shape() {
    let b = Layer::BooleanGroup(BooleanGroupLayer::new(BooleanOp::Union));
    assert_eq!(LayerCategory::of(&b), LayerCategory::Shape);
}

/// s070: FlatPage accepts Line at root.
#[test]
fn s070_flat_page_accepts_line() {
    let l = Layer::Line(LineLayer::new(0.0, 0.0, 100.0, 0.0));
    let result = validate_add_layer(WorkspaceMode::FlatPage, ContainerKind::Root, &l);
    assert!(result.is_ok());
}

/// s071: FlatPage accepts Polygon at root.
#[test]
fn s071_flat_page_accepts_polygon() {
    let p = Layer::Polygon(PolygonLayer::new(0.0, 0.0, 100.0, 100.0, 6));
    let result = validate_add_layer(WorkspaceMode::FlatPage, ContainerKind::Root, &p);
    assert!(result.is_ok());
}

/// s072: FlatPage accepts Star at root.
#[test]
fn s072_flat_page_accepts_star() {
    let s = Layer::Star(StarLayer::new(0.0, 0.0, 100.0, 100.0, 5));
    let result = validate_add_layer(WorkspaceMode::FlatPage, ContainerKind::Root, &s);
    assert!(result.is_ok());
}

/// s073: FlatPage accepts BooleanGroup at root.
#[test]
fn s073_flat_page_accepts_boolean_group() {
    let b = Layer::BooleanGroup(BooleanGroupLayer::new(BooleanOp::Subtract));
    let result = validate_add_layer(WorkspaceMode::FlatPage, ContainerKind::Root, &b);
    assert!(result.is_ok());
}

/// s074: FlatPage accepts VectorNetwork at root.
#[test]
fn s074_flat_page_accepts_vn() {
    let vn = Layer::VectorNetwork(VectorNetworkLayer::new());
    let result = validate_add_layer(WorkspaceMode::FlatPage, ContainerKind::Root, &vn);
    assert!(result.is_ok());
}

/// s075: Hybrid accepts Line at root.
#[test]
fn s075_hybrid_accepts_line() {
    let l = Layer::Line(LineLayer::new(0.0, 0.0, 200.0, 0.0));
    let result = validate_add_layer(WorkspaceMode::Hybrid, ContainerKind::Root, &l);
    assert!(result.is_ok());
}

/// s076: ArtboardSection Artboard accepts Star.
#[test]
fn s076_artboard_section_artboard_accepts_star() {
    let s = Layer::Star(StarLayer::new(0.0, 0.0, 80.0, 80.0, 4));
    let result = validate_add_layer(WorkspaceMode::ArtboardSection, ContainerKind::Artboard, &s);
    assert!(result.is_ok());
}

/// s077: Layer bounds() via Line variant uses line-computed bounds.
#[test]
fn s077_layer_bounds_via_line() {
    let l = LineLayer::new(5.0, 10.0, 105.0, 10.0);
    let expected_x = l.bounds().x;
    let layer = Layer::Line(l);
    assert_eq!(layer.bounds().x, expected_x);
}

/// s078: Layer children() for BooleanGroup with two shapes.
#[test]
fn s078_boolean_group_children_via_layer() {
    let r1 = Layer::Rect(RectLayer::new(0.0, 0.0, 50.0, 50.0));
    let r2 = Layer::Rect(RectLayer::new(25.0, 25.0, 50.0, 50.0));
    let b = BooleanGroupLayer::new(BooleanOp::Union).with_child(r1).with_child(r2);
    let layer = Layer::BooleanGroup(b);
    assert_eq!(layer.children().unwrap().len(), 2);
}

/// s079: RectLayer serde roundtrip preserves corner_smoothing (new field).
#[test]
fn s079_rect_serde_preserves_corner_smoothing() {
    let r = RectLayer::new(0.0, 0.0, 100.0, 100.0)
        .with_corner_radius(16.0)
        .with_corner_smoothing(0.75);
    let json = serde_json::to_string(&r).unwrap();
    let back: RectLayer = serde_json::from_str(&json).unwrap();
    assert!((back.corner_smoothing - 0.75).abs() < 1e-5);
    assert!((back.corner_radius - 16.0).abs() < 1e-5);
}

/// s080: All new Layer variants have unique IDs.
#[test]
fn s080_new_variants_have_unique_ids() {
    let ids: Vec<Uuid> = vec![
        Layer::Line(LineLayer::new(0.0, 0.0, 100.0, 0.0)).id(),
        Layer::Polygon(PolygonLayer::new(0.0, 0.0, 100.0, 100.0, 5)).id(),
        Layer::Star(StarLayer::new(0.0, 0.0, 100.0, 100.0, 5)).id(),
        Layer::BooleanGroup(BooleanGroupLayer::new(BooleanOp::Union)).id(),
        Layer::VectorNetwork(VectorNetworkLayer::new()).id(),
    ];
    let unique: std::collections::HashSet<Uuid> = ids.iter().cloned().collect();
    assert_eq!(unique.len(), ids.len(), "all IDs must be unique");
}
