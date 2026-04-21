// logos-desktop/src/session_state.rs
//
//! Runtime session state — tracks which server the user is currently connected
//! to and which company / project are active.
//!
//! Pure-data; no `desktop-ui` deps required.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── DesktopScreen ─────────────────────────────────────────────────────────────

/// Which top-level screen the desktop client is showing.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum DesktopScreen {
    /// Prompt user for server URL.
    #[default]
    ServerConnect,
    /// Login form.
    Login,
    /// Company selection hub.
    CompanyHub,
    /// Project browser for the selected company.
    ProjectBrowser,
    /// Canvas / editor for the active project.
    Editor,
    /// Admin panel (admin-only).
    AdminPanel,
}

// ── SessionState ──────────────────────────────────────────────────────────────

/// The live runtime session bound to one server.
#[derive(Debug, Clone)]
pub struct SessionState {
    /// Server base URL (normalised, no trailing slash).
    pub server_url:     String,
    /// Bearer token for API / WebSocket calls.
    pub token:          String,
    /// Logged-in user's id.
    pub user_id:        Uuid,
    /// Display name.
    pub display_name:   String,
    /// Whether the logged-in user has server-admin rights.
    pub is_admin:       bool,
    /// Which company is currently selected (hub → browser).
    pub active_company: Option<Uuid>,
    /// Which project is currently open in the editor.
    pub active_project: Option<Uuid>,
    /// Current screen being rendered.
    pub screen:         DesktopScreen,
}

impl SessionState {
    pub fn new(
        server_url:   impl Into<String>,
        token:        impl Into<String>,
        user_id:      Uuid,
        display_name: impl Into<String>,
        is_admin:     bool,
    ) -> Self {
        Self {
            server_url:     server_url.into(),
            token:          token.into(),
            user_id,
            display_name:   display_name.into(),
            is_admin,
            active_company: None,
            active_project: None,
            screen:         DesktopScreen::CompanyHub,
        }
    }

    /// Navigate to a company.
    pub fn select_company(&mut self, company_id: Uuid) {
        self.active_company = Some(company_id);
        self.active_project = None;
        self.screen         = DesktopScreen::ProjectBrowser;
    }

    /// Navigate back to the company hub.
    pub fn back_to_hub(&mut self) {
        self.active_company = None;
        self.active_project = None;
        self.screen         = DesktopScreen::CompanyHub;
    }

    /// Open a project in the editor.
    pub fn open_project(&mut self, project_id: Uuid) {
        self.active_project = Some(project_id);
        self.screen         = DesktopScreen::Editor;
    }

    /// Close the editor and return to the project browser.
    pub fn close_project(&mut self) {
        self.active_project = None;
        self.screen         = DesktopScreen::ProjectBrowser;
    }

    /// Navigate to the admin panel (only meaningful when `is_admin` is true).
    pub fn open_admin(&mut self) {
        self.screen = DesktopScreen::AdminPanel;
    }

    /// Log out — clears token and returns to login screen.
    /// Returns the server URL so the caller can remove it from the token store.
    pub fn logout(&mut self) -> String {
        self.token.clear();
        self.active_company = None;
        self.active_project = None;
        self.screen         = DesktopScreen::Login;
        self.server_url.clone()
    }

    pub fn auth_header(&self) -> String {
        format!("Bearer {}", self.token)
    }
}

// ── AppSession ────────────────────────────────────────────────────────────────

/// The top-level application holder — either no session (startup) or an active
/// session.
#[derive(Debug, Default)]
pub struct AppSession {
    pub session: Option<SessionState>,
}

impl AppSession {
    pub fn new() -> Self { Self::default() }

    pub fn is_connected(&self) -> bool { self.session.is_some() }

    pub fn login(&mut self, state: SessionState) {
        self.session = Some(state);
    }

    pub fn logout(&mut self) {
        if let Some(ref mut s) = self.session {
            s.logout();
        }
    }

    pub fn screen(&self) -> DesktopScreen {
        self.session.as_ref()
            .map(|s| s.screen.clone())
            .unwrap_or(DesktopScreen::ServerConnect)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests (SS-01 … SS-10)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_session() -> SessionState {
        SessionState::new("https://s.local", "tok", Uuid::new_v4(), "Alice", false)
    }

    // SS-01: new session starts at CompanyHub
    #[test]
    fn ss_01_starts_at_company_hub() {
        let s = make_session();
        assert_eq!(s.screen, DesktopScreen::CompanyHub);
    }

    // SS-02: select_company navigates to ProjectBrowser
    #[test]
    fn ss_02_select_company() {
        let mut s = make_session();
        let cid = Uuid::new_v4();
        s.select_company(cid);
        assert_eq!(s.active_company, Some(cid));
        assert_eq!(s.screen, DesktopScreen::ProjectBrowser);
    }

    // SS-03: back_to_hub clears company and project
    #[test]
    fn ss_03_back_to_hub() {
        let mut s = make_session();
        s.select_company(Uuid::new_v4());
        s.back_to_hub();
        assert!(s.active_company.is_none());
        assert_eq!(s.screen, DesktopScreen::CompanyHub);
    }

    // SS-04: open_project navigates to Editor
    #[test]
    fn ss_04_open_project() {
        let mut s = make_session();
        let pid = Uuid::new_v4();
        s.open_project(pid);
        assert_eq!(s.active_project, Some(pid));
        assert_eq!(s.screen, DesktopScreen::Editor);
    }

    // SS-05: close_project returns to ProjectBrowser
    #[test]
    fn ss_05_close_project() {
        let mut s = make_session();
        s.open_project(Uuid::new_v4());
        s.close_project();
        assert!(s.active_project.is_none());
        assert_eq!(s.screen, DesktopScreen::ProjectBrowser);
    }

    // SS-06: logout clears token and goes to Login
    #[test]
    fn ss_06_logout() {
        let mut s = make_session();
        s.logout();
        assert!(s.token.is_empty());
        assert_eq!(s.screen, DesktopScreen::Login);
    }

    // SS-07: auth_header has correct prefix
    #[test]
    fn ss_07_auth_header() {
        let s = make_session();
        assert!(s.auth_header().starts_with("Bearer "));
    }

    // SS-08: AppSession::screen returns ServerConnect when no session
    #[test]
    fn ss_08_app_session_no_session() {
        let a = AppSession::new();
        assert!(!a.is_connected());
        assert_eq!(a.screen(), DesktopScreen::ServerConnect);
    }

    // SS-09: AppSession::login records session
    #[test]
    fn ss_09_app_session_login() {
        let mut a = AppSession::new();
        a.login(make_session());
        assert!(a.is_connected());
        assert_eq!(a.screen(), DesktopScreen::CompanyHub);
    }

    // SS-10: open_admin navigates to AdminPanel
    #[test]
    fn ss_10_open_admin() {
        let mut s = make_session();
        s.open_admin();
        assert_eq!(s.screen, DesktopScreen::AdminPanel);
    }
}
