// logos-desktop/src/server_connect.rs
//
//! Pure-data state for the "Connect to Server" dialog.
//!
//! No wgpu / GTK / winit dependency — safe to test without `desktop-ui`.
//!
//! This module owns:
//! - `ServerConnectState` — everything the dialog needs to render & act
//! - `ConnectionTestResult` — outcome of a test-connection attempt
//! - `ProxyFormState` — per-dialog proxy configuration form fields
//! - URL and proxy validation logic

use serde::{Deserialize, Serialize};

// ── Validation helpers ────────────────────────────────────────────────────────

/// Returns `true` if the string looks like a connectable server URL.
///
/// Accepts:
/// - `http://…` / `https://…` with optional port
/// - `hostname:port` short form (treated as http)
/// - bare IPv4/IPv6 with port
pub fn looks_like_server_url(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() { return false; }
    // With scheme
    if let Some(rest) = s.strip_prefix("http://").or_else(|| s.strip_prefix("https://")) {
        return !rest.is_empty();
    }
    // host:port
    if let Some(colon) = s.rfind(':') {
        let port_str = &s[colon + 1..];
        if port_str.parse::<u16>().is_ok() {
            return colon > 0;
        }
    }
    false
}

/// Normalise a user-entered server URL:
/// - strips trailing slashes
/// - prepends `http://` if no scheme is present
pub fn normalise_server_url(s: &str) -> String {
    let s = s.trim().trim_end_matches('/');
    if s.starts_with("http://") || s.starts_with("https://") {
        s.to_owned()
    } else {
        format!("http://{s}")
    }
}

/// Returns `true` if the proxy host:port string is basically valid.
pub fn looks_like_proxy_address(host: &str, port: u16) -> bool {
    !host.trim().is_empty() && port > 0
}

// ── ProxyFormState ────────────────────────────────────────────────────────────

/// Which protocol to use for the proxy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ProxyProtocol {
    #[default]
    None,
    Http,
    Https,
    Socks5,
}

/// Form state for the proxy sub-section of the connect dialog.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProxyFormState {
    pub protocol: ProxyProtocol,
    pub host:     String,
    pub port:     String,     // raw text so user can type freely
    pub username: String,
    pub password: String,
    pub error:    Option<String>,
}

impl ProxyFormState {
    pub fn is_active(&self) -> bool {
        self.protocol != ProxyProtocol::None
    }

    /// Parsed port, or `None` if the text is not a valid u16.
    pub fn parsed_port(&self) -> Option<u16> {
        self.port.trim().parse().ok()
    }

    /// Validate and surface any error into `self.error`.
    pub fn validate(&mut self) -> bool {
        self.error = None;
        if !self.is_active() { return true; }
        if self.host.trim().is_empty() {
            self.error = Some("Proxy host is required".into());
            return false;
        }
        if self.parsed_port().is_none() {
            self.error = Some("Proxy port must be a number between 1 and 65535".into());
            return false;
        }
        true
    }
}

// ── ConnectionTestResult ──────────────────────────────────────────────────────

/// Result of an asynchronous connection test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionTestResult {
    /// Test not yet started.
    Idle,
    /// Test in progress.
    Testing,
    /// Connection succeeded: server version string returned.
    Ok { server_name: String, version: String },
    /// Connection failed with an error message.
    Failed(String),
}

// ── ServerConnectState ────────────────────────────────────────────────────────

/// All state owned by the "Connect to Server" dialog.
#[derive(Debug, Clone)]
pub struct ServerConnectState {
    /// Raw server URL as typed by the user.
    pub url_input:   String,
    /// Validation error for the URL field.
    pub url_error:   Option<String>,
    /// Whether TLS certificate verification is enabled.
    pub tls_verify:  bool,
    /// Proxy form.
    pub proxy:       ProxyFormState,
    /// Result of the last test-connection / save attempt.
    pub test_result: ConnectionTestResult,
    /// `true` while a connect attempt is in flight.
    pub connecting:  bool,
    /// `true` once the user successfully connected and the dialog should close.
    pub confirmed:   bool,
}

impl Default for ServerConnectState {
    fn default() -> Self {
        Self {
            url_input:   String::new(),
            url_error:   None,
            tls_verify:  true,
            proxy:       ProxyFormState::default(),
            test_result: ConnectionTestResult::Idle,
            connecting:  false,
            confirmed:   false,
        }
    }
}

impl ServerConnectState {
    pub fn new() -> Self { Self::default() }

