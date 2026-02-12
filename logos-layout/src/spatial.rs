//! Spatial Hash Grid for O(1) average-case hit testing.
//!
//! Divides 2-D space into uniform cells.  Each layer occupies one or more
//! cells based on its axis-aligned bounding box.  Point queries only inspect
//! the target cell and its 8 neighbours — constant work regardless of total
//! layer count.
//!
//! # Design decisions
//!
//! * **No heap allocation on the hit-test path.**  The 3×3 neighbourhood is
//!   iterated with a fixed-size loop — no `Vec`, no iterator adaptor.
//! * **`Aabb` is `#[repr(C)]`** for predictable memory layout and
//!   cache-friendliness (16 bytes).
//! * **Separate `bounds_cache`** allows O(1) removal without scanning the
//!   grid.

use rustc_hash::{FxHashMap, FxHashSet};
use uuid::Uuid;

// ───────────────────────────────────────────────────────────────────
// Aabb — Axis-Aligned Bounding Box
// ───────────────────────────────────────────────────────────────────

/// Compact AABB stored as min/max corners (16 bytes total).
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct Aabb {
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
}

impl Aabb {
    /// Create from origin + size (design-tool convention).
    #[inline(always)]
    pub fn from_rect(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            min_x: x,
            min_y: y,
            max_x: x + width,
            max_y: y + height,
        }
    }

    /// Point-in-AABB test.  Branchless-friendly — four comparisons.
    #[inline(always)]
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.min_x && px <= self.max_x && py >= self.min_y && py <= self.max_y
    }

    /// AABB overlap test.
    #[inline(always)]
    pub fn intersects(&self, other: &Aabb) -> bool {
        self.min_x <= other.max_x
            && self.max_x >= other.min_x
            && self.min_y <= other.max_y
            && self.max_y >= other.min_y
    }

    #[inline(always)]
    pub fn width(&self) -> f32 {
        self.max_x - self.min_x
    }

    #[inline(always)]
    pub fn height(&self) -> f32 {
        self.max_y - self.min_y
    }
}

// ───────────────────────────────────────────────────────────────────
// Cell key
// ───────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct CellKey(i32, i32);

// ───────────────────────────────────────────────────────────────────
// SpatialHash
// ───────────────────────────────────────────────────────────────────

/// Grid-based spatial hash for constant-time point queries.
#[derive(Clone)]
pub struct SpatialHash {
    /// Width/height of each grid cell in world units.
    #[allow(dead_code)]
    cell_size: f32,
    /// Inverse of cell_size (cached to replace division with multiplication).
    inv_cell_size: f32,
    /// Grid: cell → list of layer ids occupying that cell.
    grid: FxHashMap<CellKey, Vec<Uuid>>,
    /// Per-layer bounds cache for removal and precise hit testing.
    bounds: FxHashMap<Uuid, Aabb>,
}

impl SpatialHash {
    /// Create a new spatial hash with the given cell size.
    ///
    /// **Choosing cell_size:** Use the median layer dimension.  For typical
    /// design documents with 50–200 px elements, `cell_size = 128.0` works
    /// well.
    pub fn new(cell_size: f32) -> Self {
        assert!(cell_size > 0.0, "cell_size must be positive");
        Self {
            cell_size,
            inv_cell_size: 1.0 / cell_size,
            grid: FxHashMap::default(),
            bounds: FxHashMap::default(),
        }
    }

    // ───────────────────── helpers ─────────────────────

    #[inline(always)]
    fn to_cell(&self, x: f32, y: f32) -> CellKey {
        CellKey(
            (x * self.inv_cell_size).floor() as i32,
            (y * self.inv_cell_size).floor() as i32,
        )
    }

    /// Compute the cell range covered by an AABB.
    #[inline]
    fn cell_range(&self, aabb: &Aabb) -> (CellKey, CellKey) {
        let min = self.to_cell(aabb.min_x, aabb.min_y);
        let max = self.to_cell(aabb.max_x, aabb.max_y);
        (min, max)
    }

    // ───────────────────── mutation ─────────────────────

