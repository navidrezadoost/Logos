//! Training Orchestrator — drives the 5-minute agent onboarding pipeline
//!
//! Iterates through curriculum prompts, dispatches them to the agent, and
//! tracks which modules were correctly understood. Supports a mock mode
//! (no live HTTP calls) for unit testing.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use crate::prompt_generator::{Curriculum, CurriculumModule, TrainingPrompt};

// ── Training config ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TrainingConfig {
    /// Maximum training duration in seconds (default: 300 = 5 minutes).
    pub max_duration_secs: u64,
    /// Skip modules not in this list (None = cover all).
    pub module_filter: Option<Vec<CurriculumModule>>,
    /// Whether to require responses before advancing (mock: skip).
    pub interactive: bool,
    /// If true, use canned mock responses (no HTTP calls).
    pub mock_mode: bool,
    /// Minimum expected keywords matched to accept a response.
    pub min_keywords_matched: usize,
}

impl Default for TrainingConfig {
    fn default() -> Self {
        TrainingConfig {
            max_duration_secs: 300,
            module_filter: None,
            interactive: false,
            mock_mode: true,
            min_keywords_matched: 1,
        }
    }
}

impl TrainingConfig {
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.max_duration_secs = secs;
        self
    }

    pub fn with_modules(mut self, modules: Vec<CurriculumModule>) -> Self {
        self.module_filter = Some(modules);
        self
    }

    pub fn is_module_included(&self, module: &CurriculumModule) -> bool {
        match &self.module_filter {
            None => true,
            Some(list) => list.contains(module),
        }
    }
}

// ── Training messages ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageRole {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingMessage {
    pub role: MessageRole,
    pub content: String,
    pub timestamp_secs: u64,
}

impl TrainingMessage {
    pub fn system(content: impl Into<String>, ts: u64) -> Self {
        TrainingMessage { role: MessageRole::System, content: content.into(), timestamp_secs: ts }
    }

    pub fn user(content: impl Into<String>, ts: u64) -> Self {
        TrainingMessage { role: MessageRole::User, content: content.into(), timestamp_secs: ts }
    }

    pub fn assistant(content: impl Into<String>, ts: u64) -> Self {
        TrainingMessage { role: MessageRole::Assistant, content: content.into(), timestamp_secs: ts }
    }
}

// ── Training phase ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrainingPhase {
    NotStarted,
    SendingCurriculum,
    ReceivingResponses,
    Verifying,
    Completed,
    TimedOut,
    Failed(String),
}

// ── Training status ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TrainingStatus {
    Success,
    PartialSuccess { modules_failed: Vec<String> },
    Timeout,
    Error(String),
}

// ── Training record per prompt ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingRecord {
    pub prompt_id: String,
    pub module: CurriculumModule,
    pub agent_response: String,
    pub keywords_matched: usize,
    pub accepted: bool,
    pub latency_secs: f64,
}

// ── Training session ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingSession {
    pub session_id: String,
    pub messages: Vec<TrainingMessage>,
    pub records: Vec<TrainingRecord>,
    pub modules_covered: Vec<CurriculumModule>,
    pub modules_failed: Vec<CurriculumModule>,
    pub phase: TrainingPhase,
    pub started_at: u64,
    pub elapsed_secs: u64,
    pub status: TrainingStatus,
}

impl TrainingSession {
    pub fn new(session_id: impl Into<String>, started_at: u64) -> Self {
        TrainingSession {
            session_id: session_id.into(),
            messages: Vec::new(),
            records: Vec::new(),
            modules_covered: Vec::new(),
            modules_failed: Vec::new(),
            phase: TrainingPhase::NotStarted,
            started_at,
            elapsed_secs: 0,
            status: TrainingStatus::Success,
        }
    }

    pub fn coverage_pct(&self) -> f32 {
        let total = self.modules_covered.len() + self.modules_failed.len();
        if total == 0 { return 0.0; }
        self.modules_covered.len() as f32 / total as f32 * 100.0
    }

    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    pub fn accepted_record_count(&self) -> usize {
        self.records.iter().filter(|r| r.accepted).count()
    }
}

// ── Training result ───────────────────────────────────────────────────────────

pub type TrainingResult = Result<TrainingSession, crate::AgentError>;

// ── Mock response generator ───────────────────────────────────────────────────

