// SPDX-License-Identifier: MPL-2.0
// logos-desktop/src/tray.rs — System tray integration via `tray-icon`
//
//  Provides a system tray icon with a context menu for quick actions.
//  The tray icon indicates application state and provides rapid access
//  to file operations and window management without the main menu bar.

use std::collections::HashMap;
use std::fmt;

use log::{debug, info, warn};
use tray_icon::menu::{
    Menu as TrayMenu, MenuEvent as TrayMenuEvent, MenuId as TrayMenuId,
    MenuItem as TrayMenuItem, PredefinedMenuItem as TrayPredefinedMenuItem,
};

use crate::commands::Command;

// ── Tray Actions ────────────────────────────────────────────────

/// Quick actions available from the system tray context menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrayAction {
    ShowWindow,
    HideWindow,
    NewDocument,
    OpenDocument,
    OpenRecent,
    Preferences,
    CheckUpdates,
    About,
    Quit,
}

impl TrayAction {
    /// Stable identifier for the tray menu item.
    pub fn id_str(self) -> &'static str {
        match self {
            Self::ShowWindow => "tray.show",
            Self::HideWindow => "tray.hide",
            Self::NewDocument => "tray.new",
            Self::OpenDocument => "tray.open",
            Self::OpenRecent => "tray.recent",
            Self::Preferences => "tray.preferences",
            Self::CheckUpdates => "tray.updates",
            Self::About => "tray.about",
            Self::Quit => "tray.quit",
        }
    }

    /// Convert to a `Command` for the main app dispatch loop.
    /// Some tray actions (Show/Hide) don't map to Commands — they
    /// control window visibility directly.
    pub fn to_command(self) -> Option<Command> {
        match self {
            Self::NewDocument => Some(Command::NewDocument),
            Self::OpenDocument => Some(Command::OpenDocument),
            Self::Preferences => Some(Command::OpenPreferences),
            Self::Quit => Some(Command::Quit),
            Self::About => Some(Command::OpenPreferences), // placeholder
            // Window management actions are handled directly
            Self::ShowWindow | Self::HideWindow | Self::OpenRecent | Self::CheckUpdates => None,
        }
    }

    /// All known tray actions.
    pub fn all() -> &'static [TrayAction] {
        &[
            Self::ShowWindow,
            Self::HideWindow,
            Self::NewDocument,
            Self::OpenDocument,
            Self::OpenRecent,
            Self::Preferences,
            Self::CheckUpdates,
            Self::About,
            Self::Quit,
        ]
    }
}

impl fmt::Display for TrayAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.id_str())
    }
}

// ── Tray Status ─────────────────────────────────────────────────

/// Visual state of the tray icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayStatus {
    /// Normal operation — default icon.
    Idle,
    /// Actively rendering / working — could show activity indicator.
    Active,
    /// Sync in progress with collaboration server.
    Syncing,
    /// Error state — something needs attention.
    Error,
}

impl TrayStatus {
    /// Human-readable tooltip suffix for each status.
    pub fn tooltip_suffix(self) -> &'static str {
        match self {
            Self::Idle => "",
            Self::Active => " — Working",
            Self::Syncing => " — Syncing",
            Self::Error => " — Error",
        }
    }
}

// ── Tray Context Menu Builder ───────────────────────────────────

/// Builds the context menu shown when right-clicking the tray icon.
pub struct TrayContextMenu {
    /// The tray-icon menu object.
    pub menu: TrayMenu,
    /// Reverse lookup from MenuId to TrayAction.
    id_map: HashMap<TrayMenuId, TrayAction>,
}

