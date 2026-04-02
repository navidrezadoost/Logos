//! High-level prompt generator — assembles few-shot examples, chain-of-thought
//! instructions, and system templates into a ready-to-use `GeneratedPrompt`.
//!
//! Use [`PromptGenerator`] as the single entry-point for constructing prompts:
//!
//! ```rust,no_run
//! use logos_prompt_engine::generator::{PromptGenerator, TaskSpec};
//! use logos_prompt_engine::few_shot::{TaskDomain, Difficulty};
//!
//! let gen = PromptGenerator::new();
//! let spec = TaskSpec::new("Design a login screen", TaskDomain::Layout, Difficulty::Medium);
//! let result = gen.generate(&spec);
//! assert!(result.has_cot());
//! ```

use crate::{
    chain_of_thought::{CotInstruction, CotStrategy},
    few_shot::{Difficulty, ExampleLibrary, TaskDomain},
    prompt::{Prompt, PromptPayload, PromptVariables, TemplateRegistry},
};

// ── CoT strategy selection ────────────────────────────────────────────────────

/// Choose the most appropriate [`CotStrategy`] for a given domain and difficulty.
///
/// - [`Difficulty::Hard`] → [`CotStrategy::TaskDecomposition`] (complex sub-tasks)
/// - [`Difficulty::Medium`] or [`Difficulty::Easy`] → [`CotStrategy::StepByStep`]
pub fn select_cot_strategy(_domain: &TaskDomain, difficulty: Difficulty) -> CotStrategy {
    match difficulty {
        Difficulty::Hard => CotStrategy::TaskDecomposition,
        _ => CotStrategy::StepByStep,
    }
}

// ── Task spec ─────────────────────────────────────────────────────────────────

/// Specification for a single prompt-generation request.
#[derive(Debug, Clone)]
pub struct TaskSpec {
    /// Natural-language description of the task.
    pub description: String,
    /// Domain the task belongs to (used for example selection and substitution).
    pub domain: TaskDomain,
    /// Estimated difficulty (drives CoT strategy and example selection).
    pub difficulty: Difficulty,
    /// Whether to wrap the prompt with a chain-of-thought instruction.
    pub use_cot: bool,
    /// Maximum number of few-shot examples to inject (0 = none).
    pub max_examples: usize,
    /// Optional user-proficiency level string (e.g. "beginner", "expert").
    pub user_level: Option<String>,
}

impl TaskSpec {
    /// Create a new spec with sensible defaults: CoT enabled, up to 2 examples.
    pub fn new(
        description: impl Into<String>,
        domain: TaskDomain,
        difficulty: Difficulty,
    ) -> Self {
        Self {
            description: description.into(),
            domain,
            difficulty,
            use_cot: true,
            max_examples: 2,
            user_level: None,
        }
    }

    /// Disable chain-of-thought wrapping.
    pub fn without_cot(mut self) -> Self {
        self.use_cot = false;
        self
    }

    /// Set the maximum number of few-shot examples to inject.
    pub fn with_examples(mut self, n: usize) -> Self {
        self.max_examples = n;
        self
    }

    /// Record a user-proficiency level (surfaced in `{{user_level}}` template slot).
    pub fn with_user_level(mut self, level: impl Into<String>) -> Self {
        self.user_level = Some(level.into());
        self
    }
}

// ── Generated prompt ──────────────────────────────────────────────────────────

/// The result of [`PromptGenerator::generate`].
///
/// Contains the assembled `PromptPayload` plus generation metadata.
pub struct GeneratedPrompt {
    /// The fully assembled prompt payload ready for an LLM call.
    pub payload: PromptPayload,
    /// Number of few-shot examples actually injected.
    pub examples_used: usize,
    /// The CoT strategy applied, or `None` if CoT was disabled.
    pub cot_strategy: Option<CotStrategy>,
    /// Domain taken from the originating `TaskSpec`.
    pub domain: TaskDomain,
}

impl GeneratedPrompt {
    /// Total number of messages in the prompt.
    pub fn message_count(&self) -> usize {
        self.payload.messages.len()
    }

    /// Returns `true` when a chain-of-thought strategy was applied.
    pub fn has_cot(&self) -> bool {
        self.cot_strategy.is_some()
    }

    /// Short label of the CoT strategy, or `"none"` when disabled.
    pub fn strategy_label(&self) -> &str {
        self.cot_strategy.as_ref().map(|s| s.label()).unwrap_or("none")
    }

    /// Short label of the task domain.
    pub fn domain_label(&self) -> &str {
        self.domain.label()
    }

    /// Approximate token count (4 chars ≈ 1 token).
    pub fn estimated_tokens(&self) -> usize {
        let chars: usize = self.payload.messages.iter().map(|m| m.content.len()).sum();
        chars / 4 + 1
    }
}

// ── Prompt generator ──────────────────────────────────────────────────────────

