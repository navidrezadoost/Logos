//! Permission Prompt Flow — Interactive permission review for plugin install.
//!
//! When installing a plugin, the user must review and approve the
//! permissions the plugin requests. This module provides the types
//! and flow for presenting permissions to the user and recording
//! their decisions.
//!
//! ## Architecture
//!
//! ```text
//! PluginManifest
//!   ├── required permissions
//!   └── optional permissions
//!         │
//!         ▼
//! PermissionPrompt ── build_prompt() ──► PermissionPromptSession
//!                                              │
//!                                    ┌─────────┼─────────┐
//!                                    ▼         ▼         ▼
//!                                 Grant     Deny    GrantOnce
//!                                    │         │         │
//!                                    └─────────┼─────────┘
//!                                              ▼
//!                                    InstallApproval (accept / reject)
//! ```
//!
//! ## Performance Targets
//!
//! | Operation            | Target  | Note                      |
//! |----------------------|---------|---------------------------|
//! | Build prompt         | <100μs  | No I/O, struct assembly   |
//! | Record decision      | <1μs    | Single enum write         |
//! | Check approval       | <10μs   | Iterate small vec         |
//!
//! ## References
//!
//! - Chrome Extension Permissions — "Declare permissions"
//! - Android Permission Model — Runtime permissions
//! - OWASP — Principle of Least Privilege

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::manifest::{PluginManifest, SemVer};
use crate::permissions::{PermissionKind, PermissionSet};

// ═══════════════════════════════════════════════════════════════
// Permission Decision
// ═══════════════════════════════════════════════════════════════

/// User's decision about a single permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermissionDecision {
    /// Permission granted permanently for this plugin.
    Granted,
    /// Permission denied permanently (don't ask again).
    DeniedAlways,
    /// Permission granted for this session only.
    GrantedOnce,
    /// Permission denied for this session only.
    DeniedOnce,
    /// User has not yet made a decision.
    Pending,
}

impl PermissionDecision {
    /// Whether this decision allows the permission.
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Granted | Self::GrantedOnce)
    }

    /// Whether this decision is final (no further prompts needed).
    pub fn is_permanent(&self) -> bool {
        matches!(self, Self::Granted | Self::DeniedAlways)
    }

    /// Whether the user has made any decision.
    pub fn is_decided(&self) -> bool {
        !matches!(self, Self::Pending)
    }
}

impl std::fmt::Display for PermissionDecision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Granted => write!(f, "granted"),
            Self::DeniedAlways => write!(f, "denied_always"),
            Self::GrantedOnce => write!(f, "granted_once"),
            Self::DeniedOnce => write!(f, "denied_once"),
            Self::Pending => write!(f, "pending"),
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// Risk Level
// ═══════════════════════════════════════════════════════════════

/// Risk level classification for a permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RiskLevel {
    /// Safe — read-only, no external access.
    Low,
    /// Moderate — writes data or shows UI.
    Medium,
    /// High — network or filesystem access.
    High,
    /// Critical — full document write + network together.
    Critical,
}

impl RiskLevel {
    /// Classify a single permission's risk.
    pub fn for_permission(perm: &PermissionKind) -> Self {
        match perm {
            PermissionKind::DocumentRead => RiskLevel::Low,
            PermissionKind::Notifications => RiskLevel::Low,
            PermissionKind::UserPreferences => RiskLevel::Low,
            PermissionKind::Clipboard => RiskLevel::Medium,
            PermissionKind::DocumentWrite => RiskLevel::Medium,
            PermissionKind::Background => RiskLevel::Medium,
            PermissionKind::Network => RiskLevel::High,
            PermissionKind::FileRead => RiskLevel::High,
            PermissionKind::FileWrite => RiskLevel::High,
        }
    }

    /// Human-readable risk description.
    pub fn description(&self) -> &'static str {
        match self {
            Self::Low => "This permission is safe and has minimal risk.",
            Self::Medium => "This permission can modify your work. Review carefully.",
            Self::High => "This permission accesses external resources. Ensure you trust the publisher.",
            Self::Critical => "This combination of permissions requires high trust in the publisher.",
        }
    }
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Low => write!(f, "low"),
            Self::Medium => write!(f, "medium"),
            Self::High => write!(f, "high"),
            Self::Critical => write!(f, "critical"),
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// Permission Prompt Item
// ═══════════════════════════════════════════════════════════════

