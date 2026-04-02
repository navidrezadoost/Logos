//! Metric collection — per-experiment, per-variant counters.

use std::collections::HashMap;
use thiserror::Error;

/// Errors from metric operations.
#[derive(Debug, Error, PartialEq)]
pub enum MetricsError {
    #[error("experiment '{0}' not found")]
    ExperimentNotFound(String),
    #[error("variant '{0}' not found in experiment '{1}'")]
    VariantNotFound(String, String),
    #[error("conversion count exceeds exposure count for variant '{0}'")]
    ConversionExceedsExposure(String),
}

/// Counters for a single variant.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VariantStats {
    pub variant_id:  String,
    pub exposures:   u64,
    pub conversions: u64,
    pub total_value: f64,
}

impl VariantStats {
    pub fn new(variant_id: impl Into<String>) -> Self {
        Self { variant_id: variant_id.into(), ..Default::default() }
    }

    /// Conversion rate as a fraction in [0, 1].
    pub fn conversion_rate(&self) -> f64 {
        if self.exposures == 0 {
            0.0
        } else {
            self.conversions as f64 / self.exposures as f64
        }
    }

    /// Average value per exposure.
    pub fn avg_value(&self) -> f64 {
        if self.exposures == 0 {
            0.0
        } else {
            self.total_value / self.exposures as f64
        }
    }

    /// Absolute lift vs a baseline conversion rate.
    pub fn lift_vs(&self, baseline_rate: f64) -> f64 {
        self.conversion_rate() - baseline_rate
    }

    /// Relative lift vs a baseline conversion rate (as a fraction).
    pub fn relative_lift_vs(&self, baseline_rate: f64) -> f64 {
        if baseline_rate == 0.0 {
            0.0
        } else {
            (self.conversion_rate() - baseline_rate) / baseline_rate
        }
    }
}

/// Collects exposures, conversions, and optional numeric values
/// per experiment × variant.
#[derive(Debug, Default)]
pub struct ExperimentMetrics {
    // experiment_id → variant_id → VariantStats
    data: HashMap<String, HashMap<String, VariantStats>>,
}

