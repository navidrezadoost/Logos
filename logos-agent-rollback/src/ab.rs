//! A/B testing — traffic splitting between two agent versions.

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ── Errors ────────────────────────────────────────────────────────────────────

#[derive(Debug, Error, PartialEq)]
pub enum AbError {
    #[error("traffic split must sum to 100, got {0}")]
    InvalidSplit(u8),
    #[error("experiment '{0}' already exists")]
    DuplicateExperiment(String),
    #[error("experiment '{0}' not found")]
    ExperimentNotFound(String),
    #[error("experiment '{0}' is not active")]
    NotActive(String),
    #[error("sample percentage must be 0–100, got {0}")]
    InvalidSample(u8),
}

// ── Model ─────────────────────────────────────────────────────────────────────

/// Status of an A/B experiment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExperimentStatus {
    Active,
    Paused,
    Concluded,
}

impl ExperimentStatus {
    pub fn is_active(self) -> bool {
        self == Self::Active
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Concluded => "concluded",
        }
    }
}

/// An A/B experiment comparing two agent versions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AbExperiment {
    pub id: String,
    pub agent_id: String,
    /// Control — the current production version
    pub control_version: u32,
    /// Challenger — the new version being tested
    pub challenger_version: u32,
    /// Percentage of traffic routed to the challenger (0–100).
    /// Control receives `100 - challenger_pct`.
    pub challenger_pct: u8,
    pub status: ExperimentStatus,
    pub created_at: u64,
    /// Cumulative requests routed to each arm
    pub control_requests: u64,
    pub challenger_requests: u64,
}

impl AbExperiment {
    /// Winner arm based on which version received more traffic (tie → control).
    pub fn winner(&self) -> u32 {
        if self.challenger_requests > self.control_requests {
            self.challenger_version
        } else {
            self.control_version
        }
    }
}

// ── Registry ──────────────────────────────────────────────────────────────────

/// Registry that creates and manages A/B experiments.
#[derive(Debug, Default)]
pub struct AbRegistry {
    experiments: Vec<AbExperiment>,
}

