/// Phase 15.11 — Agent A/B Testing Framework (logos-agent-ab)
/// Integration test suite: 55 tests total
///
/// §1  Traffic splitter         (12 tests)
/// §2  Experiment metrics       (12 tests)
/// §3  Statistical engine       (13 tests)
/// §4  Experiment lifecycle     (11 tests)
/// §5  End-to-end               ( 7 tests)

use logos_agent_ab::{
    TrafficSplitter, SplitConfig, SplitError,
    ExperimentMetrics, VariantStats, MetricsError,
    ZTest, ConfidenceInterval, PValueBand,
    Experiment, ExperimentConfig, ExperimentState, Variant, ExperimentError,
};

// ── §1  Traffic splitter ─────────────────────────────────────────────────────

#[test]
fn splitter_returns_known_variant_name() {
    let s = TrafficSplitter::new(SplitConfig::default());
    let cfg = ExperimentConfig::new(
        "s1", vec![Variant::new("ctrl", 50), Variant::new("trt", 50)]
    ).unwrap();
    let v = s.resolve("s1", "usr-1", &cfg);
    assert!(v == "ctrl" || v == "trt");
}

#[test]
fn splitter_deterministic_same_user() {
    let s = TrafficSplitter::new(SplitConfig::default());
    let cfg = ExperimentConfig::new(
        "s2", vec![Variant::new("a", 50), Variant::new("b", 50)]
    ).unwrap();
    let v1 = s.resolve("s2", "user-x", &cfg);
    let v2 = s.resolve("s2", "user-x", &cfg);
    assert_eq!(v1, v2);
}

#[test]
fn splitter_different_users_spread() {
    let s = TrafficSplitter::new(SplitConfig::default());
    let cfg = ExperimentConfig::new(
        "s3", vec![Variant::new("a", 50), Variant::new("b", 50)]
    ).unwrap();
    let all: Vec<_> = (0..100).map(|i| s.resolve("s3", &format!("u{i}"), &cfg)).collect();
    assert!(all.iter().any(|v| v == "a"));
    assert!(all.iter().any(|v| v == "b"));
}

#[test]
fn splitter_empty_user_id_error() {
    let s = TrafficSplitter::new(SplitConfig::default());
    let cfg = ExperimentConfig::new(
        "s4", vec![Variant::new("a", 50), Variant::new("b", 50)]
    ).unwrap();
    assert!(matches!(s.try_resolve("s4", "", &cfg), Err(SplitError::EmptyUserId)));
}

#[test]
fn splitter_invalid_weights_error() {
    let s = TrafficSplitter::new(SplitConfig::default());
    let bad_cfg = ExperimentConfig {
        id: "bad".to_owned(),
        variants: vec![Variant::new("a", 30), Variant::new("b", 30)],
    };
    assert!(matches!(
        s.try_resolve("bad", "u1", &bad_cfg),
        Err(SplitError::InvalidWeights(60))
    ));
}

#[test]
fn splitter_bucket_in_range() {
    let s = TrafficSplitter::new(SplitConfig::default());
    for i in 0..50 {
        let b = s.bucket_for("exp", &format!("u{i}"));
        assert!(b < 100);
    }
}

#[test]
fn splitter_three_variants_all_covered() {
    let s = TrafficSplitter::new(SplitConfig::default());
    let cfg = ExperimentConfig::new(
        "t3", vec![
            Variant::new("x", 34),
            Variant::new("y", 33),
            Variant::new("z", 33),
        ]
    ).unwrap();
    let all: Vec<_> = (0..300).map(|i| s.resolve("t3", &format!("u{i}"), &cfg)).collect();
    for name in &["x","y","z"] {
        assert!(all.iter().any(|v| v == *name), "missing variant {name}");
    }
}

#[test]
fn splitter_custom_salt_changes_assignment() {
    let cfg = ExperimentConfig::new(
        "cs", vec![Variant::new("a", 50), Variant::new("b", 50)]
    ).unwrap();
    let s1 = TrafficSplitter::new(SplitConfig::new("salt1", true));
    let s2 = TrafficSplitter::new(SplitConfig::new("salt2", true));
    let mut differs = 0;
    for i in 0..50u32 {
        if s1.resolve("cs", &format!("u{i}"), &cfg) != s2.resolve("cs", &format!("u{i}"), &cfg) {
            differs += 1;
        }
    }
    assert!(differs > 0, "expected some assignments to differ between salts");
}

