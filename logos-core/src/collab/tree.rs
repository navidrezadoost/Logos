//! # Tree-aware CRDT layer for Logos documents
//!
//! Provides page-level hierarchy on top of the flat `"layers"` /
//! `"layer_positions"` Yrs maps introduced in Step 1a.
//!
//! ## Data Model
//!
//! ```text
//! Yrs Doc
//! ├── Map("pages")           ← PageMeta blobs keyed by page UUID
//! ├── Map("layers")          ← Layer blobs keyed by layer UUID
//! ├── Map("layer_positions") ← TreePosition blobs keyed by layer UUID
//! └── Map("metadata")       ← reserved for future use
//! ```
//!
//! Each `TreePosition` records which **page** a layer lives on, its
//! **parent layer** (for nested containers), and its **z-index** within
//! that parent.
//!
//! ## Why not nested Yrs Maps?
//!
//! Yrs Maps only support `String → Any` entries.  Nested maps would
//! require `MapRef`-per-page acquired at construction time, but pages
//! are created dynamically.  Storing everything flat with a
//! `TreePosition` sidecar keeps the schema simple, allows O(1) lookup
//! by layer UUID, and still lets us reconstruct any page tree via a
//! single scan + sort.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Layer;
use super::CollabError;

// ═══════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════

/// Metadata for a single page stored in the `"pages"` Yrs map.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct PageMeta {
    pub id: Uuid,
    pub name: String,
    /// Ordering among pages (0 = first tab).
    pub z_index: u32,
}

/// Full position of a layer inside the document tree.
///
/// Replaces the simpler `LayerPosition` from Step 1a with page
/// awareness.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct TreePosition {
    /// Which page this layer belongs to.
    pub page_id: Uuid,
    /// Parent layer UUID for nested containers (`None` = direct child
    /// of the page root).
    pub parent_id: Option<Uuid>,
    /// Z-order within the parent (0 = bottom).  `u32::MAX` = append.
    pub z_index: u32,
}

/// A fully reconstructed page with its ordered layer tree.
#[derive(Debug, Clone)]
pub struct PageSnapshot {
    pub meta: PageMeta,
    /// Layers in paint order (bottom → top), with nested children
    /// already placed inside container variants (`Frame`, `Artboard`,
    /// `Drawer`, `Section`).
    pub layers: Vec<Layer>,
}

/// Intermediate node used during tree reconstruction.
#[derive(Debug)]
struct TreeNode {
    layer: Layer,
    z_index: u32,
    children: Vec<TreeNode>,
}

// ═══════════════════════════════════════════════════════════════════
// Page-tree helpers (pure functions — no Yrs dependency)
// ═══════════════════════════════════════════════════════════════════

/// Build a flat-to-tree mapping: given layers + positions, return a
/// `Vec<Layer>` with children nested inside container layers, sorted
/// by z-index at every level.
///
/// Layers whose `parent_id` refers to a UUID not present in `layers`
/// are placed at the root level (orphan recovery).
pub fn build_layer_tree(
    layers: &[(Uuid, Layer, TreePosition)],
    page_id: Uuid,
) -> Vec<Layer> {
    // Collect layers belonging to this page
    let filtered: Vec<&(Uuid, Layer, TreePosition)> = layers
        .iter()
        .filter(|(_, _, pos)| pos.page_id == page_id)
        .collect();

    if filtered.is_empty() {
        return Vec::new();
    }

    build_sorted_tree(&filtered)
}

