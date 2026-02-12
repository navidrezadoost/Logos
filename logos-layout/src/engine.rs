use std::collections::{HashMap, HashSet};
use uuid::Uuid;
use taffy::prelude::*;
use taffy::{TaffyTree, TaffyError, Style, Layout, NodeId};
use logos_core::Layer;
use thiserror::Error;

use crate::spatial::{Aabb, SpatialHash};

#[derive(Error, Debug)]
pub enum LayoutError {
    #[error("Taffy error: {0}")]
    Taffy(#[from] TaffyError),
    #[error("Layer not found: {0}")]
    LayerNotFound(Uuid),
    #[error("Parent not found: {0}")]
    ParentNotFound(Uuid),
}

/// Core layout engine backed by Taffy for Flexbox/Grid computation.
///
/// Manages a bidirectional mapping between Logos `Layer` UUIDs and Taffy
/// `NodeId`s, with dirty-tracking for partial recomputation.
///
/// Embeds a [`SpatialHash`] that is automatically refreshed after every
/// `compute_layout()` call, enabling O(1) hit testing.
pub struct LayoutEngine {
    /// Taffy 0.9 tree
    taffy: TaffyTree,

    /// Bidirectional mapping between Layer IDs and Taffy nodes
    layer_to_node: HashMap<Uuid, NodeId>,
    node_to_layer: HashMap<NodeId, Uuid>,

    /// Dirty tracking for partial recomputation
    dirty_nodes: HashSet<Uuid>,

    /// Cache of computed layouts for fast renderer access
    layout_results: HashMap<Uuid, Layout>,

    /// Spatial index updated after each layout pass.
    spatial: SpatialHash,
}

impl Default for LayoutEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Default spatial hash cell size (128 px).
///
/// Good for typical design documents with 50–200 px elements.
const DEFAULT_CELL_SIZE: f32 = 128.0;

impl LayoutEngine {
    pub fn new() -> Self {
        Self::with_cell_size(DEFAULT_CELL_SIZE)
    }

    /// Create a layout engine with a custom spatial hash cell size.
    pub fn with_cell_size(cell_size: f32) -> Self {
        Self {
            taffy: TaffyTree::new(),
            layer_to_node: HashMap::new(),
            node_to_layer: HashMap::new(),
            dirty_nodes: HashSet::new(),
            layout_results: HashMap::new(),
            spatial: SpatialHash::new(cell_size),
        }
    }

    // ---------------------------------------------------------------
    // Style helpers  (Taffy 0.9.2 – lowercase constructors)
    // ---------------------------------------------------------------

    /// Convert a Logos `Layer` to a Taffy `Style`.
    fn layer_to_style(layer: &Layer) -> Style {
        match layer {
            Layer::Rect(rect) => Style {
                size: Size {
                    width: Dimension::length(rect.bounds.width),
                    height: Dimension::length(rect.bounds.height),
                },
                position: Position::Absolute,
                inset: taffy::Rect {
                    left: LengthPercentageAuto::length(rect.bounds.x),
                    top: LengthPercentageAuto::length(rect.bounds.y),
                    right: LengthPercentageAuto::auto(),
                    bottom: LengthPercentageAuto::auto(),
                },
                ..Style::default()
            },
            Layer::Ellipse(ellipse) => Style {
                size: Size {
                    width: Dimension::length(ellipse.bounds.width),
                    height: Dimension::length(ellipse.bounds.height),
                },
                position: Position::Absolute,
                inset: taffy::Rect {
                    left: LengthPercentageAuto::length(ellipse.bounds.x),
                    top: LengthPercentageAuto::length(ellipse.bounds.y),
                    right: LengthPercentageAuto::auto(),
                    bottom: LengthPercentageAuto::auto(),
                },
                ..Style::default()
            },
            Layer::Text(text) => Style {
                size: Size {
                    width: Dimension::length(text.bounds.width),
                    height: Dimension::length(text.bounds.height),
                },
                position: Position::Absolute,
                inset: taffy::Rect {
                    left: LengthPercentageAuto::length(text.bounds.x),
                    top: LengthPercentageAuto::length(text.bounds.y),
                    right: LengthPercentageAuto::auto(),
                    bottom: LengthPercentageAuto::auto(),
                },
                ..Style::default()
            },
            Layer::Frame(frame) => Style {
                display: Display::Flex,
                size: Size {
                    width: Dimension::length(frame.bounds.width),
                    height: Dimension::length(frame.bounds.height),
                },
                position: Position::Absolute,
                inset: taffy::Rect {
                    left: LengthPercentageAuto::length(frame.bounds.x),
                    top: LengthPercentageAuto::length(frame.bounds.y),
                    right: LengthPercentageAuto::auto(),
                    bottom: LengthPercentageAuto::auto(),
                },
                ..Style::default()
            },
        }
    }

