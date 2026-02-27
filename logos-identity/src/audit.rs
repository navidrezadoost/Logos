//! Audit trail for security and compliance logging.
//!
//! Records all identity-related actions (login, logout, permission
//! changes, etc.) with user context and timestamps.

use crate::error::IdentityError;
use crate::user::UserId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// An auditable action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditAction {
    // Authentication
    Login,
    LoginFailed,
    Logout,
    TokenRefresh,
    PasswordChanged,
    PasswordReset,

    // User lifecycle
    UserCreated,
    UserUpdated,
    UserDeleted,
    UserSuspended,
    UserReactivated,
    EmailVerified,

    // Permissions
    PermissionGranted,
    PermissionRevoked,
    RoleChanged,
    OwnershipTransferred,

    // Documents
    DocumentCreated,
    DocumentDeleted,
    DocumentShared,
    DocumentExported,

    // Sessions
    SessionCreated,
    SessionRevoked,
    AllSessionsRevoked,

    // Admin
    SystemConfigChanged,
}

impl AuditAction {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Login => "login",
            Self::LoginFailed => "login_failed",
            Self::Logout => "logout",
            Self::TokenRefresh => "token_refresh",
            Self::PasswordChanged => "password_changed",
            Self::PasswordReset => "password_reset",
            Self::UserCreated => "user_created",
            Self::UserUpdated => "user_updated",
            Self::UserDeleted => "user_deleted",
            Self::UserSuspended => "user_suspended",
            Self::UserReactivated => "user_reactivated",
            Self::EmailVerified => "email_verified",
            Self::PermissionGranted => "permission_granted",
            Self::PermissionRevoked => "permission_revoked",
            Self::RoleChanged => "role_changed",
            Self::OwnershipTransferred => "ownership_transferred",
            Self::DocumentCreated => "document_created",
            Self::DocumentDeleted => "document_deleted",
            Self::DocumentShared => "document_shared",
            Self::DocumentExported => "document_exported",
            Self::SessionCreated => "session_created",
            Self::SessionRevoked => "session_revoked",
            Self::AllSessionsRevoked => "all_sessions_revoked",
            Self::SystemConfigChanged => "system_config_changed",
        }
    }

    /// Whether this is a security-sensitive action.
    pub fn is_security_event(&self) -> bool {
        matches!(
            self,
            Self::Login | Self::LoginFailed | Self::Logout
            | Self::PasswordChanged | Self::PasswordReset
            | Self::UserSuspended | Self::UserDeleted
            | Self::PermissionGranted | Self::PermissionRevoked
            | Self::RoleChanged | Self::OwnershipTransferred
            | Self::AllSessionsRevoked | Self::SystemConfigChanged
        )
    }
}

/// The type of resource an audit entry refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResourceType {
    User,
    Document,
    Session,
    Permission,
    Comment,
    Plugin,
    System,
}

/// A single audit log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Unique entry ID.
    pub id: Uuid,
    /// When the action occurred (Unix timestamp).
    pub timestamp: u64,
    /// Who performed the action.
    pub user_id: UserId,
    /// What action was performed.
    pub action: AuditAction,
    /// What type of resource was affected.
    pub resource_type: ResourceType,
    /// ID of the affected resource.
    pub resource_id: Uuid,
    /// Additional human-readable details.
    pub details: Option<String>,
    /// Client IP address.
    pub ip_address: Option<String>,
}

impl AuditEntry {
    pub fn new(
        user_id: UserId,
        action: AuditAction,
        resource_type: ResourceType,
        resource_id: Uuid,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: crate::user::current_timestamp(),
            user_id,
            action,
            resource_type,
            resource_id,
            details: None,
            ip_address: None,
        }
    }

    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }

    pub fn with_ip(mut self, ip: impl Into<String>) -> Self {
        self.ip_address = Some(ip.into());
        self
    }
}

/// Filter for querying the audit log.
#[derive(Debug, Clone, Default)]
pub struct AuditFilter {
    pub user_id: Option<UserId>,
    pub action: Option<AuditAction>,
    pub resource_type: Option<ResourceType>,
    pub resource_id: Option<Uuid>,
    pub from_timestamp: Option<u64>,
    pub to_timestamp: Option<u64>,
    pub security_only: bool,
    pub limit: usize,
    pub offset: usize,
}

impl AuditFilter {
    pub fn new() -> Self {
        Self {
            limit: 100,
            ..Default::default()
        }
    }

