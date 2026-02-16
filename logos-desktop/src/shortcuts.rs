// SPDX-License-Identifier: MPL-2.0
// logos-desktop/src/shortcuts.rs — Keyboard shortcut registry and binding system
//
//  Maps physical key combinations to `Command`s.  Supports modifier keys
//  (Ctrl/Cmd, Shift, Alt), single-key shortcuts, and user customization.
//  Platform-aware: uses Super (⌘) on macOS, Ctrl on Linux/Windows.

use std::collections::HashMap;
use std::fmt;

use crate::commands::{Command, ToolKind};

// ── Key Types ───────────────────────────────────────────────────

/// Modifier keys that can be combined with a key press.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Modifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub super_key: bool, // ⌘ on macOS, Win on Windows
}

impl Modifiers {
    pub const NONE: Self = Self { ctrl: false, shift: false, alt: false, super_key: false };
    pub const CTRL: Self = Self { ctrl: true, shift: false, alt: false, super_key: false };
    pub const SHIFT: Self = Self { ctrl: false, shift: true, alt: false, super_key: false };
    pub const ALT: Self = Self { ctrl: false, shift: false, alt: true, super_key: false };
    pub const CTRL_SHIFT: Self = Self { ctrl: true, shift: true, alt: false, super_key: false };
    pub const CTRL_ALT: Self = Self { ctrl: true, shift: false, alt: true, super_key: false };

    pub fn is_empty(&self) -> bool {
        !self.ctrl && !self.shift && !self.alt && !self.super_key
    }

    /// Returns the primary modifier for the current platform.
    /// On macOS this would be ⌘ (super), on Linux/Windows it's Ctrl.
    pub fn primary() -> Self {
        if cfg!(target_os = "macos") {
            Self { ctrl: false, shift: false, alt: false, super_key: true }
        } else {
            Self::CTRL
        }
    }

    /// Primary + Shift.
    pub fn primary_shift() -> Self {
        if cfg!(target_os = "macos") {
            Self { ctrl: false, shift: true, alt: false, super_key: true }
        } else {
            Self::CTRL_SHIFT
        }
    }
}

impl fmt::Display for Modifiers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();
        if self.ctrl { parts.push("Ctrl"); }
        if self.super_key { parts.push("⌘"); }
        if self.alt { parts.push("Alt"); }
        if self.shift { parts.push("Shift"); }
        write!(f, "{}", parts.join("+"))
    }
}

/// Named keys that can be bound to shortcuts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    // Letters
    A, B, C, D, E, F, G, H, I, J, K, L, M,
    N, O, P, Q, R, S, T, U, V, W, X, Y, Z,

    // Numbers
    Num0, Num1, Num2, Num3, Num4, Num5,
    Num6, Num7, Num8, Num9,

    // Function keys
    F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,

    // Navigation
    Escape, Tab, Space, Enter, Backspace, Delete,
    Home, End, PageUp, PageDown,
    ArrowUp, ArrowDown, ArrowLeft, ArrowRight,

    // Punctuation
    Minus, Equal, BracketLeft, BracketRight,
    Backslash, Semicolon, Quote, Comma, Period, Slash,

    // Special
    Plus,
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Key::A => "A", Key::B => "B", Key::C => "C", Key::D => "D",
            Key::E => "E", Key::F => "F", Key::G => "G", Key::H => "H",
            Key::I => "I", Key::J => "J", Key::K => "K", Key::L => "L",
            Key::M => "M", Key::N => "N", Key::O => "O", Key::P => "P",
            Key::Q => "Q", Key::R => "R", Key::S => "S", Key::T => "T",
            Key::U => "U", Key::V => "V", Key::W => "W", Key::X => "X",
            Key::Y => "Y", Key::Z => "Z",
            Key::Num0 => "0", Key::Num1 => "1", Key::Num2 => "2",
            Key::Num3 => "3", Key::Num4 => "4", Key::Num5 => "5",
            Key::Num6 => "6", Key::Num7 => "7", Key::Num8 => "8",
            Key::Num9 => "9",
            Key::F1 => "F1", Key::F2 => "F2", Key::F3 => "F3",
            Key::F4 => "F4", Key::F5 => "F5", Key::F6 => "F6",
            Key::F7 => "F7", Key::F8 => "F8", Key::F9 => "F9",
            Key::F10 => "F10", Key::F11 => "F11", Key::F12 => "F12",
            Key::Escape => "Esc", Key::Tab => "Tab", Key::Space => "Space",
            Key::Enter => "Enter", Key::Backspace => "Backspace",
            Key::Delete => "Delete", Key::Home => "Home", Key::End => "End",
            Key::PageUp => "PgUp", Key::PageDown => "PgDn",
            Key::ArrowUp => "↑", Key::ArrowDown => "↓",
            Key::ArrowLeft => "←", Key::ArrowRight => "→",
            Key::Minus => "-", Key::Equal => "=",
            Key::BracketLeft => "[", Key::BracketRight => "]",
            Key::Backslash => "\\", Key::Semicolon => ";",
            Key::Quote => "'", Key::Comma => ",",
            Key::Period => ".", Key::Slash => "/",
            Key::Plus => "+",
        };
        write!(f, "{s}")
    }
}

