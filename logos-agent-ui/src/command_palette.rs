//! Command Palette — agent-aware command registry and fuzzy-match palette
//!
//! The Logos command palette (triggered by Ctrl+K / Cmd+K) is extended to show
//! agent commands: `/agent`, `/ask`, `/palette`, `/accessibility` etc.
//! Users can invoke commands with natural language or structured shortcuts.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

// ── Command category ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CommandCategory {
    /// Core Logos layer operations.
    Layers,
    /// Styling, fills, strokes.
    Styling,
    /// Spreadsheet and data.
    Data,
    /// AI agent commands.
    Agent,
    /// Accessibility tools.
    Accessibility,
    /// Export and share.
    Export,
    /// Settings and configuration.
    Settings,
}

impl CommandCategory {
    pub fn display_name(&self) -> &str {
        match self {
            CommandCategory::Layers => "Layers",
            CommandCategory::Styling => "Styling",
            CommandCategory::Data => "Data",
            CommandCategory::Agent => "AI Agent",
            CommandCategory::Accessibility => "Accessibility",
            CommandCategory::Export => "Export",
            CommandCategory::Settings => "Settings",
        }
    }

    pub fn icon(&self) -> &str {
        match self {
            CommandCategory::Layers => "⬜",
            CommandCategory::Styling => "🎨",
            CommandCategory::Data => "📊",
            CommandCategory::Agent => "🤖",
            CommandCategory::Accessibility => "♿",
            CommandCategory::Export => "📤",
            CommandCategory::Settings => "⚙️",
        }
    }
}

// ── Agent command shortcut ────────────────────────────────────────────────────

/// Shorthand triggers that route to the agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentCommandShortcut {
    /// Trigger prefix (e.g. "/agent", "/ask", "/fix").
    pub trigger: String,
    /// Human description.
    pub description: String,
    /// Minimum agent level required.
    pub min_level: AgentLevelReq,
    /// Example usage text.
    pub example: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentLevelReq {
    Any,
    MidOrSenior,
    SeniorOnly,
}

impl AgentCommandShortcut {
    pub fn matches(&self, input: &str) -> bool {
        input.trim_start().to_lowercase().starts_with(&self.trigger.to_lowercase())
    }
}

// ── Command entry ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandEntry {
    pub id: String,
    pub label: String,
    pub description: String,
    pub category: CommandCategory,
    pub keyboard_shortcut: Option<String>,
    /// Whether this command routes through the AI agent.
    pub is_agent_command: bool,
    /// Minimum usage frequency (higher = shown higher in default list).
    pub base_rank: u32,
    pub tags: Vec<String>,
}

impl CommandEntry {
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        description: impl Into<String>,
        category: CommandCategory,
    ) -> Self {
        CommandEntry {
            id: id.into(),
            label: label.into(),
            description: description.into(),
            category,
            keyboard_shortcut: None,
            is_agent_command: false,
            base_rank: 0,
            tags: vec![],
        }
    }

    pub fn agent(mut self) -> Self {
        self.is_agent_command = true;
        self
    }

    pub fn shortcut(mut self, s: impl Into<String>) -> Self {
        self.keyboard_shortcut = Some(s.into());
        self
    }

    pub fn rank(mut self, r: u32) -> Self {
        self.base_rank = r;
        self
    }

    pub fn tags(mut self, tags: Vec<&str>) -> Self {
        self.tags = tags.iter().map(|s| s.to_string()).collect();
        self
    }
}

// ── Command match ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CommandMatch {
    pub entry: CommandEntry,
    pub score: f32,
    pub matched_chars: Vec<usize>,
}

// ── Command suggestion ────────────────────────────────────────────────────────

/// A suggestion shown below the palette input.
#[derive(Debug, Clone)]
pub struct CommandSuggestion {
    pub text: String,
    pub command_id: Option<String>,
    pub confidence: f32,
}

