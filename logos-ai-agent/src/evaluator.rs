//! Evaluator — score test results and assign Junior/MidLevel/Senior certification
//!
//! After the test suite runs, this module aggregates scores by level, computes
//! percentages, identifies strengths/weaknesses, and issues a formal EvaluationReport.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use crate::test_suite::{TestLevel, TestResult as SuiteTestResult, TestSuite};

// ── Agent level ───────────────────────────────────────────────────────────────

/// Certified competency level for an AI agent integrated with Logos.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum AgentLevel {
    /// 0–59%: Basic layer operations only; requires guidance for advanced tasks.
    Junior,
    /// 60–79%: Formulas, plugins, design patterns; handles most common workflows.
    MidLevel,
    /// 80%+: Full system mastery including AI features, CRDT, complex workflows.
    Senior,
    /// Not yet evaluated.
    Uncertified,
}

impl AgentLevel {
    pub fn display_name(&self) -> &str {
        match self {
            AgentLevel::Junior => "Junior",
            AgentLevel::MidLevel => "Mid-Level",
            AgentLevel::Senior => "Senior",
            AgentLevel::Uncertified => "Uncertified",
        }
    }

    /// Minimum score percentage required to reach this level (inclusive).
    pub fn min_score_pct(&self) -> f32 {
        match self {
            AgentLevel::Junior => 0.0,
            AgentLevel::MidLevel => 60.0,
            AgentLevel::Senior => 80.0,
            AgentLevel::Uncertified => 0.0,
        }
    }

    /// From a score percentage, determine the level.
    pub fn from_score(pct: f32) -> Self {
        if pct >= 80.0 { AgentLevel::Senior }
        else if pct >= 60.0 { AgentLevel::MidLevel }
        else { AgentLevel::Junior }
    }

    /// What the agent is cleared to do at this level.
    pub fn capabilities_description(&self) -> &str {
        match self {
            AgentLevel::Junior =>
                "Basic layer creation, resizing, fill/stroke properties, and simple moves.",
            AgentLevel::MidLevel =>
                "All Junior capabilities plus spreadsheet formulas, plugin calls, \
                design pattern application, and accessibility checks.",
            AgentLevel::Senior =>
                "Full system mastery: complex multi-step workflows, AI pipeline, \
                CRDT collaboration, component libraries, and data-driven designs.",
            AgentLevel::Uncertified =>
                "No capabilities verified. Run evaluation first.",
        }
    }

    /// Human-readable badge color for UI display.
    pub fn badge_color(&self) -> &str {
        match self {
            AgentLevel::Junior => "#f59e0b",    // amber
            AgentLevel::MidLevel => "#3b82f6",  // blue
            AgentLevel::Senior => "#10b981",    // green
            AgentLevel::Uncertified => "#6b7280", // gray
        }
    }
}

// ── Score breakdown ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreBreakdown {
    pub level: TestLevel,
    pub points_earned: u32,
    pub points_possible: u32,
    pub tests_passed: usize,
    pub tests_total: usize,
    pub pct: f32,
}

impl ScoreBreakdown {
    pub fn pass_rate(&self) -> f32 {
        if self.tests_total == 0 { return 0.0; }
        self.tests_passed as f32 / self.tests_total as f32 * 100.0
    }
}

// ── Level thresholds ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct LevelThresholds {
    pub junior_max: f32,    // 0..this → Junior
    pub midlevel_max: f32,  // this..senior → MidLevel
    // Above midlevel_max → Senior
}

impl Default for LevelThresholds {
    fn default() -> Self {
        LevelThresholds { junior_max: 60.0, midlevel_max: 80.0 }
    }
}

// ── Evaluation config ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct EvaluationConfig {
    pub thresholds: LevelThresholds,
    /// Weight of each test level in the final score. Must sum to 1.0.
    pub level_weights: HashMap<String, f32>,
}

impl Default for EvaluationConfig {
    fn default() -> Self {
        let mut weights = HashMap::new();
        weights.insert("Simple".into(), 0.20);
        weights.insert("Intermediate".into(), 0.35);
        weights.insert("Complex".into(), 0.35);
        weights.insert("Collaboration".into(), 0.10);
        EvaluationConfig {
            thresholds: LevelThresholds::default(),
            level_weights: weights,
        }
    }
}

