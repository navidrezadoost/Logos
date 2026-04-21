// logos-collab/src/desktop_sync.rs
//
//! Desktop client connection configuration — server URL, TLS, proxy support,
//! and local credential / config persistence helpers.
//!
//! This module is **connection-config only** — it does not open sockets.
//! Actual I/O is the caller's responsibility (use `reqwest` / `tokio-tungstenite`
//! with the produced `ConnectionConfig`).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── ProxyKind ─────────────────────────────────────────────────────────────────

/// The type of proxy to use for outbound connections.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProxyKind {
    None,
    Http,
    Https,
    Socks5,
}

impl Default for ProxyKind {
    fn default() -> Self { ProxyKind::None }
}

// ── ProxyConfig ───────────────────────────────────────────────────────────────

/// Proxy address and optional credentials.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProxyConfig {
    pub kind:     ProxyKind,
    pub host:     String,
    pub port:     u16,
    pub username: Option<String>,
    pub password: Option<String>,
}

impl ProxyConfig {
    pub fn http(host: impl Into<String>, port: u16) -> Self {
        Self { kind: ProxyKind::Http, host: host.into(), port, ..Default::default() }
    }

    pub fn https(host: impl Into<String>, port: u16) -> Self {
        Self { kind: ProxyKind::Https, host: host.into(), port, ..Default::default() }
    }

    pub fn socks5(host: impl Into<String>, port: u16) -> Self {
        Self { kind: ProxyKind::Socks5, host: host.into(), port, ..Default::default() }
    }

    pub fn with_credentials(mut self, username: impl Into<String>, password: impl Into<String>) -> Self {
        self.username = Some(username.into());
        self.password = Some(password.into());
        self
    }

    /// `true` if the proxy is actually configured (kind != None and host is
    /// non-empty).
    pub fn is_active(&self) -> bool {
        self.kind != ProxyKind::None && !self.host.is_empty() && self.port > 0
    }

    /// Render the proxy URL (e.g. `socks5://user:pass@host:port`).
    pub fn url(&self) -> Option<String> {
        if !self.is_active() { return None; }
        let scheme = match self.kind {
            ProxyKind::Http   => "http",
            ProxyKind::Https  => "https",
            ProxyKind::Socks5 => "socks5",
            ProxyKind::None   => return None,
        };
        let auth = match (&self.username, &self.password) {
            (Some(u), Some(p)) => format!("{u}:{p}@"),
            (Some(u), None)    => format!("{u}@"),
            _                  => String::new(),
        };
        Some(format!("{scheme}://{auth}{}:{}", self.host, self.port))
    }
}

// ── ConnectionConfig ──────────────────────────────────────────────────────────

/// All parameters needed by the desktop client to connect to a Logos server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    /// Base server URL without trailing slash (e.g. `https://logos.example.com`
    /// or `http://192.168.1.10:8080`).
    pub server_url: String,
    /// Whether to verify TLS certificates.  Set to `false` for self-signed
    /// certificates in local deployments.
    pub tls_verify: bool,
    /// Optional proxy configuration.
    pub proxy:      ProxyConfig,
    /// Cached session token (set after successful login).
    pub session_token: Option<String>,
    /// Last-known user ID.
    pub user_id:    Option<Uuid>,
}

impl ConnectionConfig {
    /// Create a minimal config pointing at `server_url` with TLS verification
    /// enabled and no proxy.
    pub fn new(server_url: impl Into<String>) -> Self {
        Self {
            server_url:    server_url.into(),
            tls_verify:    true,
            proxy:         ProxyConfig::default(),
            session_token: None,
            user_id:       None,
        }
    }

    pub fn with_tls_verify(mut self, verify: bool) -> Self {
        self.tls_verify = verify; self
    }

    pub fn with_proxy(mut self, proxy: ProxyConfig) -> Self {
        self.proxy = proxy; self
    }

    /// Store the session token returned by a successful login.
    pub fn store_session(&mut self, token: String, user_id: Uuid) {
        self.session_token = Some(token);
        self.user_id       = Some(user_id);
    }

    pub fn clear_session(&mut self) {
        self.session_token = None;
        self.user_id       = None;
    }

    pub fn is_authenticated(&self) -> bool {
        self.session_token.is_some()
    }

    /// WebSocket URL for a given project.
    /// `wss://server/ws/project/{project_id}` (or `ws://` for non-TLS).
    pub fn ws_project_url(&self, project_id: Uuid) -> String {
        let base = self.server_url.trim_end_matches('/');
        let scheme = if base.starts_with("https") { "wss" } else { "ws" };
        let host   = base
            .trim_start_matches("https://")
            .trim_start_matches("http://");
        format!("{scheme}://{host}/ws/project/{project_id}")
    }

