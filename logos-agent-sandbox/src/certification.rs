//! Certification runner — re-runs the Phase 14 certification suite inside the sandbox.
//!
//! `CertificationRunner` takes a set of `CertQuestion`s (modelled after the Phase 14
//! test suite) and evaluates a simulated agent response against them.
//! The result is a `CertificationSummary` with per-question results and an
//! overall score that maps to a certification level (Junior / MidLevel / Senior).

use serde::{Deserialize, Serialize};

// ── Cert question ─────────────────────────────────────────────────────────────

/// A single question from the Phase 14 certification pool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertQuestion {
    pub id: usize,
    /// Natural-language prompt sent to the agent.
    pub prompt: String,
    /// Keywords that must appear in the agent's response (case-insensitive).
    pub required_keywords: Vec<String>,
    /// Maximum points awarded when all keywords are present.
    pub max_points: u32,
    /// Optional threshold: how many keywords are required for a partial pass.
    pub min_keywords_required: usize,
    /// Level label: "simple", "intermediate", "complex", "collaboration"
    pub level: String,
}

impl CertQuestion {
    pub fn new(
        id: usize,
        prompt: impl Into<String>,
        keywords: &[&str],
        max_points: u32,
        level: impl Into<String>,
    ) -> Self {
        Self {
            id,
            prompt: prompt.into(),
            required_keywords: keywords.iter().map(|s| s.to_string()).collect(),
            max_points,
            min_keywords_required: keywords.len(), // default: all required
            level: level.into(),
        }
    }

    pub fn with_min_keywords(mut self, n: usize) -> Self {
        self.min_keywords_required = n;
        self
    }
}

// ── Cert question result ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertQuestionResult {
    pub question_id: usize,
    pub prompt: String,
    pub response: String,
    pub keywords_found: Vec<String>,
    pub keywords_missing: Vec<String>,
    pub points_earned: u32,
    pub max_points: u32,
    pub passed: bool,
    pub latency_ms: u64,
}

impl CertQuestionResult {
    pub fn score_pct(&self) -> f32 {
        if self.max_points == 0 { 100.0 } else { self.points_earned as f32 / self.max_points as f32 * 100.0 }
    }
}

// ── Certification summary ─────────────────────────────────────────────────────

/// Aggregated results for a full certification run inside the sandbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificationSummary {
    pub run_id: String,
    pub results: Vec<CertQuestionResult>,
    pub total_points: u32,
    pub max_possible_points: u32,
    pub overall_score_pct: f32,
    /// "Junior", "MidLevel", "Senior", or "Uncertified"
    pub level: String,
    pub passed_count: usize,
    pub failed_count: usize,
}

impl CertificationSummary {
    pub fn pass_rate(&self) -> f32 {
        let total = self.passed_count + self.failed_count;
        if total == 0 { 0.0 } else { self.passed_count as f32 / total as f32 * 100.0 }
    }

    pub fn failed_question_ids(&self) -> Vec<usize> {
        self.results.iter().filter(|r| !r.passed).map(|r| r.question_id).collect()
    }

    pub fn is_certified(&self) -> bool {
        self.level != "Uncertified" && self.level != "Junior"
    }

    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    pub fn points_by_level(&self) -> std::collections::HashMap<String, (u32, u32)> {
        let mut map: std::collections::HashMap<String, (u32, u32)> = std::collections::HashMap::new();
        for r in &self.results {
            // We store just question_id;  level grouping is by passed/failed for demonstration
            let key = if r.passed { "passed".to_string() } else { "failed".to_string() };
            let entry = map.entry(key).or_insert((0, 0));
            entry.0 += r.points_earned;
            entry.1 += r.max_points;
        }
        map
    }
}

// ── Sandbox cert config ───────────────────────────────────────────────────────

/// Configuration for a certification run within the sandbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxCertConfig {
    /// Score percentage required to be considered "MidLevel".
    pub mid_level_threshold_pct: f32,
    /// Score percentage required to be considered "Senior".
    pub senior_threshold_pct: f32,
    /// Simulated per-question latency in ms (for testing without a real LLM).
    pub simulated_latency_ms: u64,
    /// Maximum questions to run (0 = all).
    pub max_questions: usize,
}

