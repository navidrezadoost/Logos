// logos-collab/src/network/client.rs
//
//! Async HTTP client wrapper for desktop → Logos-server communication.
//!
//! `HttpClient` wraps `reqwest::Client` and adds:
//! - Bearer-token injection on every request
//! - Proxy configuration (HTTP, HTTPS, SOCKS5)
//! - Optional TLS-cert verification bypass
//! - Uniform `ApiError` type
//!
//! The struct is **cheaply cloneable** (inner `reqwest::Client` is
//! `Arc`-backed) so callers can clone it freely across async tasks.

use serde::{Deserialize, Serialize};

use crate::desktop_sync::{ConnectionConfig, ProxyConfig, ProxyKind};

// ── ApiError ───────────────────────────────────────────────────────────────────

/// Errors returned by any HTTP API call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    /// Network / transport error (connection refused, timeout, etc.)
    Network(String),
    /// Server returned a non-success HTTP status.
    Http { status: u16, body: String },
    /// Response body could not be deserialized.
    Decode(String),
    /// Client not authenticated (no token stored).
    NotAuthenticated,
    /// The operation was denied by the server.
    PermissionDenied,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::Network(e)          => write!(f, "Network error: {e}"),
            ApiError::Http { status, body } => write!(f, "HTTP {status}: {body}"),
            ApiError::Decode(e)           => write!(f, "Decode error: {e}"),
            ApiError::NotAuthenticated    => write!(f, "Not authenticated"),
            ApiError::PermissionDenied    => write!(f, "Permission denied"),
        }
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

// ── ClientConfig ──────────────────────────────────────────────────────────────

/// Build-time configuration for `HttpClient`.
///
/// Derived from `ConnectionConfig` (desktop_sync) plus any extra
/// per-call overrides.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub base_url:   String,
    pub tls_verify: bool,
    pub proxy:      ProxyConfig,
    /// Connection timeout in milliseconds.
    pub timeout_ms: u64,
}

impl ClientConfig {
    pub fn from_connection_config(cfg: &ConnectionConfig) -> Self {
        Self {
            base_url:   cfg.server_url.trim_end_matches('/').to_owned(),
            tls_verify: cfg.tls_verify,
            proxy:      cfg.proxy.clone(),
            timeout_ms: 15_000,
        }
    }
}

// ── HttpClient ────────────────────────────────────────────────────────────────

/// Async HTTP client bound to one Logos server.
///
/// Created once per active session; cheap to clone across tasks.
#[cfg(feature = "http-client")]
#[derive(Clone)]
pub struct HttpClient {
    inner:    reqwest::Client,
    base_url: String,
    token:    std::sync::Arc<tokio::sync::RwLock<Option<String>>>,
}

#[cfg(feature = "http-client")]
impl HttpClient {
    /// Build a new `HttpClient` from `ClientConfig`.
    pub fn new(cfg: &ClientConfig) -> ApiResult<Self> {
        use std::time::Duration;
        let mut builder = reqwest::Client::builder()
            .timeout(Duration::from_millis(cfg.timeout_ms))
            .danger_accept_invalid_certs(!cfg.tls_verify);

        if cfg.proxy.is_active() {
            let proxy_url = cfg.proxy.url()
                .ok_or_else(|| ApiError::Network("Invalid proxy config".into()))?;
            let mut proxy = reqwest::Proxy::all(&proxy_url)
                .map_err(|e| ApiError::Network(e.to_string()))?;
            if let (Some(user), Some(pass)) = (&cfg.proxy.username, &cfg.proxy.password) {
                proxy = proxy.basic_auth(user, pass);
            }
            builder = builder.proxy(proxy);
        }

        let client = builder.build()
            .map_err(|e| ApiError::Network(e.to_string()))?;

        Ok(Self {
            inner:    client,
            base_url: cfg.base_url.clone(),
            token:    std::sync::Arc::new(tokio::sync::RwLock::new(None)),
        })
    }

    /// Store the session token (called after successful login).
    pub async fn set_token(&self, token: impl Into<String>) {
        *self.token.write().await = Some(token.into());
    }

    /// Clear the session token (called on logout).
    pub async fn clear_token(&self) {
        *self.token.write().await = None;
    }

    pub async fn has_token(&self) -> bool {
        self.token.read().await.is_some()
    }

    fn url(&self, path: &str) -> String {
        let path = path.trim_start_matches('/');
        format!("{}/{}", self.base_url, path)
    }

    async fn auth_header(&self) -> ApiResult<String> {
        self.token.read().await
            .as_deref()
            .map(|t| format!("Bearer {t}"))
            .ok_or(ApiError::NotAuthenticated)
    }

    // ── low-level helpers ──

    /// Authenticated GET → deserialize JSON body.
    pub async fn get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> ApiResult<T> {
        let auth = self.auth_header().await?;
        let resp = self.inner.get(self.url(path))
            .header("Authorization", auth)
            .send().await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        Self::parse_response(resp).await
    }

