// logos-desktop/src/admin_panel.rs
//
//! Pure-data state for the Admin Panel screen.
//!
//! Covers user management (list / create / delete / approve) and company +
//! project overview tables.
//!
//! No `desktop-ui` deps required.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── AdminTab ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AdminTab {
    #[default]
    Users,
    Companies,
    Projects,
}

// ── UserRow ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRow {
    pub id:           Uuid,
    pub username:     String,
    pub email:        String,
    pub display_name: String,
    pub is_admin:     bool,
    pub approved:     bool,
    pub created_at:   u64,
}

// ── CreateUserForm ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct CreateUserForm {
    pub username:       String,
    pub email:          String,
    pub password:       String,
    pub first_name:     String,
    pub last_name:      String,
    pub auto_approve:   bool,
    pub username_error: Option<String>,
    pub email_error:    Option<String>,
    pub password_error: Option<String>,
    pub server_error:   Option<String>,
    pub submitting:     bool,
    pub success:        Option<Uuid>,
}

impl CreateUserForm {
    pub fn validate(&mut self) -> bool {
        self.username_error = None;
        self.email_error    = None;
        self.password_error = None;
        self.server_error   = None;
        let mut ok = true;

        if self.username.trim().is_empty() {
            self.username_error = Some("Username is required".into());
            ok = false;
        }
        if !self.email.contains('@') || self.email.trim().is_empty() {
            self.email_error = Some("A valid email address is required".into());
            ok = false;
        }
        if self.password.len() < 8 {
            self.password_error = Some("Password must be at least 8 characters".into());
            ok = false;
        }
        ok
    }

    pub fn begin_submit(&mut self) {
        self.submitting   = true;
        self.server_error = None;
    }

    pub fn on_ok(&mut self, id: Uuid) {
        self.submitting = false;
        self.success    = Some(id);
        self.password.clear();
    }

    pub fn on_error(&mut self, msg: impl Into<String>) {
        self.submitting   = false;
        self.server_error = Some(msg.into());
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

// ── ConfirmDelete ─────────────────────────────────────────────────────────────

/// State for the "are you sure?" delete confirmation dialog.
#[derive(Debug, Clone, Default)]
pub struct ConfirmDelete {
    pub target_id:   Option<Uuid>,
    pub target_name: String,
    pub pending:     bool,
    pub error:       Option<String>,
}

impl ConfirmDelete {
    pub fn open(&mut self, id: Uuid, name: impl Into<String>) {
        self.target_id   = Some(id);
        self.target_name = name.into();
        self.pending     = false;
        self.error       = None;
    }

    pub fn close(&mut self) {
        *self = Self::default();
    }

    pub fn is_open(&self) -> bool {
        self.target_id.is_some()
    }
}

// ── AdminPanelState ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AdminPanelState {
    pub active_tab:   AdminTab,
    // ── Users tab ──
    pub users:        Vec<UserRow>,
    pub users_filter: String,
    pub users_loading: bool,
    pub users_error:  Option<String>,
    pub show_create_user: bool,
    pub create_user_form: CreateUserForm,
    pub confirm_delete:   ConfirmDelete,
    // ── action feedback ──
    pub action_message: Option<String>,
}

impl Default for AdminPanelState {
    fn default() -> Self {
        Self {
            active_tab:       AdminTab::Users,
            users:            Vec::new(),
            users_filter:     String::new(),
            users_loading:    false,
            users_error:      None,
            show_create_user: false,
            create_user_form: CreateUserForm::default(),
            confirm_delete:   ConfirmDelete::default(),
            action_message:   None,
        }
    }
}

impl AdminPanelState {
    pub fn new() -> Self { Self::default() }

    // ── load users ──

    pub fn begin_load_users(&mut self) {
        self.users_loading = true;
        self.users_error   = None;
    }

    pub fn on_users_loaded(&mut self, users: Vec<UserRow>) {
        self.users_loading = false;
        self.users         = users;
    }

    pub fn on_users_error(&mut self, msg: impl Into<String>) {
        self.users_loading = false;
        self.users_error   = Some(msg.into());
    }

    // ── filtered view ──

