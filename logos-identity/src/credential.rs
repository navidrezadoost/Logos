//! Credential types for user authentication.
//!
//! Supports multiple credential types: password hashes, OAuth tokens,
//! and API keys. The actual hashing/verification algorithms are left
//! to consumer crates — this module only defines the data structures.

use crate::permission::PermissionSet;
use serde::{Deserialize, Serialize};

/// A user's authentication credential.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Credential {
    /// Password-based authentication.
    Password(PasswordCredential),
    /// OAuth2 token-based authentication.
    OAuth(OAuthCredential),
    /// API key-based authentication.
    ApiKey(ApiKeyCredential),
}

impl Credential {
    /// Credential type label.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Password(_) => "password",
            Self::OAuth(_) => "oauth",
            Self::ApiKey(_) => "api_key",
        }
    }

    /// Whether this credential has expired (if applicable).
    pub fn is_expired(&self, now: u64) -> bool {
        match self {
            Self::Password(p) => p.requires_reset,
            Self::OAuth(o) => o.expires_at.map_or(false, |exp| now > exp),
            Self::ApiKey(k) => k.expires_at.map_or(false, |exp| now > exp),
        }
    }
}

// ── Password ─────────────────────────────────────────────────────────

/// Password credential (stores hash, never plaintext).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordCredential {
    /// The password hash (e.g., Argon2id output).
    pub hash: String,
    /// Salt used for hashing.
    pub salt: String,
    /// Algorithm used.
    pub algorithm: HashAlgorithm,
    /// When the password was last changed (Unix timestamp).
    pub changed_at: u64,
    /// Whether the user must reset their password on next login.
    pub requires_reset: bool,
}

impl PasswordCredential {
    /// Create a new password credential.
    pub fn new(hash: impl Into<String>, salt: impl Into<String>, algorithm: HashAlgorithm) -> Self {
        Self {
            hash: hash.into(),
            salt: salt.into(),
            algorithm,
            changed_at: crate::user::current_timestamp(),
            requires_reset: false,
        }
    }

    /// Create with a reset requirement.
    pub fn requiring_reset(hash: impl Into<String>, salt: impl Into<String>, algorithm: HashAlgorithm) -> Self {
        let mut cred = Self::new(hash, salt, algorithm);
        cred.requires_reset = true;
        cred
    }
}

/// Hash algorithm used for password storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HashAlgorithm {
    Argon2id,
    Bcrypt,
    Scrypt,
    Pbkdf2,
}

impl HashAlgorithm {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Argon2id => "argon2id",
            Self::Bcrypt => "bcrypt",
            Self::Scrypt => "scrypt",
            Self::Pbkdf2 => "pbkdf2",
        }
    }
}

// ── OAuth ────────────────────────────────────────────────────────────

/// OAuth2 credential (access + refresh tokens).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthCredential {
    /// OAuth provider (matches AuthProvider).
    pub provider: crate::user::AuthProvider,
    /// Provider-specific user ID.
    pub provider_user_id: String,
    /// Access token (encrypted at rest in production).
    pub access_token: Option<String>,
    /// Refresh token (encrypted at rest in production).
    pub refresh_token: Option<String>,
    /// When the access token expires (Unix timestamp).
    pub expires_at: Option<u64>,
    /// Granted OAuth scopes.
    pub scopes: Vec<String>,
}

impl OAuthCredential {
    pub fn new(
        provider: crate::user::AuthProvider,
        provider_user_id: impl Into<String>,
    ) -> Self {
        Self {
            provider,
            provider_user_id: provider_user_id.into(),
            access_token: None,
            refresh_token: None,
            expires_at: None,
            scopes: Vec::new(),
        }
    }

    pub fn with_tokens(
        mut self,
        access_token: impl Into<String>,
        refresh_token: Option<String>,
        expires_at: Option<u64>,
    ) -> Self {
        self.access_token = Some(access_token.into());
        self.refresh_token = refresh_token;
        self.expires_at = expires_at;
        self
    }

    pub fn with_scopes(mut self, scopes: Vec<String>) -> Self {
        self.scopes = scopes;
        self
    }

    pub fn is_expired(&self, now: u64) -> bool {
        self.expires_at.map_or(false, |exp| now > exp)
    }

    pub fn needs_refresh(&self, now: u64) -> bool {
        // Refresh if within 5 minutes of expiry
        self.expires_at.map_or(false, |exp| now + 300 > exp)
    }
}

// ── API Key ──────────────────────────────────────────────────────────

/// API key credential for programmatic access.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyCredential {
    /// Hash of the API key (never store plaintext).
    pub key_hash: String,
    /// First 8 characters of the key for identification.
    pub prefix: String,
    /// Human-readable name for the key.
    pub name: String,
    /// When the key was created (Unix timestamp).
    pub created_at: u64,
    /// When the key expires (None = never).
    pub expires_at: Option<u64>,
    /// When the key was last used.
    pub last_used_at: Option<u64>,
    /// Permissions this key grants (scoped subset of user's permissions).
    pub scopes: PermissionSet,
}

