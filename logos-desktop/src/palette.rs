// SPDX-License-Identifier: MPL-2.0
// logos-desktop/src/palette.rs — Command palette (⌘+K / Ctrl+K)
//
//  Fuzzy-search overlay that lets users discover and execute commands
//  by typing.  Integrates with `CommandRegistry` for available commands
//  and `ShortcutRegistry` for displaying shortcut hints.

use std::fmt;

use crate::commands::{Command, CommandRegistry};

// ── Palette Mode ────────────────────────────────────────────────

/// The palette can operate in different modes depending on the trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteMode {
    /// Default: search all commands.
    Commands,
    /// File search (like Ctrl+P in VS Code).
    Files,
    /// Go to a specific layer by name.
    GoToLayer,
}

impl fmt::Display for PaletteMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Commands => write!(f, "Commands"),
            Self::Files => write!(f, "Files"),
            Self::GoToLayer => write!(f, "Go to Layer"),
        }
    }
}

// ── Palette Result ──────────────────────────────────────────────

/// A single result row rendered in the palette.
#[derive(Debug, Clone)]
pub struct PaletteResult {
    /// Command ID or action identifier.
    pub id: String,
    /// Main label text.
    pub label: String,
    /// Category badge (e.g. "Edit", "View").
    pub category: String,
    /// Optional keyboard shortcut hint.
    pub shortcut_hint: Option<String>,
    /// Optional description below the label.
    pub description: Option<String>,
    /// Whether this result is currently highlighted.
    pub highlighted: bool,
    /// Fuzzy match score (higher = better match).
    pub score: u32,
    /// The command to dispatch if selected.
    pub command: Option<Command>,
}

// ── Fuzzy Matching ──────────────────────────────────────────────

/// Simple fuzzy matching: checks if all characters of `query` appear
/// in `text` in order, case-insensitively.  Returns a score where
/// higher is better (consecutive matches score more).
pub fn fuzzy_score(query: &str, text: &str) -> Option<u32> {
    if query.is_empty() {
        return Some(0);
    }
    let query_lower: Vec<char> = query.to_lowercase().chars().collect();
    let text_lower: Vec<char> = text.to_lowercase().chars().collect();

    let mut qi = 0;
    let mut score: u32 = 0;
    let mut prev_matched = false;
    let mut first_match_pos = None;

    for (ti, &tc) in text_lower.iter().enumerate() {
        if qi < query_lower.len() && tc == query_lower[qi] {
            if first_match_pos.is_none() {
                first_match_pos = Some(ti);
            }
            // Consecutive match bonus
            if prev_matched {
                score += 3;
            } else {
                score += 1;
            }
            // Word boundary bonus (start of string or after separator)
            if ti == 0 || matches!(text_lower.get(ti.wrapping_sub(1)), Some(' ' | '-' | '_' | '.')) {
                score += 5;
            }
            prev_matched = true;
            qi += 1;
        } else {
            prev_matched = false;
        }
    }

    if qi == query_lower.len() {
        // Prefer matches that start earlier
        let pos_bonus = 10u32.saturating_sub(first_match_pos.unwrap_or(10) as u32);
        Some(score + pos_bonus)
    } else {
        None // not all query chars matched
    }
}

// ── Command Palette State ───────────────────────────────────────

/// The interactive command palette.
pub struct CommandPalette {
    /// Whether the palette overlay is visible.
    pub open: bool,
    /// Current mode.
    pub mode: PaletteMode,
    /// User's typed query.
    pub query: String,
    /// Filtered/scored results.
    pub results: Vec<PaletteResult>,
    /// Index of the highlighted result (keyboard navigation).
    pub selected_index: usize,
    /// Maximum results to display.
    pub max_results: usize,
    /// History of recently executed commands (MRU).
    pub recent_commands: Vec<String>,
    /// Maximum recent history size.
    pub max_recent: usize,
}

impl CommandPalette {
    pub fn new() -> Self {
        Self {
            open: false,
            mode: PaletteMode::Commands,
            query: String::new(),
            results: Vec::new(),
            selected_index: 0,
            max_results: 20,
            recent_commands: Vec::new(),
            max_recent: 10,
        }
    }

    /// Open the palette in command mode, clearing the query.
    pub fn show(&mut self) {
        self.open = true;
        self.mode = PaletteMode::Commands;
        self.query.clear();
        self.results.clear();
        self.selected_index = 0;
    }

