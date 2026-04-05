//! Integration tests — Phase 15.13: Agent Workflow Orchestrator
//! §1 DAG engine        : tests  1-15
//! §2 Policies          : tests 16-28
//! §3 Executor          : tests 29-43
//! §4 Checkpointing     : tests 44-55
//! §5 End-to-end        : tests 56-70

use logos_agent_workflow::{
    // DAG
    WorkflowDef, StepNode, StepEdge, NodeKind, NodeStatus, DagError,
    // Policies
    RetryPolicy, TimeoutPolicy, BackoffPolicy, BackoffKind, PolicyError,
    // Executor
    WorkflowExecutor, StepOutcome, RunStatus, ExecutorError,
    // Checkpoint
    CheckpointStore, StepCheckpoint, CheckpointError,
};

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn simple_def(id: &str) -> WorkflowDef {
    let mut d = WorkflowDef::new(id, "Test");
    d.add_node(StepNode::new("a", NodeKind::AgentTask)).unwrap();
    d.add_node(StepNode::new("b", NodeKind::AgentTask)).unwrap();
    d.add_edge(StepEdge::new("a", "b")).unwrap();
    d
}

fn ok_handler(node_id: &str) -> StepOutcome {
    StepOutcome::success(node_id, 10)
}

fn fail_handler(node_id: &str) -> StepOutcome {
    StepOutcome::failure(node_id, "deliberate_fail", 1)
}

fn ck(id: &str) -> StepCheckpoint {
    StepCheckpoint { node_id: id.to_owned(), attempt: 1, duration_ms: 5, output: None }
}

// ═════════════════════════════════════════════════════════════════════════════
// §1  DAG Engine (tests 1–15)
// ═════════════════════════════════════════════════════════════════════════════

/// Test 1: empty workflow yields error
#[test]
fn t01_empty_workflow_topo_error() {
    let d = WorkflowDef::new("empty", "Empty");
    assert_eq!(d.topological_order().unwrap_err(), DagError::EmptyWorkflow);
}

/// Test 2: single-node topo order
#[test]
fn t02_single_node_topo_order() {
    let mut d = WorkflowDef::new("w", "W");
    d.add_node(StepNode::new("only", NodeKind::NoOp)).unwrap();
    assert_eq!(d.topological_order().unwrap(), vec!["only"]);
}

/// Test 3: two-node linear chain
#[test]
fn t03_two_node_linear_chain() {
    let d = simple_def("w");
    let order = d.topological_order().unwrap();
    assert_eq!(order, vec!["a", "b"]);
}

/// Test 4: three-node chain
#[test]
fn t04_three_node_chain() {
    let mut d = WorkflowDef::new("w", "W");
    for id in ["a", "b", "c"] {
        d.add_node(StepNode::new(id, NodeKind::AgentTask)).unwrap();
    }
    d.add_edge(StepEdge::new("a", "b")).unwrap();
    d.add_edge(StepEdge::new("b", "c")).unwrap();
    let order = d.topological_order().unwrap();
    assert_eq!(order, vec!["a", "b", "c"]);
}

/// Test 5: cycle detection A→B→A
#[test]
fn t05_cycle_two_nodes() {
    let mut d = WorkflowDef::new("w", "W");
    d.add_node(StepNode::new("a", NodeKind::AgentTask)).unwrap();
    d.add_node(StepNode::new("b", NodeKind::AgentTask)).unwrap();
    d.add_edge(StepEdge::new("a", "b")).unwrap();
    d.add_edge(StepEdge::new("b", "a")).unwrap();
    assert_eq!(d.topological_order().unwrap_err(), DagError::CycleDetected);
}

/// Test 6: self-loop rejected
#[test]
fn t06_self_loop_rejected() {
    let mut d = WorkflowDef::new("w", "W");
    d.add_node(StepNode::new("x", NodeKind::AgentTask)).unwrap();
    assert_eq!(d.add_edge(StepEdge::new("x", "x")).unwrap_err(), DagError::SelfLoop("x".to_owned()));
}

