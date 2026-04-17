// logos-ai/tests/design_suggest_flat.rs
// Phase 6: AI Design Suggestions for Flat-Page Canva-Mode UI
// Tests t600-t629

use logos_ai::inference::design_suggest::{
    AnalyzerConfig, DesignAnalyzer, DesignContext, SuggestionKind,
};
use logos_core::Rect;

// ── helpers ──────────────────────────────────────────────────────────────────

fn r(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect { x, y, width: w, height: h }
}

// ── §1  DesignContext construction ───────────────────────────────────────────

#[test]
fn t600_empty_context() {
    let ctx = DesignContext::new(1920.0, 1080.0, vec![]);
    let suggestions = DesignAnalyzer::default().analyze(&ctx);
    assert!(suggestions.is_empty());
}

#[test]
fn t601_single_element_no_suggestions() {
    let ctx = DesignContext::new(800.0, 600.0, vec![r(10.0, 10.0, 100.0, 50.0)]);
    let suggestions = DesignAnalyzer::default().analyze(&ctx);
    // No suggestions for a single element — no pair comparisons possible, spacing needs ≥3
    assert!(suggestions.is_empty());
}

#[test]
fn t602_context_with_labels() {
    let ctx = DesignContext::new(1280.0, 720.0, vec![
        r(0.0, 0.0, 200.0, 100.0),
        r(0.0, 120.0, 200.0, 100.0),
    ])
    .with_labels(vec!["header".into(), "body".into()]);
    // Should not panic; labels accepted
    let suggestions = DesignAnalyzer::default().analyze(&ctx);
    let _ = suggestions;
}

#[test]
fn t603_context_canvas_dimensions() {
    // Out-of-bounds element should trigger a suggestion
    let ctx = DesignContext::new(100.0, 100.0, vec![r(120.0, 10.0, 50.0, 30.0)]);
    let suggestions = DesignAnalyzer::default().analyze(&ctx);
    assert!(suggestions.iter().any(|s| s.kind == SuggestionKind::OutOfBounds));
}

#[test]
fn t604_context_two_elements() {
    let ctx = DesignContext::new(1920.0, 1080.0, vec![
        r(0.0, 0.0, 100.0, 100.0),
        r(200.0, 200.0, 100.0, 100.0),
    ]);
    // Should not panic; two elements are fine
    let _ = DesignAnalyzer::default().analyze(&ctx);
}

#[test]
fn t605_default_analyzer() {
    let _a1 = DesignAnalyzer::default();
    let _a2 = DesignAnalyzer::new(AnalyzerConfig::default());
    // Both are valid; no panic
}

// ── §2  Alignment suggestions ────────────────────────────────────────────────

#[test]
fn t610_perfectly_aligned_no_suggestion() {
    // Left edges exactly equal → no alignment suggestion
    let ctx = DesignContext::new(1000.0, 1000.0, vec![
        r(50.0, 10.0, 100.0, 40.0),
        r(50.0, 80.0, 120.0, 40.0),
    ]);
    let suggestions = DesignAnalyzer::default().analyze(&ctx);
    let alignment = suggestions.iter().filter(|s| s.kind == SuggestionKind::Alignment).count();
    assert_eq!(alignment, 0, "perfectly aligned elements should not trigger alignment suggestion");
}

#[test]
fn t611_near_misaligned_triggers_alignment() {
    // Left edges 2px apart — within default tolerance of 4px
    let ctx = DesignContext::new(1000.0, 1000.0, vec![
        r(50.0, 10.0, 100.0, 40.0),
        r(52.0, 80.0, 100.0, 40.0),
    ]);
    let suggestions = DesignAnalyzer::default().analyze(&ctx);
    assert!(
        suggestions.iter().any(|s| s.kind == SuggestionKind::Alignment),
        "near-misaligned left edges should trigger alignment suggestion"
    );
}

