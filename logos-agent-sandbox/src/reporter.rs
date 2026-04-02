//! Test reporter — produce structured pass/fail reports with failure reasons.
//!
//! `SandboxReport` aggregates all `SandboxTestResult`s from a run and can
//! export them as JSON or Markdown.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Test status ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TestStatus {
    Pass,
    Fail,
    Skipped,
    Error,
}

impl TestStatus {
    pub fn is_pass(&self) -> bool { matches!(self, Self::Pass) }
    pub fn label(&self) -> &str {
        match self {
            Self::Pass    => "PASS",
            Self::Fail    => "FAIL",
            Self::Skipped => "SKIP",
            Self::Error   => "ERROR",
        }
    }
}

// ── Failure reason ────────────────────────────────────────────────────────────

/// Structured description of why a test failed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureReason {
    pub code: String,
    pub message: String,
    /// Relevant diff snippet or actual vs expected description.
    pub detail: Option<String>,
}

impl FailureReason {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self { code: code.into(), message: message.into(), detail: None }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

// ── Sandbox test result ───────────────────────────────────────────────────────

/// Result for a single test case in a sandbox run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxTestResult {
    pub test_id: String,
    pub name: String,
    pub status: TestStatus,
    pub duration_ms: u64,
    pub failure: Option<FailureReason>,
    /// Arbitrary key-value annotations (e.g. "category" → "layout").
    pub tags: HashMap<String, String>,
}

impl SandboxTestResult {
    pub fn pass(id: impl Into<String>, name: impl Into<String>, duration_ms: u64) -> Self {
        Self {
            test_id: id.into(),
            name: name.into(),
            status: TestStatus::Pass,
            duration_ms,
            failure: None,
            tags: HashMap::new(),
        }
    }

    pub fn fail(
        id: impl Into<String>,
        name: impl Into<String>,
        duration_ms: u64,
        reason: FailureReason,
    ) -> Self {
        Self {
            test_id: id.into(),
            name: name.into(),
            status: TestStatus::Fail,
            duration_ms,
            failure: Some(reason),
            tags: HashMap::new(),
        }
    }

    pub fn error(id: impl Into<String>, name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            test_id: id.into(),
            name: name.into(),
            status: TestStatus::Error,
            duration_ms: 0,
            failure: Some(FailureReason::new("E_ERROR", message)),
            tags: HashMap::new(),
        }
    }

    pub fn skipped(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            test_id: id.into(),
            name: name.into(),
            status: TestStatus::Skipped,
            duration_ms: 0,
            failure: None,
            tags: HashMap::new(),
        }
    }

    pub fn with_tag(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.tags.insert(key.into(), value.into());
        self
    }
}

// ── Report format ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReportFormat {
    Json,
    Markdown,
    PlainText,
}

// ── Sandbox report ────────────────────────────────────────────────────────────

/// Aggregated report for a complete sandbox test run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxReport {
    pub report_id: String,
    pub agent_id: String,
    pub results: Vec<SandboxTestResult>,
    pub created_ts: u64,
}