/// A single permission being prompted to the user.
#[derive(Debug, Clone)]
pub struct PermissionPromptItem {
    /// The permission kind being requested.
    pub permission: PermissionKind,
    /// Human-readable description of why the plugin needs this.
    pub reason: String,
    /// Whether this permission is required (vs. optional).
    pub required: bool,
    /// Risk level classification.
    pub risk: RiskLevel,
    /// User's decision (starts as Pending).
    pub decision: PermissionDecision,
}

impl PermissionPromptItem {
    /// Create a new prompt item.
    pub fn new(permission: PermissionKind, required: bool) -> Self {
        Self {
            risk: RiskLevel::for_permission(&permission),
            reason: default_reason(&permission),
            permission,
            required,
            decision: PermissionDecision::Pending,
        }
    }

    /// Override the reason string.
    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = reason.into();
        self
    }
}

/// Default human-readable reason for a permission kind.
fn default_reason(perm: &PermissionKind) -> String {
    match perm {
        PermissionKind::DocumentRead => "Read layers, shapes and properties from the document.".into(),
        PermissionKind::DocumentWrite => "Create, modify, or delete layers and shapes.".into(),
        PermissionKind::Network => "Make HTTP requests to external services.".into(),
        PermissionKind::FileRead => "Read files from the filesystem.".into(),
        PermissionKind::FileWrite => "Write files to the filesystem.".into(),
        PermissionKind::Clipboard => "Access the system clipboard (copy/paste).".into(),
        PermissionKind::Notifications => "Show toast notifications to the user.".into(),
        PermissionKind::UserPreferences => "Read or write user preference settings.".into(),
        PermissionKind::Background => "Continue running after the command completes.".into(),
    }
}

// ═══════════════════════════════════════════════════════════════
// Permission Prompt Session
// ═══════════════════════════════════════════════════════════════

/// An interactive permission review session for a plugin install.
///
/// The UI presents each `PermissionPromptItem` and records the
/// user's decision. Once all required permissions are decided,
/// the session can produce an `InstallApproval`.
#[derive(Debug, Clone)]
pub struct PermissionPromptSession {
    /// Plugin ID being installed.
    pub plugin_id: Uuid,
    /// Plugin name (for display).
    pub plugin_name: String,
    /// Plugin version being installed.
    pub plugin_version: SemVer,
    /// Publisher name or key (for display).
    pub publisher: String,
    /// Individual permission items to review.
    pub items: Vec<PermissionPromptItem>,
    /// Allowed network domains (from manifest).
    pub allowed_domains: Vec<String>,
    /// Allowed file paths (from manifest).
    pub allowed_paths: Vec<String>,
    /// Overall risk level (highest among requested permissions).
    pub overall_risk: RiskLevel,
}

impl PermissionPromptSession {
    /// Build a prompt session from a plugin manifest.
    pub fn from_manifest(manifest: &PluginManifest) -> Self {
        let mut items: Vec<PermissionPromptItem> = manifest
            .permissions
            .granted
            .iter()
            .map(|p| PermissionPromptItem::new(*p, true))
            .collect();

        // Sort by risk (highest first) for user attention
        items.sort_by(|a, b| b.risk.cmp(&a.risk));

        let overall_risk = compute_overall_risk(&items);

        Self {
            plugin_id: manifest.id,
            plugin_name: manifest.name.clone(),
            plugin_version: manifest.version.clone(),
            publisher: manifest.author.clone(),
            items,
            allowed_domains: manifest.permissions.allowed_domains.clone(),
            allowed_paths: manifest.permissions.allowed_paths.clone(),
            overall_risk,
        }
    }

    /// Record a decision for a specific permission.
    pub fn decide(&mut self, permission: &PermissionKind, decision: PermissionDecision) -> bool {
        if let Some(item) = self.items.iter_mut().find(|i| &i.permission == permission) {
            item.decision = decision;
            true
        } else {
            false
        }
    }

    /// Grant all permissions at once.
    pub fn grant_all(&mut self) {
        for item in &mut self.items {
            item.decision = PermissionDecision::Granted;
        }
    }

    /// Deny all permissions at once.
    pub fn deny_all(&mut self) {
        for item in &mut self.items {
            item.decision = PermissionDecision::DeniedAlways;
        }
    }

    /// Check if all required permissions have been decided.
    pub fn all_required_decided(&self) -> bool {
        self.items
            .iter()
            .filter(|i| i.required)
            .all(|i| i.decision.is_decided())
    }

    /// Check if all permissions (required + optional) have been decided.
    pub fn all_decided(&self) -> bool {
        self.items.iter().all(|i| i.decision.is_decided())
    }

