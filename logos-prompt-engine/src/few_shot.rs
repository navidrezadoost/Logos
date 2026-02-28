//! Few-shot example library — curated (task, response) pairs injected into prompts
//! to steer the model toward correct output format and quality.

use crate::prompt::{Message, Prompt, Role};
use serde::{Deserialize, Serialize};

// ── Task domain ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaskDomain {
    Layout,
    Colors,
    Typography,
    Accessibility,
    Export,
    Code,
    Animation,
    Grouping,
    Custom(String),
}

impl TaskDomain {
    pub fn label(&self) -> &str {
        match self {
            Self::Layout       => "Layout",
            Self::Colors       => "Colors",
            Self::Typography   => "Typography",
            Self::Accessibility => "Accessibility",
            Self::Export       => "Export",
            Self::Code         => "Code",
            Self::Animation    => "Animation",
            Self::Grouping     => "Grouping",
            Self::Custom(s)    => s.as_str(),
        }
    }
}

// ── Difficulty ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Difficulty {
    Easy = 0,
    Medium = 1,
    Hard = 2,
}

impl Default for Difficulty { fn default() -> Self { Self::Medium } }

// ── Few-shot example ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExampleTurn {
    pub role: Role,
    pub content: String,
}

impl ExampleTurn {
    pub fn user(content: impl Into<String>) -> Self { Self { role: Role::User, content: content.into() } }
    pub fn assistant(content: impl Into<String>) -> Self { Self { role: Role::Assistant, content: content.into() } }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FewShotExample {
    pub id: String,
    pub domain: TaskDomain,
    pub difficulty: Difficulty,
    pub description: String,
    pub turns: Vec<ExampleTurn>,
    pub tags: Vec<String>,
}

impl FewShotExample {
    pub fn new(
        id: impl Into<String>,
        domain: TaskDomain,
        difficulty: Difficulty,
        description: impl Into<String>,
        turns: Vec<ExampleTurn>,
    ) -> Self {
        Self {
            id: id.into(),
            domain,
            difficulty,
            description: description.into(),
            turns,
            tags: Vec::new(),
        }
    }

    pub fn with_tags(mut self, tags: &[&str]) -> Self {
        self.tags = tags.iter().map(|s| s.to_string()).collect(); self
    }

    pub fn turn_count(&self) -> usize { self.turns.len() }

    pub fn matches_domain(&self, domain: &TaskDomain) -> bool { &self.domain == domain }
    pub fn has_tag(&self, tag: &str) -> bool { self.tags.iter().any(|t| t == tag) }
}

// ── Example library ───────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct ExampleLibrary {
    examples: Vec<FewShotExample>,
}

impl ExampleLibrary {
    pub fn new() -> Self { Self::default() }

    /// Create a library pre-populated with curated built-in examples.
    pub fn with_builtins() -> Self {
        let mut lib = Self::new();
        lib.load_builtins();
        lib
    }

    pub fn add(&mut self, example: FewShotExample) {
        self.examples.push(example);
    }

    pub fn count(&self) -> usize { self.examples.len() }

    pub fn find_by_domain(&self, domain: &TaskDomain, max_n: usize) -> Vec<&FewShotExample> {
        let mut found: Vec<&FewShotExample> = self.examples.iter()
            .filter(|e| e.matches_domain(domain))
            .collect();
        // Return easiest first so simpler examples appear before complex ones
        found.sort_by_key(|e| e.difficulty as u8);
        found.truncate(max_n);
        found
    }

    pub fn find_by_tags(&self, tags: &[&str], max_n: usize) -> Vec<&FewShotExample> {
        let mut found: Vec<&FewShotExample> = self.examples.iter()
            .filter(|e| tags.iter().all(|t| e.has_tag(t)))
            .collect();
        found.truncate(max_n);
        found
    }

    pub fn best_for(
        &self,
        domain: &TaskDomain,
        difficulty: Difficulty,
        max_n: usize,
    ) -> Vec<&FewShotExample> {
        let mut found: Vec<&FewShotExample> = self.examples.iter()
            .filter(|e| e.matches_domain(domain) && e.difficulty == difficulty)
            .collect();
        found.truncate(max_n);
        found
    }

    pub fn domains(&self) -> Vec<&TaskDomain> {
        let mut domains: Vec<&TaskDomain> = self.examples.iter().map(|e| &e.domain).collect();
        domains.dedup();
        domains
    }