/// Test 7: duplicate node rejected
#[test]
fn t07_duplicate_node_rejected() {
    let mut d = WorkflowDef::new("w", "W");
    d.add_node(StepNode::new("a", NodeKind::AgentTask)).unwrap();
    assert_eq!(d.add_node(StepNode::new("a", NodeKind::AgentTask)).unwrap_err(), DagError::DuplicateNode("a".to_owned()));
}

/// Test 8: duplicate edge rejected
#[test]
fn t08_duplicate_edge_rejected() {
    let d = simple_def("w");
    let mut d = d;
    assert_eq!(d.add_edge(StepEdge::new("a", "b")).unwrap_err(), DagError::DuplicateEdge("a".to_owned(), "b".to_owned()));
}

/// Test 9: roots identified correctly
#[test]
fn t09_roots_identified() {
    let d = simple_def("w");
    assert_eq!(d.roots(), vec!["a"]);
}

/// Test 10: leaves identified correctly
#[test]
fn t10_leaves_identified() {
    let d = simple_def("w");
    assert_eq!(d.leaves(), vec!["b"]);
}

/// Test 11: successors / predecessors
#[test]
fn t11_successors_predecessors() {
    let d = simple_def("w");
    assert_eq!(d.successors("a"), vec!["b"]);
    assert_eq!(d.predecessors("b"), vec!["a"]);
    assert!(d.predecessors("a").is_empty());
}

/// Test 12: fork topology (a → b, a → c)
#[test]
fn t12_fork_topology() {
    let mut d = WorkflowDef::new("w", "Fork");
    for id in ["a", "b", "c"] {
        d.add_node(StepNode::new(id, NodeKind::AgentTask)).unwrap();
    }
    d.add_edge(StepEdge::new("a", "b")).unwrap();
    d.add_edge(StepEdge::new("a", "c")).unwrap();
    let order = d.topological_order().unwrap();
    assert_eq!(order[0], "a");
    assert!(order.contains(&"b".to_owned()));
    assert!(order.contains(&"c".to_owned()));
}

/// Test 13: edge to unknown node rejected
#[test]
fn t13_edge_unknown_node_rejected() {
    let mut d = WorkflowDef::new("w", "W");
    d.add_node(StepNode::new("a", NodeKind::AgentTask)).unwrap();
    assert!(matches!(d.add_edge(StepEdge::new("a", "z")).unwrap_err(), DagError::NodeNotFound(_)));
}

/// Test 14: NodeKind labels correct
#[test]
fn t14_node_kind_labels() {
    assert_eq!(NodeKind::AgentTask.label(), "AGENT_TASK");
    assert_eq!(NodeKind::Transform.label(), "TRANSFORM");
    assert_eq!(NodeKind::Decision.label(), "DECISION");
    assert_eq!(NodeKind::Webhook.label(), "WEBHOOK");
    assert_eq!(NodeKind::NoOp.label(), "NO_OP");
}

/// Test 15: NodeStatus terminal flags
#[test]
fn t15_node_status_terminal_flags() {
    assert!(NodeStatus::Succeeded.is_terminal());
    assert!(NodeStatus::Failed { reason: "x".to_owned() }.is_terminal());
    assert!(NodeStatus::Skipped.is_terminal());
    assert!(!NodeStatus::Pending.is_terminal());
    assert!(!NodeStatus::Running.is_terminal());
}

// ═════════════════════════════════════════════════════════════════════════════
// §2  Policies (tests 16–28)
// ═════════════════════════════════════════════════════════════════════════════

/// Test 16: RetryPolicy::none() — no retry
#[test]
fn t16_retry_none_no_retry() {
    let rp = RetryPolicy::none();
    assert!(!rp.should_retry(0), "none policy must not retry on first failure");
}

/// Test 17: RetryPolicy::fixed(3, 100) — retries up to 2 more times
#[test]
fn t17_retry_fixed_retries() {
    let rp = RetryPolicy::fixed(3, 100);
    assert!(rp.should_retry(0));
    assert!(rp.should_retry(1));
    assert!(!rp.should_retry(2));
}