// ── Evaluation report ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationReport {
    pub session_id: String,
    /// Certified level.
    pub level: AgentLevel,
    /// Overall weighted score percentage.
    pub overall_score_pct: f32,
    /// Raw score (points earned / points possible × 100).
    pub raw_score_pct: f32,
    /// Per-level breakdowns.
    pub breakdowns: Vec<ScoreBreakdown>,
    /// Identified strength areas.
    pub strengths: Vec<String>,
    /// Identified weakness areas.
    pub weaknesses: Vec<String>,
    /// Actionable recommendations.
    pub recommendations: Vec<String>,
    /// Unix timestamp of evaluation.
    pub evaluated_at: u64,
    /// Total tests run.
    pub tests_run: usize,
    /// Total tests passed.
    pub tests_passed: usize,
}

impl EvaluationReport {
    pub fn pass_rate(&self) -> f32 {
        if self.tests_run == 0 { return 0.0; }
        self.tests_passed as f32 / self.tests_run as f32 * 100.0
    }

    pub fn is_certified(&self) -> bool {
        self.level != AgentLevel::Uncertified
    }
}

// ── Evaluator ─────────────────────────────────────────────────────────────────

pub struct Evaluator {
    config: EvaluationConfig,
}

impl Evaluator {
    pub fn new(config: EvaluationConfig) -> Self {
        Evaluator { config }
    }

    /// Evaluate a set of test results against the given suite's metadata.
    pub fn evaluate(
        &self,
        results: &[SuiteTestResult],
        suite: &TestSuite,
        session_id: &str,
        evaluated_at: u64,
    ) -> EvaluationReport {
        // Compute per-level breakdowns
        let levels = [
            TestLevel::Simple,
            TestLevel::Intermediate,
            TestLevel::Complex,
            TestLevel::Collaboration,
        ];

        let mut breakdowns = Vec::new();
        let mut weighted_score = 0.0_f32;
        let total_possible: u32 = suite.total_max_points();

        for level in &levels {
            let cases = suite.by_level(level);
            let case_ids: std::collections::HashSet<usize> =
                cases.iter().map(|c| c.id).collect();

            let level_results: Vec<&SuiteTestResult> = results.iter()
                .filter(|r| case_ids.contains(&r.test_id))
                .collect();

            let points_possible: u32 = cases.iter().map(|c| c.max_points).sum();
            let points_earned: u32 = level_results.iter().map(|r| r.points_earned).sum();
            let tests_passed = level_results.iter().filter(|r| r.passed).count();
            let tests_total = cases.len();

            let pct = if points_possible == 0 { 0.0 }
                      else { points_earned as f32 / points_possible as f32 * 100.0 };

            // Apply weight
            let weight_key = format!("{:?}", level);
            if let Some(&weight) = self.config.level_weights.get(&weight_key) {
                weighted_score += pct * weight;
            }

            breakdowns.push(ScoreBreakdown {
                level: level.clone(),
                points_earned,
                points_possible,
                tests_passed,
                tests_total,
                pct,
            });
        }

        // Raw score
        let total_earned: u32 = results.iter().map(|r| r.points_earned).sum();
        let raw_pct = if total_possible == 0 { 0.0 }
                      else { total_earned as f32 / total_possible as f32 * 100.0 };

        // Derive level
        let level = if weighted_score >= self.config.thresholds.midlevel_max {
            AgentLevel::Senior
        } else if weighted_score >= self.config.thresholds.junior_max {
            AgentLevel::MidLevel
        } else {
            AgentLevel::Junior
        };

        // Strengths & weaknesses
        let mut strengths = Vec::new();
        let mut weaknesses = Vec::new();
        for bd in &breakdowns {
            if bd.pct >= 80.0 {
                strengths.push(format!("{:?} ({:.0}%)", bd.level, bd.pct));
            } else if bd.pct < 50.0 {
                weaknesses.push(format!("{:?} ({:.0}%)", bd.level, bd.pct));
            }
        }

        // Recommendations
        let mut recs = Vec::new();
        for bd in &breakdowns {
            if bd.pct < 60.0 {
                recs.push(format!(
                    "Improve {:?} skills — review the {} training module.",
                    bd.level,
                    bd.level.description()
                ));
            }
        }
        if level == AgentLevel::Junior {
            recs.push("Focus on mastering basic layer and property commands before attempting complex workflows.".into());
        } else if level == AgentLevel::MidLevel {
            recs.push("Practice multi-step workflows and AI pipeline integration to advance to Senior.".into());
        }

        let tests_run = results.len();
        let tests_passed = results.iter().filter(|r| r.passed).count();

        EvaluationReport {
            session_id: session_id.to_string(),
            level,
            overall_score_pct: weighted_score,
            raw_score_pct: raw_pct,
            breakdowns,
            strengths,
            weaknesses,
            recommendations: recs,
            evaluated_at,
            tests_run,
            tests_passed,
        }
    }
}