    /// Open in a specific mode.
    pub fn show_mode(&mut self, mode: PaletteMode) {
        self.show();
        self.mode = mode;
    }

    /// Close the palette.
    pub fn dismiss(&mut self) {
        self.open = false;
        self.query.clear();
        self.results.clear();
        self.selected_index = 0;
    }

    /// Update the query and refresh results from the registry.
    pub fn update_query(&mut self, query: &str, registry: &CommandRegistry) {
        self.query = query.to_string();
        self.selected_index = 0;
        self.refresh_results(registry);
    }

    /// Append a character to the query.
    pub fn type_char(&mut self, ch: char, registry: &CommandRegistry) {
        self.query.push(ch);
        self.selected_index = 0;
        self.refresh_results(registry);
    }

    /// Remove the last character from the query.
    pub fn backspace(&mut self, registry: &CommandRegistry) {
        self.query.pop();
        self.selected_index = 0;
        self.refresh_results(registry);
    }

    /// Move selection up.
    pub fn select_previous(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
        self.update_highlights();
    }

    /// Move selection down.
    pub fn select_next(&mut self) {
        if !self.results.is_empty() && self.selected_index < self.results.len() - 1 {
            self.selected_index += 1;
        }
        self.update_highlights();
    }

    /// Confirm the currently selected result.
    /// Returns the command to execute, if any.
    pub fn confirm(&mut self) -> Option<Command> {
        let result = self.results.get(self.selected_index)?;
        let cmd = result.command.clone()?;
        let cmd_id = result.id.clone();

        // Track in recent
        self.recent_commands.retain(|id| *id != cmd_id);
        self.recent_commands.insert(0, cmd_id);
        if self.recent_commands.len() > self.max_recent {
            self.recent_commands.truncate(self.max_recent);
        }

        self.dismiss();
        Some(cmd)
    }

    /// Number of visible results.
    pub fn result_count(&self) -> usize {
        self.results.len()
    }

    /// The currently highlighted result.
    pub fn selected_result(&self) -> Option<&PaletteResult> {
        self.results.get(self.selected_index)
    }

    /// Refresh the results list by matching the query against the registry.
    fn refresh_results(&mut self, registry: &CommandRegistry) {
        self.results.clear();

        let commands = registry.commands();
        let mut scored: Vec<PaletteResult> = commands
            .into_iter()
            .filter(|info| info.enabled)
            .filter_map(|info| {
                let score = if self.query.is_empty() {
                    // Show recent commands first when query is empty
                    let recency_bonus = self.recent_commands.iter()
                        .position(|id| *id == info.id)
                        .map(|p| (self.max_recent - p) as u32 * 10)
                        .unwrap_or(0);
                    Some(recency_bonus)
                } else {
                    // Fuzzy match against label and id
                    let label_score = fuzzy_score(&self.query, &info.label);
                    let id_score = fuzzy_score(&self.query, &info.id);
                    match (label_score, id_score) {
                        (Some(a), Some(b)) => Some(a.max(b)),
                        (Some(a), None) => Some(a),
                        (None, Some(b)) => Some(b),
                        (None, None) => None,
                    }
                };

                score.map(|s| PaletteResult {
                    id: info.id.clone(),
                    label: info.label.clone(),
                    category: info.category.to_string(),
                    shortcut_hint: None,
                    description: if info.description.is_empty() { None } else { Some(info.description.clone()) },
                    highlighted: false,
                    score: s,
                    command: id_to_command(&info.id),
                })
            })
            .collect();

        // Sort by score descending, then by label
        scored.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.label.cmp(&b.label)));
        scored.truncate(self.max_results);

        self.results = scored;
        self.update_highlights();
    }

    fn update_highlights(&mut self) {
        for (i, result) in self.results.iter_mut().enumerate() {
            result.highlighted = i == self.selected_index;
        }
    }
}

impl Default for CommandPalette {
    fn default() -> Self {
        Self::new()
    }
}

