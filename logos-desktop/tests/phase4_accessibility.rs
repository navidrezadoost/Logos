// Phase 4 – Accessibility Module Tests (t446–t460)
//
// Tests for `logos_desktop::accessibility`:
// `AriaRole`, `AccessibilityNode`, `AccessibilityBounds`,
// `AccessibilityTree`, and `LiveRegion`.

use logos_desktop::accessibility::{
    AccessibilityBounds, AccessibilityNode, AccessibilityTree, AriaRole, LiveRegion,
};

// ── §1 AriaRole ────────────────────────────────────────────────────────────────

#[test]
fn t446_aria_role_as_str_button() {
    assert_eq!(AriaRole::Button.as_str(), "button");
}

#[test]
fn t447_aria_role_as_str_list() {
    assert_eq!(AriaRole::List.as_str(), "list");
}

#[test]
fn t448_aria_role_is_interactive_true_for_button() {
    assert!(AriaRole::Button.is_interactive());
}

#[test]
fn t449_aria_role_is_interactive_false_for_list() {
    assert!(!AriaRole::List.is_interactive());
}

#[test]
fn t450_aria_role_display_matches_as_str() {
    let role = AriaRole::MenuItem;
    assert_eq!(format!("{}", role), role.as_str());
}

// ── §2 AccessibilityBounds ────────────────────────────────────────────────────

#[test]
fn t451_bounds_contains_interior_point() {
    let b = AccessibilityBounds::new(10.0, 10.0, 100.0, 80.0);
    assert!(b.contains(50.0, 50.0));
}

#[test]
fn t452_bounds_not_contains_exterior_point() {
    let b = AccessibilityBounds::new(10.0, 10.0, 100.0, 80.0);
    assert!(!b.contains(200.0, 200.0));
}

#[test]
fn t453_bounds_area_correct() {
    let b = AccessibilityBounds::new(0.0, 0.0, 50.0, 30.0);
    assert!((b.area() - 1500.0).abs() < f64::EPSILON);
}

#[test]
fn t454_bounds_center_correct() {
    let b = AccessibilityBounds::new(0.0, 0.0, 100.0, 80.0);
    let (cx, cy) = b.center();
    assert!((cx - 50.0).abs() < f64::EPSILON);
    assert!((cy - 40.0).abs() < f64::EPSILON);
}

// ── §3 AccessibilityNode ──────────────────────────────────────────────────────

#[test]
fn t455_node_new_has_correct_role_and_label() {
    let node = AccessibilityNode::new(AriaRole::Button, "Save");
    assert_eq!(node.role, AriaRole::Button);
    assert_eq!(node.label, "Save");
}

#[test]
fn t456_node_new_button_is_focusable() {
    let node = AccessibilityNode::new(AriaRole::Button, "OK");
    assert!(node.focusable);
}

#[test]
fn t457_node_new_list_is_not_focusable() {
    let node = AccessibilityNode::new(AriaRole::List, "Variants");
    assert!(!node.focusable);
}

#[test]
fn t458_node_announce_text_includes_label() {
    let node = AccessibilityNode::new(AriaRole::Button, "ClickMe");
    let text = node.announce_text();
    assert!(text.contains("ClickMe"), "announce_text `{text}` should contain label");
}

// ── §4 AccessibilityTree ──────────────────────────────────────────────────────

#[test]
fn t459_tree_add_child_increases_node_count() {
    let mut tree = AccessibilityTree::new();
    let root = AccessibilityNode::new(AriaRole::Application, "Root");
    let root_id = root.id;
    tree.set_root(root);
    assert_eq!(tree.node_count(), 1);

    let child = AccessibilityNode::new(AriaRole::Button, "Button");
    tree.add_child(&root_id, child);
    assert_eq!(tree.node_count(), 2);
}

#[test]
fn t460_tree_remove_node_decrements_count() {
    let mut tree = AccessibilityTree::new();
    let root = AccessibilityNode::new(AriaRole::Application, "Root");
    let root_id = root.id;
    tree.set_root(root);

    let child = AccessibilityNode::new(AriaRole::Text, "Hello");
    let child_id = tree.add_child(&root_id, child).unwrap();
    assert_eq!(tree.node_count(), 2);

    tree.remove(&child_id);
    assert_eq!(tree.node_count(), 1);
}
