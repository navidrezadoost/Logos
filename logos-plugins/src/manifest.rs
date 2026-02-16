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
//! Supports both JSON and TOML serialization:
//! - JSON: `serde_json` (existing, used for binary packaging)
//! - TOML: human-readable `plugin.toml` format for developers
//!
//! Reference: Software Architecture: The Hard Parts — Plugin Contracts

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use uuid::Uuid;

use crate::permissions::{PermissionKind, PermissionSet};

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

/// Plugin category for marketplace browsing and discovery.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum PluginCategory {
    /// Layout and alignment tools
    Layout,
    /// Color and styling tools
    Color,
    /// Typography tools
    Typography,
    /// Export and publishing
    Export,
    /// Accessibility checkers
    Accessibility,
    /// Animation and motion
    Animation,
    /// Collaboration tools
    Collaboration,
    /// Developer tools and debugging
    DevTools,
    /// Asset management
    Assets,
    /// Other / uncategorized
    Other,
}

impl std::fmt::Display for PluginCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Layout => write!(f, "layout"),
            Self::Color => write!(f, "color"),
            Self::Typography => write!(f, "typography"),
            Self::Export => write!(f, "export"),
            Self::Accessibility => write!(f, "accessibility"),
            Self::Animation => write!(f, "animation"),
            Self::Collaboration => write!(f, "collaboration"),
            Self::DevTools => write!(f, "devtools"),
            Self::Assets => write!(f, "assets"),
            Self::Other => write!(f, "other"),
        }
    }
}

impl Default for PluginCategory {
    fn default() -> Self {
        Self::Other
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
    // ─── Marketplace Metadata (Day 22) ───
    /// UI entry point (HTML file for panel rendering)
    pub ui_entry_point: Option<String>,
    /// Plugin category for marketplace browsing
    pub category: PluginCategory,
    /// License identifier (SPDX, e.g. "MIT", "Apache-2.0")
    pub license: Option<String>,
    /// Source repository URL
    pub repository: Option<String>,
    /// Icon map: size → PNG data path (16, 48, 128 px)
    pub icons: HashMap<u16, String>,
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
            ui_entry_point: None,
            category: PluginCategory::Other,
            license: None,
            repository: None,
            icons: HashMap::new(),
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

    /// Builder: set UI entry point (HTML file for panel rendering).
    pub fn with_ui_entry_point(mut self, entry: impl Into<String>) -> Self {
        self.ui_entry_point = Some(entry.into());
        self
    }

    /// Builder: set category.
    pub fn with_category(mut self, category: PluginCategory) -> Self {
        self.category = category;
        self
    }

    /// Builder: set license (SPDX identifier).
    pub fn with_license(mut self, license: impl Into<String>) -> Self {
        self.license = Some(license.into());
        self
    }

    /// Builder: set repository URL.
    pub fn with_repository(mut self, repo: impl Into<String>) -> Self {
        self.repository = Some(repo.into());
        self
    }

    /// Builder: add an icon at a specific size.
    pub fn with_icon(mut self, size: u16, path: impl Into<String>) -> Self {
        self.icons.insert(size, path.into());
        self
    }

    /// Builder: set description.
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Builder: add a tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
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

// ═══════════════════════════════════════════════════════════════
// TOML Manifest Support — `plugin.toml`
// ═══════════════════════════════════════════════════════════════

/// Intermediate TOML-friendly manifest representation.
///
/// TOML doesn't natively handle `Duration`, `Uuid`, or our bitflag
/// `PermissionSet`, so this struct uses simpler types that map cleanly
/// to the TOML format a developer would write by hand.
///
/// ## Example `plugin.toml`
///
/// ```toml
/// name = "Auto Grid"
/// version = "1.2.0"
/// author = "Logos Team"
/// description = "Snap layers to a configurable grid"
/// entry_point = "auto-grid.wasm"
/// category = "layout"
/// license = "MIT"
/// repository = "https://github.com/logos/auto-grid"
/// tags = ["grid", "alignment", "layout"]
/// hooks = ["on_load", "on_selection_change"]
///
/// [permissions]
/// granted = ["document:read", "document:write", "notifications"]
///
/// [[commands]]
/// id = "snap-to-grid"
/// label = "Snap to Grid"
/// shortcut = "Ctrl+Shift+G"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TomlManifest {
    /// Plugin name (required).
    pub name: String,
    /// Version string like "1.2.3" (required).
    pub version: String,
    /// Author (optional in TOML, defaults to "").
    pub author: Option<String>,
    /// Short description.
    pub description: Option<String>,
    /// Entry point script/WASM file path (required).
    pub entry_point: String,
    /// Category string (layout, color, typography, etc.).
    pub category: Option<String>,
    /// SPDX license identifier.
    pub license: Option<String>,
    /// Repository URL.
    pub repository: Option<String>,
    /// Homepage URL.
    pub homepage: Option<String>,
    /// Icon path.
    pub icon: Option<String>,
    /// UI entry point (HTML panel file).
    pub ui_entry_point: Option<String>,
    /// Tags for marketplace.
    pub tags: Option<Vec<String>>,
    /// Hook names: ["on_load", "on_save", "on_selection_change", etc.]
    pub hooks: Option<Vec<String>>,
    /// Maximum execution time in milliseconds.
    pub max_execution_time_ms: Option<u64>,
    /// Minimum Logos version required.
    pub min_logos_version: Option<String>,
    /// Plugin UUID (optional — generated if absent).
    pub id: Option<String>,
    /// Permission section.
    pub permissions: Option<TomlPermissions>,
    /// Commands section.
    pub commands: Option<Vec<TomlCommand>>,
    /// Icon paths by size (e.g. 16, 48, 128).
    pub icons: Option<HashMap<String, String>>,
}

/// TOML-friendly permission representation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TomlPermissions {
    /// Permission names: ["document:read", "document:write", "notifications"]
    pub granted: Vec<String>,
    /// Allowed network domains.
    pub allowed_domains: Option<Vec<String>>,
    /// Allowed filesystem paths.
    pub allowed_paths: Option<Vec<String>>,
}