/// Test 18: Fixed backoff always returns same delay
#[test]
fn t18_backoff_fixed_constant() {
    let bp = BackoffPolicy::fixed(50);
    assert_eq!(bp.delay_ms(0), 50);
    assert_eq!(bp.delay_ms(3), 50);
}

/// Test 19: Exponential backoff doubles
#[test]
fn t19_backoff_exponential_doubles() {
    let bp = BackoffPolicy::exponential(10, 1000);
    assert_eq!(bp.delay_ms(0), 10);
    assert_eq!(bp.delay_ms(1), 20);
    assert_eq!(bp.delay_ms(2), 40);
}

/// Test 20: Exponential backoff capped by max
#[test]
fn t20_backoff_exponential_capped() {
    let bp = BackoffPolicy::exponential(100, 150);
    assert_eq!(bp.delay_ms(5), 150, "must not exceed max_delay_ms");
}

/// Test 21: Exponential jitter adds deterministic fraction
#[test]
fn t21_backoff_jitter_adds() {
    let bp = BackoffPolicy::exponential_jitter(100, 10_000);
    // jitter adds 25% of base (25ms)
    assert_eq!(bp.delay_ms(0), 125);
}

/// Test 22: Linear backoff grows linearly
#[test]
fn t22_backoff_linear() {
    let bp = BackoffPolicy { kind: BackoffKind::Linear, base_delay_ms: 50, max_delay_ms: 1000 };
    assert_eq!(bp.delay_ms(0), 50);
    assert_eq!(bp.delay_ms(1), 100);
    assert_eq!(bp.delay_ms(2), 150);
}

/// Test 23: TimeoutPolicy::is_exceeded
#[test]
fn t23_timeout_is_exceeded() {
    let tp = TimeoutPolicy::new(500);
    assert!(!tp.is_exceeded(499));
    assert!(!tp.is_exceeded(500));
    assert!(tp.is_exceeded(501));
}

/// Test 24: TimeoutPolicy::lenient — fail_on_timeout is false
#[test]
fn t24_timeout_lenient() {
    let tp = TimeoutPolicy::lenient(100);
    assert!(!tp.fail_on_timeout);
}

/// Test 25: Validation rejects zero max_attempts
#[test]
fn t25_policy_validate_zero_attempts() {
    let rp = RetryPolicy { max_attempts: 0, backoff: BackoffPolicy::fixed(0), transient_only: false };
    assert_eq!(rp.validate().unwrap_err(), PolicyError::InvalidMaxAttempts(0));
}

/// Test 26: Validation rejects base_delay_ms = 0
#[test]
fn t26_backoff_validate_zero_base() {
    let bp = BackoffPolicy::fixed(0);
    assert_eq!(bp.validate().unwrap_err(), PolicyError::InvalidBaseDelay(0));
}

/// Test 27: Validation rejects max < base
#[test]
fn t27_backoff_validate_max_lt_base() {
    let bp = BackoffPolicy { kind: BackoffKind::Fixed, base_delay_ms: 100, max_delay_ms: 50 };
    assert_eq!(bp.validate().unwrap_err(), PolicyError::MaxDelayTooSmall);
}

/// Test 28: delay_before_attempt returns 0 for first attempt
#[test]
fn t28_retry_first_attempt_no_delay() {
    let rp = RetryPolicy::fixed(3, 200);
    assert_eq!(rp.delay_before_attempt(0), 0);
    assert_eq!(rp.delay_before_attempt(1), 200);
}

// ═════════════════════════════════════════════════════════════════════════════
// §3  Executor (tests 29–43)
// ═════════════════════════════════════════════════════════════════════════════

/// Test 29: successful two-step run
#[test]
fn t29_executor_two_step_success() {
    let mut exec = WorkflowExecutor::new(CheckpointStore::new());
    let run = exec.run(&simple_def("wf"), ok_handler).unwrap();
    assert_eq!(run.status, RunStatus::Succeeded);
    assert_eq!(run.succeeded_steps(), 2);
}

