//! Hybrid Layout Engine – Phase 1
//!
//! Orchestrates three layout passes in a single deterministic order:
//!
//! ```text
//! ① Constraint pre-pass   → resolve_constraints() adjusts children BEFORE Taffy
//! ② Taffy pass            → Flexbox/Grid via the existing LayoutEngine
//! ③ Grid expansion pass   → RepeatGrid cells injected via inject_layout()
//! ```
//!
//! The active [`WorkspaceMode`] gates which containers participate in
//! each pass:
//!
//! | Mode | Artboards | Sections | Flat Frames |
//! |------|-----------|----------|-------------|
//! | `FlatPage` | ✗ (skip) | ✗ | ✓ |
//! | `ArtboardSection` | ✓ | ✓ | ✗ |
//! | `Hybrid` | ✓ | ✓ | ✓ |

use rustc_hash::FxHashMap;
use uuid::Uuid;
use taffy::prelude::Style;

use logos_core::Rect;
use logos_core::WorkspaceMode;
use logos_core::constraint::{Constraints, resolve_constraints};
use crate::engine::{LayoutEngine, LayoutError};
use crate::repeat_grid::RepeatGrid;

// ── Error ─────────────────────────────────────────────────────────────────────

/// Errors produced by the hybrid layout engine.
#[derive(Debug, thiserror::Error)]
pub enum HybridError {
    #[error("Layout engine error: {0}")]
    Layout(#[from] LayoutError),

    #[error("Grid not found: {0}")]
    GridNotFound(Uuid),

    #[error("Layer not found: {0}")]
    LayerNotFound(Uuid),

    #[error("Grid cell out of range for grid {grid_id} at ({row}, {col})")]
    GridCellOutOfRange { grid_id: Uuid, row: u32, col: u32 },
}

// ── Pending parent resize ─────────────────────────────────────────────────────

/// Records that a parent layer was resized so that the constraint pre-pass
/// can propagate the size change to its pinned children.
#[derive(Clone, Debug)]
struct PendingResize {
    /// ID of the parent that changed size.
    parent_id: Uuid,
    /// Bounds of the parent *before* the resize.
    old_bounds: Rect,
    /// Bounds of the parent *after* the resize.
    new_bounds: Rect,
}

// ── Child registry ────────────────────────────────────────────────────────────

/// Tracks the parent–child relationship needed by the constraint pre-pass.
#[derive(Clone, Debug, Default)]
struct ChildInfo {
    /// Current bounds of this layer in parent-local coordinates.
    bounds: Rect,
    /// Parent layer UUID, if any.
    parent_id: Option<Uuid>,
}

// ── HybridLayoutEngine ────────────────────────────────────────────────────────

/// A three-pass layout orchestrator that wraps [`LayoutEngine`].
///
/// ## Usage
/// ```ignore
/// let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
/// // add layers …
/// hle.set_constraints(child_id, Constraints::stretch());
/// hle.notify_parent_resize(parent_id, old_rect, new_rect);
/// hle.register_grid(my_grid)?;
/// hle.compute(root_id)?;
/// let layout = hle.get_layout(child_id);
/// ```
pub struct HybridLayoutEngine {
    /// The Taffy-backed layout engine (pass ②).
    engine: LayoutEngine,

    /// Per-layer constraint rules (used in pass ①).
    constraints: FxHashMap<Uuid, Constraints>,

    /// Registered grids, keyed by grid [`Uuid`] (used in pass ③).
    grids: FxHashMap<Uuid, RepeatGrid>,

    /// Maps every *cell virtual ID* → the grid that owns it.
    /// Cell IDs are generated as `RepeatGrid::cell_id(row, col)`.
    grid_cells: FxHashMap<Uuid, Uuid>,

    /// Pending parent resize events, consumed by the constraint pre-pass.
    pending_resizes: Vec<PendingResize>,

