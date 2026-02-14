//! Plugin manifest and API trait definitions.
//!
//! Defines what a plugin IS (metadata) and what it CAN DO (API trait).
//!
//! Architecture:
//! ```text
//! ┌──────────────────┐
//! │  PluginManifest   │  WHO am I?
//! │  - id, name       │
//! │  - version        │
//! │  - author         │
//! │  - permissions    │
//! │  - entry_point    │
//! └────────┬─────────┘
//!          │ implements
//!          ▼
//! ┌──────────────────┐
//! │  PluginApi        │  WHAT can I do?
//! │  - on_load()      │
//! │  - on_save()      │
//! │  - on_selection() │
//! │  - on_command()   │
//! └──────────────────┘
//! ```
//!
//! Reference: Software Architecture: The Hard Parts — Plugin Contracts

use serde::{Deserialize, Serialize};
use std::time::Duration;
use uuid::Uuid;

use crate::permissions::PermissionSet;

/// Semantic version.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemVer {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl SemVer {
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self { major, minor, patch }
    }

    /// Check if this version satisfies a minimum requirement.
    pub fn satisfies(&self, min: &SemVer) -> bool {
        if self.major != min.major {
            return self.major > min.major;
        }
        if self.minor != min.minor {
            return self.minor > min.minor;
        }
        self.patch >= min.patch
    }
}

impl std::fmt::Display for SemVer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Plugin manifest — declares metadata, permissions, and entry point.
///
/// This is the `plugin.json` equivalent. It tells the host:
/// - WHO the plugin is (id, name, author)
/// - WHAT it needs (permissions)
/// - WHERE its code is (entry_point)
/// - WHEN it should run (hooks)
///
/// Reference: Secure Programming Cookbook — Declaring Capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Unique plugin identifier (stable across versions)
    pub id: Uuid,
    /// Human-readable name
    pub name: String,
    /// Plugin version
    pub version: SemVer,
    /// Author name
    pub author: String,
    /// Short description
    pub description: String,
    /// Entry point script (relative path or inline code)
    pub entry_point: String,
    /// Required permissions
    pub permissions: PermissionSet,
    /// Minimum Logos version required
    pub min_logos_version: SemVer,
    /// Maximum execution time (overrides default if lower)
    pub max_execution_time: Option<Duration>,
    /// Hooks this plugin listens to
    pub hooks: Vec<PluginHook>,
    /// Commands this plugin registers
    pub commands: Vec<PluginCommand>,
    /// Tags for marketplace categorization
    pub tags: Vec<String>,
    /// Icon URL or path
    pub icon: Option<String>,
    /// Homepage URL
    pub homepage: Option<String>,
}

impl PluginManifest {
    /// Create a minimal manifest for testing.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            version: SemVer::new(0, 1, 0),
            author: String::new(),
            description: String::new(),
            entry_point: String::new(),
            permissions: PermissionSet::none(),
            min_logos_version: SemVer::new(0, 1, 0),
            max_execution_time: None,
            hooks: Vec::new(),
            commands: Vec::new(),
            tags: Vec::new(),
            icon: None,
            homepage: None,
        }
    }

    /// Builder: set version.
    pub fn with_version(mut self, major: u32, minor: u32, patch: u32) -> Self {
        self.version = SemVer::new(major, minor, patch);
        self
    }

    /// Builder: set author.
    pub fn with_author(mut self, author: impl Into<String>) -> Self {
        self.author = author.into();
        self
    }

    /// Builder: set entry point.
    pub fn with_entry_point(mut self, entry: impl Into<String>) -> Self {
        self.entry_point = entry.into();
        self
    }

    /// Builder: set permissions.
    pub fn with_permissions(mut self, perms: PermissionSet) -> Self {
        self.permissions = perms;
        self
    }

    /// Builder: add a hook.
    pub fn with_hook(mut self, hook: PluginHook) -> Self {
        self.hooks.push(hook);
        self
    }

    /// Builder: add a command.
    pub fn with_command(mut self, cmd: PluginCommand) -> Self {
        self.commands.push(cmd);
        self
    }

    /// Validate the manifest for completeness.
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() {
            return Err("plugin name is required".into());
        }
        if self.entry_point.is_empty() {
            return Err("entry_point is required".into());
        }
        if self.name.len() > 128 {
            return Err("plugin name too long (max 128 chars)".into());
        }
        if self.description.len() > 4096 {
            return Err("description too long (max 4096 chars)".into());
        }
        Ok(())
    }
}

