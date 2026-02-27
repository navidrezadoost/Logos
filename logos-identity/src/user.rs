//! User identity types.
//!
//! `UserId` is the canonical user identifier across all Logos subsystems.
//! It replaces bare `Uuid` usage in `logos-core::DocumentMetadata.author_id`,
//! `logos-collab::auth::Claims.sub`, `logos-comments` author tracking, etc.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── UserId ───────────────────────────────────────────────────────────

/// Strongly-typed user identifier (UUID v4 newtype).
///
/// Compatible with existing `Uuid` fields via `From`/`Into`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UserId(Uuid);

impl UserId {
    /// Generate a new random user ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create from an existing UUID.
    pub fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }

    /// The nil (all-zeros) user ID — used as a sentinel.
    pub fn nil() -> Self {
        Self(Uuid::nil())
    }

    /// Access the inner UUID.
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }

    /// Check if this is the nil sentinel.
    pub fn is_nil(&self) -> bool {
        self.0.is_nil()
    }
}

impl Default for UserId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for UserId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl From<Uuid> for UserId {
    fn from(id: Uuid) -> Self {
        Self(id)
    }
}

impl From<UserId> for Uuid {
    fn from(id: UserId) -> Self {
        id.0
    }
}

// ── Auth Provider ────────────────────────────────────────────────────

/// How the user authenticated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthProvider {
    /// Local email + password.
    Local,
    /// Google OAuth2.
    Google,
    /// GitHub OAuth2.
    GitHub,
    /// Microsoft / Azure AD.
    Microsoft,
    /// Apple Sign-In.
    Apple,
    /// SAML SSO (provider name).
    Saml(String),
    /// Custom provider (name).
    Custom(String),
}

impl AuthProvider {
    /// Human-readable label.
    pub fn label(&self) -> &str {
        match self {
            Self::Local => "Email",
            Self::Google => "Google",
            Self::GitHub => "GitHub",
            Self::Microsoft => "Microsoft",
            Self::Apple => "Apple",
            Self::Saml(name) => name.as_str(),
            Self::Custom(name) => name.as_str(),
        }
    }

    /// Whether this is an OAuth2 provider.
    pub fn is_oauth(&self) -> bool {
        matches!(self, Self::Google | Self::GitHub | Self::Microsoft | Self::Apple)
    }
}

impl Default for AuthProvider {
    fn default() -> Self {
        Self::Local
    }
}

// ── Account Status ───────────────────────────────────────────────────

/// Account lifecycle states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccountStatus {
    /// Active and usable.
    Active,
    /// Temporarily suspended by admin.
    Suspended,
    /// Awaiting email verification.
    PendingVerification,
    /// Voluntarily deactivated by user.
    Deactivated,
}

impl AccountStatus {
    /// Whether the account can authenticate.
    pub fn can_login(&self) -> bool {
        matches!(self, Self::Active)
    }

    /// Whether the account is visible to other users.
    pub fn is_visible(&self) -> bool {
        matches!(self, Self::Active | Self::PendingVerification)
    }
}

impl Default for AccountStatus {
    fn default() -> Self {
        Self::PendingVerification
    }
}

// ── User ─────────────────────────────────────────────────────────────

/// A user in the Logos system.
///
/// This is the canonical user record. All subsystems reference users
/// via `UserId` and can enrich with this full record when needed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    /// Unique identifier.
    pub id: UserId,
    /// Primary email address.
    pub email: String,
    /// Display name (shown in UI, cursors, comments).
    pub display_name: String,
    /// Avatar URL (optional).
    pub avatar_url: Option<String>,
    /// How the user was authenticated.
    pub provider: AuthProvider,
    /// Account lifecycle state.
    pub status: AccountStatus,
    /// Unix timestamp (seconds) — when account was created.
    pub created_at: u64,
    /// Unix timestamp (seconds) — last profile update.
    pub updated_at: u64,
    /// Unix timestamp (seconds) — last successful login.
    pub last_login_at: Option<u64>,
    /// Whether email has been verified.
    pub email_verified: bool,
}

impl User {
    /// Create a new user with default status (PendingVerification).
    pub fn new(email: impl Into<String>, display_name: impl Into<String>, provider: AuthProvider) -> Self {
        let now = current_timestamp();
        Self {
            id: UserId::new(),
            email: email.into(),
            display_name: display_name.into(),
            avatar_url: None,
            provider,
            status: AccountStatus::PendingVerification,
            created_at: now,
            updated_at: now,
            last_login_at: None,
            email_verified: false,
        }
    }

