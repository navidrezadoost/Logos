// SPDX-License-Identifier: MPL-2.0
// logos-desktop/src/commands.rs — Centralized command/action dispatch system
//
//  Every user action (keyboard shortcut, toolbar click, palette selection)
//  goes through `Command`.  The `CommandRegistry` maps string IDs to
//  metadata and enablement checks; `CommandHistory` provides undo/redo.

use std::collections::HashMap;
use std::fmt;
use uuid::Uuid;

// ── Command Enum ────────────────────────────────────────────────

/// Every discrete action the desktop app can perform.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    // ── Document ────────────────────────────────
    NewDocument,
    OpenDocument,
    SaveDocument,
    SaveDocumentAs,
    CloseDocument,
    ExportDocument { format: ExportFormat },

    // ── Edit ────────────────────────────────────
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    Duplicate,
    Delete,
    SelectAll,
    DeselectAll,

    // ── View ────────────────────────────────────
    ZoomIn,
    ZoomOut,
    ZoomToFit,
    ZoomToSelection,
    ResetZoom,
    ToggleGrid,
    ToggleRulers,
    ToggleSnapToGrid,

    // ── Layer ───────────────────────────────────
    AddRectangle,
    AddEllipse,
    AddText,
    AddFrame,
    GroupSelection,
    UngroupSelection,
    BringToFront,
    SendToBack,
    BringForward,
    SendBackward,
    LockLayer { id: Uuid },
    UnlockLayer { id: Uuid },
    HideLayer { id: Uuid },
    ShowLayer { id: Uuid },
    RenameLayer { id: Uuid, name: String },

    // ── Alignment ───────────────────────────────
    AlignLeft,
    AlignCenter,
    AlignRight,
    AlignTop,
    AlignMiddle,
    AlignBottom,
    DistributeHorizontally,
    DistributeVertically,

    // ── Tool selection ──────────────────────────
    SelectTool(ToolKind),

    // ── Panels ──────────────────────────────────
    TogglePanel(PanelId),
    FocusPanel(PanelId),

    // ── Application ─────────────────────────────
    OpenCommandPalette,
    OpenPreferences,
    ToggleFullscreen,
    Quit,

    // ── Plugin system ───────────────────────────
    OpenPluginManager,
    InstallPlugin { plugin_id: String },
    UninstallPlugin { plugin_id: String },
}

/// Supported export formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExportFormat {
    Png,
    Svg,
    Pdf,
    Json,
}

impl fmt::Display for ExportFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Png => write!(f, "PNG"),
            Self::Svg => write!(f, "SVG"),
            Self::Pdf => write!(f, "PDF"),
            Self::Json => write!(f, "JSON"),
        }
    }
}

/// All available design tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolKind {
    Select,
    Rectangle,
    Ellipse,
    Text,
    Pen,
    Hand,
    Zoom,
    Frame,
    Line,
    Eyedropper,
}

impl fmt::Display for ToolKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Select => "Select",
            Self::Rectangle => "Rectangle",
            Self::Ellipse => "Ellipse",
            Self::Text => "Text",
            Self::Pen => "Pen",
            Self::Hand => "Hand",
            Self::Zoom => "Zoom",
            Self::Frame => "Frame",
            Self::Line => "Line",
            Self::Eyedropper => "Eyedropper",
        };
        write!(f, "{name}")
    }
}

/// Panel identifiers for the desktop UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelId {
    Layers,
    Properties,
    Assets,
    Plugins,
    History,
    ColorPicker,
    Typography,
}

impl fmt::Display for PanelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Layers => "Layers",
            Self::Properties => "Properties",
            Self::Assets => "Assets",
            Self::Plugins => "Plugins",
            Self::History => "History",
            Self::ColorPicker => "Color Picker",
            Self::Typography => "Typography",
        };
        write!(f, "{name}")
    }
}

// ── Command Metadata ────────────────────────────────────────────

