//! Integration tests for Phase 15.5 — Advanced Prompt Engineering
//!
//! Each scenario exercises an end-to-end workflow spanning multiple modules.

#![allow(unused_imports)]

use logos_prompt_engine::*;
use logos_prompt_engine::generator::{PromptGenerator, TaskSpec, select_cot_strategy};
use logos_prompt_engine::training::{
    RubricCriterion, RubricEvaluator, TrainingConfig, TrainingSession,
};

// ─────────────────────────────────────────────────────────────────────────────
// Scenario 1: Few-shot → Prompt injection → CoT wrapping → Refinement pipeline
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn scenario_full_engineering_pipeline() {
    // Step 1: Build base prompt with template
    let mut reg = TemplateRegistry::new();
    reg.register(
        "design-task",
        "You are an expert Logos design agent. Your task: {{task}}. \
         Target viewport: {{viewport}}.",
    );
    let vars = PromptVariables::new()
        .set("task", "design an onboarding flow")
        .set("viewport", "1280 px");
    let system_text = reg.render("design-task", &vars).unwrap();

    // Step 2: Inject few-shot examples
    let lib = ExampleLibrary::with_builtins();
    let examples = lib.find_by_domain(&TaskDomain::Layout, 2);
    let base = Prompt::new()
        .system(&system_text)
        .user("Design the first onboarding screen.");
    let prompted = lib.inject_into(base, &examples);

    assert!(prompted.message_count() > 2, "Should have system + examples + user");
    assert_eq!(prompted.messages[0].role, Role::System);

    // Step 3: Wrap with CoT
    let wrapped = CotInstruction::new(CotStrategy::StepByStep).wrap(prompted);
    assert!(wrapped.system_messages()[0].content.contains("Step 1"));

    // Step 4: Simulate agent response and start refinement
    let simulated_response = "I'll create a welcome screen with a hero image and CTA button.";
    let mut session = RefinementSession::new("sess-pipeline", "Design onboarding screen", RefinementConfig::default());
    session.start(simulated_response, 1000);
    assert_eq!(session.round_count(), 1);

    // Step 5: Add a refinement round
    session.add_round(
        "I'll create a welcome screen with a hero image (1280×640), \
         headline 'Get Started', sub-text, and 'Continue' CTA button with primary blue.",
        "Added dimensions, specific copy, and button label.",
        true,
        2000,
    );
    session.finalize();

    assert!(session.final_response.is_some());
    assert!(session.best_response().unwrap().contains("Continue"));
}

// ─────────────────────────────────────────────────────────────────────────────
// Scenario 2: Template registry drives multiple prompt variants
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn scenario_template_registry_multiple_variants() {
    let mut reg = TemplateRegistry::new();
    reg.register("layout",  "Design a {{type}} layout for {{platform}}.");
    reg.register("colours", "Apply a {{mood}} colour palette for {{brand}}.");
    reg.register("export",  "Export {{layer}} as {{format}} at {{scale}}x scale.");

    assert_eq!(reg.count(), 3);

    let layout = reg.render("layout",
        &PromptVariables::new().set("type", "dashboard").set("platform", "web")).unwrap();
    assert!(layout.contains("dashboard"));
    assert!(layout.contains("web"));

    let export = reg.render("export",
        &PromptVariables::new().set("layer", "Icons").set("format", "SVG").set("scale", "2")).unwrap();
    assert!(export.contains("2x scale"));
}