/// Test 30: first step fails — workflow stops
#[test]
fn t30_executor_first_step_fails() {
    let mut exec = WorkflowExecutor::new(CheckpointStore::new());
    let run = exec.run(&simple_def("wf"), fail_handler).unwrap();
    assert!(matches!(run.status, RunStatus::Failed { failed_step, .. } if failed_step == "a"));
}

/// Test 31: second step fails — first step recorded
#[test]
fn t31_executor_second_step_fails() {
    let mut exec = WorkflowExecutor::new(CheckpointStore::new());
    let run = exec.run(&simple_def("wf"), |id| {
        if id == "a" { ok_handler(id) } else { fail_handler(id) }
    }).unwrap();
    assert!(matches!(run.status, RunStatus::Failed { failed_step, .. } if failed_step == "b"));
    assert!(run.steps.contains_key("a"));
}

/// Test 32: retry on transient failure succeeds eventually
#[test]
fn t32_executor_retry_succeeds() {
    let mut exec = WorkflowExecutor::new(CheckpointStore::new());
    exec.set_retry("a", RetryPolicy::fixed(3, 0));
    let mut count = 0;
    let run = exec.run(&simple_def("wf"), |id| {
        if id == "a" {
            count += 1;
            if count < 3 { fail_handler(id) } else { ok_handler(id) }
        } else {
            ok_handler(id)
        }
    }).unwrap();
    assert_eq!(run.status, RunStatus::Succeeded);
}

/// Test 33: retry exhausted — step fails
#[test]
fn t33_executor_retry_exhausted() {
    let mut exec = WorkflowExecutor::new(CheckpointStore::new());
    exec.set_retry("a", RetryPolicy::fixed(2, 0));
    let run = exec.run(&simple_def("wf"), |id| {
        if id == "a" { fail_handler(id) } else { ok_handler(id) }
    }).unwrap();
    assert!(matches!(run.status, RunStatus::Failed { .. }));
}

/// Test 34: timeout fails slow step
#[test]
fn t34_executor_timeout_fails_step() {
    let mut exec = WorkflowExecutor::new(CheckpointStore::new());
    exec.set_timeout("a", TimeoutPolicy::new(50));
    let run = exec.run(&simple_def("wf"), |id| {
        if id == "a" { StepOutcome::success(id, 200) } else { ok_handler(id) }
    }).unwrap();
    assert!(matches!(run.status, RunStatus::Failed { failed_step, .. } if failed_step == "a"));
}

/// Test 35: lenient timeout — step continues despite being slow
#[test]
fn t35_executor_lenient_timeout_continues() {
    let mut exec = WorkflowExecutor::new(CheckpointStore::new());
    exec.set_timeout("a", TimeoutPolicy::lenient(50));
    let run = exec.run(&simple_def("wf"), |id| {
        StepOutcome::success(id, 200)
    }).unwrap();
    assert_eq!(run.status, RunStatus::Succeeded);
}

/// Test 36: total_duration_ms accumulates step durations
#[test]
fn t36_executor_total_duration() {
    let mut exec = WorkflowExecutor::new(CheckpointStore::new());
    let run = exec.run(&simple_def("wf"), |id| StepOutcome::success(id, 30)).unwrap();
    assert_eq!(run.total_duration_ms, 60);
}

/// Test 37: output propagated in step result
#[test]
fn t37_executor_step_output() {
    let mut exec = WorkflowExecutor::new(CheckpointStore::new());
    let mut d = WorkflowDef::new("wf", "W");
    d.add_node(StepNode::new("x", NodeKind::AgentTask)).unwrap();
    let run = exec.run(&d, |id| StepOutcome::success(id, 5).with_output("png-bytes")).unwrap();
    assert_eq!(run.steps["x"].output.as_deref(), Some("png-bytes"));
}

