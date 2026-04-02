//! JWT-like session management and RBAC.
//!
//! `SessionStore` issues opaque `SessionToken`s, validates them, and revokes
//! them.  `RbacPolicy` maps `Role`s to permitted action strings, providing
//! fine-grained authorisation for agent operations.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

// ── Error ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SessionError {
    #[error("session token not found")]
    NotFound,
    #[error("session expired")]
    Expired,
    #[error("session revoked")]
    Revoked,
    #[error("insufficient permissions: action '{0}' denied for role '{1}'")]
    PermissionDenied(String, String),
}

// ── Role ──────────────────────────────────────────────────────────────────────

/// RBAC roles for agent operations.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Role {
    /// Can publish, update, and delete own agents.
    Publisher,
    /// Can invoke agents and read metrics for their own agents.
    Developer,
    /// Can invoke agents but cannot modify them.
    Viewer,
    /// Global administrator — all permissions.
    Admin,
    /// Custom role with a free-form name.
    Custom(String),
}

impl Role {
    pub fn label(&self) -> String {
        match self {
            Self::Publisher    => "publisher".to_string(),
            Self::Developer    => "developer".to_string(),
            Self::Viewer       => "viewer".to_string(),
            Self::Admin        => "admin".to_string(),
            Self::Custom(name) => name.clone(),
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "publisher" => Self::Publisher,
            "developer" => Self::Developer,
            "viewer"    => Self::Viewer,
            "admin"     => Self::Admin,
            other       => Self::Custom(other.to_string()),
        }
    }
}

// ── RBAC policy ───────────────────────────────────────────────────────────────

/// Maps roles to the set of action strings they may perform.
#[derive(Debug, Clone)]
pub struct RbacPolicy {
    rules: HashMap<String, Vec<String>>,
}

impl Default for RbacPolicy {
    fn default() -> Self {
        let mut p = Self { rules: HashMap::new() };
        p.add_rule(Role::Admin, vec![
            "agent:read", "agent:invoke", "agent:publish",
            "agent:update", "agent:delete", "metrics:read",
            "feedback:read", "session:revoke",
        ]);
        p.add_rule(Role::Publisher, vec![
            "agent:read", "agent:invoke", "agent:publish",
            "agent:update", "metrics:read", "feedback:read",
        ]);
        p.add_rule(Role::Developer, vec![
            "agent:read", "agent:invoke", "metrics:read",
        ]);
        p.add_rule(Role::Viewer, vec![
            "agent:read", "agent:invoke",
        ]);
        p
    }
}

impl RbacPolicy {
    pub fn new() -> Self { Self { rules: HashMap::new() } }

    pub fn add_rule(&mut self, role: Role, actions: Vec<&str>) {
        self.rules
            .entry(role.label())
            .or_default()
            .extend(actions.into_iter().map(|s| s.to_string()));
    }

    /// Returns `true` if `role` is allowed to perform `action`.
    pub fn allows(&self, role: &Role, action: &str) -> bool {
        self.rules
            .get(&role.label())
            .map(|actions| actions.iter().any(|a| a == action))
            .unwrap_or(false)
    }

    /// Returns `Ok(())` or a `PermissionDenied` error.
    pub fn check(&self, role: &Role, action: &str) -> Result<(), SessionError> {
        if self.allows(role, action) {
            Ok(())
        } else {
            Err(SessionError::PermissionDenied(action.to_string(), role.label()))
        }
    }
}

// ── Session ───────────────────────────────────────────────────────────────────

/// An active SSO session for an authenticated user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SsoSession {
    /// Authenticated user identity (email, UPN, or subject).
    pub user_id: String,
    /// Permissions granted to this session (action strings).
    pub permissions: Vec<String>,
    /// Roles assigned to this session.
    pub roles: Vec<Role>,
    /// Unix timestamp when this session expires.
    pub expires_at: u64,
    /// IdP that authenticated this user.
    pub idp_name: Option<String>,
}

impl SsoSession {
    /// Create a new session expiring 8 hours from now.
    pub fn new(user_id: impl Into<String>, permissions: &[&str]) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            user_id: user_id.into(),
            permissions: permissions.iter().map(|s| s.to_string()).collect(),
            roles: vec![],
            expires_at: now + 8 * 3600,
            idp_name: None,
        }
    }

    pub fn with_roles(mut self, roles: Vec<Role>) -> Self {
        self.roles = roles;
        self
    }

    pub fn with_expiry(mut self, ts: u64) -> Self {
        self.expires_at = ts;
        self
    }

    pub fn with_idp(mut self, idp: impl Into<String>) -> Self {
        self.idp_name = Some(idp.into());
        self
    }

    pub fn has_permission(&self, action: &str) -> bool {
        self.permissions.iter().any(|p| p == action)
    }

    pub fn is_expired(&self, now_ts: u64) -> bool {
        now_ts >= self.expires_at
    }
}

// ── Session token ─────────────────────────────────────────────────────────────

/// Opaque handle issued by `SessionStore::issue`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionToken(pub String);

impl SessionToken {
    pub fn as_str(&self) -> &str { &self.0 }
}

impl std::fmt::Display for SessionToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ── Session store ─────────────────────────────────────────────────────────────