const DEFAULT_TEMPLATE_NAME: &str = "_default";
const DEFAULT_TEMPLATE: &str =
    "You are an expert Logos design agent specializing in {{domain}}. \
     User level: {{user_level}}. Your task: {{task}}.";

/// High-level assembler that combines templates, few-shot examples, and CoT
/// instructions into a single [`GeneratedPrompt`].
///
/// # Example
///
/// ```rust,no_run
/// use logos_prompt_engine::generator::{PromptGenerator, TaskSpec};
/// use logos_prompt_engine::few_shot::{TaskDomain, Difficulty};
///
/// let gen = PromptGenerator::new();
/// let spec = TaskSpec::new("Align icons to the grid", TaskDomain::Layout, Difficulty::Easy);
/// let prompt = gen.generate(&spec);
/// assert!(prompt.has_cot());
/// ```
pub struct PromptGenerator {
    example_library: ExampleLibrary,
    template_registry: TemplateRegistry,
    system_template_name: String,
}

impl Default for PromptGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptGenerator {
    /// Create a generator with the built-in example library and default system template.
    pub fn new() -> Self {
        let mut registry = TemplateRegistry::new();
        registry.register(DEFAULT_TEMPLATE_NAME, DEFAULT_TEMPLATE);
        Self {
            example_library: ExampleLibrary::with_builtins(),
            template_registry: registry,
            system_template_name: DEFAULT_TEMPLATE_NAME.to_string(),
        }
    }

    /// Register a custom system template and use it for all subsequent `generate` calls.
    ///
    /// Supported slots: `{{task}}`, `{{domain}}`, `{{difficulty}}`, `{{user_level}}`.
    pub fn with_system_template(
        mut self,
        name: impl Into<String>,
        template: impl Into<String>,
    ) -> Self {
        let name = name.into();
        self.template_registry.register(name.clone(), template);
        self.system_template_name = name;
        self
    }

