//! Permission system for plugin sandboxing.
//!
//! Plugins must declare required permissions in their manifest.
//! The host checks permissions before executing gated operations.
//!
//! Architecture:
//! ```text
//! Plugin requests "network" ─► PermissionGuard.check()
//!                                    │
//!                              ┌─────┴─────┐
//!                              │ Granted?   │
//!                              ├─ Yes ─────► Execute operation
//!                              └─ No ──────► Err(PermissionDenied)
//! ```
//!
//! ## Security Model (OWASP-aligned)
//!
//! 1. **Principle of Least Privilege** — Plugins get minimum permissions
//! 2. **Explicit Grant** — User must approve each permission
//! 3. **Domain Scoping** — Network access limited to declared domains
//! 4. **Path Scoping** — File access limited to declared paths
//! 5. **Revocability** — Permissions can be revoked at any time
//!
//! Reference: OWASP Testing Guide v4 — Authorization Testing
//! Reference: Secure Programming Cookbook — Capability-Based Security

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Individual permission kinds.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum PermissionKind {
    /// Read document structure (layers, properties)
    DocumentRead,
    /// Modify document (create/delete/update layers)
    DocumentWrite,
    /// Access network (HTTP requests)
    Network,
    /// Read files from filesystem
    FileRead,
    /// Write files to filesystem
    FileWrite,
    /// Access system clipboard
    Clipboard,
    /// Show UI notifications
    Notifications,
    /// Access user preferences/settings
    UserPreferences,
    /// Run in background (persist between commands)
    Background,
}

impl PermissionKind {
    /// Returns a single-bit mask for this permission kind.
    /// Used by the bitflag cache in `PermissionSet` for O(1) lookups.
    #[inline(always)]
    pub const fn bit(&self) -> u16 {
        1u16 << (*self as u16)
    }
}

impl std::fmt::Display for PermissionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DocumentRead => write!(f, "document:read"),
            Self::DocumentWrite => write!(f, "document:write"),
            Self::Network => write!(f, "network"),
            Self::FileRead => write!(f, "file:read"),
            Self::FileWrite => write!(f, "file:write"),
            Self::Clipboard => write!(f, "clipboard"),
            Self::Notifications => write!(f, "notifications"),
            Self::UserPreferences => write!(f, "user:preferences"),
            Self::Background => write!(f, "background"),
        }
    }
}

/// A set of permissions with optional domain/path scoping.
///
/// Example manifest:
/// ```json
/// {
///   "permissions": {
///     "granted": ["document:read", "document:write", "network"],
///     "allowed_domains": ["api.logos.dev", "cdn.logos.dev"],
///     "allowed_paths": ["/tmp/logos-plugins/"]
///   }
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionSet {
    /// Granted permission kinds
    pub granted: HashSet<PermissionKind>,
    /// Allowed network domains (empty = all, if Network is granted)
    pub allowed_domains: Vec<String>,
    /// Allowed filesystem paths (empty = none, even if FileRead/Write granted)
    pub allowed_paths: Vec<String>,
    /// Bitflag cache — mirrors `granted` for O(1) `has()` checks.
    /// Each bit corresponds to a `PermissionKind` variant.
    #[serde(skip)]
    flags: u16,
}

impl PermissionSet {
    /// No permissions (maximum restriction).
    pub fn none() -> Self {
        Self {
            granted: HashSet::new(),
            allowed_domains: Vec::new(),
            allowed_paths: Vec::new(),
            flags: 0,
        }
    }

    /// Read-only document access.
    pub fn read_only() -> Self {
        let mut granted = HashSet::new();
        granted.insert(PermissionKind::DocumentRead);
        Self {
            flags: PermissionKind::DocumentRead.bit(),
            granted,
            allowed_domains: Vec::new(),
            allowed_paths: Vec::new(),
        }
    }

    /// Full document access (read + write).
    pub fn document_full() -> Self {
        let mut granted = HashSet::new();
        granted.insert(PermissionKind::DocumentRead);
        granted.insert(PermissionKind::DocumentWrite);
        Self {
            flags: PermissionKind::DocumentRead.bit() | PermissionKind::DocumentWrite.bit(),
            granted,
            allowed_domains: Vec::new(),
            allowed_paths: Vec::new(),
        }
    }

