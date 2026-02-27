//! # AI Pipeline Orchestrator
//!
//! Composes multiple AI capabilities into reusable pipelines.
//! A pipeline is a sequence of steps that transforms a design
//! context through analysis, suggestion, and refinement stages.
//!
//! ```
//! use logos_ai::inference::pipeline::{Pipeline, PipelineStep, StepKind, PipelineResult};
//!
//! let pipeline = Pipeline::new("design-review")
//!     .add_step(PipelineStep::new(StepKind::DesignAnalysis))
//!     .add_step(PipelineStep::new(StepKind::AccessibilityAudit))
//!     .add_step(PipelineStep::new(StepKind::ColorHarmony));
//!
//! assert_eq!(pipeline.steps().len(), 3);
//! ```

use std::collections::HashMap;
use std::time::{Duration, Instant};
use uuid::Uuid;

// ── Step Kinds ───────────────────────────────────────────────

/// Type of AI analysis step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StepKind {
    /// Run design suggestion analysis.
    DesignAnalysis,
    /// Run accessibility audit.
    AccessibilityAudit,
    /// Run color harmony evaluation.
    ColorHarmony,
    /// Infer smart constraints.
    SmartConstraints,
    /// Recommend components.
    ComponentRecommendation,
    /// Generate layout proposals.
    LayoutGeneration,
    /// Apply style transfer.
    StyleTransfer,
    /// Generate assets.
    AssetGeneration,
    /// Custom step (user-defined).
    Custom,
}

impl StepKind {
    /// Human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::DesignAnalysis => "Design Analysis",
            Self::AccessibilityAudit => "Accessibility Audit",
            Self::ColorHarmony => "Color Harmony",
            Self::SmartConstraints => "Smart Constraints",
            Self::ComponentRecommendation => "Component Recommendation",
            Self::LayoutGeneration => "Layout Generation",
            Self::StyleTransfer => "Style Transfer",
            Self::AssetGeneration => "Asset Generation",
            Self::Custom => "Custom",
        }
    }

    /// Estimated duration for this step type.
    pub fn estimated_duration(&self) -> Duration {
        match self {
            Self::DesignAnalysis | Self::SmartConstraints => Duration::from_millis(1),
            Self::AccessibilityAudit | Self::ColorHarmony | Self::ComponentRecommendation => {
                Duration::from_millis(5)
            }
            Self::LayoutGeneration => Duration::from_millis(50),
            Self::StyleTransfer => Duration::from_millis(16),
            Self::AssetGeneration => Duration::from_secs(2),
            Self::Custom => Duration::from_millis(10),
        }
    }

    /// Whether this step requires GPU/ML inference.
    pub fn requires_inference(&self) -> bool {
        matches!(
            self,
            Self::LayoutGeneration | Self::StyleTransfer | Self::AssetGeneration
        )
    }
}

// ── Pipeline Step ────────────────────────────────────────────

/// A single step in an AI pipeline.
#[derive(Debug, Clone)]
pub struct PipelineStep {
    /// Unique step ID.
    pub id: Uuid,
    /// What kind of analysis.
    pub kind: StepKind,
    /// Whether this step is optional (can be skipped on error).
    pub optional: bool,
    /// Configuration parameters for this step.
    pub params: HashMap<String, String>,
}

impl PipelineStep {
    /// Create a new required step.
    pub fn new(kind: StepKind) -> Self {
        Self {
            id: Uuid::new_v4(),
            kind,
            optional: false,
            params: HashMap::new(),
        }
    }

    /// Mark step as optional.
    pub fn as_optional(mut self) -> Self {
        self.optional = true;
        self
    }

    /// Add a parameter.
    pub fn with_param(mut self, key: &str, value: &str) -> Self {
        self.params.insert(key.to_string(), value.to_string());
        self
    }
}

// ── Step Result ──────────────────────────────────────────────

/// Outcome of executing a pipeline step.
#[derive(Debug, Clone)]
pub struct StepResult {
    /// Which step produced this result.
    pub step_id: Uuid,
    /// Which kind of step.
    pub kind: StepKind,
    /// Whether the step succeeded.
    pub success: bool,
    /// How long the step took.
    pub duration: Duration,
    /// Output messages/findings.
    pub findings: Vec<String>,
    /// Structured output data.
    pub data: HashMap<String, String>,
    /// Error message if failed.
    pub error: Option<String>,
}

impl StepResult {
    fn success(step: &PipelineStep, duration: Duration, findings: Vec<String>) -> Self {
        Self {
            step_id: step.id,
            kind: step.kind,
            success: true,
            duration,
            findings,
            data: HashMap::new(),
            error: None,
        }
    }

