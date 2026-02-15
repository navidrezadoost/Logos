//! Local plugin registry — installed plugins, versioning, lifecycle.
//!
//! The registry manages the collection of installed plugins on a user's
//! machine. It tracks versions, enabled/disabled state, and handles
//! install, uninstall, upgrade, and rollback operations.
//!
//! ## Architecture
//!
//! ```text
//! PluginRegistry
//!   ├── InstalledPlugin("auto-grid")
//!   │     ├── manifest: PluginManifest
//!   │     ├── package_hash: ContentHash
//!   │     ├── enabled: true
//!   │     ├── installed_at: Instant
//!   │     └── source: RegistrySource::Local
//!   │
//!   ├── InstalledPlugin("color-gen")
//!   │     ├── manifest: PluginManifest
//!   │     ├── enabled: false
//!   │     └── source: RegistrySource::Marketplace
//!   │
//!   └── InstalledPlugin("hello-world")
//!         ├── manifest: PluginManifest
//!         ├── enabled: true
//!         └── source: RegistrySource::Local
//! ```
//!
//! ## Performance Targets
//!
//! | Operation          | Target  | Reference                    |
//! |--------------------|---------|------------------------------|
//! | Registry lookup    | <1μs    | Software Architecture        |
//! | Install plugin     | <5ms    | Software Architecture        |
//! | List all plugins   | <10μs   | Software Architecture        |
//! | Enable/disable     | <1μs    | Software Architecture        |

use crate::manifest::{PluginManifest, SemVer};
use crate::packaging::PluginPackage;
use crate::signing::ContentHash;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use uuid::Uuid;

/// Where a plugin was installed from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegistrySource {
    /// Installed from local .logos-plugin file
    Local,
    /// Installed from the Logos Marketplace
    Marketplace,
    /// Installed from a URL
    Url(String),
    /// Built-in plugin (bundled with Logos)
    BuiltIn,
    /// Developer mode (loaded from source)
    Development,
}

impl std::fmt::Display for RegistrySource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Local => write!(f, "local"),
            Self::Marketplace => write!(f, "marketplace"),
            Self::Url(url) => write!(f, "url:{url}"),
            Self::BuiltIn => write!(f, "built-in"),
            Self::Development => write!(f, "development"),
        }
    }
}

/// An installed plugin entry in the registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPlugin {
    /// Plugin manifest
    pub manifest: PluginManifest,
    /// SHA-256 hash of the installed package content
    pub content_hash: ContentHash,
    /// Whether the plugin is currently enabled
    pub enabled: bool,
    /// Installation timestamp (seconds since UNIX epoch)
    pub installed_at: u64,
    /// Last updated timestamp
    pub updated_at: u64,
    /// Installation source
    pub source: RegistrySource,
    /// Whether the package was signed
    pub is_signed: bool,
    /// Signer's public key hex (if signed)
    pub signer_key: Option<String>,
}

impl InstalledPlugin {
    /// Create from a plugin package.
    fn from_package(package: &PluginPackage, source: RegistrySource) -> Self {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs();

        let signer_key = package.signature.as_ref().map(|sig| {
            sig.public_key_bytes
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        });

        Self {
            manifest: package.manifest.clone(),
            content_hash: package.content_hash.clone(),
            enabled: true,
            installed_at: now,
            updated_at: now,
            is_signed: package.is_signed(),
            signer_key,
            source,
        }
    }

    /// Plugin name.
    pub fn name(&self) -> &str {
        &self.manifest.name
    }

    /// Plugin version.
    pub fn version(&self) -> &SemVer {
        &self.manifest.version
    }

    /// Plugin UUID.
    pub fn id(&self) -> &Uuid {
        &self.manifest.id
    }
}

