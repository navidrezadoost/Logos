// logos-collab/src/auth/user.rs
//
//! User accounts, password hashing (Argon2id), and session tokens.
//!
//! ## Design
//! - Passwords are never stored in plaintext — only the Argon2id PHC hash.
//! - Sessions carry a UUID token and an expiry timestamp.
//! - `UserStore` is an in-memory store suitable for testing and a single-node
//!   server; production deployments swap it for a DB-backed implementation
//!   that satisfies the same trait.

use std::collections::HashMap;

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Timestamp ─────────────────────────────────────────────────────────────────

pub type Timestamp = u64;

fn now_ms() -> Timestamp {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn now_secs() -> Timestamp {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ── UserError ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserError {
    UsernameTaken,
    EmailTaken,
    UserNotFound,
    InvalidCredentials,
    SessionExpired,
    SessionNotFound,
    NotApproved,
    PasswordHashError(String),
}

impl std::fmt::Display for UserError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UsernameTaken       => write!(f, "username already taken"),
            Self::EmailTaken          => write!(f, "email already registered"),
            Self::UserNotFound        => write!(f, "user not found"),
            Self::InvalidCredentials  => write!(f, "invalid username or password"),
            Self::SessionExpired      => write!(f, "session token has expired"),
            Self::SessionNotFound     => write!(f, "session not found"),
            Self::NotApproved         => write!(f, "account pending admin approval"),
            Self::PasswordHashError(s)=> write!(f, "password hash error: {s}"),
        }
    }
}

// ── User ─────────────────────────────────────────────────────────────────────

/// A Logos platform user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id:            Uuid,
    pub username:      String,
    pub email:         String,
    /// Argon2id PHC string (never plaintext).
    pub password_hash: String,
    pub first_name:    String,
    pub last_name:     String,
    pub avatar_url:    Option<String>,
    pub job_title:     Option<String>,
    /// Whether an admin has approved this account (for self-registered users).
    pub approved:      bool,
    pub created_at:    Timestamp,
    pub updated_at:    Timestamp,
}

impl User {
    /// Create a new user with a hashed password.  Returns `UserError` if
    /// hashing fails (usually means invalid OS RNG — never happens in practice).
    pub fn new(
        username:   impl Into<String>,
        email:      impl Into<String>,
        password:   &str,
        first_name: impl Into<String>,
        last_name:  impl Into<String>,
        approved:   bool,
    ) -> Result<Self, UserError> {
        let hash = hash_password(password)?;
        let now  = now_ms();
        Ok(Self {
            id:            Uuid::new_v4(),
            username:      username.into(),
            email:         email.into(),
            password_hash: hash,
            first_name:    first_name.into(),
            last_name:     last_name.into(),
            avatar_url:    None,
            job_title:     None,
            approved,
            created_at:    now,
            updated_at:    now,
        })
    }

    /// Full display name.
    pub fn display_name(&self) -> String {
        format!("{} {}", self.first_name, self.last_name)
    }

    /// Verify a plaintext password against the stored hash.
    pub fn verify_password(&self, password: &str) -> bool {
        verify_password(password, &self.password_hash)
    }

    /// Update the stored password hash.
    pub fn set_password(&mut self, new_password: &str) -> Result<(), UserError> {
        self.password_hash = hash_password(new_password)?;
        self.updated_at    = now_ms();
        Ok(())
    }
}

// ── Session ───────────────────────────────────────────────────────────────────

/// A server-side session record.  The opaque `token` is returned to the client
/// and sent on every subsequent request (e.g. as a Bearer token or in the
/// WebSocket `Authorization` header).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Random token (UUID v4 used as an opaque bearer secret).
    pub token:      String,
    pub user_id:    Uuid,
    /// Unix seconds at which this session expires.
    pub expires_at: Timestamp,
    pub created_at: Timestamp,
}

impl Session {
    const DEFAULT_TTL_SECS: u64 = 86_400; // 24 h

    pub fn new(user_id: Uuid) -> Self {
        Self::with_ttl(user_id, Self::DEFAULT_TTL_SECS)
    }

    pub fn with_ttl(user_id: Uuid, ttl_secs: u64) -> Self {
        let now = now_secs();
        Self {
            token:      Uuid::new_v4().to_string(),
            user_id,
            expires_at: now + ttl_secs,
            created_at: now,
        }
    }

    pub fn is_expired(&self) -> bool {
        now_secs() >= self.expires_at
    }
}