    fn failure(step: &PipelineStep, duration: Duration, error: String) -> Self {
        Self {
            step_id: step.id,
            kind: step.kind,
            success: false,
            duration,
            findings: Vec::new(),
            data: HashMap::new(),
            error: Some(error),
        }
    }

    /// With structured data.
    pub fn with_data(mut self, key: &str, value: &str) -> Self {
        self.data.insert(key.to_string(), value.to_string());
        self
    }
}

// ── Pipeline ─────────────────────────────────────────────────

/// An ordered sequence of AI analysis steps.
#[derive(Debug, Clone)]
pub struct Pipeline {
    /// Pipeline name.
    pub name: String,
    /// Unique ID.
    pub id: Uuid,
    /// Ordered steps.
    steps: Vec<PipelineStep>,
    /// Maximum total duration before timeout.
    pub timeout: Duration,
}

impl Pipeline {
    /// Create a new empty pipeline.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            id: Uuid::new_v4(),
            steps: Vec::new(),
            timeout: Duration::from_secs(30),
        }
    }

    /// Builder: add a step.
    pub fn add_step(mut self, step: PipelineStep) -> Self {
        self.steps.push(step);
        self
    }

    /// Builder: set timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Get steps.
    pub fn steps(&self) -> &[PipelineStep] {
        &self.steps
    }

    /// Total estimated duration.
    pub fn estimated_duration(&self) -> Duration {
        self.steps.iter().map(|s| s.kind.estimated_duration()).sum()
    }

    /// Number of steps requiring inference.
    pub fn inference_steps(&self) -> usize {
        self.steps.iter().filter(|s| s.kind.requires_inference()).count()
    }

    /// Number of optional steps.
    pub fn optional_steps(&self) -> usize {
        self.steps.iter().filter(|s| s.optional).count()
    }
}

// ── Pipeline Result ──────────────────────────────────────────

/// Result of running a complete pipeline.
#[derive(Debug)]
pub struct PipelineResult {
    /// Pipeline name.
    pub pipeline_name: String,
    /// Per-step results.
    pub step_results: Vec<StepResult>,
    /// Total wall-clock duration.
    pub total_duration: Duration,
    /// Whether all required steps succeeded.
    pub success: bool,
}

impl PipelineResult {
    /// Number of successful steps.
    pub fn successful_steps(&self) -> usize {
        self.step_results.iter().filter(|r| r.success).count()
    }

    /// Number of failed steps.
    pub fn failed_steps(&self) -> usize {
        self.step_results.iter().filter(|r| !r.success).count()
    }

    /// All findings across all steps.
    pub fn all_findings(&self) -> Vec<&str> {
        self.step_results
            .iter()
            .flat_map(|r| r.findings.iter().map(|s| s.as_str()))
            .collect()
    }

    /// Errors from failed steps.
    pub fn errors(&self) -> Vec<&str> {
        self.step_results
            .iter()
            .filter_map(|r| r.error.as_deref())
            .collect()
    }
}

// ── Pipeline Runner ──────────────────────────────────────────

/// Executes pipelines by simulating step execution.
///
/// In a production system, each step would dispatch to real
/// AI modules. Here we provide a framework + simulation for
/// testing and orchestration logic.
pub struct PipelineRunner {
    /// Registered step handlers (step kind → handler).
    handlers: HashMap<StepKind, Box<dyn Fn(&PipelineStep) -> Result<Vec<String>, String>>>,
}

impl PipelineRunner {
    /// Create a new runner with no handlers.
    pub fn new() -> Self {
        Self { handlers: HashMap::new() }
    }

    /// Create a runner with default simulation handlers.
    pub fn with_defaults() -> Self {
        let mut runner = Self::new();
        runner.register_default_handlers();
        runner
    }

    /// Register a handler for a step kind.
    pub fn register_handler(
        &mut self,
        kind: StepKind,
        handler: Box<dyn Fn(&PipelineStep) -> Result<Vec<String>, String>>,
    ) {
        self.handlers.insert(kind, handler);
    }