/// TOML-friendly command representation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TomlCommand {
    /// Command identifier.
    pub id: String,
    /// Human-readable label.
    pub label: String,
    /// Keyboard shortcut hint.
    pub shortcut: Option<String>,
}

/// Error type for TOML parsing failures.
#[derive(Debug, Clone)]
pub enum ManifestError {
    /// TOML syntax/parsing error.
    ParseError(String),
    /// Semantic validation error (missing fields, invalid values).
    ValidationError(String),
    /// Version string doesn't match "major.minor.patch".
    InvalidVersion(String),
    /// Unknown hook name.
    UnknownHook(String),
    /// Unknown permission name.
    UnknownPermission(String),
    /// Unknown category name.
    UnknownCategory(String),
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ParseError(e) => write!(f, "TOML parse error: {e}"),
            Self::ValidationError(e) => write!(f, "manifest validation error: {e}"),
            Self::InvalidVersion(v) => write!(f, "invalid version '{v}' — expected major.minor.patch"),
            Self::UnknownHook(h) => write!(f, "unknown hook: '{h}'"),
            Self::UnknownPermission(p) => write!(f, "unknown permission: '{p}'"),
            Self::UnknownCategory(c) => write!(f, "unknown category: '{c}'"),
        }
    }
}

impl std::error::Error for ManifestError {}

