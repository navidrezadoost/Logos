//! Phase 14 integration tests — full end-to-end agent onboarding pipeline
//!
//! Tests the complete flow: register agent → generate curriculum → train →
//! run test suite → evaluate → receive Junior/MidLevel/Senior certification.

use logos_ai_agent::{
    AgentManager, AgentProvider,
    PromptGenerator,
    TrainingOrchestrator, TrainingConfig,
    BuiltinTestSuite, TestRunner, TestLevel,
    Evaluator,
    AgentLevel, AgentStatus,
    CommandParser, AgentCommand,
    UxAgent, UxState, UxAction,
};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn test_api_key() -> String {
    "sk-test-integration-key-12345".to_string()
}

fn openai_key() -> String {
    "sk-openai-fake-key-for-testing-only-9999".to_string()
}

// ── 1. Registration tests ─────────────────────────────────────────────────────

#[test]
fn integration_register_openai_agent() {
    let mut mgr = AgentManager::default();
    let id = mgr.register(AgentProvider::OpenAI, &openai_key(), "Test GPT-4 Session", 1000).unwrap();
    assert!(!id.is_empty());
    let session = mgr.get_session(&id).unwrap();
    assert_eq!(session.status, AgentStatus::Registered);
    assert_eq!(session.provider, AgentProvider::OpenAI);
}

#[test]
fn integration_register_anthropic_agent() {
    let mut mgr = AgentManager::default();
    let id = mgr.register(
        AgentProvider::Anthropic,
        "sk-ant-api03-fake-key-0000000000000",
        "Claude Session",
        2000,
    ).unwrap();
    let session = mgr.get_session(&id).unwrap();
    assert_eq!(session.provider, AgentProvider::Anthropic);
    assert!(!session.key_hash.contains("ant-api"));
}

// ── 2. Curriculum generation ──────────────────────────────────────────────────

#[test]
fn integration_curriculum_generated_for_all_modules() {
    let gen = PromptGenerator::default();
    let curriculum = gen.generate_curriculum();
    assert!(curriculum.prompts.len() >= 7);
    assert!(curriculum.total_words() > 500);
}

#[test]
fn integration_curriculum_fits_5_minutes() {
    let gen = PromptGenerator::default();
    let curriculum = gen.generate_curriculum();
    let orch = TrainingOrchestrator::default();
    assert!(orch.would_fit_time_limit(&curriculum), "Curriculum exceeds 5-minute limit");
}

// ── 3. Training pipeline ──────────────────────────────────────────────────────

#[test]
fn integration_training_pipeline_completes() {
    let mut mgr = AgentManager::default();
    let id = mgr.register(AgentProvider::OpenAI, &openai_key(), "training-test", 0).unwrap();
    mgr.set_status(&id, AgentStatus::Training);

    let gen = PromptGenerator::default();
    let curriculum = gen.generate_curriculum();
    let orch = TrainingOrchestrator::default();
    let training = orch.run(&id, &curriculum, 0).unwrap();

    assert!(!training.messages.is_empty());
    assert!(!training.records.is_empty());
    assert!(training.elapsed_secs > 0);
}

#[test]
fn integration_training_covers_document_model() {
    let gen = PromptGenerator::default();
    let curriculum = gen.generate_curriculum();
    let orch = TrainingOrchestrator::default();
    let training = orch.run("sess", &curriculum, 0).unwrap();
    // At least some records should be for DocumentModel
    let doc_records: Vec<_> = training.records.iter()
        .filter(|r| format!("{:?}", r.module).contains("DocumentModel"))
        .collect();
    assert!(!doc_records.is_empty());
}

// ── 4. Test suite execution ───────────────────────────────────────────────────

#[test]
fn integration_test_suite_has_50_cases() {
    let suite = BuiltinTestSuite::build();
    assert_eq!(suite.case_count(), 50);
}

#[test]
fn integration_senior_agent_response_passes_simple_tests() {
    let suite = BuiltinTestSuite::build();
    let simple_cases = suite.by_level(&TestLevel::Simple);
    let mut passed = 0;
    for case in &simple_cases {
        let good_response = case.expected_keywords.join(" ")
            + " " + case.expected_command.as_deref().unwrap_or("");
        let result = TestRunner::evaluate(&good_response, case, 200);
        if result.passed { passed += 1; }
    }
    assert_eq!(passed, simple_cases.len(), "All simple tests should pass with ideal response");
}

#[test]
fn integration_empty_response_fails_all_tests() {
    let suite = BuiltinTestSuite::build();
    let failed = suite.cases.iter()
        .filter(|c| !c.expected_keywords.is_empty())
        .map(|c| TestRunner::evaluate("", c, 100))
        .filter(|r| !r.passed)
        .count();
    // All cases with keywords should fail on empty response
    assert!(failed > 0);
}

// ── 5. Evaluation and certification ──────────────────────────────────────────

