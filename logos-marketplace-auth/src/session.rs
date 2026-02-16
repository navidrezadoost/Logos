//! Session token management for authenticated publishers.
//!
//! Publishers authenticate via challenge-response, then receive
//! a time-limited session token for subsequent API calls.

use crate::crypto::{sha256, PublicKey};
use crate::{AuthError, AuthResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use uuid::Uuid;

/// A session token for authenticated API access.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionToken {
    /// Token value (hex-encoded random bytes)
    token: String,
    /// Publisher's public key hex
    publisher_key: String,
}

impl SessionToken {
    /// Create a new random session token.
    pub fn new(publisher_key: &PublicKey) -> Self {
        let random = Self::random_token();
        Self {
            token: random,
            publisher_key: publisher_key.to_hex(),
        }
    }

    /// Get the token string.
    pub fn token(&self) -> &str {
        &self.token
    }

    /// Get the publisher key hex.
    pub fn publisher_key_hex(&self) -> &str {
        &self.publisher_key
    }

    fn random_token() -> String {
        // Use UUID v4 + timestamp hash for uniqueness
        let id = Uuid::new_v4();
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_nanos();
        let combined = format!("{id}{now}");
        let hash = sha256(combined.as_bytes());
        hash.iter().map(|b| format!("{b:02x}")).collect()
    }
}

impl std::fmt::Display for SessionToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}…", &self.token[..16])
    }
}

/// Session errors.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SessionError {
    #[error("session expired")]
    Expired,
    #[error("session not found")]
    NotFound,
    #[error("session revoked")]
    Revoked,
    #[error("max sessions exceeded")]
    MaxSessionsExceeded,
}

/// Internal session record.
#[derive(Debug, Clone)]
struct SessionRecord {
    token: SessionToken,
    publisher_key: PublicKey,
    created_at: u64,
    expires_at: u64,
    last_used_at: u64,
    revoked: bool,
}

impl SessionRecord {
    fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs();
        now > self.expires_at
    }

    fn is_valid(&self) -> bool {
        !self.revoked && !self.is_expired()
    }
}

/// Manages active sessions for authenticated publishers.
///
/// Performance:
/// - Session validation: O(1) HashMap lookup
/// - Session creation: O(1)
/// - Cleanup: O(n) — run periodically
pub struct SessionManager {
    /// Active sessions keyed by token string
    sessions: HashMap<String, SessionRecord>,
    /// Max sessions per publisher
    max_per_publisher: usize,
    /// Default session duration
    default_duration: Duration,
    /// Total sessions created (for stats)
    total_created: u64,
}