// ── Palette filter ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct PaletteFilter {
    pub query: String,
    pub category: Option<CommandCategory>,
    pub only_agent: bool,
}

impl PaletteFilter {
    pub fn from_query(q: impl Into<String>) -> Self {
        let q = q.into();
        let only_agent = q.trim_start().starts_with('/');
        PaletteFilter { query: q, category: None, only_agent }
    }
}

// ── Palette action ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PaletteAction {
    ExecuteCommand(String),
    RouteToAgent { input: String },
    OpenChatWith { input: String },
    ShowHelp,
}

// ── Palette state ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum PaletteState {
    #[default]
    Closed,
    Open { query: String },
    AgentMode { input: String },
    Executing { command_id: String },
}

impl PaletteState {
    pub fn is_open(&self) -> bool {
        !matches!(self, PaletteState::Closed)
    }

    pub fn is_agent_mode(&self) -> bool {
        matches!(self, PaletteState::AgentMode { .. })
    }

    pub fn current_query(&self) -> &str {
        match self {
            PaletteState::Open { query } => query,
            PaletteState::AgentMode { input } => input,
            _ => "",
        }
    }
}

// ── Command registry ──────────────────────────────────────────────────────────

pub struct CommandRegistry {
    commands: Vec<CommandEntry>,
    shortcuts: Vec<AgentCommandShortcut>,
    usage_counts: HashMap<String, u32>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        let mut registry = CommandRegistry {
            commands: Vec::new(),
            shortcuts: Vec::new(),
            usage_counts: HashMap::new(),
        };
        registry.register_builtins();
        registry.register_shortcuts();
        registry
    }

    fn register_builtins(&mut self) {
        // Layer commands
        self.commands.push(
            CommandEntry::new("create-rectangle", "Create Rectangle", "Add a rectangle layer", CommandCategory::Layers)
                .shortcut("R").rank(100).tags(vec!["shape", "rect", "box"])
        );
        self.commands.push(
            CommandEntry::new("create-text", "Create Text", "Add a text layer", CommandCategory::Layers)
                .shortcut("T").rank(90).tags(vec!["text", "label", "heading"])
        );
        self.commands.push(
            CommandEntry::new("create-frame", "Create Frame", "Add an artboard/frame", CommandCategory::Layers)
                .shortcut("F").rank(85).tags(vec!["frame", "artboard", "container"])
        );
        self.commands.push(
            CommandEntry::new("group-layers", "Group Layers", "Group selected layers", CommandCategory::Layers)
                .shortcut("Ctrl+G").rank(80).tags(vec!["group", "combine"])
        );
        self.commands.push(
            CommandEntry::new("delete-layer", "Delete Layer", "Delete the selected layer", CommandCategory::Layers)
                .shortcut("Delete").rank(70).tags(vec!["delete", "remove"])
        );

        // Styling
        self.commands.push(
            CommandEntry::new("set-fill", "Set Fill Color", "Change layer fill color", CommandCategory::Styling)
                .rank(75).tags(vec!["fill", "color", "paint"])
        );
        self.commands.push(
            CommandEntry::new("set-opacity", "Set Opacity", "Change layer transparency", CommandCategory::Styling)
                .rank(65).tags(vec!["opacity", "transparency", "alpha"])
        );

        // Agent commands
        self.commands.push(
            CommandEntry::new("agent-ask", "Ask AI Agent", "Ask the certified agent a question", CommandCategory::Agent)
                .agent().shortcut("Ctrl+Shift+K").rank(95).tags(vec!["ai", "agent", "ask", "help"])
        );
        self.commands.push(
            CommandEntry::new("agent-create", "Agent: Create Layer", "Ask agent to create a layer", CommandCategory::Agent)
                .agent().rank(88).tags(vec!["ai", "create", "layer"])
        );
        self.commands.push(
            CommandEntry::new("agent-fix-accessibility", "Agent: Fix Accessibility", "Ask agent to fix WCAG issues", CommandCategory::Agent)
                .agent().rank(82).tags(vec!["ai", "accessibility", "wcag", "fix"])
        );
        self.commands.push(
            CommandEntry::new("agent-generate-palette", "Agent: Generate Palette", "Ask agent to create a color scheme", CommandCategory::Agent)
                .agent().rank(78).tags(vec!["ai", "color", "palette", "scheme"])
        );
        self.commands.push(
            CommandEntry::new("agent-run-pipeline", "Agent: Run AI Pipeline", "Execute full AI analysis pipeline", CommandCategory::Agent)
                .agent().rank(72).tags(vec!["ai", "pipeline", "analyze"])
        );
        self.commands.push(
            CommandEntry::new("agent-spreadsheet", "Agent: Help with Formulas", "Ask agent for spreadsheet help", CommandCategory::Agent)
                .agent().rank(60).tags(vec!["ai", "formula", "spreadsheet", "data"])
        );

        // Accessibility
        self.commands.push(
            CommandEntry::new("check-contrast", "Check Contrast", "WCAG contrast check", CommandCategory::Accessibility)
                .rank(70).tags(vec!["contrast", "wcag", "accessibility"])
        );
        self.commands.push(
            CommandEntry::new("audit-accessibility", "Full Accessibility Audit", "Run WCAG 2.1 audit", CommandCategory::Accessibility)
                .rank(65).tags(vec!["audit", "wcag", "accessibility"])
        );

        // Export
        self.commands.push(
            CommandEntry::new("export-png", "Export as PNG", "Export design as PNG", CommandCategory::Export)
                .rank(75).tags(vec!["export", "png", "image"])
        );
        self.commands.push(
            CommandEntry::new("export-svg", "Export as SVG", "Export design as SVG", CommandCategory::Export)
                .rank(72).tags(vec!["export", "svg", "vector"])
        );
    }

    fn register_shortcuts(&mut self) {
        self.shortcuts.push(AgentCommandShortcut {
            trigger: "/agent".into(),
            description: "Send a command to the AI agent".into(),
            min_level: AgentLevelReq::Any,
            example: "/agent create a blue button".into(),
        });
        self.shortcuts.push(AgentCommandShortcut {
            trigger: "/ask".into(),
            description: "Ask the agent a question".into(),
            min_level: AgentLevelReq::Any,
            example: "/ask how do I create a gradient?".into(),
        });
        self.shortcuts.push(AgentCommandShortcut {
            trigger: "/fix".into(),
            description: "Ask the agent to fix an issue (Senior+)".into(),
            min_level: AgentLevelReq::SeniorOnly,
            example: "/fix accessibility issues on this page".into(),
        });
        self.shortcuts.push(AgentCommandShortcut {
            trigger: "/palette".into(),
            description: "Generate a color palette".into(),
            min_level: AgentLevelReq::MidOrSenior,
            example: "/palette analogous from #3b82f6".into(),
        });
        self.shortcuts.push(AgentCommandShortcut {
            trigger: "/ai".into(),
            description: "Run the AI analysis pipeline".into(),
            min_level: AgentLevelReq::MidOrSenior,
            example: "/ai check design quality".into(),
        });
        self.shortcuts.push(AgentCommandShortcut {
            trigger: "?".into(),
            description: "Quick help from the agent".into(),
            min_level: AgentLevelReq::Any,
            example: "? how do I add a drop shadow".into(),
        });
    }

    pub fn register(&mut self, entry: CommandEntry) {
        self.commands.push(entry);
    }

    /// Fuzzy search across the command registry.
    pub fn search(&self, filter: &PaletteFilter) -> Vec<CommandMatch> {
        let query = filter.query.trim().to_lowercase();
        let query = if query.starts_with('/') { &query[1..] } else { &query };

        let mut results: Vec<CommandMatch> = self.commands.iter()
            .filter(|cmd| {
                if filter.only_agent && !cmd.is_agent_command { return false; }
                if let Some(cat) = &filter.category {
                    if &cmd.category != cat { return false; }
                }
                true
            })
            .filter_map(|cmd| {
                if query.is_empty() {
                    return Some(CommandMatch {
                        score: cmd.base_rank as f32,
                        matched_chars: vec![],
                        entry: cmd.clone(),
                    });
                }
                let score = fuzzy_score(query, &cmd.label.to_lowercase())
                    + fuzzy_score(query, &cmd.tags.join(" ").to_lowercase()) * 0.5
                    + fuzzy_score(query, &cmd.description.to_lowercase()) * 0.3;
                if score > 0.0 {
                    Some(CommandMatch {
                        score: score + cmd.base_rank as f32 * 0.1,
                        matched_chars: vec![],
                        entry: cmd.clone(),
                    })
                } else {
                    None
                }
            })
            .collect();

        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results
    }

    /// Determine the palette action for a given typed input.
    pub fn resolve_action(&self, input: &str) -> PaletteAction {
        let trimmed = input.trim();

        // Check shortcuts
        for shortcut in &self.shortcuts {
            if shortcut.matches(trimmed) {
                let rest = trimmed[shortcut.trigger.len()..].trim().to_string();
                if rest.is_empty() {
                    return PaletteAction::ShowHelp;
                }
                return PaletteAction::RouteToAgent { input: rest };
            }
        }

        // Look for exact command ID match
        if let Some(cmd) = self.commands.iter().find(|c| c.id == trimmed || c.label.to_lowercase() == trimmed.to_lowercase()) {
            if cmd.is_agent_command {
                return PaletteAction::OpenChatWith { input: cmd.label.clone() };
            }
            return PaletteAction::ExecuteCommand(cmd.id.clone());
        }

        // If it looks like a natural language query, route to agent
        if trimmed.split_whitespace().count() > 2 {
            return PaletteAction::RouteToAgent { input: trimmed.to_string() };
        }

        PaletteAction::ShowHelp
    }

    pub fn record_usage(&mut self, command_id: &str) {
        *self.usage_counts.entry(command_id.to_string()).or_insert(0) += 1;
    }

    pub fn usage_count(&self, command_id: &str) -> u32 {
        *self.usage_counts.get(command_id).unwrap_or(&0)
    }

    pub fn command_count(&self) -> usize { self.commands.len() }
    pub fn shortcut_count(&self) -> usize { self.shortcuts.len() }

    pub fn agent_commands(&self) -> Vec<&CommandEntry> {
        self.commands.iter().filter(|c| c.is_agent_command).collect()
    }

    pub fn find(&self, id: &str) -> Option<&CommandEntry> {
        self.commands.iter().find(|c| c.id == id)
    }

    pub fn shortcuts_for_input(&self, input: &str) -> Vec<&AgentCommandShortcut> {
        self.shortcuts.iter().filter(|s| s.matches(input)).collect()
    }
}

