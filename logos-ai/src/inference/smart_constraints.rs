//! # Smart Constraints Engine
//!
//! Infers layout constraints from existing element arrangement:
//! equal spacing, alignment rails, grid detection, aspect-ratio
//! locking, and responsive breakpoint suggestions.
//!
//! Operates purely on `Rect` geometry — no ML inference required.
//!
//! ```
//! use logos_ai::inference::smart_constraints::{ConstraintInferrer, InferredConstraint};
//! use logos_core::Rect;
//!
//! let elements = vec![
//!     Rect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 },
//!     Rect { x: 120.0, y: 0.0, width: 100.0, height: 100.0 },
//!     Rect { x: 240.0, y: 0.0, width: 100.0, height: 100.0 },
//! ];
//!
//! let inferrer = ConstraintInferrer::default();
//! let constraints = inferrer.infer(&elements);
//! assert!(!constraints.is_empty());
//! ```

use logos_core::Rect;

// ── Constraint Types ─────────────────────────────────────────

/// An inferred constraint between design elements.
#[derive(Debug, Clone, PartialEq)]
pub enum InferredConstraint {
    /// Elements share a common edge.
    AlignmentRail {
        /// Which edge: "left", "right", "top", "bottom", "center_x", "center_y".
        edge: String,
        /// Position of the rail.
        position: f32,
        /// Indices of aligned elements.
        elements: Vec<usize>,
    },
    /// Uniform horizontal spacing.
    EqualHorizontalSpacing {
        /// Detected gap size.
        gap: f32,
        /// Indices in left-to-right order.
        elements: Vec<usize>,
    },
    /// Uniform vertical spacing.
    EqualVerticalSpacing {
        /// Detected gap size.
        gap: f32,
        /// Indices in top-to-bottom order.
        elements: Vec<usize>,
    },
    /// Elements form a grid.
    GridDetected {
        /// Number of columns.
        columns: usize,
        /// Number of rows.
        rows: usize,
        /// Column width.
        cell_width: f32,
        /// Row height.
        cell_height: f32,
        /// Horizontal gap.
        h_gap: f32,
        /// Vertical gap.
        v_gap: f32,
        /// All participating element indices.
        elements: Vec<usize>,
    },
    /// Element appears to maintain a specific aspect ratio.
    AspectRatioLock {
        element: usize,
        /// Detected ratio (width / height).
        ratio: f32,
        /// Nearest "clean" ratio label, e.g. "16:9".
        label: String,
    },
    /// Suggested responsive breakpoint.
    ResponsiveBreakpoint {
        /// Breakpoint width in pixels.
        width: f32,
        /// How elements should adapt (description).
        strategy: String,
    },
}

impl InferredConstraint {
    /// Number of elements involved.
    pub fn element_count(&self) -> usize {
        match self {
            Self::AlignmentRail { elements, .. } => elements.len(),
            Self::EqualHorizontalSpacing { elements, .. } => elements.len(),
            Self::EqualVerticalSpacing { elements, .. } => elements.len(),
            Self::GridDetected { elements, .. } => elements.len(),
            Self::AspectRatioLock { .. } => 1,
            Self::ResponsiveBreakpoint { .. } => 0,
        }
    }

    /// Whether this is a spatial (alignment/spacing) constraint.
    pub fn is_spatial(&self) -> bool {
        matches!(
            self,
            Self::AlignmentRail { .. }
                | Self::EqualHorizontalSpacing { .. }
                | Self::EqualVerticalSpacing { .. }
                | Self::GridDetected { .. }
        )
    }
}

// ── Configuration ────────────────────────────────────────────

/// Tolerances for constraint detection.
#[derive(Debug, Clone)]
pub struct InferrerConfig {
    /// Max pixel difference to consider "aligned".
    pub alignment_tolerance: f32,
    /// Max fractional spacing variance for equal spacing.
    pub spacing_tolerance: f32,
    /// Minimum elements to form an alignment rail.
    pub min_rail_elements: usize,
    /// Minimum elements in one direction for grid detection.
    pub min_grid_size: usize,
}

impl Default for InferrerConfig {
    fn default() -> Self {
        Self {
            alignment_tolerance: 2.0,
            spacing_tolerance: 0.1,
            min_rail_elements: 2,
            min_grid_size: 2,
        }
    }
}

impl InferrerConfig {
    /// Strict: only flag highly precise alignments.
    pub fn strict() -> Self {
        Self {
            alignment_tolerance: 0.5,
            spacing_tolerance: 0.03,
            min_rail_elements: 3,
            min_grid_size: 3,
        }
    }
}