impl ApiKeyCredential {
    pub fn new(key_hash: impl Into<String>, prefix: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            key_hash: key_hash.into(),
            prefix: prefix.into(),
            name: name.into(),
            created_at: crate::user::current_timestamp(),
            expires_at: None,
            last_used_at: None,
            scopes: PermissionSet::EMPTY,
        }
    }

    pub fn with_expiry(mut self, expires_at: u64) -> Self {
        self.expires_at = Some(expires_at);
        self
    }

    pub fn with_scopes(mut self, scopes: PermissionSet) -> Self {
        self.scopes = scopes;
        self
    }

    pub fn record_usage(&mut self) {
        self.last_used_at = Some(crate::user::current_timestamp());
    }

    pub fn is_expired(&self, now: u64) -> bool {
        self.expires_at.map_or(false, |exp| now > exp)
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permission::Permission;
    use crate::user::AuthProvider;

    #[test]
    fn credential_kind() {
        let pw = Credential::Password(PasswordCredential::new("hash", "salt", HashAlgorithm::Argon2id));
        assert_eq!(pw.kind(), "password");

        let oauth = Credential::OAuth(OAuthCredential::new(AuthProvider::Google, "123"));
        assert_eq!(oauth.kind(), "oauth");

        let api = Credential::ApiKey(ApiKeyCredential::new("hash", "lk_abcd", "My Key"));
        assert_eq!(api.kind(), "api_key");
    }

    #[test]
    fn credential_expired() {
        let now = crate::user::current_timestamp();

        let pw = Credential::Password(PasswordCredential::new("h", "s", HashAlgorithm::Argon2id));
        assert!(!pw.is_expired(now));

        let pw_reset = Credential::Password(PasswordCredential::requiring_reset("h", "s", HashAlgorithm::Argon2id));
        assert!(pw_reset.is_expired(now));

        let oauth = Credential::OAuth(
            OAuthCredential::new(AuthProvider::GitHub, "u1")
                .with_tokens("tok", None, Some(now - 100))
        );
        assert!(oauth.is_expired(now));

        let oauth_fresh = Credential::OAuth(
            OAuthCredential::new(AuthProvider::GitHub, "u1")
                .with_tokens("tok", None, Some(now + 3600))
        );
        assert!(!oauth_fresh.is_expired(now));
    }

    #[test]
    fn password_credential_new() {
        let cred = PasswordCredential::new("argon2hash", "randomsalt", HashAlgorithm::Argon2id);
        assert_eq!(cred.hash, "argon2hash");
        assert_eq!(cred.salt, "randomsalt");
        assert_eq!(cred.algorithm, HashAlgorithm::Argon2id);
        assert!(!cred.requires_reset);
    }

    #[test]
    fn hash_algorithm_label() {
        assert_eq!(HashAlgorithm::Argon2id.label(), "argon2id");
        assert_eq!(HashAlgorithm::Bcrypt.label(), "bcrypt");
        assert_eq!(HashAlgorithm::Scrypt.label(), "scrypt");
        assert_eq!(HashAlgorithm::Pbkdf2.label(), "pbkdf2");
    }

    #[test]
    fn oauth_credential_builder() {
        let cred = OAuthCredential::new(AuthProvider::Google, "google-uid-123")
            .with_tokens("access_tok", Some("refresh_tok".into()), Some(1000))
            .with_scopes(vec!["email".into(), "profile".into()]);
        assert_eq!(cred.provider, AuthProvider::Google);
        assert_eq!(cred.provider_user_id, "google-uid-123");
        assert_eq!(cred.access_token.as_deref(), Some("access_tok"));
        assert_eq!(cred.refresh_token.as_deref(), Some("refresh_tok"));
        assert_eq!(cred.scopes.len(), 2);
    }

    #[test]
    fn oauth_needs_refresh() {
        let now = crate::user::current_timestamp();
        let cred = OAuthCredential::new(AuthProvider::GitHub, "u1")
            .with_tokens("tok", Some("ref".into()), Some(now + 100));
        assert!(cred.needs_refresh(now)); // Within 5 min

        let cred2 = OAuthCredential::new(AuthProvider::GitHub, "u1")
            .with_tokens("tok", Some("ref".into()), Some(now + 600));
        assert!(!cred2.needs_refresh(now)); // > 5 min out
    }

    #[test]
    fn api_key_credential() {
        let mut key = ApiKeyCredential::new("sha256hash", "lk_abcd", "CI Key")
            .with_expiry(9999999999)
            .with_scopes({
                let mut ps = PermissionSet::new();
                ps.grant(Permission::ViewDocument);
                ps.grant(Permission::ExportDocument);
                ps
            });
        assert_eq!(key.prefix, "lk_abcd");
        assert_eq!(key.name, "CI Key");
        assert!(key.scopes.has(Permission::ViewDocument));
        assert!(key.scopes.has(Permission::ExportDocument));
        assert!(!key.scopes.has(Permission::DeleteDocument));
        assert!(key.last_used_at.is_none());

        key.record_usage();
        assert!(key.last_used_at.is_some());
    }

    #[test]
    fn credential_serde_roundtrip() {
        let cred = Credential::Password(PasswordCredential::new("h", "s", HashAlgorithm::Argon2id));
        let json = serde_json::to_string(&cred).unwrap();
        let back: Credential = serde_json::from_str(&json).unwrap();
        assert_eq!(back.kind(), "password");
    }
}
