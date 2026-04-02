//! Experiment lifecycle — create, run, conclude, report.

use crate::metrics::ExperimentMetrics;
use crate::stats::{ZTest, ZTestResult};
use thiserror::Error;

/// Errors from experiment operations.
#[derive(Debug, Error, PartialEq)]
pub enum ExperimentError {
    #[error("variant weights must sum to 100 (got {0})")]
    InvalidWeights(u32),
    #[error("experiment must have at least 2 variants")]
    TooFewVariants,
    #[error("experiment is not running (state: {0:?})")]
    NotRunning(ExperimentState),
    #[error("experiment is already concluded")]
    AlreadyConcluded,
    #[error("variant '{0}' not found")]
    VariantNotFound(String),
    #[error("experiment id is empty")]
    EmptyId,
}

/// Single variant in an experiment.
#[derive(Debug, Clone, PartialEq)]
pub struct Variant {
    pub name:   String,
    /// Traffic weight out of 100.
    pub weight: u32,
}

impl Variant {
    pub fn new(name: impl Into<String>, weight: u32) -> Self {
        Self { name: name.into(), weight }
    }
}

/// Lifecycle state of an experiment.
#[derive(Debug, Clone, PartialEq)]
pub enum ExperimentState {
    Draft,
    Running,
    Paused,
    Concluded { winner: Option<String> },
}

/// Immutable configuration snapshot of an experiment.
#[derive(Debug, Clone)]
pub struct ExperimentConfig {
    pub id:       String,
    pub variants: Vec<Variant>,
}

impl ExperimentConfig {
    /// Create a validated config. Weights must sum to exactly 100.
    pub fn new(
        id: impl Into<String>,
        variants: Vec<Variant>,
    ) -> Result<Self, ExperimentError> {
        let id = id.into();
        if id.is_empty() {
            return Err(ExperimentError::EmptyId);
        }
        if variants.len() < 2 {
            return Err(ExperimentError::TooFewVariants);
        }
        let total: u32 = variants.iter().map(|v| v.weight).sum();
        if total != 100 {
            return Err(ExperimentError::InvalidWeights(total));
        }
        Ok(Self { id, variants })
    }

    pub fn variant_names(&self) -> Vec<&str> {
        self.variants.iter().map(|v| v.name.as_str()).collect()
    }

    pub fn has_variant(&self, name: &str) -> bool {
        self.variants.iter().any(|v| v.name == name)
    }
}

/// Full experiment report once comparison data is available.
#[derive(Debug)]
pub struct ExperimentReport {
    pub experiment_id: String,
    pub control_variant:   String,
    pub treatment_variant: String,
    pub result: ZTestResult,
    pub recommended_winner: Option<String>,
}

impl ExperimentReport {
    pub fn is_significant(&self) -> bool {
        self.result.is_significant
    }
}

/// A live experiment with state management and metrics integration.
#[derive(Debug)]
pub struct Experiment {
    pub config:  ExperimentConfig,
    pub state:   ExperimentState,
    pub metrics: ExperimentMetrics,
}

impl Experiment {
    /// Create a new experiment in Draft state.
    pub fn new(config: ExperimentConfig) -> Self {
        Self {
            config,
            state: ExperimentState::Draft,
            metrics: ExperimentMetrics::new(),
        }
    }

    /// Start the experiment (Draft → Running, or Paused → Running).
    pub fn start(&mut self) -> Result<(), ExperimentError> {
        match &self.state {
            ExperimentState::Draft | ExperimentState::Paused => {
                self.state = ExperimentState::Running;
                Ok(())
            }
            ExperimentState::Concluded { .. } => Err(ExperimentError::AlreadyConcluded),
            ExperimentState::Running => Ok(()), // idempotent
        }
    }

    /// Pause a running experiment.
    pub fn pause(&mut self) -> Result<(), ExperimentError> {
        if self.state == ExperimentState::Running {
            self.state = ExperimentState::Paused;
            Ok(())
        } else {
            Err(ExperimentError::NotRunning(self.state.clone()))
        }
    }

    /// Conclude the experiment, optionally nominating a winner.
    pub fn conclude(&mut self, winner: Option<String>) -> Result<(), ExperimentError> {
        match &self.state {
            ExperimentState::Concluded { .. } => Err(ExperimentError::AlreadyConcluded),
            _ => {
                if let Some(ref w) = winner {
                    if !self.config.has_variant(w) {
                        return Err(ExperimentError::VariantNotFound(w.clone()));
                    }
                }
                self.state = ExperimentState::Concluded { winner };
                Ok(())
            }
        }
    }

    /// Record an exposure event (experiment must be Running).
    pub fn expose(&mut self, variant_id: &str) -> Result<(), ExperimentError> {
        self.require_running()?;
        self.metrics.record_exposure(&self.config.id, variant_id);
        Ok(())
    }

    /// Record a conversion event (experiment must be Running).
    pub fn convert(&mut self, variant_id: &str) -> Result<(), ExperimentError> {
        self.require_running()?;
        self.metrics.record_conversion(&self.config.id, variant_id);
        Ok(())
    }

    /// Record a valued conversion (experiment must be Running).
    pub fn convert_with_value(&mut self, variant_id: &str, value: f64) -> Result<(), ExperimentError> {
        self.require_running()?;
        self.metrics.record_value(&self.config.id, variant_id, value);
        Ok(())
    }

