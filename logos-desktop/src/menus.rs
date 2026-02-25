// SPDX-License-Identifier: MPL-2.0
// logos-desktop/src/menus.rs — Native menu bar using `muda`
//
//  Provides a full native menu bar for the Logos desktop application
//  using the Tauri `muda` crate.  Menu items dispatch through the
//  existing `Command` enum so all actions remain centralized.
//
//  Menu structure:
//  ┌─────────────────────────────────────────────┐
//  │ File  Edit  View  Layer  Tools  Help        │
//  └─────────────────────────────────────────────┘

use std::collections::HashMap;
use std::fmt;

use log::{debug, warn};
use muda::{
    Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu,
    accelerator::Accelerator,
    accelerator::Code,
    accelerator::Modifiers as AccelModifiers,
};

use crate::commands::{Command, ExportFormat, PanelId, ToolKind};

// ── Menu Item IDs ───────────────────────────────────────────────

/// Well-known menu item identifiers that map to `Command`s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MenuAction {
    // File
    NewDocument,
    OpenDocument,
    SaveDocument,
    SaveDocumentAs,
    CloseDocument,
    ExportPng,
    ExportSvg,
    ExportPdf,
    ExportJson,
    Quit,

    // Edit
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    Duplicate,
    Delete,
    SelectAll,
    DeselectAll,

    // View
    ZoomIn,
    ZoomOut,
    ZoomToFit,
    ZoomToSelection,
    ResetZoom,
    ToggleGrid,
    ToggleRulers,
    ToggleSnapToGrid,
    ToggleFullscreen,

    // Layer
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

    // Tools
    ToolSelect,
    ToolRectangle,
    ToolEllipse,
    ToolText,
    ToolPen,
    ToolHand,
    ToolZoom,

    // Panels
    ToggleLayers,
    ToggleProperties,
    ToggleAssets,
    ToggleHistory,

    // Application
    OpenCommandPalette,
    OpenPreferences,
    OpenPluginManager,

    // Help
    About,
}

