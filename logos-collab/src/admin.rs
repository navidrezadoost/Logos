// logos-collab/src/admin.rs
//
//! Server admin engine — first-run setup, user CRUD, approval workflow.
//!
//! On first start an admin user is created with a known password.  The admin
//! can then add/delete/approve users and manage company membership without
//! going through the self-registration flow.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::user::{User, UserError, UserStore};

// ── AdminError ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdminError {
    User(UserError),
    NotAdmin,
    AlreadyInitialized,
    NotInitialized,
}

impl From<UserError> for AdminError {
    fn from(e: UserError) -> Self { Self::User(e) }
}

impl std::fmt::Display for AdminError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::User(e)           => write!(f, "user error: {e}"),
            Self::NotAdmin          => write!(f, "caller does not have admin role"),
            Self::AlreadyInitialized=> write!(f, "server already initialized"),
            Self::NotInitialized    => write!(f, "server not yet initialized"),
        }
    }
}

// ── AdminRole ─────────────────────────────────────────────────────────────────

/// Server-wide admin flag stored alongside the user record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServerRole {
    /// Regular user — can only access their own companies/projects.
    User,
    /// Server administrator — full access to all users and companies.
    Admin,
}

// ── CreateUserRequest ─────────────────────────────────────────────────────────

/// Parameters for admin-initiated user creation.
#[derive(Debug, Clone)]
pub struct CreateUserRequest {
    pub username:   String,
    pub email:      String,
    pub password:   String,
    pub first_name: String,
    pub last_name:  String,
    pub job_title:  Option<String>,
    pub avatar_url: Option<String>,
    /// If `true` the account is pre-approved; if `false` the user must be
    /// approved via `AdminEngine::approve_user`.
    pub approved:   bool,
}

// ── AdminEngine ───────────────────────────────────────────────────────────────

/// Top-level admin engine that wraps a `UserStore` and adds server-role
/// management and a first-run initialization flow.
pub struct AdminEngine {
    pub store:        UserStore,
    /// Maps user_id → ServerRole.
    server_roles:     std::collections::HashMap<Uuid, ServerRole>,
    admin_user_id:    Option<Uuid>,
}

impl AdminEngine {
    pub fn new() -> Self {
        Self {
            store:         UserStore::new(),
            server_roles:  Default::default(),
            admin_user_id: None,
        }
    }

    // ── Initialization ────────────────────────────────────────────────────

    /// Create the first admin account.  Can only be called once.
    pub fn initialize(
        &mut self,
        username:   &str,
        email:      &str,
        password:   &str,
        first_name: &str,
        last_name:  &str,
    ) -> Result<Uuid, AdminError> {
        if self.admin_user_id.is_some() {
            return Err(AdminError::AlreadyInitialized);
        }
        let user = User::new(username, email, password, first_name, last_name, true)?;
        let id   = self.store.create_user(user)?;
        self.server_roles.insert(id, ServerRole::Admin);
        self.admin_user_id = Some(id);
        Ok(id)
    }

    pub fn is_initialized(&self) -> bool { self.admin_user_id.is_some() }

    // ── Admin user management ─────────────────────────────────────────────

    /// Admin creates a new user directly (no email verification).
    pub fn create_user(
        &mut self,
        actor_id: Uuid,
        req:      CreateUserRequest,
    ) -> Result<Uuid, AdminError> {
        self.require_admin(actor_id)?;
        let mut user = User::new(
            &req.username, &req.email, &req.password,
            &req.first_name, &req.last_name, req.approved,
        )?;
        if let Some(t) = req.job_title  { user.job_title  = Some(t); }
        if let Some(a) = req.avatar_url { user.avatar_url = Some(a); }
        let id = self.store.create_user(user)?;
        self.server_roles.insert(id, ServerRole::User);
        Ok(id)
    }

    /// Delete any user (admin only).
    pub fn delete_user(&mut self, actor_id: Uuid, target_id: Uuid) -> Result<(), AdminError> {
        self.require_admin(actor_id)?;
        self.store.delete_user(target_id)?;
        self.server_roles.remove(&target_id);
        Ok(())
    }

