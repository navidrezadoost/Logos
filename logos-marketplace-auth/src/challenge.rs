//! Challenge-response protocol for publisher verification.
//!
//! A publisher proves ownership of a private key by signing
//! a random challenge issued by the server. This is the
//! primary mechanism for upgrading from Pending → EmailVerified.

use crate::crypto::{sha256, Ed25519KeyPair, PublicKey, Signature};
use crate::{AuthError, AuthResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use uuid::Uuid;

/// A challenge issued to a publisher.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Challenge {
    /// Challenge ID
    pub id: Uuid,
    /// Random nonce (hex-encoded)
    pub nonce: String,
    /// Public key of the challenged publisher
    pub public_key_hex: String,
    /// When the challenge was issued (UNIX timestamp)
    pub issued_at: u64,
    /// When the challenge expires (UNIX timestamp)
    pub expires_at: u64,
}

impl Challenge {
    /// Create a new challenge for a publisher.
    pub fn new(public_key: &PublicKey, ttl: Duration) -> Self {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs();

        let nonce_bytes = sha256(format!("{}{}", Uuid::new_v4(), now).as_bytes());
        let nonce: String = nonce_bytes.iter().map(|b| format!("{b:02x}")).collect();

        Self {
            id: Uuid::new_v4(),
            nonce,
            public_key_hex: public_key.to_hex(),
            issued_at: now,
            expires_at: now + ttl.as_secs(),
        }
    }

    /// Check if challenge has expired.
    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs();
        now > self.expires_at
    }

    /// Get the data that must be signed to respond.
    pub fn challenge_data(&self) -> Vec<u8> {
        format!("logos-challenge:{}:{}", self.id, self.nonce).into_bytes()
    }
}

/// A response to a challenge (signed by the publisher).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeResponse {
    /// Challenge ID being responded to
    pub challenge_id: Uuid,
    /// Publisher's public key hex
    pub public_key_hex: String,
    /// Signature over the challenge data
    pub signature: Signature,
}

impl ChallengeResponse {
    /// Sign a challenge with the publisher's keypair.
    pub fn sign(keypair: &Ed25519KeyPair, challenge: &Challenge) -> Self {
        let data = challenge.challenge_data();
        let signature = keypair.sign(&data);

        Self {
            challenge_id: challenge.id,
            public_key_hex: keypair.public_key().to_hex(),
            signature,
        }
    }
}

/// Challenge verifier — issues and verifies challenges.
///
/// Performance:
/// - Issue: O(1)
/// - Verify: O(1) HashMap lookup + signature verification
pub struct ChallengeVerifier {
    /// Pending challenges keyed by challenge ID
    pending: HashMap<Uuid, Challenge>,
    /// Default challenge TTL
    default_ttl: Duration,
    /// Completed (verified) challenge IDs
    completed: Vec<Uuid>,
    /// Maximum pending challenges per publisher
    max_pending_per_publisher: usize,
}

impl ChallengeVerifier {
    /// Create a new challenge verifier.
    pub fn new() -> Self {
        Self {
            pending: HashMap::new(),
            default_ttl: Duration::from_secs(300), // 5 minutes
            completed: Vec::new(),
            max_pending_per_publisher: 5,
        }
    }

    /// Create with custom TTL.
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.default_ttl = ttl;
        self
    }

    /// Issue a new challenge for a publisher.
    pub fn issue_challenge(&mut self, public_key: &PublicKey) -> Challenge {
        // Clean up any existing expired challenges
        self.cleanup_expired();

        let challenge = Challenge::new(public_key, self.default_ttl);
        self.pending.insert(challenge.id, challenge.clone());
        challenge
    }

    /// Verify a challenge response.
    pub fn verify_response(&mut self, response: &ChallengeResponse) -> AuthResult<PublicKey> {
        // Find the pending challenge
        let challenge = self
            .pending
            .get(&response.challenge_id)
            .ok_or(AuthError::ChallengeMismatch)?;

        // Check expiry
        if challenge.is_expired() {
            self.pending.remove(&response.challenge_id);
            return Err(AuthError::ChallengeExpired);
        }

        // Verify the response is for the right publisher
        if challenge.public_key_hex != response.public_key_hex {
            return Err(AuthError::ChallengeMismatch);
        }

        // Reconstruct the public key
        let public_key = PublicKey::from_hex(&response.public_key_hex)
            .ok_or(AuthError::InvalidSignature)?;

        // Verify the signature
        let data = challenge.challenge_data();
        if !public_key.verify(&data, &response.signature) {
            return Err(AuthError::InvalidSignature);
        }

        // Mark challenge as completed
        self.pending.remove(&response.challenge_id);
        self.completed.push(response.challenge_id);

        Ok(public_key)
    }

    /// Count pending challenges.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Count completed verifications.
    pub fn completed_count(&self) -> usize {
        self.completed.len()
    }

    /// Clean up expired challenges.
    pub fn cleanup_expired(&mut self) -> usize {
        let before = self.pending.len();
        self.pending.retain(|_, c| !c.is_expired());
        before - self.pending.len()
    }
}

