//! Session storage trait and in-memory implementation.

use crate::error::IdentityError;
use crate::session::{Session, SessionId};
use crate::user::UserId;
use std::collections::HashMap;

/// Trait for persisting session records.
pub trait SessionStore {
    /// Create a new session.
    fn create_session(&mut self, session: Session) -> Result<SessionId, IdentityError>;

    /// Get a session by ID.
    fn get_session(&self, id: &SessionId) -> Result<Option<Session>, IdentityError>;

    /// Update a session (e.g., touch last_active_at).
    fn update_session(&mut self, session: &Session) -> Result<(), IdentityError>;

    /// Delete a session.
    fn delete_session(&mut self, id: &SessionId) -> Result<bool, IdentityError>;

    /// Get all sessions for a user.
    fn get_user_sessions(&self, user_id: &UserId) -> Result<Vec<Session>, IdentityError>;

    /// Revoke all sessions for a user. Returns count of revoked sessions.
    fn revoke_user_sessions(&mut self, user_id: &UserId) -> Result<usize, IdentityError>;

    /// Remove all expired sessions. Returns count removed.
    fn cleanup_expired(&mut self) -> Result<usize, IdentityError>;

    /// Count active (non-expired, non-revoked) sessions for a user.
    fn active_session_count(&self, user_id: &UserId) -> Result<usize, IdentityError> {
        let sessions = self.get_user_sessions(user_id)?;
        Ok(sessions.iter().filter(|s| s.is_valid()).count())
    }
}

// ── In-Memory Implementation ─────────────────────────────────────────

/// In-memory session store (for testing and single-user desktop mode).
#[derive(Debug, Clone)]
pub struct InMemorySessionStore {
    sessions: HashMap<SessionId, Session>,
    user_index: HashMap<UserId, Vec<SessionId>>,
}

impl InMemorySessionStore {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            user_index: HashMap::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    pub fn clear(&mut self) {
        self.sessions.clear();
        self.user_index.clear();
    }
}

impl Default for InMemorySessionStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionStore for InMemorySessionStore {
    fn create_session(&mut self, session: Session) -> Result<SessionId, IdentityError> {
        let id = session.id;
        let user_id = session.user_id;
        self.sessions.insert(id, session);
        self.user_index.entry(user_id).or_default().push(id);
        Ok(id)
    }

    fn get_session(&self, id: &SessionId) -> Result<Option<Session>, IdentityError> {
        Ok(self.sessions.get(id).cloned())
    }

    fn update_session(&mut self, session: &Session) -> Result<(), IdentityError> {
        if !self.sessions.contains_key(&session.id) {
            return Err(IdentityError::SessionNotFound);
        }
        self.sessions.insert(session.id, session.clone());
        Ok(())
    }

