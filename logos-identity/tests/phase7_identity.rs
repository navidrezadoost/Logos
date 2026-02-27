//! Phase 7 — Identity Layer integration tests.
//!
//! These tests verify end-to-end workflows through the IdentityManager
//! and cross-module interactions.

use logos_identity::*;
use uuid::Uuid;

// ── Helpers ─────────────────────────────────────────────────────────

fn manager() -> IdentityManager<InMemoryUserStore, InMemorySessionStore, InMemoryAuditLog> {
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

fn strict_manager() -> IdentityManager<InMemoryUserStore, InMemorySessionStore, InMemoryAuditLog> {
    IdentityManager::new(
        InMemoryUserStore::new(),
        InMemorySessionStore::new(),
        InMemoryAuditLog::new(),
    )
}

// ── Full Registration → Session → Permission Workflow ───────────────

#[test]
fn full_user_lifecycle() {
    let mut mgr = manager();

    // 1. Register user
    let alice = mgr.register_user("alice@example.com", "Alice", AuthProvider::Local).unwrap();
    assert_eq!(mgr.user_count().unwrap(), 1);

    // 2. Create session
    let session = mgr.create_session(&alice.id).unwrap();
    assert!(session.is_valid());

    // 3. Create document and grant access
    let doc_id = Uuid::new_v4();
    mgr.get_or_create_acl(doc_id, alice.id);
    assert!(mgr.check_permission(&alice.id, &doc_id, Permission::EditDocument));
    assert!(mgr.check_permission(&alice.id, &doc_id, Permission::DeleteDocument));

    // 4. Invite another user
    let bob = mgr.register_user("bob@example.com", "Bob", AuthProvider::Google).unwrap();
    mgr.grant_access(doc_id, bob.id, Role::Editor, alice.id).unwrap();
    assert!(mgr.check_permission(&bob.id, &doc_id, Permission::EditDocument));
    assert!(!mgr.check_permission(&bob.id, &doc_id, Permission::DeleteDocument));

    // 5. Audit trail tracks everything
    let audit = mgr.query_audit(&AuditFilter::new()).unwrap();
    assert!(audit.len() >= 4); // register, session, register, grant

    // 6. End session
    mgr.end_session(&session.id).unwrap();
    assert!(mgr.validate_session(&session.id).is_err());
}

#[test]
fn email_verification_flow() {
    let mut mgr = strict_manager();

    // Register with local auth — needs verification
    let user = mgr.register_user("new@site.com", "New User", AuthProvider::Local).unwrap();
    assert!(!user.email_verified);
    assert_eq!(user.status, AccountStatus::PendingVerification);

    // Cannot login before verification
    let err = mgr.create_session(&user.id).unwrap_err();
    assert!(matches!(err, IdentityError::AccountNotVerified));

    // Verify email
    mgr.verify_email(&user.id).unwrap();
    let updated = mgr.get_user(&user.id).unwrap().unwrap();
    assert!(updated.email_verified);
    assert_eq!(updated.status, AccountStatus::Active);

    // Now can login
    let session = mgr.create_session(&user.id).unwrap();
    assert!(session.is_valid());
}

#[test]
fn suspension_revokes_all_sessions() {
    let mut mgr = manager();
    let admin = UserId::new();
    let user = mgr.register_user("sue@test.com", "Sue", AuthProvider::Google).unwrap();

    let s1 = mgr.create_session(&user.id).unwrap();
    let s2 = mgr.create_session(&user.id).unwrap();

    mgr.suspend_user(&user.id, &admin).unwrap();

    // Both sessions should now be invalid (store revokes them)
    assert!(mgr.validate_session(&s1.id).is_err());
    assert!(mgr.validate_session(&s2.id).is_err());

    // Cannot create new session while suspended
    let err = mgr.create_session(&user.id).unwrap_err();
    assert!(matches!(err, IdentityError::AccountSuspended));

    // Reactivate
    mgr.reactivate_user(&user.id, &admin).unwrap();
    let _s3 = mgr.create_session(&user.id).unwrap();
}

#[test]
fn permission_escalation_denied() {
    let mut mgr = manager();
    let owner = UserId::new();
    let viewer_uid = UserId::new();
    let doc = Uuid::new_v4();

    mgr.get_or_create_acl(doc, owner);
    mgr.grant_access(doc, viewer_uid, Role::Viewer, owner).unwrap();

    // Viewer should not have edit or admin permissions
    assert!(mgr.check_permission(&viewer_uid, &doc, Permission::ViewDocument));
    assert!(!mgr.check_permission(&viewer_uid, &doc, Permission::EditDocument));
    assert!(!mgr.check_permission(&viewer_uid, &doc, Permission::DeleteDocument));
    assert!(!mgr.check_permission(&viewer_uid, &doc, Permission::ManageDocPermissions));
}

#[test]
fn role_upgrade_workflow() {
    let mut mgr = manager();
    let owner = UserId::new();
    let user = UserId::new();
    let doc = Uuid::new_v4();

    mgr.get_or_create_acl(doc, owner);

    // Start as Viewer
    mgr.grant_access(doc, user, Role::Viewer, owner).unwrap();
    assert!(!mgr.check_permission(&user, &doc, Permission::EditDocument));

    // Upgrade to Editor
    mgr.grant_access(doc, user, Role::Editor, owner).unwrap();
    assert!(mgr.check_permission(&user, &doc, Permission::EditDocument));
    assert!(!mgr.check_permission(&user, &doc, Permission::DeleteDocument));

    // Upgrade to Admin
    mgr.grant_access(doc, user, Role::Admin, owner).unwrap();
    assert!(mgr.check_permission(&user, &doc, Permission::ManageDocPermissions));
    assert!(mgr.check_permission(&user, &doc, Permission::ManageUsers));
    // DeleteDocument is Owner-only
    assert!(!mgr.check_permission(&user, &doc, Permission::DeleteDocument));
}

#[test]
fn multi_document_access() {
    let mut mgr = manager();
    let owner = UserId::new();
    let user = UserId::new();
    let doc1 = Uuid::new_v4();
    let doc2 = Uuid::new_v4();
    let doc3 = Uuid::new_v4();

    mgr.get_or_create_acl(doc1, owner);
    mgr.get_or_create_acl(doc2, owner);
    mgr.get_or_create_acl(doc3, owner);

    mgr.grant_access(doc1, user, Role::Editor, owner).unwrap();
    mgr.grant_access(doc2, user, Role::Viewer, owner).unwrap();
    // No access to doc3

    assert!(mgr.check_permission(&user, &doc1, Permission::EditDocument));
    assert!(mgr.check_permission(&user, &doc2, Permission::ViewDocument));
    assert!(!mgr.check_permission(&user, &doc2, Permission::EditDocument));
    assert!(!mgr.check_permission(&user, &doc3, Permission::ViewDocument));
}

#[test]
fn session_limit_and_cleanup() {
    let config = IdentityConfig {
        max_sessions_per_user: 3,
        require_email_verification: false,
        ..Default::default()
    };
    let mut mgr = IdentityManager::with_config(
        InMemoryUserStore::new(),
        InMemorySessionStore::new(),
        InMemoryAuditLog::new(),
        config,
    );

    let user = mgr.register_user("limit@test.com", "Limit", AuthProvider::Google).unwrap();
    mgr.create_session(&user.id).unwrap();
    mgr.create_session(&user.id).unwrap();
    let s3 = mgr.create_session(&user.id).unwrap();

    // Fourth session should fail
    assert!(mgr.create_session(&user.id).is_err());

    // End one session and try again
    mgr.end_session(&s3.id).unwrap();
    assert!(mgr.create_session(&user.id).is_ok());
}

#[test]
fn owner_transfer_permission_flow() {
    let mut mgr = manager();
    let owner = UserId::new();
    let new_owner = UserId::new();
    let doc = Uuid::new_v4();

    mgr.get_or_create_acl(doc, owner);

    // Owner can do everything
    assert!(mgr.check_permission(&owner, &doc, Permission::TransferOwnership));

    // Transfer ownership through ACL
    let acl = mgr.get_or_create_acl(doc, owner);
    acl.transfer_ownership(new_owner).unwrap();

    // New owner has full permissions
    assert!(mgr.check_permission(&new_owner, &doc, Permission::TransferOwnership));
    assert!(mgr.check_permission(&new_owner, &doc, Permission::DeleteDocument));

    // Old owner demoted to Admin
    assert_eq!(mgr.get_role(&owner, &doc), Some(Role::Admin));
    assert!(!mgr.check_permission(&owner, &doc, Permission::TransferOwnership));
}

#[test]
fn acl_public_access() {
    let mut mgr = manager();
    let owner = UserId::new();
    let random_user = UserId::new();
    let doc = Uuid::new_v4();

    mgr.get_or_create_acl(doc, owner);

    // Random user has no access
    assert!(!mgr.check_permission(&random_user, &doc, Permission::ViewDocument));

    // Make public
    let acl = mgr.get_or_create_acl(doc, owner);
    acl.set_public(true);

    // Now anyone can view
    assert!(mgr.check_permission(&random_user, &doc, Permission::ViewDocument));
    assert!(!mgr.check_permission(&random_user, &doc, Permission::EditDocument));
}

// ── Permission Set Algebra ──────────────────────────────────────────

#[test]
fn role_permission_hierarchy() {
    // Each higher role should be a superset of the lower
    let viewer = PermissionSet::for_role(Role::Viewer);
    let commenter = PermissionSet::for_role(Role::Commenter);
    let editor = PermissionSet::for_role(Role::Editor);
    let admin = PermissionSet::for_role(Role::Admin);
    let owner = PermissionSet::for_role(Role::Owner);

    assert!(commenter.contains_all(&viewer));
    assert!(editor.contains_all(&commenter));
    assert!(admin.contains_all(&editor));
    assert!(owner.contains_all(&admin));
}

#[test]
fn permission_set_operations() {
    let mut set = PermissionSet::new();
    set.grant(Permission::ViewDocument);
    set.grant(Permission::EditDocument);

    assert!(set.has(Permission::ViewDocument));
    assert!(set.has(Permission::EditDocument));
    assert!(!set.has(Permission::DeleteDocument));

    set.revoke(Permission::EditDocument);
    assert!(!set.has(Permission::EditDocument));
}

#[test]
fn permission_set_union_intersection() {
    let mut a = PermissionSet::new();
    a.grant(Permission::ViewDocument);
    a.grant(Permission::EditDocument);

    let mut b = PermissionSet::new();
    b.grant(Permission::EditDocument);
    b.grant(Permission::DeleteDocument);

    let union = a.union(&b);
    assert!(union.has(Permission::ViewDocument));
    assert!(union.has(Permission::EditDocument));
    assert!(union.has(Permission::DeleteDocument));

    let intersection = a.intersection(&b);
    assert!(!intersection.has(Permission::ViewDocument));
    assert!(intersection.has(Permission::EditDocument));
    assert!(!intersection.has(Permission::DeleteDocument));
}

// ── ACL Direct Tests ────────────────────────────────────────────────

#[test]
fn acl_link_sharing() {
    let owner = UserId::new();
    let resource = Uuid::new_v4();
    let mut acl = AccessControlList::new(resource, owner);

    acl.set_link_sharing(Some(Role::Commenter));
    let random = UserId::new();
    assert_eq!(acl.get_role(&random), Some(Role::Commenter));

    acl.set_link_sharing(None);
    assert_eq!(acl.get_role(&random), None);
}

// ── User Store Tests ────────────────────────────────────────────────

#[test]
fn user_store_email_case_insensitive() {
    let mut store = InMemoryUserStore::new();
    let user = User::new("Alice@Test.COM", "Alice", AuthProvider::Local);
    store.create_user(user).unwrap();

    assert!(store.get_user_by_email("alice@test.com").unwrap().is_some());
    assert!(store.get_user_by_email("ALICE@TEST.COM").unwrap().is_some());
}

#[test]
fn user_store_credentials() {
    let mut store = InMemoryUserStore::new();
    let user = User::new("a@b.c", "A", AuthProvider::Local);
    let uid = user.id;
    store.create_user(user).unwrap();

    let cred = Credential::ApiKey(ApiKeyCredential::new("hash123", "pk_", "My Key"));
    store.set_credential(&uid, cred).unwrap();

    let creds = store.get_credentials(&uid).unwrap();
    assert_eq!(creds.len(), 1);
    assert!(matches!(&creds[0], Credential::ApiKey(_)));
}

// ── Role Tests ──────────────────────────────────────────────────────

#[test]
fn role_ordering() {
    assert!(Role::Viewer < Role::Commenter);
    assert!(Role::Commenter < Role::Editor);
    assert!(Role::Editor < Role::Admin);
    assert!(Role::Admin < Role::Owner);
}

#[test]
fn role_capabilities() {
    assert!(Role::Viewer.can_view());
    assert!(!Role::Viewer.can_edit());
    assert!(Role::Editor.can_edit());
    assert!(!Role::Editor.can_moderate());
    assert!(Role::Admin.can_moderate());
    assert!(Role::Owner.is_owner());
}

// ── Session Tests ───────────────────────────────────────────────────

#[test]
fn session_metadata() {
    let session = Session::with_metadata(
        UserId::new(),
        3600,
        Some("192.168.1.1".into()),
        Some("Mozilla/5.0".into()),
        Some("fingerprint-abc".into()),
    );
    assert_eq!(session.ip_address.as_deref(), Some("192.168.1.1"));
    assert_eq!(session.user_agent.as_deref(), Some("Mozilla/5.0"));
    assert_eq!(session.device_fingerprint.as_deref(), Some("fingerprint-abc"));
}

#[test]
fn session_extend() {
    let mut session = Session::new(UserId::new(), 3600);
    let original_expiry = session.expires_at;
    session.extend(7200);
    assert!(session.expires_at > original_expiry);
}

// ── Token Claims Tests ──────────────────────────────────────────────

#[test]
fn token_claims_document_access() {
    let user_id = UserId::new();
    let doc = Uuid::new_v4();
    let claims = TokenClaims::new(user_id, "Alice", "alice@test.com", Role::Editor)
        .with_documents(vec![doc]);
    assert!(claims.can_access_document(&doc));
    assert!(!claims.can_access_document(&Uuid::new_v4()));
}

#[test]
fn token_claims_permissions() {
    let user_id = UserId::new();
    let claims = TokenClaims::new(user_id, "Alice", "alice@test.com", Role::Editor)
        .with_permissions(PermissionSet::for_role(Role::Editor));
    assert!(claims.has_permission(Permission::EditDocument));
    assert!(!claims.has_permission(Permission::DeleteDocument));
}

// ── Audit Tests ─────────────────────────────────────────────────────

#[test]
fn audit_filter_security_events() {
    let mut log = InMemoryAuditLog::new();
    let user = UserId::new();

    log.log(AuditEntry::new(user, AuditAction::Login, ResourceType::Session, Uuid::new_v4())).unwrap();
    log.log(AuditEntry::new(user, AuditAction::UserUpdated, ResourceType::User, Uuid::new_v4())).unwrap();
    log.log(AuditEntry::new(user, AuditAction::LoginFailed, ResourceType::Session, Uuid::new_v4())).unwrap();
    log.log(AuditEntry::new(user, AuditAction::PermissionGranted, ResourceType::Permission, Uuid::new_v4())).unwrap();

    let filter = AuditFilter::security_events();
    let results = log.query(&filter).unwrap();
    // Login, LoginFailed, PermissionGranted are security events
    assert_eq!(results.len(), 3);
}

#[test]
fn audit_action_labels() {
    assert_eq!(AuditAction::Login.label(), "login");
    assert_eq!(AuditAction::UserCreated.label(), "user_created");
    assert_eq!(AuditAction::PermissionGranted.label(), "permission_granted");
}

// ── OAuth Config Tests ──────────────────────────────────────────────

#[test]
fn oauth_config_presets() {
    let google = OAuthConfig::google("client_id", "secret", "https://app/callback");
    assert_eq!(google.provider, AuthProvider::Google);
    assert!(google.auth_url.contains("google"));

    let github = OAuthConfig::github("client_id", "secret", "https://app/callback");
    assert_eq!(github.provider, AuthProvider::GitHub);
    assert!(github.auth_url.contains("github"));

    let ms = OAuthConfig::microsoft("client_id", "secret", "https://app/callback", "common");
    assert_eq!(ms.provider, AuthProvider::Microsoft);
    assert!(ms.auth_url.contains("microsoft"));
}

// ── Error Display Tests ─────────────────────────────────────────────

#[test]
fn error_display() {
    let err = IdentityError::UserNotFound(UserId::new());
    assert!(err.to_string().contains("user not found"));

    let err = IdentityError::PermissionDenied("test".into());
    assert!(err.to_string().contains("permission denied"));
}

// ── Serialization Round-Trip ────────────────────────────────────────

#[test]
fn user_serialization_roundtrip() {
    let user = User::new("alice@test.com", "Alice", AuthProvider::Google);
    let json = serde_json::to_string(&user).unwrap();
    let deserialized: User = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.id, user.id);
    assert_eq!(deserialized.email, user.email);
    assert_eq!(deserialized.display_name, user.display_name);
}

#[test]
fn role_serialization_roundtrip() {
    for role in Role::all() {
        let json = serde_json::to_string(&role).unwrap();
        let deserialized: Role = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, *role);
    }
}

#[test]
fn permission_set_serialization_roundtrip() {
    let set = PermissionSet::for_role(Role::Editor);
    let json = serde_json::to_string(&set).unwrap();
    let deserialized: PermissionSet = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, set);
}