    /// Create a pre-verified active user (e.g., from OAuth).
    pub fn new_verified(email: impl Into<String>, display_name: impl Into<String>, provider: AuthProvider) -> Self {
        let now = current_timestamp();
        Self {
            id: UserId::new(),
            email: email.into(),
            display_name: display_name.into(),
            avatar_url: None,
            provider,
            status: AccountStatus::Active,
            created_at: now,
            updated_at: now,
            last_login_at: Some(now),
            email_verified: true,
        }
    }

    /// Create a user with a specific ID (for migration/testing).
    pub fn with_id(id: UserId, email: impl Into<String>, display_name: impl Into<String>) -> Self {
        let now = current_timestamp();
        Self {
            id,
            email: email.into(),
            display_name: display_name.into(),
            avatar_url: None,
            provider: AuthProvider::Local,
            status: AccountStatus::Active,
            created_at: now,
            updated_at: now,
            last_login_at: None,
            email_verified: false,
        }
    }

    pub fn is_active(&self) -> bool {
        self.status == AccountStatus::Active
    }

    pub fn verify_email(&mut self) {
        self.email_verified = true;
        if self.status == AccountStatus::PendingVerification {
            self.status = AccountStatus::Active;
        }
        self.updated_at = current_timestamp();
    }

    pub fn suspend(&mut self) {
        self.status = AccountStatus::Suspended;
        self.updated_at = current_timestamp();
    }

    pub fn reactivate(&mut self) {
        self.status = AccountStatus::Active;
        self.updated_at = current_timestamp();
    }

    pub fn deactivate(&mut self) {
        self.status = AccountStatus::Deactivated;
        self.updated_at = current_timestamp();
    }

    pub fn record_login(&mut self) {
        self.last_login_at = Some(current_timestamp());
        self.updated_at = current_timestamp();
    }

    pub fn update_profile(&mut self, display_name: impl Into<String>, avatar_url: Option<String>) {
        self.display_name = display_name.into();
        self.avatar_url = avatar_url;
        self.updated_at = current_timestamp();
    }

    /// Convert to a public-facing profile (no email, no internals).
    pub fn to_profile(&self) -> UserProfile {
        UserProfile {
            id: self.id,
            display_name: self.display_name.clone(),
            avatar_url: self.avatar_url.clone(),
        }
    }
}

// ── User Profile ─────────────────────────────────────────────────────

/// Public-facing subset of `User` — safe to share with other users.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserProfile {
    pub id: UserId,
    pub display_name: String,
    pub avatar_url: Option<String>,
}

