use criterion::{black_box, criterion_group, criterion_main, Criterion};
use logos_prompt_engine::*;

fn bench_template_render(c: &mut Criterion) {
    let mut reg = TemplateRegistry::new();
    reg.register(
        "task",
        "You are {{agent}} working on {{task}} for {{client}}. Use {{platform}} at {{viewport}}.",
    );
    let vars = PromptVariables::new()
        .set("agent", "Logos Design Agent")
        .set("task", "dashboard redesign")
        .set("client", "ACME Corp")
        .set("platform", "web")
        .set("viewport", "1440 px");

    c.bench_function("template_render_5_vars", |b| {
        b.iter(|| reg.render(black_box("task"), black_box(&vars)))
    });
}

fn bench_few_shot_find_by_domain(c: &mut Criterion) {
    let lib = ExampleLibrary::with_builtins();
    c.bench_function("few_shot_find_by_domain", |b| {
        b.iter(|| lib.find_by_domain(black_box(&TaskDomain::Accessibility), 10))
    });
}

fn bench_few_shot_inject_into(c: &mut Criterion) {
    let lib = ExampleLibrary::with_builtins();
    let examples = lib.find_by_domain(&TaskDomain::Layout, 3);

    c.bench_function("few_shot_inject_3_examples", |b| {
        b.iter(|| {
            let base = Prompt::new()
                .system("System.")
                .user("Design a layout.");
            lib.inject_into(black_box(base), black_box(&examples))
        })
    });
}

fn bench_cot_parse(c: &mut Criterion) {
    let raw = "\
Step 1: Analyse
Analyse the request in detail.\n\
Step 2: Design
Choose the appropriate layout system.\n\
Step 3: Implement
Apply the design to the canvas.\n\
Conclusion: Three-step design process complete.";

    c.bench_function("cot_parse_3_steps", |b| {
        b.iter(|| CotParser::parse(black_box(raw)))
    });
}

fn bench_refinement_session_100_rounds(c: &mut Criterion) {
    c.bench_function("refinement_100_rounds", |b| {
        b.iter(|| {
            let config = RefinementConfig { max_rounds: 100, require_improvement: false, early_stop_patience: 5 };
            let mut session = RefinementSession::new("bench", "Design a button", config);
            session.start("Initial design.", 0);
            for i in 1u64..=100 {
                session.add_round(format!("Improved v{}", i), "Critique.", true, i);
            }
            session.finalize();
        })
    });
}

criterion_group!(
    benches,
    bench_template_render,
    bench_few_shot_find_by_domain,
    bench_few_shot_inject_into,
    bench_cot_parse,
    bench_refinement_session_100_rounds,
);
criterion_main!(benches);