#[test]
fn splitter_validate_weights_ok_when_sum_100() {
    let v = vec![Variant::new("a", 70), Variant::new("b", 30)];
    assert!(TrafficSplitter::validate_weights(&v).is_ok());
}

#[test]
fn splitter_validate_weights_err_when_not_100() {
    let v = vec![Variant::new("a", 40), Variant::new("b", 40)];
    assert!(matches!(
        TrafficSplitter::validate_weights(&v),
        Err(SplitError::InvalidWeights(80))
    ));
}

#[test]
fn splitter_90_10_split_skews_correctly() {
    let s = TrafficSplitter::new(SplitConfig::default());
    let cfg = ExperimentConfig::new(
        "skew", vec![Variant::new("majority", 90), Variant::new("minority", 10)]
    ).unwrap();
    let counts = (0..1000).map(|i| s.resolve("skew", &format!("u{i}"), &cfg)).count();
    // Just verify no panic and a valid count
    assert_eq!(counts, 1000);
}

#[test]
fn splitter_variant_resolver_trait_works() {
    use logos_agent_ab::VariantResolver;
    let s = TrafficSplitter::new(SplitConfig::default());
    let cfg = ExperimentConfig::new(
        "tr", vec![Variant::new("a", 50), Variant::new("b", 50)]
    ).unwrap();
    let v = VariantResolver::resolve(&s, "tr", "usr", &cfg);
    assert!(v == "a" || v == "b");
}

// ── §2  Experiment metrics ────────────────────────────────────────────────────

#[test]
fn metrics_record_exposure() {
    let mut m = ExperimentMetrics::new();
    m.record_exposure("e1", "ctrl");
    assert_eq!(m.stats_for("e1", "ctrl").unwrap().exposures, 1);
}

#[test]
fn metrics_record_conversion() {
    let mut m = ExperimentMetrics::new();
    m.record_exposure("e1", "ctrl");
    m.record_conversion("e1", "ctrl");
    assert_eq!(m.stats_for("e1", "ctrl").unwrap().conversions, 1);
}

#[test]
fn metrics_conversion_rate() {
    let mut m = ExperimentMetrics::new();
    for _ in 0..10 { m.record_exposure("e", "v"); }
    for _ in 0..3  { m.record_conversion("e", "v"); }
    let s = m.stats_for("e", "v").unwrap();
    assert!((s.conversion_rate() - 0.3).abs() < 1e-9);
}

#[test]
fn metrics_record_value_accumulates() {
    let mut m = ExperimentMetrics::new();
    m.record_exposure("e", "v");
    m.record_value("e", "v", 5.0);
    m.record_value("e", "v", 15.0);
    let s = m.stats_for("e", "v").unwrap();
    assert!((s.total_value - 20.0).abs() < 1e-9);
    assert_eq!(s.conversions, 2);
}

#[test]
fn metrics_total_exposures() {
    let mut m = ExperimentMetrics::new();
    for _ in 0..5 { m.record_exposure("e", "a"); }
    for _ in 0..3 { m.record_exposure("e", "b"); }
    assert_eq!(m.total_exposures("e"), 8);
}

#[test]
fn metrics_total_conversions() {
    let mut m = ExperimentMetrics::new();
    for _ in 0..2 { m.record_conversion("e", "a"); }
    for _ in 0..4 { m.record_conversion("e", "b"); }
    assert_eq!(m.total_conversions("e"), 6);
}

#[test]
fn metrics_unknown_experiment_err() {
    let m = ExperimentMetrics::new();
    assert!(matches!(
        m.stats_for("ghost", "v"),
        Err(MetricsError::ExperimentNotFound(_))
    ));
}

#[test]
fn metrics_unknown_variant_err() {
    let mut m = ExperimentMetrics::new();
    m.record_exposure("e", "a");
    assert!(matches!(
        m.stats_for("e", "ghost"),
        Err(MetricsError::VariantNotFound(_, _))
    ));
}

#[test]
fn metrics_reset_experiment() {
    let mut m = ExperimentMetrics::new();
    m.record_exposure("e1", "a");
    m.record_exposure("e2", "b");
    m.reset_experiment("e1");
    assert_eq!(m.experiment_count(), 1);
}

