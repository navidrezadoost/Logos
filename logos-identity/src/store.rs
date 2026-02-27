//! User storage trait and in-memory implementation.

use crate::credential::Credential;
use crate::error::IdentityError;
use crate::user::{User, UserId};
use std::collections::HashMap;

/// Trait for persisting user records.
///
/// In-memory implementation is provided for testing and desktop use.
/// Production implementations (RocksDB, SQLite, Postgres) can be
/// provided by consumer crates.
pub trait UserStore {
    /// Create a new user. Returns error if email already exists.
    fn create_user(&mut self, user: User) -> Result<UserId, IdentityError>;

    /// Get a user by ID.
    fn get_user(&self, id: &UserId) -> Result<Option<User>, IdentityError>;

    /// Get a user by email address.
    fn get_user_by_email(&self, email: &str) -> Result<Option<User>, IdentityError>;

    /// Update an existing user record.
    fn update_user(&mut self, user: &User) -> Result<(), IdentityError>;

    /// Delete a user by ID.
    fn delete_user(&mut self, id: &UserId) -> Result<bool, IdentityError>;

    /// List users with pagination.
    fn list_users(&self, offset: usize, limit: usize) -> Result<Vec<User>, IdentityError>;

    /// Count total users.
    fn count_users(&self) -> Result<usize, IdentityError>;

    /// Search users by display name or email (case-insensitive substring).
    fn search_users(&self, query: &str) -> Result<Vec<User>, IdentityError>;

    /// Store a credential for a user.
    fn set_credential(&mut self, user_id: &UserId, credential: Credential) -> Result<(), IdentityError>;

    /// Get all credentials for a user.
    fn get_credentials(&self, user_id: &UserId) -> Result<Vec<Credential>, IdentityError>;

    /// Remove all credentials for a user.
    fn clear_credentials(&mut self, user_id: &UserId) -> Result<(), IdentityError>;

    /// Check if an email is already registered.
    fn email_exists(&self, email: &str) -> Result<bool, IdentityError> {
        Ok(self.get_user_by_email(email)?.is_some())
    }
}

// ── In-Memory Implementation ─────────────────────────────────────────

/// In-memory user store (for testing and single-user desktop mode).
#[derive(Debug, Clone)]
pub struct InMemoryUserStore {
    users: HashMap<UserId, User>,
    email_index: HashMap<String, UserId>,
    credentials: HashMap<UserId, Vec<Credential>>,
}

impl InMemoryUserStore {
    pub fn new() -> Self {
        Self {
            users: HashMap::new(),
            email_index: HashMap::new(),
            credentials: HashMap::new(),
        }
    }

    /// Number of stored users.
    pub fn len(&self) -> usize {
        self.users.len()
    }

    pub fn is_empty(&self) -> bool {
        self.users.is_empty()
    }

    /// Clear all data.
    pub fn clear(&mut self) {
        self.users.clear();
        self.email_index.clear();
        self.credentials.clear();
    }
}

impl Default for InMemoryUserStore {
    fn default() -> Self {
        Self::new()
    }
}

impl UserStore for InMemoryUserStore {
    fn create_user(&mut self, user: User) -> Result<UserId, IdentityError> {
        let email_lower = user.email.to_lowercase();
        if self.email_index.contains_key(&email_lower) {
            return Err(IdentityError::DuplicateEmail(user.email.clone()));
        }
        let id = user.id;
        self.email_index.insert(email_lower, id);
        self.users.insert(id, user);
        Ok(id)
    }

    fn get_user(&self, id: &UserId) -> Result<Option<User>, IdentityError> {
        Ok(self.users.get(id).cloned())
    }

    fn get_user_by_email(&self, email: &str) -> Result<Option<User>, IdentityError> {
        let email_lower = email.to_lowercase();
        match self.email_index.get(&email_lower) {
            Some(id) => Ok(self.users.get(id).cloned()),
            None => Ok(None),
        }
    }

