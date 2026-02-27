//! Token claims and provider trait.
//!
//! Replaces `logos_collab::auth::Claims` with a richer, role-aware
//! `TokenClaims` type. The `TokenProvider` trait abstracts over
//! signing algorithms (HMAC-SHA256, Ed25519, etc.).

use crate::permission::PermissionSet;
use crate::role::Role;
use crate::session::SessionId;
use crate::user::UserId;
use crate::error::IdentityError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// JWT-like token claims.
///
/// Superset of `logos_collab::auth::Claims`:
/// - Adds `role`, `permissions`, `email`, `session_id`
/// - Keeps `sub`, `iss`, `iat`, `exp`, `name`, `docs`
///
/// ## Migration from legacy Claims
///
/// | Legacy `Claims` field | `TokenClaims` field |
/// |-----------------------|---------------------|
/// | `sub: Uuid` | `sub: UserId` |
/// | `name: String` | `name: String` |
/// | `iss: String` | `iss: String` |
/// | `iat: u64` | `iat: u64` |
/// | `exp: u64` | `exp: u64` |
/// | `docs: Vec<Uuid>` | `document_ids: Vec<Uuid>` |
/// | — | `role: Role` |
/// | — | `permissions: PermissionSet` |
/// | — | `email: String` |
/// | — | `session_id: Option<SessionId>` |
/// | — | `jti: String` (unique token ID) |
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenClaims {
    /// Subject — user ID.
    pub sub: UserId,
    /// Issuer — always "logos".
    pub iss: String,
    /// Issued at — Unix timestamp (seconds).
    pub iat: u64,
    /// Expiration — Unix timestamp (seconds).
    pub exp: u64,
    /// Unique token identifier (for revocation).
    pub jti: String,
    /// User display name.
    pub name: String,
    /// User email.
    pub email: String,
    /// User's role (determines base permissions).
    pub role: Role,
    /// Effective permissions (may differ from role defaults if customized).
    pub permissions: PermissionSet,
    /// Associated session ID (if session-bound).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    /// Allowed document IDs (empty = all).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub document_ids: Vec<Uuid>,
}

impl TokenClaims {
    /// Create standard claims with role-based permissions.
    pub fn new(user_id: UserId, name: impl Into<String>, email: impl Into<String>, role: Role) -> Self {
        let now = crate::user::current_timestamp();
        Self {
            sub: user_id,
            iss: "logos".to_string(),
            iat: now,
            exp: now + 86400, // 24h default
            jti: Uuid::new_v4().to_string(),
            name: name.into(),
            email: email.into(),
            role,
            permissions: PermissionSet::for_role(role),
            session_id: None,
            document_ids: Vec::new(),
        }
    }

    /// Create claims with custom expiry.
    pub fn with_expiry(user_id: UserId, name: impl Into<String>, email: impl Into<String>, role: Role, duration_secs: u64) -> Self {
        let mut claims = Self::new(user_id, name, email, role);
        claims.exp = claims.iat + duration_secs;
        claims
    }

    /// Bind to a specific session.
    pub fn with_session(mut self, session_id: SessionId) -> Self {
        self.session_id = Some(session_id);
        self
    }

    /// Restrict to specific documents.
    pub fn with_documents(mut self, doc_ids: Vec<Uuid>) -> Self {
        self.document_ids = doc_ids;
        self
    }

    /// Override permissions (e.g., for scoped API keys).
    pub fn with_permissions(mut self, permissions: PermissionSet) -> Self {
        self.permissions = permissions;
        self
    }

    /// Whether this token has expired.
    pub fn is_expired(&self) -> bool {
        crate::user::current_timestamp() > self.exp
    }

    /// Whether this token grants access to a specific document.
    pub fn can_access_document(&self, doc_id: &Uuid) -> bool {
        self.document_ids.is_empty() || self.document_ids.contains(doc_id)
    }

    /// Check if the token has a specific permission.
    pub fn has_permission(&self, perm: crate::permission::Permission) -> bool {
        self.permissions.has(perm)
    }

    /// Remaining time before expiry (seconds).
    pub fn remaining(&self) -> u64 {
        self.exp.saturating_sub(crate::user::current_timestamp())
    }
}

/// Token kinds (for different purposes with different lifetimes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TokenKind {
    /// Short-lived access token (default 24h).
    Access,
    /// Long-lived refresh token (default 30d).
    Refresh,
    /// Email verification token (default 24h).
    Verification,
    /// Password reset token (default 1h).
    PasswordReset,
    /// API key (custom lifetime).
    ApiKey,
}

impl TokenKind {
    /// Default lifetime in seconds for this token kind.
    pub fn default_lifetime(&self) -> u64 {
        match self {
            Self::Access => 86400,         // 24h
            Self::Refresh => 2592000,      // 30d
            Self::Verification => 86400,   // 24h
            Self::PasswordReset => 3600,   // 1h
            Self::ApiKey => 31536000,      // 1 year
        }
    }
}

/// Trait for token issuance and verification.
///
/// Implemented by `logos-collab::auth::TokenEngine` (HMAC-SHA256)
/// and potentially by other crates for different algorithms.
pub trait TokenProvider {
    /// Issue a signed token from claims.
    fn issue(&self, claims: &TokenClaims, kind: TokenKind) -> Result<String, IdentityError>;

