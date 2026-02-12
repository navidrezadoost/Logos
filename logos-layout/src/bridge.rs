//! CRDT → Layout Bridge
//!
//! Translates `CollabOp` operations from the CRDT collaboration engine into
//! `LayoutEngine` mutations.  Designed for synchronous, zero-overhead batch
//! processing — no async runtime required.
//!
//! # Architecture
//!
//! ```text
//!  CollaborationEngine ──CollabOp──▸ LayoutBridge ──▸ LayoutEngine
//!                                        │
//!                                   batch buffer
//! ```
//!
//! The bridge buffers incoming ops and flushes them in a single pass,
//! amortising HashMap lookups and dirty-tracking overhead.

use std::collections::VecDeque;
use uuid::Uuid;
use logos_core::collab::CollabOp;
use logos_core::Layer;
use taffy::prelude::*;

use crate::engine::{LayoutEngine, LayoutError};

// ---------------------------------------------------------------
// Error types
// ---------------------------------------------------------------

/// Errors specific to the bridge layer.
#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    #[error("Layout engine error: {0}")]
    Layout(#[from] LayoutError),

    #[error("Unsupported property for layout: {0}")]
    UnsupportedProperty(String),

    #[error("Invalid property value for '{property}': {reason}")]
    InvalidValue {
        property: String,
        reason: String,
    },
}

// ---------------------------------------------------------------
// Bridge
// ---------------------------------------------------------------

/// Synchronous bridge that converts CRDT operations into layout-tree mutations.
///
/// The bridge owns no layout engine — it borrows one mutably during `flush`,
/// allowing the caller to share the engine with the renderer or other systems.
pub struct LayoutBridge {
    /// Pending operations waiting to be applied.
    pending: VecDeque<CollabOp>,

    /// Running count of ops processed (lifetime of the bridge).
    ops_processed: u64,
}

impl Default for LayoutBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutBridge {
    pub fn new() -> Self {
        Self {
            pending: VecDeque::new(),
            ops_processed: 0,
        }
    }

    // ---------------------------------------------------------------
    // Ingress
    // ---------------------------------------------------------------

    /// Enqueue a single operation for deferred application.
    #[inline]
    pub fn push(&mut self, op: CollabOp) {
        self.pending.push_back(op);
    }

    /// Enqueue multiple operations at once.
    pub fn push_batch(&mut self, ops: impl IntoIterator<Item = CollabOp>) {
        self.pending.extend(ops);
    }

    /// Number of buffered operations awaiting flush.
    #[inline]
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Total operations processed over the lifetime of this bridge.
    #[inline]
    pub fn total_processed(&self) -> u64 {
        self.ops_processed
    }

    // ---------------------------------------------------------------
    // Flush (batch apply)
    // ---------------------------------------------------------------

    /// Apply **all** pending operations to the layout engine.
    ///
    /// Returns the number of operations that produced layout-tree mutations.
    /// Non-layout operations (e.g. colour changes) are silently skipped.
    pub fn flush(&mut self, engine: &mut LayoutEngine) -> Result<FlushResult, BridgeError> {
        let mut result = FlushResult::default();
        let total = self.pending.len();

        while let Some(op) = self.pending.pop_front() {
            match self.apply_one(engine, &op) {
                Ok(applied) => {
                    if applied {
                        result.applied += 1;
                    } else {
                        result.skipped += 1;
                    }
                }
                Err(e) => {
                    log::warn!("Bridge: op failed: {e}");
                    result.errors += 1;
                }
            }
        }

        self.ops_processed += total as u64;
        result.total = total;
        Ok(result)
    }

    /// Apply **one** operation.  Returns `true` if it mutated the layout tree.
    fn apply_one(
        &self,
        engine: &mut LayoutEngine,
        op: &CollabOp,
    ) -> Result<bool, BridgeError> {
        match op {
            CollabOp::AddLayer { id, parent_id, layer, .. } => {
                // If the layer is a Frame with children we first add the frame
                // itself, then recurse for each child.
                let parent = if *parent_id == Uuid::nil() {
                    None
                } else {
                    Some(*parent_id)
                };

                self.add_layer_recursive(engine, layer, parent)?;
                // `add_layer_recursive` already registered *id via the layer's own id,
                // but the CollabOp might carry a different id for move semantics.
                // For now we trust layer.id() == *id.
                let _ = id; // acknowledge
                Ok(true)
            }

            CollabOp::ModifyProperty { id, property, value } => {
                self.apply_property(engine, *id, property, value)
            }

            CollabOp::DeleteLayer { id } => {
                engine.remove_layer(*id)?;
                Ok(true)
            }

            CollabOp::MoveLayer { id, parent_id, .. } => {
                // Move = remove + re-add with new parent.
                // The layout engine doesn't have a dedicated move, so we
                // capture the style, remove, and re-add.
                if let Some(layout) = engine.get_layout(*id) {
                    let _cached = *layout; // copy before remove
                }
                // For now, just mark dirty — full move support requires
                // storing styles separately (tracked for Day 7).
                let _ = (id, parent_id);
                Ok(false)
            }
        }
    }