fn fuzzy_score(query: &str, target: &str) -> f32 {
    if query.is_empty() { return 0.0; }
    if target.contains(query) { return query.len() as f32 * 2.0; }
    // Partial character match
    let mut score = 0.0f32;
    let mut last_pos: Option<usize> = None;
    for ch in query.chars() {
        if let Some(pos) = target[last_pos.map(|p| p + 1).unwrap_or(0)..].find(ch) {
            score += 1.0;
            if let Some(_lp) = last_pos {
                if pos == 1 { score += 0.5; } // consecutive bonus
            }
            last_pos = Some(last_pos.map(|p| p + 1).unwrap_or(0) + pos);
        }
    }
    score
}

impl Default for CommandRegistry {
    fn default() -> Self { Self::new() }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn reg() -> CommandRegistry { CommandRegistry::new() }

    #[test]
    fn registry_has_builtin_commands() {
        assert!(reg().command_count() >= 15);
    }

    #[test]
    fn registry_has_agent_commands() {
        assert!(reg().agent_commands().len() >= 5);
    }

    #[test]
    fn registry_has_shortcuts() {
        assert!(reg().shortcut_count() >= 5);
    }

    #[test]
    fn search_by_label() {
        let r = reg();
        let filter = PaletteFilter::from_query("rectangle");
        let results = r.search(&filter);
        assert!(!results.is_empty(), "Should find rectangle command");
        assert!(results[0].entry.id == "create-rectangle");
    }

