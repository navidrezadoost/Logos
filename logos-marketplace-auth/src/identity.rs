//! Publisher identity management.
//!
//! Tracks publisher registration, verification status, and profiles.

use crate::crypto::{ContentDigest, Ed25519KeyPair, PublicKey, Signature};
use crate::{AuthError, AuthResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use uuid::Uuid;

/// Publisher verification status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum VerificationStatus {
    /// Registration received, awaiting verification
    Pending = 0,
    /// Email verified via challenge-response
    EmailVerified = 1,
    /// Identity confirmed by Logos team
    Verified = 2,
    /// Official Logos publisher
    Official = 3,
    /// Suspended (e.g., policy violation)
    Suspended = 4,
    /// Permanently banned
    Banned = 5,
}

impl VerificationStatus {
    /// Whether the publisher can submit plugins.
    pub fn can_publish(&self) -> bool {
        matches!(self, Self::EmailVerified | Self::Verified | Self::Official)
    }

    /// Whether the publisher is trusted.
    pub fn is_trusted(&self) -> bool {
        matches!(self, Self::Verified | Self::Official)
    }
}

impl std::fmt::Display for VerificationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::EmailVerified => write!(f, "email_verified"),
            Self::Verified => write!(f, "verified"),
            Self::Official => write!(f, "official"),
            Self::Suspended => write!(f, "suspended"),
            Self::Banned => write!(f, "banned"),
        }
    }
}

/// A publisher's public profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublisherProfile {
    /// Display name
    pub display_name: String,
    /// Publisher bio / description
    pub bio: Option<String>,
    /// Website URL
    pub website: Option<String>,
    /// Avatar URL
    pub avatar_url: Option<String>,
    /// GitHub username
    pub github: Option<String>,
    /// Twitter handle
    pub twitter: Option<String>,
    /// Organization name (if applicable)
    pub organization: Option<String>,
}

impl PublisherProfile {
    pub fn new(display_name: impl Into<String>) -> Self {
        Self {
            display_name: display_name.into(),
            bio: None,
            website: None,
            avatar_url: None,
            github: None,
            twitter: None,
            organization: None,
        }
    }

    pub fn with_bio(mut self, bio: impl Into<String>) -> Self {
        self.bio = Some(bio.into());
        self
    }

    pub fn with_website(mut self, url: impl Into<String>) -> Self {
        self.website = Some(url.into());
        self
    }

    pub fn with_github(mut self, handle: impl Into<String>) -> Self {
        self.github = Some(handle.into());
        self
    }

    pub fn with_organization(mut self, org: impl Into<String>) -> Self {
        self.organization = Some(org.into());
        self
    }
}

/// Full publisher identity record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublisherIdentity {
    /// Unique publisher ID
    pub id: Uuid,
    /// Publisher name (unique)
    name: String,
    /// Public key for signing
    public_key: PublicKey,
    /// Registration timestamp (UNIX seconds)
    registered_at: u64,
    /// Last activity timestamp
    last_active_at: u64,
    /// Verification status
    status: VerificationStatus,
    /// Public profile information
    profile: PublisherProfile,
    /// Number of published plugins
    pub plugin_count: u32,
    /// Total downloads across all plugins
    pub total_downloads: u64,
    /// Email hash for privacy-preserving contact
    pub email_hash: Option<String>,
}

impl PublisherIdentity {
    /// Create a new publisher identity.
    pub fn new(name: impl Into<String>, public_key: PublicKey) -> Self {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs();
        let name_str = name.into();

        Self {
            id: Uuid::new_v4(),
            name: name_str.clone(),
            public_key,
            registered_at: now,
            last_active_at: now,
            status: VerificationStatus::Pending,
            profile: PublisherProfile::new(&name_str),
            plugin_count: 0,
            total_downloads: 0,
            email_hash: None,
        }
    }