    /// Recursively add a `Layer` (and its children for Frames) to the engine.
    fn add_layer_recursive(
        &self,
        engine: &mut LayoutEngine,
        layer: &Layer,
        parent_id: Option<Uuid>,
    ) -> Result<(), BridgeError> {
        match layer {
            Layer::Frame(frame) => {
                // Add the frame node itself
                let style = Style {
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
                };
                engine.add_layer(frame.id, parent_id, style)?;

                // Recurse for children
                for child in &frame.children {
                    self.add_layer_recursive(engine, child, Some(frame.id))?;
                }
                Ok(())
            }
            other => {
                engine.add_or_update_layer(other)?;
                // If there's a parent, we need to reparent.  add_or_update_layer
                // creates a root leaf, so we reparent via the engine.
                if let Some(pid) = parent_id {
                    engine.reparent(other.id(), pid)?;
                }
                Ok(())
            }
        }
    }

    // ---------------------------------------------------------------
    // Property routing
    // ---------------------------------------------------------------

    /// Apply a property modification if it affects layout.
    ///
    /// Layout-relevant properties are prefixed with `layout.` or match
    /// known geometry fields (`x`, `y`, `width`, `height`).
    fn apply_property(
        &self,
        engine: &mut LayoutEngine,
        id: Uuid,
        property: &str,
        value: &serde_json::Value,
    ) -> Result<bool, BridgeError> {
        match property {
            "width" | "layout.width" => {
                let v = value_to_f32(property, value)?;
                engine.update_dimension(id, DimAxis::Width, v)?;
                Ok(true)
            }
            "height" | "layout.height" => {
                let v = value_to_f32(property, value)?;
                engine.update_dimension(id, DimAxis::Height, v)?;
                Ok(true)
            }
            "x" | "layout.x" => {
                let v = value_to_f32(property, value)?;
                engine.update_position(id, PosAxis::Left, v)?;
                Ok(true)
            }
            "y" | "layout.y" => {
                let v = value_to_f32(property, value)?;
                engine.update_position(id, PosAxis::Top, v)?;
                Ok(true)
            }
            // Non-layout properties (fill, stroke, opacity, etc.) are ignored.
            _ => Ok(false),
        }
    }
}

// ---------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------

fn value_to_f32(property: &str, value: &serde_json::Value) -> Result<f32, BridgeError> {
    value
        .as_f64()
        .map(|v| v as f32)
        .ok_or_else(|| BridgeError::InvalidValue {
            property: property.to_string(),
            reason: format!("expected number, got {value}"),
        })
}

/// Result of a `flush()` call.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FlushResult {
    /// Total ops drained from the buffer.
    pub total: usize,
    /// Ops that mutated the layout tree.
    pub applied: usize,
    /// Ops skipped (non-layout property changes, etc.).
    pub skipped: usize,
    /// Ops that failed.
    pub errors: usize,
}

/// Axis for dimension updates.
#[derive(Debug, Clone, Copy)]
pub enum DimAxis {
    Width,
    Height,
}

/// Axis for position (inset) updates.
#[derive(Debug, Clone, Copy)]
pub enum PosAxis {
    Left,
    Top,
}

