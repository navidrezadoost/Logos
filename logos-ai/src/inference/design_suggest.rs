//! # Design Suggestion Engine
//!
//! Analyses an existing design (layers, styles, spatial relationships) and
//! produces actionable suggestions — alignment fixes, spacing corrections,
//! hierarchy improvements, visual consistency hints, and more.
//!
//! All analysis is purely geometric/heuristic: no network calls, no GPU.
//! This keeps suggestions instantaneous (<1 ms for typical canvases).
//!
//! ```
//! use logos_ai::inference::design_suggest::{DesignAnalyzer, DesignContext, SuggestionKind};
//! use logos_core::Rect;
//!
//! let ctx = DesignContext {
//!     canvas_width: 1920.0,
//!     canvas_height: 1080.0,
//!     elements: vec![
//!         Rect { x: 10.0, y: 10.0, width: 200.0, height: 100.0 },
//!         Rect { x: 215.0, y: 10.0, width: 200.0, height: 100.0 },
//!     ],
//!     labels: vec!["header".into(), "subtitle".into()],
//! };
//!
//! let analyzer = DesignAnalyzer::default();
//! let suggestions = analyzer.analyze(&ctx);
//! assert!(suggestions.iter().all(|s| s.confidence >= 0.0));
//! ```

use logos_core::Rect;

// ── Types ────────────────────────────────────────────────────

/// Input context describing the current design state.
#[derive(Debug, Clone)]
pub struct DesignContext {
    /// Canvas width in pixels.
    pub canvas_width: f32,
    /// Canvas height in pixels.
    pub canvas_height: f32,
    /// Bounding boxes of each element in the design.
    pub elements: Vec<Rect>,
    /// Optional semantic labels (e.g. "heading", "button").
    /// Must be same length as `elements`, or empty.
    pub labels: Vec<String>,
}

impl DesignContext {
    /// Create a minimal context with just canvas size and elements.
    pub fn new(canvas_width: f32, canvas_height: f32, elements: Vec<Rect>) -> Self {
        Self {
            canvas_width,
            canvas_height,
            elements,
            labels: Vec::new(),
        }
    }

    /// Add semantic labels.
    pub fn with_labels(mut self, labels: Vec<String>) -> Self {
        self.labels = labels;
        self
    }
}

/// Category of suggestion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SuggestionKind {
    /// Elements are nearly but not quite aligned.
    Alignment,
    /// Spacing between elements is inconsistent.
    Spacing,
    /// Element overlaps another unexpectedly.
    Overlap,
    /// Element is partially or fully outside the canvas.
    OutOfBounds,
    /// Visual hierarchy could be improved (size ratios).
    Hierarchy,
    /// Group of elements could be consolidated.
    Grouping,
}

impl SuggestionKind {
    /// Human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Alignment => "Alignment",
            Self::Spacing => "Spacing",
            Self::Overlap => "Overlap",
            Self::OutOfBounds => "Out of bounds",
            Self::Hierarchy => "Hierarchy",
            Self::Grouping => "Grouping",
        }
    }
}

/// A single actionable suggestion.
#[derive(Debug, Clone)]
pub struct Suggestion {
    /// What kind of issue.
    pub kind: SuggestionKind,
    /// Human-readable explanation.
    pub message: String,
    /// Confidence in [0, 1].
    pub confidence: f32,
    /// Indices of affected elements.
    pub affected: Vec<usize>,
    /// Optional corrected bounds for affected elements (same order).
    pub proposed_fix: Vec<Rect>,
}

impl Suggestion {
    fn new(kind: SuggestionKind, message: impl Into<String>, confidence: f32) -> Self {
        Self {
            kind,
            message: message.into(),
            confidence: confidence.clamp(0.0, 1.0),
            affected: Vec::new(),
            proposed_fix: Vec::new(),
        }
    }

    fn with_affected(mut self, indices: Vec<usize>) -> Self {
        self.affected = indices;
        self
    }

    fn with_fix(mut self, rects: Vec<Rect>) -> Self {
        self.proposed_fix = rects;
        self
    }

    /// Whether this suggestion includes a proposed fix.
    pub fn has_fix(&self) -> bool {
        !self.proposed_fix.is_empty()
    }
}

// ── Configuration ────────────────────────────────────────────

