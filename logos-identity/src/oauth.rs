//! OAuth2 provider types and configuration.
//!
//! Defines the data structures for OAuth2 flows. The actual HTTP
//! transport is left to consumer crates — this module only defines
//! the protocol types and provider configurations.

use crate::error::IdentityError;
use crate::user::AuthProvider;
use serde::{Deserialize, Serialize};

/// OAuth2 provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthConfig {
    /// Which provider this is.
    pub provider: AuthProvider,
    /// OAuth2 client ID.
    pub client_id: String,
    /// OAuth2 client secret (keep secure!).
    pub client_secret: String,
    /// Authorization endpoint URL.
    pub auth_url: String,
    /// Token exchange endpoint URL.
    pub token_url: String,
    /// User info endpoint URL.
    pub userinfo_url: String,
    /// Redirect URL after authorization.
    pub redirect_url: String,
    /// Requested scopes.
    pub scopes: Vec<String>,
}

impl OAuthConfig {
    /// Create a Google OAuth2 configuration.
    pub fn google(client_id: impl Into<String>, client_secret: impl Into<String>, redirect_url: impl Into<String>) -> Self {
        Self {
            provider: AuthProvider::Google,
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            auth_url: "https://accounts.google.com/o/oauth2/v2/auth".into(),
            token_url: "https://oauth2.googleapis.com/token".into(),
            userinfo_url: "https://www.googleapis.com/oauth2/v2/userinfo".into(),
            redirect_url: redirect_url.into(),
            scopes: vec!["openid".into(), "email".into(), "profile".into()],
        }
    }

    /// Create a GitHub OAuth2 configuration.
    pub fn github(client_id: impl Into<String>, client_secret: impl Into<String>, redirect_url: impl Into<String>) -> Self {
        Self {
            provider: AuthProvider::GitHub,
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            auth_url: "https://github.com/login/oauth/authorize".into(),
            token_url: "https://github.com/login/oauth/access_token".into(),
            userinfo_url: "https://api.github.com/user".into(),
            redirect_url: redirect_url.into(),
            scopes: vec!["user:email".into(), "read:user".into()],
        }
    }

    /// Create a Microsoft / Azure AD configuration.
    pub fn microsoft(client_id: impl Into<String>, client_secret: impl Into<String>, redirect_url: impl Into<String>, tenant: &str) -> Self {
        Self {
            provider: AuthProvider::Microsoft,
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            auth_url: format!("https://login.microsoftonline.com/{tenant}/oauth2/v2.0/authorize"),
            token_url: format!("https://login.microsoftonline.com/{tenant}/oauth2/v2.0/token"),
            userinfo_url: "https://graph.microsoft.com/v1.0/me".into(),
            redirect_url: redirect_url.into(),
            scopes: vec!["openid".into(), "email".into(), "profile".into()],
        }
    }
}

/// OAuth2 authorization state (anti-CSRF + PKCE).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthState {
    /// Random state parameter (anti-CSRF).
    pub state: String,
    /// Nonce for OpenID Connect.
    pub nonce: String,
    /// PKCE code verifier (S256).
    pub pkce_verifier: Option<String>,
    /// Where to redirect after auth completes.
    pub redirect_url: String,
    /// When this state was created (Unix timestamp).
    pub created_at: u64,
}

impl OAuthState {
    /// Create a new OAuth state with a random nonce.
    pub fn new(redirect_url: impl Into<String>) -> Self {
        Self {
            state: uuid::Uuid::new_v4().to_string(),
            nonce: uuid::Uuid::new_v4().to_string(),
            pkce_verifier: None,
            redirect_url: redirect_url.into(),
            created_at: crate::user::current_timestamp(),
        }
    }

    /// Whether this state has expired (default: 10 minutes).
    pub fn is_expired(&self) -> bool {
        crate::user::current_timestamp() > self.created_at + 600
    }
}

/// OAuth2 token response from the provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthTokenResponse {
    /// Access token.
    pub access_token: String,
    /// Token type (usually "Bearer").
    pub token_type: String,
    /// Lifetime in seconds.
    pub expires_in: Option<u64>,
    /// Refresh token (if granted).
    pub refresh_token: Option<String>,
    /// Granted scope.
    pub scope: Option<String>,
    /// OpenID Connect ID token.
    pub id_token: Option<String>,
}