// ── Key Binding ─────────────────────────────────────────────────

/// A specific key combination that triggers a command.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyBinding {
    pub modifiers: Modifiers,
    pub key: Key,
}

impl KeyBinding {
    pub fn new(modifiers: Modifiers, key: Key) -> Self {
        Self { modifiers, key }
    }

    /// Convenience: no modifiers.
    pub fn bare(key: Key) -> Self {
        Self { modifiers: Modifiers::NONE, key }
    }

    /// Convenience: Ctrl+key (or ⌘+key on macOS).
    pub fn primary(key: Key) -> Self {
        Self { modifiers: Modifiers::primary(), key }
    }

    /// Convenience: Ctrl+Shift+key.
    pub fn primary_shift(key: Key) -> Self {
        Self { modifiers: Modifiers::primary_shift(), key }
    }

    /// Returns a human-readable string like "Ctrl+Z" or "⌘+Z".
    pub fn display_string(&self) -> String {
        if self.modifiers.is_empty() {
            self.key.to_string()
        } else {
            format!("{}+{}", self.modifiers, self.key)
        }
    }
}

impl fmt::Display for KeyBinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_string())
    }
}

// ── Shortcut Entry ──────────────────────────────────────────────

/// A registered shortcut: binding → command.
#[derive(Debug, Clone)]
pub struct ShortcutEntry {
    pub binding: KeyBinding,
    pub command: Command,
    /// Whether this is a user-customized binding (vs. default).
    pub is_custom: bool,
    /// Optional context condition (e.g. "editor.focused").
    pub when: Option<String>,
}

// ── Shortcut Registry ───────────────────────────────────────────

/// Maps key bindings to commands.
///
/// Supports default bindings, user overrides, context conditions,
/// and conflict detection.
pub struct ShortcutRegistry {
    /// Binding → Entry lookup for fast dispatch.
    bindings: HashMap<KeyBinding, ShortcutEntry>,
    /// Command ID → binding for reverse lookup (showing shortcut in menus).
    reverse: HashMap<String, KeyBinding>,
}

impl ShortcutRegistry {
    pub fn new() -> Self {
        let mut reg = Self {
            bindings: HashMap::new(),
            reverse: HashMap::new(),
        };
        reg.register_defaults();
        reg
    }

    /// Register a key binding for a command.
    pub fn bind(&mut self, binding: KeyBinding, command: Command, is_custom: bool) {
        let cmd_id = crate::commands::command_to_id(&command).to_string();
        let entry = ShortcutEntry {
            binding: binding.clone(),
            command,
            is_custom,
            when: None,
        };
        self.bindings.insert(binding.clone(), entry);
        self.reverse.insert(cmd_id, binding);
    }