    /// Pre-populate the dialog from a previously saved URL.
    pub fn with_url(url: impl Into<String>) -> Self {
        Self { url_input: url.into(), ..Self::default() }
    }

    /// Validate URL (and proxy if active).  Surfaces errors.  Returns `true`
    /// when everything is valid.
    pub fn validate(&mut self) -> bool {
        self.url_error = None;
        let url_ok = if looks_like_server_url(&self.url_input) {
            true
        } else {
            self.url_error = Some("Enter a valid server URL (e.g. https://logos.example.com or 192.168.1.10:8080)".into());
            false
        };
        let proxy_ok = self.proxy.validate();
        url_ok && proxy_ok
    }

    /// Normalised server URL.
    pub fn normalised_url(&self) -> String {
        normalise_server_url(&self.url_input)
    }

    /// Mark that a test/connect is starting.
    pub fn begin_connect(&mut self) {
        self.connecting  = true;
        self.test_result = ConnectionTestResult::Testing;
    }

    /// Called when the connection test succeeds.
    pub fn on_connect_ok(&mut self, server_name: String, version: String) {
        self.connecting  = false;
        self.test_result = ConnectionTestResult::Ok { server_name, version };
    }

    /// Called when the connection test fails.
    pub fn on_connect_failed(&mut self, msg: impl Into<String>) {
        self.connecting  = false;
        self.test_result = ConnectionTestResult::Failed(msg.into());
    }

    /// Confirm – close the dialog.
    pub fn confirm(&mut self) {
        self.confirmed = true;
    }

    /// Reset errors and test result (e.g. when the user edits the URL).
    pub fn reset_feedback(&mut self) {
        self.url_error   = None;
        self.test_result = ConnectionTestResult::Idle;
        self.confirmed   = false;
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests (SC-01 … SC-10)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // SC-01: looks_like_server_url accepts https://
    #[test]
    fn sc_01_valid_https_url() {
        assert!(looks_like_server_url("https://logos.example.com"));
    }

    // SC-02: looks_like_server_url accepts host:port
    #[test]
    fn sc_02_valid_host_port() {
        assert!(looks_like_server_url("192.168.1.10:8080"));
    }

    // SC-03: looks_like_server_url rejects empty string
    #[test]
    fn sc_03_empty_rejected() {
        assert!(!looks_like_server_url(""));
    }

    // SC-04: normalise_server_url prepends http:// when no scheme
    #[test]
    fn sc_04_normalise_adds_scheme() {
        assert_eq!(normalise_server_url("192.168.1.10:8080"), "http://192.168.1.10:8080");
    }

    // SC-05: normalise_server_url strips trailing slash
    #[test]
    fn sc_05_normalise_strips_slash() {
        assert_eq!(normalise_server_url("https://logos.example.com/"), "https://logos.example.com");
    }

    // SC-06: validate() returns false + sets url_error for invalid URL
    #[test]
    fn sc_06_validate_bad_url() {
        let mut s = ServerConnectState::new();
        s.url_input = "not a url".into();
        assert!(!s.validate());
        assert!(s.url_error.is_some());
    }

    // SC-07: validate() returns true for good URL with no proxy
    #[test]
    fn sc_07_validate_good_url() {
        let mut s = ServerConnectState::with_url("https://s.local");
        assert!(s.validate());
        assert!(s.url_error.is_none());
    }

    // SC-08: ProxyFormState validates missing host
    #[test]
    fn sc_08_proxy_missing_host() {
        let mut p = ProxyFormState { protocol: ProxyProtocol::Http, port: "8080".into(), ..Default::default() };
        assert!(!p.validate());
        assert!(p.error.is_some());
    }

    // SC-09: begin_connect / on_connect_ok transition state correctly
    #[test]
    fn sc_09_connect_ok_transition() {
        let mut s = ServerConnectState::with_url("https://s.local");
        s.begin_connect();
        assert!(s.connecting);
        assert_eq!(s.test_result, ConnectionTestResult::Testing);
        s.on_connect_ok("Logos".into(), "1.0.0".into());
        assert!(!s.connecting);
        assert!(matches!(s.test_result, ConnectionTestResult::Ok { .. }));
    }

    // SC-10: on_connect_failed sets failed state
    #[test]
    fn sc_10_connect_failed() {
        let mut s = ServerConnectState::with_url("https://s.local");
        s.begin_connect();
        s.on_connect_failed("timeout");
        assert!(!s.connecting);
        assert!(matches!(s.test_result, ConnectionTestResult::Failed(_)));
    }
}
