//! OpenID Connect token exchange and claims validation.
//!
//! Models the OIDC authorization-code flow: the exchange of an
//! authorization code for an access + ID token, and the subsequent
//! extraction of user claims from the ID token payload.

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ── Error ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Error, PartialEq, Eq)]
pub enum OidcError {
    #[error("token expired")]
    TokenExpired,
    #[error("invalid issuer: expected '{expected}', got '{got}'")]
    InvalidIssuer { expected: String, got: String },
    #[error("invalid audience: '{0}'")]
    InvalidAudience(String),
    #[error("invalid token format: {0}")]
    InvalidFormat(String),
    #[error("nonce mismatch")]
    NonceMismatch,
    #[error("missing claim: {0}")]
    MissingClaim(String),
}

// ── Claims ────────────────────────────────────────────────────────────────────

/// Standard OIDC ID-token claims (`id_token` payload).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OidcClaims {
    /// Subject identifier (opaque user ID from the IdP).
    pub sub: String,
    /// Issuer of the token.
    pub iss: String,
    /// Audience (client_id).
    pub aud: String,
    /// Expiry (Unix timestamp).
    pub exp: u64,
    /// Issued at (Unix timestamp).
    pub iat: u64,
    /// Optional nonce echoed back from the authorization request.
    pub nonce: Option<String>,
    /// User's email address (if `email` scope was requested).
    pub email: Option<String>,
    /// Whether the email has been verified at the IdP.
    pub email_verified: Option<bool>,
    /// Given name.
    pub given_name: Option<String>,
    /// Family name.
    pub family_name: Option<String>,
    /// Roles / groups claim (non-standard but commonly added by enterprise IdPs).
    pub roles: Option<Vec<String>>,
}

impl OidcClaims {
    /// Display name: given+family name, or email, or sub.
    pub fn display_name(&self) -> &str {
        if let (Some(gn), Some(fn_)) = (&self.given_name, &self.family_name) {
            if !gn.is_empty() { return gn.as_str(); }
            if !fn_.is_empty() { return fn_.as_str(); }
        }
        self.email.as_deref().unwrap_or(&self.sub)
    }

    /// Validate exp, iss, aud, and optional nonce.
    pub fn validate(
        &self,
        now_ts: u64,
        expected_iss: &str,
        expected_aud: &str,
        nonce: Option<&str>,
    ) -> Result<(), OidcError> {
        if now_ts >= self.exp {
            return Err(OidcError::TokenExpired);
        }
        if self.iss != expected_iss {
            return Err(OidcError::InvalidIssuer {
                expected: expected_iss.to_string(),
                got: self.iss.clone(),
            });
        }
        if self.aud != expected_aud {
            return Err(OidcError::InvalidAudience(self.aud.clone()));
        }
        if let Some(n) = nonce {
            if self.nonce.as_deref() != Some(n) {
                return Err(OidcError::NonceMismatch);
            }
        }
        Ok(())
    }
}

// ── Token pair ────────────────────────────────────────────────────────────────

/// The token bundle returned by the token endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcToken {
    /// Opaque access token for calling resource APIs.
    pub access_token: String,
    /// Signed ID token (JWT format; we store it as a string for portability).
    pub id_token: String,
    /// Optional refresh token.
    pub refresh_token: Option<String>,
    /// Lifetime in seconds declared by the server.
    pub expires_in: u64,
    /// Token type (always "Bearer").
    pub token_type: String,
}

impl OidcToken {
    pub fn new(
        access_token: impl Into<String>,
        id_token: impl Into<String>,
        expires_in: u64,
    ) -> Self {
        Self {
            access_token: access_token.into(),
            id_token: id_token.into(),
            refresh_token: None,
            expires_in,
            token_type: "Bearer".to_string(),
        }
    }

    pub fn with_refresh_token(mut self, rt: impl Into<String>) -> Self {
        self.refresh_token = Some(rt.into());
        self
    }
}

// ── Exchange ──────────────────────────────────────────────────────────────────

/// Simulates the authorization-code → token exchange and ID-token parsing.
///
/// In production this would make an HTTP POST to the token endpoint and then
/// verify the JWT signature against the JWKS URI.  Here we accept a
/// JSON-encoded claims payload in lieu of a real JWT so tests are
/// deterministic without network I/O.
pub struct OidcExchange {
    pub issuer: String,
    pub client_id: String,
}

