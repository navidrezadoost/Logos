//! Authentication middleware for WebSocket connections.
//!
//! Provides connection-level authentication and per-message rate limiting.
//! Integrates TokenEngine and RateLimiter into a single middleware layer.
//!
//! Architecture:
//! ```text
//! WebSocket Upgrade Request
//!       │
//!       ├── Extract token from query string or Authorization header
//!       │
//!       ▼
//! ┌─────────────┐
//! │ authenticate │ ── verify JWT (<300ns) ── Accept/Reject
//! └──────┬──────┘
//!        │ (on accept)
//!        ▼
//! ┌─────────────┐
//! │ check_rate  │ ── token bucket (<200ns) ── Allow/Throttle
//! └─────────────┘
//! ```
//!
//! Reference: OWASP Testing Guide v4 — Session Management
//! Reference: RFC 6750 — Bearer Token Usage

use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

use super::token::{Claims, TokenEngine, TokenError};
use super::ratelimit::{RateLimiter, RateLimitConfig};

/// Authentication configuration.
#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// Whether authentication is required (false = allow anonymous)
    pub require_auth: bool,
    /// JWT secret key (32 bytes)
    pub jwt_secret: [u8; 32],
    /// Rate limit configuration
    pub rate_limit: RateLimitConfig,
    /// Anonymous user display name prefix
    pub anonymous_prefix: String,
    /// Maximum token age before forced re-auth (seconds)
    pub max_token_age: u64,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            require_auth: false,
            jwt_secret: [0u8; 32], // MUST be overridden in production
            rate_limit: RateLimitConfig::default(),
            anonymous_prefix: "Anonymous".to_string(),
            max_token_age: 86400, // 24 hours
        }
    }
}

/// Authentication errors.
#[derive(Debug, Clone, PartialEq)]
pub enum AuthError {
    /// No token provided and auth is required
    MissingToken,
    /// Token verification failed
    TokenError(TokenError),
    /// User is rate limited
    RateLimited,
    /// Room bandwidth exceeded
    BandwidthExceeded,
    /// User not allowed to access this document
    AccessDenied,
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingToken => write!(f, "Authentication required"),
            Self::TokenError(e) => write!(f, "Authentication failed: {e}"),
            Self::RateLimited => write!(f, "Rate limited"),
            Self::BandwidthExceeded => write!(f, "Room bandwidth exceeded"),
            Self::AccessDenied => write!(f, "Access denied to document"),
        }
    }
}

impl std::error::Error for AuthError {}

impl From<TokenError> for AuthError {
    fn from(e: TokenError) -> Self {
        AuthError::TokenError(e)
    }
}

/// Authenticated user context for a connection.
///
/// Created on successful authentication and attached to the connection.
/// Used for authorization checks throughout the connection lifetime.
#[derive(Debug, Clone)]
pub struct AuthContext {
    /// Authenticated user claims (None for anonymous)
    pub claims: Option<Claims>,
    /// Effective user ID
    pub user_id: Uuid,
    /// Display name
    pub name: String,
    /// Whether this is an anonymous connection
    pub is_anonymous: bool,
}

impl AuthContext {
    /// Create an authenticated context from verified claims.
    pub fn from_claims(claims: Claims) -> Self {
        Self {
            user_id: claims.sub,
            name: claims.name.clone(),
            is_anonymous: false,
            claims: Some(claims),
        }
    }

    /// Create an anonymous context.
    pub fn anonymous(prefix: &str) -> Self {
        let user_id = Uuid::new_v4();
        Self {
            user_id,
            name: format!("{prefix}_{}", &user_id.to_string()[..8]),
            is_anonymous: true,
            claims: None,
        }
    }

    /// Check if this user can access a specific document.
    pub fn can_access(&self, doc_id: &Uuid) -> bool {
        match &self.claims {
            Some(claims) => claims.can_access(doc_id),
            None => true, // Anonymous users can access all docs (if anonymous is allowed)
        }
    }
}

/// Authentication middleware.
///
/// Wraps TokenEngine and RateLimiter to provide a unified
/// authentication and authorization layer for the sync server.
///
/// Thread-safe: RateLimiter is behind a Mutex for concurrent access.
pub struct AuthMiddleware {
    /// JWT token engine
    token_engine: Arc<TokenEngine>,
    /// Rate limiter (behind Mutex for concurrent access)
    rate_limiter: Arc<Mutex<RateLimiter>>,
    /// Configuration
    config: AuthConfig,
}

impl AuthMiddleware {
    /// Create a new auth middleware with the given configuration.
    pub fn new(config: AuthConfig) -> Self {
        let token_engine = Arc::new(TokenEngine::new(config.jwt_secret));
        let rate_limiter = Arc::new(Mutex::new(
            RateLimiter::new(config.rate_limit.clone()),
        ));
        Self {
            token_engine,
            rate_limiter,
            config,
        }
    }

    /// Create middleware with default config (auth disabled, for testing).
    pub fn permissive() -> Self {
        Self::new(AuthConfig::default())
    }

