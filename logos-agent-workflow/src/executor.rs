//! Workflow executor — drives DAG step execution with policy enforcement.

use std::collections::HashMap;
use crate::dag::{WorkflowDef, NodeStatus, DagError};
use crate::policy::{RetryPolicy, TimeoutPolicy};
use crate::checkpoint::{CheckpointStore, StepCheckpoint};
use thiserror::Error;

/// Errors from the executor.
#[derive(Debug, Error, PartialEq)]
pub enum ExecutorError {
    #[error("workflow definition error: {0}")]
    DagError(#[from] DagError),
    #[error("step '{0}' failed after {1} attempt(s): {2}")]
    StepFailed(String, u32, String),
    #[error("step '{0}' timed out after {1}ms")]
    StepTimedOut(String, u64),
    #[error("workflow '{0}' already running")]
    AlreadyRunning(String),
    #[error("no step handler provided for '{0}'")]
    NoHandler(String),
}

/// The overall status of a workflow run.
#[derive(Debug, Clone, PartialEq)]
pub enum RunStatus {
    Pending,
    Running,
    Succeeded,
    Failed { failed_step: String, reason: String },
    PartiallyComplete { completed: Vec<String>, failed: Vec<String> },
}

impl RunStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, RunStatus::Succeeded | RunStatus::Failed { .. } | RunStatus::PartiallyComplete { .. })
    }
}

/// The outcome of executing a single step.
#[derive(Debug, Clone)]
pub struct StepOutcome {
    pub node_id:    String,
    pub status:     NodeStatus,
    pub duration_ms: u64,
    pub attempt:    u32,
    pub output:     Option<String>,
}

impl StepOutcome {
    pub fn success(node_id: impl Into<String>, duration_ms: u64) -> Self {
        Self {
            node_id: node_id.into(),
            status: NodeStatus::Succeeded,
            duration_ms,
            attempt: 1,
            output: None,
        }
    }

    pub fn failure(node_id: impl Into<String>, reason: impl Into<String>, attempts: u32) -> Self {
        Self {
            node_id: node_id.into(),
            status: NodeStatus::Failed { reason: reason.into() },
            duration_ms: 0,
            attempt: attempts,
            output: None,
        }
    }

    pub fn with_output(mut self, output: impl Into<String>) -> Self {
        self.output = Some(output.into());
        self
    }

    pub fn is_success(&self) -> bool {
        self.status == NodeStatus::Succeeded
    }
}

/// A completed workflow run with per-step results.
#[derive(Debug)]
pub struct WorkflowRun {
    pub workflow_id: String,
    pub status:      RunStatus,
    pub steps:       HashMap<String, StepOutcome>,
    pub total_duration_ms: u64,
}

impl WorkflowRun {
    pub fn succeeded_steps(&self) -> usize {
        self.steps.values().filter(|s| s.is_success()).count()
    }

    pub fn failed_steps(&self) -> usize {
        self.steps.values().filter(|s| !s.is_success()).count()
    }
}

/// Per-step execution configuration attached to the executor.
#[derive(Debug, Clone, Default)]
pub struct StepConfig {
    pub retry:   Option<RetryPolicy>,
    pub timeout: Option<TimeoutPolicy>,
}

/// Executes a `WorkflowDef` step by step, respecting policies.
pub struct WorkflowExecutor {
    checkpoint_store: CheckpointStore,
    step_configs:     HashMap<String, StepConfig>,
}

impl WorkflowExecutor {
    pub fn new(checkpoint_store: CheckpointStore) -> Self {
        Self { checkpoint_store, step_configs: HashMap::new() }
    }

    /// Read-only access to the checkpoint store (useful for post-run inspection).
    pub fn checkpoint_store(&self) -> &CheckpointStore {
        &self.checkpoint_store
    }

    /// Attach a retry policy to a specific step.
    pub fn set_retry(&mut self, node_id: impl Into<String>, policy: RetryPolicy) {
        self.step_configs.entry(node_id.into()).or_default().retry = Some(policy);
    }