    /// Insert (or update) a layer in the spatial hash.
    ///
    /// If the layer already exists it is removed first, then re-inserted
    /// with the new bounds.
    #[inline]
    pub fn insert(&mut self, id: Uuid, aabb: Aabb) {
        // Only pay for remove if we know the id exists.
        if self.bounds.contains_key(&id) {
            self.remove(id);
        }

        self.bounds.insert(id, aabb);
        let (min, max) = self.cell_range(&aabb);
        for cx in min.0..=max.0 {
            for cy in min.1..=max.1 {
                self.grid.entry(CellKey(cx, cy)).or_default().push(id);
            }
        }
    }

    /// Remove a layer.  No-op if the id is unknown.
    #[inline]
    pub fn remove(&mut self, id: Uuid) {
        if let Some(aabb) = self.bounds.remove(&id) {
            let (min, max) = self.cell_range(&aabb);
            for cx in min.0..=max.0 {
                for cy in min.1..=max.1 {
                    let key = CellKey(cx, cy);
                    if let Some(ids) = self.grid.get_mut(&key) {
                        // swap_remove is O(1) vs retain's O(n).
                        // Order within a cell isn't meaningful for spatial
                        // queries — hit_test uses bounds.contains() for
                        // correctness, not cell-vec ordering.
                        if let Some(pos) = ids.iter().position(|&x| x == id) {
                            ids.swap_remove(pos);
                        }
                        if ids.is_empty() {
                            self.grid.remove(&key);
                        }
                    }
                }
            }
        }
    }

    /// Remove all layers.
    pub fn clear(&mut self) {
        self.grid.clear();
        self.bounds.clear();
    }

    // ───────────────────── queries ─────────────────────