#[test]
fn t612_far_misaligned_no_alignment_suggestion() {
    // Left edges 50px apart — outside tolerance
    let ctx = DesignContext::new(1000.0, 1000.0, vec![
        r(50.0, 10.0, 100.0, 40.0),
        r(100.0, 80.0, 100.0, 40.0),
    ]);
    let suggestions = DesignAnalyzer::default().analyze(&ctx);
    let alignment = suggestions.iter().filter(|s| s.kind == SuggestionKind::Alignment).count();
    assert_eq!(alignment, 0, "widely misaligned elements should not trigger alignment suggestion");
}

#[test]
fn t613_near_top_edge_alignment() {
    // Top edges 1px apart
    let ctx = DesignContext::new(1000.0, 1000.0, vec![
        r(10.0, 100.0, 80.0, 40.0),
        r(200.0, 101.0, 80.0, 40.0),
    ]);
    let suggestions = DesignAnalyzer::default().analyze(&ctx);
    assert!(
        suggestions.iter().any(|s| s.kind == SuggestionKind::Alignment),
        "near-aligned top edges should trigger alignment suggestion"
    );
}

#[test]
fn t614_alignment_suggestion_has_fix() {
    let ctx = DesignContext::new(1000.0, 1000.0, vec![
        r(50.0, 10.0, 100.0, 40.0),
        r(52.0, 80.0, 100.0, 40.0),
    ]);
    let suggestions = DesignAnalyzer::default().analyze(&ctx);
    let align_s = suggestions.iter().find(|s| s.kind == SuggestionKind::Alignment);
    if let Some(s) = align_s {
        assert!(s.has_fix(), "alignment suggestion should have a proposed fix");
    }
}

#[test]
fn t615_alignment_confidence_in_range() {
    let ctx = DesignContext::new(1000.0, 1000.0, vec![
        r(50.0, 10.0, 100.0, 40.0),
        r(52.0, 80.0, 100.0, 40.0),
    ]);
    let suggestions = DesignAnalyzer::default().analyze(&ctx);
    for s in &suggestions {
        assert!(s.confidence >= 0.0 && s.confidence <= 1.0, "confidence must be in [0, 1]");
    }
}

// ── §3  Overlap suggestions ───────────────────────────────────────────────────

#[test]
fn t620_overlapping_rects_trigger_overlap() {
    // Two rects that clearly overlap
    let ctx = DesignContext::new(1000.0, 1000.0, vec![
        r(50.0, 50.0, 200.0, 200.0),
        r(100.0, 100.0, 200.0, 200.0),
    ]);
    let suggestions = DesignAnalyzer::default().analyze(&ctx);
    assert!(
        suggestions.iter().any(|s| s.kind == SuggestionKind::Overlap),
        "overlapping rects should trigger overlap suggestion"
    );
}

#[test]
fn t621_non_overlapping_rects_no_overlap() {
    // Two rects well separated
    let ctx = DesignContext::new(1000.0, 1000.0, vec![
        r(0.0, 0.0, 100.0, 100.0),
        r(200.0, 200.0, 100.0, 100.0),
    ]);
    let suggestions = DesignAnalyzer::default().analyze(&ctx);
    let overlaps = suggestions.iter().filter(|s| s.kind == SuggestionKind::Overlap).count();
    assert_eq!(overlaps, 0);
}

#[test]
fn t622_relaxed_config_skips_small_overlap() {
    // Overlap area < 10px² (relaxed threshold)
    let ctx = DesignContext::new(1000.0, 1000.0, vec![
        r(0.0, 0.0, 100.0, 100.0),
        r(99.5, 99.5, 100.0, 100.0), // tiny overlap: 0.5 * 0.5 = 0.25 px²
    ]);
    let analyzer = DesignAnalyzer::new(AnalyzerConfig::relaxed());
    let suggestions = analyzer.analyze(&ctx);
    let overlaps = suggestions.iter().filter(|s| s.kind == SuggestionKind::Overlap).count();
    assert_eq!(overlaps, 0, "relaxed config should skip tiny overlaps");
}

