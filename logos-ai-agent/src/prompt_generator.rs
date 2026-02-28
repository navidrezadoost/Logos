//! Prompt Generator — curriculum prompts that teach an external LLM about Logos
//!
//! Generates a structured curriculum with versioned prompts for each module,
//! covering document model, commands, spreadsheet, plugins, design patterns,
//! and collaboration. These prompts are sent during the training session.

use serde::{Deserialize, Serialize};

// ── Curriculum modules ────────────────────────────────────────────────────────

/// Major knowledge areas an agent must learn.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CurriculumModule {
    /// Document model: layers, pages, CRDT, transactions.
    DocumentModel,
    /// Available commands: create, update, delete, move, group.
    Commands,
    /// Spreadsheet: formulas, cells, data binding to layers.
    Spreadsheet,
    /// Plugin system: WASM/JS runtimes, host API, permissions.
    Plugins,
    /// Design patterns: alignment, accessibility, color harmony, constraints.
    DesignPatterns,
    /// Real-time collaboration: CRDT conflicts, presence, cursor sync.
    Collaboration,
    /// AI features: design suggestions, component recommendations, pipeline.
    AiFeatures,
}

impl CurriculumModule {
    pub fn display_name(&self) -> &str {
        match self {
            CurriculumModule::DocumentModel => "Document Model",
            CurriculumModule::Commands => "Commands",
            CurriculumModule::Spreadsheet => "Spreadsheet & Data Binding",
            CurriculumModule::Plugins => "Plugin System",
            CurriculumModule::DesignPatterns => "Design Patterns",
            CurriculumModule::Collaboration => "Collaboration",
            CurriculumModule::AiFeatures => "AI Features",
        }
    }

    /// Estimated training time for this module (seconds).
    pub fn estimated_time_secs(&self) -> u32 {
        match self {
            CurriculumModule::DocumentModel => 45,
            CurriculumModule::Commands => 60,
            CurriculumModule::Spreadsheet => 40,
            CurriculumModule::Plugins => 30,
            CurriculumModule::DesignPatterns => 35,
            CurriculumModule::Collaboration => 25,
            CurriculumModule::AiFeatures => 25,
        }
    }

    /// Points value in the final exam.
    pub fn exam_weight(&self) -> u32 {
        match self {
            CurriculumModule::DocumentModel => 25,
            CurriculumModule::Commands => 25,
            CurriculumModule::Spreadsheet => 20,
            CurriculumModule::Plugins => 10,
            CurriculumModule::DesignPatterns => 10,
            CurriculumModule::Collaboration => 5,
            CurriculumModule::AiFeatures => 5,
        }
    }
}

// ── Prompt difficulty ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PromptDifficulty {
    Intro,        // System prompt / context
    Basic,        // Core concept explanation
    Intermediate, // Worked example
    Advanced,     // Nuanced behavior
}

// ── Training prompt ───────────────────────────────────────────────────────────

/// A single prompt in the curriculum.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingPrompt {
    pub id: String,
    pub module: CurriculumModule,
    pub difficulty: PromptDifficulty,
    /// The full text sent to the agent.
    pub content: String,
    /// Optional expected response pattern (for verification).
    pub expected_keywords: Vec<String>,
}

impl TrainingPrompt {
    pub fn new(
        id: impl Into<String>,
        module: CurriculumModule,
        difficulty: PromptDifficulty,
        content: impl Into<String>,
    ) -> Self {
        TrainingPrompt {
            id: id.into(),
            module,
            difficulty,
            content: content.into(),
            expected_keywords: vec![],
        }
    }

    pub fn with_keywords(mut self, kws: Vec<&str>) -> Self {
        self.expected_keywords = kws.iter().map(|s| s.to_string()).collect();
        self
    }

    /// Word count of the prompt content.
    pub fn word_count(&self) -> usize {
        self.content.split_whitespace().count()
    }
}

// ── Prompt template ───────────────────────────────────────────────────────────

/// Reusable template for generating prompts with variable substitution.
#[derive(Debug, Clone)]
pub struct PromptTemplate {
    pub template: String,
}

