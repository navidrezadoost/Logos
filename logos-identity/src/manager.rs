//! IdentityManager — orchestrates user lifecycle, sessions, permissions, and audit.
//!
//! This is the main entry point for identity operations. It coordinates
//! the user store, session store, ACLs, and audit log into a coherent API.

use crate::acl::AccessControlList;
use crate::audit::{AuditAction, AuditEntry, AuditFilter, AuditLog, ResourceType};
use crate::credential::Credential;
use crate::error::IdentityError;
use crate::permission::{Permission, PermissionSet};
use crate::role::Role;
use crate::session::{Session, SessionId};
use crate::session_store::SessionStore;
use crate::store::UserStore;
use crate::user::{AccountStatus, AuthProvider, User, UserId, UserProfile};
use std::collections::HashMap;
use uuid::Uuid;

/// Configuration for the IdentityManager.
#[derive(Debug, Clone)]
pub struct IdentityConfig {
    /// Default session TTL in seconds.
    pub session_ttl: u64,
    /// Maximum concurrent sessions per user.
    pub max_sessions_per_user: usize,
    /// Whether email verification is required before login.
    pub require_email_verification: bool,
    /// Whether to allow anonymous users.
    pub allow_anonymous: bool,
    /// Name prefix for anonymous users.
    pub anonymous_prefix: String,
}

impl Default for IdentityConfig {
    fn default() -> Self {
        Self {
            session_ttl: 86400,
            max_sessions_per_user: 10,
            require_email_verification: true,
            allow_anonymous: false,
            anonymous_prefix: "Anonymous".into(),
        }
    }
}

/// The central identity manager.
///
/// Generic over store implementations so consumers can plug in
/// any backend (in-memory, RocksDB, SQLite, Postgres).
pub struct IdentityManager<U: UserStore, S: SessionStore, A: AuditLog> {
    user_store: U,
    session_store: S,
    audit_log: A,
    acls: HashMap<Uuid, AccessControlList>,
    config: IdentityConfig,
}

impl<U: UserStore, S: SessionStore, A: AuditLog> IdentityManager<U, S, A> {
    /// Create a new IdentityManager with default config.
    pub fn new(user_store: U, session_store: S, audit_log: A) -> Self {
        Self {
            user_store,
            session_store,
            audit_log,
            acls: HashMap::new(),
            config: IdentityConfig::default(),
        }
    }

    /// Create with custom config.
    pub fn with_config(user_store: U, session_store: S, audit_log: A, config: IdentityConfig) -> Self {
        Self {
            user_store,
            session_store,
            audit_log,
            acls: HashMap::new(),
            config,
        }
    }

    // ── User Lifecycle ──────────────────────────────────────────

    /// Register a new user.
    pub fn register_user(
        &mut self,
        email: impl Into<String>,
        display_name: impl Into<String>,
        provider: AuthProvider,
    ) -> Result<User, IdentityError> {
        let email = email.into();
        let display_name = display_name.into();

        if email.is_empty() || !email.contains('@') {
            return Err(IdentityError::InvalidInput("Invalid email address".into()));
        }
        if display_name.is_empty() {
            return Err(IdentityError::InvalidInput("Display name required".into()));
        }

        let user = if provider.is_oauth() {
            User::new_verified(&email, &display_name, provider)
        } else {
            User::new(&email, &display_name, provider)
        };

        let user_id = self.user_store.create_user(user.clone())?;

        self.audit_log.log(
            AuditEntry::new(user_id, AuditAction::UserCreated, ResourceType::User, user_id.as_uuid())
                .with_details(format!("Registered: {}", &email)),
        )?;

        Ok(user)
    }

    /// Register and set a credential in one step.
    pub fn register_with_credential(
        &mut self,
        email: impl Into<String>,
        display_name: impl Into<String>,
        provider: AuthProvider,
        credential: Credential,
    ) -> Result<User, IdentityError> {
        let user = self.register_user(email, display_name, provider)?;
        self.user_store.set_credential(&user.id, credential)?;
        Ok(user)
    }

