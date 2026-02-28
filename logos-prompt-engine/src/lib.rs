//! `logos-prompt-engine` — Advanced Prompt Engineering for Logos AI Agents
//!
//! Provides building blocks for constructing high-quality prompts:
//!
//! * [`prompt`] — Core `Prompt` builder, `TemplateRegistry`, `PromptVariables`
//! * [`few_shot`] — `ExampleLibrary` with curated design examples and `inject_into`
//! * [`chain_of_thought`] — `CotStrategy`, `CotInstruction`, `CotParser`, `ChainOfThought`
//! * [`refinement`] — `RefinementSession`, `CritiqueTemplate`, `FeedbackStore`

pub mod chain_of_thought;
pub mod few_shot;
pub mod prompt;
pub mod refinement;

// ── Flat re-exports ───────────────────────────────────────────────────────────

pub use chain_of_thought::{
    ChainOfThought, CotInstruction, CotParseError, CotParser, CotStrategy, ThoughtStep,
};
pub use few_shot::{
    Difficulty, ExampleLibrary, ExampleTurn, FewShotExample, TaskDomain,
};
pub use prompt::{
    Message, Prompt, PromptConfig, PromptPayload, PromptVariables, Role, TemplateRegistry,
};
pub use refinement::{
    CritiqueTemplate, FeedbackAnnotation, FeedbackStore, RefinementConfig, RefinementRound,
    RefinementSession,
};