    /// Register a binding with a context condition.
    pub fn bind_when(
        &mut self,
        binding: KeyBinding,
        command: Command,
        when: impl Into<String>,
    ) {
        let cmd_id = crate::commands::command_to_id(&command).to_string();
        let entry = ShortcutEntry {
            binding: binding.clone(),
            command,
            is_custom: false,
            when: Some(when.into()),
        };
        self.bindings.insert(binding.clone(), entry);
        self.reverse.insert(cmd_id, binding);
    }

    /// Remove a binding.
    pub fn unbind(&mut self, binding: &KeyBinding) -> Option<ShortcutEntry> {
        if let Some(entry) = self.bindings.remove(binding) {
            let cmd_id = crate::commands::command_to_id(&entry.command).to_string();
            self.reverse.remove(&cmd_id);
            Some(entry)
        } else {
            None
        }
    }

    /// Look up a command by key binding.
    pub fn resolve(&self, binding: &KeyBinding) -> Option<&Command> {
        self.bindings.get(binding).map(|e| &e.command)
    }

    /// Look up the key binding for a command by its registry ID.
    pub fn binding_for_command(&self, command_id: &str) -> Option<&KeyBinding> {
        self.reverse.get(command_id)
    }

    /// Get the display string for a command's shortcut (e.g. "Ctrl+Z").
    pub fn shortcut_label(&self, command_id: &str) -> Option<String> {
        self.reverse.get(command_id).map(|b| b.display_string())
    }

    /// Detect if a binding would conflict with an existing one.
    pub fn has_conflict(&self, binding: &KeyBinding) -> bool {
        self.bindings.contains_key(binding)
    }

    /// Get the existing binding that conflicts with a proposed new one.
    pub fn conflict_with(&self, binding: &KeyBinding) -> Option<&ShortcutEntry> {
        self.bindings.get(binding)
    }

    /// Number of registered bindings.
    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    /// All registered entries.
    pub fn entries(&self) -> Vec<&ShortcutEntry> {
        self.bindings.values().collect()
    }

    /// All entries sorted by their display string for preferences UI.
    pub fn entries_sorted(&self) -> Vec<&ShortcutEntry> {
        let mut entries: Vec<_> = self.bindings.values().collect();
        entries.sort_by(|a, b| a.binding.display_string().cmp(&b.binding.display_string()));
        entries
    }

    /// All user-customized bindings.
    pub fn custom_entries(&self) -> Vec<&ShortcutEntry> {
        self.entries().into_iter().filter(|e| e.is_custom).collect()
    }

    /// Reset all bindings to defaults.
    pub fn reset_to_defaults(&mut self) {
        self.bindings.clear();
        self.reverse.clear();
        self.register_defaults();
    }

    // ── Default bindings ───────────────────────