    /// Check if all required permissions were granted.
    pub fn all_required_granted(&self) -> bool {
        self.items
            .iter()
            .filter(|i| i.required)
            .all(|i| i.decision.is_allowed())
    }

    /// Count of pending (undecided) items.
    pub fn pending_count(&self) -> usize {
        self.items.iter().filter(|i| !i.decision.is_decided()).count()
    }

    /// Produce the final approval result.
    ///
    /// Returns `None` if required permissions haven't been decided.
    pub fn finalize(&self) -> Option<InstallApproval> {
        if !self.all_required_decided() {
            return None;
        }

        let approved = self.all_required_granted();

        let mut granted_permissions = PermissionSet::none();
        for item in &self.items {
            if item.decision.is_allowed() {
                granted_permissions.grant(item.permission);
            }
        }

        // Copy scoping from manifest
        for domain in &self.allowed_domains {
            granted_permissions.allow_domain(domain.clone());
        }
        for path in &self.allowed_paths {
            granted_permissions.allow_path(path.clone());
        }

        let decisions: HashMap<PermissionKind, PermissionDecision> = self
            .items
            .iter()
            .map(|i| (i.permission, i.decision))
            .collect();

        Some(InstallApproval {
            plugin_id: self.plugin_id,
            plugin_name: self.plugin_name.clone(),
            plugin_version: self.plugin_version.clone(),
            approved,
            granted_permissions,
            decisions,
            overall_risk: self.overall_risk,
        })
    }
}

/// Compute the overall risk from a set of permission items.
fn compute_overall_risk(items: &[PermissionPromptItem]) -> RiskLevel {
    let max_risk = items.iter().map(|i| i.risk).max().unwrap_or(RiskLevel::Low);

    // Check for critical combinations
    let has_network = items.iter().any(|i| i.permission == PermissionKind::Network);
    let has_doc_write = items.iter().any(|i| i.permission == PermissionKind::DocumentWrite);
    let has_file_write = items.iter().any(|i| i.permission == PermissionKind::FileWrite);

    if has_network && (has_doc_write || has_file_write) {
        RiskLevel::Critical
    } else {
        max_risk
    }
}

// ═══════════════════════════════════════════════════════════════
// Install Approval
// ═══════════════════════════════════════════════════════════════

/// The final result of a permission review session.
///
/// This is passed to the install pipeline to determine whether
/// installation should proceed and with what permissions.
#[derive(Debug, Clone)]
pub struct InstallApproval {
    /// Plugin ID.
    pub plugin_id: Uuid,
    /// Plugin name.
    pub plugin_name: String,
    /// Plugin version.
    pub plugin_version: SemVer,
    /// Whether the install was approved.
    pub approved: bool,
    /// The permissions that were granted (subset of requested).
    pub granted_permissions: PermissionSet,
    /// Per-permission decisions.
    pub decisions: HashMap<PermissionKind, PermissionDecision>,
    /// Overall risk level.
    pub overall_risk: RiskLevel,
}

impl InstallApproval {
    /// Whether any permission was denied permanently.
    pub fn has_permanent_denials(&self) -> bool {
        self.decisions.values().any(|d| *d == PermissionDecision::DeniedAlways)
    }

    /// Whether any permission was granted only once (temporary).
    pub fn has_temporary_grants(&self) -> bool {
        self.decisions.values().any(|d| *d == PermissionDecision::GrantedOnce)
    }

    /// Number of permissions granted.
    pub fn granted_count(&self) -> usize {
        self.decisions.values().filter(|d| d.is_allowed()).count()
    }

    /// Number of permissions denied.
    pub fn denied_count(&self) -> usize {
        self.decisions.values().filter(|d| !d.is_allowed()).count()
    }
}