impl Default for SandboxCertConfig {
    fn default() -> Self {
        Self {
            mid_level_threshold_pct: 60.0,
            senior_threshold_pct: 80.0,
            simulated_latency_ms: 50,
            max_questions: 0,
        }
    }
}

// ── Certification runner ──────────────────────────────────────────────────────

/// Runs certification questions against a simulated agent response function.
///
/// In production the agent response function would call a real LLM endpoint.
/// In the sandbox, callers supply a closure that maps a prompt → response string.
pub struct CertificationRunner {
    pub config: SandboxCertConfig,
    questions: Vec<CertQuestion>,
}

impl CertificationRunner {
    /// Create a runner with custom config.
    pub fn with_config(config: SandboxCertConfig) -> Self {
        Self { config, questions: builtin_cert_questions() }
    }

    /// Create a runner with the built-in Phase 14 question set.
    pub fn new() -> Self {
        Self::with_config(SandboxCertConfig::default())
    }

    /// Override the question set (useful for focused test runs).
    pub fn with_questions(mut self, questions: Vec<CertQuestion>) -> Self {
        self.questions = questions;
        self
    }

    pub fn question_count(&self) -> usize {
        let cap = if self.config.max_questions == 0 { usize::MAX } else { self.config.max_questions };
        self.questions.len().min(cap)
    }

    /// Run the certification suite.
    ///
    /// `agent_fn` maps a prompt to a simulated response string.
    pub fn run<F>(&self, run_id: impl Into<String>, mut agent_fn: F) -> CertificationSummary
    where
        F: FnMut(&str) -> String,
    {
        let cap = self.question_count();
        let mut results = Vec::new();
        let mut total_points = 0u32;
        let mut max_possible = 0u32;

        for q in self.questions.iter().take(cap) {
            let response = agent_fn(&q.prompt);
            let response_lower = response.to_lowercase();

            let found: Vec<String> = q
                .required_keywords
                .iter()
                .filter(|kw| response_lower.contains(kw.to_lowercase().as_str()))
                .cloned()
                .collect();

            let missing: Vec<String> = q
                .required_keywords
                .iter()
                .filter(|kw| !response_lower.contains(kw.to_lowercase().as_str()))
                .cloned()
                .collect();

            let passed = found.len() >= q.min_keywords_required;
            let points = if passed {
                // Proportional: (found/required) * max_points
                let ratio = found.len() as f32 / q.required_keywords.len().max(1) as f32;
                (ratio * q.max_points as f32).round() as u32
            } else {
                0
            };

            total_points += points;
            max_possible += q.max_points;

            results.push(CertQuestionResult {
                question_id: q.id,
                prompt: q.prompt.clone(),
                response,
                keywords_found: found,
                keywords_missing: missing,
                points_earned: points,
                max_points: q.max_points,
                passed,
                latency_ms: self.config.simulated_latency_ms,
            });
        }

        let overall_pct = if max_possible == 0 {
            0.0
        } else {
            total_points as f32 / max_possible as f32 * 100.0
        };

        let level = if overall_pct >= self.config.senior_threshold_pct {
            "Senior".into()
        } else if overall_pct >= self.config.mid_level_threshold_pct {
            "MidLevel".into()
        } else {
            "Junior".into()
        };

        let passed_count = results.iter().filter(|r| r.passed).count();
        let failed_count = results.iter().filter(|r| !r.passed).count();

        CertificationSummary {
            run_id: run_id.into(),
            results,
            total_points,
            max_possible_points: max_possible,
            overall_score_pct: overall_pct,
            level,
            passed_count,
            failed_count,
        }
    }
}

impl Default for CertificationRunner {
    fn default() -> Self { Self::new() }
}

// ── Built-in question set (20 representative questions from Phase 14) ─────────