impl MenuAction {
    /// Stable string ID used as the muda `MenuId`.
    pub fn id_str(self) -> &'static str {
        match self {
            Self::NewDocument => "file.new",
            Self::OpenDocument => "file.open",
            Self::SaveDocument => "file.save",
            Self::SaveDocumentAs => "file.save_as",
            Self::CloseDocument => "file.close",
            Self::ExportPng => "file.export.png",
            Self::ExportSvg => "file.export.svg",
            Self::ExportPdf => "file.export.pdf",
            Self::ExportJson => "file.export.json",
            Self::Quit => "file.quit",
            Self::Undo => "edit.undo",
            Self::Redo => "edit.redo",
            Self::Cut => "edit.cut",
            Self::Copy => "edit.copy",
            Self::Paste => "edit.paste",
            Self::Duplicate => "edit.duplicate",
            Self::Delete => "edit.delete",
            Self::SelectAll => "edit.select_all",
            Self::DeselectAll => "edit.deselect_all",
            Self::ZoomIn => "view.zoom_in",
            Self::ZoomOut => "view.zoom_out",
            Self::ZoomToFit => "view.zoom_to_fit",
            Self::ZoomToSelection => "view.zoom_to_selection",
            Self::ResetZoom => "view.reset_zoom",
            Self::ToggleGrid => "view.toggle_grid",
            Self::ToggleRulers => "view.toggle_rulers",
            Self::ToggleSnapToGrid => "view.toggle_snap",
            Self::ToggleFullscreen => "view.fullscreen",
            Self::AddRectangle => "layer.add_rect",
            Self::AddEllipse => "layer.add_ellipse",
            Self::AddText => "layer.add_text",
            Self::AddFrame => "layer.add_frame",
            Self::GroupSelection => "layer.group",
            Self::UngroupSelection => "layer.ungroup",
            Self::BringToFront => "layer.bring_front",
            Self::SendToBack => "layer.send_back",
            Self::BringForward => "layer.bring_forward",
            Self::SendBackward => "layer.send_backward",
            Self::ToolSelect => "tool.select",
            Self::ToolRectangle => "tool.rectangle",
            Self::ToolEllipse => "tool.ellipse",
            Self::ToolText => "tool.text",
            Self::ToolPen => "tool.pen",
            Self::ToolHand => "tool.hand",
            Self::ToolZoom => "tool.zoom",
            Self::ToggleLayers => "panel.layers",
            Self::ToggleProperties => "panel.properties",
            Self::ToggleAssets => "panel.assets",
            Self::ToggleHistory => "panel.history",
            Self::OpenCommandPalette => "app.command_palette",
            Self::OpenPreferences => "app.preferences",
            Self::OpenPluginManager => "app.plugin_manager",
            Self::About => "help.about",
        }
    }

    /// Convert this action into its `Command` equivalent.
    pub fn to_command(self) -> Command {
        match self {
            Self::NewDocument => Command::NewDocument,
            Self::OpenDocument => Command::OpenDocument,
            Self::SaveDocument => Command::SaveDocument,
            Self::SaveDocumentAs => Command::SaveDocumentAs,
            Self::CloseDocument => Command::CloseDocument,
            Self::ExportPng => Command::ExportDocument { format: ExportFormat::Png },
            Self::ExportSvg => Command::ExportDocument { format: ExportFormat::Svg },
            Self::ExportPdf => Command::ExportDocument { format: ExportFormat::Pdf },
            Self::ExportJson => Command::ExportDocument { format: ExportFormat::Json },
            Self::Quit => Command::Quit,
            Self::Undo => Command::Undo,
            Self::Redo => Command::Redo,
            Self::Cut => Command::Cut,
            Self::Copy => Command::Copy,
            Self::Paste => Command::Paste,
            Self::Duplicate => Command::Duplicate,
            Self::Delete => Command::Delete,
            Self::SelectAll => Command::SelectAll,
            Self::DeselectAll => Command::DeselectAll,
            Self::ZoomIn => Command::ZoomIn,
            Self::ZoomOut => Command::ZoomOut,
            Self::ZoomToFit => Command::ZoomToFit,
            Self::ZoomToSelection => Command::ZoomToSelection,
            Self::ResetZoom => Command::ResetZoom,
            Self::ToggleGrid => Command::ToggleGrid,
            Self::ToggleRulers => Command::ToggleRulers,
            Self::ToggleSnapToGrid => Command::ToggleSnapToGrid,
            Self::ToggleFullscreen => Command::ToggleFullscreen,
            Self::AddRectangle => Command::AddRectangle,
            Self::AddEllipse => Command::AddEllipse,
            Self::AddText => Command::AddText,
            Self::AddFrame => Command::AddFrame,
            Self::GroupSelection => Command::GroupSelection,
            Self::UngroupSelection => Command::UngroupSelection,
            Self::BringToFront => Command::BringToFront,
            Self::SendToBack => Command::SendToBack,
            Self::BringForward => Command::BringForward,
            Self::SendBackward => Command::SendBackward,
            Self::ToolSelect => Command::SelectTool(ToolKind::Select),
            Self::ToolRectangle => Command::SelectTool(ToolKind::Rectangle),
            Self::ToolEllipse => Command::SelectTool(ToolKind::Ellipse),
            Self::ToolText => Command::SelectTool(ToolKind::Text),
            Self::ToolPen => Command::SelectTool(ToolKind::Pen),
            Self::ToolHand => Command::SelectTool(ToolKind::Hand),
            Self::ToolZoom => Command::SelectTool(ToolKind::Zoom),
            Self::ToggleLayers => Command::TogglePanel(PanelId::Layers),
            Self::ToggleProperties => Command::TogglePanel(PanelId::Properties),
            Self::ToggleAssets => Command::TogglePanel(PanelId::Assets),
            Self::ToggleHistory => Command::TogglePanel(PanelId::History),
            Self::OpenCommandPalette => Command::OpenCommandPalette,
            Self::OpenPreferences => Command::OpenPreferences,
            Self::OpenPluginManager => Command::OpenPluginManager,
            Self::About => Command::OpenPreferences, // placeholder
        }
    }

    /// All known actions in declaration order.
    pub fn all() -> &'static [MenuAction] {
        &[
            Self::NewDocument, Self::OpenDocument, Self::SaveDocument,
            Self::SaveDocumentAs, Self::CloseDocument, Self::ExportPng,
            Self::ExportSvg, Self::ExportPdf, Self::ExportJson, Self::Quit,
            Self::Undo, Self::Redo, Self::Cut, Self::Copy, Self::Paste,
            Self::Duplicate, Self::Delete, Self::SelectAll, Self::DeselectAll,
            Self::ZoomIn, Self::ZoomOut, Self::ZoomToFit, Self::ZoomToSelection,
            Self::ResetZoom, Self::ToggleGrid, Self::ToggleRulers,
            Self::ToggleSnapToGrid, Self::ToggleFullscreen,
            Self::AddRectangle, Self::AddEllipse, Self::AddText, Self::AddFrame,
            Self::GroupSelection, Self::UngroupSelection,
            Self::BringToFront, Self::SendToBack, Self::BringForward, Self::SendBackward,
            Self::ToolSelect, Self::ToolRectangle, Self::ToolEllipse,
            Self::ToolText, Self::ToolPen, Self::ToolHand, Self::ToolZoom,
            Self::ToggleLayers, Self::ToggleProperties,
            Self::ToggleAssets, Self::ToggleHistory,
            Self::OpenCommandPalette, Self::OpenPreferences,
            Self::OpenPluginManager, Self::About,
        ]
    }
}