    /// Is the experiment currently running?
    pub fn is_running(&self) -> bool {
        self.state == ExperimentState::Running
    }

    /// Generate a ZTest report comparing two variants.
    pub fn report(
        &self,
        control_variant: &str,
        treatment_variant: &str,
    ) -> Result<ExperimentReport, ExperimentError> {
        if !self.config.has_variant(control_variant) {
            return Err(ExperimentError::VariantNotFound(control_variant.to_owned()));
        }
        if !self.config.has_variant(treatment_variant) {
            return Err(ExperimentError::VariantNotFound(treatment_variant.to_owned()));
        }

        let exp_id = &self.config.id;
        let (n_c, conv_c) = self.metrics
            .stats_for(exp_id, control_variant)
            .map(|s| (s.exposures, s.conversions))
            .unwrap_or((0, 0));
        let (n_t, conv_t) = self.metrics
            .stats_for(exp_id, treatment_variant)
            .map(|s| (s.exposures, s.conversions))
            .unwrap_or((0, 0));

        let result = ZTest::run(n_c, conv_c, n_t, conv_t);
        let recommended_winner = if result.is_significant {
            if result.absolute_lift > 0.0 {
                Some(treatment_variant.to_owned())
            } else {
                Some(control_variant.to_owned())
            }
        } else {
            None
        };

        Ok(ExperimentReport {
            experiment_id: exp_id.clone(),
            control_variant:   control_variant.to_owned(),
            treatment_variant: treatment_variant.to_owned(),
            result,
            recommended_winner,
        })
    }

    fn require_running(&self) -> Result<(), ExperimentError> {
        if self.state == ExperimentState::Running {
            Ok(())
        } else {
            Err(ExperimentError::NotRunning(self.state.clone()))
        }
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn simple_exp() -> Experiment {
        let cfg = ExperimentConfig::new(
            "exp-1",
            vec![Variant::new("control", 50), Variant::new("treatment", 50)],
        ).unwrap();
        Experiment::new(cfg)
    }

    #[test]
    fn new_experiment_is_draft() {
        let e = simple_exp();
        assert_eq!(e.state, ExperimentState::Draft);
    }

    #[test]
    fn start_moves_to_running() {
        let mut e = simple_exp();
        e.start().unwrap();
        assert!(e.is_running());
    }

    #[test]
    fn pause_moves_to_paused() {
        let mut e = simple_exp();
        e.start().unwrap();
        e.pause().unwrap();
        assert_eq!(e.state, ExperimentState::Paused);
    }

    #[test]
    fn conclude_with_winner() {
        let mut e = simple_exp();
        e.start().unwrap();
        e.conclude(Some("treatment".to_owned())).unwrap();
        assert!(matches!(e.state, ExperimentState::Concluded { winner: Some(_) }));
    }

    #[test]
    fn conclude_twice_is_error() {
        let mut e = simple_exp();
        e.conclude(None).unwrap();
        assert!(matches!(e.conclude(None), Err(ExperimentError::AlreadyConcluded)));
    }

    #[test]
    fn expose_requires_running() {
        let mut e = simple_exp();
        assert!(matches!(
            e.expose("control"),
            Err(ExperimentError::NotRunning(_))
        ));
    }

    #[test]
    fn expose_and_convert_accumulate() {
        let mut e = simple_exp();
        e.start().unwrap();
        for _ in 0..10 { e.expose("control").unwrap(); }
        for _ in 0..5  { e.convert("control").unwrap(); }
        let s = e.metrics.stats_for("exp-1", "control").unwrap();
        assert_eq!(s.exposures, 10);
        assert_eq!(s.conversions, 5);
    }

    #[test]
    fn invalid_weights_rejected() {
        let result = ExperimentConfig::new(
            "bad",
            vec![Variant::new("a", 40), Variant::new("b", 40)],
        );
        assert!(matches!(result, Err(ExperimentError::InvalidWeights(80))));
    }

    #[test]
    fn too_few_variants_rejected() {
        let result = ExperimentConfig::new("x", vec![Variant::new("only", 100)]);
        assert!(matches!(result, Err(ExperimentError::TooFewVariants)));
    }

    #[test]
    fn report_recommends_winner_when_significant() {
        let cfg = ExperimentConfig::new(
            "big-exp",
            vec![Variant::new("control", 50), Variant::new("treatment", 50)],
        ).unwrap();
        let mut e = Experiment::new(cfg);
        e.start().unwrap();
        // control: 5000 / 10000, treatment: 6200 / 10000
        for _ in 0..10_000 { e.expose("control").unwrap(); }
        for _ in 0..5_000  { e.convert("control").unwrap(); }
        for _ in 0..10_000 { e.expose("treatment").unwrap(); }
        for _ in 0..6_200  { e.convert("treatment").unwrap(); }
        let report = e.report("control", "treatment").unwrap();
        assert!(report.is_significant());
        assert_eq!(report.recommended_winner, Some("treatment".to_owned()));
    }

    #[test]
    fn report_unknown_variant_returns_error() {
        let e = simple_exp();
        assert!(matches!(
            e.report("control", "ghost"),
            Err(ExperimentError::VariantNotFound(_))
        ));
    }
}