/// Command metadata for the palette and menus.
#[derive(Debug, Clone)]
pub struct CommandInfo {
    /// Unique string identifier, e.g. `"edit.undo"`.
    pub id: String,
    /// Human-readable label shown in menus and palette.
    pub label: String,
    /// Optional menu category for grouping.
    pub category: CommandCategory,
    /// Short description for tooltip / palette detail.
    pub description: String,
    /// Whether the command is currently available.
    pub enabled: bool,
    /// Optional icon identifier.
    pub icon: Option<String>,
}

impl CommandInfo {
    pub fn new(id: impl Into<String>, label: impl Into<String>, category: CommandCategory) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            category,
            description: String::new(),
            enabled: true,
            icon: None,
        }
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    pub fn with_icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

/// Menu/palette categories for grouping commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandCategory {
    Document,
    Edit,
    View,
    Layer,
    Alignment,
    Tool,
    Panel,
    Application,
    Plugin,
}

impl fmt::Display for CommandCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Document => "Document",
            Self::Edit => "Edit",
            Self::View => "View",
            Self::Layer => "Layer",
            Self::Alignment => "Alignment",
            Self::Tool => "Tool",
            Self::Panel => "Panel",
            Self::Application => "Application",
            Self::Plugin => "Plugin",
        };
        write!(f, "{name}")
    }
}

// ── Command Registry ────────────────────────────────────────────