impl TrayContextMenu {
    /// Build the full tray context menu.
    pub fn new() -> Self {
        let menu = TrayMenu::new();
        let mut id_map = HashMap::new();

        // ── Window control ──────────────────────
        Self::add(&menu, &mut id_map, TrayAction::ShowWindow, "Show Window");
        Self::add(&menu, &mut id_map, TrayAction::HideWindow, "Hide Window");
        menu.append(&TrayPredefinedMenuItem::separator()).ok();

        // ── Quick actions ───────────────────────
        Self::add(&menu, &mut id_map, TrayAction::NewDocument, "New Document");
        Self::add(&menu, &mut id_map, TrayAction::OpenDocument, "Open Document…");
        menu.append(&TrayPredefinedMenuItem::separator()).ok();

        // ── Settings ────────────────────────────
        Self::add(&menu, &mut id_map, TrayAction::Preferences, "Preferences…");
        Self::add(&menu, &mut id_map, TrayAction::CheckUpdates, "Check for Updates…");
        menu.append(&TrayPredefinedMenuItem::separator()).ok();

        // ── Info + Quit ─────────────────────────
        Self::add(&menu, &mut id_map, TrayAction::About, "About Logos");
        menu.append(&TrayPredefinedMenuItem::separator()).ok();
        Self::add(&menu, &mut id_map, TrayAction::Quit, "Quit Logos");

        Self { menu, id_map }
    }

    /// Helper to add a menu item.
    fn add(
        menu: &TrayMenu,
        id_map: &mut HashMap<TrayMenuId, TrayAction>,
        action: TrayAction,
        label: &str,
    ) {
        let item = TrayMenuItem::with_id(TrayMenuId::new(action.id_str()), label, true, None);
        menu.append(&item).ok();
        id_map.insert(TrayMenuId::new(action.id_str()), action);
    }

    /// Resolve a tray menu ID to its action.
    pub fn resolve(&self, id: &TrayMenuId) -> Option<TrayAction> {
        self.id_map.get(id).copied()
    }

    /// Resolve a tray menu ID directly to a Command.
    pub fn resolve_command(&self, id: &TrayMenuId) -> Option<Command> {
        self.resolve(id).and_then(|a| a.to_command())
    }

    /// Number of registered items.
    pub fn item_count(&self) -> usize {
        self.id_map.len()
    }
}

// ── Tray Manager ────────────────────────────────────────────────

/// High-level manager for the system tray icon lifecycle.
///
/// The actual `TrayIcon` from `tray-icon` requires platform-specific
/// initialization (loading an icon, creating the tray on the main
/// thread).  This struct manages the context menu and event processing
/// while the icon itself is created when the window is available.
pub struct TrayManager {
    /// Context menu for the tray icon.
    context_menu: TrayContextMenu,
    /// Current visual status — could drive icon changes.
    status: TrayStatus,
    /// Application name shown in tooltip.
    app_name: String,
    /// Whether the tray icon is currently active.
    active: bool,
}

impl TrayManager {
    /// Create a new tray manager.
    pub fn new(app_name: &str) -> Self {
        Self {
            context_menu: TrayContextMenu::new(),
            status: TrayStatus::Idle,
            app_name: app_name.to_string(),
            active: false,
        }
    }

    /// Mark the tray as active (icon created).
    pub fn set_active(&mut self, active: bool) {
        self.active = active;
        info!("Tray icon {}", if active { "activated" } else { "deactivated" });
    }

    /// Whether the tray icon is active.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Update the tray status (changes tooltip and potentially icon).
    pub fn set_status(&mut self, status: TrayStatus) {
        self.status = status;
        debug!("Tray status: {:?}", status);
    }

    /// Get current status.
    pub fn status(&self) -> TrayStatus {
        self.status
    }

    /// Get the current tooltip text.
    pub fn tooltip(&self) -> String {
        format!("{}{}", self.app_name, self.status.tooltip_suffix())
    }

    /// Access the context menu (needed for `TrayIcon` construction).
    pub fn context_menu(&self) -> &TrayContextMenu {
        &self.context_menu
    }

    /// Process queued tray menu events and return `TrayAction`s.
    pub fn drain_events(&self) -> Vec<TrayEventResult> {
        let mut results = Vec::new();
        while let Ok(event) = TrayMenuEvent::receiver().try_recv() {
            if let Some(action) = self.context_menu.resolve(event.id()) {
                debug!("Tray event → {:?}", action);
                results.push(TrayEventResult {
                    action,
                    command: action.to_command(),
                });
            } else {
                warn!("Unknown tray event: {:?}", event.id());
            }
        }
        results
    }

