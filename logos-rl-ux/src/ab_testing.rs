//! A/B Testing — production experiment framework for RL-UX policy variants
//!
//! Runs controlled experiments comparing RL policy variants (treatment) against
//! the baseline heuristic (control). Tracks statistical significance, computes
//! lift, handles traffic splitting, and records experiment results.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Experiment variant ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExperimentVariant {
    /// Existing rule-based suggestions.
    Control,
    /// RL Q-table predictions.
    Treatment(String),
}

impl ExperimentVariant {
    pub fn label(&self) -> &str {
        match self {
            ExperimentVariant::Control => "control",
            ExperimentVariant::Treatment(name) => name,
        }
    }

    pub fn is_control(&self) -> bool { matches!(self, ExperimentVariant::Control) }
}

// ── Traffic split ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrafficSplit {
    /// Fraction of traffic sent to treatment (0.0–1.0).
    pub treatment_fraction: f32,
    pub rng_seed: u64,
}

impl TrafficSplit {
    pub fn new(treatment_fraction: f32) -> Self {
        assert!((0.0..=1.0).contains(&treatment_fraction));
        TrafficSplit { treatment_fraction, rng_seed: 42 }
    }

    /// Deterministic assignment based on session_id hash.
    pub fn assign(&self, session_id: &str) -> ExperimentVariant {
        let hash = simple_hash(session_id, self.rng_seed);
        let bucket = (hash % 1_000_000) as f32 / 1_000_000.0;
        if bucket < self.treatment_fraction {
            ExperimentVariant::Treatment("rl_v1".into())
        } else {
            ExperimentVariant::Control
        }
    }
}

fn simple_hash(s: &str, seed: u64) -> u64 {
    let mut h = seed.wrapping_add(0x9e3779b97f4a7c15);
    for b in s.bytes() {
        h = h.wrapping_mul(0x6c62272e07bb0142).wrapping_add(b as u64);
    }
    h
}

// ── Experiment metrics ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VariantMetrics {
    pub variant: String,
    pub sessions: u32,
    pub suggestions_shown: u32,
    pub suggestions_accepted: u32,
    pub suggestions_rejected: u32,
    pub total_reward: f32,
    pub sum_action_latency_ms: u64,
}

impl VariantMetrics {
    pub fn new(variant: impl Into<String>) -> Self {
        VariantMetrics { variant: variant.into(), ..Default::default() }
    }

    pub fn acceptance_rate(&self) -> f32 {
        if self.suggestions_shown == 0 { return 0.0; }
        self.suggestions_accepted as f32 / self.suggestions_shown as f32
    }

    pub fn avg_reward(&self) -> f32 {
        if self.sessions == 0 { return 0.0; }
        self.total_reward / self.sessions as f32
    }

    pub fn avg_latency_ms(&self) -> f32 {
        if self.suggestions_shown == 0 { return 0.0; }
        self.sum_action_latency_ms as f32 / self.suggestions_shown as f32
    }

    pub fn record_suggestion(&mut self, accepted: bool, reward: f32, latency_ms: u64) {
        self.suggestions_shown += 1;
        if accepted { self.suggestions_accepted += 1; } else { self.suggestions_rejected += 1; }
        self.total_reward += reward;
        self.sum_action_latency_ms += latency_ms;
    }
}

// ── Statistical test ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatTest {
    pub control_rate: f32,
    pub treatment_rate: f32,
    pub lift_pct: f32,
    pub z_score: f32,
    pub is_significant: bool,
    pub confidence_pct: f32,
}

impl StatTest {
    /// Two-proportion Z-test.
    pub fn compute(
        control: &VariantMetrics,
        treatment: &VariantMetrics,
        confidence: f32,
    ) -> Self {
        let p1 = control.acceptance_rate();
        let p2 = treatment.acceptance_rate();
        let n1 = control.suggestions_shown as f32;
        let n2 = treatment.suggestions_shown as f32;

        let lift_pct = if p1 == 0.0 { 0.0 } else { (p2 - p1) / p1 * 100.0 };

        let z_score = if n1 < 2.0 || n2 < 2.0 {
            0.0
        } else {
            let p_pool = (control.suggestions_accepted + treatment.suggestions_accepted) as f32
                / (n1 + n2);
            let denom = (p_pool * (1.0 - p_pool) * (1.0 / n1 + 1.0 / n2)).sqrt();
            if denom == 0.0 { 0.0 } else { (p2 - p1) / denom }
        };

        // Critical z values: 90%→1.28, 95%→1.645, 99%→2.326
        let z_critical = if confidence >= 99.0 { 2.326 }
            else if confidence >= 95.0 { 1.645 }
            else { 1.28 };

        StatTest {
            control_rate: p1,
            treatment_rate: p2,
            lift_pct,
            z_score,
            is_significant: z_score.abs() >= z_critical,
            confidence_pct: confidence,
        }
    }