/// Test 38: checkpointed step not re-executed
#[test]
fn t38_executor_checkpoint_skips_step() {
    let mut store = CheckpointStore::new();
    store.save_step("wf", ck("a"));
    let mut exec = WorkflowExecutor::new(store);
    let mut ran_a = false;
    exec.run(&simple_def("wf"), |id| {
        if id == "a" { ran_a = true; }
        ok_handler(id)
    }).unwrap();
    assert!(!ran_a);
}

/// Test 39: three-step chain executes in order
#[test]
fn t39_executor_three_step_order() {
    let mut d = WorkflowDef::new("wf", "W");
    for id in ["a", "b", "c"] {
        d.add_node(StepNode::new(id, NodeKind::AgentTask)).unwrap();
    }
    d.add_edge(StepEdge::new("a", "b")).unwrap();
    d.add_edge(StepEdge::new("b", "c")).unwrap();
    let mut order_executed = vec![];
    let mut exec = WorkflowExecutor::new(CheckpointStore::new());
    exec.run(&d, |id| {
        order_executed.push(id.to_owned());
        ok_handler(id)
    }).unwrap();
    assert_eq!(order_executed, vec!["a", "b", "c"]);
}

/// Test 40: cycle in workflow fails with DagError
#[test]
fn t40_executor_cycle_returns_dag_error() {
    let mut d = WorkflowDef::new("wf", "W");
    d.add_node(StepNode::new("a", NodeKind::AgentTask)).unwrap();
    d.add_node(StepNode::new("b", NodeKind::AgentTask)).unwrap();
    d.add_edge(StepEdge::new("a", "b")).unwrap();
    d.add_edge(StepEdge::new("b", "a")).unwrap();
    let mut exec = WorkflowExecutor::new(CheckpointStore::new());
    assert!(matches!(exec.run(&d, ok_handler).unwrap_err(), ExecutorError::DagError(_)));
}

/// Test 41: empty workflow fails with DagError
#[test]
fn t41_executor_empty_workflow_error() {
    let d = WorkflowDef::new("wf", "W");
    let mut exec = WorkflowExecutor::new(CheckpointStore::new());
    assert!(matches!(exec.run(&d, ok_handler).unwrap_err(), ExecutorError::DagError(_)));
}

/// Test 42: succeeded_steps / failed_steps counts correct
#[test]
fn t42_executor_step_counts() {
    let mut exec = WorkflowExecutor::new(CheckpointStore::new());
    let run = exec.run(&simple_def("wf"), ok_handler).unwrap();
    assert_eq!(run.succeeded_steps(), 2);
    assert_eq!(run.failed_steps(), 0);
}

/// Test 43: RunStatus terminal flags
#[test]
fn t43_run_status_terminal() {
    assert!(RunStatus::Succeeded.is_terminal());
    assert!(RunStatus::Failed { failed_step: "x".to_owned(), reason: "r".to_owned() }.is_terminal());
    assert!(!RunStatus::Running.is_terminal());
    assert!(!RunStatus::Pending.is_terminal());
}

// ═════════════════════════════════════════════════════════════════════════════
// §4  Checkpointing (tests 44–55)
// ═════════════════════════════════════════════════════════════════════════════

/// Test 44: save and query a step
#[test]
fn t44_checkpoint_save_and_query() {
    let mut store = CheckpointStore::new();
    store.save_step("wf", ck("step-a"));
    assert!(store.is_completed("wf", "step-a"));
}

/// Test 45: different workflows are isolated
#[test]
fn t45_checkpoint_isolation() {
    let mut store = CheckpointStore::new();
    store.save_step("wf1", ck("a"));
    assert!(!store.is_completed("wf2", "a"));
}

/// Test 46: get_step returns correct record
#[test]
fn t46_checkpoint_get_step() {
    let mut store = CheckpointStore::new();
    store.save_step("wf", StepCheckpoint { node_id: "x".to_owned(), attempt: 3, duration_ms: 99, output: Some("data".to_owned()) });
    let r = store.get_step("wf", "x").unwrap();
    assert_eq!(r.attempt, 3);
    assert_eq!(r.duration_ms, 99);
    assert_eq!(r.output.as_deref(), Some("data"));
}

