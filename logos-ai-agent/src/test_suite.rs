//! Test Suite — graded evaluation tests for external AI agents
//!
//! 50+ test cases spanning Simple → Intermediate → Complex → Collaboration.
//! Each test presents a natural language prompt and evaluates whether the
//! agent's JSON response contains the expected commands/keywords.

use serde::{Deserialize, Serialize};

// ── Test level ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TestLevel {
    /// Core layer operations (expected by Junior+).
    Simple,
    /// Formulas, plugins, styling (expected by MidLevel+).
    Intermediate,
    /// Multi-step workflows, CRDT, AI pipeline (expected by Senior).
    Complex,
    /// Real-time collaboration scenarios (Senior bonus).
    Collaboration,
}

impl TestLevel {
    pub fn point_value(&self) -> u32 {
        match self {
            TestLevel::Simple => 1,
            TestLevel::Intermediate => 2,
            TestLevel::Complex => 4,
            TestLevel::Collaboration => 3,
        }
    }

    pub fn description(&self) -> &str {
        match self {
            TestLevel::Simple => "Basic layer & property commands",
            TestLevel::Intermediate => "Formulas, plugins, design patterns",
            TestLevel::Complex => "Multi-step workflows and AI features",
            TestLevel::Collaboration => "CRDT and real-time collaboration",
        }
    }
}

// ── Test category ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TestCategory {
    Layers,
    Styling,
    Text,
    Groups,
    Spreadsheet,
    Plugins,
    AiFeatures,
    Accessibility,
    Workflow,
    Collaboration,
}

// ── Test case ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCase {
    pub id: usize,
    pub level: TestLevel,
    pub category: TestCategory,
    /// Natural language prompt sent to the agent.
    pub prompt: String,
    /// Keywords that MUST appear in the agent's response.
    pub expected_keywords: Vec<String>,
    /// If set, the JSON command name expected in the response.
    pub expected_command: Option<String>,
    /// Maximum points for this test.
    pub max_points: u32,
    /// Maximum allowed response latency in ms.
    pub time_limit_ms: u64,
}

impl TestCase {
    pub fn new(
        id: usize,
        level: TestLevel,
        category: TestCategory,
        prompt: impl Into<String>,
        expected_keywords: Vec<&str>,
        expected_command: Option<&str>,
    ) -> Self {
        let max_points = level.point_value();
        TestCase {
            id,
            level,
            category,
            prompt: prompt.into(),
            expected_keywords: expected_keywords.iter().map(|s| s.to_string()).collect(),
            expected_command: expected_command.map(|s| s.to_string()),
            max_points,
            time_limit_ms: 5000,
        }
    }
}

// ── Test result ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub test_id: usize,
    pub level: TestLevel,
    pub response: String,
    pub keywords_found: Vec<String>,
    pub keywords_missing: Vec<String>,
    pub command_matched: bool,
    pub points_earned: u32,
    pub max_points: u32,
    pub passed: bool,
    pub latency_ms: u64,
    pub feedback: String,
}

impl TestResult {
    pub fn score_pct(&self) -> f32 {
        if self.max_points == 0 { return 100.0; }
        self.points_earned as f32 / self.max_points as f32 * 100.0
    }
}

// ── Test runner ───────────────────────────────────────────────────────────────

pub struct TestRunner;

impl TestRunner {
    /// Evaluate an agent's response against a test case.
    pub fn evaluate(response: &str, case: &TestCase, latency_ms: u64) -> TestResult {
        let resp_lower = response.to_lowercase();

        let mut found = Vec::new();
        let mut missing = Vec::new();
        for kw in &case.expected_keywords {
            if resp_lower.contains(&kw.to_lowercase()) {
                found.push(kw.clone());
            } else {
                missing.push(kw.clone());
            }
        }

        let command_matched = case.expected_command.as_ref()
            .map(|cmd| resp_lower.contains(&cmd.to_lowercase()))
            .unwrap_or(true); // no expected command → auto-pass

        let keyword_score = if case.expected_keywords.is_empty() {
            1.0
        } else {
            found.len() as f32 / case.expected_keywords.len() as f32
        };

        let cmd_score = if command_matched { 1.0_f32 } else { 0.5 };
        let latency_ok = latency_ms <= case.time_limit_ms;
        let latency_factor = if latency_ok { 1.0_f32 } else { 0.8 };

        let raw_score = keyword_score * cmd_score * latency_factor;
        let points = (case.max_points as f32 * raw_score).round() as u32;
        // Latency violation always prevents passing (though partial credit may still be given)
        let passed = points >= case.max_points && missing.is_empty() && latency_ok;

        let feedback = if passed {
            "All criteria met.".to_string()
        } else {
            let mut parts = Vec::new();
            if !missing.is_empty() {
                parts.push(format!("Missing keywords: {}", missing.join(", ")));
            }
            if !command_matched {
                parts.push(format!("Expected command '{}' not found", case.expected_command.as_deref().unwrap_or("")));
            }
            if !latency_ok {
                parts.push(format!("Response too slow ({}ms > {}ms)", latency_ms, case.time_limit_ms));
            }
            parts.join("; ")
        };

        TestResult {
            test_id: case.id,
            level: case.level.clone(),
            response: response.to_string(),
            keywords_found: found,
            keywords_missing: missing,
            command_matched,
            points_earned: points,
            max_points: case.max_points,
            passed,
            latency_ms,
            feedback,
        }
    }
}