impl Default for Evaluator {
    fn default() -> Self {
        Self::new(EvaluationConfig::default())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_suite::{BuiltinTestSuite, TestRunner};

    fn evaluator() -> Evaluator {
        Evaluator::default()
    }

    fn suite() -> TestSuite {
        BuiltinTestSuite::build()
    }

    /// Simulate an agent that always responds perfectly.
    fn perfect_results(suite: &TestSuite) -> Vec<SuiteTestResult> {
        suite.cases.iter().map(|c| {
            let kw_response = c.expected_keywords.join(" ")
                + " " + c.expected_command.as_deref().unwrap_or("");
            TestRunner::evaluate(&kw_response, c, 100)
        }).collect()
    }

    /// Simulate an agent that only answers simple questions correctly.
    fn junior_results(suite: &TestSuite) -> Vec<SuiteTestResult> {
        suite.cases.iter().map(|c| {
            let response = if c.level == TestLevel::Simple {
                c.expected_keywords.join(" ")
                    + " " + c.expected_command.as_deref().unwrap_or("")
            } else {
                "I don't know".to_string()
            };
            TestRunner::evaluate(&response, c, 100)
        }).collect()
    }

    #[test]
    fn senior_agent_certified_senior() {
        let s = suite();
        let results = perfect_results(&s);
        let eval = evaluator();
        let report = eval.evaluate(&results, &s, "sess-1", 1000);
        assert_eq!(report.level, AgentLevel::Senior);
    }

    #[test]
    fn junior_agent_certified_junior() {
        let s = suite();
        let results = junior_results(&s);
        let eval = evaluator();
        let report = eval.evaluate(&results, &s, "sess-2", 1000);
        assert_eq!(report.level, AgentLevel::Junior);
    }

    #[test]
    fn report_has_4_breakdowns() {
        let s = suite();
        let results = perfect_results(&s);
        let report = evaluator().evaluate(&results, &s, "s1", 0);
        assert_eq!(report.breakdowns.len(), 4);
    }

    #[test]
    fn pass_rate_100_for_perfect_agent() {
        let s = suite();
        let results = perfect_results(&s);
        let report = evaluator().evaluate(&results, &s, "s1", 0);
        assert!(report.pass_rate() > 90.0, "Pass rate: {}", report.pass_rate());
    }

    #[test]
    fn zero_results_means_junior() {
        let s = suite();
        let report = evaluator().evaluate(&[], &s, "empty", 0);
        assert_eq!(report.level, AgentLevel::Junior);
        assert_eq!(report.overall_score_pct, 0.0);
    }

    #[test]
    fn senior_has_strengths() {
        let s = suite();
        let results = perfect_results(&s);
        let report = evaluator().evaluate(&results, &s, "s1", 0);
        assert!(!report.strengths.is_empty());
    }

    #[test]
    fn junior_has_recommendations() {
        let s = suite();
        let results = junior_results(&s);
        let report = evaluator().evaluate(&results, &s, "s1", 0);
        assert!(!report.recommendations.is_empty());
    }

    #[test]
    fn agent_level_ordering() {
        assert!(AgentLevel::Junior < AgentLevel::MidLevel);
        assert!(AgentLevel::MidLevel < AgentLevel::Senior);
    }

    #[test]
    fn from_score_boundaries() {
        assert_eq!(AgentLevel::from_score(0.0), AgentLevel::Junior);
        assert_eq!(AgentLevel::from_score(59.9), AgentLevel::Junior);
        assert_eq!(AgentLevel::from_score(60.0), AgentLevel::MidLevel);
        assert_eq!(AgentLevel::from_score(79.9), AgentLevel::MidLevel);
        assert_eq!(AgentLevel::from_score(80.0), AgentLevel::Senior);
        assert_eq!(AgentLevel::from_score(100.0), AgentLevel::Senior);
    }

    #[test]
    fn report_is_certified_after_eval() {
        let s = suite();
        let results = perfect_results(&s);
        let report = evaluator().evaluate(&results, &s, "s1", 0);
        assert!(report.is_certified());
    }

    #[test]
    fn badge_colors_are_hex() {
        assert!(AgentLevel::Junior.badge_color().starts_with('#'));
        assert!(AgentLevel::MidLevel.badge_color().starts_with('#'));
        assert!(AgentLevel::Senior.badge_color().starts_with('#'));
    }
}