/// Test 47: get_snapshot error when not found
#[test]
fn t47_checkpoint_snapshot_not_found() {
    let store = CheckpointStore::new();
    assert_eq!(store.get_snapshot("missing").unwrap_err(), CheckpointError::NotFound("missing".to_owned()));
}

/// Test 48: completed_steps preserves insertion order
#[test]
fn t48_checkpoint_completed_order() {
    let mut store = CheckpointStore::new();
    store.save_step("wf", ck("a"));
    store.save_step("wf", ck("b"));
    store.save_step("wf", ck("c"));
    assert_eq!(store.completed_steps("wf"), vec!["a", "b", "c"]);
}

/// Test 49: remaining steps computed correctly
#[test]
fn t49_checkpoint_remaining_steps() {
    let mut store = CheckpointStore::new();
    store.save_step("wf", ck("a"));
    let all = vec!["a".to_owned(), "b".to_owned(), "c".to_owned()];
    let rem = store.remaining_steps("wf", &all);
    assert_eq!(rem.len(), 2);
}

/// Test 50: clear_workflow removes data
#[test]
fn t50_checkpoint_clear_workflow() {
    let mut store = CheckpointStore::new();
    store.save_step("wf", ck("a"));
    store.clear_workflow("wf");
    assert!(!store.is_completed("wf", "a"));
}

/// Test 51: clear_all removes everything
#[test]
fn t51_checkpoint_clear_all() {
    let mut store = CheckpointStore::new();
    store.save_step("wf1", ck("a"));
    store.save_step("wf2", ck("b"));
    store.clear_all();
    assert_eq!(store.workflow_count(), 0);
}

/// Test 52: duplicate step silently ignored
#[test]
fn t52_checkpoint_duplicate_step_ignored() {
    let mut store = CheckpointStore::new();
    store.save_step("wf", ck("a"));
    store.save_step("wf", ck("a"));
    assert_eq!(store.completed_steps("wf").len(), 1);
}

/// Test 53: workflow_ids lists all tracked workflows
#[test]
fn t53_checkpoint_workflow_ids() {
    let mut store = CheckpointStore::new();
    store.save_step("alpha", ck("a"));
    store.save_step("beta", ck("b"));
    let mut ids = store.workflow_ids();
    ids.sort();
    assert_eq!(ids, vec!["alpha", "beta"]);
}

/// Test 54: empty store workflow_count is 0
#[test]
fn t54_checkpoint_empty_count() {
    let store = CheckpointStore::new();
    assert_eq!(store.workflow_count(), 0);
}

/// Test 55: remaining_steps empty when all done
#[test]
fn t55_checkpoint_remaining_empty_when_all_done() {
    let mut store = CheckpointStore::new();
    let all = vec!["a".to_owned(), "b".to_owned()];
    store.save_step("wf", ck("a"));
    store.save_step("wf", ck("b"));
    assert!(store.remaining_steps("wf", &all).is_empty());
}

// ═════════════════════════════════════════════════════════════════════════════
// §5  End-to-end (tests 56–70)
// ═════════════════════════════════════════════════════════════════════════════

/// Test 56: define → execute → all steps succeeded
#[test]
fn t56_e2e_define_and_execute() {
    let def = simple_def("e2e-56");
    let mut exec = WorkflowExecutor::new(CheckpointStore::new());
    let run = exec.run(&def, ok_handler).unwrap();
    assert_eq!(run.workflow_id, "e2e-56");
    assert_eq!(run.status, RunStatus::Succeeded);
}

/// Test 57: retry on first step, then success
#[test]
fn t57_e2e_retry_then_success() {
    let def = simple_def("e2e-57");
    let mut exec = WorkflowExecutor::new(CheckpointStore::new());
    exec.set_retry("a", RetryPolicy::fixed(3, 0));
    let mut attempts = 0;
    let run = exec.run(&def, |id| {
        if id == "a" {
            attempts += 1;
            if attempts < 2 { fail_handler(id) } else { ok_handler(id) }
        } else { ok_handler(id) }
    }).unwrap();
    assert_eq!(run.status, RunStatus::Succeeded);
}