/// Internal recursive builder.
fn build_sorted_tree(
    entries: &[&(Uuid, Layer, TreePosition)],
) -> Vec<Layer> {
    // ── 1. Group by parent_id ──
    let mut roots: Vec<TreeNode> = Vec::new();
    let mut children_map: std::collections::HashMap<Uuid, Vec<TreeNode>>
        = std::collections::HashMap::new();

    // Identify which IDs exist so we can detect orphans
    let known_ids: std::collections::HashSet<Uuid> = entries
        .iter()
        .map(|(id, _, _)| *id)
        .collect();

    for (id, layer, pos) in entries.iter() {
        let node = TreeNode {
            layer: layer.clone(),
            z_index: pos.z_index,
            children: Vec::new(),
        };

        match pos.parent_id {
            Some(pid) if known_ids.contains(&pid) => {
                children_map.entry(pid).or_default().push(node);
            }
            // Root-level or orphan → goes to roots
            _ => {
                roots.push(node);
            }
        }
        let _ = id; // suppress unused warning
    }

    // ── 2. Recursively attach children, deepest first ──
    fn attach_children(
        node: &mut TreeNode,
        children_map: &mut std::collections::HashMap<Uuid, Vec<TreeNode>>,
    ) {
        let id = node.layer.id();
        if let Some(mut kids) = children_map.remove(&id) {
            for child in kids.iter_mut() {
                attach_children(child, children_map);
            }
            kids.sort_by_key(|n| n.z_index);
            node.children = kids;
        }
    }

    for root in roots.iter_mut() {
        attach_children(root, &mut children_map);
    }

    // Any remaining entries in children_map are orphans whose parent
    // was itself a child of something removed — promote them to root.
    let mut orphaned: Vec<TreeNode> = children_map
        .drain()
        .flat_map(|(_, nodes)| nodes)
        .collect();
    orphaned.sort_by_key(|n| n.z_index);
    roots.extend(orphaned);

    // ── 3. Sort roots by z-index ──
    roots.sort_by_key(|n| n.z_index);

    // ── 4. Flatten nodes → Layers (injecting children into containers) ──
    roots.into_iter().map(|n| node_to_layer(n)).collect()
}

/// Convert a `TreeNode` back into a `Layer`, placing children inside
/// container variants.
fn node_to_layer(node: TreeNode) -> Layer {
    let children_layers: Vec<Layer> = node
        .children
        .into_iter()
        .map(|c| node_to_layer(c))
        .collect();

    if children_layers.is_empty() {
        return node.layer;
    }

    // Inject children into the container variant
    match node.layer {
        Layer::Frame(mut f) => {
            f.children = children_layers;
            Layer::Frame(f)
        }
        Layer::Artboard(mut a) => {
            a.children = children_layers;
            Layer::Artboard(a)
        }
        Layer::Drawer(mut d) => {
            d.children = children_layers;
            Layer::Drawer(d)
        }
        Layer::Section(mut s) => {
            s.children = children_layers;
            Layer::Section(s)
        }
        // Non-container layers cannot hold children — children become
        // orphans at root level.  This should not happen in practice
        // because `move_layer_local` only allows reparenting into
        // container IDs.
        other => other,
    }
}

/// Validate that `parent_id` refers to a container layer.
pub fn is_container(layer: &Layer) -> bool {
    matches!(
        layer,
        Layer::Frame(_) | Layer::Artboard(_) | Layer::Drawer(_) | Layer::Section(_)
    )
}