    /// Build a rect-style for a given width/height at the origin.
    pub fn create_rect_style(width: f32, height: f32) -> Style {
        Style {
            size: Size {
                width: Dimension::length(width),
                height: Dimension::length(height),
            },
            ..Style::default()
        }
    }

    /// Build a flex-container style.
    pub fn create_flex_style(direction: FlexDirection, gap: f32) -> Style {
        Style {
            display: Display::Flex,
            flex_direction: direction,
            gap: Size {
                width: LengthPercentage::length(gap),
                height: LengthPercentage::length(gap),
            },
            ..Style::default()
        }
    }

    // ---------------------------------------------------------------
    // Mutation
    // ---------------------------------------------------------------

    /// Add or update a layer (from a `logos_core::Layer`) in the layout tree.
    pub fn add_or_update_layer(&mut self, layer: &Layer) -> Result<(), LayoutError> {
        let style = Self::layer_to_style(layer);
        let id = layer.id();

        if let Some(&node) = self.layer_to_node.get(&id) {
            self.taffy.set_style(node, style)?;
        } else {
            let node = self.taffy.new_leaf(style)?;
            self.layer_to_node.insert(id, node);
            self.node_to_layer.insert(node, id);
        }
        self.dirty_nodes.insert(id);
        Ok(())
    }

    /// Add a layer by explicit id + style, optionally parented.
    pub fn add_layer(
        &mut self,
        id: Uuid,
        parent_id: Option<Uuid>,
        style: Style,
    ) -> Result<(), LayoutError> {
        let node = self.taffy.new_leaf(style)?;
        self.layer_to_node.insert(id, node);
        self.node_to_layer.insert(node, id);

        if let Some(pid) = parent_id {
            let parent_node = *self.layer_to_node
                .get(&pid)
                .ok_or(LayoutError::ParentNotFound(pid))?;
            self.taffy.add_child(parent_node, node)?;
        }

        self.dirty_nodes.insert(id);
        Ok(())
    }

    /// Remove a layer from the tree.
    pub fn remove_layer(&mut self, id: Uuid) -> Result<(), LayoutError> {
        let node = *self.layer_to_node
            .get(&id)
            .ok_or(LayoutError::LayerNotFound(id))?;

        self.taffy.remove(node)?;
        self.layer_to_node.remove(&id);
        self.node_to_layer.remove(&node);
        self.layout_results.remove(&id);
        self.dirty_nodes.remove(&id);
        self.spatial.remove(id);
        Ok(())
    }

    // ---------------------------------------------------------------
    // Layout computation
    // ---------------------------------------------------------------

    /// Compute layout for a subtree rooted at `root_layer_id`.
    ///
    /// Only recomputes when dirty nodes are present. After computation
    /// the layout cache and spatial index are refreshed for every tracked
    /// node.
    pub fn compute_layout(&mut self, root_layer_id: Uuid) -> Result<(), LayoutError> {
        if self.dirty_nodes.is_empty() {
            return Ok(());
        }

        let root_node = *self.layer_to_node
            .get(&root_layer_id)
            .ok_or(LayoutError::LayerNotFound(root_layer_id))?;

        self.taffy.compute_layout(root_node, Size::MAX_CONTENT)?;

        // Walk every mapped node and cache its layout result + spatial bounds.
        let ids: Vec<(Uuid, NodeId)> = self
            .layer_to_node
            .iter()
            .map(|(&id, &node)| (id, node))
            .collect();

        for (id, node) in ids {
            if let Ok(layout) = self.taffy.layout(node) {
                self.layout_results.insert(id, *layout);

                // Update spatial index with the computed position & size.
                let aabb = Aabb::from_rect(
                    layout.location.x,
                    layout.location.y,
                    layout.size.width,
                    layout.size.height,
                );
                self.spatial.insert(id, aabb);
            }
        }

        self.dirty_nodes.clear();
        Ok(())
    }

