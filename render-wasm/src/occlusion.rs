/// Per-tile opaque coverage tracker for occlusion culling (P1.6).
///
/// During tile rendering we do a front-to-back pre-scan of the root-level shapes
/// assigned to the tile and record the bounding rectangles of shapes that are:
///   • fully opaque (opacity == 1.0, normal blend mode)
///   • rectangular / frame shapes without complex clips
///
/// Shapes whose bounds are entirely covered by previously-accumulated opaque
/// regions are skipped, saving GPU work on occluded content.
use crate::math::Rect;

pub struct OpaqueCoverage {
    opaque_regions: Vec<Rect>,
}

impl OpaqueCoverage {
    pub fn new() -> Self {
        OpaqueCoverage {
            opaque_regions: Vec::new(),
        }
    }

    /// Register `bounds` as a new opaque region.
    pub fn add_opaque_rect(&mut self, bounds: Rect) {
        self.opaque_regions.push(bounds);
    }

    /// Returns `true` if every pixel of `bounds` is already covered by at least
    /// one previously-added opaque region (i.e. the shape is completely hidden).
    pub fn is_fully_occluded(&self, bounds: &Rect) -> bool {
        self.opaque_regions.iter().any(|r| r.contains(*bounds))
    }

    pub fn clear(&mut self) {
        self.opaque_regions.clear();
    }
}