impl PromptTemplate {
    pub fn new(template: impl Into<String>) -> Self {
        PromptTemplate { template: template.into() }
    }

    /// Render the template by replacing `{key}` with `value`.
    pub fn render(&self, vars: &[(&str, &str)]) -> String {
        let mut result = self.template.clone();
        for (key, val) in vars {
            result = result.replace(&format!("{{{}}}", key), val);
        }
        result
    }
}

// ── Generator config ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct GeneratorConfig {
    /// Which modules to include in the curriculum.
    pub modules: Vec<CurriculumModule>,
    /// Whether to include advanced prompts.
    pub include_advanced: bool,
    /// App version string to embed in prompts.
    pub logos_version: String,
}

impl Default for GeneratorConfig {
    fn default() -> Self {
        GeneratorConfig {
            modules: vec![
                CurriculumModule::DocumentModel,
                CurriculumModule::Commands,
                CurriculumModule::Spreadsheet,
                CurriculumModule::Plugins,
                CurriculumModule::DesignPatterns,
                CurriculumModule::Collaboration,
                CurriculumModule::AiFeatures,
            ],
            include_advanced: true,
            logos_version: "2.0.0".to_string(),
        }
    }
}

// ── Curriculum ────────────────────────────────────────────────────────────────

/// Ordered list of training prompts forming the full curriculum.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Curriculum {
    pub version: String,
    pub prompts: Vec<TrainingPrompt>,
}

impl Curriculum {
    pub fn new(version: impl Into<String>, prompts: Vec<TrainingPrompt>) -> Self {
        Curriculum { version: version.into(), prompts }
    }

    /// Total estimated training time.
    pub fn total_time_secs(&self) -> u32 {
        self.prompts.iter().map(|p| {
            match p.difficulty {
                PromptDifficulty::Intro => 5,
                PromptDifficulty::Basic => 15,
                PromptDifficulty::Intermediate => 20,
                PromptDifficulty::Advanced => 30,
            }
        }).sum()
    }

    /// Number of prompts per module.
    pub fn prompts_for_module(&self, module: &CurriculumModule) -> Vec<&TrainingPrompt> {
        self.prompts.iter().filter(|p| &p.module == module).collect()
    }

    /// Total word count (helps estimate token usage).
    pub fn total_words(&self) -> usize {
        self.prompts.iter().map(|p| p.word_count()).sum()
    }
}

// ── Prompt Generator ──────────────────────────────────────────────────────────

pub struct PromptGenerator {
    config: GeneratorConfig,
}

impl PromptGenerator {
    pub fn new(config: GeneratorConfig) -> Self {
        PromptGenerator { config }
    }

    /// Generate the full curriculum for onboarding an external agent.
    pub fn generate_curriculum(&self) -> Curriculum {
        let mut prompts = Vec::new();

        // System intro
        prompts.push(self.system_intro());

        for module in &self.config.modules {
            prompts.extend(self.prompts_for_module(module));
        }

        Curriculum::new(format!("logos-{}", self.config.logos_version), prompts)
    }

    fn system_intro(&self) -> TrainingPrompt {
        let ver = &self.config.logos_version;
        TrainingPrompt::new(
            "sys-intro",
            CurriculumModule::DocumentModel,
            PromptDifficulty::Intro,
            format!(
                "You are an AI assistant integrated into Logos v{ver}, a professional \
                vector design tool. Your role is to help users create, edit, and organize \
                designs. You will receive commands in JSON format and must respond with \
                structured JSON actions. You have access to the full Logos document model, \
                command set, and AI features. Always respond in valid JSON. When uncertain, \
                ask a clarifying question rather than guessing. Safety: never delete layers \
                without explicit user confirmation."
            ),
        )
    }

