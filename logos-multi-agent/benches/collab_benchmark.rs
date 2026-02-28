use criterion::{black_box, criterion_group, criterion_main, Criterion};
use logos_multi_agent::*;

fn bench_task_decompose(c: &mut Criterion) {
    c.bench_function("task_decompose_complex_goal", |b| {
        b.iter(|| {
            TaskDecomposer::decompose(
                black_box("Design an accessible dashboard with colors, typography, and export to SVG"),
                0,
            )
        })
    });
}

fn bench_task_queue_push_pop(c: &mut Criterion) {
    c.bench_function("task_queue_1000_push_pop", |b| {
        b.iter(|| {
            let mut q = TaskQueue::new();
            for i in 0u64..1000 {
                let priority = match i % 4 {
                    0 => TaskPriority::Critical,
                    1 => TaskPriority::High,
                    2 => TaskPriority::Normal,
                    _ => TaskPriority::Low,
                };
                q.push(SubTask::new(
                    TaskKind::ApplyColors,
                    format!("task-{}", i),
                    priority,
                    i,
                ));
            }
            while q.pop_pending().is_some() {}
        })
    });
}

fn bench_team_find_best_for(c: &mut Criterion) {
    let mut team = AgentTeam::new("bench-team", "Bench", 0);
    for i in 0..100u32 {
        team.add_member(TeamMember::new(
            format!("agent-{}", i),
            format!("Agent {}", i),
            AgentRole::Junior,
            vec![TaskKind::ApplyColors, TaskKind::DesignLayout],
            0,
        ));
    }

    c.bench_function("team_find_best_100_members", |b| {
        b.iter(|| team.find_best_for(black_box(&TaskKind::ApplyColors)))
    });
}

fn bench_coordinator_dispatch(c: &mut Criterion) {
    c.bench_function("coordinator_dispatch_50_tasks", |b| {
        b.iter(|| {
            let mut coord = Coordinator::new();
            let mut team = AgentTeam::new("t", "T", 0);
            for j in 0..50usize {
                team.add_member(TeamMember::new(
                    format!("agent-{}", j),
                    format!("A{}", j),
                    AgentRole::Junior,
                    vec![TaskKind::DesignLayout],
                    0,
                ));
                coord.enqueue(SubTask::new(TaskKind::DesignLayout, format!("task-{}", j), TaskPriority::Normal, j as u64));
            }
            for _ in 0..50 {
                coord.dispatch_next(&mut team, 0);
            }
        })
    });
}

fn bench_oversight_auto_approve(c: &mut Criterion) {
    c.bench_function("oversight_auto_approve_200_requests", |b| {
        b.iter(|| {
            let mut mgr = OversightManager::new(OversightPolicy {
                auto_approve_threshold: 0.85,
                ..Default::default()
            });
            let mut req_ids = Vec::new();
            for i in 0..200u64 {
                let req = ApprovalRequest::new(format!("task-{}", i), "agent-x", "done", i)
                    .with_quality(0.90);
                let req_id = mgr.submit_for_approval(req).req_id.clone();
                req_ids.push(req_id);
            }
            for req_id in &req_ids {
                mgr.auto_approve_if_eligible(req_id);
            }
        })
    });
}

criterion_group!(
    benches,
    bench_task_decompose,
    bench_task_queue_push_pop,
    bench_team_find_best_for,
    bench_coordinator_dispatch,
    bench_oversight_auto_approve,
);
criterion_main!(benches);
