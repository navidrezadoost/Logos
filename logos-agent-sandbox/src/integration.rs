//! CLI integration and marketplace pre-publish gate.
//!
//! `SandboxCliRunner` orchestrates all sandbox modules in a single run.
//! `MarketplaceGate` uses the run output to approve or block a plugin/agent
//! before it can be published to the Logos marketplace.

use crate::{
    profiler::{PerformanceProfiler, RunMetrics},
    reporter::{FailureReason, ReportFormat, SandboxReport, SandboxTestResult},
    sandbox::{ResourceLimits, SandboxEnv},
    simulator::InteractionSimulator,
};
use std::collections::HashMap;

// ── Publish decision ──────────────────────────────────────────────────────────

/// Decision returned by `MarketplaceGate`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublishDecision {
    /// All checks passed — artifact can be published.
    Approved,
    /// One or more checks failed — publishing blocked.
    Blocked { reasons: Vec<String> },
}

impl PublishDecision {
    pub fn is_approved(&self) -> bool { matches!(self, Self::Approved) }

    pub fn reasons(&self) -> Vec<&str> {
        match self {
            Self::Approved => vec![],
            Self::Blocked { reasons } => reasons.iter().map(|s| s.as_str()).collect(),
        }
    }
}

// ── Gate config ───────────────────────────────────────────────────────────────

/// Minimum bar required to publish to the Logos marketplace.
#[derive(Debug, Clone)]
pub struct GateConfig {
    /// Minimum pass-rate percentage (0–100).
    pub min_pass_rate: f32,
    /// Peak memory allowed (bytes; 0 = unlimited).
    pub max_memory_bytes: usize,
    /// Maximum elapsed time allowed (ms; 0 = unlimited).
    pub max_elapsed_ms: u64,
}

impl Default for GateConfig {
    fn default() -> Self {
        Self {
            min_pass_rate: 80.0,
            max_memory_bytes: 256 * 1024 * 1024, // 256 MB
            max_elapsed_ms: 5_000,
        }
    }
}

// ── Marketplace gate ──────────────────────────────────────────────────────────

/// Evaluates a sandbox report and profiler metrics against the marketplace bar.
pub struct MarketplaceGate {
    config: GateConfig,
}

impl MarketplaceGate {
    pub fn new(config: GateConfig) -> Self { Self { config } }

    pub fn evaluate(&self, report: &SandboxReport, metrics: Option<&RunMetrics>) -> PublishDecision {
        let mut reasons = Vec::new();

        // 1. Pass-rate check.
        if report.pass_rate() < self.config.min_pass_rate {
            reasons.push(format!(
                "Pass rate {:.1}% is below minimum {:.1}%",
                report.pass_rate(),
                self.config.min_pass_rate,
            ));
        }

        // 2. Performance checks (only when profiler metrics provided).
        if let Some(m) = metrics {
            if self.config.max_memory_bytes > 0 {
                let peak = m.peak_memory_bytes();
                if peak > self.config.max_memory_bytes {
                    reasons.push(format!(
                        "Peak memory {}B exceeds limit {}B",
                        peak, self.config.max_memory_bytes,
                    ));
                }
            }
            if self.config.max_elapsed_ms > 0 && m.elapsed_ms > self.config.max_elapsed_ms {
                reasons.push(format!(
                    "Elapsed {}ms exceeds limit {}ms",
                    m.elapsed_ms, self.config.max_elapsed_ms,
                ));
            }
        }

        if reasons.is_empty() {
            PublishDecision::Approved
        } else {
            PublishDecision::Blocked { reasons }
        }
    }
}

impl Default for MarketplaceGate {
    fn default() -> Self { Self::new(GateConfig::default()) }
}

// ── Run config ────────────────────────────────────────────────────────────────

/// Configuration for a `SandboxCliRunner` run.
#[derive(Debug, Clone)]
pub struct SandboxRunConfig {
    pub agent_id: String,
    pub plugin_path: Option<String>,
    pub report_format: ReportFormat,
    pub apply_gate: bool,
    pub resource_limits: ResourceLimits,
    /// Extra key-value options.
    pub extra_options: HashMap<String, String>,
}

impl SandboxRunConfig {
    pub fn new(agent_id: impl Into<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
            plugin_path: None,
            report_format: ReportFormat::PlainText,
            apply_gate: true,
            resource_limits: ResourceLimits::default(),
            extra_options: HashMap::new(),
        }
    }

    pub fn with_report_format(mut self, f: ReportFormat) -> Self {
        self.report_format = f;
        self
    }

    pub fn with_plugin_path(mut self, p: impl Into<String>) -> Self {
        self.plugin_path = Some(p.into());
        self
    }

    pub fn no_gate(mut self) -> Self {
        self.apply_gate = false;
        self
    }

    pub fn opt(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra_options.insert(key.into(), value.into());
        self
    }
}

// ── Run result ────────────────────────────────────────────────────────────────

/// Combined output from a `SandboxCliRunner` run.
#[derive(Debug)]
pub struct CliRunResult {
    pub report: SandboxReport,
    pub metrics: RunMetrics,
    pub decision: Option<PublishDecision>,
    pub rendered: String,
}

// ── SandboxCliRunner ──────────────────────────────────────────────────────────

/// Orchestrates a complete sandbox test run and optional gate evaluation.
pub struct SandboxCliRunner {
    gate_config: GateConfig,
}

impl SandboxCliRunner {
    pub fn new(gate_config: GateConfig) -> Self { Self { gate_config } }