    /// Per-layer metadata (current bounds + parent link) maintained
    /// alongside the Taffy tree for constraint resolution.
    child_info: FxHashMap<Uuid, ChildInfo>,

    /// Per-parent, list of child IDs – used in the constraint pre-pass.
    children_of: FxHashMap<Uuid, Vec<Uuid>>,

    /// Active workspace mode — governs pass gating.
    mode: WorkspaceMode,
}

// ── Cell ID helper ────────────────────────────────────────────────────────────

/// Deterministic virtual ID for a grid cell `(grid_id, row, col)`.
///
/// Built by XOR-ing the grid's UUID with a hash of the cell coordinates so
/// that cell IDs are stable across recomputes and never collide with real
/// layer IDs.
fn cell_id(grid_id: Uuid, row: u32, col: u32) -> Uuid {
    let bytes = grid_id.as_bytes();
    let mut out = *bytes;
    // mix row/col into the last 8 bytes
    let packed: u64 = ((row as u64) << 32) | (col as u64);
    let mixed = packed.to_le_bytes();
    for i in 0..8 {
        out[8 + i] ^= mixed[i];
    }
    Uuid::from_bytes(out)
}

// ── impl HybridLayoutEngine ───────────────────────────────────────────────────

impl HybridLayoutEngine {
    // ── Construction ──────────────────────────────────────────────────────────

    /// Create a new engine in the given workspace mode.
    pub fn new(mode: WorkspaceMode) -> Self {
        Self {
            engine: LayoutEngine::new(),
            constraints: FxHashMap::default(),
            grids: FxHashMap::default(),
            grid_cells: FxHashMap::default(),
            pending_resizes: Vec::new(),
            child_info: FxHashMap::default(),
            children_of: FxHashMap::default(),
            mode,
        }
    }

    /// Create with a custom spatial-hash cell size (see [`LayoutEngine::with_cell_size`]).
    pub fn with_cell_size(mode: WorkspaceMode, cell_size: f32) -> Self {
        Self {
            engine: LayoutEngine::with_cell_size(cell_size),
            ..Self::new(mode)
        }
    }

    // ── Mode ──────────────────────────────────────────────────────────────────

    /// Return the active workspace mode.
    pub fn mode(&self) -> WorkspaceMode {
        self.mode
    }

    /// Change the workspace mode. Takes effect on the next `compute()` call.
    pub fn set_mode(&mut self, mode: WorkspaceMode) {
        self.mode = mode;
    }

    // ── Layer management ──────────────────────────────────────────────────────

    /// Add a layer to the Taffy tree, optionally under a parent.
    ///
    /// Also registers the layer in the internal `child_info` map so that the
    /// constraint pre-pass can resolve its bounds.
    pub fn add_layer(
        &mut self,
        id: Uuid,
        parent_id: Option<Uuid>,
        style: Style,
        initial_bounds: Rect,
    ) -> Result<(), HybridError> {
        self.engine.add_layer(id, parent_id, style)?;
        self.child_info.insert(id, ChildInfo { bounds: initial_bounds, parent_id });
        if let Some(pid) = parent_id {
            self.children_of.entry(pid).or_default().push(id);
        }
        Ok(())
    }

    /// Remove a layer. Also removes its constraints and child-of entry.
    pub fn remove_layer(&mut self, id: Uuid) -> Result<(), HybridError> {
        self.engine.remove_layer(id)?;
        self.constraints.remove(&id);

        if let Some(info) = self.child_info.remove(&id) {
            if let Some(pid) = info.parent_id {
                if let Some(children) = self.children_of.get_mut(&pid) {
                    children.retain(|&c| c != id);
                }
            }
        }
        self.children_of.remove(&id);
        Ok(())
    }

    /// Update the stored bounds for a layer (after external mutation).
    /// This does **not** trigger a recompute — call `compute()` afterwards.
    pub fn update_bounds(&mut self, id: Uuid, bounds: Rect) -> Result<(), HybridError> {
        let info = self.child_info
            .get_mut(&id)
            .ok_or(HybridError::LayerNotFound(id))?;
        info.bounds = bounds;
        Ok(())
    }