    /// Approve a self-registered (pending) user.
    pub fn approve_user(&mut self, actor_id: Uuid, target_id: Uuid) -> Result<(), AdminError> {
        self.require_admin(actor_id)?;
        self.store.approve_user(target_id)?;
        Ok(())
    }

    /// Grant server admin role to a user.
    pub fn grant_admin(&mut self, actor_id: Uuid, target_id: Uuid) -> Result<(), AdminError> {
        self.require_admin(actor_id)?;
        if self.store.get_user(target_id).is_none() {
            return Err(AdminError::User(UserError::UserNotFound));
        }
        self.server_roles.insert(target_id, ServerRole::Admin);
        Ok(())
    }

    /// Revoke server admin role.
    pub fn revoke_admin(&mut self, actor_id: Uuid, target_id: Uuid) -> Result<(), AdminError> {
        self.require_admin(actor_id)?;
        self.server_roles.insert(target_id, ServerRole::User);
        Ok(())
    }

    pub fn server_role(&self, user_id: Uuid) -> ServerRole {
        self.server_roles.get(&user_id).cloned().unwrap_or(ServerRole::User)
    }

    pub fn is_admin(&self, user_id: Uuid) -> bool {
        matches!(self.server_role(user_id), ServerRole::Admin)
    }

    pub fn list_users(&self) -> Vec<&User> {
        self.store.list_users()
    }

    pub fn pending_users(&self) -> Vec<&User> {
        self.store.list_users().into_iter().filter(|u| !u.approved).collect()
    }

    // ── Self-registration flow ────────────────────────────────────────────

    /// A user self-registers.  Account is created with `approved = false` and
    /// must be approved by an admin before login is possible.
    pub fn self_register(
        &mut self,
        username:   &str,
        email:      &str,
        password:   &str,
        first_name: &str,
        last_name:  &str,
    ) -> Result<Uuid, AdminError> {
        let user = User::new(username, email, password, first_name, last_name, false)?;
        let id   = self.store.create_user(user)?;
        self.server_roles.insert(id, ServerRole::User);
        Ok(id)
    }

    fn require_admin(&self, actor_id: Uuid) -> Result<(), AdminError> {
        if self.is_admin(actor_id) { Ok(()) } else { Err(AdminError::NotAdmin) }
    }
}

impl Default for AdminEngine {
    fn default() -> Self { Self::new() }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn init_engine() -> (AdminEngine, Uuid) {
        let mut e = AdminEngine::new();
        let id = e.initialize("admin", "admin@example.com", "adminpass", "Admin", "User").unwrap();
        (e, id)
    }

    fn req(username: &str) -> CreateUserRequest {
        CreateUserRequest {
            username:   username.into(),
            email:      format!("{username}@example.com"),
            password:   "pass123".into(),
            first_name: "First".into(),
            last_name:  "Last".into(),
            job_title:  None,
            avatar_url: None,
            approved:   true,
        }
    }

    // A-01: initialize creates an admin user.
    #[test]
    fn a_01_initialize_creates_admin() {
        let (e, id) = init_engine();
        assert!(e.is_admin(id));
    }

    // A-02: second initialize call returns AlreadyInitialized.
    #[test]
    fn a_02_double_initialize_rejected() {
        let (mut e, _) = init_engine();
        let err = e.initialize("x", "x@x.com", "pass", "X", "X").unwrap_err();
        assert_eq!(err, AdminError::AlreadyInitialized);
    }

    // A-03: Admin can create a user.
    #[test]
    fn a_03_admin_creates_user() {
        let (mut e, aid) = init_engine();
        let uid = e.create_user(aid, req("bob")).unwrap();
        assert!(e.store.get_user(uid).is_some());
    }

    // A-04: Non-admin cannot create a user.
    #[test]
    fn a_04_nonadmin_cannot_create_user() {
        let (mut e, aid) = init_engine();
        let uid = e.create_user(aid, req("carol")).unwrap();
        let err = e.create_user(uid, req("dave")).unwrap_err();
        assert_eq!(err, AdminError::NotAdmin);
    }