impl OidcExchange {
    pub fn new(issuer: impl Into<String>, client_id: impl Into<String>) -> Self {
        Self { issuer: issuer.into(), client_id: client_id.into() }
    }

    /// Parse and validate an ID-token whose payload is a plain JSON object
    /// (test-only convenience; production callers would split a real JWT).
    pub fn parse_id_token(
        &self,
        id_token_payload: &str,
        now_ts: u64,
        nonce: Option<&str>,
    ) -> Result<OidcClaims, OidcError> {
        let claims: OidcClaims = serde_json::from_str(id_token_payload)
            .map_err(|e| OidcError::InvalidFormat(e.to_string()))?;
        claims.validate(now_ts, &self.issuer, &self.client_id, nonce)?;
        Ok(claims)
    }

    /// Extract claims from an already-parsed `OidcToken.id_token` field
    /// that contains a JSON payload (test helper).
    pub fn extract_claims(&self, token: &OidcToken, now_ts: u64) -> Result<OidcClaims, OidcError> {
        self.parse_id_token(&token.id_token, now_ts, None)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_claims(exp: u64) -> OidcClaims {
        OidcClaims {
            sub: "u-001".into(),
            iss: "https://sso.corp.example/oidc".into(),
            aud: "logos-client".into(),
            exp,
            iat: 1_000_000,
            nonce: Some("abc123".into()),
            email: Some("user@corp.example".into()),
            email_verified: Some(true),
            given_name: Some("Alice".into()),
            family_name: Some("Smith".into()),
            roles: Some(vec!["publisher".into()]),
        }
    }

    #[test]
    fn claims_validate_ok() {
        let c = valid_claims(9_999_999_999);
        assert!(c.validate(1_000_001, "https://sso.corp.example/oidc", "logos-client", Some("abc123")).is_ok());
    }

    #[test]
    fn claims_expired_err() {
        let c = valid_claims(100);
        let err = c.validate(200, "https://sso.corp.example/oidc", "logos-client", None).unwrap_err();
        assert_eq!(err, OidcError::TokenExpired);
    }

    #[test]
    fn claims_invalid_issuer_err() {
        let c = valid_claims(9_999_999_999);
        let err = c.validate(1_000_001, "https://wrong.example", "logos-client", None).unwrap_err();
        assert!(matches!(err, OidcError::InvalidIssuer { .. }));
    }

    #[test]
    fn claims_nonce_mismatch_err() {
        let c = valid_claims(9_999_999_999);
        let err = c.validate(1_000_001, "https://sso.corp.example/oidc", "logos-client", Some("wrong")).unwrap_err();
        assert_eq!(err, OidcError::NonceMismatch);
    }

    #[test]
    fn claims_display_name_given_name() {
        let c = valid_claims(9_999_999_999);
        assert_eq!(c.display_name(), "Alice");
    }

    #[test]
    fn claims_display_name_falls_back_to_email() {
        let mut c = valid_claims(9_999_999_999);
        c.given_name = None;
        c.family_name = None;
        assert_eq!(c.display_name(), "user@corp.example");
    }

    #[test]
    fn claims_display_name_falls_back_to_sub() {
        let mut c = valid_claims(9_999_999_999);
        c.given_name = None;
        c.family_name = None;
        c.email = None;
        assert_eq!(c.display_name(), "u-001");
    }

    #[test]
    fn oidc_token_new() {
        let tok = OidcToken::new("acc-tok", "{}", 3600);
        assert_eq!(tok.token_type, "Bearer");
        assert_eq!(tok.expires_in, 3600);
    }

    #[test]
    fn exchange_parse_id_token_ok() {
        let exch = OidcExchange::new("https://sso.corp.example/oidc", "logos-client");
        let payload = serde_json::to_string(&valid_claims(9_999_999_999)).unwrap();
        let claims = exch.parse_id_token(&payload, 1_000_001, None).unwrap();
        assert_eq!(claims.sub, "u-001");
    }

    #[test]
    fn exchange_parse_invalid_json_err() {
        let exch = OidcExchange::new("issuer", "client");
        assert!(matches!(exch.parse_id_token("bad-json", 0, None), Err(OidcError::InvalidFormat(_))));
    }
}
