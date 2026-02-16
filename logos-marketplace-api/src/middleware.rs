//! Authentication middleware — validates requests and enforces access control.
//!
//! Works with the `ApiRequest.publisher_key` field which is extracted
//! from the Bearer token during request parsing via `with_auth`.

use crate::{ApiError, ApiResult};
use crate::request::ApiRequest;

/// Authentication level required for an endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthLevel {
    /// No authentication required.
    Public,
    /// Must have a valid session / publisher key.
    Authenticated,
    /// Must be a verified publisher.
    Publisher,
    /// Must be a moderator / admin.
    Moderator,
}

/// Authentication middleware that validates requests against authorization policies.
pub struct AuthMiddleware {
    moderator_keys: Vec<String>,
}

impl AuthMiddleware {
    pub fn new() -> Self {
        Self {
            moderator_keys: Vec::new(),
        }
    }

    /// Add a moderator public key.
    pub fn add_moderator_key(&mut self, key: String) {
        self.moderator_keys.push(key);
    }

    /// Check if a key is a moderator.
    pub fn is_moderator(&self, public_key_hex: &str) -> bool {
        self.moderator_keys.iter().any(|k| k == public_key_hex)
    }

    /// Validate a request against the given auth level.
    ///
    /// Returns `Ok(Some(publisher_key))` for authenticated requests,
    /// `Ok(None)` for public endpoints.
    pub fn validate(
        &self,
        request: &ApiRequest,
        level: AuthLevel,
    ) -> ApiResult<Option<String>> {
        match level {
            AuthLevel::Public => Ok(None),
            AuthLevel::Authenticated | AuthLevel::Publisher | AuthLevel::Moderator => {
                let key = request
                    .publisher_key
                    .as_ref()
                    .ok_or_else(|| ApiError::Unauthorized("Missing authentication".into()))?;

                if level == AuthLevel::Moderator && !self.is_moderator(key) {
                    return Err(ApiError::Forbidden("Moderator access required".into()));
                }

                Ok(Some(key.clone()))
            }
        }
    }

    /// Extract publisher key from request without strict validation (for optional auth).
    pub fn extract_publisher_key(&self, request: &ApiRequest) -> Option<String> {
        request.publisher_key.clone()
    }
}

impl Default for AuthMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request::ApiRequest;

    #[test]
    fn test_public_auth() {
        let mw = AuthMiddleware::new();
        let req = ApiRequest::empty();
        let result = mw.validate(&req, AuthLevel::Public);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_authenticated_requires_key() {
        let mw = AuthMiddleware::new();
        let req = ApiRequest::empty();
        let result = mw.validate(&req, AuthLevel::Authenticated);
        assert!(result.is_err());
    }

    #[test]
    fn test_authenticated_with_key() {
        let mw = AuthMiddleware::new();
        let req = ApiRequest::empty().with_auth("pub_key_123");
        let result = mw.validate(&req, AuthLevel::Authenticated);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some("pub_key_123".to_string()));
    }

    #[test]
    fn test_moderator_access() {
        let mut mw = AuthMiddleware::new();
        mw.add_moderator_key("mod_key".to_string());

        // Moderator key → OK
        let req = ApiRequest::empty().with_auth("mod_key");
        let result = mw.validate(&req, AuthLevel::Moderator);
        assert!(result.is_ok());

        // Non-moderator key → Forbidden
        let req2 = ApiRequest::empty().with_auth("user_key");
        let result2 = mw.validate(&req2, AuthLevel::Moderator);
        assert!(result2.is_err());
    }

    #[test]
    fn test_extract_optional() {
        let mw = AuthMiddleware::new();

        let req1 = ApiRequest::empty();
        assert!(mw.extract_publisher_key(&req1).is_none());

        let req2 = ApiRequest::empty().with_auth("key_hex");
        assert_eq!(mw.extract_publisher_key(&req2), Some("key_hex".to_string()));
    }
}