/// Test 58: resume from checkpoint — only remaining steps run
#[test]
fn t58_e2e_resume_from_checkpoint() {
    let def = simple_def("e2e-58");
    let mut store = CheckpointStore::new();
    store.save_step("e2e-58", ck("a"));
    let mut exec = WorkflowExecutor::new(store);
    let mut ran = vec![];
    let run = exec.run(&def, |id| { ran.push(id.to_owned()); ok_handler(id) }).unwrap();
    assert!(!ran.contains(&"a".to_owned()), "a was checkpointed, should not run");
    assert!(ran.contains(&"b".to_owned()));
    assert_eq!(run.status, RunStatus::Succeeded);
}

/// Test 59: DAG with 4 nodes and diamond shape
#[test]
fn t59_e2e_diamond_dag() {
    let mut d = WorkflowDef::new("e2e-59", "Diamond");
    for id in ["in", "l", "r", "out"] {
        d.add_node(StepNode::new(id, NodeKind::AgentTask)).unwrap();
    }
    d.add_edge(StepEdge::new("in", "l")).unwrap();
    d.add_edge(StepEdge::new("in", "r")).unwrap();
    d.add_edge(StepEdge::new("l",  "out")).unwrap();
    d.add_edge(StepEdge::new("r",  "out")).unwrap();
    let mut exec = WorkflowExecutor::new(CheckpointStore::new());
    let run = exec.run(&d, ok_handler).unwrap();
    assert_eq!(run.status, RunStatus::Succeeded);
    assert_eq!(run.succeeded_steps(), 4);
}

/// Test 60: timeout + retry together, timeout wins
#[test]
fn t60_e2e_timeout_wins_over_retry() {
    let def = simple_def("e2e-60");
    let mut exec = WorkflowExecutor::new(CheckpointStore::new());
    exec.set_retry("a", RetryPolicy::fixed(3, 0));
    exec.set_timeout("a", TimeoutPolicy::new(50));
    let run = exec.run(&def, |id| {
        if id == "a" { StepOutcome::success(id, 200) } else { ok_handler(id) }
    }).unwrap();
    // The step "succeeds" from handler perspective but duration > timeout → fails
    assert!(matches!(run.status, RunStatus::Failed { .. }));
}

/// Test 61: step with agent_id set executes normally
#[test]
fn t61_e2e_agent_step() {
    let mut d = WorkflowDef::new("e2e-61", "W");
    d.add_node(StepNode::new("gen", NodeKind::AgentTask).with_agent("agent-v2")).unwrap();
    let mut exec = WorkflowExecutor::new(CheckpointStore::new());
    let run = exec.run(&d, ok_handler).unwrap();
    assert_eq!(run.status, RunStatus::Succeeded);
}

/// Test 62: Transform node executes normally
#[test]
fn t62_e2e_transform_node() {
    let mut d = WorkflowDef::new("e2e-62", "W");
    d.add_node(StepNode::new("resize", NodeKind::Transform)).unwrap();
    let mut exec = WorkflowExecutor::new(CheckpointStore::new());
    assert_eq!(exec.run(&d, ok_handler).unwrap().status, RunStatus::Succeeded);
}

/// Test 63: full pipeline: define → policy → execute → checkpoint verified
#[test]
fn t63_e2e_full_pipeline_checkpoint_verified() {
    let def = simple_def("e2e-63");
    let store = CheckpointStore::new();
    let mut exec = WorkflowExecutor::new(store);
    exec.run(&def, ok_handler).unwrap();
    // After run, checkpoint store should have both steps recorded
    assert!(exec.checkpoint_store().is_completed("e2e-63", "a"));
    assert!(exec.checkpoint_store().is_completed("e2e-63", "b"));
}