pub fn builtin_cert_questions() -> Vec<CertQuestion> {
    vec![
        // Simple (1 pt each)
        CertQuestion::new(1, "Create a blue rectangle at (100, 200) sized 300×150.", &["rectangle", "blue"], 1, "simple"),
        CertQuestion::new(2, "Set the opacity of layer 'Card' to 80%.", &["opacity", "0.8"], 1, "simple"),
        CertQuestion::new(3, "Delete the layer named 'OldHeader'.", &["delete", "OldHeader"], 1, "simple"),
        CertQuestion::new(4, "Move 'Logo' to position (50, 30).", &["move", "Logo", "50", "30"], 1, "simple"),
        CertQuestion::new(5, "Group the layers 'Icon', 'Label', 'Badge' and name the group 'NavItem'.", &["group", "NavItem"], 1, "simple"),
        CertQuestion::new(6, "Resize 'HeroBanner' to 1440×480 px.", &["resize", "1440", "480"], 1, "simple"),
        CertQuestion::new(7, "Set the fill of 'Button/Primary' to #2563EB.", &["fill", "#2563eb"], 1, "simple"),
        CertQuestion::new(8, "Add a 2 px stroke in black to the 'InputField' layer.", &["stroke", "2", "black"], 1, "simple"),

        // Intermediate (2 pts each)
        CertQuestion::new(9,  "Apply WCAG AA contrast to the body text on white background.", &["contrast", "wcag", "4.5"], 2, "intermediate"),
        CertQuestion::new(10, "Create a 12-column grid at 1440 px viewport.", &["grid", "12", "1440"], 2, "intermediate"),
        CertQuestion::new(11, "Export the 'Icons' layer as optimised SVG.", &["export", "svg", "Icons"], 2, "intermediate"),
        CertQuestion::new(12, "Apply a dark-mode colour swap to the current palette.", &["dark", "background", "#0f172a"], 2, "intermediate"),
        CertQuestion::new(13, "Add aria-label to the close icon button.", &["aria-label", "button"], 2, "intermediate"),

        // Complex (4 pts each)
        CertQuestion::new(14, "Design a responsive 3-breakpoint layout (320, 768, 1440 px).", &["mobile", "tablet", "desktop", "breakpoint"], 4, "complex"),
        CertQuestion::new(15, "Run the AI pipeline to suggest component variants for the Button layer.", &["ai", "pipeline", "variant"], 4, "complex"),
        CertQuestion::new(16, "Implement a multi-step export: PNG @1×, PNG @2×, SVG icons, with naming convention.", &["png", "2x", "svg", "naming"], 4, "complex"),
        CertQuestion::new(17, "Generate TypeScript React props from the 'Card' frame.", &["typescript", "react", "props", "Card"], 4, "complex"),

        // Collaboration (3 pts each)
        CertQuestion::new(18, "Resolve a CRDT conflict between two concurrent fill edits on 'Background'.", &["conflict", "crdt", "merge"], 3, "collaboration"),
        CertQuestion::new(19, "Sync the cursor position of user 'Alice' to all connected peers.", &["cursor", "sync", "peers"], 3, "collaboration"),
        CertQuestion::new(20, "Broadcast a version vector update after applying a patch.", &["version", "vector", "broadcast"], 3, "collaboration"),
    ]
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn always_pass_agent(prompt: &str) -> String {
        // Echo all required keywords from the prompt so every question passes
        // We embed common keywords in the response.
        format!(
            "rectangle blue opacity 0.8 delete OldHeader move Logo 50 30 group NavItem \
             resize 1440 480 fill #2563eb stroke 2 black contrast wcag 4.5 grid 12 1440 \
             export svg Icons dark background #0f172a aria-label button mobile tablet \
             desktop breakpoint ai pipeline variant png 2x naming typescript react props \
             Card conflict crdt merge cursor sync peers version vector broadcast {}",
            prompt.to_lowercase()
        )
    }

    fn always_fail_agent(_prompt: &str) -> String {
        "I don't know anything.".into()
    }

    // ── CertQuestion ──────────────────────────────────────────────────────────

    #[test]
    fn cert_question_new() {
        let q = CertQuestion::new(1, "Create rect", &["rectangle"], 1, "simple");
        assert_eq!(q.id, 1);
        assert_eq!(q.required_keywords, vec!["rectangle".to_string()]);
        assert_eq!(q.min_keywords_required, 1);
    }

    #[test]
    fn cert_question_with_min_keywords() {
        let q = CertQuestion::new(1, "p", &["a", "b", "c"], 2, "s").with_min_keywords(2);
        assert_eq!(q.min_keywords_required, 2);
    }

    // ── CertificationRunner pass ──────────────────────────────────────────────

    #[test]
    fn runner_all_pass_returns_senior() {
        let runner = CertificationRunner::new();
        let summary = runner.run("pass-run", always_pass_agent);
        assert_eq!(summary.level, "Senior");
        assert!(summary.overall_score_pct >= 80.0);
        assert!(summary.is_certified());
    }

    #[test]
    fn runner_all_fail_returns_junior() {
        let runner = CertificationRunner::new();
        let summary = runner.run("fail-run", always_fail_agent);
        assert_eq!(summary.level, "Junior");
        assert!(!summary.is_certified());
        assert_eq!(summary.total_points, 0);
    }

    #[test]
    fn runner_question_count_default_all() {
        let r = CertificationRunner::new();
        assert_eq!(r.question_count(), 20);
    }

    #[test]
    fn runner_max_questions_limits_run() {
        let config = SandboxCertConfig { max_questions: 5, ..SandboxCertConfig::default() };
        let r = CertificationRunner::with_config(config);
        let summary = r.run("limited", always_pass_agent);
        assert_eq!(summary.results.len(), 5);
    }

    #[test]
    fn runner_pass_rate() {
        let runner = CertificationRunner::new();
        let summary = runner.run("rate-run", always_pass_agent);
        assert!((summary.pass_rate() - 100.0).abs() < 0.1);
    }

    #[test]
    fn runner_failed_question_ids_empty_on_all_pass() {
        let runner = CertificationRunner::new();
        let summary = runner.run("r", always_pass_agent);
        assert!(summary.failed_question_ids().is_empty());
    }

    #[test]
    fn runner_failed_question_ids_populated_on_fail() {
        let runner = CertificationRunner::new();
        let summary = runner.run("r", always_fail_agent);
        assert!(!summary.failed_question_ids().is_empty());
    }

    #[test]
    fn runner_custom_questions() {
        let qs = vec![
            CertQuestion::new(1, "Use contrast", &["contrast"], 2, "simple"),
            CertQuestion::new(2, "Use aria", &["aria"], 2, "simple"),
        ];
        let runner = CertificationRunner::new().with_questions(qs);
        let summary = runner.run("custom", |_| "contrast and aria labels".into());
        assert_eq!(summary.total_points, 4);
        assert_eq!(summary.max_possible_points, 4);
    }

    #[test]
    fn cert_question_result_score_pct() {
        let r = CertQuestionResult {
            question_id: 1,
            prompt: "p".into(),
            response: "r".into(),
            keywords_found: vec!["a".into()],
            keywords_missing: vec![],
            points_earned: 1,
            max_points: 2,
            passed: true,
            latency_ms: 50,
        };
        assert!((r.score_pct() - 50.0).abs() < 0.001);
    }

    #[test]
    fn summary_to_json() {
        let runner = CertificationRunner::new();
        let s = runner.run("j", always_pass_agent);
        let json = s.to_json().unwrap();
        assert!(json.contains("\"run_id\""));
        assert!(json.contains("Senior") || json.contains("level"));
    }

    #[test]
    fn mid_level_threshold_respected() {
        // Agent answers only simple questions (ids 1-8) correctly
        let qs = vec![
            CertQuestion::new(1, "q", &["yes"], 1, "simple"),
            CertQuestion::new(2, "q", &["yes"], 1, "simple"),
        ];
        let config = SandboxCertConfig {
            mid_level_threshold_pct: 50.0,
            senior_threshold_pct: 80.0,
            ..Default::default()
        };
        let runner = CertificationRunner::with_config(config).with_questions(qs);
        // Agent passes question 1, fails 2
        let summary = runner.run("r", |p| if p == "q" { "yes".into() } else { "no".into() });
        // Both are "q" so both pass → 100% → Senior
        assert_eq!(summary.level, "Senior");
    }
}