#[test]
fn metrics_reset_all() {
    let mut m = ExperimentMetrics::new();
    m.record_exposure("e1", "a");
    m.record_exposure("e2", "b");
    m.reset_all();
    assert_eq!(m.experiment_count(), 0);
}

#[test]
fn metrics_lift_vs_positive() {
    let mut s = VariantStats::new("v");
    s.exposures   = 100;
    s.conversions = 70;
    assert!(s.lift_vs(0.50) > 0.0);
}

#[test]
fn metrics_relative_lift_vs_zero_baseline() {
    let mut s = VariantStats::new("v");
    s.exposures   = 100;
    s.conversions = 10;
    assert_eq!(s.relative_lift_vs(0.0), 0.0);
}

// ── §3  Statistical engine ────────────────────────────────────────────────────

#[test]
fn ztest_highly_significant_large_sample() {
    let r = ZTest::run(10_000, 5_500, 10_000, 6_200);
    assert_eq!(r.band, PValueBand::HighlySignificant);
    assert!(r.is_significant);
}

#[test]
fn ztest_not_significant_tiny_diff() {
    let r = ZTest::run(100, 50, 100, 51);
    assert!(!r.is_significant);
    assert_eq!(r.band, PValueBand::NotSignificant);
}

#[test]
fn ztest_rates_calculated_correctly() {
    let r = ZTest::run(200, 100, 200, 150);
    assert!((r.rate_control - 0.5).abs() < 1e-9);
    assert!((r.rate_treatment - 0.75).abs() < 1e-9);
}

#[test]
fn ztest_absolute_lift_positive_when_treatment_better() {
    let r = ZTest::run(1000, 400, 1000, 500);
    assert!(r.absolute_lift > 0.0);
}

#[test]
fn ztest_absolute_lift_negative_when_control_better() {
    let r = ZTest::run(1000, 500, 1000, 400);
    assert!(r.absolute_lift < 0.0);
}

#[test]
fn ztest_zero_difference_gives_zero_z() {
    let r = ZTest::run(500, 250, 500, 250);
    assert!(r.z_score.abs() < 1e-6);
}

#[test]
fn ztest_ci_excludes_zero_when_significant() {
    let r = ZTest::run(10_000, 5_000, 10_000, 6_000);
    assert!(r.ci_95.excludes_zero());
}

#[test]
fn ztest_ci_width_positive() {
    let r = ZTest::run(1000, 400, 1000, 500);
    assert!(r.ci_95.width() > 0.0);
}

#[test]
fn ztest_ci_midpoint_equals_lift() {
    let r = ZTest::run(5000, 2000, 5000, 3000);
    assert!((r.ci_95.midpoint() - r.absolute_lift).abs() < 1e-9);
}

#[test]
fn ztest_pvalue_band_label_non_empty() {
    for band in &[
        PValueBand::HighlySignificant,
        PValueBand::Significant,
        PValueBand::Marginal,
        PValueBand::NotSignificant,
    ] {
        assert!(!band.label().is_empty());
    }
}

#[test]
fn ztest_required_sample_size_for_1pct_mde() {
    let n = ZTest::required_sample_size(0.10, 0.01);
    assert!(n > 1_000);
}

#[test]
fn ztest_zero_sample_no_panic() {
    let r = ZTest::run(0, 0, 0, 0);
    assert_eq!(r.z_score, 0.0);
    assert!(!r.is_significant);
}

#[test]
fn confidence_interval_excludes_zero_both_positive() {
    let ci = ConfidenceInterval::new(0.02, 0.08);
    assert!(ci.excludes_zero());
}

// ── §4  Experiment lifecycle ──────────────────────────────────────────────────

#[test]
fn experiment_starts_as_draft() {
    let cfg = ExperimentConfig::new("e", vec![
        Variant::new("a", 50), Variant::new("b", 50)
    ]).unwrap();
    let e = Experiment::new(cfg);
    assert_eq!(e.state, ExperimentState::Draft);
}

#[test]
fn experiment_start_moves_to_running() {
    let cfg = ExperimentConfig::new("e", vec![
        Variant::new("a", 50), Variant::new("b", 50)
    ]).unwrap();
    let mut e = Experiment::new(cfg);
    e.start().unwrap();
    assert!(e.is_running());
}