    /// Get publisher name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get public key.
    pub fn public_key(&self) -> &PublicKey {
        &self.public_key
    }

    /// Get verification status.
    pub fn status(&self) -> VerificationStatus {
        self.status
    }

    /// Set verification status.
    pub fn set_status(&mut self, status: VerificationStatus) {
        self.status = status;
        self.touch();
    }

    /// Get profile.
    pub fn profile(&self) -> &PublisherProfile {
        &self.profile
    }

    /// Update profile.
    pub fn set_profile(&mut self, profile: PublisherProfile) {
        self.profile = profile;
        self.touch();
    }

    /// Check if publisher can publish plugins.
    pub fn can_publish(&self) -> bool {
        self.status.can_publish()
    }

    /// Set email hash.
    pub fn set_email_hash(&mut self, email: &str) {
        let hash = crate::crypto::sha256(email.as_bytes());
        self.email_hash = Some(hash.iter().map(|b| format!("{b:02x}")).collect());
    }

    /// Register timestamp.
    pub fn registered_at(&self) -> u64 {
        self.registered_at
    }

    /// Update last activity.
    fn touch(&mut self) {
        self.last_active_at = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs();
    }

    /// Verify a signature from this publisher.
    pub fn verify_signature(&self, message: &[u8], signature: &Signature) -> bool {
        self.public_key.verify(message, signature)
    }
}

/// Publisher registry — manages all known publishers.
///
/// Performance:
/// - Lookup by key: O(1) via HashMap
/// - Lookup by name: O(1) via secondary index
/// - List publishers: O(n)
pub struct PublisherRegistry {
    /// Publishers keyed by public key hex
    by_key: HashMap<String, PublisherIdentity>,
    /// Name → key hex index
    by_name: HashMap<String, String>,
    /// Revoked keys
    revoked_keys: Vec<String>,
}

impl PublisherRegistry {
    /// Create empty registry.
    pub fn new() -> Self {
        Self {
            by_key: HashMap::new(),
            by_name: HashMap::new(),
            revoked_keys: Vec::new(),
        }
    }

    /// Register a new publisher.
    pub fn register(&mut self, identity: PublisherIdentity) -> AuthResult<()> {
        let key_hex = identity.public_key().to_hex();

        if self.by_key.contains_key(&key_hex) {
            return Err(AuthError::DuplicateKey);
        }

        if self.by_name.contains_key(identity.name()) {
            return Err(AuthError::DuplicateKey);
        }

        self.by_name.insert(identity.name().to_string(), key_hex.clone());
        self.by_key.insert(key_hex, identity);
        Ok(())
    }

    /// Look up publisher by public key.
    pub fn get_by_key(&self, public_key: &PublicKey) -> Option<&PublisherIdentity> {
        let key_hex = public_key.to_hex();
        if self.revoked_keys.contains(&key_hex) {
            return None;
        }
        self.by_key.get(&key_hex)
    }

    /// Look up publisher by public key (mutable).
    pub fn get_by_key_mut(&mut self, public_key: &PublicKey) -> Option<&mut PublisherIdentity> {
        let key_hex = public_key.to_hex();
        if self.revoked_keys.contains(&key_hex) {
            return None;
        }
        self.by_key.get_mut(&key_hex)
    }

    /// Look up publisher by name.
    pub fn get_by_name(&self, name: &str) -> Option<&PublisherIdentity> {
        let key_hex = self.by_name.get(name)?;
        if self.revoked_keys.contains(key_hex) {
            return None;
        }
        self.by_key.get(key_hex)
    }

    /// Check if a key is registered and not revoked.
    pub fn is_registered(&self, public_key: &PublicKey) -> bool {
        let key_hex = public_key.to_hex();
        self.by_key.contains_key(&key_hex) && !self.revoked_keys.contains(&key_hex)
    }

    /// Revoke a publisher key.
    pub fn revoke_key(&mut self, public_key: &PublicKey) {
        self.revoked_keys.push(public_key.to_hex());
    }