    fn register_default_handlers(&mut self) {
        self.handlers.insert(StepKind::DesignAnalysis, Box::new(|_step| {
            Ok(vec!["Analyzed design layout".to_string()])
        }));
        self.handlers.insert(StepKind::AccessibilityAudit, Box::new(|_step| {
            Ok(vec!["Accessibility audit complete".to_string()])
        }));
        self.handlers.insert(StepKind::ColorHarmony, Box::new(|_step| {
            Ok(vec!["Color harmony evaluated".to_string()])
        }));
        self.handlers.insert(StepKind::SmartConstraints, Box::new(|_step| {
            Ok(vec!["Constraints inferred".to_string()])
        }));
        self.handlers.insert(StepKind::ComponentRecommendation, Box::new(|_step| {
            Ok(vec!["Components recommended".to_string()])
        }));
        self.handlers.insert(StepKind::LayoutGeneration, Box::new(|_step| {
            Ok(vec!["Layout proposals generated".to_string()])
        }));
        self.handlers.insert(StepKind::StyleTransfer, Box::new(|_step| {
            Ok(vec!["Style transfer applied".to_string()])
        }));
        self.handlers.insert(StepKind::AssetGeneration, Box::new(|_step| {
            Ok(vec!["Assets generated".to_string()])
        }));
    }

    /// Execute a pipeline.
    pub fn run(&self, pipeline: &Pipeline) -> PipelineResult {
        let start = Instant::now();
        let mut step_results = Vec::new();
        let mut all_ok = true;

        for step in pipeline.steps() {
            let step_start = Instant::now();

            let result = if let Some(handler) = self.handlers.get(&step.kind) {
                match handler(step) {
                    Ok(findings) => StepResult::success(step, step_start.elapsed(), findings),
                    Err(e) => {
                        let sr = StepResult::failure(step, step_start.elapsed(), e);
                        if !step.optional {
                            all_ok = false;
                        }
                        sr
                    }
                }
            } else if step.kind == StepKind::Custom {
                // Custom steps without handlers just pass through
                StepResult::success(step, step_start.elapsed(), vec!["Custom step executed".into()])
            } else {
                let sr = StepResult::failure(
                    step,
                    step_start.elapsed(),
                    format!("No handler for {:?}", step.kind),
                );
                if !step.optional {
                    all_ok = false;
                }
                sr
            };

            step_results.push(result);

            // Check timeout
            if start.elapsed() > pipeline.timeout {
                all_ok = false;
                break;
            }
        }

        PipelineResult {
            pipeline_name: pipeline.name.clone(),
            step_results,
            total_duration: start.elapsed(),
            success: all_ok,
        }
    }
}

impl Default for PipelineRunner {
    fn default() -> Self {
        Self::with_defaults()
    }
}

// ── Preset Pipelines ─────────────────────────────────────────

/// Common pipeline presets.
pub struct PipelinePresets;

impl PipelinePresets {
    /// Full design review: all heuristic checks.
    pub fn design_review() -> Pipeline {
        Pipeline::new("design-review")
            .add_step(PipelineStep::new(StepKind::DesignAnalysis))
            .add_step(PipelineStep::new(StepKind::AccessibilityAudit))
            .add_step(PipelineStep::new(StepKind::ColorHarmony))
            .add_step(PipelineStep::new(StepKind::SmartConstraints))
            .add_step(PipelineStep::new(StepKind::ComponentRecommendation))
    }

    /// Quick accessibility check.
    pub fn accessibility_only() -> Pipeline {
        Pipeline::new("accessibility-only")
            .add_step(PipelineStep::new(StepKind::AccessibilityAudit))
    }

    /// AI-powered design generation.
    pub fn generative() -> Pipeline {
        Pipeline::new("generative")
            .add_step(PipelineStep::new(StepKind::LayoutGeneration))
            .add_step(PipelineStep::new(StepKind::StyleTransfer).as_optional())
            .add_step(PipelineStep::new(StepKind::AssetGeneration).as_optional())
    }
}