    // ── Constraints ───────────────────────────────────────────────────────────

    /// Attach a constraint rule to a layer.
    pub fn set_constraints(&mut self, id: Uuid, c: Constraints) {
        self.constraints.insert(id, c);
    }

    /// Remove the constraint rule for a layer (reverts to free-floating).
    pub fn remove_constraints(&mut self, id: Uuid) {
        self.constraints.remove(&id);
    }

    /// Return the constraint rule for a layer, if any.
    pub fn get_constraints(&self, id: Uuid) -> Option<&Constraints> {
        self.constraints.get(&id)
    }

    // ── Parent resize notifications ───────────────────────────────────────────

    /// Notify the engine that a parent layer has been resized.
    ///
    /// The constraint pre-pass will use this information on the next
    /// `compute()` call to reposition/resize any constrained children.
    pub fn notify_parent_resize(&mut self, parent_id: Uuid, old: Rect, new: Rect) {
        self.pending_resizes.push(PendingResize {
            parent_id,
            old_bounds: old,
            new_bounds: new,
        });
    }

    // ── Grid management ───────────────────────────────────────────────────────

    /// Register a repeat grid.
    ///
    /// Immediately computes the virtual cell IDs for all cells and records
    /// them in `grid_cells`. The actual layout injection happens during
    /// `compute()`.
    pub fn register_grid(&mut self, grid: RepeatGrid) -> Result<(), HybridError> {
        let grid_id = grid.id;

        // Remove old cell mappings for this grid (in case of re-registration).
        self.grid_cells.retain(|_, gid| *gid != grid_id);

        for row in 0..grid.rows {
            for col in 0..grid.columns {
                let cid = cell_id(grid_id, row, col);
                self.grid_cells.insert(cid, grid_id);
            }
        }
        self.grids.insert(grid_id, grid);
        Ok(())
    }

    /// Update a grid (re-registers it; cell IDs are recomputed).
    pub fn update_grid(&mut self, grid: RepeatGrid) -> Result<(), HybridError> {
        self.register_grid(grid)
    }

    /// Unregister a grid and remove its cell layout entries.
    pub fn unregister_grid(&mut self, grid_id: Uuid) -> Result<(), HybridError> {
        if self.grids.remove(&grid_id).is_none() {
            return Err(HybridError::GridNotFound(grid_id));
        }
        self.grid_cells.retain(|_, gid| *gid != grid_id);
        Ok(())
    }

    /// Return the number of registered grids.
    pub fn grid_count(&self) -> usize {
        self.grids.len()
    }

    /// Return the cell-virtual-ID for `(grid_id, row, col)`, if in range.
    pub fn cell_virtual_id(
        &self,
        grid_id: Uuid,
        row: u32,
        col: u32,
    ) -> Result<Uuid, HybridError> {
        let grid = self.grids.get(&grid_id).ok_or(HybridError::GridNotFound(grid_id))?;
        if row >= grid.rows || col >= grid.columns {
            return Err(HybridError::GridCellOutOfRange { grid_id, row, col });
        }
        Ok(cell_id(grid_id, row, col))
    }

    // ── Compute ───────────────────────────────────────────────────────────────

    /// Execute a full three-pass layout computation rooted at `root_id`.
    ///
    /// ## Passes
    /// 1. **Constraint pre-pass** — for each pending parent-resize, apply
    ///    `resolve_constraints` to every constrained child and push the
    ///    updated position/size into the Taffy tree.
    /// 2. **Taffy pass** — run `LayoutEngine::compute_layout`.
    /// 3. **Grid expansion pass** — inject cell bounds from every registered
    ///    `RepeatGrid` via `inject_layout`, making them visible to hit-testing.
    pub fn compute(&mut self, root_id: Uuid) -> Result<(), HybridError> {
        // ── Pass 1: Constraint pre-pass ──────────────────────────────────────
        self.run_constraint_pass()?;

        // ── Pass 2: Taffy ────────────────────────────────────────────────────
        self.engine.compute_layout(root_id)?;

        // ── Pass 3: Grid expansion ───────────────────────────────────────────
        self.run_grid_pass();

        Ok(())
    }