    fn prompts_for_module(&self, module: &CurriculumModule) -> Vec<TrainingPrompt> {
        let mut out = Vec::new();
        match module {
            CurriculumModule::DocumentModel => {
                out.push(TrainingPrompt::new(
                    "doc-model-1",
                    CurriculumModule::DocumentModel,
                    PromptDifficulty::Basic,
                    "## Document Model\n\
                     A Logos document consists of Pages, and each page contains Layers.\n\
                     Layers can be: Frame, Rectangle, Ellipse, Text, Path, Group, Image, or Component.\n\
                     Each layer has a unique LayerId (UUID), a parent, bounds (x, y, width, height),\n\
                     a LayerStyle (fills, strokes, shadows), and metadata (name, visible, locked).\n\
                     Changes are tracked as Transactions (batch atomic operations) and synchronized\n\
                     via CRDT (Conflict-free Replicated Data Type) for real-time collaboration.\n\
                     Example JSON layer:\n\
                     {\"id\": \"abc\", \"kind\": \"Rectangle\", \"x\": 100, \"y\": 100, \"width\": 200, \"height\": 50, \
                     \"fill\": \"#3b82f6\", \"name\": \"Button\"}"
                ).with_keywords(vec!["Layer", "Transaction", "CRDT", "LayerId"]));

                out.push(TrainingPrompt::new(
                    "doc-model-2",
                    CurriculumModule::DocumentModel,
                    PromptDifficulty::Intermediate,
                    "## Layer Style\n\
                     A LayerStyle includes: fills (solid/gradient), strokes (color, width, position),\n\
                     shadows (drop/inner), opacity (0-1), corner_radii ([f32;4]), and blend_mode.\n\
                     Fills can be Solid {color: Color} or Gradient {stops: Vec<GradientStop>}.\n\
                     Colors are RGBA floats: Color {r: 0.0-1.0, g: 0.0-1.0, b: 0.0-1.0, a: 0.0-1.0}.\n\
                     Example: blue fill = Color { r: 0.231, g: 0.510, b: 0.965, a: 1.0 }"
                ).with_keywords(vec!["fill", "stroke", "opacity", "Color"]));

                if self.config.include_advanced {
                    out.push(TrainingPrompt::new(
                        "doc-model-3",
                        CurriculumModule::DocumentModel,
                        PromptDifficulty::Advanced,
                        "## CRDT Conflict Resolution\n\
                         When two users edit the same layer simultaneously, Logos uses vector clocks\n\
                         to determine chronological ordering. Last-Write-Wins semantics apply to scalar\n\
                         properties. For structural changes (layer add/delete), CRDT ensures both\n\
                         operations are preserved (add wins over delete). Delta encoding reduces\n\
                         bandwidth by only transmitting changed properties, not full layer state.\n\
                         When you detect a conflict in your response, always include a\n\
                         'conflict_resolution' field in your JSON output."
                    ));
                }
            }

            CurriculumModule::Commands => {
                out.push(TrainingPrompt::new(
                    "cmd-1",
                    CurriculumModule::Commands,
                    PromptDifficulty::Basic,
                    "## Commands\n\
                     All Logos operations are expressed as JSON commands. Core commands:\n\
                     - create_layer: {type, x, y, width, height, name, fill}\n\
                     - update_layer: {id, properties...}\n\
                     - delete_layer: {id} (requires confirmation)\n\
                     - move_layer: {id, x, y}\n\
                     - resize_layer: {id, width, height}\n\
                     - group_layers: {ids, name}\n\
                     - set_fill: {id, fill: {type: 'solid', color: '#HEX'}}\n\
                     - set_opacity: {id, opacity: 0.0-1.0}\n\
                     Always wrap multiple commands in a transaction: {transaction: [cmd1, cmd2, ...]}"
                ).with_keywords(vec!["create_layer", "update_layer", "transaction"]));

                out.push(TrainingPrompt::new(
                    "cmd-2",
                    CurriculumModule::Commands,
                    PromptDifficulty::Intermediate,
                    "## Example: Create a styled button\n\
                     User: 'Create a blue rounded button labeled Submit at position (100, 200)'\n\
                     Response:\n\
                     {\"transaction\": [\n\
                       {\"cmd\": \"create_layer\", \"type\": \"Rectangle\", \"name\": \"Button\",\n\
                        \"x\": 100, \"y\": 200, \"width\": 120, \"height\": 40,\n\
                        \"fill\": {\"type\": \"solid\", \"color\": \"#3b82f6\"},\n\
                        \"corner_radius\": 8},\n\
                       {\"cmd\": \"create_layer\", \"type\": \"Text\", \"name\": \"Button Label\",\n\
                        \"x\": 100, \"y\": 200, \"width\": 120, \"height\": 40,\n\
                        \"text\": \"Submit\", \"font_size\": 16, \"color\": \"#ffffff\",\n\
                        \"text_align\": \"center\"}\n\
                     ]}"
                ));
            }

            CurriculumModule::Spreadsheet => {
                out.push(TrainingPrompt::new(
                    "sheet-1",
                    CurriculumModule::Spreadsheet,
                    PromptDifficulty::Basic,
                    "## Spreadsheet & Data Binding\n\
                     Logos includes a spreadsheet engine with Excel-compatible formulas.\n\
                     Cells are referenced as A1, B2, etc. Formulas start with '='.\n\
                     Common functions: SUM, AVERAGE, IF, VLOOKUP, COUNT, MAX, MIN.\n\
                     Data binding: any cell value can drive a layer property.\n\
                     Syntax: @bind(layer_id, property, cell_ref)\n\
                     Example: @bind('btn-1', 'width', 'B3') — button width = cell B3 value\n\
                     Ranges: A1:A10 for a column, A1:D10 for a block."
                ));

                out.push(TrainingPrompt::new(
                    "sheet-2",
                    CurriculumModule::Spreadsheet,
                    PromptDifficulty::Intermediate,
                    "## Example: Dynamic progress bar\n\
                     User: 'Connect cell B1 (percentage) to a rectangle width'\n\
                     Assume rectangle id = 'progress-bar', canvas width = 300px.\n\
                     Response:\n\
                     {\"transaction\": [\n\
                       {\"cmd\": \"write_formula\", \"cell\": \"C1\", \"formula\": \"=B1/100*300\"},\n\
                       {\"cmd\": \"bind\", \"layer_id\": \"progress-bar\", \"property\": \"width\", \"cell\": \"C1\"}\n\
                     ]}\n\
                     This makes the bar width proportional to the B1 percentage value."
                ));
            }

            CurriculumModule::Plugins => {
                out.push(TrainingPrompt::new(
                    "plugin-1",
                    CurriculumModule::Plugins,
                    PromptDifficulty::Basic,
                    "## Plugin System\n\
                     Logos supports WASM and JavaScript plugins in sandboxed environments.\n\
                     Plugins interact via 27 Host Functions grouped in: Document, Selection,\n\
                     Viewport, UI, Lifecycle, State, and AI/ML categories.\n\
                     Key AI host functions: ai_analyze_design, ai_check_accessibility,\n\
                     ai_generate_palette, ai_infer_constraints, ai_recommend_components.\n\
                     As an AI agent, you can instruct plugins via: {cmd: 'call_plugin', plugin_id, function, args}."
                ));
            }

            CurriculumModule::DesignPatterns => {
                out.push(TrainingPrompt::new(
                    "design-1",
                    CurriculumModule::DesignPatterns,
                    PromptDifficulty::Basic,
                    "## Design Patterns\n\
                     Follow these guidelines when generating designs:\n\
                     - ALIGNMENT: Elements should align to 4px or 8px grids\n\
                     - SPACING: Consistent gaps (8, 16, 24, 32px — 8px multiples)\n\
                     - CONTRAST: Text must meet WCAG AA (4.5:1 normal, 3:1 large text)\n\
                     - TOUCH TARGETS: Interactive elements ≥44×44px (WCAG AAA)\n\
                     - COLOR HARMONY: Use complementary or triadic color schemes\n\
                     - TYPOGRAPHY: Body ≥16px, line-height ≥1.4, max 75 chars/line\n\
                     When creating UI, always check contrast and alignment automatically."
                ).with_keywords(vec!["alignment", "contrast", "WCAG", "spacing"]));
            }

            CurriculumModule::Collaboration => {
                out.push(TrainingPrompt::new(
                    "collab-1",
                    CurriculumModule::Collaboration,
                    PromptDifficulty::Basic,
                    "## Collaboration\n\
                     Multiple users can edit simultaneously. When you act as an agent:\n\
                     - Use transactions to batch your operations atomically\n\
                     - Never assume your changes are the only ones in flight\n\
                     - If a layer you want to edit has been modified since you read it,\n\
                       re-fetch its state with: {cmd: 'get_layer', id}\n\
                     - Presence: other cursors are visible via CursorSync\n\
                     - To avoid conflicts, lock a layer before editing: {cmd: 'lock', id}\n\
                       Always unlock after: {cmd: 'unlock', id}"
                ));
            }

            CurriculumModule::AiFeatures => {
                out.push(TrainingPrompt::new(
                    "ai-1",
                    CurriculumModule::AiFeatures,
                    PromptDifficulty::Basic,
                    "## AI Features\n\
                     Logos AI provides heuristic design assistance:\n\
                     - Design Suggestions: detect alignment/spacing/overlap issues\n\
                     - Accessibility Checker: WCAG contrast, touch targets, CVD simulation\n\
                     - Color Harmony: generate palettes (complementary/triadic/analogous)\n\
                     - Smart Constraints: detect grids, rails, aspect ratios\n\
                     - Component Recommendations: find repeatable patterns\n\
                     - Pipeline: chain multiple AI steps\n\
                     Trigger via: {cmd: 'run_ai', step: 'DesignAnalysis'} or call the Rust API directly."
                ));
            }
        }
        out
    }