/// Test 64: five-node linear chain executes completely
#[test]
fn t64_e2e_five_node_chain() {
    let mut d = WorkflowDef::new("e2e-64", "W");
    for id in ["a", "b", "c", "d", "e"] {
        d.add_node(StepNode::new(id, NodeKind::AgentTask)).unwrap();
    }
    d.add_edge(StepEdge::new("a", "b")).unwrap();
    d.add_edge(StepEdge::new("b", "c")).unwrap();
    d.add_edge(StepEdge::new("c", "d")).unwrap();
    d.add_edge(StepEdge::new("d", "e")).unwrap();
    let mut exec = WorkflowExecutor::new(CheckpointStore::new());
    let run = exec.run(&d, ok_handler).unwrap();
    assert_eq!(run.succeeded_steps(), 5);
}

/// Test 65: two parallel branches both succeed
#[test]
fn t65_e2e_parallel_branches() {
    let mut d = WorkflowDef::new("e2e-65", "W");
    for id in ["start", "branch_a", "branch_b"] {
        d.add_node(StepNode::new(id, NodeKind::AgentTask)).unwrap();
    }
    d.add_edge(StepEdge::new("start", "branch_a")).unwrap();
    d.add_edge(StepEdge::new("start", "branch_b")).unwrap();
    let mut exec = WorkflowExecutor::new(CheckpointStore::new());
    let run = exec.run(&d, ok_handler).unwrap();
    assert_eq!(run.status, RunStatus::Succeeded);
    assert_eq!(run.succeeded_steps(), 3);
}

/// Test 66: step description and label accessible
#[test]
fn t66_dag_step_description() {
    let node = StepNode::new("gen", NodeKind::AgentTask).with_description("Generate assets");
    assert_eq!(node.description, "Generate assets");
}

/// Test 67: edge label accessible
#[test]
fn t67_dag_edge_label() {
    let edge = StepEdge::new("a", "b").with_label("on_success");
    assert_eq!(edge.label.as_deref(), Some("on_success"));
}

/// Test 68: multiple policies on different steps
#[test]
fn t68_e2e_multi_policy() {
    let mut d = WorkflowDef::new("e2e-68", "W");
    for id in ["a", "b", "c"] {
        d.add_node(StepNode::new(id, NodeKind::AgentTask)).unwrap();
    }
    d.add_edge(StepEdge::new("a", "b")).unwrap();
    d.add_edge(StepEdge::new("b", "c")).unwrap();
    let mut exec = WorkflowExecutor::new(CheckpointStore::new());
    exec.set_retry("a", RetryPolicy::fixed(2, 0));
    exec.set_retry("b", RetryPolicy::exponential(2, 10));
    exec.set_timeout("c", TimeoutPolicy::new(1000));
    let run = exec.run(&d, ok_handler).unwrap();
    assert_eq!(run.status, RunStatus::Succeeded);
}

/// Test 69: webhook node executes correctly
#[test]
fn t69_e2e_webhook_node() {
    let mut d = WorkflowDef::new("e2e-69", "W");
    d.add_node(StepNode::new("hook", NodeKind::Webhook)).unwrap();
    let mut exec = WorkflowExecutor::new(CheckpointStore::new());
    assert_eq!(exec.run(&d, ok_handler).unwrap().status, RunStatus::Succeeded);
}

/// Test 70: decision node with fan-out executes all branches
#[test]
fn t70_e2e_decision_fanout() {
    let mut d = WorkflowDef::new("e2e-70", "W");
    d.add_node(StepNode::new("decide", NodeKind::Decision)).unwrap();
    d.add_node(StepNode::new("yes",    NodeKind::AgentTask)).unwrap();
    d.add_node(StepNode::new("no",     NodeKind::AgentTask)).unwrap();
    d.add_edge(StepEdge::new("decide", "yes").with_label("true")).unwrap();
    d.add_edge(StepEdge::new("decide", "no").with_label("false")).unwrap();
    let mut exec = WorkflowExecutor::new(CheckpointStore::new());
    let run = exec.run(&d, ok_handler).unwrap();
    assert_eq!(run.succeeded_steps(), 3);
}
