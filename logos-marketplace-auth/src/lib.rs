//! # logos-marketplace-auth — Ed25519 Publisher Verification
//!
//! Provides cryptographic identity for the Logos Plugin Marketplace.
//! Publishers generate Ed25519 keypairs, register their public keys,
//! and sign plugin submissions. The marketplace verifies signatures
//! and manages publisher trust levels.
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────────────────────────────┐
//! │          PublisherIdentity           │
//! │  ┌────────────┬───────────────────┐  │
//! │  │  Ed25519   │  Challenge-Resp   │  │  key generation + verification
//! │  │  KeyPair   │  Protocol         │  │
//! │  └────────────┴───────────────────┘  │
//! │          SessionManager              │  token-based sessions
//! │  ┌────────────┬───────────────────┐  │
//! │  │  Tokens    │  Attestation      │  │  JWT-like + identity binding
//! │  └────────────┴───────────────────┘  │
//! └──────────────────────────────────────┘
//! ```
//!
//! ## Security Model
//!
//! - Ed25519 signatures (RFC 8032)
//! - SHA-256 content hashing
//! - Challenge-response for publisher verification
//! - Time-limited session tokens
//! - Key rotation support with revocation lists

pub mod crypto;
pub mod identity;
pub mod session;
pub mod challenge;
pub mod attestation;

pub use crypto::{Ed25519KeyPair, PublicKey, Signature, ContentDigest};
pub use identity::{PublisherIdentity, VerificationStatus, PublisherProfile};
pub use session::{SessionToken, SessionManager, SessionError};
pub use challenge::{Challenge, ChallengeResponse, ChallengeVerifier};
pub use attestation::{KeyAttestation, AttestationChain, AttestationType};

/// Auth errors.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AuthError {
    #[error("invalid signature")]
    InvalidSignature,
    #[error("key not found: {0}")]
    KeyNotFound(String),
    #[error("challenge expired")]
    ChallengeExpired,
    #[error("challenge mismatch")]
    ChallengeMismatch,
    #[error("session expired")]
    SessionExpired,
    #[error("session not found: {0}")]
    SessionNotFound(String),
    #[error("publisher not verified")]
    NotVerified,
    #[error("key revoked: {0}")]
    KeyRevoked(String),
    #[error("attestation invalid: {0}")]
    AttestationInvalid(String),
    #[error("duplicate key registration")]
    DuplicateKey,
    #[error("rate limited")]
    RateLimited,
    #[error("invalid token format")]
    InvalidToken,
}

pub type AuthResult<T> = Result<T, AuthError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_error_display() {
        assert_eq!(AuthError::InvalidSignature.to_string(), "invalid signature");
        assert_eq!(AuthError::ChallengeExpired.to_string(), "challenge expired");
        assert_eq!(
            AuthError::KeyRevoked("abc".into()).to_string(),
            "key revoked: abc"
        );
    }

    #[test]
    fn test_full_publisher_flow() {
        // 1. Generate keypair
        let kp = Ed25519KeyPair::generate();
        assert_eq!(kp.public_key().as_bytes().len(), 32);

        // 2. Sign some data
        let data = b"plugin-manifest-v1";
        let sig = kp.sign(data);
        assert!(kp.public_key().verify(data, &sig));

        // 3. Create identity
        let identity = PublisherIdentity::new("Test Publisher", kp.public_key());
        assert_eq!(identity.name(), "Test Publisher");
        assert_eq!(identity.status(), VerificationStatus::Pending);
    }

    #[test]
    fn test_challenge_response_flow() {
        let kp = Ed25519KeyPair::generate();
        let mut verifier = ChallengeVerifier::new();

        // Issue challenge
        let challenge = verifier.issue_challenge(&kp.public_key());

        // Publisher signs challenge
        let response = ChallengeResponse::sign(&kp, &challenge);

        // Verify response
        let result = verifier.verify_response(&response);
        assert!(result.is_ok());
    }

    #[test]
    fn test_session_management() {
        let kp = Ed25519KeyPair::generate();
        let mut sessions = SessionManager::new();

        // Create session
        let token = sessions.create_session(kp.public_key(), std::time::Duration::from_secs(3600));
        assert!(sessions.validate(&token).is_ok());

        // Revoke session
        sessions.revoke(&token);
        assert!(sessions.validate(&token).is_err());
    }

    #[test]
    fn test_key_attestation() {
        let kp = Ed25519KeyPair::generate();
        let attestation = KeyAttestation::self_signed(
            &kp,
            "publisher@example.com",
            AttestationType::EmailVerified,
        );
        assert!(attestation.verify(&kp.public_key()));
    }

    #[test]
    fn test_content_digest() {
        let d1 = ContentDigest::compute(b"hello world");
        let d2 = ContentDigest::compute(b"hello world");
        let d3 = ContentDigest::compute(b"different");
        assert_eq!(d1, d2);
        assert_ne!(d1, d3);
        assert_eq!(d1.to_hex().len(), 64); // SHA-256 = 32 bytes = 64 hex
    }
}