    /// Execute a full sandbox run based on the given config.
    pub fn run(&self, cfg: SandboxRunConfig) -> CliRunResult {
        let mut sandbox = SandboxEnv::with_limits(&cfg.agent_id, cfg.resource_limits.clone());
        let _simulator  = InteractionSimulator::new();
        let mut profiler = PerformanceProfiler::new(format!("run-{}", cfg.agent_id));

        profiler.start();

        let mut report = SandboxReport::new(
            format!("run-{}", cfg.agent_id),
            &cfg.agent_id,
        );

        // ── Built-in gate test cases ──────────────────────────────────────────

        // 1. Network isolation check
        let t_start = std::time::Instant::now();
        let isolated = cfg.resource_limits.network_blocked;
        report.add_result(if isolated {
            SandboxTestResult::pass("s01", "network_isolation", t_start.elapsed().as_millis() as u64)
        } else {
            SandboxTestResult::fail(
                "s01", "network_isolation", t_start.elapsed().as_millis() as u64,
                FailureReason::new("E_ISOLATION", "Sandbox network is not blocked"),
            )
        });

        // 2. Canvas state reset
        let t_start = std::time::Instant::now();
        sandbox.reset();
        report.add_result(SandboxTestResult::pass(
            "s02", "canvas_reset", t_start.elapsed().as_millis() as u64,
        ));

        // 3. Plugin path check
        let t_start = std::time::Instant::now();
        if let Some(path) = &cfg.plugin_path {
            let valid = path.starts_with('/');
            report.add_result(if valid {
                SandboxTestResult::pass("s03", "plugin_path_valid", t_start.elapsed().as_millis() as u64)
            } else {
                SandboxTestResult::fail(
                    "s03", "plugin_path_valid", t_start.elapsed().as_millis() as u64,
                    FailureReason::new("E_PATH", format!("Plugin path '{}' is not absolute", path)),
                )
            });
        } else {
            report.add_result(SandboxTestResult::skipped("s03", "plugin_path_valid"));
        }

        profiler.snapshot_memory("post-run", 1024 * 512);
        profiler.stop();

        let metrics = profiler.metrics();

        let decision = if cfg.apply_gate {
            Some(MarketplaceGate::new(self.gate_config.clone()).evaluate(&report, Some(&metrics)))
        } else {
            None
        };

        let rendered = report.render(cfg.report_format);

        CliRunResult { report, metrics, decision, rendered }
    }
}

impl Default for SandboxCliRunner {
    fn default() -> Self { Self::new(GateConfig::default()) }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reporter::{SandboxReport, SandboxTestResult};

    // ── PublishDecision ───────────────────────────────────────────────────────

    #[test]
    fn publish_decision_approved_is_approved() {
        assert!(PublishDecision::Approved.is_approved());
    }

    #[test]
    fn publish_decision_blocked_not_approved() {
        let d = PublishDecision::Blocked { reasons: vec!["low pass rate".into()] };
        assert!(!d.is_approved());
        assert_eq!(d.reasons().len(), 1);
    }

    // ── MarketplaceGate ───────────────────────────────────────────────────────

    #[test]
    fn gate_approves_perfect_report() {
        let mut r = SandboxReport::new("r", "agent-a");
        r.add_result(SandboxTestResult::pass("t1", "test", 5));
        r.add_result(SandboxTestResult::pass("t2", "test", 3));
        let gate = MarketplaceGate::default();
        assert!(gate.evaluate(&r, None).is_approved());
    }

    #[test]
    fn gate_blocks_low_pass_rate() {
        let mut r = SandboxReport::new("r", "agent-b");
        r.add_result(SandboxTestResult::pass("t1", "t", 5));
        r.add_result(SandboxTestResult::fail("t2", "t", 3, FailureReason::new("E", "m")));
        r.add_result(SandboxTestResult::fail("t3", "t", 3, FailureReason::new("E", "m")));
        let gate = MarketplaceGate::default(); // requires 80%
        assert!(!gate.evaluate(&r, None).is_approved());
    }

    // ── SandboxRunConfig ──────────────────────────────────────────────────────

    #[test]
    fn run_config_builder() {
        let cfg = SandboxRunConfig::new("agent-x")
            .with_report_format(ReportFormat::Json)
            .with_plugin_path("/plugins/my-plugin.wasm")
            .opt("timeout", "5000");
        assert_eq!(cfg.agent_id, "agent-x");
        assert!(cfg.plugin_path.is_some());
        assert_eq!(cfg.extra_options.get("timeout").map(|s| s.as_str()), Some("5000"));
    }

    // ── SandboxCliRunner ──────────────────────────────────────────────────────

    #[test]
    fn cli_run_produces_report() {
        let runner = SandboxCliRunner::default();
        let cfg = SandboxRunConfig::new("agent-demo").no_gate();
        let result = runner.run(cfg);
        assert!(result.report.total_count() >= 2);
        assert!(result.decision.is_none());
    }

    #[test]
    fn cli_run_with_valid_plugin_path_passes() {
        let runner = SandboxCliRunner::default();
        let cfg = SandboxRunConfig::new("agent-demo")
            .with_plugin_path("/plugins/valid.wasm")
            .no_gate();
        let result = runner.run(cfg);
        let plugin_test = result.report.results.iter().find(|r| r.test_id == "s03");
        assert!(plugin_test.map(|r| r.status.is_pass()).unwrap_or(false));
    }

    #[test]
    fn cli_run_with_gate_produces_decision() {
        let runner = SandboxCliRunner::default();
        let cfg = SandboxRunConfig::new("agent-gated");
        let result = runner.run(cfg);
        assert!(result.decision.is_some());
    }

    #[test]
    fn cli_run_renders_output() {
        let runner = SandboxCliRunner::default();
        let result = runner.run(SandboxRunConfig::new("agent-r"));
        assert!(!result.rendered.is_empty());
    }
}