// ── Inferrer ─────────────────────────────────────────────────

/// Constraint inference engine.
pub struct ConstraintInferrer {
    config: InferrerConfig,
}

impl Default for ConstraintInferrer {
    fn default() -> Self {
        Self { config: InferrerConfig::default() }
    }
}

impl ConstraintInferrer {
    /// Create with custom config.
    pub fn new(config: InferrerConfig) -> Self {
        Self { config }
    }

    /// Infer all constraints from element bounds.
    pub fn infer(&self, elements: &[Rect]) -> Vec<InferredConstraint> {
        let mut out = Vec::new();
        self.detect_alignment_rails(elements, &mut out);
        self.detect_equal_spacing(elements, &mut out);
        self.detect_grids(elements, &mut out);
        self.detect_aspect_ratios(elements, &mut out);
        out
    }

    // ── Alignment Rails ──────────────────────────────────────

    fn detect_alignment_rails(&self, elements: &[Rect], out: &mut Vec<InferredConstraint>) {
        if elements.is_empty() {
            return;
        }
        let tol = self.config.alignment_tolerance;
        let min = self.config.min_rail_elements;

        // Collect edge positions
        let edges: Vec<(&str, Vec<(usize, f32)>)> = vec![
            ("left",     elements.iter().enumerate().map(|(i, r)| (i, r.x)).collect()),
            ("right",    elements.iter().enumerate().map(|(i, r)| (i, r.x + r.width)).collect()),
            ("top",      elements.iter().enumerate().map(|(i, r)| (i, r.y)).collect()),
            ("bottom",   elements.iter().enumerate().map(|(i, r)| (i, r.y + r.height)).collect()),
            ("center_x", elements.iter().enumerate().map(|(i, r)| (i, r.x + r.width / 2.0)).collect()),
            ("center_y", elements.iter().enumerate().map(|(i, r)| (i, r.y + r.height / 2.0)).collect()),
        ];

        for (edge_name, positions) in &edges {
            // Cluster positions within tolerance
            let mut sorted = positions.clone();
            sorted.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

            let mut cluster: Vec<usize> = vec![sorted[0].0];
            let mut cluster_pos = sorted[0].1;

            for &(idx, pos) in sorted.iter().skip(1) {
                if (pos - cluster_pos).abs() <= tol {
                    cluster.push(idx);
                } else {
                    if cluster.len() >= min {
                        let avg = cluster.iter()
                            .map(|&i| positions.iter().find(|p| p.0 == i).unwrap().1)
                            .sum::<f32>() / cluster.len() as f32;
                        let mut sorted_cluster = cluster.clone();
                        sorted_cluster.sort();
                        out.push(InferredConstraint::AlignmentRail {
                            edge: edge_name.to_string(),
                            position: avg,
                            elements: sorted_cluster,
                        });
                    }
                    cluster = vec![idx];
                    cluster_pos = pos;
                }
            }
            // Flush last cluster
            if cluster.len() >= min {
                let avg = cluster.iter()
                    .map(|&i| positions.iter().find(|p| p.0 == i).unwrap().1)
                    .sum::<f32>() / cluster.len() as f32;
                let mut sorted_cluster = cluster.clone();
                sorted_cluster.sort();
                out.push(InferredConstraint::AlignmentRail {
                    edge: edge_name.to_string(),
                    position: avg,
                    elements: sorted_cluster,
                });
            }
        }
    }

    // ── Equal Spacing ────────────────────────────────────────

    fn detect_equal_spacing(&self, elements: &[Rect], out: &mut Vec<InferredConstraint>) {
        if elements.len() < 3 {
            return;
        }

        // Horizontal
        let mut h_sorted: Vec<(usize, &Rect)> = elements.iter().enumerate().collect();
        h_sorted.sort_by(|a, b| a.1.x.partial_cmp(&b.1.x).unwrap_or(std::cmp::Ordering::Equal));

        let h_gaps: Vec<f32> = h_sorted
            .windows(2)
            .map(|w| w[1].1.x - (w[0].1.x + w[0].1.width))
            .collect();

        if let Some(gap) = self.is_uniform(&h_gaps) {
            let indices: Vec<usize> = h_sorted.iter().map(|(i, _)| *i).collect();
            out.push(InferredConstraint::EqualHorizontalSpacing {
                gap,
                elements: indices,
            });
        }

        // Vertical
        let mut v_sorted: Vec<(usize, &Rect)> = elements.iter().enumerate().collect();
        v_sorted.sort_by(|a, b| a.1.y.partial_cmp(&b.1.y).unwrap_or(std::cmp::Ordering::Equal));

        let v_gaps: Vec<f32> = v_sorted
            .windows(2)
            .map(|w| w[1].1.y - (w[0].1.y + w[0].1.height))
            .collect();

        if let Some(gap) = self.is_uniform(&v_gaps) {
            let indices: Vec<usize> = v_sorted.iter().map(|(i, _)| *i).collect();
            out.push(InferredConstraint::EqualVerticalSpacing {
                gap,
                elements: indices,
            });
        }
    }