    /// Generate a single module's prompts (useful for targeted re-training).
    pub fn module_prompts(&self, module: &CurriculumModule) -> Vec<TrainingPrompt> {
        self.prompts_for_module(module)
    }

    /// Generate a minimal (intro-only) curriculum for quick testing.
    pub fn quick_curriculum(&self) -> Curriculum {
        let prompts = vec![
            self.system_intro(),
            self.prompts_for_module(&CurriculumModule::DocumentModel).into_iter().next().unwrap(),
            self.prompts_for_module(&CurriculumModule::Commands).into_iter().next().unwrap(),
        ];
        Curriculum::new("quick", prompts)
    }
}

impl Default for PromptGenerator {
    fn default() -> Self {
        Self::new(GeneratorConfig::default())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn generator() -> PromptGenerator {
        PromptGenerator::default()
    }

    #[test]
    fn curriculum_has_prompts_for_all_modules() {
        let gen = generator();
        let curriculum = gen.generate_curriculum();
        for module in &[
            CurriculumModule::DocumentModel,
            CurriculumModule::Commands,
            CurriculumModule::Spreadsheet,
            CurriculumModule::Plugins,
            CurriculumModule::DesignPatterns,
            CurriculumModule::Collaboration,
            CurriculumModule::AiFeatures,
        ] {
            assert!(
                !curriculum.prompts_for_module(module).is_empty(),
                "No prompts for {:?}", module
            );
        }
    }

    #[test]
    fn curriculum_includes_system_intro() {
        let gen = generator();
        let c = gen.generate_curriculum();
        assert!(c.prompts.iter().any(|p| p.id == "sys-intro"));
    }

    #[test]
    fn curriculum_total_words_nonzero() {
        let gen = generator();
        let c = gen.generate_curriculum();
        assert!(c.total_words() > 200);
    }

    #[test]
    fn curriculum_total_time_under_5_minutes() {
        let gen = generator();
        let c = gen.generate_curriculum();
        assert!(c.total_time_secs() <= 300, "Curriculum too long: {}s", c.total_time_secs());
    }

    #[test]
    fn quick_curriculum_has_3_prompts() {
        let gen = generator();
        let c = gen.quick_curriculum();
        assert_eq!(c.prompts.len(), 3);
    }

    #[test]
    fn prompt_template_renders() {
        let t = PromptTemplate::new("Hello {name}, you are using Logos v{version}!");
        let rendered = t.render(&[("name", "GPT-4"), ("version", "2.0")]);
        assert_eq!(rendered, "Hello GPT-4, you are using Logos v2.0!");
    }

    #[test]
    fn prompt_template_missing_var_kept() {
        let t = PromptTemplate::new("Hello {name}!");
        let rendered = t.render(&[]);
        assert_eq!(rendered, "Hello {name}!");
    }

    #[test]
    fn prompt_word_count() {
        let p = TrainingPrompt::new("x", CurriculumModule::Commands, PromptDifficulty::Basic, "one two three");
        assert_eq!(p.word_count(), 3);
    }

    #[test]
    fn module_exam_weights_sum_to_100() {
        let modules = [
            CurriculumModule::DocumentModel,
            CurriculumModule::Commands,
            CurriculumModule::Spreadsheet,
            CurriculumModule::Plugins,
            CurriculumModule::DesignPatterns,
            CurriculumModule::Collaboration,
            CurriculumModule::AiFeatures,
        ];
        let total: u32 = modules.iter().map(|m| m.exam_weight()).sum();
        assert_eq!(total, 100);
    }

    #[test]
    fn advanced_prompts_excluded_when_configured() {
        let config = GeneratorConfig { include_advanced: false, ..Default::default() };
        let gen = PromptGenerator::new(config);
        let c = gen.generate_curriculum();
        let advanced: Vec<_> = c.prompts.iter()
            .filter(|p| p.difficulty == PromptDifficulty::Advanced)
            .collect();
        assert!(advanced.is_empty());
    }

    #[test]
    fn advanced_prompts_included_by_default() {
        let gen = generator();
        let c = gen.generate_curriculum();
        let advanced: Vec<_> = c.prompts.iter()
            .filter(|p| p.difficulty == PromptDifficulty::Advanced)
            .collect();
        assert!(!advanced.is_empty());
    }

    #[test]
    fn document_model_prompts_have_keywords() {
        let gen = generator();
        let c = gen.generate_curriculum();
        let doc_prompts = c.prompts_for_module(&CurriculumModule::DocumentModel);
        let any_has_keywords = doc_prompts.iter().any(|p| !p.expected_keywords.is_empty());
        assert!(any_has_keywords);
    }

    #[test]
    fn commands_prompt_mentions_json() {
        let gen = generator();
        let module_prompts = gen.module_prompts(&CurriculumModule::Commands);
        let mentions_json = module_prompts.iter().any(|p| p.content.contains("JSON") || p.content.contains("json"));
        assert!(mentions_json);
    }

    #[test]
    fn curriculum_version_contains_logos() {
        let gen = generator();
        let c = gen.generate_curriculum();
        assert!(c.version.contains("logos"));
    }

    #[test]
    fn design_patterns_prompt_mentions_wcag() {
        let gen = generator();
        let module_prompts = gen.module_prompts(&CurriculumModule::DesignPatterns);
        let mentions_wcag = module_prompts.iter().any(|p| p.content.contains("WCAG"));
        assert!(mentions_wcag);
    }

    #[test]
    fn spreadsheet_prompt_mentions_formula() {
        let gen = generator();
        let module_prompts = gen.module_prompts(&CurriculumModule::Spreadsheet);
        let has_formula = module_prompts.iter().any(|p| p.content.contains("formula") || p.content.contains("Formula"));
        assert!(has_formula);
    }

    #[test]
    fn module_display_names_nonempty() {
        let modules = vec![
            CurriculumModule::DocumentModel,
            CurriculumModule::Commands,
            CurriculumModule::Spreadsheet,
        ];
        for m in modules {
            assert!(!m.display_name().is_empty());
        }
    }

    #[test]
    fn custom_config_modules_subset() {
        let config = GeneratorConfig {
            modules: vec![CurriculumModule::DocumentModel, CurriculumModule::Commands],
            include_advanced: true,
            logos_version: "2.0.0".into(),
        };
        let gen = PromptGenerator::new(config);
        let c = gen.generate_curriculum();
        // Should NOT have spreadsheet prompts
        assert!(c.prompts_for_module(&CurriculumModule::Spreadsheet).is_empty());
        // Should have doc model and commands
        assert!(!c.prompts_for_module(&CurriculumModule::DocumentModel).is_empty());
    }
}