    // ---------------------------------------------------------------
    // Queries
    // ---------------------------------------------------------------

    /// Retrieve the cached layout for a layer.
    pub fn get_layout(&self, id: Uuid) -> Option<&Layout> {
        self.layout_results.get(&id)
    }

    /// Number of nodes tracked by the engine.
    pub fn node_count(&self) -> usize {
        self.layer_to_node.len()
    }

    /// Number of nodes currently marked dirty.
    pub fn dirty_count(&self) -> usize {
        self.dirty_nodes.len()
    }

    // ---------------------------------------------------------------
    // Spatial queries (delegated to embedded SpatialHash)
    // ---------------------------------------------------------------

    /// O(1) point hit test — returns the topmost layer at `(px, py)`.
    #[inline]
    pub fn hit_test(&self, px: f32, py: f32) -> Option<Uuid> {
        self.spatial.hit_test(px, py)
    }

    /// Return **all** layers at the point (top-to-bottom order).
    pub fn hit_test_all(&self, px: f32, py: f32) -> Vec<Uuid> {
        self.spatial.hit_test_all(px, py)
    }

    /// Region query: return all layers whose bounds intersect the rect.
    pub fn query_region(&self, region: &Aabb) -> Vec<Uuid> {
        self.spatial.query_region(region)
    }

    /// Read-only access to the embedded spatial hash.
    pub fn spatial(&self) -> &SpatialHash {
        &self.spatial
    }

    // ---------------------------------------------------------------
    // Fine-grained mutations (used by the bridge)
    // ---------------------------------------------------------------

    /// Reparent a node under a new parent.
    pub fn reparent(&mut self, child_id: Uuid, parent_id: Uuid) -> Result<(), LayoutError> {
        let child_node = *self.layer_to_node
            .get(&child_id)
            .ok_or(LayoutError::LayerNotFound(child_id))?;
        let parent_node = *self.layer_to_node
            .get(&parent_id)
            .ok_or(LayoutError::ParentNotFound(parent_id))?;

        self.taffy.add_child(parent_node, child_node)?;
        self.dirty_nodes.insert(child_id);
        self.dirty_nodes.insert(parent_id);
        Ok(())
    }

    /// Update a single dimension (width or height) for an existing node.
    pub fn update_dimension(
        &mut self,
        id: Uuid,
        axis: crate::bridge::DimAxis,
        value: f32,
    ) -> Result<(), LayoutError> {
        let node = *self.layer_to_node
            .get(&id)
            .ok_or(LayoutError::LayerNotFound(id))?;

        let mut style = self.taffy.style(node)?.clone();
        match axis {
            crate::bridge::DimAxis::Width => {
                style.size.width = Dimension::length(value);
            }
            crate::bridge::DimAxis::Height => {
                style.size.height = Dimension::length(value);
            }
        }
        self.taffy.set_style(node, style)?;
        self.dirty_nodes.insert(id);
        Ok(())
    }

    /// Update a single position axis (left/top) for an existing node.
    pub fn update_position(
        &mut self,
        id: Uuid,
        axis: crate::bridge::PosAxis,
        value: f32,
    ) -> Result<(), LayoutError> {
        let node = *self.layer_to_node
            .get(&id)
            .ok_or(LayoutError::LayerNotFound(id))?;

        let mut style = self.taffy.style(node)?.clone();
        match axis {
            crate::bridge::PosAxis::Left => {
                style.inset.left = LengthPercentageAuto::length(value);
            }
            crate::bridge::PosAxis::Top => {
                style.inset.top = LengthPercentageAuto::length(value);
            }
        }
        self.taffy.set_style(node, style)?;
        self.dirty_nodes.insert(id);
        Ok(())
    }
}

// ===================================================================
// Tests
// ===================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use logos_core::{RectLayer, FrameLayer, Rect as LogosRect};