    /// Get a user by ID.
    pub fn get_user(&self, id: &UserId) -> Result<Option<User>, IdentityError> {
        self.user_store.get_user(id)
    }

    /// Get a user by email.
    pub fn get_user_by_email(&self, email: &str) -> Result<Option<User>, IdentityError> {
        self.user_store.get_user_by_email(email)
    }

    /// Get a user's public profile.
    pub fn get_profile(&self, id: &UserId) -> Result<Option<UserProfile>, IdentityError> {
        Ok(self.user_store.get_user(id)?.map(|u| u.to_profile()))
    }

    /// Update a user's display name and avatar.
    pub fn update_profile(
        &mut self,
        user_id: &UserId,
        display_name: impl Into<String>,
        avatar_url: Option<String>,
    ) -> Result<(), IdentityError> {
        let mut user = self.user_store.get_user(user_id)?
            .ok_or(IdentityError::UserNotFound(*user_id))?;
        user.update_profile(display_name, avatar_url);
        self.user_store.update_user(&user)?;
        self.audit_log.log(
            AuditEntry::new(*user_id, AuditAction::UserUpdated, ResourceType::User, user_id.as_uuid()),
        )?;
        Ok(())
    }

    /// Verify a user's email.
    pub fn verify_email(&mut self, user_id: &UserId) -> Result<(), IdentityError> {
        let mut user = self.user_store.get_user(user_id)?
            .ok_or(IdentityError::UserNotFound(*user_id))?;
        user.verify_email();
        self.user_store.update_user(&user)?;
        self.audit_log.log(
            AuditEntry::new(*user_id, AuditAction::EmailVerified, ResourceType::User, user_id.as_uuid()),
        )?;
        Ok(())
    }

    /// Suspend a user account.
    pub fn suspend_user(&mut self, user_id: &UserId, suspended_by: &UserId) -> Result<(), IdentityError> {
        let mut user = self.user_store.get_user(user_id)?
            .ok_or(IdentityError::UserNotFound(*user_id))?;
        user.suspend();
        self.user_store.update_user(&user)?;
        // Revoke all sessions
        self.session_store.revoke_user_sessions(user_id)?;
        self.audit_log.log(
            AuditEntry::new(*suspended_by, AuditAction::UserSuspended, ResourceType::User, user_id.as_uuid()),
        )?;
        Ok(())
    }

    /// Reactivate a suspended user.
    pub fn reactivate_user(&mut self, user_id: &UserId, reactivated_by: &UserId) -> Result<(), IdentityError> {
        let mut user = self.user_store.get_user(user_id)?
            .ok_or(IdentityError::UserNotFound(*user_id))?;
        user.reactivate();
        self.user_store.update_user(&user)?;
        self.audit_log.log(
            AuditEntry::new(*reactivated_by, AuditAction::UserReactivated, ResourceType::User, user_id.as_uuid()),
        )?;
        Ok(())
    }

    /// Deactivate a user (self-service).
    pub fn deactivate_user(&mut self, user_id: &UserId) -> Result<(), IdentityError> {
        let mut user = self.user_store.get_user(user_id)?
            .ok_or(IdentityError::UserNotFound(*user_id))?;
        user.deactivate();
        self.user_store.update_user(&user)?;
        self.session_store.revoke_user_sessions(user_id)?;
        self.audit_log.log(
            AuditEntry::new(*user_id, AuditAction::UserDeleted, ResourceType::User, user_id.as_uuid()),
        )?;
        Ok(())
    }

    /// Delete a user permanently.
    pub fn delete_user(&mut self, user_id: &UserId, deleted_by: &UserId) -> Result<bool, IdentityError> {
        self.session_store.revoke_user_sessions(user_id)?;
        self.user_store.clear_credentials(user_id)?;
        let deleted = self.user_store.delete_user(user_id)?;
        if deleted {
            self.audit_log.log(
                AuditEntry::new(*deleted_by, AuditAction::UserDeleted, ResourceType::User, user_id.as_uuid()),
            )?;
        }
        Ok(deleted)
    }