    /// Check if a permission is granted.
    /// O(1) via bitflag — single AND instruction, ~1ns.
    #[inline(always)]
    pub fn has(&self, perm: &PermissionKind) -> bool {
        self.flags & perm.bit() != 0
    }

    /// Grant a permission.
    pub fn grant(&mut self, perm: PermissionKind) {
        self.flags |= perm.bit();
        self.granted.insert(perm);
    }

    /// Revoke a permission.
    pub fn revoke(&mut self, perm: &PermissionKind) {
        self.flags &= !perm.bit();
        self.granted.remove(perm);
    }

    /// Add an allowed domain (for network access).
    pub fn allow_domain(&mut self, domain: impl Into<String>) {
        self.allowed_domains.push(domain.into());
    }

    /// Add an allowed filesystem path.
    pub fn allow_path(&mut self, path: impl Into<String>) {
        self.allowed_paths.push(path.into());
    }

    /// Check if a domain is allowed.
    /// Avoids heap allocation — uses manual suffix matching instead of format!.
    pub fn is_domain_allowed(&self, domain: &str) -> bool {
        if !self.has(&PermissionKind::Network) {
            return false;
        }
        // Empty allowed_domains = all domains allowed
        if self.allowed_domains.is_empty() {
            return true;
        }
        self.allowed_domains.iter().any(|d| {
            if domain == d.as_str() {
                return true;
            }
            // Check subdomain: domain must end with the allowed domain
            // AND have a '.' separator immediately before it
            if let Some(prefix) = domain.strip_suffix(d.as_str()) {
                prefix.ends_with('.')
            } else {
                false
            }
        })
    }

    /// Check if a file path is allowed.
    pub fn is_path_allowed(&self, path: &str) -> bool {
        if !self.has(&PermissionKind::FileRead) && !self.has(&PermissionKind::FileWrite) {
            return false;
        }
        // Must have at least one allowed path
        self.allowed_paths.iter().any(|p| path.starts_with(p.as_str()))
    }

    /// Count of granted permissions.
    pub fn count(&self) -> usize {
        self.granted.len()
    }

    /// List all granted permissions.
    pub fn list(&self) -> Vec<&PermissionKind> {
        self.granted.iter().collect()
    }
}

impl Default for PermissionSet {
    fn default() -> Self {
        Self::none()
    }
}

/// Rebuild the bitflag cache after deserialization.
impl PermissionSet {
    /// Recompute cached flags from the HashSet (call after deserialization).
    pub fn rebuild_flags(&mut self) {
        self.flags = self.granted.iter().fold(0u16, |acc, p| acc | p.bit());
    }
}

/// Permission guard that enforces checks at runtime.
///
/// The guard wraps a `PermissionSet` and provides check methods
/// that return `Result` for clean error handling.
///
/// Performance: ~1ns per check (bitflag AND operation).
///
/// Reference: OWASP — Principle of Least Privilege
pub struct PermissionGuard {
    permissions: PermissionSet,
    /// Log of denied requests (for auditing)
    denied_log: Vec<PermissionDenial>,
}

/// Record of a denied permission request.
#[derive(Debug, Clone)]
pub struct PermissionDenial {
    /// What was requested
    pub permission: PermissionKind,
    /// Additional context (e.g., domain, path)
    pub context: String,
    /// When it was denied
    pub timestamp: std::time::Instant,
}

impl PermissionGuard {
    /// Create a guard with the given permissions.
    pub fn new(permissions: PermissionSet) -> Self {
        Self {
            permissions,
            denied_log: Vec::new(),
        }
    }

    /// Check a permission. Returns Ok(()) if granted.
    #[inline]
    pub fn check(&mut self, perm: &PermissionKind) -> Result<(), String> {
        if self.permissions.has(perm) {
            Ok(())
        } else {
            self.denied_log.push(PermissionDenial {
                permission: *perm,
                context: String::new(),
                timestamp: std::time::Instant::now(),
            });
            Err(format!("permission denied: {perm}"))
        }
    }