    /// Attach a timeout policy to a specific step.
    pub fn set_timeout(&mut self, node_id: impl Into<String>, policy: TimeoutPolicy) {
        self.step_configs.entry(node_id.into()).or_default().timeout = Some(policy);
    }

    /// Run the workflow synchronously.
    ///
    /// `handler` is called once per step and returns the `StepOutcome`.
    /// The handler is responsible for simulating the actual work.
    pub fn run<F>(&mut self, def: &WorkflowDef, mut handler: F) -> Result<WorkflowRun, ExecutorError>
    where
        F: FnMut(&str) -> StepOutcome,
    {
        let order = def.topological_order()?;
        let mut steps: HashMap<String, StepOutcome> = HashMap::new();
        let mut total_ms = 0u64;

        for node_id in &order {
            // Skip if already checkpointed
            if self.checkpoint_store.is_completed(def.id.as_str(), node_id) {
                let ck = self.checkpoint_store
                    .get_step(def.id.as_str(), node_id)
                    .unwrap();
                steps.insert(node_id.clone(), StepOutcome {
                    node_id: node_id.clone(),
                    status: NodeStatus::Succeeded,
                    duration_ms: ck.duration_ms,
                    attempt: ck.attempt,
                    output: ck.output.clone(),
                });
                continue;
            }

            let retry = self.step_configs
                .get(node_id.as_str())
                .and_then(|c| c.retry.clone())
                .unwrap_or_default();

            let timeout = self.step_configs
                .get(node_id.as_str())
                .and_then(|c| c.timeout.clone());

            let mut outcome = self.execute_with_retry(node_id, &retry, &timeout, &mut handler);

            // Enforce timeout check on duration
            if let Some(ref tp) = timeout {
                if tp.is_exceeded(outcome.duration_ms) && outcome.is_success() {
                    if tp.fail_on_timeout {
                        outcome.status = NodeStatus::Failed {
                            reason: format!("timed out after {}ms", outcome.duration_ms),
                        };
                    }
                }
            }

            total_ms += outcome.duration_ms;

            if !outcome.is_success() {
                let reason = match &outcome.status {
                    NodeStatus::Failed { reason } => reason.clone(),
                    _ => "unknown".to_owned(),
                };
                let _attempts = outcome.attempt;
                steps.insert(node_id.clone(), outcome);
                return Ok(WorkflowRun {
                    workflow_id: def.id.clone(),
                    status: RunStatus::Failed { failed_step: node_id.clone(), reason },
                    steps,
                    total_duration_ms: total_ms,
                });
            }

            // Checkpoint the successful step
            self.checkpoint_store.save_step(def.id.to_owned(), StepCheckpoint {
                node_id: node_id.clone(),
                attempt: outcome.attempt,
                duration_ms: outcome.duration_ms,
                output: outcome.output.clone(),
            });

            steps.insert(node_id.clone(), outcome);
        }

        Ok(WorkflowRun {
            workflow_id: def.id.clone(),
            status: RunStatus::Succeeded,
            steps,
            total_duration_ms: total_ms,
        })
    }