/// In-memory store that issues and validates `SessionToken`s.
#[derive(Debug, Default)]
pub struct SessionStore {
    sessions: HashMap<String, SsoSession>,
    revoked: std::collections::HashSet<String>,
    counter: u64,
}

impl SessionStore {
    pub fn new() -> Self { Self::default() }

    // ── Issue ─────────────────────────────────────────────────────────────────

    /// Issue a new token for `session`.
    pub fn issue(&mut self, session: SsoSession) -> SessionToken {
        self.counter += 1;
        let token = format!("sso-tok-{}-{}", self.counter, session.user_id.replace('@', "_"));
        self.sessions.insert(token.clone(), session);
        SessionToken(token)
    }

    // ── Validate ──────────────────────────────────────────────────────────────

    /// Validate a token, returning a reference to the session on success.
    pub fn validate(&self, token: &SessionToken) -> Result<&SsoSession, SessionError> {
        if self.revoked.contains(token.as_str()) {
            return Err(SessionError::Revoked);
        }
        let sess = self.sessions.get(token.as_str()).ok_or(SessionError::NotFound)?;
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if sess.is_expired(now) {
            return Err(SessionError::Expired);
        }
        Ok(sess)
    }

    /// Validate a token against an explicit timestamp (for deterministic tests).
    pub fn validate_at(&self, token: &SessionToken, now_ts: u64) -> Result<&SsoSession, SessionError> {
        if self.revoked.contains(token.as_str()) {
            return Err(SessionError::Revoked);
        }
        let sess = self.sessions.get(token.as_str()).ok_or(SessionError::NotFound)?;
        if sess.is_expired(now_ts) {
            return Err(SessionError::Expired);
        }
        Ok(sess)
    }

    // ── Revoke ────────────────────────────────────────────────────────────────

    pub fn revoke(&mut self, token: &SessionToken) -> Result<(), SessionError> {
        if !self.sessions.contains_key(token.as_str()) {
            return Err(SessionError::NotFound);
        }
        self.revoked.insert(token.0.clone());
        Ok(())
    }

    // ── Query ─────────────────────────────────────────────────────────────────

    pub fn active_count(&self) -> usize {
        self.sessions.len() - self.revoked.len()
    }

    pub fn total_issued(&self) -> usize { self.sessions.len() }

    pub fn is_revoked(&self, token: &SessionToken) -> bool {
        self.revoked.contains(token.as_str())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_session(user: &str) -> SsoSession {
        SsoSession::new(user, &["agent:read", "agent:invoke"])
    }

    #[test]
    fn issue_and_validate_ok() {
        let mut store = SessionStore::new();
        let tok = store.issue(make_session("alice@corp.example"));
        assert!(store.validate(&tok).is_ok());
    }

    #[test]
    fn unknown_token_returns_not_found() {
        let store = SessionStore::new();
        let tok = SessionToken("ghost-token".into());
        assert_eq!(store.validate(&tok).unwrap_err(), SessionError::NotFound);
    }

    #[test]
    fn revoke_then_validate_returns_revoked() {
        let mut store = SessionStore::new();
        let tok = store.issue(make_session("bob@corp.example"));
        store.revoke(&tok).unwrap();
        assert_eq!(store.validate(&tok).unwrap_err(), SessionError::Revoked);
    }

    #[test]
    fn expired_session_returns_expired() {
        let mut store = SessionStore::new();
        let sess = SsoSession::new("carol@corp.example", &[]).with_expiry(100);
        let tok = store.issue(sess);
        assert_eq!(store.validate_at(&tok, 200).unwrap_err(), SessionError::Expired);
    }

    #[test]
    fn session_has_permission_true() {
        let sess = make_session("user@x.example");
        assert!(sess.has_permission("agent:read"));
        assert!(!sess.has_permission("agent:delete"));
    }

    #[test]
    fn active_count_decrements_after_revoke() {
        let mut store = SessionStore::new();
        let t1 = store.issue(make_session("u1@x.example"));
        let _t2 = store.issue(make_session("u2@x.example"));
        store.revoke(&t1).unwrap();
        assert_eq!(store.active_count(), 1);
    }

    #[test]
    fn rbac_admin_can_do_everything() {
        let policy = RbacPolicy::default();
        for action in ["agent:read", "agent:publish", "agent:delete", "session:revoke"] {
            assert!(policy.allows(&Role::Admin, action), "{action}");
        }
    }

    #[test]
    fn rbac_viewer_cannot_publish() {
        let policy = RbacPolicy::default();
        assert!(!policy.allows(&Role::Viewer, "agent:publish"));
    }

    #[test]
    fn rbac_publisher_can_publish() {
        let policy = RbacPolicy::default();
        assert!(policy.allows(&Role::Publisher, "agent:publish"));
    }

    #[test]
    fn rbac_check_returns_permission_denied() {
        let policy = RbacPolicy::default();
        let err = policy.check(&Role::Viewer, "agent:delete").unwrap_err();
        assert!(matches!(err, SessionError::PermissionDenied(_, _)));
    }

    #[test]
    fn role_from_str_roundtrip() {
        let r = Role::from_str("publisher");
        assert_eq!(r, Role::Publisher);
        assert_eq!(r.label(), "publisher");
    }

    #[test]
    fn role_custom_label() {
        let r = Role::Custom("data-scientist".into());
        assert_eq!(r.label(), "data-scientist");
    }
}