    fn register_defaults(&mut self) {
        // Document
        self.bind(KeyBinding::primary(Key::N), Command::NewDocument, false);
        self.bind(KeyBinding::primary(Key::O), Command::OpenDocument, false);
        self.bind(KeyBinding::primary(Key::S), Command::SaveDocument, false);
        self.bind(KeyBinding::primary_shift(Key::S), Command::SaveDocumentAs, false);
        self.bind(KeyBinding::primary(Key::W), Command::CloseDocument, false);

        // Edit
        self.bind(KeyBinding::primary(Key::Z), Command::Undo, false);
        self.bind(KeyBinding::primary_shift(Key::Z), Command::Redo, false);
        self.bind(KeyBinding::primary(Key::X), Command::Cut, false);
        self.bind(KeyBinding::primary(Key::C), Command::Copy, false);
        self.bind(KeyBinding::primary(Key::V), Command::Paste, false);
        self.bind(KeyBinding::primary(Key::D), Command::Duplicate, false);
        self.bind(KeyBinding::bare(Key::Backspace), Command::Delete, false);
        self.bind(KeyBinding::bare(Key::Delete), Command::Delete, false);
        self.bind(KeyBinding::primary(Key::A), Command::SelectAll, false);
        self.bind(KeyBinding::bare(Key::Escape), Command::DeselectAll, false);

        // View
        self.bind(KeyBinding::primary(Key::Equal), Command::ZoomIn, false);
        self.bind(KeyBinding::primary(Key::Plus), Command::ZoomIn, false);
        self.bind(KeyBinding::primary(Key::Minus), Command::ZoomOut, false);
        self.bind(KeyBinding::primary(Key::Num1), Command::ZoomToFit, false);
        self.bind(KeyBinding::primary(Key::Num0), Command::ResetZoom, false);
        self.bind(KeyBinding::primary(Key::Quote), Command::ToggleGrid, false);

        // Tool shortcuts (bare keys, Figma-like)
        self.bind(KeyBinding::bare(Key::V), Command::SelectTool(ToolKind::Select), false);
        self.bind(KeyBinding::bare(Key::R), Command::SelectTool(ToolKind::Rectangle), false);
        self.bind(KeyBinding::bare(Key::O), Command::SelectTool(ToolKind::Ellipse), false);
        self.bind(KeyBinding::bare(Key::T), Command::SelectTool(ToolKind::Text), false);
        self.bind(KeyBinding::bare(Key::P), Command::SelectTool(ToolKind::Pen), false);
        self.bind(KeyBinding::bare(Key::H), Command::SelectTool(ToolKind::Hand), false);
        self.bind(KeyBinding::bare(Key::Z), Command::SelectTool(ToolKind::Zoom), false);
        self.bind(KeyBinding::bare(Key::F), Command::SelectTool(ToolKind::Frame), false);
        self.bind(KeyBinding::bare(Key::L), Command::SelectTool(ToolKind::Line), false);
        self.bind(KeyBinding::bare(Key::I), Command::SelectTool(ToolKind::Eyedropper), false);

        // Layer operations
        self.bind(KeyBinding::primary(Key::G), Command::GroupSelection, false);
        self.bind(KeyBinding::primary_shift(Key::G), Command::UngroupSelection, false);
        self.bind(KeyBinding::primary(Key::BracketRight), Command::BringToFront, false);
        self.bind(KeyBinding::primary(Key::BracketLeft), Command::SendToBack, false);

        // Application
        self.bind(KeyBinding::primary(Key::K), Command::OpenCommandPalette, false);
        self.bind(KeyBinding::primary(Key::Comma), Command::OpenPreferences, false);
        self.bind(KeyBinding::bare(Key::F11), Command::ToggleFullscreen, false);
        self.bind(KeyBinding::primary(Key::Q), Command::Quit, false);
    }
}

