//! Workflow checkpointing — persist completed steps so a workflow can resume.

use std::collections::HashMap;
use thiserror::Error;

/// Errors produced by the checkpoint store.
#[derive(Debug, Error, PartialEq)]
pub enum CheckpointError {
    #[error("checkpoint for workflow '{0}' not found")]
    NotFound(String),
    #[error("step '{0}' in workflow '{1}' already exists")]
    DuplicateStep(String, String),
    #[error("workflow '{0}' has no completed steps")]
    EmptyWorkflow(String),
}

/// The persisted state of a single completed step.
#[derive(Debug, Clone)]
pub struct StepCheckpoint {
    pub node_id:     String,
    pub attempt:     u32,
    pub duration_ms: u64,
    pub output:      Option<String>,
}

/// A snapshot of an entire workflow's progress.
#[derive(Debug, Clone)]
pub struct WorkflowCheckpoint {
    pub workflow_id: String,
    /// Ordered list of step IDs that have been completed.
    pub completed:   Vec<String>,
    /// Map of step_id → per-step data.
    pub steps:       HashMap<String, StepCheckpoint>,
}

impl WorkflowCheckpoint {
    fn new(workflow_id: impl Into<String>) -> Self {
        Self {
            workflow_id: workflow_id.into(),
            completed: Vec::new(),
            steps: HashMap::new(),
        }
    }

    /// Return the node IDs that still have to be executed.
    pub fn remaining<'a>(&self, all: &'a [String]) -> Vec<&'a String> {
        all.iter().filter(|id| !self.completed.contains(id)).collect()
    }
}

/// In-memory store for workflow checkpoints.
#[derive(Debug, Default)]
pub struct CheckpointStore {
    snapshots: HashMap<String, WorkflowCheckpoint>,
}