impl PluginManifest {
    /// Parse a `plugin.toml` string into a `PluginManifest`.
    ///
    /// Generates a UUID if `id` is not specified in the TOML.
    /// Validates all fields and returns descriptive errors.
    pub fn from_toml_str(toml_str: &str) -> Result<Self, ManifestError> {
        let tm: TomlManifest = toml::from_str(toml_str)
            .map_err(|e| ManifestError::ParseError(e.to_string()))?;

        // Parse version
        let version = parse_semver(&tm.version)?;

        // Parse min_logos_version
        let min_logos_version = match &tm.min_logos_version {
            Some(v) => parse_semver(v)?,
            None => SemVer::new(0, 1, 0),
        };

        // Parse category
        let category = match &tm.category {
            Some(c) => parse_category(c)?,
            None => PluginCategory::Other,
        };

        // Parse hooks
        let hooks = match &tm.hooks {
            Some(names) => names.iter()
                .map(|n| parse_hook(n))
                .collect::<Result<Vec<_>, _>>()?,
            None => Vec::new(),
        };

        // Parse permissions
        let permissions = match &tm.permissions {
            Some(tp) => parse_permissions(tp)?,
            None => PermissionSet::none(),
        };

        // Parse commands
        let commands = match &tm.commands {
            Some(cmds) => cmds.iter().map(|c| {
                let mut cmd = PluginCommand::new(&c.id, &c.label);
                if let Some(ref s) = c.shortcut {
                    cmd = cmd.with_shortcut(s);
                }
                cmd
            }).collect(),
            None => Vec::new(),
        };

        // Parse icons
        let icons = match &tm.icons {
            Some(map) => {
                let mut result = HashMap::new();
                for (size_str, path) in map {
                    let size: u16 = size_str.parse()
                        .map_err(|_| ManifestError::ValidationError(
                            format!("invalid icon size '{size_str}' — expected integer")))?;
                    result.insert(size, path.clone());
                }
                result
            }
            None => HashMap::new(),
        };

        // Parse UUID or generate
        let id = match &tm.id {
            Some(s) => Uuid::parse_str(s)
                .map_err(|e| ManifestError::ValidationError(format!("invalid UUID: {e}")))?,
            None => Uuid::new_v4(),
        };

        let manifest = Self {
            id,
            name: tm.name,
            version,
            author: tm.author.unwrap_or_default(),
            description: tm.description.unwrap_or_default(),
            entry_point: tm.entry_point,
            permissions,
            min_logos_version,
            max_execution_time: tm.max_execution_time_ms.map(Duration::from_millis),
            hooks,
            commands,
            tags: tm.tags.unwrap_or_default(),
            icon: tm.icon,
            homepage: tm.homepage,
            ui_entry_point: tm.ui_entry_point,
            category,
            license: tm.license,
            repository: tm.repository,
            icons,
        };

        manifest.validate()
            .map_err(|e| ManifestError::ValidationError(e))?;

        Ok(manifest)
    }

    /// Serialize this manifest to a TOML string.
    ///
    /// Produces a human-readable `plugin.toml` suitable for
    /// inclusion in a plugin source repository.
    pub fn to_toml_string(&self) -> Result<String, ManifestError> {
        let hooks: Vec<String> = self.hooks.iter().map(|h| h.to_string()).collect();
        let permissions = if self.permissions.granted.is_empty() {
            None
        } else {
            let granted: Vec<String> = self.permissions.granted
                .iter()
                .map(|p| p.to_string())
                .collect();
            let allowed_domains = if self.permissions.allowed_domains.is_empty() {
                None
            } else {
                Some(self.permissions.allowed_domains.clone())
            };
            let allowed_paths = if self.permissions.allowed_paths.is_empty() {
                None
            } else {
                Some(self.permissions.allowed_paths.clone())
            };
            Some(TomlPermissions { granted, allowed_domains, allowed_paths })
        };

        let commands = if self.commands.is_empty() {
            None
        } else {
            Some(self.commands.iter().map(|c| TomlCommand {
                id: c.id.clone(),
                label: c.label.clone(),
                shortcut: c.shortcut.clone(),
            }).collect())
        };

        let icons = if self.icons.is_empty() {
            None
        } else {
            Some(self.icons.iter().map(|(k, v)| (k.to_string(), v.clone())).collect())
        };

        let tm = TomlManifest {
            name: self.name.clone(),
            version: self.version.to_string(),
            author: if self.author.is_empty() { None } else { Some(self.author.clone()) },
            description: if self.description.is_empty() { None } else { Some(self.description.clone()) },
            entry_point: self.entry_point.clone(),
            category: Some(self.category.to_string()),
            license: self.license.clone(),
            repository: self.repository.clone(),
            homepage: self.homepage.clone(),
            icon: self.icon.clone(),
            ui_entry_point: self.ui_entry_point.clone(),
            tags: if self.tags.is_empty() { None } else { Some(self.tags.clone()) },
            hooks: if hooks.is_empty() { None } else { Some(hooks) },
            max_execution_time_ms: self.max_execution_time.map(|d| d.as_millis() as u64),
            min_logos_version: Some(self.min_logos_version.to_string()),
            id: Some(self.id.to_string()),
            permissions,
            commands,
            icons,
        };

        toml::to_string_pretty(&tm)
            .map_err(|e| ManifestError::ParseError(format!("TOML serialization failed: {e}")))
    }
}

