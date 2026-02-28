//! Integration tests for Phase 15.4 — Multi-Agent Collaboration
//!
//! Each test exercises an end-to-end scenario spanning multiple modules.

use logos_multi_agent::*;

// ─────────────────────────────────────────────────────────────────────────────
// Helper builders
// ─────────────────────────────────────────────────────────────────────────────

fn build_standard_team() -> AgentTeam {
    let mut team = AgentTeam::new("team-main", "Main Design Team", 0);

    team.add_member(TeamMember::new(
        "senior-1", "Alice (Senior)", AgentRole::Senior,
        vec![TaskKind::ReviewQuality, TaskKind::DesignLayout, TaskKind::ExportAsset], 0,
    ));
    team.add_member(TeamMember::new(
        "junior-layout", "Bob (Layout)", AgentRole::Junior,
        vec![TaskKind::DesignLayout, TaskKind::GroupLayers], 0,
    ));
    team.add_member(TeamMember::new(
        "junior-color", "Carol (Color)", AgentRole::Junior,
        vec![TaskKind::ApplyColors, TaskKind::SetTypography], 0,
    ));
    team.add_member(TeamMember::new(
        "specialist-a11y", "Dan (A11y)", AgentRole::Specialist("Accessibility".into()),
        vec![TaskKind::CheckAccessibility, TaskKind::ReviewQuality], 0,
    ));
    team.add_member(TeamMember::new(
        "reviewer-1", "Eve (Reviewer)", AgentRole::Reviewer,
        vec![TaskKind::ReviewQuality, TaskKind::CheckAccessibility], 0,
    ));
    team
}

fn default_oversight() -> OversightManager {
    OversightManager::new(OversightPolicy::default())
}

// ─────────────────────────────────────────────────────────────────────────────
// Integration scenarios
// ─────────────────────────────────────────────────────────────────────────────

/// Scenario 1: Full task decompose → assign to team → complete lifecycle
#[test]
fn scenario_decompose_assign_complete_lifecycle() {
    let tasks = TaskDecomposer::decompose("Design a login screen with accessibility", 0);
    assert!(!tasks.is_empty(), "Decomposer should produce at least one task");

    let mut team = build_standard_team();
    let mut coord = Coordinator::new();

    for task in tasks {
        coord.enqueue(task);
    }

    let result = coord.dispatch_next(&mut team, 1000);
    assert!(result.is_some(), "Coordinator should dispatch first task");
    let (task_id, agent_id) = result.unwrap();
    assert!(!task_id.is_empty());
    assert!(!agent_id.is_empty());

    coord.update_progress(&task_id, &agent_id, 100, "done", 2000);
    let p = coord.get_progress(&task_id).unwrap();
    assert_eq!(p.percent, 100);
}

/// Scenario 2: Conflict detection when two agents touch the same layer
#[test]
fn scenario_conflict_detected_shared_layer() {
    let mut coord = Coordinator::new();
    let shared_layer = "layer-header";

    let mut t1 = SubTask::new(TaskKind::DesignLayout, "Header layout", TaskPriority::High, 0)
        .with_layers(&[shared_layer]);
    let mut t2 = SubTask::new(TaskKind::ApplyColors, "Header colors", TaskPriority::Normal, 1)
        .with_layers(&[shared_layer]);

    t1.assign("junior-layout");
    t2.assign("junior-color");
    let t2_id = t2.id.clone();

    coord.task_queue.insert(t1);
    coord.task_queue.push(t2);

    let conflict = coord.detect_pending_conflict(&t2_id, "junior-color");
    assert!(conflict.is_some(), "Conflict should be detected for shared layers");
    let c = conflict.unwrap();
    assert!(c.layer_ids.contains(&shared_layer.to_string()));
}

/// Scenario 3: Senior approves Junior work before output is finalized
#[test]
fn scenario_senior_approves_junior_work() {
    let mut oversight = default_oversight();
    let req = ApprovalRequest::new("task-layout-01", "junior-layout", "Layout completed", 100)
        .with_quality(0.88);
    let req_id = oversight.submit_for_approval(req).req_id.clone();

    assert_eq!(oversight.pending_count(), 1);
    assert!(oversight.approve(&req_id, "senior-1", 200));
    assert_eq!(oversight.pending_count(), 0);
    assert_eq!(oversight.approved_count(), 1);

    let approved = oversight.get_request(&req_id).unwrap();
    assert!(approved.status.is_approved());
}

/// Scenario 4: Quality check gates task completion
#[test]
fn scenario_quality_check_gates_completion() {
    let mut oversight = default_oversight();

    let criteria = vec![
        QualityCriterion::pass("contrast-ratio", 0.95),
        QualityCriterion::pass("layer-naming", 1.0),
        QualityCriterion::fail("export-resolution", "Too low DPI"),
    ];
    let check = oversight.run_quality_check("task-export-01", "junior-layout", criteria, 100);
    assert!(!check.passed, "Check with failed criterion should not pass");
    assert!(check.overall_score < 1.0);

    // Task should require retry or senior override
    let all_pass = vec![
        QualityCriterion::pass("contrast-ratio", 0.95),
        QualityCriterion::pass("layer-naming", 1.0),
        QualityCriterion::pass("export-resolution", 1.0),
    ];
    let check2 = oversight.run_quality_check("task-export-01", "junior-layout", all_pass, 200);
    assert!(check2.passed);

    // Latest check is the second one
    let latest = oversight.latest_check_for("task-export-01").unwrap();
    assert_eq!(latest.timestamp_secs, 200);
}