// ── UserStore ─────────────────────────────────────────────────────────────────

/// In-memory user and session store.
///
/// All operations are `O(1)` via HashMap.  Thread-safety is the caller's
/// responsibility (wrap in `tokio::sync::Mutex` for multi-task use).
#[derive(Debug, Default)]
pub struct UserStore {
    /// keyed by user id
    users_by_id:       HashMap<Uuid, User>,
    /// secondary index: username → id
    username_index:    HashMap<String, Uuid>,
    /// secondary index: lower-case email → id
    email_index:       HashMap<String, Uuid>,
    /// keyed by session token string
    sessions:          HashMap<String, Session>,
}

impl UserStore {
    pub fn new() -> Self { Self::default() }

    // ── User CRUD ─────────────────────────────────────────────────────────

    /// Insert a new user.  Returns `UserError` if username or email is taken.
    pub fn create_user(&mut self, user: User) -> Result<Uuid, UserError> {
        if self.username_index.contains_key(&user.username) {
            return Err(UserError::UsernameTaken);
        }
        let email_key = user.email.to_lowercase();
        if self.email_index.contains_key(&email_key) {
            return Err(UserError::EmailTaken);
        }
        let id = user.id;
        self.username_index.insert(user.username.clone(), id);
        self.email_index.insert(email_key, id);
        self.users_by_id.insert(id, user);
        Ok(id)
    }

    pub fn get_user(&self, id: Uuid) -> Option<&User> {
        self.users_by_id.get(&id)
    }

    pub fn get_user_by_username(&self, username: &str) -> Option<&User> {
        self.username_index.get(username).and_then(|id| self.users_by_id.get(id))
    }

    pub fn get_user_by_email(&self, email: &str) -> Option<&User> {
        let key = email.to_lowercase();
        self.email_index.get(&key).and_then(|id| self.users_by_id.get(id))
    }

    pub fn get_user_mut(&mut self, id: Uuid) -> Option<&mut User> {
        self.users_by_id.get_mut(&id)
    }

    pub fn delete_user(&mut self, id: Uuid) -> Result<(), UserError> {
        let user = self.users_by_id.remove(&id).ok_or(UserError::UserNotFound)?;
        self.username_index.remove(&user.username);
        self.email_index.remove(&user.email.to_lowercase());
        // Remove all sessions for this user
        self.sessions.retain(|_, s| s.user_id != id);
        Ok(())
    }

    /// Approve a pending self-registered user.
    pub fn approve_user(&mut self, id: Uuid) -> Result<(), UserError> {
        let u = self.users_by_id.get_mut(&id).ok_or(UserError::UserNotFound)?;
        u.approved   = true;
        u.updated_at = now_ms();
        Ok(())
    }

    pub fn list_users(&self) -> Vec<&User> {
        self.users_by_id.values().collect()
    }

    pub fn user_count(&self) -> usize { self.users_by_id.len() }

    // ── Authentication ────────────────────────────────────────────────────

    /// Authenticate by username or email.  Returns a new `Session` on success.
    pub fn login(&mut self, login: &str, password: &str) -> Result<Session, UserError> {
        // Try username first, then email
        let user_id = self.username_index.get(login)
            .or_else(|| self.email_index.get(&login.to_lowercase()))
            .copied()
            .ok_or(UserError::InvalidCredentials)?;

        let user = self.users_by_id.get(&user_id).ok_or(UserError::UserNotFound)?;

        if !user.approved {
            return Err(UserError::NotApproved);
        }
        if !user.verify_password(password) {
            return Err(UserError::InvalidCredentials);
        }

        let session = Session::new(user_id);
        self.sessions.insert(session.token.clone(), session.clone());
        Ok(session)
    }

    /// Validate a session token.  Returns the `User` if session is valid.
    pub fn validate_session(&self, token: &str) -> Result<&User, UserError> {
        let session = self.sessions.get(token).ok_or(UserError::SessionNotFound)?;
        if session.is_expired() {
            return Err(UserError::SessionExpired);
        }
        self.users_by_id.get(&session.user_id).ok_or(UserError::UserNotFound)
    }

    /// Invalidate (logout) a session token.
    pub fn logout(&mut self, token: &str) -> Result<(), UserError> {
        self.sessions.remove(token).map(|_| ()).ok_or(UserError::SessionNotFound)
    }