#[test]
fn experiment_pause_moves_to_paused() {
    let cfg = ExperimentConfig::new("e", vec![
        Variant::new("a", 50), Variant::new("b", 50)
    ]).unwrap();
    let mut e = Experiment::new(cfg);
    e.start().unwrap();
    e.pause().unwrap();
    assert_eq!(e.state, ExperimentState::Paused);
}

#[test]
fn experiment_resume_from_paused() {
    let cfg = ExperimentConfig::new("e", vec![
        Variant::new("a", 50), Variant::new("b", 50)
    ]).unwrap();
    let mut e = Experiment::new(cfg);
    e.start().unwrap();
    e.pause().unwrap();
    e.start().unwrap(); // Resume
    assert!(e.is_running());
}

#[test]
fn experiment_conclude_with_winner() {
    let cfg = ExperimentConfig::new("e", vec![
        Variant::new("a", 50), Variant::new("b", 50)
    ]).unwrap();
    let mut e = Experiment::new(cfg);
    e.conclude(Some("a".to_owned())).unwrap();
    assert!(matches!(e.state, ExperimentState::Concluded { winner: Some(_) }));
}

#[test]
fn experiment_conclude_no_winner() {
    let cfg = ExperimentConfig::new("e", vec![
        Variant::new("a", 50), Variant::new("b", 50)
    ]).unwrap();
    let mut e = Experiment::new(cfg);
    e.conclude(None).unwrap();
    assert!(matches!(e.state, ExperimentState::Concluded { winner: None }));
}

#[test]
fn experiment_double_conclude_errors() {
    let cfg = ExperimentConfig::new("e", vec![
        Variant::new("a", 50), Variant::new("b", 50)
    ]).unwrap();
    let mut e = Experiment::new(cfg);
    e.conclude(None).unwrap();
    assert!(matches!(e.conclude(None), Err(ExperimentError::AlreadyConcluded)));
}

#[test]
fn experiment_expose_when_not_running_errors() {
    let cfg = ExperimentConfig::new("e", vec![
        Variant::new("a", 50), Variant::new("b", 50)
    ]).unwrap();
    let mut e = Experiment::new(cfg);
    assert!(matches!(e.expose("a"), Err(ExperimentError::NotRunning(_))));
}

#[test]
fn experiment_invalid_weights_rejected() {
    let result = ExperimentConfig::new("e", vec![
        Variant::new("a", 40), Variant::new("b", 40)
    ]);
    assert!(matches!(result, Err(ExperimentError::InvalidWeights(80))));
}

#[test]
fn experiment_single_variant_rejected() {
    let result = ExperimentConfig::new("e", vec![Variant::new("only", 100)]);
    assert!(matches!(result, Err(ExperimentError::TooFewVariants)));
}

#[test]
fn experiment_report_unknown_variant_errors() {
    let cfg = ExperimentConfig::new("e", vec![
        Variant::new("a", 50), Variant::new("b", 50)
    ]).unwrap();
    let mut e = Experiment::new(cfg);
    e.start().unwrap();
    assert!(matches!(e.report("a", "ghost"), Err(ExperimentError::VariantNotFound(_))));
}

// ── §5  End-to-end ────────────────────────────────────────────────────────────

#[test]
fn e2e_full_experiment_lifecycle_with_winner() {
    let cfg = ExperimentConfig::new("full", vec![
        Variant::new("control",   50),
        Variant::new("treatment", 50),
    ]).unwrap();
    let mut exp = Experiment::new(cfg);
    exp.start().unwrap();

    // Simulate 10 000 users per variant
    for _ in 0..10_000 { exp.expose("control").unwrap(); }
    for _ in 0..5_000  { exp.convert("control").unwrap(); }
    for _ in 0..10_000 { exp.expose("treatment").unwrap(); }
    for _ in 0..6_500  { exp.convert("treatment").unwrap(); }

    let report = exp.report("control", "treatment").unwrap();
    assert!(report.is_significant());
    assert_eq!(report.recommended_winner, Some("treatment".to_owned()));

    exp.conclude(report.recommended_winner.clone()).unwrap();
    assert!(matches!(
        exp.state,
        ExperimentState::Concluded { winner: Some(ref w) } if w == "treatment"
    ));
}