/// Scenario 5: Auto-approval when quality score exceeds threshold
#[test]
fn scenario_auto_approval_high_quality() {
    let policy = OversightPolicy {
        auto_approve_threshold: 0.90,
        ..Default::default()
    };
    let mut oversight = OversightManager::new(policy);

    let req = ApprovalRequest::new("task-colors-01", "junior-color", "Colors applied", 0)
        .with_quality(0.93);
    let req_id = oversight.submit_for_approval(req).req_id.clone();

    assert!(oversight.auto_approve_if_eligible(&req_id));
    let r = oversight.get_request(&req_id).unwrap();
    assert!(matches!(&r.status, ApprovalStatus::AutoApproved { .. }));
}

/// Scenario 6: Critical task unblocks before Normal in priority queue
#[test]
fn scenario_critical_task_dispatches_first() {
    let mut queue = TaskQueue::new();
    queue.push(SubTask::new(TaskKind::GroupLayers, "Low task", TaskPriority::Low, 100));
    queue.push(SubTask::new(TaskKind::ApplyColors, "Normal task", TaskPriority::Normal, 100));
    queue.push(SubTask::new(TaskKind::ExportAsset, "CRITICAL export", TaskPriority::Critical, 100));

    let first = queue.pop_pending().unwrap();
    assert_eq!(first.priority, TaskPriority::Critical);

    let second = queue.pop_pending().unwrap();
    assert_eq!(second.priority, TaskPriority::Normal);
}

/// Scenario 7: Senior broadcasts status message to all team members
#[test]
fn scenario_senior_broadcasts_to_team() {
    let mut coord = Coordinator::new();
    let team = build_standard_team();

    let member_ids: Vec<String> = vec![
        "junior-layout".into(),
        "junior-color".into(),
        "specialist-a11y".into(),
        "reviewer-1".into(),
    ];

    for member_id in &member_ids {
        coord.send_message(
            "senior-1",
            member_id,
            MessageContent::StatusBroadcast { message: "Stand-by for sprint review".into() },
            1000,
        );
    }
    assert_eq!(coord.total_messages(), member_ids.len());

    // Each member has one unread message
    for member_id in &member_ids {
        assert_eq!(coord.unacknowledged_for(member_id).len(), 1);
    }

    // Acknowledge for one member
    coord.acknowledge_all_for("junior-layout");
    assert_eq!(coord.unacknowledged_for("junior-layout").len(), 0);
    assert_eq!(coord.unacknowledged_for("junior-color").len(), 1);

    drop(team); // team used for context only
}

/// Scenario 8: Retry after Senior rejection
#[test]
fn scenario_retry_after_rejection() {
    let mut oversight = default_oversight();

    // First submission — rejected
    let req1 = ApprovalRequest::new("task-code-01", "junior-layout", "First attempt", 100);
    let req1_id = oversight.submit_for_approval(req1).req_id.clone();
    oversight.reject(&req1_id, "senior-1", "Spacing inconsistent", 200);

    assert_eq!(oversight.rejected_count(), 1);
    assert!(oversight.should_retry(0), "Should retry after first rejection");

    // Second submission — approved
    let req2 = ApprovalRequest::new("task-code-01", "junior-layout", "Second attempt (fixed spacing)", 300)
        .with_quality(0.91);
    let req2_id = oversight.submit_for_approval(req2).req_id.clone();
    oversight.approve(&req2_id, "senior-1", 400);
    assert_eq!(oversight.approved_count(), 1);
}

/// Scenario 9: Specialist handles accessibility sub-task in mixed team
#[test]
fn scenario_specialist_handles_a11y_task() {
    let team = build_standard_team();
    let a11y_task_kind = TaskKind::CheckAccessibility;
    let best = team.find_best_for(&a11y_task_kind).unwrap();
    // Both specialist and reviewer can handle accessibility; pick highest success rate (both 1.0)
    assert!(
        best.agent_id == "specialist-a11y" || best.agent_id == "reviewer-1",
        "A capable agent should handle accessibility"
    );
}

/// Scenario 10: Full pipeline — decompose → assign → coordinate → oversight → complete
#[test]
fn scenario_full_pipeline() {
    let mut team = build_standard_team();
    let mut coord = Coordinator::new();
    let mut oversight = default_oversight();

    // 1. Decompose goal
    let tasks = TaskDecomposer::decompose("Design a dashboard with accessibility and export to SVG", 0);
    let task_count = tasks.len();
    assert!(task_count > 2, "Should decompose into multiple tasks");

    // 2. Enqueue all
    for task in tasks { coord.enqueue(task); }

    // 3. Dispatch until queue exhausted or no agent available
    let mut dispatched: Vec<(String, String)> = Vec::new();
    for _ in 0..task_count {
        if let Some(pair) = coord.dispatch_next(&mut team, 1000) {
            dispatched.push(pair);
        } else {
            break; // Team is at capacity
        }
    }

    // At least one dispatch must have succeeded
    assert!(!dispatched.is_empty(), "At least one task should be dispatched");

    // 4. Progress updates
    for (task_id, agent_id) in &dispatched {
        coord.update_progress(task_id, agent_id, 100, "complete", 2000);
    }

    // 5. Oversight for tasks requiring approval
    for (task_id, agent_id) in &dispatched {
        let req = ApprovalRequest::new(task_id, agent_id, "Task output ready", 2100)
            .with_quality(0.96);
        let req_id = oversight.submit_for_approval(req).req_id.clone();
        oversight.auto_approve_if_eligible(&req_id);
    }

    // All high-quality tasks should be auto-approved
    assert!(oversight.approved_count() > 0, "At least one task should be auto-approved");
}