// ===================================================================
// Tests
// ===================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use logos_core::{RectLayer, FrameLayer, Rect as LogosRect};

    fn make_rect(x: f32, y: f32, w: f32, h: f32) -> (Uuid, Layer) {
        let r = RectLayer::new(x, y, w, h);
        let id = r.id;
        (id, Layer::Rect(r))
    }

    fn make_add_op(layer: Layer) -> CollabOp {
        CollabOp::AddLayer {
            id: layer.id(),
            parent_id: Uuid::nil(),
            index: 0,
            layer,
        }
    }

    #[allow(dead_code)]
    fn make_add_op_with_parent(layer: Layer, parent: Uuid) -> CollabOp {
        CollabOp::AddLayer {
            id: layer.id(),
            parent_id: parent,
            index: 0,
            layer,
        }
    }

    // ---------------------------------------------------------------
    // Basic lifecycle
    // ---------------------------------------------------------------

    #[test]
    fn test_new_bridge() {
        let bridge = LayoutBridge::new();
        assert_eq!(bridge.pending_count(), 0);
        assert_eq!(bridge.total_processed(), 0);
    }

    #[test]
    fn test_push_and_pending_count() {
        let mut bridge = LayoutBridge::new();
        let (_, layer) = make_rect(0.0, 0.0, 100.0, 50.0);
        bridge.push(make_add_op(layer));
        assert_eq!(bridge.pending_count(), 1);
    }

    #[test]
    fn test_push_batch() {
        let mut bridge = LayoutBridge::new();
        let ops: Vec<CollabOp> = (0..5)
            .map(|i| {
                let (_, layer) = make_rect(i as f32 * 10.0, 0.0, 50.0, 50.0);
                make_add_op(layer)
            })
            .collect();
        bridge.push_batch(ops);
        assert_eq!(bridge.pending_count(), 5);
    }

    // ---------------------------------------------------------------
    // Flush – AddLayer
    // ---------------------------------------------------------------

    #[test]
    fn test_flush_add_layer() {
        let mut bridge = LayoutBridge::new();
        let mut engine = LayoutEngine::new();

        let (_id, layer) = make_rect(10.0, 20.0, 100.0, 50.0);
        bridge.push(make_add_op(layer));

        let result = bridge.flush(&mut engine).unwrap();
        assert_eq!(result.total, 1);
        assert_eq!(result.applied, 1);
        assert_eq!(result.skipped, 0);
        assert_eq!(result.errors, 0);
        assert_eq!(engine.node_count(), 1);
        assert_eq!(bridge.pending_count(), 0);
        assert_eq!(bridge.total_processed(), 1);
    }

    #[test]
    fn test_flush_batch_add() {
        let mut bridge = LayoutBridge::new();
        let mut engine = LayoutEngine::new();

        let ops: Vec<CollabOp> = (0..100)
            .map(|i| {
                let (_, layer) = make_rect(i as f32, 0.0, 50.0, 50.0);
                make_add_op(layer)
            })
            .collect();
        bridge.push_batch(ops);

        let result = bridge.flush(&mut engine).unwrap();
        assert_eq!(result.total, 100);
        assert_eq!(result.applied, 100);
        assert_eq!(engine.node_count(), 100);
    }

    // ---------------------------------------------------------------
    // Flush – DeleteLayer
    // ---------------------------------------------------------------

    #[test]
    fn test_flush_delete_layer() {
        let mut bridge = LayoutBridge::new();
        let mut engine = LayoutEngine::new();

        let (id, layer) = make_rect(0.0, 0.0, 100.0, 50.0);
        bridge.push(make_add_op(layer));
        bridge.flush(&mut engine).unwrap();
        assert_eq!(engine.node_count(), 1);

        bridge.push(CollabOp::DeleteLayer { id });
        let result = bridge.flush(&mut engine).unwrap();
        assert_eq!(result.applied, 1);
        assert_eq!(engine.node_count(), 0);
    }

    // ---------------------------------------------------------------
    // Flush – ModifyProperty (layout-relevant)
    // ---------------------------------------------------------------

    #[test]
    fn test_flush_modify_width() {
        let mut bridge = LayoutBridge::new();
        let mut engine = LayoutEngine::new();

        let (id, layer) = make_rect(0.0, 0.0, 100.0, 50.0);
        bridge.push(make_add_op(layer));
        bridge.flush(&mut engine).unwrap();

        bridge.push(CollabOp::ModifyProperty {
            id,
            property: "width".to_string(),
            value: serde_json::json!(200.0),
        });
        let result = bridge.flush(&mut engine).unwrap();
        assert_eq!(result.applied, 1);
        assert_eq!(engine.dirty_count(), 1);
    }

    #[test]
    fn test_flush_modify_position() {
        let mut bridge = LayoutBridge::new();
        let mut engine = LayoutEngine::new();

        let (id, layer) = make_rect(0.0, 0.0, 100.0, 50.0);
        bridge.push(make_add_op(layer));
        bridge.flush(&mut engine).unwrap();

        bridge.push(CollabOp::ModifyProperty {
            id,
            property: "x".to_string(),
            value: serde_json::json!(42.0),
        });
        bridge.push(CollabOp::ModifyProperty {
            id,
            property: "y".to_string(),
            value: serde_json::json!(99.0),
        });
        let result = bridge.flush(&mut engine).unwrap();
        assert_eq!(result.applied, 2);
    }

    #[test]
    fn test_flush_modify_nonlayout_property_skipped() {
        let mut bridge = LayoutBridge::new();
        let mut engine = LayoutEngine::new();

        let (id, layer) = make_rect(0.0, 0.0, 100.0, 50.0);
        bridge.push(make_add_op(layer));
        bridge.flush(&mut engine).unwrap();

        bridge.push(CollabOp::ModifyProperty {
            id,
            property: "fill.color".to_string(),
            value: serde_json::json!("#ff0000"),
        });
        let result = bridge.flush(&mut engine).unwrap();
        assert_eq!(result.skipped, 1);
        assert_eq!(result.applied, 0);
    }

    // ---------------------------------------------------------------
    // Error handling
    // ---------------------------------------------------------------

    #[test]
    fn test_flush_delete_nonexistent_counted_as_error() {
        let mut bridge = LayoutBridge::new();
        let mut engine = LayoutEngine::new();

        bridge.push(CollabOp::DeleteLayer { id: Uuid::new_v4() });
        let result = bridge.flush(&mut engine).unwrap();
        assert_eq!(result.errors, 1);
        assert_eq!(result.applied, 0);
    }

    #[test]
    fn test_flush_invalid_property_value() {
        let mut bridge = LayoutBridge::new();
        let mut engine = LayoutEngine::new();

        let (id, layer) = make_rect(0.0, 0.0, 100.0, 50.0);
        bridge.push(make_add_op(layer));
        bridge.flush(&mut engine).unwrap();

        bridge.push(CollabOp::ModifyProperty {
            id,
            property: "width".to_string(),
            value: serde_json::json!("not-a-number"),
        });
        let result = bridge.flush(&mut engine).unwrap();
        assert_eq!(result.errors, 1);
    }

    // ---------------------------------------------------------------
    // MoveLayer (currently no-op, returns skipped)
    // ---------------------------------------------------------------

    #[test]
    fn test_flush_move_layer_skipped() {
        let mut bridge = LayoutBridge::new();
        let mut engine = LayoutEngine::new();

        bridge.push(CollabOp::MoveLayer {
            id: Uuid::new_v4(),
            parent_id: Uuid::new_v4(),
            index: 0,
        });
        let result = bridge.flush(&mut engine).unwrap();
        assert_eq!(result.skipped, 1);
    }

    // ---------------------------------------------------------------
    // Frame with children (recursive add)
    // ---------------------------------------------------------------

    #[test]
    fn test_flush_add_frame_with_children() {
        let mut bridge = LayoutBridge::new();
        let mut engine = LayoutEngine::new();

        let child1 = Layer::Rect(RectLayer::new(0.0, 0.0, 50.0, 30.0));
        let child2 = Layer::Rect(RectLayer::new(60.0, 0.0, 50.0, 30.0));
        let frame = FrameLayer {
            id: Uuid::new_v4(),
            children: vec![child1, child2],
            bounds: LogosRect { x: 0.0, y: 0.0, width: 200.0, height: 100.0 },
        };
        let _frame_id = frame.id;
        let frame_layer = Layer::Frame(frame);

        bridge.push(make_add_op(frame_layer));
        let result = bridge.flush(&mut engine).unwrap();

        assert_eq!(result.applied, 1); // one CollabOp
        assert_eq!(engine.node_count(), 3); // frame + 2 children
    }

    // ---------------------------------------------------------------
    // End-to-end: add → modify → compute → read
    // ---------------------------------------------------------------

    #[test]
    fn test_end_to_end_add_modify_compute() {
        let mut bridge = LayoutBridge::new();
        let mut engine = LayoutEngine::new();

        // 1. Add a rect via bridge
        let (id, layer) = make_rect(10.0, 20.0, 100.0, 50.0);
        bridge.push(make_add_op(layer));
        bridge.flush(&mut engine).unwrap();

        // 2. Compute layout
        engine.compute_layout(id).unwrap();
        let layout = engine.get_layout(id).unwrap();
        assert_eq!(layout.size.width, 100.0);
        assert_eq!(layout.size.height, 50.0);

        // 3. Modify width via bridge
        bridge.push(CollabOp::ModifyProperty {
            id,
            property: "width".to_string(),
            value: serde_json::json!(250.0),
        });
        bridge.flush(&mut engine).unwrap();

        // 4. Recompute and verify
        engine.compute_layout(id).unwrap();
        let layout = engine.get_layout(id).unwrap();
        assert_eq!(layout.size.width, 250.0);
    }

    // ---------------------------------------------------------------
    // Lifetime tracking
    // ---------------------------------------------------------------

    #[test]
    fn test_total_processed_accumulates() {
        let mut bridge = LayoutBridge::new();
        let mut engine = LayoutEngine::new();

        for _ in 0..3 {
            let (_, layer) = make_rect(0.0, 0.0, 10.0, 10.0);
            bridge.push(make_add_op(layer));
        }
        bridge.flush(&mut engine).unwrap();
        assert_eq!(bridge.total_processed(), 3);

        for _ in 0..2 {
            let (_, layer) = make_rect(0.0, 0.0, 10.0, 10.0);
            bridge.push(make_add_op(layer));
        }
        bridge.flush(&mut engine).unwrap();
        assert_eq!(bridge.total_processed(), 5);
    }
}