    /// Inject examples as user/assistant turn pairs at the start of a prompt
    /// (after any existing system message).
    pub fn inject_into(&self, prompt: Prompt, examples: &[&FewShotExample]) -> Prompt {
        let mut all_messages = Vec::new();

        // Keep system messages first
        for m in &prompt.messages {
            if m.role == Role::System {
                all_messages.push(m.clone());
            }
        }

        // Inject example turns
        for ex in examples {
            for turn in &ex.turns {
                all_messages.push(Message {
                    role: turn.role.clone(),
                    content: turn.content.clone(),
                });
            }
        }

        // Append remaining non-system messages (the real task turns)
        for m in &prompt.messages {
            if m.role != Role::System {
                all_messages.push(m.clone());
            }
        }

        Prompt { messages: all_messages, ..prompt.clone() }
    }

    // ── Curated examples ──────────────────────────────────────────────────

    fn load_builtins(&mut self) {
        // ── Layout
        self.add(FewShotExample::new(
            "layout-basic-01", TaskDomain::Layout, Difficulty::Easy,
            "Create a simple two-column layout",
            vec![
                ExampleTurn::user("Create a two-column layout with a sidebar."),
                ExampleTurn::assistant(
                    "I'll create a two-column layout with a left sidebar (160 px, layer: \
                     'Sidebar') and a content area (flex-grow: 1, layer: 'Content'). \
                     Both are wrapped in a parent Frame 'Root' with flex-direction: row."
                ),
            ],
        ).with_tags(&["layout", "flex", "sidebar"]));

        self.add(FewShotExample::new(
            "layout-grid-02", TaskDomain::Layout, Difficulty::Medium,
            "Implement a 12-column grid system",
            vec![
                ExampleTurn::user("Set up a 12-column grid at 1440 px viewport width."),
                ExampleTurn::assistant(
                    "Setting up a 12-column grid: container width 1440 px, \
                     column width 80 px, gutter 24 px (11 gutters = 264 px), \
                     total padding 48 px each side. Layer: 'Grid/Container'."
                ),
            ],
        ).with_tags(&["layout", "grid", "responsive"]));

        // ── Colors
        self.add(FewShotExample::new(
            "colors-palette-01", TaskDomain::Colors, Difficulty::Easy,
            "Apply a primary color palette",
            vec![
                ExampleTurn::user("Apply a blue primary palette with accessible contrast."),
                ExampleTurn::assistant(
                    "Applying blue primary palette:\n\
                     - Primary/500: #2563EB (AAA on white)\n\
                     - Primary/700: #1D4ED8 (hover state)\n\
                     - Primary/100: #DBEAFE (tint / backgrounds)\n\
                     Contrast ratio Primary/500 on white = 5.21:1 — passes AA."
                ),
            ],
        ).with_tags(&["colors", "palette", "contrast"]));

        self.add(FewShotExample::new(
            "colors-dark-mode-02", TaskDomain::Colors, Difficulty::Medium,
            "Create a dark-mode color set",
            vec![
                ExampleTurn::user("Generate a dark-mode version of our current light palette."),
                ExampleTurn::assistant(
                    "Dark-mode mapping:\n\
                     - Background: #0F172A (was #FFFFFF)\n\
                     - Surface: #1E293B (was #F8FAFC)\n\
                     - On-Surface text: #F1F5F9 (was #0F172A)\n\
                     - Primary: #60A5FA (was #2563EB — lightened for contrast)"
                ),
            ],
        ).with_tags(&["colors", "dark-mode"]));

        // ── Accessibility
        self.add(FewShotExample::new(
            "a11y-contrast-01", TaskDomain::Accessibility, Difficulty::Easy,
            "Check and fix contrast ratio",
            vec![
                ExampleTurn::user("Check if grey body text on white passes WCAG AA."),
                ExampleTurn::assistant(
                    "Computing contrast ratio for #6B7280 on #FFFFFF:\n\
                     Relative luminance text = 0.186, background = 1.0\n\
                     Ratio = (1.0 + 0.05) / (0.186 + 0.05) = 4.44:1\n\
                     WCAG AA requires 4.5:1 for normal text — FAILS by 0.06:1.\n\
                     Recommendation: darken to #6B7180 → ratio 4.52:1 ✓"
                ),
            ],
        ).with_tags(&["accessibility", "contrast", "wcag"]));

        self.add(FewShotExample::new(
            "a11y-aria-02", TaskDomain::Accessibility, Difficulty::Medium,
            "Add ARIA labels to icon buttons",
            vec![
                ExampleTurn::user("Our icon-only close button needs accessibility."),
                ExampleTurn::assistant(
                    "Adding aria-label=\"Close dialog\" and role=\"button\" to \
                     the close icon layer. Also adding tabIndex=0 for keyboard \
                     navigation and a focus ring (outline: 2px solid #2563EB, \
                     offset: 2px)."
                ),
            ],
        ).with_tags(&["accessibility", "aria", "keyboard"]));

        // ── Export
        self.add(FewShotExample::new(
            "export-svg-01", TaskDomain::Export, Difficulty::Easy,
            "Export artboard as optimised SVG",
            vec![
                ExampleTurn::user("Export the icons artboard as SVG with clean output."),
                ExampleTurn::assistant(
                    "Exporting 'Icons' artboard as SVG:\n\
                     - Flatten groups: yes\n\
                     - Remove invisible layers: yes\n\
                     - Inline styles: no (use class attributes)\n\
                     - Decimal precision: 2\n\
                     File: icons.svg, 12.4 KB"
                ),
            ],
        ).with_tags(&["export", "svg"]));

        // ── Typography
        self.add(FewShotExample::new(
            "typography-scale-01", TaskDomain::Typography, Difficulty::Easy,
            "Define a modular type scale",
            vec![
                ExampleTurn::user("Create a type scale based on a 1.25 major-third ratio."),
                ExampleTurn::assistant(
                    "Modular type scale (base 16 px, ratio 1.25):\n\
                     - xs: 10 px  | sm: 12 px  | base: 16 px\n\
                     - lg: 20 px  | xl: 24 px  | 2xl: 32 px\n\
                     - 3xl: 40 px | 4xl: 48 px\n\
                     Line-height: 1.5 for body, 1.2 for headings."
                ),
            ],
        ).with_tags(&["typography", "scale"]));
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_library_has_examples() {
        let lib = ExampleLibrary::with_builtins();
        assert!(lib.count() > 0, "Should have built-in examples");
    }

    #[test]
    fn find_by_domain_returns_correct_subset() {
        let lib = ExampleLibrary::with_builtins();
        let layout = lib.find_by_domain(&TaskDomain::Layout, 10);
        assert!(!layout.is_empty());
        assert!(layout.iter().all(|e| e.domain == TaskDomain::Layout));
    }

    #[test]
    fn find_by_domain_respects_max_n() {
        let lib = ExampleLibrary::with_builtins();
        let result = lib.find_by_domain(&TaskDomain::Layout, 1);
        assert!(result.len() <= 1);
    }

    #[test]
    fn find_by_tags() {
        let lib = ExampleLibrary::with_builtins();
        let tagged = lib.find_by_tags(&["accessibility", "contrast"], 10);
        assert!(!tagged.is_empty());
        assert!(tagged.iter().all(|e| e.has_tag("accessibility") && e.has_tag("contrast")));
    }

    #[test]
    fn best_for_filters_difficulty() {
        let lib = ExampleLibrary::with_builtins();
        let easy = lib.best_for(&TaskDomain::Colors, Difficulty::Easy, 10);
        assert!(easy.iter().all(|e| e.difficulty == Difficulty::Easy));
    }

    #[test]
    fn inject_into_preserves_system_message() {
        let lib = ExampleLibrary::with_builtins();
        let base = Prompt::new()
            .system("You are an expert design agent.")
            .user("Design a form.");
        let examples = lib.find_by_domain(&TaskDomain::Layout, 1);
        let injected = lib.inject_into(base, &examples);
        assert_eq!(injected.messages[0].role, Role::System);
        assert!(injected.message_count() > 2); // system + example + user
    }

    #[test]
    fn inject_appends_original_user_task_last() {
        let lib = ExampleLibrary::with_builtins();
        let base = Prompt::new()
            .system("sys")
            .user("My real task");
        let examples = lib.find_by_domain(&TaskDomain::Colors, 1);
        let injected = lib.inject_into(base, &examples);
        let last = injected.messages.last().unwrap();
        assert_eq!(last.content, "My real task");
    }

    #[test]
    fn difficulty_ordering() {
        assert!(Difficulty::Easy < Difficulty::Medium);
        assert!(Difficulty::Medium < Difficulty::Hard);
    }

    #[test]
    fn find_by_domain_sorted_by_difficulty_ascending() {
        let mut lib = ExampleLibrary::new();
        lib.add(FewShotExample::new("h", TaskDomain::Code, Difficulty::Hard,   "h", vec![]));
        lib.add(FewShotExample::new("e", TaskDomain::Code, Difficulty::Easy,   "e", vec![]));
        lib.add(FewShotExample::new("m", TaskDomain::Code, Difficulty::Medium, "m", vec![]));
        let sorted = lib.find_by_domain(&TaskDomain::Code, 10);
        assert_eq!(sorted[0].difficulty, Difficulty::Easy);
        assert_eq!(sorted[1].difficulty, Difficulty::Medium);
        assert_eq!(sorted[2].difficulty, Difficulty::Hard);
    }
}