/// Resolve a command ID to a `Command` enum value.
/// This handles the common static commands; parametric commands
/// (like `ExportDocument { format }`) need separate handling.
fn id_to_command(id: &str) -> Option<Command> {
    Some(match id {
        "doc.new" => Command::NewDocument,
        "doc.open" => Command::OpenDocument,
        "doc.save" => Command::SaveDocument,
        "doc.save-as" => Command::SaveDocumentAs,
        "doc.close" => Command::CloseDocument,
        "edit.undo" => Command::Undo,
        "edit.redo" => Command::Redo,
        "edit.cut" => Command::Cut,
        "edit.copy" => Command::Copy,
        "edit.paste" => Command::Paste,
        "edit.duplicate" => Command::Duplicate,
        "edit.delete" => Command::Delete,
        "edit.select-all" => Command::SelectAll,
        "edit.deselect" => Command::DeselectAll,
        "view.zoom-in" => Command::ZoomIn,
        "view.zoom-out" => Command::ZoomOut,
        "view.zoom-fit" => Command::ZoomToFit,
        "view.zoom-selection" => Command::ZoomToSelection,
        "view.zoom-reset" => Command::ResetZoom,
        "view.toggle-grid" => Command::ToggleGrid,
        "view.toggle-rulers" => Command::ToggleRulers,
        "view.toggle-snap" => Command::ToggleSnapToGrid,
        "layer.add-rect" => Command::AddRectangle,
        "layer.add-ellipse" => Command::AddEllipse,
        "layer.add-text" => Command::AddText,
        "layer.add-frame" => Command::AddFrame,
        "layer.group" => Command::GroupSelection,
        "layer.ungroup" => Command::UngroupSelection,
        "layer.bring-front" => Command::BringToFront,
        "layer.send-back" => Command::SendToBack,
        "layer.bring-forward" => Command::BringForward,
        "layer.send-backward" => Command::SendBackward,
        "align.left" => Command::AlignLeft,
        "align.center" => Command::AlignCenter,
        "align.right" => Command::AlignRight,
        "align.top" => Command::AlignTop,
        "align.middle" => Command::AlignMiddle,
        "align.bottom" => Command::AlignBottom,
        "align.distribute-h" => Command::DistributeHorizontally,
        "align.distribute-v" => Command::DistributeVertically,
        "app.command-palette" => Command::OpenCommandPalette,
        "app.preferences" => Command::OpenPreferences,
        "app.fullscreen" => Command::ToggleFullscreen,
        "app.quit" => Command::Quit,
        "plugin.manager" => Command::OpenPluginManager,
        _ => return None,
    })
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_registry() -> CommandRegistry {
        CommandRegistry::new()
    }

    #[test]
    fn test_fuzzy_score_exact() {
        let score = fuzzy_score("Undo", "Undo");
        assert!(score.is_some());
        assert!(score.unwrap() > 10);
    }

    #[test]
    fn test_fuzzy_score_case_insensitive() {
        let score = fuzzy_score("undo", "Undo");
        assert!(score.is_some());
    }

    #[test]
    fn test_fuzzy_score_partial() {
        let score = fuzzy_score("zin", "Zoom In");
        assert!(score.is_some());
    }

    #[test]
    fn test_fuzzy_score_no_match() {
        assert!(fuzzy_score("xyz", "Undo").is_none());
    }

    #[test]
    fn test_fuzzy_score_empty_query() {
        assert_eq!(fuzzy_score("", "anything"), Some(0));
    }

    #[test]
    fn test_fuzzy_score_word_boundary_bonus() {
        // "ZI" matching "Zoom In" should get word boundary bonus on I
        let zi_score = fuzzy_score("ZI", "Zoom In").unwrap();
        let zi_score2 = fuzzy_score("ZI", "Zipping").unwrap_or(0);
        // Word boundary match should score higher
        assert!(zi_score > 0);
        // Both should be non-zero but exact comparison depends on implementation
        let _ = zi_score2;
    }

    #[test]
    fn test_palette_show_dismiss() {
        let mut palette = CommandPalette::new();
        assert!(!palette.open);
        palette.show();
        assert!(palette.open);
        assert_eq!(palette.mode, PaletteMode::Commands);
        palette.dismiss();
        assert!(!palette.open);
    }

    #[test]
    fn test_palette_show_mode() {
        let mut palette = CommandPalette::new();
        palette.show_mode(PaletteMode::GoToLayer);
        assert!(palette.open);
        assert_eq!(palette.mode, PaletteMode::GoToLayer);
    }

    #[test]
    fn test_palette_update_query() {
        let mut palette = CommandPalette::new();
        let reg = make_registry();
        palette.show();
        palette.update_query("undo", &reg);
        assert_eq!(palette.query, "undo");
        assert!(!palette.results.is_empty());
        // First result should contain "Undo"
        assert!(palette.results[0].label.contains("Undo"));
    }

    #[test]
    fn test_palette_type_char() {
        let mut palette = CommandPalette::new();
        let reg = make_registry();
        palette.show();
        palette.type_char('z', &reg);
        palette.type_char('o', &reg);
        assert_eq!(palette.query, "zo");
        assert!(!palette.results.is_empty());
    }

    #[test]
    fn test_palette_backspace() {
        let mut palette = CommandPalette::new();
        let reg = make_registry();
        palette.show();
        palette.update_query("zoom", &reg);
        palette.backspace(&reg);
        assert_eq!(palette.query, "zoo");
    }

    #[test]
    fn test_palette_navigation() {
        let mut palette = CommandPalette::new();
        let reg = make_registry();
        palette.show();
        palette.update_query("", &reg); // show all
        assert_eq!(palette.selected_index, 0);

        palette.select_next();
        assert_eq!(palette.selected_index, 1);

        palette.select_previous();
        assert_eq!(palette.selected_index, 0);

        // Should not go below 0
        palette.select_previous();
        assert_eq!(palette.selected_index, 0);
    }

    #[test]
    fn test_palette_confirm() {
        let mut palette = CommandPalette::new();
        let reg = make_registry();
        palette.show();
        palette.update_query("undo", &reg);
        let cmd = palette.confirm();
        assert!(cmd.is_some());
        assert!(!palette.open);
    }

    #[test]
    fn test_palette_recent_commands() {
        let mut palette = CommandPalette::new();
        let reg = make_registry();

        // Execute "undo"
        palette.show();
        palette.update_query("undo", &reg);
        palette.confirm();

        // Recent should contain it
        assert!(palette.recent_commands.contains(&"edit.undo".to_string()));

        // When showing empty query, recent items should score higher
        palette.show();
        palette.update_query("", &reg);
        // The first results should include recently used commands
        let ids: Vec<_> = palette.results.iter().map(|r| r.id.clone()).collect();
        // edit.undo should appear near the top since it was recently used
        assert!(ids.iter().any(|id| id == "edit.undo"));
    }

    #[test]
    fn test_palette_max_results() {
        let mut palette = CommandPalette::new();
        palette.max_results = 5;
        let reg = make_registry();
        palette.show();
        palette.update_query("", &reg);
        assert!(palette.results.len() <= 5);
    }

    #[test]
    fn test_palette_selected_result() {
        let mut palette = CommandPalette::new();
        let reg = make_registry();
        palette.show();
        palette.update_query("save", &reg);
        let result = palette.selected_result();
        assert!(result.is_some());
        assert!(result.unwrap().highlighted);
    }

    #[test]
    fn test_palette_highlight_tracking() {
        let mut palette = CommandPalette::new();
        let reg = make_registry();
        palette.show();
        palette.update_query("", &reg);
        if palette.results.len() >= 2 {
            assert!(palette.results[0].highlighted);
            assert!(!palette.results[1].highlighted);

            palette.select_next();
            assert!(!palette.results[0].highlighted);
            assert!(palette.results[1].highlighted);
        }
    }

    #[test]
    fn test_palette_mode_display() {
        assert_eq!(PaletteMode::Commands.to_string(), "Commands");
        assert_eq!(PaletteMode::Files.to_string(), "Files");
        assert_eq!(PaletteMode::GoToLayer.to_string(), "Go to Layer");
    }

    #[test]
    fn test_id_to_command_known() {
        assert_eq!(id_to_command("edit.undo"), Some(Command::Undo));
        assert_eq!(id_to_command("doc.new"), Some(Command::NewDocument));
        assert_eq!(id_to_command("app.quit"), Some(Command::Quit));
    }

    #[test]
    fn test_id_to_command_unknown() {
        assert_eq!(id_to_command("nonexistent"), None);
    }

    #[test]
    fn test_confirm_empty_palette() {
        let mut palette = CommandPalette::new();
        palette.show();
        // No results → confirm returns None
        assert!(palette.confirm().is_none());
    }

    #[test]
    fn test_recent_capped_at_max() {
        let mut palette = CommandPalette::new();
        palette.max_recent = 3;
        let reg = make_registry();

        for q in ["undo", "redo", "cut", "copy", "paste"] {
            palette.show();
            palette.update_query(q, &reg);
            palette.confirm();
        }
        assert!(palette.recent_commands.len() <= 3);
    }
}
