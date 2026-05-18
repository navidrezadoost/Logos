//! Anchor — a spatial point in the vector network with optional Bézier handles.
//!
//! Each anchor holds:
//!
//! - `(x, y)` — position in canvas-local coordinates.
//! - `handle_in`  — optional incoming Bézier control point (relative to anchor).
//! - `handle_out` — optional outgoing Bézier control point (relative to anchor).
//! - `incident_segments` — indices into `VectorNetwork::segments` for all
//!   segments that start **or** end at this anchor.
//!
//! Handles are stored as **absolute** coordinates (same space as `x`/`y`).
//! A `None` handle means the anchor is a corner (no tangent smoothing).

/// A point in the vector network with optional Bézier control handles and
/// a list of all segments connected to it.
#[derive(Debug, Clone, PartialEq)]
pub struct Anchor {
    /// Canvas-local X coordinate.
    pub x: f64,
    /// Canvas-local Y coordinate.
    pub y: f64,
    /// Incoming Bézier control point (absolute coordinates).
    /// `None` → sharp corner on the incoming side.
    pub handle_in: Option<(f64, f64)>,
    /// Outgoing Bézier control point (absolute coordinates).
    /// `None` → sharp corner on the outgoing side.
    pub handle_out: Option<(f64, f64)>,
    /// Indices into `VectorNetwork::segments` for all segments that start
    /// or end at this anchor. Maintained automatically by the graph.
    incident_segments: Vec<usize>,
}

impl Anchor {
    /// Create a new anchor at the given position with no handles.
    pub fn new(x: f64, y: f64) -> Self {
        Self {
            x,
            y,
            handle_in: None,
            handle_out: None,
            incident_segments: Vec::new(),
        }
    }

    /// Create an anchor with explicit handles.
    pub fn with_handles(
        x: f64,
        y: f64,
        handle_in: Option<(f64, f64)>,
        handle_out: Option<(f64, f64)>,
    ) -> Self {
        Self {
            x,
            y,
            handle_in,
            handle_out,
            incident_segments: Vec::new(),
        }
    }

    /// Read-only view of the incident segment index list.
    pub fn incident_segments(&self) -> &[usize] {
        &self.incident_segments
    }

    /// Returns `true` if this anchor has no incident segments (it is isolated).
    pub fn is_isolated(&self) -> bool {
        self.incident_segments.is_empty()
    }

    /// Returns the degree of this anchor (number of connected segments).
    pub fn degree(&self) -> usize {
        self.incident_segments.len()
    }

    // ── Internal mutation — called only by VectorNetwork ─────────────────────

    /// Register a new incident segment. Panics in debug if already present.
    pub(crate) fn add_incident(&mut self, segment_id: usize) {
        debug_assert!(
            !self.incident_segments.contains(&segment_id),
            "segment {segment_id} already incident on this anchor"
        );
        self.incident_segments.push(segment_id);
    }

    /// Remove an incident segment. No-op if not present.
    pub(crate) fn remove_incident(&mut self, segment_id: usize) {
        self.incident_segments.retain(|&s| s != segment_id);
    }
}

impl Default for Anchor {
    fn default() -> Self {
        Self::new(0.0, 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_anchor_is_isolated() {
        let a = Anchor::new(10.0, 20.0);
        assert_eq!(a.x, 10.0);
        assert_eq!(a.y, 20.0);
        assert!(a.handle_in.is_none());
        assert!(a.handle_out.is_none());
        assert!(a.is_isolated());
        assert_eq!(a.degree(), 0);
    }

    #[test]
    fn add_remove_incident() {
        let mut a = Anchor::new(0.0, 0.0);
        a.add_incident(0);
        a.add_incident(3);
        assert_eq!(a.degree(), 2);
        assert_eq!(a.incident_segments(), &[0, 3]);
        a.remove_incident(0);
        assert_eq!(a.incident_segments(), &[3]);
        // Remove non-existent — no panic
        a.remove_incident(99);
        assert_eq!(a.degree(), 1);
    }

    #[test]
    fn anchor_with_handles() {
        let a = Anchor::with_handles(5.0, 5.0, Some((4.0, 5.0)), Some((6.0, 5.0)));
        assert_eq!(a.handle_in, Some((4.0, 5.0)));
        assert_eq!(a.handle_out, Some((6.0, 5.0)));
    }
}
