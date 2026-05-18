//! Segment — a directed cubic Bézier edge connecting two anchors.
//!
//! A segment stores:
//!
//! - `start_anchor`, `end_anchor` — indices into `VectorNetwork::anchors`.
//! - `control1`, `control2` — the two cubic Bézier control points
//!   (absolute coordinates, matching SVG / Skia `cubicTo` convention).
//!
//! Both control points are optional. When absent the segment degrades to
//! a quadratic Bézier (one control point) or a straight line (neither).
//!
//! The segment is **directed**: `start_anchor → end_anchor`. For undirected
//! traversal the graph maintains incident lists on both endpoints.

/// A directed cubic Bézier edge in the vector network.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    /// Index of the starting anchor.
    pub start_anchor: usize,
    /// Index of the ending anchor.
    pub end_anchor: usize,
    /// First cubic Bézier control point (absolute).
    /// Corresponds to the tangent at `start_anchor`.
    /// `None` → use `start_anchor` position (straight / quadratic).
    pub control1: Option<(f64, f64)>,
    /// Second cubic Bézier control point (absolute).
    /// Corresponds to the tangent at `end_anchor`.
    /// `None` → use `end_anchor` position (straight / quadratic).
    pub control2: Option<(f64, f64)>,
}

impl Segment {
    /// Create a straight-line segment between two anchors.
    pub fn line(start_anchor: usize, end_anchor: usize) -> Self {
        Self {
            start_anchor,
            end_anchor,
            control1: None,
            control2: None,
        }
    }

    /// Create a full cubic Bézier segment.
    pub fn cubic(
        start_anchor: usize,
        end_anchor: usize,
        control1: (f64, f64),
        control2: (f64, f64),
    ) -> Self {
        Self {
            start_anchor,
            end_anchor,
            control1: Some(control1),
            control2: Some(control2),
        }
    }

    /// Returns `true` if both control points are absent (straight line).
    pub fn is_line(&self) -> bool {
        self.control1.is_none() && self.control2.is_none()
    }

    /// Returns `true` if both control points are present (full cubic Bézier).
    pub fn is_cubic(&self) -> bool {
        self.control1.is_some() && self.control2.is_some()
    }

    /// Returns the other anchor index given one endpoint.
    /// Returns `None` if `anchor_id` is not an endpoint of this segment.
    pub fn other_end(&self, anchor_id: usize) -> Option<usize> {
        if self.start_anchor == anchor_id {
            Some(self.end_anchor)
        } else if self.end_anchor == anchor_id {
            Some(self.start_anchor)
        } else {
            None
        }
    }

    /// Returns `true` if the given anchor is an endpoint of this segment.
    pub fn has_anchor(&self, anchor_id: usize) -> bool {
        self.start_anchor == anchor_id || self.end_anchor == anchor_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_segment() {
        let s = Segment::line(0, 1);
        assert_eq!(s.start_anchor, 0);
        assert_eq!(s.end_anchor, 1);
        assert!(s.is_line());
        assert!(!s.is_cubic());
    }

    #[test]
    fn cubic_segment() {
        let s = Segment::cubic(0, 1, (10.0, 0.0), (90.0, 0.0));
        assert!(s.is_cubic());
        assert!(!s.is_line());
        assert_eq!(s.control1, Some((10.0, 0.0)));
        assert_eq!(s.control2, Some((90.0, 0.0)));
    }

    #[test]
    fn other_end() {
        let s = Segment::line(2, 7);
        assert_eq!(s.other_end(2), Some(7));
        assert_eq!(s.other_end(7), Some(2));
        assert_eq!(s.other_end(5), None);
    }

    #[test]
    fn has_anchor() {
        let s = Segment::line(3, 9);
        assert!(s.has_anchor(3));
        assert!(s.has_anchor(9));
        assert!(!s.has_anchor(0));
    }
}