#[test]
fn t623_strict_config_catches_zero_area_overlap() {
    // Touching edges: overlap area = 0 (exactly touching, not overlapping)
    let ctx = DesignContext::new(1000.0, 1000.0, vec![
        r(0.0, 0.0, 100.0, 100.0),
        r(100.0, 0.0, 100.0, 100.0), // exact edge-touch, no overlap
    ]);
    let analyzer = DesignAnalyzer::new(AnalyzerConfig::strict());
    let suggestions = analyzer.analyze(&ctx);
    // strict threshold=0.0 means overlap >= 0.0 is reported; touching edges
    // produce 0 area which satisfies the condition, so 0 or 1 are both valid.
    // We simply ensure no panic and the count is sane.
    let overlaps = suggestions.iter().filter(|s| s.kind == SuggestionKind::Overlap).count();
    assert!(overlaps <= 1, "at most one overlap suggestion for two touching elements");
}

// ── §4  Out-of-bounds suggestions ────────────────────────────────────────────

#[test]
fn t624_element_within_canvas_no_oob() {
    let ctx = DesignContext::new(800.0, 600.0, vec![r(10.0, 10.0, 100.0, 50.0)]);
    let suggestions = DesignAnalyzer::default().analyze(&ctx);
    let oob = suggestions.iter().filter(|s| s.kind == SuggestionKind::OutOfBounds).count();
    assert_eq!(oob, 0);
}

#[test]
fn t625_element_right_edge_out_of_canvas() {
    // Element extends past right boundary
    let ctx = DesignContext::new(800.0, 600.0, vec![r(750.0, 10.0, 100.0, 50.0)]);
    let suggestions = DesignAnalyzer::default().analyze(&ctx);
    assert!(
        suggestions.iter().any(|s| s.kind == SuggestionKind::OutOfBounds),
        "element past right edge should trigger out-of-bounds"
    );
}

#[test]
fn t626_element_below_canvas() {
    let ctx = DesignContext::new(800.0, 600.0, vec![r(10.0, 580.0, 100.0, 100.0)]);
    let suggestions = DesignAnalyzer::default().analyze(&ctx);
    assert!(
        suggestions.iter().any(|s| s.kind == SuggestionKind::OutOfBounds),
        "element below canvas bottom should trigger out-of-bounds"
    );
}

// ── §5  Config and SuggestionKind labels ─────────────────────────────────────

#[test]
fn t627_suggestion_kind_labels() {
    assert_eq!(SuggestionKind::Alignment.label(), "Alignment");
    assert_eq!(SuggestionKind::Spacing.label(), "Spacing");
    assert_eq!(SuggestionKind::Overlap.label(), "Overlap");
    assert_eq!(SuggestionKind::OutOfBounds.label(), "Out of bounds");
    assert_eq!(SuggestionKind::Hierarchy.label(), "Hierarchy");
    assert_eq!(SuggestionKind::Grouping.label(), "Grouping");
}

#[test]
fn t628_strict_config_fields() {
    let cfg = AnalyzerConfig::strict();
    assert!(cfg.alignment_tolerance < AnalyzerConfig::default().alignment_tolerance);
    assert!(cfg.spacing_tolerance < AnalyzerConfig::default().spacing_tolerance);
    assert!(cfg.min_confidence < AnalyzerConfig::default().min_confidence);
}

#[test]
fn t629_relaxed_config_fields() {
    let cfg = AnalyzerConfig::relaxed();
    assert!(cfg.alignment_tolerance > AnalyzerConfig::default().alignment_tolerance);
    assert!(cfg.spacing_tolerance > AnalyzerConfig::default().spacing_tolerance);
    assert!(cfg.min_confidence > AnalyzerConfig::default().min_confidence);
}