    /// REST URL for a given path.
    pub fn api_url(&self, path: &str) -> String {
        let base = self.server_url.trim_end_matches('/');
        let path = path.trim_start_matches('/');
        format!("{base}/{path}")
    }

    /// Authorization header value for use with HTTP requests.
    pub fn auth_header(&self) -> Option<String> {
        self.session_token.as_ref().map(|t| format!("Bearer {t}"))
    }
}

// ── LocalClientConfig ─────────────────────────────────────────────────────────

/// Persisted per-client configuration stored in `~/.logos/config.json`.
///
/// Holds one connection config per server (keyed by server URL) so the user
/// can work with multiple companies / servers simultaneously.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LocalClientConfig {
    pub connections: Vec<ConnectionConfig>,
    pub active_server_url: Option<String>,
}

impl LocalClientConfig {
    pub fn new() -> Self { Self::default() }

    pub fn add_connection(&mut self, config: ConnectionConfig) {
        // Replace if server URL already exists
        if let Some(existing) = self.connections.iter_mut()
            .find(|c| c.server_url == config.server_url)
        {
            *existing = config;
        } else {
            self.connections.push(config);
        }
    }

    pub fn get_connection(&self, server_url: &str) -> Option<&ConnectionConfig> {
        self.connections.iter().find(|c| c.server_url == server_url)
    }

    pub fn get_connection_mut(&mut self, server_url: &str) -> Option<&mut ConnectionConfig> {
        self.connections.iter_mut().find(|c| c.server_url == server_url)
    }

    pub fn active_connection(&self) -> Option<&ConnectionConfig> {
        self.active_server_url.as_deref()
            .and_then(|url| self.get_connection(url))
    }

    pub fn set_active(&mut self, server_url: &str) {
        if self.connections.iter().any(|c| c.server_url == server_url) {
            self.active_server_url = Some(server_url.to_owned());
        }
    }

    pub fn remove_connection(&mut self, server_url: &str) {
        self.connections.retain(|c| c.server_url != server_url);
        if self.active_server_url.as_deref() == Some(server_url) {
            self.active_server_url = None;
        }
    }