impl fmt::Display for MenuAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.id_str())
    }
}

// ── Accelerator Helpers ─────────────────────────────────────────

/// Build a keyboard accelerator from modifiers + key code.
fn accel(mods: AccelModifiers, code: Code) -> Option<Accelerator> {
    Some(Accelerator::new(Some(mods), code))
}

/// Primary modifier: Ctrl on Linux/Windows.
fn primary() -> AccelModifiers {
    AccelModifiers::CONTROL
}

/// Primary + Shift.
fn primary_shift() -> AccelModifiers {
    AccelModifiers::CONTROL | AccelModifiers::SHIFT
}

// ── Menu Builder ────────────────────────────────────────────────

/// Builds the complete native menu bar and returns a reverse
/// lookup from `MenuId` → `MenuAction`.
pub struct AppMenuBar {
    /// The muda `Menu` that should be attached to the window.
    pub menu: Menu,
    /// Maps muda `MenuId`s back to our `MenuAction` for event dispatch.
    id_map: HashMap<MenuId, MenuAction>,
}

impl AppMenuBar {
    /// Create a new menu bar with all standard Logos menus.
    pub fn new() -> Self {
        let menu = Menu::new();
        let mut id_map = HashMap::new();

        // ── File ────────────────────────────────
        let file_menu = Submenu::new("&File", true);
        Self::add(&file_menu, &mut id_map, MenuAction::NewDocument,
            "&New Document", accel(primary(), Code::KeyN));
        Self::add(&file_menu, &mut id_map, MenuAction::OpenDocument,
            "&Open…", accel(primary(), Code::KeyO));
        file_menu.append(&PredefinedMenuItem::separator()).ok();
        Self::add(&file_menu, &mut id_map, MenuAction::SaveDocument,
            "&Save", accel(primary(), Code::KeyS));
        Self::add(&file_menu, &mut id_map, MenuAction::SaveDocumentAs,
            "Save &As…", accel(primary_shift(), Code::KeyS));
        file_menu.append(&PredefinedMenuItem::separator()).ok();

        // Export submenu
        let export_sub = Submenu::new("&Export", true);
        Self::add(&export_sub, &mut id_map, MenuAction::ExportPng,
            "Export as &PNG…", accel(primary_shift(), Code::KeyE));
        Self::add(&export_sub, &mut id_map, MenuAction::ExportSvg,
            "Export as &SVG…", None);
        Self::add(&export_sub, &mut id_map, MenuAction::ExportPdf,
            "Export as P&DF…", None);
        Self::add(&export_sub, &mut id_map, MenuAction::ExportJson,
            "Export as &JSON…", None);
        file_menu.append(&export_sub).ok();

        file_menu.append(&PredefinedMenuItem::separator()).ok();
        Self::add(&file_menu, &mut id_map, MenuAction::CloseDocument,
            "&Close Document", accel(primary(), Code::KeyW));
        Self::add(&file_menu, &mut id_map, MenuAction::Quit,
            "&Quit", accel(primary(), Code::KeyQ));
        menu.append(&file_menu).ok();

        // ── Edit ────────────────────────────────
        let edit_menu = Submenu::new("&Edit", true);
        Self::add(&edit_menu, &mut id_map, MenuAction::Undo,
            "&Undo", accel(primary(), Code::KeyZ));
        Self::add(&edit_menu, &mut id_map, MenuAction::Redo,
            "&Redo", accel(primary_shift(), Code::KeyZ));
        edit_menu.append(&PredefinedMenuItem::separator()).ok();
        Self::add(&edit_menu, &mut id_map, MenuAction::Cut,
            "Cu&t", accel(primary(), Code::KeyX));
        Self::add(&edit_menu, &mut id_map, MenuAction::Copy,
            "&Copy", accel(primary(), Code::KeyC));
        Self::add(&edit_menu, &mut id_map, MenuAction::Paste,
            "&Paste", accel(primary(), Code::KeyV));
        Self::add(&edit_menu, &mut id_map, MenuAction::Duplicate,
            "&Duplicate", accel(primary(), Code::KeyD));
        edit_menu.append(&PredefinedMenuItem::separator()).ok();
        Self::add(&edit_menu, &mut id_map, MenuAction::Delete,
            "De&lete", Some(Accelerator::new(None::<AccelModifiers>, Code::Delete)));
        Self::add(&edit_menu, &mut id_map, MenuAction::SelectAll,
            "Select &All", accel(primary(), Code::KeyA));
        Self::add(&edit_menu, &mut id_map, MenuAction::DeselectAll,
            "D&eselect All", None);
        menu.append(&edit_menu).ok();

        // ── View ────────────────────────────────
        let view_menu = Submenu::new("&View", true);
        Self::add(&view_menu, &mut id_map, MenuAction::ZoomIn,
            "Zoom &In", accel(primary(), Code::Equal));
        Self::add(&view_menu, &mut id_map, MenuAction::ZoomOut,
            "Zoom &Out", accel(primary(), Code::Minus));
        Self::add(&view_menu, &mut id_map, MenuAction::ResetZoom,
            "&Reset Zoom", accel(primary(), Code::Digit0));
        Self::add(&view_menu, &mut id_map, MenuAction::ZoomToFit,
            "Zoom to &Fit", accel(primary(), Code::Digit1));
        Self::add(&view_menu, &mut id_map, MenuAction::ZoomToSelection,
            "Zoom to &Selection", accel(primary(), Code::Digit2));
        view_menu.append(&PredefinedMenuItem::separator()).ok();
        Self::add(&view_menu, &mut id_map, MenuAction::ToggleGrid,
            "Toggle &Grid", accel(primary(), Code::Quote));
        Self::add(&view_menu, &mut id_map, MenuAction::ToggleRulers,
            "Toggle R&ulers", accel(primary_shift(), Code::KeyR));
        Self::add(&view_menu, &mut id_map, MenuAction::ToggleSnapToGrid,
            "Toggle Sna&p", None);
        view_menu.append(&PredefinedMenuItem::separator()).ok();

        // Panels sub-menu inside View
        let panels_sub = Submenu::new("&Panels", true);
        Self::add(&panels_sub, &mut id_map, MenuAction::ToggleLayers,
            "&Layers", None);
        Self::add(&panels_sub, &mut id_map, MenuAction::ToggleProperties,
            "&Properties", None);
        Self::add(&panels_sub, &mut id_map, MenuAction::ToggleAssets,
            "&Assets", None);
        Self::add(&panels_sub, &mut id_map, MenuAction::ToggleHistory,
            "&History", None);
        view_menu.append(&panels_sub).ok();

        view_menu.append(&PredefinedMenuItem::separator()).ok();
        Self::add(&view_menu, &mut id_map, MenuAction::ToggleFullscreen,
            "&Fullscreen", Some(Accelerator::new(None::<AccelModifiers>, Code::F11)));
        Self::add(&view_menu, &mut id_map, MenuAction::OpenCommandPalette,
            "Command &Palette", accel(primary_shift(), Code::KeyP));
        menu.append(&view_menu).ok();

        // ── Layer ───────────────────────────────
        let layer_menu = Submenu::new("&Layer", true);
        Self::add(&layer_menu, &mut id_map, MenuAction::AddRectangle,
            "Add &Rectangle", Some(Accelerator::new(None::<AccelModifiers>, Code::KeyR)));
        Self::add(&layer_menu, &mut id_map, MenuAction::AddEllipse,
            "Add &Ellipse", Some(Accelerator::new(None::<AccelModifiers>, Code::KeyO)));
        Self::add(&layer_menu, &mut id_map, MenuAction::AddText,
            "Add &Text", Some(Accelerator::new(None::<AccelModifiers>, Code::KeyT)));
        Self::add(&layer_menu, &mut id_map, MenuAction::AddFrame,
            "Add &Frame", Some(Accelerator::new(None::<AccelModifiers>, Code::KeyF)));
        layer_menu.append(&PredefinedMenuItem::separator()).ok();
        Self::add(&layer_menu, &mut id_map, MenuAction::GroupSelection,
            "&Group", accel(primary(), Code::KeyG));
        Self::add(&layer_menu, &mut id_map, MenuAction::UngroupSelection,
            "&Ungroup", accel(primary_shift(), Code::KeyG));
        layer_menu.append(&PredefinedMenuItem::separator()).ok();

        let order_sub = Submenu::new("&Order", true);
        Self::add(&order_sub, &mut id_map, MenuAction::BringToFront,
            "Bring to &Front", accel(primary_shift(), Code::BracketRight));
        Self::add(&order_sub, &mut id_map, MenuAction::SendToBack,
            "Send to &Back", accel(primary_shift(), Code::BracketLeft));
        Self::add(&order_sub, &mut id_map, MenuAction::BringForward,
            "Bring For&ward", accel(primary(), Code::BracketRight));
        Self::add(&order_sub, &mut id_map, MenuAction::SendBackward,
            "Send Back&ward", accel(primary(), Code::BracketLeft));
        layer_menu.append(&order_sub).ok();
        menu.append(&layer_menu).ok();

        // ── Tools ───────────────────────────────
        let tool_menu = Submenu::new("&Tools", true);
        Self::add(&tool_menu, &mut id_map, MenuAction::ToolSelect,
            "&Select", Some(Accelerator::new(None::<AccelModifiers>, Code::KeyV)));
        Self::add(&tool_menu, &mut id_map, MenuAction::ToolRectangle,
            "&Rectangle", Some(Accelerator::new(None::<AccelModifiers>, Code::KeyR)));
        Self::add(&tool_menu, &mut id_map, MenuAction::ToolEllipse,
            "&Ellipse", Some(Accelerator::new(None::<AccelModifiers>, Code::KeyO)));
        Self::add(&tool_menu, &mut id_map, MenuAction::ToolText,
            "&Text", Some(Accelerator::new(None::<AccelModifiers>, Code::KeyT)));
        Self::add(&tool_menu, &mut id_map, MenuAction::ToolPen,
            "&Pen", Some(Accelerator::new(None::<AccelModifiers>, Code::KeyP)));
        Self::add(&tool_menu, &mut id_map, MenuAction::ToolHand,
            "&Hand", Some(Accelerator::new(None::<AccelModifiers>, Code::KeyH)));
        Self::add(&tool_menu, &mut id_map, MenuAction::ToolZoom,
            "&Zoom", Some(Accelerator::new(None::<AccelModifiers>, Code::KeyZ)));
        tool_menu.append(&PredefinedMenuItem::separator()).ok();
        Self::add(&tool_menu, &mut id_map, MenuAction::OpenPreferences,
            "P&references…", accel(primary(), Code::Comma));
        Self::add(&tool_menu, &mut id_map, MenuAction::OpenPluginManager,
            "Plugin &Manager…", None);
        menu.append(&tool_menu).ok();

        // ── Help ────────────────────────────────
        let help_menu = Submenu::new("&Help", true);
        Self::add(&help_menu, &mut id_map, MenuAction::About,
            "&About Logos", None);
        menu.append(&help_menu).ok();

        Self { menu, id_map }
    }