impl AbRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    // ── Lifecycle ─────────────────────────────────────────────────────────────

    /// Create a new experiment.  `challenger_pct` is the percentage routed to
    /// the challenger; `100 - challenger_pct` goes to the control.
    pub fn create(
        &mut self,
        id: impl Into<String>,
        agent_id: impl Into<String>,
        control_version: u32,
        challenger_version: u32,
        challenger_pct: u8,
        created_at: u64,
    ) -> Result<&AbExperiment, AbError> {
        if challenger_pct > 100 {
            return Err(AbError::InvalidSplit(challenger_pct));
        }
        let id = id.into();
        if self.experiments.iter().any(|e| e.id == id) {
            return Err(AbError::DuplicateExperiment(id));
        }
        self.experiments.push(AbExperiment {
            id: id.clone(),
            agent_id: agent_id.into(),
            control_version,
            challenger_version,
            challenger_pct,
            status: ExperimentStatus::Active,
            created_at,
            control_requests: 0,
            challenger_requests: 0,
        });
        Ok(self.experiments.last().unwrap())
    }

    /// Route a single request: returns which version should handle it.
    ///
    /// `sample` is a value 0–99 representing the traffic bucket for this
    /// request (e.g. `user_id % 100`).
    pub fn route(&mut self, experiment_id: &str, sample: u8) -> Result<u32, AbError> {
        if sample > 99 {
            return Err(AbError::InvalidSample(sample));
        }
        let exp = self
            .experiments
            .iter_mut()
            .find(|e| e.id == experiment_id)
            .ok_or_else(|| AbError::ExperimentNotFound(experiment_id.to_owned()))?;
        if !exp.status.is_active() {
            return Err(AbError::NotActive(experiment_id.to_owned()));
        }
        if sample < exp.challenger_pct {
            exp.challenger_requests += 1;
            Ok(exp.challenger_version)
        } else {
            exp.control_requests += 1;
            Ok(exp.control_version)
        }
    }

    pub fn pause(&mut self, id: &str) -> Result<(), AbError> {
        self.get_mut(id)?.status = ExperimentStatus::Paused;
        Ok(())
    }

    pub fn conclude(&mut self, id: &str) -> Result<(), AbError> {
        self.get_mut(id)?.status = ExperimentStatus::Concluded;
        Ok(())
    }

    // ── Queries ───────────────────────────────────────────────────────────────

    pub fn get(&self, id: &str) -> Result<&AbExperiment, AbError> {
        self.experiments
            .iter()
            .find(|e| e.id == id)
            .ok_or_else(|| AbError::ExperimentNotFound(id.to_owned()))
    }

    fn get_mut(&mut self, id: &str) -> Result<&mut AbExperiment, AbError> {
        self.experiments
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or_else(|| AbError::ExperimentNotFound(id.to_owned()))
    }

    pub fn active_count(&self) -> usize {
        self.experiments
            .iter()
            .filter(|e| e.status.is_active())
            .count()
    }

    pub fn all(&self) -> &[AbExperiment] {
        &self.experiments
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn reg_with_exp(pct: u8) -> AbRegistry {
        let mut r = AbRegistry::new();
        r.create("exp1", "bot", 1, 2, pct, 0).unwrap();
        r
    }

    #[test]
    fn create_experiment() {
        let r = reg_with_exp(20);
        let exp = r.get("exp1").unwrap();
        assert_eq!(exp.control_version, 1);
        assert_eq!(exp.challenger_version, 2);
        assert_eq!(exp.challenger_pct, 20);
        assert_eq!(exp.status, ExperimentStatus::Active);
    }

    #[test]
    fn route_below_pct_goes_challenger() {
        let mut r = reg_with_exp(50);
        let ver = r.route("exp1", 10).unwrap();
        assert_eq!(ver, 2);
        assert_eq!(r.get("exp1").unwrap().challenger_requests, 1);
    }

    #[test]
    fn route_at_pct_goes_control() {
        let mut r = reg_with_exp(50);
        let ver = r.route("exp1", 50).unwrap();
        assert_eq!(ver, 1);
        assert_eq!(r.get("exp1").unwrap().control_requests, 1);
    }

    #[test]
    fn route_above_pct_goes_control() {
        let mut r = reg_with_exp(30);
        assert_eq!(r.route("exp1", 80).unwrap(), 1);
    }

    #[test]
    fn zero_pct_all_control() {
        let mut r = reg_with_exp(0);
        for i in 0..10u8 {
            assert_eq!(r.route("exp1", i).unwrap(), 1);
        }
    }

    #[test]
    fn hundred_pct_all_challenger() {
        let mut r = reg_with_exp(100);
        // sample 0–99 all < 100
        for i in 0..10u8 {
            assert_eq!(r.route("exp1", i).unwrap(), 2);
        }
    }

    #[test]
    fn route_paused_errors() {
        let mut r = reg_with_exp(50);
        r.pause("exp1").unwrap();
        assert_eq!(
            r.route("exp1", 10),
            Err(AbError::NotActive("exp1".into()))
        );
    }

    #[test]
    fn route_concluded_errors() {
        let mut r = reg_with_exp(50);
        r.conclude("exp1").unwrap();
        assert!(r.route("exp1", 10).is_err());
    }

    #[test]
    fn duplicate_experiment_errors() {
        let mut r = reg_with_exp(50);
        assert_eq!(
            r.create("exp1", "bot", 1, 2, 50, 0),
            Err(AbError::DuplicateExperiment("exp1".into()))
        );
    }

    #[test]
    fn invalid_split_errors() {
        let mut r = AbRegistry::new();
        assert_eq!(
            r.create("e", "bot", 1, 2, 101, 0),
            Err(AbError::InvalidSplit(101))
        );
    }

    #[test]
    fn invalid_sample_errors() {
        let mut r = reg_with_exp(50);
        assert_eq!(r.route("exp1", 100), Err(AbError::InvalidSample(100)));
    }

    #[test]
    fn winner_higher_challenger_requests() {
        let mut r = reg_with_exp(90);
        for i in 0..9u8 {
            r.route("exp1", i).unwrap();
        }
        r.route("exp1", 95).unwrap();
        let exp = r.get("exp1").unwrap();
        assert_eq!(exp.winner(), 2);
    }

    #[test]
    fn winner_tie_goes_control() {
        let mut r = reg_with_exp(50);
        r.route("exp1", 10).unwrap(); // challenger
        r.route("exp1", 60).unwrap(); // control
        let exp = r.get("exp1").unwrap();
        assert_eq!(exp.winner(), 1);
    }

    #[test]
    fn active_count() {
        let mut r = AbRegistry::new();
        r.create("e1", "bot", 1, 2, 50, 0).unwrap();
        r.create("e2", "bot", 1, 2, 50, 0).unwrap();
        r.pause("e1").unwrap();
        assert_eq!(r.active_count(), 1);
    }

    #[test]
    fn status_as_str() {
        assert_eq!(ExperimentStatus::Active.as_str(), "active");
        assert_eq!(ExperimentStatus::Paused.as_str(), "paused");
        assert_eq!(ExperimentStatus::Concluded.as_str(), "concluded");
    }
}