/// Thresholds for the analyzer.
#[derive(Debug, Clone)]
pub struct AnalyzerConfig {
    /// Maximum pixel difference to consider "nearly aligned".
    pub alignment_tolerance: f32,
    /// Maximum relative spacing variance before flagging.
    pub spacing_tolerance: f32,
    /// Minimum overlap area (px²) to report.
    pub overlap_threshold: f32,
    /// Minimum confidence to include a suggestion.
    pub min_confidence: f32,
}

impl Default for AnalyzerConfig {
    fn default() -> Self {
        Self {
            alignment_tolerance: 4.0,
            spacing_tolerance: 0.15,
            overlap_threshold: 1.0,
            min_confidence: 0.3,
        }
    }
}

impl AnalyzerConfig {
    /// Strict mode: tighter tolerances.
    pub fn strict() -> Self {
        Self {
            alignment_tolerance: 1.0,
            spacing_tolerance: 0.05,
            overlap_threshold: 0.0,
            min_confidence: 0.1,
        }
    }

    /// Relaxed mode: wider tolerances.
    pub fn relaxed() -> Self {
        Self {
            alignment_tolerance: 8.0,
            spacing_tolerance: 0.30,
            overlap_threshold: 10.0,
            min_confidence: 0.5,
        }
    }
}

// ── Analyzer ─────────────────────────────────────────────────

/// Design suggestion engine.
///
/// Run [`analyze`](DesignAnalyzer::analyze) to get a list of suggestions.
pub struct DesignAnalyzer {
    config: AnalyzerConfig,
}

impl Default for DesignAnalyzer {
    fn default() -> Self {
        Self {
            config: AnalyzerConfig::default(),
        }
    }
}

impl DesignAnalyzer {
    /// Create with a custom configuration.
    pub fn new(config: AnalyzerConfig) -> Self {
        Self { config }
    }

    /// Analyze a design context and return suggestions.
    pub fn analyze(&self, ctx: &DesignContext) -> Vec<Suggestion> {
        let mut out = Vec::new();
        self.check_alignment(ctx, &mut out);
        self.check_spacing(ctx, &mut out);
        self.check_overlaps(ctx, &mut out);
        self.check_bounds(ctx, &mut out);
        self.check_hierarchy(ctx, &mut out);
        out.retain(|s| s.confidence >= self.config.min_confidence);
        out
    }

    // ── Alignment ────────────────────────────────────────────

    fn check_alignment(&self, ctx: &DesignContext, out: &mut Vec<Suggestion>) {
        let tol = self.config.alignment_tolerance;
        let elems = &ctx.elements;

        for i in 0..elems.len() {
            for j in (i + 1)..elems.len() {
                let a = &elems[i];
                let b = &elems[j];

                // Near-aligned left edges
                let dx_left = (a.x - b.x).abs();
                if dx_left > 0.0 && dx_left <= tol {
                    let snap_x = (a.x + b.x) / 2.0;
                    let fix_a = Rect { x: snap_x, ..*a };
                    let fix_b = Rect { x: snap_x, ..*b };
                    let conf = 1.0 - dx_left / tol;
                    out.push(
                        Suggestion::new(
                            SuggestionKind::Alignment,
                            format!("Elements {} and {} have nearly aligned left edges (off by {:.1}px)", i, j, dx_left),
                            conf,
                        )
                        .with_affected(vec![i, j])
                        .with_fix(vec![fix_a, fix_b]),
                    );
                }

                // Near-aligned top edges
                let dy_top = (a.y - b.y).abs();
                if dy_top > 0.0 && dy_top <= tol {
                    let snap_y = (a.y + b.y) / 2.0;
                    let fix_a = Rect { y: snap_y, ..*a };
                    let fix_b = Rect { y: snap_y, ..*b };
                    let conf = 1.0 - dy_top / tol;
                    out.push(
                        Suggestion::new(
                            SuggestionKind::Alignment,
                            format!("Elements {} and {} have nearly aligned top edges (off by {:.1}px)", i, j, dy_top),
                            conf,
                        )
                        .with_affected(vec![i, j])
                        .with_fix(vec![fix_a, fix_b]),
                    );
                }
            }
        }
    }

    // ── Spacing ──────────────────────────────────────────────

