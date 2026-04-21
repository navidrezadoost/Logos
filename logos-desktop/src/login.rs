// logos-desktop/src/login.rs
//
//! Pure-data state for the login screen and token storage.
//!
//! No wgpu / GTK / winit — safe to test without `desktop-ui`.
//!
//! Covers:
//! - `LoginFormState`  — username/email + password fields + validation
//! - `LoginResult`     — outcome of an auth attempt
//! - `StoredCredentials` — in-memory representation of a saved session
//! - `TokenStore`      — per-server session token registry

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── LoginFormState ────────────────────────────────────────────────────────────

/// State of the login form (username/email + password).
#[derive(Debug, Clone, Default)]
pub struct LoginFormState {
    /// User typed login (username or email).
    pub login_input:    String,
    /// User typed password (kept in memory; not serialised).
    pub password_input: String,
    /// Validation error for the login field.
    pub login_error:    Option<String>,
    /// Validation error for the password field.
    pub password_error: Option<String>,
    /// General error returned by the server (e.g. wrong credentials).
    pub server_error:   Option<String>,
    /// `true` while a login request is in flight.
    pub logging_in:     bool,
    /// `true` when login succeeded and the screen should transition away.
    pub success:        bool,
    /// `true` to show the password as plain text (eye button).
    pub show_password:  bool,
}

impl LoginFormState {
    pub fn new() -> Self { Self::default() }

    /// Validate fields — returns `true` if both fields are non-empty.
    pub fn validate(&mut self) -> bool {
        self.login_error    = None;
        self.password_error = None;
        self.server_error   = None;
        let mut ok = true;

        if self.login_input.trim().is_empty() {
            self.login_error = Some("Username or email is required".into());
            ok = false;
        }
        if self.password_input.is_empty() {
            self.password_error = Some("Password is required".into());
            ok = false;
        }
        ok
    }

    pub fn begin_login(&mut self) {
        self.logging_in   = true;
        self.server_error = None;
    }

    pub fn on_login_ok(&mut self) {
        self.logging_in = false;
        self.success    = true;
        self.password_input.clear(); // don't hold plaintext longer than needed
    }

    pub fn on_login_failed(&mut self, msg: impl Into<String>) {
        self.logging_in   = false;
        self.server_error = Some(msg.into());
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn toggle_show_password(&mut self) {
        self.show_password = !self.show_password;
    }

    pub fn has_any_error(&self) -> bool {
        self.login_error.is_some()
        || self.password_error.is_some()
        || self.server_error.is_some()
    }
}

// ── LoginResult ───────────────────────────────────────────────────────────────

/// Outcome from a login call (populated by the network layer, consumed by the
/// login form).
#[derive(Debug, Clone)]
pub struct LoginResult {
    pub token:       String,
    pub user_id:     Uuid,
    pub username:    String,
    pub display_name: String,
    pub is_admin:    bool,
}

// ── StoredCredentials ─────────────────────────────────────────────────────────

/// A saved session token for one (server_url, user) pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredCredentials {
    pub server_url:   String,
    pub user_id:      Uuid,
    pub username:     String,
    pub display_name: String,
    pub token:        String,
    pub is_admin:     bool,
}

// ── TokenStore ────────────────────────────────────────────────────────────────

/// In-memory registry of active sessions keyed by server URL.
///
/// This is the runtime counterpart to `LocalClientConfig`.  The desktop app
/// holds one `TokenStore` for its lifetime; the OS keyring integration layer
/// populates it on start-up.
#[derive(Debug, Clone, Default)]
pub struct TokenStore {
    entries: Vec<StoredCredentials>,
}

impl TokenStore {
    pub fn new() -> Self { Self::default() }

    /// Store (or replace) credentials for a server.
    pub fn store(&mut self, creds: StoredCredentials) {
        if let Some(existing) = self.entries.iter_mut()
            .find(|c| c.server_url == creds.server_url)
        {
            *existing = creds;
        } else {
            self.entries.push(creds);
        }
    }

    pub fn get(&self, server_url: &str) -> Option<&StoredCredentials> {
        self.entries.iter().find(|c| c.server_url == server_url)
    }

    pub fn remove(&mut self, server_url: &str) {
        self.entries.retain(|c| c.server_url != server_url);
    }

    pub fn count(&self) -> usize { self.entries.len() }

    pub fn is_logged_in(&self, server_url: &str) -> bool {
        self.get(server_url).is_some()
    }

    /// All servers for which a session exists.
    pub fn server_urls(&self) -> Vec<&str> {
        self.entries.iter().map(|c| c.server_url.as_str()).collect()
    }

    /// Active token for a server, if any.
    pub fn token(&self, server_url: &str) -> Option<&str> {
        self.get(server_url).map(|c| c.token.as_str())
    }