    // ── Session Lifecycle ───────────────────────────────────────

    /// Create a new session for a user (typically after authentication).
    pub fn create_session(&mut self, user_id: &UserId) -> Result<Session, IdentityError> {
        let user = self.user_store.get_user(user_id)?
            .ok_or(IdentityError::UserNotFound(*user_id))?;

        if user.status == AccountStatus::Suspended {
            return Err(IdentityError::AccountSuspended);
        }
        if self.config.require_email_verification
            && user.status == AccountStatus::PendingVerification
        {
            return Err(IdentityError::AccountNotVerified);
        }

        // Check session limit
        let active = self.session_store.active_session_count(user_id)?;
        if active >= self.config.max_sessions_per_user {
            return Err(IdentityError::CapacityExceeded(
                format!("Max {} sessions per user", self.config.max_sessions_per_user),
            ));
        }

        let session = Session::new(*user_id, self.config.session_ttl);
        self.session_store.create_session(session.clone())?;

        // Record login
        let mut user = user;
        user.record_login();
        self.user_store.update_user(&user)?;

        self.audit_log.log(
            AuditEntry::new(*user_id, AuditAction::SessionCreated, ResourceType::Session, session.id.as_uuid()),
        )?;

        Ok(session)
    }

    /// Validate an existing session.
    pub fn validate_session(&self, session_id: &SessionId) -> Result<Session, IdentityError> {
        let session = self.session_store.get_session(session_id)?
            .ok_or(IdentityError::SessionNotFound)?;
        if session.is_revoked {
            return Err(IdentityError::SessionRevoked);
        }
        if session.is_expired() {
            return Err(IdentityError::SessionExpired(session_id.to_string()));
        }
        Ok(session)
    }

    /// Touch a session (update last-active timestamp).
    pub fn touch_session(&mut self, session_id: &SessionId) -> Result<(), IdentityError> {
        let mut session = self.session_store.get_session(session_id)?
            .ok_or(IdentityError::SessionNotFound)?;
        session.touch();
        self.session_store.update_session(&session)?;
        Ok(())
    }

    /// End a session (logout).
    pub fn end_session(&mut self, session_id: &SessionId) -> Result<(), IdentityError> {
        let session = self.session_store.get_session(session_id)?
            .ok_or(IdentityError::SessionNotFound)?;
        self.session_store.delete_session(session_id)?;
        self.audit_log.log(
            AuditEntry::new(session.user_id, AuditAction::Logout, ResourceType::Session, session_id.as_uuid()),
        )?;
        Ok(())
    }

    /// Revoke all sessions for a user (e.g., after password change).
    pub fn end_all_sessions(&mut self, user_id: &UserId) -> Result<usize, IdentityError> {
        let count = self.session_store.revoke_user_sessions(user_id)?;
        self.audit_log.log(
            AuditEntry::new(*user_id, AuditAction::AllSessionsRevoked, ResourceType::Session, user_id.as_uuid())
                .with_details(format!("{} sessions revoked", count)),
        )?;
        Ok(count)
    }

    // ── Permission Management ───────────────────────────────────

    /// Get or create the ACL for a resource.
    pub fn get_or_create_acl(&mut self, resource_id: Uuid, owner: UserId) -> &mut AccessControlList {
        self.acls
            .entry(resource_id)
            .or_insert_with(|| AccessControlList::new(resource_id, owner))
    }

    /// Get an existing ACL.
    pub fn get_acl(&self, resource_id: &Uuid) -> Option<&AccessControlList> {
        self.acls.get(resource_id)
    }