    #[test]
    fn search_empty_returns_all() {
        let r = reg();
        let filter = PaletteFilter::from_query("");
        let results = r.search(&filter);
        assert_eq!(results.len(), r.command_count());
    }

    #[test]
    fn search_agent_only_filter() {
        let r = reg();
        let filter = PaletteFilter { query: "".into(), category: None, only_agent: true };
        let results = r.search(&filter);
        assert!(results.iter().all(|m| m.entry.is_agent_command));
    }

    #[test]
    fn search_by_tag() {
        let r = reg();
        let filter = PaletteFilter::from_query("wcag");
        let results = r.search(&filter);
        assert!(!results.is_empty());
    }

    #[test]
    fn resolve_agent_shortcut() {
        let r = reg();
        let action = r.resolve_action("/agent create a blue button");
        assert!(matches!(action, PaletteAction::RouteToAgent { .. }));
    }

    #[test]
    fn resolve_ask_shortcut() {
        let r = reg();
        let action = r.resolve_action("/ask how do I add shadow?");
        assert!(matches!(action, PaletteAction::RouteToAgent { .. }));
    }

    #[test]
    fn resolve_natural_language() {
        let r = reg();
        let action = r.resolve_action("make the background dark blue");
        assert!(matches!(action, PaletteAction::RouteToAgent { .. }));
    }