    /// Authenticated POST with JSON body → deserialize JSON response.
    pub async fn post<B: Serialize, T: for<'de> Deserialize<'de>>(
        &self, path: &str, body: &B,
    ) -> ApiResult<T> {
        let auth = self.auth_header().await?;
        let resp = self.inner.post(self.url(path))
            .header("Authorization", auth)
            .json(body)
            .send().await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        Self::parse_response(resp).await
    }

    /// Unauthenticated POST (used for login).
    pub async fn post_anon<B: Serialize, T: for<'de> Deserialize<'de>>(
        &self, path: &str, body: &B,
    ) -> ApiResult<T> {
        let resp = self.inner.post(self.url(path))
            .json(body)
            .send().await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        Self::parse_response(resp).await
    }

    /// Authenticated DELETE → no response body.
    pub async fn delete(&self, path: &str) -> ApiResult<()> {
        let auth = self.auth_header().await?;
        let resp = self.inner.delete(self.url(path))
            .header("Authorization", auth)
            .send().await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        let status = resp.status().as_u16();
        if status >= 200 && status < 300 { return Ok(()); }
        let body = resp.text().await.unwrap_or_default();
        Err(if status == 403 { ApiError::PermissionDenied } else { ApiError::Http { status, body } })
    }

    /// Authenticated PATCH with JSON body.
    pub async fn patch<B: Serialize, T: for<'de> Deserialize<'de>>(
        &self, path: &str, body: &B,
    ) -> ApiResult<T> {
        let auth = self.auth_header().await?;
        let resp = self.inner.patch(self.url(path))
            .header("Authorization", auth)
            .json(body)
            .send().await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        Self::parse_response(resp).await
    }

    /// Unauthenticated GET (for e.g. `/api/info`).
    pub async fn inner_get_anon<T: for<'de> Deserialize<'de>>(&self, path: &str) -> ApiResult<T> {
        let resp = self.inner.get(self.url(path))
            .send().await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        Self::parse_response(resp).await
    }

    async fn parse_response<T: for<'de> Deserialize<'de>>(
        resp: reqwest::Response,
    ) -> ApiResult<T> {
        let status = resp.status().as_u16();
        if status == 403 { return Err(ApiError::PermissionDenied); }
        if status < 200 || status >= 300 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ApiError::Http { status, body });
        }
        resp.json::<T>().await
            .map_err(|e| ApiError::Decode(e.to_string()))
    }
}

// ── Tests (pure-logic, no network) ────────────────────────────────────────────
//
// These tests exercise the non-reqwest parts: ApiError display, ClientConfig
// construction from ConnectionConfig, and ApiResult typing.
// They compile without the `http-client` feature.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop_sync::ConnectionConfig;

    // NC-01: ApiError::Network display
    #[test]
    fn nc_01_api_error_network_display() {
        let e = ApiError::Network("timeout".into());
        assert!(e.to_string().contains("timeout"));
    }

    // NC-02: ApiError::Http display
    #[test]
    fn nc_02_api_error_http_display() {
        let e = ApiError::Http { status: 404, body: "not found".into() };
        assert!(e.to_string().contains("404"));
    }

    // NC-03: ApiError::PermissionDenied display
    #[test]
    fn nc_03_api_error_permission_denied() {
        assert!(ApiError::PermissionDenied.to_string().contains("denied"));
    }

    // NC-04: ApiError::NotAuthenticated display
    #[test]
    fn nc_04_api_error_not_authenticated() {
        assert!(ApiError::NotAuthenticated.to_string().contains("authenticated"));
    }

    // NC-05: ApiError equality
    #[test]
    fn nc_05_api_error_equality() {
        assert_eq!(ApiError::NotAuthenticated, ApiError::NotAuthenticated);
        assert_ne!(ApiError::NotAuthenticated, ApiError::PermissionDenied);
    }

    // NC-06: ClientConfig::from_connection_config maps base_url
    #[test]
    fn nc_06_client_config_base_url() {
        let cc = ConnectionConfig::new("https://s.local/");
        let cfg = ClientConfig::from_connection_config(&cc);
        // Trailing slash stripped
        assert_eq!(cfg.base_url, "https://s.local");
    }

    // NC-07: ClientConfig::from_connection_config maps tls_verify
    #[test]
    fn nc_07_client_config_tls_verify() {
        let cc = ConnectionConfig::new("https://s.local").with_tls_verify(false);
        let cfg = ClientConfig::from_connection_config(&cc);
        assert!(!cfg.tls_verify);
    }

    // NC-08: ClientConfig::from_connection_config copies proxy
    #[test]
    fn nc_08_client_config_proxy() {
        use crate::desktop_sync::ProxyConfig;
        let cc = ConnectionConfig::new("https://s.local")
            .with_proxy(ProxyConfig::http("proxy", 8080));
        let cfg = ClientConfig::from_connection_config(&cc);
        assert!(cfg.proxy.is_active());
    }

    // NC-09: ApiError::Decode display
    #[test]
    fn nc_09_api_error_decode_display() {
        let e = ApiError::Decode("unexpected token".into());
        assert!(e.to_string().contains("Decode"));
    }

    // NC-10: ApiResult Ok carries value
    #[test]
    fn nc_10_api_result_ok() {
        let r: ApiResult<u32> = Ok(42);
        assert_eq!(r.unwrap(), 42);
    }
}