/// Registry of all available commands with metadata.
///
/// Used by the command palette to list commands, by menus to show
/// labels and enabled/disabled state, and by the shortcut system
/// to resolve string IDs.
pub struct CommandRegistry {
    commands: HashMap<String, CommandInfo>,
    /// Insertion order for stable palette display.
    order: Vec<String>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        let mut reg = Self {
            commands: HashMap::new(),
            order: Vec::new(),
        };
        reg.register_defaults();
        reg
    }

    /// Register a single command.
    pub fn register(&mut self, info: CommandInfo) {
        let id = info.id.clone();
        self.commands.insert(id.clone(), info);
        if !self.order.contains(&id) {
            self.order.push(id);
        }
    }

    /// Look up command info by ID.
    pub fn get(&self, id: &str) -> Option<&CommandInfo> {
        self.commands.get(id)
    }

    /// Get mutable access to command info (for toggling enabled state).
    pub fn get_mut(&mut self, id: &str) -> Option<&mut CommandInfo> {
        self.commands.get_mut(id)
    }

    /// Set the `enabled` flag for a command.
    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> bool {
        if let Some(info) = self.commands.get_mut(id) {
            info.enabled = enabled;
            true
        } else {
            false
        }
    }

    /// All registered command IDs in insertion order.
    pub fn command_ids(&self) -> &[String] {
        &self.order
    }

    /// All registered commands in insertion order.
    pub fn commands(&self) -> Vec<&CommandInfo> {
        self.order
            .iter()
            .filter_map(|id| self.commands.get(id))
            .collect()
    }

    /// Commands in a specific category.
    pub fn commands_in_category(&self, category: CommandCategory) -> Vec<&CommandInfo> {
        self.commands()
            .into_iter()
            .filter(|c| c.category == category)
            .collect()
    }

    /// Total number of registered commands.
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Search commands by label (case-insensitive substring match).
    pub fn search(&self, query: &str) -> Vec<&CommandInfo> {
        let q = query.to_lowercase();
        self.commands()
            .into_iter()
            .filter(|c| c.label.to_lowercase().contains(&q) || c.id.to_lowercase().contains(&q))
            .collect()
    }

    /// Search enabled commands only.
    pub fn search_enabled(&self, query: &str) -> Vec<&CommandInfo> {
        self.search(query)
            .into_iter()
            .filter(|c| c.enabled)
            .collect()
    }

    // ── Default registration ───────────────────

    fn register_defaults(&mut self) {
        // Document
        self.register(CommandInfo::new("doc.new", "New Document", CommandCategory::Document)
            .with_description("Create a new empty document")
            .with_icon("file-plus"));
        self.register(CommandInfo::new("doc.open", "Open Document", CommandCategory::Document)
            .with_description("Open an existing document")
            .with_icon("folder-open"));
        self.register(CommandInfo::new("doc.save", "Save", CommandCategory::Document)
            .with_description("Save the current document")
            .with_icon("save"));
        self.register(CommandInfo::new("doc.save-as", "Save As…", CommandCategory::Document)
            .with_description("Save the current document with a new name"));
        self.register(CommandInfo::new("doc.close", "Close Document", CommandCategory::Document)
            .with_description("Close the current document"));
        self.register(CommandInfo::new("doc.export.png", "Export as PNG", CommandCategory::Document)
            .with_description("Export the current document as PNG"));
        self.register(CommandInfo::new("doc.export.svg", "Export as SVG", CommandCategory::Document)
            .with_description("Export the current document as SVG"));

        // Edit
        self.register(CommandInfo::new("edit.undo", "Undo", CommandCategory::Edit)
            .with_description("Undo the last action")
            .with_icon("undo"));
        self.register(CommandInfo::new("edit.redo", "Redo", CommandCategory::Edit)
            .with_description("Redo the last undone action")
            .with_icon("redo"));
        self.register(CommandInfo::new("edit.cut", "Cut", CommandCategory::Edit)
            .with_description("Cut the selection to clipboard"));
        self.register(CommandInfo::new("edit.copy", "Copy", CommandCategory::Edit)
            .with_description("Copy the selection to clipboard"));
        self.register(CommandInfo::new("edit.paste", "Paste", CommandCategory::Edit)
            .with_description("Paste from clipboard"));
        self.register(CommandInfo::new("edit.duplicate", "Duplicate", CommandCategory::Edit)
            .with_description("Duplicate the selected layers"));
        self.register(CommandInfo::new("edit.delete", "Delete", CommandCategory::Edit)
            .with_description("Delete the selected layers")
            .with_icon("trash"));
        self.register(CommandInfo::new("edit.select-all", "Select All", CommandCategory::Edit)
            .with_description("Select all layers"));
        self.register(CommandInfo::new("edit.deselect", "Deselect All", CommandCategory::Edit)
            .with_description("Clear the selection"));

        // View
        self.register(CommandInfo::new("view.zoom-in", "Zoom In", CommandCategory::View)
            .with_description("Zoom into the canvas")
            .with_icon("zoom-in"));
        self.register(CommandInfo::new("view.zoom-out", "Zoom Out", CommandCategory::View)
            .with_description("Zoom out from the canvas")
            .with_icon("zoom-out"));
        self.register(CommandInfo::new("view.zoom-fit", "Zoom to Fit", CommandCategory::View)
            .with_description("Fit the entire canvas in view"));
        self.register(CommandInfo::new("view.zoom-selection", "Zoom to Selection", CommandCategory::View)
            .with_description("Focus on the selected layers"));
        self.register(CommandInfo::new("view.zoom-reset", "Reset Zoom", CommandCategory::View)
            .with_description("Reset zoom to 100%"));
        self.register(CommandInfo::new("view.toggle-grid", "Toggle Grid", CommandCategory::View)
            .with_description("Show or hide the grid overlay"));
        self.register(CommandInfo::new("view.toggle-rulers", "Toggle Rulers", CommandCategory::View)
            .with_description("Show or hide rulers"));
        self.register(CommandInfo::new("view.toggle-snap", "Toggle Snap to Grid", CommandCategory::View)
            .with_description("Enable or disable snap-to-grid"));

        // Layer
        self.register(CommandInfo::new("layer.add-rect", "Add Rectangle", CommandCategory::Layer)
            .with_description("Insert a new rectangle layer")
            .with_icon("square"));
        self.register(CommandInfo::new("layer.add-ellipse", "Add Ellipse", CommandCategory::Layer)
            .with_description("Insert a new ellipse layer")
            .with_icon("circle"));
        self.register(CommandInfo::new("layer.add-text", "Add Text", CommandCategory::Layer)
            .with_description("Insert a new text layer")
            .with_icon("type"));
        self.register(CommandInfo::new("layer.add-frame", "Add Frame", CommandCategory::Layer)
            .with_description("Insert a new frame layer"));
        self.register(CommandInfo::new("layer.group", "Group Selection", CommandCategory::Layer)
            .with_description("Group selected layers into a frame"));
        self.register(CommandInfo::new("layer.ungroup", "Ungroup", CommandCategory::Layer)
            .with_description("Ungroup the selected frame"));
        self.register(CommandInfo::new("layer.bring-front", "Bring to Front", CommandCategory::Layer));
        self.register(CommandInfo::new("layer.send-back", "Send to Back", CommandCategory::Layer));
        self.register(CommandInfo::new("layer.bring-forward", "Bring Forward", CommandCategory::Layer));
        self.register(CommandInfo::new("layer.send-backward", "Send Backward", CommandCategory::Layer));

        // Alignment
        self.register(CommandInfo::new("align.left", "Align Left", CommandCategory::Alignment));
        self.register(CommandInfo::new("align.center", "Align Center", CommandCategory::Alignment));
        self.register(CommandInfo::new("align.right", "Align Right", CommandCategory::Alignment));
        self.register(CommandInfo::new("align.top", "Align Top", CommandCategory::Alignment));
        self.register(CommandInfo::new("align.middle", "Align Middle", CommandCategory::Alignment));
        self.register(CommandInfo::new("align.bottom", "Align Bottom", CommandCategory::Alignment));
        self.register(CommandInfo::new("align.distribute-h", "Distribute Horizontally", CommandCategory::Alignment));
        self.register(CommandInfo::new("align.distribute-v", "Distribute Vertically", CommandCategory::Alignment));

        // Tools
        self.register(CommandInfo::new("tool.select", "Select Tool", CommandCategory::Tool)
            .with_icon("cursor"));
        self.register(CommandInfo::new("tool.rectangle", "Rectangle Tool", CommandCategory::Tool)
            .with_icon("square"));
        self.register(CommandInfo::new("tool.ellipse", "Ellipse Tool", CommandCategory::Tool)
            .with_icon("circle"));
        self.register(CommandInfo::new("tool.text", "Text Tool", CommandCategory::Tool)
            .with_icon("type"));
        self.register(CommandInfo::new("tool.pen", "Pen Tool", CommandCategory::Tool)
            .with_icon("pen-tool"));
        self.register(CommandInfo::new("tool.hand", "Hand Tool", CommandCategory::Tool)
            .with_icon("hand"));
        self.register(CommandInfo::new("tool.zoom", "Zoom Tool", CommandCategory::Tool)
            .with_icon("zoom-in"));

        // Panels
        self.register(CommandInfo::new("panel.layers", "Toggle Layers Panel", CommandCategory::Panel));
        self.register(CommandInfo::new("panel.properties", "Toggle Properties Panel", CommandCategory::Panel));
        self.register(CommandInfo::new("panel.assets", "Toggle Assets Panel", CommandCategory::Panel));
        self.register(CommandInfo::new("panel.plugins", "Toggle Plugins Panel", CommandCategory::Panel));
        self.register(CommandInfo::new("panel.history", "Toggle History Panel", CommandCategory::Panel));

        // Application
        self.register(CommandInfo::new("app.command-palette", "Open Command Palette", CommandCategory::Application)
            .with_description("Search and run commands")
            .with_icon("terminal"));
        self.register(CommandInfo::new("app.preferences", "Preferences", CommandCategory::Application)
            .with_description("Open application settings")
            .with_icon("settings"));
        self.register(CommandInfo::new("app.fullscreen", "Toggle Fullscreen", CommandCategory::Application));
        self.register(CommandInfo::new("app.quit", "Quit", CommandCategory::Application)
            .with_description("Exit the application"));

        // Plugins
        self.register(CommandInfo::new("plugin.manager", "Plugin Manager", CommandCategory::Plugin)
            .with_description("Browse and manage plugins"));
    }
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ── Command History (undo/redo tracking) ────────────────────────