    /// Authenticate a WebSocket connection.
    ///
    /// Extracts and verifies the JWT token from the request.
    /// Returns an AuthContext on success.
    ///
    /// Token can be provided via:
    /// 1. Query string: `?token=<jwt>`
    /// 2. Authorization header: `Bearer <jwt>`
    pub fn authenticate(&self, token: Option<&str>) -> Result<AuthContext, AuthError> {
        match token {
            Some(t) => {
                let claims = self.token_engine.verify(t)?;
                Ok(AuthContext::from_claims(claims))
            }
            None => {
                if self.config.require_auth {
                    Err(AuthError::MissingToken)
                } else {
                    Ok(AuthContext::anonymous(&self.config.anonymous_prefix))
                }
            }
        }
    }

    /// Check if a user is rate limited for sending a message.
    ///
    /// Returns Ok(()) if allowed, Err(AuthError::RateLimited) if throttled.
    pub async fn check_rate(&self, user_id: Uuid) -> Result<(), AuthError> {
        let mut limiter = self.rate_limiter.lock().await;
        if limiter.check_user(user_id) {
            Ok(())
        } else {
            Err(AuthError::RateLimited)
        }
    }

    /// Check room bandwidth limit.
    pub async fn check_bandwidth(&self, room_id: Uuid, bytes: u64) -> Result<(), AuthError> {
        let mut limiter = self.rate_limiter.lock().await;
        if limiter.check_room_bandwidth(room_id, bytes) {
            Ok(())
        } else {
            Err(AuthError::BandwidthExceeded)
        }
    }

    /// Combined check: user rate + room bandwidth.
    pub async fn check_message(
        &self,
        user_id: Uuid,
        room_id: Uuid,
        message_bytes: u64,
    ) -> Result<(), AuthError> {
        let mut limiter = self.rate_limiter.lock().await;
        if !limiter.check_user(user_id) {
            return Err(AuthError::RateLimited);
        }
        if !limiter.check_room_bandwidth(room_id, message_bytes) {
            return Err(AuthError::BandwidthExceeded);
        }
        Ok(())
    }

    /// Issue a new JWT token for a user.
    pub fn issue_token(&self, user_id: Uuid, name: &str) -> Result<String, TokenError> {
        let claims = Claims::new(user_id, name);
        self.token_engine.issue(&claims)
    }

    /// Issue a token with document restrictions.
    pub fn issue_restricted_token(
        &self,
        user_id: Uuid,
        name: &str,
        allowed_docs: Vec<Uuid>,
    ) -> Result<String, TokenError> {
        let claims = Claims::new(user_id, name).with_docs(allowed_docs);
        self.token_engine.issue(&claims)
    }

    /// Get the underlying token engine (for direct use in benchmarks).
    pub fn token_engine(&self) -> &Arc<TokenEngine> {
        &self.token_engine
    }

    /// Run rate limiter GC.
    pub async fn gc(&self) -> usize {
        let mut limiter = self.rate_limiter.lock().await;
        limiter.gc()
    }

    /// Get rate limiter stats.
    pub async fn rate_limit_stats(&self) -> super::ratelimit::RateLimitStats {
        let limiter = self.rate_limiter.lock().await;
        limiter.stats()
    }
}