// ── Built-in test suite ───────────────────────────────────────────────────────

pub struct TestSuite {
    pub cases: Vec<TestCase>,
}

impl TestSuite {
    pub fn new(cases: Vec<TestCase>) -> Self {
        TestSuite { cases }
    }

    pub fn by_level(&self, level: &TestLevel) -> Vec<&TestCase> {
        self.cases.iter().filter(|c| &c.level == level).collect()
    }

    pub fn total_max_points(&self) -> u32 {
        self.cases.iter().map(|c| c.max_points).sum()
    }

    pub fn case_count(&self) -> usize {
        self.cases.len()
    }

    pub fn get(&self, id: usize) -> Option<&TestCase> {
        self.cases.iter().find(|c| c.id == id)
    }
}

/// Factory for the canonical built-in Logos agent test suite (50+ cases).
pub struct BuiltinTestSuite;

impl BuiltinTestSuite {
    pub fn build() -> TestSuite {
        let mut cases = Vec::new();

        // ── SIMPLE (10 cases, 1 point each) ─────────────────────────────────

        cases.push(TestCase::new(
            1, TestLevel::Simple, TestCategory::Layers,
            "Create a rectangle at x=10, y=10, width=100, height=50",
            vec!["create_layer", "rectangle", "100", "50"],
            Some("create_layer"),
        ));
        cases.push(TestCase::new(
            2, TestLevel::Simple, TestCategory::Styling,
            "Set the fill color of the selected layer to red (#FF0000)",
            vec!["set_fill", "FF0000"],
            Some("set_fill"),
        ));
        cases.push(TestCase::new(
            3, TestLevel::Simple, TestCategory::Text,
            "Add a text layer with the content 'Hello World' at position (50, 50)",
            vec!["create_layer", "text", "Hello World"],
            Some("create_layer"),
        ));
        cases.push(TestCase::new(
            4, TestLevel::Simple, TestCategory::Styling,
            "Set the opacity of layer 'btn-1' to 50%",
            vec!["set_opacity", "0.5"],
            Some("set_opacity"),
        ));
        cases.push(TestCase::new(
            5, TestLevel::Simple, TestCategory::Layers,
            "Delete the layer with id 'old-header'",
            vec!["delete_layer", "old-header"],
            Some("delete_layer"),
        ));
        cases.push(TestCase::new(
            6, TestLevel::Simple, TestCategory::Layers,
            "Resize layer 'card' to width=200 height=100",
            vec!["resize_layer", "200", "100"],
            Some("resize_layer"),
        ));
        cases.push(TestCase::new(
            7, TestLevel::Simple, TestCategory::Layers,
            "Move layer 'icon' to position x=50, y=50",
            vec!["move_layer", "50"],
            Some("move_layer"),
        ));
        cases.push(TestCase::new(
            8, TestLevel::Simple, TestCategory::Styling,
            "Set stroke width of layer 'border' to 2 pixels",
            vec!["set_stroke", "2"],
            Some("set_stroke"),
        ));
        cases.push(TestCase::new(
            9, TestLevel::Simple, TestCategory::Groups,
            "Group the layers 'icon', 'label', and 'background' into a group called 'Button'",
            vec!["group_layers", "Button"],
            Some("group_layers"),
        ));
        cases.push(TestCase::new(
            10, TestLevel::Simple, TestCategory::Groups,
            "Ungroup the group 'card-group'",
            vec!["ungroup", "card-group"],
            Some("ungroup"),
        ));

        // ── INTERMEDIATE (20 cases, 2 points each) ───────────────────────────

        cases.push(TestCase::new(
            11, TestLevel::Intermediate, TestCategory::Spreadsheet,
            "Write a formula =SUM(A1:A10) in cell B1",
            vec!["write_formula", "SUM", "A1:A10", "B1"],
            Some("write_formula"),
        ));
        cases.push(TestCase::new(
            12, TestLevel::Intermediate, TestCategory::Spreadsheet,
            "Bind cell C3 value to the width property of layer 'progress-bar'",
            vec!["bind", "C3", "width", "progress-bar"],
            Some("bind"),
        ));
        cases.push(TestCase::new(
            13, TestLevel::Intermediate, TestCategory::Plugins,
            "Call plugin 'chart-maker' function 'render' with data from cells A1:A5",
            vec!["call_plugin", "chart-maker", "render"],
            Some("call_plugin"),
        ));
        cases.push(TestCase::new(
            14, TestLevel::Intermediate, TestCategory::Styling,
            "Create a complementary color palette from base color #3b82f6",
            vec!["generate_palette", "complementary", "3b82f6"],
            Some("generate_palette"),
        ));
        cases.push(TestCase::new(
            15, TestLevel::Intermediate, TestCategory::Accessibility,
            "Check the contrast ratio between 'title-text' (white) and 'background' (dark blue)",
            vec!["check_contrast", "title-text"],
            Some("check_contrast"),
        ));
        cases.push(TestCase::new(
            16, TestLevel::Intermediate, TestCategory::Layers,
            "Create a reusable component named 'PrimaryButton' from layers 'btn-bg' and 'btn-label'",
            vec!["create_component", "PrimaryButton"],
            Some("create_component"),
        ));
        cases.push(TestCase::new(
            17, TestLevel::Intermediate, TestCategory::Layers,
            "Enable auto-layout on frame 'nav-bar' with horizontal direction and 16px gap",
            vec!["set_auto_layout", "horizontal", "16"],
            Some("set_auto_layout"),
        ));
        cases.push(TestCase::new(
            18, TestLevel::Intermediate, TestCategory::Layers,
            "Set a fixed-width constraint on layer 'sidebar' so it stays 240px wide on window resize",
            vec!["set_constraint", "fixed", "240"],
            Some("set_constraint"),
        ));
        cases.push(TestCase::new(
            19, TestLevel::Intermediate, TestCategory::Layers,
            "Rename layer 'Rectangle 1' to 'Card Background'",
            vec!["rename_layer", "Card Background"],
            Some("rename_layer"),
        ));
        cases.push(TestCase::new(
            20, TestLevel::Intermediate, TestCategory::Layers,
            "Move layer 'tooltip' to the top of the z-order",
            vec!["reorder_layer", "top"],
            Some("reorder_layer"),
        ));
        cases.push(TestCase::new(
            21, TestLevel::Intermediate, TestCategory::Spreadsheet,
            "Write a conditional formula: =IF(B2>100, 'High', 'Low') in cell D1",
            vec!["write_formula", "IF", "B2", "High", "Low"],
            Some("write_formula"),
        ));
        cases.push(TestCase::new(
            22, TestLevel::Intermediate, TestCategory::Styling,
            "Apply a gradient fill from #3b82f6 to #1d4ed8 on layer 'hero-bg'",
            vec!["set_fill", "gradient", "3b82f6", "1d4ed8"],
            Some("set_fill"),
        ));
        cases.push(TestCase::new(
            23, TestLevel::Intermediate, TestCategory::Text,
            "Set font size of 'heading' to 32px and line height to 1.5",
            vec!["update_text", "32", "1.5"],
            Some("update_text"),
        ));
        cases.push(TestCase::new(
            24, TestLevel::Intermediate, TestCategory::Layers,
            "Lock layer 'background-pattern' to prevent accidental edits",
            vec!["lock_layer", "background-pattern"],
            Some("lock_layer"),
        ));
        cases.push(TestCase::new(
            25, TestLevel::Intermediate, TestCategory::Accessibility,
            "Run the accessibility audit on the current page",
            vec!["run_accessibility_audit"],
            Some("run_accessibility_audit"),
        ));
        cases.push(TestCase::new(
            26, TestLevel::Intermediate, TestCategory::Layers,
            "Duplicate layer 'card-template' 3 times with 24px vertical spacing",
            vec!["duplicate_layer", "3", "24"],
            Some("duplicate_layer"),
        ));
        cases.push(TestCase::new(
            27, TestLevel::Intermediate, TestCategory::AiFeatures,
            "Get design improvement suggestions for the current selection",
            vec!["analyze_design", "suggestions"],
            Some("analyze_design"),
        ));
        cases.push(TestCase::new(
            28, TestLevel::Intermediate, TestCategory::Layers,
            "Align selected layers to the horizontal center of the artboard",
            vec!["align_layers", "center", "horizontal"],
            Some("align_layers"),
        ));
        cases.push(TestCase::new(
            29, TestLevel::Intermediate, TestCategory::Spreadsheet,
            "Calculate the average of column A (A1:A20) and show it in B22",
            vec!["write_formula", "AVERAGE", "A1:A20", "B22"],
            Some("write_formula"),
        ));
        cases.push(TestCase::new(
            30, TestLevel::Intermediate, TestCategory::Workflow,
            "Export the page as a PNG at 2x resolution",
            vec!["export_page", "png", "2x"],
            Some("export_page"),
        ));

        // ── COMPLEX (15 cases, 4 points each) ───────────────────────────────

        cases.push(TestCase::new(
            31, TestLevel::Complex, TestCategory::Workflow,
            "Create a complete card UI: frame (300×200, rounded 12px), title text, subtitle text, and a primary button, all aligned with 16px padding",
            vec!["transaction", "create_layer", "frame", "button", "padding"],
            Some("transaction"),
        ));
        cases.push(TestCase::new(
            32, TestLevel::Complex, TestCategory::AiFeatures,
            "Run the full AI pipeline: analyze design, check accessibility, generate color suggestions, and recommend component patterns",
            vec!["run_pipeline", "analyze", "accessibility", "color", "component"],
            Some("run_pipeline"),
        ));
        cases.push(TestCase::new(
            33, TestLevel::Complex, TestCategory::Spreadsheet,
            "Build a data-driven bar chart: use VLOOKUP to get sales data from Sheet2 and bind each value to the height of corresponding bar layers",
            vec!["vlookup", "bind", "height", "sheet"],
            None,
        ));
        cases.push(TestCase::new(
            34, TestLevel::Complex, TestCategory::Workflow,
            "Create a responsive layout: frame with auto-layout, min-width 320px, max-width 1200px, with breakpoint overrides at 768px",
            vec!["auto_layout", "constraint", "breakpoint", "responsive"],
            None,
        ));
        cases.push(TestCase::new(
            35, TestLevel::Complex, TestCategory::AiFeatures,
            "Detect all alignment issues in the current page and automatically fix spacing to the nearest 8px grid",
            vec!["analyze_design", "alignment", "8", "fix"],
            Some("analyze_design"),
        ));
        cases.push(TestCase::new(
            36, TestLevel::Complex, TestCategory::Accessibility,
            "Run a full WCAG 2.1 AA audit: check contrast ratios, touch target sizes, and generate an accessibility report",
            vec!["accessibility", "wcag", "contrast", "touch_target", "report"],
            None,
        ));
        cases.push(TestCase::new(
            37, TestLevel::Complex, TestCategory::Workflow,
            "Create a component library: convert 5 button variants into components with proper naming and organize them in a 'Components' page",
            vec!["create_component", "variant", "page", "library"],
            None,
        ));
        cases.push(TestCase::new(
            38, TestLevel::Complex, TestCategory::Plugins,
            "Install and configure the 'iconify' plugin, call its search function to find 'arrow' icons, and add the first result to the canvas",
            vec!["install_plugin", "call_plugin", "iconify", "search"],
            None,
        ));
        cases.push(TestCase::new(
            39, TestLevel::Complex, TestCategory::Workflow,
            "Export the design as a developer handoff: generate CSS variables for colors, spacing tokens, and typography scale",
            vec!["export", "css", "variables", "token"],
            None,
        ));
        cases.push(TestCase::new(
            40, TestLevel::Complex, TestCategory::AiFeatures,
            "Analyze the design for accessibility issues, auto-fix contrast problems by adjusting colors while preserving brand guidelines",
            vec!["accessibility", "contrast", "fix", "color"],
            None,
        ));
        cases.push(TestCase::new(
            41, TestLevel::Complex, TestCategory::Spreadsheet,
            "Create a chart that updates in real-time as spreadsheet data changes, using data binding with automatic re-render triggers",
            vec!["bind", "formula", "trigger", "update"],
            None,
        ));
        cases.push(TestCase::new(
            42, TestLevel::Complex, TestCategory::Workflow,
            "Set up a design system with global color styles, text styles, and effect styles that can be applied across all pages",
            vec!["style", "global", "color", "typography"],
            None,
        ));
        cases.push(TestCase::new(
            43, TestLevel::Complex, TestCategory::Layers,
            "Create a complex mask: use an ellipse to clip a photo layer, add a gradient overlay, and apply a drop shadow to the composite",
            vec!["mask", "clip", "gradient", "shadow"],
            None,
        ));
        cases.push(TestCase::new(
            44, TestLevel::Complex, TestCategory::AiFeatures,
            "Use AI component recommendation to identify repeating patterns in the design and convert them to reusable components automatically",
            vec!["recommend_components", "pattern", "convert", "component"],
            None,
        ));
        cases.push(TestCase::new(
            45, TestLevel::Complex, TestCategory::Workflow,
            "Build a multi-page document with navigation: home, about, contact pages with consistent headers, footers, and shared styles",
            vec!["page", "navigation", "shared", "style", "header"],
            None,
        ));

        // ── COLLABORATION (5 cases, 3 points each) ───────────────────────────

        cases.push(TestCase::new(
            46, TestLevel::Collaboration, TestCategory::Collaboration,
            "Two users are editing the same layer simultaneously. Detect and resolve the conflict using last-write-wins semantics",
            vec!["conflict", "resolve", "crdt", "lock"],
            None,
        ));
        cases.push(TestCase::new(
            47, TestLevel::Collaboration, TestCategory::Collaboration,
            "Lock a layer before making changes, apply the edits, then unlock it for other collaborators",
            vec!["lock", "unlock", "transaction"],
            Some("lock"),
        ));
        cases.push(TestCase::new(
            48, TestLevel::Collaboration, TestCategory::Collaboration,
            "Merge incoming remote changes with local pending changes, preserving both additions",
            vec!["merge", "crdt", "transaction", "preserve"],
            None,
        ));
        cases.push(TestCase::new(
            49, TestLevel::Collaboration, TestCategory::Collaboration,
            "Show the current presence/cursor positions of all active collaborators",
            vec!["presence", "cursor", "collaborator"],
            Some("get_presence"),
        ));
        cases.push(TestCase::new(
            50, TestLevel::Collaboration, TestCategory::Collaboration,
            "Create a shared comment thread on layer 'hero-section' for collaborator review",
            vec!["comment", "thread", "hero-section"],
            Some("add_comment"),
        ));

        TestSuite::new(cases)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn suite() -> TestSuite {
        BuiltinTestSuite::build()
    }

    #[test]
    fn suite_has_50_cases() {
        assert_eq!(suite().case_count(), 50);
    }

    #[test]
    fn suite_has_10_simple_cases() {
        assert_eq!(suite().by_level(&TestLevel::Simple).len(), 10);
    }

    #[test]
    fn suite_has_20_intermediate_cases() {
        assert_eq!(suite().by_level(&TestLevel::Intermediate).len(), 20);
    }

    #[test]
    fn suite_has_15_complex_cases() {
        assert_eq!(suite().by_level(&TestLevel::Complex).len(), 15);
    }

    #[test]
    fn suite_has_5_collaboration_cases() {
        assert_eq!(suite().by_level(&TestLevel::Collaboration).len(), 5);
    }

    #[test]
    fn total_max_points_correct() {
        let s = suite();
        let expected = 10 * 1 + 20 * 2 + 15 * 4 + 5 * 3;
        assert_eq!(s.total_max_points(), expected);
    }

    #[test]
    fn runner_passes_perfect_response() {
        let case = TestCase::new(
            1, TestLevel::Simple, TestCategory::Layers,
            "Create a rectangle",
            vec!["create_layer", "rectangle"],
            Some("create_layer"),
        );
        let perfect = r#"{"cmd": "create_layer", "type": "rectangle", "x": 10}"#;
        let result = TestRunner::evaluate(perfect, &case, 100);
        assert!(result.passed);
        assert_eq!(result.points_earned, result.max_points);
    }

    #[test]
    fn runner_fails_empty_response() {
        let case = TestCase::new(
            2, TestLevel::Simple, TestCategory::Layers,
            "Create a rectangle",
            vec!["create_layer", "rectangle"],
            Some("create_layer"),
        );
        let result = TestRunner::evaluate("", &case, 100);
        assert!(!result.passed);
        assert_eq!(result.points_earned, 0);
    }

    #[test]
    fn runner_penalizes_slow_response() {
        let mut case = TestCase::new(
            3, TestLevel::Simple, TestCategory::Layers,
            "Create a rectangle",
            vec!["create_layer"],
            Some("create_layer"),
        );
        case.time_limit_ms = 1000; // 1 second limit
        let result = TestRunner::evaluate(r#"{"cmd": "create_layer"}"#, &case, 5000); // 5s → slow
        // Should penalize but still give partial credit
        assert!(!result.passed || result.points_earned < result.max_points || result.feedback.contains("slow"));
    }

    #[test]
    fn runner_missing_keywords_reported() {
        let case = TestCase::new(
            4, TestLevel::Simple, TestCategory::Layers,
            "Test",
            vec!["alpha", "beta", "gamma"],
            None,
        );
        let result = TestRunner::evaluate("alpha only", &case, 100);
        assert!(result.keywords_missing.contains(&"beta".to_string()));
        assert!(result.keywords_missing.contains(&"gamma".to_string()));
        assert!(!result.passed);
    }

    #[test]
    fn runner_no_expected_keywords_passes() {
        let case = TestCase::new(
            5, TestLevel::Intermediate, TestCategory::Workflow,
            "Do something",
            vec![],
            None,
        );
        let result = TestRunner::evaluate("any response here", &case, 100);
        assert!(result.passed);
    }

    #[test]
    fn score_pct_is_100_for_perfect() {
        let case = TestCase::new(
            6, TestLevel::Simple, TestCategory::Layers,
            "Create",
            vec!["create"],
            Some("create"),
        );
        let result = TestRunner::evaluate(r#"{"cmd": "create"}"#, &case, 100);
        assert_eq!(result.score_pct(), 100.0);
    }

    #[test]
    fn all_case_ids_unique() {
        let s = suite();
        let mut ids: Vec<usize> = s.cases.iter().map(|c| c.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 50);
    }

    #[test]
    fn get_case_by_id() {
        let s = suite();
        assert!(s.get(1).is_some());
        assert!(s.get(50).is_some());
        assert!(s.get(999).is_none());
    }

    #[test]
    fn test_level_ordering() {
        assert!(TestLevel::Simple < TestLevel::Intermediate);
        assert!(TestLevel::Intermediate < TestLevel::Complex);
        assert!(TestLevel::Complex < TestLevel::Collaboration);
    }

    #[test]
    fn point_values_increase_with_difficulty() {
        assert!(TestLevel::Simple.point_value() < TestLevel::Intermediate.point_value());
        assert!(TestLevel::Intermediate.point_value() < TestLevel::Complex.point_value());
    }

    #[test]
    fn feedback_nonempty_on_failure() {
        let case = TestCase::new(
            7, TestLevel::Simple, TestCategory::Layers,
            "Create",
            vec!["missing-keyword"],
            None,
        );
        let result = TestRunner::evaluate("nothing useful", &case, 100);
        assert!(!result.feedback.is_empty());
        assert!(result.feedback.contains("missing-keyword"));
    }

    #[test]
    fn case_1_prompt_mentions_rectangle() {
        let s = suite();
        let c = s.get(1).unwrap();
        assert!(c.prompt.to_lowercase().contains("rectangle"));
    }

    #[test]
    fn collaboration_cases_have_crdt_keywords() {
        let s = suite();
        let collab = s.by_level(&TestLevel::Collaboration);
        let any_crdt = collab.iter().any(|c| {
            c.expected_keywords.iter().any(|k| k == "crdt")
        });
        assert!(any_crdt);
    }

    #[test]
    fn runner_case_insensitive_matching() {
        let case = TestCase::new(
            8, TestLevel::Simple, TestCategory::Layers,
            "Test",
            vec!["CREATE_LAYER"],
            None,
        );
        // Response uses lowercase
        let result = TestRunner::evaluate(r#"{"cmd": "create_layer"}"#, &case, 100);
        assert!(!result.keywords_missing.contains(&"CREATE_LAYER".to_string()));
    }
}