/// Tracks executed commands for undo/redo support.
///
/// Wraps the core `UndoStack` concept at the desktop level,
/// recording which `Command`s were executed and in what order.
pub struct CommandHistory {
    undo_stack: Vec<CommandRecord>,
    redo_stack: Vec<CommandRecord>,
    max_depth: usize,
}

/// A recorded command execution.
#[derive(Debug, Clone)]
pub struct CommandRecord {
    pub command: Command,
    pub timestamp_ms: u64,
}

impl CommandHistory {
    pub fn new(max_depth: usize) -> Self {
        Self {
            undo_stack: Vec::with_capacity(max_depth.min(256)),
            redo_stack: Vec::with_capacity(64),
            max_depth,
        }
    }

    /// Record a command after it was executed.
    pub fn push(&mut self, command: Command) {
        self.redo_stack.clear(); // new action invalidates redo
        if self.undo_stack.len() >= self.max_depth {
            self.undo_stack.remove(0);
        }
        self.undo_stack.push(CommandRecord {
            command,
            timestamp_ms: current_time_ms(),
        });
    }

    /// Pop the most recent command for undo.
    pub fn pop_undo(&mut self) -> Option<CommandRecord> {
        let record = self.undo_stack.pop()?;
        self.redo_stack.push(record.clone());
        Some(record)
    }