    #[test]
    fn test_new_engine() {
        let engine = LayoutEngine::new();
        assert_eq!(engine.node_count(), 0);
        assert_eq!(engine.dirty_count(), 0);
    }

    #[test]
    fn test_add_single_rect() {
        let mut engine = LayoutEngine::new();
        let layer = Layer::Rect(RectLayer::new(10.0, 20.0, 100.0, 50.0));
        engine.add_or_update_layer(&layer).unwrap();
        assert_eq!(engine.node_count(), 1);
        assert_eq!(engine.dirty_count(), 1);
    }

    #[test]
    fn test_add_layer_by_style() {
        let mut engine = LayoutEngine::new();
        let id = Uuid::new_v4();
        let style = LayoutEngine::create_rect_style(200.0, 150.0);
        engine.add_layer(id, None, style).unwrap();
        assert_eq!(engine.node_count(), 1);
    }

    #[test]
    fn test_parent_child_hierarchy() {
        let mut engine = LayoutEngine::new();
        let parent_id = Uuid::new_v4();
        let child_id = Uuid::new_v4();

        let parent_style = LayoutEngine::create_flex_style(FlexDirection::Column, 8.0);
        let child_style = LayoutEngine::create_rect_style(80.0, 40.0);

        engine.add_layer(parent_id, None, parent_style).unwrap();
        engine.add_layer(child_id, Some(parent_id), child_style).unwrap();
        assert_eq!(engine.node_count(), 2);
    }