    fn check_spacing(&self, ctx: &DesignContext, out: &mut Vec<Suggestion>) {
        let elems = &ctx.elements;
        if elems.len() < 3 {
            return;
        }

        // Sort by left edge and check horizontal gaps
        let mut sorted: Vec<(usize, &Rect)> = elems.iter().enumerate().collect();
        sorted.sort_by(|a, b| a.1.x.partial_cmp(&b.1.x).unwrap_or(std::cmp::Ordering::Equal));

        let gaps: Vec<(usize, usize, f32)> = sorted
            .windows(2)
            .map(|w| {
                let (i, a) = w[0];
                let (j, b) = w[1];
                let gap = b.x - (a.x + a.width);
                (i, j, gap)
            })
            .collect();

        if gaps.is_empty() {
            return;
        }

        let mean_gap: f32 = gaps.iter().map(|(_, _, g)| g).sum::<f32>() / gaps.len() as f32;
        if mean_gap.abs() < 0.01 {
            return;
        }

        for &(i, j, gap) in &gaps {
            let deviation = ((gap - mean_gap) / mean_gap).abs();
            if deviation > self.config.spacing_tolerance {
                let conf = (deviation / (self.config.spacing_tolerance * 3.0)).min(1.0);
                out.push(
                    Suggestion::new(
                        SuggestionKind::Spacing,
                        format!(
                            "Gap between elements {} and {} ({:.1}px) deviates from average ({:.1}px)",
                            i, j, gap, mean_gap
                        ),
                        conf,
                    )
                    .with_affected(vec![i, j]),
                );
            }
        }
    }

    // ── Overlaps ─────────────────────────────────────────────

    fn check_overlaps(&self, ctx: &DesignContext, out: &mut Vec<Suggestion>) {
        let elems = &ctx.elements;
        for i in 0..elems.len() {
            for j in (i + 1)..elems.len() {
                let area = overlap_area(&elems[i], &elems[j]);
                if area >= self.config.overlap_threshold {
                    let total = (elems[i].width * elems[i].height)
                        .min(elems[j].width * elems[j].height);
                    let ratio = if total > 0.0 { area / total } else { 0.0 };
                    let conf = (0.3 + ratio * 0.7).min(1.0);
                    out.push(
                        Suggestion::new(
                            SuggestionKind::Overlap,
                            format!(
                                "Elements {} and {} overlap by {:.0}px² ({:.0}% of smaller)",
                                i, j, area, ratio * 100.0,
                            ),
                            conf,
                        )
                        .with_affected(vec![i, j]),
                    );
                }
            }
        }
    }

    // ── Out of Bounds ────────────────────────────────────────

    fn check_bounds(&self, ctx: &DesignContext, out: &mut Vec<Suggestion>) {
        for (i, e) in ctx.elements.iter().enumerate() {
            let right = e.x + e.width;
            let bottom = e.y + e.height;
            let outside = e.x < 0.0
                || e.y < 0.0
                || right > ctx.canvas_width
                || bottom > ctx.canvas_height;
            if outside {
                let clamped = Rect {
                    x: e.x.max(0.0).min(ctx.canvas_width - e.width),
                    y: e.y.max(0.0).min(ctx.canvas_height - e.height),
                    width: e.width,
                    height: e.height,
                };
                out.push(
                    Suggestion::new(
                        SuggestionKind::OutOfBounds,
                        format!("Element {} extends outside the canvas", i),
                        0.9,
                    )
                    .with_affected(vec![i])
                    .with_fix(vec![clamped]),
                );
            }
        }
    }

    // ── Hierarchy ────────────────────────────────────────────

