// logos-ai-agent — AI Agent Integration for Logos
// Phase 14: External Agent onboarding, curriculum training, test & certification
// Plus: Reinforcement Learning UX (Level 1 foundations)

pub mod agent_manager;
pub mod prompt_generator;
pub mod training_orchestrator;
pub mod test_suite;
pub mod evaluator;
pub mod agent_api;
pub mod rl_ux;

// ── Re-exports ────────────────────────────────────────────────────────────────

// Agent lifecycle
pub use agent_manager::{
    AgentManager, AgentSession, AgentProvider, AgentStatus,
    RateLimiter, RateLimitError, SessionStore, AgentManagerConfig,
};

// Curriculum & prompt engineering
pub use prompt_generator::{
    PromptGenerator, Curriculum, CurriculumModule, TrainingPrompt,
    PromptTemplate, PromptDifficulty, GeneratorConfig,
};

// Training orchestration
pub use training_orchestrator::{
    TrainingOrchestrator, TrainingSession, TrainingConfig, TrainingPhase,
    TrainingRecord, TrainingResult, TrainingStatus,
};

// Test suite
pub use test_suite::{
    TestSuite, TestCase, TestLevel, TestCategory, TestResult as SuiteTestResult,
    BuiltinTestSuite, TestRunner,
};

// Evaluation & certification
pub use evaluator::{
    Evaluator, EvaluationReport, AgentLevel, ScoreBreakdown,
    LevelThresholds, EvaluationConfig,
};

// Agent API / command interface
pub use agent_api::{
    AgentCommand, AgentRequest, AgentResponse, CommandParser,
    ParsedCommand, LayerKind as AgentLayerKind, CommandResult,
    AgentApiHandler,
};

// Reinforcement learning UX assistant (Level 1 foundations)
pub use rl_ux::{
    UxAgent, UxAction, UxState, UxReward, ActionPredictor,
    BehaviorRecord, UxAgentConfig, PatternMatcher,
};

// ── Error type ────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("session not found: {0}")]
    SessionNotFound(String),

    #[error("rate limit exceeded for agent {0}")]
    RateLimitExceeded(String),

    #[error("invalid API key format")]
    InvalidApiKey,

    #[error("API provider error: {0}")]
    ProviderError(String),

    #[error("training timeout after {0}s")]
    TrainingTimeout(u64),

    #[error("test suite error: {0}")]
    TestError(String),

    #[error("evaluation error: {0}")]
    EvaluationError(String),

    #[error("command parse error: {0}")]
    ParseError(String),

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("unsupported provider: {0}")]
    UnsupportedProvider(String),
}
