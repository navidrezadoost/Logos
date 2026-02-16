//! Key attestation — binding publisher identity to cryptographic keys.
//!
//! An attestation is a signed statement binding a public key
//! to an identity claim (email, domain, organization). Attestations
//! can be self-signed (initial claim) or counter-signed (verified by Logos).

use crate::crypto::{sha256, Ed25519KeyPair, PublicKey, Signature};
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime};
use uuid::Uuid;

/// Type of attestation claim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttestationType {
    /// Email address verified
    EmailVerified,
    /// Domain ownership verified (via DNS TXT record)
    DomainVerified,
    /// Organization verified (manual review)
    OrganizationVerified,
    /// GitHub account linked
    GitHubLinked,
    /// Self-attestation (unverified claim)
    SelfAttested,
}

impl std::fmt::Display for AttestationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmailVerified => write!(f, "email_verified"),
            Self::DomainVerified => write!(f, "domain_verified"),
            Self::OrganizationVerified => write!(f, "organization_verified"),
            Self::GitHubLinked => write!(f, "github_linked"),
            Self::SelfAttested => write!(f, "self_attested"),
        }
    }
}

/// A key attestation — a signed claim binding identity to a key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyAttestation {
    /// Attestation ID
    pub id: Uuid,
    /// Public key being attested
    pub public_key_hex: String,
    /// Type of attestation
    pub attestation_type: AttestationType,
    /// The claim value (e.g., email address, domain)
    pub claim: String,
    /// Signature over the attestation data
    pub signature: Signature,
    /// When the attestation was created (UNIX timestamp)
    pub created_at: u64,
    /// When the attestation expires (UNIX timestamp, 0 = no expiry)
    pub expires_at: u64,
    /// Optional counter-signer public key (for verified attestations)
    pub counter_signer_key: Option<String>,
    /// Optional counter-signature
    pub counter_signature: Option<Signature>,
}

impl KeyAttestation {
    /// Create a self-signed attestation.
    ///
    /// The publisher signs a claim about their own identity.
    /// This is unverified until counter-signed by a trusted party.
    pub fn self_signed(
        keypair: &Ed25519KeyPair,
        claim: &str,
        attestation_type: AttestationType,
    ) -> Self {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs();

        let id = Uuid::new_v4();
        let public_key_hex = keypair.public_key().to_hex();

        let data = Self::attestation_data(&id, &public_key_hex, claim, &attestation_type, now);
        let signature = keypair.sign(&data);

        Self {
            id,
            public_key_hex,
            attestation_type,
            claim: claim.to_string(),
            signature,
            created_at: now,
            expires_at: 0,
            counter_signer_key: None,
            counter_signature: None,
        }
    }

    /// Counter-sign an attestation (verification by trusted party).
    pub fn counter_sign(&mut self, counter_signer: &Ed25519KeyPair) {
        let data = self.counter_sign_data();
        self.counter_signature = Some(counter_signer.sign(&data));
        self.counter_signer_key = Some(counter_signer.public_key().to_hex());
    }

    /// Verify the self-signature.
    pub fn verify(&self, public_key: &PublicKey) -> bool {
        if self.public_key_hex != public_key.to_hex() {
            return false;
        }

        let data = Self::attestation_data(
            &self.id,
            &self.public_key_hex,
            &self.claim,
            &self.attestation_type,
            self.created_at,
        );
        public_key.verify(&data, &self.signature)
    }

    /// Verify the counter-signature (if present).
    pub fn verify_counter_signature(&self, counter_signer_key: &PublicKey) -> bool {
        match (&self.counter_signature, &self.counter_signer_key) {
            (Some(sig), Some(key_hex)) => {
                if key_hex != &counter_signer_key.to_hex() {
                    return false;
                }
                let data = self.counter_sign_data();
                counter_signer_key.verify(&data, sig)
            }
            _ => false,
        }
    }

    /// Is this attestation counter-signed (verified)?
    pub fn is_verified(&self) -> bool {
        self.counter_signature.is_some()
    }