    pub fn visible_users(&self) -> Vec<&UserRow> {
        let f = self.users_filter.to_lowercase();
        let mut v: Vec<&UserRow> = self.users.iter()
            .filter(|u| {
                f.is_empty()
                || u.username.to_lowercase().contains(&f)
                || u.email.to_lowercase().contains(&f)
                || u.display_name.to_lowercase().contains(&f)
            })
            .collect();
        v.sort_by(|a, b| a.username.cmp(&b.username));
        v
    }

    pub fn pending_users(&self) -> Vec<&UserRow> {
        self.users.iter().filter(|u| !u.approved).collect()
    }

    // ── create form ──

    pub fn open_create_user(&mut self) {
        self.create_user_form.reset();
        self.show_create_user = true;
    }

    pub fn close_create_user(&mut self) {
        self.show_create_user = false;
    }

    pub fn add_user(&mut self, row: UserRow) {
        self.users.push(row);
        self.show_create_user = false;
        self.action_message   = Some("User created successfully".into());
    }

    // ── approve / promote / delete ──

    pub fn approve_user(&mut self, id: Uuid) {
        if let Some(u) = self.users.iter_mut().find(|u| u.id == id) {
            u.approved = true;
        }
        self.action_message = Some("User approved".into());
    }

    pub fn set_admin(&mut self, id: Uuid, is_admin: bool) {
        if let Some(u) = self.users.iter_mut().find(|u| u.id == id) {
            u.is_admin = is_admin;
        }
        self.action_message = Some(if is_admin { "Admin granted".into() } else { "Admin revoked".into() });
    }

    pub fn delete_user(&mut self, id: Uuid) {
        self.users.retain(|u| u.id != id);
        self.confirm_delete.close();
        self.action_message = Some("User deleted".into());
    }

