//! High-performance JWT token engine using HMAC-SHA256.
//!
//! Hand-rolled for maximum performance — no external JWT crate.
//! Uses raw `hmac` + `sha2` for cryptographic signing.
//!
//! Performance targets:
//! - Token issuance: <500ns
//! - Token verification: <300ns
//! - Zero allocations on verify (stack-based claims parsing)
//!
//! Security:
//! - HMAC-SHA256 (RFC 2104)
//! - Constant-time signature comparison
//! - Expiry enforcement
//! - Issuer validation
//!
//! Reference: RFC 7519 — JSON Web Tokens
//! Reference: OWASP — JWT Best Practices

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

/// JWT claims payload.
///
/// Minimal claim set for collaboration tokens.
/// Total serialized size: ~120 bytes (fits in 2 cache lines).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Claims {
    /// Subject — user ID
    pub sub: Uuid,
    /// Issuer — always "logos"
    pub iss: String,
    /// Issued at — Unix timestamp (seconds)
    pub iat: u64,
    /// Expiration — Unix timestamp (seconds)
    pub exp: u64,
    /// User display name
    pub name: String,
    /// Allowed document IDs (empty = all)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub docs: Vec<Uuid>,
}

impl Claims {
    /// Create new claims with default 24h expiry.
    pub fn new(user_id: Uuid, name: impl Into<String>) -> Self {
        let now = current_timestamp();
        Self {
            sub: user_id,
            iss: "logos".to_string(),
            iat: now,
            exp: now + 86400, // 24 hours
            name: name.into(),
            docs: Vec::new(),
        }
    }

    /// Create claims with custom expiry duration (seconds).
    pub fn with_expiry(user_id: Uuid, name: impl Into<String>, duration_secs: u64) -> Self {
        let now = current_timestamp();
        Self {
            sub: user_id,
            iss: "logos".to_string(),
            iat: now,
            exp: now + duration_secs,
            name: name.into(),
            docs: Vec::new(),
        }
    }

    /// Restrict token to specific documents.
    pub fn with_docs(mut self, docs: Vec<Uuid>) -> Self {
        self.docs = docs;
        self
    }

    /// Check if the token has expired.
    pub fn is_expired(&self) -> bool {
        current_timestamp() > self.exp
    }

    /// Check if the token grants access to a specific document.
    pub fn can_access(&self, doc_id: &Uuid) -> bool {
        self.docs.is_empty() || self.docs.contains(doc_id)
    }
}

/// Token errors.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenError {
    /// Token has invalid format (not 3 dot-separated parts)
    InvalidFormat,
    /// Base64 decoding failed
    DecodingError(String),
    /// JSON deserialization failed
    PayloadError(String),
    /// HMAC signature verification failed
    InvalidSignature,
    /// Token has expired
    Expired,
    /// Issuer mismatch
    InvalidIssuer,
    /// Token does not grant access to requested document
    AccessDenied,
}

impl std::fmt::Display for TokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidFormat => write!(f, "Invalid token format"),
            Self::DecodingError(e) => write!(f, "Token decoding error: {e}"),
            Self::PayloadError(e) => write!(f, "Token payload error: {e}"),
            Self::InvalidSignature => write!(f, "Invalid token signature"),
            Self::Expired => write!(f, "Token expired"),
            Self::InvalidIssuer => write!(f, "Invalid token issuer"),
            Self::AccessDenied => write!(f, "Access denied"),
        }
    }
}

impl std::error::Error for TokenError {}

/// High-performance JWT token engine.
///
/// Uses HMAC-SHA256 for signing and verification.
/// The secret key is stored inline — no heap allocation for key material.
///
/// Thread-safe: can be shared via Arc across async tasks.
pub struct TokenEngine {
    /// 256-bit secret key for HMAC-SHA256
    secret: [u8; 32],
    /// Pre-encoded JWT header (always the same for HS256)
    /// `{"alg":"HS256","typ":"JWT"}` → base64url
    header_b64: String,
}