// ── TOML Parsing Helpers ────────────────────────────────────────

/// Parse a "major.minor.patch" version string.
fn parse_semver(s: &str) -> Result<SemVer, ManifestError> {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 3 {
        return Err(ManifestError::InvalidVersion(s.to_string()));
    }
    let major: u32 = parts[0].parse()
        .map_err(|_| ManifestError::InvalidVersion(s.to_string()))?;
    let minor: u32 = parts[1].parse()
        .map_err(|_| ManifestError::InvalidVersion(s.to_string()))?;
    let patch: u32 = parts[2].parse()
        .map_err(|_| ManifestError::InvalidVersion(s.to_string()))?;
    Ok(SemVer::new(major, minor, patch))
}

/// Parse a hook name string to a `PluginHook` enum.
fn parse_hook(name: &str) -> Result<PluginHook, ManifestError> {
    match name {
        "on_load" => Ok(PluginHook::OnLoad),
        "on_save" => Ok(PluginHook::OnSave),
        "on_selection_change" => Ok(PluginHook::OnSelectionChange),
        "on_frame" => Ok(PluginHook::OnFrame),
        "on_layer_create" => Ok(PluginHook::OnLayerCreate),
        "on_layer_delete" => Ok(PluginHook::OnLayerDelete),
        "on_export" => Ok(PluginHook::OnExport),
        other => Err(ManifestError::UnknownHook(other.to_string())),
    }
}

/// Parse a category name string to a `PluginCategory` enum.
fn parse_category(name: &str) -> Result<PluginCategory, ManifestError> {
    match name {
        "layout" => Ok(PluginCategory::Layout),
        "color" => Ok(PluginCategory::Color),
        "typography" => Ok(PluginCategory::Typography),
        "export" => Ok(PluginCategory::Export),
        "accessibility" => Ok(PluginCategory::Accessibility),
        "animation" => Ok(PluginCategory::Animation),
        "collaboration" => Ok(PluginCategory::Collaboration),
        "devtools" => Ok(PluginCategory::DevTools),
        "assets" => Ok(PluginCategory::Assets),
        "other" => Ok(PluginCategory::Other),
        other => Err(ManifestError::UnknownCategory(other.to_string())),
    }
}

/// Parse a permission name string to a `PermissionKind` enum.
fn parse_permission(name: &str) -> Result<PermissionKind, ManifestError> {
    match name {
        "document:read" => Ok(PermissionKind::DocumentRead),
        "document:write" => Ok(PermissionKind::DocumentWrite),
        "network" => Ok(PermissionKind::Network),
        "file:read" => Ok(PermissionKind::FileRead),
        "file:write" => Ok(PermissionKind::FileWrite),
        "clipboard" => Ok(PermissionKind::Clipboard),
        "notifications" => Ok(PermissionKind::Notifications),
        "user:preferences" => Ok(PermissionKind::UserPreferences),
        "background" => Ok(PermissionKind::Background),
        other => Err(ManifestError::UnknownPermission(other.to_string())),
    }
}

