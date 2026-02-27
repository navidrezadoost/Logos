//! Identity error types.

use crate::user::UserId;

/// Unified error type for identity operations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum IdentityError {
    #[error("user not found: {0}")]
    UserNotFound(UserId),

    #[error("duplicate email: {0}")]
    DuplicateEmail(String),

    #[error("invalid credentials")]
    InvalidCredentials,

    #[error("account suspended")]
    AccountSuspended,

    #[error("account not verified")]
    AccountNotVerified,

    #[error("session expired: {0}")]
    SessionExpired(String),

    #[error("session not found")]
    SessionNotFound,

    #[error("session revoked")]
    SessionRevoked,

    #[error("permission denied: {0}")]
    PermissionDenied(String),

    #[error("token error: {0}")]
    TokenError(String),

    #[error("invalid token format")]
    InvalidTokenFormat,

    #[error("token expired")]
    TokenExpired,

    #[error("provider error: {0}")]
    ProviderError(String),

    #[error("storage error: {0}")]
    StorageError(String),

    #[error("resource not found: {0}")]
    ResourceNotFound(String),

    #[error("capacity exceeded: {0}")]
    CapacityExceeded(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display() {
        let uid = UserId::nil();
        let e = IdentityError::UserNotFound(uid);
        assert!(e.to_string().contains("user not found"));
    }

    #[test]
    fn error_variants_are_clone() {
        let e = IdentityError::InvalidCredentials;
        let e2 = e.clone();
        assert_eq!(e.to_string(), e2.to_string());
    }

    #[test]
    fn all_variants_display() {
        let cases: Vec<IdentityError> = vec![
            IdentityError::UserNotFound(UserId::nil()),
            IdentityError::DuplicateEmail("a@b.c".into()),
            IdentityError::InvalidCredentials,
            IdentityError::AccountSuspended,
            IdentityError::AccountNotVerified,
            IdentityError::SessionExpired("s1".into()),
            IdentityError::SessionNotFound,
            IdentityError::SessionRevoked,
            IdentityError::PermissionDenied("read".into()),
            IdentityError::TokenError("bad".into()),
            IdentityError::InvalidTokenFormat,
            IdentityError::TokenExpired,
            IdentityError::ProviderError("google".into()),
            IdentityError::StorageError("disk".into()),
            IdentityError::ResourceNotFound("doc".into()),
            IdentityError::CapacityExceeded("max".into()),
            IdentityError::InvalidInput("empty".into()),
        ];
        for e in &cases {
            assert!(!e.to_string().is_empty());
        }
    }
}