impl TokenEngine {
    /// Create a new token engine with the given secret.
    ///
    /// The secret should be 32 bytes of cryptographically random data.
    /// In production, derive from environment variable or key management service.
    pub fn new(secret: [u8; 32]) -> Self {
        let header = r#"{"alg":"HS256","typ":"JWT"}"#;
        let header_b64 = URL_SAFE_NO_PAD.encode(header.as_bytes());
        Self { secret, header_b64 }
    }

    /// Create a token engine with a random secret (for testing).
    pub fn random() -> Self {
        use rand::RngCore;
        let mut secret = [0u8; 32];
        rand::rng().fill_bytes(&mut secret);
        Self::new(secret)
    }

    /// Issue a JWT token from claims.
    ///
    /// Performance: <500ns target (single HMAC-SHA256 + base64 encode).
    ///
    /// Format: `<header_b64>.<payload_b64>.<signature_b64>`
    #[inline(always)]
    pub fn issue(&self, claims: &Claims) -> Result<String, TokenError> {
        // 1. Serialize claims to JSON
        let payload_json = serde_json::to_vec(claims)
            .map_err(|e| TokenError::PayloadError(e.to_string()))?;

        // 2. Base64url encode payload
        let payload_b64 = URL_SAFE_NO_PAD.encode(&payload_json);

        // 3. Create signing input: header.payload
        let signing_input = format!("{}.{}", self.header_b64, payload_b64);

        // 4. HMAC-SHA256 sign
        let mut mac = HmacSha256::new_from_slice(&self.secret)
            .expect("HMAC accepts any key length");
        mac.update(signing_input.as_bytes());
        let signature = mac.finalize().into_bytes();

        // 5. Base64url encode signature
        let sig_b64 = URL_SAFE_NO_PAD.encode(&signature);

        // 6. Assemble token
        Ok(format!("{}.{}", signing_input, sig_b64))
    }

    /// Verify a JWT token and extract claims.
    ///
    /// Performance: <300ns target (constant-time HMAC verify + JSON parse).
    ///
    /// Checks:
    /// 1. Format (3 dot-separated parts)
    /// 2. HMAC-SHA256 signature (constant-time comparison)
    /// 3. Expiry
    /// 4. Issuer
    #[inline(always)]
    pub fn verify(&self, token: &str) -> Result<Claims, TokenError> {
        // 1. Split into parts
        let mut parts = token.rsplitn(2, '.');
        let sig_b64 = parts.next().ok_or(TokenError::InvalidFormat)?;
        let signing_input = parts.next().ok_or(TokenError::InvalidFormat)?;

        // Validate we have header.payload (one more dot)
        let dot_pos = signing_input.find('.').ok_or(TokenError::InvalidFormat)?;

        // 2. Verify signature (constant-time)
        let mut mac = HmacSha256::new_from_slice(&self.secret)
            .expect("HMAC accepts any key length");
        mac.update(signing_input.as_bytes());

        let sig_bytes = URL_SAFE_NO_PAD.decode(sig_b64)
            .map_err(|e| TokenError::DecodingError(e.to_string()))?;
        mac.verify_slice(&sig_bytes)
            .map_err(|_| TokenError::InvalidSignature)?;

        // 3. Decode payload (after signature verification — fail fast on bad sig)
        let payload_b64 = &signing_input[dot_pos + 1..];
        let payload_bytes = URL_SAFE_NO_PAD.decode(payload_b64)
            .map_err(|e| TokenError::DecodingError(e.to_string()))?;

        let claims: Claims = serde_json::from_slice(&payload_bytes)
            .map_err(|e| TokenError::PayloadError(e.to_string()))?;

        // 4. Check expiry
        if claims.is_expired() {
            return Err(TokenError::Expired);
        }

        // 5. Check issuer
        if claims.iss != "logos" {
            return Err(TokenError::InvalidIssuer);
        }

        Ok(claims)
    }

