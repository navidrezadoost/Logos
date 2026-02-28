use criterion::{black_box, criterion_group, criterion_main, Criterion};
use logos_rl_ux::{
    q_table::{QTable, StateKey},
    data_collector::{DataCollector, InteractionEvent},
    ab_testing::{Experiment, ExperimentConfig, TrafficSplit, ExperimentVariant},
    policy_engine::{Feedback, PolicyConfig, PolicyEngine, PolicyVariant, PredictionRequest},
};

fn bench_q_table_update(c: &mut Criterion) {
    let mut q = QTable::new(0.1, 0.9, 1000);
    let actions: &[&str] = &["CreateLayer", "SetFill", "DeleteLayer", "GroupLayers", "MoveLayer"];
    c.bench_function("q_table_update", |b| {
        let mut ts = 0u64;
        b.iter(|| {
            let state = StateKey::new(black_box(1), 100.0, "select", false, false, 0);
            let next = StateKey::new(black_box(2), 100.0, "select", false, false, 0);
            q.update(&state, black_box("SetFill"), black_box(0.8), &next, actions, ts);
            ts += 1;
            black_box(q.total_updates())
        });
    });
}

fn bench_q_table_predict(c: &mut Criterion) {
    let mut q = QTable::new(0.1, 0.9, 10_000);
    let actions: &[&str] = &["CreateLayer", "SetFill", "DeleteLayer", "GroupLayers", "MoveLayer",
        "ResizeLayer", "UndoAction", "OpenColorPicker", "ExportDesign", "CheckAccessibility"];

    // Warm up Q-table
    let base = StateKey::new(1, 100.0, "select", false, false, 0);
    let next = StateKey::new(2, 100.0, "select", false, false, 0);
    for i in 0..500u64 {
        q.update(&base, "SetFill", 0.9, &next, actions, i);
        q.update(&base, "CreateLayer", 0.3, &next, actions, i);
    }

    c.bench_function("q_table_best_action", |b| {
        b.iter(|| {
            let state = StateKey::new(black_box(1), 100.0, "select", false, false, 0);
            black_box(q.best_action(&state, actions))
        });
    });
}

fn bench_data_collector_record(c: &mut Criterion) {
    let mut collector = DataCollector::default();
    c.bench_function("data_collector_record", |b| {
        let mut ts = 0u64;
        b.iter(|| {
            let event = InteractionEvent::new(
                black_box("session-1"),
                black_box("SetFill"),
                black_box(r#"{"sel":1}"#),
                ts,
            ).with_duration(50);
            black_box(collector.record(event));
            ts += 1;
        });
    });
}

fn bench_ab_traffic_split(c: &mut Criterion) {
    let split = TrafficSplit::new(0.5);
    c.bench_function("ab_traffic_split_assign", |b| {
        let mut i = 0u32;
        b.iter(|| {
            let session = format!("user-{}", i);
            i += 1;
            black_box(split.assign(&session))
        });
    });
}

fn bench_policy_predict(c: &mut Criterion) {
    let config = PolicyConfig {
        variant: PolicyVariant::BlendedPolicy { confidence_threshold: 5 },
        min_confidence_to_show: 0.0,
        ..Default::default()
    };
    let mut engine = PolicyEngine::new(config);

    // Warm up heuristic
    for _ in 0..100 { engine.heuristic.record("SetFill"); }
    for _ in 0..50 { engine.heuristic.record("CreateLayer"); }

    c.bench_function("policy_engine_predict", |b| {
        let mut ts = 0u64;
        b.iter(|| {
            let state = StateKey::new(black_box(1), 100.0, "select", false, false, 0);
            let req = PredictionRequest::new(black_box("session-1"), state, ts);
            ts += 1;
            black_box(engine.predict(&req).count())
        });
    });
}

criterion_group!(
    benches,
    bench_q_table_update,
    bench_q_table_predict,
    bench_data_collector_record,
    bench_ab_traffic_split,
    bench_policy_predict,
);
criterion_main!(benches);