/// Errors from registry operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    /// Plugin not found in registry
    NotFound(String),
    /// Plugin already installed
    AlreadyInstalled(String),
    /// Version conflict
    VersionConflict {
        name: String,
        installed: String,
        attempted: String,
    },
    /// Plugin is a built-in and cannot be uninstalled
    BuiltInProtected(String),
    /// Package verification failed
    VerificationFailed(String),
    /// Registry is at capacity
    CapacityReached(usize),
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(id) => write!(f, "plugin not found: {id}"),
            Self::AlreadyInstalled(name) => write!(f, "plugin already installed: {name}"),
            Self::VersionConflict {
                name,
                installed,
                attempted,
            } => write!(
                f,
                "version conflict for {name}: installed={installed}, attempted={attempted}"
            ),
            Self::BuiltInProtected(name) => {
                write!(f, "built-in plugin cannot be removed: {name}")
            }
            Self::VerificationFailed(msg) => write!(f, "verification failed: {msg}"),
            Self::CapacityReached(max) => write!(f, "registry capacity reached ({max})"),
        }
    }
}

impl std::error::Error for RegistryError {}

/// Result type for registry operations.
pub type RegistryResult<T> = Result<T, RegistryError>;

/// Search/filter criteria for listing plugins.
#[derive(Debug, Clone, Default)]
pub struct PluginFilter {
    /// Filter by name (substring match)
    pub name: Option<String>,
    /// Filter by enabled state
    pub enabled: Option<bool>,
    /// Filter by source
    pub source: Option<RegistrySource>,
    /// Filter by signed state
    pub signed: Option<bool>,
    /// Filter by author (substring match)
    pub author: Option<String>,
}

impl PluginFilter {
    /// Create a filter that matches everything.
    pub fn all() -> Self {
        Self::default()
    }

    /// Filter by name.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Filter by enabled state.
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = Some(enabled);
        self
    }

    /// Filter by source.
    pub fn with_source(mut self, source: RegistrySource) -> Self {
        self.source = Some(source);
        self
    }

    /// Filter by signed state.
    pub fn with_signed(mut self, signed: bool) -> Self {
        self.signed = Some(signed);
        self
    }

    /// Check if an installed plugin matches this filter.
    fn matches(&self, plugin: &InstalledPlugin) -> bool {
        if let Some(ref name) = self.name {
            if !plugin.manifest.name.to_lowercase().contains(&name.to_lowercase()) {
                return false;
            }
        }
        if let Some(enabled) = self.enabled {
            if plugin.enabled != enabled {
                return false;
            }
        }
        if let Some(ref source) = self.source {
            if &plugin.source != source {
                return false;
            }
        }
        if let Some(signed) = self.signed {
            if plugin.is_signed != signed {
                return false;
            }
        }
        if let Some(ref author) = self.author {
            if !plugin
                .manifest
                .author
                .to_lowercase()
                .contains(&author.to_lowercase())
            {
                return false;
            }
        }
        true
    }
}

/// Local plugin registry.
///
/// Stores installed plugins and their metadata.
/// Thread-safe via interior mutability patterns at the caller level.
///
/// Performance: HashMap-based, O(1) lookup by plugin ID.
pub struct PluginRegistry {
    /// Installed plugins keyed by UUID string
    plugins: HashMap<String, InstalledPlugin>,
    /// Maximum number of installed plugins
    max_plugins: usize,
    /// Trusted public keys (hex-encoded)
    trusted_keys: Vec<String>,
    /// Require signatures for installation
    require_signatures: bool,
}