    /// Pop from redo stack.
    pub fn pop_redo(&mut self) -> Option<CommandRecord> {
        let record = self.redo_stack.pop()?;
        self.undo_stack.push(record.clone());
        Some(record)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn undo_depth(&self) -> usize {
        self.undo_stack.len()
    }

    pub fn redo_depth(&self) -> usize {
        self.redo_stack.len()
    }

    /// Peek at the next undoable command without popping.
    pub fn peek_undo(&self) -> Option<&Command> {
        self.undo_stack.last().map(|r| &r.command)
    }

    /// Peek at the next redoable command without popping.
    pub fn peek_redo(&self) -> Option<&Command> {
        self.redo_stack.last().map(|r| &r.command)
    }

    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    pub fn max_depth(&self) -> usize {
        self.max_depth
    }
}

impl Default for CommandHistory {
    fn default() -> Self {
        Self::new(200)
    }
}

/// Resolves a `Command` to its registry ID string.
pub fn command_to_id(cmd: &Command) -> &'static str {
    match cmd {
        Command::NewDocument => "doc.new",
        Command::OpenDocument => "doc.open",
        Command::SaveDocument => "doc.save",
        Command::SaveDocumentAs => "doc.save-as",
        Command::CloseDocument => "doc.close",
        Command::ExportDocument { .. } => "doc.export",
        Command::Undo => "edit.undo",
        Command::Redo => "edit.redo",
        Command::Cut => "edit.cut",
        Command::Copy => "edit.copy",
        Command::Paste => "edit.paste",
        Command::Duplicate => "edit.duplicate",
        Command::Delete => "edit.delete",
        Command::SelectAll => "edit.select-all",
        Command::DeselectAll => "edit.deselect",
        Command::ZoomIn => "view.zoom-in",
        Command::ZoomOut => "view.zoom-out",
        Command::ZoomToFit => "view.zoom-fit",
        Command::ZoomToSelection => "view.zoom-selection",
        Command::ResetZoom => "view.zoom-reset",
        Command::ToggleGrid => "view.toggle-grid",
        Command::ToggleRulers => "view.toggle-rulers",
        Command::ToggleSnapToGrid => "view.toggle-snap",
        Command::AddRectangle => "layer.add-rect",
        Command::AddEllipse => "layer.add-ellipse",
        Command::AddText => "layer.add-text",
        Command::AddFrame => "layer.add-frame",
        Command::GroupSelection => "layer.group",
        Command::UngroupSelection => "layer.ungroup",
        Command::BringToFront => "layer.bring-front",
        Command::SendToBack => "layer.send-back",
        Command::BringForward => "layer.bring-forward",
        Command::SendBackward => "layer.send-backward",
        Command::LockLayer { .. } => "layer.lock",
        Command::UnlockLayer { .. } => "layer.unlock",
        Command::HideLayer { .. } => "layer.hide",
        Command::ShowLayer { .. } => "layer.show",
        Command::RenameLayer { .. } => "layer.rename",
        Command::AlignLeft => "align.left",
        Command::AlignCenter => "align.center",
        Command::AlignRight => "align.right",
        Command::AlignTop => "align.top",
        Command::AlignMiddle => "align.middle",
        Command::AlignBottom => "align.bottom",
        Command::DistributeHorizontally => "align.distribute-h",
        Command::DistributeVertically => "align.distribute-v",
        Command::SelectTool(_) => "tool.select",
        Command::TogglePanel(_) => "panel.toggle",
        Command::FocusPanel(_) => "panel.focus",
        Command::OpenCommandPalette => "app.command-palette",
        Command::OpenPreferences => "app.preferences",
        Command::ToggleFullscreen => "app.fullscreen",
        Command::Quit => "app.quit",
        Command::OpenPluginManager => "plugin.manager",
        Command::InstallPlugin { .. } => "plugin.install",
        Command::UninstallPlugin { .. } => "plugin.uninstall",
    }
}