    pub fn for_user(user_id: UserId) -> Self {
        Self { user_id: Some(user_id), limit: 100, ..Default::default() }
    }

    pub fn for_resource(resource_type: ResourceType, resource_id: Uuid) -> Self {
        Self {
            resource_type: Some(resource_type),
            resource_id: Some(resource_id),
            limit: 100,
            ..Default::default()
        }
    }

    pub fn security_events() -> Self {
        Self { security_only: true, limit: 100, ..Default::default() }
    }

    fn matches(&self, entry: &AuditEntry) -> bool {
        if let Some(uid) = &self.user_id {
            if entry.user_id != *uid { return false; }
        }
        if let Some(action) = &self.action {
            if entry.action != *action { return false; }
        }
        if let Some(rt) = &self.resource_type {
            if entry.resource_type != *rt { return false; }
        }
        if let Some(rid) = &self.resource_id {
            if entry.resource_id != *rid { return false; }
        }
        if let Some(from) = self.from_timestamp {
            if entry.timestamp < from { return false; }
        }
        if let Some(to) = self.to_timestamp {
            if entry.timestamp > to { return false; }
        }
        if self.security_only && !entry.action.is_security_event() {
            return false;
        }
        true
    }
}

/// Trait for audit log storage.
pub trait AuditLog {
    /// Record an audit entry.
    fn log(&mut self, entry: AuditEntry) -> Result<(), IdentityError>;

    /// Query entries matching a filter.
    fn query(&self, filter: &AuditFilter) -> Result<Vec<AuditEntry>, IdentityError>;

    /// Count entries matching a filter.
    fn count(&self, filter: &AuditFilter) -> Result<usize, IdentityError>;
}

/// In-memory audit log (for testing).
#[derive(Debug, Clone)]
pub struct InMemoryAuditLog {
    entries: Vec<AuditEntry>,
    max_entries: usize,
}