    /// Assemble a complete prompt from a [`TaskSpec`].
    ///
    /// Pipeline:
    /// 1. Render the system template with spec variables.
    /// 2. Inject few-shot examples selected for the spec's domain and difficulty.
    /// 3. Optionally wrap with a chain-of-thought instruction.
    /// 4. Attach generation metadata to the payload.
    pub fn generate(&self, spec: &TaskSpec) -> GeneratedPrompt {
        // 1. Build system prompt via template substitution
        let vars = PromptVariables::new()
            .set("task", &spec.description)
            .set("domain", spec.domain.label())
            .set("difficulty", difficulty_label(spec.difficulty))
            .set(
                "user_level",
                spec.user_level.as_deref().unwrap_or("intermediate"),
            );

        let system_text = self
            .template_registry
            .render(&self.system_template_name, &vars)
            .unwrap_or_else(|| spec.description.clone());

        // 2. Assemble base prompt
        let base = Prompt::new()
            .system(system_text)
            .user(&spec.description);

        // 3. Inject few-shot examples
        let examples = if spec.max_examples > 0 {
            self.example_library
                .dynamic_select(&spec.domain, spec.difficulty, spec.max_examples)
        } else {
            vec![]
        };
        let examples_used = examples.len();
        let prompted = self.example_library.inject_into(base, &examples);

        // 4. Optionally wrap with CoT
        let (prompted, cot_strategy) = if spec.use_cot {
            let strategy = select_cot_strategy(&spec.domain, spec.difficulty);
            let wrapped = CotInstruction::new(strategy.clone()).wrap(prompted);
            (wrapped, Some(strategy))
        } else {
            (prompted, None)
        };

        // 5. Build payload and attach metadata
        let mut payload = prompted.build();
        payload
            .metadata
            .insert("examples_used".into(), examples_used.to_string());
        payload
            .metadata
            .insert("domain".into(), spec.domain.label().to_string());
        payload
            .metadata
            .insert("difficulty".into(), difficulty_label(spec.difficulty).to_string());
        if let Some(ref tag) = spec.user_level {
            payload.metadata.insert("user_level".into(), tag.clone());
        }
        if let Some(ref cot) = cot_strategy {
            payload.metadata.insert("cot_strategy".into(), cot.label().to_string());
        }

        GeneratedPrompt {
            payload,
            examples_used,
            cot_strategy,
            domain: spec.domain.clone(),
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn difficulty_label(d: Difficulty) -> &'static str {
    match d {
        Difficulty::Easy => "easy",
        Difficulty::Medium => "medium",
        Difficulty::Hard => "hard",
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompt::Role;

    #[test]
    fn select_cot_strategy_hard_returns_decomposition() {
        assert_eq!(
            select_cot_strategy(&TaskDomain::Layout, Difficulty::Hard),
            CotStrategy::TaskDecomposition
        );
    }

    #[test]
    fn select_cot_strategy_medium_returns_step_by_step() {
        assert_eq!(
            select_cot_strategy(&TaskDomain::Colors, Difficulty::Medium),
            CotStrategy::StepByStep
        );
    }

    #[test]
    fn select_cot_strategy_easy_returns_step_by_step() {
        assert_eq!(
            select_cot_strategy(&TaskDomain::Typography, Difficulty::Easy),
            CotStrategy::StepByStep
        );
    }

    #[test]
    fn task_spec_defaults() {
        let spec = TaskSpec::new("Test task", TaskDomain::Layout, Difficulty::Medium);
        assert!(spec.use_cot);
        assert_eq!(spec.max_examples, 2);
        assert!(spec.user_level.is_none());
    }

    #[test]
    fn task_spec_without_cot() {
        let spec = TaskSpec::new("t", TaskDomain::Export, Difficulty::Easy).without_cot();
        assert!(!spec.use_cot);
    }

    #[test]
    fn task_spec_with_examples_and_user_level() {
        let spec = TaskSpec::new("t", TaskDomain::Code, Difficulty::Hard)
            .with_examples(5)
            .with_user_level("expert");
        assert_eq!(spec.max_examples, 5);
        assert_eq!(spec.user_level.as_deref(), Some("expert"));
    }

    #[test]
    fn generator_builds_prompt_with_cot() {
        let gen = PromptGenerator::new();
        let spec = TaskSpec::new("Design sidebar", TaskDomain::Layout, Difficulty::Medium);
        let result = gen.generate(&spec);
        assert!(result.has_cot());
        assert_eq!(result.strategy_label(), "StepByStep");
        assert!(result.message_count() >= 2);
    }

    #[test]
    fn generator_no_cot_when_disabled() {
        let gen = PromptGenerator::new();
        let spec = TaskSpec::new("t", TaskDomain::Layout, Difficulty::Easy).without_cot();
        let result = gen.generate(&spec);
        assert!(!result.has_cot());
        assert_eq!(result.strategy_label(), "none");
    }

    #[test]
    fn generator_hard_uses_task_decomposition() {
        let gen = PromptGenerator::new();
        let spec = TaskSpec::new("Audit form WCAG", TaskDomain::Accessibility, Difficulty::Hard);
        let result = gen.generate(&spec);
        assert_eq!(result.strategy_label(), "TaskDecomposition");
    }

    #[test]
    fn generator_domain_label_correct() {
        let gen = PromptGenerator::new();
        let spec = TaskSpec::new("t", TaskDomain::Colors, Difficulty::Easy);
        assert_eq!(gen.generate(&spec).domain_label(), "Colors");
    }

    #[test]
    fn generator_metadata_examples_used() {
        let gen = PromptGenerator::new();
        let spec = TaskSpec::new("t", TaskDomain::Colors, Difficulty::Easy);
        let result = gen.generate(&spec);
        let recorded: usize = result
            .payload
            .metadata
            .get("examples_used")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        assert_eq!(recorded, result.examples_used);
    }

    #[test]
    fn generator_zero_examples_still_builds() {
        let gen = PromptGenerator::new();
        let spec = TaskSpec::new("t", TaskDomain::Animation, Difficulty::Easy).with_examples(0);
        let result = gen.generate(&spec);
        assert_eq!(result.examples_used, 0);
        assert!(result.message_count() >= 2);
    }

    #[test]
    fn generator_default_template_contains_task() {
        let gen = PromptGenerator::new();
        let spec = TaskSpec::new("Design a hero card", TaskDomain::Layout, Difficulty::Easy)
            .with_examples(0);
        let result = gen.generate(&spec);
        let sys = result
            .payload
            .messages
            .iter()
            .find(|m| matches!(m.role, Role::System))
            .map(|m| m.content.as_str())
            .unwrap_or("");
        assert!(sys.contains("Layout") || sys.contains("layout"));
    }

    #[test]
    fn generator_custom_system_template_substitutes_vars() {
        let gen = PromptGenerator::new().with_system_template(
            "custom",
            "Domain={{domain}} Level={{user_level}} Task={{task}}",
        );
        let spec = TaskSpec::new("Animate button", TaskDomain::Animation, Difficulty::Medium)
            .with_user_level("beginner")
            .with_examples(0);
        let result = gen.generate(&spec);
        let sys = result
            .payload
            .messages
            .iter()
            .find(|m| matches!(m.role, Role::System))
            .map(|m| m.content.as_str())
            .unwrap_or("");
        assert!(sys.contains("Animation"));
        assert!(sys.contains("beginner"));
        assert!(sys.contains("Animate button"));
    }

    #[test]
    fn generator_estimated_tokens_positive() {
        let gen = PromptGenerator::new();
        let spec = TaskSpec::new("t", TaskDomain::Layout, Difficulty::Easy);
        assert!(gen.generate(&spec).estimated_tokens() > 0);
    }
}