// ═══════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RectLayer, EllipseLayer, TextLayer, FrameLayer, Rect};

    fn make_rect(x: f32) -> (Uuid, Layer) {
        let r = RectLayer::new(x, 0.0, 10.0, 10.0);
        let id = r.id;
        (id, Layer::Rect(r))
    }

    fn make_frame() -> (Uuid, Layer) {
        let f = FrameLayer {
            id: Uuid::new_v4(),
            children: Vec::new(),
            bounds: Rect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 },
        };
        let id = f.id;
        (id, Layer::Frame(f))
    }

    fn pos(page: Uuid, parent: Option<Uuid>, z: u32) -> TreePosition {
        TreePosition { page_id: page, parent_id: parent, z_index: z }
    }

    // ── PageMeta ──

    #[test]
    fn test_page_meta_serde_roundtrip() {
        let meta = PageMeta {
            id: Uuid::new_v4(),
            name: "Design".into(),
            z_index: 0,
        };
        let bytes = bincode::serialize(&meta).unwrap();
        let restored: PageMeta = bincode::deserialize(&bytes).unwrap();
        assert_eq!(meta, restored);
    }

    #[test]
    fn test_tree_position_serde_roundtrip() {
        let tp = TreePosition {
            page_id: Uuid::new_v4(),
            parent_id: Some(Uuid::new_v4()),
            z_index: 42,
        };
        let bytes = bincode::serialize(&tp).unwrap();
        let restored: TreePosition = bincode::deserialize(&bytes).unwrap();
        assert_eq!(tp, restored);
    }

    // ── build_layer_tree: empty ──

    #[test]
    fn test_build_empty() {
        let page = Uuid::new_v4();
        let result = build_layer_tree(&[], page);
        assert!(result.is_empty());
    }

    // ── build_layer_tree: single root ──

    #[test]
    fn test_build_single_root() {
        let page = Uuid::new_v4();
        let (id, layer) = make_rect(0.0);
        let entries = vec![(id, layer.clone(), pos(page, None, 0))];
        let result = build_layer_tree(&entries, page);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id(), id);
    }

    // ── build_layer_tree: z-order sorting ──

    #[test]
    fn test_build_z_order() {
        let page = Uuid::new_v4();
        let (id_a, la) = make_rect(1.0);
        let (id_b, lb) = make_rect(2.0);
        let (id_c, lc) = make_rect(3.0);
        let entries = vec![
            (id_c, lc, pos(page, None, 2)),
            (id_a, la, pos(page, None, 0)),
            (id_b, lb, pos(page, None, 1)),
        ];
        let result = build_layer_tree(&entries, page);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].id(), id_a);
        assert_eq!(result[1].id(), id_b);
        assert_eq!(result[2].id(), id_c);
    }

    // ── build_layer_tree: page filtering ──

    #[test]
    fn test_build_filters_by_page() {
        let page1 = Uuid::new_v4();
        let page2 = Uuid::new_v4();
        let (id1, l1) = make_rect(1.0);
        let (id2, l2) = make_rect(2.0);
        let entries = vec![
            (id1, l1, pos(page1, None, 0)),
            (id2, l2, pos(page2, None, 0)),
        ];
        let p1_layers = build_layer_tree(&entries, page1);
        assert_eq!(p1_layers.len(), 1);
        assert_eq!(p1_layers[0].id(), id1);

        let p2_layers = build_layer_tree(&entries, page2);
        assert_eq!(p2_layers.len(), 1);
        assert_eq!(p2_layers[0].id(), id2);
    }

    // ── build_layer_tree: parent-child nesting ──

    #[test]
    fn test_build_nesting_in_frame() {
        let page = Uuid::new_v4();
        let (fid, frame) = make_frame();
        let (rid, rect) = make_rect(5.0);
        let entries = vec![
            (fid, frame, pos(page, None, 0)),
            (rid, rect, pos(page, Some(fid), 0)),
        ];
        let result = build_layer_tree(&entries, page);
        assert_eq!(result.len(), 1);
        // The frame should now contain the rect as a child
        if let Layer::Frame(f) = &result[0] {
            assert_eq!(f.children.len(), 1);
            assert_eq!(f.children[0].id(), rid);
        } else {
            panic!("expected Frame");
        }
    }

    // ── build_layer_tree: deep nesting (frame inside frame) ──

    #[test]
    fn test_build_deep_nesting() {
        let page = Uuid::new_v4();
        let (f1, frame1) = make_frame();
        let (f2, frame2) = make_frame();
        let (rid, rect) = make_rect(1.0);

        let entries = vec![
            (f1, frame1, pos(page, None, 0)),
            (f2, frame2, pos(page, Some(f1), 0)),
            (rid, rect, pos(page, Some(f2), 0)),
        ];
        let result = build_layer_tree(&entries, page);
        assert_eq!(result.len(), 1);

        if let Layer::Frame(outer) = &result[0] {
            assert_eq!(outer.children.len(), 1);
            if let Layer::Frame(inner) = &outer.children[0] {
                assert_eq!(inner.children.len(), 1);
                assert_eq!(inner.children[0].id(), rid);
            } else {
                panic!("expected inner Frame");
            }
        } else {
            panic!("expected outer Frame");
        }
    }

    // ── build_layer_tree: children sorted by z_index ──

    #[test]
    fn test_build_children_z_order() {
        let page = Uuid::new_v4();
        let (fid, frame) = make_frame();
        let (r1, rect1) = make_rect(1.0);
        let (r2, rect2) = make_rect(2.0);
        let (r3, rect3) = make_rect(3.0);

        let entries = vec![
            (fid, frame, pos(page, None, 0)),
            (r3, rect3, pos(page, Some(fid), 2)),
            (r1, rect1, pos(page, Some(fid), 0)),
            (r2, rect2, pos(page, Some(fid), 1)),
        ];
        let result = build_layer_tree(&entries, page);
        if let Layer::Frame(f) = &result[0] {
            assert_eq!(f.children.len(), 3);
            assert_eq!(f.children[0].id(), r1);
            assert_eq!(f.children[1].id(), r2);
            assert_eq!(f.children[2].id(), r3);
        } else {
            panic!("expected Frame");
        }
    }

    // ── build_layer_tree: orphan recovery ──

    #[test]
    fn test_build_orphan_promoted_to_root() {
        let page = Uuid::new_v4();
        let deleted_parent = Uuid::new_v4(); // doesn't exist in entries
        let (rid, rect) = make_rect(7.0);
        let entries = vec![
            (rid, rect, pos(page, Some(deleted_parent), 0)),
        ];
        let result = build_layer_tree(&entries, page);
        // Orphan should be promoted to root
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id(), rid);
    }

    // ── is_container ──

    #[test]
    fn test_is_container_frame() {
        let (_, frame) = make_frame();
        assert!(is_container(&frame));
    }

    #[test]
    fn test_is_container_rect() {
        let (_, rect) = make_rect(0.0);
        assert!(!is_container(&rect));
    }

    #[test]
    fn test_is_container_text() {
        let t = TextLayer::new("hello", 0.0, 0.0, 50.0, 20.0);
        assert!(!is_container(&Layer::Text(t)));
    }

    #[test]
    fn test_is_container_ellipse() {
        let e = EllipseLayer::new(0.0, 0.0, 30.0, 30.0);
        assert!(!is_container(&Layer::Ellipse(e)));
    }

    // ── build_layer_tree: mixed roots and children ──

    #[test]
    fn test_build_mixed_roots_and_children() {
        let page = Uuid::new_v4();
        let (fid, frame) = make_frame();
        let (r1, rect1) = make_rect(1.0);
        let (r2, rect2) = make_rect(2.0); // root-level peer

        let entries = vec![
            (fid, frame, pos(page, None, 0)),
            (r1, rect1, pos(page, Some(fid), 0)),
            (r2, rect2, pos(page, None, 1)),
        ];
        let result = build_layer_tree(&entries, page);
        assert_eq!(result.len(), 2); // frame + standalone rect
        assert_eq!(result[0].id(), fid);
        assert_eq!(result[1].id(), r2);
        if let Layer::Frame(f) = &result[0] {
            assert_eq!(f.children.len(), 1);
            assert_eq!(f.children[0].id(), r1);
        } else {
            panic!("expected Frame");
        }
    }

    // ── build_layer_tree: multiple pages independence ──

    #[test]
    fn test_build_multi_page_independence() {
        let pa = Uuid::new_v4();
        let pb = Uuid::new_v4();
        let (id_a, la) = make_rect(1.0);
        let (id_b, lb) = make_rect(2.0);
        let (id_c, lc) = make_rect(3.0);

        let entries = vec![
            (id_a, la, pos(pa, None, 0)),
            (id_b, lb, pos(pb, None, 0)),
            (id_c, lc, pos(pb, None, 1)),
        ];

        let pa_layers = build_layer_tree(&entries, pa);
        assert_eq!(pa_layers.len(), 1);

        let pb_layers = build_layer_tree(&entries, pb);
        assert_eq!(pb_layers.len(), 2);
    }
}