/// User info from an OAuth2 provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthUserInfo {
    /// Provider-specific user ID.
    pub provider_user_id: String,
    /// Email address (may not be available).
    pub email: Option<String>,
    /// Display name.
    pub name: Option<String>,
    /// Avatar URL.
    pub avatar_url: Option<String>,
    /// Which provider this came from.
    pub provider: AuthProvider,
}

impl OAuthUserInfo {
    /// Best-effort display name (falls back to email prefix).
    pub fn display_name(&self) -> String {
        if let Some(name) = &self.name {
            name.clone()
        } else if let Some(email) = &self.email {
            email.split('@').next().unwrap_or("User").to_string()
        } else {
            format!("User-{}", &self.provider_user_id[..8.min(self.provider_user_id.len())])
        }
    }
}

/// Trait for OAuth2 provider implementations.
///
/// Consumer crates implement this with actual HTTP clients.
pub trait OAuthProvider {
    /// Build the authorization URL for the user to visit.
    fn authorize_url(&self, state: &OAuthState) -> String;

    /// Exchange an authorization code for tokens.
    fn exchange_code(&self, code: &str, state: &OAuthState) -> Result<OAuthTokenResponse, IdentityError>;

    /// Fetch user info using an access token.
    fn user_info(&self, token: &OAuthTokenResponse) -> Result<OAuthUserInfo, IdentityError>;

    /// Refresh an expired access token.
    fn refresh_token(&self, refresh_token: &str) -> Result<OAuthTokenResponse, IdentityError>;
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn google_config() {
        let cfg = OAuthConfig::google("client_id", "secret", "http://localhost:8080/callback");
        assert_eq!(cfg.provider, AuthProvider::Google);
        assert!(cfg.auth_url.contains("google"));
        assert!(cfg.token_url.contains("google"));
        assert!(cfg.scopes.contains(&"openid".to_string()));
    }

    #[test]
    fn github_config() {
        let cfg = OAuthConfig::github("client_id", "secret", "http://localhost:8080/callback");
        assert_eq!(cfg.provider, AuthProvider::GitHub);
        assert!(cfg.auth_url.contains("github"));
        assert!(cfg.scopes.contains(&"user:email".to_string()));
    }

    #[test]
    fn microsoft_config() {
        let cfg = OAuthConfig::microsoft("id", "secret", "http://localhost:8080/callback", "common");
        assert_eq!(cfg.provider, AuthProvider::Microsoft);
        assert!(cfg.auth_url.contains("common"));
    }

    #[test]
    fn oauth_state_new() {
        let state = OAuthState::new("http://localhost/callback");
        assert!(!state.state.is_empty());
        assert!(!state.nonce.is_empty());
        assert_ne!(state.state, state.nonce);
        assert!(!state.is_expired());
    }

    #[test]
    fn oauth_state_expired() {
        let mut state = OAuthState::new("http://localhost/callback");
        state.created_at = crate::user::current_timestamp().saturating_sub(700);
        assert!(state.is_expired());
    }

    #[test]
    fn oauth_user_info_display_name() {
        let info = OAuthUserInfo {
            provider_user_id: "123456789".into(),
            email: Some("alice@example.com".into()),
            name: Some("Alice".into()),
            avatar_url: None,
            provider: AuthProvider::Google,
        };
        assert_eq!(info.display_name(), "Alice");

        let info_no_name = OAuthUserInfo {
            name: None,
            email: Some("bob@example.com".into()),
            ..info.clone()
        };
        assert_eq!(info_no_name.display_name(), "bob");

        let info_no_email = OAuthUserInfo {
            name: None,
            email: None,
            ..info
        };
        assert_eq!(info_no_email.display_name(), "User-12345678");
    }

    #[test]
    fn oauth_token_response_serde() {
        let resp = OAuthTokenResponse {
            access_token: "access_tok".into(),
            token_type: "Bearer".into(),
            expires_in: Some(3600),
            refresh_token: Some("refresh_tok".into()),
            scope: Some("openid email".into()),
            id_token: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: OAuthTokenResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.access_token, "access_tok");
        assert_eq!(back.expires_in, Some(3600));
    }

    #[test]
    fn oauth_config_serde() {
        let cfg = OAuthConfig::google("id", "secret", "http://localhost/cb");
        let json = serde_json::to_string(&cfg).unwrap();
        let back: OAuthConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.provider, AuthProvider::Google);
        assert_eq!(back.client_id, "id");
    }
}