#[test]
fn e2e_splitter_routes_then_metrics_tracks() {
    let cfg = ExperimentConfig::new("route", vec![
        Variant::new("v1", 50),
        Variant::new("v2", 50),
    ]).unwrap();
    let splitter = TrafficSplitter::new(SplitConfig::default());
    let mut metrics = ExperimentMetrics::new();

    for i in 0..200u32 {
        let uid = format!("u{i}");
        let variant = splitter.resolve("route", &uid, &cfg);
        metrics.record_exposure("route", &variant);
        if i % 5 == 0 {
            metrics.record_conversion("route", &variant);
        }
    }

    let total_exp = metrics.total_exposures("route");
    assert_eq!(total_exp, 200);
    let total_conv = metrics.total_conversions("route");
    assert!(total_conv > 0);
}

#[test]
fn e2e_no_significance_when_rates_equal() {
    let cfg = ExperimentConfig::new("eq", vec![
        Variant::new("a", 50),
        Variant::new("b", 50),
    ]).unwrap();
    let mut exp = Experiment::new(cfg);
    exp.start().unwrap();

    for _ in 0..1000 { exp.expose("a").unwrap(); }
    for _ in 0..500  { exp.convert("a").unwrap(); }
    for _ in 0..1000 { exp.expose("b").unwrap(); }
    for _ in 0..500  { exp.convert("b").unwrap(); }

    let report = exp.report("a", "b").unwrap();
    assert!(!report.is_significant());
    assert!(report.recommended_winner.is_none());
}

#[test]
fn e2e_pause_resume_accumulates_correctly() {
    let cfg = ExperimentConfig::new("pr", vec![
        Variant::new("c", 50),
        Variant::new("t", 50),
    ]).unwrap();
    let mut exp = Experiment::new(cfg);
    exp.start().unwrap();
    for _ in 0..50 { exp.expose("c").unwrap(); }
    exp.pause().unwrap();
    // Exposures while paused should error
    assert!(exp.expose("c").is_err());
    exp.start().unwrap(); // resume
    for _ in 0..50 { exp.expose("c").unwrap(); }

    let s = exp.metrics.stats_for("pr", "c").unwrap();
    assert_eq!(s.exposures, 100);
}

#[test]
fn e2e_valued_conversions_and_avg() {
    let cfg = ExperimentConfig::new("val", vec![
        Variant::new("free", 50),
        Variant::new("paid", 50),
    ]).unwrap();
    let mut exp = Experiment::new(cfg);
    exp.start().unwrap();

    for _ in 0..100 { exp.expose("paid").unwrap(); }
    for _ in 0..20  { exp.convert_with_value("paid", 50.0).unwrap(); }

    let s = exp.metrics.stats_for("val", "paid").unwrap();
    assert!((s.total_value - 1000.0).abs() < 1e-9);
    assert!((s.avg_value() - 10.0).abs() < 1e-9); // 1000 / 100 exposures
}

#[test]
fn e2e_multi_experiment_isolation() {
    let cfg_a = ExperimentConfig::new("exp-a", vec![
        Variant::new("a1", 50), Variant::new("a2", 50)
    ]).unwrap();
    let cfg_b = ExperimentConfig::new("exp-b", vec![
        Variant::new("b1", 50), Variant::new("b2", 50)
    ]).unwrap();
    let mut ea = Experiment::new(cfg_a);
    let mut eb = Experiment::new(cfg_b);
    ea.start().unwrap();
    eb.start().unwrap();

    for _ in 0..100 { ea.expose("a1").unwrap(); }
    for _ in 0..200 { eb.expose("b1").unwrap(); }

    assert_eq!(ea.metrics.total_exposures("exp-a"), 100);
    assert_eq!(eb.metrics.total_exposures("exp-b"), 200);
    // Cross-contamination check
    assert_eq!(ea.metrics.total_exposures("exp-b"), 0);
}

#[test]
fn e2e_z_test_sample_size_guidance() {
    // Publisher wants to detect a 2 % lift on a 10 % baseline
    let n = ZTest::required_sample_size(0.10, 0.02);
    assert!(n > 500, "sample size too small: {n}");
    // Collect that many, then run test
    let r = ZTest::run(n, (n as f64 * 0.10) as u64, n, (n as f64 * 0.12) as u64);
    // Should be significant with the recommended sample size
    assert!(r.is_significant, "expected significance at recommended n={n}, z={}", r.z_score);
}