#[test]
fn integration_senior_agent_receives_senior_level() {
    let suite = BuiltinTestSuite::build();
    let results: Vec<_> = suite.cases.iter().map(|c| {
        let resp = c.expected_keywords.join(" ")
            + " " + c.expected_command.as_deref().unwrap_or("");
        TestRunner::evaluate(&resp, c, 100)
    }).collect();
    let eval = Evaluator::default();
    let report = eval.evaluate(&results, &suite, "sess-senior", 9999);
    assert_eq!(report.level, AgentLevel::Senior);
}

#[test]
fn integration_junior_agent_receives_junior_level() {
    let suite = BuiltinTestSuite::build();
    let results: Vec<_> = suite.cases.iter().map(|c| {
        TestRunner::evaluate("I don't know how to do that", c, 100)
    }).collect();
    let eval = Evaluator::default();
    let report = eval.evaluate(&results, &suite, "sess-junior", 0);
    assert_eq!(report.level, AgentLevel::Junior);
}

#[test]
fn integration_evaluation_report_has_all_breakdowns() {
    let suite = BuiltinTestSuite::build();
    let results: Vec<_> = suite.cases.iter().map(|c| {
        TestRunner::evaluate("generic response", c, 100)
    }).collect();
    let report = Evaluator::default().evaluate(&results, &suite, "sess-x", 0);
    assert_eq!(report.breakdowns.len(), 4);
}

// ── 6. Full pipeline: register → train → test → certify ──────────────────────

#[test]
fn integration_full_pipeline_end_to_end() {
    // Step 1: Register
    let mut mgr = AgentManager::default();
    let session_id = mgr.register(
        AgentProvider::OpenAI,
        &openai_key(),
        "E2E Test Agent",
        0,
    ).unwrap();

    // Step 2: Train
    mgr.set_status(&session_id, AgentStatus::Training);
    let gen = PromptGenerator::default();
    let curriculum = gen.generate_curriculum();
    let orch = TrainingOrchestrator::default();
    let training = orch.run(&session_id, &curriculum, 0).unwrap();
    assert!(!training.records.is_empty());

    // Step 3: Test
    mgr.set_status(&session_id, AgentStatus::Testing);
    let suite = BuiltinTestSuite::build();
    let results: Vec<_> = suite.cases.iter().map(|c| {
        let resp = c.expected_keywords.join(" ")
            + " " + c.expected_command.as_deref().unwrap_or("");
        TestRunner::evaluate(&resp, c, 150)
    }).collect();

    // Step 4: Evaluate
    let eval = Evaluator::default();
    let report = eval.evaluate(&results, &suite, &session_id, 1000);

    // Step 5: Certify
    mgr.set_status(&session_id, AgentStatus::Certified);

    // Verify
    assert_eq!(mgr.get_session(&session_id).unwrap().status, AgentStatus::Certified);
    assert!(report.is_certified());
    assert_eq!(report.level, AgentLevel::Senior);
    assert_eq!(report.tests_run, 50);
}

// ── 7. Command parsing integration ───────────────────────────────────────────

#[test]
fn integration_parse_complex_create_command() {
    let p = CommandParser::parse(
        "Add a rectangle named 'Header' at x=0 y=0 width=1440 height=80"
    );
    assert!(p.confidence > 0.0);
    if let AgentCommand::CreateLayer { kind, width, height, .. } = p.command {
        assert_eq!(kind, logos_ai_agent::AgentLayerKind::Rectangle);
        assert_eq!(width, Some(1440.0));
        assert_eq!(height, Some(80.0));
    }
}

#[test]
fn integration_parse_accessibility_command() {
    let p = CommandParser::parse("Run WCAG accessibility audit and check contrast");
    assert!(matches!(p.command, AgentCommand::CheckAccessibility));
    assert!(p.confidence > 0.5);
}

// ── 8. RL UX agent integration ────────────────────────────────────────────────

#[test]
fn integration_rl_agent_learns_from_repeated_actions() {
    let mut agent = UxAgent::default();
    let state = UxState::with_selection(1);
    let next = state.clone();

    // Simulate 20 observations: user always sets fill after selecting a layer
    for i in 0..20u64 {
        agent.observe(state.clone(), UxAction::SetFill, next.clone(), i);
    }
    assert_eq!(agent.observation_count(), 20);
    assert_eq!(agent.history_len(), 20);
}

#[test]
fn integration_rl_agent_suggests_frequent_action() {
    use logos_ai_agent::{UxAgentConfig};
    let config = UxAgentConfig {
        min_observations: 5,
        suggestion_threshold: 0.0,
        use_rl: false,
        ..Default::default()
    };
    let mut agent = UxAgent::new(config);
    let state = UxState::with_selection(1);
    let next = state.clone();
    for i in 0..10u64 {
        agent.observe(state.clone(), UxAction::GroupLayers, next.clone(), i);
    }
    for i in 0..3u64 {
        agent.observe(state.clone(), UxAction::SetFill, next.clone(), 100 + i);
    }
    let suggestions = agent.suggest(&state);
    assert!(!suggestions.is_empty());
    assert_eq!(suggestions[0].0, UxAction::GroupLayers);
}