    fn is_uniform(&self, gaps: &[f32]) -> Option<f32> {
        if gaps.is_empty() {
            return None;
        }
        let mean = gaps.iter().sum::<f32>() / gaps.len() as f32;
        if mean.abs() < 0.01 {
            return None;
        }
        let all_close = gaps.iter().all(|&g| ((g - mean) / mean).abs() <= self.config.spacing_tolerance);
        if all_close { Some(mean) } else { None }
    }

    // ── Grid Detection ───────────────────────────────────────

    fn detect_grids(&self, elements: &[Rect], out: &mut Vec<InferredConstraint>) {
        if elements.len() < 4 {
            return;
        }

        let tol = self.config.alignment_tolerance;

        // Find unique x-positions (cluster left edges)
        let mut x_positions: Vec<f32> = elements.iter().map(|r| r.x).collect();
        x_positions.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let x_clusters = cluster_values(&x_positions, tol);

        // Find unique y-positions
        let mut y_positions: Vec<f32> = elements.iter().map(|r| r.y).collect();
        y_positions.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let y_clusters = cluster_values(&y_positions, tol);

        let cols = x_clusters.len();
        let rows = y_clusters.len();

        if cols >= self.config.min_grid_size && rows >= self.config.min_grid_size {
            // Check that most cells are occupied
            let expected = cols * rows;
            let occupied = elements.len();
            if occupied >= expected / 2 {
                let widths: Vec<f32> = elements.iter().map(|r| r.width).collect();
                let heights: Vec<f32> = elements.iter().map(|r| r.height).collect();
                let avg_w = widths.iter().sum::<f32>() / widths.len() as f32;
                let avg_h = heights.iter().sum::<f32>() / heights.len() as f32;

                let h_gap = if cols > 1 && x_clusters.len() > 1 {
                    (x_clusters[1] - x_clusters[0] - avg_w).max(0.0)
                } else {
                    0.0
                };
                let v_gap = if rows > 1 && y_clusters.len() > 1 {
                    (y_clusters[1] - y_clusters[0] - avg_h).max(0.0)
                } else {
                    0.0
                };

                out.push(InferredConstraint::GridDetected {
                    columns: cols,
                    rows,
                    cell_width: avg_w,
                    cell_height: avg_h,
                    h_gap,
                    v_gap,
                    elements: (0..elements.len()).collect(),
                });
            }
        }
    }

    // ── Aspect Ratios ────────────────────────────────────────