    /// Has this attestation expired?
    pub fn is_expired(&self) -> bool {
        if self.expires_at == 0 {
            return false; // No expiry
        }
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs();
        now > self.expires_at
    }

    /// Build the attestation data to sign.
    fn attestation_data(
        id: &Uuid,
        public_key_hex: &str,
        claim: &str,
        attestation_type: &AttestationType,
        timestamp: u64,
    ) -> Vec<u8> {
        format!(
            "logos-attestation:{}:{}:{}:{}:{}",
            id, public_key_hex, claim, attestation_type, timestamp
        )
        .into_bytes()
    }

    /// Build the data for counter-signing.
    fn counter_sign_data(&self) -> Vec<u8> {
        let sig_hex: String = self.signature.as_bytes().iter().map(|b| format!("{b:02x}")).collect();
        format!(
            "logos-countersign:{}:{}:{}",
            self.id, self.public_key_hex, sig_hex
        )
        .into_bytes()
    }
}

/// Chain of attestations for a publisher key.
///
/// A publisher can have multiple attestations (email, domain, org).
/// The chain tracks all attestations and their verification status.
#[derive(Debug, Clone)]
pub struct AttestationChain {
    /// All attestations for a key
    attestations: Vec<KeyAttestation>,
    /// Public key being attested
    public_key: PublicKey,
}

impl AttestationChain {
    /// Create a new empty chain for a key.
    pub fn new(public_key: PublicKey) -> Self {
        Self {
            attestations: Vec::new(),
            public_key,
        }
    }

    /// Add an attestation to the chain.
    pub fn add(&mut self, attestation: KeyAttestation) -> bool {
        // Verify the attestation is for our key
        if !attestation.verify(&self.public_key) {
            return false;
        }
        self.attestations.push(attestation);
        true
    }

    /// Get all attestations.
    pub fn attestations(&self) -> &[KeyAttestation] {
        &self.attestations
    }

    /// Get verified attestations only.
    pub fn verified_attestations(&self) -> Vec<&KeyAttestation> {
        self.attestations
            .iter()
            .filter(|a| a.is_verified() && !a.is_expired())
            .collect()
    }

    /// Check if the chain has a specific type of verified attestation.
    pub fn has_verified(&self, attestation_type: &AttestationType) -> bool {
        self.verified_attestations()
            .iter()
            .any(|a| &a.attestation_type == attestation_type)
    }

    /// Total attestation count.
    pub fn count(&self) -> usize {
        self.attestations.len()
    }

    /// Verified attestation count.
    pub fn verified_count(&self) -> usize {
        self.verified_attestations().len()
    }