impl From<&User> for UserProfile {
    fn from(user: &User) -> Self {
        user.to_profile()
    }
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Current Unix timestamp in seconds.
pub(crate) fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_id_new_is_unique() {
        let a = UserId::new();
        let b = UserId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn user_id_nil() {
        let nil = UserId::nil();
        assert!(nil.is_nil());
        assert!(!UserId::new().is_nil());
    }

    #[test]
    fn user_id_uuid_roundtrip() {
        let uuid = Uuid::new_v4();
        let uid = UserId::from(uuid);
        let back: Uuid = uid.into();
        assert_eq!(uuid, back);
    }

    #[test]
    fn user_id_serde_roundtrip() {
        let uid = UserId::new();
        let json = serde_json::to_string(&uid).unwrap();
        let back: UserId = serde_json::from_str(&json).unwrap();
        assert_eq!(uid, back);
    }

    #[test]
    fn user_id_display() {
        let uid = UserId::nil();
        assert_eq!(uid.to_string(), "00000000-0000-0000-0000-000000000000");
    }

    #[test]
    fn user_id_hash() {
        use std::collections::HashSet;
        let uid = UserId::new();
        let mut set = HashSet::new();
        set.insert(uid);
        assert!(set.contains(&uid));
    }

    #[test]
    fn auth_provider_labels() {
        assert_eq!(AuthProvider::Local.label(), "Email");
        assert_eq!(AuthProvider::Google.label(), "Google");
        assert_eq!(AuthProvider::GitHub.label(), "GitHub");
        assert_eq!(AuthProvider::Microsoft.label(), "Microsoft");
        assert_eq!(AuthProvider::Apple.label(), "Apple");
        assert_eq!(AuthProvider::Saml("Okta".into()).label(), "Okta");
        assert_eq!(AuthProvider::Custom("LDAP".into()).label(), "LDAP");
    }

    #[test]
    fn auth_provider_is_oauth() {
        assert!(!AuthProvider::Local.is_oauth());
        assert!(AuthProvider::Google.is_oauth());
        assert!(AuthProvider::GitHub.is_oauth());
        assert!(AuthProvider::Microsoft.is_oauth());
        assert!(AuthProvider::Apple.is_oauth());
        assert!(!AuthProvider::Saml("Okta".into()).is_oauth());
    }

    #[test]
    fn account_status_can_login() {
        assert!(AccountStatus::Active.can_login());
        assert!(!AccountStatus::Suspended.can_login());
        assert!(!AccountStatus::PendingVerification.can_login());
        assert!(!AccountStatus::Deactivated.can_login());
    }

    #[test]
    fn account_status_visibility() {
        assert!(AccountStatus::Active.is_visible());
        assert!(AccountStatus::PendingVerification.is_visible());
        assert!(!AccountStatus::Suspended.is_visible());
        assert!(!AccountStatus::Deactivated.is_visible());
    }

    #[test]
    fn user_new_defaults() {
        let u = User::new("test@example.com", "Test User", AuthProvider::Local);
        assert!(!u.id.is_nil());
        assert_eq!(u.email, "test@example.com");
        assert_eq!(u.display_name, "Test User");
        assert_eq!(u.provider, AuthProvider::Local);
        assert_eq!(u.status, AccountStatus::PendingVerification);
        assert!(!u.email_verified);
        assert!(u.last_login_at.is_none());
        assert!(u.avatar_url.is_none());
    }

    #[test]
    fn user_new_verified() {
        let u = User::new_verified("alice@google.com", "Alice", AuthProvider::Google);
        assert_eq!(u.status, AccountStatus::Active);
        assert!(u.email_verified);
        assert!(u.last_login_at.is_some());
    }

    #[test]
    fn user_with_id() {
        let id = UserId::new();
        let u = User::with_id(id, "bob@test.com", "Bob");
        assert_eq!(u.id, id);
        assert_eq!(u.status, AccountStatus::Active);
    }

    #[test]
    fn user_lifecycle() {
        let mut u = User::new("test@test.com", "Test", AuthProvider::Local);
        assert!(!u.is_active());

        u.verify_email();
        assert!(u.is_active());
        assert!(u.email_verified);

        u.suspend();
        assert_eq!(u.status, AccountStatus::Suspended);
        assert!(!u.is_active());

        u.reactivate();
        assert!(u.is_active());

        u.deactivate();
        assert_eq!(u.status, AccountStatus::Deactivated);
    }

    #[test]
    fn user_record_login() {
        let mut u = User::new_verified("a@b.com", "A", AuthProvider::Local);
        let _before = u.last_login_at;
        std::thread::sleep(std::time::Duration::from_millis(10));
        u.record_login();
        // last_login_at should be updated (or same second)
        assert!(u.last_login_at.is_some());
    }

    #[test]
    fn user_update_profile() {
        let mut u = User::new_verified("a@b.com", "Alice", AuthProvider::Local);
        u.update_profile("Alice Smith", Some("https://avatar.com/alice.png".into()));
        assert_eq!(u.display_name, "Alice Smith");
        assert_eq!(u.avatar_url.as_deref(), Some("https://avatar.com/alice.png"));
    }

    #[test]
    fn user_to_profile() {
        let u = User::new_verified("a@b.com", "Alice", AuthProvider::Local);
        let p = u.to_profile();
        assert_eq!(p.id, u.id);
        assert_eq!(p.display_name, "Alice");
        assert_eq!(p.avatar_url, None);
    }

    #[test]
    fn user_profile_from_ref() {
        let u = User::new_verified("a@b.com", "Bob", AuthProvider::GitHub);
        let p: UserProfile = (&u).into();
        assert_eq!(p.id, u.id);
    }

    #[test]
    fn user_serde_roundtrip() {
        let u = User::new_verified("a@b.com", "Serde Test", AuthProvider::Google);
        let json = serde_json::to_string(&u).unwrap();
        let back: User = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, u.id);
        assert_eq!(back.email, u.email);
        assert_eq!(back.display_name, u.display_name);
        assert_eq!(back.provider, u.provider);
    }
}
