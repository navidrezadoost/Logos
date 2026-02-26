//! Section hierarchy — nesting, parent-child relationships, traversal,
//! depth limits, and reparenting operations.
//!
//! Sections can nest inside each other to form a tree. This module
//! provides tools to walk that tree, enforce depth limits, move
//! sections between parents, and collect statistics.

use logos_core::container::SectionData;
use logos_core::Layer;
use uuid::Uuid;

/// Maximum nesting depth for sections (prevents unbounded recursion).
pub const MAX_SECTION_DEPTH: usize = 8;

// ═══════════════════════════════════════════════════════════════════
// Tree statistics
// ═══════════════════════════════════════════════════════════════════

/// Summary statistics for a section tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeStats {
    /// Total number of sections (including root).
    pub section_count: usize,
    /// Maximum nesting depth (root = 0).
    pub max_depth: usize,
    /// Total number of non-section leaf layers.
    pub leaf_count: usize,
    /// Total number of layers (sections + leaves).
    pub total_layers: usize,
}

/// Compute tree statistics for a section.
pub fn tree_stats(section: &SectionData) -> TreeStats {
    let mut stats = TreeStats {
        section_count: 1, // root section itself
        max_depth: 0,
        leaf_count: 0,
        total_layers: 0,
    };
    count_recursive(&section.children, 1, &mut stats);
    stats
}