    /// Helper: create a `MenuItem` with optional accelerator and append it.
    fn add(
        submenu: &Submenu,
        id_map: &mut HashMap<MenuId, MenuAction>,
        action: MenuAction,
        label: &str,
        accelerator: Option<Accelerator>,
    ) {
        let item = MenuItem::with_id(
            MenuId::new(action.id_str()),
            label,
            true,
            accelerator,
        );
        submenu.append(&item).ok();
        id_map.insert(MenuId::new(action.id_str()), action);
    }

    /// Look up a `MenuAction` by its `MenuId` (from `MenuEvent`).
    pub fn resolve(&self, id: &MenuId) -> Option<MenuAction> {
        self.id_map.get(id).copied()
    }

    /// Look up a `MenuAction` and convert directly to a `Command`.
    pub fn resolve_command(&self, id: &MenuId) -> Option<Command> {
        self.resolve(id).map(|a| a.to_command())
    }

    /// Total number of registered menu items.
    pub fn item_count(&self) -> usize {
        self.id_map.len()
    }

    /// Get a list of all registered (MenuId, MenuAction) pairs.
    pub fn entries(&self) -> Vec<(MenuId, MenuAction)> {
        self.id_map.iter().map(|(k, v)| (k.clone(), *v)).collect()
    }
}