impl PluginRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
            max_plugins: 256,
            trusted_keys: Vec::new(),
            require_signatures: false,
        }
    }

    /// Create a registry with security policy.
    pub fn with_policy(max_plugins: usize, require_signatures: bool) -> Self {
        Self {
            plugins: HashMap::new(),
            max_plugins,
            trusted_keys: Vec::new(),
            require_signatures,
        }
    }

    /// Set max plugins capacity.
    pub fn set_max_plugins(&mut self, max: usize) {
        self.max_plugins = max;
    }

    /// Add a trusted signing key.
    pub fn add_trusted_key(&mut self, key_hex: impl Into<String>) {
        self.trusted_keys.push(key_hex.into());
    }

    /// Set whether signatures are required.
    pub fn set_require_signatures(&mut self, require: bool) {
        self.require_signatures = require;
    }

    /// Install a plugin from a package.
    ///
    /// Verifies signature (if required), checks for duplicates,
    /// and adds to the registry.
    pub fn install(
        &mut self,
        package: &PluginPackage,
        source: RegistrySource,
    ) -> RegistryResult<()> {
        // Check capacity
        if self.plugins.len() >= self.max_plugins {
            return Err(RegistryError::CapacityReached(self.max_plugins));
        }

        // Verify signature if required
        if self.require_signatures && !package.is_signed() {
            return Err(RegistryError::VerificationFailed(
                "package is not signed (signatures required)".into(),
            ));
        }

        // Verify signature if present
        if package.is_signed() {
            package.verify_signature().map_err(|e| {
                RegistryError::VerificationFailed(format!("signature invalid: {e}"))
            })?;
        }

        // Check for integrity
        package.verify_integrity().map_err(|e| {
            RegistryError::VerificationFailed(format!("integrity check failed: {e}"))
        })?;

        let plugin_id = package.manifest.id.to_string();

        // Check for duplicates
        if self.plugins.contains_key(&plugin_id) {
            return Err(RegistryError::AlreadyInstalled(
                package.manifest.name.clone(),
            ));
        }

        let entry = InstalledPlugin::from_package(package, source);
        self.plugins.insert(plugin_id, entry);
        Ok(())
    }

    /// Uninstall a plugin by ID.
    pub fn uninstall(&mut self, plugin_id: &str) -> RegistryResult<()> {
        let entry = self.plugins.get(plugin_id).ok_or_else(|| {
            RegistryError::NotFound(plugin_id.to_string())
        })?;

        // Protect built-in plugins
        if entry.source == RegistrySource::BuiltIn {
            return Err(RegistryError::BuiltInProtected(entry.manifest.name.clone()));
        }

        self.plugins.remove(plugin_id);
        Ok(())
    }

    /// Upgrade a plugin to a new version.
    ///
    /// Validates that the new version is higher than the installed one.
    pub fn upgrade(
        &mut self,
        package: &PluginPackage,
        source: RegistrySource,
    ) -> RegistryResult<()> {
        let plugin_id = package.manifest.id.to_string();

        // Must already be installed
        let existing = self.plugins.get(&plugin_id).ok_or_else(|| {
            RegistryError::NotFound(plugin_id.clone())
        })?;

        // Version must be newer
        let installed_ver = &existing.manifest.version;
        let new_ver = &package.manifest.version;
        if !is_newer(new_ver, installed_ver) {
            return Err(RegistryError::VersionConflict {
                name: package.manifest.name.clone(),
                installed: installed_ver.to_string(),
                attempted: new_ver.to_string(),
            });
        }

        // Verify signature if required
        if self.require_signatures && !package.is_signed() {
            return Err(RegistryError::VerificationFailed(
                "package is not signed (signatures required)".into(),
            ));
        }

        if package.is_signed() {
            package.verify_signature().map_err(|e| {
                RegistryError::VerificationFailed(format!("signature invalid: {e}"))
            })?;
        }

        let mut entry = InstalledPlugin::from_package(package, source);
        // Preserve enabled state from existing installation
        entry.enabled = existing.enabled;
        // Preserve original install time
        entry.installed_at = existing.installed_at;

        self.plugins.insert(plugin_id, entry);
        Ok(())
    }

    /// Enable a plugin.
    pub fn enable(&mut self, plugin_id: &str) -> RegistryResult<()> {
        let entry = self.plugins.get_mut(plugin_id).ok_or_else(|| {
            RegistryError::NotFound(plugin_id.to_string())
        })?;
        entry.enabled = true;
        Ok(())
    }

    /// Disable a plugin.
    pub fn disable(&mut self, plugin_id: &str) -> RegistryResult<()> {
        let entry = self.plugins.get_mut(plugin_id).ok_or_else(|| {
            RegistryError::NotFound(plugin_id.to_string())
        })?;
        entry.enabled = false;
        Ok(())
    }

    /// Look up a plugin by ID.
    ///
    /// Performance target: <1μs (HashMap lookup).
    pub fn get(&self, plugin_id: &str) -> Option<&InstalledPlugin> {
        self.plugins.get(plugin_id)
    }

    /// Check if a plugin is installed.
    pub fn is_installed(&self, plugin_id: &str) -> bool {
        self.plugins.contains_key(plugin_id)
    }

    /// Count of installed plugins.
    pub fn count(&self) -> usize {
        self.plugins.len()
    }

    /// Count of enabled plugins.
    pub fn enabled_count(&self) -> usize {
        self.plugins.values().filter(|p| p.enabled).count()
    }

    /// List all installed plugin IDs.
    pub fn list_ids(&self) -> Vec<&str> {
        self.plugins.keys().map(|s| s.as_str()).collect()
    }

    /// List all installed plugins.
    pub fn list_all(&self) -> Vec<&InstalledPlugin> {
        self.plugins.values().collect()
    }

    /// Search/filter plugins.
    pub fn search(&self, filter: &PluginFilter) -> Vec<&InstalledPlugin> {
        self.plugins
            .values()
            .filter(|p| filter.matches(p))
            .collect()
    }

    /// Find a plugin by name (exact match).
    pub fn find_by_name(&self, name: &str) -> Option<&InstalledPlugin> {
        self.plugins.values().find(|p| p.manifest.name == name)
    }

    /// Get all enabled plugins.
    pub fn enabled_plugins(&self) -> Vec<&InstalledPlugin> {
        self.plugins.values().filter(|p| p.enabled).collect()
    }

    /// Get all disabled plugins.
    pub fn disabled_plugins(&self) -> Vec<&InstalledPlugin> {
        self.plugins.values().filter(|p| !p.enabled).collect()
    }

    /// Clear all plugins (for testing).
    pub fn clear(&mut self) {
        self.plugins.clear();
    }

    /// Serialize registry state to JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        let entries: Vec<&InstalledPlugin> = self.plugins.values().collect();
        serde_json::to_string_pretty(&entries)
    }

    /// Get registry statistics.
    pub fn stats(&self) -> RegistryStats {
        let total = self.plugins.len();
        let enabled = self.enabled_count();
        let signed = self.plugins.values().filter(|p| p.is_signed).count();
        let from_marketplace = self
            .plugins
            .values()
            .filter(|p| p.source == RegistrySource::Marketplace)
            .count();

        RegistryStats {
            total,
            enabled,
            disabled: total - enabled,
            signed,
            unsigned: total - signed,
            from_marketplace,
            from_local: total - from_marketplace,
            capacity: self.max_plugins,
        }
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Registry statistics.
#[derive(Debug, Clone)]
pub struct RegistryStats {
    pub total: usize,
    pub enabled: usize,
    pub disabled: usize,
    pub signed: usize,
    pub unsigned: usize,
    pub from_marketplace: usize,
    pub from_local: usize,
    pub capacity: usize,
}

impl std::fmt::Display for RegistryStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Plugins: {}/{} (enabled: {}, signed: {}, marketplace: {})",
            self.total, self.capacity, self.enabled, self.signed, self.from_marketplace
        )
    }
}