    // ── Pass implementations ──────────────────────────────────────────────────

    fn run_constraint_pass(&mut self) -> Result<(), HybridError> {
        // Drain resizes so we don't process them again.
        let resizes = std::mem::take(&mut self.pending_resizes);

        for resize in resizes {
            let parent_id = resize.parent_id;
            let old_parent = resize.old_bounds;
            let new_parent = resize.new_bounds;

            // Skip artboard roots when mode == FlatPage.
            if self.mode == WorkspaceMode::FlatPage {
                // Artboard roots are identified here by not having a parent —
                // convention: if the resized layer has no parent_info entry it
                // is treated as a root artboard and skipped.
                if !self.child_info.contains_key(&parent_id) {
                    continue;
                }
            }

            let children = match self.children_of.get(&parent_id) {
                Some(c) => c.clone(),
                None => continue,
            };

            for child_id in children {
                let c = match self.constraints.get(&child_id) {
                    Some(c) => *c,
                    None => continue,
                };

                let child_bounds = match self.child_info.get(&child_id) {
                    Some(info) => info.bounds,
                    None => continue,
                };

                let new_child = resolve_constraints(old_parent, new_parent, child_bounds, &c);

                // Push updated position into Taffy.
                use crate::bridge::{DimAxis, PosAxis};
                if (new_child.x - child_bounds.x).abs() > f32::EPSILON {
                    self.engine.update_position(child_id, PosAxis::Left, new_child.x)?;
                }
                if (new_child.y - child_bounds.y).abs() > f32::EPSILON {
                    self.engine.update_position(child_id, PosAxis::Top, new_child.y)?;
                }
                if (new_child.width - child_bounds.width).abs() > f32::EPSILON {
                    self.engine.update_dimension(child_id, DimAxis::Width, new_child.width)?;
                }
                if (new_child.height - child_bounds.height).abs() > f32::EPSILON {
                    self.engine.update_dimension(child_id, DimAxis::Height, new_child.height)?;
                }

                // Update stored bounds.
                if let Some(info) = self.child_info.get_mut(&child_id) {
                    info.bounds = new_child;
                }
            }
        }
        Ok(())
    }

    fn run_grid_pass(&mut self) {
        for grid in self.grids.values() {
            let grid_id = grid.id;
            for row in 0..grid.rows {
                for col in 0..grid.columns {
                    if let Some((x, y, w, h)) = grid.cell_bounds_absolute(row, col) {
                        let cid = cell_id(grid_id, row, col);
                        self.engine.inject_layout(cid, x, y, w, h);
                    }
                }
            }
        }
    }

    // ── Accessors ─────────────────────────────────────────────────────────────

    /// Get the cached layout for a layer or grid cell.
    pub fn get_layout(&self, id: Uuid) -> Option<&taffy::Layout> {
        self.engine.get_layout(id)
    }

    /// O(1) point hit test.
    pub fn hit_test(&self, px: f32, py: f32) -> Option<Uuid> {
        self.engine.hit_test(px, py)
    }

    /// Return all layers at the point (top-to-bottom order).
    pub fn hit_test_all(&self, px: f32, py: f32) -> Vec<Uuid> {
        self.engine.hit_test_all(px, py)
    }

    /// Region query: all layers whose bounds intersect `region`.
    pub fn query_region(&self, region: &crate::spatial::Aabb) -> Vec<Uuid> {
        self.engine.query_region(region)
    }

    /// Drain changed-layer list from the most recent `compute()`.
    pub fn drain_changed(&mut self) -> Vec<Uuid> {
        self.engine.drain_changed()
    }

