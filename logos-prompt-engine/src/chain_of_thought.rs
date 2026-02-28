//! Chain-of-thought — prompt strategies and response parsing.
//!
//! Wrapping a `Prompt` with a `CotStrategy` instructs the model to reason
//! explicitly before answering. `CotParser` extracts the resulting structured
//! steps from the model's text response.

use crate::prompt::Prompt;
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ── CoT strategy ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CotStrategy {
    /// Show scratch-pad reasoning before final answer
    Scratchpad,
    /// Explicit "Step 1 … Step N … Conclusion:" format
    StepByStep,
    /// Break the problem into sub-tasks, solve each, then synthesise
    TaskDecomposition,
    /// Solve, then critique own answer, then revise
    SelfCheck,
}

impl CotStrategy {
    pub fn label(&self) -> &str {
        match self {
            Self::Scratchpad        => "Scratchpad",
            Self::StepByStep        => "StepByStep",
            Self::TaskDecomposition => "TaskDecomposition",
            Self::SelfCheck         => "SelfCheck",
        }
    }

    pub fn instruction(&self) -> &str {
        match self {
            Self::Scratchpad =>
                "Think through the problem step by step in a scratchpad before \
                 writing your final answer. Use the format:\n\
                 <scratchpad>\n...\n</scratchpad>\n\
                 Final Answer: ...",

            Self::StepByStep =>
                "Break down your reasoning into numbered steps, then give a \
                 conclusion. Use the exact format:\n\
                 Step 1: ...\nStep 2: ...\n[additional steps as needed]\n\
                 Conclusion: ...",

            Self::TaskDecomposition =>
                "Decompose the problem into sub-tasks. For each sub-task, think \
                 through and answer it separately. Finally, synthesise the results.\n\
                 Sub-task 1: ...\nSub-task 2: ...\n\
                 Synthesis: ...",

            Self::SelfCheck =>
                "First, solve the problem. Then critically review your answer for \
                 errors or gaps. Finally, provide a revised answer.\n\
                 Initial Answer: ...\nCritique: ...\nRevised Answer: ...",
        }
    }

    /// True when the strategy produces a parse-able stepped response.
    pub fn is_parseable(&self) -> bool {
        matches!(self, Self::StepByStep | Self::TaskDecomposition)
    }
}

// ── CoT instruction wrapper ───────────────────────────────────────────────────

pub struct CotInstruction {
    pub strategy: CotStrategy,
}

impl CotInstruction {
    pub fn new(strategy: CotStrategy) -> Self { Self { strategy } }

    /// Append the CoT strategy instruction to the system prompt layer.
    /// If there is no system message, one is inserted at the front.
    pub fn wrap(&self, mut prompt: Prompt) -> Prompt {
        let instruction = self.strategy.instruction().to_string();
        // Find existing system message and append, or prepend a new one
        let has_system = prompt.messages.iter().any(|m| m.role == crate::prompt::Role::System);
        if has_system {
            for m in prompt.messages.iter_mut() {
                if m.role == crate::prompt::Role::System {
                    m.content.push('\n');
                    m.content.push_str(&instruction);
                    break;
                }
            }
        } else {
            prompt = prompt.prepend_message(crate::prompt::Message::system(instruction));
        }
        prompt.with_meta("cot_strategy", self.strategy.label())
    }
}

// ── Thought step ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThoughtStep {
    pub index: usize,
    pub title: String,
    pub content: String,
}

impl ThoughtStep {
    pub fn new(index: usize, title: impl Into<String>, content: impl Into<String>) -> Self {
        Self { index, title: title.into(), content: content.into() }
    }
}

// ── Chain of thought ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainOfThought {
    pub steps: Vec<ThoughtStep>,
    pub conclusion: String,
    pub strategy: Option<CotStrategy>,
}

impl ChainOfThought {
    pub fn new() -> Self {
        Self { steps: Vec::new(), conclusion: String::new(), strategy: None }
    }

    pub fn with_strategy(mut self, s: CotStrategy) -> Self { self.strategy = Some(s); self }

    pub fn add_step(&mut self, title: impl Into<String>, content: impl Into<String>) {
        let idx = self.steps.len() + 1;
        self.steps.push(ThoughtStep::new(idx, title, content));
    }

    pub fn set_conclusion(&mut self, text: impl Into<String>) { self.conclusion = text.into(); }

    pub fn step_count(&self) -> usize { self.steps.len() }

    pub fn has_conclusion(&self) -> bool { !self.conclusion.trim().is_empty() }

    /// Render the chain as plain text (Step 1: … \nConclusion: …)
    pub fn to_text(&self) -> String {
        let mut result = String::new();
        for step in &self.steps {
            result.push_str(&format!("Step {}: {}\n{}\n\n", step.index, step.title, step.content));
        }
        if self.has_conclusion() {
            result.push_str(&format!("Conclusion: {}", self.conclusion));
        }
        result.trim_end().to_string()
    }
}

impl Default for ChainOfThought { fn default() -> Self { Self::new() } }

// ── Parse error ───────────────────────────────────────────────────────────────

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CotParseError {
    #[error("No steps found in response")]
    NoStepsFound,
    #[error("Missing conclusion section")]
    MissingConclusion,
    #[error("Malformed step: {0}")]
    MalformedStep(String),
}

// ── CoT parser ────────────────────────────────────────────────────────────────

pub struct CotParser;