    /// Remove all expired sessions (call periodically).
    pub fn gc_sessions(&mut self) -> usize {
        let before = self.sessions.len();
        self.sessions.retain(|_, s| !s.is_expired());
        before - self.sessions.len()
    }
}

// ── Password helpers ──────────────────────────────────────────────────────────

fn hash_password(password: &str) -> Result<String, UserError> {
    let salt   = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| UserError::PasswordHashError(e.to_string()))
}

fn verify_password(password: &str, hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(hash) else { return false; };
    Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok()
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_user(username: &str, approved: bool) -> User {
        User::new(username, &format!("{username}@example.com"), "hunter2", "Alice", "Smith", approved).unwrap()
    }

    // U-01: User::new hashes the password (not stored in plaintext).
    #[test]
    fn u_01_password_not_plaintext() {
        let u = make_user("alice", true);
        assert_ne!(u.password_hash, "hunter2");
        assert!(u.password_hash.starts_with("$argon2"), "should be Argon2 PHC");
    }

    // U-02: User::verify_password returns true for correct password.
    #[test]
    fn u_02_verify_correct_password() {
        let u = make_user("bob", true);
        assert!(u.verify_password("hunter2"));
    }

    // U-03: User::verify_password returns false for wrong password.
    #[test]
    fn u_03_verify_wrong_password() {
        let u = make_user("carol", true);
        assert!(!u.verify_password("wrongpass"));
    }

    // U-04: display_name concatenates first and last name.
    #[test]
    fn u_04_display_name() {
        let u = make_user("dave", true);
        assert_eq!(u.display_name(), "Alice Smith");
    }

    // U-05: set_password updates the hash.
    #[test]
    fn u_05_set_password_updates_hash() {
        let mut u = make_user("eve", true);
        let old = u.password_hash.clone();
        u.set_password("newpass").unwrap();
        assert_ne!(u.password_hash, old);
        assert!(u.verify_password("newpass"));
    }

    // U-06: UserStore::create_user rejects duplicate username.
    #[test]
    fn u_06_duplicate_username_rejected() {
        let mut store = UserStore::new();
        store.create_user(make_user("frank", true)).unwrap();
        let err = store.create_user(make_user("frank", true)).unwrap_err();
        assert_eq!(err, UserError::UsernameTaken);
    }

    // U-07: UserStore::create_user rejects duplicate email.
    #[test]
    fn u_07_duplicate_email_rejected() {
        let mut store = UserStore::new();
        let u1 = make_user("grace", true);
        store.create_user(u1).unwrap();
        let mut u2 = make_user("grace2", true);
        u2.email = "grace@example.com".into(); // same email as u1
        let err = store.create_user(u2).unwrap_err();
        assert_eq!(err, UserError::EmailTaken);
    }

    // U-08: Login by username succeeds.
    #[test]
    fn u_08_login_by_username() {
        let mut store = UserStore::new();
        store.create_user(make_user("henry", true)).unwrap();
        let session = store.login("henry", "hunter2").unwrap();
        assert_eq!(session.user_id, store.get_user_by_username("henry").unwrap().id);
    }

    // U-09: Login by email succeeds.
    #[test]
    fn u_09_login_by_email() {
        let mut store = UserStore::new();
        store.create_user(make_user("iris", true)).unwrap();
        let session = store.login("iris@example.com", "hunter2").unwrap();
        assert!(!session.token.is_empty());
    }

    // U-10: Login with wrong password fails.
    #[test]
    fn u_10_login_wrong_password() {
        let mut store = UserStore::new();
        store.create_user(make_user("jack", true)).unwrap();
        let err = store.login("jack", "bad").unwrap_err();
        assert_eq!(err, UserError::InvalidCredentials);
    }

    // U-11: Login with unknown username fails.
    #[test]
    fn u_11_login_unknown_user() {
        let mut store = UserStore::new();
        let err = store.login("nobody", "pass").unwrap_err();
        assert_eq!(err, UserError::InvalidCredentials);
    }

    // U-12: Unapproved user cannot log in.
    #[test]
    fn u_12_unapproved_user_blocked() {
        let mut store = UserStore::new();
        store.create_user(make_user("kate", false)).unwrap();
        let err = store.login("kate", "hunter2").unwrap_err();
        assert_eq!(err, UserError::NotApproved);
    }

    // U-13: approve_user allows subsequent login.
    #[test]
    fn u_13_approve_user_allows_login() {
        let mut store = UserStore::new();
        let id = store.create_user(make_user("liam", false)).unwrap();
        store.approve_user(id).unwrap();
        store.login("liam", "hunter2").unwrap();
    }

    // U-14: validate_session returns the user for a fresh session.
    #[test]
    fn u_14_validate_session_ok() {
        let mut store = UserStore::new();
        store.create_user(make_user("mia", true)).unwrap();
        let s = store.login("mia", "hunter2").unwrap();
        let user = store.validate_session(&s.token).unwrap();
        assert_eq!(user.username, "mia");
    }

    // U-15: validate_session returns error for unknown token.
    #[test]
    fn u_15_validate_session_unknown_token() {
        let store = UserStore::new();
        let err = store.validate_session("bogus-token").unwrap_err();
        assert_eq!(err, UserError::SessionNotFound);
    }

    // U-16: logout invalidates the token.
    #[test]
    fn u_16_logout_invalidates_token() {
        let mut store = UserStore::new();
        store.create_user(make_user("noah", true)).unwrap();
        let s = store.login("noah", "hunter2").unwrap();
        store.logout(&s.token).unwrap();
        let err = store.validate_session(&s.token).unwrap_err();
        assert_eq!(err, UserError::SessionNotFound);
    }

    // U-17: delete_user removes user and all associated sessions.
    #[test]
    fn u_17_delete_user_removes_sessions() {
        let mut store = UserStore::new();
        let id = store.create_user(make_user("olivia", true)).unwrap();
        let s = store.login("olivia", "hunter2").unwrap();
        store.delete_user(id).unwrap();
        assert!(store.validate_session(&s.token).is_err());
        assert!(store.get_user_by_username("olivia").is_none());
    }

    // U-18: user_count reflects insertions and deletions.
    #[test]
    fn u_18_user_count() {
        let mut store = UserStore::new();
        assert_eq!(store.user_count(), 0);
        let id = store.create_user(make_user("peter", true)).unwrap();
        assert_eq!(store.user_count(), 1);
        store.delete_user(id).unwrap();
        assert_eq!(store.user_count(), 0);
    }

    // U-19: gc_sessions removes expired sessions.
    #[test]
    fn u_19_gc_sessions_removes_expired() {
        let mut store = UserStore::new();
        store.create_user(make_user("quinn", true)).unwrap();
        // Insert a session that is already expired (ttl = 0)
        let uid = store.get_user_by_username("quinn").unwrap().id;
        let mut s = Session::with_ttl(uid, 0);
        s.expires_at = 0; // definitely expired
        store.sessions.insert(s.token.clone(), s);
        assert_eq!(store.gc_sessions(), 1);
    }

    // U-20: Session::is_expired is false for a fresh session.
    #[test]
    fn u_20_fresh_session_not_expired() {
        let s = Session::new(Uuid::new_v4());
        assert!(!s.is_expired());
    }

    // U-21: Multiple users can coexist without index collision.
    #[test]
    fn u_21_multiple_users_coexist() {
        let mut store = UserStore::new();
        for i in 0..10 {
            store.create_user(make_user(&format!("user{i}"), true)).unwrap();
        }
        assert_eq!(store.user_count(), 10);
    }

    // U-22: list_users returns all created users.
    #[test]
    fn u_22_list_users() {
        let mut store = UserStore::new();
        store.create_user(make_user("rose", true)).unwrap();
        store.create_user(make_user("sam", true)).unwrap();
        assert_eq!(store.list_users().len(), 2);
    }

    // U-23: Email lookup is case-insensitive.
    #[test]
    fn u_23_email_lookup_case_insensitive() {
        let mut store = UserStore::new();
        store.create_user(make_user("tara", true)).unwrap();
        let u = store.login("TARA@EXAMPLE.COM", "hunter2").unwrap();
        assert!(!u.token.is_empty());
    }

    // U-24: delete_user on unknown id returns UserNotFound.
    #[test]
    fn u_24_delete_nonexistent_user() {
        let mut store = UserStore::new();
        let err = store.delete_user(Uuid::new_v4()).unwrap_err();
        assert_eq!(err, UserError::UserNotFound);
    }

    // U-25: approve_user on unknown id returns UserNotFound.
    #[test]
    fn u_25_approve_nonexistent_user() {
        let mut store = UserStore::new();
        let err = store.approve_user(Uuid::new_v4()).unwrap_err();
        assert_eq!(err, UserError::UserNotFound);
    }
}