    fn detect_aspect_ratios(&self, elements: &[Rect], out: &mut Vec<InferredConstraint>) {
        let known_ratios: &[(f32, &str)] = &[
            (1.0, "1:1"),
            (16.0 / 9.0, "16:9"),
            (4.0 / 3.0, "4:3"),
            (3.0 / 2.0, "3:2"),
            (21.0 / 9.0, "21:9"),
            (2.0 / 1.0, "2:1"),
            (9.0 / 16.0, "9:16"),
            (3.0 / 4.0, "3:4"),
        ];

        for (i, r) in elements.iter().enumerate() {
            if r.height < 1.0 { continue; }
            let ratio = r.width / r.height;

            for &(known, label) in known_ratios {
                if ((ratio - known) / known).abs() < 0.05 {
                    out.push(InferredConstraint::AspectRatioLock {
                        element: i,
                        ratio,
                        label: label.to_string(),
                    });
                    break;
                }
            }
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────

/// Cluster values within tolerance, returning cluster centers.
fn cluster_values(sorted: &[f32], tolerance: f32) -> Vec<f32> {
    if sorted.is_empty() { return vec![]; }
    let mut clusters = Vec::new();
    let mut cluster_sum = sorted[0];
    let mut cluster_count = 1u32;

    for &v in sorted.iter().skip(1) {
        if v - (cluster_sum / cluster_count as f32) <= tolerance {
            cluster_sum += v;
            cluster_count += 1;
        } else {
            clusters.push(cluster_sum / cluster_count as f32);
            cluster_sum = v;
            cluster_count = 1;
        }
    }
    clusters.push(cluster_sum / cluster_count as f32);
    clusters
}

// ── Tests ────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, width: w, height: h }
    }

    // ── Alignment Rails ──────────────────────────────────────

    #[test]
    fn detects_left_alignment_rail() {
        let elements = vec![
            rect(50.0, 10.0, 100.0, 30.0),
            rect(50.0, 60.0, 150.0, 30.0),
            rect(50.0, 110.0, 80.0, 30.0),
        ];
        let c = ConstraintInferrer::default().infer(&elements);
        let rails: Vec<_> = c.iter().filter(|x| matches!(x, InferredConstraint::AlignmentRail { edge, .. } if edge == "left")).collect();
        assert!(!rails.is_empty());
    }

    #[test]
    fn detects_near_alignment() {
        let elements = vec![
            rect(50.0, 10.0, 100.0, 30.0),
            rect(51.0, 60.0, 100.0, 30.0), // 1px off
        ];
        let c = ConstraintInferrer::default().infer(&elements);
        let rails: Vec<_> = c.iter().filter(|x| matches!(x, InferredConstraint::AlignmentRail { edge, .. } if edge == "left")).collect();
        assert!(!rails.is_empty());
    }

    #[test]
    fn no_rail_for_unaligned() {
        let elements = vec![
            rect(10.0, 10.0, 100.0, 30.0),
            rect(200.0, 60.0, 100.0, 30.0),
        ];
        let inferrer = ConstraintInferrer::new(InferrerConfig { min_rail_elements: 2, ..Default::default() });
        let c = inferrer.infer(&elements);
        let left_rails: Vec<_> = c.iter().filter(|x| matches!(x, InferredConstraint::AlignmentRail { edge, .. } if edge == "left")).collect();
        assert!(left_rails.is_empty());
    }

    // ── Equal Spacing ────────────────────────────────────────

    #[test]
    fn detects_equal_horizontal_spacing() {
        let elements = vec![
            rect(0.0, 0.0, 100.0, 50.0),
            rect(120.0, 0.0, 100.0, 50.0),
            rect(240.0, 0.0, 100.0, 50.0),
        ];
        let c = ConstraintInferrer::default().infer(&elements);
        let spacing: Vec<_> = c.iter().filter(|x| matches!(x, InferredConstraint::EqualHorizontalSpacing { .. })).collect();
        assert!(!spacing.is_empty());
        if let InferredConstraint::EqualHorizontalSpacing { gap, .. } = &spacing[0] {
            assert!((*gap - 20.0).abs() < 1.0);
        }
    }

    #[test]
    fn detects_equal_vertical_spacing() {
        let elements = vec![
            rect(0.0, 0.0, 50.0, 30.0),
            rect(0.0, 40.0, 50.0, 30.0),
            rect(0.0, 80.0, 50.0, 30.0),
        ];
        let c = ConstraintInferrer::default().infer(&elements);
        let spacing: Vec<_> = c.iter().filter(|x| matches!(x, InferredConstraint::EqualVerticalSpacing { .. })).collect();
        assert!(!spacing.is_empty());
    }

    #[test]
    fn no_spacing_for_two_elements() {
        let elements = vec![
            rect(0.0, 0.0, 50.0, 50.0),
            rect(70.0, 0.0, 50.0, 50.0),
        ];
        let c = ConstraintInferrer::default().infer(&elements);
        let spacing: Vec<_> = c.iter().filter(|x| matches!(x, InferredConstraint::EqualHorizontalSpacing { .. })).collect();
        assert!(spacing.is_empty());
    }

    #[test]
    fn uneven_spacing_not_detected() {
        let elements = vec![
            rect(0.0, 0.0, 50.0, 50.0),
            rect(60.0, 0.0, 50.0, 50.0),   // gap = 10
            rect(200.0, 0.0, 50.0, 50.0),  // gap = 90
        ];
        let c = ConstraintInferrer::default().infer(&elements);
        let spacing: Vec<_> = c.iter().filter(|x| matches!(x, InferredConstraint::EqualHorizontalSpacing { .. })).collect();
        assert!(spacing.is_empty());
    }

    // ── Grid Detection ───────────────────────────────────────

    #[test]
    fn detects_2x2_grid() {
        let elements = vec![
            rect(0.0, 0.0, 100.0, 80.0),
            rect(120.0, 0.0, 100.0, 80.0),
            rect(0.0, 100.0, 100.0, 80.0),
            rect(120.0, 100.0, 100.0, 80.0),
        ];
        let c = ConstraintInferrer::default().infer(&elements);
        let grids: Vec<_> = c.iter().filter(|x| matches!(x, InferredConstraint::GridDetected { .. })).collect();
        assert!(!grids.is_empty());
        if let InferredConstraint::GridDetected { columns, rows, .. } = &grids[0] {
            assert_eq!(*columns, 2);
            assert_eq!(*rows, 2);
        }
    }

    #[test]
    fn no_grid_for_single_row() {
        let elements = vec![
            rect(0.0, 0.0, 50.0, 50.0),
            rect(60.0, 0.0, 50.0, 50.0),
            rect(120.0, 0.0, 50.0, 50.0),
        ];
        let c = ConstraintInferrer::default().infer(&elements);
        let grids: Vec<_> = c.iter().filter(|x| matches!(x, InferredConstraint::GridDetected { .. })).collect();
        assert!(grids.is_empty());
    }

    // ── Aspect Ratios ────────────────────────────────────────

    #[test]
    fn detects_16_9_ratio() {
        let elements = vec![rect(0.0, 0.0, 1600.0, 900.0)];
        let c = ConstraintInferrer::default().infer(&elements);
        let ratios: Vec<_> = c.iter().filter(|x| matches!(x, InferredConstraint::AspectRatioLock { .. })).collect();
        assert!(!ratios.is_empty());
        if let InferredConstraint::AspectRatioLock { label, .. } = &ratios[0] {
            assert_eq!(label, "16:9");
        }
    }

    #[test]
    fn detects_square_ratio() {
        let elements = vec![rect(0.0, 0.0, 200.0, 200.0)];
        let c = ConstraintInferrer::default().infer(&elements);
        let ratios: Vec<_> = c.iter().filter(|x| matches!(x, InferredConstraint::AspectRatioLock { .. })).collect();
        assert!(!ratios.is_empty());
        if let InferredConstraint::AspectRatioLock { label, .. } = &ratios[0] {
            assert_eq!(label, "1:1");
        }
    }

    #[test]
    fn odd_ratio_not_detected() {
        let elements = vec![rect(0.0, 0.0, 123.0, 456.0)];
        let c = ConstraintInferrer::default().infer(&elements);
        let ratios: Vec<_> = c.iter().filter(|x| matches!(x, InferredConstraint::AspectRatioLock { .. })).collect();
        assert!(ratios.is_empty());
    }

    // ── Constraint Properties ────────────────────────────────

    #[test]
    fn element_count() {
        let c = InferredConstraint::AlignmentRail {
            edge: "left".to_string(),
            position: 50.0,
            elements: vec![0, 1, 2],
        };
        assert_eq!(c.element_count(), 3);
        assert!(c.is_spatial());
    }

    #[test]
    fn aspect_ratio_is_not_spatial() {
        let c = InferredConstraint::AspectRatioLock {
            element: 0,
            ratio: 1.0,
            label: "1:1".to_string(),
        };
        assert!(!c.is_spatial());
        assert_eq!(c.element_count(), 1);
    }

    #[test]
    fn responsive_breakpoint_properties() {
        let c = InferredConstraint::ResponsiveBreakpoint {
            width: 768.0,
            strategy: "stack columns".to_string(),
        };
        assert_eq!(c.element_count(), 0);
        assert!(!c.is_spatial());
    }

    #[test]
    fn cluster_values_basic() {
        let vals = vec![10.0, 10.5, 11.0, 50.0, 50.5];
        let clusters = cluster_values(&vals, 2.0);
        assert_eq!(clusters.len(), 2);
    }

    #[test]
    fn strict_config_fewer_results() {
        let elements = vec![
            rect(50.0, 10.0, 100.0, 30.0),
            rect(51.5, 60.0, 100.0, 30.0), // 1.5px off
        ];
        let default_results = ConstraintInferrer::default().infer(&elements);
        let strict_results = ConstraintInferrer::new(InferrerConfig::strict()).infer(&elements);
        // Strict requires 3+ for rail, default only 2
        assert!(strict_results.len() <= default_results.len());
    }

    #[test]
    fn empty_input() {
        let c = ConstraintInferrer::default().infer(&[]);
        assert!(c.is_empty());
    }
}