/// Extract a JWT token from a WebSocket upgrade URI.
///
/// Looks for `?token=<jwt>` in the query string.
/// Returns `None` if no token is found.
pub fn extract_token_from_uri(uri: &str) -> Option<String> {
    let query = uri.split('?').nth(1)?;
    for param in query.split('&') {
        if let Some(value) = param.strip_prefix("token=") {
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Extract a JWT token from an Authorization header value.
///
/// Expects format: `Bearer <token>`
pub fn extract_token_from_header(header: &str) -> Option<String> {
    header.strip_prefix("Bearer ").map(|t| t.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_middleware() -> AuthMiddleware {
        AuthMiddleware::new(AuthConfig {
            jwt_secret: [42u8; 32],
            require_auth: false,
            ..Default::default()
        })
    }

    fn auth_required_middleware() -> AuthMiddleware {
        AuthMiddleware::new(AuthConfig {
            jwt_secret: [42u8; 32],
            require_auth: true,
            ..Default::default()
        })
    }

    #[test]
    fn test_authenticate_with_valid_token() {
        let mw = test_middleware();
        let user_id = Uuid::new_v4();
        let token = mw.issue_token(user_id, "Alice").unwrap();

        let ctx = mw.authenticate(Some(&token)).unwrap();
        assert_eq!(ctx.user_id, user_id);
        assert_eq!(ctx.name, "Alice");
        assert!(!ctx.is_anonymous);
    }

    #[test]
    fn test_authenticate_anonymous_allowed() {
        let mw = test_middleware();

        let ctx = mw.authenticate(None).unwrap();
        assert!(ctx.is_anonymous);
        assert!(ctx.name.starts_with("Anonymous_"));
    }

    #[test]
    fn test_authenticate_anonymous_rejected() {
        let mw = auth_required_middleware();

        let result = mw.authenticate(None);
        assert_eq!(result.err(), Some(AuthError::MissingToken));
    }

    #[test]
    fn test_authenticate_invalid_token() {
        let mw = test_middleware();

        let result = mw.authenticate(Some("invalid.token.here"));
        assert!(matches!(result, Err(AuthError::TokenError(_))));
    }

    #[tokio::test]
    async fn test_rate_limiting() {
        let mw = AuthMiddleware::new(AuthConfig {
            jwt_secret: [42u8; 32],
            require_auth: false,
            rate_limit: RateLimitConfig {
                messages_per_second: 5.0,
                burst_capacity: 5.0,
                ..Default::default()
            },
            ..Default::default()
        });

        let user = Uuid::new_v4();

        // 5 messages should pass
        for _ in 0..5 {
            assert!(mw.check_rate(user).await.is_ok());
        }

        // 6th should be rate limited
        assert_eq!(mw.check_rate(user).await.err(), Some(AuthError::RateLimited));
    }

    #[tokio::test]
    async fn test_bandwidth_limiting() {
        let mw = AuthMiddleware::new(AuthConfig {
            jwt_secret: [42u8; 32],
            require_auth: false,
            rate_limit: RateLimitConfig {
                room_bytes_per_second: 1000,
                ..Default::default()
            },
            ..Default::default()
        });

        let room = Uuid::new_v4();

        assert!(mw.check_bandwidth(room, 500).await.is_ok());
        assert!(mw.check_bandwidth(room, 400).await.is_ok());
        assert_eq!(mw.check_bandwidth(room, 200).await.err(), Some(AuthError::BandwidthExceeded));
    }

    #[tokio::test]
    async fn test_combined_message_check() {
        let mw = test_middleware();
        let user = Uuid::new_v4();
        let room = Uuid::new_v4();

        assert!(mw.check_message(user, room, 100).await.is_ok());
    }

    #[test]
    fn test_extract_token_from_uri() {
        assert_eq!(
            extract_token_from_uri("/ws?token=abc123"),
            Some("abc123".to_string())
        );
        assert_eq!(
            extract_token_from_uri("/ws?room=test&token=xyz&other=1"),
            Some("xyz".to_string())
        );
        assert_eq!(extract_token_from_uri("/ws?room=test"), None);
        assert_eq!(extract_token_from_uri("/ws"), None);
        assert_eq!(extract_token_from_uri("/ws?token="), None);
    }

    #[test]
    fn test_extract_token_from_header() {
        assert_eq!(
            extract_token_from_header("Bearer abc123"),
            Some("abc123".to_string())
        );
        assert_eq!(extract_token_from_header("Basic abc"), None);
        assert_eq!(extract_token_from_header(""), None);
    }

    #[test]
    fn test_auth_context_access_check() {
        let engine = TokenEngine::new([42u8; 32]);
        let doc1 = Uuid::new_v4();
        let doc2 = Uuid::new_v4();

        let claims = Claims::new(Uuid::new_v4(), "Alice").with_docs(vec![doc1]);
        let ctx = AuthContext::from_claims(claims);

        assert!(ctx.can_access(&doc1));
        assert!(!ctx.can_access(&doc2));

        // Anonymous can access everything
        let anon = AuthContext::anonymous("Anon");
        assert!(anon.can_access(&doc1));
        assert!(anon.can_access(&doc2));

        // Suppress unused variable warning
        let _ = engine;
    }

    #[test]
    fn test_restricted_token() {
        let mw = test_middleware();
        let user = Uuid::new_v4();
        let doc = Uuid::new_v4();

        let token = mw.issue_restricted_token(user, "Alice", vec![doc]).unwrap();
        let ctx = mw.authenticate(Some(&token)).unwrap();

        assert!(ctx.can_access(&doc));
        assert!(!ctx.can_access(&Uuid::new_v4()));
    }

    #[test]
    fn test_auth_error_display() {
        assert_eq!(AuthError::MissingToken.to_string(), "Authentication required");
        assert_eq!(AuthError::RateLimited.to_string(), "Rate limited");
        assert_eq!(AuthError::BandwidthExceeded.to_string(), "Room bandwidth exceeded");
        assert_eq!(AuthError::AccessDenied.to_string(), "Access denied to document");
    }

    #[test]
    fn test_permissive_middleware() {
        let mw = AuthMiddleware::permissive();

        // Should allow anonymous
        let ctx = mw.authenticate(None).unwrap();
        assert!(ctx.is_anonymous);
    }

    #[tokio::test]
    async fn test_gc() {
        let mw = AuthMiddleware::new(AuthConfig {
            jwt_secret: [42u8; 32],
            rate_limit: RateLimitConfig {
                bucket_ttl: std::time::Duration::from_millis(50),
                ..Default::default()
            },
            ..Default::default()
        });

        let user = Uuid::new_v4();
        mw.check_rate(user).await.unwrap();

        std::thread::sleep(std::time::Duration::from_millis(100));

        let removed = mw.gc().await;
        assert_eq!(removed, 1);
    }
}