    fn check_hierarchy(&self, ctx: &DesignContext, out: &mut Vec<Suggestion>) {
        let elems = &ctx.elements;
        if elems.len() < 2 {
            return;
        }

        // Find the largest and smallest elements — large size disparity
        // without clear grouping can indicate hierarchy issues.
        let areas: Vec<f32> = elems.iter().map(|e| e.width * e.height).collect();
        let max_area = areas.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let min_area = areas.iter().cloned().fold(f32::INFINITY, f32::min);

        if min_area > 0.0 {
            let ratio = max_area / min_area;
            if ratio > 50.0 {
                let max_idx = areas.iter().position(|&a| a == max_area).unwrap_or(0);
                let min_idx = areas.iter().position(|&a| a == min_area).unwrap_or(0);
                let conf = ((ratio - 50.0) / 200.0).min(1.0);
                out.push(
                    Suggestion::new(
                        SuggestionKind::Hierarchy,
                        format!(
                            "Large size ratio ({:.0}x) between elements {} and {} — consider grouping or resizing",
                            ratio, max_idx, min_idx,
                        ),
                        conf,
                    )
                    .with_affected(vec![max_idx, min_idx]),
                );
            }
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────

/// Calculate the overlap area between two rectangles.
fn overlap_area(a: &Rect, b: &Rect) -> f32 {
    let x_overlap = (a.x + a.width).min(b.x + b.width) - a.x.max(b.x);
    let y_overlap = (a.y + a.height).min(b.y + b.height) - a.y.max(b.y);
    if x_overlap > 0.0 && y_overlap > 0.0 {
        x_overlap * y_overlap
    } else {
        0.0
    }
}

// ── Tests ────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, width: w, height: h }
    }

    #[test]
    fn empty_context_no_suggestions() {
        let ctx = DesignContext::new(800.0, 600.0, vec![]);
        let s = DesignAnalyzer::default().analyze(&ctx);
        assert!(s.is_empty());
    }

    #[test]
    fn single_element_no_alignment() {
        let ctx = DesignContext::new(800.0, 600.0, vec![rect(10.0, 10.0, 100.0, 50.0)]);
        let s = DesignAnalyzer::default().analyze(&ctx);
        // Single element can't have alignment/spacing issues
        assert!(s.iter().all(|x| x.kind != SuggestionKind::Alignment));
        assert!(s.iter().all(|x| x.kind != SuggestionKind::Spacing));
    }

    #[test]
    fn detects_near_alignment() {
        let ctx = DesignContext::new(800.0, 600.0, vec![
            rect(10.0, 50.0, 100.0, 50.0),
            rect(12.0, 150.0, 100.0, 50.0), // x off by 2
        ]);
        let s = DesignAnalyzer::default().analyze(&ctx);
        assert!(s.iter().any(|x| x.kind == SuggestionKind::Alignment));

        let align = s.iter().find(|x| x.kind == SuggestionKind::Alignment).unwrap();
        assert_eq!(align.affected.len(), 2);
        assert!(align.has_fix());
        // Fix should snap both to the midpoint (11.0)
        assert!((align.proposed_fix[0].x - 11.0).abs() < 0.01);
        assert!((align.proposed_fix[1].x - 11.0).abs() < 0.01);
    }

    #[test]
    fn perfectly_aligned_no_suggestion() {
        let ctx = DesignContext::new(800.0, 600.0, vec![
            rect(50.0, 10.0, 100.0, 50.0),
            rect(50.0, 80.0, 100.0, 50.0),
        ]);
        let s = DesignAnalyzer::default().analyze(&ctx);
        // dx = 0 exactly → not "nearly aligned", it IS aligned → no suggestion
        assert!(s.iter().all(|x| x.kind != SuggestionKind::Alignment
            || !x.message.contains("left edges")));
    }

    #[test]
    fn detects_spacing_inconsistency() {
        let ctx = DesignContext::new(800.0, 600.0, vec![
            rect(0.0, 0.0, 50.0, 50.0),
            rect(60.0, 0.0, 50.0, 50.0),   // gap = 10
            rect(120.0, 0.0, 50.0, 50.0),  // gap = 10
            rect(210.0, 0.0, 50.0, 50.0),  // gap = 40 (way off)
        ]);
        let s = DesignAnalyzer::default().analyze(&ctx);
        assert!(s.iter().any(|x| x.kind == SuggestionKind::Spacing));
    }

    #[test]
    fn uniform_spacing_no_suggestions() {
        let ctx = DesignContext::new(800.0, 600.0, vec![
            rect(0.0, 0.0, 50.0, 50.0),
            rect(60.0, 0.0, 50.0, 50.0),
            rect(120.0, 0.0, 50.0, 50.0),
            rect(180.0, 0.0, 50.0, 50.0),
        ]);
        let s = DesignAnalyzer::default().analyze(&ctx);
        assert!(s.iter().all(|x| x.kind != SuggestionKind::Spacing));
    }

    #[test]
    fn detects_overlap() {
        let ctx = DesignContext::new(800.0, 600.0, vec![
            rect(0.0, 0.0, 100.0, 100.0),
            rect(50.0, 50.0, 100.0, 100.0),
        ]);
        let s = DesignAnalyzer::default().analyze(&ctx);
        assert!(s.iter().any(|x| x.kind == SuggestionKind::Overlap));
    }

    #[test]
    fn no_overlap_for_adjacent() {
        let ctx = DesignContext::new(800.0, 600.0, vec![
            rect(0.0, 0.0, 100.0, 100.0),
            rect(100.0, 0.0, 100.0, 100.0),
        ]);
        let s = DesignAnalyzer::default().analyze(&ctx);
        assert!(s.iter().all(|x| x.kind != SuggestionKind::Overlap));
    }

    #[test]
    fn detects_out_of_bounds() {
        let ctx = DesignContext::new(800.0, 600.0, vec![
            rect(-10.0, 50.0, 100.0, 50.0),
        ]);
        let s = DesignAnalyzer::default().analyze(&ctx);
        let oob = s.iter().find(|x| x.kind == SuggestionKind::OutOfBounds).unwrap();
        assert!(oob.has_fix());
        assert!(oob.proposed_fix[0].x >= 0.0);
    }

    #[test]
    fn element_inside_canvas_ok() {
        let ctx = DesignContext::new(800.0, 600.0, vec![
            rect(10.0, 10.0, 100.0, 50.0),
        ]);
        let s = DesignAnalyzer::default().analyze(&ctx);
        assert!(s.iter().all(|x| x.kind != SuggestionKind::OutOfBounds));
    }

    #[test]
    fn detects_hierarchy_issue() {
        let ctx = DesignContext::new(1920.0, 1080.0, vec![
            rect(0.0, 0.0, 1000.0, 800.0), // 800_000 px²
            rect(500.0, 400.0, 5.0, 5.0),   // 25 px² → ratio = 32000
        ]);
        let s = DesignAnalyzer::default().analyze(&ctx);
        assert!(s.iter().any(|x| x.kind == SuggestionKind::Hierarchy));
    }

    #[test]
    fn similar_sizes_no_hierarchy() {
        let ctx = DesignContext::new(800.0, 600.0, vec![
            rect(0.0, 0.0, 100.0, 100.0),
            rect(120.0, 0.0, 80.0, 90.0),
        ]);
        let s = DesignAnalyzer::default().analyze(&ctx);
        assert!(s.iter().all(|x| x.kind != SuggestionKind::Hierarchy));
    }

    #[test]
    fn strict_config_catches_more() {
        let ctx = DesignContext::new(800.0, 600.0, vec![
            rect(10.0, 50.0, 100.0, 50.0),
            rect(10.5, 150.0, 100.0, 50.0), // off by 0.5
        ]);
        let relaxed = DesignAnalyzer::new(AnalyzerConfig::relaxed()).analyze(&ctx);
        let strict = DesignAnalyzer::new(AnalyzerConfig::strict()).analyze(&ctx);
        // Strict should find alignment issue; relaxed may filter by min_confidence
        assert!(strict.len() >= relaxed.len());
    }

    #[test]
    fn suggestion_confidence_clamped() {
        let s = Suggestion::new(SuggestionKind::Alignment, "test", 5.0);
        assert_eq!(s.confidence, 1.0);
        let s2 = Suggestion::new(SuggestionKind::Alignment, "test", -1.0);
        assert_eq!(s2.confidence, 0.0);
    }

    #[test]
    fn suggestion_kind_labels() {
        assert_eq!(SuggestionKind::Alignment.label(), "Alignment");
        assert_eq!(SuggestionKind::Spacing.label(), "Spacing");
        assert_eq!(SuggestionKind::Overlap.label(), "Overlap");
        assert_eq!(SuggestionKind::OutOfBounds.label(), "Out of bounds");
        assert_eq!(SuggestionKind::Hierarchy.label(), "Hierarchy");
        assert_eq!(SuggestionKind::Grouping.label(), "Grouping");
    }

    #[test]
    fn overlap_area_computation() {
        assert_eq!(overlap_area(&rect(0.0, 0.0, 10.0, 10.0), &rect(5.0, 5.0, 10.0, 10.0)), 25.0);
        assert_eq!(overlap_area(&rect(0.0, 0.0, 10.0, 10.0), &rect(20.0, 0.0, 10.0, 10.0)), 0.0);
    }

    #[test]
    fn context_with_labels() {
        let ctx = DesignContext::new(800.0, 600.0, vec![rect(0.0, 0.0, 50.0, 50.0)])
            .with_labels(vec!["header".into()]);
        assert_eq!(ctx.labels.len(), 1);
    }

    #[test]
    fn multiple_issue_types_in_one_analysis() {
        let ctx = DesignContext::new(400.0, 300.0, vec![
            rect(-5.0, 10.0, 100.0, 100.0),   // OOB
            rect(-3.0, 120.0, 100.0, 100.0),   // OOB + near-aligned to [0]
            rect(200.0, 10.0, 50.0, 50.0),
        ]);
        let s = DesignAnalyzer::default().analyze(&ctx);
        let kinds: std::collections::HashSet<_> = s.iter().map(|x| x.kind).collect();
        assert!(kinds.contains(&SuggestionKind::OutOfBounds));
    }
}