impl CotParser {
    /// Parse a `StepByStep`-format response.
    ///
    /// Expects:
    /// ```text
    /// Step 1: <title>
    /// <content lines>
    /// Step 2: <title>
    /// <content lines>
    /// ...
    /// Conclusion: <text>
    /// ```
    pub fn parse(text: &str) -> Result<ChainOfThought, CotParseError> {
        let mut cot = ChainOfThought::new();
        let mut current_title: Option<String> = None;
        let mut current_content: Vec<String> = Vec::new();
        let mut conclusion_lines: Vec<String> = Vec::new();
        let mut in_conclusion = false;

        for raw_line in text.lines() {
            let line = raw_line.trim();

            if in_conclusion {
                conclusion_lines.push(line.to_string());
                continue;
            }

            // Step N: ...
            if let Some(stripped) = line.strip_prefix("Step ") {
                // Flush previous step
                if let Some(title) = current_title.take() {
                    cot.add_step(title, current_content.join("\n").trim().to_string());
                    current_content.clear();
                }
                // Parse "N: title"
                if let Some(colon_pos) = stripped.find(':') {
                    let title_part = stripped[colon_pos + 1..].trim();
                    current_title = Some(title_part.to_string());
                } else {
                    return Err(CotParseError::MalformedStep(line.to_string()));
                }
            } else if line.starts_with("Conclusion:") {
                // Flush last step
                if let Some(title) = current_title.take() {
                    cot.add_step(title, current_content.join("\n").trim().to_string());
                    current_content.clear();
                }
                let content = line["Conclusion:".len()..].trim().to_string();
                conclusion_lines.push(content);
                in_conclusion = true;
            } else if current_title.is_some() {
                current_content.push(line.to_string());
            }
        }

        // Flush trailing step if no conclusion marker found
        if let Some(title) = current_title.take() {
            cot.add_step(title, current_content.join("\n").trim().to_string());
        }

        if cot.steps.is_empty() { return Err(CotParseError::NoStepsFound); }
        if conclusion_lines.is_empty() { return Err(CotParseError::MissingConclusion); }

        cot.set_conclusion(conclusion_lines.join("\n").trim().to_string());
        Ok(cot)
    }

    /// Lenient parse — returns what it can without erroring.
    pub fn parse_lenient(text: &str) -> ChainOfThought {
        Self::parse(text).unwrap_or_else(|_| {
            let mut cot = ChainOfThought::new();
            cot.add_step("Response", text);
            cot.set_conclusion("(parsed from unstructured response)");
            cot
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
Step 1: Understand the task
Analyse the login screen requirements.

Step 2: Identify components
Header, form fields, submit button, footer.

Step 3: Choose layout
A centred card layout with max-width 420 px.

Conclusion: Use a centred card with three sections: header, form, and footer.";

    #[test]
    fn strategy_labels_and_instructions() {
        assert_eq!(CotStrategy::StepByStep.label(), "StepByStep");
        assert!(!CotStrategy::StepByStep.instruction().is_empty());
        assert!(CotStrategy::SelfCheck.is_parseable() == false);
        assert!(CotStrategy::StepByStep.is_parseable());
    }

    #[test]
    fn cot_instruction_wraps_system_prompt() {
        let base = Prompt::new()
            .system("Base system prompt.")
            .user("Design a navbar.");
        let instruction = CotInstruction::new(CotStrategy::StepByStep);
        let wrapped = instruction.wrap(base);
        let sys = wrapped.system_messages();
        assert!(!sys.is_empty());
        assert!(sys[0].content.contains("Step 1"));
    }

    #[test]
    fn cot_instruction_creates_system_when_missing() {
        let base = Prompt::new().user("No system here.");
        let wrapped = CotInstruction::new(CotStrategy::Scratchpad).wrap(base);
        assert!(!wrapped.system_messages().is_empty());
    }

    #[test]
    fn parse_valid_step_by_step() {
        let cot = CotParser::parse(SAMPLE).expect("Should parse valid CoT");
        assert_eq!(cot.step_count(), 3);
        assert_eq!(cot.steps[0].title, "Understand the task");
        assert!(cot.has_conclusion());
        assert!(cot.conclusion.contains("centred card"));
    }

    #[test]
    fn parse_missing_conclusion_errors() {
        let text = "Step 1: Think\nSome content.";
        let result = CotParser::parse(text);
        assert_eq!(result.unwrap_err(), CotParseError::MissingConclusion);
    }

    #[test]
    fn parse_no_steps_errors() {
        let result = CotParser::parse("Just some random text without any steps.");
        assert_eq!(result.unwrap_err(), CotParseError::NoStepsFound);
    }

    #[test]
    fn parse_lenient_never_errors() {
        let cot = CotParser::parse_lenient("Random text");
        assert_eq!(cot.step_count(), 1);
        assert!(cot.has_conclusion());
    }

    #[test]
    fn chain_of_thought_to_text_format() {
        let mut cot = ChainOfThought::new();
        cot.add_step("Analyse", "Break the problem down.");
        cot.add_step("Design", "Choose a layout.");
        cot.set_conclusion("Use a two-column layout.");
        let text = cot.to_text();
        assert!(text.contains("Step 1:"));
        assert!(text.contains("Step 2:"));
        assert!(text.contains("Conclusion:"));
    }

    #[test]
    fn chain_step_indexes_are_sequential() {
        let mut cot = ChainOfThought::new();
        cot.add_step("A", ""); cot.add_step("B", ""); cot.add_step("C", "");
        assert_eq!(cot.steps[0].index, 1);
        assert_eq!(cot.steps[2].index, 3);
    }

    #[test]
    fn cot_instruction_sets_metadata() {
        let p = Prompt::new().user("task");
        let wrapped = CotInstruction::new(CotStrategy::TaskDecomposition).wrap(p);
        assert_eq!(wrapped.metadata.get("cot_strategy"), Some(&"TaskDecomposition".to_string()));
    }
}