    // A-05: Admin can delete a user.
    #[test]
    fn a_05_admin_deletes_user() {
        let (mut e, aid) = init_engine();
        let uid = e.create_user(aid, req("eve")).unwrap();
        e.delete_user(aid, uid).unwrap();
        assert!(e.store.get_user(uid).is_none());
    }

    // A-06: Admin can approve a pending self-registered user.
    #[test]
    fn a_06_approve_pending_user() {
        let (mut e, aid) = init_engine();
        let uid = e.self_register("grace", "grace@x.com", "p", "G", "H").unwrap();
        assert!(!e.store.get_user(uid).unwrap().approved);
        e.approve_user(aid, uid).unwrap();
        assert!(e.store.get_user(uid).unwrap().approved);
    }

    // A-07: Self-registered user cannot log in before approval.
    #[test]
    fn a_07_unapproved_cannot_login() {
        let (mut e, _) = init_engine();
        e.self_register("henry", "henry@x.com", "p", "H", "I").unwrap();
        let err = e.store.login("henry", "p").unwrap_err();
        assert_eq!(err, UserError::NotApproved);
    }

    // A-08: After approval, self-registered user can log in.
    #[test]
    fn a_08_approved_can_login() {
        let (mut e, aid) = init_engine();
        let uid = e.self_register("iris", "iris@x.com", "p", "I", "J").unwrap();
        e.approve_user(aid, uid).unwrap();
        e.store.login("iris", "p").unwrap();
    }

    // A-09: Admin can grant and revoke admin role.
    #[test]
    fn a_09_grant_revoke_admin() {
        let (mut e, aid) = init_engine();
        let uid = e.create_user(aid, req("jack")).unwrap();
        e.grant_admin(aid, uid).unwrap();
        assert!(e.is_admin(uid));
        e.revoke_admin(aid, uid).unwrap();
        assert!(!e.is_admin(uid));
    }

    // A-10: pending_users only returns unapproved users.
    #[test]
    fn a_10_pending_users_list() {
        let (mut e, aid) = init_engine();
        e.self_register("kate", "k@x.com", "p", "K", "L").unwrap();
        e.self_register("leo", "l@x.com", "p", "L", "M").unwrap();
        e.create_user(aid, req("approved_user")).unwrap();
        let pending = e.pending_users();
        assert_eq!(pending.len(), 2);
        assert!(pending.iter().all(|u| !u.approved));
    }

    // A-11: Non-admin cannot delete a user.
    #[test]
    fn a_11_nonadmin_cannot_delete() {
        let (mut e, aid) = init_engine();
        let uid = e.create_user(aid, req("mike")).unwrap();
        let err = e.delete_user(uid, aid).unwrap_err();
        assert_eq!(err, AdminError::NotAdmin);
    }

    // A-12: Admin can create user with job title and avatar.
    #[test]
    fn a_12_create_user_with_details() {
        let (mut e, aid) = init_engine();
        let mut r = req("nancy");
        r.job_title  = Some("Designer".into());
        r.avatar_url = Some("https://example.com/avatar.png".into());
        let uid = e.create_user(aid, r).unwrap();
        let u = e.store.get_user(uid).unwrap();
        assert_eq!(u.job_title.as_deref(), Some("Designer"));
        assert!(u.avatar_url.is_some());
    }

    // A-13: is_initialized returns true after initialize.
    #[test]
    fn a_13_is_initialized() {
        let mut e = AdminEngine::new();
        assert!(!e.is_initialized());
        e.initialize("a", "a@a.com", "p", "A", "B").unwrap();
        assert!(e.is_initialized());
    }

    // A-14: grant_admin on unknown user returns error.
    #[test]
    fn a_14_grant_admin_unknown_user() {
        let (mut e, aid) = init_engine();
        let err = e.grant_admin(aid, Uuid::new_v4()).unwrap_err();
        matches!(err, AdminError::User(UserError::UserNotFound));
    }

    // A-15: list_users returns all users including admin.
    #[test]
    fn a_15_list_users() {
        let (mut e, aid) = init_engine();
        e.create_user(aid, req("olive")).unwrap();
        let users = e.list_users();
        assert_eq!(users.len(), 2); // admin + olive
    }
}