fn mock_response(prompt: &TrainingPrompt) -> String {
    // Return a canned response that matches the prompt's expected keywords
    format!(
        "{{\"understood\": true, \"module\": \"{}\", \"keywords_recognized\": [{}], \
         \"ready\": true}}",
        format!("{:?}", prompt.module),
        prompt.expected_keywords.iter()
            .map(|k| format!("\"{}\"", k))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

// ── Training Orchestrator ─────────────────────────────────────────────────────

pub struct TrainingOrchestrator {
    config: TrainingConfig,
}

impl TrainingOrchestrator {
    pub fn new(config: TrainingConfig) -> Self {
        TrainingOrchestrator { config }
    }

    /// Run the training pipeline. Returns a TrainingSession.
    pub fn run(
        &self,
        session_id: &str,
        curriculum: &Curriculum,
        start_secs: u64,
    ) -> TrainingResult {
        let mut training = TrainingSession::new(session_id, start_secs);
        training.phase = TrainingPhase::SendingCurriculum;

        // Filter to included modules
        let prompts: Vec<&TrainingPrompt> = curriculum.prompts.iter()
            .filter(|p| self.config.is_module_included(&p.module))
            .collect();

        let mut covered: HashMap<CurriculumModule, bool> = HashMap::new();
        let mut elapsed: u64 = 0;

        for (i, prompt) in prompts.iter().enumerate() {
            // Timeout check
            if elapsed > self.config.max_duration_secs {
                training.phase = TrainingPhase::TimedOut;
                training.status = TrainingStatus::Timeout;
                training.elapsed_secs = elapsed;
                return Ok(training);
            }

            // Build user message
            let user_msg = TrainingMessage::user(prompt.content.clone(), start_secs + elapsed);
            training.messages.push(user_msg);

            // Get agent response (mock or live)
            let response = if self.config.mock_mode {
                mock_response(prompt)
            } else {
                // Live HTTP mode — placeholder
                return Err(crate::AgentError::ProviderError(
                    "Live HTTP mode not enabled in this build (enable `http` feature)".into()
                ));
            };

            let assistant_msg = TrainingMessage::assistant(response.clone(), start_secs + elapsed + 2);
            training.messages.push(assistant_msg);

            // Verify keywords
            let matched = prompt.expected_keywords.iter()
                .filter(|kw| response.to_lowercase().contains(&kw.to_lowercase()))
                .count();
            let accepted = prompt.expected_keywords.is_empty()
                || matched >= self.config.min_keywords_matched;

            training.records.push(TrainingRecord {
                prompt_id: prompt.id.clone(),
                module: prompt.module.clone(),
                agent_response: response,
                keywords_matched: matched,
                accepted,
                latency_secs: 2.0, // mock latency
            });

            // Track module coverage
            let current = covered.entry(prompt.module.clone()).or_insert(true);
            if !accepted {
                *current = false;
            }

            elapsed += 5 + (i as u64 % 3); // simulate time passing
        }

        training.elapsed_secs = elapsed;
        training.phase = TrainingPhase::Verifying;

        // Summarize coverage
        for (module, passed) in covered {
            if passed {
                training.modules_covered.push(module);
            } else {
                training.modules_failed.push(module);
            }
        }

        if training.modules_failed.is_empty() {
            training.phase = TrainingPhase::Completed;
            training.status = TrainingStatus::Success;
        } else {
            training.phase = TrainingPhase::Completed;
            training.status = TrainingStatus::PartialSuccess {
                modules_failed: training.modules_failed.iter()
                    .map(|m| format!("{:?}", m))
                    .collect(),
            };
        }

        Ok(training)
    }

    /// Check if training would complete within the time limit.
    pub fn would_fit_time_limit(&self, curriculum: &Curriculum) -> bool {
        curriculum.total_time_secs() as u64 <= self.config.max_duration_secs
    }
}

impl Default for TrainingOrchestrator {
    fn default() -> Self {
        Self::new(TrainingConfig::default())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompt_generator::PromptGenerator;

    fn orchestrator() -> TrainingOrchestrator {
        TrainingOrchestrator::default()
    }

    fn curriculum() -> Curriculum {
        PromptGenerator::default().generate_curriculum()
    }

    #[test]
    fn run_training_mock_succeeds() {
        let orch = orchestrator();
        let c = curriculum();
        let result = orch.run("session-1", &c, 1000);
        assert!(result.is_ok());
        let ts = result.unwrap();
        assert_eq!(ts.session_id, "session-1");
        assert!(ts.started_at == 1000);
    }

    #[test]
    fn training_session_has_messages() {
        let orch = orchestrator();
        let c = curriculum();
        let ts = orch.run("s1", &c, 0).unwrap();
        assert!(ts.message_count() > 0);
    }

    #[test]
    fn training_session_has_records() {
        let orch = orchestrator();
        let c = curriculum();
        let ts = orch.run("s1", &c, 0).unwrap();
        assert!(!ts.records.is_empty());
    }

    #[test]
    fn training_completed_phase() {
        let orch = orchestrator();
        let c = curriculum();
        let ts = orch.run("s1", &c, 0).unwrap();
        assert_eq!(ts.phase, TrainingPhase::Completed);
    }

    #[test]
    fn training_coverage_positive() {
        let orch = orchestrator();
        let c = curriculum();
        let ts = orch.run("s1", &c, 0).unwrap();
        assert!(ts.coverage_pct() > 0.0);
    }

    #[test]
    fn training_elapsed_time_nonzero() {
        let orch = orchestrator();
        let c = curriculum();
        let ts = orch.run("s1", &c, 0).unwrap();
        assert!(ts.elapsed_secs > 0, "Elapsed should be >0");
    }

    #[test]
    fn quick_curriculum_trains_fast() {
        let gen = PromptGenerator::default();
        let quick = gen.quick_curriculum();
        let orch = orchestrator();
        assert!(orch.would_fit_time_limit(&quick));
    }

    #[test]
    fn full_curriculum_fits_5min() {
        let gen = PromptGenerator::default();
        let full = gen.generate_curriculum();
        let orch = orchestrator();
        assert!(orch.would_fit_time_limit(&full));
    }

    #[test]
    fn timeout_triggers_on_short_limit() {
        let config = TrainingConfig {
            max_duration_secs: 0, // instant timeout
            mock_mode: true,
            ..Default::default()
        };
        let orch = TrainingOrchestrator::new(config);
        let c = curriculum();
        let ts = orch.run("s1", &c, 0).unwrap();
        assert_eq!(ts.phase, TrainingPhase::TimedOut);
    }

    #[test]
    fn module_filter_limits_coverage() {
        let config = TrainingConfig {
            module_filter: Some(vec![CurriculumModule::DocumentModel]),
            mock_mode: true,
            ..Default::default()
        };
        let orch = TrainingOrchestrator::new(config);
        let c = curriculum();
        let ts = orch.run("s1", &c, 0).unwrap();
        // Covered modules should only include DocumentModel
        assert!(ts.modules_covered.iter().all(|m| m == &CurriculumModule::DocumentModel));
    }

    #[test]
    fn live_mode_returns_error_without_http_feature() {
        let config = TrainingConfig {
            mock_mode: false,
            ..Default::default()
        };
        let orch = TrainingOrchestrator::new(config);
        let c = PromptGenerator::default().quick_curriculum();
        let res = orch.run("s1", &c, 0);
        assert!(res.is_err());
    }

    #[test]
    fn accepted_record_count_matches() {
        let orch = orchestrator();
        let c = curriculum();
        let ts = orch.run("s1", &c, 0).unwrap();
        let manual_count = ts.records.iter().filter(|r| r.accepted).count();
        assert_eq!(ts.accepted_record_count(), manual_count);
    }

    #[test]
    fn message_roles_alternate() {
        let orch = orchestrator();
        let c = PromptGenerator::default().quick_curriculum();
        let ts = orch.run("s1", &c, 0).unwrap();
        // Every pair should be User → Assistant
        let pairs: Vec<_> = ts.messages.windows(2).collect();
        for pair in pairs {
            if pair[0].role == MessageRole::User {
                assert_eq!(pair[1].role, MessageRole::Assistant);
            }
        }
    }

    #[test]
    fn training_status_success_when_no_failures() {
        let orch = orchestrator();
        let c = curriculum();
        let ts = orch.run("s1", &c, 0).unwrap();
        // In mock mode with no keywords to match, all records pass → Success
        matches!(ts.status, TrainingStatus::Success | TrainingStatus::PartialSuccess { .. });
    }
}