impl InMemoryAuditLog {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            max_entries: 100_000,
        }
    }

    pub fn with_capacity(max_entries: usize) -> Self {
        Self {
            entries: Vec::new(),
            max_entries,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

impl Default for InMemoryAuditLog {
    fn default() -> Self {
        Self::new()
    }
}

impl AuditLog for InMemoryAuditLog {
    fn log(&mut self, entry: AuditEntry) -> Result<(), IdentityError> {
        if self.entries.len() >= self.max_entries {
            // Evict oldest 10%
            let to_remove = self.max_entries / 10;
            self.entries.drain(..to_remove);
        }
        self.entries.push(entry);
        Ok(())
    }

    fn query(&self, filter: &AuditFilter) -> Result<Vec<AuditEntry>, IdentityError> {
        let results: Vec<AuditEntry> = self.entries.iter()
            .rev() // Most recent first
            .filter(|e| filter.matches(e))
            .skip(filter.offset)
            .take(if filter.limit == 0 { usize::MAX } else { filter.limit })
            .cloned()
            .collect();
        Ok(results)
    }

    fn count(&self, filter: &AuditFilter) -> Result<usize, IdentityError> {
        Ok(self.entries.iter().filter(|e| filter.matches(e)).count())
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn user1() -> UserId { UserId::from_uuid(Uuid::from_bytes([1; 16])) }
    fn user2() -> UserId { UserId::from_uuid(Uuid::from_bytes([2; 16])) }
    fn doc1() -> Uuid { Uuid::from_bytes([10; 16]) }

    #[test]
    fn log_and_query() {
        let mut log = InMemoryAuditLog::new();
        log.log(AuditEntry::new(user1(), AuditAction::Login, ResourceType::Session, Uuid::new_v4())).unwrap();
        log.log(AuditEntry::new(user1(), AuditAction::DocumentCreated, ResourceType::Document, doc1())).unwrap();
        log.log(AuditEntry::new(user2(), AuditAction::Login, ResourceType::Session, Uuid::new_v4())).unwrap();
        assert_eq!(log.len(), 3);

        let all = log.query(&AuditFilter::new()).unwrap();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn filter_by_user() {
        let mut log = InMemoryAuditLog::new();
        log.log(AuditEntry::new(user1(), AuditAction::Login, ResourceType::Session, Uuid::new_v4())).unwrap();
        log.log(AuditEntry::new(user2(), AuditAction::Login, ResourceType::Session, Uuid::new_v4())).unwrap();
        let results = log.query(&AuditFilter::for_user(user1())).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].user_id, user1());
    }

    #[test]
    fn filter_by_resource() {
        let mut log = InMemoryAuditLog::new();
        log.log(AuditEntry::new(user1(), AuditAction::DocumentCreated, ResourceType::Document, doc1())).unwrap();
        log.log(AuditEntry::new(user1(), AuditAction::Login, ResourceType::Session, Uuid::new_v4())).unwrap();
        let results = log.query(&AuditFilter::for_resource(ResourceType::Document, doc1())).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].action, AuditAction::DocumentCreated);
    }

    #[test]
    fn filter_security_only() {
        let mut log = InMemoryAuditLog::new();
        log.log(AuditEntry::new(user1(), AuditAction::Login, ResourceType::Session, Uuid::new_v4())).unwrap();
        log.log(AuditEntry::new(user1(), AuditAction::DocumentCreated, ResourceType::Document, doc1())).unwrap();
        log.log(AuditEntry::new(user1(), AuditAction::PasswordChanged, ResourceType::User, user1().as_uuid())).unwrap();
        let results = log.query(&AuditFilter::security_events()).unwrap();
        assert_eq!(results.len(), 2); // Login + PasswordChanged
    }

    #[test]
    fn count() {
        let mut log = InMemoryAuditLog::new();
        for _ in 0..5 {
            log.log(AuditEntry::new(user1(), AuditAction::Login, ResourceType::Session, Uuid::new_v4())).unwrap();
        }
        assert_eq!(log.count(&AuditFilter::new()).unwrap(), 5);
        assert_eq!(log.count(&AuditFilter::for_user(user2())).unwrap(), 0);
    }

    #[test]
    fn pagination() {
        let mut log = InMemoryAuditLog::new();
        for i in 0..10 {
            log.log(AuditEntry::new(user1(), AuditAction::Login, ResourceType::Session, Uuid::new_v4())
                .with_details(format!("login {}", i))).unwrap();
        }
        let filter = AuditFilter { limit: 3, offset: 0, ..Default::default() };
        assert_eq!(log.query(&filter).unwrap().len(), 3);
        let filter = AuditFilter { limit: 3, offset: 8, ..Default::default() };
        assert_eq!(log.query(&filter).unwrap().len(), 2);
    }

    #[test]
    fn eviction() {
        let mut log = InMemoryAuditLog::with_capacity(10);
        for _ in 0..15 {
            log.log(AuditEntry::new(user1(), AuditAction::Login, ResourceType::Session, Uuid::new_v4())).unwrap();
        }
        assert!(log.len() <= 15);
    }

    #[test]
    fn entry_with_details() {
        let entry = AuditEntry::new(user1(), AuditAction::Login, ResourceType::Session, Uuid::new_v4())
            .with_details("From Chrome on macOS")
            .with_ip("192.168.1.100");
        assert_eq!(entry.details.as_deref(), Some("From Chrome on macOS"));
        assert_eq!(entry.ip_address.as_deref(), Some("192.168.1.100"));
    }

    #[test]
    fn audit_action_labels() {
        assert_eq!(AuditAction::Login.label(), "login");
        assert_eq!(AuditAction::PasswordChanged.label(), "password_changed");
        assert_eq!(AuditAction::OwnershipTransferred.label(), "ownership_transferred");
    }

    #[test]
    fn security_event_classification() {
        assert!(AuditAction::Login.is_security_event());
        assert!(AuditAction::LoginFailed.is_security_event());
        assert!(AuditAction::PasswordChanged.is_security_event());
        assert!(!AuditAction::DocumentCreated.is_security_event());
        assert!(!AuditAction::DocumentExported.is_security_event());
    }

    #[test]
    fn entry_serde_roundtrip() {
        let entry = AuditEntry::new(user1(), AuditAction::Login, ResourceType::Session, Uuid::new_v4())
            .with_details("test");
        let json = serde_json::to_string(&entry).unwrap();
        let back: AuditEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back.user_id, user1());
        assert_eq!(back.action, AuditAction::Login);
    }

    #[test]
    fn clear() {
        let mut log = InMemoryAuditLog::new();
        log.log(AuditEntry::new(user1(), AuditAction::Login, ResourceType::Session, Uuid::new_v4())).unwrap();
        log.clear();
        assert!(log.is_empty());
    }
}