impl Default for ShortcutRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_shortcuts_populated() {
        let reg = ShortcutRegistry::new();
        assert!(reg.len() > 20, "should have many default bindings");
    }

    #[test]
    fn test_resolve_ctrl_z() {
        let reg = ShortcutRegistry::new();
        let binding = KeyBinding::primary(Key::Z);
        let cmd = reg.resolve(&binding).unwrap();
        assert_eq!(*cmd, Command::Undo);
    }

    #[test]
    fn test_resolve_bare_escape() {
        let reg = ShortcutRegistry::new();
        let cmd = reg.resolve(&KeyBinding::bare(Key::Escape)).unwrap();
        assert_eq!(*cmd, Command::DeselectAll);
    }

    #[test]
    fn test_resolve_tool_shortcut() {
        let reg = ShortcutRegistry::new();
        let cmd = reg.resolve(&KeyBinding::bare(Key::R)).unwrap();
        assert_eq!(*cmd, Command::SelectTool(ToolKind::Rectangle));
    }

    #[test]
    fn test_shortcut_label() {
        let reg = ShortcutRegistry::new();
        let label = reg.shortcut_label("edit.undo").unwrap();
        assert!(label.contains("Z"), "label should contain the key: {label}");
    }

    #[test]
    fn test_binding_for_command() {
        let reg = ShortcutRegistry::new();
        let binding = reg.binding_for_command("edit.copy").unwrap();
        assert_eq!(binding.key, Key::C);
    }

    #[test]
    fn test_custom_binding() {
        let mut reg = ShortcutRegistry::new();
        let binding = KeyBinding::new(Modifiers::CTRL_ALT, Key::M);
        reg.bind(binding.clone(), Command::ToggleGrid, true);
        let cmd = reg.resolve(&binding).unwrap();
        assert_eq!(*cmd, Command::ToggleGrid);

        let customs = reg.custom_entries();
        assert!(customs.iter().any(|e| e.binding == binding));
    }

    #[test]
    fn test_conflict_detection() {
        let reg = ShortcutRegistry::new();
        let ctrl_z = KeyBinding::primary(Key::Z);
        assert!(reg.has_conflict(&ctrl_z));

        let ctrl_alt_z = KeyBinding::new(Modifiers::CTRL_ALT, Key::Z);
        assert!(!reg.has_conflict(&ctrl_alt_z));
    }

    #[test]
    fn test_unbind() {
        let mut reg = ShortcutRegistry::new();
        let ctrl_z = KeyBinding::primary(Key::Z);
        assert!(reg.resolve(&ctrl_z).is_some());
        reg.unbind(&ctrl_z);
        assert!(reg.resolve(&ctrl_z).is_none());
    }

    #[test]
    fn test_reset_to_defaults() {
        let mut reg = ShortcutRegistry::new();
        let ctrl_z = KeyBinding::primary(Key::Z);
        reg.unbind(&ctrl_z);
        assert!(reg.resolve(&ctrl_z).is_none());

        reg.reset_to_defaults();
        assert_eq!(*reg.resolve(&ctrl_z).unwrap(), Command::Undo);
    }

    #[test]
    fn test_entries_sorted() {
        let reg = ShortcutRegistry::new();
        let sorted = reg.entries_sorted();
        assert!(!sorted.is_empty());
        // Verify sorting by display string
        for window in sorted.windows(2) {
            let a = window[0].binding.display_string();
            let b = window[1].binding.display_string();
            assert!(a <= b, "should be sorted: {a} <= {b}");
        }
    }

    #[test]
    fn test_bind_when_context() {
        use crate::commands::PanelId;
        let mut reg = ShortcutRegistry::new();
        let binding = KeyBinding::new(Modifiers::ALT, Key::Num1);
        reg.bind_when(binding.clone(), Command::FocusPanel(PanelId::Layers), "editor.focused");
        let entry = reg.bindings.get(&binding).unwrap();
        assert_eq!(entry.when, Some("editor.focused".to_string()));
    }

    #[test]
    fn test_modifiers_display() {
        assert_eq!(Modifiers::NONE.to_string(), "");
        assert_eq!(Modifiers::CTRL.to_string(), "Ctrl");
        assert_eq!(Modifiers::CTRL_SHIFT.to_string(), "Ctrl+Shift");
    }

    #[test]
    fn test_modifiers_is_empty() {
        assert!(Modifiers::NONE.is_empty());
        assert!(!Modifiers::CTRL.is_empty());
    }

    #[test]
    fn test_key_binding_display() {
        let binding = KeyBinding::new(Modifiers::CTRL, Key::S);
        assert_eq!(binding.to_string(), "Ctrl+S");

        let bare = KeyBinding::bare(Key::Escape);
        assert_eq!(bare.to_string(), "Esc");
    }

    #[test]
    fn test_conflict_with_returns_entry() {
        let reg = ShortcutRegistry::new();
        let ctrl_c = KeyBinding::primary(Key::C);
        let entry = reg.conflict_with(&ctrl_c).unwrap();
        assert_eq!(entry.command, Command::Copy);
    }

    #[test]
    fn test_all_tool_shortcuts_registered() {
        let reg = ShortcutRegistry::new();
        let tools = [Key::V, Key::R, Key::O, Key::T, Key::P, Key::H, Key::Z, Key::F, Key::L, Key::I];
        for key in tools {
            let cmd = reg.resolve(&KeyBinding::bare(key));
            assert!(cmd.is_some(), "tool shortcut for {:?} should be registered", key);
        }
    }
}
