//! Integration tests for Phase 15.5 — Advanced Prompt Engineering
//!
//! Each scenario exercises an end-to-end workflow spanning multiple modules.

use logos_prompt_engine::*;

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