/// Parse a `TomlPermissions` into a `PermissionSet`.
fn parse_permissions(tp: &TomlPermissions) -> Result<PermissionSet, ManifestError> {
    let mut perms = PermissionSet::none();
    for name in &tp.granted {
        let kind = parse_permission(name)?;
        perms.grant(kind);
    }
    if let Some(ref domains) = tp.allowed_domains {
        for d in domains {
            perms.allow_domain(d);
        }
    }
    if let Some(ref paths) = tp.allowed_paths {
        for p in paths {
            perms.allow_path(p);
        }
    }
    Ok(perms)
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

    // ─── Marketplace Metadata Tests (Day 22) ───

    #[test]
    fn test_manifest_marketplace_fields() {
        let m = PluginManifest::new("Market Plugin")
            .with_entry_point("main.js")
            .with_ui_entry_point("panel.html")
            .with_category(PluginCategory::Layout)
            .with_license("MIT")
            .with_repository("https://github.com/logos/auto-grid")
            .with_description("Auto grid plugin")
            .with_tag("grid")
            .with_tag("layout")
            .with_icon(16, "icons/16.png")
            .with_icon(48, "icons/48.png")
            .with_icon(128, "icons/128.png");

        assert_eq!(m.ui_entry_point, Some("panel.html".into()));
        assert_eq!(m.category, PluginCategory::Layout);
        assert_eq!(m.license, Some("MIT".into()));
        assert_eq!(m.repository, Some("https://github.com/logos/auto-grid".into()));
        assert_eq!(m.description, "Auto grid plugin");
        assert_eq!(m.tags, vec!["grid", "layout"]);
        assert_eq!(m.icons.len(), 3);
        assert_eq!(m.icons.get(&16), Some(&"icons/16.png".into()));
    }

    #[test]
    fn test_plugin_category_display() {
        assert_eq!(PluginCategory::Layout.to_string(), "layout");
        assert_eq!(PluginCategory::Color.to_string(), "color");
        assert_eq!(PluginCategory::Typography.to_string(), "typography");
        assert_eq!(PluginCategory::Export.to_string(), "export");
        assert_eq!(PluginCategory::Accessibility.to_string(), "accessibility");
        assert_eq!(PluginCategory::Animation.to_string(), "animation");
        assert_eq!(PluginCategory::Collaboration.to_string(), "collaboration");
        assert_eq!(PluginCategory::DevTools.to_string(), "devtools");
        assert_eq!(PluginCategory::Assets.to_string(), "assets");
        assert_eq!(PluginCategory::Other.to_string(), "other");
    }

    #[test]
    fn test_plugin_category_default() {
        assert_eq!(PluginCategory::default(), PluginCategory::Other);
    }

    #[test]
    fn test_marketplace_manifest_serialization() {
        let m = PluginManifest::new("Serialized")
            .with_entry_point("main.js")
            .with_category(PluginCategory::Color)
            .with_license("Apache-2.0")
            .with_icon(48, "icon48.png");

        let json = serde_json::to_string(&m).unwrap();
        let parsed: PluginManifest = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.category, PluginCategory::Color);
        assert_eq!(parsed.license, Some("Apache-2.0".into()));
        assert_eq!(parsed.icons.get(&48), Some(&"icon48.png".into()));
    }

    #[test]
    fn test_manifest_defaults() {
        let m = PluginManifest::new("Defaults");
        assert_eq!(m.category, PluginCategory::Other);
        assert_eq!(m.ui_entry_point, None);
        assert_eq!(m.license, None);
        assert_eq!(m.repository, None);
        assert!(m.icons.is_empty());
    }

    // ═══════════════════════════════════════════════════════════
    // TOML Manifest Tests (Week 3)
    // ═══════════════════════════════════════════════════════════

    #[test]
    fn test_toml_parse_minimal() {
        let toml = r#"
            name = "Test Plugin"
            version = "1.0.0"
            entry_point = "main.wasm"
        "#;
        let m = PluginManifest::from_toml_str(toml).unwrap();
        assert_eq!(m.name, "Test Plugin");
        assert_eq!(m.version, SemVer::new(1, 0, 0));
        assert_eq!(m.entry_point, "main.wasm");
        assert_eq!(m.category, PluginCategory::Other);
    }

    #[test]
    fn test_toml_parse_full() {
        let toml = r#"
            name = "Auto Grid"
            version = "1.2.0"
            author = "Logos Team"
            description = "Snap layers to a configurable grid"
            entry_point = "auto-grid.wasm"
            category = "layout"
            license = "MIT"
            repository = "https://github.com/logos/auto-grid"
            tags = ["grid", "alignment", "layout"]
            hooks = ["on_load", "on_selection_change"]
            max_execution_time_ms = 500

            [permissions]
            granted = ["document:read", "document:write", "notifications"]

            [[commands]]
            id = "snap-to-grid"
            label = "Snap to Grid"
            shortcut = "Ctrl+Shift+G"

            [[commands]]
            id = "configure-grid"
            label = "Configure Grid"
        "#;
        let m = PluginManifest::from_toml_str(toml).unwrap();
        assert_eq!(m.name, "Auto Grid");
        assert_eq!(m.version, SemVer::new(1, 2, 0));
        assert_eq!(m.author, "Logos Team");
        assert_eq!(m.category, PluginCategory::Layout);
        assert_eq!(m.license, Some("MIT".to_string()));
        assert_eq!(m.tags, vec!["grid", "alignment", "layout"]);
        assert_eq!(m.hooks.len(), 2);
        assert_eq!(m.commands.len(), 2);
        assert_eq!(m.commands[0].id, "snap-to-grid");
        assert_eq!(m.commands[0].shortcut, Some("Ctrl+Shift+G".to_string()));
        assert!(m.permissions.has(&crate::permissions::PermissionKind::DocumentRead));
        assert!(m.permissions.has(&crate::permissions::PermissionKind::Notifications));
        assert_eq!(m.max_execution_time, Some(std::time::Duration::from_millis(500)));
    }

    #[test]
    fn test_toml_parse_with_permissions_domains() {
        let toml = r#"
            name = "Network Plugin"
            version = "0.1.0"
            entry_point = "net.wasm"

            [permissions]
            granted = ["network", "document:read"]
            allowed_domains = ["api.logos.dev", "cdn.logos.dev"]
        "#;
        let m = PluginManifest::from_toml_str(toml).unwrap();
        assert!(m.permissions.has(&crate::permissions::PermissionKind::Network));
        assert_eq!(m.permissions.allowed_domains, vec!["api.logos.dev", "cdn.logos.dev"]);
    }

    #[test]
    fn test_toml_parse_invalid_version() {
        let toml = r#"
            name = "Bad"
            version = "1.2"
            entry_point = "main.wasm"
        "#;
        let err = PluginManifest::from_toml_str(toml).unwrap_err();
        assert!(matches!(err, ManifestError::InvalidVersion(_)));
    }

    #[test]
    fn test_toml_parse_unknown_hook() {
        let toml = r#"
            name = "Bad"
            version = "1.0.0"
            entry_point = "main.wasm"
            hooks = ["on_load", "on_explode"]
        "#;
        let err = PluginManifest::from_toml_str(toml).unwrap_err();
        assert!(matches!(err, ManifestError::UnknownHook(_)));
    }

    #[test]
    fn test_toml_parse_unknown_permission() {
        let toml = r#"
            name = "Bad"
            version = "1.0.0"
            entry_point = "main.wasm"

            [permissions]
            granted = ["admin:root"]
        "#;
        let err = PluginManifest::from_toml_str(toml).unwrap_err();
        assert!(matches!(err, ManifestError::UnknownPermission(_)));
    }

    #[test]
    fn test_toml_parse_unknown_category() {
        let toml = r#"
            name = "Bad"
            version = "1.0.0"
            entry_point = "main.wasm"
            category = "weapons"
        "#;
        let err = PluginManifest::from_toml_str(toml).unwrap_err();
        assert!(matches!(err, ManifestError::UnknownCategory(_)));
    }

    #[test]
    fn test_toml_parse_invalid_toml() {
        let err = PluginManifest::from_toml_str("not { valid toml ][").unwrap_err();
        assert!(matches!(err, ManifestError::ParseError(_)));
    }

    #[test]
    fn test_toml_parse_missing_name() {
        let toml = r#"
            version = "1.0.0"
            entry_point = "main.wasm"
        "#;
        let err = PluginManifest::from_toml_str(toml);
        assert!(err.is_err()); // missing required field
    }

    #[test]
    fn test_toml_roundtrip() {
        let original = PluginManifest::new("Roundtrip Test")
            .with_entry_point("plugin.wasm")
            .with_version(2, 1, 0)
            .with_author("Test Author")
            .with_description("A test plugin")
            .with_category(PluginCategory::Typography)
            .with_license("Apache-2.0")
            .with_hook(PluginHook::OnLoad)
            .with_hook(PluginHook::OnSave)
            .with_command(PluginCommand::new("do-thing", "Do Thing")
                .with_shortcut("Ctrl+D"))
            .with_tag("test")
            .with_tag("roundtrip");

        let toml_str = original.to_toml_string().unwrap();
        assert!(toml_str.contains("Roundtrip Test"));
        assert!(toml_str.contains("2.1.0"));
        assert!(toml_str.contains("plugin.wasm"));

        let parsed = PluginManifest::from_toml_str(&toml_str).unwrap();
        assert_eq!(parsed.name, original.name);
        assert_eq!(parsed.version, original.version);
        assert_eq!(parsed.author, original.author);
        assert_eq!(parsed.category, original.category);
        assert_eq!(parsed.hooks.len(), original.hooks.len());
        assert_eq!(parsed.commands.len(), original.commands.len());
        assert_eq!(parsed.tags, original.tags);
    }

    #[test]
    fn test_toml_roundtrip_with_permissions() {
        let mut perms = crate::permissions::PermissionSet::document_full();
        perms.grant(crate::permissions::PermissionKind::Network);
        perms.allow_domain("api.example.com");

        let original = PluginManifest::new("Perms Test")
            .with_entry_point("main.wasm")
            .with_permissions(perms);

        let toml_str = original.to_toml_string().unwrap();
        let parsed = PluginManifest::from_toml_str(&toml_str).unwrap();
        assert!(parsed.permissions.has(&crate::permissions::PermissionKind::DocumentRead));
        assert!(parsed.permissions.has(&crate::permissions::PermissionKind::DocumentWrite));
        assert!(parsed.permissions.has(&crate::permissions::PermissionKind::Network));
    }

    #[test]
    fn test_toml_to_string() {
        let m = PluginManifest::new("TOML Output")
            .with_entry_point("main.wasm")
            .with_version(1, 0, 0);
        let toml_str = m.to_toml_string().unwrap();
        assert!(toml_str.contains("name = \"TOML Output\""));
        assert!(toml_str.contains("version = \"1.0.0\""));
        assert!(toml_str.contains("entry_point = \"main.wasm\""));
    }

    #[test]
    fn test_toml_with_icons() {
        let toml = r#"
            name = "Icon Plugin"
            version = "1.0.0"
            entry_point = "main.wasm"

            [icons]
            16 = "icons/16.png"
            48 = "icons/48.png"
            128 = "icons/128.png"
        "#;
        let m = PluginManifest::from_toml_str(toml).unwrap();
        assert_eq!(m.icons.len(), 3);
        assert_eq!(m.icons.get(&16), Some(&"icons/16.png".to_string()));
        assert_eq!(m.icons.get(&128), Some(&"icons/128.png".to_string()));
    }

    #[test]
    fn test_manifest_error_display() {
        assert_eq!(
            ManifestError::UnknownHook("foo".into()).to_string(),
            "unknown hook: 'foo'"
        );
        assert_eq!(
            ManifestError::InvalidVersion("bad".into()).to_string(),
            "invalid version 'bad' — expected major.minor.patch"
        );
        assert_eq!(
            ManifestError::UnknownPermission("admin".into()).to_string(),
            "unknown permission: 'admin'"
        );
        assert_eq!(
            ManifestError::UnknownCategory("weapons".into()).to_string(),
            "unknown category: 'weapons'"
        );
    }

    #[test]
    fn test_parse_semver_helper() {
        assert_eq!(parse_semver("1.2.3").unwrap(), SemVer::new(1, 2, 3));
        assert_eq!(parse_semver("0.0.0").unwrap(), SemVer::new(0, 0, 0));
        assert!(parse_semver("1.2").is_err());
        assert!(parse_semver("abc").is_err());
    }
}