// ── Tests ────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Pipeline Construction ────────────────────────────────

    #[test]
    fn build_pipeline() {
        let p = Pipeline::new("test")
            .add_step(PipelineStep::new(StepKind::DesignAnalysis))
            .add_step(PipelineStep::new(StepKind::AccessibilityAudit));
        assert_eq!(p.steps().len(), 2);
        assert_eq!(p.name, "test");
    }

    #[test]
    fn estimated_duration() {
        let p = Pipeline::new("test")
            .add_step(PipelineStep::new(StepKind::DesignAnalysis))
            .add_step(PipelineStep::new(StepKind::AssetGeneration));
        assert!(p.estimated_duration() >= Duration::from_secs(2));
    }

    #[test]
    fn inference_step_count() {
        let p = Pipeline::new("test")
            .add_step(PipelineStep::new(StepKind::DesignAnalysis))
            .add_step(PipelineStep::new(StepKind::LayoutGeneration))
            .add_step(PipelineStep::new(StepKind::StyleTransfer));
        assert_eq!(p.inference_steps(), 2);
    }

    #[test]
    fn optional_step_count() {
        let p = Pipeline::new("test")
            .add_step(PipelineStep::new(StepKind::DesignAnalysis))
            .add_step(PipelineStep::new(StepKind::StyleTransfer).as_optional());
        assert_eq!(p.optional_steps(), 1);
    }

    #[test]
    fn pipeline_timeout() {
        let p = Pipeline::new("test")
            .with_timeout(Duration::from_secs(5));
        assert_eq!(p.timeout, Duration::from_secs(5));
    }

    // ── Step Properties ──────────────────────────────────────

    #[test]
    fn step_kind_labels() {
        assert_eq!(StepKind::DesignAnalysis.label(), "Design Analysis");
        assert_eq!(StepKind::AccessibilityAudit.label(), "Accessibility Audit");
        assert_eq!(StepKind::Custom.label(), "Custom");
    }

    #[test]
    fn step_requires_inference() {
        assert!(StepKind::LayoutGeneration.requires_inference());
        assert!(!StepKind::DesignAnalysis.requires_inference());
    }

    #[test]
    fn step_with_params() {
        let step = PipelineStep::new(StepKind::DesignAnalysis)
            .with_param("tolerance", "strict");
        assert_eq!(step.params.get("tolerance").unwrap(), "strict");
    }

    // ── Pipeline Runner ──────────────────────────────────────

    #[test]
    fn run_default_pipeline() {
        let runner = PipelineRunner::with_defaults();
        let pipeline = PipelinePresets::design_review();
        let result = runner.run(&pipeline);
        assert!(result.success);
        assert_eq!(result.successful_steps(), 5);
        assert_eq!(result.failed_steps(), 0);
    }

    #[test]
    fn run_empty_pipeline() {
        let runner = PipelineRunner::with_defaults();
        let pipeline = Pipeline::new("empty");
        let result = runner.run(&pipeline);
        assert!(result.success);
        assert_eq!(result.step_results.len(), 0);
    }

    #[test]
    fn run_with_failing_handler() {
        let mut runner = PipelineRunner::new();
        runner.register_handler(
            StepKind::DesignAnalysis,
            Box::new(|_| Err("analysis failed".to_string())),
        );
        let pipeline = Pipeline::new("fail-test")
            .add_step(PipelineStep::new(StepKind::DesignAnalysis));
        let result = runner.run(&pipeline);
        assert!(!result.success);
        assert_eq!(result.failed_steps(), 1);
        assert!(!result.errors().is_empty());
    }

    #[test]
    fn optional_step_failure_doesnt_fail_pipeline() {
        let mut runner = PipelineRunner::new();
        runner.register_handler(
            StepKind::DesignAnalysis,
            Box::new(|_| Ok(vec!["ok".into()])),
        );
        // No handler for ColorHarmony → will fail, but it's optional
        let pipeline = Pipeline::new("test")
            .add_step(PipelineStep::new(StepKind::DesignAnalysis))
            .add_step(PipelineStep::new(StepKind::ColorHarmony).as_optional());
        let result = runner.run(&pipeline);
        assert!(result.success); // Optional failure doesn't affect overall
    }

    #[test]
    fn custom_step_passes_through() {
        let runner = PipelineRunner::new();
        let pipeline = Pipeline::new("custom")
            .add_step(PipelineStep::new(StepKind::Custom));
        let result = runner.run(&pipeline);
        assert!(result.success);
    }

    #[test]
    fn findings_aggregated() {
        let runner = PipelineRunner::with_defaults();
        let pipeline = PipelinePresets::design_review();
        let result = runner.run(&pipeline);
        let findings = result.all_findings();
        assert!(findings.len() >= 5);
    }

    #[test]
    fn step_result_with_data() {
        let step = PipelineStep::new(StepKind::DesignAnalysis);
        let sr = StepResult::success(&step, Duration::from_millis(1), vec![])
            .with_data("count", "42");
        assert_eq!(sr.data.get("count").unwrap(), "42");
    }

    // ── Presets ──────────────────────────────────────────────

    #[test]
    fn preset_design_review() {
        let p = PipelinePresets::design_review();
        assert_eq!(p.steps().len(), 5);
        assert_eq!(p.inference_steps(), 0);
    }

    #[test]
    fn preset_accessibility() {
        let p = PipelinePresets::accessibility_only();
        assert_eq!(p.steps().len(), 1);
    }

    #[test]
    fn preset_generative() {
        let p = PipelinePresets::generative();
        assert_eq!(p.steps().len(), 3);
        assert_eq!(p.optional_steps(), 2); // style transfer + asset gen optional
        assert!(p.inference_steps() >= 1);
    }

    #[test]
    fn generative_pipeline_runs() {
        let runner = PipelineRunner::with_defaults();
        let pipeline = PipelinePresets::generative();
        let result = runner.run(&pipeline);
        assert!(result.success);
    }
}