// ── Menu Event Processor ────────────────────────────────────────

/// Processes queued `muda::MenuEvent`s and returns the corresponding
/// list of `Command`s to execute.
pub struct MenuEventProcessor {
    menu_bar: AppMenuBar,
}

impl MenuEventProcessor {
    pub fn new() -> Self {
        Self {
            menu_bar: AppMenuBar::new(),
        }
    }

    /// Returns a reference to the underlying menu bar.
    pub fn menu_bar(&self) -> &AppMenuBar {
        &self.menu_bar
    }

    /// Drain all queued menu events into `Command`s.
    ///
    /// This should be called each frame (or on each event loop wake)
    /// to process menu clicks.
    pub fn drain_commands(&self) -> Vec<Command> {
        let mut commands = Vec::new();
        // muda delivers events via a channel; drain them.
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if let Some(cmd) = self.menu_bar.resolve_command(event.id()) {
                debug!("Menu event → {:?}", cmd);
                commands.push(cmd);
            } else {
                warn!("Unknown menu event: {:?}", event.id());
            }
        }
        commands
    }

    /// Convenience: process events and return the first command, if any.
    pub fn poll_one(&self) -> Option<Command> {
        if let Ok(event) = MenuEvent::receiver().try_recv() {
            self.menu_bar.resolve_command(event.id())
        } else {
            None
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_bar_builds_without_panic() {
        let bar = AppMenuBar::new();
        // We register 53 distinct menu items (see MenuAction::all())
        assert!(bar.item_count() >= 50, "Expected ≥50 items, got {}", bar.item_count());
    }

    #[test]
    fn all_actions_have_stable_ids() {
        let ids: Vec<&str> = MenuAction::all().iter().map(|a| a.id_str()).collect();
        // No duplicates
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(ids.len(), sorted.len(), "Duplicate menu action IDs detected");
    }

    #[test]
    fn all_actions_convert_to_commands() {
        for action in MenuAction::all() {
            let cmd = action.to_command();
            // Smoke test: Debug formatting shouldn't panic
            let _ = format!("{:?}", cmd);
        }
    }

    #[test]
    fn resolve_known_id() {
        let bar = AppMenuBar::new();
        let id = MenuId::new("file.new");
        let action = bar.resolve(&id);
        assert_eq!(action, Some(MenuAction::NewDocument));
    }

    #[test]
    fn resolve_unknown_id_returns_none() {
        let bar = AppMenuBar::new();
        let id = MenuId::new("nonexistent.item");
        assert_eq!(bar.resolve(&id), None);
    }

    #[test]
    fn resolve_command_chains() {
        let bar = AppMenuBar::new();
        let id = MenuId::new("edit.undo");
        let cmd = bar.resolve_command(&id);
        assert_eq!(cmd, Some(Command::Undo));
    }

    #[test]
    fn export_actions_produce_correct_formats() {
        assert_eq!(
            MenuAction::ExportPng.to_command(),
            Command::ExportDocument { format: ExportFormat::Png },
        );
        assert_eq!(
            MenuAction::ExportSvg.to_command(),
            Command::ExportDocument { format: ExportFormat::Svg },
        );
        assert_eq!(
            MenuAction::ExportPdf.to_command(),
            Command::ExportDocument { format: ExportFormat::Pdf },
        );
        assert_eq!(
            MenuAction::ExportJson.to_command(),
            Command::ExportDocument { format: ExportFormat::Json },
        );
    }

    #[test]
    fn tool_actions_produce_correct_tools() {
        assert_eq!(
            MenuAction::ToolSelect.to_command(),
            Command::SelectTool(ToolKind::Select),
        );
        assert_eq!(
            MenuAction::ToolPen.to_command(),
            Command::SelectTool(ToolKind::Pen),
        );
        assert_eq!(
            MenuAction::ToolHand.to_command(),
            Command::SelectTool(ToolKind::Hand),
        );
    }

    #[test]
    fn panel_actions_produce_correct_panels() {
        assert_eq!(
            MenuAction::ToggleLayers.to_command(),
            Command::TogglePanel(PanelId::Layers),
        );
        assert_eq!(
            MenuAction::ToggleProperties.to_command(),
            Command::TogglePanel(PanelId::Properties),
        );
    }

    #[test]
    fn menu_event_processor_creates() {
        let processor = MenuEventProcessor::new();
        assert!(processor.menu_bar().item_count() >= 50);
    }

    #[test]
    fn drain_commands_returns_empty_when_no_events() {
        let processor = MenuEventProcessor::new();
        let cmds = processor.drain_commands();
        assert!(cmds.is_empty());
    }

    #[test]
    fn poll_one_returns_none_when_no_events() {
        let processor = MenuEventProcessor::new();
        assert_eq!(processor.poll_one(), None);
    }

    #[test]
    fn entries_returns_all_registered() {
        let bar = AppMenuBar::new();
        let entries = bar.entries();
        assert!(entries.len() >= 50);
        // Every entry should have a valid action
        for (id, action) in &entries {
            assert_eq!(id, &MenuId::new(action.id_str()));
        }
    }

    #[test]
    fn action_display_matches_id_str() {
        for action in MenuAction::all() {
            assert_eq!(format!("{}", action), action.id_str());
        }
    }
}