    fn execute_with_retry<F>(
        &self,
        node_id: &str,
        retry: &RetryPolicy,
        _timeout: &Option<TimeoutPolicy>,
        handler: &mut F,
    ) -> StepOutcome
    where
        F: FnMut(&str) -> StepOutcome,
    {
        let mut attempt = 0u32;
        loop {
            let mut outcome = handler(node_id);
            outcome.attempt = attempt + 1;

            if outcome.is_success() {
                return outcome;
            }
            if !retry.should_retry(attempt) {
                return outcome;
            }
            // In a real executor we'd sleep delay_before_attempt(attempt+1) ms.
            // In sync tests we just increment.
            attempt += 1;
        }
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dag::{WorkflowDef, StepNode, StepEdge, NodeKind};
    use crate::checkpoint::CheckpointStore;

    fn two_step_def() -> WorkflowDef {
        let mut def = WorkflowDef::new("wf", "Two-step");
        def.add_node(StepNode::new("a", NodeKind::AgentTask)).unwrap();
        def.add_node(StepNode::new("b", NodeKind::AgentTask)).unwrap();
        def.add_edge(StepEdge::new("a", "b")).unwrap();
        def
    }

    #[test]
    fn successful_run_two_steps() {
        let mut exec = WorkflowExecutor::new(CheckpointStore::new());
        let def = two_step_def();
        let run = exec.run(&def, |id| StepOutcome::success(id, 10)).unwrap();
        assert_eq!(run.status, RunStatus::Succeeded);
        assert_eq!(run.succeeded_steps(), 2);
    }

    #[test]
    fn failing_step_stops_workflow() {
        let mut exec = WorkflowExecutor::new(CheckpointStore::new());
        let def = two_step_def();
        let run = exec.run(&def, |id| {
            if id == "a" { StepOutcome::failure(id, "boom", 1) }
            else         { StepOutcome::success(id, 10) }
        }).unwrap();
        assert!(matches!(run.status, RunStatus::Failed { .. }));
        assert_eq!(run.failed_steps(), 1);
    }

    #[test]
    fn retry_policy_retries_on_failure() {
        let mut exec = WorkflowExecutor::new(CheckpointStore::new());
        exec.set_retry("a", RetryPolicy::fixed(3, 0));
        let def = two_step_def();
        let mut calls = 0u32;
        let run = exec.run(&def, |id| {
            if id == "a" {
                calls += 1;
                if calls < 2 { StepOutcome::failure(id, "transient", calls) }
                else          { StepOutcome::success(id, 5) }
            } else {
                StepOutcome::success(id, 5)
            }
        }).unwrap();
        assert_eq!(run.status, RunStatus::Succeeded);
    }

    #[test]
    fn checkpointed_step_not_re_run() {
        let mut store = CheckpointStore::new();
        store.save_step("wf", StepCheckpoint { node_id: "a".to_owned(), attempt: 1, duration_ms: 5, output: None });
        let mut exec = WorkflowExecutor::new(store);
        let def = two_step_def();
        let mut called_a = false;
        let run = exec.run(&def, |id| {
            if id == "a" { called_a = true; }
            StepOutcome::success(id, 10)
        }).unwrap();
        assert!(!called_a, "checkpointed step should not be re-executed");
        assert_eq!(run.status, RunStatus::Succeeded);
    }

    #[test]
    fn step_outcome_success_flag() {
        let o = StepOutcome::success("x", 10);
        assert!(o.is_success());
    }

    #[test]
    fn step_outcome_failure_flag() {
        let o = StepOutcome::failure("x", "err", 1);
        assert!(!o.is_success());
    }

    #[test]
    fn run_status_terminal_flags() {
        assert!(RunStatus::Succeeded.is_terminal());
        assert!(!RunStatus::Running.is_terminal());
    }

    #[test]
    fn total_duration_accumulates() {
        let mut exec = WorkflowExecutor::new(CheckpointStore::new());
        let def = two_step_def();
        let run = exec.run(&def, |id| StepOutcome::success(id, 100)).unwrap();
        assert_eq!(run.total_duration_ms, 200);
    }

    #[test]
    fn step_output_propagated() {
        let mut exec = WorkflowExecutor::new(CheckpointStore::new());
        let mut def = WorkflowDef::new("wf2", "W");
        def.add_node(StepNode::new("only", NodeKind::AgentTask)).unwrap();
        let run = exec.run(&def, |id| {
            StepOutcome::success(id, 5).with_output("result-data")
        }).unwrap();
        let step = &run.steps["only"];
        assert_eq!(step.output.as_deref(), Some("result-data"));
    }

    #[test]
    fn timeout_fails_slow_step() {
        let mut exec = WorkflowExecutor::new(CheckpointStore::new());
        exec.set_timeout("a", TimeoutPolicy::new(50));
        let def = two_step_def();
        let run = exec.run(&def, |id| {
            if id == "a" { StepOutcome::success(id, 200) } // 200ms > 50ms timeout
            else          { StepOutcome::success(id, 10) }
        }).unwrap();
        assert!(matches!(run.status, RunStatus::Failed { failed_step, .. } if failed_step == "a"));
    }
}
