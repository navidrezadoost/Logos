//! Region — a closed cycle of segments forming a filled area (V2 placeholder).
//!
//! In V1, `Region` is a minimal stub: the data structure is defined so the
//! rest of the crate can reference it, but cycle detection (DFS on the half-
//! edge graph) is deferred to V2.
//!
//! A region is a closed walk through the segment graph: an ordered list of
//! segment indices such that the end anchor of segment[i] equals the start
//! anchor of segment[i+1], and segment[last].end == segment[0].start.
//!
//! Each region can carry its own fill (independent of the shape's fill).
//! This is what allows different enclosed areas in a complex vector network
//! to have different colors — a key external design tool capability Logos will match.

/// A closed region (filled area) in a vector network.
///
/// Defined by an ordered list of segment indices forming a closed walk.
/// Fill is optional; a region with no fill is "empty" (transparent hole).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Region {
    /// Ordered segment indices forming the closed boundary of this region.
    /// `boundary[i].end_anchor == boundary[i+1].start_anchor` (mod len).
    pub boundary: Vec<usize>,

    /// Optional fill color for this region as `0xAARRGGBB`.
    /// `None` → use the shape's default fill.
    pub fill_argb: Option<u32>,
}

impl Region {
    /// Create a new empty region with no boundary and no fill.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a region from an ordered boundary and an optional fill.
    pub fn with_boundary(boundary: Vec<usize>, fill_argb: Option<u32>) -> Self {
        Self {
            boundary,
            fill_argb,
        }
    }

    /// Returns `true` if the boundary list is empty (degenerate region).
    pub fn is_empty(&self) -> bool {
        self.boundary.is_empty()
    }

    /// Number of segments in the boundary.
    pub fn len(&self) -> usize {
        self.boundary.len()
    }
}