fn current_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_registry_defaults() {
        let reg = CommandRegistry::new();
        assert!(reg.len() > 40, "should register many default commands");
        assert!(!reg.is_empty());
    }

    #[test]
    fn test_registry_get_by_id() {
        let reg = CommandRegistry::new();
        let info = reg.get("edit.undo").unwrap();
        assert_eq!(info.label, "Undo");
        assert_eq!(info.category, CommandCategory::Edit);
        assert!(info.enabled);
    }

    #[test]
    fn test_registry_search_case_insensitive() {
        let reg = CommandRegistry::new();
        let results = reg.search("zoom");
        assert!(results.len() >= 3, "zoom-in, zoom-out, zoom-fit at least");
    }

    #[test]
    fn test_registry_search_enabled_only() {
        let mut reg = CommandRegistry::new();
        reg.set_enabled("view.zoom-in", false);
        let all = reg.search("zoom in");
        let enabled = reg.search_enabled("zoom in");
        assert!(all.len() > enabled.len());
    }

    #[test]
    fn test_registry_category_filter() {
        let reg = CommandRegistry::new();
        let edit_cmds = reg.commands_in_category(CommandCategory::Edit);
        assert!(edit_cmds.len() >= 7, "undo, redo, cut, copy, paste, duplicate, delete at least");
    }

    #[test]
    fn test_registry_set_enabled() {
        let mut reg = CommandRegistry::new();
        assert!(reg.set_enabled("edit.undo", false));
        assert!(!reg.get("edit.undo").unwrap().enabled);
        assert!(reg.set_enabled("edit.undo", true));
        assert!(reg.get("edit.undo").unwrap().enabled);
        assert!(!reg.set_enabled("nonexistent.cmd", false));
    }

    #[test]
    fn test_registry_custom_command() {
        let mut reg = CommandRegistry::new();
        let before = reg.len();
        reg.register(CommandInfo::new("custom.hello", "Say Hello", CommandCategory::Plugin)
            .with_description("A custom plugin command")
            .with_icon("wave"));
        assert_eq!(reg.len(), before + 1);
        let info = reg.get("custom.hello").unwrap();
        assert_eq!(info.icon, Some("wave".to_string()));
    }

    #[test]
    fn test_command_info_builder() {
        let info = CommandInfo::new("test", "Test", CommandCategory::Edit)
            .with_description("desc")
            .with_icon("icon")
            .disabled();
        assert_eq!(info.description, "desc");
        assert_eq!(info.icon, Some("icon".to_string()));
        assert!(!info.enabled);
    }

    #[test]
    fn test_registry_insertion_order() {
        let reg = CommandRegistry::new();
        let ids = reg.command_ids();
        assert_eq!(ids[0], "doc.new");
        assert_eq!(ids[1], "doc.open");
    }

    #[test]
    fn test_command_history_push_pop() {
        let mut history = CommandHistory::new(100);
        assert!(!history.can_undo());
        assert!(!history.can_redo());

        history.push(Command::AddRectangle);
        history.push(Command::AddEllipse);
        assert_eq!(history.undo_depth(), 2);

        let record = history.pop_undo().unwrap();
        assert_eq!(record.command, Command::AddEllipse);
        assert!(history.can_redo());
        assert_eq!(history.redo_depth(), 1);
    }

    #[test]
    fn test_history_redo_clears_on_new_action() {
        let mut history = CommandHistory::new(100);
        history.push(Command::AddRectangle);
        history.push(Command::AddEllipse);
        history.pop_undo(); // undo ellipse
        assert!(history.can_redo());

        history.push(Command::AddText); // new action clears redo
        assert!(!history.can_redo());
    }

    #[test]
    fn test_history_max_depth() {
        let mut history = CommandHistory::new(3);
        history.push(Command::ZoomIn);
        history.push(Command::ZoomOut);
        history.push(Command::ZoomIn);
        history.push(Command::ZoomOut); // exceeds max, drops oldest
        assert_eq!(history.undo_depth(), 3);
    }

    #[test]
    fn test_history_peek() {
        let mut history = CommandHistory::new(100);
        history.push(Command::Cut);
        assert_eq!(history.peek_undo(), Some(&Command::Cut));
        history.pop_undo();
        assert_eq!(history.peek_redo(), Some(&Command::Cut));
    }

    #[test]
    fn test_history_clear() {
        let mut history = CommandHistory::new(100);
        history.push(Command::Paste);
        history.push(Command::Copy);
        history.pop_undo();
        history.clear();
        assert!(!history.can_undo());
        assert!(!history.can_redo());
    }

    #[test]
    fn test_history_default() {
        let history = CommandHistory::default();
        assert_eq!(history.max_depth(), 200);
    }

    #[test]
    fn test_command_to_id_mapping() {
        assert_eq!(command_to_id(&Command::Undo), "edit.undo");
        assert_eq!(command_to_id(&Command::NewDocument), "doc.new");
        assert_eq!(command_to_id(&Command::Quit), "app.quit");
        assert_eq!(command_to_id(&Command::AddRectangle), "layer.add-rect");
        assert_eq!(command_to_id(&Command::AlignLeft), "align.left");
        assert_eq!(command_to_id(&Command::OpenPluginManager), "plugin.manager");
    }

    #[test]
    fn test_tool_kind_display() {
        assert_eq!(ToolKind::Select.to_string(), "Select");
        assert_eq!(ToolKind::Pen.to_string(), "Pen");
        assert_eq!(ToolKind::Eyedropper.to_string(), "Eyedropper");
    }

    #[test]
    fn test_panel_id_display() {
        assert_eq!(PanelId::Layers.to_string(), "Layers");
        assert_eq!(PanelId::ColorPicker.to_string(), "Color Picker");
    }

    #[test]
    fn test_export_format_display() {
        assert_eq!(ExportFormat::Png.to_string(), "PNG");
        assert_eq!(ExportFormat::Svg.to_string(), "SVG");
    }

    #[test]
    fn test_command_category_display() {
        assert_eq!(CommandCategory::Document.to_string(), "Document");
        assert_eq!(CommandCategory::Alignment.to_string(), "Alignment");
    }

    #[test]
    fn test_history_roundtrip_undo_redo() {
        let mut history = CommandHistory::new(100);
        history.push(Command::Delete);

        let undone = history.pop_undo().unwrap();
        assert_eq!(undone.command, Command::Delete);
        assert_eq!(history.undo_depth(), 0);
        assert_eq!(history.redo_depth(), 1);

        let redone = history.pop_redo().unwrap();
        assert_eq!(redone.command, Command::Delete);
        assert_eq!(history.undo_depth(), 1);
        assert_eq!(history.redo_depth(), 0);
    }

    #[test]
    fn test_command_record_has_timestamp() {
        let mut history = CommandHistory::new(100);
        history.push(Command::Copy);
        let record = history.pop_undo().unwrap();
        assert!(record.timestamp_ms > 0);
    }
}