    /// Check network access for a specific domain.
    pub fn check_network(&mut self, domain: &str) -> Result<(), String> {
        if self.permissions.is_domain_allowed(domain) {
            Ok(())
        } else {
            self.denied_log.push(PermissionDenial {
                permission: PermissionKind::Network,
                context: domain.to_string(),
                timestamp: std::time::Instant::now(),
            });
            Err(format!("network access denied for domain: {domain}"))
        }
    }

    /// Check file access for a specific path.
    pub fn check_file(&mut self, path: &str, write: bool) -> Result<(), String> {
        let perm = if write {
            PermissionKind::FileWrite
        } else {
            PermissionKind::FileRead
        };
        if !self.permissions.has(&perm) {
            self.denied_log.push(PermissionDenial {
                permission: perm,
                context: path.to_string(),
                timestamp: std::time::Instant::now(),
            });
            return Err(format!("file access denied: {path}"));
        }
        if !self.permissions.is_path_allowed(path) {
            self.denied_log.push(PermissionDenial {
                permission: perm,
                context: path.to_string(),
                timestamp: std::time::Instant::now(),
            });
            return Err(format!("path not in allowed list: {path}"));
        }
        Ok(())
    }

    /// Get the denial log (for security auditing).
    pub fn denied_log(&self) -> &[PermissionDenial] {
        &self.denied_log
    }

    /// Clear the denial log.
    pub fn clear_denied_log(&mut self) {
        self.denied_log.clear();
    }

    /// Grant a permission at runtime (requires user approval flow).
    pub fn runtime_grant(&mut self, perm: PermissionKind) {
        self.permissions.grant(perm);
    }

    /// Revoke a permission at runtime.
    pub fn runtime_revoke(&mut self, perm: &PermissionKind) {
        self.permissions.revoke(perm);
    }

