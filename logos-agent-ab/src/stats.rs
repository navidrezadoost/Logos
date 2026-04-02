//! Statistical significance — two-proportion Z-test and confidence intervals.

/// Classification of the p-value into a human-readable band.
#[derive(Debug, Clone, PartialEq)]
pub enum PValueBand {
    /// p < 0.01  — highly significant
    HighlySignificant,
    /// p < 0.05  — significant
    Significant,
    /// p < 0.10  — marginally significant
    Marginal,
    /// p >= 0.10 — not significant
    NotSignificant,
}

impl PValueBand {
    pub fn label(&self) -> &'static str {
        match self {
            PValueBand::HighlySignificant => "p < 0.01 (highly significant)",
            PValueBand::Significant       => "p < 0.05 (significant)",
            PValueBand::Marginal          => "p < 0.10 (marginal)",
            PValueBand::NotSignificant    => "p >= 0.10 (not significant)",
        }
    }
}

/// A 95 % confidence interval [lower, upper].
#[derive(Debug, Clone, PartialEq)]
pub struct ConfidenceInterval {
    pub lower: f64,
    pub upper: f64,
}

impl ConfidenceInterval {
    pub fn new(lower: f64, upper: f64) -> Self {
        Self { lower, upper }
    }

    /// Returns true if zero is NOT inside the interval (i.e. effect is significant).
    pub fn excludes_zero(&self) -> bool {
        self.lower > 0.0 || self.upper < 0.0
    }

    /// Width of the confidence interval.
    pub fn width(&self) -> f64 {
        self.upper - self.lower
    }

    /// Midpoint of the confidence interval.
    pub fn midpoint(&self) -> f64 {
        (self.lower + self.upper) / 2.0
    }
}

/// Result of a two-proportion Z-test.
#[derive(Debug, Clone)]
pub struct ZTestResult {
    /// Observed conversion rates for control and treatment.
    pub rate_control:   f64,
    pub rate_treatment: f64,
    /// Absolute difference (treatment − control).
    pub absolute_lift: f64,
    /// Relative lift as a fraction.
    pub relative_lift: f64,
    /// Z-score.
    pub z_score: f64,
    /// Approximate p-value (two-tailed, normal approximation).
    pub p_value: f64,
    /// Classified significance band.
    pub band: PValueBand,
    /// 95 % CI for the absolute lift.
    pub ci_95: ConfidenceInterval,
    /// Whether the result is significant at α = 0.05.
    pub is_significant: bool,
}

/// Two-proportion Z-test for A/B experiments.
///
/// Uses the pooled proportion to estimate the standard error under H₀.
pub struct ZTest;

impl ZTest {
    /// Run a two-proportion Z-test.
    ///
    /// * `n_control`   — exposures in control
    /// * `conv_control` — conversions in control
    /// * `n_treatment` — exposures in treatment
    /// * `conv_treatment` — conversions in treatment
    pub fn run(
        n_control:     u64,
        conv_control:  u64,
        n_treatment:   u64,
        conv_treatment: u64,
    ) -> ZTestResult {
        let p_c = if n_control   == 0 { 0.0 } else { conv_control   as f64 / n_control   as f64 };
        let p_t = if n_treatment == 0 { 0.0 } else { conv_treatment as f64 / n_treatment as f64 };

        let absolute_lift = p_t - p_c;
        let relative_lift = if p_c == 0.0 { 0.0 } else { absolute_lift / p_c };

        // Pooled proportion
        let n_total    = n_control + n_treatment;
        let conv_total = conv_control + conv_treatment;
        let p_pool = if n_total == 0 { 0.0 } else { conv_total as f64 / n_total as f64 };

        let se = if n_control == 0 || n_treatment == 0 {
            0.0
        } else {
            (p_pool * (1.0 - p_pool)
                * (1.0 / n_control as f64 + 1.0 / n_treatment as f64))
                .sqrt()
        };

        let z_score = if se == 0.0 { 0.0 } else { absolute_lift / se };

        // Two-tailed p-value approximation via normal CDF
        let p_value = 2.0 * Self::normal_sf(z_score.abs());

        let band = Self::classify(p_value);
        let is_significant = p_value < 0.05;

        // 95 % CI: lift ± 1.96 * se_unpooled
        let se_unpooled = if n_control == 0 || n_treatment == 0 {
            0.0
        } else {
            (p_c * (1.0 - p_c) / n_control as f64
                + p_t * (1.0 - p_t) / n_treatment as f64)
                .sqrt()
        };
        let margin = 1.96 * se_unpooled;
        let ci_95 = ConfidenceInterval::new(absolute_lift - margin, absolute_lift + margin);

        ZTestResult {
            rate_control: p_c,
            rate_treatment: p_t,
            absolute_lift,
            relative_lift,
            z_score,
            p_value,
            band,
            ci_95,
            is_significant,
        }
    }