    #[test]
    fn test_compute_layout_single_node() {
        let mut engine = LayoutEngine::new();
        let layer = Layer::Rect(RectLayer::new(0.0, 0.0, 120.0, 60.0));
        let id = layer.id();
        engine.add_or_update_layer(&layer).unwrap();
        engine.compute_layout(id).unwrap();

        let layout = engine.get_layout(id).expect("layout should be cached");
        assert!((layout.size.width - 120.0).abs() < f32::EPSILON);
        assert!((layout.size.height - 60.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_compute_layout_flex_children() {
        let mut engine = LayoutEngine::new();
        let parent_id = Uuid::new_v4();
        let c1 = Uuid::new_v4();
        let c2 = Uuid::new_v4();

        engine
            .add_layer(
                parent_id,
                None,
                Style {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    size: Size {
                        width: Dimension::length(300.0),
                        height: Dimension::length(100.0),
                    },
                    ..Style::default()
                },
            )
            .unwrap();

        engine
            .add_layer(c1, Some(parent_id), LayoutEngine::create_rect_style(100.0, 50.0))
            .unwrap();
        engine
            .add_layer(c2, Some(parent_id), LayoutEngine::create_rect_style(100.0, 50.0))
            .unwrap();

        engine.compute_layout(parent_id).unwrap();

        let parent_layout = engine.get_layout(parent_id).unwrap();
        assert!((parent_layout.size.width - 300.0).abs() < f32::EPSILON);

        // Both children should have computed layouts
        assert!(engine.get_layout(c1).is_some());
        assert!(engine.get_layout(c2).is_some());
    }

    #[test]
    fn test_update_existing_layer() {
        let mut engine = LayoutEngine::new();
        let rect = RectLayer::new(0.0, 0.0, 100.0, 100.0);
        let id = rect.id;
        let layer = Layer::Rect(rect);

        engine.add_or_update_layer(&layer).unwrap();
        engine.compute_layout(id).unwrap();

        // Update bounds
        let updated_rect = RectLayer {
            id,
            bounds: LogosRect { x: 0.0, y: 0.0, width: 200.0, height: 100.0 },
        };
        let updated = Layer::Rect(updated_rect);
        engine.add_or_update_layer(&updated).unwrap();
        engine.compute_layout(id).unwrap();

        let layout = engine.get_layout(id).unwrap();
        assert!((layout.size.width - 200.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_remove_layer() {
        let mut engine = LayoutEngine::new();
        let layer = Layer::Rect(RectLayer::new(0.0, 0.0, 50.0, 50.0));
        let id = layer.id();
        engine.add_or_update_layer(&layer).unwrap();
        assert_eq!(engine.node_count(), 1);

        engine.remove_layer(id).unwrap();
        assert_eq!(engine.node_count(), 0);
        assert!(engine.get_layout(id).is_none());
    }

    #[test]
    fn test_remove_nonexistent_errors() {
        let mut engine = LayoutEngine::new();
        let result = engine.remove_layer(Uuid::new_v4());
        assert!(result.is_err());
    }

    #[test]
    fn test_dirty_clears_after_compute() {
        let mut engine = LayoutEngine::new();
        let layer = Layer::Rect(RectLayer::new(0.0, 0.0, 50.0, 50.0));
        let id = layer.id();
        engine.add_or_update_layer(&layer).unwrap();
        assert_eq!(engine.dirty_count(), 1);

        engine.compute_layout(id).unwrap();
        assert_eq!(engine.dirty_count(), 0);
    }

    #[test]
    fn test_no_recompute_when_clean() {
        let mut engine = LayoutEngine::new();
        let layer = Layer::Rect(RectLayer::new(0.0, 0.0, 50.0, 50.0));
        let id = layer.id();
        engine.add_or_update_layer(&layer).unwrap();
        engine.compute_layout(id).unwrap();
        // Second compute is a no-op
        engine.compute_layout(id).unwrap();
        assert_eq!(engine.dirty_count(), 0);
    }

    #[test]
    fn test_all_layer_variants() {
        let rect = Layer::Rect(RectLayer::new(0.0, 0.0, 10.0, 10.0));
        let ellipse = Layer::Ellipse(logos_core::EllipseLayer {
            id: Uuid::new_v4(),
            bounds: LogosRect { x: 0.0, y: 0.0, width: 20.0, height: 20.0 },
        });
        let text = Layer::Text(logos_core::TextLayer {
            id: Uuid::new_v4(),
            content: "hi".into(),
            bounds: LogosRect { x: 0.0, y: 0.0, width: 30.0, height: 12.0 },
        });
        let frame = Layer::Frame(FrameLayer {
            id: Uuid::new_v4(),
            children: vec![],
            bounds: LogosRect { x: 0.0, y: 0.0, width: 400.0, height: 300.0 },
        });

        let mut engine = LayoutEngine::new();
        for layer in &[rect, ellipse, text, frame] {
            engine.add_or_update_layer(layer).unwrap();
        }
        assert_eq!(engine.node_count(), 4);
    }

    // ---------------------------------------------------------------
    // Layout → Spatial integration tests
    // ---------------------------------------------------------------

    #[test]
    fn test_spatial_populated_after_compute() {
        let mut engine = LayoutEngine::new();
        let layer = Layer::Rect(RectLayer::new(10.0, 20.0, 100.0, 50.0));
        let id = layer.id();
        engine.add_or_update_layer(&layer).unwrap();
        engine.compute_layout(id).unwrap();

        // Spatial hash should now contain this layer
        assert_eq!(engine.spatial().len(), 1);
    }

    #[test]
    fn test_hit_test_via_engine() {
        let mut engine = LayoutEngine::new();
        let layer = Layer::Rect(RectLayer::new(0.0, 0.0, 100.0, 50.0));
        let id = layer.id();
        engine.add_or_update_layer(&layer).unwrap();
        engine.compute_layout(id).unwrap();

        // Point inside the rect
        assert_eq!(engine.hit_test(50.0, 25.0), Some(id));
        // Point outside
        assert!(engine.hit_test(200.0, 200.0).is_none());
    }

    #[test]
    fn test_hit_test_all_via_engine() {
        let mut engine = LayoutEngine::new();
        let r1 = Layer::Rect(RectLayer::new(0.0, 0.0, 100.0, 100.0));
        let r2 = Layer::Rect(RectLayer::new(0.0, 0.0, 100.0, 100.0));
        let id1 = r1.id();
        let id2 = r2.id();

        engine.add_or_update_layer(&r1).unwrap();
        engine.compute_layout(id1).unwrap();
        engine.add_or_update_layer(&r2).unwrap();
        engine.compute_layout(id2).unwrap();

        let hits = engine.hit_test_all(50.0, 50.0);
        assert_eq!(hits.len(), 2);
        assert!(hits.contains(&id1));
        assert!(hits.contains(&id2));
    }

    #[test]
    fn test_spatial_cleared_on_remove() {
        let mut engine = LayoutEngine::new();
        let layer = Layer::Rect(RectLayer::new(0.0, 0.0, 100.0, 50.0));
        let id = layer.id();
        engine.add_or_update_layer(&layer).unwrap();
        engine.compute_layout(id).unwrap();
        assert_eq!(engine.spatial().len(), 1);

        engine.remove_layer(id).unwrap();
        assert_eq!(engine.spatial().len(), 0);
        assert!(engine.hit_test(50.0, 25.0).is_none());
    }

    #[test]
    fn test_query_region_via_engine() {
        let mut engine = LayoutEngine::new();
        let layer = Layer::Rect(RectLayer::new(10.0, 10.0, 50.0, 50.0));
        let id = layer.id();
        engine.add_or_update_layer(&layer).unwrap();
        engine.compute_layout(id).unwrap();

        let region = crate::spatial::Aabb::from_rect(0.0, 0.0, 200.0, 200.0);
        let hits = engine.query_region(&region);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0], id);
    }

    #[test]
    fn test_spatial_updates_on_recompute() {
        let mut engine = LayoutEngine::new();

        // Create a parent frame large enough to contain the child.
        let parent_id = Uuid::new_v4();
        engine
            .add_layer(
                parent_id,
                None,
                Style {
                    size: Size {
                        width: Dimension::length(1000.0),
                        height: Dimension::length(1000.0),
                    },
                    ..Style::default()
                },
            )
            .unwrap();

        // Add child at position (0,0), size 100×100
        let child_id = Uuid::new_v4();
        engine
            .add_layer(
                child_id,
                Some(parent_id),
                Style {
                    size: Size {
                        width: Dimension::length(100.0),
                        height: Dimension::length(100.0),
                    },
                    position: Position::Absolute,
                    inset: taffy::Rect {
                        left: LengthPercentageAuto::length(0.0),
                        top: LengthPercentageAuto::length(0.0),
                        right: LengthPercentageAuto::auto(),
                        bottom: LengthPercentageAuto::auto(),
                    },
                    ..Style::default()
                },
            )
            .unwrap();

        engine.compute_layout(parent_id).unwrap();

        // Child should be hit-testable at its initial position (0,0)
        let initial_hits = engine.hit_test_all(50.0, 50.0);
        assert!(initial_hits.contains(&child_id), "child should be at initial position");

        // Move child to (500, 500)
        engine
            .update_position(child_id, crate::bridge::PosAxis::Left, 500.0)
            .unwrap();
        engine
            .update_position(child_id, crate::bridge::PosAxis::Top, 500.0)
            .unwrap();
        engine.compute_layout(parent_id).unwrap();

        // Old position should not contain the child
        let old_hits = engine.hit_test_all(50.0, 50.0);
        assert!(!old_hits.contains(&child_id), "child should not be at old position");

        // New position should contain the child
        let new_hits = engine.hit_test_all(550.0, 550.0);
        assert!(new_hits.contains(&child_id), "child should be at new position");
    }

    #[test]
    fn test_with_cell_size() {
        let engine = LayoutEngine::with_cell_size(64.0);
        assert_eq!(engine.node_count(), 0);
        assert_eq!(engine.spatial().len(), 0);
    }
}