    /// Get the underlying permission set.
    pub fn permissions(&self) -> &PermissionSet {
        &self.permissions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_kind_display() {
        assert_eq!(PermissionKind::DocumentRead.to_string(), "document:read");
        assert_eq!(PermissionKind::Network.to_string(), "network");
        assert_eq!(PermissionKind::Clipboard.to_string(), "clipboard");
    }

    #[test]
    fn test_permission_set_none() {
        let perms = PermissionSet::none();
        assert!(!perms.has(&PermissionKind::DocumentRead));
        assert!(!perms.has(&PermissionKind::Network));
        assert_eq!(perms.count(), 0);
    }

    #[test]
    fn test_permission_set_read_only() {
        let perms = PermissionSet::read_only();
        assert!(perms.has(&PermissionKind::DocumentRead));
        assert!(!perms.has(&PermissionKind::DocumentWrite));
        assert_eq!(perms.count(), 1);
    }

    #[test]
    fn test_permission_set_document_full() {
        let perms = PermissionSet::document_full();
        assert!(perms.has(&PermissionKind::DocumentRead));
        assert!(perms.has(&PermissionKind::DocumentWrite));
        assert_eq!(perms.count(), 2);
    }

    #[test]
    fn test_grant_revoke() {
        let mut perms = PermissionSet::none();
        perms.grant(PermissionKind::Clipboard);
        assert!(perms.has(&PermissionKind::Clipboard));
        perms.revoke(&PermissionKind::Clipboard);
        assert!(!perms.has(&PermissionKind::Clipboard));
    }

    #[test]
    fn test_domain_allowed_no_network() {
        let perms = PermissionSet::none();
        assert!(!perms.is_domain_allowed("example.com"));
    }

    #[test]
    fn test_domain_allowed_all() {
        let mut perms = PermissionSet::none();
        perms.grant(PermissionKind::Network);
        // Empty allowed_domains = all allowed
        assert!(perms.is_domain_allowed("example.com"));
    }

    #[test]
    fn test_domain_allowed_scoped() {
        let mut perms = PermissionSet::none();
        perms.grant(PermissionKind::Network);
        perms.allow_domain("api.logos.dev");
        assert!(perms.is_domain_allowed("api.logos.dev"));
        assert!(!perms.is_domain_allowed("evil.com"));
    }

    #[test]
    fn test_domain_subdomain_match() {
        let mut perms = PermissionSet::none();
        perms.grant(PermissionKind::Network);
        perms.allow_domain("logos.dev");
        assert!(perms.is_domain_allowed("logos.dev"));
        assert!(perms.is_domain_allowed("api.logos.dev"));
        assert!(!perms.is_domain_allowed("notlogos.dev"));
    }

    #[test]
    fn test_path_allowed() {
        let mut perms = PermissionSet::none();
        perms.grant(PermissionKind::FileRead);
        perms.allow_path("/tmp/logos/");
        assert!(perms.is_path_allowed("/tmp/logos/file.txt"));
        assert!(!perms.is_path_allowed("/etc/passwd"));
    }

    #[test]
    fn test_path_no_permission() {
        let perms = PermissionSet::none();
        assert!(!perms.is_path_allowed("/tmp/test"));
    }

    #[test]
    fn test_guard_check_granted() {
        let mut guard = PermissionGuard::new(PermissionSet::read_only());
        assert!(guard.check(&PermissionKind::DocumentRead).is_ok());
    }

    #[test]
    fn test_guard_check_denied() {
        let mut guard = PermissionGuard::new(PermissionSet::none());
        assert!(guard.check(&PermissionKind::DocumentRead).is_err());
        assert_eq!(guard.denied_log().len(), 1);
    }

    #[test]
    fn test_guard_network_check() {
        let mut perms = PermissionSet::none();
        perms.grant(PermissionKind::Network);
        perms.allow_domain("api.logos.dev");

        let mut guard = PermissionGuard::new(perms);
        assert!(guard.check_network("api.logos.dev").is_ok());
        assert!(guard.check_network("evil.com").is_err());
    }

    #[test]
    fn test_guard_file_check() {
        let mut perms = PermissionSet::none();
        perms.grant(PermissionKind::FileRead);
        perms.allow_path("/tmp/logos/");

        let mut guard = PermissionGuard::new(perms);
        assert!(guard.check_file("/tmp/logos/file.txt", false).is_ok());
        assert!(guard.check_file("/etc/passwd", false).is_err());
        assert!(guard.check_file("/tmp/logos/file.txt", true).is_err()); // No write perm
    }

    #[test]
    fn test_guard_runtime_grant() {
        let mut guard = PermissionGuard::new(PermissionSet::none());
        assert!(guard.check(&PermissionKind::Clipboard).is_err());
        guard.runtime_grant(PermissionKind::Clipboard);
        assert!(guard.check(&PermissionKind::Clipboard).is_ok());
    }

    #[test]
    fn test_guard_runtime_revoke() {
        let mut guard = PermissionGuard::new(PermissionSet::document_full());
        assert!(guard.check(&PermissionKind::DocumentWrite).is_ok());
        guard.runtime_revoke(&PermissionKind::DocumentWrite);
        assert!(guard.check(&PermissionKind::DocumentWrite).is_err());
    }

    #[test]
    fn test_guard_denial_log() {
        let mut guard = PermissionGuard::new(PermissionSet::none());
        let _ = guard.check(&PermissionKind::Network);
        let _ = guard.check_network("evil.com");
        assert_eq!(guard.denied_log().len(), 2);
        guard.clear_denied_log();
        assert_eq!(guard.denied_log().len(), 0);
    }

    #[test]
    fn test_permission_set_serialization() {
        let mut perms = PermissionSet::document_full();
        perms.allow_domain("api.logos.dev");
        let json = serde_json::to_string(&perms).unwrap();
        let mut parsed: PermissionSet = serde_json::from_str(&json).unwrap();
        parsed.rebuild_flags(); // Rebuild bitflag cache after deserialization
        assert!(parsed.has(&PermissionKind::DocumentRead));
        assert_eq!(parsed.allowed_domains.len(), 1);
    }
}