impl CheckpointStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that a single step has completed.
    pub fn save_step(&mut self, workflow_id: impl Into<String>, step: StepCheckpoint) {
        let wid = workflow_id.into();
        let snapshot = self.snapshots.entry(wid.clone()).or_insert_with(|| {
            WorkflowCheckpoint::new(wid.clone())
        });
        if !snapshot.completed.contains(&step.node_id) {
            snapshot.completed.push(step.node_id.clone());
            snapshot.steps.insert(step.node_id.clone(), step);
        }
    }

    /// Check whether a step is already recorded as done.
    pub fn is_completed(&self, workflow_id: &str, node_id: &str) -> bool {
        self.snapshots
            .get(workflow_id)
            .map(|s| s.completed.contains(&node_id.to_owned()))
            .unwrap_or(false)
    }

    /// Retrieve the per-step record (if available).
    pub fn get_step(&self, workflow_id: &str, node_id: &str) -> Option<&StepCheckpoint> {
        self.snapshots
            .get(workflow_id)
            .and_then(|s| s.steps.get(node_id))
    }

    /// Return the full snapshot for a workflow, if one exists.
    pub fn get_snapshot(&self, workflow_id: &str) -> Result<&WorkflowCheckpoint, CheckpointError> {
        self.snapshots
            .get(workflow_id)
            .ok_or_else(|| CheckpointError::NotFound(workflow_id.to_owned()))
    }

    /// Return the list of step IDs that have been completed.
    pub fn completed_steps(&self, workflow_id: &str) -> Vec<String> {
        self.snapshots
            .get(workflow_id)
            .map(|s| s.completed.clone())
            .unwrap_or_default()
    }

    /// Return step IDs that still need executing, given the full ordered list.
    pub fn remaining_steps<'a>(&self, workflow_id: &str, all: &'a [String]) -> Vec<&'a String> {
        let done = self.completed_steps(workflow_id);
        all.iter().filter(|id| !done.contains(id)).collect()
    }

    /// Remove checkpointing data for one workflow.
    pub fn clear_workflow(&mut self, workflow_id: &str) {
        self.snapshots.remove(workflow_id);
    }

    /// Remove all checkpoint data.
    pub fn clear_all(&mut self) {
        self.snapshots.clear();
    }

    /// How many workflows currently have checkpoint data.
    pub fn workflow_count(&self) -> usize {
        self.snapshots.len()
    }

    /// List all workflow IDs that have at least one checkpoint.
    pub fn workflow_ids(&self) -> Vec<String> {
        self.snapshots.keys().cloned().collect()
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn ck(id: &str) -> StepCheckpoint {
        StepCheckpoint { node_id: id.to_owned(), attempt: 1, duration_ms: 10, output: None }
    }

    #[test]
    fn save_and_is_completed() {
        let mut store = CheckpointStore::new();
        assert!(!store.is_completed("wf1", "step-a"));
        store.save_step("wf1", ck("step-a"));
        assert!(store.is_completed("wf1", "step-a"));
    }

    #[test]
    fn different_workflows_isolated() {
        let mut store = CheckpointStore::new();
        store.save_step("wf1", ck("a"));
        assert!(!store.is_completed("wf2", "a"));
    }

    #[test]
    fn get_step_returns_record() {
        let mut store = CheckpointStore::new();
        store.save_step("wf1", StepCheckpoint { node_id: "a".to_owned(), attempt: 2, duration_ms: 50, output: Some("out".to_owned()) });
        let step = store.get_step("wf1", "a").unwrap();
        assert_eq!(step.attempt, 2);
        assert_eq!(step.duration_ms, 50);
        assert_eq!(step.output.as_deref(), Some("out"));
    }

    #[test]
    fn get_step_missing_returns_none() {
        let store = CheckpointStore::new();
        assert!(store.get_step("wf1", "x").is_none());
    }

    #[test]
    fn completed_steps_ordering() {
        let mut store = CheckpointStore::new();
        store.save_step("wf", ck("a"));
        store.save_step("wf", ck("b"));
        store.save_step("wf", ck("c"));
        assert_eq!(store.completed_steps("wf"), vec!["a", "b", "c"]);
    }

    #[test]
    fn remaining_steps_filters_done() {
        let mut store = CheckpointStore::new();
        store.save_step("wf", ck("a"));
        store.save_step("wf", ck("b"));
        let all: Vec<String> = vec!["a".to_owned(), "b".to_owned(), "c".to_owned()];
        let remaining = store.remaining_steps("wf", &all);
        assert_eq!(remaining, vec![&"c".to_owned()]);
    }

    #[test]
    fn clear_workflow_removes_data() {
        let mut store = CheckpointStore::new();
        store.save_step("wf", ck("a"));
        store.clear_workflow("wf");
        assert!(!store.is_completed("wf", "a"));
    }

    #[test]
    fn clear_all_removes_everything() {
        let mut store = CheckpointStore::new();
        store.save_step("wf1", ck("a"));
        store.save_step("wf2", ck("b"));
        store.clear_all();
        assert_eq!(store.workflow_count(), 0);
    }

    #[test]
    fn get_snapshot_error_when_missing() {
        let store = CheckpointStore::new();
        assert_eq!(store.get_snapshot("missing").unwrap_err(), CheckpointError::NotFound("missing".to_owned()));
    }

    #[test]
    fn workflow_ids_lists_tracked() {
        let mut store = CheckpointStore::new();
        store.save_step("wf1", ck("a"));
        store.save_step("wf2", ck("b"));
        let mut ids = store.workflow_ids();
        ids.sort();
        assert_eq!(ids, vec!["wf1", "wf2"]);
    }

    #[test]
    fn duplicate_step_ignored() {
        let mut store = CheckpointStore::new();
        store.save_step("wf", ck("a"));
        store.save_step("wf", ck("a")); // duplicate — should be silently ignored
        assert_eq!(store.completed_steps("wf").len(), 1);
    }

    #[test]
    fn snapshot_remaining_empty_when_all_done() {
        let mut store = CheckpointStore::new();
        let all = vec!["a".to_owned(), "b".to_owned()];
        store.save_step("wf", ck("a"));
        store.save_step("wf", ck("b"));
        let rem = store.remaining_steps("wf", &all);
        assert!(rem.is_empty());
    }
}
