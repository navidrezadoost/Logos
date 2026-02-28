use criterion::{black_box, criterion_group, criterion_main, Criterion};
use logos_ai_agent::{
    BuiltinTestSuite, TestRunner,
    PromptGenerator, TrainingOrchestrator,
    CommandParser, Evaluator,
    UxAgent, UxState, UxAction,
};

fn bench_test_suite_run(c: &mut Criterion) {
    let suite = BuiltinTestSuite::build();
    c.bench_function("full_test_suite_evaluate", |b| {
        b.iter(|| {
            suite.cases.iter().map(|case| {
                let resp = case.expected_keywords.join(" ");
                TestRunner::evaluate(black_box(&resp), black_box(case), 100)
            }).count()
        })
    });
}

fn bench_curriculum_generation(c: &mut Criterion) {
    let gen = PromptGenerator::default();
    c.bench_function("generate_curriculum", |b| {
        b.iter(|| {
            black_box(gen.generate_curriculum())
        })
    });
}

fn bench_training_pipeline(c: &mut Criterion) {
    let gen = PromptGenerator::default();
    let curriculum = gen.generate_curriculum();
    let orch = TrainingOrchestrator::default();
    c.bench_function("training_pipeline_mock", |b| {
        b.iter(|| {
            black_box(orch.run("bench-session", &curriculum, 0).unwrap())
        })
    });
}

fn bench_command_parser(c: &mut Criterion) {
    let commands = vec![
        "Create a rectangle at x=100 y=200 width=300 height=150",
        "Set fill to #3b82f6 on layer 'button'",
        "Move 'icon' to x=50 y=50",
        "Generate a complementary palette from #ff5733",
        "Check accessibility and WCAG contrast",
        "Set opacity to 75% on 'overlay'",
        "Delete layer 'old-bg'",
        "undo",
        "help plugins",
    ];
    c.bench_function("parse_commands_batch", |b| {
        b.iter(|| {
            commands.iter().map(|cmd| {
                black_box(CommandParser::parse(black_box(cmd)))
            }).count()
        })
    });
}

fn bench_evaluator(c: &mut Criterion) {
    let suite = BuiltinTestSuite::build();
    let results: Vec<_> = suite.cases.iter().map(|case| {
        let resp = case.expected_keywords.join(" ");
        TestRunner::evaluate(&resp, case, 100)
    }).collect();
    let eval = Evaluator::default();
    c.bench_function("evaluate_50_results", |b| {
        b.iter(|| {
            black_box(eval.evaluate(
                black_box(&results),
                black_box(&suite),
                "bench-sess",
                0,
            ))
        })
    });
}

fn bench_rl_ux_observe(c: &mut Criterion) {
    c.bench_function("rl_ux_observe_100", |b| {
        b.iter(|| {
            let mut agent = UxAgent::default();
            let state = UxState::with_selection(1);
            let next = state.clone();
            for i in 0..100u64 {
                agent.observe(
                    black_box(state.clone()),
                    black_box(UxAction::SetFill),
                    black_box(next.clone()),
                    i,
                );
            }
            black_box(agent.observation_count())
        })
    });
}

criterion_group!(
    benches,
    bench_test_suite_run,
    bench_curriculum_generation,
    bench_training_pipeline,
    bench_command_parser,
    bench_evaluator,
    bench_rl_ux_observe,
);
criterion_main!(benches);