// ─────────────────────────────────────────────────────────────────────────────
// Scenario 3: Chain-of-thought parse then re-render
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn scenario_cot_parse_and_rerender() {
    let raw = "\
Step 1: Understand the request
The user wants a dark-mode dashboard.

Step 2: Choose color palette
Use deep navy surfaces (#0F172A) with slate text (#F1F5F9).

Step 3: Apply typography
Inter 16 px body, 24 px heading.

Conclusion: Dark navy dashboard with slate text and Inter typography.";

    let cot = CotParser::parse(raw).expect("Valid CoT response");
    assert_eq!(cot.step_count(), 3);
    assert!(cot.has_conclusion());
    let text = cot.to_text();
    assert!(text.contains("Step 1:"));
    assert!(text.contains("Conclusion:"));
}

// ─────────────────────────────────────────────────────────────────────────────
// Scenario 4: Self-critique refinement loop
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn scenario_self_critique_loop() {
    let config = RefinementConfig { max_rounds: 3, require_improvement: true, early_stop_patience: 2 };
    let mut session = RefinementSession::new("sess-critique", "Create accessible icon set", config);

    // Initial response
    session.start("I'll make 24 icons in blue.", 0);

    // Critique prompt should reference the task and response
    let critique_prompt = session.next_critique_prompt().unwrap();
    assert!(critique_prompt.contains("Create accessible icon set"));
    assert!(critique_prompt.contains("I'll make 24 icons in blue."));

    // Round 1 — improved
    session.add_round(
        "24 icons, 24×24 px, colour-blind safe blue #2563EB, stroke 2 px, aria-label on each.",
        "Added dimensions, colour-blind safe colour, a11y labels.",
        true, 1,
    );

    // Round 2 — no improvement
    session.add_round("Same as above.", "No change.", false, 2);
    // Round 3 — still no improvement → early stop triggered
    session.add_round("Same as above.", "No change.", false, 3);
    assert!(session.is_done());
}

// ─────────────────────────────────────────────────────────────────────────────
// Scenario 5: Few-shot difficulty progression
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn scenario_few_shot_difficulty_graduation() {
    let lib = ExampleLibrary::with_builtins();

    // For a new-user context: inject easy examples only
    let easy = lib.best_for(&TaskDomain::Layout, Difficulty::Easy, 2);
    assert!(easy.iter().all(|e| e.difficulty == Difficulty::Easy));

    // For an advanced user: use best available (returns easy first due to sorting)
    let advanced = lib.find_by_domain(&TaskDomain::Accessibility, 10);
    assert!(!advanced.is_empty());
    // Sorted easy-first, so if mixed difficulties, easy should be index 0
    for w in advanced.windows(2) {
        assert!(w[0].difficulty <= w[1].difficulty, "Should be sorted easiest first");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Scenario 6: Human feedback improves aggregate score
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn scenario_feedback_collection() {
    let mut store = FeedbackStore::new();

    store.add(FeedbackAnnotation::new("session-A", 0, "user-1", 0.6, "Basic but useful.", 100));
    store.add(FeedbackAnnotation::new("session-A", 1, "user-1", 0.85, "Much better with examples.", 200));
    store.add(FeedbackAnnotation::new("session-A", 2, "user-2", 0.95, "Excellent final output.", 300));

    let avg = store.average_score_for("session-A").unwrap();
    assert!(avg > 0.7, "Average score should reflect improvements");
    assert_eq!(store.for_session("session-A").len(), 3);
}

// ─────────────────────────────────────────────────────────────────────────────
// Scenario 7: Prompt token estimation stays within limits
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn scenario_token_budget_not_exceeded() {
    let lib = ExampleLibrary::with_builtins();
    let examples = lib.find_by_domain(&TaskDomain::Colors, 3);

    let base = Prompt::new()
        .system("You are a design agent.")
        .user("Apply a comprehensive brand palette.")
        .with_max_tokens(4096);

    let injected = lib.inject_into(base, &examples);
    // Current impl uses 4-chars-per-token estimate; verify we have something reasonable
    assert!(injected.estimated_tokens() > 0);
    assert!(injected.estimated_tokens() < 4096);
}

// ─────────────────────────────────────────────────────────────────────────────
// Scenario 8: CoT + few-shot combined for accessibility task
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn scenario_cot_plus_few_shot_accessibility() {
    let lib = ExampleLibrary::with_builtins();
    let a11y_examples = lib.find_by_domain(&TaskDomain::Accessibility, 2);

    let base = Prompt::new()
        .system("You are an accessibility expert agent.")
        .user("Audit the current design for WCAG 2.1 AA compliance.");

    // Inject examples first
    let with_examples = lib.inject_into(base, &a11y_examples);

    // Then wrap with CoT
    let final_prompt = CotInstruction::new(CotStrategy::StepByStep).wrap(with_examples);

    // System message should have CoT instructions
    let sys = final_prompt.system_messages();
    assert!(!sys.is_empty());
    assert!(sys[0].content.contains("Step 1") || sys[0].content.contains("step"));

    // Last message should be the original user task
    let last = final_prompt.messages.last().unwrap();
    assert!(last.content.contains("WCAG"));
}

// ─────────────────────────────────────────────────────────────────────────────
// Scenario 9: Refinement session finalize + report
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn scenario_refinement_finalize_report() {
    let mut session = RefinementSession::new(
        "sess-export", "Export design assets for production", RefinementConfig::default()
    );
    session.start("Export everything as PNG at 1x.", 0);
    session.add_round(
        "Export all 47 layers as PNG at 1x and 2x, naming convention: \
         component-variant@2x.png.  SVG for icons.", "Added 2x, naming.", true, 1,
    );
    session.add_round(
        "Export all 47 layers as PNG at 1x and 2x, and SVG icons. \
         Naming: component-variant@{scale}.{ext}. Include light/dark variants.",
        "Added light/dark variants.", true, 2,
    );
    session.finalize();

    assert!(session.final_response.is_some());
    assert!(session.final_response.as_deref().unwrap().contains("light/dark"));
    assert_eq!(session.improvement_trajectory().iter().filter(|&&i| i).count(), 3);
}

// ─────────────────────────────────────────────────────────────────────────────
// Scenario 10: Focused critique template for specific issue
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn scenario_focused_critique_specific_issue() {
    let task = "Design a navigation bar";
    let response = "Blue navbar with logo on left and links on right.";
    let issue = "Missing mobile responsive behaviour";

    let prompt = CritiqueTemplate::build_focused_prompt(task, response, issue);
    assert!(prompt.contains("Missing mobile responsive behaviour"));
    assert!(prompt.contains("Design a navigation bar"));
    assert!(prompt.contains("Improved Response:"));
}

// ─────────────────────────────────────────────────────────────────────────────
// NEW PHASE 15.5 TESTS — PromptGenerator, Training, advanced few-shot
// ─────────────────────────────────────────────────────────────────────────────

// ── PromptGenerator integration ──────────────────────────────────────────────

#[test]
fn generator_end_to_end_layout_medium() {
    let gen = PromptGenerator::new();
    let spec = TaskSpec::new(
        "Design a responsive dashboard",
        TaskDomain::Layout,
        Difficulty::Medium,
    );
    let result = gen.generate(&spec);
    // Should have at least system + user = 2 messages; examples push it higher
    assert!(result.message_count() >= 2);
    assert!(result.has_cot());
    assert_eq!(result.strategy_label(), "StepByStep");
    assert_eq!(result.domain_label(), "Layout");
}

#[test]
fn generator_no_cot_returns_strategy_none() {
    let gen = PromptGenerator::new();
    let spec = TaskSpec::new("Quick layout", TaskDomain::Layout, Difficulty::Easy)
        .without_cot();
    let result = gen.generate(&spec);
    assert!(!result.has_cot());
    assert_eq!(result.strategy_label(), "none");
}

#[test]
fn generator_hard_accessibility_uses_task_decomposition() {
    let gen = PromptGenerator::new();
    let spec = TaskSpec::new(
        "Full WCAG audit",
        TaskDomain::Accessibility,
        Difficulty::Hard,
    );
    let result = gen.generate(&spec);
    assert_eq!(result.strategy_label(), "TaskDecomposition");
}

#[test]
fn generator_metadata_includes_examples_used() {
    let gen = PromptGenerator::new();
    let spec = TaskSpec::new("Apply brand colours", TaskDomain::Colors, Difficulty::Easy);
    let result = gen.generate(&spec);
    let meta_examples = result
        .payload
        .metadata
        .get("examples_used")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);
    // examples_used must equal what's recorded on the struct
    assert_eq!(meta_examples, result.examples_used);
}

#[test]
fn generator_custom_system_template_with_user_level() {
    let gen = PromptGenerator::new().with_system_template(
        "lvl",
        "Level: {{user_level}} | Domain: {{domain}} | Task: {{task}}.",
    );
    let spec = TaskSpec::new("Draw a chart", TaskDomain::Code, Difficulty::Medium)
        .with_user_level("expert");
    let result = gen.generate(&spec);
    let sys_content = result
        .payload
        .messages
        .iter()
        .find(|m| matches!(m.role, Role::System))
        .map(|m| m.content.as_str())
        .unwrap_or("");
    assert!(sys_content.contains("expert"));
    assert!(sys_content.contains("Code"));
}

#[test]
fn generator_max_examples_zero_still_builds_prompt() {
    let gen = PromptGenerator::new();
    let spec = TaskSpec::new("task", TaskDomain::Animation, Difficulty::Easy)
        .with_examples(0);
    let result = gen.generate(&spec);
    assert_eq!(result.examples_used, 0);
    // A prompt with 0 examples still has at least system + user
    assert!(result.message_count() >= 2);
}

#[test]
fn generator_estimated_tokens_increases_with_more_examples() {
    let gen = PromptGenerator::new();
    let spec_few  = TaskSpec::new("t", TaskDomain::Layout, Difficulty::Easy).with_examples(1);
    let spec_many = TaskSpec::new("t", TaskDomain::Layout, Difficulty::Easy).with_examples(5);
    let few_tokens  = gen.generate(&spec_few).estimated_tokens();
    let many_tokens = gen.generate(&spec_many).estimated_tokens();
    assert!(many_tokens >= few_tokens);
}

// ── TrainingSession integration ───────────────────────────────────────────────

#[test]
fn training_full_pipeline_reaches_certification() {
    let mut evaluator = RubricEvaluator::new();
    evaluator.add_criterion(RubricCriterion::new("contrast", 1.0, "WCAG contrast"));
    evaluator.add_criterion(RubricCriterion::new("aria", 0.8, "ARIA labels"));

    let mut session = TrainingSession::new(
        "cert-01",
        "Design accessible icon button",
        TrainingConfig::default().with_threshold(0.8),
        evaluator,
    );

    // Round 0: partial
    session.start("blue icon button", &["contrast", "aria"], 0);
    assert!(!session.is_certified());

    // Round 1: improved — both keywords present → score = 1.0
    session.train_round(
        "blue icon button with contrast ratio 4.6:1 and aria-label=\"Close\"",
        "Added contrast ratio and aria-label",
        &["contrast", "aria"],
        1,
    );
    assert!(session.is_certified());
    assert!(session.is_done());
}

#[test]
fn training_score_trajectory_is_recorded() {
    let mut ev = RubricEvaluator::new();
    ev.add_criterion(RubricCriterion::new("keyword", 1.0, ""));

    let mut s = TrainingSession::new("t2", "task", TrainingConfig::default(), ev);
    s.start("no match", &["keyword"], 0);
    s.train_round("keyword present now", "improved", &["keyword"], 1);
    let traj = s.score_trajectory();
    assert_eq!(traj.len(), 2);
    assert!(traj[1] > traj[0]);
}

#[test]
fn training_best_score_equals_max_of_trajectory() {
    let ev = RubricEvaluator::new();
    let mut s = TrainingSession::new("t3", "task", TrainingConfig::default(), ev);
    s.scores = vec![0.3, 0.9, 0.5];
    let best = s.best_score();
    assert!((best - 0.9).abs() < 0.001);
}

#[test]
fn training_session_certification_tag_propagates() {
    let config = TrainingConfig::default().with_tag("phase-15.5-advanced");
    let s = TrainingSession::new("t4", "task", config, RubricEvaluator::new());
    assert_eq!(s.certification_tag(), Some("phase-15.5-advanced"));
}

// ── Dynamic few-shot selection ────────────────────────────────────────────────

#[test]
fn dynamic_select_falls_back_to_easier_examples() {
    let lib = ExampleLibrary::with_builtins();
    // Colors has Easy examples but likely no Hard — dynamic_select should fill from fallback
    let selected = lib.dynamic_select(&TaskDomain::Colors, Difficulty::Hard, 2);
    // We asked for 2; should get at most 2 and at least whatever domain has
    assert!(selected.len() <= 2);
    assert!(selected.iter().all(|e| e.domain == TaskDomain::Colors));
}

#[test]
fn dynamic_select_respects_max_n() {
    let lib = ExampleLibrary::with_builtins();
    let selected = lib.dynamic_select(&TaskDomain::Layout, Difficulty::Easy, 1);
    assert!(selected.len() <= 1);
}

#[test]
fn count_by_domain_code_has_examples() {
    let lib = ExampleLibrary::with_builtins();
    // New code examples were added in this phase
    assert!(lib.count_by_domain(&TaskDomain::Code) >= 2);
}

#[test]
fn count_by_difficulty_hard_has_examples() {
    let lib = ExampleLibrary::with_builtins();
    // Hard examples for Layout, Accessibility, Animation, Code were added
    assert!(lib.count_by_difficulty(Difficulty::Hard) >= 4);
}

// ── End-to-end: Generator → Training loop ────────────────────────────────────

#[test]
fn scenario_generator_feeds_training_loop() {
    // Build a prompt with the generator, then simulate a training session
    let gen = PromptGenerator::new();
    let spec = TaskSpec::new(
        "Make the icon button accessible",
        TaskDomain::Accessibility,
        Difficulty::Medium,
    );
    let generated = gen.generate(&spec);

    // Verify prompt quality before handing it to the training loop
    assert!(generated.has_cot());
    assert!(generated.estimated_tokens() > 0);

    // Simulate training based on the generated task description
    let mut ev = RubricEvaluator::new();
    ev.add_criterion(RubricCriterion::new("accessible", 1.0, ""));
    ev.add_criterion(RubricCriterion::new("aria", 1.0, ""));

    let mut training = TrainingSession::new(
        "sess-gen-train",
        &spec.description,
        TrainingConfig::default(),
        ev,
    );
    training.start("button is small", &["accessible", "aria"], 0);
    training.train_round(
        "accessible icon button with aria-label and focus ring",
        "Added aria and accessible keyword",
        &["accessible", "aria"],
        1,
    );

    assert!(training.is_certified());
    assert!(training.score_trajectory().len() == 2);
}

#[test]
fn scenario_selfcheck_cot_plus_refinement() {
    // SelfCheck strategy wraps the prompt, then a refinement session improves it
    let base = Prompt::new()
        .system("You are a layout expert.")
        .user("Design a sidebar.");
    let wrapped = CotInstruction::new(CotStrategy::SelfCheck).wrap(base);

    // System should contain "Initial Answer" marker from SelfCheck instruction
    let sys = wrapped.system_messages();
    assert!(!sys.is_empty());
    assert!(sys[0].content.contains("Initial Answer"));

    // Now run a refinement session tied to this task
    let mut session = RefinementSession::new(
        "sess-selfcheck",
        "Design a sidebar",
        RefinementConfig { max_rounds: 2, require_improvement: true, early_stop_patience: 2 },
    );
    session.start("Sidebar: 200 px, nav links.", 0);
    session.add_round_auto_detect(
        "Sidebar: 200 px, nav links, collapsible on mobile, aria-navigation.",
        "Added mobile + accessibility",
        1,
    );
    assert_eq!(session.round_count(), 2);
    assert!(session.best_response().unwrap().contains("aria"));
}

#[test]
fn scenario_full_advanced_pipeline_all_modules() {
    // Full pipeline: TemplateRegistry → ExampleLibrary → PromptGenerator →
    // CotInstruction → RefinementSession → FeedbackStore

    // 1. Custom template
    let gen = PromptGenerator::new().with_system_template(
        "advanced",
        "Expert agent for {{domain}} ({{difficulty}}). Task: {{task}}.",
    );

    // 2. Generate a hard typography prompt
    let spec = TaskSpec::new(
        "Implement a responsive variable-font system",
        TaskDomain::Typography,
        Difficulty::Hard,
    ).with_examples(2);
    let generated = gen.generate(&spec);
    assert!(generated.has_cot());
    assert_eq!(generated.strategy_label(), "TaskDecomposition");

    // 3. Training session validates response quality
    let mut ev = RubricEvaluator::new();
    ev.add_criterion(RubricCriterion::new("variable-font", 1.0, ""));
    ev.add_criterion(RubricCriterion::new("responsive", 0.9, ""));

    let mut training = TrainingSession::new(
        "sess-advanced",
        &spec.description,
        TrainingConfig::default().with_tag("phase-15.5"),
        ev,
    );
    training.start("set font-size based on viewport", &["variable-font", "responsive"], 0);
    training.train_round(
        "Use variable-font with responsive clamp(): font-size: clamp(14px, 2vw, 20px).",
        "Added variable-font keyword and responsive sizing",
        &["variable-font", "responsive"],
        1,
    );
    training.finalize();

    assert!(training.is_certified());
    assert_eq!(training.certification_tag(), Some("phase-15.5"));

    // 4. Feedback store records quality
    let mut store = FeedbackStore::new();
    store.add(FeedbackAnnotation::new("sess-advanced", 0, "auto", 0.5, "partial", 0));
    store.add(FeedbackAnnotation::new("sess-advanced", 1, "auto", 1.0, "certified", 1));
    let avg = store.average_score_for("sess-advanced").unwrap();
    assert!(avg > 0.7);
}