    /// Serialize to JSON (for writing to `~/.logos/config.json`).
    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| e.to_string())
    }

    /// Deserialize from JSON.
    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| e.to_string())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // D-01: ProxyConfig::http sets kind and host correctly.
    #[test]
    fn d_01_proxy_config_http() {
        let p = ProxyConfig::http("proxy.local", 3128);
        assert_eq!(p.kind, ProxyKind::Http);
        assert_eq!(p.host, "proxy.local");
        assert_eq!(p.port, 3128);
    }

    // D-02: ProxyConfig::socks5 sets kind correctly.
    #[test]
    fn d_02_proxy_socks5() {
        let p = ProxyConfig::socks5("10.0.0.1", 1080);
        assert_eq!(p.kind, ProxyKind::Socks5);
    }

    // D-03: ProxyConfig::url renders full proxy URL with auth.
    #[test]
    fn d_03_proxy_url_with_auth() {
        let p = ProxyConfig::socks5("proxy.local", 1080)
            .with_credentials("user", "pass");
        assert_eq!(p.url(), Some("socks5://user:pass@proxy.local:1080".into()));
    }

    // D-04: ProxyConfig::url returns None when kind is None.
    #[test]
    fn d_04_no_proxy_url_none() {
        let p = ProxyConfig::default();
        assert!(p.url().is_none());
        assert!(!p.is_active());
    }

    // D-05: ProxyConfig::url without credentials omits auth section.
    #[test]
    fn d_05_proxy_url_no_auth() {
        let p = ProxyConfig::http("corp.proxy", 8080);
        assert_eq!(p.url(), Some("http://corp.proxy:8080".into()));
    }

    // D-06: ConnectionConfig::new has TLS verify enabled by default.
    #[test]
    fn d_06_tls_verify_default_true() {
        let c = ConnectionConfig::new("https://logos.example.com");
        assert!(c.tls_verify);
    }

    // D-07: with_tls_verify(false) disables TLS verify.
    #[test]
    fn d_07_disable_tls_verify() {
        let c = ConnectionConfig::new("https://self-signed.local")
            .with_tls_verify(false);
        assert!(!c.tls_verify);
    }

    // D-08: is_authenticated is false before login.
    #[test]
    fn d_08_not_authenticated_before_login() {
        let c = ConnectionConfig::new("https://server.local");
        assert!(!c.is_authenticated());
    }

    // D-09: store_session sets is_authenticated.
    #[test]
    fn d_09_store_session_authenticates() {
        let mut c = ConnectionConfig::new("https://server.local");
        c.store_session("tok-abc".into(), Uuid::new_v4());
        assert!(c.is_authenticated());
    }

    // D-10: clear_session removes authentication.
    #[test]
    fn d_10_clear_session() {
        let mut c = ConnectionConfig::new("https://server.local");
        c.store_session("tok".into(), Uuid::new_v4());
        c.clear_session();
        assert!(!c.is_authenticated());
    }

    // D-11: auth_header produces correct Bearer prefix.
    #[test]
    fn d_11_auth_header() {
        let mut c = ConnectionConfig::new("https://s.local");
        c.store_session("abc123".into(), Uuid::new_v4());
        assert_eq!(c.auth_header(), Some("Bearer abc123".to_owned()));
    }

    // D-12: ws_project_url maps https → wss.
    #[test]
    fn d_12_ws_url_https_to_wss() {
        let c   = ConnectionConfig::new("https://logos.example.com");
        let pid = Uuid::nil();
        assert!(c.ws_project_url(pid).starts_with("wss://"));
    }

    // D-13: ws_project_url maps http → ws.
    #[test]
    fn d_13_ws_url_http_to_ws() {
        let c   = ConnectionConfig::new("http://192.168.1.10:8080");
        let pid = Uuid::nil();
        assert!(c.ws_project_url(pid).starts_with("ws://"));
    }

    // D-14: api_url builds correct URL.
    #[test]
    fn d_14_api_url() {
        let c = ConnectionConfig::new("https://server.local");
        assert_eq!(c.api_url("/api/auth/login"), "https://server.local/api/auth/login");
    }

    // D-15: LocalClientConfig roundtrips to/from JSON.
    #[test]
    fn d_15_local_config_json_roundtrip() {
        let mut cfg = LocalClientConfig::new();
        cfg.add_connection(ConnectionConfig::new("https://s1.local"));
        cfg.add_connection(ConnectionConfig::new("https://s2.local"));
        cfg.set_active("https://s1.local");

        let json    = cfg.to_json().unwrap();
        let parsed  = LocalClientConfig::from_json(&json).unwrap();
        assert_eq!(parsed.connections.len(), 2);
        assert_eq!(parsed.active_server_url, Some("https://s1.local".into()));
    }

    // D-16: add_connection replaces existing entry for same URL.
    #[test]
    fn d_16_add_connection_upserts() {
        let mut cfg = LocalClientConfig::new();
        cfg.add_connection(ConnectionConfig::new("https://s.local").with_tls_verify(true));
        cfg.add_connection(ConnectionConfig::new("https://s.local").with_tls_verify(false));
        assert_eq!(cfg.connections.len(), 1);
        assert!(!cfg.active_connection().map_or(true, |c| c.tls_verify)
            || cfg.get_connection("https://s.local").map(|c| c.tls_verify) == Some(false));
    }

    // D-17: remove_connection removes entry and clears active if needed.
    #[test]
    fn d_17_remove_connection() {
        let mut cfg = LocalClientConfig::new();
        cfg.add_connection(ConnectionConfig::new("https://s.local"));
        cfg.set_active("https://s.local");
        cfg.remove_connection("https://s.local");
        assert_eq!(cfg.connections.len(), 0);
        assert!(cfg.active_server_url.is_none());
    }

    // D-18: ConnectionConfig can carry a proxy.
    #[test]
    fn d_18_connection_with_proxy() {
        let c = ConnectionConfig::new("https://s.local")
            .with_proxy(ProxyConfig::http("proxy", 8080));
        assert!(c.proxy.is_active());
        assert_eq!(c.proxy.kind, ProxyKind::Http);
    }

    // D-19: ProxyConfig with empty host is not active.
    #[test]
    fn d_19_empty_host_not_active() {
        let p = ProxyConfig { kind: ProxyKind::Http, host: String::new(), port: 8080, ..Default::default() };
        assert!(!p.is_active());
    }

    // D-20: ProxyConfig with port 0 is not active.
    #[test]
    fn d_20_port_zero_not_active() {
        let p = ProxyConfig { kind: ProxyKind::Http, host: "proxy".into(), port: 0, ..Default::default() };
        assert!(!p.is_active());
    }
}