    pub fn is_treatment_better(&self) -> bool {
        self.is_significant && self.lift_pct > 0.0
    }
}

// ── Experiment config ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentConfig {
    pub name: String,
    pub description: String,
    pub traffic_split: f32,
    pub min_sample_size: u32,
    pub confidence_level: f32,
    pub max_duration_days: u32,
}

impl ExperimentConfig {
    pub fn new(name: impl Into<String>, traffic_split: f32) -> Self {
        ExperimentConfig {
            name: name.into(),
            description: String::new(),
            traffic_split,
            min_sample_size: 1000,
            confidence_level: 95.0,
            max_duration_days: 14,
        }
    }
}

// ── Experiment state ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExperimentStatus {
    Draft,
    Running,
    Paused,
    Concluded { winner: String },
    Aborted { reason: String },
}

impl ExperimentStatus {
    pub fn is_active(&self) -> bool { matches!(self, ExperimentStatus::Running) }
}

// ── Experiment ────────────────────────────────────────────────────────────────

pub struct Experiment {
    pub id: String,
    pub config: ExperimentConfig,
    pub status: ExperimentStatus,
    pub split: TrafficSplit,
    pub control: VariantMetrics,
    pub treatment: VariantMetrics,
    pub started_ts: Option<u64>,
    pub ended_ts: Option<u64>,
    enrolled_sessions: std::collections::HashSet<String>,
}

impl Experiment {
    pub fn new(config: ExperimentConfig) -> Self {
        let split = TrafficSplit::new(config.traffic_split);
        let treatment_name = format!("{}_treatment", config.name);
        Experiment {
            id: uuid::Uuid::new_v4().to_string(),
            split,
            control: VariantMetrics::new("control"),
            treatment: VariantMetrics::new(&treatment_name),
            status: ExperimentStatus::Draft,
            started_ts: None,
            ended_ts: None,
            enrolled_sessions: std::collections::HashSet::new(),
            config,
        }
    }

    pub fn start(&mut self, ts: u64) {
        self.status = ExperimentStatus::Running;
        self.started_ts = Some(ts);
    }

    pub fn pause(&mut self) {
        if self.status.is_active() {
            self.status = ExperimentStatus::Paused;
        }
    }

    pub fn assign_variant(&mut self, session_id: &str) -> ExperimentVariant {
        self.enrolled_sessions.insert(session_id.to_string());
        self.split.assign(session_id)
    }

    pub fn record_observation(
        &mut self,
        session_id: &str,
        accepted: bool,
        reward: f32,
        latency_ms: u64,
    ) {
        if !self.status.is_active() { return; }
        let variant = self.split.assign(session_id);
        match variant {
            ExperimentVariant::Control => {
                self.control.sessions += 1;
                self.control.record_suggestion(accepted, reward, latency_ms);
            }
            ExperimentVariant::Treatment(_) => {
                self.treatment.sessions += 1;
                self.treatment.record_suggestion(accepted, reward, latency_ms);
            }
        }
    }

    pub fn compute_stat_test(&self) -> StatTest {
        StatTest::compute(&self.control, &self.treatment, self.config.confidence_level)
    }

    pub fn has_enough_data(&self) -> bool {
        self.control.suggestions_shown >= self.config.min_sample_size
            && self.treatment.suggestions_shown >= self.config.min_sample_size
    }

    pub fn conclude(&mut self, ts: u64) -> String {
        let test = self.compute_stat_test();
        let winner = if test.is_treatment_better() {
            self.treatment.variant.clone()
        } else {
            "control".to_string()
        };
        self.status = ExperimentStatus::Concluded { winner: winner.clone() };
        self.ended_ts = Some(ts);
        winner
    }

    pub fn enrolled_count(&self) -> usize { self.enrolled_sessions.len() }

    pub fn lift_summary(&self) -> String {
        let test = self.compute_stat_test();
        format!(
            "Experiment '{}': control={:.1}%, treatment={:.1}%, lift={:.1}%, significant={}",
            self.config.name,
            test.control_rate * 100.0,
            test.treatment_rate * 100.0,
            test.lift_pct,
            test.is_significant,
        )
    }
}

// ── Experiment registry ───────────────────────────────────────────────────────

pub struct ExperimentRegistry {
    experiments: HashMap<String, Experiment>,
}

impl ExperimentRegistry {
    pub fn new() -> Self { ExperimentRegistry { experiments: HashMap::new() } }