    /// Grant access to a resource.
    pub fn grant_access(
        &mut self,
        resource_id: Uuid,
        user_id: UserId,
        role: Role,
        granted_by: UserId,
    ) -> Result<(), IdentityError> {
        let acl = self.acls.get_mut(&resource_id)
            .ok_or_else(|| IdentityError::ResourceNotFound(resource_id.to_string()))?;
        acl.grant(user_id, role, granted_by)?;
        self.audit_log.log(
            AuditEntry::new(granted_by, AuditAction::PermissionGranted, ResourceType::Permission, resource_id)
                .with_details(format!("Granted {:?} to {}", role, user_id)),
        )?;
        Ok(())
    }

    /// Revoke access to a resource.
    pub fn revoke_access(
        &mut self,
        resource_id: Uuid,
        user_id: UserId,
        revoked_by: UserId,
    ) -> Result<bool, IdentityError> {
        let acl = self.acls.get_mut(&resource_id)
            .ok_or_else(|| IdentityError::ResourceNotFound(resource_id.to_string()))?;
        let removed = acl.revoke(user_id)?;
        if removed {
            self.audit_log.log(
                AuditEntry::new(revoked_by, AuditAction::PermissionRevoked, ResourceType::Permission, resource_id)
                    .with_details(format!("Revoked access for {}", user_id)),
            )?;
        }
        Ok(removed)
    }

    /// Check if a user has a specific permission on a resource.
    pub fn check_permission(
        &self,
        user_id: &UserId,
        resource_id: &Uuid,
        permission: Permission,
    ) -> bool {
        self.acls.get(resource_id)
            .map_or(false, |acl| acl.check(user_id, permission))
    }

    /// Get a user's effective role on a resource.
    pub fn get_role(&self, user_id: &UserId, resource_id: &Uuid) -> Option<Role> {
        self.acls.get(resource_id)
            .and_then(|acl| acl.get_role(user_id))
    }

    /// Get effective permissions for a user on a resource.
    pub fn effective_permissions(&self, user_id: &UserId, resource_id: &Uuid) -> PermissionSet {
        self.acls.get(resource_id)
            .map_or(PermissionSet::EMPTY, |acl| acl.effective_permissions(user_id))
    }

    /// List all users with access to a resource.
    pub fn list_resource_access(&self, resource_id: &Uuid) -> Result<Vec<(UserId, Role)>, IdentityError> {
        let acl = self.acls.get(resource_id)
            .ok_or_else(|| IdentityError::ResourceNotFound(resource_id.to_string()))?;
        Ok(acl.list_users_with_access())
    }

    // ── Audit ───────────────────────────────────────────────────

    /// Query the audit log.
    pub fn query_audit(&self, filter: &AuditFilter) -> Result<Vec<AuditEntry>, IdentityError> {
        self.audit_log.query(filter)
    }

    /// Count audit entries matching a filter.
    pub fn count_audit(&self, filter: &AuditFilter) -> Result<usize, IdentityError> {
        self.audit_log.count(filter)
    }

    // ── Accessors ───────────────────────────────────────────────

    pub fn user_store(&self) -> &U { &self.user_store }
    pub fn user_store_mut(&mut self) -> &mut U { &mut self.user_store }
    pub fn session_store(&self) -> &S { &self.session_store }
    pub fn session_store_mut(&mut self) -> &mut S { &mut self.session_store }
    pub fn audit_log(&self) -> &A { &self.audit_log }
    pub fn config(&self) -> &IdentityConfig { &self.config }

    // ── Maintenance ─────────────────────────────────────────────

    /// Clean up expired sessions.
    pub fn cleanup_expired_sessions(&mut self) -> Result<usize, IdentityError> {
        self.session_store.cleanup_expired()
    }

    /// Search users.
    pub fn search_users(&self, query: &str) -> Result<Vec<User>, IdentityError> {
        self.user_store.search_users(query)
    }