    /// Process and return only commands (ignoring window-management actions).
    pub fn drain_commands(&self) -> Vec<Command> {
        self.drain_events()
            .into_iter()
            .filter_map(|r| r.command)
            .collect()
    }
}

/// Result of processing a tray menu event.
#[derive(Debug)]
pub struct TrayEventResult {
    /// The tray action that was triggered.
    pub action: TrayAction,
    /// The corresponding Command, if any (None for window control actions).
    pub command: Option<Command>,
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_context_menu_builds() {
        let menu = TrayContextMenu::new();
        // 8 tray items registered (OpenRecent not in context menu)
        assert_eq!(menu.item_count(), 8);
    }

    #[test]
    fn tray_action_ids_are_unique() {
        let ids: Vec<&str> = TrayAction::all().iter().map(|a| a.id_str()).collect();
        let mut deduped = ids.clone();
        deduped.sort();
        deduped.dedup();
        assert_eq!(ids.len(), deduped.len());
    }

    #[test]
    fn tray_action_to_command_some() {
        assert_eq!(TrayAction::NewDocument.to_command(), Some(Command::NewDocument));
        assert_eq!(TrayAction::OpenDocument.to_command(), Some(Command::OpenDocument));
        assert_eq!(TrayAction::Quit.to_command(), Some(Command::Quit));
    }

    #[test]
    fn tray_action_to_command_none() {
        assert_eq!(TrayAction::ShowWindow.to_command(), None);
        assert_eq!(TrayAction::HideWindow.to_command(), None);
        assert_eq!(TrayAction::CheckUpdates.to_command(), None);
    }

    #[test]
    fn resolve_known_tray_id() {
        let menu = TrayContextMenu::new();
        let id = TrayMenuId::new("tray.quit");
        assert_eq!(menu.resolve(&id), Some(TrayAction::Quit));
    }

    #[test]
    fn resolve_unknown_tray_id() {
        let menu = TrayContextMenu::new();
        let id = TrayMenuId::new("tray.nonexistent");
        assert_eq!(menu.resolve(&id), None);
    }

    #[test]
    fn tray_manager_initial_state() {
        let mgr = TrayManager::new("Logos");
        assert!(!mgr.is_active());
        assert_eq!(mgr.status(), TrayStatus::Idle);
        assert_eq!(mgr.tooltip(), "Logos");
    }

    #[test]
    fn tray_manager_set_status() {
        let mut mgr = TrayManager::new("Logos");
        mgr.set_status(TrayStatus::Syncing);
        assert_eq!(mgr.status(), TrayStatus::Syncing);
        assert_eq!(mgr.tooltip(), "Logos — Syncing");
    }

    #[test]
    fn tray_manager_set_active() {
        let mut mgr = TrayManager::new("Logos");
        mgr.set_active(true);
        assert!(mgr.is_active());
        mgr.set_active(false);
        assert!(!mgr.is_active());
    }

    #[test]
    fn tray_status_tooltips() {
        assert_eq!(TrayStatus::Idle.tooltip_suffix(), "");
        assert_eq!(TrayStatus::Active.tooltip_suffix(), " — Working");
        assert_eq!(TrayStatus::Syncing.tooltip_suffix(), " — Syncing");
        assert_eq!(TrayStatus::Error.tooltip_suffix(), " — Error");
    }

    #[test]
    fn drain_events_empty() {
        let mgr = TrayManager::new("Logos");
        let events = mgr.drain_events();
        assert!(events.is_empty());
    }

    #[test]
    fn drain_commands_empty() {
        let mgr = TrayManager::new("Logos");
        let cmds = mgr.drain_commands();
        assert!(cmds.is_empty());
    }

    #[test]
    fn tray_action_display() {
        assert_eq!(format!("{}", TrayAction::Quit), "tray.quit");
        assert_eq!(format!("{}", TrayAction::ShowWindow), "tray.show");
    }
}