    fn update_user(&mut self, user: &User) -> Result<(), IdentityError> {
        if !self.users.contains_key(&user.id) {
            return Err(IdentityError::UserNotFound(user.id));
        }
        // Update email index if email changed
        let old = self.users.get(&user.id).unwrap();
        let old_email = old.email.to_lowercase();
        let new_email = user.email.to_lowercase();
        if old_email != new_email {
            if self.email_index.contains_key(&new_email) {
                return Err(IdentityError::DuplicateEmail(user.email.clone()));
            }
            self.email_index.remove(&old_email);
            self.email_index.insert(new_email, user.id);
        }
        self.users.insert(user.id, user.clone());
        Ok(())
    }

    fn delete_user(&mut self, id: &UserId) -> Result<bool, IdentityError> {
        if let Some(user) = self.users.remove(id) {
            self.email_index.remove(&user.email.to_lowercase());
            self.credentials.remove(id);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn list_users(&self, offset: usize, limit: usize) -> Result<Vec<User>, IdentityError> {
        let mut users: Vec<User> = self.users.values().cloned().collect();
        users.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok(users.into_iter().skip(offset).take(limit).collect())
    }

    fn count_users(&self) -> Result<usize, IdentityError> {
        Ok(self.users.len())
    }

    fn search_users(&self, query: &str) -> Result<Vec<User>, IdentityError> {
        let q = query.to_lowercase();
        Ok(self.users.values()
            .filter(|u| {
                u.display_name.to_lowercase().contains(&q)
                    || u.email.to_lowercase().contains(&q)
            })
            .cloned()
            .collect())
    }

    fn set_credential(&mut self, user_id: &UserId, credential: Credential) -> Result<(), IdentityError> {
        if !self.users.contains_key(user_id) {
            return Err(IdentityError::UserNotFound(*user_id));
        }
        self.credentials
            .entry(*user_id)
            .or_default()
            .push(credential);
        Ok(())
    }

    fn get_credentials(&self, user_id: &UserId) -> Result<Vec<Credential>, IdentityError> {
        Ok(self.credentials.get(user_id).cloned().unwrap_or_default())
    }

    fn clear_credentials(&mut self, user_id: &UserId) -> Result<(), IdentityError> {
        self.credentials.remove(user_id);
        Ok(())
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credential::{HashAlgorithm, PasswordCredential};
    use crate::user::AuthProvider;

    fn make_user(email: &str, name: &str) -> User {
        User::new(email, name, AuthProvider::Local)
    }

    #[test]
    fn create_and_get() {
        let mut store = InMemoryUserStore::new();
        let user = make_user("alice@test.com", "Alice");
        let id = user.id;
        store.create_user(user).unwrap();
        let fetched = store.get_user(&id).unwrap().unwrap();
        assert_eq!(fetched.display_name, "Alice");
    }

    #[test]
    fn duplicate_email_rejected() {
        let mut store = InMemoryUserStore::new();
        store.create_user(make_user("alice@test.com", "Alice")).unwrap();
        let result = store.create_user(make_user("alice@test.com", "Alice 2"));
        assert!(matches!(result, Err(IdentityError::DuplicateEmail(_))));
    }

    #[test]
    fn email_case_insensitive() {
        let mut store = InMemoryUserStore::new();
        store.create_user(make_user("Alice@Test.com", "Alice")).unwrap();
        let result = store.create_user(make_user("alice@test.com", "Alicia"));
        assert!(matches!(result, Err(IdentityError::DuplicateEmail(_))));
        assert!(store.get_user_by_email("ALICE@TEST.COM").unwrap().is_some());
    }

    #[test]
    fn get_by_email() {
        let mut store = InMemoryUserStore::new();
        store.create_user(make_user("bob@test.com", "Bob")).unwrap();
        let user = store.get_user_by_email("bob@test.com").unwrap().unwrap();
        assert_eq!(user.display_name, "Bob");
    }

    #[test]
    fn update_user() {
        let mut store = InMemoryUserStore::new();
        let mut user = make_user("alice@test.com", "Alice");
        let id = user.id;
        store.create_user(user.clone()).unwrap();
        user.display_name = "Alice Smith".to_string();
        store.update_user(&user).unwrap();
        let fetched = store.get_user(&id).unwrap().unwrap();
        assert_eq!(fetched.display_name, "Alice Smith");
    }

    #[test]
    fn update_user_email_change() {
        let mut store = InMemoryUserStore::new();
        let mut user = make_user("old@test.com", "User");
        store.create_user(user.clone()).unwrap();
        user.email = "new@test.com".to_string();
        store.update_user(&user).unwrap();
        assert!(store.get_user_by_email("old@test.com").unwrap().is_none());
        assert!(store.get_user_by_email("new@test.com").unwrap().is_some());
    }

    #[test]
    fn update_nonexistent_fails() {
        let mut store = InMemoryUserStore::new();
        let user = make_user("ghost@test.com", "Ghost");
        let result = store.update_user(&user);
        assert!(matches!(result, Err(IdentityError::UserNotFound(_))));
    }

    #[test]
    fn delete_user() {
        let mut store = InMemoryUserStore::new();
        let user = make_user("alice@test.com", "Alice");
        let id = user.id;
        store.create_user(user).unwrap();
        assert!(store.delete_user(&id).unwrap());
        assert!(store.get_user(&id).unwrap().is_none());
        assert!(store.get_user_by_email("alice@test.com").unwrap().is_none());
    }

    #[test]
    fn delete_nonexistent() {
        let mut store = InMemoryUserStore::new();
        assert!(!store.delete_user(&UserId::new()).unwrap());
    }

    #[test]
    fn list_users_pagination() {
        let mut store = InMemoryUserStore::new();
        for i in 0..10 {
            store.create_user(make_user(&format!("user{}@test.com", i), &format!("User {}", i))).unwrap();
        }
        let page1 = store.list_users(0, 3).unwrap();
        assert_eq!(page1.len(), 3);
        let page2 = store.list_users(3, 3).unwrap();
        assert_eq!(page2.len(), 3);
        let all = store.list_users(0, 100).unwrap();
        assert_eq!(all.len(), 10);
    }

    #[test]
    fn count_users() {
        let mut store = InMemoryUserStore::new();
        assert_eq!(store.count_users().unwrap(), 0);
        store.create_user(make_user("a@b.c", "A")).unwrap();
        assert_eq!(store.count_users().unwrap(), 1);
    }

    #[test]
    fn search_users() {
        let mut store = InMemoryUserStore::new();
        store.create_user(make_user("alice@test.com", "Alice Smith")).unwrap();
        store.create_user(make_user("bob@test.com", "Bob Jones")).unwrap();
        store.create_user(make_user("charlie@test.com", "Charlie Smith")).unwrap();
        let results = store.search_users("smith").unwrap();
        assert_eq!(results.len(), 2);
        let results = store.search_users("bob").unwrap();
        assert_eq!(results.len(), 1);
        let results = store.search_users("test.com").unwrap();
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn credential_crud() {
        let mut store = InMemoryUserStore::new();
        let user = make_user("a@b.c", "A");
        let id = user.id;
        store.create_user(user).unwrap();

        let cred = Credential::Password(PasswordCredential::new("hash", "salt", HashAlgorithm::Argon2id));
        store.set_credential(&id, cred).unwrap();
        let creds = store.get_credentials(&id).unwrap();
        assert_eq!(creds.len(), 1);

        store.clear_credentials(&id).unwrap();
        let creds = store.get_credentials(&id).unwrap();
        assert_eq!(creds.len(), 0);
    }

    #[test]
    fn credential_for_nonexistent_user() {
        let mut store = InMemoryUserStore::new();
        let cred = Credential::Password(PasswordCredential::new("h", "s", HashAlgorithm::Argon2id));
        let result = store.set_credential(&UserId::new(), cred);
        assert!(matches!(result, Err(IdentityError::UserNotFound(_))));
    }

    #[test]
    fn email_exists() {
        let mut store = InMemoryUserStore::new();
        store.create_user(make_user("a@b.c", "A")).unwrap();
        assert!(store.email_exists("a@b.c").unwrap());
        assert!(!store.email_exists("x@y.z").unwrap());
    }

    #[test]
    fn clear_store() {
        let mut store = InMemoryUserStore::new();
        store.create_user(make_user("a@b.c", "A")).unwrap();
        assert_eq!(store.len(), 1);
        store.clear();
        assert!(store.is_empty());
    }
}