impl Default for ChallengeVerifier {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_challenge_new() {
        let kp = Ed25519KeyPair::generate();
        let challenge = Challenge::new(&kp.public_key(), Duration::from_secs(300));

        assert_eq!(challenge.public_key_hex, kp.public_key().to_hex());
        assert!(!challenge.is_expired());
        assert_eq!(challenge.nonce.len(), 64);
    }

    #[test]
    fn test_challenge_data() {
        let kp = Ed25519KeyPair::generate();
        let challenge = Challenge::new(&kp.public_key(), Duration::from_secs(300));
        let data = challenge.challenge_data();
        assert!(String::from_utf8_lossy(&data).starts_with("logos-challenge:"));
    }

    #[test]
    fn test_challenge_response_sign() {
        let kp = Ed25519KeyPair::generate();
        let challenge = Challenge::new(&kp.public_key(), Duration::from_secs(300));
        let response = ChallengeResponse::sign(&kp, &challenge);

        assert_eq!(response.challenge_id, challenge.id);
        assert_eq!(response.public_key_hex, kp.public_key().to_hex());
    }

    #[test]
    fn test_verifier_issue_and_verify() {
        let kp = Ed25519KeyPair::generate();
        let mut verifier = ChallengeVerifier::new();

        let challenge = verifier.issue_challenge(&kp.public_key());
        assert_eq!(verifier.pending_count(), 1);

        let response = ChallengeResponse::sign(&kp, &challenge);
        let result = verifier.verify_response(&response);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), kp.public_key());
        assert_eq!(verifier.pending_count(), 0);
        assert_eq!(verifier.completed_count(), 1);
    }

    #[test]
    fn test_verifier_wrong_key() {
        let kp1 = Ed25519KeyPair::generate();
        let kp2 = Ed25519KeyPair::generate();
        let mut verifier = ChallengeVerifier::new();

        let challenge = verifier.issue_challenge(&kp1.public_key());

        // Sign with wrong keypair (but pretend to be kp1)
        let data = challenge.challenge_data();
        let wrong_sig = kp2.sign(&data);
        let response = ChallengeResponse {
            challenge_id: challenge.id,
            public_key_hex: kp1.public_key().to_hex(),
            signature: wrong_sig,
        };

        let result = verifier.verify_response(&response);
        assert_eq!(result.unwrap_err(), AuthError::InvalidSignature);
    }

    #[test]
    fn test_verifier_unknown_challenge() {
        let kp = Ed25519KeyPair::generate();
        let mut verifier = ChallengeVerifier::new();

        // Create a response for a challenge that doesn't exist
        let fake_challenge = Challenge::new(&kp.public_key(), Duration::from_secs(300));
        let response = ChallengeResponse::sign(&kp, &fake_challenge);

        let result = verifier.verify_response(&response);
        assert_eq!(result.unwrap_err(), AuthError::ChallengeMismatch);
    }

    #[test]
    fn test_verifier_double_use() {
        let kp = Ed25519KeyPair::generate();
        let mut verifier = ChallengeVerifier::new();

        let challenge = verifier.issue_challenge(&kp.public_key());
        let response = ChallengeResponse::sign(&kp, &challenge);

        assert!(verifier.verify_response(&response.clone()).is_ok());
        // Second use should fail (challenge consumed)
        assert_eq!(verifier.verify_response(&response).unwrap_err(), AuthError::ChallengeMismatch);
    }
}