impl std::fmt::Display for InstallApproval {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.approved {
            write!(
                f,
                "APPROVED: {} v{} ({} permissions granted, risk: {})",
                self.plugin_name,
                self.plugin_version,
                self.granted_count(),
                self.overall_risk
            )
        } else {
            write!(
                f,
                "DENIED: {} v{} ({} permissions denied)",
                self.plugin_name,
                self.plugin_version,
                self.denied_count()
            )
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// Saved Permission Preferences
// ═══════════════════════════════════════════════════════════════

/// Stores user's saved permission decisions for known plugins.
///
/// When a plugin is updated, the saved decisions carry forward
/// so the user doesn't have to re-approve unchanged permissions.
#[derive(Debug, Clone, Default)]
pub struct SavedPermissionPreferences {
    /// Map of plugin_id → (permission → decision).
    entries: HashMap<Uuid, HashMap<PermissionKind, PermissionDecision>>,
}

impl SavedPermissionPreferences {
    /// Create empty preferences.
    pub fn new() -> Self {
        Self::default()
    }

    /// Save decisions from an install approval.
    pub fn save_from_approval(&mut self, approval: &InstallApproval) {
        let permanent: HashMap<PermissionKind, PermissionDecision> = approval
            .decisions
            .iter()
            .filter(|(_, d)| d.is_permanent())
            .map(|(k, d)| (*k, *d))
            .collect();
        if !permanent.is_empty() {
            self.entries.insert(approval.plugin_id, permanent);
        }
    }

    /// Look up a saved decision for a plugin's permission.
    pub fn get_decision(
        &self,
        plugin_id: &Uuid,
        permission: &PermissionKind,
    ) -> Option<PermissionDecision> {
        self.entries
            .get(plugin_id)
            .and_then(|m| m.get(permission))
            .copied()
    }

    /// Apply saved decisions to a prompt session.
    ///
    /// Returns the number of decisions applied.
    pub fn apply_to_session(&self, session: &mut PermissionPromptSession) -> usize {
        let mut count = 0;
        if let Some(saved) = self.entries.get(&session.plugin_id) {
            for item in &mut session.items {
                if let Some(decision) = saved.get(&item.permission) {
                    item.decision = *decision;
                    count += 1;
                }
            }
        }
        count
    }

    /// Remove saved preferences for a plugin.
    pub fn remove_plugin(&mut self, plugin_id: &Uuid) -> bool {
        self.entries.remove(plugin_id).is_some()
    }

    /// Number of plugins with saved preferences.
    pub fn plugin_count(&self) -> usize {
        self.entries.len()
    }
}

// ═══════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a test manifest with specific permissions.
    fn test_manifest(perms: &[PermissionKind]) -> PluginManifest {
        let mut manifest = PluginManifest::new("Test Plugin");
        manifest.author = "test-author".to_string();
        manifest.version = SemVer::new(1, 0, 0);
        for p in perms {
            manifest.permissions.grant(*p);
        }
        manifest
    }

    // ── PermissionDecision Tests ─────────────────────────────

    #[test]
    fn test_decision_allowed() {
        assert!(PermissionDecision::Granted.is_allowed());
        assert!(PermissionDecision::GrantedOnce.is_allowed());
        assert!(!PermissionDecision::DeniedAlways.is_allowed());
        assert!(!PermissionDecision::DeniedOnce.is_allowed());
        assert!(!PermissionDecision::Pending.is_allowed());
    }

    #[test]
    fn test_decision_permanent() {
        assert!(PermissionDecision::Granted.is_permanent());
        assert!(PermissionDecision::DeniedAlways.is_permanent());
        assert!(!PermissionDecision::GrantedOnce.is_permanent());
        assert!(!PermissionDecision::DeniedOnce.is_permanent());
    }

    #[test]
    fn test_decision_display() {
        assert_eq!(PermissionDecision::Granted.to_string(), "granted");
        assert_eq!(PermissionDecision::DeniedAlways.to_string(), "denied_always");
        assert_eq!(PermissionDecision::Pending.to_string(), "pending");
    }

    // ── RiskLevel Tests ──────────────────────────────────────

    #[test]
    fn test_risk_level_ordering() {
        assert!(RiskLevel::Low < RiskLevel::Medium);
        assert!(RiskLevel::Medium < RiskLevel::High);
        assert!(RiskLevel::High < RiskLevel::Critical);
    }

    #[test]
    fn test_risk_level_classification() {
        assert_eq!(RiskLevel::for_permission(&PermissionKind::DocumentRead), RiskLevel::Low);
        assert_eq!(RiskLevel::for_permission(&PermissionKind::DocumentWrite), RiskLevel::Medium);
        assert_eq!(RiskLevel::for_permission(&PermissionKind::Network), RiskLevel::High);
        assert_eq!(RiskLevel::for_permission(&PermissionKind::FileWrite), RiskLevel::High);
    }

    #[test]
    fn test_risk_level_display() {
        assert_eq!(RiskLevel::Low.to_string(), "low");
        assert_eq!(RiskLevel::Critical.to_string(), "critical");
    }

    #[test]
    fn test_risk_description_not_empty() {
        assert!(!RiskLevel::Low.description().is_empty());
        assert!(!RiskLevel::Critical.description().is_empty());
    }

    // ── PermissionPromptItem Tests ───────────────────────────

    #[test]
    fn test_prompt_item_new() {
        let item = PermissionPromptItem::new(PermissionKind::Network, true);
        assert_eq!(item.permission, PermissionKind::Network);
        assert!(item.required);
        assert_eq!(item.risk, RiskLevel::High);
        assert_eq!(item.decision, PermissionDecision::Pending);
        assert!(!item.reason.is_empty());
    }

    #[test]
    fn test_prompt_item_custom_reason() {
        let item = PermissionPromptItem::new(PermissionKind::Network, false)
            .with_reason("fetch AI model weights");
        assert_eq!(item.reason, "fetch AI model weights");
    }

    // ── PermissionPromptSession Tests ────────────────────────

    #[test]
    fn test_session_from_manifest() {
        let manifest = test_manifest(&[
            PermissionKind::DocumentRead,
            PermissionKind::DocumentWrite,
        ]);
        let session = PermissionPromptSession::from_manifest(&manifest);
        assert_eq!(session.plugin_name, "Test Plugin");
        assert_eq!(session.items.len(), 2);
        assert_eq!(session.pending_count(), 2);
        assert!(!session.all_decided());
    }

    #[test]
    fn test_session_risk_sorting() {
        let manifest = test_manifest(&[
            PermissionKind::DocumentRead,
            PermissionKind::Network,
            PermissionKind::Notifications,
        ]);
        let session = PermissionPromptSession::from_manifest(&manifest);
        // Highest risk should be first
        assert_eq!(session.items[0].permission, PermissionKind::Network);
    }

    #[test]
    fn test_session_critical_risk() {
        let manifest = test_manifest(&[
            PermissionKind::Network,
            PermissionKind::DocumentWrite,
        ]);
        let session = PermissionPromptSession::from_manifest(&manifest);
        assert_eq!(session.overall_risk, RiskLevel::Critical);
    }

    #[test]
    fn test_session_decide() {
        let manifest = test_manifest(&[PermissionKind::DocumentRead]);
        let mut session = PermissionPromptSession::from_manifest(&manifest);
        assert!(session.decide(&PermissionKind::DocumentRead, PermissionDecision::Granted));
        assert!(session.all_decided());
        assert!(session.all_required_granted());
    }

    #[test]
    fn test_session_decide_nonexistent() {
        let manifest = test_manifest(&[PermissionKind::DocumentRead]);
        let mut session = PermissionPromptSession::from_manifest(&manifest);
        assert!(!session.decide(&PermissionKind::Network, PermissionDecision::Granted));
    }

    #[test]
    fn test_session_grant_all() {
        let manifest = test_manifest(&[
            PermissionKind::DocumentRead,
            PermissionKind::Network,
        ]);
        let mut session = PermissionPromptSession::from_manifest(&manifest);
        session.grant_all();
        assert!(session.all_decided());
        assert!(session.all_required_granted());
    }

    #[test]
    fn test_session_deny_all() {
        let manifest = test_manifest(&[
            PermissionKind::DocumentRead,
            PermissionKind::Network,
        ]);
        let mut session = PermissionPromptSession::from_manifest(&manifest);
        session.deny_all();
        assert!(session.all_decided());
        assert!(!session.all_required_granted());
    }

    #[test]
    fn test_session_finalize_approved() {
        let manifest = test_manifest(&[
            PermissionKind::DocumentRead,
            PermissionKind::Notifications,
        ]);
        let mut session = PermissionPromptSession::from_manifest(&manifest);
        session.grant_all();
        let approval = session.finalize().unwrap();
        assert!(approval.approved);
        assert_eq!(approval.granted_count(), 2);
        assert_eq!(approval.denied_count(), 0);
        assert!(approval.granted_permissions.has(&PermissionKind::DocumentRead));
    }

    #[test]
    fn test_session_finalize_denied() {
        let manifest = test_manifest(&[PermissionKind::Network]);
        let mut session = PermissionPromptSession::from_manifest(&manifest);
        session.decide(&PermissionKind::Network, PermissionDecision::DeniedAlways);
        let approval = session.finalize().unwrap();
        assert!(!approval.approved);
        assert!(approval.has_permanent_denials());
    }

    #[test]
    fn test_session_finalize_pending_returns_none() {
        let manifest = test_manifest(&[PermissionKind::DocumentRead]);
        let session = PermissionPromptSession::from_manifest(&manifest);
        assert!(session.finalize().is_none());
    }

    #[test]
    fn test_session_finalize_with_domains() {
        let mut manifest = test_manifest(&[PermissionKind::Network]);
        manifest.permissions.allow_domain("api.logos.dev");
        let mut session = PermissionPromptSession::from_manifest(&manifest);
        session.grant_all();
        let approval = session.finalize().unwrap();
        assert!(approval.granted_permissions.is_domain_allowed("api.logos.dev"));
    }

    // ── InstallApproval Tests ────────────────────────────────

    #[test]
    fn test_approval_display_approved() {
        let manifest = test_manifest(&[PermissionKind::DocumentRead]);
        let mut session = PermissionPromptSession::from_manifest(&manifest);
        session.grant_all();
        let approval = session.finalize().unwrap();
        let display = approval.to_string();
        assert!(display.contains("APPROVED"));
        assert!(display.contains("Test Plugin"));
    }

    #[test]
    fn test_approval_display_denied() {
        let manifest = test_manifest(&[PermissionKind::Network]);
        let mut session = PermissionPromptSession::from_manifest(&manifest);
        session.deny_all();
        let approval = session.finalize().unwrap();
        let display = approval.to_string();
        assert!(display.contains("DENIED"));
    }

    #[test]
    fn test_approval_temporary_grants() {
        let manifest = test_manifest(&[PermissionKind::DocumentRead]);
        let mut session = PermissionPromptSession::from_manifest(&manifest);
        session.decide(&PermissionKind::DocumentRead, PermissionDecision::GrantedOnce);
        let approval = session.finalize().unwrap();
        assert!(approval.has_temporary_grants());
    }

    // ── SavedPermissionPreferences Tests ─────────────────────

    #[test]
    fn test_saved_prefs_new() {
        let prefs = SavedPermissionPreferences::new();
        assert_eq!(prefs.plugin_count(), 0);
    }

    #[test]
    fn test_saved_prefs_save_and_lookup() {
        let mut prefs = SavedPermissionPreferences::new();
        let manifest = test_manifest(&[PermissionKind::DocumentRead, PermissionKind::Network]);
        let mut session = PermissionPromptSession::from_manifest(&manifest);
        session.decide(&PermissionKind::DocumentRead, PermissionDecision::Granted);
        session.decide(&PermissionKind::Network, PermissionDecision::DeniedAlways);
        let approval = session.finalize().unwrap();
        prefs.save_from_approval(&approval);

        assert_eq!(prefs.plugin_count(), 1);
        assert_eq!(
            prefs.get_decision(&manifest.id, &PermissionKind::DocumentRead),
            Some(PermissionDecision::Granted)
        );
        assert_eq!(
            prefs.get_decision(&manifest.id, &PermissionKind::Network),
            Some(PermissionDecision::DeniedAlways)
        );
    }

    #[test]
    fn test_saved_prefs_temporary_not_saved() {
        let mut prefs = SavedPermissionPreferences::new();
        let manifest = test_manifest(&[PermissionKind::DocumentRead]);
        let mut session = PermissionPromptSession::from_manifest(&manifest);
        session.decide(&PermissionKind::DocumentRead, PermissionDecision::GrantedOnce);
        let approval = session.finalize().unwrap();
        prefs.save_from_approval(&approval);
        // GrantedOnce is not permanent, so nothing saved
        assert_eq!(prefs.plugin_count(), 0);
    }

    #[test]
    fn test_saved_prefs_apply_to_session() {
        let mut prefs = SavedPermissionPreferences::new();
        let manifest = test_manifest(&[PermissionKind::DocumentRead]);
        let mut session = PermissionPromptSession::from_manifest(&manifest);
        session.grant_all();
        let approval = session.finalize().unwrap();
        prefs.save_from_approval(&approval);

        // New session for same plugin
        let mut session2 = PermissionPromptSession::from_manifest(&manifest);
        let applied = prefs.apply_to_session(&mut session2);
        assert_eq!(applied, 1);
        assert!(session2.all_decided());
    }

    #[test]
    fn test_saved_prefs_remove_plugin() {
        let mut prefs = SavedPermissionPreferences::new();
        let manifest = test_manifest(&[PermissionKind::DocumentRead]);
        let mut session = PermissionPromptSession::from_manifest(&manifest);
        session.grant_all();
        let approval = session.finalize().unwrap();
        prefs.save_from_approval(&approval);
        assert_eq!(prefs.plugin_count(), 1);
        assert!(prefs.remove_plugin(&manifest.id));
        assert_eq!(prefs.plugin_count(), 0);
    }
}