    pub fn clear_action_message(&mut self) {
        self.action_message = None;
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests (AP-01 … AP-20)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_user(username: &str, approved: bool) -> UserRow {
        UserRow {
            id:           Uuid::new_v4(),
            username:     username.into(),
            email:        format!("{username}@example.com"),
            display_name: username.into(),
            is_admin:     false,
            approved,
            created_at:   0,
        }
    }

    // AP-01: starts with Users tab
    #[test]
    fn ap_01_starts_users_tab() {
        let a = AdminPanelState::new();
        assert_eq!(a.active_tab, AdminTab::Users);
    }

    // AP-02: on_users_loaded populates list
    #[test]
    fn ap_02_on_users_loaded() {
        let mut a = AdminPanelState::new();
        a.on_users_loaded(vec![make_user("alice", true), make_user("bob", true)]);
        assert_eq!(a.users.len(), 2);
    }

    // AP-03: on_users_error sets error
    #[test]
    fn ap_03_on_users_error() {
        let mut a = AdminPanelState::new();
        a.begin_load_users();
        a.on_users_error("timeout");
        assert!(a.users_error.is_some());
        assert!(!a.users_loading);
    }

    // AP-04: visible_users filters by username
    #[test]
    fn ap_04_visible_filter_username() {
        let mut a = AdminPanelState::new();
        a.on_users_loaded(vec![make_user("alice", true), make_user("bob", true)]);
        a.users_filter = "ali".into();
        assert_eq!(a.visible_users().len(), 1);
    }

    // AP-05: visible_users sorted alphabetically
    #[test]
    fn ap_05_visible_sorted() {
        let mut a = AdminPanelState::new();
        a.on_users_loaded(vec![make_user("zoe", true), make_user("alice", true)]);
        let v = a.visible_users();
        assert_eq!(v[0].username, "alice");
    }

    // AP-06: pending_users returns only unapproved
    #[test]
    fn ap_06_pending_users() {
        let mut a = AdminPanelState::new();
        a.on_users_loaded(vec![make_user("alice", true), make_user("bob", false)]);
        assert_eq!(a.pending_users().len(), 1);
        assert_eq!(a.pending_users()[0].username, "bob");
    }

    // AP-07: open_create_user resets form and shows dialog
    #[test]
    fn ap_07_open_create() {
        let mut a = AdminPanelState::new();
        a.create_user_form.username = "stale".into();
        a.open_create_user();
        assert!(a.show_create_user);
        assert!(a.create_user_form.username.is_empty());
    }

    // AP-08: CreateUserForm validates missing username
    #[test]
    fn ap_08_form_missing_username() {
        let mut f = CreateUserForm { email: "a@b.com".into(), password: "12345678".into(), ..Default::default() };
        assert!(!f.validate());
        assert!(f.username_error.is_some());
    }

    // AP-09: CreateUserForm validates bad email
    #[test]
    fn ap_09_form_bad_email() {
        let mut f = CreateUserForm { username: "alice".into(), email: "notanemail".into(), password: "12345678".into(), ..Default::default() };
        assert!(!f.validate());
        assert!(f.email_error.is_some());
    }

    // AP-10: CreateUserForm validates short password
    #[test]
    fn ap_10_form_short_password() {
        let mut f = CreateUserForm { username: "alice".into(), email: "a@b.com".into(), password: "short".into(), ..Default::default() };
        assert!(!f.validate());
        assert!(f.password_error.is_some());
    }

    // AP-11: CreateUserForm validates successfully
    #[test]
    fn ap_11_form_valid() {
        let mut f = CreateUserForm { username: "alice".into(), email: "a@b.com".into(), password: "supersecret".into(), ..Default::default() };
        assert!(f.validate());
    }

    // AP-12: on_ok clears password and sets success
    #[test]
    fn ap_12_form_on_ok_clears_password() {
        let mut f = CreateUserForm { username: "a".into(), email: "a@b.com".into(), password: "longenough".into(), ..Default::default() };
        f.begin_submit();
        let id = Uuid::new_v4();
        f.on_ok(id);
        assert!(f.password.is_empty());
        assert_eq!(f.success, Some(id));
    }

    // AP-13: add_user appends row and sets action_message
    #[test]
    fn ap_13_add_user() {
        let mut a = AdminPanelState::new();
        a.show_create_user = true;
        let u = make_user("carol", true);
        let uid = u.id;
        a.add_user(u);
        assert_eq!(a.users.len(), 1);
        assert!(!a.show_create_user);
        assert!(a.action_message.is_some());
        let _ = uid;
    }

    // AP-14: approve_user sets approved flag
    #[test]
    fn ap_14_approve_user() {
        let mut a = AdminPanelState::new();
        let u = make_user("dave", false);
        let id = u.id;
        a.on_users_loaded(vec![u]);
        a.approve_user(id);
        assert!(a.users.iter().find(|u| u.id == id).unwrap().approved);
    }

    // AP-15: set_admin grants admin
    #[test]
    fn ap_15_set_admin_grant() {
        let mut a = AdminPanelState::new();
        let u = make_user("eve", true);
        let id = u.id;
        a.on_users_loaded(vec![u]);
        a.set_admin(id, true);
        assert!(a.users.iter().find(|u| u.id == id).unwrap().is_admin);
    }

    // AP-16: set_admin revokes admin
    #[test]
    fn ap_16_set_admin_revoke() {
        let mut a = AdminPanelState::new();
        let mut u = make_user("frank", true); u.is_admin = true;
        let id = u.id;
        a.on_users_loaded(vec![u]);
        a.set_admin(id, false);
        assert!(!a.users.iter().find(|u| u.id == id).unwrap().is_admin);
    }

    // AP-17: delete_user removes from list
    #[test]
    fn ap_17_delete_user() {
        let mut a = AdminPanelState::new();
        let u = make_user("grace", true);
        let id = u.id;
        a.on_users_loaded(vec![u]);
        a.delete_user(id);
        assert!(a.users.is_empty());
    }

    // AP-18: ConfirmDelete::open sets target
    #[test]
    fn ap_18_confirm_delete_open() {
        let mut c = ConfirmDelete::default();
        let id = Uuid::new_v4();
        c.open(id, "grace");
        assert!(c.is_open());
        assert_eq!(c.target_id, Some(id));
    }

    // AP-19: ConfirmDelete::close clears state
    #[test]
    fn ap_19_confirm_delete_close() {
        let mut c = ConfirmDelete::default();
        c.open(Uuid::new_v4(), "test");
        c.close();
        assert!(!c.is_open());
    }

    // AP-20: clear_action_message removes message
    #[test]
    fn ap_20_clear_action_message() {
        let mut a = AdminPanelState::new();
        a.action_message = Some("done".into());
        a.clear_action_message();
        assert!(a.action_message.is_none());
    }
}