    /// Verify a token and extract claims.
    fn verify(&self, token: &str, kind: TokenKind) -> Result<TokenClaims, IdentityError>;

    /// Revoke a token by its JTI (unique ID).
    fn revoke(&mut self, token_id: &str) -> Result<(), IdentityError>;

    /// Check if a token has been revoked.
    fn is_revoked(&self, token_id: &str) -> bool;
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permission::Permission;

    #[test]
    fn token_claims_new() {
        let uid = UserId::new();
        let claims = TokenClaims::new(uid, "Alice", "alice@test.com", Role::Editor);
        assert_eq!(claims.sub, uid);
        assert_eq!(claims.name, "Alice");
        assert_eq!(claims.email, "alice@test.com");
        assert_eq!(claims.role, Role::Editor);
        assert_eq!(claims.iss, "logos");
        assert!(!claims.is_expired());
        assert!(claims.document_ids.is_empty());
        assert!(claims.session_id.is_none());
        assert!(claims.permissions.has(Permission::EditDocument));
    }

    #[test]
    fn token_claims_with_expiry() {
        let uid = UserId::new();
        let claims = TokenClaims::with_expiry(uid, "Bob", "bob@test.com", Role::Viewer, 7200);
        assert_eq!(claims.exp - claims.iat, 7200);
    }

    #[test]
    fn token_claims_with_session() {
        let uid = UserId::new();
        let sid = SessionId::new();
        let claims = TokenClaims::new(uid, "A", "a@b.c", Role::Editor)
            .with_session(sid);
        assert_eq!(claims.session_id, Some(sid));
    }

    #[test]
    fn token_claims_with_documents() {
        let uid = UserId::new();
        let doc1 = Uuid::new_v4();
        let doc2 = Uuid::new_v4();
        let doc3 = Uuid::new_v4();
        let claims = TokenClaims::new(uid, "A", "a@b.c", Role::Editor)
            .with_documents(vec![doc1, doc2]);
        assert!(claims.can_access_document(&doc1));
        assert!(claims.can_access_document(&doc2));
        assert!(!claims.can_access_document(&doc3));
    }

    #[test]
    fn token_claims_empty_docs_allows_all() {
        let uid = UserId::new();
        let claims = TokenClaims::new(uid, "A", "a@b.c", Role::Editor);
        assert!(claims.can_access_document(&Uuid::new_v4()));
    }

    #[test]
    fn token_claims_has_permission() {
        let uid = UserId::new();
        let claims = TokenClaims::new(uid, "A", "a@b.c", Role::Commenter);
        assert!(claims.has_permission(Permission::CreateComment));
        assert!(!claims.has_permission(Permission::EditDocument));
    }

    #[test]
    fn token_claims_with_permissions_override() {
        let uid = UserId::new();
        let mut custom = PermissionSet::new();
        custom.grant(Permission::ViewDocument);
        custom.grant(Permission::ExportDocument);
        let claims = TokenClaims::new(uid, "A", "a@b.c", Role::Editor)
            .with_permissions(custom);
        assert!(claims.has_permission(Permission::ViewDocument));
        assert!(claims.has_permission(Permission::ExportDocument));
        assert!(!claims.has_permission(Permission::EditDocument)); // Overridden
    }

    #[test]
    fn token_claims_remaining() {
        let uid = UserId::new();
        let claims = TokenClaims::new(uid, "A", "a@b.c", Role::Viewer);
        assert!(claims.remaining() > 86000);
        assert!(claims.remaining() <= 86400);
    }

    #[test]
    fn token_claims_expired() {
        let uid = UserId::new();
        let mut claims = TokenClaims::new(uid, "A", "a@b.c", Role::Viewer);
        claims.exp = claims.iat.saturating_sub(1);
        assert!(claims.is_expired());
    }

    #[test]
    fn token_kind_lifetimes() {
        assert_eq!(TokenKind::Access.default_lifetime(), 86400);
        assert_eq!(TokenKind::Refresh.default_lifetime(), 2592000);
        assert_eq!(TokenKind::Verification.default_lifetime(), 86400);
        assert_eq!(TokenKind::PasswordReset.default_lifetime(), 3600);
        assert_eq!(TokenKind::ApiKey.default_lifetime(), 31536000);
    }

    #[test]
    fn token_claims_jti_unique() {
        let uid = UserId::new();
        let c1 = TokenClaims::new(uid, "A", "a@b.c", Role::Viewer);
        let c2 = TokenClaims::new(uid, "A", "a@b.c", Role::Viewer);
        assert_ne!(c1.jti, c2.jti);
    }

    #[test]
    fn token_claims_serde_roundtrip() {
        let uid = UserId::new();
        let claims = TokenClaims::new(uid, "Test", "test@test.com", Role::Admin)
            .with_session(SessionId::new())
            .with_documents(vec![Uuid::new_v4()]);
        let json = serde_json::to_string(&claims).unwrap();
        let back: TokenClaims = serde_json::from_str(&json).unwrap();
        assert_eq!(back.sub, uid);
        assert_eq!(back.name, "Test");
        assert_eq!(back.role, Role::Admin);
        assert!(back.session_id.is_some());
        assert_eq!(back.document_ids.len(), 1);
    }
}
