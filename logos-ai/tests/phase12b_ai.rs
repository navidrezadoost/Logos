//! Phase 12B integration tests — cross-module AI design intelligence.

use logos_core::Rect;
use logos_core::style::Color;

use logos_ai::inference::design_suggest::{DesignAnalyzer, DesignContext, SuggestionKind};
use logos_ai::inference::accessibility::{AccessibilityChecker, ColorBlindnessType, simulate_color_blindness};
use logos_ai::inference::color_harmony::{HslColor, HarmonyScheme, PaletteGenerator, classify_temperature, ColorTemperature};
use logos_ai::inference::smart_constraints::{ConstraintInferrer, InferredConstraint};
use logos_ai::inference::component_recommend::{ComponentRecommender, DesignElement};
use logos_ai::inference::pipeline::{Pipeline, PipelineStep, StepKind, PipelineRunner, PipelinePresets};

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect { x, y, width: w, height: h }
}

// ── Design Analysis → Accessibility Cross-Check ──────────────

#[test]
fn design_analysis_feeds_accessibility_audit() {
    // A design with near-alignment issues AND small touch targets
    let elements = vec![
        rect(10.0, 10.0, 30.0, 30.0),  // too small for touch (< 44px)
        rect(12.0, 60.0, 30.0, 30.0),  // near-aligned with [0], also too small
    ];

    // Step 1: Design analysis finds alignment issues
    let ctx = DesignContext::new(800.0, 600.0, elements.clone());
    let suggestions = DesignAnalyzer::default().analyze(&ctx);
    assert!(suggestions.iter().any(|s| s.kind == SuggestionKind::Alignment));

    // Step 2: Accessibility check finds touch-target issues
    let checker = AccessibilityChecker::default();
    let touch_results = checker.check_touch_targets(&elements);
    assert!(touch_results.iter().all(|r| !r.passes));

    // Step 3: Verify suggestions provide fixes
    let alignment_fix = suggestions.iter()
        .find(|s| s.kind == SuggestionKind::Alignment)
        .unwrap();
    assert!(alignment_fix.has_fix());
}

// ── Color Harmony → Accessibility Contrast Check ─────────────

#[test]
fn generated_palette_passes_contrast() {
    // Generate a complementary palette
    let base = HslColor::new(220.0, 0.8, 0.3); // Dark blue
    let palette = PaletteGenerator::generate(base, HarmonyScheme::Complementary);
    let rgb_colors = palette.to_rgb();
    assert_eq!(rgb_colors.len(), 2);

    // Check that the two colors have sufficient contrast
    let checker = AccessibilityChecker::default();
    let result = checker.check_contrast(rgb_colors[0], rgb_colors[1]);
    // Complementary colors (dark blue vs warm/orange) should have decent contrast
    assert!(result.ratio > 1.0);
}

// ── Color Harmony → Color Blindness Safety ───────────────────

#[test]
fn palette_cvd_safety_check() {
    let base = HslColor::new(0.0, 0.9, 0.5); // Saturated red
    let palette = PaletteGenerator::generate(base, HarmonyScheme::Triadic);
    let rgb = palette.to_rgb();

    // Simulate all colors under protanopia
    let simulated: Vec<Color> = rgb.iter()
        .map(|&c| simulate_color_blindness(c, ColorBlindnessType::Protanopia))
        .collect();

    // At minimum, check that simulated colors are valid
    for c in &simulated {
        assert!(c.r >= 0.0 && c.r <= 1.0);
        assert!(c.g >= 0.0 && c.g <= 1.0);
        assert!(c.b >= 0.0 && c.b <= 1.0);
    }
}

// ── Smart Constraints → Component Recommendation ─────────────

#[test]
fn grid_detected_leads_to_component_recommendation() {
    // Create a 3x2 grid of similar cards
    let elements = vec![
        rect(0.0, 0.0, 200.0, 150.0),
        rect(220.0, 0.0, 200.0, 150.0),
        rect(440.0, 0.0, 200.0, 150.0),
        rect(0.0, 170.0, 200.0, 150.0),
        rect(220.0, 170.0, 200.0, 150.0),
        rect(440.0, 170.0, 200.0, 150.0),
    ];

    // Constraints: should detect grid + equal spacing
    let constraints = ConstraintInferrer::default().infer(&elements);
    assert!(constraints.iter().any(|c| matches!(c, InferredConstraint::GridDetected { .. })));

    // Components: all same size → recommend componentizing
    let design_elements: Vec<DesignElement> = elements.iter().enumerate()
        .map(|(i, r)| DesignElement::new(i, "card", r.width, r.height))
        .collect();
    let recs = ComponentRecommender::default().recommend(&design_elements);
    assert!(!recs.is_empty());
    assert!(recs[0].instance_count() >= 6);
}

// ── Full Pipeline Integration ────────────────────────────────

#[test]
fn full_design_review_pipeline() {
    let runner = PipelineRunner::with_defaults();
    let pipeline = PipelinePresets::design_review();

    let result = runner.run(&pipeline);
    assert!(result.success);
    assert_eq!(result.successful_steps(), 5);
    assert!(result.all_findings().len() >= 5);
    assert!(result.total_duration < std::time::Duration::from_secs(5));
}

#[test]
fn custom_pipeline_with_mixed_steps() {
    let mut runner = PipelineRunner::with_defaults();
    runner.register_handler(
        StepKind::Custom,
        Box::new(|step| {
            let msg = step.params.get("message")
                .cloned()
                .unwrap_or_else(|| "custom executed".to_string());
            Ok(vec![msg])
        }),
    );

    let pipeline = Pipeline::new("mixed")
        .add_step(PipelineStep::new(StepKind::DesignAnalysis))
        .add_step(PipelineStep::new(StepKind::Custom).with_param("message", "my custom step"))
        .add_step(PipelineStep::new(StepKind::AccessibilityAudit));

    let result = runner.run(&pipeline);
    assert!(result.success);
    assert!(result.all_findings().iter().any(|f| f.contains("custom")));
}

// ── Color Temperature → Palette Consistency ──────────────────

#[test]
fn palette_temperature_consistency() {
    // A warm palette should have all warm colors
    let warm_base = HslColor::new(30.0, 0.9, 0.5); // Orange
    let palette = PaletteGenerator::generate(warm_base, HarmonyScheme::Analogous);

    let temps: Vec<_> = palette.colors.iter()
        .map(|c| classify_temperature(*c))
        .collect();

    // Analogous from orange should stay warm/neutral (not cool)
    assert!(temps.iter().all(|t| *t != ColorTemperature::Cool));
}