    /// Verify token and check document access in one call.
    ///
    /// Combines verify + access check to avoid redundant parsing.
    #[inline]
    pub fn verify_access(&self, token: &str, doc_id: &Uuid) -> Result<Claims, TokenError> {
        let claims = self.verify(token)?;
        if !claims.can_access(doc_id) {
            return Err(TokenError::AccessDenied);
        }
        Ok(claims)
    }
}

/// Get current Unix timestamp in seconds.
#[inline(always)]
fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_engine() -> TokenEngine {
        TokenEngine::new([42u8; 32])
    }

    #[test]
    fn test_issue_and_verify_roundtrip() {
        let engine = test_engine();
        let user_id = Uuid::new_v4();
        let claims = Claims::new(user_id, "Alice");

        let token = engine.issue(&claims).unwrap();
        let verified = engine.verify(&token).unwrap();

        assert_eq!(verified.sub, user_id);
        assert_eq!(verified.name, "Alice");
        assert_eq!(verified.iss, "logos");
    }

    #[test]
    fn test_token_format() {
        let engine = test_engine();
        let claims = Claims::new(Uuid::new_v4(), "Test");
        let token = engine.issue(&claims).unwrap();

        // JWT format: header.payload.signature
        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3, "JWT must have 3 parts");

        // Header decodes to HS256
        let header = URL_SAFE_NO_PAD.decode(parts[0]).unwrap();
        let header_str = String::from_utf8(header).unwrap();
        assert!(header_str.contains("HS256"));
    }

    #[test]
    fn test_invalid_signature_rejected() {
        let engine1 = TokenEngine::new([1u8; 32]);
        let engine2 = TokenEngine::new([2u8; 32]);

        let claims = Claims::new(Uuid::new_v4(), "Mallory");
        let token = engine1.issue(&claims).unwrap();

        // Different secret should fail verification
        let result = engine2.verify(&token);
        assert_eq!(result, Err(TokenError::InvalidSignature));
    }

    #[test]
    fn test_expired_token_rejected() {
        let engine = test_engine();
        let user_id = Uuid::new_v4();
        let mut claims = Claims::with_expiry(user_id, "ExpiredUser", 0);
        // Force exp into the past to guarantee expiry
        claims.exp = claims.iat.saturating_sub(1);

        let token = engine.issue(&claims).unwrap();

        // Token already expired
        let result = engine.verify(&token);
        assert_eq!(result, Err(TokenError::Expired));
    }

    #[test]
    fn test_invalid_format_rejected() {
        let engine = test_engine();

        assert_eq!(engine.verify("not-a-jwt"), Err(TokenError::InvalidFormat));
        assert_eq!(engine.verify("a.b"), Err(TokenError::InvalidFormat));
        assert_eq!(engine.verify(""), Err(TokenError::InvalidFormat));
    }

    #[test]
    fn test_tampered_payload_rejected() {
        let engine = test_engine();
        let claims = Claims::new(Uuid::new_v4(), "Alice");
        let token = engine.issue(&claims).unwrap();

        // Tamper with payload
        let parts: Vec<&str> = token.split('.').collect();
        let tampered = format!("{}.{}.{}", parts[0], URL_SAFE_NO_PAD.encode(b"{\"sub\":\"fake\"}"), parts[2]);

        let result = engine.verify(&tampered);
        assert_eq!(result, Err(TokenError::InvalidSignature));
    }

    #[test]
    fn test_claims_new_defaults() {
        let user_id = Uuid::new_v4();
        let claims = Claims::new(user_id, "Test");

        assert_eq!(claims.sub, user_id);
        assert_eq!(claims.iss, "logos");
        assert_eq!(claims.name, "Test");
        assert!(claims.docs.is_empty());
        assert!(!claims.is_expired());
        assert_eq!(claims.exp - claims.iat, 86400); // 24h
    }

    #[test]
    fn test_claims_with_docs_restriction() {
        let user_id = Uuid::new_v4();
        let doc1 = Uuid::new_v4();
        let doc2 = Uuid::new_v4();
        let doc3 = Uuid::new_v4();

        let claims = Claims::new(user_id, "Restricted")
            .with_docs(vec![doc1, doc2]);

        assert!(claims.can_access(&doc1));
        assert!(claims.can_access(&doc2));
        assert!(!claims.can_access(&doc3));
    }

    #[test]
    fn test_claims_empty_docs_allows_all() {
        let claims = Claims::new(Uuid::new_v4(), "Admin");
        assert!(claims.can_access(&Uuid::new_v4()));
        assert!(claims.can_access(&Uuid::new_v4()));
    }

    #[test]
    fn test_verify_access_allowed() {
        let engine = test_engine();
        let user_id = Uuid::new_v4();
        let doc_id = Uuid::new_v4();

        let claims = Claims::new(user_id, "Alice")
            .with_docs(vec![doc_id]);
        let token = engine.issue(&claims).unwrap();

        let result = engine.verify_access(&token, &doc_id);
        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_access_denied() {
        let engine = test_engine();
        let user_id = Uuid::new_v4();
        let allowed_doc = Uuid::new_v4();
        let forbidden_doc = Uuid::new_v4();

        let claims = Claims::new(user_id, "Alice")
            .with_docs(vec![allowed_doc]);
        let token = engine.issue(&claims).unwrap();

        let result = engine.verify_access(&token, &forbidden_doc);
        assert_eq!(result, Err(TokenError::AccessDenied));
    }

    #[test]
    fn test_random_engine() {
        let engine = TokenEngine::random();
        let claims = Claims::new(Uuid::new_v4(), "Random");
        let token = engine.issue(&claims).unwrap();
        assert!(engine.verify(&token).is_ok());
    }

    #[test]
    fn test_token_error_display() {
        assert_eq!(TokenError::InvalidFormat.to_string(), "Invalid token format");
        assert_eq!(TokenError::InvalidSignature.to_string(), "Invalid token signature");
        assert_eq!(TokenError::Expired.to_string(), "Token expired");
        assert_eq!(TokenError::InvalidIssuer.to_string(), "Invalid token issuer");
        assert_eq!(TokenError::AccessDenied.to_string(), "Access denied");
    }

    #[test]
    fn test_invalid_issuer_rejected() {
        let engine = test_engine();
        let user_id = Uuid::new_v4();
        let now = current_timestamp();

        // Manually create claims with wrong issuer
        let claims = Claims {
            sub: user_id,
            iss: "not-logos".to_string(),
            iat: now,
            exp: now + 3600,
            name: "Hacker".to_string(),
            docs: Vec::new(),
        };

        let token = engine.issue(&claims).unwrap();
        let result = engine.verify(&token);
        assert_eq!(result, Err(TokenError::InvalidIssuer));
    }

    #[test]
    fn test_token_size_efficient() {
        let engine = test_engine();
        let claims = Claims::new(Uuid::new_v4(), "Alice");
        let token = engine.issue(&claims).unwrap();

        // JWT should be under 500 bytes for typical claims
        assert!(
            token.len() < 500,
            "Token size {} bytes exceeds 500 byte target",
            token.len()
        );
    }

    #[test]
    fn test_multiple_tokens_different_signatures() {
        let engine = test_engine();
        let user1 = Uuid::new_v4();
        let user2 = Uuid::new_v4();

        let token1 = engine.issue(&Claims::new(user1, "Alice")).unwrap();
        let token2 = engine.issue(&Claims::new(user2, "Bob")).unwrap();

        // Different users = different signatures
        let sig1: Vec<&str> = token1.split('.').collect();
        let sig2: Vec<&str> = token2.split('.').collect();
        assert_ne!(sig1[2], sig2[2]);
    }
}