    /// Apply a minimum detectable effect (MDE) check.
    /// Returns the sample size required per variant for 80 % power at α = 0.05.
    pub fn required_sample_size(baseline_rate: f64, mde: f64) -> u64 {
        if mde == 0.0 || baseline_rate == 0.0 {
            return u64::MAX;
        }
        let p1 = baseline_rate;
        let p2 = baseline_rate + mde;
        // n = (z_alpha/2 + z_beta)^2 * (p1*(1-p1) + p2*(1-p2)) / mde^2
        // z_alpha/2 = 1.96, z_beta (80% power) = 0.842
        let z = 1.96 + 0.842;
        let n = z * z * (p1 * (1.0 - p1) + p2 * (1.0 - p2)) / (mde * mde);
        n.ceil() as u64
    }

    // Survival function of the standard normal using Hart's rational approximation.
    fn normal_sf(z: f64) -> f64 {
        // Approximation from Abramowitz & Stegun §26.2.17
        let t = 1.0 / (1.0 + 0.2316419 * z);
        let poly = t * (0.319381530
            + t * (-0.356563782
            + t * (1.781477937
            + t * (-1.821255978
            + t * 1.330274429))));
        let pdf = (-z * z / 2.0).exp() / (2.0 * std::f64::consts::PI).sqrt();
        pdf * poly
    }

    fn classify(p: f64) -> PValueBand {
        if p < 0.01      { PValueBand::HighlySignificant }
        else if p < 0.05 { PValueBand::Significant }
        else if p < 0.10 { PValueBand::Marginal }
        else             { PValueBand::NotSignificant }
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn z_test_highly_significant() {
        // Large sample, big difference
        let r = ZTest::run(10_000, 5_500, 10_000, 6_200);
        assert!(r.is_significant);
        assert_eq!(r.band, PValueBand::HighlySignificant);
    }

    #[test]
    fn z_test_not_significant_small_diff() {
        // Same rate → p ≈ 1
        let r = ZTest::run(100, 50, 100, 51);
        assert!(!r.is_significant);
        assert_eq!(r.band, PValueBand::NotSignificant);
    }

    #[test]
    fn z_test_absolute_lift_positive() {
        let r = ZTest::run(1000, 100, 1000, 150);
        assert!(r.absolute_lift > 0.0);
    }

    #[test]
    fn z_test_absolute_lift_negative() {
        let r = ZTest::run(1000, 150, 1000, 100);
        assert!(r.absolute_lift < 0.0);
    }

    #[test]
    fn z_test_rates_correct() {
        let r = ZTest::run(200, 100, 200, 80);
        assert!((r.rate_control   - 0.5).abs() < 1e-9);
        assert!((r.rate_treatment - 0.4).abs() < 1e-9);
    }

    #[test]
    fn z_test_zero_difference() {
        let r = ZTest::run(500, 250, 500, 250);
        assert!((r.z_score).abs() < 1e-9);
        assert!((r.absolute_lift).abs() < 1e-9);
    }

    #[test]
    fn ci_excludes_zero_when_significant() {
        let r = ZTest::run(10_000, 5_500, 10_000, 6_500);
        assert!(r.ci_95.excludes_zero());
    }

    #[test]
    fn ci_width_positive() {
        let r = ZTest::run(1000, 400, 1000, 500);
        assert!(r.ci_95.width() > 0.0);
    }

    #[test]
    fn ci_midpoint_approximates_lift() {
        let r = ZTest::run(5000, 2500, 5000, 3000);
        assert!((r.ci_95.midpoint() - r.absolute_lift).abs() < 1e-9);
    }

    #[test]
    fn required_sample_size_reasonable() {
        // 5 % baseline, 1 % MDE → needs large sample
        let n = ZTest::required_sample_size(0.05, 0.01);
        assert!(n > 1000);
    }

    #[test]
    fn p_value_band_labels_non_empty() {
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
    fn zero_sample_size_doesnt_panic() {
        let r = ZTest::run(0, 0, 0, 0);
        assert_eq!(r.z_score, 0.0);
    }
}