    /// List all active publishers.
    pub fn list_active(&self) -> Vec<&PublisherIdentity> {
        self.by_key
            .iter()
            .filter(|(k, _)| !self.revoked_keys.contains(k))
            .map(|(_, v)| v)
            .collect()
    }

    /// List publishers by verification status.
    pub fn list_by_status(&self, status: VerificationStatus) -> Vec<&PublisherIdentity> {
        self.list_active()
            .into_iter()
            .filter(|p| p.status() == status)
            .collect()
    }

    /// Total number of registered publishers.
    pub fn total_count(&self) -> usize {
        self.by_key.len()
    }

    /// Number of active (non-revoked) publishers.
    pub fn active_count(&self) -> usize {
        self.by_key.keys().filter(|k| !self.revoked_keys.contains(k)).count()
    }

    /// Set verification status for a publisher.
    pub fn set_status(&mut self, public_key: &PublicKey, status: VerificationStatus) -> AuthResult<()> {
        let identity = self.get_by_key_mut(public_key)
            .ok_or_else(|| AuthError::KeyNotFound(public_key.to_hex()))?;
        identity.set_status(status);
        Ok(())
    }
}

impl Default for PublisherRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_keypair() -> Ed25519KeyPair {
        Ed25519KeyPair::generate()
    }

    #[test]
    fn test_verification_status_can_publish() {
        assert!(!VerificationStatus::Pending.can_publish());
        assert!(VerificationStatus::EmailVerified.can_publish());
        assert!(VerificationStatus::Verified.can_publish());
        assert!(VerificationStatus::Official.can_publish());
        assert!(!VerificationStatus::Suspended.can_publish());
        assert!(!VerificationStatus::Banned.can_publish());
    }

    #[test]
    fn test_verification_status_is_trusted() {
        assert!(!VerificationStatus::Pending.is_trusted());
        assert!(!VerificationStatus::EmailVerified.is_trusted());
        assert!(VerificationStatus::Verified.is_trusted());
        assert!(VerificationStatus::Official.is_trusted());
    }

    #[test]
    fn test_publisher_profile_builder() {
        let profile = PublisherProfile::new("Logos Labs")
            .with_bio("Building the future of design")
            .with_website("https://logos.dev")
            .with_github("logos-dev")
            .with_organization("Logos Inc.");

        assert_eq!(profile.display_name, "Logos Labs");
        assert_eq!(profile.bio.as_deref(), Some("Building the future of design"));
        assert_eq!(profile.website.as_deref(), Some("https://logos.dev"));
        assert_eq!(profile.github.as_deref(), Some("logos-dev"));
        assert_eq!(profile.organization.as_deref(), Some("Logos Inc."));
    }

    #[test]
    fn test_publisher_identity_new() {
        let kp = test_keypair();
        let identity = PublisherIdentity::new("Alice", kp.public_key());

        assert_eq!(identity.name(), "Alice");
        assert_eq!(identity.status(), VerificationStatus::Pending);
        assert!(!identity.can_publish());
        assert_eq!(identity.plugin_count, 0);
    }

    #[test]
    fn test_publisher_identity_set_status() {
        let kp = test_keypair();
        let mut identity = PublisherIdentity::new("Bob", kp.public_key());

        identity.set_status(VerificationStatus::EmailVerified);
        assert!(identity.can_publish());
        assert!(!identity.status().is_trusted());

        identity.set_status(VerificationStatus::Verified);
        assert!(identity.status().is_trusted());
    }

    #[test]
    fn test_publisher_identity_verify_signature() {
        let kp = test_keypair();
        let identity = PublisherIdentity::new("Charlie", kp.public_key());

        let sig = kp.sign(b"plugin-data");
        assert!(identity.verify_signature(b"plugin-data", &sig));
        assert!(!identity.verify_signature(b"tampered-data", &sig));
    }

    #[test]
    fn test_publisher_identity_email_hash() {
        let kp = test_keypair();
        let mut identity = PublisherIdentity::new("Dave", kp.public_key());

        identity.set_email_hash("dave@example.com");
        assert!(identity.email_hash.is_some());
        assert_eq!(identity.email_hash.as_ref().unwrap().len(), 64);
    }

    #[test]
    fn test_publisher_registry_register() {
        let mut reg = PublisherRegistry::new();
        let kp = test_keypair();
        let identity = PublisherIdentity::new("Publisher A", kp.public_key());

        assert!(reg.register(identity).is_ok());
        assert_eq!(reg.total_count(), 1);
        assert_eq!(reg.active_count(), 1);
    }

    #[test]
    fn test_publisher_registry_duplicate() {
        let mut reg = PublisherRegistry::new();
        let kp = test_keypair();
        let id1 = PublisherIdentity::new("Publisher", kp.public_key());
        let id2 = PublisherIdentity::new("Publisher Dup", kp.public_key());

        assert!(reg.register(id1).is_ok());
        assert_eq!(reg.register(id2).unwrap_err(), AuthError::DuplicateKey);
    }

    #[test]
    fn test_publisher_registry_lookup() {
        let mut reg = PublisherRegistry::new();
        let kp = test_keypair();
        let pk = kp.public_key();
        let identity = PublisherIdentity::new("Findable", pk.clone());
        reg.register(identity).unwrap();

        assert!(reg.get_by_key(&pk).is_some());
        assert_eq!(reg.get_by_key(&pk).unwrap().name(), "Findable");

        assert!(reg.get_by_name("Findable").is_some());
        assert!(reg.get_by_name("Missing").is_none());
    }

    #[test]
    fn test_publisher_registry_revoke() {
        let mut reg = PublisherRegistry::new();
        let kp = test_keypair();
        let pk = kp.public_key();
        reg.register(PublisherIdentity::new("Revoked", pk.clone())).unwrap();

        assert!(reg.is_registered(&pk));
        reg.revoke_key(&pk);
        assert!(!reg.is_registered(&pk));
        assert!(reg.get_by_key(&pk).is_none());
        assert_eq!(reg.active_count(), 0);
    }

    #[test]
    fn test_publisher_registry_list_by_status() {
        let mut reg = PublisherRegistry::new();

        let kp1 = test_keypair();
        let mut id1 = PublisherIdentity::new("Pending", kp1.public_key());
        id1.set_status(VerificationStatus::Pending);
        reg.register(id1).unwrap();

        let kp2 = test_keypair();
        let mut id2 = PublisherIdentity::new("Verified", kp2.public_key());
        id2.set_status(VerificationStatus::Verified);
        reg.register(id2).unwrap();

        let kp3 = test_keypair();
        let mut id3 = PublisherIdentity::new("Official", kp3.public_key());
        id3.set_status(VerificationStatus::Official);
        reg.register(id3).unwrap();

        assert_eq!(reg.list_by_status(VerificationStatus::Pending).len(), 1);
        assert_eq!(reg.list_by_status(VerificationStatus::Verified).len(), 1);
        assert_eq!(reg.list_by_status(VerificationStatus::Official).len(), 1);
        assert_eq!(reg.list_active().len(), 3);
    }

    #[test]
    fn test_publisher_registry_set_status() {
        let mut reg = PublisherRegistry::new();
        let kp = test_keypair();
        let pk = kp.public_key();
        reg.register(PublisherIdentity::new("Upgradable", pk.clone())).unwrap();

        assert_eq!(reg.get_by_key(&pk).unwrap().status(), VerificationStatus::Pending);
        reg.set_status(&pk, VerificationStatus::Verified).unwrap();
        assert_eq!(reg.get_by_key(&pk).unwrap().status(), VerificationStatus::Verified);
    }
}