/// Lifecycle hooks a plugin can subscribe to.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PluginHook {
    /// Called when a document is loaded
    OnLoad,
    /// Called when a document is saved
    OnSave,
    /// Called when selection changes
    OnSelectionChange,
    /// Called on each frame (performance-critical!)
    OnFrame,
    /// Called when a layer is created
    OnLayerCreate,
    /// Called when a layer is deleted
    OnLayerDelete,
    /// Called when document is exported
    OnExport,
}

impl std::fmt::Display for PluginHook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OnLoad => write!(f, "on_load"),
            Self::OnSave => write!(f, "on_save"),
            Self::OnSelectionChange => write!(f, "on_selection_change"),
            Self::OnFrame => write!(f, "on_frame"),
            Self::OnLayerCreate => write!(f, "on_layer_create"),
            Self::OnLayerDelete => write!(f, "on_layer_delete"),
            Self::OnExport => write!(f, "on_export"),
        }
    }
}

/// A command registered by a plugin.
///
/// Commands appear in the command palette and can be bound to shortcuts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCommand {
    /// Command identifier (unique within plugin)
    pub id: String,
    /// Human-readable label
    pub label: String,
    /// Keyboard shortcut hint (e.g., "Ctrl+Shift+P")
    pub shortcut: Option<String>,
}

impl PluginCommand {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            shortcut: None,
        }
    }

    pub fn with_shortcut(mut self, shortcut: impl Into<String>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semver_display() {
        let v = SemVer::new(1, 2, 3);
        assert_eq!(v.to_string(), "1.2.3");
    }

    #[test]
    fn test_semver_satisfies() {
        let v = SemVer::new(1, 2, 3);
        assert!(v.satisfies(&SemVer::new(1, 2, 0)));
        assert!(v.satisfies(&SemVer::new(1, 2, 3)));
        assert!(!v.satisfies(&SemVer::new(1, 2, 4)));
        assert!(!v.satisfies(&SemVer::new(2, 0, 0)));
    }

    #[test]
    fn test_manifest_new() {
        let m = PluginManifest::new("Test Plugin");
        assert_eq!(m.name, "Test Plugin");
        assert_eq!(m.version.to_string(), "0.1.0");
    }

    #[test]
    fn test_manifest_builder() {
        let m = PluginManifest::new("My Plugin")
            .with_version(1, 0, 0)
            .with_author("Logos Team")
            .with_entry_point("main.js")
            .with_hook(PluginHook::OnLoad)
            .with_command(PluginCommand::new("do-thing", "Do Thing"));

        assert_eq!(m.version.to_string(), "1.0.0");
        assert_eq!(m.author, "Logos Team");
        assert_eq!(m.entry_point, "main.js");
        assert_eq!(m.hooks.len(), 1);
        assert_eq!(m.commands.len(), 1);
    }

    #[test]
    fn test_manifest_validate_ok() {
        let m = PluginManifest::new("Valid")
            .with_entry_point("index.js");
        assert!(m.validate().is_ok());
    }

    #[test]
    fn test_manifest_validate_no_name() {
        let m = PluginManifest::new("")
            .with_entry_point("index.js");
        assert!(m.validate().is_err());
    }

    #[test]
    fn test_manifest_validate_no_entry() {
        let m = PluginManifest::new("Plugin");
        assert!(m.validate().is_err());
    }

    #[test]
    fn test_plugin_hook_display() {
        assert_eq!(PluginHook::OnLoad.to_string(), "on_load");
        assert_eq!(PluginHook::OnFrame.to_string(), "on_frame");
    }

    #[test]
    fn test_plugin_command() {
        let cmd = PluginCommand::new("align", "Align Layers")
            .with_shortcut("Ctrl+Shift+A");
        assert_eq!(cmd.id, "align");
        assert_eq!(cmd.shortcut, Some("Ctrl+Shift+A".into()));
    }

    #[test]
    fn test_manifest_serialization() {
        let m = PluginManifest::new("Test")
            .with_entry_point("main.js")
            .with_version(1, 0, 0);
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("\"name\":\"Test\""));
        let parsed: PluginManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "Test");
    }
}