impl SessionManager {
    /// Create a new session manager.
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            max_per_publisher: 5,
            default_duration: Duration::from_secs(3600), // 1 hour
            total_created: 0,
        }
    }

    /// Create with custom settings.
    pub fn with_settings(max_per_publisher: usize, default_duration: Duration) -> Self {
        Self {
            sessions: HashMap::new(),
            max_per_publisher,
            default_duration,
            total_created: 0,
        }
    }

    /// Create a new session for a publisher.
    pub fn create_session(&mut self, publisher_key: PublicKey, duration: Duration) -> SessionToken {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs();

        let token = SessionToken::new(&publisher_key);
        let record = SessionRecord {
            token: token.clone(),
            publisher_key,
            created_at: now,
            expires_at: now + duration.as_secs(),
            last_used_at: now,
            revoked: false,
        };

        self.sessions.insert(token.token().to_string(), record);
        self.total_created += 1;
        token
    }

    /// Create a session with default duration.
    pub fn create_default_session(&mut self, publisher_key: PublicKey) -> SessionToken {
        let dur = self.default_duration;
        self.create_session(publisher_key, dur)
    }

    /// Validate a session token and return the publisher's public key.
    pub fn validate(&self, token: &SessionToken) -> AuthResult<PublicKey> {
        let record = self
            .sessions
            .get(token.token())
            .ok_or(AuthError::SessionNotFound(token.token().to_string()))?;

        if record.revoked {
            return Err(AuthError::SessionExpired);
        }

        if record.is_expired() {
            return Err(AuthError::SessionExpired);
        }

        Ok(record.publisher_key.clone())
    }

    /// Touch a session (update last_used_at).
    pub fn touch(&mut self, token: &SessionToken) -> AuthResult<()> {
        let record = self
            .sessions
            .get_mut(token.token())
            .ok_or(AuthError::SessionNotFound(token.token().to_string()))?;

        if !record.is_valid() {
            return Err(AuthError::SessionExpired);
        }

        record.last_used_at = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs();
        Ok(())
    }

    /// Revoke a specific session.
    pub fn revoke(&mut self, token: &SessionToken) {
        if let Some(record) = self.sessions.get_mut(token.token()) {
            record.revoked = true;
        }
    }

    /// Revoke all sessions for a publisher.
    pub fn revoke_all(&mut self, publisher_key: &PublicKey) {
        let key_hex = publisher_key.to_hex();
        for record in self.sessions.values_mut() {
            if record.token.publisher_key_hex() == key_hex {
                record.revoked = true;
            }
        }
    }

    /// Count active (non-expired, non-revoked) sessions.
    pub fn active_count(&self) -> usize {
        self.sessions.values().filter(|r| r.is_valid()).count()
    }

    /// Count sessions for a specific publisher.
    pub fn publisher_session_count(&self, publisher_key: &PublicKey) -> usize {
        let key_hex = publisher_key.to_hex();
        self.sessions
            .values()
            .filter(|r| r.token.publisher_key_hex() == key_hex && r.is_valid())
            .count()
    }

    /// Clean up expired sessions.
    pub fn cleanup_expired(&mut self) -> usize {
        let before = self.sessions.len();
        self.sessions.retain(|_, r| !r.is_expired());
        before - self.sessions.len()
    }

    /// Total sessions ever created.
    pub fn total_created(&self) -> u64 {
        self.total_created
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::Ed25519KeyPair;

    #[test]
    fn test_session_token_new() {
        let kp = Ed25519KeyPair::generate();
        let token = SessionToken::new(&kp.public_key());
        assert_eq!(token.token().len(), 64);
        assert_eq!(token.publisher_key_hex(), kp.public_key().to_hex());
    }

    #[test]
    fn test_session_token_unique() {
        let kp = Ed25519KeyPair::generate();
        let t1 = SessionToken::new(&kp.public_key());
        let t2 = SessionToken::new(&kp.public_key());
        assert_ne!(t1.token(), t2.token());
    }

    #[test]
    fn test_session_manager_create_validate() {
        let kp = Ed25519KeyPair::generate();
        let mut mgr = SessionManager::new();

        let token = mgr.create_session(kp.public_key(), Duration::from_secs(3600));
        let result = mgr.validate(&token);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), kp.public_key());
    }

    #[test]
    fn test_session_manager_revoke() {
        let kp = Ed25519KeyPair::generate();
        let mut mgr = SessionManager::new();

        let token = mgr.create_session(kp.public_key(), Duration::from_secs(3600));
        assert!(mgr.validate(&token).is_ok());

        mgr.revoke(&token);
        assert!(mgr.validate(&token).is_err());
    }

    #[test]
    fn test_session_manager_revoke_all() {
        let kp = Ed25519KeyPair::generate();
        let mut mgr = SessionManager::new();

        let t1 = mgr.create_session(kp.public_key(), Duration::from_secs(3600));
        let t2 = mgr.create_session(kp.public_key(), Duration::from_secs(3600));

        mgr.revoke_all(&kp.public_key());
        assert!(mgr.validate(&t1).is_err());
        assert!(mgr.validate(&t2).is_err());
    }

    #[test]
    fn test_session_manager_not_found() {
        let kp = Ed25519KeyPair::generate();
        let mgr = SessionManager::new();
        let fake_token = SessionToken::new(&kp.public_key());
        assert!(mgr.validate(&fake_token).is_err());
    }

    #[test]
    fn test_session_manager_active_count() {
        let kp1 = Ed25519KeyPair::generate();
        let kp2 = Ed25519KeyPair::generate();
        let mut mgr = SessionManager::new();

        mgr.create_session(kp1.public_key(), Duration::from_secs(3600));
        mgr.create_session(kp2.public_key(), Duration::from_secs(3600));

        assert_eq!(mgr.active_count(), 2);
        assert_eq!(mgr.publisher_session_count(&kp1.public_key()), 1);
    }

    #[test]
    fn test_session_manager_touch() {
        let kp = Ed25519KeyPair::generate();
        let mut mgr = SessionManager::new();

        let token = mgr.create_session(kp.public_key(), Duration::from_secs(3600));
        assert!(mgr.touch(&token).is_ok());
    }

    #[test]
    fn test_session_manager_total_created() {
        let kp = Ed25519KeyPair::generate();
        let mut mgr = SessionManager::new();

        mgr.create_session(kp.public_key(), Duration::from_secs(3600));
        mgr.create_session(kp.public_key(), Duration::from_secs(3600));
        assert_eq!(mgr.total_created(), 2);
    }
}