    /// Count total users.
    pub fn user_count(&self) -> Result<usize, IdentityError> {
        self.user_store.count_users()
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::InMemoryAuditLog;
    use crate::session_store::InMemorySessionStore;
    use crate::store::InMemoryUserStore;

    type TestManager = IdentityManager<InMemoryUserStore, InMemorySessionStore, InMemoryAuditLog>;

    fn make_manager() -> TestManager {
        let config = IdentityConfig {
            require_email_verification: false,
            ..Default::default()
        };
        IdentityManager::with_config(
            InMemoryUserStore::new(),
            InMemorySessionStore::new(),
            InMemoryAuditLog::new(),
            config,
        )
    }

    fn make_strict_manager() -> TestManager {
        IdentityManager::new(
            InMemoryUserStore::new(),
            InMemorySessionStore::new(),
            InMemoryAuditLog::new(),
        )
    }

    #[test]
    fn register_user() {
        let mut mgr = make_manager();
        let user = mgr.register_user("alice@test.com", "Alice", AuthProvider::Local).unwrap();
        assert_eq!(user.email, "alice@test.com");
        assert_eq!(user.display_name, "Alice");
        assert!(mgr.get_user(&user.id).unwrap().is_some());
    }

    #[test]
    fn register_oauth_user_is_verified() {
        let mut mgr = make_manager();
        let user = mgr.register_user("alice@google.com", "Alice", AuthProvider::Google).unwrap();
        assert!(user.email_verified);
        assert_eq!(user.status, AccountStatus::Active);
    }

    #[test]
    fn register_duplicate_email() {
        let mut mgr = make_manager();
        mgr.register_user("alice@test.com", "Alice", AuthProvider::Local).unwrap();
        let result = mgr.register_user("alice@test.com", "Alice2", AuthProvider::Local);
        assert!(matches!(result, Err(IdentityError::DuplicateEmail(_))));
    }

    #[test]
    fn register_invalid_email() {
        let mut mgr = make_manager();
        let result = mgr.register_user("not-an-email", "Test", AuthProvider::Local);
        assert!(matches!(result, Err(IdentityError::InvalidInput(_))));
    }

    #[test]
    fn register_empty_name() {
        let mut mgr = make_manager();
        let result = mgr.register_user("a@b.c", "", AuthProvider::Local);
        assert!(matches!(result, Err(IdentityError::InvalidInput(_))));
    }

    #[test]
    fn get_user_by_email() {
        let mut mgr = make_manager();
        mgr.register_user("alice@test.com", "Alice", AuthProvider::Local).unwrap();
        let user = mgr.get_user_by_email("alice@test.com").unwrap().unwrap();
        assert_eq!(user.display_name, "Alice");
    }

    #[test]
    fn get_profile() {
        let mut mgr = make_manager();
        let user = mgr.register_user("alice@test.com", "Alice", AuthProvider::Local).unwrap();
        let profile = mgr.get_profile(&user.id).unwrap().unwrap();
        assert_eq!(profile.display_name, "Alice");
        assert_eq!(profile.id, user.id);
    }

    #[test]
    fn update_profile() {
        let mut mgr = make_manager();
        let user = mgr.register_user("alice@test.com", "Alice", AuthProvider::Local).unwrap();
        mgr.update_profile(&user.id, "Alice Smith", Some("https://avatar.com/alice.png".into())).unwrap();
        let updated = mgr.get_user(&user.id).unwrap().unwrap();
        assert_eq!(updated.display_name, "Alice Smith");
        assert_eq!(updated.avatar_url.as_deref(), Some("https://avatar.com/alice.png"));
    }

    #[test]
    fn verify_email() {
        let mut mgr = make_manager();
        let user = mgr.register_user("alice@test.com", "Alice", AuthProvider::Local).unwrap();
        assert!(!user.email_verified);
        mgr.verify_email(&user.id).unwrap();
        let updated = mgr.get_user(&user.id).unwrap().unwrap();
        assert!(updated.email_verified);
        assert_eq!(updated.status, AccountStatus::Active);
    }

    #[test]
    fn suspend_and_reactivate() {
        let mut mgr = make_manager();
        let admin = UserId::new();
        let user = mgr.register_user("alice@test.com", "Alice", AuthProvider::Google).unwrap();

        mgr.suspend_user(&user.id, &admin).unwrap();
        let suspended = mgr.get_user(&user.id).unwrap().unwrap();
        assert_eq!(suspended.status, AccountStatus::Suspended);

        mgr.reactivate_user(&user.id, &admin).unwrap();
        let reactivated = mgr.get_user(&user.id).unwrap().unwrap();
        assert_eq!(reactivated.status, AccountStatus::Active);
    }

    #[test]
    fn deactivate_user() {
        let mut mgr = make_manager();
        let user = mgr.register_user("alice@test.com", "Alice", AuthProvider::Google).unwrap();
        mgr.deactivate_user(&user.id).unwrap();
        let deactivated = mgr.get_user(&user.id).unwrap().unwrap();
        assert_eq!(deactivated.status, AccountStatus::Deactivated);
    }

    #[test]
    fn delete_user() {
        let mut mgr = make_manager();
        let admin = UserId::new();
        let user = mgr.register_user("alice@test.com", "Alice", AuthProvider::Local).unwrap();
        assert!(mgr.delete_user(&user.id, &admin).unwrap());
        assert!(mgr.get_user(&user.id).unwrap().is_none());
    }

    #[test]
    fn create_session() {
        let mut mgr = make_manager();
        let user = mgr.register_user("alice@test.com", "Alice", AuthProvider::Google).unwrap();
        let session = mgr.create_session(&user.id).unwrap();
        assert_eq!(session.user_id, user.id);
        assert!(session.is_valid());
    }

    #[test]
    fn create_session_suspended() {
        let mut mgr = make_manager();
        let admin = UserId::new();
        let user = mgr.register_user("alice@test.com", "Alice", AuthProvider::Google).unwrap();
        mgr.suspend_user(&user.id, &admin).unwrap();
        let result = mgr.create_session(&user.id);
        assert!(matches!(result, Err(IdentityError::AccountSuspended)));
    }

    #[test]
    fn create_session_requires_verification() {
        let mut mgr = make_strict_manager();
        let user = mgr.register_user("alice@test.com", "Alice", AuthProvider::Local).unwrap();
        let result = mgr.create_session(&user.id);
        assert!(matches!(result, Err(IdentityError::AccountNotVerified)));
    }

    #[test]
    fn validate_session() {
        let mut mgr = make_manager();
        let user = mgr.register_user("alice@test.com", "Alice", AuthProvider::Google).unwrap();
        let session = mgr.create_session(&user.id).unwrap();
        let validated = mgr.validate_session(&session.id).unwrap();
        assert_eq!(validated.user_id, user.id);
    }

    #[test]
    fn validate_nonexistent_session() {
        let mgr = make_manager();
        let result = mgr.validate_session(&SessionId::new());
        assert!(matches!(result, Err(IdentityError::SessionNotFound)));
    }

    #[test]
    fn end_session() {
        let mut mgr = make_manager();
        let user = mgr.register_user("alice@test.com", "Alice", AuthProvider::Google).unwrap();
        let session = mgr.create_session(&user.id).unwrap();
        mgr.end_session(&session.id).unwrap();
        let result = mgr.validate_session(&session.id);
        assert!(matches!(result, Err(IdentityError::SessionNotFound)));
    }

    #[test]
    fn end_all_sessions() {
        let mut mgr = make_manager();
        let user = mgr.register_user("alice@test.com", "Alice", AuthProvider::Google).unwrap();
        mgr.create_session(&user.id).unwrap();
        mgr.create_session(&user.id).unwrap();
        let count = mgr.end_all_sessions(&user.id).unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn session_limit() {
        let config = IdentityConfig {
            max_sessions_per_user: 2,
            require_email_verification: false,
            ..Default::default()
        };
        let mut mgr = IdentityManager::with_config(
            InMemoryUserStore::new(),
            InMemorySessionStore::new(),
            InMemoryAuditLog::new(),
            config,
        );
        let user = mgr.register_user("a@b.c", "A", AuthProvider::Google).unwrap();
        mgr.create_session(&user.id).unwrap();
        mgr.create_session(&user.id).unwrap();
        let result = mgr.create_session(&user.id);
        assert!(matches!(result, Err(IdentityError::CapacityExceeded(_))));
    }

    #[test]
    fn permission_management() {
        let mut mgr = make_manager();
        let owner = UserId::new();
        let alice = UserId::new();
        let doc = Uuid::new_v4();

        mgr.get_or_create_acl(doc, owner);
        mgr.grant_access(doc, alice, Role::Editor, owner).unwrap();

        assert!(mgr.check_permission(&alice, &doc, Permission::EditDocument));
        assert!(!mgr.check_permission(&alice, &doc, Permission::DeleteDocument));
        assert_eq!(mgr.get_role(&alice, &doc), Some(Role::Editor));
    }

    #[test]
    fn revoke_access() {
        let mut mgr = make_manager();
        let owner_id = UserId::new();
        let alice = UserId::new();
        let doc = Uuid::new_v4();

        mgr.get_or_create_acl(doc, owner_id);
        mgr.grant_access(doc, alice, Role::Editor, owner_id).unwrap();
        mgr.revoke_access(doc, alice, owner_id).unwrap();
        assert!(!mgr.check_permission(&alice, &doc, Permission::EditDocument));
    }

    #[test]
    fn effective_permissions() {
        let mut mgr = make_manager();
        let owner_id = UserId::new();
        let doc = Uuid::new_v4();

        mgr.get_or_create_acl(doc, owner_id);
        let perms = mgr.effective_permissions(&owner_id, &doc);
        assert!(perms.has(Permission::DeleteDocument));
        assert!(perms.has(Permission::TransferOwnership));
    }

    #[test]
    fn list_resource_access() {
        let mut mgr = make_manager();
        let owner_id = UserId::new();
        let alice = UserId::new();
        let bob = UserId::new();
        let doc = Uuid::new_v4();

        mgr.get_or_create_acl(doc, owner_id);
        mgr.grant_access(doc, alice, Role::Editor, owner_id).unwrap();
        mgr.grant_access(doc, bob, Role::Viewer, owner_id).unwrap();
        let users = mgr.list_resource_access(&doc).unwrap();
        assert_eq!(users.len(), 3);
    }

    #[test]
    fn audit_trail() {
        let mut mgr = make_manager();
        let user = mgr.register_user("alice@test.com", "Alice", AuthProvider::Google).unwrap();
        mgr.create_session(&user.id).unwrap();

        let all = mgr.query_audit(&AuditFilter::new()).unwrap();
        assert!(all.len() >= 2); // UserCreated + SessionCreated

        let user_events = mgr.query_audit(&AuditFilter::for_user(user.id)).unwrap();
        assert!(user_events.len() >= 2);
    }

    #[test]
    fn search_users() {
        let mut mgr = make_manager();
        mgr.register_user("alice@test.com", "Alice Smith", AuthProvider::Local).unwrap();
        mgr.register_user("bob@test.com", "Bob Jones", AuthProvider::Local).unwrap();
        let results = mgr.search_users("alice").unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn user_count() {
        let mut mgr = make_manager();
        assert_eq!(mgr.user_count().unwrap(), 0);
        mgr.register_user("a@b.c", "A", AuthProvider::Local).unwrap();
        assert_eq!(mgr.user_count().unwrap(), 1);
    }

    #[test]
    fn cleanup_expired_sessions() {
        let mut mgr = make_manager();
        let user = mgr.register_user("a@b.c", "A", AuthProvider::Google).unwrap();
        let mut session = mgr.create_session(&user.id).unwrap();
        session.expires_at = crate::user::current_timestamp().saturating_sub(100);
        mgr.session_store_mut().update_session(&session).unwrap();
        let removed = mgr.cleanup_expired_sessions().unwrap();
        assert_eq!(removed, 1);
    }
}