    /// Trust score (0-100) based on attestations.
    ///
    /// Scoring:
    /// - Self-attested: +5
    /// - Email verified (counter-signed): +20
    /// - Domain verified (counter-signed): +30
    /// - Organization verified (counter-signed): +30
    /// - GitHub linked (counter-signed): +15
    pub fn trust_score(&self) -> u32 {
        let mut score = 0u32;
        for a in &self.attestations {
            if a.is_expired() {
                continue;
            }
            let points = if a.is_verified() {
                match a.attestation_type {
                    AttestationType::EmailVerified => 20,
                    AttestationType::DomainVerified => 30,
                    AttestationType::OrganizationVerified => 30,
                    AttestationType::GitHubLinked => 15,
                    AttestationType::SelfAttested => 5,
                }
            } else {
                match a.attestation_type {
                    AttestationType::SelfAttested => 5,
                    _ => 2, // Unverified claims get minimal credit
                }
            };
            score += points;
        }
        score.min(100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attestation_type_display() {
        assert_eq!(AttestationType::EmailVerified.to_string(), "email_verified");
        assert_eq!(AttestationType::DomainVerified.to_string(), "domain_verified");
        assert_eq!(AttestationType::GitHubLinked.to_string(), "github_linked");
    }

    #[test]
    fn test_self_signed_attestation() {
        let kp = Ed25519KeyPair::generate();
        let attest = KeyAttestation::self_signed(
            &kp,
            "dev@logos.dev",
            AttestationType::EmailVerified,
        );

        assert_eq!(attest.claim, "dev@logos.dev");
        assert_eq!(attest.attestation_type, AttestationType::EmailVerified);
        assert!(!attest.is_verified()); // Not counter-signed yet
        assert!(attest.verify(&kp.public_key()));
    }

    #[test]
    fn test_counter_signed_attestation() {
        let publisher_kp = Ed25519KeyPair::generate();
        let logos_kp = Ed25519KeyPair::generate(); // Logos verification key

        let mut attest = KeyAttestation::self_signed(
            &publisher_kp,
            "dev@logos.dev",
            AttestationType::EmailVerified,
        );

        assert!(!attest.is_verified());
        attest.counter_sign(&logos_kp);
        assert!(attest.is_verified());
        assert!(attest.verify_counter_signature(&logos_kp.public_key()));
    }

    #[test]
    fn test_attestation_wrong_key() {
        let kp1 = Ed25519KeyPair::generate();
        let kp2 = Ed25519KeyPair::generate();

        let attest = KeyAttestation::self_signed(
            &kp1,
            "claim",
            AttestationType::SelfAttested,
        );

        // Should not verify with wrong key
        assert!(!attest.verify(&kp2.public_key()));
    }

    #[test]
    fn test_attestation_chain() {
        let kp = Ed25519KeyPair::generate();
        let mut chain = AttestationChain::new(kp.public_key());

        let a1 = KeyAttestation::self_signed(
            &kp,
            "self-claim",
            AttestationType::SelfAttested,
        );
        assert!(chain.add(a1));
        assert_eq!(chain.count(), 1);
        assert_eq!(chain.verified_count(), 0);
    }

    #[test]
    fn test_attestation_chain_reject_wrong_key() {
        let kp1 = Ed25519KeyPair::generate();
        let kp2 = Ed25519KeyPair::generate();
        let mut chain = AttestationChain::new(kp1.public_key());

        let wrong_attest = KeyAttestation::self_signed(
            &kp2,
            "wrong",
            AttestationType::SelfAttested,
        );
        assert!(!chain.add(wrong_attest));
        assert_eq!(chain.count(), 0);
    }

    #[test]
    fn test_attestation_chain_trust_score() {
        let kp = Ed25519KeyPair::generate();
        let logos_kp = Ed25519KeyPair::generate();
        let mut chain = AttestationChain::new(kp.public_key());

        // Self-attested: +5
        let a1 = KeyAttestation::self_signed(&kp, "self", AttestationType::SelfAttested);
        chain.add(a1);
        assert_eq!(chain.trust_score(), 5);

        // Email verified + counter-signed: +20
        let mut a2 = KeyAttestation::self_signed(&kp, "a@b.com", AttestationType::EmailVerified);
        a2.counter_sign(&logos_kp);
        chain.add(a2);
        assert_eq!(chain.trust_score(), 25);

        // Domain verified + counter-signed: +30
        let mut a3 = KeyAttestation::self_signed(&kp, "logos.dev", AttestationType::DomainVerified);
        a3.counter_sign(&logos_kp);
        chain.add(a3);
        assert_eq!(chain.trust_score(), 55);
    }

    #[test]
    fn test_attestation_chain_has_verified() {
        let kp = Ed25519KeyPair::generate();
        let logos_kp = Ed25519KeyPair::generate();
        let mut chain = AttestationChain::new(kp.public_key());

        let mut attest = KeyAttestation::self_signed(&kp, "a@b.com", AttestationType::EmailVerified);
        attest.counter_sign(&logos_kp);
        chain.add(attest);

        assert!(chain.has_verified(&AttestationType::EmailVerified));
        assert!(!chain.has_verified(&AttestationType::DomainVerified));
    }
}