    /// **O(1) average** point hit test.
    ///
    /// Returns the **topmost** (last-inserted) layer whose AABB contains
    /// the point, or `None`.
    ///
    /// Zero heap allocations.  Checks the center cell first (fast path),
    /// then only the 8 neighbours if no hit was found in the center.
    #[inline]
    pub fn hit_test(&self, px: f32, py: f32) -> Option<Uuid> {
        let center = self.to_cell(px, py);

        // Fast path: check center cell only (handles >90% of cases when
        // layers are smaller than cell_size).
        if let Some(ids) = self.grid.get(&center) {
            for &id in ids.iter().rev() {
                if let Some(aabb) = self.bounds.get(&id) {
                    if aabb.contains(px, py) {
                        return Some(id);
                    }
                }
            }
        }

        // Slow path: check 8 neighbours for layers spanning cell boundaries.
        for dx in -1_i32..=1 {
            for dy in -1_i32..=1 {
                if dx == 0 && dy == 0 {
                    continue; // already checked
                }
                let key = CellKey(center.0 + dx, center.1 + dy);
                if let Some(ids) = self.grid.get(&key) {
                    for &id in ids.iter().rev() {
                        if let Some(aabb) = self.bounds.get(&id) {
                            if aabb.contains(px, py) {
                                return Some(id);
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// Hit test returning **all** layers at the point (top-to-bottom order).
    pub fn hit_test_all(&self, px: f32, py: f32) -> Vec<Uuid> {
        let center = self.to_cell(px, py);
        let mut result = Vec::new();
        let mut seen = FxHashSet::default();

        for dx in -1_i32..=1 {
            for dy in -1_i32..=1 {
                let key = CellKey(center.0 + dx, center.1 + dy);
                if let Some(ids) = self.grid.get(&key) {
                    for &id in ids.iter().rev() {
                        if seen.insert(id) {
                            if let Some(aabb) = self.bounds.get(&id) {
                                if aabb.contains(px, py) {
                                    result.push(id);
                                }
                            }
                        }
                    }
                }
            }
        }
        result
    }

    /// Region query: return all layers whose AABB intersects the given rect.
    pub fn query_region(&self, region: &Aabb) -> Vec<Uuid> {
        let mut result = Vec::new();
        let mut seen = FxHashSet::default();

        let (min, max) = self.cell_range(region);
        for cx in min.0..=max.0 {
            for cy in min.1..=max.1 {
                let key = CellKey(cx, cy);
                if let Some(ids) = self.grid.get(&key) {
                    for &id in ids {
                        if seen.insert(id) {
                            if let Some(aabb) = self.bounds.get(&id) {
                                if aabb.intersects(region) {
                                    result.push(id);
                                }
                            }
                        }
                    }
                }
            }
        }
        result
    }

    // ───────────────────── stats ─────────────────────

    /// Number of layers tracked.
    #[inline]
    pub fn len(&self) -> usize {
        self.bounds.len()
    }

    /// Whether the hash is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.bounds.is_empty()
    }

    /// Number of occupied grid cells.
    #[inline]
    pub fn cell_count(&self) -> usize {
        self.grid.len()
    }

    /// Memory estimate in bytes (approximate).
    pub fn memory_bytes(&self) -> usize {
        let bounds_size = self.bounds.capacity()
            * (std::mem::size_of::<Uuid>() + std::mem::size_of::<Aabb>());
        let grid_overhead: usize = self.grid.values().map(|v| v.capacity() * 16).sum();
        let grid_keys = self.grid.capacity()
            * (std::mem::size_of::<CellKey>() + std::mem::size_of::<Vec<Uuid>>());
        bounds_size + grid_overhead + grid_keys
    }
}

// ===================================================================
// Tests
// ===================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn uid() -> Uuid {
        Uuid::new_v4()
    }

    // ─────────────── Aabb tests ───────────────

    #[test]
    fn test_aabb_from_rect() {
        let b = Aabb::from_rect(10.0, 20.0, 100.0, 50.0);
        assert_eq!(b.min_x, 10.0);
        assert_eq!(b.min_y, 20.0);
        assert_eq!(b.max_x, 110.0);
        assert_eq!(b.max_y, 70.0);
    }

    #[test]
    fn test_aabb_contains_inside() {
        let b = Aabb::from_rect(0.0, 0.0, 100.0, 100.0);
        assert!(b.contains(50.0, 50.0));
    }

    #[test]
    fn test_aabb_contains_edge() {
        let b = Aabb::from_rect(0.0, 0.0, 100.0, 100.0);
        assert!(b.contains(0.0, 0.0));
        assert!(b.contains(100.0, 100.0));
        assert!(b.contains(100.0, 0.0));
        assert!(b.contains(0.0, 100.0));
    }

    #[test]
    fn test_aabb_contains_outside() {
        let b = Aabb::from_rect(10.0, 10.0, 50.0, 50.0);
        assert!(!b.contains(0.0, 0.0));
        assert!(!b.contains(100.0, 100.0));
        assert!(!b.contains(10.0, 61.0));
        assert!(!b.contains(61.0, 10.0));
    }

    #[test]
    fn test_aabb_intersects() {
        let a = Aabb::from_rect(0.0, 0.0, 100.0, 100.0);
        let b = Aabb::from_rect(50.0, 50.0, 100.0, 100.0);
        assert!(a.intersects(&b));
        assert!(b.intersects(&a));
    }

    #[test]
    fn test_aabb_no_intersect() {
        let a = Aabb::from_rect(0.0, 0.0, 10.0, 10.0);
        let b = Aabb::from_rect(20.0, 20.0, 10.0, 10.0);
        assert!(!a.intersects(&b));
    }

    #[test]
    fn test_aabb_size() {
        // Aabb should be exactly 16 bytes (4 × f32).
        assert_eq!(std::mem::size_of::<Aabb>(), 16);
    }

    // ─────────────── SpatialHash basic tests ───────────────

    #[test]
    fn test_new_hash() {
        let sh = SpatialHash::new(128.0);
        assert_eq!(sh.len(), 0);
        assert!(sh.is_empty());
        assert_eq!(sh.cell_count(), 0);
    }

    #[test]
    #[should_panic]
    fn test_new_hash_zero_cell_size() {
        let _ = SpatialHash::new(0.0);
    }

    #[test]
    fn test_insert_single() {
        let mut sh = SpatialHash::new(128.0);
        let id = uid();
        sh.insert(id, Aabb::from_rect(10.0, 10.0, 50.0, 50.0));
        assert_eq!(sh.len(), 1);
        assert!(!sh.is_empty());
    }

    #[test]
    fn test_remove_single() {
        let mut sh = SpatialHash::new(128.0);
        let id = uid();
        sh.insert(id, Aabb::from_rect(10.0, 10.0, 50.0, 50.0));
        sh.remove(id);
        assert_eq!(sh.len(), 0);
        assert!(sh.is_empty());
        assert_eq!(sh.cell_count(), 0);
    }

    #[test]
    fn test_remove_unknown_noop() {
        let mut sh = SpatialHash::new(128.0);
        sh.remove(uid()); // Should not panic.
        assert_eq!(sh.len(), 0);
    }

    #[test]
    fn test_clear() {
        let mut sh = SpatialHash::new(128.0);
        for _ in 0..10 {
            sh.insert(uid(), Aabb::from_rect(0.0, 0.0, 50.0, 50.0));
        }
        sh.clear();
        assert_eq!(sh.len(), 0);
        assert_eq!(sh.cell_count(), 0);
    }

    // ─────────────── Hit testing ───────────────

    #[test]
    fn test_hit_test_single() {
        let mut sh = SpatialHash::new(128.0);
        let id = uid();
        sh.insert(id, Aabb::from_rect(10.0, 10.0, 50.0, 50.0));

        assert_eq!(sh.hit_test(30.0, 30.0), Some(id));
        assert_eq!(sh.hit_test(0.0, 0.0), None);
        assert_eq!(sh.hit_test(100.0, 100.0), None);
    }

    #[test]
    fn test_hit_test_edge() {
        let mut sh = SpatialHash::new(128.0);
        let id = uid();
        sh.insert(id, Aabb::from_rect(0.0, 0.0, 100.0, 100.0));

        // Edges should be hits (inclusive bounds).
        assert_eq!(sh.hit_test(0.0, 0.0), Some(id));
        assert_eq!(sh.hit_test(100.0, 100.0), Some(id));
    }

    #[test]
    fn test_hit_test_overlapping() {
        let mut sh = SpatialHash::new(128.0);
        let id1 = uid();
        let id2 = uid();
        sh.insert(id1, Aabb::from_rect(0.0, 0.0, 100.0, 100.0));
        sh.insert(id2, Aabb::from_rect(50.0, 50.0, 100.0, 100.0));

        // Point in overlap — last-inserted (id2) wins.
        let hit = sh.hit_test(75.0, 75.0);
        assert_eq!(hit, Some(id2));

        // Point only in id1.
        assert_eq!(sh.hit_test(10.0, 10.0), Some(id1));

        // Point only in id2.
        assert_eq!(sh.hit_test(140.0, 140.0), Some(id2));
    }

    #[test]
    fn test_hit_test_after_remove() {
        let mut sh = SpatialHash::new(128.0);
        let id = uid();
        sh.insert(id, Aabb::from_rect(0.0, 0.0, 100.0, 100.0));
        assert_eq!(sh.hit_test(50.0, 50.0), Some(id));

        sh.remove(id);
        assert_eq!(sh.hit_test(50.0, 50.0), None);
    }

    #[test]
    fn test_hit_test_empty() {
        let sh = SpatialHash::new(128.0);
        assert_eq!(sh.hit_test(50.0, 50.0), None);
    }

    #[test]
    fn test_hit_test_negative_coords() {
        let mut sh = SpatialHash::new(128.0);
        let id = uid();
        sh.insert(id, Aabb::from_rect(-50.0, -50.0, 100.0, 100.0));

        assert_eq!(sh.hit_test(0.0, 0.0), Some(id));
        assert_eq!(sh.hit_test(-25.0, -25.0), Some(id));
        assert_eq!(sh.hit_test(-60.0, -60.0), None);
    }

    #[test]
    fn test_hit_test_across_cell_boundary() {
        // Layer spans multiple cells.
        let mut sh = SpatialHash::new(64.0);
        let id = uid();
        sh.insert(id, Aabb::from_rect(50.0, 50.0, 100.0, 100.0)); // spans cells

        assert_eq!(sh.hit_test(60.0, 60.0), Some(id));
        assert_eq!(sh.hit_test(120.0, 120.0), Some(id));
    }

    // ─────────────── hit_test_all ───────────────

    #[test]
    fn test_hit_test_all_overlapping() {
        let mut sh = SpatialHash::new(128.0);
        let id1 = uid();
        let id2 = uid();
        let id3 = uid();
        sh.insert(id1, Aabb::from_rect(0.0, 0.0, 100.0, 100.0));
        sh.insert(id2, Aabb::from_rect(20.0, 20.0, 100.0, 100.0));
        sh.insert(id3, Aabb::from_rect(40.0, 40.0, 100.0, 100.0));

        let hits = sh.hit_test_all(50.0, 50.0);
        assert_eq!(hits.len(), 3);
        assert!(hits.contains(&id1));
        assert!(hits.contains(&id2));
        assert!(hits.contains(&id3));
    }

    // ─────────────── query_region ───────────────

    #[test]
    fn test_query_region() {
        let mut sh = SpatialHash::new(128.0);
        let id1 = uid();
        let id2 = uid();
        let id3 = uid();
        sh.insert(id1, Aabb::from_rect(0.0, 0.0, 50.0, 50.0));
        sh.insert(id2, Aabb::from_rect(200.0, 200.0, 50.0, 50.0));
        sh.insert(id3, Aabb::from_rect(400.0, 400.0, 50.0, 50.0));

        let region = Aabb::from_rect(0.0, 0.0, 250.0, 250.0);
        let results = sh.query_region(&region);

        assert_eq!(results.len(), 2);
        assert!(results.contains(&id1));
        assert!(results.contains(&id2));
        assert!(!results.contains(&id3));
    }

    // ─────────────── Update (re-insert) ───────────────

    #[test]
    fn test_update_layer_bounds() {
        let mut sh = SpatialHash::new(128.0);
        let id = uid();
        sh.insert(id, Aabb::from_rect(0.0, 0.0, 50.0, 50.0));

        assert_eq!(sh.hit_test(25.0, 25.0), Some(id));
        assert_eq!(sh.hit_test(200.0, 200.0), None);

        // Move the layer.
        sh.insert(id, Aabb::from_rect(180.0, 180.0, 50.0, 50.0));

        assert_eq!(sh.hit_test(25.0, 25.0), None);
        assert_eq!(sh.hit_test(200.0, 200.0), Some(id));
        assert_eq!(sh.len(), 1);
    }

    // ─────────────── Large scale ───────────────

    #[test]
    fn test_10k_layers_hit_test() {
        let mut sh = SpatialHash::new(128.0);
        let mut ids = Vec::with_capacity(10_000);

        // 100×100 grid of 50×50 layers with 10px gaps.
        for row in 0..100 {
            for col in 0..100 {
                let id = uid();
                let x = col as f32 * 60.0;
                let y = row as f32 * 60.0;
                sh.insert(id, Aabb::from_rect(x, y, 50.0, 50.0));
                ids.push((id, x, y));
            }
        }

        assert_eq!(sh.len(), 10_000);

        // Hit test a point inside each of the first 10 layers.
        for (id, x, y) in ids.iter().take(10) {
            assert_eq!(sh.hit_test(x + 25.0, y + 25.0), Some(*id));
        }

        // Hit test a point in the gap — should miss.
        assert_eq!(sh.hit_test(55.0, 55.0), None);
    }

    // ─────────────── Memory overhead ───────────────

    #[test]
    fn test_memory_overhead_per_layer() {
        let mut sh = SpatialHash::new(128.0);
        let n = 1_000;

        for i in 0..n {
            let x = (i % 100) as f32 * 60.0;
            let y = (i / 100) as f32 * 60.0;
            sh.insert(uid(), Aabb::from_rect(x, y, 50.0, 50.0));
        }

        let bytes_per_layer = sh.memory_bytes() / n;
        // Target: < 128 bytes/layer (generous for HashMap overhead).
        // Actual expectation with HashMap: ~80–120 bytes.
        assert!(
            bytes_per_layer < 128,
            "Memory overhead too high: {} bytes/layer",
            bytes_per_layer
        );
    }
}