/// Check if version `a` is newer than version `b`.
fn is_newer(a: &SemVer, b: &SemVer) -> bool {
    if a.major != b.major {
        return a.major > b.major;
    }
    if a.minor != b.minor {
        return a.minor > b.minor;
    }
    a.patch > b.patch
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::PluginManifest;
    use crate::packaging::PluginPackage;
    use crate::permissions::PermissionSet;
    use crate::signing::PluginKeyPair;

    fn test_manifest(name: &str) -> PluginManifest {
        PluginManifest::new(name)
            .with_version(1, 0, 0)
            .with_author("Test Author")
            .with_entry_point("main.js")
            .with_permissions(PermissionSet::read_only())
    }

    fn test_package(name: &str) -> PluginPackage {
        let manifest = test_manifest(name);
        let code = format!("console.log('Hello from {name}');");
        PluginPackage::create(&manifest, code.as_bytes()).unwrap()
    }

    fn signed_package(name: &str) -> PluginPackage {
        let mut pkg = test_package(name);
        let kp = PluginKeyPair::generate();
        pkg.sign(&kp);
        pkg
    }

    // ─── Basic Operations ───

    #[test]
    fn test_registry_new() {
        let reg = PluginRegistry::new();
        assert_eq!(reg.count(), 0);
        assert_eq!(reg.enabled_count(), 0);
    }

    #[test]
    fn test_install_plugin() {
        let mut reg = PluginRegistry::new();
        let pkg = test_package("Test Plugin");
        assert!(reg.install(&pkg, RegistrySource::Local).is_ok());
        assert_eq!(reg.count(), 1);
    }

    #[test]
    fn test_install_duplicate() {
        let mut reg = PluginRegistry::new();
        let pkg = test_package("Test Plugin");
        reg.install(&pkg, RegistrySource::Local).unwrap();
        assert!(matches!(
            reg.install(&pkg, RegistrySource::Local),
            Err(RegistryError::AlreadyInstalled(_))
        ));
    }

    #[test]
    fn test_install_capacity() {
        let mut reg = PluginRegistry::with_policy(1, false);
        let pkg1 = test_package("Plugin 1");
        let pkg2 = test_package("Plugin 2");
        reg.install(&pkg1, RegistrySource::Local).unwrap();
        assert!(matches!(
            reg.install(&pkg2, RegistrySource::Local),
            Err(RegistryError::CapacityReached(1))
        ));
    }

    #[test]
    fn test_uninstall_plugin() {
        let mut reg = PluginRegistry::new();
        let pkg = test_package("Test Plugin");
        let id = pkg.manifest.id.to_string();
        reg.install(&pkg, RegistrySource::Local).unwrap();
        assert!(reg.uninstall(&id).is_ok());
        assert_eq!(reg.count(), 0);
    }

    #[test]
    fn test_uninstall_nonexistent() {
        let mut reg = PluginRegistry::new();
        assert!(matches!(
            reg.uninstall("nonexistent"),
            Err(RegistryError::NotFound(_))
        ));
    }

    #[test]
    fn test_uninstall_builtin_protected() {
        let mut reg = PluginRegistry::new();
        let pkg = test_package("Core Plugin");
        let id = pkg.manifest.id.to_string();
        reg.install(&pkg, RegistrySource::BuiltIn).unwrap();
        assert!(matches!(
            reg.uninstall(&id),
            Err(RegistryError::BuiltInProtected(_))
        ));
    }

    // ─── Enable / Disable ───

    #[test]
    fn test_enable_disable() {
        let mut reg = PluginRegistry::new();
        let pkg = test_package("Test Plugin");
        let id = pkg.manifest.id.to_string();
        reg.install(&pkg, RegistrySource::Local).unwrap();

        // Installed plugins are enabled by default
        assert!(reg.get(&id).unwrap().enabled);

        reg.disable(&id).unwrap();
        assert!(!reg.get(&id).unwrap().enabled);

        reg.enable(&id).unwrap();
        assert!(reg.get(&id).unwrap().enabled);
    }

    #[test]
    fn test_enable_nonexistent() {
        let mut reg = PluginRegistry::new();
        assert!(matches!(
            reg.enable("nope"),
            Err(RegistryError::NotFound(_))
        ));
    }

    // ─── Upgrade ───

    #[test]
    fn test_upgrade_plugin() {
        let mut reg = PluginRegistry::new();
        let manifest_v1 = test_manifest("Upgrader").with_version(1, 0, 0);
        let pkg_v1 =
            PluginPackage::create(&manifest_v1, b"v1 code").unwrap();
        let id = manifest_v1.id.to_string();

        reg.install(&pkg_v1, RegistrySource::Local).unwrap();
        assert_eq!(reg.get(&id).unwrap().version().to_string(), "1.0.0");

        // Create v2 with same UUID
        let mut manifest_v2 = manifest_v1.clone();
        manifest_v2.version = SemVer::new(2, 0, 0);
        let pkg_v2 =
            PluginPackage::create(&manifest_v2, b"v2 code").unwrap();

        reg.upgrade(&pkg_v2, RegistrySource::Local).unwrap();
        assert_eq!(reg.get(&id).unwrap().version().to_string(), "2.0.0");
    }

    #[test]
    fn test_upgrade_same_version_fails() {
        let mut reg = PluginRegistry::new();
        let manifest = test_manifest("Plugin");
        let pkg = PluginPackage::create(&manifest, b"code").unwrap();
        let _id = manifest.id.to_string();

        reg.install(&pkg, RegistrySource::Local).unwrap();

        let same_pkg = PluginPackage::create(&manifest, b"same code").unwrap();
        assert!(matches!(
            reg.upgrade(&same_pkg, RegistrySource::Local),
            Err(RegistryError::VersionConflict { .. })
        ));
    }

    #[test]
    fn test_upgrade_preserves_enabled_state() {
        let mut reg = PluginRegistry::new();
        let manifest_v1 = test_manifest("Plugin").with_version(1, 0, 0);
        let pkg_v1 = PluginPackage::create(&manifest_v1, b"v1").unwrap();
        let id = manifest_v1.id.to_string();

        reg.install(&pkg_v1, RegistrySource::Local).unwrap();
        reg.disable(&id).unwrap();

        let mut manifest_v2 = manifest_v1.clone();
        manifest_v2.version = SemVer::new(2, 0, 0);
        let pkg_v2 = PluginPackage::create(&manifest_v2, b"v2").unwrap();

        reg.upgrade(&pkg_v2, RegistrySource::Local).unwrap();
        assert!(!reg.get(&id).unwrap().enabled); // Stayed disabled
    }

    #[test]
    fn test_upgrade_nonexistent() {
        let mut reg = PluginRegistry::new();
        let pkg = test_package("New Plugin");
        assert!(matches!(
            reg.upgrade(&pkg, RegistrySource::Local),
            Err(RegistryError::NotFound(_))
        ));
    }

    // ─── Lookup & Search ───

    #[test]
    fn test_get_plugin() {
        let mut reg = PluginRegistry::new();
        let pkg = test_package("Lookup Test");
        let id = pkg.manifest.id.to_string();
        reg.install(&pkg, RegistrySource::Local).unwrap();

        let found = reg.get(&id).unwrap();
        assert_eq!(found.name(), "Lookup Test");
    }

    #[test]
    fn test_is_installed() {
        let mut reg = PluginRegistry::new();
        let pkg = test_package("Check Test");
        let id = pkg.manifest.id.to_string();
        assert!(!reg.is_installed(&id));
        reg.install(&pkg, RegistrySource::Local).unwrap();
        assert!(reg.is_installed(&id));
    }

    #[test]
    fn test_find_by_name() {
        let mut reg = PluginRegistry::new();
        let pkg = test_package("Named Plugin");
        reg.install(&pkg, RegistrySource::Local).unwrap();

        assert!(reg.find_by_name("Named Plugin").is_some());
        assert!(reg.find_by_name("Nonexistent").is_none());
    }

    #[test]
    fn test_list_all() {
        let mut reg = PluginRegistry::new();
        reg.install(&test_package("P1"), RegistrySource::Local).unwrap();
        reg.install(&test_package("P2"), RegistrySource::Local).unwrap();
        assert_eq!(reg.list_all().len(), 2);
    }

    #[test]
    fn test_list_ids() {
        let mut reg = PluginRegistry::new();
        let pkg = test_package("Listed");
        let id = pkg.manifest.id.to_string();
        reg.install(&pkg, RegistrySource::Local).unwrap();
        let ids = reg.list_ids();
        assert!(ids.contains(&id.as_str()));
    }

    #[test]
    fn test_enabled_disabled_plugins() {
        let mut reg = PluginRegistry::new();
        let p1 = test_package("Enabled");
        let p2 = test_package("Disabled");
        let id2 = p2.manifest.id.to_string();

        reg.install(&p1, RegistrySource::Local).unwrap();
        reg.install(&p2, RegistrySource::Local).unwrap();
        reg.disable(&id2).unwrap();

        assert_eq!(reg.enabled_plugins().len(), 1);
        assert_eq!(reg.disabled_plugins().len(), 1);
        assert_eq!(reg.enabled_count(), 1);
    }

    // ─── Search / Filter ───

    #[test]
    fn test_search_by_name() {
        let mut reg = PluginRegistry::new();
        reg.install(&test_package("Auto Grid"), RegistrySource::Local).unwrap();
        reg.install(&test_package("Color Gen"), RegistrySource::Local).unwrap();

        let results = reg.search(&PluginFilter::all().with_name("grid"));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name(), "Auto Grid");
    }

    #[test]
    fn test_search_by_enabled() {
        let mut reg = PluginRegistry::new();
        let p1 = test_package("Active");
        let p2 = test_package("Inactive");
        let id2 = p2.manifest.id.to_string();

        reg.install(&p1, RegistrySource::Local).unwrap();
        reg.install(&p2, RegistrySource::Local).unwrap();
        reg.disable(&id2).unwrap();

        let enabled = reg.search(&PluginFilter::all().with_enabled(true));
        assert_eq!(enabled.len(), 1);
    }

    #[test]
    fn test_search_by_source() {
        let mut reg = PluginRegistry::new();
        reg.install(&test_package("Local One"), RegistrySource::Local)
            .unwrap();
        reg.install(&test_package("Market One"), RegistrySource::Marketplace)
            .unwrap();

        let marketplace = reg.search(
            &PluginFilter::all().with_source(RegistrySource::Marketplace),
        );
        assert_eq!(marketplace.len(), 1);
        assert_eq!(marketplace[0].name(), "Market One");
    }

    #[test]
    fn test_search_by_signed() {
        let mut reg = PluginRegistry::new();
        reg.install(&test_package("Unsigned"), RegistrySource::Local)
            .unwrap();
        reg.install(&signed_package("Signed"), RegistrySource::Local)
            .unwrap();

        let signed = reg.search(&PluginFilter::all().with_signed(true));
        assert_eq!(signed.len(), 1);
        assert_eq!(signed[0].name(), "Signed");
    }

    // ─── Signature Policy ───

    #[test]
    fn test_require_signatures() {
        let mut reg = PluginRegistry::with_policy(256, true);
        let unsigned = test_package("Unsigned");
        assert!(matches!(
            reg.install(&unsigned, RegistrySource::Local),
            Err(RegistryError::VerificationFailed(_))
        ));

        let signed = signed_package("Signed");
        assert!(reg.install(&signed, RegistrySource::Local).is_ok());
    }

    // ─── Statistics ───

    #[test]
    fn test_registry_stats() {
        let mut reg = PluginRegistry::new();
        reg.install(&test_package("P1"), RegistrySource::Local).unwrap();
        reg.install(&signed_package("P2"), RegistrySource::Marketplace)
            .unwrap();
        let p3 = test_package("P3");
        let id3 = p3.manifest.id.to_string();
        reg.install(&p3, RegistrySource::Local).unwrap();
        reg.disable(&id3).unwrap();

        let stats = reg.stats();
        assert_eq!(stats.total, 3);
        assert_eq!(stats.enabled, 2);
        assert_eq!(stats.disabled, 1);
        assert_eq!(stats.signed, 1);
        assert_eq!(stats.unsigned, 2);
        assert_eq!(stats.from_marketplace, 1);
    }

    #[test]
    fn test_registry_stats_display() {
        let reg = PluginRegistry::new();
        let display = reg.stats().to_string();
        assert!(display.contains("Plugins:"));
    }

    // ─── Serialization ───

    #[test]
    fn test_registry_to_json() {
        let mut reg = PluginRegistry::new();
        reg.install(&test_package("JSON Test"), RegistrySource::Local)
            .unwrap();
        let json = reg.to_json().unwrap();
        assert!(json.contains("JSON Test"));
    }

    // ─── Source Display ───

    #[test]
    fn test_registry_source_display() {
        assert_eq!(RegistrySource::Local.to_string(), "local");
        assert_eq!(RegistrySource::Marketplace.to_string(), "marketplace");
        assert_eq!(
            RegistrySource::Url("https://example.com".into()).to_string(),
            "url:https://example.com"
        );
        assert_eq!(RegistrySource::BuiltIn.to_string(), "built-in");
        assert_eq!(RegistrySource::Development.to_string(), "development");
    }

    // ─── Error Display ───

    #[test]
    fn test_registry_error_display() {
        assert!(RegistryError::NotFound("x".into())
            .to_string()
            .contains("not found"));
        assert!(RegistryError::AlreadyInstalled("y".into())
            .to_string()
            .contains("already installed"));
        assert!(RegistryError::BuiltInProtected("z".into())
            .to_string()
            .contains("built-in"));
        assert!(RegistryError::CapacityReached(10)
            .to_string()
            .contains("10"));
    }

    // ─── Version Helper ───

    #[test]
    fn test_is_newer() {
        assert!(is_newer(&SemVer::new(2, 0, 0), &SemVer::new(1, 0, 0)));
        assert!(is_newer(&SemVer::new(1, 1, 0), &SemVer::new(1, 0, 0)));
        assert!(is_newer(&SemVer::new(1, 0, 1), &SemVer::new(1, 0, 0)));
        assert!(!is_newer(&SemVer::new(1, 0, 0), &SemVer::new(1, 0, 0)));
        assert!(!is_newer(&SemVer::new(0, 9, 0), &SemVer::new(1, 0, 0)));
    }

    // ─── Clear ───

    #[test]
    fn test_clear() {
        let mut reg = PluginRegistry::new();
        reg.install(&test_package("P1"), RegistrySource::Local).unwrap();
        reg.install(&test_package("P2"), RegistrySource::Local).unwrap();
        reg.clear();
        assert_eq!(reg.count(), 0);
    }

    // ─── InstalledPlugin ───

    #[test]
    fn test_installed_plugin_metadata() {
        let mut reg = PluginRegistry::new();
        let pkg = test_package("Metadata Test");
        let id = pkg.manifest.id.to_string();
        reg.install(&pkg, RegistrySource::Marketplace).unwrap();

        let entry = reg.get(&id).unwrap();
        assert_eq!(entry.name(), "Metadata Test");
        assert_eq!(entry.version().to_string(), "1.0.0");
        assert_eq!(entry.source, RegistrySource::Marketplace);
        assert!(entry.installed_at > 0);
        assert!(!entry.is_signed);
        assert!(entry.signer_key.is_none());
    }

    #[test]
    fn test_installed_plugin_signed_metadata() {
        let mut reg = PluginRegistry::new();
        let pkg = signed_package("Signed Plugin");
        let id = pkg.manifest.id.to_string();
        reg.install(&pkg, RegistrySource::Local).unwrap();

        let entry = reg.get(&id).unwrap();
        assert!(entry.is_signed);
        assert!(entry.signer_key.is_some());
        assert_eq!(entry.signer_key.as_ref().unwrap().len(), 64);
    }
}