    pub fn add(&mut self, exp: Experiment) {
        self.experiments.insert(exp.config.name.clone(), exp);
    }

    pub fn get(&self, name: &str) -> Option<&Experiment> { self.experiments.get(name) }
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Experiment> { self.experiments.get_mut(name) }
    pub fn count(&self) -> usize { self.experiments.len() }
    pub fn active_count(&self) -> usize { self.experiments.values().filter(|e| e.status.is_active()).count() }
}

impl Default for ExperimentRegistry {
    fn default() -> Self { Self::new() }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn exp() -> Experiment {
        Experiment::new(ExperimentConfig::new("rl_v1_test", 0.5))
    }

    #[test]
    fn experiment_starts_as_draft() {
        assert_eq!(exp().status, ExperimentStatus::Draft);
    }

    #[test]
    fn experiment_start_transitions_to_running() {
        let mut e = exp();
        e.start(100);
        assert!(e.status.is_active());
        assert_eq!(e.started_ts, Some(100));
    }

    #[test]
    fn traffic_split_deterministic() {
        let split = TrafficSplit::new(0.5);
        let v1 = split.assign("user-abc");
        let v2 = split.assign("user-abc");
        assert_eq!(v1, v2, "Same session must get same variant");
    }

    #[test]
    fn traffic_split_distributes_roughly_evenly() {
        let split = TrafficSplit::new(0.5);
        let (mut ctrl, mut treat) = (0, 0);
        for i in 0..1000 {
            match split.assign(&format!("user-{}", i)) {
                ExperimentVariant::Control => ctrl += 1,
                ExperimentVariant::Treatment(_) => treat += 1,
            }
        }
        // Allow 10% deviation from 50/50
        assert!(ctrl > 400 && ctrl < 600, "ctrl={}, treat={}", ctrl, treat);
    }

    #[test]
    fn variant_metrics_acceptance_rate() {
        let mut m = VariantMetrics::new("test");
        m.record_suggestion(true, 1.0, 50);
        m.record_suggestion(true, 1.0, 50);
        m.record_suggestion(false, 0.0, 100);
        assert!((m.acceptance_rate() - 2.0/3.0).abs() < 0.01);
    }

    #[test]
    fn stat_test_detects_significant_lift() {
        let mut ctrl = VariantMetrics::new("control");
        let mut treat = VariantMetrics::new("treatment");
        // High sample, big lift
        for _ in 0..1000 { ctrl.record_suggestion(true, 1.0, 50); ctrl.record_suggestion(false, 0.0, 50); }
        for _ in 0..1000 { treat.record_suggestion(true, 1.0, 50); treat.record_suggestion(true, 1.0, 50); treat.record_suggestion(false, 0.0, 50); }
        let test = StatTest::compute(&ctrl, &treat, 95.0);
        assert!(test.is_significant, "Should detect significance with large sample + big lift");
        assert!(test.is_treatment_better());
    }

    #[test]
    fn stat_test_no_significance_small_sample() {
        let mut ctrl = VariantMetrics::new("control");
        let mut treat = VariantMetrics::new("treatment");
        ctrl.record_suggestion(true, 1.0, 50);
        treat.record_suggestion(true, 1.0, 50);
        let test = StatTest::compute(&ctrl, &treat, 95.0);
        assert!(!test.is_significant);
    }

    #[test]
    fn experiment_records_observations() {
        let mut e = exp();
        e.start(0);
        // Session assigned to treatment
        for i in 0..20 {
            e.record_observation(&format!("user-{}", i), true, 1.0, 50);
        }
        let total = e.control.suggestions_shown + e.treatment.suggestions_shown;
        assert_eq!(total, 20);
    }

    #[test]
    fn experiment_conclude_picks_winner() {
        let mut e = Experiment::new(ExperimentConfig {
            min_sample_size: 0,
            ..ExperimentConfig::new("quick", 0.5)
        });
        e.start(0);
        // Pump up treatment acceptance
        for i in 0..100 {
            e.record_observation(&format!("user-{}", i), i % 3 != 0, 1.0, 50);
        }
        let winner = e.conclude(100);
        assert!(!winner.is_empty());
        assert!(matches!(e.status, ExperimentStatus::Concluded { .. }));
    }

    #[test]
    fn experiment_registry_active_count() {
        let mut reg = ExperimentRegistry::new();
        let mut e1 = Experiment::new(ExperimentConfig::new("exp1", 0.5));
        e1.start(0);
        let e2 = Experiment::new(ExperimentConfig::new("exp2", 0.3));
        reg.add(e1); reg.add(e2);
        assert_eq!(reg.count(), 2);
        assert_eq!(reg.active_count(), 1);
    }
}