    #[test]
    fn resolve_exact_command() {
        let r = reg();
        let action = r.resolve_action("create-rectangle");
        assert!(matches!(action, PaletteAction::ExecuteCommand(_)));
    }

    #[test]
    fn usage_count_tracking() {
        let mut r = reg();
        r.record_usage("create-rectangle");
        r.record_usage("create-rectangle");
        r.record_usage("set-fill");
        assert_eq!(r.usage_count("create-rectangle"), 2);
        assert_eq!(r.usage_count("set-fill"), 1);
        assert_eq!(r.usage_count("unknown"), 0);
    }

    #[test]
    fn find_command_by_id() {
        let r = reg();
        assert!(r.find("agent-ask").is_some());
        assert!(r.find("nonexistent").is_none());
    }

    #[test]
    fn palette_state_is_open() {
        let state = PaletteState::Open { query: "hello".into() };
        assert!(state.is_open());
        assert!(!PaletteState::Closed.is_open());
    }

    #[test]
    fn palette_state_agent_mode() {
        let state = PaletteState::AgentMode { input: "/ask".into() };
        assert!(state.is_agent_mode());
        assert!(!PaletteState::Open { query: "".into() }.is_agent_mode());
    }

    #[test]
    fn shortcut_trigger_matching() {
        let s = AgentCommandShortcut {
            trigger: "/agent".into(),
            description: "test".into(),
            min_level: AgentLevelReq::Any,
            example: "".into(),
        };
        assert!(s.matches("/agent create a button"));
        assert!(s.matches("/AGENT  hello")); // case-insensitive
        assert!(!s.matches("/ask something"));
    }

    #[test]
    fn search_results_sorted_by_score() {
        let r = reg();
        let filter = PaletteFilter::from_query("agent");
        let results = r.search(&filter);
        for i in 0..results.len().saturating_sub(1) {
            assert!(results[i].score >= results[i+1].score,
                "Results should be sorted descending by score");
        }
    }
}