impl SandboxReport {
    pub fn new(report_id: impl Into<String>, agent_id: impl Into<String>) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            report_id: report_id.into(),
            agent_id: agent_id.into(),
            results: Vec::new(),
            created_ts: ts,
        }
    }

    pub fn add_result(&mut self, result: SandboxTestResult) {
        self.results.push(result);
    }

    // ── Aggregates ────────────────────────────────────────────────────────────

    pub fn total_count(&self) -> usize { self.results.len() }

    pub fn pass_count(&self) -> usize {
        self.results.iter().filter(|r| r.status.is_pass()).count()
    }

    pub fn fail_count(&self) -> usize {
        self.results.iter().filter(|r| r.status == TestStatus::Fail).count()
    }

    pub fn error_count(&self) -> usize {
        self.results.iter().filter(|r| r.status == TestStatus::Error).count()
    }

    pub fn skip_count(&self) -> usize {
        self.results.iter().filter(|r| r.status == TestStatus::Skipped).count()
    }

    pub fn pass_rate(&self) -> f32 {
        let actionable = self.pass_count() + self.fail_count() + self.error_count();
        if actionable == 0 { return 100.0; }
        self.pass_count() as f32 / actionable as f32 * 100.0
    }

    pub fn total_duration_ms(&self) -> u64 {
        self.results.iter().map(|r| r.duration_ms).sum()
    }

    pub fn all_passed(&self) -> bool {
        self.results.iter().all(|r| r.status.is_pass())
    }

    pub fn failures(&self) -> Vec<&SandboxTestResult> {
        self.results.iter().filter(|r| !r.status.is_pass()).collect()
    }

    pub fn results_by_tag(&self, key: &str, value: &str) -> Vec<&SandboxTestResult> {
        self.results
            .iter()
            .filter(|r| r.tags.get(key).map(|v| v == value).unwrap_or(false))
            .collect()
    }

    // ── Serialisation ─────────────────────────────────────────────────────────

    /// Export the report to JSON.
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    /// Export the report as a Markdown summary table.
    pub fn to_markdown(&self) -> String {
        let mut md = format!(
            "# Sandbox Report: {}\n\nAgent: `{}`  \n\n",
            self.report_id, self.agent_id
        );
        md.push_str(&format!(
            "**Pass:** {}  **Fail:** {}  **Skip:** {}  **Error:** {}  \
             **Pass rate:** {:.1}%  **Duration:** {}ms\n\n",
            self.pass_count(),
            self.fail_count(),
            self.skip_count(),
            self.error_count(),
            self.pass_rate(),
            self.total_duration_ms(),
        ));
        md.push_str("| # | Name | Status | Duration | Failure |\n");
        md.push_str("|---|------|--------|----------|---------|\n");
        for r in &self.results {
            let failure_msg = r
                .failure
                .as_ref()
                .map(|f| f.message.replace('|', "\\|"))
                .unwrap_or_default();
            md.push_str(&format!(
                "| {} | {} | {} | {}ms | {} |\n",
                r.test_id, r.name, r.status.label(), r.duration_ms, failure_msg
            ));
        }
        md
    }

    /// Export as plain text (for CLI output).
    pub fn to_plain_text(&self) -> String {
        let mut out = format!(
            "Sandbox Report [{}] — Agent: {}\n",
            self.report_id, self.agent_id
        );
        out.push_str(&format!(
            "  PASS: {}  FAIL: {}  SKIP: {}  ERROR: {}  ({:.1}%)  {}ms total\n",
            self.pass_count(),
            self.fail_count(),
            self.skip_count(),
            self.error_count(),
            self.pass_rate(),
            self.total_duration_ms(),
        ));
        for r in &self.results {
            out.push_str(&format!("  [{}] {} — {}ms\n", r.status.label(), r.name, r.duration_ms));
            if let Some(f) = &r.failure {
                out.push_str(&format!("       → {}: {}\n", f.code, f.message));
            }
        }
        out
    }

    pub fn render(&self, format: ReportFormat) -> String {
        match format {
            ReportFormat::Json      => self.to_json().unwrap_or_else(|e| e.to_string()),
            ReportFormat::Markdown  => self.to_markdown(),
            ReportFormat::PlainText => self.to_plain_text(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_report() -> SandboxReport {
        let mut r = SandboxReport::new("rpt-1", "agent-demo");
        r.add_result(SandboxTestResult::pass("t1", "sandbox_init", 5));
        r.add_result(SandboxTestResult::pass("t2", "click_test", 3));
        r.add_result(SandboxTestResult::fail(
            "t3",
            "contrast_check",
            8,
            FailureReason::new("E_CONTRAST", "Contrast 3.1:1 fails WCAG AA")
                .with_detail("Expected ≥ 4.5:1"),
        ));
        r.add_result(SandboxTestResult::skipped("t4", "mobile_test"));
        r
    }

    // ── TestStatus ────────────────────────────────────────────────────────────

    #[test]
    fn test_status_labels() {
        assert_eq!(TestStatus::Pass.label(), "PASS");
        assert_eq!(TestStatus::Fail.label(), "FAIL");
        assert_eq!(TestStatus::Skipped.label(), "SKIP");
        assert_eq!(TestStatus::Error.label(), "ERROR");
    }

    #[test]
    fn test_status_is_pass() {
        assert!(TestStatus::Pass.is_pass());
        assert!(!TestStatus::Fail.is_pass());
    }

    // ── FailureReason ─────────────────────────────────────────────────────────

    #[test]
    fn failure_reason_with_detail() {
        let f = FailureReason::new("E_OPACITY", "Opacity out of range").with_detail("Got 1.5");
        assert_eq!(f.code, "E_OPACITY");
        assert!(f.detail.is_some());
    }

    // ── SandboxTestResult ─────────────────────────────────────────────────────

    #[test]
    fn sandbox_test_result_pass() {
        let r = SandboxTestResult::pass("t1", "init", 10);
        assert_eq!(r.status, TestStatus::Pass);
        assert!(r.failure.is_none());
    }

    #[test]
    fn sandbox_test_result_fail_has_reason() {
        let r = SandboxTestResult::fail("t2", "contrast", 5, FailureReason::new("E", "msg"));
        assert_eq!(r.status, TestStatus::Fail);
        assert!(r.failure.is_some());
    }

    #[test]
    fn sandbox_test_result_with_tag() {
        let r = SandboxTestResult::pass("t1", "n", 0).with_tag("category", "layout");
        assert_eq!(r.tags.get("category").map(|s| s.as_str()), Some("layout"));
    }

    // ── SandboxReport ─────────────────────────────────────────────────────────

    #[test]
    fn report_counts() {
        let r = make_report();
        assert_eq!(r.pass_count(), 2);
        assert_eq!(r.fail_count(), 1);
        assert_eq!(r.skip_count(), 1);
        assert_eq!(r.total_count(), 4);
    }

    #[test]
    fn report_pass_rate() {
        let r = make_report();
        // actionable = 2 pass + 1 fail = 3; rate = 2/3 * 100 ≈ 66.7
        assert!((r.pass_rate() - 66.666).abs() < 0.1);
    }

    #[test]
    fn report_total_duration() {
        let r = make_report();
        assert_eq!(r.total_duration_ms(), 5 + 3 + 8 + 0);
    }

    #[test]
    fn report_all_passed_false_when_failures_exist() {
        assert!(!make_report().all_passed());
    }

    #[test]
    fn report_all_passed_true_when_empty() {
        let r = SandboxReport::new("e", "a");
        assert!(r.all_passed()); // vacuously true
    }

    #[test]
    fn report_failures_returns_non_passing() {
        let r = make_report();
        let failures = r.failures();
        assert_eq!(failures.len(), 2); // 1 fail + 1 skip
    }

    #[test]
    fn report_to_json_contains_report_id() {
        let json = make_report().to_json().unwrap();
        assert!(json.contains("rpt-1"));
        assert!(json.contains("agent-demo"));
    }

    #[test]
    fn report_to_markdown_contains_header() {
        let md = make_report().to_markdown();
        assert!(md.contains("# Sandbox Report"));
        assert!(md.contains("FAIL"));
        assert!(md.contains("PASS"));
    }

    #[test]
    fn report_to_plain_text() {
        let text = make_report().to_plain_text();
        assert!(text.contains("Sandbox Report"));
        assert!(text.contains("E_CONTRAST"));
    }

    #[test]
    fn report_render_dispatches() {
        let r = make_report();
        assert!(r.render(ReportFormat::Json).contains("{"));
        assert!(r.render(ReportFormat::Markdown).contains("#"));
        assert!(r.render(ReportFormat::PlainText).contains("Sandbox Report"));
    }

    #[test]
    fn report_results_by_tag() {
        let mut r = SandboxReport::new("r", "a");
        r.add_result(SandboxTestResult::pass("t1", "a11y test", 1).with_tag("category", "a11y"));
        r.add_result(SandboxTestResult::pass("t2", "layout test", 1).with_tag("category", "layout"));
        r.add_result(SandboxTestResult::pass("t3", "a11y test 2", 1).with_tag("category", "a11y"));
        assert_eq!(r.results_by_tag("category", "a11y").len(), 2);
    }
}
