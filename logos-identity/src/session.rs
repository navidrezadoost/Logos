//! Session management types.
//!
//! Replaces `logos_sync::session::SessionId` and provides the canonical
//! session lifecycle (create, validate, touch, revoke, expire).

use crate::user::UserId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Strongly-typed session identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(Uuid);

impl SessionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }

    pub fn as_uuid(&self) -> Uuid {
        self.0
    }

    pub fn nil() -> Self {
        Self(Uuid::nil())
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl From<Uuid> for SessionId {
    fn from(id: Uuid) -> Self {
        Self(id)
    }
}

impl From<SessionId> for Uuid {
    fn from(id: SessionId) -> Self {
        id.0
    }
}

/// A user session.
///
/// Sessions are created on login and tracked for security auditing,
/// concurrent session limits, and device management.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique session identifier.
    pub id: SessionId,
    /// The user this session belongs to.
    pub user_id: UserId,
    /// When the session was created (Unix timestamp).
    pub created_at: u64,
    /// When the session expires (Unix timestamp).
    pub expires_at: u64,
    /// Last activity timestamp.
    pub last_active_at: u64,
    /// Client IP address (for audit).
    pub ip_address: Option<String>,
    /// Client user agent string.
    pub user_agent: Option<String>,
    /// Device fingerprint for multi-device tracking.
    pub device_fingerprint: Option<String>,
    /// Whether this session has been explicitly revoked.
    pub is_revoked: bool,
}

impl Session {
    /// Create a new session with the given TTL (in seconds).
    pub fn new(user_id: UserId, ttl_secs: u64) -> Self {
        let now = crate::user::current_timestamp();
        Self {
            id: SessionId::new(),
            user_id,
            created_at: now,
            expires_at: now + ttl_secs,
            last_active_at: now,
            ip_address: None,
            user_agent: None,
            device_fingerprint: None,
            is_revoked: false,
        }
    }

    /// Create a session with device metadata.
    pub fn with_metadata(
        user_id: UserId,
        ttl_secs: u64,
        ip_address: Option<String>,
        user_agent: Option<String>,
        device_fingerprint: Option<String>,
    ) -> Self {
        let mut session = Self::new(user_id, ttl_secs);
        session.ip_address = ip_address;
        session.user_agent = user_agent;
        session.device_fingerprint = device_fingerprint;
        session
    }

    /// Whether this session has expired.
    pub fn is_expired(&self) -> bool {
        crate::user::current_timestamp() > self.expires_at
    }

    /// Whether this session is still valid (not expired, not revoked).
    pub fn is_valid(&self) -> bool {
        !self.is_expired() && !self.is_revoked
    }

    /// Update the last-active timestamp (sliding expiry).
    pub fn touch(&mut self) {
        self.last_active_at = crate::user::current_timestamp();
    }

    /// Extend the session by the given duration.
    pub fn extend(&mut self, additional_secs: u64) {
        self.expires_at += additional_secs;
        self.touch();
    }

    /// Revoke this session immediately.
    pub fn revoke(&mut self) {
        self.is_revoked = true;
    }

    /// Duration since last activity (seconds).
    pub fn idle_duration(&self) -> u64 {
        crate::user::current_timestamp().saturating_sub(self.last_active_at)
    }

    /// Remaining time before expiry (seconds, 0 if expired).
    pub fn remaining(&self) -> u64 {
        self.expires_at.saturating_sub(crate::user::current_timestamp())
    }

    /// Session duration in seconds (from creation).
    pub fn age(&self) -> u64 {
        crate::user::current_timestamp().saturating_sub(self.created_at)
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_id_unique() {
        let a = SessionId::new();
        let b = SessionId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn session_id_nil() {
        let nil = SessionId::nil();
        assert_eq!(nil.as_uuid(), Uuid::nil());
    }

    #[test]
    fn session_id_uuid_roundtrip() {
        let uuid = Uuid::new_v4();
        let sid = SessionId::from(uuid);
        let back: Uuid = sid.into();
        assert_eq!(uuid, back);
    }

    #[test]
    fn session_id_display() {
        let sid = SessionId::nil();
        assert_eq!(sid.to_string(), "00000000-0000-0000-0000-000000000000");
    }

    #[test]
    fn session_new() {
        let uid = UserId::new();
        let s = Session::new(uid, 3600);
        assert_eq!(s.user_id, uid);
        assert!(!s.is_revoked);
        assert!(s.ip_address.is_none());
        assert!(s.is_valid());
    }

    #[test]
    fn session_with_metadata() {
        let uid = UserId::new();
        let s = Session::with_metadata(
            uid, 3600,
            Some("192.168.1.1".into()),
            Some("Mozilla/5.0".into()),
            Some("fp-abc123".into()),
        );
        assert_eq!(s.ip_address.as_deref(), Some("192.168.1.1"));
        assert_eq!(s.user_agent.as_deref(), Some("Mozilla/5.0"));
        assert_eq!(s.device_fingerprint.as_deref(), Some("fp-abc123"));
    }

    #[test]
    fn session_validity() {
        let uid = UserId::new();
        let mut s = Session::new(uid, 3600);
        assert!(s.is_valid());

        s.revoke();
        assert!(!s.is_valid());
        assert!(s.is_revoked);
    }

    #[test]
    fn session_expired() {
        let uid = UserId::new();
        let now = crate::user::current_timestamp();
        let mut s = Session::new(uid, 3600);
        // Force expiry in the past
        s.expires_at = now.saturating_sub(1);
        assert!(s.is_expired());
        assert!(!s.is_valid());
    }

    #[test]
    fn session_touch() {
        let uid = UserId::new();
        let mut s = Session::new(uid, 3600);
        let before = s.last_active_at;
        std::thread::sleep(std::time::Duration::from_millis(10));
        s.touch();
        assert!(s.last_active_at >= before);
    }

    #[test]
    fn session_extend() {
        let uid = UserId::new();
        let mut s = Session::new(uid, 3600);
        let old_exp = s.expires_at;
        s.extend(1800);
        assert_eq!(s.expires_at, old_exp + 1800);
    }

    #[test]
    fn session_remaining() {
        let uid = UserId::new();
        let s = Session::new(uid, 3600);
        assert!(s.remaining() > 3500); // At least 3500 of 3600 remaining
        assert!(s.remaining() <= 3600);
    }

    #[test]
    fn session_idle_duration() {
        let uid = UserId::new();
        let s = Session::new(uid, 3600);
        // Just created, idle duration should be ~0
        assert!(s.idle_duration() < 2);
    }

    #[test]
    fn session_age() {
        let uid = UserId::new();
        let s = Session::new(uid, 3600);
        assert!(s.age() < 2);
    }

    #[test]
    fn session_serde_roundtrip() {
        let uid = UserId::new();
        let s = Session::new(uid, 3600);
        let json = serde_json::to_string(&s).unwrap();
        let back: Session = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, s.id);
        assert_eq!(back.user_id, uid);
    }
}