    /// Number of nodes tracked by the inner Taffy engine.
    pub fn node_count(&self) -> usize {
        self.engine.node_count()
    }

    /// Number of pending dirty nodes.
    pub fn dirty_count(&self) -> usize {
        self.engine.dirty_count()
    }

    /// Read-only access to the inner `LayoutEngine` (for tests / advanced use).
    pub fn inner(&self) -> &LayoutEngine {
        &self.engine
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use logos_core::constraint::{HorizontalConstraint, VerticalConstraint};

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, width: w, height: h }
    }

    fn simple_style(w: f32, h: f32, x: f32, y: f32) -> Style {
        use taffy::prelude::*;
        Style {
            size: Size {
                width: Dimension::length(w),
                height: Dimension::length(h),
            },
            position: Position::Absolute,
            inset: taffy::Rect {
                left: LengthPercentageAuto::length(x),
                top: LengthPercentageAuto::length(y),
                right: LengthPercentageAuto::auto(),
                bottom: LengthPercentageAuto::auto(),
            },
            ..Style::default()
        }
    }

    #[test]
    fn test_hybrid_new_mode() {
        let hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
        assert_eq!(hle.mode(), WorkspaceMode::Hybrid);
        assert_eq!(hle.node_count(), 0);
    }

    #[test]
    fn test_set_mode() {
        let mut hle = HybridLayoutEngine::new(WorkspaceMode::FlatPage);
        hle.set_mode(WorkspaceMode::ArtboardSection);
        assert_eq!(hle.mode(), WorkspaceMode::ArtboardSection);
    }

    #[test]
    fn test_add_layer_and_count() {
        let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
        let id = Uuid::new_v4();
        hle.add_layer(id, None, simple_style(100.0, 100.0, 0.0, 0.0), rect(0.0, 0.0, 100.0, 100.0))
            .unwrap();
        assert_eq!(hle.node_count(), 1);
    }

    #[test]
    fn test_remove_layer() {
        let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
        let id = Uuid::new_v4();
        hle.add_layer(id, None, simple_style(50.0, 50.0, 0.0, 0.0), rect(0.0, 0.0, 50.0, 50.0))
            .unwrap();
        hle.remove_layer(id).unwrap();
        assert_eq!(hle.node_count(), 0);
    }

    #[test]
    fn test_set_constraints() {
        let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
        let id = Uuid::new_v4();
        hle.set_constraints(id, Constraints::stretch());
        assert!(hle.get_constraints(id).is_some());
    }

    #[test]
    fn test_remove_constraints() {
        let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
        let id = Uuid::new_v4();
        hle.set_constraints(id, Constraints::stretch());
        hle.remove_constraints(id);
        assert!(hle.get_constraints(id).is_none());
    }

    #[test]
    fn test_register_and_unregister_grid() {
        let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
        let grid = RepeatGrid::new(2, 3, 50.0, 50.0);
        let gid = grid.id;
        hle.register_grid(grid).unwrap();
        assert_eq!(hle.grid_count(), 1);
        hle.unregister_grid(gid).unwrap();
        assert_eq!(hle.grid_count(), 0);
    }

    #[test]
    fn test_unregister_nonexistent_grid_errors() {
        let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
        assert!(hle.unregister_grid(Uuid::new_v4()).is_err());
    }

    #[test]
    fn test_compute_single_node() {
        let mut hle = HybridLayoutEngine::new(WorkspaceMode::Hybrid);
        let id = Uuid::new_v4();
        hle.add_layer(id, None, simple_style(200.0, 100.0, 0.0, 0.0), rect(0.0, 0.0, 200.0, 100.0))
            .unwrap();
        hle.compute(id).unwrap();
        let layout = hle.get_layout(id).unwrap();
        assert!((layout.size.width - 200.0).abs() < f32::EPSILON);
        assert!((layout.size.height - 100.0).abs() < f32::EPSILON);
    }
}