impl ExperimentMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    fn entry(&mut self, experiment_id: &str, variant_id: &str) -> &mut VariantStats {
        self.data
            .entry(experiment_id.to_owned())
            .or_default()
            .entry(variant_id.to_owned())
            .or_insert_with(|| VariantStats::new(variant_id))
    }

    /// Record that `user` was exposed to `variant_id` in `experiment_id`.
    pub fn record_exposure(&mut self, experiment_id: &str, variant_id: &str) {
        self.entry(experiment_id, variant_id).exposures += 1;
    }

    /// Record a conversion event for a variant.
    pub fn record_conversion(&mut self, experiment_id: &str, variant_id: &str) {
        self.entry(experiment_id, variant_id).conversions += 1;
    }

    /// Record a numeric value (e.g. revenue) alongside a conversion.
    pub fn record_value(&mut self, experiment_id: &str, variant_id: &str, value: f64) {
        let e = self.entry(experiment_id, variant_id);
        e.conversions  += 1;
        e.total_value  += value;
    }

    /// Retrieve stats for a specific variant.
    pub fn stats_for(
        &self,
        experiment_id: &str,
        variant_id: &str,
    ) -> Result<&VariantStats, MetricsError> {
        let exp = self.data
            .get(experiment_id)
            .ok_or_else(|| MetricsError::ExperimentNotFound(experiment_id.to_owned()))?;
        exp.get(variant_id)
            .ok_or_else(|| MetricsError::VariantNotFound(
                variant_id.to_owned(),
                experiment_id.to_owned(),
            ))
    }

    /// All variant stats for an experiment.
    pub fn all_stats(&self, experiment_id: &str) -> Vec<&VariantStats> {
        self.data
            .get(experiment_id)
            .map(|m| m.values().collect())
            .unwrap_or_default()
    }

    /// Total exposures across all variants of an experiment.
    pub fn total_exposures(&self, experiment_id: &str) -> u64 {
        self.data
            .get(experiment_id)
            .map(|m| m.values().map(|s| s.exposures).sum())
            .unwrap_or(0)
    }

    /// Total conversions across all variants of an experiment.
    pub fn total_conversions(&self, experiment_id: &str) -> u64 {
        self.data
            .get(experiment_id)
            .map(|m| m.values().map(|s| s.conversions).sum())
            .unwrap_or(0)
    }

    /// Reset all metrics for a specific experiment.
    pub fn reset_experiment(&mut self, experiment_id: &str) {
        self.data.remove(experiment_id);
    }

    /// Reset all metrics.
    pub fn reset_all(&mut self) {
        self.data.clear();
    }

    /// Number of tracked experiments.
    pub fn experiment_count(&self) -> usize {
        self.data.len()
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_and_retrieve_exposures() {
        let mut m = ExperimentMetrics::new();
        m.record_exposure("exp-1", "control");
        m.record_exposure("exp-1", "control");
        let s = m.stats_for("exp-1", "control").unwrap();
        assert_eq!(s.exposures, 2);
        assert_eq!(s.conversions, 0);
    }

    #[test]
    fn record_and_retrieve_conversions() {
        let mut m = ExperimentMetrics::new();
        m.record_exposure("exp-1", "treatment");
        m.record_conversion("exp-1", "treatment");
        let s = m.stats_for("exp-1", "treatment").unwrap();
        assert_eq!(s.conversions, 1);
    }

    #[test]
    fn conversion_rate_correct() {
        let mut m = ExperimentMetrics::new();
        for _ in 0..10 { m.record_exposure("e", "v"); }
        for _ in 0..4  { m.record_conversion("e", "v"); }
        let s = m.stats_for("e", "v").unwrap();
        assert!((s.conversion_rate() - 0.4).abs() < 1e-9);
    }

    #[test]
    fn conversion_rate_zero_when_no_exposures() {
        let s = VariantStats::new("v");
        assert_eq!(s.conversion_rate(), 0.0);
    }

    #[test]
    fn record_value_accumulates() {
        let mut m = ExperimentMetrics::new();
        m.record_exposure("e", "v");
        m.record_value("e", "v", 10.0);
        m.record_value("e", "v", 20.0);
        let s = m.stats_for("e", "v").unwrap();
        assert!((s.total_value - 30.0).abs() < 1e-9);
        assert_eq!(s.conversions, 2);
    }

    #[test]
    fn avg_value_correct() {
        let mut s = VariantStats::new("v");
        s.exposures   = 4;
        s.total_value = 80.0;
        assert!((s.avg_value() - 20.0).abs() < 1e-9);
    }

    #[test]
    fn lift_vs_positive_when_better() {
        let mut s = VariantStats::new("v");
        s.exposures   = 100;
        s.conversions = 60;
        assert!(s.lift_vs(0.50) > 0.0);
    }

    #[test]
    fn total_exposures_sums_variants() {
        let mut m = ExperimentMetrics::new();
        for _ in 0..5 { m.record_exposure("e", "control"); }
        for _ in 0..7 { m.record_exposure("e", "treatment"); }
        assert_eq!(m.total_exposures("e"), 12);
    }

    #[test]
    fn total_conversions_sums_variants() {
        let mut m = ExperimentMetrics::new();
        for _ in 0..3 { m.record_conversion("e", "control"); }
        for _ in 0..4 { m.record_conversion("e", "treatment"); }
        assert_eq!(m.total_conversions("e"), 7);
    }

    #[test]
    fn unknown_experiment_returns_error() {
        let m = ExperimentMetrics::new();
        assert!(matches!(
            m.stats_for("missing", "c"),
            Err(MetricsError::ExperimentNotFound(_))
        ));
    }

    #[test]
    fn unknown_variant_returns_error() {
        let mut m = ExperimentMetrics::new();
        m.record_exposure("e", "a");
        assert!(matches!(
            m.stats_for("e", "missing"),
            Err(MetricsError::VariantNotFound(_, _))
        ));
    }

    #[test]
    fn reset_experiment_clears_data() {
        let mut m = ExperimentMetrics::new();
        m.record_exposure("e", "v");
        m.reset_experiment("e");
        assert_eq!(m.experiment_count(), 0);
    }
}