fn count_recursive(children: &[Layer], depth: usize, stats: &mut TreeStats) {
    for child in children {
        stats.total_layers += 1;
        match child {
            Layer::Section(s) => {
                stats.section_count += 1;
                if depth > stats.max_depth {
                    stats.max_depth = depth;
                }
                count_recursive(&s.children, depth + 1, stats);
            }
            other => {
                stats.leaf_count += 1;
                if let Some(children) = other.children() {
                    count_recursive(children, depth, stats);
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Depth validation
// ═══════════════════════════════════════════════════════════════════

/// Error returned when a section operation violates constraints.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HierarchyError {
    /// Nesting would exceed `MAX_SECTION_DEPTH`.
    DepthLimitExceeded { attempted: usize, max: usize },
    /// Target section not found.
    SectionNotFound(Uuid),
    /// Cannot move a section into its own descendant.
    CircularReference(Uuid),
    /// Layer not found within the section tree.
    LayerNotFound(Uuid),
}

impl std::fmt::Display for HierarchyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DepthLimitExceeded { attempted, max } => {
                write!(f, "section depth {attempted} exceeds limit {max}")
            }
            Self::SectionNotFound(id) => write!(f, "section {id} not found"),
            Self::CircularReference(id) => {
                write!(f, "circular reference: cannot nest section {id} inside itself")
            }
            Self::LayerNotFound(id) => write!(f, "layer {id} not found"),
        }
    }
}

impl std::error::Error for HierarchyError {}

/// Check whether adding a child section at the given `current_depth`
/// would exceed `MAX_SECTION_DEPTH`.
pub fn check_depth(current_depth: usize) -> Result<(), HierarchyError> {
    if current_depth >= MAX_SECTION_DEPTH {
        Err(HierarchyError::DepthLimitExceeded {
            attempted: current_depth + 1,
            max: MAX_SECTION_DEPTH,
        })
    } else {
        Ok(())
    }
}

/// Compute the section-nesting depth of a layer within a root section.
/// Returns `None` if the layer is not found.
pub fn depth_of(section: &SectionData, target_id: Uuid) -> Option<usize> {
    if section.id == target_id {
        return Some(0);
    }
    depth_recursive(&section.children, target_id, 1)
}

fn depth_recursive(children: &[Layer], target_id: Uuid, depth: usize) -> Option<usize> {
    for child in children {
        if child.id() == target_id {
            return Some(depth);
        }
        if let Layer::Section(s) = child {
            if let Some(d) = depth_recursive(&s.children, target_id, depth + 1) {
                return Some(d);
            }
        }
    }
    None
}

// ═══════════════════════════════════════════════════════════════════
// Traversal
// ═══════════════════════════════════════════════════════════════════

/// Visitor callback: receives `(depth, layer)` for each node.
pub type VisitFn<'a> = &'a mut dyn FnMut(usize, &Layer);

/// Walk all layers in a section tree, depth-first.
pub fn walk(section: &SectionData, visitor: &mut dyn FnMut(usize, &Layer)) {
    walk_recursive(&section.children, 0, visitor);
}

fn walk_recursive(children: &[Layer], depth: usize, visitor: &mut dyn FnMut(usize, &Layer)) {
    for child in children {
        visitor(depth, child);
        match child {
            Layer::Section(s) => walk_recursive(&s.children, depth + 1, visitor),
            other => {
                if let Some(kids) = other.children() {
                    walk_recursive(kids, depth + 1, visitor);
                }
            }
        }
    }
}

/// Collect all section IDs in a tree (including the root).
pub fn all_section_ids(section: &SectionData) -> Vec<Uuid> {
    let mut ids = vec![section.id];
    collect_section_ids(&section.children, &mut ids);
    ids
}

fn collect_section_ids(children: &[Layer], ids: &mut Vec<Uuid>) {
    for child in children {
        if let Layer::Section(s) = child {
            ids.push(s.id);
            collect_section_ids(&s.children, ids);
        }
    }
}

/// Flatten all layers in the section tree into a Vec (depth-first).
pub fn flatten(section: &SectionData) -> Vec<&Layer> {
    let mut out = Vec::new();
    flatten_recursive(&section.children, &mut out);
    out
}

fn flatten_recursive<'a>(children: &'a [Layer], out: &mut Vec<&'a Layer>) {
    for child in children {
        out.push(child);
        match child {
            Layer::Section(s) => flatten_recursive(&s.children, out),
            other => {
                if let Some(kids) = other.children() {
                    flatten_recursive(kids, out);
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Reparenting
// ═══════════════════════════════════════════════════════════════════

/// Check if `ancestor_id` is an ancestor of `descendant_id` in the tree.
pub fn is_ancestor_of(section: &SectionData, ancestor_id: Uuid, descendant_id: Uuid) -> bool {
    if section.id == ancestor_id {
        return depth_of(section, descendant_id).is_some();
    }
    // Find the sub-section with ancestor_id and check if descendant is inside
    if let Some(sub) = find_section(section, ancestor_id) {
        depth_of(sub, descendant_id).is_some()
    } else {
        false
    }
}

/// Find a section by ID within the tree (returns reference).
pub fn find_section(section: &SectionData, id: Uuid) -> Option<&SectionData> {
    if section.id == id {
        return Some(section);
    }
    for child in &section.children {
        if let Layer::Section(s) = child {
            if let Some(found) = find_section(s, id) {
                return Some(found);
            }
        }
    }
    None
}

/// Find a mutable section by ID within the tree.
pub fn find_section_mut(section: &mut SectionData, id: Uuid) -> Option<&mut SectionData> {
    if section.id == id {
        return Some(section);
    }
    for child in &mut section.children {
        if let Layer::Section(s) = child {
            if let Some(found) = find_section_mut(s, id) {
                return Some(found);
            }
        }
    }
    None
}

/// Remove a layer by ID from anywhere in the section tree.
/// Returns the removed layer, or None if not found.
pub fn remove_layer(section: &mut SectionData, layer_id: Uuid) -> Option<Layer> {
    // Check direct children first
    if let Some(pos) = section.children.iter().position(|c| c.id() == layer_id) {
        return Some(section.children.remove(pos));
    }
    // Recurse into sub-sections
    for child in &mut section.children {
        if let Layer::Section(s) = child {
            if let Some(removed) = remove_layer(s, layer_id) {
                return Some(removed);
            }
        }
    }
    None
}

/// Move a layer from anywhere in the tree into a target section.
///
/// Validates:
/// - Layer exists in the tree
/// - Target section exists
/// - Moving a section into its own descendant is forbidden (circular ref)
/// - Depth limit is respected
pub fn reparent(
    root: &mut SectionData,
    layer_id: Uuid,
    target_section_id: Uuid,
) -> Result<(), HierarchyError> {
    // Compute target depth
    let target_depth = depth_of(root, target_section_id)
        .ok_or(HierarchyError::SectionNotFound(target_section_id))?;

    // If the layer being moved is a section, check for circular reference
    if let Some(moving_section) = find_section(root, layer_id) {
        if depth_of(moving_section, target_section_id).is_some() {
            return Err(HierarchyError::CircularReference(layer_id));
        }
        // Check depth limit for the sub-tree being moved
        let sub_stats = tree_stats(moving_section);
        let new_max = target_depth + 1 + sub_stats.max_depth;
        if new_max > MAX_SECTION_DEPTH {
            return Err(HierarchyError::DepthLimitExceeded {
                attempted: new_max,
                max: MAX_SECTION_DEPTH,
            });
        }
    }

    // Remove the layer
    let layer = remove_layer(root, layer_id)
        .ok_or(HierarchyError::LayerNotFound(layer_id))?;

    // Insert into target
    let target = find_section_mut(root, target_section_id)
        .ok_or(HierarchyError::SectionNotFound(target_section_id))?;
    target.children.push(layer);

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use logos_core::container::SectionData;
    use logos_core::{Layer, RectLayer};

    fn make_section(name: &str) -> SectionData {
        SectionData::new(name)
    }

    fn make_rect(x: f32, y: f32, w: f32, h: f32) -> Layer {
        Layer::Rect(RectLayer::new(x, y, w, h))
    }

    // ─── tree_stats ─────────────────────────────────────────────

    #[test]
    fn test_stats_empty_section() {
        let s = make_section("Empty");
        let stats = tree_stats(&s);
        assert_eq!(stats.section_count, 1);
        assert_eq!(stats.max_depth, 0);
        assert_eq!(stats.leaf_count, 0);
        assert_eq!(stats.total_layers, 0);
    }

    #[test]
    fn test_stats_flat_children() {
        let mut s = make_section("Root");
        s.add_child(make_rect(0.0, 0.0, 10.0, 10.0));
        s.add_child(make_rect(20.0, 0.0, 10.0, 10.0));
        let stats = tree_stats(&s);
        assert_eq!(stats.section_count, 1);
        assert_eq!(stats.leaf_count, 2);
        assert_eq!(stats.total_layers, 2);
        assert_eq!(stats.max_depth, 0);
    }

    #[test]
    fn test_stats_nested_sections() {
        let mut root = make_section("Root");
        let mut child = make_section("Child");
        let mut grandchild = make_section("Grandchild");
        grandchild.add_child(make_rect(0.0, 0.0, 5.0, 5.0));
        child.add_child(Layer::Section(grandchild));
        child.add_child(make_rect(0.0, 0.0, 10.0, 10.0));
        root.add_child(Layer::Section(child));

        let stats = tree_stats(&root);
        assert_eq!(stats.section_count, 3);
        assert_eq!(stats.max_depth, 2);
        assert_eq!(stats.leaf_count, 2);
        assert_eq!(stats.total_layers, 4); // child_section + grandchild_section + 2 rects
    }

    // ─── depth_of ───────────────────────────────────────────────

    #[test]
    fn test_depth_of_root() {
        let s = make_section("Root");
        assert_eq!(depth_of(&s, s.id), Some(0));
    }

    #[test]
    fn test_depth_of_nested() {
        let mut root = make_section("Root");
        let child = make_section("Child");
        let child_id = child.id;
        root.add_child(Layer::Section(child));
        assert_eq!(depth_of(&root, child_id), Some(1));
    }

    #[test]
    fn test_depth_of_not_found() {
        let s = make_section("Root");
        assert_eq!(depth_of(&s, Uuid::new_v4()), None);
    }

    #[test]
    fn test_depth_of_deep() {
        let mut root = make_section("Root");
        let mut d1 = make_section("D1");
        let mut d2 = make_section("D2");
        let d3 = make_section("D3");
        let d3_id = d3.id;
        d2.add_child(Layer::Section(d3));
        d1.add_child(Layer::Section(d2));
        root.add_child(Layer::Section(d1));
        assert_eq!(depth_of(&root, d3_id), Some(3));
    }

    // ─── check_depth ────────────────────────────────────────────

    #[test]
    fn test_check_depth_ok() {
        assert!(check_depth(0).is_ok());
        assert!(check_depth(MAX_SECTION_DEPTH - 1).is_ok());
    }

    #[test]
    fn test_check_depth_exceeded() {
        let err = check_depth(MAX_SECTION_DEPTH).unwrap_err();
        assert_eq!(
            err,
            HierarchyError::DepthLimitExceeded {
                attempted: MAX_SECTION_DEPTH + 1,
                max: MAX_SECTION_DEPTH,
            }
        );
    }

    // ─── walk ───────────────────────────────────────────────────

    #[test]
    fn test_walk_visits_all() {
        let mut root = make_section("Root");
        root.add_child(make_rect(0.0, 0.0, 10.0, 10.0));
        let mut child = make_section("Child");
        child.add_child(make_rect(20.0, 0.0, 10.0, 10.0));
        root.add_child(Layer::Section(child));

        let mut visited = Vec::new();
        walk(&root, &mut |depth, layer| {
            visited.push((depth, layer.id()));
        });
        assert_eq!(visited.len(), 3); // rect, child section, child rect
    }

    #[test]
    fn test_walk_depths() {
        let mut root = make_section("Root");
        let mut child = make_section("Child");
        child.add_child(make_rect(0.0, 0.0, 5.0, 5.0));
        root.add_child(Layer::Section(child));

        let mut depths = Vec::new();
        walk(&root, &mut |depth, _| depths.push(depth));
        assert_eq!(depths, vec![0, 1]); // section at 0, rect inside at 1
    }

    // ─── all_section_ids ────────────────────────────────────────

    #[test]
    fn test_all_section_ids_single() {
        let s = make_section("Single");
        let ids = all_section_ids(&s);
        assert_eq!(ids, vec![s.id]);
    }

    #[test]
    fn test_all_section_ids_nested() {
        let mut root = make_section("Root");
        let child = make_section("Child");
        let child_id = child.id;
        root.add_child(Layer::Section(child));
        root.add_child(make_rect(0.0, 0.0, 10.0, 10.0)); // non-section

        let ids = all_section_ids(&root);
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&root.id));
        assert!(ids.contains(&child_id));
    }

    // ─── flatten ────────────────────────────────────────────────

    #[test]
    fn test_flatten_empty() {
        let s = make_section("Empty");
        assert!(flatten(&s).is_empty());
    }

    #[test]
    fn test_flatten_mixed() {
        let mut root = make_section("Root");
        root.add_child(make_rect(0.0, 0.0, 10.0, 10.0));
        let mut child = make_section("Child");
        child.add_child(make_rect(20.0, 0.0, 5.0, 5.0));
        root.add_child(Layer::Section(child));

        let flat = flatten(&root);
        assert_eq!(flat.len(), 3);
    }

    // ─── find_section ───────────────────────────────────────────

    #[test]
    fn test_find_section_root() {
        let s = make_section("Root");
        let found = find_section(&s, s.id);
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "Root");
    }

    #[test]
    fn test_find_section_nested() {
        let mut root = make_section("Root");
        let child = make_section("Child");
        let child_id = child.id;
        root.add_child(Layer::Section(child));

        let found = find_section(&root, child_id);
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "Child");
    }

    #[test]
    fn test_find_section_not_found() {
        let s = make_section("Root");
        assert!(find_section(&s, Uuid::new_v4()).is_none());
    }

    // ─── remove_layer ───────────────────────────────────────────

    #[test]
    fn test_remove_direct_child() {
        let mut root = make_section("Root");
        let rect = make_rect(0.0, 0.0, 10.0, 10.0);
        let rect_id = rect.id();
        root.add_child(rect);

        let removed = remove_layer(&mut root, rect_id);
        assert!(removed.is_some());
        assert_eq!(root.child_count(), 0);
    }

    #[test]
    fn test_remove_nested_child() {
        let mut root = make_section("Root");
        let mut child = make_section("Child");
        let rect = make_rect(5.0, 5.0, 10.0, 10.0);
        let rect_id = rect.id();
        child.add_child(rect);
        root.add_child(Layer::Section(child));

        let removed = remove_layer(&mut root, rect_id);
        assert!(removed.is_some());
    }

    #[test]
    fn test_remove_not_found() {
        let mut root = make_section("Root");
        root.add_child(make_rect(0.0, 0.0, 10.0, 10.0));
        assert!(remove_layer(&mut root, Uuid::new_v4()).is_none());
    }

    // ─── reparent ───────────────────────────────────────────────

    #[test]
    fn test_reparent_success() {
        let mut root = make_section("Root");
        let rect = make_rect(0.0, 0.0, 10.0, 10.0);
        let rect_id = rect.id();
        root.add_child(rect);

        let child = make_section("Target");
        let target_id = child.id;
        root.add_child(Layer::Section(child));

        assert!(reparent(&mut root, rect_id, target_id).is_ok());
        // rect should now be inside the child section
        let target = find_section(&root, target_id).unwrap();
        assert_eq!(target.child_count(), 1);
    }

    #[test]
    fn test_reparent_circular_ref() {
        let mut root = make_section("Root");
        let child = make_section("Child");
        let child_id = child.id;
        root.add_child(Layer::Section(child));

        // Try to move root-level child into itself
        let err = reparent(&mut root, child_id, child_id);
        assert!(err.is_err());
        match err.unwrap_err() {
            HierarchyError::CircularReference(id) => assert_eq!(id, child_id),
            other => panic!("expected CircularReference, got {other:?}"),
        }
    }

    #[test]
    fn test_reparent_target_not_found() {
        let mut root = make_section("Root");
        let rect = make_rect(0.0, 0.0, 10.0, 10.0);
        let rect_id = rect.id();
        root.add_child(rect);

        let err = reparent(&mut root, rect_id, Uuid::new_v4()).unwrap_err();
        matches!(err, HierarchyError::SectionNotFound(_));
    }

    #[test]
    fn test_reparent_layer_not_found() {
        let mut root = make_section("Root");
        let child = make_section("Child");
        let child_id = child.id;
        root.add_child(Layer::Section(child));

        let err = reparent(&mut root, Uuid::new_v4(), child_id).unwrap_err();
        matches!(err, HierarchyError::LayerNotFound(_));
    }

    // ─── is_ancestor_of ─────────────────────────────────────────

    #[test]
    fn test_is_ancestor() {
        let mut root = make_section("Root");
        let mut child = make_section("Child");
        let grandchild = make_section("Grandchild");
        let gc_id = grandchild.id;
        child.add_child(Layer::Section(grandchild));
        let child_id = child.id;
        root.add_child(Layer::Section(child));

        assert!(is_ancestor_of(&root, root.id, child_id));
        assert!(is_ancestor_of(&root, root.id, gc_id));
        assert!(is_ancestor_of(&root, child_id, gc_id));
        assert!(!is_ancestor_of(&root, gc_id, child_id)); // grandchild is not ancestor of child
    }

    // ─── HierarchyError Display ─────────────────────────────────

    #[test]
    fn test_error_display() {
        let err = HierarchyError::DepthLimitExceeded { attempted: 9, max: 8 };
        assert!(err.to_string().contains("depth 9"));
        assert!(err.to_string().contains("limit 8"));

        let id = Uuid::new_v4();
        let err2 = HierarchyError::SectionNotFound(id);
        assert!(err2.to_string().contains(&id.to_string()));

        let err3 = HierarchyError::CircularReference(id);
        assert!(err3.to_string().contains("circular"));

        let err4 = HierarchyError::LayerNotFound(id);
        assert!(err4.to_string().contains(&id.to_string()));
    }
}