    fn delete_session(&mut self, id: &SessionId) -> Result<bool, IdentityError> {
        if let Some(session) = self.sessions.remove(id) {
            if let Some(ids) = self.user_index.get_mut(&session.user_id) {
                ids.retain(|sid| sid != id);
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn get_user_sessions(&self, user_id: &UserId) -> Result<Vec<Session>, IdentityError> {
        let ids = self.user_index.get(user_id).cloned().unwrap_or_default();
        Ok(ids.iter()
            .filter_map(|id| self.sessions.get(id).cloned())
            .collect())
    }

    fn revoke_user_sessions(&mut self, user_id: &UserId) -> Result<usize, IdentityError> {
        let ids = self.user_index.get(user_id).cloned().unwrap_or_default();
        let mut count = 0;
        for id in &ids {
            if let Some(session) = self.sessions.get_mut(id) {
                if !session.is_revoked {
                    session.revoke();
                    count += 1;
                }
            }
        }
        Ok(count)
    }

    fn cleanup_expired(&mut self) -> Result<usize, IdentityError> {
        let expired: Vec<SessionId> = self.sessions.values()
            .filter(|s| s.is_expired())
            .map(|s| s.id)
            .collect();
        let count = expired.len();
        for id in &expired {
            self.delete_session(id)?;
        }
        Ok(count)
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn make_session(user_id: UserId) -> Session {
        Session::new(user_id, 3600)
    }

    #[test]
    fn create_and_get() {
        let mut store = InMemorySessionStore::new();
        let uid = UserId::new();
        let session = make_session(uid);
        let id = session.id;
        store.create_session(session).unwrap();
        let fetched = store.get_session(&id).unwrap().unwrap();
        assert_eq!(fetched.user_id, uid);
    }

    #[test]
    fn get_nonexistent() {
        let store = InMemorySessionStore::new();
        assert!(store.get_session(&SessionId::new()).unwrap().is_none());
    }

    #[test]
    fn update_session() {
        let mut store = InMemorySessionStore::new();
        let uid = UserId::new();
        let mut session = make_session(uid);
        store.create_session(session.clone()).unwrap();
        session.touch();
        store.update_session(&session).unwrap();
        let fetched = store.get_session(&session.id).unwrap().unwrap();
        assert_eq!(fetched.last_active_at, session.last_active_at);
    }

    #[test]
    fn update_nonexistent_fails() {
        let mut store = InMemorySessionStore::new();
        let session = make_session(UserId::new());
        let result = store.update_session(&session);
        assert!(matches!(result, Err(IdentityError::SessionNotFound)));
    }

    #[test]
    fn delete_session() {
        let mut store = InMemorySessionStore::new();
        let uid = UserId::new();
        let session = make_session(uid);
        let id = session.id;
        store.create_session(session).unwrap();
        assert!(store.delete_session(&id).unwrap());
        assert!(store.get_session(&id).unwrap().is_none());
        assert!(!store.delete_session(&id).unwrap());
    }

    #[test]
    fn get_user_sessions() {
        let mut store = InMemorySessionStore::new();
        let uid = UserId::new();
        store.create_session(make_session(uid)).unwrap();
        store.create_session(make_session(uid)).unwrap();
        store.create_session(make_session(UserId::new())).unwrap(); // Different user
        let sessions = store.get_user_sessions(&uid).unwrap();
        assert_eq!(sessions.len(), 2);
    }

    #[test]
    fn revoke_user_sessions() {
        let mut store = InMemorySessionStore::new();
        let uid = UserId::new();
        store.create_session(make_session(uid)).unwrap();
        store.create_session(make_session(uid)).unwrap();
        let count = store.revoke_user_sessions(&uid).unwrap();
        assert_eq!(count, 2);
        let sessions = store.get_user_sessions(&uid).unwrap();
        assert!(sessions.iter().all(|s| s.is_revoked));
    }

    #[test]
    fn cleanup_expired() {
        let mut store = InMemorySessionStore::new();
        let uid = UserId::new();

        // Create a valid session
        store.create_session(make_session(uid)).unwrap();

        // Create an expired session
        let now = crate::user::current_timestamp();
        let mut expired = Session::new(uid, 1);
        expired.expires_at = now.saturating_sub(100);
        store.create_session(expired).unwrap();

        assert_eq!(store.len(), 2);
        let removed = store.cleanup_expired().unwrap();
        assert_eq!(removed, 1);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn active_session_count() {
        let mut store = InMemorySessionStore::new();
        let uid = UserId::new();
        store.create_session(make_session(uid)).unwrap();
        store.create_session(make_session(uid)).unwrap();
        let mut revoked = make_session(uid);
        revoked.revoke();
        store.create_session(revoked).unwrap();
        assert_eq!(store.active_session_count(&uid).unwrap(), 2);
    }

    #[test]
    fn clear_store() {
        let mut store = InMemorySessionStore::new();
        store.create_session(make_session(UserId::new())).unwrap();
        store.clear();
        assert!(store.is_empty());
    }
}