    pub fn is_admin(&self, server_url: &str) -> bool {
        self.get(server_url).map(|c| c.is_admin).unwrap_or(false)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests (LG-01 … LG-15)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_creds(server: &str) -> StoredCredentials {
        StoredCredentials {
            server_url:   server.into(),
            user_id:      Uuid::new_v4(),
            username:     "alice".into(),
            display_name: "Alice".into(),
            token:        "tok-123".into(),
            is_admin:     false,
        }
    }

    // LG-01: validate returns false when both fields empty
    #[test]
    fn lg_01_validate_empty() {
        let mut f = LoginFormState::new();
        assert!(!f.validate());
        assert!(f.login_error.is_some());
        assert!(f.password_error.is_some());
    }

    // LG-02: validate passes when both fields filled
    #[test]
    fn lg_02_validate_ok() {
        let mut f = LoginFormState { login_input: "alice".into(), password_input: "secret".into(), ..Default::default() };
        assert!(f.validate());
        assert!(!f.has_any_error());
    }

    // LG-03: validate error only on empty password
    #[test]
    fn lg_03_validate_missing_password() {
        let mut f = LoginFormState { login_input: "alice".into(), ..Default::default() };
        assert!(!f.validate());
        assert!(f.login_error.is_none());
        assert!(f.password_error.is_some());
    }

    // LG-04: begin_login sets logging_in and clears server_error
    #[test]
    fn lg_04_begin_login() {
        let mut f = LoginFormState::new();
        f.server_error = Some("old error".into());
        f.begin_login();
        assert!(f.logging_in);
        assert!(f.server_error.is_none());
    }

    // LG-05: on_login_ok sets success and clears password
    #[test]
    fn lg_05_on_login_ok() {
        let mut f = LoginFormState { password_input: "s3cr3t".into(), ..Default::default() };
        f.on_login_ok();
        assert!(f.success);
        assert!(!f.logging_in);
        assert!(f.password_input.is_empty());
    }

    // LG-06: on_login_failed sets server_error
    #[test]
    fn lg_06_on_login_failed() {
        let mut f = LoginFormState::new();
        f.on_login_failed("Invalid credentials");
        assert!(!f.logging_in);
        assert!(f.server_error.is_some());
    }

    // LG-07: toggle_show_password flips the flag
    #[test]
    fn lg_07_toggle_show_password() {
        let mut f = LoginFormState::new();
        assert!(!f.show_password);
        f.toggle_show_password();
        assert!(f.show_password);
        f.toggle_show_password();
        assert!(!f.show_password);
    }

    // LG-08: reset clears all fields
    #[test]
    fn lg_08_reset() {
        let mut f = LoginFormState::new();
        f.login_input = "alice".into();
        f.password_input = "pass".into();
        f.server_error = Some("err".into());
        f.reset();
        assert!(f.login_input.is_empty());
        assert!(f.password_input.is_empty());
        assert!(f.server_error.is_none());
    }

    // LG-09: TokenStore starts empty
    #[test]
    fn lg_09_token_store_empty() {
        let s = TokenStore::new();
        assert_eq!(s.count(), 0);
    }

    // LG-10: store and retrieve credentials
    #[test]
    fn lg_10_store_retrieve() {
        let mut s = TokenStore::new();
        s.store(sample_creds("https://s1.local"));
        assert!(s.is_logged_in("https://s1.local"));
        assert_eq!(s.token("https://s1.local"), Some("tok-123"));
    }

    // LG-11: store replaces existing entry for same server
    #[test]
    fn lg_11_store_replaces() {
        let mut s = TokenStore::new();
        s.store(sample_creds("https://s1.local"));
        let mut c2 = sample_creds("https://s1.local");
        c2.token = "tok-new".into();
        s.store(c2);
        assert_eq!(s.count(), 1);
        assert_eq!(s.token("https://s1.local"), Some("tok-new"));
    }

    // LG-12: remove logs out of a server
    #[test]
    fn lg_12_remove() {
        let mut s = TokenStore::new();
        s.store(sample_creds("https://s1.local"));
        s.remove("https://s1.local");
        assert!(!s.is_logged_in("https://s1.local"));
        assert_eq!(s.count(), 0);
    }

    // LG-13: server_urls returns all known servers
    #[test]
    fn lg_13_server_urls() {
        let mut s = TokenStore::new();
        s.store(sample_creds("https://s1.local"));
        s.store(sample_creds("https://s2.local"));
        let urls = s.server_urls();
        assert_eq!(urls.len(), 2);
        assert!(urls.contains(&"https://s1.local"));
    }

    // LG-14: is_admin returns false for non-admin
    #[test]
    fn lg_14_not_admin() {
        let mut s = TokenStore::new();
        s.store(sample_creds("https://s1.local"));
        assert!(!s.is_admin("https://s1.local"));
    }

    // LG-15: is_admin returns true when credentials mark admin
    #[test]
    fn lg_15_is_admin() {
        let mut s = TokenStore::new();
        let mut c = sample_creds("https://s1.local");
        c.is_admin = true;
        s.store(c);
        assert!(s.is_admin("https://s1.local"));
    }
}
